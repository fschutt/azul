//! Platform dispatcher for biometric-auth requests
//! (SUPER_PLAN_2 §1 feature 4 + research/02).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::biometric::BiometricManager`. A callback
//! queues a request via `CallbackInfo::request_biometric_auth(prompt)`
//! (parked in azul-layout's process-global request channel); the layout
//! pass drains it and calls [`request`] here, which turns the prompt into
//! the right native API call:
//!
//! | Platform | Boolean auth | Probe |
//! |----------|--------------|-------|
//! | iOS / macOS | `-[LAContext evaluatePolicy:localizedReason:reply:]` | `canEvaluatePolicy:` + `biometryType` |
//! | Android | `BiometricPrompt.authenticate(promptInfo)` | `BiometricManager.canAuthenticate(...)` |
//! | Windows | `UserConsentVerifier.RequestVerificationAsync(msg)` | `CheckAvailabilityAsync()` |
//! | Linux | polkit `CheckAuthorization` / PAM `pam_authenticate` | (degraded) |
//!
//! This tick lands the dispatcher with a no-backend stub that resolves
//! every request to [`BiometricResult::Unavailable`] (research/02 §12
//! step 3: "green-light the API returning Unavailable on every platform
//! first"). The OS draws its own modal asynchronously, so the real
//! backends (objc2 `LAContext`, Android `BiometricPrompt` JNI) park the
//! outcome back through `push_biometric_result` from the reply
//! block / callback on a later frame — exactly mirroring how the
//! geolocation dispatcher shipped ahead of its native subscriptions.

use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};
use azul_layout::managers::biometric::push_biometric_result;

/// Dispatch one biometric-auth request to the native prompt. Called from
/// `regenerate_layout` for each prompt the layout pass drained from the
/// request channel.
///
/// Stub for now: no native backend, so the request resolves to
/// `Unavailable` immediately (parked in the manager's result channel so
/// the request → result round-trip is observable —
/// `CallbackInfo::get_biometric_result()` reads `Unavailable` next
/// frame). The iOS/macOS `LAContext` and Android `BiometricPrompt`
/// backends replace this body in a later tick; they will return without
/// pushing here and instead push the true outcome from the OS reply
/// asynchronously.
pub fn request(prompt: &BiometricPrompt) {
    let _ = prompt;
    push_biometric_result(BiometricResult::Unavailable);
}

/// Synchronous availability probe — what biometric hardware the device
/// can use. Stub returns `NotAvailable`; the real backends query
/// `LAContext.biometryType` / `BiometricManager.canAuthenticate` on
/// startup and write it via `BiometricManager::set_availability`, which
/// `CallbackInfo::get_biometric_kind()` then reads. This entry point
/// exists so that probe lands here without re-plumbing.
pub fn probe_availability() -> BiometricKind {
    BiometricKind::NotAvailable
}
