// Motion sensors: accelerometer, gyroscope, magnetometer.
//
// Kind contract (sensors/android.rs): 0=Accelerometer, 1=Gyroscope,
// 2=Magnetometer. Unknown kinds are dropped on the Rust side rather than
// mapped to a wrong sensor, so this must not invent codes.

package com.azul.sensors;

import android.app.Activity;
import android.content.Context;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;

public final class AzulSensors {

    private AzulSensors() {}

    private static SensorManager manager;
    private static SensorEventListener listener;

    /** Begin delivering readings. Idempotent. */
    public static void start(Activity activity) {
        if (activity == null || listener != null) {
            return;
        }
        try {
            manager = (SensorManager) activity.getSystemService(Context.SENSOR_SERVICE);
            if (manager == null) {
                return;
            }
            listener = new SensorEventListener() {
                @Override
                public void onSensorChanged(SensorEvent event) {
                    int kind;
                    switch (event.sensor.getType()) {
                        case Sensor.TYPE_ACCELEROMETER: kind = 0; break;
                        case Sensor.TYPE_GYROSCOPE:     kind = 1; break;
                        case Sensor.TYPE_MAGNETIC_FIELD: kind = 2; break;
                        // The FUSED and single-value sensors. Codes continue
                        // the SensorKind discriminant order; map_kind() in
                        // sensors/android.rs is the other half of this
                        // contract and the two must be edited together.
                        case Sensor.TYPE_ROTATION_VECTOR: kind = 3; break;
                        case Sensor.TYPE_GRAVITY: kind = 4; break;
                        case Sensor.TYPE_LINEAR_ACCELERATION: kind = 5; break;
                        case Sensor.TYPE_LIGHT: kind = 6; break;
                        case Sensor.TYPE_PROXIMITY: kind = 7; break;
                        case Sensor.TYPE_PRESSURE: kind = 8; break;
                        case Sensor.TYPE_STEP_COUNTER: kind = 9; break;
                        case Sensor.TYPE_HINGE_ANGLE: kind = 10; break;
                        default: return;
                    }
                    float x = event.values.length > 0 ? event.values[0] : 0f;
                    float y = event.values.length > 1 ? event.values[1] : 0f;
                    float z = event.values.length > 2 ? event.values[2] : 0f;
                    // SensorEvent.timestamp is nanoseconds on an arbitrary
                    // monotonic base, NOT epoch. Converted to ms here; the
                    // Rust side only ever diffs them.
                    nativeOnSensorReading(kind, x, y, z, event.timestamp / 1_000_000L);
                    // The typed proximity answer needs the sensor's own
                    // maximum range: that value IS "far" on a binary sensor.
                    if (kind == 7) {
                        nativeOnProximity(x, event.sensor.getMaximumRange());
                    }
                }

                @Override
                public void onAccuracyChanged(Sensor sensor, int accuracy) {}
            };
            // GAME rate (~50 Hz), not FASTEST: fastest floods the JNI boundary
            // on some devices for data no UI can use.
            register(Sensor.TYPE_ACCELEROMETER);
            register(Sensor.TYPE_GYROSCOPE);
            register(Sensor.TYPE_MAGNETIC_FIELD);
            // Fused orientation/motion. The OS produces these from the three
            // above with drift correction an app cannot reproduce, which is
            // the whole reason to expose them rather than make every app
            // redo the fusion badly.
            register(Sensor.TYPE_ROTATION_VECTOR);
            register(Sensor.TYPE_GRAVITY);
            register(Sensor.TYPE_LINEAR_ACCELERATION);
            // Single-value environment sensors: the reading lands in x, and
            // values[1]/[2] are absent, which the 0f defaults above already
            // handle.
            register(Sensor.TYPE_LIGHT);
            register(Sensor.TYPE_PROXIMITY);
            register(Sensor.TYPE_PRESSURE);
            register(Sensor.TYPE_STEP_COUNTER);
            // TYPE_HINGE_ANGLE is API 30. It compiles against the SDK the
            // build already uses (android-34), and on an older device
            // getDefaultSensor() simply returns null and register() no-ops -
            // so no version guard is needed, only a device that has a hinge.
            register(Sensor.TYPE_HINGE_ANGLE);
        } catch (Throwable t) {
            // A device with no sensors is a normal device.
            listener = null;
        }
    }

    private static void register(int type) {
        Sensor s = manager.getDefaultSensor(type);
        if (s != null) {
            manager.registerListener(listener, s, SensorManager.SENSOR_DELAY_GAME);
        }
    }

    /** Stop delivering readings. Safe to call when never started. */
    public static void stop(Activity activity) {
        try {
            if (manager != null && listener != null) {
                manager.unregisterListener(listener);
            }
        } catch (Throwable ignored) {
        } finally {
            listener = null;
        }
    }

    private static native void nativeOnSensorReading(int kind, float x, float y, float z,
                                                     long timestampMs);
    private static native void nativeOnProximity(float distanceCm, float maxRangeCm);
}
