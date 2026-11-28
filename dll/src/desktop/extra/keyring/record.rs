//! The pure pieces the platform backends build their protected items from: the record that says
//! an item's level without a user check, the name of an item's Windows Hello key, and what the
//! numbers Windows Hello answers with mean. Compiled and tested on every host, so a runner without
//! Windows Hello still runs what can be run.

use azul_core::{
    biometric::BiometricResult,
    keyring::{KeyringProtection, KeyringResult},
};

/// What a Windows credential's comment starts with when the item is protected: the format
/// version and the level's number ([`KeyringProtection::code`]). A credential with no such
/// comment (one made before the levels, or by another program) is `DeviceUnlocked`.
pub(crate) const COMMENT_PREFIX: &str = "azul-keyring:v1;p=";

/// The comment of an item guarded as `protection`; `None` for the unprotected one, which has no
/// record (it is the plain credential every azul app wrote before the levels).
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) fn comment_of(protection: KeyringProtection) -> Option<String> {
    protection
        .needs_user()
        .then(|| format!("{COMMENT_PREFIX}{}", protection.code()))
}

/// The level a credential's comment records (`DeviceUnlocked` for none, or one that is no record
/// of ours: an unknown level is never read as a weaker one than the number says, and a text that
/// does not parse is a credential of another program).
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) fn protection_of_comment(comment: Option<&str>) -> KeyringProtection {
    comment
        .and_then(|comment| comment.strip_prefix(COMMENT_PREFIX))
        .and_then(|number| number.trim().parse::<u8>().ok())
        .and_then(KeyringProtection::from_code)
        .unwrap_or(KeyringProtection::DeviceUnlocked)
}

/// The fixed data the Windows Hello key signs: the signature (RSA PKCS#1 v1.5, which is
/// deterministic: the same key and data give the same bytes) is the entropy the secret is sealed
/// with, so it can be opened again only after the user's Hello gesture has produced it.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) const HELLO_CHALLENGE: &[u8] = b"azul-keyring-hello-challenge-v1";

/// The name of the Hello key of the item `key` in `service`. Hello keys are named by an account
/// id of limited length, so the name is a hash of both; two items sharing a name would share a
/// Hello key, which is harmless: the key only asks for the user's gesture.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) fn hello_account(service: &str, key: &str) -> String {
    // 64-bit FNV-1a over `service NUL key`: a name, not a secret.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in service.bytes().chain(core::iter::once(0)).chain(key.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("azul-keyring-{hash:016x}")
}

/// What a `KeyCredentialStatus` (the number Windows Hello answers a create, an open or a sign
/// with) means for the keyring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) enum HelloStatus {
    /// `Success`: the key is there / the data is signed.
    Success,
    /// `UserCanceled` and `UserPrefersPassword`: the user dismissed the Hello sheet or chose to
    /// type a password instead.
    Cancelled,
    /// `SecurityDeviceLocked`: the TPM / security device is locked out (too many wrong PINs).
    LockedOut,
    /// `NotFound`: there is no key of that name (Hello was reset, or the item's key was removed).
    NotFound,
    /// `UnknownError`, `CredentialAlreadyExists` and anything Windows adds later.
    Other,
}

/// The meaning of a `KeyCredentialStatus` number: 0 Success, 1 UnknownError, 2 NotFound,
/// 3 UserCanceled, 4 UserPrefersPassword, 5 CredentialAlreadyExists, 6 SecurityDeviceLocked.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) const fn hello_status(code: i32) -> HelloStatus {
    match code {
        0 => HelloStatus::Success,
        2 => HelloStatus::NotFound,
        3 | 4 => HelloStatus::Cancelled,
        6 => HelloStatus::LockedOut,
        _ => HelloStatus::Other,
    }
}


// ───────── The user check of the library gate (Linux; macOS builds that cannot bind) ──────────

/// The suffix of the key of an item's record: the record is a sibling item (a secret of its own,
/// readable without a user check) where the platform's store has no room for it beside the
/// secret. The unit separator is in no name an app picks.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) const META_SUFFIX: &str = "\u{1f}azul-meta";

/// The key of the record of the item `key`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn meta_key(key: &str) -> String {
    format!("{key}{META_SUFFIX}")
}

/// What a library-gated item's record says: its level and, for `Biometric`, the digest of the
/// fingers that were enrolled when it was stored.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) struct GateRecord {
    pub protection: KeyringProtection,
    pub enrolled: Option<String>,
}

impl GateRecord {
    /// The record as the text that is stored: `v1;p=<level>` and, when there is one, `;f=<digest>`.
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    pub(crate) fn text(&self) -> String {
        match &self.enrolled {
            Some(digest) => format!("v1;p={};f={digest}", self.protection.code()),
            None => format!("v1;p={}", self.protection.code()),
        }
    }

    /// The record a text holds; `None` for a text that is no record of ours (an item with no
    /// record is read as unprotected).
    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split(';');
        if parts.next()? != "v1" {
            return None;
        }
        let protection = parts
            .next()?
            .strip_prefix("p=")?
            .parse::<u8>()
            .ok()
            .and_then(KeyringProtection::from_code)?;
        let enrolled = parts
            .next()
            .and_then(|part| part.strip_prefix("f="))
            .map(str::to_string);
        Some(Self {
            protection,
            enrolled,
        })
    }
}

/// What the fingerprint service says about the enrolled fingers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) enum Fingers {
    /// No fingerprint service or reader at all.
    Absent,
    /// A reader with no finger enrolled.
    None,
    /// The enrolled fingers, as a digest of their names (it changes when one is added or removed).
    Enrolled(String),
}

/// The digest of the enrolled fingers' names, whatever order the service lists them in.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn fingers_digest(names: &[String]) -> String {
    let mut sorted: Vec<&str> = names.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for name in sorted {
        for byte in name.bytes().chain(core::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// What the desktop's authorisation service answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) enum Authorization {
    /// The user proved who they are (password or fingerprint, as the system is set up).
    Granted,
    /// The user dismissed the dialog, or did not prove it.
    Cancelled,
    /// The service does not know the action: its policy file is not installed.
    Unregistered,
    /// No authorisation service or no agent to ask the user.
    Unavailable,
    /// Any other answer.
    Error,
}

/// The services the library gate asks the user through: a fingerprint reader and the desktop's
/// authorisation dialog. A trait so the decision below is tested without either.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) trait UserCheck {
    /// The enrolled fingers, asked quietly.
    fn fingers(&self) -> Fingers;
    /// One fingerprint verification (the reader waits for a finger).
    fn verify_finger(&self) -> BiometricResult;
    /// The desktop's authorisation dialog, with `reason` as its text.
    fn authorize(&self, reason: &str) -> Authorization;
}

/// The library gate's decision before a protected item is read: `Ok` when the user's check
/// passed, else the keyring's answer. `UserPresence` goes through the desktop's authorisation
/// dialog (a password, or a fingerprint where the system's PAM takes one); `Biometric` through
/// the fingerprint reader, and the fingers enrolled must still be the ones the item was stored
/// under (`record.enrolled`), else it is `Invalidated`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn run_check(
    record: &GateRecord,
    check: &dyn UserCheck,
    reason: &str,
) -> Result<(), KeyringResult> {
    match record.protection {
        KeyringProtection::DeviceUnlocked => Ok(()),
        KeyringProtection::UserPresence => match check.authorize(reason) {
            Authorization::Granted => Ok(()),
            Authorization::Cancelled => Err(KeyringResult::Cancelled),
            Authorization::Unregistered => Err(KeyringResult::NotEnrolled),
            Authorization::Unavailable => Err(KeyringResult::Unavailable),
            Authorization::Error => Err(KeyringResult::Error),
        },
        KeyringProtection::Biometric => {
            match check.fingers() {
                Fingers::Absent => return Err(KeyringResult::Unavailable),
                Fingers::None => return Err(KeyringResult::NotEnrolled),
                Fingers::Enrolled(now) => {
                    if record.enrolled.as_deref().is_some_and(|then| then != now) {
                        return Err(KeyringResult::Invalidated);
                    }
                }
            }
            match check.verify_finger() {
                BiometricResult::Authenticated | BiometricResult::FellBackToPasscode => Ok(()),
                BiometricResult::Failed => Err(KeyringResult::Failed),
                BiometricResult::Cancelled => Err(KeyringResult::Cancelled),
                BiometricResult::LockedOut => Err(KeyringResult::LockedOut),
                BiometricResult::NotEnrolled => Err(KeyringResult::NotEnrolled),
                BiometricResult::Unavailable => Err(KeyringResult::Unavailable),
                BiometricResult::Error => Err(KeyringResult::Error),
            }
        }
    }
}

/// What a read of an item with `record` needs, from the record and the fingers alone.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn need_of_record(record: &GateRecord, fingers: &Fingers) -> azul_core::keyring::KeyringNeed {
    use azul_core::keyring::KeyringNeed;
    match record.protection {
        KeyringProtection::DeviceUnlocked => KeyringNeed::Nothing,
        KeyringProtection::UserPresence => KeyringNeed::UserPresence,
        KeyringProtection::Biometric => match (fingers, &record.enrolled) {
            (Fingers::Enrolled(now), Some(then)) if now != then => KeyringNeed::Invalidated,
            _ => KeyringNeed::Biometric,
        },
    }
}

#[cfg(test)]
#[path = "record_test.rs"]
mod record_test;
