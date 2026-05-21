//! Linux motion-sensor backend - industrial I/O (iio) via sysfs.
//!
//! Reads `/sys/bus/iio/devices/iio:deviceN/in_{accel,anglvel,magn}_{x,y,z}_raw`
//! (scaled by the channel's `in_<type>_scale`) once per [`poll`], pushing each
//! present sensor's reading into azul-layout's channel - the same channel the
//! CoreMotion / Android backends feed.
//!
//! Pure sysfs file reads: no system library, no dlopen, so it cross-compiles
//! anywhere, and it gracefully reads nothing when the machine has no iio motion
//! sensors (most desktops; common on laptops / tablets / SBCs with an IMU).
//!
//! Units follow the iio ABI: accelerometer in m/s^2, gyroscope in rad/s
//! (already azul-core's units); magnetometer in Gauss, converted to microtesla
//! (x100) to match azul-core.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use azul_core::sensors::{SensorKind, SensorReading};
use azul_layout::managers::sensors::push_sensor_reading;

/// No persistent subscription - [`poll`] scans sysfs each frame.
pub fn start() {}

/// Scan the iio devices and push the latest accelerometer / gyroscope /
/// magnetometer reading from each device that exposes one.
pub fn poll() {
    let dir = match fs::read_dir("/sys/bus/iio/devices") {
        Ok(d) => d,
        Err(_) => return, // no iio subsystem -> no motion sensors on this host
    };
    let now_ms = now_ms();
    for entry in dir.flatten() {
        let dev = entry.path();
        if let Some(r) = read_axes(&dev, "in_accel", SensorKind::Accelerometer, now_ms) {
            push_sensor_reading(r);
        }
        if let Some(r) = read_axes(&dev, "in_anglvel", SensorKind::Gyroscope, now_ms) {
            push_sensor_reading(r);
        }
        if let Some(r) = read_axes(&dev, "in_magn", SensorKind::Magnetometer, now_ms) {
            push_sensor_reading(r);
        }
    }
}

/// Read a 3-axis iio channel (`<prefix>_{x,y,z}_raw` * `<prefix>_scale`), or
/// `None` if this device doesn't expose all three axes.
fn read_axes(dev: &Path, prefix: &str, kind: SensorKind, now_ms: u64) -> Option<SensorReading> {
    // Gauss -> microtesla for the magnetometer; accel/gyro are already in
    // azul-core's units after the iio scale.
    let unit = if matches!(kind, SensorKind::Magnetometer) {
        100.0
    } else {
        1.0
    };
    let scale = read_f32(&dev.join(format!("{prefix}_scale"))).unwrap_or(1.0) * unit;
    let x = read_f32(&dev.join(format!("{prefix}_x_raw")))? * scale;
    let y = read_f32(&dev.join(format!("{prefix}_y_raw")))? * scale;
    let z = read_f32(&dev.join(format!("{prefix}_z_raw")))? * scale;
    Some(SensorReading {
        kind,
        x,
        y,
        z,
        timestamp_ms: now_ms,
    })
}

fn read_f32(p: &Path) -> Option<f32> {
    fs::read_to_string(p).ok()?.trim().parse().ok()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
