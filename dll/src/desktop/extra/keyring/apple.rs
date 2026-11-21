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
//!
//! A quiet op (background work) never shows a prompt: on macOS the keychain's
//! dialogs are off while it runs ([`in_turn`]), so an item only that dialog
//! would hand over (another app's, an unsigned build's, a locked keychain's)
//! fails with `errSecInteractionNotAllowed`; a quiet read skips the items behind
//! Touch ID or a passcode. Both answer `Denied`.

use std::sync::{Mutex, PoisonError};

use azul_core::keyring::{KeyringRequest, KeyringResult};
#[cfg(target_os = "macos")]
use security_framework::os::macos::keychain::SecKeychain;
use security_framework::{
    item::{update_item, ItemClass, ItemSearchOptions, ItemUpdateOptions, Limit, SearchResult},
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
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;
const ERR_SEC_PARAM: i32 = -50;

/// The keychain ops of this process take turns: turning the prompts off is one switch for the
/// whole process (`SecKeychainSetUserInteractionAllowed`), which must not catch another thread's
/// op that may ask. An op that waits for the user holds the turn; a quiet one waits behind it.
static TURN: Mutex<()> = Mutex::new(());

/// Runs `op` in this process's turn at the keychain; `quiet`: with the keychain's prompts off
/// (macOS), on again before the turn passes.
pub(super) fn in_turn<T>(quiet: bool, op: impl FnOnce() -> T) -> T {
    let _turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    // Dropped before the turn: the prompts are on again when the next op runs.
    #[cfg(target_os = "macos")]
    let _prompts_off = if quiet {
        SecKeychain::disable_user_interaction().ok()
    } else {
        None
    };
    #[cfg(not(target_os = "macos"))]
    let _ = quiet;
    op()
}

/// One op on the items of `service`, on the calling thread (a biometry-bound read waits for
/// the OS prompt here; a `quiet` read never does).
pub(super) fn handle(service: &str, req: &KeyringRequest, quiet: bool) -> KeyringResult {
    match req {
        KeyringRequest::Get { key } if quiet => get_quietly(service, key.as_str()),
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
            Ok(bytes) => text_of(bytes),
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

/// A read that never shows a prompt: the search skips an item behind Touch ID or a passcode
/// (`kSecUseAuthenticationUISkip`), so an item that is there but was not read answers `Denied`,
/// as does one only the keychain's dialog would hand over while the prompts are off. A keychain
/// that does not take the skip (`errSecParam`) is read plainly - on macOS with the prompts off.
fn get_quietly(service: &str, key: &str) -> KeyringResult {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(key)
        .load_data(true)
        .limit(Limit::Max(1))
        .skip_authenticated_items(true);
    match search.search() {
        Ok(found) => {
            let data = found.into_iter().find_map(|item| match item {
                SearchResult::Data(bytes) => Some(bytes),
                _ => None,
            });
            match data {
                Some(bytes) => text_of(bytes),
                None => absent_or_denied(service, key),
            }
        }
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => absent_or_denied(service, key),
        Err(e) if e.code() == ERR_SEC_PARAM => match get_generic_password(service, key) {
            Ok(bytes) => text_of(bytes),
            Err(e) => map_err(e),
        },
        Err(e) => map_err(e),
    }
}

/// What a quiet read that got no secret says: `Denied` when the item is there (its attributes
/// are read without a prompt), else `NotFound`.
fn absent_or_denied(service: &str, key: &str) -> KeyringResult {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(key)
        .load_attributes(true)
        .limit(Limit::Max(1));
    match search.search() {
        Ok(found) if !found.is_empty() => KeyringResult::Denied,
        _ => KeyringResult::NotFound,
    }
}

/// A secret as text; a non-UTF-8 blob was stored out-of-band, which the `AzString` surface
/// cannot carry.
fn text_of(bytes: Vec<u8>) -> KeyringResult {
    match String::from_utf8(bytes) {
        Ok(s) => KeyringResult::Retrieved(s.into()),
        Err(_) => KeyringResult::Error,
    }
}

fn map_err(e: security_framework::base::Error) -> KeyringResult {
    match e.code() {
        ERR_SEC_ITEM_NOT_FOUND => KeyringResult::NotFound,
        // User cancelled or failed the biometric gate on a protected read; or a quiet op met an
        // item only a prompt would have handed over.
        ERR_SEC_USER_CANCELED | ERR_SEC_AUTH_FAILED | ERR_SEC_INTERACTION_NOT_ALLOWED => {
            KeyringResult::Denied
        }
        _ => KeyringResult::Error,
    }
}
