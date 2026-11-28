//! Apple (iOS + macOS) biometric backend — Face ID / Touch ID / Optic ID
//! via `LAContext` (`LocalAuthentication.framework`, objc2).
//!
//! iOS and macOS expose an identical `LAContext` surface, so this single
//! module backs both (dispatched from `mod.rs` for `any(ios, macos)`).
//!
//! `request` shows the OS-drawn modal (`evaluatePolicy:localizedReason:reply:`)
//! and parks the outcome in the result channel from the reply block —
//! which fires on an arbitrary thread, so it routes through the
//! thread-safe `results` queue (the app's, a clone of which the block
//! holds) rather than touching any window state directly. `probe_availability` is the synchronous capability
//! check (`canEvaluatePolicy:` + `biometryType`).
//!
//! `biometryType` maps TouchID→Fingerprint, FaceID/OpticID→Face. iOS
//! requires the `NSFaceIDUsageDescription` Info.plist key for Face ID;
//! an unsigned / non-bundled macOS binary may get `NotAvailable` because
//! `LAContext` needs the app code-signed with the right entitlement.

use azul_core::biometric::{BiometricKind, BiometricPrompt, BiometricResult};
use azul_layout::managers::shared_queue::SharedQueue;
use block2::RcBlock;
use objc2::{rc::Retained, runtime::Bool};
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};

use crate::desktop::extra::keyring::apple_logic::{gate_outcome, GateOutcome};

// LAPolicy values (`<LocalAuthentication/LAPublicDefines.h>`):
//   1 = DeviceOwnerAuthenticationWithBiometrics (biometric only)
//   2 = DeviceOwnerAuthentication (biometric, falls back to passcode)
// Used as raw `LAPolicy(NSInteger)` to avoid pulling the LAPublicDefines feature.
const POLICY_BIOMETRICS: isize = 1;
const POLICY_WITH_PASSCODE: isize = 2;

/// Show the biometric prompt and deliver the outcome asynchronously via
/// the result channel. Returns immediately.
pub fn request(prompt: &BiometricPrompt, results: &SharedQueue<BiometricResult>) {
    let results = results.clone();
    let policy = LAPolicy(if prompt.allow_device_credential {
        POLICY_WITH_PASSCODE
    } else {
        POLICY_BIOMETRICS
    });
    // Never an empty reason: LocalAuthentication raises NSInvalidArgumentException for one (an abort through Rust frames).
    let reason = NSString::from_str(&crate::desktop::extra::info_plist::prompt_reason_or_default(prompt.reason.as_str()));
    // Face ID needs the purpose string; without it the system does not use Face ID (no crash), which this says out loud.
    #[cfg(target_os = "ios")]
    if crate::desktop::extra::info_plist::is_bundled()
        && !crate::desktop::extra::info_plist::has_key(crate::desktop::extra::info_plist::keys::FACE_ID)
    {
        crate::log_warn!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[biometric] this app's Info.plist has no {}: Face ID is not used (Touch ID and the passcode still are)",
            crate::desktop::extra::info_plist::keys::FACE_ID
        );
    }

    let ctx: Retained<LAContext> = unsafe { LAContext::new() };
    // The sheet's own words: the cancel button's label when the app gave one, and no "Enter Password" fallback button
    // on a biometrics-only prompt (it answered -3 and was reported as a failed match).
    unsafe {
        if !prompt.cancel_label.as_str().is_empty() {
            ctx.setLocalizedCancelTitle(Some(&NSString::from_str(prompt.cancel_label.as_str())));
        }
        if !prompt.allow_device_credential {
            ctx.setLocalizedFallbackTitle(Some(&NSString::from_str("")));
        }
    }
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
        results.push(result);
    });

    unsafe {
        ctx.evaluatePolicy_localizedReason_reply(policy, &reason, &reply);
    }
}

/// Map an `LAError` code (`NSError.code` in the `LAErrorDomain`) onto a `BiometricResult`, one cause per answer (the
/// table is `keyring/apple_logic.rs::gate_outcome`, tested on every host): a cancelled sheet, a finger that did not
/// match, a lockout, a missing enrolment and a missing sensor are five different things an app answers differently
/// (the old mapping said Unavailable for "not enrolled" and "no passcode" and Failed for a lockout and the fallback).
fn map_la_error(code: isize) -> BiometricResult {
    match gate_outcome(code as i64) {
        GateOutcome::Authenticated => BiometricResult::Authenticated,
        GateOutcome::Failed => BiometricResult::Failed,
        GateOutcome::Cancelled => BiometricResult::Cancelled,
        GateOutcome::FellBackToPasscode => BiometricResult::FellBackToPasscode,
        GateOutcome::LockedOut => BiometricResult::LockedOut,
        GateOutcome::NotEnrolled => BiometricResult::NotEnrolled,
        GateOutcome::Unavailable => BiometricResult::Unavailable,
        GateOutcome::Error => BiometricResult::Error,
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
