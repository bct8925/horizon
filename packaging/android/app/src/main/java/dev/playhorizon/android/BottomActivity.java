package dev.playhorizon.android;

import android.app.Activity;
import android.app.ActivityOptions;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.view.Display;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;

/**
 * The second screen (the AYANEO Pocket DS's bottom one): an activity there, filled with a
 * surface the game draws its bottom panel on (horizon-android's bottom.rs). An activity rather
 * than a Presentation: the Pocket DS keeps an overlay of its own over that screen unless an
 * app runs on it.
 *
 * Its window never takes input focus, and once it is up it hands the focus back to the game's
 * screen, so the controller keeps driving the game; touches on it still come here.
 */
public class BottomActivity extends Activity implements SurfaceHolder.Callback {
    private static final int TOUCH_DOWN = 0, TOUCH_MOVE = 1, TOUCH_UP = 2, TOUCH_CANCEL = 3;

    /** Whether the game has something to show here: its Bottom screen setting, in a world. */
    static native boolean nativeWanted();
    static native void nativeSurface(Surface surface, int width, int height);
    static native void nativeSurfaceGone();
    static native void nativeTouch(int action, int id, float x, float y);

    /** The one shown, for the game's activity to close. */
    private static BottomActivity current;

    static BottomActivity current() {
        return current;
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        current = this;
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
                | WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE);
        SurfaceView view = new SurfaceView(this);
        view.getHolder().addCallback(this);
        view.setOnTouchListener((v, event) -> touch(event));
        setContentView(view);
        hideSystemBars();
    }

    /** Starting here made this screen the focused one: the game's screen gets it back. */
    @Override
    protected void onResume() {
        super.onResume();
        getWindow().getDecorView().post(() -> {
            Intent game = new Intent(this, HorizonActivity.class)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK
                            | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT
                            | Intent.FLAG_ACTIVITY_NO_ANIMATION);
            ActivityOptions options = ActivityOptions.makeBasic();
            options.setLaunchDisplayId(Display.DEFAULT_DISPLAY);
            startActivity(game, options.toBundle());
        });
    }

    @Override
    protected void onDestroy() {
        if (current == this) {
            current = null;
        }
        super.onDestroy();
    }

    private void hideSystemBars() {
        View decor = getWindow().getDecorView();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowInsetsController controller = decor.getWindowInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.systemBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            decor.setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                    | View.SYSTEM_UI_FLAG_FULLSCREEN
                    | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                    | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                    | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
        }
    }

    /** Every finger, in the surface's pixels. */
    private boolean touch(MotionEvent event) {
        int masked = event.getActionMasked();
        switch (masked) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN:
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_POINTER_UP: {
                int i = event.getActionIndex();
                boolean down = masked == MotionEvent.ACTION_DOWN
                        || masked == MotionEvent.ACTION_POINTER_DOWN;
                nativeTouch(down ? TOUCH_DOWN : TOUCH_UP, event.getPointerId(i),
                        event.getX(i), event.getY(i));
                return true;
            }
            case MotionEvent.ACTION_MOVE:
                for (int i = 0; i < event.getPointerCount(); i++) {
                    nativeTouch(TOUCH_MOVE, event.getPointerId(i), event.getX(i), event.getY(i));
                }
                return true;
            case MotionEvent.ACTION_CANCEL:
                for (int i = 0; i < event.getPointerCount(); i++) {
                    nativeTouch(TOUCH_CANCEL, event.getPointerId(i), event.getX(i),
                            event.getY(i));
                }
                return true;
            default:
                return false;
        }
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {}

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        nativeSurface(holder.getSurface(), width, height);
    }

    /** Returns once the game has let go of the surface, as Android requires. */
    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        nativeSurfaceGone();
    }
}
