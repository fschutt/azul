//! POD types for the biometric-authentication surface
//! (SUPER_PLAN_2 §1 feature 4 + research/02).
//!
//! Defined here in `azul-core` so the request config and result types
//! can cross the FFI without `azul-layout` having to be a dependency.
//! The stateful side (latest result, sync availability, async result
//! channel) lives in `azul_layout::managers::biometric::BiometricManager`
//! and re-exports these types for the existing import paths.
//!
//! Unlike geolocation (a continuous probe-driven subscription), biometric
//! auth is **request-driven**: a callback asks `App::request_biometric_auth`
//! with a [`BiometricPrompt`]; the OS draws its own modal; the platform
//! backend parks the [`BiometricResult`] in the manager's async channel
//! when the user responds.

use azul_css::AzString;

/// What biometric hardware the device can authenticate with right now.
///
/// This is the *sync availability probe* (iOS `LAContext.biometryType` /
/// `canEvaluatePolicy`; Android `BiometricManager.canAuthenticate`), not
/// the outcome of an auth attempt — that is [`BiometricResult`].
/// `NotAvailable` covers "no sensor", "not enrolled", and "disabled by
/// policy" alike; callers that need to distinguish those use the richer
/// per-attempt [`BiometricResult`] variants.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BiometricKind {
    /// No usable biometric sensor (absent, unenrolled, or disabled).
    NotAvailable,
    /// Fingerprint reader (Touch ID, Android fingerprint, Windows Hello
    /// fingerprint).
    Fingerprint,
    /// Face recognition (Face ID, Android face unlock, Windows Hello face).
    Face,
    /// Iris scanner (Samsung legacy, some Android OEMs).
    Iris,
}

impl Default for BiometricKind {
    fn default() -> Self {
        BiometricKind::NotAvailable
    }
}

impl BiometricKind {
    /// `true` for any real sensor — i.e. anything except `NotAvailable`.
    /// Lets the demo gate decide whether to even offer a biometric unlock.
    pub fn is_available(&self) -> bool {
        !matches!(self, BiometricKind::NotAvailable)
    }
}

/// The outcome of one `request_biometric_auth` attempt, delivered to the
/// caller's completion callback once the OS prompt resolves.
///
/// Maps onto every platform's result enum: iOS `LAError`, Android
/// `BiometricPrompt.AuthenticationCallback`, Windows
/// `UserConsentVerificationResult`, Linux polkit / PAM (research/02 §6).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BiometricResult {
    /// The user matched their face / finger / iris. Unlock granted.
    Authenticated,
    /// The user presented a biometric but it did not match (wrong
    /// finger / face). Distinct from `Cancelled` — the prompt is still
    /// up or retries were exhausted without a deliberate cancel.
    Failed,
    /// The user dismissed the prompt (tapped Cancel / pressed back).
    Cancelled,
    /// Biometrics failed but the user authenticated via the OS passcode
    /// / PIN / device-credential fallback. Still a successful unlock —
    /// only delivered when [`BiometricPrompt::allow_device_credential`]
    /// was set.
    FellBackToPasscode,
    /// No usable biometric is enrolled / available on this device, so
    /// the prompt could not be shown (Linux degraded path, or hardware
    /// absent). Pairs with [`BiometricKind::NotAvailable`].
    Unavailable,
    /// A platform error occurred (sensor busy, lockout, key invalidated,
    /// or an unmapped native error code).
    Error,
}

impl BiometricResult {
    /// `true` when the user successfully unlocked — either by biometric
    /// match (`Authenticated`) or by the OS passcode fallback
    /// (`FellBackToPasscode`). The vault gate keys off this.
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            BiometricResult::Authenticated | BiometricResult::FellBackToPasscode
        )
    }
}

// FFI Option wrapper. `CallbackInfo::get_biometric_result() ->
// Option<BiometricResult>` returns `None` until the first request
// completes; this is the no-codegen prerequisite for that accessor
// (mirrors `OptionLocationFix`). The `availability` accessor returns a
// bare `BiometricKind` (NotAvailable encodes "none"), so no Option there.
impl_option!(
    BiometricResult,
    OptionBiometricResult,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Configuration for one biometric-auth request — what the OS prompt
/// shows and which fallbacks are allowed. Passed to
/// `App::request_biometric_auth`.
///
/// Strings are plain [`AzString`]; an empty string means "use the
/// platform default label" (so callers only override what they care
/// about). This keeps the public surface engine-agnostic and codegen
/// stays a single struct with no nested `Option<String>` wrappers.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiometricPrompt {
    /// Reason shown in the OS prompt — required on iOS
    /// (`localizedReason`; the `NSFaceIDUsageDescription` plist key is
    /// declared separately), shown as the Android subtitle and the
    /// Windows / Linux message line. Empty is accepted but discouraged.
    pub reason: AzString,
    /// Label for the cancel / negative button (Android requires one;
    /// iOS `localizedCancelTitle`). Empty → platform default ("Cancel").
    pub cancel_label: AzString,
    /// Allow the OS passcode / PIN / device-credential fallback when
    /// biometrics fail or aren't enrolled. When the user takes that
    /// path the result is [`BiometricResult::FellBackToPasscode`].
    /// `false` = biometric-only (iOS `…WithBiometrics`, Android
    /// `BIOMETRIC_STRONG` without `DEVICE_CREDENTIAL`).
    pub allow_device_credential: bool,
}

impl Default for BiometricPrompt {
    fn default() -> Self {
        Self {
            reason: AzString::from_const_str(""),
            cancel_label: AzString::from_const_str(""),
            allow_device_credential: false,
        }
    }
}

impl BiometricPrompt {
    /// Convenience constructor: a biometric-only prompt showing `reason`,
    /// with the platform-default cancel label and no passcode fallback.
    pub fn new(reason: AzString) -> Self {
        Self {
            reason,
            ..Self::default()
        }
    }
}
