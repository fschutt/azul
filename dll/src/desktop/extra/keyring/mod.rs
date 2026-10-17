//! Platform dispatcher for system-keyring operations
//! (SUPER_PLAN_2 §4 P4.2).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::keyring::KeyringManager`. A callback queues a
//! `KeyringRequest` (`CallbackInfo::keyring_store/get/delete`); the
//! capability pump drains it and calls [`request`] here, which turns each
//! op into the right native keyring call and parks its answer in the result
//! channel (a later frame's `KeyringResult` event):
//!
//! | Platform | Backend |
//! |----------|---------|
//! | iOS / macOS | Keychain `SecItemAdd` / `SecItemCopyMatching` / `SecItemDelete` (objc2 / Security.framework), `kSecAttrAccessControl = biometryCurrentSet` for biometry-bound items |
//! | Android | `KeyStore` + `setUserAuthenticationRequired(true)` (JNI helper) |
//! | Linux | libsecret (`secret_password_store/lookup/clear`) via the secret-service D-Bus |
//! | Windows | Credential Manager (`CredWriteW` / `CredReadW` / `CredDeleteW`, generic credentials) |
//!
//! MWA-C-keyring: ALL FOUR desktop backends are real (the stub-era note
//! that used to live here claimed Windows/Linux resolve to Unavailable —
//! stale). A biometry-bound `Get` parks its outcome back through
//! `push_keyring_result` asynchronously from the OS prompt's reply.
//!
//! [`request_blocking`] (the C API's [`Keyring`]: `Keyring::get_blocking`,
//! `store_blocking`, `delete_blocking`) runs one op on the CALLING thread and
//! returns its answer: what an app's worker `Thread` reads and writes the
//! keyring with - it re-reads a secret another process may have changed (an
//! Azlin drive's rotating token) and writes one before its work goes on. Both
//! ways reach the same keyring. Android's helper answers through its Java
//! callback only: a blocking call answers `Unavailable` there.
//!
//! Headless / E2E runs (`AZ_BACKEND=headless`, `AZ_E2E_TEST`) never touch the
//! REAL host secret store: both ways reach the stand-in of `memory.rs` - one
//! map for the process, or the file `AZ_KEYRING_FILE` names (which keeps it
//! for the next run of a test and shares it between the test's processes).

use std::path::PathBuf;

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_css::AzString;

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
#[cfg(target_os = "linux")]
pub mod linux;
mod memory;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod tests;

/// The variable naming the file that keeps a headless / E2E run's keyring
/// stand-in (ignored by every other run).
pub const KEYRING_FILE_VAR: &str = "AZ_KEYRING_FILE";

/// Where a run's keyring ops go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Backend {
    /// The platform's keyring.
    Platform,
    /// The headless stand-in in this process's memory.
    Memory,
    /// The headless stand-in in a file ([`KEYRING_FILE_VAR`]).
    File(PathBuf),
}

/// The keyring of this run: the stand-in in a headless / E2E run (the
/// dispatcher is keyed on `target_os`, so a headless test on a dev Mac would
/// otherwise write to the real login Keychain under "com.azul.keyring"),
/// else the platform's.
fn backend_of_run() -> Backend {
    let headless = std::env::var("AZ_BACKEND").as_deref() == Ok("headless")
        || std::env::var("AZ_E2E_TEST").is_ok();
    if !headless {
        return Backend::Platform;
    }
    match std::env::var_os(KEYRING_FILE_VAR).filter(|path| !path.is_empty()) {
        Some(path) => Backend::File(PathBuf::from(path)),
        None => Backend::Memory,
    }
}

/// Dispatch one keyring op to the native keyring. Called from the
/// capability pump for each request drained from the channel.
///
/// iOS/macOS → Keychain; Windows → Credential Manager; Linux → libsecret;
/// Android → KeyStore (JNI). Targets without any backend resolve to
/// `Unavailable` so the request → result round-trip stays observable —
/// `CallbackInfo::get_keyring_result()` reads it next frame.
pub fn request(req: &KeyringRequest) {
    match backend_of_run() {
        Backend::Platform => platform_request(req),
        stand_in => {
            azul_layout::managers::keyring::push_keyring_result(answer_with(req, &stand_in));
        }
    }
}

/// [`request`] on the platform's keyring: answered through the result channel.
fn platform_request(req: &KeyringRequest) {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    apple::request(req);
    #[cfg(target_os = "android")]
    android::request(req);
    #[cfg(target_os = "windows")]
    windows::request(req);
    #[cfg(target_os = "linux")]
    linux::request(req);
    #[cfg(not(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "android",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        let _ = req;
        azul_layout::managers::keyring::push_keyring_result(KeyringResult::Unavailable);
    }
}

/// Runs one keyring op on the calling thread and answers it - blocking: a
/// biometry-bound read waits for the OS prompt, a locked keychain for its
/// unlock. For worker threads; a UI callback queues its op with
/// `CallbackInfo::keyring_*` instead.
#[must_use]
pub fn request_blocking(req: &KeyringRequest) -> KeyringResult {
    answer_with(req, &backend_of_run())
}

/// One op on `backend`, answered on this thread.
pub(crate) fn answer_with(req: &KeyringRequest, backend: &Backend) -> KeyringResult {
    match backend {
        Backend::Platform => platform_blocking(req),
        Backend::Memory => memory::answer(req),
        Backend::File(path) => memory::answer_in_file(req, path),
    }
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_blocking(req: &KeyringRequest) -> KeyringResult {
    apple::handle(req)
}

#[cfg(target_os = "windows")]
fn platform_blocking(req: &KeyringRequest) -> KeyringResult {
    windows::handle(req)
}

#[cfg(target_os = "linux")]
fn platform_blocking(req: &KeyringRequest) -> KeyringResult {
    linux::handle(req)
}

/// Android's KeyStore helper answers through its Java callback only; other
/// targets have no keyring.
#[cfg(not(any(
    target_os = "ios",
    target_os = "macos",
    target_os = "windows",
    target_os = "linux"
)))]
fn platform_blocking(_req: &KeyringRequest) -> KeyringResult {
    KeyringResult::Unavailable
}

/// The system keyring as a worker thread uses it: every call runs its op
/// where it is made and returns the answer (blocking) - unlike
/// `CallbackInfo::keyring_*`, whose answer arrives on a later frame as the
/// window's `KeyringResult` event. What an app's `Thread` re-reads a secret
/// with that another process may have changed, and writes one with before
/// its work goes on. Never call it from a callback: a biometry-bound item
/// shows the OS prompt and the call waits for it. Both reach the same
/// keyring (in a headless / E2E run the same stand-in).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder
                                        // field
pub struct Keyring {
    pub _reserved: u8,
}

impl Default for Keyring {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyring {
    /// Returns a zero-initialised namespace handle. Static-only - the struct
    /// is just a hook for the FFI layer.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Reads the secret stored under `key`, on this thread: `Retrieved`,
    /// `NotFound`, or why not (`Denied`, `Unavailable`, `Error`).
    #[must_use]
    pub fn get_blocking(key: AzString) -> KeyringResult {
        request_blocking(&KeyringRequest::Get { key })
    }

    /// Stores `secret` under `key` (replacing what was there), on this
    /// thread: `Stored`, or why not. `require_biometry` as in
    /// `CallbackInfo::keyring_store`.
    #[must_use]
    pub fn store_blocking(
        key: AzString,
        secret: AzString,
        require_biometry: bool,
    ) -> KeyringResult {
        request_blocking(&KeyringRequest::Store {
            key,
            secret,
            require_biometry,
        })
    }

    /// Removes the item stored under `key`, on this thread: `Deleted` (also
    /// when there was none), or why not.
    #[must_use]
    pub fn delete_blocking(key: AzString) -> KeyringResult {
        request_blocking(&KeyringRequest::Delete { key })
    }
}
