//! Keyring manager — cross-platform state for the system-keyring surface
//! (`SUPER_PLAN_2` §4 P4.2).
//!
//! Request-driven, mirroring [`crate::managers::biometric`]:
//!
//! - A **callback** calls `CallbackInfo::keyring_store/get/delete(...)`,
//!   which parks a [`KeyringRequest`] in the request channel.
//! - The dll **layout pass** drains it and dispatches to the platform
//!   backend (`dll::desktop::extra::keyring`) — Keychain / `KeyStore` /
//!   libsecret / `CredentialLocker`. A biometry-bound `Get` shows the OS
//!   prompt; the outcome is parked in the result channel.
//! - The layout pass folds the latest result into the manager via
//!   [`KeyringManager::set_last_result`]; callbacks read it with
//!   `CallbackInfo::get_keyring_result()`.
//!
//! No platform deps (`SUPER_PLAN_2` §0.5); the channels are the same
//! poison-recovering `Mutex<Vec<_>>` pattern as the geolocation /
//! biometric managers.

use alloc::vec::Vec;

// `KeyringRequest` / `KeyringResult` live in `azul-core` so they cross the
// FFI without a cyclic dep on `azul-layout`. Re-exported for the existing
// `azul_layout::managers::keyring::*` import paths.
pub use azul_core::keyring::{KeyringRequest, KeyringResult};

use azul_core::dom::DomNodeId;
use azul_core::events::{
    EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
};
use azul_core::task::Instant;

/// Cross-platform keyring state. One per `App` — the OS keyring is a
/// per-process (per-app-identity) store, not per-window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyringManager {
    /// Outcome of the most recent keyring op, or `None` until the first
    /// completes. Read by callbacks via `CallbackInfo::get_keyring_result()`.
    pub last_result: Option<KeyringResult>,
    /// Ops dispatched to the native backend whose outcome has not been
    /// folded back yet (MWA-A1b arming signal for the capability pump).
    pub in_flight: u32,
    /// `true` when an op outcome was folded since the last event pass (set
    /// on EVERY completion — a repeated identical outcome still answers a
    /// fresh op). Read by the `EventProvider` impl
    /// (`EventType::KeyringResult`), cleared by
    /// [`clear_pending_event`](Self::clear_pending_event).
    pub pending_event: bool,
}

impl KeyringManager {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Most recent keyring outcome, or `None` until the first op resolves.
    #[must_use] pub const fn last_result(&self) -> Option<&KeyringResult> {
        self.last_result.as_ref()
    }

    /// Apply the outcome the backend delivered. Returns `true` if it
    /// differs from the previous one (so the window can be marked dirty to
    /// re-render the revealed / stored state).
    pub fn set_last_result(&mut self, result: KeyringResult) -> bool {
        let changed = self.last_result.as_ref() != Some(&result);
        self.last_result = Some(result);
        // MWA-A1b: every completion fires an event and retires one
        // in-flight op.
        self.pending_event = true;
        self.in_flight = self.in_flight.saturating_sub(1);
        changed
    }

    /// The pump dispatched `n` ops to the native backend; keep the timer
    /// armed until their outcomes fold back (MWA-A1b).
    pub const fn mark_requests_dispatched(&mut self, n: u32) {
        self.in_flight = self.in_flight.saturating_add(n);
    }

    /// Clear the pending-event flag. The dll calls this after the event
    /// pass has collected the `KeyringResult` event.
    pub const fn clear_pending_event(&mut self) {
        self.pending_event = false;
    }

    /// `true` while a dispatched op's outcome is still outstanding
    /// (MWA-A1b arming signal).
    #[must_use] pub const fn has_pending_async(&self) -> bool {
        self.in_flight > 0
    }
}

impl EventProvider for KeyringManager {
    /// Yield a window-level `KeyringResult` event when an op outcome was
    /// folded since the last pass (target = root; read the outcome via
    /// `CallbackInfo::get_keyring_result` inside the callback).
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        if self.pending_event {
            alloc::vec![SyntheticEvent::new(
                EventType::KeyringResult,
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

// ────────── Request channel (callback → platform backend) ─────────────

static PENDING_REQUESTS: std::sync::Mutex<Vec<KeyringRequest>> =
    std::sync::Mutex::new(Vec::new());

/// Queue a keyring op from a callback. Drained by the dll layout pass and
/// dispatched to the native keyring. Thread-safe; poison-recovering.
pub fn push_keyring_request(request: KeyringRequest) {
    let mut q = PENDING_REQUESTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(request);
}

/// Drain every queued keyring op, in arrival order. Called once per
/// layout pass; the dll dispatches each to the platform backend.
pub fn drain_keyring_requests() -> Vec<KeyringRequest> {
    let mut q = PENDING_REQUESTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// MWA-C-biometric/keyring: see `biometric::has_queued_requests` — pump
/// arming must count parked-but-undispatched requests.
pub fn has_queued_requests() -> bool {
    PENDING_REQUESTS
        .lock()
        .map_or_else(|e| !e.into_inner().is_empty(), |q| !q.is_empty())
}

// ────────── Result channel (platform backend → manager) ───────────────

static PENDING_RESULTS: std::sync::Mutex<Vec<KeyringResult>> =
    std::sync::Mutex::new(Vec::new());

/// Park a keyring result delivered by a platform backend (in the dll).
/// Thread-safe; poison-recovering (a biometry-bound `Get` resolves from
/// the OS prompt's reply on an arbitrary thread).
pub fn push_keyring_result(result: KeyringResult) {
    let mut q = PENDING_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(result);
}

/// Drain every parked keyring result, in arrival order. Called once per
/// layout pass; the caller applies them via [`KeyringManager::set_last_result`].
pub fn drain_keyring_results() -> Vec<KeyringResult> {
    let mut q = PENDING_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_css::AzString;

    #[test]
    fn manager_defaults_to_no_result() {
        let mgr = KeyringManager::new();
        assert_eq!(mgr.last_result(), None);
    }

    #[test]
    fn set_last_result_returns_change_flag() {
        let mut mgr = KeyringManager::new();
        assert!(mgr.set_last_result(KeyringResult::Stored));
        assert_eq!(mgr.last_result(), Some(&KeyringResult::Stored));
        // Re-applying the same outcome is not a change.
        assert!(!mgr.set_last_result(KeyringResult::Stored));
        // A new outcome is a change.
        assert!(mgr.set_last_result(KeyringResult::Deleted));
    }

    #[test]
    fn result_helpers() {
        let secret = KeyringResult::Retrieved(AzString::from_const_str("hunter2"));
        assert_eq!(secret.secret().map(AzString::as_str), Some("hunter2"));
        assert!(secret.is_ok());
        assert!(KeyringResult::Stored.is_ok());
        assert!(KeyringResult::Deleted.is_ok());
        for r in [
            KeyringResult::NotFound,
            KeyringResult::Denied,
            KeyringResult::Unavailable,
            KeyringResult::Error,
        ] {
            assert!(!r.is_ok(), "{r:?} must not be ok");
            assert_eq!(r.secret(), None);
        }
    }

    #[test]
    fn requests_round_trip_through_channel() {
        // Process-global; serialize against every other channel test, then
        // clear residue.
        let _guard = super::autotest_generated::lock_channels();
        drop(drain_keyring_requests());

        push_keyring_request(KeyringRequest::Store {
            key: AzString::from_const_str("token"),
            secret: AzString::from_const_str("abc"),
            require_biometry: true,
        });
        push_keyring_request(KeyringRequest::Get {
            key: AzString::from_const_str("token"),
        });
        let drained = drain_keyring_requests();
        assert_eq!(drained.len(), 2, "both queued requests drain in order");
        assert!(matches!(drained[0], KeyringRequest::Store { .. }));
        assert!(matches!(drained[1], KeyringRequest::Get { .. }));
        assert!(drain_keyring_requests().is_empty());
    }

    #[test]
    fn results_round_trip_through_manager() {
        // Process-global; serialize against every other channel test, then
        // clear residue.
        let _guard = super::autotest_generated::lock_channels();
        drop(drain_keyring_results());

        push_keyring_result(KeyringResult::NotFound);
        push_keyring_result(KeyringResult::Retrieved(AzString::from_const_str("s"))); // last wins
        let drained = drain_keyring_results();
        assert_eq!(drained.len(), 2);

        let mut mgr = KeyringManager::new();
        for r in drained {
            mgr.set_last_result(r);
        }
        assert_eq!(
            mgr.last_result().and_then(|r| r.secret()).map(AzString::as_str),
            Some("s"),
            "the last applied result wins"
        );
        assert!(drain_keyring_results().is_empty());
    }
}

#[cfg(test)]
mod pump_provider_tests {
    use super::*;
    use azul_core::task::{Instant, SystemTick};

    fn ts() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    #[test]
    fn in_flight_and_events_track_op_lifecycle() {
        let mut mgr = KeyringManager::new();
        assert!(!mgr.has_pending_async());
        mgr.mark_requests_dispatched(1);
        assert!(mgr.has_pending_async());
        assert!(mgr.get_pending_events(ts()).is_empty(), "no outcome yet");

        mgr.set_last_result(KeyringResult::Stored);
        assert!(!mgr.has_pending_async());
        let events = mgr.get_pending_events(ts());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::KeyringResult);
        mgr.clear_pending_event();

        // repeated identical outcome still fires — it answers a fresh op
        mgr.mark_requests_dispatched(1);
        mgr.set_last_result(KeyringResult::Stored);
        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::sync::{Mutex, PoisonError};

    use azul_core::task::SystemTick;
    use azul_css::AzString;

    use super::*;

    /// The request/result channels are process-global statics and `cargo
    /// test` runs tests in parallel threads, so EVERY test that pushes to or
    /// drains a channel must hold this lock — otherwise one test's pushes
    /// land inside another's drain window. Shared with the hand-written
    /// `tests` module above.
    static CHANNEL_LOCK: Mutex<()> = Mutex::new(());

    pub(super) fn lock_channels() -> std::sync::MutexGuard<'static, ()> {
        CHANNEL_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn ts() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    /// Interior NUL, C0/C1 controls and CRLF — breaks naive C-string /
    /// byte-length handling in the FFI layer the channels feed.
    const NASTY_BYTES: &str = "pw\0with\u{1}nul\u{7f}\r\n\t";
    /// Combining marks, an RTL override, the replacement char and the
    /// maximum scalar value.
    const NASTY_UNICODE: &str = "🔑 ключ 鍵 مفتاح e\u{301}\u{202e}terces\u{202c}\u{fffd}\u{10ffff}";

    /// Every `KeyringResult` variant, in declaration order.
    fn all_results() -> [KeyringResult; 7] {
        [
            KeyringResult::Stored,
            KeyringResult::Retrieved(AzString::from("s3cr3t")),
            KeyringResult::Deleted,
            KeyringResult::NotFound,
            KeyringResult::Denied,
            KeyringResult::Unavailable,
            KeyringResult::Error,
        ]
    }

    // ── constructor: KeyringManager::new ───────────────────────────────

    #[test]
    fn new_equals_default_and_holds_the_zero_state_invariants() {
        let a = KeyringManager::new();
        let b = KeyringManager::default();
        assert_eq!(a, b, "new() must be exactly default()");

        // Every documented field is at its zero value, and the derived
        // accessors agree with the raw fields.
        assert_eq!(a.last_result, None);
        assert_eq!(a.last_result(), None);
        assert_eq!(a.in_flight, 0);
        assert!(!a.has_pending_async());
        assert!(!a.pending_event);
        assert!(
            a.get_pending_events(ts()).is_empty(),
            "a fresh manager has nothing to report"
        );
    }

    #[test]
    fn new_carries_no_hidden_global_state_between_instances() {
        // The manager is per-App, but the channels are process-global — a
        // fresh manager must not pick up anything parked in them.
        let _guard = lock_channels();
        drop(drain_keyring_results());
        push_keyring_result(KeyringResult::Retrieved(AzString::from("leaked")));

        let mgr = KeyringManager::new();
        assert_eq!(
            mgr.last_result(),
            None,
            "construction must not drain the result channel"
        );

        // The parked result is still there — new() did not consume it.
        assert_eq!(drain_keyring_results().len(), 1);
    }

    // ── getter: KeyringManager::last_result ────────────────────────────

    #[test]
    fn last_result_returns_the_exact_outcome_that_was_folded() {
        let mut mgr = KeyringManager::new();
        assert_eq!(mgr.last_result(), None, "None until the first op resolves");

        for r in all_results() {
            mgr.set_last_result(r.clone());
            assert_eq!(
                mgr.last_result(),
                Some(&r),
                "last_result() must hand back exactly what was folded"
            );
        }
    }

    #[test]
    fn last_result_preserves_nul_control_and_max_scalar_payloads() {
        // `AzString::as_str` is `from_utf8_unchecked`, so a byte-level
        // mangling on the way through the manager would be UB rather than a
        // clean error — assert byte equality, not just string equality.
        for payload in [NASTY_BYTES, NASTY_UNICODE, ""] {
            let mut mgr = KeyringManager::new();
            mgr.set_last_result(KeyringResult::Retrieved(AzString::from(payload)));

            let s = mgr
                .last_result()
                .and_then(KeyringResult::secret)
                .expect("Retrieved must expose its secret");
            assert_eq!(s.as_str(), payload);
            assert_eq!(s.as_str().as_bytes(), payload.as_bytes());
            assert_eq!(s.as_str().len(), payload.len(), "nothing truncated at the NUL");
        }
    }

    #[test]
    fn last_result_handles_a_megabyte_secret() {
        let big = "k".repeat(1 << 20);
        let mut mgr = KeyringManager::new();
        assert!(mgr.set_last_result(KeyringResult::Retrieved(AzString::from(big.clone()))));

        let s = mgr
            .last_result()
            .and_then(KeyringResult::secret)
            .expect("Retrieved must expose its secret");
        assert_eq!(s.as_str().len(), 1 << 20);
        assert_eq!(s.as_str(), big.as_str());

        // Re-folding the identical megabyte payload is not a change (the
        // comparison must be by value, not by pointer).
        assert!(!mgr.set_last_result(KeyringResult::Retrieved(AzString::from(big))));
    }

    #[test]
    fn last_result_survives_the_flag_and_counter_mutators() {
        let mut mgr = KeyringManager::new();
        mgr.set_last_result(KeyringResult::Retrieved(AzString::from("keep-me")));

        mgr.clear_pending_event();
        mgr.mark_requests_dispatched(7);
        mgr.mark_requests_dispatched(0);

        assert_eq!(
            mgr.last_result().and_then(KeyringResult::secret).map(AzString::as_str),
            Some("keep-me"),
            "neither the event flag nor the in-flight counter may clobber the outcome"
        );
    }

    // ── other: KeyringManager::set_last_result ─────────────────────────

    #[test]
    fn set_last_result_change_flag_is_true_only_on_a_distinct_outcome() {
        let mut mgr = KeyringManager::new();
        for r in all_results() {
            assert!(
                mgr.set_last_result(r.clone()),
                "{r:?} differs from the previous outcome → changed"
            );
            assert!(
                !mgr.set_last_result(r.clone()),
                "re-folding the identical {r:?} is not a change"
            );
        }
    }

    #[test]
    fn set_last_result_compares_retrieved_payloads_by_value() {
        let mut mgr = KeyringManager::new();
        assert!(mgr.set_last_result(KeyringResult::Retrieved(AzString::from("a"))));
        assert!(
            !mgr.set_last_result(KeyringResult::Retrieved(AzString::from("a"))),
            "same payload, freshly allocated → not a change"
        );
        assert!(
            mgr.set_last_result(KeyringResult::Retrieved(AzString::from("b"))),
            "a different secret under the same variant IS a change"
        );
        // An empty secret is a *present* secret, distinct from NotFound.
        assert!(mgr.set_last_result(KeyringResult::Retrieved(AzString::from(""))));
        assert!(!mgr.set_last_result(KeyringResult::Retrieved(AzString::from(""))));
        assert!(
            mgr.set_last_result(KeyringResult::NotFound),
            "Retrieved(\"\") and NotFound must not collapse into one another"
        );
    }

    #[test]
    fn set_last_result_fires_the_event_even_when_the_outcome_is_unchanged() {
        // MWA-A1b: a repeated identical outcome still answers a *fresh* op,
        // so the event must fire regardless of the change flag.
        let mut mgr = KeyringManager::new();
        mgr.set_last_result(KeyringResult::Denied);
        mgr.clear_pending_event();

        assert!(!mgr.set_last_result(KeyringResult::Denied), "not a change…");
        assert!(mgr.pending_event, "…but still a completion → event pending");
        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
    }

    #[test]
    fn set_last_result_without_a_dispatch_does_not_underflow_in_flight() {
        // The adversarial case: a backend delivers an outcome nobody armed.
        // A wrapping `-1` here would leave in_flight = u32::MAX and pin the
        // capability pump on forever.
        let mut mgr = KeyringManager::new();
        assert_eq!(mgr.in_flight, 0);
        for _ in 0..1_000 {
            mgr.set_last_result(KeyringResult::Error);
        }
        assert_eq!(mgr.in_flight, 0, "saturating_sub must clamp at 0, not wrap");
        assert!(!mgr.has_pending_async());
    }

    #[test]
    fn set_last_result_retires_exactly_one_in_flight_op_per_fold() {
        let mut mgr = KeyringManager::new();
        mgr.mark_requests_dispatched(3);
        for expected in [2_u32, 1, 0] {
            mgr.set_last_result(KeyringResult::Stored);
            assert_eq!(mgr.in_flight, expected, "one fold retires exactly one op");
        }
        assert!(!mgr.has_pending_async(), "all dispatched ops resolved");

        // Extra outcomes past the dispatched count clamp at zero.
        mgr.set_last_result(KeyringResult::Deleted);
        assert_eq!(mgr.in_flight, 0);
    }

    #[test]
    fn set_last_result_retires_one_op_even_at_the_u32_max_ceiling() {
        let mut mgr = KeyringManager::new();
        mgr.in_flight = u32::MAX;
        mgr.set_last_result(KeyringResult::Stored);
        assert_eq!(
            mgr.in_flight,
            u32::MAX - 1,
            "the counter is only saturated at the bottom, not stuck at the top"
        );
        assert!(mgr.has_pending_async());
    }

    // ── numeric: KeyringManager::mark_requests_dispatched ──────────────

    #[test]
    fn mark_requests_dispatched_zero_is_a_no_op() {
        let mut mgr = KeyringManager::new();
        mgr.mark_requests_dispatched(0);
        assert_eq!(mgr.in_flight, 0);
        assert!(
            !mgr.has_pending_async(),
            "dispatching nothing must not arm the pump"
        );

        mgr.mark_requests_dispatched(5);
        mgr.mark_requests_dispatched(0);
        assert_eq!(mgr.in_flight, 5, "a zero dispatch must not disturb the count");
    }

    #[test]
    fn mark_requests_dispatched_accumulates_across_calls() {
        let mut mgr = KeyringManager::new();
        mgr.mark_requests_dispatched(1);
        mgr.mark_requests_dispatched(2);
        mgr.mark_requests_dispatched(3);
        assert_eq!(mgr.in_flight, 6);

        let mut many = KeyringManager::new();
        for _ in 0..10_000 {
            many.mark_requests_dispatched(1);
        }
        assert_eq!(many.in_flight, 10_000);
        assert!(many.has_pending_async());
    }

    #[test]
    fn mark_requests_dispatched_saturates_at_u32_max_instead_of_overflowing() {
        // A debug build would panic on a plain `+` here; the contract is
        // saturation.
        let mut mgr = KeyringManager::new();
        mgr.mark_requests_dispatched(u32::MAX);
        assert_eq!(mgr.in_flight, u32::MAX);

        mgr.mark_requests_dispatched(1);
        assert_eq!(mgr.in_flight, u32::MAX, "saturating_add clamps at the ceiling");
        mgr.mark_requests_dispatched(u32::MAX);
        assert_eq!(mgr.in_flight, u32::MAX);
        assert!(mgr.has_pending_async());

        // And from a non-zero base: MAX-1 plus 2 still clamps.
        let mut near = KeyringManager::new();
        near.in_flight = u32::MAX - 1;
        near.mark_requests_dispatched(2);
        assert_eq!(near.in_flight, u32::MAX);
    }

    #[test]
    fn mark_requests_dispatched_touches_neither_the_event_flag_nor_the_outcome() {
        let mut mgr = KeyringManager::new();
        mgr.mark_requests_dispatched(u32::MAX);
        assert!(
            !mgr.pending_event,
            "dispatching is not a completion — no event may fire"
        );
        assert_eq!(mgr.last_result(), None);
        assert!(mgr.get_pending_events(ts()).is_empty());
    }

    // ── other: KeyringManager::clear_pending_event ─────────────────────

    #[test]
    fn clear_pending_event_is_idempotent_and_only_touches_the_flag() {
        let mut mgr = KeyringManager::new();
        // Clearing an already-clear flag is a no-op, not an underflow.
        mgr.clear_pending_event();
        assert!(!mgr.pending_event);

        mgr.mark_requests_dispatched(2);
        mgr.set_last_result(KeyringResult::Stored);
        assert!(mgr.pending_event);

        mgr.clear_pending_event();
        mgr.clear_pending_event();
        assert!(!mgr.pending_event);
        assert!(
            mgr.get_pending_events(ts()).is_empty(),
            "a cleared flag yields no events"
        );

        // The outcome and the in-flight counter are untouched by the clear.
        assert_eq!(mgr.last_result(), Some(&KeyringResult::Stored));
        assert_eq!(mgr.in_flight, 1);
        assert!(
            mgr.has_pending_async(),
            "clearing the event flag must not disarm the still-outstanding op"
        );
    }

    // ── predicate: KeyringManager::has_pending_async ───────────────────

    #[test]
    fn has_pending_async_is_exactly_in_flight_greater_than_zero() {
        let mut mgr = KeyringManager::new();
        for (in_flight, expected) in [(0_u32, false), (1, true), (2, true), (u32::MAX, true)] {
            mgr.in_flight = in_flight;
            assert_eq!(
                mgr.has_pending_async(),
                expected,
                "in_flight = {in_flight} → has_pending_async() = {expected}"
            );
        }
    }

    #[test]
    fn has_pending_async_goes_false_only_when_the_last_op_folds_back() {
        let mut mgr = KeyringManager::new();
        assert!(!mgr.has_pending_async());
        mgr.mark_requests_dispatched(2);
        assert!(mgr.has_pending_async());
        mgr.set_last_result(KeyringResult::Stored);
        assert!(mgr.has_pending_async(), "one op is still outstanding");
        mgr.set_last_result(KeyringResult::Stored);
        assert!(!mgr.has_pending_async(), "both ops resolved → pump disarms");
    }

    // ── EventProvider ──────────────────────────────────────────────────

    #[test]
    fn pending_event_is_a_root_targeted_user_sourced_keyring_result() {
        let mut mgr = KeyringManager::new();
        mgr.set_last_result(KeyringResult::Retrieved(AzString::from("s")));

        let stamp = Instant::Tick(SystemTick::new(u64::MAX));
        let events = mgr.get_pending_events(stamp.clone());
        assert_eq!(events.len(), 1, "exactly one event per pass");

        let e = &events[0];
        assert_eq!(e.event_type, EventType::KeyringResult);
        assert_eq!(e.source, CoreEventSource::User);
        assert_eq!(e.target, DomNodeId::ROOT, "keyring events are window-level");
        assert_eq!(e.current_target, DomNodeId::ROOT);
        assert_eq!(e.timestamp, stamp, "the caller's timestamp is echoed verbatim");
        assert!(
            matches!(e.data, EventData::None),
            "the outcome is read via CallbackInfo, not carried in the event"
        );
    }

    #[test]
    fn get_pending_events_is_non_destructive_until_cleared() {
        // The dll clears the flag explicitly; polling must not consume it,
        // or an event pass that reads twice would drop the event.
        let mut mgr = KeyringManager::new();
        mgr.set_last_result(KeyringResult::Deleted);

        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
        assert_eq!(mgr.get_pending_events(ts()).len(), 1, "still pending");
        assert!(mgr.pending_event);

        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts()).is_empty());
    }

    // ── channels: push / drain / has_queued_requests ───────────────────

    #[test]
    fn request_channel_is_fifo_across_every_variant() {
        let _guard = lock_channels();
        drop(drain_keyring_requests());

        push_keyring_request(KeyringRequest::Store {
            key: AzString::from("k1"),
            secret: AzString::from("s1"),
            require_biometry: true,
        });
        push_keyring_request(KeyringRequest::Get {
            key: AzString::from("k2"),
        });
        push_keyring_request(KeyringRequest::Delete {
            key: AzString::from("k3"),
        });

        let drained = drain_keyring_requests();
        assert_eq!(drained.len(), 3);
        assert_eq!(
            drained[0],
            KeyringRequest::Store {
                key: AzString::from("k1"),
                secret: AzString::from("s1"),
                require_biometry: true,
            },
            "arrival order and payload preserved exactly"
        );
        assert_eq!(
            drained[1],
            KeyringRequest::Get {
                key: AzString::from("k2")
            }
        );
        assert_eq!(
            drained[2],
            KeyringRequest::Delete {
                key: AzString::from("k3")
            }
        );

        // The queue was taken, not copied.
        assert!(drain_keyring_requests().is_empty());
    }

    #[test]
    fn draining_an_empty_channel_is_an_empty_vec_not_a_panic() {
        let _guard = lock_channels();
        drop(drain_keyring_requests());
        drop(drain_keyring_results());

        for _ in 0..3 {
            assert!(drain_keyring_requests().is_empty());
            assert!(drain_keyring_results().is_empty());
        }
        assert!(!has_queued_requests());
    }

    #[test]
    fn has_queued_requests_tracks_only_the_request_channel() {
        let _guard = lock_channels();
        drop(drain_keyring_requests());
        drop(drain_keyring_results());
        assert!(!has_queued_requests(), "empty queue → nothing to pump");

        // A parked *result* is not a queued *request* — the two statics must
        // not be conflated, or the pump would arm on its own output.
        push_keyring_result(KeyringResult::Stored);
        assert!(!has_queued_requests());

        push_keyring_request(KeyringRequest::Get {
            key: AzString::from("k"),
        });
        assert!(has_queued_requests(), "a parked-but-undispatched request arms the pump");
        // Reading the predicate does not consume the request.
        assert!(has_queued_requests());

        assert_eq!(drain_keyring_requests().len(), 1);
        assert!(!has_queued_requests(), "draining disarms it");

        drop(drain_keyring_results());
    }

    #[test]
    fn request_channel_preserves_nul_unicode_and_megabyte_payloads() {
        let _guard = lock_channels();
        drop(drain_keyring_requests());

        let big = "s".repeat(1 << 20);
        push_keyring_request(KeyringRequest::Store {
            key: AzString::from(NASTY_BYTES),
            secret: AzString::from(NASTY_UNICODE),
            require_biometry: false,
        });
        push_keyring_request(KeyringRequest::Get {
            key: AzString::from(big.clone()),
        });

        let drained = drain_keyring_requests();
        assert_eq!(drained.len(), 2);

        let KeyringRequest::Store {
            key,
            secret,
            require_biometry,
        } = &drained[0]
        else {
            panic!("first request must still be the Store: {:?}", drained[0]);
        };
        // Byte-exact both ways: no truncation at the interior NUL, no
        // re-encoding of the astral / combining-mark scalars.
        assert_eq!(key.as_str().as_bytes(), NASTY_BYTES.as_bytes());
        assert_eq!(secret.as_str().as_bytes(), NASTY_UNICODE.as_bytes());
        assert!(secret.as_str().ends_with('\u{10ffff}'));
        assert!(!*require_biometry);

        let KeyringRequest::Get { key } = &drained[1] else {
            panic!("second request must still be the Get: {:?}", drained[1]);
        };
        assert_eq!(key.as_str().len(), 1 << 20);
        assert_eq!(key.as_str(), big.as_str());
    }

    #[test]
    fn request_channel_keeps_arrival_order_under_many_pushes() {
        let _guard = lock_channels();
        drop(drain_keyring_requests());

        const N: usize = 1_000;
        for i in 0..N {
            push_keyring_request(KeyringRequest::Get {
                key: AzString::from(format!("key-{i}")),
            });
        }

        let drained = drain_keyring_requests();
        assert_eq!(drained.len(), N);
        for (i, req) in drained.iter().enumerate() {
            let KeyringRequest::Get { key } = req else {
                panic!("expected a Get at {i}, got {req:?}");
            };
            assert_eq!(key.as_str(), format!("key-{i}"), "FIFO order at {i}");
        }
        assert!(drain_keyring_requests().is_empty());
    }

    #[test]
    fn concurrent_pushes_from_many_threads_lose_no_requests() {
        // The channel is documented thread-safe: a biometry-bound Get can
        // resolve on an arbitrary thread. Lost or duplicated entries here
        // would mean a dropped keyring op.
        let _guard = lock_channels();
        drop(drain_keyring_requests());

        const THREADS: usize = 8;
        const PER_THREAD: usize = 50;

        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                std::thread::spawn(move || {
                    for i in 0..PER_THREAD {
                        push_keyring_request(KeyringRequest::Delete {
                            key: AzString::from(format!("t{t}-{i}")),
                        });
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("pushing thread must not panic");
        }

        let drained = drain_keyring_requests();
        assert_eq!(drained.len(), THREADS * PER_THREAD, "no request lost or duplicated");

        // Cross-thread interleaving is unspecified, but the multiset of keys
        // must be exactly what was pushed.
        let mut keys: Vec<String> = drained
            .iter()
            .map(|r| match r {
                KeyringRequest::Delete { key } => key.as_str().to_string(),
                other => panic!("unexpected request in the channel: {other:?}"),
            })
            .collect();
        keys.sort();
        let mut expected: Vec<String> = (0..THREADS)
            .flat_map(|t| (0..PER_THREAD).map(move |i| format!("t{t}-{i}")))
            .collect();
        expected.sort();
        assert_eq!(keys, expected);
    }

    #[test]
    fn result_channel_folds_into_the_manager_with_last_write_winning() {
        let _guard = lock_channels();
        drop(drain_keyring_results());

        let mut mgr = KeyringManager::new();
        mgr.mark_requests_dispatched(3);

        push_keyring_result(KeyringResult::Denied);
        push_keyring_result(KeyringResult::NotFound);
        push_keyring_result(KeyringResult::Retrieved(AzString::from(NASTY_UNICODE)));

        let drained = drain_keyring_results();
        assert_eq!(drained.len(), 3, "results drain in arrival order");
        assert_eq!(drained[0], KeyringResult::Denied);
        assert_eq!(drained[1], KeyringResult::NotFound);

        for r in drained {
            mgr.set_last_result(r);
        }
        assert_eq!(
            mgr.last_result().and_then(KeyringResult::secret).map(AzString::as_str),
            Some(NASTY_UNICODE),
            "the last applied result wins"
        );
        assert_eq!(mgr.in_flight, 0, "all three dispatched ops retired");
        assert!(!mgr.has_pending_async());
        assert!(mgr.pending_event);
        assert!(drain_keyring_results().is_empty());
    }
}
