//! Geolocation manager — cross-platform state for the GPS/location surface
//! (`SUPER_PLAN_2` §1.5 + research/04 §3 + research/08 §6).
//!
//! Three callers drive it:
//!
//! - The **layout pass** scans the styled DOM for `GeolocationProbe`
//!   `NodeTypes`. When the first probe appears the framework fires
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

use azul_core::dom::DomNodeId;
use azul_core::events::{
    EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
};
use azul_core::task::Instant;

// `LocationFix` + `GeolocationProbeConfig` live in `azul-core` so
// `NodeType::GeolocationProbe(GeolocationProbeConfig)` can reference
// the config struct without a cyclic dep on `azul-layout`. We re-export
// them here for the existing `azul_layout::managers::geolocation::*`
// import paths.
pub use azul_core::geolocation::{GeolocationProbeConfig, LocationFix};

/// Diff event the layout pass emits when a probe appears or disappears.
/// Symmetric to `PermissionDiffEvent` — drives the platform backend's
/// native subscribe / release calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, u8)]
pub enum GeolocationDiffEvent {
    /// First probe of this config landed in the layout — start a
    /// native subscription with these options.
    Subscribe { config: GeolocationProbeConfig },
    /// Last probe left — stop the native subscription.
    Release,
    /// Probe config changed without subscriber churn — reconfigure
    /// the running subscription in place (e.g. `high_accuracy` false →
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
    /// `true` when a fix advanced since the last event-pass drain. Set by
    /// [`set_latest_fix`](Self::set_latest_fix), read by the `EventProvider`
    /// impl (yields `EventType::GeolocationFix`), cleared by
    /// [`clear_pending_event`](Self::clear_pending_event) after dispatch
    /// (MWA-A1 — this manager previously computed fixes nobody dispatched).
    pending_event: bool,
    /// Most recent backend error (subscription failed / timed out /
    /// revoked), or `None`. Cleared on the next successful fix (MWA-A1b).
    pub last_error: Option<LocationError>,
    /// `true` when an error arrived since the last event-pass drain — the
    /// `EventProvider` impl yields `EventType::GeolocationError` for it.
    pending_error_event: bool,
}

/// Error from the native geolocation backend (MWA-A1b).
///
/// Layout-internal
/// (not FFI-exposed) until the `CallbackInfo` getter lands with the Phase C
/// geolocation item — the `GeolocationError` EVENT carries no payload, so
/// callbacks poll this via the manager.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocationError {
    /// Platform-specific error code (`CLError` code / `GError` code / HRESULT).
    pub code: u32,
    /// Human-readable message from the backend.
    pub message: String,
}

impl GeolocationManager {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    #[must_use] pub const fn latest_fix(&self) -> Option<LocationFix> {
        self.latest_fix
    }

    #[must_use] pub const fn refcount(&self) -> u32 {
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
        let changed = self
            .latest_fix
            .is_none_or(|prev| !Self::location_fix_bitwise_eq(&prev, &fix));
        self.latest_fix = Some(fix);
        if changed {
            self.pending_event = true;
            // A successful fix supersedes any earlier error.
            self.last_error = None;
        }
        changed
    }

    /// Platform backend reports a subscription error (MWA-A1b). Always
    /// raises the error-event flag — repeated errors each answer a live
    /// subscription and callbacks should hear every one.
    pub fn set_last_error(&mut self, error: LocationError) {
        self.last_error = Some(error);
        self.pending_error_event = true;
    }

    /// Clear the pending-event flags (fix + error). The dll calls this
    /// after the event pass has collected the geolocation events.
    pub const fn clear_pending_event(&mut self) {
        self.pending_event = false;
        self.pending_error_event = false;
    }

    /// `true` while at least one `GeolocationProbe` is mounted — the
    /// capability pump keeps its drain timer armed while this holds, so
    /// fixes parked by the native backend reach callbacks without waiting
    /// for unrelated input (MWA-A1 arming signal).
    #[must_use] pub const fn has_active_subscription(&self) -> bool {
        self.refcount > 0
    }

    const fn location_fix_bitwise_eq(a: &LocationFix, b: &LocationFix) -> bool {
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

impl EventProvider for GeolocationManager {
    /// Yield a window-level `GeolocationFix` event when a fix advanced since
    /// the last drain (target = root; read the value via
    /// `CallbackInfo::get_geolocation_fix` inside the callback). Mirrors the
    /// sensor / gamepad providers; before MWA-A1 nothing ever produced
    /// `EventType::GeolocationFix`, so fix callbacks could never fire.
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();
        if self.pending_event {
            events.push(SyntheticEvent::new(
                EventType::GeolocationFix,
                CoreEventSource::User,
                DomNodeId::ROOT,
                timestamp.clone(),
                EventData::None,
            ));
        }
        if self.pending_error_event {
            events.push(SyntheticEvent::new(
                EventType::GeolocationError,
                CoreEventSource::User,
                DomNodeId::ROOT,
                timestamp,
                EventData::None,
            ));
        }
        events
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
    let mut q = PENDING_FIXES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(fix);
}

/// Drain every fix parked by [`push_location_fix`], in arrival order.
/// Called once per layout pass; the caller applies them through
/// [`GeolocationManager::set_latest_fix`] (the last one wins).
pub fn drain_location_fixes() -> Vec<LocationFix> {
    let mut q = PENDING_FIXES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

// Error channel (MWA-A1b) — same shape as the fix channel: the native
// backend's error callback fires on an OS thread and parks here; the
// capability pump drains into `set_last_error`.

static PENDING_ERRORS: std::sync::Mutex<Vec<LocationError>> =
    std::sync::Mutex::new(Vec::new());

/// Park a geolocation error delivered by a platform backend (in the dll).
/// Thread-safe; poison-recovering.
pub fn push_location_error(error: LocationError) {
    let mut q = PENDING_ERRORS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(error);
}

/// Drain every error parked by [`push_location_error`], in arrival order.
pub fn drain_location_errors() -> Vec<LocationError> {
    let mut q = PENDING_ERRORS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        drop(mgr.take_pending_events());

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
        drop(mgr.take_pending_events());

        mgr.diff_layout(|emit| emit(high_accuracy_cfg()));
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        match ev {
            GeolocationDiffEvent::Reconfigure { config } => {
                assert!(config.high_accuracy);
            }
            _ => panic!("expected Reconfigure, got {ev:?}"),
        }
    }

    #[test]
    fn stable_config_does_not_re_emit() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(cfg()));
        drop(mgr.take_pending_events());

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
    fn provider_yields_fix_event_then_clears() {
        use azul_core::task::{Instant, SystemTick};

        let ts = Instant::Tick(SystemTick::new(0));
        let mut mgr = GeolocationManager::new();
        assert!(mgr.get_pending_events(ts.clone()).is_empty(), "no fix yet");

        mgr.set_latest_fix(fix(37.0, -122.0));
        let events = mgr.get_pending_events(ts.clone());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::GeolocationFix);

        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts.clone()).is_empty(), "cleared after dispatch");

        // An identical fix is not a change — no re-fire.
        mgr.set_latest_fix(fix(37.0, -122.0));
        assert!(mgr.get_pending_events(ts).is_empty());
    }

    #[test]
    fn error_channel_and_provider_event() {
        use azul_core::task::{Instant, SystemTick};

        drop(drain_location_errors());
        push_location_error(LocationError { code: 1, message: "denied".into() });
        let errs = drain_location_errors();
        assert_eq!(errs.len(), 1);
        assert!(drain_location_errors().is_empty());

        let ts = Instant::Tick(SystemTick::new(0));
        let mut mgr = GeolocationManager::new();
        mgr.set_last_error(errs[0].clone());
        let events = mgr.get_pending_events(ts.clone());
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event_type,
            EventType::GeolocationError
        );
        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts).is_empty());

        // a successful (changed) fix supersedes the stored error
        mgr.set_latest_fix(fix(1.0, 2.0));
        assert!(mgr.last_error.is_none());
    }

    #[test]
    fn subscription_flag_follows_probe_refcount() {
        let mut mgr = GeolocationManager::new();
        assert!(!mgr.has_active_subscription());
        mgr.diff_layout(|emit| emit(cfg()));
        assert!(mgr.has_active_subscription());
        mgr.diff_layout(|_emit| {});
        assert!(!mgr.has_active_subscription());
    }

    #[test]
    #[allow(clippy::float_cmp)] // test asserts exact float equality on deterministic values
    fn async_fixes_round_trip_through_manager() {
        // The channel is process-global; clear any residue first.
        drop(drain_location_fixes());

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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::eq_op, clippy::redundant_clone)]
mod autotest_generated {
    use azul_core::task::{Instant, SystemTick};

    use super::*;

    // ─────────────────────────── helpers ────────────────────────────

    /// A fix where every optional field is *present* — knocking out a single
    /// field then flips exactly one comparison.
    const fn full_fix() -> LocationFix {
        LocationFix {
            latitude_deg: 48.208_8,
            longitude_deg: 16.372_1,
            accuracy_m: 5.0,
            altitude_m: 171.0,
            altitude_accuracy_m: 3.0,
            heading_deg: 90.0,
            speed_mps: 1.5,
            timestamp_ms: 1_234,
        }
    }

    /// "Platform reported nothing but lat/lon" — the NaN-heavy shape
    /// `set_latest_fix` documents as the reason it compares bit patterns.
    const fn nan_fix() -> LocationFix {
        LocationFix {
            latitude_deg: 37.0,
            longitude_deg: -122.0,
            accuracy_m: 10.0,
            altitude_m: f32::NAN,
            altitude_accuracy_m: f32::NAN,
            heading_deg: f32::NAN,
            speed_mps: f32::NAN,
            timestamp_ms: 0,
        }
    }

    const fn probe_cfg(high_accuracy: bool, max_accuracy_m: f32) -> GeolocationProbeConfig {
        GeolocationProbeConfig {
            high_accuracy,
            background: false,
            max_accuracy_m,
            min_interval_ms: 0,
        }
    }

    fn tick() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    /// The 8 single-field mutations of [`full_fix`], one per field the
    /// bit-pattern comparison has to look at.
    fn single_field_mutations() -> Vec<(&'static str, LocationFix)> {
        let base = full_fix();
        let mut out = Vec::new();

        let mut f = base;
        f.latitude_deg = -48.208_8;
        out.push(("latitude_deg", f));

        let mut f = base;
        f.longitude_deg = 16.372_2;
        out.push(("longitude_deg", f));

        let mut f = base;
        f.accuracy_m = 5.000_001;
        out.push(("accuracy_m", f));

        let mut f = base;
        f.altitude_m = f32::NAN;
        out.push(("altitude_m", f));

        let mut f = base;
        f.altitude_accuracy_m = f32::NAN;
        out.push(("altitude_accuracy_m", f));

        let mut f = base;
        f.heading_deg = 270.0;
        out.push(("heading_deg", f));

        let mut f = base;
        f.speed_mps = -1.5;
        out.push(("speed_mps", f));

        let mut f = base;
        f.timestamp_ms = u64::MAX;
        out.push(("timestamp_ms", f));

        out
    }

    // ───────────────────── constructor / getters ────────────────────

    #[test]
    fn new_equals_default_and_holds_construction_invariants() {
        let mgr = GeolocationManager::new();
        assert_eq!(mgr, GeolocationManager::default());
        assert_eq!(mgr, GeolocationManager::new(), "construction is deterministic");

        assert_eq!(mgr.latest_fix(), None);
        assert_eq!(mgr.refcount(), 0);
        assert_eq!(mgr.active_config, None);
        assert_eq!(mgr.last_error, None);
        assert!(!mgr.has_active_subscription());
        assert!(mgr.get_pending_events(tick()).is_empty());
    }

    #[test]
    fn getters_on_a_fresh_manager_do_not_panic() {
        let mut mgr = GeolocationManager::new();
        // Repeated reads of an empty manager stay None / 0 and never panic.
        for _ in 0..3 {
            assert_eq!(mgr.latest_fix(), None);
            assert_eq!(mgr.refcount(), 0);
            assert!(!mgr.has_active_subscription());
            assert!(mgr.take_pending_events().is_empty());
            mgr.clear_pending_event();
        }
    }

    #[test]
    fn latest_fix_round_trips_extreme_bit_patterns() {
        let extreme = LocationFix {
            latitude_deg: f64::INFINITY,
            longitude_deg: f64::NEG_INFINITY,
            accuracy_m: f32::MAX,
            altitude_m: f32::MIN_POSITIVE,
            altitude_accuracy_m: f32::from_bits(1), // subnormal
            heading_deg: -0.0,
            speed_mps: f32::NEG_INFINITY,
            timestamp_ms: u64::MAX,
        };

        let mut mgr = GeolocationManager::new();
        assert!(mgr.set_latest_fix(extreme), "first fix is always a change");

        let got = mgr.latest_fix().expect("a fix was stored");
        // encode == decode, bit for bit (not float-eq: -0.0 and subnormals).
        assert_eq!(got.latitude_deg.to_bits(), extreme.latitude_deg.to_bits());
        assert_eq!(got.longitude_deg.to_bits(), extreme.longitude_deg.to_bits());
        assert_eq!(got.accuracy_m.to_bits(), extreme.accuracy_m.to_bits());
        assert_eq!(got.altitude_m.to_bits(), extreme.altitude_m.to_bits());
        assert_eq!(
            got.altitude_accuracy_m.to_bits(),
            extreme.altitude_accuracy_m.to_bits()
        );
        assert_eq!(got.heading_deg.to_bits(), extreme.heading_deg.to_bits());
        assert_eq!(got.speed_mps.to_bits(), extreme.speed_mps.to_bits());
        assert_eq!(got.timestamp_ms, u64::MAX);

        // Infinities are *not* NaN, so the getters report them as present.
        assert_eq!(got.speed(), Some(f32::NEG_INFINITY));
        assert!(mgr.latest_fix().is_some());
        // Re-applying the identical extreme fix is not a change.
        assert!(!mgr.set_latest_fix(extreme));
    }

    #[test]
    fn nan_fix_round_trips_and_getters_still_report_missing() {
        let mut mgr = GeolocationManager::new();
        assert!(mgr.set_latest_fix(nan_fix()));

        let got = mgr.latest_fix().expect("a fix was stored");
        assert_eq!(got.altitude(), None);
        assert_eq!(got.altitude_accuracy(), None);
        assert_eq!(got.heading(), None);
        assert_eq!(got.speed(), None);
        assert_eq!(got.latitude_deg.to_bits(), 37.0_f64.to_bits());
    }

    // ───────────────── location_fix_bitwise_eq (private) ─────────────

    #[test]
    fn bitwise_eq_is_reflexive_on_nan_unlike_partial_eq() {
        let f = nan_fix();
        // Derived PartialEq is *not* reflexive here — that is the whole
        // reason `set_latest_fix` compares bit patterns instead.
        assert!(f != f, "PartialEq on a NaN-carrying fix is not reflexive");
        assert!(GeolocationManager::location_fix_bitwise_eq(&f, &f));
        assert!(GeolocationManager::location_fix_bitwise_eq(&nan_fix(), &nan_fix()));
    }

    #[test]
    fn bitwise_eq_is_reflexive_and_symmetric_over_extremes() {
        let extremes = [
            full_fix(),
            nan_fix(),
            LocationFix {
                latitude_deg: f64::INFINITY,
                longitude_deg: f64::NEG_INFINITY,
                accuracy_m: f32::INFINITY,
                altitude_m: f32::MAX,
                altitude_accuracy_m: f32::MIN,
                heading_deg: f32::from_bits(1),
                speed_mps: -0.0,
                timestamp_ms: u64::MAX,
            },
            LocationFix {
                latitude_deg: 0.0,
                longitude_deg: 0.0,
                accuracy_m: 0.0,
                altitude_m: 0.0,
                altitude_accuracy_m: 0.0,
                heading_deg: 0.0,
                speed_mps: 0.0,
                timestamp_ms: 0,
            },
        ];

        for (i, a) in extremes.iter().enumerate() {
            assert!(
                GeolocationManager::location_fix_bitwise_eq(a, a),
                "not reflexive for extreme #{i}"
            );
            for (j, b) in extremes.iter().enumerate() {
                assert_eq!(
                    GeolocationManager::location_fix_bitwise_eq(a, b),
                    GeolocationManager::location_fix_bitwise_eq(b, a),
                    "not symmetric for ({i}, {j})"
                );
                if i != j {
                    assert!(
                        !GeolocationManager::location_fix_bitwise_eq(a, b),
                        "distinct extremes #{i}/#{j} compared equal"
                    );
                }
            }
        }
    }

    #[test]
    fn bitwise_eq_looks_at_every_field() {
        let base = full_fix();
        for (field, mutated) in single_field_mutations() {
            assert!(
                !GeolocationManager::location_fix_bitwise_eq(&base, &mutated),
                "`{field}` is ignored by location_fix_bitwise_eq"
            );
            assert!(
                !GeolocationManager::location_fix_bitwise_eq(&mutated, &base),
                "`{field}` comparison is asymmetric"
            );
            assert!(GeolocationManager::location_fix_bitwise_eq(&mutated, &mutated));
        }
    }

    #[test]
    fn bitwise_eq_separates_positive_and_negative_zero() {
        let mut a = full_fix();
        a.heading_deg = 0.0;
        let mut b = a;
        b.heading_deg = -0.0;

        // Float equality says these are the same heading; the bit patterns
        // do not. `set_latest_fix` therefore treats +0 → -0 as a change.
        assert!(a.heading_deg == b.heading_deg);
        assert!(!GeolocationManager::location_fix_bitwise_eq(&a, &b));
    }

    // ─────────────────────── set_latest_fix ─────────────────────────

    #[test]
    fn repeated_identical_nan_fix_reports_no_change() {
        let mut mgr = GeolocationManager::new();
        assert!(mgr.set_latest_fix(nan_fix()), "first fix advances");
        for _ in 0..100 {
            assert!(
                !mgr.set_latest_fix(nan_fix()),
                "an unchanged NaN-carrying fix must not look like movement"
            );
        }
        // The one real change is still pending until the event pass drains it.
        assert_eq!(mgr.get_pending_events(tick()).len(), 1);
    }

    #[test]
    fn set_latest_fix_detects_a_change_in_every_field() {
        for (field, mutated) in single_field_mutations() {
            let mut mgr = GeolocationManager::new();
            assert!(mgr.set_latest_fix(full_fix()));
            mgr.clear_pending_event();

            assert!(
                mgr.set_latest_fix(mutated),
                "a change in `{field}` was not reported"
            );
            assert_eq!(
                mgr.get_pending_events(tick()).len(),
                1,
                "a change in `{field}` raised no GeolocationFix event"
            );
        }
    }

    #[test]
    fn set_latest_fix_survives_extreme_and_pathological_values() {
        let mut mgr = GeolocationManager::new();

        // Out-of-range coordinates: the manager stores whatever the backend
        // hands it, it is not a validator — but it must not panic.
        let pathological = [
            (f64::MAX, f64::MIN),
            (f64::INFINITY, f64::NEG_INFINITY),
            (f64::NAN, f64::NAN),
            (1e308, -1e308),
            (f64::MIN_POSITIVE, -f64::MIN_POSITIVE),
            (91.0, 181.0),    // beyond the WGS-84 range
            (-91.0, -181.0),  // …and the other way
        ];

        for (lat, lon) in pathological {
            let f = LocationFix {
                latitude_deg: lat,
                longitude_deg: lon,
                accuracy_m: f32::INFINITY,
                altitude_m: f32::NAN,
                altitude_accuracy_m: f32::NAN,
                heading_deg: f32::NAN,
                speed_mps: f32::NAN,
                timestamp_ms: u64::MAX,
            };
            mgr.set_latest_fix(f);
            let got = mgr.latest_fix().expect("stored");
            assert_eq!(got.latitude_deg.to_bits(), lat.to_bits());
            assert_eq!(got.longitude_deg.to_bits(), lon.to_bits());
            // Idempotent re-apply: even an all-NaN coordinate pair compares
            // equal to itself under bit-pattern equality.
            assert!(!mgr.set_latest_fix(f), "re-applying the same fix is not a change");
        }
    }

    #[test]
    fn timestamp_only_movement_counts_as_a_change() {
        let mut mgr = GeolocationManager::new();
        let mut f = nan_fix();
        assert!(mgr.set_latest_fix(f));

        f.timestamp_ms = u64::MAX;
        assert!(
            mgr.set_latest_fix(f),
            "a fresh sample at the same coordinates is still a new fix"
        );
        assert_eq!(mgr.latest_fix().expect("stored").timestamp_ms, u64::MAX);
    }

    #[test]
    fn a_changed_fix_supersedes_the_stored_error_but_an_unchanged_one_does_not() {
        let mut mgr = GeolocationManager::new();
        assert!(mgr.set_latest_fix(full_fix()));

        mgr.set_last_error(LocationError {
            code: 2,
            message: String::from("timed out"),
        });
        assert!(mgr.last_error.is_some());

        // Re-delivering the *same* fix is not a change, so the error survives.
        assert!(!mgr.set_latest_fix(full_fix()));
        assert!(
            mgr.last_error.is_some(),
            "an unchanged fix does not clear the error (documented `changed`-gated behaviour)"
        );

        // A fix that actually advanced clears it.
        assert!(mgr.set_latest_fix(nan_fix()));
        assert_eq!(mgr.last_error, None);
    }

    // ─────────────────────── set_last_error ─────────────────────────

    #[test]
    fn set_last_error_round_trips_extreme_payloads() {
        let huge = "x".repeat(1 << 16);
        let unicode = String::from("\u{0}dénié 🛰️\u{0301}\u{fffd}中文\u{1f680}");
        let payloads = [
            LocationError { code: 0, message: String::new() },
            LocationError { code: u32::MAX, message: huge.clone() },
            LocationError { code: 1, message: unicode.clone() },
        ];

        for err in payloads {
            let mut mgr = GeolocationManager::new();
            mgr.set_last_error(err.clone());
            let got = mgr.last_error.clone().expect("error was stored");
            assert_eq!(got, err, "error payload did not round-trip");
            assert_eq!(got.message.len(), err.message.len());

            let events = mgr.get_pending_events(tick());
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].event_type, EventType::GeolocationError);
        }

        // The huge/unicode strings survived untouched.
        assert_eq!(huge.len(), 1 << 16);
        assert!(unicode.contains('\u{0}'));
    }

    #[test]
    fn every_repeated_error_raises_its_own_event() {
        let mut mgr = GeolocationManager::new();
        let err = LocationError {
            code: 7,
            message: String::from("revoked"),
        };

        for _ in 0..5 {
            mgr.set_last_error(err.clone());
            let events = mgr.get_pending_events(tick());
            assert_eq!(
                events.len(),
                1,
                "a repeated error must still answer the live subscription"
            );
            assert_eq!(events[0].event_type, EventType::GeolocationError);
            mgr.clear_pending_event();
            assert!(mgr.get_pending_events(tick()).is_empty());
        }
        assert_eq!(mgr.last_error, Some(err));
    }

    // ───────────── clear_pending_event / take_pending_events ─────────

    #[test]
    fn clear_pending_event_is_idempotent_and_spares_the_diff_queue() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(probe_cfg(false, 0.0)));
        mgr.set_latest_fix(full_fix());
        mgr.set_last_error(LocationError {
            code: 1,
            message: String::from("e"),
        });

        for _ in 0..3 {
            mgr.clear_pending_event();
            assert!(
                mgr.get_pending_events(tick()).is_empty(),
                "clearing twice must stay cleared"
            );
        }

        // Clearing the *event* flags does not eat the queued *diff* events —
        // they are two independent queues with two independent drains.
        let diffs = mgr.take_pending_events();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], GeolocationDiffEvent::Subscribe { .. }));
        // …nor the stored values themselves.
        assert!(mgr.latest_fix().is_some());
        assert!(mgr.last_error.is_some());
    }

    #[test]
    fn take_pending_events_drains_in_arrival_order_and_leaves_the_queue_empty() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(probe_cfg(false, 0.0))); // Subscribe
        mgr.diff_layout(|emit| emit(probe_cfg(true, 0.0))); // Reconfigure
        mgr.diff_layout(|_emit| {}); // Release

        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], GeolocationDiffEvent::Subscribe { .. }));
        assert!(matches!(events[1], GeolocationDiffEvent::Reconfigure { .. }));
        assert_eq!(events[2], GeolocationDiffEvent::Release);

        assert!(mgr.take_pending_events().is_empty(), "taken, not copied");
        assert!(mgr.take_pending_events().is_empty(), "still empty");
    }

    #[test]
    fn get_pending_events_does_not_consume_the_flags() {
        let mut mgr = GeolocationManager::new();
        mgr.set_latest_fix(full_fix());
        mgr.set_last_error(LocationError {
            code: 3,
            message: String::from("boom"),
        });

        // Both flags are up: fix first, then error.
        for _ in 0..3 {
            let events = mgr.get_pending_events(tick());
            assert_eq!(events.len(), 2, "the getter is a read, not a drain");
            assert_eq!(events[0].event_type, EventType::GeolocationFix);
            assert_eq!(events[1].event_type, EventType::GeolocationError);
            assert_eq!(events[0].target, DomNodeId::ROOT);
            assert_eq!(events[1].target, DomNodeId::ROOT);
        }

        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(tick()).is_empty());
    }

    // ──────────────────────── diff_layout ───────────────────────────

    #[test]
    fn no_probes_on_a_fresh_manager_emits_nothing() {
        let mut mgr = GeolocationManager::new();
        for _ in 0..4 {
            mgr.diff_layout(|_emit| {});
            assert_eq!(mgr.refcount(), 0);
            assert!(!mgr.has_active_subscription());
            assert_eq!(mgr.active_config, None);
            assert!(
                mgr.take_pending_events().is_empty(),
                "0 → 0 must not emit a spurious Release"
            );
        }
    }

    #[test]
    fn refcount_tracks_the_probe_count_and_gates_the_subscription_flag() {
        let mut mgr = GeolocationManager::new();

        mgr.diff_layout(|emit| {
            for _ in 0..10_000 {
                emit(probe_cfg(false, 0.0));
            }
        });
        assert_eq!(mgr.refcount(), 10_000);
        assert!(mgr.has_active_subscription());
        assert_eq!(mgr.take_pending_events().len(), 1, "one Subscribe, not 10k");

        // Probe count churn with an unchanged config emits nothing at all.
        mgr.diff_layout(|emit| {
            for _ in 0..3 {
                emit(probe_cfg(false, 0.0));
            }
        });
        assert_eq!(mgr.refcount(), 3);
        assert!(mgr.has_active_subscription());
        assert!(mgr.take_pending_events().is_empty());

        // The boundary: exactly one probe still counts as subscribed.
        mgr.diff_layout(|emit| emit(probe_cfg(false, 0.0)));
        assert_eq!(mgr.refcount(), 1);
        assert!(mgr.has_active_subscription());

        mgr.diff_layout(|_emit| {});
        assert_eq!(mgr.refcount(), 0);
        assert!(!mgr.has_active_subscription(), "0 probes ⇒ no subscription");
        assert_eq!(mgr.take_pending_events(), vec![GeolocationDiffEvent::Release]);
    }

    #[test]
    fn the_first_probes_config_wins_the_subscribe() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| {
            emit(probe_cfg(true, 25.0));
            emit(probe_cfg(false, 999.0)); // disagreeing second probe
            emit(probe_cfg(false, 0.0));
        });
        assert_eq!(mgr.refcount(), 3);

        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        match events[0] {
            GeolocationDiffEvent::Subscribe { config } => {
                assert!(config.high_accuracy);
                assert_eq!(config.max_accuracy_m, 25.0);
            }
            ref other => panic!("expected Subscribe, got {other:?}"),
        }
        assert_eq!(mgr.active_config, Some(probe_cfg(true, 25.0)));

        // A frame whose *first* probe keeps the active config emits nothing,
        // even when a later probe disagrees.
        mgr.diff_layout(|emit| {
            emit(probe_cfg(true, 25.0));
            emit(probe_cfg(false, 0.0));
        });
        assert!(mgr.take_pending_events().is_empty());
    }

    #[test]
    fn release_clears_the_fix_and_a_resubscribe_starts_from_scratch() {
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(probe_cfg(false, 0.0)));
        assert!(mgr.set_latest_fix(full_fix()));
        assert!(mgr.take_pending_events().len() == 1);

        mgr.diff_layout(|_emit| {}); // Release
        assert_eq!(mgr.latest_fix(), None, "Release drops the stale fix");
        assert_eq!(mgr.active_config, None);
        assert_eq!(mgr.take_pending_events(), vec![GeolocationDiffEvent::Release]);

        // Re-mounting emits a fresh Subscribe (not a Reconfigure) and the fix
        // stays None until the backend delivers a new sample.
        mgr.diff_layout(|emit| emit(probe_cfg(true, 5.0)));
        assert_eq!(mgr.refcount(), 1);
        assert_eq!(mgr.latest_fix(), None);
        let events = mgr.take_pending_events();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            GeolocationDiffEvent::Subscribe {
                config: probe_cfg(true, 5.0)
            }
        );
    }

    #[test]
    fn release_leaves_the_fix_event_flag_raised_with_no_fix_behind_it() {
        // Characterisation of the current behaviour: `diff_layout` clears
        // `latest_fix` on Release but not `pending_event`, so the event pass
        // still dispatches a GeolocationFix whose payload is already gone.
        // Callbacks must therefore tolerate `get_geolocation_fix() == None`
        // inside a GeolocationFix handler.
        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(probe_cfg(false, 0.0)));
        assert!(mgr.set_latest_fix(full_fix()));

        mgr.diff_layout(|_emit| {}); // Release
        assert_eq!(mgr.latest_fix(), None);

        let events = mgr.get_pending_events(tick());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::GeolocationFix);

        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(tick()).is_empty());
    }

    #[test]
    fn a_nan_probe_config_re_emits_reconfigure_on_every_frame() {
        // Characterisation: the Reconfigure check uses `PartialEq` on
        // `GeolocationProbeConfig`, whose `max_accuracy_m` is an f32. A NaN
        // there never compares equal to itself, so an *unchanged* config
        // still looks drifted and every frame queues another Reconfigure.
        // (`set_latest_fix` avoids exactly this trap by comparing bits.)
        let nan_cfg = probe_cfg(false, f32::NAN);

        let mut mgr = GeolocationManager::new();
        mgr.diff_layout(|emit| emit(nan_cfg));
        assert_eq!(mgr.take_pending_events().len(), 1, "the initial Subscribe");

        for _ in 0..3 {
            mgr.diff_layout(|emit| emit(nan_cfg));
            let events = mgr.take_pending_events();
            assert_eq!(
                events.len(),
                1,
                "an unchanged NaN config still queues a Reconfigure every frame"
            );
            assert!(matches!(events[0], GeolocationDiffEvent::Reconfigure { .. }));
        }

        // A sane config settles immediately, by contrast.
        mgr.diff_layout(|emit| emit(probe_cfg(false, 0.0)));
        assert_eq!(mgr.take_pending_events().len(), 1, "one Reconfigure to sane");
        mgr.diff_layout(|emit| emit(probe_cfg(false, 0.0)));
        assert!(mgr.take_pending_events().is_empty(), "then quiet");
    }

    #[test]
    fn extreme_probe_configs_survive_the_diff() {
        let extremes = [
            probe_cfg(true, f32::MAX),
            probe_cfg(false, f32::INFINITY),
            probe_cfg(true, -0.0),
            GeolocationProbeConfig {
                high_accuracy: true,
                background: true,
                max_accuracy_m: f32::MIN_POSITIVE,
                min_interval_ms: u32::MAX,
            },
        ];

        let mut mgr = GeolocationManager::new();
        for cfg in extremes {
            mgr.diff_layout(|emit| emit(cfg));
            assert_eq!(mgr.refcount(), 1);
            assert_eq!(mgr.active_config, Some(cfg));
            let events = mgr.take_pending_events();
            assert_eq!(events.len(), 1, "each distinct config emits exactly one event");
            match events[0] {
                GeolocationDiffEvent::Subscribe { config }
                | GeolocationDiffEvent::Reconfigure { config } => {
                    assert_eq!(config.max_accuracy_m.to_bits(), cfg.max_accuracy_m.to_bits());
                    assert_eq!(config.min_interval_ms, cfg.min_interval_ms);
                }
                GeolocationDiffEvent::Release => panic!("probes are mounted, not released"),
            }
        }
    }

    // ─────────────────────── equality invariants ────────────────────

    #[test]
    fn manager_equality_is_bit_blind_for_nan_fixes() {
        // The derived `PartialEq` on the manager inherits float semantics:
        // a manager holding a NaN-carrying fix is not even equal to its own
        // clone. Callers must not use `==` on the manager to detect movement
        // — that is what `set_latest_fix`'s return value is for.
        let mut with_nan = GeolocationManager::new();
        with_nan.set_latest_fix(nan_fix());
        assert_ne!(with_nan, with_nan.clone());

        let mut without_nan = GeolocationManager::new();
        without_nan.set_latest_fix(full_fix());
        assert_eq!(without_nan, without_nan.clone());
    }
}
