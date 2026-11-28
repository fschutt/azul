//! POD types for the system-keyring surface
//! (SUPER_PLAN_2 §4 P4.2 + research/02 §0 "hardware-bound" storage).
//!
//! A secret store backed by the OS keyring: iOS/macOS Keychain (`SecItem*`,
//! optionally a `SecAccessControl` of `userPresence` / `biometryCurrentSet`), Android
//! `KeyStore` (a key with `setUserAuthenticationRequired`, opened through a
//! `BiometricPrompt` `CryptoObject`), Linux libsecret, Windows Credential Manager +
//! the Windows Hello key. Defined here in `azul-core` so the request/result types
//! cross the FFI without `azul-layout` being a dependency; the stateful side lives in
//! `azul_layout::managers::keyring::KeyringManager`.
//!
//! One model on every platform (the guide has the table: platform x protection x
//! enforced-by):
//!
//! - A secret has a [`KeyringProtection`]: `DeviceUnlocked` (never a sheet), `UserPresence` (a
//!   biometric OR the device passcode on every read), `Biometric` (a biometric only, invalidated
//!   when the enrolled set changes).
//! - The user-check sheet appears only for [`KeyringRequest::Unlock`] and a protected
//!   [`KeyringRequest::Store`]; a `Get`, a worker thread's blocking call and a quiet call on a
//!   protected item answer [`KeyringResult::WouldPrompt`] without showing anything.
//! - The answer says why: [`KeyringResult::Cancelled`] (the user dismissed the sheet),
//!   `Failed` (no match), `LockedOut`, `NotEnrolled`, `Invalidated` (the enrolled set changed:
//!   store again), `Denied` (the system refused this app), `WouldPrompt` (a check would be
//!   needed, none was shown).
//! - [`KeyringNeed`] (what a read of this key needs) and [`KeyringSupport`] (what this device
//!   does for a level, in hardware or in the library) are quiet probes, so an app can explain
//!   BEFORE the OS sheet.
//!
//! Request-driven and channel-delivered, mirroring biometric ([`crate::
//! biometric`]): an `Unlock` of a protected item shows the OS prompt and
//! resolves asynchronously, so *every* op resolves through the result
//! channel for a uniform, engine-agnostic surface. One op is in flight at
//! a time (the demo reveals one entry at a time); request↔result
//! correlation by id is a future refinement.

use alloc::string::ToString;

use azul_css::AzString;

/// Which keyring service an entry lives in.
///
/// An app sees only the entries of the namespaces it names. The default (an empty service) is the app's own id
/// (`AppConfig::app_id`, or what its bundle / package declares), so two azul
/// apps never read each other's secrets by accident; apps that share entries
/// (the Azlin apps, `io.azlin.shared`) name the same service.
///
/// On macOS and iOS the service is the Keychain item's service, on Linux the
/// libsecret schema, on Windows the credential's target prefix. Android keeps
/// every app's entries in the app's own storage already, so there the
/// namespace changes nothing. On Linux and Windows any program of the same
/// user can still read every entry: a namespace stops accidental sharing, not
/// a hostile program.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct KeyringNamespace {
    /// The service name (reverse-DNS); empty: the app's own id.
    pub service: AzString,
}

impl KeyringNamespace {
    /// The service every azul app shared before namespaces existed. An entry
    /// still there moves into the namespace that first asks for its name.
    pub const LEGACY_SERVICE: &'static str = "com.azul.keyring";

    /// The namespace `service` (reverse-DNS, `io.azlin.shared`); an empty one
    /// is the app's own.
    #[must_use]
    pub fn create(service: AzString) -> Self {
        let trimmed = service.as_str().trim();
        if trimmed.len() == service.as_str().len() {
            Self { service }
        } else {
            Self {
                service: AzString::from(trimmed.to_string()),
            }
        }
    }

    /// The app's own namespace: its id, which the platform layer fills in.
    #[must_use]
    pub const fn app() -> Self {
        Self {
            service: AzString::from_const_str(""),
        }
    }

    /// Whether this is the app's own namespace (no service named).
    #[must_use]
    pub fn is_app(&self) -> bool {
        self.service.as_str().trim().is_empty()
    }
}

/// How a stored secret is guarded when it is read back. Chosen when it is stored and kept with the
/// item (a record readable without a user check says which level it has: [`KeyringNeed`]).
///
/// The brief called the first level `None`; it is `DeviceUnlocked` here because `None` is a macro
/// of X11 (the C binding) and a keyword of Python.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum KeyringProtection {
    /// Readable while the user's session / the device is unlocked, and by the app that stored
    /// it. Never shows a sheet.
    #[default]
    DeviceUnlocked,
    /// The user must be present on every read: a biometric OR the device passcode / PIN /
    /// password. Survives a change of the enrolled biometrics (the passcode still works).
    UserPresence,
    /// A biometric match is required on every read, with no passcode fallback. The item is
    /// invalidated ([`KeyringResult::Invalidated`]) when the enrolled set changes: a finger or a
    /// face is added or removed.
    Biometric,
}

impl KeyringProtection {
    /// `true` for the levels that need the user on every read.
    #[must_use]
    pub const fn needs_user(self) -> bool {
        !matches!(self, Self::DeviceUnlocked)
    }

    /// The level as the number the backends keep in an item's record (0, 1, 2).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::DeviceUnlocked => 0,
            Self::UserPresence => 1,
            Self::Biometric => 2,
        }
    }

    /// The level of a record's number; `None` for a number no level has.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::DeviceUnlocked),
            1 => Some(Self::UserPresence),
            2 => Some(Self::Biometric),
            _ => None,
        }
    }
}

/// The text of the OS sheet an op may show (iOS `localizedReason` and `localizedCancelTitle`,
/// Android `PromptInfo`, the Windows Hello message, the polkit message). An empty `reason` is
/// "Unlock <the app's name>"; an empty `cancel_label` is the platform's own, localised "Cancel".
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyringPrompt {
    /// Why the app wants the secret, as the sheet's main line ("Unlock your vault").
    pub reason: AzString,
    /// The sheet's cancel button ("Use password"); empty: the platform's own.
    pub cancel_label: AzString,
}

impl KeyringPrompt {
    /// A prompt that shows `reason` and the platform's own cancel button.
    #[must_use]
    pub fn create(reason: AzString) -> Self {
        Self {
            reason,
            cancel_label: AzString::from_const_str(""),
        }
    }

    /// The same prompt with `cancel_label` on the sheet's cancel button.
    #[must_use]
    pub fn with_cancel_label(mut self, cancel_label: AzString) -> Self {
        self.cancel_label = cancel_label;
        self
    }
}

/// A keyring operation queued by a callback
/// (`CallbackInfo::keyring_store` / `keyring_store_protected` / `keyring_get` /
/// `keyring_unlock` / `keyring_delete`) and dispatched to the platform backend by the layout pass.
///
/// `secret` is an [`AzString`] — the common case is a password / token;
/// binary blobs are base64-encoded by the caller. `key` is the lookup
/// name, scoped to a [`KeyringNamespace`] (the app's own unless the call
/// names one).
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringRequest {
    /// Write `secret` under `key`, overwriting any existing value, guarded as `protection` says.
    /// A level above `DeviceUnlocked` may show the sheet (`prompt`: the Windows Hello key is
    /// made behind a gesture) and is refused with `NotEnrolled` / `Unavailable` when the device
    /// cannot meet it: an item is never stored weaker than asked.
    Store {
        key: AzString,
        secret: AzString,
        protection: KeyringProtection,
        prompt: KeyringPrompt,
    },
    /// Read the secret stored under `key` without the user: an unprotected item answers
    /// `Retrieved`, a protected one `WouldPrompt` (no sheet), a protected one whose enrolled set
    /// changed `Invalidated`.
    Get { key: AzString },
    /// Read the secret stored under `key` behind the user check its protection asks (the only
    /// read that shows the sheet, with `prompt`). An unprotected item is read plainly.
    Unlock { key: AzString, prompt: KeyringPrompt },
    /// Remove the item stored under `key` and its record (no-op if absent); never a sheet.
    Delete { key: AzString },
}

impl KeyringRequest {
    /// The key the op names.
    #[must_use]
    pub const fn key(&self) -> &AzString {
        match self {
            Self::Store { key, .. }
            | Self::Get { key }
            | Self::Unlock { key, .. }
            | Self::Delete { key } => key,
        }
    }

    /// `true` for the ops that may show the user-check sheet: an `Unlock`, and a `Store` of a
    /// protected level.
    #[must_use]
    pub const fn may_show_sheet(&self) -> bool {
        match self {
            Self::Store { protection, .. } => protection.needs_user(),
            Self::Unlock { .. } => true,
            Self::Get { .. } | Self::Delete { .. } => false,
        }
    }
}

#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
/// The outcome of a [`KeyringRequest`], delivered to the result channel
/// and read by callbacks via `CallbackInfo::get_keyring_result()`.
///
/// The first seven variants are the ones of the first version and keep their numbers; the six
/// after `Error` say WHY an op did not go through, where `Denied` once said all of it.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringResult {
    /// A `Store` succeeded.
    Stored,
    /// A `Get` / `Unlock` returned the secret.
    Retrieved(AzString),
    /// A `Delete` succeeded (the key is now absent).
    Deleted,
    /// The requested key was not present in the keyring.
    NotFound,
    /// The SYSTEM refused this app: another app's item, an access-control list, a policy. Never
    /// "the user cancelled" ([`Self::Cancelled`]) and never "a check would be needed"
    /// ([`Self::WouldPrompt`]).
    Denied,
    /// No keyring backend is available on this platform / it isn't
    /// configured (e.g. Linux without a running secret service).
    Unavailable,
    /// A platform error occurred (locked keychain, I/O, unmapped code); the backend logs the
    /// OS code.
    Error,
    /// The user dismissed the sheet (cancel, back, the negative button, the system cancelled it).
    Cancelled,
    /// The biometric did not match and the tries are used up; no lockout yet.
    Failed,
    /// Too many failures: biometrics are locked until the passcode is entered.
    LockedOut,
    /// The protection cannot be met or bound: no biometric enrolled, no passcode / PIN set, no
    /// Windows Hello key. The item was NOT stored weaker.
    NotEnrolled,
    /// The enrolled biometric set changed (or the key was permanently invalidated): the secret
    /// is gone, store it again.
    Invalidated,
    /// A user check (or, for a quiet op, the system keychain's own dialog) would be needed;
    /// none was shown. Wait for the user's own action, then `Unlock`.
    WouldPrompt,
}

impl KeyringResult {
    /// The retrieved secret, if this is a successful `Get`.
    #[must_use]
    pub const fn secret(&self) -> Option<&AzString> {
        match self {
            Self::Retrieved(s) => Some(s),
            _ => None,
        }
    }

    /// `true` for the success outcomes (`Stored` / `Retrieved` / `Deleted`).
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Stored | Self::Retrieved(_) | Self::Deleted)
    }

    /// `true` when the USER'S check did not pass: they cancelled, the biometric did not match,
    /// or biometrics are locked out. The app offers its own fallback (the password).
    #[must_use]
    pub const fn is_user_refusal(&self) -> bool {
        matches!(self, Self::Cancelled | Self::Failed | Self::LockedOut)
    }

    /// `true` when the op needs the user and was not allowed to ask: the app calls `Unlock`
    /// from the user's next action.
    #[must_use]
    pub const fn needs_user(&self) -> bool {
        matches!(self, Self::WouldPrompt)
    }
}

/// What a read of a key needs, asked quietly (never a sheet, from the item's record alone):
/// what an app looks at BEFORE it calls `Unlock`, to explain the sheet that will come.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum KeyringNeed {
    /// Nothing is stored under the key (a read answers `NotFound`).
    #[default]
    NotFound,
    /// A read answers at once, no sheet (`DeviceUnlocked`).
    Nothing,
    /// The system keyring itself is locked (a locked libsecret collection, a locked keychain): a
    /// read shows the system's own unlock dialog unless it is quiet.
    Unlock,
    /// `Unlock` shows a sheet that takes a biometric OR the device passcode.
    UserPresence,
    /// `Unlock` shows a sheet that takes a biometric only.
    Biometric,
    /// The item can no longer be opened (the enrolled set changed): store it again.
    Invalidated,
    /// No keyring backend here.
    Unavailable,
    /// The probe itself failed (the backend logs why).
    Error,
}

/// What this device does for a [`KeyringProtection`] level, asked quietly. The honest answer to
/// "is my secret held by hardware?": the guide has the per-platform table.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum KeyringSupport {
    /// The OS or the secure hardware (Secure Enclave, the Keystore's TEE / `StrongBox`, a TPM)
    /// releases the secret only after the user check.
    Hardware,
    /// azul runs the user check and then reads an item that is not itself bound to it: a
    /// boolean gate. Another process of the same user can read the item without the check.
    Library,
    /// The device has the means but nothing is set up: no biometric enrolled, no passcode / PIN,
    /// no Hello key.
    NotEnrolled,
    /// This device cannot do it: no sensor, no backend, or the platform has no such level (Windows
    /// Hello cannot require a biometric over the PIN).
    #[default]
    Unavailable,
}

// FFI Option wrapper for `CallbackInfo::get_keyring_result() ->
// Option<KeyringResult>` — `None` until the first op completes. Not Copy
// (carries an `AzString` in `Retrieved`), so `copy = false` (mirrors
// `OptionNodeType`).
impl_option!(
    KeyringResult,
    OptionKeyringResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

#[cfg(test)]
#[path = "keyring_test.rs"]
mod keyring_test;
