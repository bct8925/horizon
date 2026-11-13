package dev.playhorizon.android;

import android.content.Context;
import android.content.pm.PackageManager;
import android.content.res.AssetManager;
import android.util.Log;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;

/**
 * The mods built into the APK (tools/android_native_mods.py, packaging/android/mods.toml) carry
 * files the game reads from disk: their content (blocks, items, textures), as the desktop game
 * reads `native-content/` beside its executable, and the settings they start with. The APK holds
 * them as assets; this puts them where the game looks, before it starts:
 *
 * - `native-content/` into the app's files folder, which horizon-android names the game's version
 *   folder (`HORIZON_VERSION_DIR`). It is replaced whenever the APK is, so content always matches
 *   the code built with it.
 * - `mod-settings/<id>/` into the game's `horizon-mods/.horizon-mod-data/<id>/`, only the files
 *   the player doesn't have yet: settings changed in the game stay.
 */
final class ModFiles {
    private static final String TAG = "horizon";
    private static final String CONTENT = "native-content";
    private static final String SETTINGS = "mod-settings";

    private ModFiles() {}

    static void unpack(Context context) {
        File files = context.getFilesDir();
        try {
            long installed = context.getPackageManager()
                    .getPackageInfo(context.getPackageName(), 0).lastUpdateTime;
            File stamp = new File(files, CONTENT + ".stamp");
            if (!String.valueOf(installed).equals(read(stamp))) {
                File content = new File(files, CONTENT);
                delete(content);
                copy(context.getAssets(), CONTENT, content, true);
                write(stamp, String.valueOf(installed));
            }
            File modData = new File(files, "horizon/horizon-mods/.horizon-mod-data");
            copy(context.getAssets(), SETTINGS, modData, false);
        } catch (IOException | PackageManager.NameNotFoundException e) {
            Log.w(TAG, "Horizon: unpacking the mods' files: " + e);
        }
    }

    /** Copies the asset or asset folder `path` to `dest`; a file already there stays unless
     *  `overwrite`. A path the APK doesn't have copies nothing. */
    private static void copy(AssetManager assets, String path, File dest, boolean overwrite)
            throws IOException {
        String[] children = assets.list(path);
        if (children != null && children.length > 0) {
            for (String child : children) {
                copy(assets, path + "/" + child, new File(dest, child), overwrite);
            }
            return;
        }
        if (!overwrite && dest.exists()) {
            return;
        }
        InputStream in;
        try {
            in = assets.open(path);
        } catch (IOException e) {
            return; // An empty folder, or none: the APK was built without mods.
        }
        File parent = dest.getParentFile();
        if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
            in.close();
            throw new IOException("can't make " + parent);
        }
        try (InputStream source = in; OutputStream out = new FileOutputStream(dest)) {
            byte[] buffer = new byte[64 * 1024];
            int n;
            while ((n = source.read(buffer)) > 0) {
                out.write(buffer, 0, n);
            }
        }
    }

    private static void delete(File file) {
        File[] children = file.listFiles();
        if (children != null) {
            for (File child : children) {
                delete(child);
            }
        }
        if (file.exists() && !file.delete()) {
            Log.w(TAG, "Horizon: can't delete " + file);
        }
    }

    private static String read(File file) {
        try (InputStream in = new FileInputStream(file)) {
            byte[] bytes = new byte[64];
            int n = in.read(bytes);
            return n > 0 ? new String(bytes, 0, n, StandardCharsets.UTF_8).trim() : "";
        } catch (IOException e) {
            return null;
        }
    }

    private static void write(File file, String text) throws IOException {
        try (OutputStream out = new FileOutputStream(file)) {
            out.write(text.getBytes(StandardCharsets.UTF_8));
        }
    }
}
