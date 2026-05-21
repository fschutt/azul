//! Windows biometric backend — Windows Hello via WinRT `UserConsentVerifier`.
//!
//! A classic desktop (Win32 HWND) app can't call `RequestVerificationAsync`
//! directly (no CoreWindow) — it must go through `IUserConsentVerifierInterop`
//! with a real top-level HWND. We use the foreground window (the app's window
//! when the user triggers the prompt). Mirrors apple.rs (spawn thread ->
//! push_biometric_result). Availability is opaque about modality, so
//! `Available` reports a generic `Fingerprint`.

use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};
use azul_layout::managers::biometric::push_biometric_result;

use windows::core::{factory, HSTRING};
use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
// windows 0.62's async lives in `windows-future`; its blocking `Async` trait is
// private, so block on the public `IntoFuture` via pollster (a tiny executor —
// the WinRT completion handler drives the waker, so no reactor is needed).
use std::future::IntoFuture;
use windows_future::IAsyncOperation;

/// Synchronous availability probe — no HWND needed.
pub fn probe_availability() -> BiometricKind {
    let avail = (|| -> windows::core::Result<UserConsentVerifierAvailability> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        pollster::block_on(UserConsentVerifier::CheckAvailabilityAsync()?.into_future())
    })();
    match avail {
        Ok(UserConsentVerifierAvailability::Available) => BiometricKind::Fingerprint,
        _ => BiometricKind::NotAvailable,
    }
}

/// Show the Hello prompt; deliver the outcome async via the result channel.
pub fn request(prompt: &BiometricPrompt) {
    let message = HSTRING::from(prompt.reason.as_str());
    std::thread::spawn(move || {
        let result = run(message).unwrap_or(BiometricResult::Error);
        push_biometric_result(result);
    });
}

fn run(message: HSTRING) -> windows::core::Result<BiometricResult> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    // Desktop path: activation factory -> interop -> RequestVerificationForWindowAsync.
    let interop: IUserConsentVerifierInterop =
        factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?;
    // Foreground window = the app's window when the user triggered the prompt.
    let raw = unsafe { winapi::um::winuser::GetForegroundWindow() };
    let hwnd = HWND(raw as *mut core::ffi::c_void);
    // The interop method is generic over the return interface, so name the type.
    let op: IAsyncOperation<UserConsentVerificationResult> =
        unsafe { interop.RequestVerificationForWindowAsync(hwnd, &message)? };
    Ok(match pollster::block_on(op.into_future())? {
        UserConsentVerificationResult::Verified => BiometricResult::Authenticated,
        UserConsentVerificationResult::Canceled => BiometricResult::Cancelled,
        UserConsentVerificationResult::RetriesExhausted => BiometricResult::Failed,
        UserConsentVerificationResult::DeviceBusy => BiometricResult::Error,
        // DeviceNotPresent / NotConfiguredForUser / DisabledByPolicy
        _ => BiometricResult::Unavailable,
    })
}
