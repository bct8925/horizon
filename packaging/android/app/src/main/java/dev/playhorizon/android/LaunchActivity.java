package dev.playhorizon.android;

import android.app.Activity;
import android.app.ActivityOptions;
import android.content.Intent;
import android.os.Bundle;
import android.view.Display;

/**
 * The launcher icon. A dual-screen handheld's second screen has a launcher of its own, and an
 * app started there runs there: this starts the game on the main screen instead, and the
 * second screen gets the game's panel (BottomActivity).
 */
public class LaunchActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent game = new Intent(this, HorizonActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        ActivityOptions options = ActivityOptions.makeBasic();
        options.setLaunchDisplayId(Display.DEFAULT_DISPLAY);
        startActivity(game, options.toBundle());
        finish();
    }
}
