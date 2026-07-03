//! Windows motion-sensor backend — `Windows.Devices.Sensors` (WinRT).
//!
//! Polls `GetCurrentReading()` each [`poll`] (the MS-documented preferred mode
//! for frame-rate UIs) and pushes into azul-layout's channel — the same one the
//! CoreMotion / Android / iio backends feed. Graceful no-op on the many
//! desktops with no IMU (`GetDefault()` -> `None`, guarded reads). Units ->
//! azul-core: accelerometer g -> m/s² (×9.80665), gyrometer deg/s -> rad/s
//! (×π/180), magnetometer µT -> µT (pass-through).

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use azul_core::sensors::{SensorKind, SensorReading};
use azul_layout::managers::sensors::push_sensor_reading;
use windows::Devices::Sensors::{Accelerometer, Gyrometer, Magnetometer};

const G_TO_MS2: f32 = 9.806_65;
const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

/// Cached sensor handles; a slot stays `None` when the device lacks that sensor.
struct Sensors {
    accel: Option<Accelerometer>,
    gyro: Option<Gyrometer>,
    mag: Option<Magnetometer>,
}
// WinRT sensor objects are agile; only the layout thread touches them here.
unsafe impl Send for Sensors {}
unsafe impl Sync for Sensors {}
static SENSORS: OnceLock<Sensors> = OnceLock::new();

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
    };
    // Polling requires a report interval to be allocated (use the device floor).
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
    let _ = SENSORS.set(s);
}

pub fn poll() {
    let Some(s) = SENSORS.get() else {
        return;
    };
    let now = now_ms();

    if let Some(a) = &s.accel {
        if let Ok(r) = a.GetCurrentReading() {
            if let (Ok(x), Ok(y), Ok(z)) =
                (r.AccelerationX(), r.AccelerationY(), r.AccelerationZ())
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
            if let (Ok(x), Ok(y), Ok(z)) =
                (r.AngularVelocityX(), r.AngularVelocityY(), r.AngularVelocityZ())
            {
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
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
