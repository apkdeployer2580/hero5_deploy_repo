package kr.heroeslore.five;

import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.hardware.input.InputManager;
import android.os.Build;
import android.os.Bundle;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.Window;
import android.view.WindowManager;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.widget.FrameLayout;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileReader;
import java.io.IOException;

import com.google.androidgamesdk.GameActivity;

/**
 * Thin GameActivity subclass. Everything else lives in the Rust cdylib
 * (libmain.so), entered via android_main(). The static loadLibrary and the
 * manifest's android.app.lib_name meta-data both name "main".
 */
public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("main");
    }

    /** Set while the InputManager listener is registered, so it is removed once. */
    private InputManager.InputDeviceListener padListener;

    /**
     * The 세로 setting. Read here, before the window exists, because the
     * orientation has to be asked for before {@code super.onCreate} builds it
     * and the keypad has to be in the layout {@code onCreateSurfaceView} makes.
     * The native side reads the same file for the canvas; the token is
     * {@code hl5_core::ScreenMode::letter}.
     */
    private boolean portrait;

    /** {@code <files>/hl5_resolution.txt} says P. The shell's storage is
     *  {@code internal_data_path()/files}, i.e. getFilesDir()/files. */
    private boolean portraitChosen() {
        File f = new File(new File(getFilesDir(), "files"), "hl5_resolution.txt");
        try (BufferedReader r = new BufferedReader(new FileReader(f))) {
            String s = r.readLine();
            if (s == null) {
                return false;
            }
            s = s.trim();
            return s.equalsIgnoreCase("p") || s.equalsIgnoreCase("portrait");
        } catch (IOException e) {
            return false;
        }
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        portrait = portraitChosen();
        if (portrait) {
            setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT);
        }
        super.onCreate(savedInstanceState);
        Hl5Save.attach(this);
        watchGamepads();
        // The game is played with long stretches of reading and no touches, so
        // without this the display times out mid-session. FLAG_KEEP_SCREEN_ON
        // rather than a wake lock: it needs no permission and is scoped to this
        // window, so it cannot outlive the app.
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        // Fill the display, camera cutout included. Without this the system
        // letterboxes the window around the punch-hole in landscape, and the
        // player reads the resulting bar as "the sides are cut off". The
        // native side stretches its picture over every pixel it is given, so
        // the cutout area holds game, not black.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams lp = getWindow().getAttributes();
            lp.layoutInDisplayCutoutMode = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                    ? WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                    : WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(lp);
        }
        hideSystemBars();
    }

    /**
     * GameActivity puts its SurfaceView in a FrameLayout of its own here. In 세로
     * the keypad joins it and cuts the SurfaceView down to a 3:4 box above the
     * keys; the native side then sees a 3:4 window and draws its 240x320
     * canvas into it, touches included, with no idea a keypad exists.
     */
    @Override
    protected void onCreateSurfaceView() {
        super.onCreateSurfaceView();
        if (portrait && mSurfaceView != null && mSurfaceView.getParent() instanceof FrameLayout) {
            PhoneKeypad.install(this, (FrameLayout) mSurfaceView.getParent(), mSurfaceView);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        // The legacy visibility flags are cleared by the system whenever the
        // bars are shown, so re-assert on every focus gain rather than only at
        // create time.
        if (hasFocus) {
            hideSystemBars();
            // Re-report the pad. Belt and braces for two cases the listener
            // does not cover: a pad that was already attached when the
            // process started (that report is made from onCreate, and the
            // native side may not have a VM to hand it to yet), and one
            // attached or removed while this activity was in the background.
            nativePadPresent(anyGamepad());
        }
    }

    /**
     * Hide the status and navigation bars. Sticky/transient on purpose: an edge
     * swipe brings them back briefly so the player can still leave the app, then
     * they hide again by themselves.
     */
    private void hideSystemBars() {
        final Window w = getWindow();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            w.setDecorFitsSystemWindows(false);
            WindowInsetsController c = w.getInsetsController();
            if (c != null) {
                c.hide(WindowInsets.Type.statusBars() | WindowInsets.Type.navigationBars());
                c.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            w.getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY);
        }
    }

    // ---- Gamepad ----------------------------------------------------------
    //
    // Captured HERE, at the top of the dispatch chain, and not left to the
    // GameActivity native input path, because that path cannot carry a pad:
    // android_native_app_glue's default_motion_filter drops every
    // non-touchscreen MotionEvent (so stick axes never reach Rust at all), and
    // winit reads any MotionAction.MOVE that did survive as a TOUCH — it would
    // hand the stick's -1..1 axis values to the app as touch coordinates.
    // Buttons take the same JNI route so all pad input has one owner, and so a
    // remap in hl5_gamepad.txt applies to every button on every pad.
    //
    // Non-pad events fall through to super untouched, so touch, volume and the
    // phone's own back key behave exactly as before.

    private static boolean fromSource(int source, int mask) {
        return (source & mask) == mask;
    }

    /** Full-mask idiom: a plain `!= 0` would also match keyboards, which share
     *  the BUTTON class bit with gamepads. */
    private static boolean isGamepadSource(int source) {
        return fromSource(source, InputDevice.SOURCE_GAMEPAD)
                || fromSource(source, InputDevice.SOURCE_JOYSTICK);
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent e) {
        if (isGamepadSource(e.getSource())) {
            int action = e.getAction();
            if (action == KeyEvent.ACTION_DOWN || action == KeyEvent.ACTION_UP) {
                nativePadButton(e.getKeyCode(), action == KeyEvent.ACTION_DOWN,
                        e.getRepeatCount() > 0);
                return true;
            }
        }
        return super.dispatchKeyEvent(e);
    }

    @Override
    public boolean dispatchGenericMotionEvent(MotionEvent e) {
        if (fromSource(e.getSource(), InputDevice.SOURCE_JOYSTICK)
                && e.getAction() == MotionEvent.ACTION_MOVE) {
            // Android folds the ACTION_MOVEs of one frame into one event, the
            // older samples kept as history. For the stick only the newest
            // position matters, but the hat and the triggers are buttons: a hat
            // press and its release inside one frame would otherwise arrive as
            // "neutral" alone, and the press would never happen. So every older
            // sample that changes one of them goes first, in order, and then the
            // newest.
            int n = e.getHistorySize();
            for (int h = 0; h < n; h++) {
                if (buttonsChanged(e, h)) {
                    sendPadSample(e, h);
                }
            }
            sendPadSample(e, -1);
            return true;
        }
        return super.dispatchGenericMotionEvent(e);
    }

    /** The hat and trigger values last handed to the native side. */
    private float sentHatX, sentHatY, sentLt, sentRt;

    /** One sample of {@code e}: history index {@code h}, or the newest for -1. */
    private static float sample(MotionEvent e, int axis, int h) {
        return h < 0 ? e.getAxisValue(axis) : e.getHistoricalAxisValue(axis, h);
    }

    /** The triggers, which many Bluetooth pads report ONLY as axes and under
     *  either name depending on the pad, so whichever of the pair is larger. */
    private static float leftTrigger(MotionEvent e, int h) {
        return Math.max(sample(e, MotionEvent.AXIS_LTRIGGER, h), sample(e, MotionEvent.AXIS_BRAKE, h));
    }

    private static float rightTrigger(MotionEvent e, int h) {
        return Math.max(sample(e, MotionEvent.AXIS_RTRIGGER, h), sample(e, MotionEvent.AXIS_GAS, h));
    }

    private boolean buttonsChanged(MotionEvent e, int h) {
        return sample(e, MotionEvent.AXIS_HAT_X, h) != sentHatX
                || sample(e, MotionEvent.AXIS_HAT_Y, h) != sentHatY
                || leftTrigger(e, h) != sentLt
                || rightTrigger(e, h) != sentRt;
    }

    /** Left stick + hat (most pads report their d-pad as HAT axes rather than
     *  as DPAD key events) + the triggers, as they were at sample {@code h}. */
    private void sendPadSample(MotionEvent e, int h) {
        sentHatX = sample(e, MotionEvent.AXIS_HAT_X, h);
        sentHatY = sample(e, MotionEvent.AXIS_HAT_Y, h);
        sentLt = leftTrigger(e, h);
        sentRt = rightTrigger(e, h);
        nativePadAxes(
                flattened(e, MotionEvent.AXIS_X, h), flattened(e, MotionEvent.AXIS_Y, h),
                sentHatX, sentHatY, sentLt, sentRt);
    }

    /** One analog stick axis with the DEVICE'S OWN slop taken out.
     *
     *  <p>A stick at rest does not report 0. Android has the pad declare how
     *  far it wanders in {@link InputDevice.MotionRange#getFlat()}, and the
     *  documented handling is to treat anything inside that as centred and to
     *  rescale the rest so full deflection is still 1. Passing the raw value
     *  through instead leaves a permanently-deflected stick, which the native
     *  side reads as a thumb still on the joystick: the hero walks and never
     *  stops.
     *
     *  <p>This only fixes the pads that are honest about their slop. The ones
     *  that report {@code flat == 0} and drift anyway are caught by the host's
     *  own release threshold — see {@code RELEASE} in {@code gamepad.rs}. The
     *  hat axes are left alone: they are digital, −1/0/1 exactly.
     */
    private static float flattened(MotionEvent e, int axis, int h) {
        float v = sample(e, axis, h);
        InputDevice dev = e.getDevice();
        if (dev == null) {
            return v;
        }
        InputDevice.MotionRange range = dev.getMotionRange(axis, e.getSource());
        if (range == null) {
            return v;
        }
        float flat = range.getFlat();
        if (!(flat > 0f) || flat >= 1f) {
            return v;
        }
        float mag = Math.abs(v);
        if (mag <= flat) {
            return 0f;
        }
        float scaled = (mag - flat) / (1f - flat);
        return v < 0f ? -scaled : scaled;
    }

    /** Is any gamepad attached right now? Drives the on-screen badges, which
     *  the operator asked to appear only while a pad is connected. */
    private static boolean anyGamepad() {
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice d = InputDevice.getDevice(id);
            if (d != null && !d.isVirtual() && isGamepadSource(d.getSources())) {
                return true;
            }
        }
        return false;
    }

    /** Report the current answer, and keep reporting it as pads come and go.
     *  A Bluetooth pad is routinely connected after the game is already up. */
    private void watchGamepads() {
        nativePadPresent(anyGamepad());
        InputManager im = (InputManager) getSystemService(INPUT_SERVICE);
        if (im == null) {
            return;
        }
        padListener = new InputManager.InputDeviceListener() {
            @Override
            public void onInputDeviceAdded(int deviceId) {
                nativePadPresent(anyGamepad());
            }

            @Override
            public void onInputDeviceRemoved(int deviceId) {
                nativePadPresent(anyGamepad());
            }

            @Override
            public void onInputDeviceChanged(int deviceId) {
                nativePadPresent(anyGamepad());
            }
        };
        im.registerInputDeviceListener(padListener, null);
    }

    /** The 세이브 내보내기 / 불러오기 document screens answer here. */
    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        if (!Hl5Save.onActivityResult(this, requestCode, resultCode, data)) {
            super.onActivityResult(requestCode, resultCode, data);
        }
    }

    @Override
    protected void onDestroy() {
        if (padListener != null) {
            InputManager im = (InputManager) getSystemService(INPUT_SERVICE);
            if (im != null) {
                im.unregisterInputDeviceListener(padListener);
            }
            padListener = null;
        }
        super.onDestroy();
    }

    /** Implemented in Rust — lib.rs `pad_jni`, resolved by JNI symbol name. */
    private static native void nativePadButton(int keyCode, boolean down, boolean repeat);

    private static native void nativePadAxes(
            float x, float y, float hatX, float hatY, float lt, float rt);

    private static native void nativePadPresent(boolean present);

    /** One key of the 세로 keypad, by its index in hl5_core::portrait::KEYPAD. */
    static native void nativeKeypad(int index, boolean down);
}
