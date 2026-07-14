//! Biometric manager — cross-platform state for the biometric-auth
//! surface (`SUPER_PLAN_2` §1 feature 4 + research/02).
//!
//! **Request-driven**, unlike the continuous `GeolocationManager`. The
//! three callers are:
//!
//! - A **callback** invokes `App::request_biometric_auth(prompt)` (e.g.
//!   the `AzulVault` unlock button). The OS draws its own modal sheet; the
//!   app cannot skin it.
//!
//! - The **platform backend** (`dll/src/desktop/extra/biometric/<plat>.rs`)
//!   shows the prompt (iOS / macOS `LAContext.evaluatePolicy`, Android
//!   `BiometricPrompt.authenticate`, Windows `UserConsentVerifier`, Linux
//!   polkit / PAM) and, when the user responds, parks the outcome in the
//!   async result channel [`push_biometric_result`]. It also writes the
//!   sync availability probe via [`BiometricManager::set_availability`].
//!
//! - The dll **layout pass** drains the channel once per frame via
//!   [`drain_biometric_results`] and applies the latest through
//!   [`BiometricManager::set_last_result`]; callbacks then read it with
//!   `CallbackInfo::get_biometric_result()` and the device capability via
//!   the sync availability accessor.
//!
//! No platform deps (`SUPER_PLAN_2` §0.5); the async-result channel is
//! copied verbatim from `geolocation.rs`.

use alloc::vec::Vec;

use azul_core::dom::DomNodeId;
use azul_core::events::{
    EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
};
use azul_core::task::Instant;

// `BiometricKind` / `BiometricResult` / `BiometricPrompt` live in
// `azul-core` so the request config can cross the FFI without a cyclic
// dep on `azul-layout`. Re-exported here for the existing
// `azul_layout::managers::biometric::*` import paths.
pub use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};

/// Cross-platform biometric state. One per `App` — the OS exposes a
/// single per-process authentication surface, not per-window.
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub struct BiometricManager {
    /// Outcome of the most recent `request_biometric_auth`, or `None`
    /// until the first request completes. Read by callbacks via
    /// `CallbackInfo::get_biometric_result()`.
    pub last_result: Option<BiometricResult>,
    /// Cached sync availability probe — what the device *can* do
    /// (`Face` / `Fingerprint` / `Iris` / `NotAvailable`). The backend
    /// refreshes it on startup and after enrollment changes; callbacks
    /// read it to decide whether to even offer biometric unlock.
    pub availability: BiometricKind,
    /// Prompts dispatched to the native backend whose outcome has not been
    /// folded back yet (MWA-A1b). While non-zero the capability pump keeps
    /// its timer armed so the reply reaches callbacks in an idle app.
    pub in_flight: u32,
    /// `true` when a prompt outcome was folded since the last event pass
    /// (set on EVERY completion, even a repeated identical outcome — the
    /// user re-authenticated and the callback must hear about it). Read by
    /// the `EventProvider` impl (`EventType::BiometricResult`), cleared by
    /// [`clear_pending_event`](Self::clear_pending_event).
    pub pending_event: bool,
}

impl Default for BiometricManager {
    fn default() -> Self {
        Self {
            last_result: None,
            availability: BiometricKind::NotAvailable,
            in_flight: 0,
            pending_event: false,
        }
    }
}

impl BiometricManager {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Most recent auth outcome, or `None` until the first request
    /// resolves.
    #[must_use] pub const fn last_result(&self) -> Option<BiometricResult> {
        self.last_result
    }

    /// Device capability probe (sync). `NotAvailable` until the backend
    /// reports otherwise.
    #[must_use] pub const fn availability(&self) -> BiometricKind {
        self.availability
    }

    /// `true` if the device has a usable biometric sensor.
    #[must_use] pub const fn is_available(&self) -> bool {
        self.availability.is_available()
    }

    /// Platform backend records the device's biometric capability.
    /// Returns `true` if it changed, so the caller can relayout to
    /// reflect a newly-enrolled (or newly-removed) sensor.
    pub fn set_availability(&mut self, kind: BiometricKind) -> bool {
        let changed = self.availability != kind;
        self.availability = kind;
        changed
    }

    /// Apply the outcome the backend delivered for the user's request.
    /// Returns `true` if it differs from the previous outcome (so the
    /// window can be marked dirty to re-render the unlocked / denied
    /// state).
    pub fn set_last_result(&mut self, result: BiometricResult) -> bool {
        let changed = self.last_result != Some(result);
        self.last_result = Some(result);
        // MWA-A1b: every completion fires an event (even an identical
        // repeat outcome — it answers a fresh request) and retires one
        // in-flight prompt.
        self.pending_event = true;
        self.in_flight = self.in_flight.saturating_sub(1);
        changed
    }

    /// The pump dispatched `n` prompts to the native backend; keep the
    /// timer armed until their outcomes fold back (MWA-A1b).
    pub const fn mark_requests_dispatched(&mut self, n: u32) {
        self.in_flight = self.in_flight.saturating_add(n);
    }

    /// Clear the pending-event flag. The dll calls this after the event
    /// pass has collected the `BiometricResult` event.
    pub const fn clear_pending_event(&mut self) {
        self.pending_event = false;
    }

    /// `true` while a dispatched prompt's outcome is still outstanding
    /// (MWA-A1b arming signal).
    #[must_use] pub const fn has_pending_async(&self) -> bool {
        self.in_flight > 0
    }

    /// `true` if the last attempt unlocked successfully (biometric match
    /// or OS passcode fallback). Convenience for the vault gate.
    #[must_use] pub const fn last_was_success(&self) -> bool {
        matches!(self.last_result, Some(r) if r.is_success())
    }
}

impl EventProvider for BiometricManager {
    /// Yield a window-level `BiometricResult` event when a prompt outcome
    /// was folded since the last pass (target = root; read the outcome via
    /// `CallbackInfo::get_biometric_result` inside the callback).
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        if self.pending_event {
            alloc::vec![SyntheticEvent::new(
                EventType::BiometricResult,
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

// ────────── Async result channel (platform backend → manager) ─────────
//
// The OS prompt's reply block / `AuthenticationCallback` fires on an
// arbitrary thread with no handle to the live `BiometricManager` (it
// lives inside the window's `LayoutWindow`). The backend parks each
// result here; the layout pass drains it once per frame via
// [`drain_biometric_results`] and applies the latest through
// [`BiometricManager::set_last_result`]. Pure Rust — no platform
// dependency (SUPER_PLAN_2 §0.5). Mirrors the geolocation manager's
// async-fix channel.

static PENDING_RESULTS: std::sync::Mutex<Vec<BiometricResult>> =
    std::sync::Mutex::new(Vec::new());

/// Park a biometric result delivered by a platform backend (in the dll).
/// Thread-safe; recovers from a poisoned lock so one panicking applier
/// can't wedge delivery forever.
pub fn push_biometric_result(result: BiometricResult) {
    let mut q = PENDING_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(result);
}

/// Drain every result parked by [`push_biometric_result`], in arrival
/// order. Called once per layout pass; the caller applies them through
/// [`BiometricManager::set_last_result`] (the last one wins).
pub fn drain_biometric_results() -> Vec<BiometricResult> {
    let mut q = PENDING_RESULTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

// ────────── Request channel (callback → platform backend) ─────────────
//
// The reverse direction: a callback (e.g. an unlock button's `on_click`)
// calls `CallbackInfo::request_biometric_auth(prompt)`, which parks the
// prompt here. The dll layout pass drains it via
// [`drain_biometric_requests`] and dispatches each to the native backend
// (`dll::desktop::extra::biometric::request`), which shows the OS prompt
// and later parks the outcome back through [`push_biometric_result`].
// Decoupling via a channel keeps the request callable from any callback
// without threading the window's backend handle through `CallbackInfo`,
// and keeps `azul-layout` platform-free (SUPER_PLAN_2 §0.5).

static PENDING_REQUESTS: std::sync::Mutex<Vec<BiometricPrompt>> =
    std::sync::Mutex::new(Vec::new());

/// Queue a biometric-auth request from a callback. Picked up by the dll
/// layout pass and dispatched to the native prompt. Thread-safe;
/// poison-recovering.
pub fn push_biometric_request(prompt: BiometricPrompt) {
    let mut q = PENDING_REQUESTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    q.push(prompt);
}

/// Drain every request queued by [`push_biometric_request`], in arrival
/// order. Called once per layout pass; the dll dispatches each to the
/// platform backend.
pub fn drain_biometric_requests() -> Vec<BiometricPrompt> {
    let mut q = PENDING_REQUESTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

/// MWA-C-biometric: true while requests are parked in the channel but not
/// yet dispatched.
///
/// The capability pump's arming check must count these —
/// `has_pending_async` only sees `in_flight` (post-dispatch), so a prompt
/// queued MID-pass (after the top-of-pass pump already ran) would otherwise
/// wait for an unrelated event before ever being shown.
pub fn has_queued_requests() -> bool {
    PENDING_REQUESTS
        .lock()
        .map_or_else(|e| !e.into_inner().is_empty(), |q| !q.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_defaults_to_unavailable_and_no_result() {
        let mgr = BiometricManager::new();
        assert_eq!(mgr.availability(), BiometricKind::NotAvailable);
        assert!(!mgr.is_available());
        assert_eq!(mgr.last_result(), None);
        assert!(!mgr.last_was_success());
    }

    #[test]
    fn set_availability_returns_change_flag() {
        let mut mgr = BiometricManager::new();
        assert!(mgr.set_availability(BiometricKind::Face));
        assert!(mgr.is_available());
        assert_eq!(mgr.availability(), BiometricKind::Face);
        // Same value again — no change.
        assert!(!mgr.set_availability(BiometricKind::Face));
        // Different value — change.
        assert!(mgr.set_availability(BiometricKind::Fingerprint));
    }

    #[test]
    fn set_last_result_returns_change_flag() {
        let mut mgr = BiometricManager::new();
        assert!(mgr.set_last_result(BiometricResult::Failed));
        assert_eq!(mgr.last_result(), Some(BiometricResult::Failed));
        assert!(!mgr.last_was_success());
        // Re-applying the same outcome is not a change.
        assert!(!mgr.set_last_result(BiometricResult::Failed));
        // A new outcome is a change, and Authenticated is a success.
        assert!(mgr.set_last_result(BiometricResult::Authenticated));
        assert!(mgr.last_was_success());
    }

    #[test]
    fn passcode_fallback_counts_as_success() {
        let mut mgr = BiometricManager::new();
        mgr.set_last_result(BiometricResult::FellBackToPasscode);
        assert!(mgr.last_was_success());
        assert!(BiometricResult::FellBackToPasscode.is_success());
        // Cancelled / Failed / Unavailable / Error are not successes.
        for r in [
            BiometricResult::Cancelled,
            BiometricResult::Failed,
            BiometricResult::Unavailable,
            BiometricResult::Error,
        ] {
            assert!(!r.is_success(), "{r:?} must not be a success");
        }
    }

    #[test]
    fn async_results_round_trip_through_manager() {
        // The channel is process-global; serialize against every other
        // channel test, then clear any residue.
        let _guard = super::autotest_generated::lock_channels();
        drop(drain_biometric_results());

        push_biometric_result(BiometricResult::Failed);
        push_biometric_result(BiometricResult::Authenticated); // last wins
        let drained = drain_biometric_results();
        assert_eq!(drained.len(), 2, "both parked results drain in order");
        assert_eq!(drained[0], BiometricResult::Failed);
        assert_eq!(drained[1], BiometricResult::Authenticated);

        // Applying them reflects in last_result() — what the layout pass does.
        let mut mgr = BiometricManager::new();
        for r in &drained {
            mgr.set_last_result(*r);
        }
        assert_eq!(
            mgr.last_result(),
            Some(BiometricResult::Authenticated),
            "the last applied result wins"
        );
        assert!(mgr.last_was_success());

        // A second drain is empty — the queue was taken, not copied.
        assert!(drain_biometric_results().is_empty());
    }

    #[test]
    fn requests_round_trip_through_channel() {
        // Process-global; serialize against every other channel test, then
        // clear residue.
        let _guard = super::autotest_generated::lock_channels();
        drop(drain_biometric_requests());

        push_biometric_request(BiometricPrompt::new("Unlock A".into()));
        push_biometric_request(BiometricPrompt::new("Unlock B".into()));
        let drained = drain_biometric_requests();
        assert_eq!(drained.len(), 2, "both queued requests drain in order");
        assert_eq!(drained[0].reason.as_str(), "Unlock A");
        assert_eq!(drained[1].reason.as_str(), "Unlock B");

        // A second drain is empty — the queue was taken, not copied.
        assert!(drain_biometric_requests().is_empty());
    }

    #[test]
    fn biometric_prompt_defaults_and_constructor() {
        let d = BiometricPrompt::default();
        assert!(!d.allow_device_credential);
        assert_eq!(d.reason.as_str(), "");

        let p = BiometricPrompt::new("Unlock your vault".into());
        assert_eq!(p.reason.as_str(), "Unlock your vault");
        assert_eq!(p.cancel_label.as_str(), "");
        assert!(!p.allow_device_credential);
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
    fn in_flight_tracks_dispatch_and_completion() {
        let mut mgr = BiometricManager::new();
        assert!(!mgr.has_pending_async());
        mgr.mark_requests_dispatched(2);
        assert!(mgr.has_pending_async());
        mgr.set_last_result(BiometricResult::Cancelled);
        assert!(mgr.has_pending_async(), "one of two still outstanding");
        mgr.set_last_result(BiometricResult::Authenticated);
        assert!(!mgr.has_pending_async());
        // saturates — an unsolicited result never underflows
        mgr.set_last_result(BiometricResult::Failed);
        assert!(!mgr.has_pending_async());
    }

    #[test]
    fn every_completion_fires_an_event_even_identical_repeats() {
        let mut mgr = BiometricManager::new();
        assert!(mgr.get_pending_events(ts()).is_empty());
        mgr.set_last_result(BiometricResult::Cancelled);
        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts()).is_empty());
        // identical outcome answering a FRESH prompt → fresh event
        mgr.set_last_result(BiometricResult::Cancelled);
        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
        assert_eq!(
            mgr.get_pending_events(ts())[0].event_type,
            EventType::BiometricResult
        );
    }
}

#[cfg(test)]
mod autotest_generated {
    use alloc::string::{String, ToString};
    use std::sync::{Mutex, PoisonError};

    use azul_core::task::SystemTick;

    use super::*;

    /// The result/request channels are process-global statics and `cargo
    /// test` runs tests in parallel threads, so EVERY test that pushes to
    /// or drains a channel must hold this lock — otherwise one test's
    /// pushes land inside another's drain window. Shared with the
    /// hand-written `tests` module above.
    static CHANNEL_LOCK: Mutex<()> = Mutex::new(());

    pub(super) fn lock_channels() -> std::sync::MutexGuard<'static, ()> {
        CHANNEL_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn ts() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    const ALL_KINDS: [BiometricKind; 4] = [
        BiometricKind::NotAvailable,
        BiometricKind::Fingerprint,
        BiometricKind::Face,
        BiometricKind::Iris,
    ];

    const ALL_RESULTS: [BiometricResult; 6] = [
        BiometricResult::Authenticated,
        BiometricResult::Failed,
        BiometricResult::Cancelled,
        BiometricResult::FellBackToPasscode,
        BiometricResult::Unavailable,
        BiometricResult::Error,
    ];

    // ── constructor ────────────────────────────────────────────────────

    #[test]
    fn new_equals_default_and_holds_the_zero_state_invariants() {
        let a = BiometricManager::new();
        let b = BiometricManager::default();
        assert_eq!(a, b, "new() must be exactly default()");
        // Every field is at its documented zero state.
        assert_eq!(a.last_result, None);
        assert_eq!(a.availability, BiometricKind::NotAvailable);
        assert_eq!(a.in_flight, 0);
        assert!(!a.pending_event);
        // ...and the accessors agree with the fields.
        assert_eq!(a.last_result(), None);
        assert_eq!(a.availability(), BiometricKind::NotAvailable);
        assert!(!a.is_available());
        assert!(!a.has_pending_async());
        assert!(!a.last_was_success());
        assert!(
            a.get_pending_events(ts()).is_empty(),
            "a fresh manager owes the event pass nothing"
        );
        // Repeated construction is deterministic.
        assert_eq!(BiometricManager::new(), BiometricManager::new());
    }

    // ── getters / predicates ───────────────────────────────────────────

    #[test]
    fn getters_mirror_the_public_fields_for_every_kind_result_pair() {
        for kind in ALL_KINDS {
            for result in ALL_RESULTS {
                let mut mgr = BiometricManager::new();
                mgr.set_availability(kind);
                mgr.set_last_result(result);

                assert_eq!(mgr.availability(), kind, "{kind:?}");
                assert_eq!(mgr.availability(), mgr.availability, "{kind:?}");
                assert_eq!(
                    mgr.is_available(),
                    kind.is_available(),
                    "is_available must delegate to the kind ({kind:?})"
                );
                assert_eq!(mgr.last_result(), Some(result), "{result:?}");
                assert_eq!(mgr.last_result(), mgr.last_result, "{result:?}");
                assert_eq!(
                    mgr.last_was_success(),
                    result.is_success(),
                    "last_was_success must delegate to the result ({result:?})"
                );
                // The two axes are independent: an unavailable sensor can
                // still hold a successful (passcode-fallback) outcome.
                assert_eq!(mgr.availability(), kind);
            }
        }
    }

    #[test]
    fn is_available_is_true_for_every_real_sensor_and_false_only_for_none() {
        let mut mgr = BiometricManager::new();
        assert!(!mgr.is_available(), "known-false: the default");
        mgr.set_availability(BiometricKind::Fingerprint);
        assert!(mgr.is_available(), "known-true: a real sensor");

        for kind in ALL_KINDS {
            let mut mgr = BiometricManager::new();
            mgr.set_availability(kind);
            assert_eq!(
                mgr.is_available(),
                kind != BiometricKind::NotAvailable,
                "only NotAvailable is unavailable ({kind:?})"
            );
        }
    }

    #[test]
    fn last_was_success_is_false_while_no_request_has_resolved() {
        // `matches!(None, Some(r) if ..)` must not be read as "unknown".
        let mgr = BiometricManager::new();
        assert_eq!(mgr.last_result(), None);
        assert!(
            !mgr.last_was_success(),
            "an unauthenticated vault must stay locked before the first attempt"
        );
        // An available sensor alone never implies a success.
        let mut mgr = BiometricManager::new();
        mgr.set_availability(BiometricKind::Face);
        assert!(mgr.is_available());
        assert!(!mgr.last_was_success());
    }

    #[test]
    fn only_authenticated_and_passcode_fallback_unlock_the_gate() {
        for result in ALL_RESULTS {
            let mut mgr = BiometricManager::new();
            mgr.set_last_result(result);
            let expected = matches!(
                result,
                BiometricResult::Authenticated | BiometricResult::FellBackToPasscode
            );
            assert_eq!(
                mgr.last_was_success(),
                expected,
                "{result:?} must {} unlock the gate",
                if expected { "" } else { "not" }
            );
        }
    }

    // ── set_availability / set_last_result change flags ─────────────────

    #[test]
    fn set_availability_change_flag_is_exactly_inequality_for_all_pairs() {
        for from in ALL_KINDS {
            for to in ALL_KINDS {
                let mut mgr = BiometricManager::new();
                mgr.availability = from;
                let changed = mgr.set_availability(to);
                assert_eq!(
                    changed,
                    from != to,
                    "set_availability({to:?}) over {from:?} reported the wrong change flag"
                );
                assert_eq!(mgr.availability(), to, "the write must land regardless");
                // Re-applying the same value is never a change.
                assert!(!mgr.set_availability(to), "idempotent re-apply of {to:?}");
                assert_eq!(mgr.availability(), to);
            }
        }
    }

    #[test]
    fn set_availability_does_not_disturb_the_async_or_result_state() {
        let mut mgr = BiometricManager::new();
        mgr.mark_requests_dispatched(3);
        mgr.set_last_result(BiometricResult::Authenticated);
        mgr.clear_pending_event();
        let before = mgr;

        mgr.set_availability(BiometricKind::Iris);

        assert_eq!(mgr.last_result, before.last_result, "result untouched");
        assert_eq!(mgr.in_flight, before.in_flight, "in_flight untouched");
        assert_eq!(
            mgr.pending_event, before.pending_event,
            "an availability probe is not a prompt outcome — it must not fire an event"
        );
    }

    #[test]
    fn set_last_result_change_flag_is_exactly_inequality_for_all_pairs() {
        for first in ALL_RESULTS {
            for second in ALL_RESULTS {
                let mut mgr = BiometricManager::new();
                assert!(
                    mgr.set_last_result(first),
                    "None -> Some({first:?}) is always a change"
                );
                let changed = mgr.set_last_result(second);
                assert_eq!(
                    changed,
                    first != second,
                    "set_last_result({second:?}) over {first:?} reported the wrong change flag"
                );
                assert_eq!(mgr.last_result(), Some(second), "the write must land");
            }
        }
    }

    #[test]
    fn every_completion_arms_a_pending_event_even_when_unchanged() {
        // The change flag ("re-render?") and the event flag ("tell the
        // callback?") are different questions — a repeat outcome answers a
        // FRESH prompt and must still be delivered.
        for result in ALL_RESULTS {
            let mut mgr = BiometricManager::new();
            mgr.set_last_result(result);
            mgr.clear_pending_event();

            let changed = mgr.set_last_result(result);
            assert!(!changed, "identical outcome is not a state change ({result:?})");
            assert!(
                mgr.pending_event,
                "a repeated {result:?} must still fire an event"
            );
            assert_eq!(mgr.get_pending_events(ts()).len(), 1);
        }
    }

    // ── mark_requests_dispatched (numeric) ─────────────────────────────

    #[test]
    fn mark_requests_dispatched_zero_is_a_no_op() {
        let mut mgr = BiometricManager::new();
        mgr.mark_requests_dispatched(0);
        assert_eq!(mgr.in_flight, 0, "0 prompts dispatched adds nothing");
        assert!(
            !mgr.has_pending_async(),
            "dispatching nothing must not arm the pump's timer"
        );

        mgr.mark_requests_dispatched(1);
        mgr.mark_requests_dispatched(0);
        assert_eq!(mgr.in_flight, 1, "0 must not clear an outstanding prompt");
        assert!(mgr.has_pending_async());
    }

    #[test]
    fn mark_requests_dispatched_saturates_at_u32_max_instead_of_overflowing() {
        let mut mgr = BiometricManager::new();
        mgr.mark_requests_dispatched(u32::MAX);
        assert_eq!(mgr.in_flight, u32::MAX);
        assert!(mgr.has_pending_async());

        // A debug-build `+` here would panic; saturating_add must clamp.
        mgr.mark_requests_dispatched(1);
        assert_eq!(mgr.in_flight, u32::MAX, "saturates, does not wrap to 0");
        mgr.mark_requests_dispatched(u32::MAX);
        assert_eq!(mgr.in_flight, u32::MAX, "still clamped");
        assert!(
            mgr.has_pending_async(),
            "wrapping to 0 here would disarm the pump and strand the reply"
        );
    }

    #[test]
    fn mark_requests_dispatched_accumulates_across_calls() {
        let mut mgr = BiometricManager::new();
        for _ in 0..10u32 {
            mgr.mark_requests_dispatched(7);
        }
        assert_eq!(mgr.in_flight, 70);
        // Boundary: u32::MAX - 1 then +1 lands exactly on MAX, not past it.
        let mut mgr = BiometricManager::new();
        mgr.mark_requests_dispatched(u32::MAX - 1);
        mgr.mark_requests_dispatched(1);
        assert_eq!(mgr.in_flight, u32::MAX);
    }

    #[test]
    fn unsolicited_results_saturate_in_flight_at_zero_instead_of_underflowing() {
        // A backend that parks a result for a prompt we never dispatched
        // (or a duplicate delivery) must not wrap in_flight to u32::MAX —
        // that would arm the capability timer forever.
        let mut mgr = BiometricManager::new();
        assert_eq!(mgr.in_flight, 0);
        for result in ALL_RESULTS {
            mgr.set_last_result(result);
            assert_eq!(mgr.in_flight, 0, "underflow on unsolicited {result:?}");
            assert!(!mgr.has_pending_async());
        }
    }

    #[test]
    fn each_completion_retires_exactly_one_in_flight_prompt() {
        let mut mgr = BiometricManager::new();
        mgr.mark_requests_dispatched(3);
        assert_eq!(mgr.in_flight, 3);

        mgr.set_last_result(BiometricResult::Cancelled);
        assert_eq!(mgr.in_flight, 2);
        assert!(mgr.has_pending_async());
        mgr.set_last_result(BiometricResult::Failed);
        assert_eq!(mgr.in_flight, 1);
        assert!(mgr.has_pending_async());
        mgr.set_last_result(BiometricResult::Authenticated);
        assert_eq!(mgr.in_flight, 0);
        assert!(!mgr.has_pending_async(), "all three outcomes folded back");

        // Even from a saturated count, one completion retires exactly one.
        let mut mgr = BiometricManager::new();
        mgr.mark_requests_dispatched(u32::MAX);
        mgr.set_last_result(BiometricResult::Error);
        assert_eq!(mgr.in_flight, u32::MAX - 1);
    }

    #[test]
    fn has_pending_async_is_true_exactly_while_in_flight_is_non_zero() {
        for n in [0u32, 1, 2, u32::MAX - 1, u32::MAX] {
            let mut mgr = BiometricManager::new();
            mgr.mark_requests_dispatched(n);
            assert_eq!(
                mgr.has_pending_async(),
                n > 0,
                "has_pending_async disagreed with in_flight = {n}"
            );
        }
    }

    // ── clear_pending_event ────────────────────────────────────────────

    #[test]
    fn clear_pending_event_is_idempotent_and_touches_nothing_else() {
        let mut mgr = BiometricManager::new();
        mgr.set_availability(BiometricKind::Face);
        mgr.mark_requests_dispatched(2);
        mgr.set_last_result(BiometricResult::Authenticated);
        assert!(mgr.pending_event);

        mgr.clear_pending_event();
        assert!(!mgr.pending_event);
        let after_first = mgr;

        // Clearing twice (a second event pass with no new outcome) is safe.
        mgr.clear_pending_event();
        assert_eq!(mgr, after_first, "the second clear is a no-op");
        assert!(mgr.get_pending_events(ts()).is_empty());

        // The outcome, the sensor probe and the in-flight count survive it.
        assert_eq!(mgr.last_result(), Some(BiometricResult::Authenticated));
        assert!(mgr.last_was_success(), "clearing the event must not re-lock the vault");
        assert_eq!(mgr.availability(), BiometricKind::Face);
        assert_eq!(mgr.in_flight, 1);
        assert!(mgr.has_pending_async(), "the second prompt is still outstanding");
    }

    #[test]
    fn clear_pending_event_on_a_fresh_manager_is_a_no_op() {
        let mut mgr = BiometricManager::new();
        mgr.clear_pending_event();
        assert_eq!(mgr, BiometricManager::new());
    }

    // ── EventProvider ──────────────────────────────────────────────────

    #[test]
    fn pending_event_yields_exactly_one_root_event_and_is_not_consuming() {
        let mut mgr = BiometricManager::new();
        mgr.set_last_result(BiometricResult::Authenticated);

        let events = mgr.get_pending_events(ts());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::BiometricResult);
        assert_eq!(events[0].source, CoreEventSource::User);
        assert_eq!(
            events[0].target,
            DomNodeId::ROOT,
            "the outcome is window-level, not node-level"
        );
        assert_eq!(events[0].data, EventData::None);
        assert_eq!(events[0].timestamp, ts(), "the caller's timestamp is preserved");

        // Reading is not draining — only clear_pending_event() retires it.
        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts()).is_empty());
    }

    // ── Copy semantics ─────────────────────────────────────────────────

    #[test]
    fn manager_is_copy_so_a_snapshot_never_sees_later_mutations() {
        // `BiometricManager: Copy` — a caller holding `let mut m = *mgr;`
        // mutates a detached snapshot. Pin the semantics so a future field
        // (e.g. a Vec of pending prompts) can't silently change them.
        let mut original = BiometricManager::new();
        original.mark_requests_dispatched(1);

        let mut snapshot = original;
        snapshot.set_last_result(BiometricResult::Authenticated);
        snapshot.set_availability(BiometricKind::Iris);

        assert_eq!(original.last_result(), None, "the original is untouched");
        assert_eq!(original.availability(), BiometricKind::NotAvailable);
        assert_eq!(original.in_flight, 1);
        assert!(!original.pending_event);

        assert_eq!(snapshot.last_result(), Some(BiometricResult::Authenticated));
        assert_eq!(snapshot.in_flight, 0);
    }

    // ── async result channel ───────────────────────────────────────────

    #[test]
    fn results_channel_round_trips_every_variant_in_arrival_order() {
        let _guard = lock_channels();
        drop(drain_biometric_results());

        for result in ALL_RESULTS {
            push_biometric_result(result);
        }
        let drained = drain_biometric_results();
        assert_eq!(drained.len(), ALL_RESULTS.len(), "no result is dropped");
        assert_eq!(
            drained.as_slice(),
            ALL_RESULTS.as_slice(),
            "the channel is FIFO — the layout pass relies on last-one-wins"
        );
        assert!(
            drain_biometric_results().is_empty(),
            "draining takes the queue, it does not copy it"
        );
    }

    #[test]
    fn draining_an_empty_results_channel_repeatedly_is_safe() {
        let _guard = lock_channels();
        drop(drain_biometric_results());
        for _ in 0..3 {
            assert!(drain_biometric_results().is_empty());
        }
    }

    #[test]
    fn duplicate_results_are_all_preserved_in_the_channel() {
        // Dedup is the manager's job (via the change flag), not the
        // channel's — every parked outcome answers its own prompt.
        let _guard = lock_channels();
        drop(drain_biometric_results());

        for _ in 0..64 {
            push_biometric_result(BiometricResult::Failed);
        }
        let drained = drain_biometric_results();
        assert_eq!(drained.len(), 64);
        assert!(drained.iter().all(|r| *r == BiometricResult::Failed));

        // Folding them all back never underflows in_flight, and the last
        // one still wins.
        let mut mgr = BiometricManager::new();
        mgr.mark_requests_dispatched(2);
        for r in &drained {
            mgr.set_last_result(*r);
        }
        assert_eq!(mgr.in_flight, 0, "saturating_sub floors at zero");
        assert_eq!(mgr.last_result(), Some(BiometricResult::Failed));
    }

    #[test]
    fn results_pushed_from_many_threads_are_all_delivered() {
        let _guard = lock_channels();
        drop(drain_biometric_results());

        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..25 {
                        push_biometric_result(BiometricResult::Authenticated);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().expect("a backend thread panicked while parking a result");
        }

        let drained = drain_biometric_results();
        assert_eq!(drained.len(), 100, "no result lost across 4 backend threads");
        assert!(drained.iter().all(|r| r.is_success()));
        assert!(drain_biometric_results().is_empty());
    }

    // ── request channel ────────────────────────────────────────────────

    #[test]
    fn has_queued_requests_tracks_the_queue_exactly() {
        let _guard = lock_channels();
        drop(drain_biometric_requests());
        assert!(
            !has_queued_requests(),
            "known-false: nothing parked after a drain"
        );

        push_biometric_request(BiometricPrompt::new("Unlock".into()));
        assert!(has_queued_requests(), "known-true: one prompt parked");
        // Reading the flag must not consume the queue.
        assert!(has_queued_requests());

        push_biometric_request(BiometricPrompt::default());
        assert!(has_queued_requests());

        let drained = drain_biometric_requests();
        assert_eq!(drained.len(), 2);
        assert!(
            !has_queued_requests(),
            "the pump's arming check must go quiet once the prompts are dispatched"
        );
    }

    #[test]
    fn request_channel_round_trips_hostile_prompt_strings_byte_for_byte() {
        let _guard = lock_channels();
        drop(drain_biometric_requests());

        let hostile: Vec<String> = alloc::vec![
            String::new(),                           // empty → platform default
            "Entsperre den Tresor 🔐".to_string(),   // emoji (4-byte scalar)
            "افتح الخزنة".to_string(),               // RTL
            "e\u{0301}\u{0301}\u{0301}".to_string(), // stacked combining marks
            "line\nbreak\ttab\r\n".to_string(),      // control chars
            "nul\0inside".to_string(),               // interior NUL
            "%s %n {} \\0 ${x}".to_string(),         // format-string bait
            "u".repeat(64 * 1024),                   // 64 KiB reason
        ];

        for reason in &hostile {
            push_biometric_request(BiometricPrompt {
                reason: reason.clone().into(),
                cancel_label: reason.clone().into(),
                allow_device_credential: true,
            });
        }

        let drained = drain_biometric_requests();
        assert_eq!(drained.len(), hostile.len(), "no prompt is dropped");
        for (prompt, expected) in drained.iter().zip(hostile.iter()) {
            assert_eq!(
                prompt.reason.as_str(),
                expected.as_str(),
                "the reason must survive the channel unmangled"
            );
            assert_eq!(prompt.cancel_label.as_str(), expected.as_str());
            assert!(prompt.allow_device_credential);
        }
        assert_eq!(
            drained.last().unwrap().reason.as_str().len(),
            64 * 1024,
            "a 64 KiB reason is not truncated"
        );
        assert!(!has_queued_requests());
    }

    #[test]
    fn requests_pushed_from_many_threads_are_all_delivered() {
        let _guard = lock_channels();
        drop(drain_biometric_requests());

        let threads: Vec<_> = (0..4)
            .map(|i| {
                std::thread::spawn(move || {
                    for j in 0..25 {
                        push_biometric_request(BiometricPrompt::new(
                            alloc::format!("t{i}-{j}").into(),
                        ));
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().expect("a callback thread panicked while queueing a prompt");
        }

        let drained = drain_biometric_requests();
        assert_eq!(drained.len(), 100, "no prompt lost across 4 callback threads");
        // Each thread's own prompts keep their relative order.
        for i in 0..4 {
            let mine: Vec<_> = drained
                .iter()
                .filter(|p| p.reason.as_str().starts_with(&alloc::format!("t{i}-")))
                .collect();
            assert_eq!(mine.len(), 25, "thread {i} lost a prompt");
            for (j, p) in mine.iter().enumerate() {
                assert_eq!(p.reason.as_str(), alloc::format!("t{i}-{j}"));
            }
        }
        assert!(!has_queued_requests());
    }

    // ── full request → dispatch → result loop ──────────────────────────

    #[test]
    fn full_request_dispatch_result_loop_leaves_the_pump_disarmed() {
        let _guard = lock_channels();
        drop(drain_biometric_requests());
        drop(drain_biometric_results());

        let mut mgr = BiometricManager::new();
        mgr.set_availability(BiometricKind::Fingerprint);

        // 1. Two callbacks each queue a prompt.
        push_biometric_request(BiometricPrompt::new("Unlock A".into()));
        push_biometric_request(BiometricPrompt::new("Unlock B".into()));
        assert!(
            has_queued_requests(),
            "queued-but-undispatched prompts must arm the pump on their own — \
             has_pending_async() cannot see them yet"
        );
        assert!(!mgr.has_pending_async());

        // 2. The layout pass drains and dispatches them.
        let requests = drain_biometric_requests();
        assert_eq!(requests.len(), 2);
        mgr.mark_requests_dispatched(u32::try_from(requests.len()).unwrap());
        assert!(!has_queued_requests());
        assert!(mgr.has_pending_async(), "now the in-flight count arms the pump");

        // 3. The backend parks both outcomes; the next pass folds them back.
        push_biometric_result(BiometricResult::Failed);
        push_biometric_result(BiometricResult::FellBackToPasscode);
        for r in drain_biometric_results() {
            mgr.set_last_result(r);
        }

        assert_eq!(mgr.in_flight, 0);
        assert!(!mgr.has_pending_async(), "pump disarms once every outcome folded");
        assert!(mgr.pending_event);
        assert_eq!(mgr.get_pending_events(ts()).len(), 1);
        assert_eq!(mgr.last_result(), Some(BiometricResult::FellBackToPasscode));
        assert!(mgr.last_was_success(), "the passcode fallback unlocks the vault");

        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts()).is_empty());
    }
}
