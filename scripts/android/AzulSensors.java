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
                        default: return;
                    }
                    float x = event.values.length > 0 ? event.values[0] : 0f;
                    float y = event.values.length > 1 ? event.values[1] : 0f;
                    float z = event.values.length > 2 ? event.values[2] : 0f;
                    // SensorEvent.timestamp is nanoseconds on an arbitrary
                    // monotonic base, NOT epoch. Converted to ms here; the
                    // Rust side only ever diffs them.
                    nativeOnSensorReading(kind, x, y, z, event.timestamp / 1_000_000L);
                }

                @Override
                public void onAccuracyChanged(Sensor sensor, int accuracy) {}
            };
            // GAME rate (~50 Hz), not FASTEST: fastest floods the JNI boundary
            // on some devices for data no UI can use.
            register(Sensor.TYPE_ACCELEROMETER);
            register(Sensor.TYPE_GYROSCOPE);
            register(Sensor.TYPE_MAGNETIC_FIELD);
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
}
