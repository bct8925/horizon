package dev.playhorizon.android;

import android.app.ActivityOptions;
import android.content.Intent;
import android.hardware.display.DisplayManager;
import android.hardware.input.InputManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.view.Display;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;

import com.google.androidgamesdk.GameActivity;

/**
 * Horizon's activity: GameActivity runs the game (libhorizon_android.so, crates/horizon-android).
 *
 * winit hands the game Android's gamepad buttons as keys but drops the sticks and triggers, so
 * this activity takes every controller event first and gives the game the whole pad
 * (horizon-android's pad.rs, gamepad::host): the AYANEO Pocket DS's built-in controls or any
 * other controller. Touch and keyboard events go on to the game through GameActivity.
 *
 * A second screen (the Pocket DS's bottom one) gets the game's bottom panel (BottomActivity)
 * while the game asks for it.
 */
public class HorizonActivity extends GameActivity
        implements InputManager.InputDeviceListener, DisplayManager.DisplayListener {
    // Button::ALL's order (crates/horizon-client-input/src/gamepad.rs).
    private static final int SOUTH = 0, EAST = 1, WEST = 2, NORTH = 3;
    private static final int LEFT_BUMPER = 4, RIGHT_BUMPER = 5, LEFT_TRIGGER = 6, RIGHT_TRIGGER = 7;
    private static final int SELECT = 8, START = 9, LEFT_STICK = 10, RIGHT_STICK = 11;
    private static final int DPAD_UP = 12, DPAD_DOWN = 13, DPAD_LEFT = 14, DPAD_RIGHT = 15;

    /** How often the activity asks the game whether it wants the second screen. */
    private static final long BOTTOM_CHECK_MS = 500;
    /** How long a panel just launched has to come up before it is launched again. */
    private static final long BOTTOM_LAUNCH_MS = 3000;

    /** How long a Back press holds Start, so the game sees it in a frame. */
    private static final long BACK_HOLD_MS = 80;

    private native void nativeButton(int index, boolean down);
    private native void nativeAxes(float lx, float ly, float rx, float ry, float lt, float rt);
    private native void nativeDisconnect();
    /** The app went to the background: the game pauses and saves its world. */
    private native void nativeStopped();

    private final Handler handler = new Handler(Looper.getMainLooper());
    private InputManager inputManager;
    private DisplayManager displayManager;
    private boolean started;
    /** When the panel was last launched (`SystemClock.uptimeMillis`), or 0. */
    private long bottomLaunched;
    /** The bottom panel follows what the game wants while the activity is started. */
    private final Runnable checkBottom = new Runnable() {
        @Override
        public void run() {
            if (!started) {
                return;
            }
            if (BottomActivity.nativeWanted()) {
                showBottom();
            } else {
                hideBottom();
            }
            handler.postDelayed(this, BOTTOM_CHECK_MS);
        }
    };
    /** The hat's last direction, -1, 0 or 1 on each axis: the D-pad as buttons. */
    private int hatX, hatY;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Before the game starts with GameActivity's onCreate: the files of the mods built in.
        ModFiles.unpack(this);
        super.onCreate(savedInstanceState);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        inputManager = (InputManager) getSystemService(INPUT_SERVICE);
        inputManager.registerInputDeviceListener(this, handler);
        displayManager = (DisplayManager) getSystemService(DISPLAY_SERVICE);
        displayManager.registerDisplayListener(this, handler);
        hideSystemBars();
    }

    @Override
    protected void onStart() {
        super.onStart();
        started = true;
        handler.removeCallbacks(checkBottom);
        handler.post(checkBottom);
    }

    @Override
    protected void onStop() {
        started = false;
        handler.removeCallbacks(checkBottom);
        hideBottom();
        // Android may end a stopped app without another word, and the window (whose loss is
        // the game's own cue) may outlive this.
        nativeStopped();
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        inputManager.unregisterInputDeviceListener(this);
        displayManager.unregisterDisplayListener(this);
        super.onDestroy();
    }

    /** The screen the panel goes on: a presentation display other than the game's. */
    private Display bottomDisplay() {
        int own = getWindowManager().getDefaultDisplay().getDisplayId();
        for (Display display : displayManager.getDisplays(
                DisplayManager.DISPLAY_CATEGORY_PRESENTATION)) {
            if (display.getDisplayId() != own) {
                return display;
            }
        }
        return null;
    }

    /** The bottom panel on the second screen, when there is one and it isn't up yet. */
    private void showBottom() {
        if (!started || BottomActivity.current() != null
                || (bottomLaunched != 0
                        && SystemClock.uptimeMillis() - bottomLaunched < BOTTOM_LAUNCH_MS)) {
            return;
        }
        Display display = bottomDisplay();
        if (display == null) {
            return;
        }
        Intent panel = new Intent(this, BottomActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_MULTIPLE_TASK
                        | Intent.FLAG_ACTIVITY_NO_ANIMATION);
        ActivityOptions options = ActivityOptions.makeBasic();
        options.setLaunchDisplayId(display.getDisplayId());
        bottomLaunched = SystemClock.uptimeMillis();
        try {
            startActivity(panel, options.toBundle());
        } catch (RuntimeException e) {
            // The display went away, or the system won't put an activity on it.
            bottomLaunched = 0;
        }
    }

    private void hideBottom() {
        bottomLaunched = 0;
        BottomActivity shown = BottomActivity.current();
        if (shown != null) {
            shown.finish();
        }
    }

    @Override
    public void onDisplayAdded(int displayId) {}

    @Override
    public void onDisplayChanged(int displayId) {}

    @Override
    public void onDisplayRemoved(int displayId) {
        BottomActivity shown = BottomActivity.current();
        if (shown != null && shown.getDisplay() != null
                && shown.getDisplay().getDisplayId() == displayId) {
            hideBottom();
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            hideSystemBars();
        }
    }

    /** The game fills the screen; a swipe from the edge shows the bars for a moment. */
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

    private static boolean fromController(int source) {
        return (source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK;
    }

    /** A controller key's button, or -1. */
    private static int button(int keyCode) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_BUTTON_A: return SOUTH;
            case KeyEvent.KEYCODE_BUTTON_B: return EAST;
            case KeyEvent.KEYCODE_BUTTON_X: return WEST;
            case KeyEvent.KEYCODE_BUTTON_Y: return NORTH;
            case KeyEvent.KEYCODE_BUTTON_L1: return LEFT_BUMPER;
            case KeyEvent.KEYCODE_BUTTON_R1: return RIGHT_BUMPER;
            case KeyEvent.KEYCODE_BUTTON_L2: return LEFT_TRIGGER;
            case KeyEvent.KEYCODE_BUTTON_R2: return RIGHT_TRIGGER;
            case KeyEvent.KEYCODE_BUTTON_SELECT: return SELECT;
            case KeyEvent.KEYCODE_BUTTON_START: return START;
            case KeyEvent.KEYCODE_BUTTON_THUMBL: return LEFT_STICK;
            case KeyEvent.KEYCODE_BUTTON_THUMBR: return RIGHT_STICK;
            case KeyEvent.KEYCODE_DPAD_UP: return DPAD_UP;
            case KeyEvent.KEYCODE_DPAD_DOWN: return DPAD_DOWN;
            case KeyEvent.KEYCODE_DPAD_LEFT: return DPAD_LEFT;
            case KeyEvent.KEYCODE_DPAD_RIGHT: return DPAD_RIGHT;
            default: return -1;
        }
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        int action = event.getAction();
        if (action != KeyEvent.ACTION_DOWN && action != KeyEvent.ACTION_UP) {
            return super.dispatchKeyEvent(event);
        }
        boolean down = action == KeyEvent.ACTION_DOWN;
        int keyCode = event.getKeyCode();
        if (fromController(event.getSource())) {
            int index = button(keyCode);
            // Some controllers report B as Back.
            if (index < 0 && keyCode == KeyEvent.KEYCODE_BACK) {
                index = EAST;
            }
            if (index >= 0) {
                if (event.getRepeatCount() == 0) {
                    nativeButton(index, down);
                }
                return true;
            }
        }
        // Back (the gesture, a navigation key): Start, which pauses the game and closes a
        // screen, instead of closing the app. Held a moment so a quick press reaches a frame.
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            if (down && event.getRepeatCount() == 0) {
                nativeButton(START, true);
                handler.postDelayed(() -> nativeButton(START, false), BACK_HOLD_MS);
            }
            return true;
        }
        return super.dispatchKeyEvent(event);
    }

    /** Both of a stick's or trigger's axes the device may report it on, the larger one. */
    private static float either(MotionEvent event, int a, int b) {
        return Math.max(event.getAxisValue(a), event.getAxisValue(b));
    }

    @Override
    public boolean dispatchGenericMotionEvent(MotionEvent event) {
        if (!fromController(event.getSource()) || event.getAction() != MotionEvent.ACTION_MOVE) {
            return super.dispatchGenericMotionEvent(event);
        }
        // Android's sticks are down-positive; the game's are up-positive.
        nativeAxes(
                event.getAxisValue(MotionEvent.AXIS_X),
                -event.getAxisValue(MotionEvent.AXIS_Y),
                event.getAxisValue(MotionEvent.AXIS_Z),
                -event.getAxisValue(MotionEvent.AXIS_RZ),
                either(event, MotionEvent.AXIS_LTRIGGER, MotionEvent.AXIS_BRAKE),
                either(event, MotionEvent.AXIS_RTRIGGER, MotionEvent.AXIS_GAS));
        // A D-pad that reports as a hat instead of keys.
        int x = Math.round(event.getAxisValue(MotionEvent.AXIS_HAT_X));
        int y = Math.round(event.getAxisValue(MotionEvent.AXIS_HAT_Y));
        if (x != hatX) {
            nativeButton(DPAD_LEFT, x < 0);
            nativeButton(DPAD_RIGHT, x > 0);
            hatX = x;
        }
        if (y != hatY) {
            nativeButton(DPAD_UP, y < 0);
            nativeButton(DPAD_DOWN, y > 0);
            hatY = y;
        }
        return true;
    }

    private boolean anyController() {
        for (int id : inputManager.getInputDeviceIds()) {
            InputDevice device = inputManager.getInputDevice(id);
            if (device != null && fromController(device.getSources())) {
                return true;
            }
        }
        return false;
    }

    @Override
    public void onInputDeviceAdded(int deviceId) {}

    @Override
    public void onInputDeviceChanged(int deviceId) {}

    @Override
    public void onInputDeviceRemoved(int deviceId) {
        if (!anyController()) {
            hatX = 0;
            hatY = 0;
            nativeDisconnect();
        }
    }
}
