// Horizon for Android (docs/android.md). The game is libhorizon_android.so, built from
// crates/horizon-android with cargo-ndk before each build; the resources it plays with are
// pushed from a PC (tools/android_push_resources.ps1), not packed in the APK.
plugins {
    id("com.android.application")
}

android {
    namespace = "dev.playhorizon.android"
    compileSdk = 37
    // r28 is the NDK the Rust build is tested with; set ANDROID_NDK_HOME for another.
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "dev.playhorizon.android"
        // AAudio (cpal) needs 26; Vulkan 1.1 and getifaddrs are there too.
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        debug {
            // The game is the native library; the Java side is one small activity.
            isMinifyEnabled = false
        }
        release {
            isMinifyEnabled = false
            // Sideloaded onto the player's own device: the debug key signs it.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
}

dependencies {
    // Must match the C glue android-activity bundles (its README: 4.4.0, or 4.0.0).
    implementation("androidx.games:games-activity:4.4.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
}

// cargo-ndk builds the game into src/main/jniLibs/arm64-v8a before the APK packs it.
// `-PrustProfile=dev` builds unoptimized (faster to build, far too slow to play).
val rustProfile = (findProperty("rustProfile") as String?) ?: "release"
val workspace = rootDir.resolve("../..").canonicalFile
// `-PnativeMods=<folder>;<folder>` (each absolute or from the repository root) builds native
// mods into the game, as the launcher does on a PC: tools/android_native_mods.py applies them on
// a copy of the source in .local/android/stage, and cargo-ndk builds that copy (with its own
// target folder, .local/android/target).
val nativeMods = (findProperty("nativeMods") as String?)
    ?.split(';')?.map { it.trim() }?.filter { it.isNotEmpty() }.orEmpty()
val androidWork = workspace.resolve(".local/android")
val stageNativeMods = tasks.register<Exec>("stageNativeMods") {
    group = "build"
    description = "Applies native mods on a copy of Horizon's source"
    workingDir = workspace
    val python = if (System.getProperty("os.name").startsWith("Windows")) "python" else "python3"
    val folders = nativeMods.map { workspace.resolve(it).path }
    commandLine(listOf(python, "tools/android_native_mods.py") + folders)
}
val cargoNdk = tasks.register<Exec>("cargoNdk") {
    group = "build"
    description = "Builds libhorizon_android.so with cargo-ndk"
    workingDir = workspace
    if (nativeMods.isNotEmpty()) {
        dependsOn(stageNativeMods)
        workingDir = androidWork.resolve("stage")
        environment("CARGO_TARGET_DIR", androidWork.resolve("target").path)
    }
    val args = mutableListOf(
        "cargo", "ndk", "-t", "arm64-v8a", "-P", "26",
        "-o", projectDir.resolve("src/main/jniLibs").path,
        "build", "-p", "horizon-android",
    )
    if (rustProfile == "release") args += "--release" else args += listOf("--profile", rustProfile)
    commandLine(args)
    // The NDK the app names, unless one is set already (cargo-ndk would take the newest).
    val ndk = System.getenv("ANDROID_NDK_HOME")
        ?: androidComponents.sdkComponents.sdkDirectory.get().asFile
            .resolve("ndk/${android.ndkVersion}").path
    environment("ANDROID_NDK_HOME", ndk)
}
tasks.named("preBuild") { dependsOn(cargoNdk) }
