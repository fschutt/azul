//! Sensor manager — cross-platform state for the motion-sensor surface
//! (`SUPER_PLAN_2` §1 feature 5 + research/03).
//!
//! Continuous + push-driven, like geolocation:
//!
//! - The **platform backend** (`dll/src/desktop/extra/sensors/<plat>.rs`)
//!   subscribes to `CoreMotion` (`CMMotionManager`) / Android `SensorManager`
//!   and calls [`push_sensor_reading`] on every sample (arbitrary thread).
//! - The dll **layout pass** drains the channel via
//!   [`drain_sensor_readings`] and folds each into the manager through
//!   [`SensorManager::set_reading`].
//! - **Callbacks** read `reading(kind)` synchronously (via
//!   `CallbackInfo::get_sensor_reading`) to drive tilt / shake / compass UI.
//!
//! One reading slot per [`SensorKind`]. No platform deps
//! (`SUPER_PLAN_2` §0.5); the channel mirrors `geolocation.rs` verbatim.

use alloc::vec::Vec;

use azul_core::dom::DomNodeId;
use azul_core::events::{
    EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
};
use azul_core::task::Instant;
pub use azul_core::sensors::{SensorKind, SensorReading};

/// Cross-platform sensor state. One per `App` — the OS exposes a single
/// per-process sensor subscription, not per-window.
#[derive(Copy, Debug, Clone, PartialEq, Default)]
pub struct SensorManager {
    /// Latest accelerometer reading (m/s²), or `None` until a sample arrives.
    pub accelerometer: Option<SensorReading>,
    /// Latest gyroscope reading (rad/s).
    pub gyroscope: Option<SensorReading>,
    /// Latest magnetometer reading (µT).
    pub magnetometer: Option<SensorReading>,
    /// `true` when a reading advanced since the last event-pass drain. Set by
    /// [`set_reading`](Self::set_reading), read by the `EventProvider` impl,
    /// cleared by [`clear_pending_event`](Self::clear_pending_event).
    pub pending_event: bool,
    /// `true` while any node in the current layout registers a
    /// `SensorChanged` callback (Hover or Window filter). Recomputed on every
    /// relayout by the DOM walk in `shell2::common::layout`; the capability
    /// pump polls the platform sensor backend only while this is set
    /// (MWA-A1 arming signal — no listeners, no polling, no timer).
    pub has_listeners: bool,
}

impl SensorManager {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Latest reading for `kind`, or `None` if no backend has delivered one.
    #[must_use] pub const fn reading(&self, kind: SensorKind) -> Option<SensorReading> {
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
        let changed = slot.as_mut().is_none_or(|prev| !reading_bitwise_eq(prev, &reading));
        *slot = Some(reading);
        if changed {
            self.pending_event = true;
        }
        changed
    }

    /// Clear the pending-event flag. The dll calls this after the event pass
    /// has collected the `SensorChanged` event (mirrors `clear_changeset`).
    pub const fn clear_pending_event(&mut self) {
        self.pending_event = false;
    }

    /// Relayout walk reports whether any node listens for `SensorChanged`.
    pub const fn set_has_listeners(&mut self, has: bool) {
        self.has_listeners = has;
    }

    /// `true` while the capability pump should poll the sensor backend.
    #[must_use] pub const fn has_listeners(&self) -> bool {
        self.has_listeners
    }
}

impl EventProvider for SensorManager {
    /// Yield a window-level `SensorChanged` event when a reading advanced
    /// since the last drain (target = root; read the value via
    /// `CallbackInfo::get_sensor_reading` inside the callback).
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        if self.pending_event {
            alloc::vec![SyntheticEvent::new(
                EventType::SensorChanged,
                CoreEventSource::User,
                DomNodeId::ROOT,
                timestamp,
                EventData::None,
            )]
        } else {
            Vec::new()
        }
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
    let mut q = PENDING_READINGS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(reading);
}

/// Drain every reading parked by [`push_sensor_reading`], in arrival order.
/// Called once per layout pass; the caller applies them through
/// [`SensorManager::set_reading`] (the last per kind wins).
pub fn drain_sensor_readings() -> Vec<SensorReading> {
    let mut q = PENDING_READINGS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_flag_gates_polling_decision() {
        let mut mgr = SensorManager::new();
        assert!(!mgr.has_listeners(), "no listeners until the relayout walk reports some");
        mgr.set_has_listeners(true);
        assert!(mgr.has_listeners());
        mgr.set_has_listeners(false);
        assert!(!mgr.has_listeners());
    }

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
        drop(drain_sensor_readings());

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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::eq_op)] // bit-exactness IS the invariant under test
mod autotest_generated {
    // Imported explicitly (not just via the `super::*` glob) so the trait
    // method `get_pending_events` and `Vec` resolve regardless of how the
    // parent's own imports are re-exported.
    use alloc::vec::Vec;

    use azul_core::{
        dom::DomNodeId,
        events::{EventProvider, EventType},
        task::{Instant, SystemTick},
    };

    use super::*;

    // ─────────────────────────── helpers ────────────────────────────
    //
    // NOTE — the process-global `PENDING_READINGS` channel
    // (`push_sensor_reading` / `drain_sensor_readings`) is deliberately NOT
    // exercised here. `tests::readings_round_trip_through_manager` above
    // asserts an *exact* drain count (`len() == 3`) on that same global, and
    // libtest runs the two modules' tests concurrently in one binary — any
    // push or drain from here could be observed by (or steal readings from)
    // that test and make it flake. The sibling `geolocation.rs` autotest
    // module leaves its identical channel alone for the same reason.

    const KINDS: [SensorKind; 3] = [
        SensorKind::Accelerometer,
        SensorKind::Gyroscope,
        SensorKind::Magnetometer,
    ];

    const fn r(kind: SensorKind, x: f32, y: f32, z: f32, timestamp_ms: u64) -> SensorReading {
        SensorReading {
            kind,
            x,
            y,
            z,
            timestamp_ms,
        }
    }

    /// A representative "real sample" — every field distinct and non-zero, so
    /// knocking out a single field flips exactly one comparison.
    const fn base() -> SensorReading {
        r(SensorKind::Accelerometer, 1.5, -2.25, 9.81, 1_234)
    }

    /// The shape `set_reading` documents as its reason for comparing bit
    /// patterns: a backend that reports "axis missing" as `NaN`.
    const fn nan_sample(kind: SensorKind) -> SensorReading {
        r(kind, f32::NAN, f32::NAN, f32::NAN, 7)
    }

    fn tick() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    /// The smallest possible perturbation of a finite float: flip the lowest
    /// mantissa bit. The result is a *different* bit pattern by construction,
    /// so "did this sample advance?" must answer yes for it.
    fn ulp_flip(v: f32) -> f32 {
        f32::from_bits(v.to_bits() ^ 1)
    }

    /// Every field of a `SensorReading`, compared bit-for-bit — the property
    /// the manager promises when it stores a sample.
    fn bits_identical(a: &SensorReading, b: &SensorReading) -> bool {
        a.kind == b.kind
            && a.x.to_bits() == b.x.to_bits()
            && a.y.to_bits() == b.y.to_bits()
            && a.z.to_bits() == b.z.to_bits()
            && a.timestamp_ms == b.timestamp_ms
    }

    /// Float values that break naive `==` comparisons: both zeroes, both
    /// infinities, two distinct NaN bit patterns, the subnormal floor and the
    /// finite extremes.
    fn hostile_floats() -> [f32; 12] {
        [
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MIN_POSITIVE,
            f32::from_bits(1), // smallest subnormal
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::from_bits(0xffc0_0000), // negative NaN
        ]
    }

    // ───────────────────── SensorManager::new (constructor) ─────────────────

    /// no_panic + invariants_hold: a fresh manager has no readings, no pending
    /// event and no listeners — the "cold start, don't poll the backend" state.
    #[test]
    fn new_starts_cold_and_matches_default() {
        let mgr = SensorManager::new();
        for kind in KINDS {
            assert_eq!(mgr.reading(kind), None, "{kind:?} slot must start empty");
        }
        assert!(!mgr.pending_event, "no event may be pending before any sample");
        assert!(!mgr.has_listeners(), "polling must not be armed at construction");
        assert_eq!(mgr, SensorManager::default(), "new() must equal Default");
        assert_eq!(mgr, SensorManager::new(), "new() must be deterministic");
    }

    // ───────────────────────── reading (accessor) ───────────────────────────

    /// invariant: the three slots are independent — writing one kind must not
    /// make any other kind report a reading.
    #[test]
    fn reading_slots_are_independent_per_kind() {
        for written in KINDS {
            let mut mgr = SensorManager::new();
            assert!(mgr.set_reading(r(written, 1.0, 2.0, 3.0, 0)));
            for probed in KINDS {
                if probed == written {
                    assert_eq!(
                        mgr.reading(probed).map(|s| s.kind),
                        Some(written),
                        "{written:?} must land in its own slot, tagged with its kind"
                    );
                } else {
                    assert_eq!(
                        mgr.reading(probed),
                        None,
                        "writing {written:?} must not leak into the {probed:?} slot"
                    );
                }
            }
        }
    }

    /// invariant: `reading` hands back a *copy* (`SensorReading: Copy`), so a
    /// caller mutating it cannot corrupt the manager's stored sample.
    #[test]
    fn reading_returns_a_copy_not_an_alias() {
        let mut mgr = SensorManager::new();
        mgr.set_reading(base());
        let mut got = mgr.reading(SensorKind::Accelerometer).unwrap();
        got.x = -999.0;
        got.timestamp_ms = u64::MAX;
        assert_eq!(
            mgr.reading(SensorKind::Accelerometer),
            Some(base()),
            "mutating the returned copy must not write through to the manager"
        );
    }

    // ──────────────────────────── set_reading ───────────────────────────────

    /// The headline case from the doc comment: an all-`NaN` sample repeated
    /// verbatim must NOT look like it advanced. Under `PartialEq` every NaN
    /// compares unequal, so a naive `prev != new` would report a change on
    /// *every* sample and wake the event pass forever.
    #[test]
    fn repeated_nan_sample_does_not_advance() {
        for kind in KINDS {
            let mut mgr = SensorManager::new();
            let sample = nan_sample(kind);

            assert!(
                mgr.set_reading(sample),
                "{kind:?}: the first sample always advances (slot was empty)"
            );
            assert!(
                !mgr.set_reading(sample),
                "{kind:?}: an identical NaN sample must not be reported as changed"
            );
            assert!(
                !mgr.set_reading(sample),
                "{kind:?}: still no change on the third identical NaN sample"
            );

            // And the derived `PartialEq` really would have disagreed:
            let stored = mgr.reading(kind).unwrap();
            assert!(
                stored != sample,
                "{kind:?}: sanity — NaN fields make PartialEq report inequality, \
                 which is exactly why set_reading must compare bit patterns"
            );
        }
    }

    /// no_panic + invariant: the first sample into an empty slot always counts
    /// as an advance, even when every axis is NaN or infinite.
    #[test]
    fn first_sample_always_advances() {
        for kind in KINDS {
            for v in hostile_floats() {
                let mut mgr = SensorManager::new();
                assert!(
                    mgr.set_reading(r(kind, v, v, v, 0)),
                    "{kind:?}: first sample ({v:?}) must advance"
                );
                assert!(mgr.pending_event, "{kind:?}: an advance must arm the event");
            }
        }
    }

    /// round-trip: whatever is stored comes back bit-for-bit — NaN payloads,
    /// signed zeroes, infinities, subnormals, `u64::MAX` timestamps included.
    /// A resend of that exact sample is then correctly reported as *no* change.
    #[test]
    fn extremes_round_trip_bit_exactly_and_resend_is_idempotent() {
        for kind in KINDS {
            for x in hostile_floats() {
                for ts in [0_u64, 1, u64::MAX] {
                    let sample = r(kind, x, -x, x, ts);
                    let mut mgr = SensorManager::new();

                    assert!(mgr.set_reading(sample));
                    let stored = mgr.reading(kind).expect("just written");
                    assert!(
                        bits_identical(&stored, &sample),
                        "{kind:?}: stored sample must be bit-identical to the input \
                         (x = {x:?}, ts = {ts})"
                    );
                    assert!(
                        !mgr.set_reading(sample),
                        "{kind:?}: resending the identical sample must not advance \
                         (x = {x:?}, ts = {ts})"
                    );
                }
            }
        }
    }

    /// Every mutable field must be watched, down to the last mantissa bit.
    /// Perturbing x, y, z (by one ULP) or the timestamp alone has to register
    /// as an advance — a comparison that skipped one field would silently
    /// swallow that sensor axis.
    #[test]
    fn a_change_in_any_single_field_advances() {
        let mut with_x = base();
        with_x.x = ulp_flip(base().x);
        let mut with_y = base();
        with_y.y = ulp_flip(base().y);
        let mut with_z = base();
        with_z.z = ulp_flip(base().z);
        let mut with_ts = base();
        with_ts.timestamp_ms = base().timestamp_ms + 1;

        for (field, mutated) in [
            ("x", with_x),
            ("y", with_y),
            ("z", with_z),
            ("timestamp_ms", with_ts),
        ] {
            let mut mgr = SensorManager::new();
            assert!(mgr.set_reading(base()));
            assert!(
                mgr.set_reading(mutated),
                "a change in `{field}` alone must be reported as an advance"
            );
            assert!(
                !mgr.set_reading(mutated),
                "`{field}`: the mutated sample is now the previous one"
            );
            assert!(
                mgr.set_reading(base()),
                "`{field}`: reverting back to the original is also an advance"
            );
        }
    }

    /// Bit-pattern comparison, not numeric: `-0.0 == 0.0` is *true* for floats,
    /// but the two are different samples and must be reported as an advance.
    #[test]
    fn signed_zero_flip_advances_even_though_it_compares_equal() {
        assert_eq!(0.0_f32, -0.0_f32, "sanity: the two zeroes compare equal");

        let mut mgr = SensorManager::new();
        let pos = r(SensorKind::Gyroscope, 0.0, 0.0, 0.0, 0);
        let neg = r(SensorKind::Gyroscope, -0.0, 0.0, 0.0, 0);

        assert!(mgr.set_reading(pos));
        assert!(
            mgr.set_reading(neg),
            "+0.0 → -0.0 differs in bits and must count as an advance"
        );
        assert!(!mgr.set_reading(neg), "…but the same -0.0 twice must not");
        assert!(
            mgr.reading(SensorKind::Gyroscope)
                .is_some_and(|s| s.x.is_sign_negative()),
            "the -0.0 must actually be the stored value"
        );
    }

    /// Distinct NaN payloads are distinct samples; the same payload is not.
    #[test]
    fn distinct_nan_payloads_advance_but_identical_ones_do_not() {
        let quiet = f32::NAN;
        let other = f32::from_bits(quiet.to_bits() | 1); // different payload
        let negative = f32::from_bits(0xffc0_0000); // sign bit set
        assert!(quiet.is_nan() && other.is_nan() && negative.is_nan());

        let mut mgr = SensorManager::new();
        assert!(mgr.set_reading(r(SensorKind::Magnetometer, quiet, 0.0, 0.0, 0)));
        assert!(
            mgr.set_reading(r(SensorKind::Magnetometer, other, 0.0, 0.0, 0)),
            "a different NaN payload is a different bit pattern → advance"
        );
        assert!(
            mgr.set_reading(r(SensorKind::Magnetometer, negative, 0.0, 0.0, 0)),
            "a sign-flipped NaN is a different bit pattern → advance"
        );
        assert!(
            !mgr.set_reading(r(SensorKind::Magnetometer, negative, 0.0, 0.0, 0)),
            "the very same NaN bit pattern → no advance"
        );
    }

    /// invariant: the sample's own `kind` picks the slot, whatever was written
    /// before — a gyroscope sample can never overwrite the accelerometer.
    #[test]
    fn set_reading_routes_strictly_by_kind() {
        let mut mgr = SensorManager::new();
        mgr.set_reading(r(SensorKind::Accelerometer, 1.0, 1.0, 1.0, 1));
        mgr.set_reading(r(SensorKind::Gyroscope, 2.0, 2.0, 2.0, 2));
        mgr.set_reading(r(SensorKind::Magnetometer, 3.0, 3.0, 3.0, 3));

        assert_eq!(mgr.reading(SensorKind::Accelerometer).map(|s| s.x), Some(1.0));
        assert_eq!(mgr.reading(SensorKind::Gyroscope).map(|s| s.x), Some(2.0));
        assert_eq!(mgr.reading(SensorKind::Magnetometer).map(|s| s.x), Some(3.0));
        for kind in KINDS {
            assert_eq!(
                mgr.reading(kind).map(|s| s.kind),
                Some(kind),
                "the {kind:?} slot must hold a {kind:?}-tagged sample"
            );
        }
    }

    /// The `pending_event` flag is *sticky*: it stays armed until the event
    /// pass clears it. A redundant sample arriving in between returns `false`
    /// but must not silently disarm the pending event (an implementation that
    /// wrote `self.pending_event = changed` would drop the notification).
    #[test]
    fn redundant_sample_does_not_disarm_a_pending_event() {
        let mut mgr = SensorManager::new();
        let sample = base();

        assert!(mgr.set_reading(sample));
        assert!(mgr.pending_event, "the advance armed the event");

        assert!(!mgr.set_reading(sample), "identical sample: no advance");
        assert!(
            mgr.pending_event,
            "a redundant sample must NOT clear an event the pass has not seen yet"
        );
        assert_eq!(
            mgr.get_pending_events(tick()).len(),
            1,
            "the event must still be deliverable"
        );
    }

    /// …and the converse: once cleared, a redundant sample must not re-arm it.
    #[test]
    fn redundant_sample_after_clear_does_not_re_arm() {
        let mut mgr = SensorManager::new();
        mgr.set_reading(base());
        mgr.clear_pending_event();

        assert!(!mgr.set_reading(base()), "identical sample: no advance");
        assert!(
            !mgr.pending_event,
            "a no-op sample must not raise a fresh event"
        );
        assert!(mgr.get_pending_events(tick()).is_empty());

        let mut moved = base();
        moved.x += 1.0;
        assert!(mgr.set_reading(moved), "a real change re-arms");
        assert!(mgr.pending_event);
    }

    // ───────────────────── clear_pending_event / listeners ──────────────────

    /// no_panic + invariant: clearing is idempotent, safe on a fresh manager,
    /// and touches neither the stored readings nor the listener flag.
    #[test]
    fn clear_pending_event_is_idempotent_and_narrow() {
        let mut mgr = SensorManager::new();
        mgr.clear_pending_event(); // on a cold manager: no-op, no panic
        assert!(!mgr.pending_event);

        mgr.set_has_listeners(true);
        mgr.set_reading(base());
        assert!(mgr.pending_event);

        mgr.clear_pending_event();
        mgr.clear_pending_event(); // twice — still just cleared
        assert!(!mgr.pending_event);
        assert_eq!(
            mgr.reading(SensorKind::Accelerometer),
            Some(base()),
            "clearing the flag must not discard the stored reading"
        );
        assert!(
            mgr.has_listeners(),
            "clearing the flag must not disarm polling"
        );
    }

    /// basic_true_false + edge_inputs: the listener flag round-trips, is
    /// idempotent, and is independent of the reading/event state.
    #[test]
    fn has_listeners_round_trips_and_stays_independent() {
        let mut mgr = SensorManager::new();
        assert!(!mgr.has_listeners(), "default is 'do not poll'");

        for arm in [true, true, false, false, true] {
            mgr.set_has_listeners(arm);
            assert_eq!(mgr.has_listeners(), arm, "set_has_listeners({arm}) must stick");
        }

        // Arming/disarming polling must not fabricate or destroy readings.
        assert_eq!(mgr.reading(SensorKind::Accelerometer), None);
        assert!(!mgr.pending_event);

        mgr.set_reading(base());
        mgr.set_has_listeners(false);
        assert!(
            mgr.pending_event,
            "dropping the listeners must not swallow an already-pending event"
        );
        assert_eq!(mgr.reading(SensorKind::Accelerometer), Some(base()));
    }

    /// Applying a reading must not arm polling by itself — only the relayout
    /// walk gets to decide that (the MWA-A1 arming signal).
    #[test]
    fn set_reading_never_arms_the_listener_flag() {
        let mut mgr = SensorManager::new();
        for kind in KINDS {
            mgr.set_reading(nan_sample(kind));
            assert!(
                !mgr.has_listeners(),
                "{kind:?}: only set_has_listeners may arm polling"
            );
        }
    }

    // ────────────────────── reading_bitwise_eq (private) ────────────────────

    /// The crux of the helper: it is *reflexive even for NaN*, where the
    /// derived `PartialEq` is not.
    #[test]
    fn bitwise_eq_is_reflexive_even_for_nan() {
        for kind in KINDS {
            let a = nan_sample(kind);
            let b = a; // Copy — same bit pattern
            assert!(
                reading_bitwise_eq(&a, &b),
                "{kind:?}: identical bit patterns must compare equal"
            );
            assert!(
                a != b,
                "{kind:?}: sanity — derived PartialEq disagrees, because NaN != NaN"
            );
        }
    }

    /// …and it is *not* reflexive over numeric equality: `-0.0` and `0.0`
    /// compare equal as floats but are different bit patterns.
    #[test]
    fn bitwise_eq_separates_signed_zeroes_that_partialeq_merges() {
        let pos = r(SensorKind::Accelerometer, 0.0, 0.0, 0.0, 0);
        let neg = r(SensorKind::Accelerometer, -0.0, -0.0, -0.0, 0);

        assert_eq!(pos, neg, "sanity: derived PartialEq calls the zeroes equal");
        assert!(
            !reading_bitwise_eq(&pos, &neg),
            "the bitwise comparison must tell +0.0 and -0.0 apart"
        );
    }

    /// Completeness: flipping *any single field* — including `kind`, the branch
    /// `set_reading` can never reach because the kind picks the slot — must make
    /// the helper report "not equal".
    #[test]
    fn bitwise_eq_watches_every_field() {
        let b = base();

        let mut other_kind = b;
        other_kind.kind = SensorKind::Gyroscope;
        let mut other_x = b;
        other_x.x = ulp_flip(b.x);
        let mut other_y = b;
        other_y.y = ulp_flip(b.y);
        let mut other_z = b;
        other_z.z = ulp_flip(b.z);
        let mut other_ts = b;
        other_ts.timestamp_ms = b.timestamp_ms + 1;

        for (field, mutated) in [
            ("kind", other_kind),
            ("x", other_x),
            ("y", other_y),
            ("z", other_z),
            ("timestamp_ms", other_ts),
        ] {
            assert!(
                !reading_bitwise_eq(&b, &mutated),
                "a difference in `{field}` must be detected"
            );
            assert!(
                !reading_bitwise_eq(&mutated, &b),
                "`{field}`: and symmetrically so"
            );
        }
    }

    /// no_panic + invariant: reflexive and symmetric across a hostile matrix of
    /// float values, kinds and boundary timestamps.
    #[test]
    fn bitwise_eq_is_reflexive_and_symmetric_over_hostile_inputs() {
        let mut samples = Vec::new();
        for kind in KINDS {
            for v in hostile_floats() {
                for ts in [0_u64, u64::MAX] {
                    samples.push(r(kind, v, -v, v, ts));
                }
            }
        }

        for a in &samples {
            assert!(
                reading_bitwise_eq(a, a),
                "must be reflexive for {a:?}"
            );
            for b in &samples {
                assert_eq!(
                    reading_bitwise_eq(a, b),
                    reading_bitwise_eq(b, a),
                    "must be symmetric for {a:?} vs {b:?}"
                );
                assert_eq!(
                    reading_bitwise_eq(a, b),
                    bits_identical(a, b),
                    "must agree with a field-by-field bit comparison for {a:?} vs {b:?}"
                );
            }
        }
    }

    // ───────────────────────── EventProvider surface ────────────────────────

    /// The event is emitted only while armed, exactly once, at the root — and
    /// `clear_pending_event` is what stops it repeating every pass.
    #[test]
    fn sensor_changed_event_is_emitted_only_while_pending() {
        let mut mgr = SensorManager::new();
        assert!(
            mgr.get_pending_events(tick()).is_empty(),
            "a cold manager emits nothing"
        );

        assert!(mgr.set_reading(base()));
        let events = mgr.get_pending_events(tick());
        assert_eq!(events.len(), 1, "one advance → exactly one event");
        assert_eq!(events[0].event_type, EventType::SensorChanged);
        assert_eq!(events[0].target, DomNodeId::ROOT, "window-level: target is root");

        assert_eq!(
            mgr.get_pending_events(tick()).len(),
            1,
            "collecting is non-destructive; only clear_pending_event disarms"
        );

        mgr.clear_pending_event();
        assert!(
            mgr.get_pending_events(tick()).is_empty(),
            "after the event pass drained it, nothing more is pending"
        );
    }

    /// Several kinds advancing in one pass still collapse into a single
    /// window-level event (the callback reads the values it wants by kind).
    #[test]
    fn many_advances_collapse_into_one_event() {
        let mut mgr = SensorManager::new();
        for kind in KINDS {
            for (ts, x) in [(0_u64, 1.0_f32), (1, 2.0), (2, 3.0), (3, 4.0)] {
                mgr.set_reading(r(kind, x, 0.0, 0.0, ts));
            }
        }
        assert_eq!(
            mgr.get_pending_events(tick()).len(),
            1,
            "12 samples across 3 kinds still yield one SensorChanged"
        );
    }
}
