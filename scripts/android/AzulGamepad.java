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
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.hardware.input.InputManager;
import android.os.Build;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;

import java.util.HashMap;
import java.util.Map;

public final class AzulGamepad {

    private AzulGamepad() {}

    private static InputManager inputManager;
    private static InputManager.InputDeviceListener deviceListener;

    // The per-controller sensor listeners, so they can be unregistered when
    // the pad leaves. Keyed by device id; a pad with no IMU has no entry.
    private static final Map<Integer, PadSensorListener> sensorListeners = new HashMap<>();

    /**
     * A controller's own accelerometer / gyroscope.
     *
     * The SensorManager here comes from InputDevice.getSensorManager(), which
     * scopes it to ONE input device - so this is the pad's motion, not the
     * phone's. That is the whole reason the values go to GamepadState rather
     * than to the ordinary sensor path.
     */
    private static final class PadSensorListener implements SensorEventListener {
        private final int deviceId;

        PadSensorListener(int deviceId) {
            this.deviceId = deviceId;
        }

        @Override
        public void onSensorChanged(SensorEvent event) {
            if (event == null || event.values == null || event.values.length < 3) {
                return;
            }
            // The sensor TYPE is passed through unchanged: the Rust side keys
            // on Android's own constant, so there is no second numbering that
            // could drift out of sync with this file.
            nativeOnMotionSensor(deviceId, event.sensor.getType(),
                    event.values[0], event.values[1], event.values[2]);
        }

        @Override
        public void onAccuracyChanged(Sensor sensor, int accuracy) {}
    }

    /**
     * Subscribe to a controller's IMU, if it has one.
     *
     * API 31. The version check is NOT optional even though this compiles
     * against a newer SDK: getSensorManager() does not exist on an older
     * device and calling it there is a NoSuchMethodError at runtime, which
     * would take the whole listener down rather than degrade.
     */
    private static void attachSensors(int deviceId) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            return;
        }
        if (sensorListeners.containsKey(deviceId)) {
            return;
        }
        try {
            InputDevice device = InputDevice.getDevice(deviceId);
            if (device == null) {
                return;
            }
            SensorManager sensors = device.getSensorManager();
            if (sensors == null) {
                return;
            }
            Sensor accel = sensors.getDefaultSensor(Sensor.TYPE_ACCELEROMETER);
            Sensor gyro = sensors.getDefaultSensor(Sensor.TYPE_GYROSCOPE);
            if (accel == null && gyro == null) {
                // Most controllers have neither; not an error.
                return;
            }
            PadSensorListener listener = new PadSensorListener(deviceId);
            // SENSOR_DELAY_GAME rather than FASTEST: this feeds a per-frame
            // state snapshot, so a higher rate would only add wake-ups.
            if (accel != null) {
                sensors.registerListener(listener, accel, SensorManager.SENSOR_DELAY_GAME);
            }
            if (gyro != null) {
                sensors.registerListener(listener, gyro, SensorManager.SENSOR_DELAY_GAME);
            }
            sensorListeners.put(deviceId, listener);
        } catch (Throwable ignored) {
        }
    }

    /** Unsubscribe a controller's IMU. Safe to call for a pad that had none. */
    private static void detachSensors(int deviceId) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            return;
        }
        PadSensorListener listener = sensorListeners.remove(deviceId);
        if (listener == null) {
            return;
        }
        try {
            // The device is GONE by the time a removal arrives, so its
            // SensorManager cannot be fetched to unregister against. The
            // platform tears the per-device listeners down with the device;
            // dropping the reference here is what stops us leaking one per
            // reconnect.
            InputDevice device = InputDevice.getDevice(deviceId);
            if (device != null) {
                SensorManager sensors = device.getSensorManager();
                if (sensors != null) {
                    sensors.unregisterListener(listener);
                }
            }
        } catch (Throwable ignored) {
        }
    }

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
                        attachSensors(deviceId);
                    }
                }

                @Override
                public void onInputDeviceRemoved(int deviceId) {
                    // Cannot re-query a removed device to ask whether it WAS a
                    // gamepad, so every removal is reported; the Rust side
                    // ignores ids it never saw added.
                    nativeOnDeviceChanged(deviceId, 0);
                    detachSensors(deviceId);
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
                    attachSensors(id);
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
            for (Integer id : new java.util.ArrayList<>(sensorListeners.keySet())) {
                detachSensors(id);
            }
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

    /**
     * A captured-pointer event (AzulInputView.onCapturedPointerEvent). Only a
     * SOURCE_TOUCHPAD event is a pad's touch surface (8f-i-a-ii); a captured
     * mouse is not ours. The touchpad is its OWN InputDevice, separate from
     * the pad's gamepad device, so it is paired to the gamepad device with
     * the same vendor/product - when exactly one matches. Two identical pads
     * are ambiguous and nothing is guessed (8f-i-a-ii-a).
     *
     * Positions are normalised to 0..1 over the device's reported motion
     * range, y flipped to the bottom-left origin GamepadState::touchpad_x
     * documents. Only the first pointer is forwarded (8f-i-a-ii-a).
     *
     * @return true when consumed.
     */
    public static boolean onCapturedPointer(MotionEvent event) {
        if (event == null
                || (event.getSource() & InputDevice.SOURCE_TOUCHPAD) != InputDevice.SOURCE_TOUCHPAD) {
            return false;
        }
        InputDevice surface = event.getDevice();
        if (surface == null) {
            return false;
        }
        int padId = -1;
        if (isGamepad(surface.getId())) {
            padId = surface.getId();
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.KITKAT) {
            int vendor = surface.getVendorId();
            int product = surface.getProductId();
            int found = 0;
            for (int id : InputDevice.getDeviceIds()) {
                InputDevice d = InputDevice.getDevice(id);
                if (d != null && isGamepad(id)
                        && d.getVendorId() == vendor && d.getProductId() == product) {
                    padId = id;
                    found++;
                }
            }
            if (found != 1) {
                return false;
            }
        }
        if (padId < 0) {
            return false;
        }
        int action = event.getActionMasked();
        boolean active = action != MotionEvent.ACTION_UP
                && action != MotionEvent.ACTION_CANCEL
                && event.getPointerCount() > 0;
        float x = 0f;
        float y = 0f;
        if (active) {
            InputDevice.MotionRange rx = surface.getMotionRange(MotionEvent.AXIS_X, event.getSource());
            InputDevice.MotionRange ry = surface.getMotionRange(MotionEvent.AXIS_Y, event.getSource());
            x = normalise(event.getX(0), rx);
            y = 1f - normalise(event.getY(0), ry);
        }
        nativeOnTouchpad(padId, active ? 1 : 0, x, y);
        return true;
    }

    private static float normalise(float value, InputDevice.MotionRange range) {
        if (range == null || range.getRange() <= 0f) {
            return value;
        }
        return (value - range.getMin()) / range.getRange();
    }

    private static native void nativeOnButton(int deviceId, int keycode, int isDown);

    private static native void nativeOnTouchpad(int deviceId, int active, float x, float y);

    private static native void nativeOnAxes(int deviceId, float x, float y, float z, float rz,
                                            float ltrigger, float rtrigger,
                                            float hatX, float hatY);

    private static native void nativeOnDeviceChanged(int deviceId, int connected);

    private static native void nativeOnMotionSensor(int deviceId, int kind,
                                                    float x, float y, float z);
}
