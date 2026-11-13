//! A second screen beside the window (the AYANEO Pocket DS's bottom one): its own surface on
//! this renderer's device, where a HUD batch of its own is drawn (the game's bottom panel).
//! It shares the HUD atlas with the window's HUD, so it draws the same sprites, text and items.
use super::{Renderer, upload};
use crate::Result;
use crate::hud::{HudBatch, HudRenderer};

pub struct SecondScreen {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    hud: HudRenderer,
    uploads: upload::Uploads,
    /// The HUD's depth attachment: only entity previews test against it.
    depth: wgpu::TextureView,
}

impl Renderer {
    /// Draw on `target` too, `size` pixels: a new second screen, or the same one at a new size.
    pub fn attach_second_screen(
        &mut self,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: [u32; 2],
    ) -> Result<()> {
        self.detach_second_screen();
        let max = self.device.limits().max_texture_dimension_2d;
        if !super::drawable_size(size[0], size[1], max) {
            return Err(format!("the second screen's size {size:?} can't be drawn").into());
        }
        let surface = self.instance.create_surface(target)?;
        let caps = surface.get_capabilities(&self.adapter);
        if caps.formats.is_empty() {
            return Err(
                "the second screen's surface doesn't work with this graphics adapter".into(),
            );
        }
        let format = super::surface_format(&caps.formats, caps.formats[0]);
        // A present that waits for the display would hold up the window's frames: this screen
        // waits only where there is no other way.
        let present_mode = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
            .into_iter()
            .find(|m| caps.present_modes.contains(m))
            .unwrap_or(wgpu::PresentMode::Fifo);
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            caps.alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto)
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size[0],
            height: size[1],
            present_mode,
            desired_maximum_frame_latency: 1,
            alpha_mode,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::default(),
        };
        // Configuring waits for the device: nothing of the window's may be in flight.
        self.queue.wait();
        surface.configure(&self.device, &config);
        let (glint_view, glint_armor_view, glint_sampler) = &self.hud_glint;
        let atlas = self
            .hud
            .atlas_texture
            .as_ref()
            .ok_or("the HUD has no atlas for the second screen")?
            .create_view(&Default::default());
        let mut hud = HudRenderer::new(
            &self.device,
            format,
            &atlas,
            glint_view,
            glint_armor_view,
            glint_sampler,
            false,
        )?;
        hud.atlas_texture = self.hud.atlas_texture.clone();
        let depth = Self::depth(&self.device, &config);
        self.second_screen = Some(SecondScreen {
            surface,
            config,
            hud,
            uploads: upload::Uploads::new(&self.device),
            depth,
        });
        Ok(())
    }

    /// Stop drawing on the second screen and let go of its surface.
    pub fn detach_second_screen(&mut self) {
        if self.second_screen.is_some() {
            self.queue.wait();
            self.second_screen = None;
        }
    }

    /// The second screen's size in pixels, while there is one.
    pub fn second_screen_size(&self) -> Option<[u32; 2]> {
        self.second_screen
            .as_ref()
            .map(|s| [s.config.width, s.config.height])
    }

    /// Clear the second screen to black, draw `batch` on it and show it. False when there was
    /// nothing to draw on, or no frame to draw into this time (it tries again next call).
    pub fn draw_second_screen(&mut self, batch: &HudBatch) -> bool {
        let Some(screen) = &mut self.second_screen else {
            return false;
        };
        let frame = match screen.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                screen.surface.configure(&self.device, &screen.config);
                return false;
            }
            _ => return false,
        };
        let size = [screen.config.width, screen.config.height];
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Second screen"),
            });
        let target = frame.texture.create_view(&Default::default());
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Second screen clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &screen.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        }));
        screen
            .hud
            .prepare(&self.device, &mut screen.uploads, &mut encoder, batch, size);
        screen
            .hud
            .draw(&mut encoder, &target, &screen.depth, batch, size);
        screen.uploads.finish(&encoder);
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        true
    }
}
