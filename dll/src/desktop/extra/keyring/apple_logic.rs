//! What the Apple keyring DECIDES, without an Apple: the answers the Security and LocalAuthentication frameworks give
//! (an `OSStatus`, an `LAError` code, whether an item is there) turned into the keyring's answers, the record kept
//! beside a protected item, and the access-control flags of a protection level. Compiled on every host, so the table
//! is tested on a Linux runner; `apple.rs` does the calls (iOS + macOS).
//!
//! The model (briefs/mobile_capabilities_ledger_2026-11-28.md, MODEL): a protected item is bound by the OS to a user
//! check (SecAccessControl `.userPresence` / `.biometryCurrentSet` on the keychain); beside it lives an UNPROTECTED
//! META item (service `<service>.azul-meta`, account = key) that outlives it. Meta present and the protected item
//! gone = `Invalidated` (the OS deletes a `biometryCurrentSet` item when the enrolled set changes), not `NotFound`.

// ---- OSStatus codes (`<Security/SecBase.h>`) ---------------------------------------------------------------------
pub(crate) const ERR_SEC_SUCCESS: i32 = 0;
pub(crate) const ERR_SEC_PARAM: i32 = -50;
pub(crate) const ERR_SEC_USER_CANCELED: i32 = -128;
pub(crate) const ERR_SEC_NOT_AVAILABLE: i32 = -25291;
pub(crate) const ERR_SEC_AUTH_FAILED: i32 = -25293;
pub(crate) const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;
pub(crate) const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
pub(crate) const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;
/// A data-protection keychain call from a binary without the `keychain-access-groups` entitlement (an unsigned or
/// ad-hoc macOS build): the item cannot be bound there.
pub(crate) const ERR_SEC_MISSING_ENTITLEMENT: i32 = -34018;

// ---- LAError codes (`<LocalAuthentication/LAError.h>`, domain LAErrorDomain) -----------------------------------------
pub(crate) const LA_AUTHENTICATION_FAILED: i64 = -1;
pub(crate) const LA_USER_CANCEL: i64 = -2;
pub(crate) const LA_USER_FALLBACK: i64 = -3;
pub(crate) const LA_SYSTEM_CANCEL: i64 = -4;
pub(crate) const LA_PASSCODE_NOT_SET: i64 = -5;
pub(crate) const LA_BIOMETRY_NOT_AVAILABLE: i64 = -6;
pub(crate) const LA_BIOMETRY_NOT_ENROLLED: i64 = -7;
pub(crate) const LA_BIOMETRY_LOCKOUT: i64 = -8;
pub(crate) const LA_APP_CANCEL: i64 = -9;
pub(crate) const LA_INVALID_CONTEXT: i64 = -10;
/// `LAErrorNotInteractive`: the context forbids a sheet (`interactionNotAllowed`).
pub(crate) const LA_NOT_INTERACTIVE: i64 = -1004;

/// `SecAccessControlCreateFlags` (`<Security/SecAccessControl.h>`): the constraints of a protection level.
pub(crate) const ACCESS_USER_PRESENCE: usize = 1 << 0;
pub(crate) const ACCESS_BIOMETRY_CURRENT_SET: usize = 1 << 3;

/// The protection levels that bind an item to a user check (`DeviceUnlocked` binds nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    /// Biometric OR the device passcode, on every read; survives an enrolment change.
    UserPresence,
    /// Biometric only, on every read; invalidated when the enrolled set changes.
    Biometric,
}

/// The access-control constraint of a level: what `SecAccessControlCreateWithFlags` is asked for. (The accessibility
/// of such an item is `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly`: no passcode, no item, never a weaker one.)
#[must_use]
pub(crate) const fn access_flags(level: Level) -> usize {
    match level {
        Level::UserPresence => ACCESS_USER_PRESENCE,
        Level::Biometric => ACCESS_BIOMETRY_CURRENT_SET,
    }
}

/// What `LAContext.canEvaluatePolicy` says about the device for a level, from the `LAError` code of a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeviceState {
    /// The device can do the user check.
    Ready,
    /// Too many failures: locked until the passcode is entered.
    LockedOut,
    /// No biometric enrolled / no passcode set: the level cannot be met.
    NotEnrolled,
    /// No sensor, or the policy is not available to this app.
    Unavailable,
}

/// The state of the device from a `canEvaluatePolicy:error:` outcome: `None` = it can, `Some(code)` = the `LAError` code.
#[must_use]
pub(crate) const fn device_state(refusal: Option<i64>) -> DeviceState {
    match refusal {
        None => DeviceState::Ready,
        Some(LA_BIOMETRY_LOCKOUT) => DeviceState::LockedOut,
        Some(LA_BIOMETRY_NOT_ENROLLED | LA_PASSCODE_NOT_SET) => DeviceState::NotEnrolled,
        Some(_) => DeviceState::Unavailable,
    }
}

/// An answer of the keyring (the values of `azul_core::keyring::KeyringResult` this backend produces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The call worked.
    Ok,
    /// The user dismissed the sheet.
    Cancelled,
    /// The biometric did not match, no lockout yet.
    Failed,
    /// Biometrics are locked until the passcode is entered.
    LockedOut,
    /// The level cannot be met: no biometric enrolled, no passcode set.
    NotEnrolled,
    /// The enrolled set changed: the secret is gone, store again.
    Invalidated,
    /// The system refused this app.
    Denied,
    /// No keychain / no entitlement / no sensor.
    Unavailable,
    /// A user check would be needed; none was shown.
    WouldPrompt,
    /// There is no such item.
    NotFound,
    /// Anything else (the code is logged by the caller).
    Error,
}

/// A read of a protected item WITH the user check (`keyring_unlock`): the status the keychain answered after the
/// sheet, the device's own state (asked after a failure) and whether the meta record is there.
#[must_use]
pub(crate) const fn after_unlock(status: i32, device: DeviceState, meta_present: bool) -> Outcome {
    match status {
        ERR_SEC_SUCCESS => Outcome::Ok,
        ERR_SEC_USER_CANCELED => Outcome::Cancelled,
        ERR_SEC_AUTH_FAILED => match device {
            DeviceState::LockedOut => Outcome::LockedOut,
            DeviceState::NotEnrolled => Outcome::NotEnrolled,
            _ => Outcome::Failed,
        },
        ERR_SEC_ITEM_NOT_FOUND => {
            if meta_present {
                Outcome::Invalidated
            } else {
                Outcome::NotFound
            }
        }
        // A sheet was forbidden (or the device is locked and the item needs it unlocked): nothing was shown.
        ERR_SEC_INTERACTION_NOT_ALLOWED => Outcome::WouldPrompt,
        // `errSecParam` on a bound read: the access control cannot be evaluated (no passcode / biometric left).
        ERR_SEC_PARAM => match device {
            DeviceState::NotEnrolled => Outcome::NotEnrolled,
            _ => Outcome::Error,
        },
        ERR_SEC_MISSING_ENTITLEMENT | ERR_SEC_NOT_AVAILABLE => Outcome::Unavailable,
        _ => Outcome::Error,
    }
}

/// A read WITHOUT any user check (`keyring_get`, the quiet statics) of an item the caller holds no secret for: the
/// secret came (`Ok`), or the keychain wanted a sheet it was not allowed to show (`WouldPrompt`), or the item is gone.
#[must_use]
pub(crate) const fn after_plain_read(status: i32, meta_present: bool) -> Outcome {
    match status {
        ERR_SEC_SUCCESS => Outcome::Ok,
        ERR_SEC_ITEM_NOT_FOUND => {
            if meta_present {
                Outcome::Invalidated
            } else {
                Outcome::NotFound
            }
        }
        // The item is bound to a user check (or only the keychain's own dialog would hand it over): not shown.
        ERR_SEC_INTERACTION_NOT_ALLOWED | ERR_SEC_AUTH_FAILED | ERR_SEC_USER_CANCELED => Outcome::WouldPrompt,
        ERR_SEC_MISSING_ENTITLEMENT | ERR_SEC_NOT_AVAILABLE => Outcome::Unavailable,
        _ => Outcome::Error,
    }
}

/// A write of an item (`Store`): `protected` = bound to a user check.
#[must_use]
pub(crate) const fn after_store(status: i32, protected: bool, device: DeviceState) -> Outcome {
    match status {
        ERR_SEC_SUCCESS => Outcome::Ok,
        ERR_SEC_MISSING_ENTITLEMENT | ERR_SEC_NOT_AVAILABLE => Outcome::Unavailable,
        // A bound item cannot be made where the device cannot do the check: refused, never a weaker store.
        ERR_SEC_PARAM | ERR_SEC_AUTH_FAILED if protected => match device {
            DeviceState::Unavailable => Outcome::Unavailable,
            _ => Outcome::NotEnrolled,
        },
        ERR_SEC_USER_CANCELED => Outcome::Cancelled,
        ERR_SEC_INTERACTION_NOT_ALLOWED => Outcome::WouldPrompt,
        _ => Outcome::Error,
    }
}

/// What `Keyring::needs_prompt` answers (the values of `KeyringNeed` this backend produces), from the quiet facts: the
/// level in the meta record (when there is one) and whether the protected item (its attributes, no secret) is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
    NotFound,
    Nothing,
    UserPresence,
    Biometric,
    Invalidated,
}

/// Whether the item is there: asked WITHOUT its secret (the attributes of an item behind a user check are readable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemState {
    Present,
    Absent,
}

#[must_use]
pub(crate) const fn need_of(meta: Option<Level>, item: ItemState) -> Need {
    match (meta, item) {
        (None, ItemState::Present) => Need::Nothing,
        (None, ItemState::Absent) => Need::NotFound,
        (Some(Level::UserPresence), ItemState::Present) => Need::UserPresence,
        (Some(Level::Biometric), ItemState::Present) => Need::Biometric,
        (Some(_), ItemState::Absent) => Need::Invalidated,
    }
}

/// What the biometric gate (`request_biometric_auth`) answers, from an `LAError` code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateOutcome {
    Authenticated,
    Failed,
    Cancelled,
    FellBackToPasscode,
    LockedOut,
    NotEnrolled,
    Unavailable,
    Error,
}

/// The `LAError` code of a failed `evaluatePolicy` as the gate's answer. `userFallback` is the user choosing the
/// fallback button ("Enter password"), which the gate reports as such; a biometric that does not match is `Failed`.
#[must_use]
pub(crate) const fn gate_outcome(code: i64) -> GateOutcome {
    match code {
        LA_USER_CANCEL | LA_SYSTEM_CANCEL | LA_APP_CANCEL => GateOutcome::Cancelled,
        LA_USER_FALLBACK => GateOutcome::FellBackToPasscode,
        LA_AUTHENTICATION_FAILED => GateOutcome::Failed,
        LA_BIOMETRY_LOCKOUT => GateOutcome::LockedOut,
        LA_BIOMETRY_NOT_ENROLLED | LA_PASSCODE_NOT_SET => GateOutcome::NotEnrolled,
        LA_BIOMETRY_NOT_AVAILABLE => GateOutcome::Unavailable,
        _ => GateOutcome::Error,
    }
}

// ---- the meta record ---------------------------------------------------------------------------------------------

/// The text of a meta record: `AZM1:<U|B>` and, after a `:`, the enrolment stamp (the library gate of an unsigned
/// macOS build keeps the hash of `evaluatedPolicyDomainState` there: a changed one = `Invalidated`).
const META_PREFIX: &str = "AZM1:";

#[must_use]
pub(crate) fn encode_meta(level: Level, stamp: Option<&str>) -> Vec<u8> {
    let letter = match level {
        Level::UserPresence => 'U',
        Level::Biometric => 'B',
    };
    match stamp {
        Some(stamp) if !stamp.is_empty() => format!("{META_PREFIX}{letter}:{stamp}").into_bytes(),
        _ => format!("{META_PREFIX}{letter}").into_bytes(),
    }
}

/// The level (and the stamp) of a meta record; `None` for anything that is not one (the item is then no protected
/// item of ours).
#[must_use]
pub(crate) fn decode_meta(bytes: &[u8]) -> Option<(Level, Option<String>)> {
    let text = core::str::from_utf8(bytes).ok()?;
    let rest = text.strip_prefix(META_PREFIX)?;
    let (letter, stamp) = match rest.split_once(':') {
        Some((letter, stamp)) => (letter, Some(stamp)),
        None => (rest, None),
    };
    let level = match letter {
        "U" => Level::UserPresence,
        "B" => Level::Biometric,
        _ => return None,
    };
    Some((level, stamp.filter(|s| !s.is_empty()).map(String::from)))
}

/// The service of the meta item of `service`.
#[must_use]
pub(crate) fn meta_service(service: &str) -> String {
    format!("{service}.azul-meta")
}

#[cfg(test)]
#[path = "apple_logic_test.rs"]
mod apple_logic_test;
