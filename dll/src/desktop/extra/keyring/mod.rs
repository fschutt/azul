//! Platform dispatcher for system-keyring operations
//! (SUPER_PLAN_2 §4 P4.2).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::keyring::KeyringManager`. A callback queues a
//! `KeyringRequest` (`CallbackInfo::keyring_store/get/delete`); the layout
//! pass drains it and calls [`request`] here, which turns each op into the
//! right native keyring call:
//!
//! | Platform | Backend |
//! |----------|---------|
//! | iOS / macOS | Keychain `SecItemAdd` / `SecItemCopyMatching` / `SecItemDelete` (objc2 / Security.framework), `kSecAttrAccessControl = biometryCurrentSet` for biometry-bound items |
//! | Android | `KeyStore` + `setUserAuthenticationRequired(true)` (JNI helper) |
//! | Linux | libsecret (`secret_password_store/lookup/clear`) via the secret-service D-Bus |
//! | Windows | `Windows.Security.Credentials.PasswordVault` (CredentialLocker) |
//!
//! This tick lands the dispatcher with a no-backend stub that resolves
//! every op to [`KeyringResult::Unavailable`] (so the request → result
//! round-trip stays observable). The native backends replace this body in
//! later ticks; a biometry-bound `Get` parks its outcome back through
//! `push_keyring_result` asynchronously from the OS prompt's reply.

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_layout::managers::keyring::push_keyring_result;

/// Dispatch one keyring op to the native keyring. Called from
/// `regenerate_layout` for each request the layout pass drained.
///
/// Stub for now: no backend, so the op resolves to `Unavailable` (parked
/// in the result channel so `CallbackInfo::get_keyring_result()` reads it
/// next frame).
pub fn request(req: &KeyringRequest) {
    let _ = req;
    push_keyring_result(KeyringResult::Unavailable);
}
