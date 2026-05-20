//! Android motion-sensor backend (JNI).
//!
//! `start` calls a Java helper `com.azul.sensors.AzulSensors` (same
//! Rust/Java split as `AzulBiometric` / `AzulGeolocation`):
//! `start(Activity)` registers a `SensorEventListener` for the default
//! accelerometer, gyroscope, and magnetometer (`SENSOR_DELAY_GAME`). From
//! its `onSensorChanged`, the Java side calls back into the
//! `nativeOnSensorReading` symbol below with `(kind, x, y, z, timestampMs)`,
//! which parks a [`SensorReading`] in azul-layout's async channel
//! (`push_sensor_reading`); the layout pass folds it into `SensorManager`.
//!
//! Android reports accelerometer in m/s², gyroscope in rad/s, magnetometer
//! in µT — already azul-core's units (research/03 §2), so the Java side
//! forwards `SensorEvent.values[0..3]` verbatim.
//!
//! Pending (non-Rust): the `AzulSensors.java` helper plus its sensor
//! permissions (none needed for accel/gyro/mag; `HIGH_SAMPLING_RATE_SENSORS`
//! only above 200 Hz). Until it ships, `find_class` fails and `start` is a
//! no-op (no samples flow), exactly like the biometric backend pre-shim.

use azul_core::sensors::{SensorKind, SensorReading};
use azul_layout::managers::sensors::push_sensor_reading;

#[cfg(target_os = "android")]
pub fn start() {
    let _ = attach(|env, activity| {
        use jni::objects::JValue;
        let class = env.find_class("com/azul/sensors/AzulSensors").ok()?;
        env.call_static_method(
            class,
            "start",
            "(Landroid/app/Activity;)V",
            &[JValue::Object(&activity)],
        )
        .ok()?;
        Some(())
    });
}

#[cfg(not(target_os = "android"))]
pub fn start() {}

// Kind contract with the Java side: 0=Accelerometer, 1=Gyroscope,
// 2=Magnetometer (mirrors the `SensorKind` discriminant order). Unknown
// codes are dropped rather than mapped to a wrong sensor.
#[cfg(target_os = "android")]
fn map_kind(code: i32) -> Option<SensorKind> {
    match code {
        0 => Some(SensorKind::Accelerometer),
        1 => Some(SensorKind::Gyroscope),
        2 => Some(SensorKind::Magnetometer),
        _ => None,
    }
}

/// Attach the current thread to the published JavaVM and run `f` with the
/// `JNIEnv` + the activity `JObject`. `None` if the VM/activity aren't
/// published or `f` short-circuits. Mirrors the biometric / geolocation
/// backend attach sequence.
#[cfg(target_os = "android")]
fn attach<R>(
    f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> Option<R>,
) -> Option<R> {
    use jni::objects::JObject;
    use jni::JavaVM;

    let vm_ptr = crate::desktop::shell2::android::java_vm_ptr();
    let activity_ptr = crate::desktop::shell2::android::activity_ptr();
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    let vm = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    f(&mut env, activity)
}

// ───────── JNI inbound: Java → Rust ─────────────────────────────────

/// Receives one motion sample from `AzulSensors`' `SensorEventListener`.
/// Maps the kind code and parks a [`SensorReading`] in the async channel
/// for the next layout pass; an unrecognized kind is dropped. Runs on
/// Android's sensor thread — `push_sensor_reading` is the thread-safe
/// hand-off into azul-layout (no `LayoutWindow` handle here).
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_sensors_AzulSensors_nativeOnSensorReading(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    kind: jni::sys::jint,
    x: jni::sys::jfloat,
    y: jni::sys::jfloat,
    z: jni::sys::jfloat,
    timestamp_ms: jni::sys::jlong,
) {
    if let Some(kind) = map_kind(kind) {
        push_sensor_reading(SensorReading {
            kind,
            x,
            y,
            z,
            timestamp_ms: timestamp_ms as u64,
        });
    }
}
