//! The second screen's panel (the AYANEO Pocket DS's bottom screen, handed in through
//! `platform::second_screen`): the hotbar, the experience bar, hearts, armour, food and air,
//! laid out as vanilla's HUD lays them out (`hud_layout`) but bigger and upside down: the hotbar
//! along the top, then the experience bar, hearts and food, armour and air, and the coordinates at
//! the bottom left. A minimap a mod shows (the Minimap mod's, with its zoom, shape, turning and
//! outline) goes in the bottom right, at its image's full size. A tap on a hotbar slot selects it.
//! While the panel shows, the window's HUD leaves those widgets out (`Game::hud_layers`,
//! `Game::draw_extras`).
//!
//! The `bottomScreen` extra turns it off: the game then stops asking for the screen, and the
//! host gives it back to the system.
use super::Game;
use horizon_client::extras::Corner;
use horizon_client::extras_hud::{self, Corners};
use horizon_client::hud::HudBatch;
use horizon_client::hud_layout;
use horizon_client::platform::second_screen::{self, Change, TouchPhase};
use std::time::{Duration, Instant};

/// The panel is redrawn at most this often: its bars change at tick rate.
const FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// GUI pixels the panel keeps across: the hotbar with the off-hand slot beside it.
const GUI_WIDTH: u32 = 250;
/// GUI pixels from the panel's top edge to the hotbar's bottom one: the hotbar along the top,
/// its selection frame one pixel above it.
const HOTBAR_BOTTOM: i32 = 26;
/// A line of text's height in GUI pixels, without its shadow.
const TEXT_HEIGHT: i32 = 8;
/// GUI pixels the level number moves down after the flip: vanilla lets its bottom overlap the
/// experience bar's top, and mirrored its top would overlap the bar's bottom; this puts it under
/// the bar, centred on the row of hearts and food between them.
const LEVEL_DROP: i32 = 5;
/// GUI pixels between the coordinates' last line and the panel's bottom edge, and between lines.
const TEXT_MARGIN: f32 = 6.0;
const LINE: f32 = 11.0;
/// The minimap's side in GUI pixels: one per pixel of its image, which fits under the bars.
const MAP_SIDE: f32 = horizon_client::minimap::SIZE as f32;
/// GUI pixels of each hotbar slot, and the hotbar's half width (`hud_layout::hotbar`).
const SLOT: i32 = 20;
const HOTBAR_HALF: i32 = 91;

#[derive(Default)]
pub(super) struct BottomPanel {
    /// The renderer draws on the second screen.
    attached: bool,
    /// The panel's own heart blink and bubble state: the window's HUD keeps its own.
    health: hud_layout::HealthHud,
    drawn: Option<Instant>,
}

impl BottomPanel {
    /// Whether the panel is on the second screen (the window's HUD leaves its widgets out).
    pub(super) fn shown(&self) -> bool {
        self.attached
    }
}

/// The panel's GUI scale on a screen `size` pixels (the largest whole one that keeps
/// [`GUI_WIDTH`] across) and the GUI size it gives.
fn layout(size: [u32; 2]) -> (u32, [f32; 2]) {
    let scale = (size[0] / GUI_WIDTH).max(1);
    let gui = size.map(|side| (side as f32 / scale as f32).ceil());
    (scale, gui)
}

/// Mirrors `draws` top to bottom across the line `axis2 / 2` (a GUI y, doubled to stay whole):
/// each draw keeps its look and its place flips, so a stack that grew up grows down.
fn flip(draws: &mut [hud_layout::GuiDraw], axis2: i32) {
    use hud_layout::GuiDraw;
    for d in draws {
        match d {
            GuiDraw::Sprite { y, h, .. }
            | GuiDraw::AtlasSprite { y, h, .. }
            | GuiDraw::Blit { y, h, .. } => *y = axis2 - *y - *h,
            GuiDraw::Fill { y0, y1, .. } => (*y0, *y1) = (axis2 - *y1, axis2 - *y0),
            GuiDraw::Text { y, .. } => *y = axis2 - *y - TEXT_HEIGHT,
            GuiDraw::Item { y, .. } => *y = axis2 - *y - 16,
            GuiDraw::Push(_) | GuiDraw::Pop => {}
        }
    }
}

/// The hotbar slot under a GUI point, on a panel `gui` wide and tall.
fn slot_at(gui: [f32; 2], at: [f32; 2]) -> Option<usize> {
    let (w, h) = (gui[0] as i32, HOTBAR_BOTTOM);
    let left = w / 2 - HOTBAR_HALF;
    let [x, y] = at.map(|v| v.floor() as i32);
    // The hotbar's row, and a little more above and below it.
    if !(h - 22 - 4..h + 4).contains(&y) || x < left {
        return None;
    }
    let slot = (x - left) / SLOT;
    (slot < 9).then_some(slot as usize)
}

impl Game<'_> {
    /// Between frames: ask for the second screen or give it back, take a surface it hands
    /// over or let go of one it takes back, and the taps on it.
    pub(super) fn poll_bottom_panel(&mut self) {
        let in_world = self.title.is_none() && self.player.is_some();
        second_screen::set_wanted(self.settings.extras.bottom_screen && in_world);
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        while let Some(change) = second_screen::poll() {
            match change {
                Change::Offered { window, size } => {
                    self.bottom_panel.attached = match renderer.attach_second_screen(window, size) {
                        Ok(()) => true,
                        Err(e) => {
                            log::warn!("Horizon: the second screen can't be drawn on: {e}");
                            second_screen::released();
                            false
                        }
                    };
                    self.bottom_panel.drawn = None;
                }
                Change::Withdrawn => {
                    renderer.detach_second_screen();
                    self.bottom_panel.attached = false;
                    second_screen::released();
                }
            }
        }
        let touches = second_screen::touches();
        let Some(size) = renderer.second_screen_size().filter(|_| in_world) else {
            return;
        };
        let (scale, gui) = layout(size);
        for touch in touches {
            if touch.phase != TouchPhase::Down || self.spectator() {
                continue;
            }
            let at = touch.position.map(|v| v / scale as f32);
            if let Some(slot) = slot_at(gui, at) {
                // As a number key: the tick selects it (`handleKeybinds`).
                self.hotbar_presses.push(slot);
            }
        }
    }

    /// After the window's frame: the panel, at most every [`FRAME_INTERVAL`].
    pub(super) fn draw_bottom_panel(&mut self) {
        if !self.bottom_panel.attached
            || self
                .bottom_panel
                .drawn
                .is_some_and(|drawn| drawn.elapsed() < FRAME_INTERVAL)
        {
            return;
        }
        let Some(size) = self.renderer.as_ref().and_then(|r| r.second_screen_size()) else {
            return;
        };
        let batch = self.bottom_panel_batch(size);
        if let Some(renderer) = self.renderer.as_mut()
            && renderer.draw_second_screen(&batch)
        {
            self.bottom_panel.drawn = Some(Instant::now());
        }
    }

    fn bottom_panel_batch(&mut self, size: [u32; 2]) -> HudBatch {
        let mut batch = HudBatch::new(size);
        let (scale, [w, h]) = layout(size);
        batch.scale = scale as f32;
        batch.width = w;
        batch.height = h;
        batch.glint_colour = self.settings.extras.pvp.tweaks.glint_texel();
        if self.title.is_some() || self.player.is_none() {
            return batch;
        }
        let (wi, hi) = (w as i32, HOTBAR_BOTTOM);
        // The bars vanilla stacks above the hotbar, mirrored across its middle row (`hi - 11`):
        // the experience bar right under it, then hearts and food, then armour and air.
        let below = 2 * (hi - 11);
        let spectator = self.spectator();
        if !spectator {
            let partial = self.clock.fraction() as f32;
            let inputs = hud_layout::HotbarInputs {
                selected: self.selected as i32,
                right_handed: !self.settings.main_hand_left,
                offhand: self.offhand.is_some(),
                slots: std::array::from_fn(|i| {
                    self.inventory[i]
                        .as_ref()
                        .map(|_| (f32::from(self.hotbar_pop[i]) - partial).max(0.0))
                }),
                offhand_pop: (f32::from(self.hotbar_pop[9]) - partial).max(0.0),
                attack: None,
            };
            let draws = hud_layout::hotbar(wi, hi, &inputs);
            hud_layout::paint(&mut batch, &self.gui.atlas, &draws);
            for d in &draws {
                if let hud_layout::GuiDraw::Item { slot, x, y, .. } = *d {
                    let stack = match slot {
                        40 => self.offhand.as_ref(),
                        slot => self.inventory[slot as usize].as_ref(),
                    };
                    if let Some(stack) = stack {
                        self.draw_item(&mut batch, stack, x as f32, y as f32);
                    }
                }
            }
        }
        if !self.creative && !spectator {
            let atlas = &self.gui.atlas;
            let width = |s: &str| HudBatch::text_width(atlas, s) as i32;
            let mut draws = hud_layout::experience(
                wi,
                hi,
                self.experience.progress,
                self.experience.level,
                true,
                &width,
            );
            flip(&mut draws, below);
            for d in &mut draws {
                if let hud_layout::GuiDraw::Text { y, .. } = d {
                    *y += LEVEL_DROP;
                }
            }
            hud_layout::paint(&mut batch, atlas, &draws);
        }
        if let Some((inputs, at)) = self.health_inputs(0) {
            let mut draws = self.bottom_panel.health.layout(&inputs, wi, hi);
            flip(&mut draws, below);
            hud_layout::paint(&mut batch, &self.gui.atlas, &draws);
            if let Some(sound) = self.bottom_panel.health.bubble_pop_sound.take() {
                self.play_bubble_pop(sound, at);
            }
        }
        if let Some(look) = self.minimap_look() {
            let look = extras_hud::MinimapLook {
                corner: Corner::BottomRight,
                size: MAP_SIDE,
                ..look
            };
            let mut corners = Corners::new(w, h, 0.0);
            extras_hud::draw_minimap(&mut batch, &self.gui.atlas, &mut corners, &look);
        }
        if !self.reduced_debug_info()
            && let Some(p) = &self.player
        {
            let [x, y, z] = [p.position.x, p.position.y, p.position.z].map(|v| v.floor() as i64);
            let facing = horizon_client::pvp_hud::facing(self.yaw);
            let name = horizon_client::lang::get_or(
                &format!("horizon.pvp.hud.direction.{facing}"),
                facing,
            );
            let lines = [
                horizon_client::lang::tr("horizon.pvp.hud.coords", &[&x, &y, &z]),
                horizon_client::lang::tr("horizon.pvp.hud.facing", &[&name]),
            ];
            for (i, line) in lines.iter().enumerate() {
                batch.text(
                    &self.gui.atlas,
                    line,
                    TEXT_MARGIN,
                    h - TEXT_MARGIN - 8.0 - (lines.len() - 1 - i) as f32 * LINE,
                    u32::MAX,
                    true,
                );
            }
        }
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bars_mirror_under_the_hotbar_keeping_their_size() {
        use hud_layout::GuiDraw;
        let hi = HOTBAR_BOTTOM;
        // Vanilla's experience bar (`h - 29`, 5 tall) and a heart (`h - 39`, 9 tall).
        let sprite = |y, h| GuiDraw::Sprite {
            sprite: "s".into(),
            x: 0,
            y,
            w: 9,
            h,
            region: None,
            color: 0,
            blend: hud_layout::Blend::Textured,
        };
        let mut draws = vec![sprite(hi - 29, 5), sprite(hi - 39, 9)];
        flip(&mut draws, 2 * (hi - 11));
        let ys: Vec<_> = draws
            .iter()
            .map(|d| match d {
                GuiDraw::Sprite { y, h, .. } => (*y, *h),
                _ => unreachable!(),
            })
            .collect();
        // The bar two pixels under the hotbar, as it is two above it in vanilla; the hearts
        // under the bar.
        assert_eq!(ys, [(hi + 2, 5), (hi + 8, 9)]);
    }

    #[test]
    fn the_minimap_fits_under_the_bars_at_the_bottom_right() {
        let (_, [w, h]) = layout([1024, 768]);
        let (x, y) = Corners::new(w, h, 0.0).place(Corner::BottomRight, MAP_SIDE, MAP_SIDE);
        assert_eq!((x, y), (126.0, 62.0));
        // The armour row, the lowest, starts 18 under the hotbar (vanilla's `h - 49`, mirrored)
        // and is 9 tall: the map clears it even moved up for the Minimap mod's coordinates strip
        // (7 more, and a margin of 2).
        assert!(y - 9.0 >= (HOTBAR_BOTTOM + 18 + 9) as f32);
    }

    #[test]
    fn the_pocket_ds_bottom_screen_gets_scale_4_and_slots_by_column() {
        let (scale, gui) = layout([1024, 768]);
        assert_eq!((scale, gui), (4, [256.0, 192.0]));
        // The hotbar spans 128 - 91 = 37 to 37 + 180 across, and 26 - 22 = 4 to 26 down.
        assert_eq!(slot_at(gui, [37.0, 10.0]), Some(0));
        assert_eq!(slot_at(gui, [57.5, 25.0]), Some(1));
        assert_eq!(slot_at(gui, [216.0, 10.0]), Some(8));
        assert_eq!(slot_at(gui, [218.0, 10.0]), None);
        assert_eq!(slot_at(gui, [36.0, 10.0]), None);
        // The bars below it and the coordinates at the bottom.
        assert_eq!(slot_at(gui, [100.0, 45.0]), None);
        assert_eq!(slot_at(gui, [100.0, 180.0]), None);
    }
}
