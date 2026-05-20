//! Apple (iOS + macOS) keyring backend — Keychain generic passwords via
//! the `security-framework` crate (`SecItemAdd/CopyMatching/Delete` under
//! the hood).
//!
//! Items are stored as `kSecClassGenericPassword` scoped to the [`SERVICE`]
//! string, keyed by the request's `key` (the Keychain *account*). A
//! `Store { require_biometry: true }` sets `kSecAttrAccessControl =
//! biometryCurrentSet`, so a later `Get` triggers the OS biometric prompt
//! and the item is invalidated if the user re-enrolls biometrics.
//!
//! Runs each op on a spawned thread: a biometry-bound `Get` calls
//! `SecItemCopyMatching`, which **blocks synchronously** on the OS prompt —
//! doing that on the layout thread would freeze rendering. The outcome is
//! parked in the result channel (`push_keyring_result`) for the next layout
//! pass to fold into the manager, exactly like the async backends.

use azul_core::keyring::{KeyringRequest, KeyringResult};
use azul_layout::managers::keyring::push_keyring_result;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
    set_generic_password_options, AccessControlOptions, PasswordOptions,
};

/// Keychain service name scoping this app's items.
const SERVICE: &str = "com.azul.keyring";

// OSStatus codes from `<Security/SecBase.h>`.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_USER_CANCELED: i32 = -128;
const ERR_SEC_AUTH_FAILED: i32 = -25293;

/// Dispatch a keyring op. Returns immediately; the outcome is delivered
/// asynchronously via the result channel (the op runs on a worker thread
/// so a biometry-bound read's OS prompt doesn't block the layout thread).
pub fn request(req: &KeyringRequest) {
    let req = req.clone();
    std::thread::spawn(move || {
        push_keyring_result(handle(&req));
    });
}

fn handle(req: &KeyringRequest) -> KeyringResult {
    match req {
        KeyringRequest::Store {
            key,
            secret,
            require_biometry,
        } => {
            let bytes = secret.as_str().as_bytes();
            let res = if *require_biometry {
                let mut opts = PasswordOptions::new_generic_password(SERVICE, key.as_str());
                opts.set_access_control_options(AccessControlOptions::BIOMETRY_CURRENT_SET);
                set_generic_password_options(bytes, opts)
            } else {
                set_generic_password(SERVICE, key.as_str(), bytes)
            };
            match res {
                Ok(()) => KeyringResult::Stored,
                Err(e) => map_err(e),
            }
        }
        KeyringRequest::Get { key } => match get_generic_password(SERVICE, key.as_str()) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(s) => KeyringResult::Retrieved(s.into()),
                // A non-UTF-8 blob was stored out-of-band; the AzString
                // surface can't represent it.
                Err(_) => KeyringResult::Error,
            },
            Err(e) => map_err(e),
        },
        KeyringRequest::Delete { key } => match delete_generic_password(SERVICE, key.as_str()) {
            Ok(()) => KeyringResult::Deleted,
            // Already absent — treat delete as idempotent success.
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => KeyringResult::Deleted,
            Err(e) => map_err(e),
        },
    }
}

fn map_err(e: security_framework::base::Error) -> KeyringResult {
    match e.code() {
        ERR_SEC_ITEM_NOT_FOUND => KeyringResult::NotFound,
        // User cancelled or failed the biometric gate on a protected read.
        ERR_SEC_USER_CANCELED | ERR_SEC_AUTH_FAILED => KeyringResult::Denied,
        _ => KeyringResult::Error,
    }
}
