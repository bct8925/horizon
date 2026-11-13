# Android

Horizon builds for Android (arm64) as an APK. The game runs full screen with a controller: a
handheld's built-in controls (tested target: the AYANEO Pocket DS), or any Bluetooth or USB pad.
Touch works as the mouse in menus. A second screen (the Pocket DS's bottom one) shows the hotbar,
the status bars and the minimap. In-world touch controls, content mods and Microsoft sign-in on the
device are not done yet; native mods can be built in ([Native mods](#native-mods)). Worlds are
offline singleplayer and LAN, under an offline player name.

## What's where

| Part | Path |
|---|---|
| Entry point, folders, logcat, controller JNI | `crates/horizon-android` |
| Arguments and event loop handed in by the host app | `crates/horizon-client/src/platform.rs` |
| The pad the host app fills in (no gilrs on Android) | `crates/horizon-client-input/src/gamepad/host.rs` |
| Surface dropped and remade on `Suspended`/`Resumed` | `Renderer::drop_surface`/`resume_surface` (`horizon-client-render/src/gfx.rs`), the `suspended` handlers in `game/` |
| The second screen: surface and touches handed in, the panel drawn on it | `platform/second_screen.rs`, `game/bottom_panel.rs` (horizon-client), `gfx/second_screen.rs` (horizon-client-render), `bottom.rs` (horizon-android) |
| The app: launcher, GameActivity subclass, bottom-screen activity, manifest, Gradle | `packaging/android` |
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
every push to `android` or `android-ayaneo-pocket`:
- `android-ayaneo-pocket` builds on a self-hosted runner on the PC (label `horizon-android`). It
  keeps its build folders between runs, and copies each APK to `%USERPROFILE%\Horizon APKs`.
- `android` builds on GitHub's runner, which only shows that the base still builds.

With the repository variable `APK_RUNNER` set to `cloud`, both build on GitHub's runner. It signs
with the `ANDROID_DEBUG_KEYSTORE` secret (the PC's debug keystore, base64), so its APKs install
over the PC's. Either way the run keeps the APK as an artifact. `HORIZON_ANDROID_DIR` (absolute
or from the repository root) gives a build its own work folder instead of `.local/android`, so
each branch built on one machine keeps its own build warm.

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

`-PnativeMods=packaging/android/mods.toml` builds the mods that list names instead. Each entry is
either a git repository, branch and folder (fetched with this machine's git sign-in, so private
repositories work) or a registry `.hnmod` by URL and SHA-256. A mod may bring native code, content
(`horizon-mod.toml`, `assets/`, `data/`), or both. When the mods pin different horizon-mixins
commits, the newest applies them all. A mod made for an older Horizon whose hooked functions have
since moved gets a `retarget` table naming where they are now. The content goes into the APK as
`native-content/<id>/`. `ModFiles` unpacks it into the app's files folder at start, replacing it
when the APK changes, and the game reads it there (`HORIZON_VERSION_DIR`), as it reads a native
build's version folder on a PC. `packaging/android/settings/<id>/` holds settings for those mods.
They're copied into `horizon-mods/.horizon-mod-data/<id>/` only where the player has none yet, so
settings changed in the game stay. The `android-ayaneo-mods` branch builds with that list.

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

## The second screen

On a device with a second display (the AYANEO Pocket DS's 1024x768 bottom screen), the game runs on
the main screen and the second one shows a panel in a world: the hotbar (tap a slot to select
it), the experience bar, hearts, armour, food and air, the coordinates, and the minimap when the
Minimap mod is built in ([Native mods](#native-mods)). While the panel shows, the main screen's HUD
leaves those widgets out. Options → Horizon Settings → Extras… → Second Screen →
**Bottom screen** turns it off. The panel is also off on the title screen, where the second screen
keeps the system's own launcher. The panel is vanilla's HUD upside down: the hotbar along
the top, then the experience bar, hearts and food, armour and air, and the coordinates at the
bottom left. The minimap is at the bottom right, 128 GUI pixels across, one per pixel of its image.
It keeps the mod's zoom, shape, turning and outline settings (Options → Mods → Minimap); the
mod's corner and size are for the main screen.

How it fits together:
- `LaunchActivity` is the launcher icon. It starts `HorizonActivity` on the main display, so the
  game goes on the top screen even when the icon is tapped on the bottom one.
- `HorizonActivity` asks the game twice a second whether it wants the second screen
  (`second_screen::wanted`). If it does, it starts `BottomActivity` on that display; if not, it
  closes it.
- `BottomActivity` is an activity, not a `Presentation`. The Pocket DS keeps a full-screen
  overlay of its own (`com.ayaneo.gamewindow`) over the bottom screen unless an app runs there.
  Its window never takes input focus, and once it's up it gives focus back to the game, so the
  controller keeps working.
- Its surface reaches the renderer as a second wgpu surface on the same device, with a HUD
  renderer of its own over the shared atlas. The game draws the panel at most 30 times a second,
  after each frame.

## Debugging

```sh
adb logcat -s horizon        # the game's log lines (stderr goes to logcat)
adb exec-out screencap -p > shot.png
```
