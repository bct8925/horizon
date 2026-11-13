//! Horizon on Android (docs/android.md). The activity (`packaging/android`,
//! `HorizonActivity`, a GameActivity) loads this library and calls
//! `android_main` (app.rs) on its own thread. There is no command line: the game's
//! folders and arguments come from the app's storage and its
//! `horizon-android.txt`, then the game runs as the desktop program does.
//!
//! Unsafe code lives here, not in the game (which forbids it): the entry point
//! the activity looks up by name, the JNI calls that bring the controller in
//! and report the app's lifecycle, and handing stdout and stderr to logcat.

#[cfg(target_os = "android")]
mod app;
mod config;
#[cfg(target_os = "android")]
mod logcat;
#[cfg(target_os = "android")]
mod pad;
