//! Apple (iOS + macOS) biometric backend — Face ID / Touch ID / Optic ID
//! via `LAContext` (`LocalAuthentication.framework`, objc2).
//!
//! iOS and macOS expose an identical `LAContext` surface, so this single
//! module backs both (dispatched from `mod.rs` for `any(ios, macos)`).
//!
//! `request` shows the OS-drawn modal (`evaluatePolicy:localizedReason:reply:`)
//! and parks the outcome in the result channel from the reply block —
//! which fires on an arbitrary thread, so it routes through the
//! thread-safe `push_biometric_result` rather than touching any window
//! state directly. `probe_availability` is the synchronous capability
//! check (`canEvaluatePolicy:` + `biometryType`).
//!
//! `biometryType` maps TouchID→Fingerprint, FaceID/OpticID→Face. iOS
//! requires the `NSFaceIDUsageDescription` Info.plist key for Face ID;
//! an unsigned / non-bundled macOS binary may get `NotAvailable` because
//! `LAContext` needs the app code-signed with the right entitlement.

use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};
use azul_layout::managers::biometric::push_biometric_result;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};

// LAPolicy values (`<LocalAuthentication/LAPublicDefines.h>`):
//   1 = DeviceOwnerAuthenticationWithBiometrics (biometric only)
//   2 = DeviceOwnerAuthentication (biometric, falls back to passcode)
// Used as raw `LAPolicy(NSInteger)` to avoid pulling the LAPublicDefines feature.
const POLICY_BIOMETRICS: isize = 1;
const POLICY_WITH_PASSCODE: isize = 2;

/// Show the biometric prompt and deliver the outcome asynchronously via
/// the result channel. Returns immediately.
pub fn request(prompt: &BiometricPrompt) {
    let policy = LAPolicy(if prompt.allow_device_credential {
        POLICY_WITH_PASSCODE
    } else {
        POLICY_BIOMETRICS
    });
    let reason = NSString::from_str(prompt.reason.as_str());

    let ctx: Retained<LAContext> = unsafe { LAContext::new() };
    // Keep the context alive until the (async) reply fires — releasing it
    // early would invalidate the in-flight evaluation.
    let ctx_keepalive = ctx.clone();
    let reply = RcBlock::new(move |success: Bool, error: *mut NSError| {
        let _keep = &ctx_keepalive;
        let result = if success.as_bool() {
            BiometricResult::Authenticated
        } else if error.is_null() {
            BiometricResult::Failed
        } else {
            map_la_error(unsafe { &*error }.code())
        };
        push_biometric_result(result);
    });

    unsafe {
        ctx.evaluatePolicy_localizedReason_reply(policy, &reason, &reply);
    }
}

/// Map an `LAError` code (`NSError.code` in the `LAErrorDomain`) onto a
/// `BiometricResult`. Codes per `<LocalAuthentication/LAError.h>`.
fn map_la_error(code: isize) -> BiometricResult {
    match code {
        // userCancel (-2), systemCancel (-4), appCancel (-9)
        -2 | -4 | -9 => BiometricResult::Cancelled,
        // passcodeNotSet (-5), biometryNotAvailable (-6), biometryNotEnrolled (-7)
        -5 | -6 | -7 => BiometricResult::Unavailable,
        // authenticationFailed (-1), userFallback (-3), biometryLockout (-8), …
        _ => BiometricResult::Failed,
    }
}

/// Synchronous capability probe: can the device evaluate the biometrics
/// policy, and which sensor it has. Returns `NotAvailable` when biometrics
/// can't be evaluated (no sensor / not enrolled / disabled / unsigned app).
pub fn probe_availability() -> BiometricKind {
    let ctx: Retained<LAContext> = unsafe { LAContext::new() };
    let can = unsafe { ctx.canEvaluatePolicy_error(LAPolicy(POLICY_BIOMETRICS)) }.is_ok();
    if !can {
        return BiometricKind::NotAvailable;
    }
    // LABiometryType: None=0, TouchID=1, FaceID=2, OpticID=4.
    match unsafe { ctx.biometryType() }.0 {
        1 => BiometricKind::Fingerprint,
        2 => BiometricKind::Face,
        // OpticID has no dedicated Azul kind; Face is the closest match.
        4 => BiometricKind::Face,
        _ => BiometricKind::NotAvailable,
    }
}
