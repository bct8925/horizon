//! A controller the host app reads itself and hands to the game: Android's
//! built-in controls (the AYANEO Pocket DS's, any Bluetooth pad), which gilrs
//! has no backend for. `horizon-android`'s activity forwards the system's
//! gamepad events here; the game reads them as its hardware pad.
use super::{Button, PadState};
use std::sync::Mutex;

static PAD: Mutex<Option<PadState>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Option<PadState>) -> R) -> R {
    f(&mut PAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner))
}

/// A button went down or up. The pad counts as connected from its first input on.
pub fn set_button(button: Button, down: bool) {
    with(|pad| pad.get_or_insert_with(PadState::default).set(button, down));
}

/// The sticks (`[x, y]`, right and up positive, -1..1) and the triggers (0..1).
pub fn set_axes(left: [f32; 2], right: [f32; 2], triggers: [f32; 2]) {
    let clamp = |v: [f32; 2], low: f32| {
        v.map(|a| {
            if a.is_finite() {
                a.clamp(low, 1.0)
            } else {
                0.0
            }
        })
    };
    with(|pad| {
        let pad = pad.get_or_insert_with(PadState::default);
        pad.left = clamp(left, -1.0);
        pad.right = clamp(right, -1.0);
        pad.triggers = clamp(triggers, 0.0);
    });
}

/// The controller went away: everything released.
pub fn disconnect() {
    with(|pad| *pad = None);
}

/// The pad's state now, or `None` when the host has none.
pub fn state() -> Option<PadState> {
    with(|pad| *pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_and_axes_reach_the_state() {
        disconnect();
        assert_eq!(state(), None);
        set_button(Button::South, true);
        set_axes([0.5, -2.0], [f32::NAN, 1.0], [-1.0, 0.25]);
        let pad = state().unwrap();
        assert!(pad.held(Button::South));
        assert_eq!(pad.left, [0.5, -1.0]);
        assert_eq!(pad.right, [0.0, 1.0]);
        assert_eq!(pad.triggers, [0.0, 0.25]);
        set_button(Button::South, false);
        assert!(!state().unwrap().held(Button::South));
        disconnect();
        assert_eq!(state(), None);
    }
}
