//! POD types for the system-keyring surface
//! (SUPER_PLAN_2 §4 P4.2 + research/02 §0 "hardware-bound" storage).
//!
//! A biometry-bindable secret key/value store backed by the OS keyring:
//! iOS/macOS Keychain (`SecItem*`, optionally `kSecAttrAccessControl =
//! biometryCurrentSet`), Android `KeyStore` (`setUserAuthenticationRequired`),
//! Linux libsecret, Windows `CredentialLocker`. Defined here in `azul-core`
//! so the request/result types cross the FFI without `azul-layout` being a
//! dependency; the stateful side lives in
//! `azul_layout::managers::keyring::KeyringManager`.
//!
//! Request-driven and channel-delivered, mirroring biometric ([`crate::
//! biometric`]): a `Get` of a biometry-bound item shows the OS prompt and
//! resolves asynchronously, so *every* op resolves through the result
//! channel for a uniform, engine-agnostic surface. One op is in flight at
//! a time (the demo reveals one entry at a time); request↔result
//! correlation by id is a future refinement.

use azul_css::AzString;

/// A keyring operation queued by a callback
/// (`CallbackInfo::keyring_store` / `keyring_get` / `keyring_delete`) and
/// dispatched to the platform backend by the layout pass.
///
/// `secret` is an [`AzString`] — the common case is a password / token;
/// binary blobs are base64-encoded by the caller. `key` is the lookup
/// name, scoped to the app's keyring service.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum KeyringRequest {
    /// Write `secret` under `key`, overwriting any existing value. When
    /// `require_biometry` is set the item is stored access-controlled so a
    /// later `Get` triggers the OS biometric prompt (Keychain
    /// `biometryCurrentSet` / KeyStore `setUserAuthenticationRequired`).
    Store {
        key: AzString,
        secret: AzString,
        require_biometry: bool,
    },
    /// Read the secret stored under `key`. For a biometry-bound item the
    /// OS shows its auth prompt first; the result arrives asynchronously.
    Get { key: AzString },
    /// Remove the item stored under `key` (no-op if absent).
    Delete { key: AzString },
}

/// The outcome of a [`KeyringRequest`], delivered to the result channel
/// and read by callbacks via `CallbackInfo::get_keyring_result()`.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum KeyringResult {
    /// A `Store` succeeded.
    Stored,
    /// A `Get` returned the secret.
    Retrieved(AzString),
    /// A `Delete` succeeded (the key is now absent).
    Deleted,
    /// The requested key was not present in the keyring.
    NotFound,
    /// A biometry-bound read was refused — the user failed or cancelled
    /// the OS auth prompt.
    Denied,
    /// No keyring backend is available on this platform / it isn't
    /// configured (e.g. Linux without a running secret service).
    Unavailable,
    /// A platform error occurred (locked keychain, I/O, unmapped code).
    Error,
}

impl KeyringResult {
    /// The retrieved secret, if this is a successful `Get`.
    pub fn secret(&self) -> Option<&AzString> {
        match self {
            KeyringResult::Retrieved(s) => Some(s),
            _ => None,
        }
    }

    /// `true` for the success outcomes (`Stored` / `Retrieved` / `Deleted`).
    pub fn is_ok(&self) -> bool {
        matches!(
            self,
            KeyringResult::Stored | KeyringResult::Retrieved(_) | KeyringResult::Deleted
        )
    }
}

// FFI Option wrapper for `CallbackInfo::get_keyring_result() ->
// Option<KeyringResult>` — `None` until the first op completes. Not Copy
// (carries an `AzString` in `Retrieved`), so `copy = false` (mirrors
// `OptionNodeType`).
impl_option!(
    KeyringResult,
    OptionKeyringResult,
    copy = false,
    [Debug, Clone, PartialEq]
);
