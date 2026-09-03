//! Android motion-sensor backend (JNI).
//!
//! `start` calls a Java helper `com.azul.sensors.AzulSensors` (same
//! Rust/Java split as `AzulBiometric` / `AzulGeolocation`):
//! `start(Activity)` registers a `SensorEventListener` for every sensor
//! azul models - the raw accelerometer/gyroscope/magnetometer, the fused
//! rotation-vector/gravity/linear-acceleration the OS derives from them,
//! and the single-value light/proximity/pressure/step-count/hinge-angle
//! sensors (`SENSOR_DELAY_GAME`). From
//! its `onSensorChanged`, the Java side calls back into the
//! `nativeOnSensorReading` symbol below with `(kind, x, y, z, timestampMs)`,
//! which parks a [`SensorReading`] in azul-layout's async channel
//! (`push_sensor_reading`); the layout pass folds it into `SensorManager`.
//!
//! Android reports accelerometer in m/s², gyroscope in rad/s, magnetometer
//! in µT — already azul-core's units (research/03 §2), so the Java side
//! forwards `SensorEvent.values[0..3]` verbatim.
//!
//! `AzulSensors.java` SHIPPED (`scripts/android/`); the "pending" note that
//! stood here was stale. No sensor permission is needed for any of these
//! (`HIGH_SAMPLING_RATE_SENSORS` applies only above 200 Hz, and this
//! registers at GAME rate ~50 Hz). A device missing a given sensor returns
//! null from `getDefaultSensor` and simply never reports it, which is why
//! nothing here is version-gated.

use azul_core::sensors::{SensorKind, SensorReading};
use azul_layout::managers::sensors::push_sensor_reading;

#[cfg(target_os = "android")]
pub fn start() {
    let _ = attach(|env, activity| {
        use jni::objects::JValue;
        let class =
            crate::desktop::extra::find_app_class(env, &activity, "com/azul/sensors/AzulSensors")?;
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

// Kind contract with the Java side, mirroring the `SensorKind` discriminant
// order. `AzulSensors.java`'s switch is the other half of this contract and
// the two must be edited together - a code that means one sensor here and
// another there would report a barometer's hPa as a step count, silently.
//
// Unknown codes are DROPPED rather than mapped to a nearby sensor: a reading
// attributed to the wrong kind is worse than a missing one, because the
// units differ and nothing downstream can detect it.
#[cfg(target_os = "android")]
fn map_kind(code: i32) -> Option<SensorKind> {
    match code {
        0 => Some(SensorKind::Accelerometer),
        1 => Some(SensorKind::Gyroscope),
        2 => Some(SensorKind::Magnetometer),
        3 => Some(SensorKind::RotationVector),
        4 => Some(SensorKind::Gravity),
        5 => Some(SensorKind::LinearAcceleration),
        6 => Some(SensorKind::AmbientLight),
        7 => Some(SensorKind::Proximity),
        8 => Some(SensorKind::Barometer),
        9 => Some(SensorKind::StepCounter),
        10 => Some(SensorKind::HingeAngle),
        _ => None,
    }
}

/// Attach the current thread to the published JavaVM and run `f` with the
/// `JNIEnv` + the activity `JObject`. `None` if the VM/activity aren't
/// published or `f` short-circuits. Mirrors the biometric / geolocation
/// backend attach sequence.
#[cfg(target_os = "android")]
fn attach<R>(f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JObject) -> Option<R>) -> Option<R> {
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

/// The typed proximity answer (8e-i-a-ii's sibling on Android). Android's
/// own rule: a sensor that cannot range reports its MAXIMUM RANGE when far
/// and a lesser value when near; a ranging sensor reports the distance in
/// cm. So the maximum range IS far, zero IS near (touching, or the usual
/// binary near value), and anything between is a distance the app judges.
#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_azul_sensors_AzulSensors_nativeOnProximity(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    distance_cm: jni::sys::jfloat,
    max_range_cm: jni::sys::jfloat,
) {
    use azul_core::sensors::{DistanceUnit, Proximity, ProximityDistance};
    use azul_layout::managers::sensors::push_proximity;

    let proximity = if max_range_cm > 0.0 && distance_cm >= max_range_cm {
        Proximity::Far
    } else if distance_cm <= 0.0 {
        Proximity::Near
    } else {
        Proximity::Distance(ProximityDistance {
            value: distance_cm,
            unit: DistanceUnit::Centimeters,
        })
    };
    push_proximity(proximity);
}
