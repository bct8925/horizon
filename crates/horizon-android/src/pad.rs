//! The controller, from `HorizonActivity`: winit hands Android's gamepad buttons on as keys
//! but drops the sticks and triggers, so the activity catches the gamepad's key and motion
//! events itself and calls these. They land in `gamepad::host`, which the game reads as its
//! hardware pad: the same mapping, menus and on-screen keyboard as a controller on a PC.
//!
//! The JNI names follow the activity's class, `dev.playhorizon.android.HorizonActivity`.
use horizon_client::gamepad::{Button, host};
use std::ffi::c_void;

type JniEnv = *mut c_void;
type JObject = *mut c_void;
type JBoolean = u8;

/// `nativeButton(index, down)`: `index` is the button's position in `Button::ALL`.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_HorizonActivity_nativeButton(
    _env: JniEnv,
    _this: JObject,
    index: i32,
    down: JBoolean,
) {
    if let Some(button) = usize::try_from(index).ok().and_then(|i| Button::ALL.get(i)) {
        host::set_button(*button, down != 0);
    }
}

/// `nativeAxes(lx, ly, rx, ry, lt, rt)`: sticks with right and up positive, triggers 0..1.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_HorizonActivity_nativeAxes(
    _env: JniEnv,
    _this: JObject,
    lx: f32,
    ly: f32,
    rx: f32,
    ry: f32,
    lt: f32,
    rt: f32,
) {
    host::set_axes([lx, ly], [rx, ry], [lt, rt]);
}

/// `nativeDisconnect()`: the last controller went away.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_HorizonActivity_nativeDisconnect(
    _env: JniEnv,
    _this: JObject,
) {
    host::disconnect();
}
