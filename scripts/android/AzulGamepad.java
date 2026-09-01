// Game controllers.
//
// A NativeActivity DOES receive gamepad KeyEvents and MotionEvents through its
// own input queue, so why route them through Java at all? Because
// InputManager.InputDeviceListener is the only hotplug notification Android
// offers — native code cannot learn that a controller connected, only that a
// button was pressed on one. Without it, a pad plugged in after launch is
// invisible until the user presses something, and one unplugged mid-game never
// reports as gone.

package com.azul.gamepad;

import android.app.Activity;
import android.hardware.input.InputManager;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;

public final class AzulGamepad {

    private AzulGamepad() {}

    private static InputManager inputManager;
    private static InputManager.InputDeviceListener deviceListener;

    /** Start watching for controllers. Reports the ones already attached. */
    public static void attach(Activity activity) {
        if (activity == null || deviceListener != null) {
            return;
        }
        try {
            inputManager = (InputManager) activity.getSystemService(Activity.INPUT_SERVICE);
            if (inputManager == null) {
                return;
            }
            deviceListener = new InputManager.InputDeviceListener() {
                @Override
                public void onInputDeviceAdded(int deviceId) {
                    if (isGamepad(deviceId)) {
                        nativeOnDeviceChanged(deviceId, 1);
                    }
                }

                @Override
                public void onInputDeviceRemoved(int deviceId) {
                    // Cannot re-query a removed device to ask whether it WAS a
                    // gamepad, so every removal is reported; the Rust side
                    // ignores ids it never saw added.
                    nativeOnDeviceChanged(deviceId, 0);
                }

                @Override
                public void onInputDeviceChanged(int deviceId) {}
            };
            inputManager.registerInputDeviceListener(deviceListener, null);
            // Controllers connected BEFORE the listener existed produce no
            // callback, so enumerate once.
            for (int id : InputDevice.getDeviceIds()) {
                if (isGamepad(id)) {
                    nativeOnDeviceChanged(id, 1);
                }
            }
        } catch (Throwable t) {
            deviceListener = null;
        }
    }

    public static void detach(Activity activity) {
        try {
            if (inputManager != null && deviceListener != null) {
                inputManager.unregisterInputDeviceListener(deviceListener);
            }
        } catch (Throwable ignored) {
        } finally {
            deviceListener = null;
        }
    }

    private static boolean isGamepad(int deviceId) {
        InputDevice d = InputDevice.getDevice(deviceId);
        if (d == null) {
            return false;
        }
        int sources = d.getSources();
        return (sources & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                || (sources & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK;
    }

    /**
     * Forward a controller button. Called from AzulActivity's key hooks.
     *
     * @return true when handled, so the activity can stop the event.
     */
    public static boolean onKey(KeyEvent event, boolean isDown) {
        if (event == null || !isGamepad(event.getDeviceId())) {
            return false;
        }
        nativeOnButton(event.getDeviceId(), event.getKeyCode(), isDown ? 1 : 0);
        return true;
    }

    /** Forward a stick/trigger sample. */
    public static boolean onMotion(MotionEvent event) {
        if (event == null || !isGamepad(event.getDeviceId())) {
            return false;
        }
        nativeOnAxes(event.getDeviceId(),
                event.getAxisValue(MotionEvent.AXIS_X),
                event.getAxisValue(MotionEvent.AXIS_Y),
                event.getAxisValue(MotionEvent.AXIS_Z),
                event.getAxisValue(MotionEvent.AXIS_RZ),
                event.getAxisValue(MotionEvent.AXIS_LTRIGGER),
                event.getAxisValue(MotionEvent.AXIS_RTRIGGER),
                event.getAxisValue(MotionEvent.AXIS_HAT_X),
                event.getAxisValue(MotionEvent.AXIS_HAT_Y));
        return true;
    }

    private static native void nativeOnButton(int deviceId, int keycode, int isDown);

    private static native void nativeOnAxes(int deviceId, float x, float y, float z, float rz,
                                            float ltrigger, float rtrigger,
                                            float hatX, float hatY);

    private static native void nativeOnDeviceChanged(int deviceId, int connected);
}
