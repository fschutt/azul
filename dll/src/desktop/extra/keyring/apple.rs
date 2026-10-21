//! Apple (iOS + macOS) keyring backend — Keychain generic passwords via
//! the `security-framework` crate (`SecItemAdd/CopyMatching/Delete` under
//! the hood).
//!
//! Items are stored as `kSecClassGenericPassword` under the namespace's
//! service, keyed by the request's `key` (the Keychain *account*). A
//! `Store { require_biometry: true }` sets `kSecAttrAccessControl =
//! biometryCurrentSet`, so a later `Get` triggers the OS biometric prompt
//! and the item is invalidated if the user re-enrolls biometrics.
//!
//! A callback's op runs on a spawned thread (the dispatcher's): a biometry-bound
//! `Get` calls `SecItemCopyMatching`, which **blocks synchronously** on the OS
//! prompt — doing that on the layout thread would freeze rendering.

use azul_core::keyring::{KeyringRequest, KeyringResult};
use security_framework::{
    item::{update_item, ItemClass, ItemSearchOptions, ItemUpdateOptions},
    passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
        set_generic_password_options, AccessControlOptions, PasswordOptions,
    },
};

// OSStatus codes from `<Security/SecBase.h>`.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;
const ERR_SEC_USER_CANCELED: i32 = -128;
const ERR_SEC_AUTH_FAILED: i32 = -25293;

/// One op on the items of `service`, on the calling thread (a biometry-bound read waits for
/// the OS prompt here).
pub(super) fn handle(service: &str, req: &KeyringRequest) -> KeyringResult {
    match req {
        KeyringRequest::Store {
            key,
            secret,
            require_biometry,
        } => {
            let bytes = secret.as_str().as_bytes();
            let res = if *require_biometry {
                let mut opts = PasswordOptions::new_generic_password(service, key.as_str());
                opts.set_access_control_options(AccessControlOptions::BIOMETRY_CURRENT_SET);
                set_generic_password_options(bytes, opts)
            } else {
                set_generic_password(service, key.as_str(), bytes)
            };
            match res {
                Ok(()) => KeyringResult::Stored,
                Err(e) => map_err(e),
            }
        }
        KeyringRequest::Get { key } => match get_generic_password(service, key.as_str()) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(s) => KeyringResult::Retrieved(s.into()),
                // A non-UTF-8 blob was stored out-of-band; the AzString
                // surface can't represent it.
                Err(_) => KeyringResult::Error,
            },
            Err(e) => map_err(e),
        },
        KeyringRequest::Delete { key } => match delete_generic_password(service, key.as_str()) {
            Ok(()) => KeyringResult::Deleted,
            // Already absent — treat delete as idempotent success.
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => KeyringResult::Deleted,
            Err(e) => map_err(e),
        },
    }
}

/// Moves the item `key` from the service `from` to `to` by changing its service in place: its
/// secret is never read (no prompt) and a biometry-bound item stays bound. When `to` holds
/// the entry already, that one is the newer: the old item goes. `true` when nothing is left
/// in `from` (also when there was nothing).
pub(super) fn move_entry(key: &str, from: &str, to: &str) -> bool {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(from)
        .account(key);
    let mut update = ItemUpdateOptions::new();
    update.set_service(to);
    match update_item(&search, &update) {
        Ok(()) => true,
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => true,
        Err(e) if e.code() == ERR_SEC_DUPLICATE_ITEM => match delete_generic_password(from, key) {
            Ok(()) => true,
            Err(e) => e.code() == ERR_SEC_ITEM_NOT_FOUND,
        },
        Err(_) => false,
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
