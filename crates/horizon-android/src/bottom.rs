//! The second screen, from `BottomActivity` (packaging/android): its surface as an
//! `ANativeWindow` and its touches, handed to the game through
//! `platform::second_screen`, which draws its bottom panel there.
//!
//! The JNI names follow the class, `dev.playhorizon.android.BottomActivity`; its natives are
//! static, so the second argument is the class.
use horizon_client::platform::second_screen::{self, Touch, TouchPhase};
use ndk::native_window::NativeWindow;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;

type JniEnv = *mut c_void;
type JClass = *mut c_void;
type JObject = *mut c_void;

/// How long `surfaceDestroyed` waits for the game to let go of the surface. A game stuck
/// longer than this is stuck anyway; Android's own deadline for the UI thread is 5 s.
const RELEASE_WAIT: Duration = Duration::from_secs(2);

/// The surface's window, as wgpu makes a surface on it. Holding it keeps the window.
struct Window(NativeWindow);

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::android())
    }
}

/// `nativeWanted()`: whether the game has something to show on the second screen.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_BottomActivity_nativeWanted(
    _env: JniEnv,
    _class: JClass,
) -> u8 {
    u8::from(second_screen::wanted())
}

/// `nativeSurface(surface, width, height)`: the surface is there, or changed size.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_BottomActivity_nativeSurface(
    env: JniEnv,
    _class: JClass,
    surface: JObject,
    width: i32,
    height: i32,
) {
    // SAFETY: the JNI environment of this call and a live android.view.Surface; the window
    // takes its own reference.
    let Some(window) = (unsafe { NativeWindow::from_surface(env.cast(), surface.cast()) }) else {
        eprintln!("Horizon: the bottom screen's surface has no window");
        return;
    };
    let size = [width.max(1) as u32, height.max(1) as u32];
    second_screen::offer(Arc::new(Window(window)), size);
}

/// `nativeSurfaceGone()`: Android takes the surface away once this returns.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_BottomActivity_nativeSurfaceGone(
    _env: JniEnv,
    _class: JClass,
) {
    if !second_screen::withdraw(RELEASE_WAIT) {
        eprintln!("Horizon: the game kept the bottom screen's surface past its end");
    }
}

/// `nativeTouch(action, id, x, y)`: 0 down, 1 moved, 2 up, 3 cancelled.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_BottomActivity_nativeTouch(
    _env: JniEnv,
    _class: JClass,
    action: i32,
    id: i32,
    x: f32,
    y: f32,
) {
    let phase = match action {
        0 => TouchPhase::Down,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Up,
        _ => TouchPhase::Cancelled,
    };
    second_screen::touch(Touch {
        phase,
        id,
        position: [x, y],
    });
}
