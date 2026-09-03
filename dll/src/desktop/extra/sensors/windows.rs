//! Windows motion-sensor backend — `Windows.Devices.Sensors` (WinRT).
//!
//! Polls `GetCurrentReading()` each [`poll`] (the MS-documented preferred mode
//! for frame-rate UIs) and pushes into azul-layout's channel — the same one the
//! CoreMotion / Android / iio backends feed. Graceful no-op on the many
//! desktops with no IMU (`GetDefault()` -> `None`, guarded reads). Units ->
//! azul-core: accelerometer g -> m/s² (×9.80665), gyrometer deg/s -> rad/s
//! (×π/180), magnetometer µT -> µT (pass-through).
//!
//! # The fused kinds (8e-i-a), and why two of them are not polled
//!
//! The ledger's note said "Linux and Windows have no fused-sensor concept at
//! all outside of tablets". That is true about the HARDWARE and wrong about
//! the API: `Windows.Devices.Sensors` has a full fused set, and a machine
//! without the hardware simply returns no default - the same no-op the raw
//! three already get.
//!
//! Most of them are synchronous and join the poll: `OrientationSensor` gives
//! the fused quaternion, and gravity and linear acceleration are the SAME
//! `Accelerometer` class opened with a different `AccelerometerReadingType`
//! rather than separate classes, which is easy to miss.
//!
//! TWO ARE ASYNC-ONLY and cannot be polled. `Pedometer` and `HingeAngleSensor`
//! have no `GetDefault()`, only `GetDefaultAsync()`, and blocking the layout
//! thread on a WinRT async call once per frame is not an option. They are
//! resolved ONCE on a background thread; after that the pedometer's
//! `GetCurrentReadings()` is synchronous and joins the poll, while the hinge
//! has no synchronous read at all and is driven by its `ReadingChanged` event
//! instead - which suits it anyway, since a hinge angle changes when someone
//! folds the machine and not at 60 Hz.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use azul_core::sensors::{SensorKind, SensorReading};
use azul_layout::managers::sensors::push_sensor_reading;
use windows::Devices::Sensors::{
    Accelerometer, AccelerometerReadingType, Barometer, Gyrometer, HingeAngleSensor, LightSensor,
    Magnetometer, OrientationSensor, Pedometer, PedometerStepKind, ProximitySensor,
};

use super::units::{DEG_TO_RAD, G_TO_MS2};

/// Cached sensor handles; a slot stays `None` when the device lacks that sensor.
struct Sensors {
    accel: Option<Accelerometer>,
    gyro: Option<Gyrometer>,
    mag: Option<Magnetometer>,
    /// The SAME class as `accel`, opened with a different reading type - not a
    /// separate sensor class, which is the part of this API that is easy to
    /// miss and would otherwise leave gravity permanently unfilled.
    gravity: Option<Accelerometer>,
    linear: Option<Accelerometer>,
    orientation: Option<OrientationSensor>,
    light: Option<LightSensor>,
    barometer: Option<Barometer>,
}
// WinRT sensor objects are agile; only the layout thread touches them here.
unsafe impl Send for Sensors {}
unsafe impl Sync for Sensors {}
static SENSORS: OnceLock<Sensors> = OnceLock::new();

/// A WinRT object parked in a static.
///
/// WinRT objects are agile - callable from any apartment - which is what makes
/// this sound; the existing `Sensors` above asserts the same thing for the
/// same reason.
struct Agile<T>(T);
unsafe impl<T> Send for Agile<T> {}
unsafe impl<T> Sync for Agile<T> {}

/// Resolved asynchronously by `start_async_sensors`; empty until it answers,
/// and forever on a machine without the sensor.
static PEDOMETER: OnceLock<Agile<Pedometer>> = OnceLock::new();
/// Held only to keep the `ReadingChanged` subscription alive.
static HINGE: OnceLock<Agile<HingeAngleSensor>> = OnceLock::new();
/// The proximity sensor (8e-i-a-ii). No `GetDefault()` on this one: it is
/// found through device enumeration over its own selector and opened by id.
static PROXIMITY: OnceLock<Agile<ProximitySensor>> = OnceLock::new();

/// MWA-C-sensors: non-destructive IMU presence probe for AzCapability —
/// the capability report used to hardcode available=false on Windows
/// while GetDefault() is exactly the real check. Reuses the started
/// sensor set when available.
pub fn has_motion_hardware() -> bool {
    if let Some(s) = SENSORS.get() {
        return s.accel.is_some() || s.gyro.is_some() || s.mag.is_some();
    }
    // GetDefault() errs when no sensor is present (same contract start()
    // relies on with `.ok()`).
    Accelerometer::GetDefault().is_ok()
        || Gyrometer::GetDefault().is_ok()
        || Magnetometer::GetDefault().is_ok()
}

pub fn start() {
    let s = Sensors {
        accel: Accelerometer::GetDefault().ok(),
        gyro: Gyrometer::GetDefault().ok(),
        mag: Magnetometer::GetDefault().ok(),
        gravity: Accelerometer::GetDefaultWithAccelerometerReadingType(
            AccelerometerReadingType::Gravity,
        )
        .ok(),
        linear: Accelerometer::GetDefaultWithAccelerometerReadingType(
            AccelerometerReadingType::Linear,
        )
        .ok(),
        orientation: OrientationSensor::GetDefault().ok(),
        light: LightSensor::GetDefault().ok(),
        barometer: Barometer::GetDefault().ok(),
    };
    // Polling requires a report interval to be allocated (use the device
    // floor). WITHOUT THIS `GetCurrentReading` returns a stale or empty
    // reading - allocating the interval is what starts the sensor.
    if let Some(a) = &s.accel {
        if let Ok(m) = a.MinimumReportInterval() {
            let _ = a.SetReportInterval(m);
        }
    }
    if let Some(g) = &s.gyro {
        if let Ok(m) = g.MinimumReportInterval() {
            let _ = g.SetReportInterval(m);
        }
    }
    if let Some(mg) = &s.mag {
        if let Ok(m) = mg.MinimumReportInterval() {
            let _ = mg.SetReportInterval(m);
        }
    }
    for a in [s.gravity.as_ref(), s.linear.as_ref()].into_iter().flatten() {
        if let Ok(m) = a.MinimumReportInterval() {
            let _ = a.SetReportInterval(m);
        }
    }
    if let Some(o) = &s.orientation {
        if let Ok(m) = o.MinimumReportInterval() {
            let _ = o.SetReportInterval(m);
        }
    }
    if let Some(l) = &s.light {
        if let Ok(m) = l.MinimumReportInterval() {
            let _ = l.SetReportInterval(m);
        }
    }
    if let Some(b) = &s.barometer {
        if let Ok(m) = b.MinimumReportInterval() {
            let _ = b.SetReportInterval(m);
        }
    }
    let _ = SENSORS.set(s);

    start_async_sensors();
}

/// The pedometer and the hinge sensor, which have no synchronous `GetDefault`.
///
/// On a thread of its own, because `pollster::block_on` on a WinRT
/// `IAsyncOperation` blocks until the platform answers and this is called from
/// the layout pass. On the overwhelmingly common machine that has neither, the
/// operation completes with an error almost immediately and the thread exits.
fn start_async_sensors() {
    std::thread::Builder::new()
        .name("azul-sensors-winrt".into())
        .spawn(|| {
            use std::future::IntoFuture;

            use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
            unsafe {
                // Same requirement the biometric backend documents: a WinRT
                // async call needs an initialised apartment on its thread.
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            if let Ok(p) = Pedometer::GetDefaultAsync()
                .and_then(|op| pollster::block_on(op.into_future()))
            {
                if let Ok(m) = p.MinimumReportInterval() {
                    let _ = p.SetReportInterval(m);
                }
                // `GetCurrentReadings` is synchronous once the object exists,
                // so from here the pedometer is an ordinary polled sensor.
                let _ = PEDOMETER.set(Agile(p));
            }

            // PROXIMITY (8e-i-a-ii): `ProximitySensor` has no `GetDefault()`;
            // the documented route is `DeviceInformation` enumeration over
            // its selector, then `FromId`, which is synchronous. The first
            // device is the built-in one on every machine that has any.
            if let Ok(selector) = ProximitySensor::GetDeviceSelector() {
                if let Ok(devices) =
                    windows::Devices::Enumeration::DeviceInformation::FindAllAsyncAqsFilter(
                        &selector,
                    )
                    .and_then(|op| pollster::block_on(op.into_future()))
                {
                    if devices.Size().unwrap_or(0) > 0 {
                        if let Ok(id) = devices.GetAt(0).and_then(|d| d.Id()) {
                            if let Ok(p) = ProximitySensor::FromId(&id) {
                                let _ = PROXIMITY.set(Agile(p));
                            }
                        }
                    }
                }
            }

            if let Ok(h) = HingeAngleSensor::GetDefaultAsync()
                .and_then(|op| pollster::block_on(op.into_future()))
            {
                // EVENT-DRIVEN, not polled: the only read is
                // `GetCurrentReadingAsync`, and a hinge angle changes when
                // someone folds the machine rather than at frame rate. The
                // threshold is the device's own floor so no motion is missed.
                if let Ok(t) = h.MinReportThresholdInDegrees() {
                    let _ = h.SetReportThresholdInDegrees(t);
                }
                let handler = windows::Foundation::TypedEventHandler::<
                    HingeAngleSensor,
                    windows::Devices::Sensors::HingeAngleSensorReadingChangedEventArgs,
                >::new(|_sender, args| {
                    if let Some(args) = args.as_ref() {
                        if let Ok(r) = args.Reading() {
                            if let Ok(deg) = r.AngleInDegrees() {
                                push_sensor_reading(SensorReading {
                                    kind: SensorKind::HingeAngle,
                                    x: deg as f32,
                                    y: 0.0,
                                    z: 0.0,
                                    timestamp_ms: now_ms(),
                                });
                            }
                        }
                    }
                    Ok(())
                });
                let _ = h.ReadingChanged(&handler);
                // The sensor must OUTLIVE this thread or the subscription is
                // torn down the moment it exits and no fold is ever reported.
                let _ = HINGE.set(Agile(h));
            }
        })
        .ok();
}

pub fn poll() {
    // The typed proximity answer (8e-i-a-ii), before the IMU gate below: a
    // machine can have the one without the other. `DistanceInMillimeters` is
    // OPTIONAL (an `IReference`) - present only on a ranging sensor - and
    // `IsDetected` is the whole answer of a binary one.
    if let Some(p) = PROXIMITY.get() {
        if let Ok(r) = p.0.GetCurrentReading() {
            use azul_core::sensors::{DistanceUnit, Proximity, ProximityDistance};
            let ranged = r
                .DistanceInMillimeters()
                .ok()
                .and_then(|d| d.Value().ok());
            let proximity = match ranged {
                Some(mm) => Proximity::Distance(ProximityDistance {
                    value: mm as f32,
                    unit: DistanceUnit::Millimeters,
                }),
                None if r.IsDetected().unwrap_or(false) => Proximity::Near,
                None => Proximity::Far,
            };
            azul_layout::managers::sensors::push_proximity(proximity);
        }
    }

    let Some(s) = SENSORS.get() else {
        return;
    };
    let now = now_ms();

    if let Some(a) = &s.accel {
        if let Ok(r) = a.GetCurrentReading() {
            if let (Ok(x), Ok(y), Ok(z)) = (r.AccelerationX(), r.AccelerationY(), r.AccelerationZ())
            {
                push_sensor_reading(SensorReading {
                    kind: SensorKind::Accelerometer,
                    x: x as f32 * G_TO_MS2,
                    y: y as f32 * G_TO_MS2,
                    z: z as f32 * G_TO_MS2,
                    timestamp_ms: now,
                });
            }
        }
    }
    if let Some(g) = &s.gyro {
        if let Ok(r) = g.GetCurrentReading() {
            if let (Ok(x), Ok(y), Ok(z)) = (
                r.AngularVelocityX(),
                r.AngularVelocityY(),
                r.AngularVelocityZ(),
            ) {
                push_sensor_reading(SensorReading {
                    kind: SensorKind::Gyroscope,
                    x: x as f32 * DEG_TO_RAD,
                    y: y as f32 * DEG_TO_RAD,
                    z: z as f32 * DEG_TO_RAD,
                    timestamp_ms: now,
                });
            }
        }
    }
    if let Some(mg) = &s.mag {
        if let Ok(r) = mg.GetCurrentReading() {
            if let (Ok(x), Ok(y), Ok(z)) =
                (r.MagneticFieldX(), r.MagneticFieldY(), r.MagneticFieldZ())
            {
                // Already microtesla.
                push_sensor_reading(SensorReading {
                    kind: SensorKind::Magnetometer,
                    x,
                    y,
                    z,
                    timestamp_ms: now,
                });
            }
        }
    }

    // The two fused accelerometer views. Same class, same reading accessors -
    // only the reading TYPE the object was opened with differs, so the values
    // arrive in G exactly like the raw one.
    for (sensor, kind) in [
        (s.gravity.as_ref(), SensorKind::Gravity),
        (s.linear.as_ref(), SensorKind::LinearAcceleration),
    ] {
        let Some(a) = sensor else { continue };
        if let Ok(r) = a.GetCurrentReading() {
            if let (Ok(x), Ok(y), Ok(z)) = (r.AccelerationX(), r.AccelerationY(), r.AccelerationZ())
            {
                push_sensor_reading(SensorReading {
                    kind,
                    x: x as f32 * G_TO_MS2,
                    y: y as f32 * G_TO_MS2,
                    z: z as f32 * G_TO_MS2,
                    timestamp_ms: now,
                });
            }
        }
    }

    if let Some(o) = &s.orientation {
        if let Ok(r) = o.GetCurrentReading() {
            if let Ok(q) = r.Quaternion() {
                if let (Ok(x), Ok(y), Ok(z)) = (q.X(), q.Y(), q.Z()) {
                    // `SensorKind::RotationVector` carries the VECTOR PART and
                    // drops `w`, matching Android's `TYPE_ROTATION_VECTOR`.
                    // `w` is recoverable for a unit quaternion.
                    push_sensor_reading(SensorReading {
                        kind: SensorKind::RotationVector,
                        x,
                        y,
                        z,
                        timestamp_ms: now,
                    });
                }
            }
        }
    }

    if let Some(l) = &s.light {
        if let Ok(r) = l.GetCurrentReading() {
            if let Ok(lux) = r.IlluminanceInLux() {
                push_sensor_reading(SensorReading {
                    kind: SensorKind::AmbientLight,
                    x: lux,
                    y: 0.0,
                    z: 0.0,
                    timestamp_ms: now,
                });
            }
        }
    }

    if let Some(b) = &s.barometer {
        if let Ok(r) = b.GetCurrentReading() {
            if let Ok(hpa) = r.StationPressureInHectopascals() {
                // ALREADY hectopascals - the one platform that needs no
                // pressure conversion, unlike iio's kilopascals.
                push_sensor_reading(SensorReading {
                    kind: SensorKind::Barometer,
                    x: hpa as f32,
                    y: 0.0,
                    z: 0.0,
                    timestamp_ms: now,
                });
            }
        }
    }

    if let Some(p) = PEDOMETER.get() {
        if let Ok(readings) = p.0.GetCurrentReadings() {
            // ONE ENTRY PER STEP KIND, and the total is their sum: a device
            // that distinguishes walking from running reports two counters and
            // neither one alone is "steps since boot". A kind the device does
            // not report is simply absent, so a failed lookup is skipped
            // rather than treated as zero.
            let total: i32 = [
                PedometerStepKind::Unknown,
                PedometerStepKind::Walking,
                PedometerStepKind::Running,
            ]
            .into_iter()
            .filter_map(|k| readings.Lookup(k).ok())
            .filter_map(|r| r.CumulativeSteps().ok())
            .sum();
            push_sensor_reading(SensorReading {
                kind: SensorKind::StepCounter,
                x: total as f32,
                y: 0.0,
                z: 0.0,
                timestamp_ms: now,
            });
        }
    }

    // No hinge arm here on purpose: it is event-driven (see the module docs).
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
