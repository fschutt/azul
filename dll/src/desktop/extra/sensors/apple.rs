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
//! # The FUSED kinds (8e-i-a) come from a different object
//!
//! `CMDeviceMotion` is not a fourth sensor: it is the OS's SENSOR FUSION, and
//! its `gravity`, `userAcceleration` and `attitude` are what an app cannot
//! reproduce from the raw three - the drift correction is the whole point.
//! It joins the same pull API, so it is one more `deviceMotion()` read per
//! frame and no new plumbing.
//!
//! `gravity` and `userAcceleration` always sum back to the raw accelerometer;
//! they are separate kinds precisely because the SPLIT is what the fusion buys.
//!
//! # The barometer and the step counter are iOS-only, and push-only
//!
//! `CMAltimeter` and `CMPedometer` have no pull API at all, so unlike
//! everything else here they take handler blocks. They are also gated to iOS:
//! both classes are in fact PRESENT on this macOS (checked with
//! `objc_getClass`, not assumed), but a Mac has neither sensor, and linking a
//! class the docs mark iOS-only would make an older macOS fail to LAUNCH
//! rather than quietly report nothing.
//!
//! `AmbientLight` and `HingeAngle` are not filled on Apple - iOS has no public
//! ambient-light API and no hinge (see 8e-i-a-i). `Proximity` IS, on iOS: it
//! is a boolean there (`UIDevice.proximityState`), which the typed
//! `Proximity::Near` / `Far` answer carries without inventing a distance for
//! "far" - the mismatch that kept it unfilled (8e-i-a-i, user ruling).
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

use super::units::G_TO_MS2;
/// Target sample interval (s). CoreMotion clamps to the hardware max rate.
const UPDATE_INTERVAL_S: f64 = 1.0 / 60.0;

/// The process-lifetime `CMMotionManager` (leaked +1 retain — see module
/// docs). Null until [`start`] runs; read by [`poll`].
static MANAGER: AtomicPtr<CMMotionManager> = AtomicPtr::new(core::ptr::null_mut());

/// Create the motion manager and begin sampling every available sensor.
/// Called once per process via the dispatcher's OnceLock.
pub fn start() {
    #[cfg(target_os = "ios")]
    start_proximity();
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
        // The FUSED stream. Available separately from the raw three: a device
        // can have an accelerometer and no fusion, and asking for device
        // motion on one that cannot fuse simply never produces a sample.
        if mgr.isDeviceMotionAvailable() {
            mgr.setDeviceMotionUpdateInterval(UPDATE_INTERVAL_S);
            mgr.startDeviceMotionUpdates();
        }
        // Leak a +1 retain so the manager keeps sampling for the process
        // lifetime; `poll` reads it through this pointer.
        MANAGER.store(Retained::into_raw(mgr), Ordering::Release);
    }

    #[cfg(target_os = "ios")]
    start_push_only_sensors();
}

/// `CMAltimeter` (pressure) and `CMPedometer` (steps).
///
/// These are the two sensors with NO pull API, so they take handler blocks and
/// park their samples in the same channel the polled ones use - which is what
/// makes the difference invisible to the layout pass.
///
/// Both are also PERMISSIONED on iOS: they need `NSMotionUsageDescription` in
/// the app's Info.plist, and without it the handler is called once with an
/// error and never again. That degrades to "no readings", which is the same
/// outcome as absent hardware and needs no separate handling.
#[cfg(target_os = "ios")]
fn start_push_only_sensors() {
    use block2::RcBlock;
    use objc2_core_motion::{CMAltimeter, CMPedometer};
    use objc2_foundation::{NSDate, NSOperationQueue};

    unsafe {
        if CMAltimeter::isRelativeAltitudeAvailable() {
            let altimeter = CMAltimeter::new();
            let queue = NSOperationQueue::new();
            let handler = RcBlock::new(
                move |data: *mut objc2_core_motion::CMAltitudeData,
                      _err: *mut objc2_foundation::NSError| {
                    let Some(data) = data.as_ref() else {
                        return;
                    };
                    // KILOPASCALS, like iio and unlike the WinRT barometer.
                    let kpa = data.pressure().doubleValue() as f32;
                    push_sensor_reading(SensorReading {
                        kind: SensorKind::Barometer,
                        x: super::units::kpa_to_hpa(kpa),
                        y: 0.0,
                        z: 0.0,
                        timestamp_ms: (data.timestamp() * 1000.0) as u64,
                    });
                },
            );
            altimeter.startRelativeAltitudeUpdatesToQueue_withHandler(
                &queue,
                RcBlock::as_ptr(&handler),
            );
            // The altimeter, the queue and the block must all OUTLIVE this
            // scope - CoreMotion holds the block and calls it later - so all
            // three are leaked deliberately, once, for the process lifetime.
            core::mem::forget(handler);
            core::mem::forget(queue);
            core::mem::forget(altimeter);
        }

        if CMPedometer::isStepCountingAvailable() {
            let pedometer = CMPedometer::new();
            // FROM NOW, not from boot. iOS counts from a date you give it,
            // where Android's `TYPE_STEP_COUNTER` counts from boot - so this
            // reports steps since the app started. Still monotonic, which is
            // what `SensorKind::StepCounter` actually asks for: it tells apps
            // to take differences against their own baseline rather than to
            // expect an absolute origin.
            let from = NSDate::dateWithTimeIntervalSinceNow(0.0);
            let handler = RcBlock::new(
                move |data: *mut objc2_core_motion::CMPedometerData,
                      _err: *mut objc2_foundation::NSError| {
                    let Some(data) = data.as_ref() else {
                        return;
                    };
                    push_sensor_reading(SensorReading {
                        kind: SensorKind::StepCounter,
                        x: data.numberOfSteps().floatValue(),
                        y: 0.0,
                        z: 0.0,
                        timestamp_ms: now_ms(),
                    });
                },
            );
            pedometer
                .startPedometerUpdatesFromDate_withHandler(&from, RcBlock::as_ptr(&handler));
            core::mem::forget(handler);
            core::mem::forget(pedometer);
        }
    }
}

/// Wall-clock milliseconds, for the push-only sensors whose sample carries no
/// `CMLogItem` timestamp.
#[cfg(target_os = "ios")]
/// Turn proximity monitoring on. UIKit answers by leaving
/// `proximityMonitoringEnabled` false on a device without the sensor, which
/// is the only way it says so.
#[cfg(target_os = "ios")]
fn start_proximity() {
    use objc2::runtime::AnyObject;
    unsafe {
        let device: *mut AnyObject = objc2::msg_send![objc2::class!(UIDevice), currentDevice];
        if device.is_null() {
            return;
        }
        let _: () = objc2::msg_send![device, setProximityMonitoringEnabled: true];
        let enabled: bool = objc2::msg_send![device, isProximityMonitoringEnabled];
        if !enabled {
            crate::plog_info!("[sensors] no proximity sensor on this device");
        }
    }
}

/// Publish `UIDevice.proximityState` when it changes. A boolean, so the
/// typed `Near` / `Far` answer is the whole truth (8e-i-a-i).
#[cfg(target_os = "ios")]
fn poll_proximity() {
    use core::sync::atomic::AtomicU8;

    use azul_core::sensors::Proximity;
    use azul_layout::managers::sensors::push_proximity;
    use objc2::runtime::AnyObject;

    /// 2 = not yet read; UIKit's answer is published on every change.
    static LAST: AtomicU8 = AtomicU8::new(2);
    unsafe {
        let device: *mut AnyObject = objc2::msg_send![objc2::class!(UIDevice), currentDevice];
        if device.is_null() {
            return;
        }
        let enabled: bool = objc2::msg_send![device, isProximityMonitoringEnabled];
        if !enabled {
            return;
        }
        let near: bool = objc2::msg_send![device, proximityState];
        let now = u8::from(near);
        if LAST.swap(now, Ordering::Relaxed) != now {
            push_proximity(if near { Proximity::Near } else { Proximity::Far });
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// MWA-C-sensors: non-destructive IMU presence probe for AzCapability —
/// the capability report used to hardcode available=false on macOS while
/// this real check existed unused. Reuses the started manager when
/// available (avoids a second CMMotionManager); otherwise creates a
/// temporary one just for the availability bits.
pub fn has_motion_hardware() -> bool {
    unsafe {
        let existing = MANAGER.load(Ordering::Acquire);
        if !existing.is_null() {
            let mgr = &*existing;
            return mgr.isAccelerometerAvailable()
                || mgr.isGyroAvailable()
                || mgr.isMagnetometerAvailable();
        }
        let mgr = CMMotionManager::new();
        mgr.isAccelerometerAvailable() || mgr.isGyroAvailable() || mgr.isMagnetometerAvailable()
    }
}

/// Read the latest sample of each sensor and park it for the layout pass.
/// No-op until [`start`] has published the manager.
pub fn poll() {
    // Independent of CoreMotion: a device with no IMU can still have the
    // proximity sensor.
    #[cfg(target_os = "ios")]
    poll_proximity();
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
        // ONE object, THREE kinds - the fused stream is a single read and the
        // reason `CMDeviceMotion` is worth having at all.
        if let Some(d) = mgr.deviceMotion() {
            let ts = (d.timestamp() * 1000.0) as u64;
            let g = d.gravity();
            push_sensor_reading(SensorReading {
                kind: SensorKind::Gravity,
                x: g.x as f32 * G_TO_MS2,
                y: g.y as f32 * G_TO_MS2,
                z: g.z as f32 * G_TO_MS2,
                timestamp_ms: ts,
            });
            let u = d.userAcceleration();
            push_sensor_reading(SensorReading {
                kind: SensorKind::LinearAcceleration,
                x: u.x as f32 * G_TO_MS2,
                y: u.y as f32 * G_TO_MS2,
                z: u.z as f32 * G_TO_MS2,
                timestamp_ms: ts,
            });
            // The VECTOR PART only, matching Android's `TYPE_ROTATION_VECTOR`
            // and the WinRT `SensorQuaternion` reads beside it. `w` is
            // recoverable for a unit quaternion, which an attitude always is.
            let q = d.attitude().quaternion();
            push_sensor_reading(SensorReading {
                kind: SensorKind::RotationVector,
                x: q.x as f32,
                y: q.y as f32,
                z: q.z as f32,
                timestamp_ms: ts,
            });
        }
    }
}
