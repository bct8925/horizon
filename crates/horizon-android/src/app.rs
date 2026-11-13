//! The activity's entry point: folders, environment and arguments, then the game.

use crate::{config, logcat};

use horizon_client::platform::{self, AndroidApp};
use std::path::{Path, PathBuf};

#[global_allocator]
static ALLOCATOR: horizon_threads::Allocator = horizon_threads::ALLOCATOR;

/// The mod API level this game serves, as the desktop program keeps it (`main.rs`).
static MOD_API_LEVEL: [u8; 27] = horizon_mods::api_level::MARK;

/// The app's package, as the manifest names it (`packaging/android`).
const PACKAGE: &str = "dev.playhorizon.android";

/// GameActivity's entry point, on the thread android-activity made for it.
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    std::hint::black_box(&MOD_API_LEVEL);
    logcat::forward_stdio("horizon");
    let folders = Folders::of(&app);
    let config = config::Config::read(&folders.external.join(config::FILE));
    if let Err(e) = folders.prepare() {
        eprintln!("Horizon: could not make the game's folders: {e}");
    }
    // Before any thread of the game reads them.
    // SAFETY: the game has not started; the activity's own threads don't read these variables.
    unsafe {
        std::env::set_var("HORIZON_HOME", &folders.home);
        std::env::set_var("TMPDIR", folders.home.join("tmp"));
        std::env::set_var("XDG_RUNTIME_DIR", folders.home.join("run"));
        // Discord has no desktop client to talk to here.
        std::env::set_var("HORIZON_NO_DISCORD", "1");
        // The version folder of a native build, as the launcher's is on a PC: HorizonActivity
        // (ModFiles) unpacks the built-in mods' `native-content/` into the app's files folder.
        if let Some(files) = folders.home.parent() {
            std::env::set_var("HORIZON_VERSION_DIR", files);
        }
        // `--benchmark`'s reports where a PC can read them (`adb pull`), beside the resources.
        std::env::set_var("HORIZON_BENCHMARK_DIR", folders.external.join("benchmarks"));
        // Vulkan, as wgpu picks it, else OpenGL ES; `env.WGPU_BACKEND=gl` in
        // horizon-android.txt asks for OpenGL ES.
        for (key, value) in &config.env {
            std::env::set_var(key, value);
        }
    }
    // Worlds, options.txt and servers.dat are relative to the game's folder, as the launcher
    // starts the game in its home.
    if let Err(e) = std::env::set_current_dir(&folders.home) {
        eprintln!("Horizon: could not enter {}: {e}", folders.home.display());
    }
    let resources = folders.resources();
    eprintln!(
        "Horizon for Android: home {}, resources {}",
        folders.home.display(),
        resources.display()
    );
    let mut args = vec![
        "--home".into(),
        folders.home.display().to_string(),
        "--resources".into(),
        resources.display().to_string(),
        "--fullscreen".into(),
        "--control-socket".into(),
        folders
            .home
            .join("run")
            .join("control.sock")
            .display()
            .to_string(),
    ];
    if let Some(name) = &config.name {
        args.extend(["--name".into(), name.clone()]);
    }
    args.extend(config.args.iter().cloned());
    platform::set_args(args);
    platform::set_android_app(app);
    // The game ends the process itself after an error; either way the activity goes first.
    // SAFETY: registering a plain `extern "C"` function with no arguments.
    unsafe {
        libc::atexit(finish_on_exit);
    }
    // On android_main's own thread (winit polls the looper that belongs to it), which the
    // patched glue gives 8 MB of stack (third_party/android-activity).
    horizon_client::entry::main();
    // GameActivity keeps the process for the next launch, but winit's event loop can't be
    // made twice in one process: end it, so the next launch starts afresh.
    std::process::exit(0);
}

/// `HorizonActivity.nativeStopped()`, from `onStop`: the app is in the background, where Android
/// may end it without another word. winit only says so when the window goes, which it may not.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_playhorizon_android_HorizonActivity_nativeStopped(
    _env: *mut std::ffi::c_void,
    _this: *mut std::ffi::c_void,
) {
    platform::sent_to_background();
}

/// As the process exits: `Activity.finish()`. Android starts an activity again whose
/// process died while it was on the screen, and the game would start, fail the same way
/// and exit again; a finishing activity stays closed.
extern "C" fn finish_on_exit() {
    let Some(app) = platform::android_app() else {
        return;
    };
    // SAFETY: android-activity's pointers to the process's VM and the activity, which
    // outlive this call (the process ends right after it).
    let finished = (|| -> jni::errors::Result<()> {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr().cast()) };
        env.call_method(&activity, "finish", "()V", &[])?;
        Ok(())
    })();
    if let Err(e) = finished {
        eprintln!("Horizon: could not close the activity: {e}");
    }
}

/// Where the game keeps its files on the device.
struct Folders {
    /// The app's private storage: worlds, options, logs (`--home`).
    home: PathBuf,
    /// The app's folder on shared storage, which `adb push` reaches without root:
    /// `/sdcard/Android/data/dev.playhorizon.android/files`.
    external: PathBuf,
}

impl Folders {
    fn of(app: &AndroidApp) -> Self {
        let internal = app
            .internal_data_path()
            .unwrap_or_else(|| PathBuf::from(format!("/data/data/{PACKAGE}/files")));
        let external = app.external_data_path().unwrap_or_else(|| {
            PathBuf::from(format!("/storage/emulated/0/Android/data/{PACKAGE}/files"))
        });
        Self {
            home: internal.join("horizon"),
            external,
        }
    }
    fn prepare(&self) -> std::io::Result<()> {
        for dir in [
            self.home.clone(),
            self.home.join("tmp"),
            self.home.join("run"),
            self.external.clone(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
    /// The prepared resources (`horizon-resources.json` and the rest): pushed from a PC to
    /// shared storage (`tools/android_push_resources.ps1`), or copied into the private folder.
    fn resources(&self) -> PathBuf {
        let private = self.home.join("resources");
        if has_resources(&private) {
            private
        } else {
            self.external.join("resources")
        }
    }
}

fn has_resources(dir: &Path) -> bool {
    dir.join("horizon-resources.json").is_file()
}
