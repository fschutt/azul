//! Platform dispatcher for system-keyring operations
//! (SUPER_PLAN_2 §4 P4.2).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::keyring::KeyringManager`. A callback queues a
//! `KeyringRequest` (`CallbackInfo::keyring_store/get/delete`); the
//! capability pump drains it and calls [`request`] here, which turns each
//! op into the right native keyring call:
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

use azul_core::keyring::KeyringRequest;

#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

/// Dispatch one keyring op to the native keyring. Called from the
/// capability pump for each request drained from the channel.
///
/// iOS/macOS → Keychain; Windows → Credential Manager; Linux → libsecret;
/// Android → KeyStore (JNI). Targets without any backend resolve to
/// `Unavailable` so the request → result round-trip stays observable —
/// `CallbackInfo::get_keyring_result()` reads it next frame.
pub fn request(req: &KeyringRequest) {
    // MWA-C-keyring: headless / E2E runs must never touch the REAL host
    // secret store (the dispatcher is keyed on target_os, so a headless
    // test on a dev Mac would write to the actual login Keychain under
    // "com.azul.keyring"). Serve them from an in-memory store instead —
    // Store/Get/Delete round-trips stay fully observable and deterministic.
    if std::env::var("AZ_BACKEND").as_deref() == Ok("headless")
        || std::env::var("AZ_E2E_TEST").is_ok()
    {
        e2e_memory_store(req);
        return;
    }
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
        azul_layout::managers::keyring::push_keyring_result(
            azul_core::keyring::KeyringResult::Unavailable,
        );
    }
}

/// MWA-C-keyring: deterministic in-memory secret store for headless / E2E
/// runs (see [`request`]). Process-lifetime only, never persisted.
fn e2e_memory_store(req: &KeyringRequest) {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use azul_core::keyring::KeyringResult;
    use azul_layout::managers::keyring::push_keyring_result;

    static STORE: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

    let mut store = STORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = match req {
        KeyringRequest::Store { key, secret, .. } => {
            store.insert(key.as_str().to_string(), secret.as_str().to_string());
            KeyringResult::Stored
        }
        KeyringRequest::Get { key } => match store.get(key.as_str()) {
            Some(secret) => KeyringResult::Retrieved(secret.as_str().into()),
            None => KeyringResult::NotFound,
        },
        KeyringRequest::Delete { key } => {
            store.remove(key.as_str());
            KeyringResult::Deleted
        }
    };
    push_keyring_result(result);
}
