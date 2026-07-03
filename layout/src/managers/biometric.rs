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
        // The channel is process-global; clear any residue first.
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
        // Process-global; clear residue first.
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
