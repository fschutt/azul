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

#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

/// Dispatch one biometric-auth request to the native prompt. Called from
/// `regenerate_layout` for each prompt the layout pass drained from the
/// request channel.
///
/// iOS/macOS route to the real `LAContext` backend; Android to the
/// `BiometricPrompt` JNI backend; Windows to `UserConsentVerifier`
/// (Hello); Linux to fprintd over D-Bus — all four park the outcome in
/// the result channel asynchronously (MWA-C-biometric: doc was stale,
/// every desktop backend is real). Targets without any backend resolve
/// to `Unavailable` immediately so the request → result round-trip stays
/// observable — `CallbackInfo::get_biometric_result()` reads it next
/// frame.
pub fn request(prompt: &BiometricPrompt) {
    // MWA-C-biometric: never pop a REAL OS auth sheet from a headless /
    // E2E run (the dispatcher is keyed on target_os, not backend — a
    // headless test on a Mac dev box would otherwise raise a live TouchID
    // sheet). Resolve to Unavailable so the round-trip stays observable.
    if std::env::var("AZ_BACKEND").as_deref() == Ok("headless")
        || std::env::var("AZ_E2E_TEST").is_ok()
    {
        let _ = prompt;
        azul_layout::managers::biometric::push_biometric_result(
            azul_core::biometric::BiometricResult::Unavailable,
        );
        return;
    }
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    apple::request(prompt);
    #[cfg(target_os = "android")]
    android::request(prompt);
    #[cfg(target_os = "windows")]
    windows::request(prompt);
    #[cfg(target_os = "linux")]
    linux::request(prompt);
    #[cfg(not(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "android",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        let _ = prompt;
        azul_layout::managers::biometric::push_biometric_result(
            azul_core::biometric::BiometricResult::Unavailable,
        );
    }
}

/// Synchronous availability probe — what biometric hardware the device
/// can use. iOS/macOS query `canEvaluatePolicy` + `biometryType`; Android
/// queries `BiometricManager.canAuthenticate` + `PackageManager` features;
/// other platforms return `NotAvailable` until their backend lands. The
/// result is written into `BiometricManager::set_availability` (a later
/// wiring tick), which `CallbackInfo::get_biometric_kind()` then reads.
pub fn probe_availability() -> BiometricKind {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        apple::probe_availability()
    }
    #[cfg(target_os = "android")]
    {
        android::probe_availability()
    }
    #[cfg(target_os = "windows")]
    {
        windows::probe_availability()
    }
    #[cfg(target_os = "linux")]
    {
        linux::probe_availability()
    }
    #[cfg(not(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "android",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        BiometricKind::NotAvailable
    }
}

/// Device biometric capability, probed once and cached for the process.
/// [`probe_availability`] is a native call (`LAContext` create +
/// `canEvaluatePolicy`, or a JNI round-trip), so the layout pass folds
/// *this* — a cheap cached read after the first probe — into the manager
/// each frame rather than re-probing at frame rate. The device capability
/// rarely changes at runtime; refresh-on-enrollment-change is a future
/// refinement if needed.
pub fn availability_cached() -> BiometricKind {
    static CACHED: std::sync::OnceLock<BiometricKind> = std::sync::OnceLock::new();
    *CACHED.get_or_init(probe_availability)
}
