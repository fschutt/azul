//! Geolocation manager — cross-platform state for the GPS/location surface
//! (SUPER_PLAN_2 §1.5 + research/04 §3 + research/08 §6).
//!
//! Three callers drive it:
//!
//! - The **layout pass** scans the styled DOM for `GeolocationProbe`
//!   NodeTypes. When the first probe appears the framework fires
//!   `PermissionDiffEvent::Subscribe(Capability::Geolocation)` and the
//!   platform backend starts a native `CLLocationManager` /
//!   `LocationManager` / `geoclue` subscription. The reverse on the
//!   last probe leaving.
//!
//! - The **platform backend** (`dll/src/desktop/extra/geolocation/<plat>.rs`)
//!   calls `set_latest_fix(...)` whenever the native subscription
//!   delivers an update. The manager debounces and records the most
//!   recent value; callbacks read it via `CallbackInfo::get_geolocation_fix`.
//!
//! - **Callbacks** read `latest_fix()` synchronously to render the map
//!   centre, decide whether to show "acquiring signal…", etc.
//!
//! No platform deps; `no_std`-friendly via `alloc::collections::BTreeMap`.

use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;

/// One GPS / network-located fix. Mirrors the W3C
/// [`GeolocationPosition`](https://www.w3.org/TR/geolocation/#position_interface)
/// shape so the future web backend lands without API churn.
///
/// `accuracy_m` is the 1-sigma radius in metres. `altitude_m` /
/// `altitude_accuracy_m` / `heading_deg` / `speed_mps` are reported
/// as `None` when the platform doesn't supply them — iOS / Android
/// always supply lat/lon but the other fields depend on hardware
/// (some indoor location providers can't measure altitude or heading).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct LocationFix {
    /// Latitude in WGS-84 degrees (positive = north, negative = south).
    pub latitude_deg: f64,
    /// Longitude in WGS-84 degrees (positive = east, negative = west).
    pub longitude_deg: f64,
    /// 1-sigma horizontal accuracy radius in metres.
    pub accuracy_m: f32,
    /// Altitude above the WGS-84 ellipsoid in metres. `NaN` if not
    /// reported (the platform couldn't measure it).
    pub altitude_m: f32,
    /// 1-sigma altitude accuracy in metres. `NaN` if `altitude_m` is
    /// `NaN` or the platform doesn't report it.
    pub altitude_accuracy_m: f32,
    /// Bearing in degrees clockwise from true north, `0..360`. `NaN`
    /// if the device is stationary or the platform doesn't report it.
    pub heading_deg: f32,
    /// Ground speed in metres per second. `NaN` if not reported.
    pub speed_mps: f32,
    /// Monotonic timestamp in milliseconds since program start.
    /// Lets callers detect stale fixes without depending on
    /// wall-clock time.
    pub timestamp_ms: u64,
}

impl LocationFix {
    pub fn altitude(&self) -> Option<f32> {
        if self.altitude_m.is_nan() {
            None
        } else {
            Some(self.altitude_m)
        }
    }

    pub fn altitude_accuracy(&self) -> Option<f32> {
        if self.altitude_accuracy_m.is_nan() {
            None
        } else {
            Some(self.altitude_accuracy_m)
        }
    }

    pub fn heading(&self) -> Option<f32> {
        if self.heading_deg.is_nan() {
            None
        } else {
            Some(self.heading_deg)
        }
    }

    pub fn speed(&self) -> Option<f32> {
        if self.speed_mps.is_nan() {
            None
        } else {
            Some(self.speed_mps)
        }
    }
}

/// Configuration the user can attach to a `NodeType::GeolocationProbe`
/// to tune the platform subscription. Maps to W3C
/// `PositionOptions` (`enableHighAccuracy` + `maximumAge` +
/// `timeout`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct GeolocationProbeConfig {
    /// `true` requests precise (GPS-driven) location. iOS maps this
    /// to `CLLocationManager.desiredAccuracy = kCLLocationAccuracyBest`;
    /// Android to `LocationRequest.PRIORITY_HIGH_ACCURACY`. Costs
    /// battery — leave `false` for city-block-level apps.
    pub high_accuracy: bool,
    /// Subscribe to *background* location updates. Requires extra
    /// per-platform manifest declarations and a separate
    /// `Capability::GeolocationBackground` permission grant. `false`
    /// is the safe default.
    pub background: bool,
    /// Reject any fix whose `accuracy_m` exceeds this radius. `0`
    /// disables the filter — every native sample is delivered.
    pub max_accuracy_m: f32,
    /// Minimum time between delivered updates, in milliseconds. `0`
    /// disables throttling (every native sample is delivered;
    /// expensive when the platform fires at 10 Hz indoors).
    pub min_interval_ms: u32,
}

impl Default for GeolocationProbeConfig {
    fn default() -> Self {
        Self {
            high_accuracy: false,
            background: false,
            max_accuracy_m: 0.0,
            min_interval_ms: 0,
        }
    }
}

/// Diff event the layout pass emits when a probe appears or disappears.
/// Symmetric to `PermissionDiffEvent` — drives the platform backend's
/// native subscribe / release calls.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, u8)]
pub enum GeolocationDiffEvent {
    /// First probe of this config landed in the layout — start a
    /// native subscription with these options.
    Subscribe { config: GeolocationProbeConfig },
    /// Last probe left — stop the native subscription.
    Release,
    /// Probe config changed without subscriber churn — reconfigure
    /// the running subscription in place (e.g. high_accuracy false →
    /// true).
    Reconfigure { config: GeolocationProbeConfig },
}

/// Cross-platform geolocation state. One per `App` (the OS gives us
/// a single per-process subscription, not per-window).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GeolocationManager {
    /// Most recent fix from the platform backend, or `None` until the
    /// first native sample arrives (or `None` again after a Release).
    pub latest_fix: Option<LocationFix>,
    /// Active probe config — set on each Subscribe / Reconfigure,
    /// cleared on Release.
    pub active_config: Option<GeolocationProbeConfig>,
    /// Diff queue drained once per frame by the platform backend.
    pending_events: Vec<GeolocationDiffEvent>,
    /// Refcount of `GeolocationProbe` nodes currently in the layout.
    refcount: u32,
}

impl GeolocationManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn latest_fix(&self) -> Option<LocationFix> {
        self.latest_fix
    }

    pub fn refcount(&self) -> u32 {
        self.refcount
    }

    /// Platform backend writes the freshly-received fix. Returns true
    /// if the fix actually advanced (different from the previous one)
    /// so the caller can mark the window dirty for relayout.
    ///
    /// Compared via bit-pattern equality so missing fields (encoded as
    /// `f32::NAN`) compare equal — `PartialEq` returns `false` on
    /// NaN-vs-NaN, which would make every fix look "changed" even
    /// when nothing actually moved.
    pub fn set_latest_fix(&mut self, fix: LocationFix) -> bool {
        let changed = match self.latest_fix {
            Some(prev) => !Self::location_fix_bitwise_eq(&prev, &fix),
            None => true,
        };
        self.latest_fix = Some(fix);
        changed
    }

    fn location_fix_bitwise_eq(a: &LocationFix, b: &LocationFix) -> bool {
        a.latitude_deg.to_bits() == b.latitude_deg.to_bits()
            && a.longitude_deg.to_bits() == b.longitude_deg.to_bits()
            && a.accuracy_m.to_bits() == b.accuracy_m.to_bits()
            && a.altitude_m.to_bits() == b.altitude_m.to_bits()
            && a.altitude_accuracy_m.to_bits() == b.altitude_accuracy_m.to_bits()
            && a.heading_deg.to_bits() == b.heading_deg.to_bits()
            && a.speed_mps.to_bits() == b.speed_mps.to_bits()
            && a.timestamp_ms == b.timestamp_ms
    }

    /// Drain queued diff events. Platform backend calls this once per
    /// frame.
    pub fn take_pending_events(&mut self) -> Vec<GeolocationDiffEvent> {
        core::mem::take(&mut self.pending_events)
    }

    /// Diff entry point. The layout pass walks the styled DOM for
    /// `GeolocationProbe` nodes and feeds each `(config, node_id)`
    /// pair to the closure. The manager bumps the refcount, watches
    /// for config drift, and enqueues the right Subscribe / Release /
    /// Reconfigure event.
    pub fn diff_layout<F>(&mut self, mut for_each_probe: F)
    where
        F: FnMut(&mut dyn FnMut(GeolocationProbeConfig)),
    {
        let mut new_count: u32 = 0;
        let mut next_config: Option<GeolocationProbeConfig> = None;
        for_each_probe(&mut |cfg| {
            new_count += 1;
            // First probe's config wins. Subsequent probes that
            // disagree are accepted silently — a real app shouldn't
            // mount two `GeolocationProbe`s with different configs
            // but the framework can't assert that here.
            if next_config.is_none() {
                next_config = Some(cfg);
            }
        });

        let old_count = self.refcount;
        self.refcount = new_count;

        match (old_count, new_count) {
            (0, n) if n > 0 => {
                let config = next_config.unwrap_or_default();
                self.active_config = Some(config);
                self.pending_events
                    .push(GeolocationDiffEvent::Subscribe { config });
            }
            (m, 0) if m > 0 => {
                self.active_config = None;
                self.latest_fix = None;
                self.pending_events.push(GeolocationDiffEvent::Release);
            }
            (m, n) if m > 0 && n > 0 => {
                // Both frames have probes. Emit Reconfigure if the
                // config actually drifted.
                let new_config = next_config.unwrap_or_default();
                if Some(new_config) != self.active_config {
                    self.active_config = Some(new_config);
                    self.pending_events
                        .push(GeolocationDiffEvent::Reconfigure { config: new_config });
                }
            }
            _ => {
                // 0 → 0 — nothing to do.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GeolocationProbeConfig {
        GeolocationProbeConfig::default()
    }

    fn high_accuracy_cfg() -> GeolocationProbeConfig {
        GeolocationProbeConfig {
            high_accuracy: true,
            ..GeolocationProbeConfig::default()
        }
    }

    fn fix(lat: f64, lon: f64) -> LocationFix {
        LocationFix {
            latitude_deg: lat,
            longitude_deg: lon,
            accuracy_m: 10.0,
            altitude_m: f32::NAN,
            altitude_accuracy_m: f32::NAN,
            heading_deg: f32::NAN,
            speed_mps: f32::NAN,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn first_probe_emits_subscribe_with_config() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(cfg()));
        assert_eq!(mgr.refcount(), 1);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], GeolocationDiffEvent::Subscribe { .. }));
    }

    #[test]
    fn last_probe_drop_emits_release_and_clears_fix() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(cfg()));
        mgr.set_latest_fix(fix(37.0, -122.0));
        let _ = mgr.take_pending_events();

        mgr.diff_layout(|_emit| {});
        assert_eq!(mgr.refcount(), 0);
        assert_eq!(mgr.latest_fix(), None);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], GeolocationDiffEvent::Release));
    }

    #[test]
    fn config_drift_emits_reconfigure() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(cfg()));
        let _ = mgr.take_pending_events();

        mgr.diff_layout(|emit| emit(high_accuracy_cfg()));
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        match ev {
            GeolocationDiffEvent::Reconfigure { config } => {
                assert!(config.high_accuracy);
            }
            _ => panic!("expected Reconfigure, got {:?}", ev),
        }
    }

    #[test]
    fn stable_config_does_not_re_emit() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(cfg()));
        let _ = mgr.take_pending_events();

        // Same config across frames — no events.
        mgr.diff_layout(|emit| emit(cfg()));
        assert!(mgr.take_pending_events().is_empty());
    }

    #[test]
    fn set_latest_fix_returns_change_flag() {
        let mut mgr = GeolocationManager::new();
        assert!(mgr.set_latest_fix(fix(37.0, -122.0)));
        assert!(!mgr.set_latest_fix(fix(37.0, -122.0)));
        assert!(mgr.set_latest_fix(fix(37.7749, -122.4194)));
    }

    #[test]
    fn missing_fields_decode_to_none() {
        let f = fix(0.0, 0.0);
        assert_eq!(f.altitude(), None);
        assert_eq!(f.heading(), None);
        assert_eq!(f.speed(), None);
    }
}
