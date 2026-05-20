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

// `LocationFix` + `GeolocationProbeConfig` live in `azul-core` so
// `NodeType::GeolocationProbe(GeolocationProbeConfig)` can reference
// the config struct without a cyclic dep on `azul-layout`. We re-export
// them here for the existing `azul_layout::managers::geolocation::*`
// import paths.
pub use azul_core::geolocation::{GeolocationProbeConfig, LocationFix};

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

// ────────── Async fix channel (platform backend → manager) ────────────
//
// A native location callback (Android `FusedLocationProvider`
// `onLocationResult`, iOS `CLLocationManagerDelegate`) fires on an
// arbitrary thread with no handle to the live `GeolocationManager` (it
// lives inside the window's `LayoutWindow`). The backend parks each fix
// here; the layout pass drains it once per frame via
// [`drain_location_fixes`] and applies the latest through
// [`GeolocationManager::set_latest_fix`]. Pure Rust — no platform
// dependency (SUPER_PLAN_2 §0.5). Mirrors the permission manager's
// async-result channel.

static PENDING_FIXES: std::sync::Mutex<Vec<LocationFix>> =
    std::sync::Mutex::new(Vec::new());

/// Park a location fix delivered by a platform backend (in the dll).
/// Thread-safe; recovers from a poisoned lock so one panicking applier
/// can't wedge delivery forever.
pub fn push_location_fix(fix: LocationFix) {
    let mut q = PENDING_FIXES.lock().unwrap_or_else(|e| e.into_inner());
    q.push(fix);
}

/// Drain every fix parked by [`push_location_fix`], in arrival order.
/// Called once per layout pass; the caller applies them through
/// [`GeolocationManager::set_latest_fix`] (the last one wins).
pub fn drain_location_fixes() -> Vec<LocationFix> {
    let mut q = PENDING_FIXES.lock().unwrap_or_else(|e| e.into_inner());
    core::mem::take(&mut *q)
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

    #[test]
    fn async_fixes_round_trip_through_manager() {
        // The channel is process-global; clear any residue first.
        let _ = drain_location_fixes();

        push_location_fix(fix(37.0, -122.0));
        push_location_fix(fix(48.8566, 2.3522)); // Paris — last wins
        let drained = drain_location_fixes();
        assert_eq!(drained.len(), 2, "both parked fixes drain in order");
        assert_eq!(drained[0].latitude_deg, 37.0);
        assert_eq!(drained[1].latitude_deg, 48.8566);

        // Applying them reflects in latest_fix() — what the layout pass does.
        let mut mgr = GeolocationManager::new();
        for f in &drained {
            mgr.set_latest_fix(*f);
        }
        let got = mgr.latest_fix().expect("a fix was applied");
        assert_eq!(got.latitude_deg, 48.8566, "the last applied fix wins");

        // A second drain is empty — the queue was taken, not copied.
        assert!(drain_location_fixes().is_empty());
    }
}
