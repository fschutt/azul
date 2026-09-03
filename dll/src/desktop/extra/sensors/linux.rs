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
//! Units follow the iio ABI, and the ABI's units are NOT azul-core's for half
//! of these: accelerometer m/s^2 and gyroscope rad/s pass through, but the
//! magnetometer is Gauss (-> microtesla), pressure is KILOpascals (-> hPa),
//! proximity is METRES (-> cm) and the hinge angle is RADIANS (-> degrees).
//! Every one of those conversions lives in `units.rs` with a test, because a
//! missed factor of ten produces a number that still looks like a reading.
//!
//! # The channel names come from the kernel's own ABI document
//!
//! `Documentation/ABI/testing/sysfs-bus-iio`, not from memory: the fused
//! channels are spelled inconsistently enough that guessing produces files
//! that simply never exist, which is indistinguishable from a machine without
//! the sensor. `in_gravity_x_raw` but `in_accel_linear_x_raw` (a MODIFIER on
//! the accelerometer, not a channel of its own); `in_steps_input` with no
//! `_raw` sibling; `in_angl_raw` for the hinge.
//!
//! # `_input` versus `_raw`
//!
//! The ABI offers both for the scalar channels: `_input` is already in the
//! documented unit, `_raw` needs multiplying by `_scale`. Drivers ship one or
//! the other and rarely both, so [`read_scalar`] tries `_input` first and falls
//! back - preferring `_raw` would silently apply a scale to an
//! already-scaled value.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use azul_core::sensors::{SensorKind, SensorReading};
use azul_layout::managers::sensors::push_sensor_reading;

use super::units;

/// No persistent subscription - [`poll`] scans sysfs each frame.
pub fn start() {}

/// Scan the iio devices and push the latest accelerometer / gyroscope /
/// magnetometer reading from each device that exposes one.
pub fn poll() {
    let dir = match fs::read_dir("/sys/bus/iio/devices") {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // No iio subsystem -> no motion sensors on this host. Normal on
            // desktops; staying quiet here is correct.
            return;
        }
        Err(e) => {
            // The subsystem EXISTS but cannot be read (EACCES etc.) — an IMU
            // may be present and silently unreadable. This runs per frame, so
            // say it once.
            static UNREADABLE: std::sync::Once = std::sync::Once::new();
            UNREADABLE.call_once(|| {
                crate::plog_warn!(
                    "[sensors] /sys/bus/iio/devices exists but cannot be read ({}) — \
                     motion sensors will report nothing",
                    e
                );
            });
            return;
        }
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

        // The FUSED channels (8e-i-a). The ledger's note said "Linux and
        // Windows have no fused-sensor concept at all outside of tablets" -
        // which is true about the HARDWARE and not about the ABI: iio has had
        // dedicated gravity, linear-acceleration and rotation channels for
        // years, and a machine without the hardware simply has no such files,
        // which is the same no-op every other channel here already gets.
        if let Some(r) = read_axes(&dev, "in_gravity", SensorKind::Gravity, now_ms) {
            push_sensor_reading(r);
        }
        // A MODIFIER on the accelerometer, not a channel of its own - hence
        // `in_accel_linear_x_raw` and not `in_linear_accel_x_raw`.
        if let Some(r) = read_axes(
            &dev,
            "in_accel_linear",
            SensorKind::LinearAcceleration,
            now_ms,
        ) {
            push_sensor_reading(r);
        }
        if let Some(r) = read_quaternion(&dev, now_ms) {
            push_sensor_reading(r);
        }

        // The SCALAR channels. Each carries its value in `x` and leaves
        // `y`/`z` at zero, as `SensorKind` specifies.
        for (prefix, kind, convert) in [
            (
                "in_illuminance",
                SensorKind::AmbientLight,
                None::<fn(f32) -> f32>,
            ),
            ("in_pressure", SensorKind::Barometer, Some(units::kpa_to_hpa as fn(f32) -> f32)),
            ("in_proximity", SensorKind::Proximity, Some(units::m_to_cm as fn(f32) -> f32)),
            ("in_steps", SensorKind::StepCounter, None),
            ("in_angl", SensorKind::HingeAngle, Some(units::rad_to_deg as fn(f32) -> f32)),
        ] {
            if let Some(v) = read_scalar(&dev, prefix) {
                push_sensor_reading(SensorReading {
                    kind,
                    x: convert.map_or(v, |f| f(v)),
                    y: 0.0,
                    z: 0.0,
                    timestamp_ms: now_ms,
                });
            }
        }
    }
}

/// Read a scalar iio channel: `<prefix>_input` if the driver publishes one,
/// otherwise `<prefix>_raw * <prefix>_scale`.
///
/// `_input` FIRST and not last. Both spellings exist and a driver ships one or
/// the other; reading `_raw` when an `_input` is present and then applying the
/// scale would multiply an already-converted value, which for a barometer
/// turns 1013 hPa into something that still looks like a pressure.
///
/// Step counters are the reason the `_scale` is optional rather than required:
/// the ABI gives `in_steps_input` with no scale at all, because a count has no
/// unit to scale into.
fn read_scalar(dev: &Path, prefix: &str) -> Option<f32> {
    if let Some(v) = read_f32(&dev.join(format!("{prefix}_input"))) {
        return Some(v);
    }
    let raw = read_f32(&dev.join(format!("{prefix}_raw")))?;
    let scale = read_f32(&dev.join(format!("{prefix}_scale"))).unwrap_or(1.0);
    let offset = read_f32(&dev.join(format!("{prefix}_offset"))).unwrap_or(0.0);
    // The ABI's formula is `(raw + offset) * scale`, and the offset is applied
    // BEFORE the scale. Dropping it is usually harmless and is not for a
    // barometer, where the offset carries the sensor's calibration.
    Some((raw + offset) * scale)
}

/// Read the fused orientation quaternion.
///
/// ONE FILE HOLDING FOUR NUMBERS: the driver implements `read_raw_multi`, so
/// `in_rot_quaternion_raw` prints `x y z w` space-separated. A plain parse of
/// that string fails, which would read as "this device has no rotation sensor"
/// - see `units::parse_multi_value`.
///
/// `SensorKind::RotationVector` carries the VECTOR PART in x/y/z and drops
/// `w`, which is the same shape Android's `TYPE_ROTATION_VECTOR` reports and
/// is recoverable: for a unit quaternion `w = sqrt(1 - x^2 - y^2 - z^2)`.
fn read_quaternion(dev: &Path, now_ms: u64) -> Option<SensorReading> {
    let text = fs::read_to_string(dev.join("in_rot_quaternion_raw")).ok()?;
    let parts = units::parse_multi_value(&text);
    if parts.len() < 3 {
        return None;
    }
    let scale = read_f32(&dev.join("in_rot_quaternion_scale")).unwrap_or(1.0);
    Some(SensorReading {
        kind: SensorKind::RotationVector,
        x: parts[0] * scale,
        y: parts[1] * scale,
        z: parts[2] * scale,
        timestamp_ms: now_ms,
    })
}

/// Read a 3-axis iio channel (`<prefix>_{x,y,z}_raw` * `<prefix>_scale`), or
/// `None` if this device doesn't expose all three axes.
fn read_axes(dev: &Path, prefix: &str, kind: SensorKind, now_ms: u64) -> Option<SensorReading> {
    // Gauss -> microtesla for the magnetometer; accel/gyro are already in
    // azul-core's units after the iio scale.
    let unit = if matches!(kind, SensorKind::Magnetometer) {
        units::GAUSS_TO_UT
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
