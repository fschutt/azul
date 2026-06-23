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

/// Cross-platform keyring state. One per `App` — the OS keyring is a
/// per-process (per-app-identity) store, not per-window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyringManager {
    /// Outcome of the most recent keyring op, or `None` until the first
    /// completes. Read by callbacks via `CallbackInfo::get_keyring_result()`.
    pub last_result: Option<KeyringResult>,
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
        changed
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
        let _ = drain_keyring_requests();

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
        let _ = drain_keyring_results();

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
