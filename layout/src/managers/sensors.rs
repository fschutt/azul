//! Sensor manager — cross-platform state for the motion-sensor surface
//! (SUPER_PLAN_2 §1 feature 5 + research/03).
//!
//! Continuous + push-driven, like geolocation:
//!
//! - The **platform backend** (`dll/src/desktop/extra/sensors/<plat>.rs`)
//!   subscribes to CoreMotion (`CMMotionManager`) / Android `SensorManager`
//!   and calls [`push_sensor_reading`] on every sample (arbitrary thread).
//! - The dll **layout pass** drains the channel via
//!   [`drain_sensor_readings`] and folds each into the manager through
//!   [`SensorManager::set_reading`].
//! - **Callbacks** read `reading(kind)` synchronously (via
//!   `CallbackInfo::get_sensor_reading`) to drive tilt / shake / compass UI.
//!
//! One reading slot per [`SensorKind`]. No platform deps
//! (SUPER_PLAN_2 §0.5); the channel mirrors `geolocation.rs` verbatim.

use alloc::vec::Vec;

pub use azul_core::sensors::{SensorKind, SensorReading};

/// Cross-platform sensor state. One per `App` — the OS exposes a single
/// per-process sensor subscription, not per-window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SensorManager {
    /// Latest accelerometer reading (m/s²), or `None` until a sample arrives.
    pub accelerometer: Option<SensorReading>,
    /// Latest gyroscope reading (rad/s).
    pub gyroscope: Option<SensorReading>,
    /// Latest magnetometer reading (µT).
    pub magnetometer: Option<SensorReading>,
}

impl SensorManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Latest reading for `kind`, or `None` if no backend has delivered one.
    pub fn reading(&self, kind: SensorKind) -> Option<SensorReading> {
        match kind {
            SensorKind::Accelerometer => self.accelerometer,
            SensorKind::Gyroscope => self.gyroscope,
            SensorKind::Magnetometer => self.magnetometer,
        }
    }

    /// Apply a reading the backend delivered. Returns `true` if it advanced
    /// (bit-pattern different from the previous, so missing-as-`NaN` axes
    /// don't make every sample look "changed").
    pub fn set_reading(&mut self, reading: SensorReading) -> bool {
        let slot = match reading.kind {
            SensorKind::Accelerometer => &mut self.accelerometer,
            SensorKind::Gyroscope => &mut self.gyroscope,
            SensorKind::Magnetometer => &mut self.magnetometer,
        };
        let changed = match slot {
            Some(prev) => !reading_bitwise_eq(prev, &reading),
            None => true,
        };
        *slot = Some(reading);
        changed
    }
}

fn reading_bitwise_eq(a: &SensorReading, b: &SensorReading) -> bool {
    a.kind == b.kind
        && a.x.to_bits() == b.x.to_bits()
        && a.y.to_bits() == b.y.to_bits()
        && a.z.to_bits() == b.z.to_bits()
        && a.timestamp_ms == b.timestamp_ms
}

// ────────── Async reading channel (platform backend → manager) ─────────
//
// CoreMotion / Android `SensorManager` deliver on an arbitrary thread with
// no handle to the live `SensorManager` (inside the window's
// `LayoutWindow`). The backend parks each reading here; the layout pass
// drains it and applies the latest per kind. Pure Rust — no platform
// dependency (SUPER_PLAN_2 §0.5). Mirrors the geolocation fix channel.

static PENDING_READINGS: std::sync::Mutex<Vec<SensorReading>> =
    std::sync::Mutex::new(Vec::new());

/// Park a sensor reading delivered by a platform backend (in the dll).
/// Thread-safe; poison-recovering.
pub fn push_sensor_reading(reading: SensorReading) {
    let mut q = PENDING_READINGS.lock().unwrap_or_else(|e| e.into_inner());
    q.push(reading);
}

/// Drain every reading parked by [`push_sensor_reading`], in arrival order.
/// Called once per layout pass; the caller applies them through
/// [`SensorManager::set_reading`] (the last per kind wins).
pub fn drain_sensor_readings() -> Vec<SensorReading> {
    let mut q = PENDING_READINGS.lock().unwrap_or_else(|e| e.into_inner());
    core::mem::take(&mut *q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(kind: SensorKind, x: f32, y: f32, z: f32) -> SensorReading {
        SensorReading {
            kind,
            x,
            y,
            z,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn manager_defaults_to_no_readings() {
        let mgr = SensorManager::new();
        assert_eq!(mgr.reading(SensorKind::Accelerometer), None);
        assert_eq!(mgr.reading(SensorKind::Gyroscope), None);
        assert_eq!(mgr.reading(SensorKind::Magnetometer), None);
    }

    #[test]
    fn set_reading_routes_by_kind_and_flags_change() {
        let mut mgr = SensorManager::new();
        assert!(mgr.set_reading(r(SensorKind::Accelerometer, 0.0, 0.0, 9.81)));
        // Only the accelerometer slot is filled.
        assert!(mgr.reading(SensorKind::Accelerometer).is_some());
        assert_eq!(mgr.reading(SensorKind::Gyroscope), None);
        // Same value again — no change.
        assert!(!mgr.set_reading(r(SensorKind::Accelerometer, 0.0, 0.0, 9.81)));
        // Different value — change.
        assert!(mgr.set_reading(r(SensorKind::Accelerometer, 1.0, 0.0, 9.81)));
        // A different kind fills its own slot.
        assert!(mgr.set_reading(r(SensorKind::Gyroscope, 0.1, 0.0, 0.0)));
        assert_eq!(
            mgr.reading(SensorKind::Gyroscope).map(|r| r.x),
            Some(0.1)
        );
    }

    #[test]
    fn magnitude_of_resting_accelerometer() {
        let g = r(SensorKind::Accelerometer, 0.0, 0.0, 9.81);
        assert!((g.magnitude() - 9.81).abs() < 1e-4);
    }

    #[test]
    fn readings_round_trip_through_manager() {
        let _ = drain_sensor_readings();

        push_sensor_reading(r(SensorKind::Accelerometer, 1.0, 2.0, 3.0));
        push_sensor_reading(r(SensorKind::Accelerometer, 4.0, 5.0, 6.0)); // last wins per kind
        push_sensor_reading(r(SensorKind::Magnetometer, 20.0, 0.0, 40.0));
        let drained = drain_sensor_readings();
        assert_eq!(drained.len(), 3, "all parked readings drain in order");

        let mut mgr = SensorManager::new();
        for reading in &drained {
            mgr.set_reading(*reading);
        }
        assert_eq!(
            mgr.reading(SensorKind::Accelerometer).map(|r| r.x),
            Some(4.0),
            "the last accelerometer reading wins"
        );
        assert_eq!(
            mgr.reading(SensorKind::Magnetometer).map(|r| r.z),
            Some(40.0)
        );

        assert!(drain_sensor_readings().is_empty());
    }
}
