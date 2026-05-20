//! Apple (iOS / macOS) motion-sensor backend — CoreMotion `CMMotionManager`.
//!
//! Uses CoreMotion's **pull** API (`startAccelerometerUpdates` +
//! `accelerometerData`, no handler block): [`start`] creates the manager and
//! begins sampling; [`poll`] — called once per layout pass — reads the
//! latest sample of each sensor and parks it through `push_sensor_reading`,
//! which the pass then folds into `SensorManager`. The pull API avoids the
//! `NSOperationQueue` + handler-block plumbing the push API needs, and the
//! per-frame poll cadence matches how the manager is consumed anyway.
//!
//! The manager is a process-lifetime singleton: [`start`] leaks a +1 retain
//! into [`MANAGER`] so it outlives the call and keeps sampling. It's created
//! once (the dispatcher's `ensure_started` is OnceLock-guarded) and read each
//! frame by [`poll`] on the layout thread — CoreMotion's pull API is designed
//! for exactly this polling use.
//!
//! Units: CoreMotion reports acceleration in **G**, so we scale to azul-core's
//! m/s² ([`G_TO_MS2`]); gyroscope (rad/s) and magnetometer (µT) already match
//! Android's units (research/03 §2) and pass through. Axis *sign* conventions
//! differ between iOS and Android — the `SensorReading::magnitude` helper that
//! shake/tilt detection uses is sign-agnostic, and per-axis sign calibration
//! is a future refinement. The sample `timestamp` (seconds since boot, from
//! the `CMLogItem` superclass) becomes `timestamp_ms` so the manager's
//! change-detection sees a stable stamp when the sensor hasn't advanced.

use std::sync::atomic::{AtomicPtr, Ordering};

use objc2::rc::Retained;
use objc2_core_motion::CMMotionManager;

use azul_core::sensors::{SensorKind, SensorReading};
use azul_layout::managers::sensors::push_sensor_reading;

/// Standard gravity — CoreMotion accelerometer is in G; azul-core is m/s².
const G_TO_MS2: f32 = 9.806_65;
/// Target sample interval (s). CoreMotion clamps to the hardware max rate.
const UPDATE_INTERVAL_S: f64 = 1.0 / 60.0;

/// The process-lifetime `CMMotionManager` (leaked +1 retain — see module
/// docs). Null until [`start`] runs; read by [`poll`].
static MANAGER: AtomicPtr<CMMotionManager> = AtomicPtr::new(core::ptr::null_mut());

/// Create the motion manager and begin sampling every available sensor.
/// Called once per process via the dispatcher's OnceLock.
pub fn start() {
    unsafe {
        let mgr = CMMotionManager::new();
        if mgr.isAccelerometerAvailable() {
            mgr.setAccelerometerUpdateInterval(UPDATE_INTERVAL_S);
            mgr.startAccelerometerUpdates();
        }
        if mgr.isGyroAvailable() {
            mgr.setGyroUpdateInterval(UPDATE_INTERVAL_S);
            mgr.startGyroUpdates();
        }
        if mgr.isMagnetometerAvailable() {
            mgr.setMagnetometerUpdateInterval(UPDATE_INTERVAL_S);
            mgr.startMagnetometerUpdates();
        }
        // Leak a +1 retain so the manager keeps sampling for the process
        // lifetime; `poll` reads it through this pointer.
        MANAGER.store(Retained::into_raw(mgr), Ordering::Release);
    }
}

/// Read the latest sample of each sensor and park it for the layout pass.
/// No-op until [`start`] has published the manager.
pub fn poll() {
    let ptr = MANAGER.load(Ordering::Acquire);
    if ptr.is_null() {
        return;
    }
    // SAFETY: `start` published a leaked, process-lifetime manager; the
    // pull-API data accessors are read-only and safe to call per frame.
    let mgr: &CMMotionManager = unsafe { &*ptr };
    unsafe {
        if let Some(d) = mgr.accelerometerData() {
            let a = d.acceleration();
            push_sensor_reading(SensorReading {
                kind: SensorKind::Accelerometer,
                x: a.x as f32 * G_TO_MS2,
                y: a.y as f32 * G_TO_MS2,
                z: a.z as f32 * G_TO_MS2,
                timestamp_ms: (d.timestamp() * 1000.0) as u64,
            });
        }
        if let Some(d) = mgr.gyroData() {
            let r = d.rotationRate();
            push_sensor_reading(SensorReading {
                kind: SensorKind::Gyroscope,
                x: r.x as f32,
                y: r.y as f32,
                z: r.z as f32,
                timestamp_ms: (d.timestamp() * 1000.0) as u64,
            });
        }
        if let Some(d) = mgr.magnetometerData() {
            let m = d.magneticField();
            push_sensor_reading(SensorReading {
                kind: SensorKind::Magnetometer,
                x: m.x as f32,
                y: m.y as f32,
                z: m.z as f32,
                timestamp_ms: (d.timestamp() * 1000.0) as u64,
            });
        }
    }
}
