//! What a host app hands the game when the game is not started as a program.
//!
//! On Android there is no command line and no `main`: `horizon-android`'s
//! `android_main` gets the activity, sets the arguments and the app here, then
//! starts the game through [`crate::entry::main`]. Everywhere else nothing is
//! set and the game reads its own command line.
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// The host app went to the background ([`sent_to_background`]) since the game last looked.
static BACKGROUND: AtomicBool = AtomicBool::new(false);

/// The host app is in the background now (Android's `onStop`): the system may end the process
/// from here on without another word, while the window, and so the game's `Suspended`, may
/// stay. The game pauses and saves the next time its loop runs, which this wakes.
pub fn sent_to_background() {
    BACKGROUND.store(true, Ordering::Release);
    #[cfg(target_os = "android")]
    if let Some(app) = android_app() {
        app.create_waker().wake();
    }
}

/// Whether the host app went to the background since the last call.
pub fn take_sent_to_background() -> bool {
    BACKGROUND.swap(false, Ordering::AcqRel)
}

static ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// The arguments the game runs with instead of the process's own. False when
/// they were already set.
pub fn set_args(args: Vec<String>) -> bool {
    ARGS.set(args).is_ok()
}

/// The arguments after the program's name: the ones [`set_args`] gave, or the
/// process's command line.
pub fn args() -> Vec<String> {
    match ARGS.get() {
        Some(args) => args.clone(),
        None => std::env::args().skip(1).collect(),
    }
}

#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

/// The activity the game's event loop runs in. Set once, before the game starts.
#[cfg(target_os = "android")]
pub fn set_android_app(app: AndroidApp) -> bool {
    ANDROID_APP.set(app).is_ok()
}

/// The activity [`set_android_app`] gave.
#[cfg(target_os = "android")]
pub fn android_app() -> Option<&'static AndroidApp> {
    ANDROID_APP.get()
}

/// The game's event loop. On Android it runs in the activity the host app set.
pub fn event_loop() -> Result<winit::event_loop::EventLoop<()>, winit::error::EventLoopError> {
    #[cfg(target_os = "android")]
    {
        use winit::platform::android::EventLoopBuilderExtAndroid;
        let mut builder = winit::event_loop::EventLoop::builder();
        if let Some(app) = android_app() {
            builder.with_android_app(app.clone());
        }
        builder.build()
    }
    #[cfg(not(target_os = "android"))]
    winit::event_loop::EventLoop::new()
}
