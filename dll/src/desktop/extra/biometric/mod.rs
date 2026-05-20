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

use azul_core::biometric::{BiometricKind, BiometricPrompt};

#[cfg(target_os = "macos")]
pub mod macos;

/// Dispatch one biometric-auth request to the native prompt. Called from
/// `regenerate_layout` for each prompt the layout pass drained from the
/// request channel.
///
/// macOS routes to the real `LAContext` backend (the reply block parks the
/// outcome in the result channel asynchronously). Platforms without a
/// backend yet (iOS / Android / Windows / Linux) resolve to `Unavailable`
/// immediately so the request → result round-trip stays observable —
/// `CallbackInfo::get_biometric_result()` reads it next frame.
pub fn request(prompt: &BiometricPrompt) {
    #[cfg(target_os = "macos")]
    macos::request(prompt);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = prompt;
        azul_layout::managers::biometric::push_biometric_result(
            azul_core::biometric::BiometricResult::Unavailable,
        );
    }
}

/// Synchronous availability probe — what biometric hardware the device
/// can use. macOS queries `canEvaluatePolicy` + `biometryType`; other
/// platforms return `NotAvailable` until their backend lands. The result
/// is written into `BiometricManager::set_availability` (a later wiring
/// tick), which `CallbackInfo::get_biometric_kind()` then reads.
pub fn probe_availability() -> BiometricKind {
    #[cfg(target_os = "macos")]
    {
        macos::probe_availability()
    }
    #[cfg(not(target_os = "macos"))]
    {
        BiometricKind::NotAvailable
    }
}
