//! Platform dispatcher for biometric-auth requests
//! (SUPER_PLAN_2 §1 feature 4 + research/02).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::biometric::BiometricManager`. A callback
//! queues a request via `CallbackInfo::request_biometric_auth(prompt)`
//! (parked in the app's `BiometricChannels`); the layout pass drains it
//! and calls [`request`] here, which turns the prompt into the right
//! native API call:
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
//! outcome back through the app's result queue from the reply
//! block / callback on a later frame — exactly mirroring how the
//! geolocation dispatcher shipped ahead of its native subscriptions.

use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};
use azul_layout::managers::shared_queue::SharedQueue;

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

/// Dispatch one biometric-auth request to the native prompt. Called from
/// `regenerate_layout` for each prompt the layout pass drained from the
/// request channel. `results` is the app's queue the outcome is parked in
/// (`BiometricChannels::results`): the OS reply block, thread or JNI entry
/// that delivers it holds a clone.
///
/// iOS/macOS route to the real `LAContext` backend; Android to the
/// `BiometricPrompt` JNI backend; Windows to `UserConsentVerifier`
/// (Hello); Linux to fprintd over D-Bus — all four park the outcome in
/// the result channel asynchronously (MWA-C-biometric: doc was stale,
/// every desktop backend is real). Targets without any backend resolve
/// to `Unavailable` immediately so the request → result round-trip stays
/// observable — `CallbackInfo::get_biometric_result()` reads it next
/// frame.
pub fn request(prompt: &BiometricPrompt, results: &SharedQueue<BiometricResult>) {
    request_with(azul_core::run_mode::test_hooks(), prompt, results);
}

/// [`request`] under the hooks `hooks` of the run.
fn request_with(
    hooks: &azul_core::run_mode::TestHooks,
    prompt: &BiometricPrompt,
    results: &SharedQueue<BiometricResult>,
) {
    // MWA-C-biometric: never pop a REAL OS auth sheet from a headless /
    // E2E TEST run (the dispatcher is keyed on target_os, not backend — a
    // headless test on a Mac dev box would otherwise raise a live TouchID
    // sheet). Resolve to Unavailable so the round-trip stays observable.
    // Only the hooks of a test build say so (RF38): in a shipped build a
    // variable in the environment (`AZ_BACKEND=headless` is a windowing
    // choice) never answers "unavailable" for the user's sensor, so an
    // app's presence check (AzDrive's key change) sees the real answer.
    if hooks.no_biometric {
        let _ = prompt;
        results.push(BiometricResult::Unavailable);
        return;
    }
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    apple::request(prompt, results);
    #[cfg(target_os = "android")]
    android::request(prompt, results);
    #[cfg(target_os = "windows")]
    windows::request(prompt, results);
    #[cfg(target_os = "linux")]
    linux::request(prompt, results);
    #[cfg(not(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "android",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        let _ = prompt;
        results.push(BiometricResult::Unavailable);
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

#[cfg(test)]
mod tests {
    use azul_core::{
        biometric::{BiometricPrompt, BiometricResult},
        run_mode::TestHooks,
    };
    use azul_layout::managers::shared_queue::SharedQueue;

    /// A test run's prompt shows no sheet and resolves "unavailable" through the normal
    /// result channel. (The other half - a run without the hook reaches the platform's sheet -
    /// cannot be run in a test: it would raise one; `azul-core`'s `tests/run_mode_release.rs`
    /// proves the hook is off in a build without the feature, whatever the environment holds.)
    #[test]
    fn a_test_runs_prompt_resolves_unavailable_without_a_sheet() {
        let results = SharedQueue::new();
        super::request_with(&TestHooks::HEADLESS, &BiometricPrompt::default(), &results);
        assert_eq!(results.drain(), vec![BiometricResult::Unavailable]);
    }
}
