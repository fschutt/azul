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
#[derive(Default)]
pub enum BiometricKind {
    /// No usable biometric sensor (absent, unenrolled, or disabled).
    #[default]
    NotAvailable,
    /// Fingerprint reader (Touch ID, Android fingerprint, Windows Hello
    /// fingerprint).
    Fingerprint,
    /// Face recognition (Face ID, Android face unlock, Windows Hello face).
    Face,
    /// Iris scanner (Samsung legacy, some Android OEMs).
    Iris,
}


impl BiometricKind {
    /// `true` for any real sensor — i.e. anything except `NotAvailable`.
    /// Lets the demo gate decide whether to even offer a biometric unlock.
    #[must_use] pub const fn is_available(&self) -> bool {
        !matches!(self, Self::NotAvailable)
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
    #[must_use] pub const fn is_success(&self) -> bool {
        matches!(
            self,
            Self::Authenticated | Self::FellBackToPasscode
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
    #[must_use] pub fn new(reason: AzString) -> Self {
        Self {
            reason,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ------------------------------------------------------------------
    // BiometricKind::is_available  (predicate)
    // ------------------------------------------------------------------

    /// basic_true_false: one known-false (`NotAvailable`) and the three
    /// known-true sensor variants return the documented bool.
    #[test]
    fn is_available_known_true_false() {
        assert!(!BiometricKind::NotAvailable.is_available());
        assert!(BiometricKind::Fingerprint.is_available());
        assert!(BiometricKind::Face.is_available());
        assert!(BiometricKind::Iris.is_available());
    }

    /// edge_inputs: `Default::default()` is `NotAvailable`, so the default
    /// gate must report "no biometric offered" — never panics.
    #[test]
    fn is_available_default_is_unavailable() {
        assert_eq!(BiometricKind::default(), BiometricKind::NotAvailable);
        assert!(!BiometricKind::default().is_available());
    }

    /// invariant: `is_available()` is exactly "not the NotAvailable
    /// variant" for every variant — catches a mis-mapped `matches!`.
    #[test]
    fn is_available_iff_not_notavailable() {
        let all = [
            BiometricKind::NotAvailable,
            BiometricKind::Fingerprint,
            BiometricKind::Face,
            BiometricKind::Iris,
        ];
        for k in all {
            assert_eq!(k.is_available(), k != BiometricKind::NotAvailable);
        }
        // Exactly one variant is unavailable.
        let unavailable = all.iter().filter(|k| !k.is_available()).count();
        assert_eq!(unavailable, 1);
    }

    /// The `const fn` really is usable in a const context (compile-time
    /// evaluation) — a regression guard against dropping `const`.
    #[test]
    fn is_available_const_evaluable() {
        const NONE: BiometricKind = BiometricKind::NotAvailable;
        const FACE: BiometricKind = BiometricKind::Face;
        const NONE_AVAIL: bool = NONE.is_available();
        const FACE_AVAIL: bool = FACE.is_available();
        const _: () = assert!(!NONE_AVAIL && FACE_AVAIL);
    }

    // ------------------------------------------------------------------
    // BiometricResult::is_success  (predicate)
    // ------------------------------------------------------------------

    /// basic_true_false + full variant sweep: only `Authenticated` and
    /// `FellBackToPasscode` count as success. In particular `Failed`
    /// (biometric mismatch) and `Cancelled` must NOT unlock the vault.
    #[test]
    fn is_success_per_variant() {
        assert!(BiometricResult::Authenticated.is_success());
        assert!(BiometricResult::FellBackToPasscode.is_success());

        assert!(!BiometricResult::Failed.is_success());
        assert!(!BiometricResult::Cancelled.is_success());
        assert!(!BiometricResult::Unavailable.is_success());
        assert!(!BiometricResult::Error.is_success());
    }

    /// invariant: exactly two of the six variants are successes — a naive
    /// impl that treated `FellBackToPasscode` as failure, or `Failed` as
    /// success, would break this count.
    #[test]
    fn is_success_exactly_two_successes() {
        let all = [
            BiometricResult::Authenticated,
            BiometricResult::Failed,
            BiometricResult::Cancelled,
            BiometricResult::FellBackToPasscode,
            BiometricResult::Unavailable,
            BiometricResult::Error,
        ];
        let successes = all.iter().filter(|r| r.is_success()).count();
        assert_eq!(successes, 2);
        // Every non-success variant is deterministically false.
        for r in all {
            let expect = matches!(
                r,
                BiometricResult::Authenticated | BiometricResult::FellBackToPasscode
            );
            assert_eq!(r.is_success(), expect);
        }
    }

    /// The `const fn` is const-evaluable — regression guard.
    #[test]
    fn is_success_const_evaluable() {
        const OK: BiometricResult = BiometricResult::Authenticated;
        const BAD: BiometricResult = BiometricResult::Failed;
        const OK_S: bool = OK.is_success();
        const BAD_S: bool = BAD.is_success();
        const _: () = assert!(OK_S && !BAD_S);
    }

    // ------------------------------------------------------------------
    // BiometricPrompt::new  (constructor)
    // ------------------------------------------------------------------

    /// invariants_hold: `new` copies `reason` verbatim and applies the
    /// documented defaults (empty cancel label, no device-credential
    /// fallback). Uses AzString content-equality.
    #[test]
    fn new_sets_reason_and_defaults() {
        let reason = AzString::from("Unlock your vault");
        let prompt = BiometricPrompt::new(reason.clone());
        assert_eq!(prompt.reason, reason);
        assert_eq!(prompt.reason.as_str(), "Unlock your vault");
        assert!(prompt.cancel_label.is_empty());
        assert!(!prompt.allow_device_credential);
    }

    /// `new("")` must be indistinguishable from `Default::default()`.
    #[test]
    fn new_empty_equals_default() {
        let from_new = BiometricPrompt::new(AzString::from(""));
        assert_eq!(from_new, BiometricPrompt::default());
        assert!(from_new.reason.is_empty());
    }

    /// no_panic: a multi-megabyte ASCII reason is stored without
    /// truncation or panic.
    #[test]
    fn new_huge_ascii_reason_no_panic() {
        let big = "x".repeat(1_000_000);
        let prompt = BiometricPrompt::new(AzString::from(big.as_str()));
        assert_eq!(prompt.reason.as_str().len(), 1_000_000);
        assert_eq!(prompt.reason.as_str(), big.as_str());
        // Defaults still hold for the extreme input.
        assert!(prompt.cancel_label.is_empty());
        assert!(!prompt.allow_device_credential);
    }

    /// no_panic + unicode: emoji, combining marks, CJK and RTL text
    /// survive byte-for-byte through the constructor.
    #[test]
    fn new_unicode_reason_roundtrips() {
        let weird = "Fingerprint 🔒 verificação — 指紋 مرحبا e\u{0301}";
        let prompt = BiometricPrompt::new(AzString::from(weird));
        assert_eq!(prompt.reason.as_str(), weird);
        assert_eq!(prompt.reason.as_bytes(), weird.as_bytes());
        assert_eq!(prompt.reason.as_str().chars().count(), weird.chars().count());
    }

    /// A multibyte reason repeated to ~1 MB: char count and byte count
    /// stay consistent (no UTF-8 boundary corruption).
    #[test]
    fn new_huge_multibyte_reason() {
        let big = "λ".repeat(500_000); // 2 bytes each
        let prompt = BiometricPrompt::new(AzString::from(big.as_str()));
        assert_eq!(prompt.reason.as_str().chars().count(), 500_000);
        assert_eq!(prompt.reason.as_bytes().len(), 1_000_000);
    }

    /// A reason containing embedded NUL / control bytes must NOT be
    /// truncated at the NUL (guards against C-string-style handling).
    #[test]
    fn new_embedded_nul_and_control_preserved() {
        let s = String::from("before\0mid\tline\nafter\r\u{7}end");
        let prompt = BiometricPrompt::new(AzString::from(s.clone()));
        assert_eq!(prompt.reason.as_str(), s.as_str());
        assert_eq!(prompt.reason.as_bytes().len(), s.len());
        // The NUL is retained, not used as a terminator.
        assert!(prompt.reason.as_bytes().contains(&0));
    }

    /// Boundary reason lengths around common allocation/size thresholds
    /// all round-trip their length exactly.
    #[test]
    fn new_boundary_lengths() {
        for len in [0usize, 1, 2, 15, 16, 17, 255, 256, 4096] {
            let s = "a".repeat(len);
            let prompt = BiometricPrompt::new(AzString::from(s.as_str()));
            assert_eq!(prompt.reason.as_str().len(), len, "len {len} mismatch");
            assert!(!prompt.allow_device_credential);
        }
    }

    /// `new` equals a hand-built struct with the same reason and the
    /// documented default fields — pins the field wiring.
    #[test]
    fn new_matches_manual_construction() {
        let reason = AzString::from("Confirm identity");
        let via_new = BiometricPrompt::new(reason.clone());
        let manual = BiometricPrompt {
            reason,
            cancel_label: AzString::from(""),
            allow_device_credential: false,
        };
        assert_eq!(via_new, manual);
    }

    /// `new` never enables the device-credential fallback, whatever the
    /// reason — it is documented as biometric-only.
    #[test]
    fn new_is_always_biometric_only() {
        let reasons = [
            String::new(),
            "ok".to_string(),
            "🔒".to_string(),
            "z".repeat(10_000),
        ];
        for reason in &reasons {
            let prompt = BiometricPrompt::new(AzString::from(reason.as_str()));
            assert!(!prompt.allow_device_credential);
            assert!(prompt.cancel_label.is_empty());
        }
    }

    /// Clone of a constructed prompt compares equal (Clone/PartialEq
    /// consistency with the deep AzString buffer).
    #[test]
    fn new_clone_is_equal() {
        let prompt = BiometricPrompt::new(AzString::from("clone me"));
        assert_eq!(prompt.clone(), prompt);
    }

    // ------------------------------------------------------------------
    // OptionBiometricResult round-trip (encode == decode)
    // ------------------------------------------------------------------

    /// `Option<BiometricResult>` <-> `OptionBiometricResult` round-trips
    /// losslessly for every variant and for `None`.
    #[test]
    fn option_result_roundtrips() {
        let cases = [
            None,
            Some(BiometricResult::Authenticated),
            Some(BiometricResult::Failed),
            Some(BiometricResult::Cancelled),
            Some(BiometricResult::FellBackToPasscode),
            Some(BiometricResult::Unavailable),
            Some(BiometricResult::Error),
        ];
        for original in cases {
            let wrapped: OptionBiometricResult = original.into();
            assert_eq!(wrapped.is_some(), original.is_some());
            assert_eq!(wrapped.is_none(), original.is_none());
            assert_eq!(wrapped.as_option(), original.as_ref());
            let back: Option<BiometricResult> = wrapped.into();
            assert_eq!(back, original);
        }
    }

    // ------------------------------------------------------------------
    // FFI discriminant stability (`#[repr(C)]` ABI contract)
    //
    // Both enums cross the FFI boundary as C enums; the platform backends
    // (iOS/Android/Windows/Linux) map their native codes onto these
    // discriminants. Re-ordering or inserting a variant silently
    // re-numbers them, which would turn a `Cancelled` into a `Failed` —
    // or worse, an `Error` into an `Authenticated` — for any already
    // compiled shell. These pin the numbering.
    // ------------------------------------------------------------------

    /// `BiometricKind` discriminants are 0..=3 in declaration order, with
    /// `NotAvailable == 0` (so a zeroed C struct means "no sensor").
    #[test]
    fn kind_discriminants_are_stable() {
        assert_eq!(BiometricKind::NotAvailable as u32, 0);
        assert_eq!(BiometricKind::Fingerprint as u32, 1);
        assert_eq!(BiometricKind::Face as u32, 2);
        assert_eq!(BiometricKind::Iris as u32, 3);
        // The zero value is the safe/default one.
        assert_eq!(BiometricKind::default() as u32, 0);
    }

    /// `BiometricResult` discriminants are 0..=5 in declaration order.
    /// Note `Authenticated == 0`: a zeroed result is a *success*, so the
    /// backends must never hand out a default-initialised result.
    #[test]
    fn result_discriminants_are_stable() {
        assert_eq!(BiometricResult::Authenticated as u32, 0);
        assert_eq!(BiometricResult::Failed as u32, 1);
        assert_eq!(BiometricResult::Cancelled as u32, 2);
        assert_eq!(BiometricResult::FellBackToPasscode as u32, 3);
        assert_eq!(BiometricResult::Unavailable as u32, 4);
        assert_eq!(BiometricResult::Error as u32, 5);
    }

    /// encode == decode: variant -> discriminant -> variant is the
    /// identity for every variant, and out-of-range codes (the boundary /
    /// overflow cases a native backend could hand us) decode to `None`
    /// rather than being reinterpreted as a valid variant.
    #[test]
    fn discriminant_roundtrip_and_out_of_range() {
        fn kind_from_u32(d: u32) -> Option<BiometricKind> {
            match d {
                0 => Some(BiometricKind::NotAvailable),
                1 => Some(BiometricKind::Fingerprint),
                2 => Some(BiometricKind::Face),
                3 => Some(BiometricKind::Iris),
                _ => None,
            }
        }
        fn result_from_u32(d: u32) -> Option<BiometricResult> {
            match d {
                0 => Some(BiometricResult::Authenticated),
                1 => Some(BiometricResult::Failed),
                2 => Some(BiometricResult::Cancelled),
                3 => Some(BiometricResult::FellBackToPasscode),
                4 => Some(BiometricResult::Unavailable),
                5 => Some(BiometricResult::Error),
                _ => None,
            }
        }

        for k in [
            BiometricKind::NotAvailable,
            BiometricKind::Fingerprint,
            BiometricKind::Face,
            BiometricKind::Iris,
        ] {
            assert_eq!(kind_from_u32(k as u32), Some(k));
        }
        for r in [
            BiometricResult::Authenticated,
            BiometricResult::Failed,
            BiometricResult::Cancelled,
            BiometricResult::FellBackToPasscode,
            BiometricResult::Unavailable,
            BiometricResult::Error,
        ] {
            assert_eq!(result_from_u32(r as u32), Some(r));
        }

        // Boundary / bogus native codes.
        assert_eq!(kind_from_u32(4), None);
        assert_eq!(kind_from_u32(u32::MAX), None);
        assert_eq!(result_from_u32(6), None);
        assert_eq!(result_from_u32(u32::MAX), None);
    }

    /// Both C enums share one underlying repr, and the FFI Option wrapper
    /// is at least as large as its payload (no niche-packing surprise that
    /// would break the `#[repr(C, u8)]` layout the shells rely on).
    #[test]
    fn repr_layout_invariants() {
        use core::mem::size_of;
        assert_eq!(size_of::<BiometricKind>(), size_of::<BiometricResult>());
        assert!(size_of::<OptionBiometricResult>() >= size_of::<BiometricResult>());
        assert!(size_of::<BiometricKind>() > 0);
    }

    // ------------------------------------------------------------------
    // Ord / Hash / Copy derive invariants
    // ------------------------------------------------------------------

    /// The derived total order follows declaration order (it is the same
    /// order the discriminants encode), and sorting is stable against it.
    #[test]
    fn ord_matches_declaration_order() {
        assert!(BiometricKind::NotAvailable < BiometricKind::Fingerprint);
        assert!(BiometricKind::Fingerprint < BiometricKind::Face);
        assert!(BiometricKind::Face < BiometricKind::Iris);

        let mut kinds = [
            BiometricKind::Iris,
            BiometricKind::NotAvailable,
            BiometricKind::Face,
            BiometricKind::Fingerprint,
        ];
        kinds.sort_unstable();
        assert_eq!(
            kinds,
            [
                BiometricKind::NotAvailable,
                BiometricKind::Fingerprint,
                BiometricKind::Face,
                BiometricKind::Iris,
            ]
        );
        // The one unavailable kind sorts first — `kinds[1..]` are all real
        // sensors.
        assert!(!kinds[0].is_available());
        assert!(kinds[1..].iter().all(BiometricKind::is_available));

        let mut results = [
            BiometricResult::Error,
            BiometricResult::Authenticated,
            BiometricResult::Cancelled,
        ];
        results.sort_unstable();
        assert_eq!(
            results,
            [
                BiometricResult::Authenticated,
                BiometricResult::Cancelled,
                BiometricResult::Error,
            ]
        );
    }

    /// Hash is variant-distinguishing (no two variants collide in a set)
    /// and equal values hash equal — the manager keys cached results by
    /// these types.
    #[test]
    fn hash_is_consistent_and_distinct() {
        use std::{
            collections::{hash_map::DefaultHasher, HashSet},
            hash::{Hash, Hasher},
        };

        fn hash_of<T: Hash>(t: &T) -> u64 {
            let mut h = DefaultHasher::new();
            t.hash(&mut h);
            h.finish()
        }

        let kinds = [
            BiometricKind::NotAvailable,
            BiometricKind::Fingerprint,
            BiometricKind::Face,
            BiometricKind::Iris,
        ];
        let results = [
            BiometricResult::Authenticated,
            BiometricResult::Failed,
            BiometricResult::Cancelled,
            BiometricResult::FellBackToPasscode,
            BiometricResult::Unavailable,
            BiometricResult::Error,
        ];

        assert_eq!(kinds.iter().collect::<HashSet<_>>().len(), 4);
        assert_eq!(results.iter().collect::<HashSet<_>>().len(), 6);

        // Eq => equal hashes (the Hash/Eq contract).
        for k in kinds {
            let copy = k;
            assert_eq!(k, copy);
            assert_eq!(hash_of(&k), hash_of(&copy));
        }
        for r in results {
            let copy = r;
            assert_eq!(r, copy);
            assert_eq!(hash_of(&r), hash_of(&copy));
        }
    }

    /// Both enums are `Copy`: reading a result does not consume it, so a
    /// manager can hand the same value to several callbacks.
    #[test]
    fn enums_are_copy() {
        let r = BiometricResult::FellBackToPasscode;
        let moved = r;
        // `r` is still usable — this only compiles/behaves if `Copy`.
        assert!(r.is_success());
        assert!(moved.is_success());
        assert_eq!(r, moved);

        let k = BiometricKind::Face;
        let moved_k = k;
        assert!(k.is_available());
        assert_eq!(k, moved_k);
    }

    // ------------------------------------------------------------------
    // Documented cross-type pairing
    // ------------------------------------------------------------------

    /// `BiometricResult::Unavailable` is documented to pair with
    /// `BiometricKind::NotAvailable`: neither may unlock anything.
    #[test]
    fn unavailable_pairs_with_not_available() {
        assert!(!BiometricResult::Unavailable.is_success());
        assert!(!BiometricKind::NotAvailable.is_available());
        // Every non-success result must not accidentally be readable as an
        // available sensor kind through the shared discriminant space.
        assert_ne!(
            BiometricResult::Authenticated as u32,
            BiometricResult::Unavailable as u32
        );
    }

    // ------------------------------------------------------------------
    // OptionBiometricResult — the FFI accessor's "no request yet" state
    // ------------------------------------------------------------------

    /// The wrapper defaults to `None` ("no request has completed yet") —
    /// crucially NOT to `Some(Authenticated)`, which discriminant 0 of the
    /// payload would be.
    #[test]
    fn option_result_default_is_none() {
        let d = OptionBiometricResult::default();
        assert!(d.is_none());
        assert!(!d.is_some());
        assert_eq!(d.as_option(), None);
        assert_eq!(Option::<BiometricResult>::from(d), None);
        // A defaulted accessor must never read as a successful unlock.
        assert!(!d.as_option().is_some_and(BiometricResult::is_success));
    }

    /// `replace` has `mem::replace` semantics: it returns the *previous*
    /// value and installs the new one. A vault gate that mis-read the
    /// return value would act on the stale result.
    #[test]
    fn option_result_replace_returns_previous() {
        let mut slot = OptionBiometricResult::None;
        let prev = slot.replace(BiometricResult::Failed);
        assert!(prev.is_none());
        assert_eq!(slot, OptionBiometricResult::Some(BiometricResult::Failed));

        let prev = slot.replace(BiometricResult::Authenticated);
        assert_eq!(prev, OptionBiometricResult::Some(BiometricResult::Failed));
        assert_eq!(
            slot,
            OptionBiometricResult::Some(BiometricResult::Authenticated)
        );
        assert!(slot.as_option().is_some_and(BiometricResult::is_success));
        // The previous (failed) attempt is not a success.
        assert!(!prev.as_option().is_some_and(BiometricResult::is_success));
    }

    /// `map` / `and_then` / `as_ref` / `as_mut` / `into_option` behave
    /// exactly like the std `Option` equivalents for every variant.
    #[test]
    fn option_result_combinators_match_std_option() {
        for original in [
            None,
            Some(BiometricResult::Authenticated),
            Some(BiometricResult::Failed),
            Some(BiometricResult::Cancelled),
            Some(BiometricResult::FellBackToPasscode),
            Some(BiometricResult::Unavailable),
            Some(BiometricResult::Error),
        ] {
            let wrapped: OptionBiometricResult = original.into();

            assert_eq!(wrapped.into_option(), original);
            assert_eq!(wrapped.as_ref(), original.as_ref());
            assert_eq!(
                wrapped.map(|r| r.is_success()),
                original.map(|r| r.is_success())
            );
            assert_eq!(
                wrapped.and_then(|r| r.is_success().then_some(r)),
                original.and_then(|r| r.is_success().then_some(r))
            );

            // as_mut writes through to the wrapper (and is `None` exactly
            // when the source Option was `None` — it must not conjure a
            // slot out of the `None` variant).
            let mut target = wrapped;
            let wrote = target.as_mut().map(|slot| *slot = BiometricResult::Error);
            assert_eq!(wrote.is_some(), original.is_some());
            if original.is_some() {
                assert_eq!(target, OptionBiometricResult::Some(BiometricResult::Error));
            } else {
                assert_eq!(target, OptionBiometricResult::None);
            }
        }
    }

    /// The wrapper's derived order puts `None` before every `Some(_)`, and
    /// `Some(_)` follows the payload order — pins the `#[repr(C, u8)]`
    /// tag ordering that codegen mirrors.
    #[test]
    fn option_result_ord_none_first() {
        assert!(
            OptionBiometricResult::None
                < OptionBiometricResult::Some(BiometricResult::Authenticated)
        );
        assert!(
            OptionBiometricResult::Some(BiometricResult::Authenticated)
                < OptionBiometricResult::Some(BiometricResult::Error)
        );
    }

    // ------------------------------------------------------------------
    // BiometricPrompt — equality / field wiring / string invariants
    // ------------------------------------------------------------------

    /// `PartialEq` is sensitive to *every* field: two prompts that differ
    /// only in the cancel label, only in the reason, or only in the
    /// device-credential flag must not compare equal. A derive that
    /// dropped a field would let a biometric-only prompt compare equal to
    /// a passcode-fallback one.
    #[test]
    fn prompt_eq_is_field_sensitive() {
        let base = BiometricPrompt {
            reason: AzString::from("Unlock"),
            cancel_label: AzString::from("Nope"),
            allow_device_credential: true,
        };

        let mut other_reason = base.clone();
        other_reason.reason = AzString::from("Unlock!");
        assert_ne!(base, other_reason);

        let mut other_cancel = base.clone();
        other_cancel.cancel_label = AzString::from("nope");
        assert_ne!(base, other_cancel);

        let mut other_flag = base.clone();
        other_flag.allow_device_credential = false;
        assert_ne!(base, other_flag);

        // …and an identical rebuild does compare equal (deep AzString eq).
        let same = BiometricPrompt {
            reason: AzString::from("Unlock"),
            cancel_label: AzString::from("Nope"),
            allow_device_credential: true,
        };
        assert_eq!(base, same);
    }

    /// Post-construction length invariants: `len()`, `as_str().len()` and
    /// `as_bytes().len()` agree for ASCII, multibyte, embedded-NUL and
    /// empty reasons — no off-by-one or capacity/len confusion.
    #[test]
    fn prompt_string_len_invariants() {
        let cases = [
            String::new(),
            "a".to_string(),
            "λ🔒指".to_string(),
            String::from("nul\0inside"),
            "z".repeat(70_000),
        ];
        for s in &cases {
            let prompt = BiometricPrompt::new(AzString::from(s.as_str()));
            assert_eq!(prompt.reason.len(), s.len());
            assert_eq!(prompt.reason.as_str().len(), s.len());
            assert_eq!(prompt.reason.as_bytes().len(), s.len());
            assert_eq!(prompt.reason.is_empty(), s.is_empty());
            // The untouched field keeps its documented default.
            assert_eq!(prompt.cancel_label.len(), 0);
            assert!(prompt.cancel_label.is_empty());
        }
    }

    /// No Unicode normalization happens: an NFC "é" and an NFD "e" + U+0301
    /// stay distinct byte sequences (a normalizing string type would make
    /// these two prompts compare equal and silently change what the OS
    /// modal displays).
    #[test]
    fn prompt_does_not_normalize_unicode() {
        let nfc = BiometricPrompt::new(AzString::from("caf\u{e9}"));
        let nfd = BiometricPrompt::new(AzString::from("cafe\u{301}"));
        assert_ne!(nfc, nfd);
        assert_eq!(nfc.reason.as_bytes().len(), 5);
        assert_eq!(nfd.reason.as_bytes().len(), 6);
        assert_eq!(nfc.reason.as_str().chars().count(), 4);
        assert_eq!(nfd.reason.as_str().chars().count(), 5);
    }

    /// The constructor does not trim, collapse or otherwise rewrite the
    /// reason — leading/trailing whitespace survives verbatim.
    #[test]
    fn prompt_preserves_whitespace_verbatim() {
        let raw = "  \t Unlock the vault \n ";
        let prompt = BiometricPrompt::new(AzString::from(raw));
        assert_eq!(prompt.reason.as_str(), raw);
        assert_ne!(prompt.reason.as_str(), raw.trim());
    }

    /// `AzString` round-trips out of a constructed prompt unchanged
    /// (`into_library_owned_string` gives back exactly what went in),
    /// including for a multibyte reason.
    #[test]
    fn prompt_reason_string_roundtrip() {
        for original in [String::new(), "Unlock".to_string(), "🔒λ指紋".to_string()] {
            let prompt = BiometricPrompt::new(AzString::from(original.clone()));
            let back = prompt.reason.clone().into_library_owned_string();
            assert_eq!(back, original);
        }
    }

    /// A prompt whose *both* strings are ~1 MB clones deeply and compares
    /// equal — exercises the deep-copy path for the non-defaulted field
    /// too (the constructor never sets it, so it is built by hand).
    #[test]
    fn prompt_huge_both_strings_clone_deeply() {
        let big_reason = "r".repeat(1_000_000);
        let big_cancel = "c".repeat(1_000_000);
        let prompt = BiometricPrompt {
            reason: AzString::from(big_reason.as_str()),
            cancel_label: AzString::from(big_cancel.as_str()),
            allow_device_credential: true,
        };
        let cloned = prompt.clone();
        assert_eq!(cloned, prompt);
        assert_eq!(cloned.reason.as_str(), big_reason.as_str());
        assert_eq!(cloned.cancel_label.as_str(), big_cancel.as_str());
        assert!(cloned.allow_device_credential);
        // The two buffers did not get aliased/swapped by the clone.
        assert_ne!(cloned.reason, cloned.cancel_label);
    }

    /// `Default::default()` is idempotent and fully empty — two defaults
    /// compare equal and neither enables the passcode fallback.
    #[test]
    fn prompt_default_is_empty_and_stable() {
        let a = BiometricPrompt::default();
        let b = BiometricPrompt::default();
        assert_eq!(a, b);
        assert!(a.reason.is_empty());
        assert!(a.cancel_label.is_empty());
        assert!(!a.allow_device_credential);
        // An empty reason is accepted (documented as "discouraged", not
        // rejected) — the constructor must not panic on it.
        assert_eq!(BiometricPrompt::new(AzString::from_const_str("")), a);
    }

    /// `Debug` renders the reason (used in backend logs); it must not lose
    /// the field or print a placeholder.
    #[test]
    fn prompt_debug_contains_fields() {
        let prompt = BiometricPrompt {
            reason: AzString::from("Unlock vault"),
            cancel_label: AzString::from("Abort"),
            allow_device_credential: true,
        };
        let dbg = format!("{prompt:?}");
        assert!(dbg.contains("BiometricPrompt"), "{dbg}");
        assert!(dbg.contains("Unlock vault"), "{dbg}");
        assert!(dbg.contains("Abort"), "{dbg}");
        assert!(dbg.contains("true"), "{dbg}");
    }
}
