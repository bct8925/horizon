//! A second screen the host app hands the game, beside its window: the AYANEO Pocket DS's
//! bottom one, where the game draws its bottom panel (`game/bottom_panel.rs`).
//!
//! The game says whether it wants the screen ([`set_wanted`]): the host shows its window only
//! then, and leaves the screen to the system otherwise.
//!
//! The host offers a window (and offers it again when its size changes), withdraws it before
//! the system takes it away, and passes on the touches on it. The game, on its own thread,
//! polls for changes once a frame and says when it let go of a withdrawn window: the host
//! waits for that, since its window must not be drawn on after it returns.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// A window wgpu can make a surface on, from any thread.
pub trait Window: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static {}
impl<T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Window for T {}

/// What changed on the second screen since the game last looked.
pub enum Change {
    /// A window to draw on, `size` in pixels: a new one, or the same one with a new size.
    Offered {
        window: Arc<dyn Window>,
        size: [u32; 2],
    },
    /// The window the game holds is going away: drop its surface, then [`released`].
    Withdrawn,
}

/// A finger on the second screen, in its pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Touch {
    pub phase: TouchPhase,
    pub id: i32,
    pub position: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Moved,
    Up,
    Cancelled,
}

#[derive(Default)]
struct State {
    /// The window offered and not taken yet.
    offered: Option<(Arc<dyn Window>, [u32; 2])>,
    /// Whether the game holds a window (it took an offer and has not released it).
    held: bool,
    /// The host asked for the held window back.
    withdrawing: bool,
    touches: Vec<Touch>,
}

struct Slot {
    state: Mutex<State>,
    released: Condvar,
}

static SLOT: Slot = Slot {
    state: Mutex::new(State {
        offered: None,
        held: false,
        withdrawing: false,
        touches: Vec::new(),
    }),
    released: Condvar::new(),
};

static WANTED: AtomicBool = AtomicBool::new(false);

/// Game: whether it has something to show on the second screen (the setting is on and a
/// world is open).
pub fn set_wanted(wanted: bool) {
    WANTED.store(wanted, Ordering::Relaxed);
}

/// Host: whether to show its window on the second screen.
pub fn wanted() -> bool {
    WANTED.load(Ordering::Relaxed)
}

/// Touches kept for a game that isn't reading them: the newest.
const TOUCHES_KEPT: usize = 64;

fn state() -> std::sync::MutexGuard<'static, State> {
    SLOT.state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Host: a window to draw on, or the same one at a new size.
pub fn offer(window: Arc<dyn Window>, size: [u32; 2]) {
    let mut state = state();
    state.offered = Some((window, size));
    state.withdrawing = false;
}

/// Host: the window is going away. Returns once the game has let go of it, or after
/// `timeout` when the game doesn't (true when it did, or never held it).
pub fn withdraw(timeout: Duration) -> bool {
    let mut state = state();
    state.offered = None;
    state.touches.clear();
    if !state.held {
        return true;
    }
    state.withdrawing = true;
    let end = Instant::now() + timeout;
    while state.held {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        state = SLOT
            .released
            .wait_timeout(state, left)
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
    true
}

/// Host: a finger on the second screen.
pub fn touch(touch: Touch) {
    let mut state = state();
    if state.touches.len() >= TOUCHES_KEPT {
        state.touches.remove(0);
    }
    state.touches.push(touch);
}

/// Game: what changed since the last call. A withdrawal comes first.
pub fn poll() -> Option<Change> {
    let mut state = state();
    if state.withdrawing && state.held {
        return Some(Change::Withdrawn);
    }
    let (window, size) = state.offered.take()?;
    state.held = true;
    Some(Change::Offered { window, size })
}

/// Game: it dropped the surface of the window it held.
pub fn released() {
    let mut state = state();
    state.held = false;
    state.withdrawing = false;
    SLOT.released.notify_all();
}

/// Game: the touches since the last call, oldest first.
pub fn touches() -> Vec<Touch> {
    std::mem::take(&mut state().touches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::raw_window_handle::{DisplayHandle, HandleError, WindowHandle};

    /// A window nothing draws on.
    struct Fake;
    impl HasWindowHandle for Fake {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }
    impl HasDisplayHandle for Fake {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }

    #[test]
    fn offers_withdrawals_and_touches() {
        // Nothing held: withdrawing returns at once.
        assert!(withdraw(Duration::ZERO));
        assert!(poll().is_none());
        offer(Arc::new(Fake), [1024, 768]);
        offer(Arc::new(Fake), [1024, 734]);
        // The newest offer.
        match poll() {
            Some(Change::Offered { size, .. }) => assert_eq!(size, [1024, 734]),
            _ => panic!("no offer"),
        }
        assert!(poll().is_none());
        touch(Touch {
            phase: TouchPhase::Down,
            id: 0,
            position: [10.0, 20.0],
        });
        assert_eq!(touches().len(), 1);
        assert!(touches().is_empty());
        // Held: the game doesn't release, so withdrawing times out and the game sees it.
        assert!(!withdraw(Duration::from_millis(10)));
        assert!(matches!(poll(), Some(Change::Withdrawn)));
        let host = std::thread::spawn(|| withdraw(Duration::from_secs(10)));
        std::thread::sleep(Duration::from_millis(20));
        released();
        assert!(host.join().unwrap());
        assert!(poll().is_none());
    }
}
