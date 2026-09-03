//! Unit conversions shared by the four sensor backends.
//!
//! # Why this is its own module and not three private helpers
//!
//! `azul_core::sensors::SensorKind` fixes ONE unit per kind, and every platform
//! reports something else: CoreMotion gives acceleration in G, Windows gives
//! angular velocity in degrees per second, iio gives pressure in kilopascals
//! and proximity in metres. Each conversion is one multiplication, which is
//! exactly why getting one wrong is invisible - a barometer reading 101.3
//! instead of 1013 is a plausible-looking number that is off by an order of
//! magnitude, and nothing downstream can tell.
//!
//! The backends are `cfg`-gated to their own platform, so a test inside
//! `linux.rs` never runs on a macOS host and a test inside `windows.rs` never
//! runs at all here. This module is compiled EVERYWHERE, so the arithmetic that
//! actually goes wrong is covered by tests that run on whatever machine builds
//! the crate.

/// Standard gravity. CoreMotion and the WinRT `Accelerometer` both report in
/// G; `SensorKind::Accelerometer` is m/s^2.
pub const G_TO_MS2: f32 = 9.806_65;

/// The WinRT `Gyrometer` reports degrees per second; `SensorKind::Gyroscope`
/// is rad/s.
pub const DEG_TO_RAD: f32 = core::f32::consts::PI / 180.0;

/// iio reports pressure in kilopascals (`in_pressure_raw` scaled), the WinRT
/// `Barometer` in hectopascals, and `SensorKind::Barometer` is hPa.
///
/// Standard sea-level pressure is 101.325 kPa = 1013.25 hPa, which is the
/// value the test pins - a backend that skipped this conversion reports a
/// number that still looks like a pressure.
pub fn kpa_to_hpa(kpa: f32) -> f32 {
    kpa * 10.0
}

/// iio reports angles in radians (`in_angl_raw` scaled);
/// `SensorKind::HingeAngle` is degrees, matching Android's `TYPE_HINGE_ANGLE`
/// and the WinRT `HingeAngleReading`.
pub fn rad_to_deg(rad: f32) -> f32 {
    rad * (180.0 / core::f32::consts::PI)
}

/// iio reports proximity in metres (`in_proximity_raw` scaled);
/// `SensorKind::Proximity` is centimetres.
pub fn m_to_cm(m: f32) -> f32 {
    m * 100.0
}

/// The WinRT `ProximitySensorReading` reports millimetres.
pub fn mm_to_cm(mm: f32) -> f32 {
    mm / 10.0
}

/// iio's magnetometer channel is in Gauss; `SensorKind::Magnetometer` is
/// microtesla. 1 G = 100 uT.
pub const GAUSS_TO_UT: f32 = 100.0;

/// Parse an iio sysfs attribute that holds SEVERAL numbers.
///
/// Most `_raw` attributes hold one value, but a channel whose driver
/// implements `read_raw_multi` - the rotation quaternion is the one that
/// matters here - prints its components space-separated in a single file. A
/// plain `str::parse::<f32>()` fails on the whole string, which reads as "no
/// such sensor" rather than as "wrong parser", so this is worth having
/// explicitly.
///
/// Returns an empty vec when nothing parses, so a caller can treat "absent"
/// and "unparseable" the same way.
pub fn parse_multi_value(s: &str) -> Vec<f32> {
    s.split_whitespace().filter_map(|t| t.parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard sea-level pressure, the one number anyone would sanity-check
    /// a barometer against.
    #[test]
    fn sea_level_pressure_converts_to_the_familiar_1013() {
        let hpa = kpa_to_hpa(101.325);
        assert!((hpa - 1013.25).abs() < 0.01, "got {hpa}");
    }

    /// A fold's two named postures: flat and closed.
    #[test]
    fn a_flat_hinge_is_180_degrees() {
        assert!((rad_to_deg(core::f32::consts::PI) - 180.0).abs() < 0.001);
        assert!(rad_to_deg(0.0).abs() < 0.001);
        assert!((rad_to_deg(core::f32::consts::FRAC_PI_2) - 90.0).abs() < 0.001);
    }

    #[test]
    fn distances_land_in_centimetres_from_both_directions() {
        // iio: metres.
        assert!((m_to_cm(0.05) - 5.0).abs() < 1e-4);
        // WinRT: millimetres.
        assert!((mm_to_cm(50.0) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn one_g_is_standard_gravity() {
        assert!((1.0 * G_TO_MS2 - 9.806_65).abs() < 1e-4);
        // A gyro turning 360 deg/s is 2*pi rad/s.
        assert!((360.0 * DEG_TO_RAD - core::f32::consts::TAU).abs() < 1e-4);
        // The earth's field is around 50 uT, i.e. 0.5 Gauss.
        assert!((0.5 * GAUSS_TO_UT - 50.0).abs() < 1e-4);
    }

    /// THE QUATERNION CASE. One sysfs file, four numbers.
    #[test]
    fn a_multi_value_attribute_parses_all_of_its_components() {
        let v = parse_multi_value("0 0 707106781 707106781\n");
        assert_eq!(v.len(), 4);
        assert_eq!(v[0], 0.0);

        // A single value still parses, because most attributes hold one.
        assert_eq!(parse_multi_value("1234\n"), vec![1234.0]);

        // Negative and fractional components, which a raw channel can hold
        // once a scale has been applied by the driver.
        let v = parse_multi_value("-0.5 0.5 -0.5 0.5");
        assert_eq!(v.len(), 4);
        assert_eq!(v[0], -0.5);

        // Garbage yields nothing rather than a partial vector of zeroes: a
        // caller must be able to tell "no sensor" from "a reading of 0".
        assert!(parse_multi_value("").is_empty());
        assert!(parse_multi_value("n/a").is_empty());
    }
}
