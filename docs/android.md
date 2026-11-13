# Android

Horizon builds for Android (arm64) as an APK. The game runs full screen with a controller: a
handheld's built-in controls (tested target: the AYANEO Pocket DS), or any Bluetooth or USB pad.
Touch works as the mouse in menus. In-world touch controls, content mods and Microsoft sign-in on
the device are not done yet; native mods can be built in ([Native mods](#native-mods)). Worlds are
offline singleplayer and LAN, under an offline player name.

## What's where

| Part | Path |
|---|---|
| Entry point, folders, logcat, controller JNI | `crates/horizon-android` |
| Arguments and event loop handed in by the host app | `crates/horizon-client/src/platform.rs` |
| The pad the host app fills in (no gilrs on Android) | `crates/horizon-client-input/src/gamepad/host.rs` |
| Surface dropped and remade on `Suspended`/`Resumed` | `Renderer::drop_surface`/`resume_surface` (`horizon-client-render/src/gfx.rs`), the `suspended` handlers in `game/` |
| The app: GameActivity subclass, manifest, Gradle | `packaging/android` |
| Pushing resources from a PC | `tools/android_push_resources.ps1` |
| Native mods applied for the Android build | `tools/android_native_mods.py` |

The Android build turns `controllers` off, since gilrs and hidapi have no Android backend. It also
turns `mods` off. Features are `--no-default-features --features audio,horizon-extras`, which
`horizon-android`'s manifest sets.

## Build

You need:
- Android Studio, with these from its SDK Manager: an SDK platform, **NDK (Side by side)** 28.2,
  and **Platform-Tools**.
- `rustup target add aarch64-linux-android`
- `cargo install cargo-ndk`

Then:

```sh
cd packaging/android
./gradlew assembleRelease      # runs cargo ndk first; the APK is app/build/outputs/apk/release/
```

You can also open `packaging/android` in Android Studio and press Run. The Gradle task `cargoNdk`
builds `libhorizon_android.so` into `app/src/main/jniLibs/arm64-v8a`. To build the library alone:

```sh
cargo ndk -t arm64-v8a -P 26 build -p horizon-android --release
```

In bct8925's fork, `.github/workflows/android-apk.yml` builds the APK, with the Minimap mod, on
every push to `android` or `android-ayaneo-pocket`. It builds on a self-hosted runner on the PC
(label `horizon-android`), which keeps its build folders between runs and copies each APK to
`%USERPROFILE%\Horizon APKs`. With the repository variable `APK_RUNNER` set to `cloud`, it
builds on GitHub's runner instead, signed by the `ANDROID_DEBUG_KEYSTORE` secret (the PC's debug
keystore, base64), so its APKs install over the PC's. Either way the run keeps the APK as an
artifact.

### Native mods

Native mods ([native-mods.md](native-mods.md)) are built into the game, as the launcher builds them
on a PC. Pass the mods' folders, as [horizon-rust/horizon-mods](https://github.com/horizon-rust/horizon-mods)
has them (`native/<name>`, each with its `compatibility.json`), separated by `;`, absolute or from
the repository root:

```sh
./gradlew assembleRelease "-PnativeMods=../horizon-mods/native/minimap"
```

`tools/android_native_mods.py` then copies this checkout's source (tracked and new files) to
`.local/android/source`. It builds horizon-mixins once, at the commit the mods pin, into
`.local/android/tools`. Then it applies the mods into `.local/android/stage`. cargo-ndk builds that
copy with its own target folder, `.local/android/target`. Your checkout is never edited. Without
`nativeMods`, the game is built without any.

## Install and play

1. On the device, turn on Developer options and then USB debugging, and connect it.
2. `adb install -r packaging/android/app/build/outputs/apk/release/app-release.apk`
3. Open Horizon once, so Android makes its folder, then close it.
4. Push the resources prepared from your own Minecraft: Java Edition. The default source is the
   newest version the Horizon launcher installed on this PC:
   `pwsh tools/android_push_resources.ps1 -Name YourName`
5. Open Horizon.

Files on the device:
- `/sdcard/Android/data/dev.playhorizon.android/files/resources`: the pushed resources.
- `/sdcard/Android/data/dev.playhorizon.android/files/horizon-android.txt`: settings (below).
- The app's private storage, `files/horizon`: worlds, `options.txt`, logs.

`horizon-android.txt`:

```text
# The offline player name.
name=Steve
# More game arguments.
args=--render-distance 8
# An environment variable, e.g. OpenGL ES instead of Vulkan.
env.WGPU_BACKEND=gl
```

The game uses Vulkan on Android, and OpenGL ES where there is no Vulkan. Vulkan keeps the fast
paths OpenGL ES lacks: indirect multi-draws, GPU culling and occlusion culling.

The Pocket DS's Adreno 740 Vulkan driver (build 0676.41, of 2023-08-14) has two quirks the game
works around:
- Its compiler fails to link a shader that indexes a constant table by vertex or pixel, and wgpu
  reports the failed pipeline as a lost device. The game's shaders pack such tables into words
  instead (`box_triangle_corner` in `cull.wgsl`, `cube_corner` in `panorama.wgsl`, the Bayer
  matrix in `terrain.wgsl`).
- Occlusion culling's early depth test now and then leaves sections out, and the world flashes
  black or see-through. On Adreno the boxes are tested in the shader instead, as on Metal
  (`Cull::supported`).

### Benchmark

`args=--benchmark standard` in `horizon-android.txt` runs the game's own benchmark
([benchmark.md](benchmark.md)) with the player's settings. It plays in a world of its own. The report goes to
`/sdcard/Android/data/dev.playhorizon.android/files/benchmarks` (`HORIZON_BENCHMARK_DIR`), where
`adb pull` reads it. Take the line out again afterwards: the game quits when the run ends.

## Controls and lifecycle

- The controller works as it does on a PC (`docs/controllers.md`), on-screen keyboard included.
- Android's Back presses Start, which pauses the game or closes a screen.
- Going to the background (`onStop`) pauses a world in play and saves it, so a swipe from
  Recents loses nothing. winit says so only when the window goes, which may not happen, so
  `HorizonActivity` tells the game itself (`platform::sent_to_background`). The renderer lets go
  of the window's surface when the window goes and makes a new one on return.
- The activity handles every configuration change itself, because winit's event loop can't be
  made twice in a process. When the game quits, the process ends with it.

## Debugging

```sh
adb logcat -s horizon        # the game's log lines (stderr goes to logcat)
adb exec-out screencap -p > shot.png
```
