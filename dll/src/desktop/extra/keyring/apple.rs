//! Apple (iOS + macOS) keyring backend: Keychain generic passwords (`security-framework`: `SecItemAdd` /
//! `SecItemCopyMatching` / `SecItemDelete`), bound to a user check by the OS where the level asks for one.
//!
//! The model (briefs/mobile_capabilities_ledger_2026-11-28.md): a PROTECTED item (`UserPresence`, `Biometric`) is an
//! item with a `SecAccessControl` (`.userPresence` / `.biometryCurrentSet`, accessible only while a passcode is set
//! on this device): the Secure Enclave releases its secret only after the user check, and a read with
//! `kSecUseAuthenticationContext` (an `LAContext` carrying the reason text and the cancel title) shows the sheet.
//! Beside it lives an UNPROTECTED META item (service `<service>.azul-meta`, account = key) that says which level it
//! is and outlives it: the OS deletes a `biometryCurrentSet` item when the enrolled set changes, and meta present
//! with the item gone is `Invalidated`, not `NotFound`.
//!
//! * the sheet appears only for [`KeyringRequest::Unlock`] (and nothing else shows one): `Get` and every quiet or
//!   blocking op read unprotected items only, with `kSecUseAuthenticationUISkip` so that no sheet can show, and
//!   answer `WouldPrompt` for a protected item;
//! * a protected `Store` is refused where the device cannot do the check (`NotEnrolled`: no passcode / biometric,
//!   `Unavailable`): never a silently weaker item;
//! * every answer is one cause: `Cancelled`, `Failed`, `LockedOut`, `NotEnrolled`, `Invalidated`, `WouldPrompt`,
//!   `Denied` (the system refused this app), `Unavailable`, `Error` (the decisions are `apple_logic.rs`, tested on
//!   every host);
//! * the op runs on the dispatcher's worker thread: an `Unlock` blocks it on the sheet, the UI thread never waits.
//!
//! macOS: the legacy (file-based) keychain has no access-control items, and the data-protection keychain needs the
//! `keychain-access-groups` entitlement (`errSecMissingEntitlement`, -34018, on an unsigned build): a protected level
//! is `Unavailable` there until the library gate lands; unprotected items work in the login keychain as before.

use std::sync::{Mutex, PoisonError};

use azul_core::keyring::{KeyringNeed, KeyringProtection, KeyringPrompt, KeyringRequest, KeyringResult, KeyringSupport};
use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_local_authentication::{LAContext, LAPolicy};
#[cfg(target_os = "macos")]
use security_framework::os::macos::keychain::SecKeychain;
use security_framework::{
    access_control::{ProtectionMode, SecAccessControl},
    item::{update_item, ItemClass, ItemSearchOptions, ItemUpdateOptions, Limit, SearchResult},
    passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
        set_generic_password_options, PasswordOptions,
    },
};

use super::{
    apple_logic::{
        self as logic, access_flags, decode_meta, encode_meta, meta_service, DeviceState, ItemState, Level, Need,
        Outcome, ERR_SEC_DUPLICATE_ITEM, ERR_SEC_ITEM_NOT_FOUND, ERR_SEC_PARAM,
    },
    Interaction,
};

// LAPolicy values (`<LocalAuthentication/LAPublicDefines.h>`): 1 = biometrics only, 2 = biometrics or the passcode.
const POLICY_BIOMETRICS: isize = 1;
const POLICY_WITH_PASSCODE: isize = 2;

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

/// One op on the items of `service`, on the calling thread (an `Unlock` waits for the sheet here; nothing else
/// ever does).
pub(super) fn handle(service: &str, req: &KeyringRequest, interaction: Interaction) -> KeyringResult {
    let quiet = interaction.quiet();
    match req {
        KeyringRequest::Store {
            key,
            secret,
            protection,
            prompt: _,
        } => in_turn(quiet, || store(service, key.as_str(), secret.as_str().as_bytes(), *protection, interaction)),
        KeyringRequest::Get { key } => in_turn(quiet, || get(service, key.as_str())),
        KeyringRequest::Unlock { key, prompt } => in_turn(false, || unlock(service, key.as_str(), prompt, interaction)),
        KeyringRequest::Delete { key } => in_turn(quiet, || delete(service, key.as_str())),
    }
}

fn level_of(protection: KeyringProtection) -> Option<Level> {
    match protection {
        KeyringProtection::DeviceUnlocked => None,
        KeyringProtection::UserPresence => Some(Level::UserPresence),
        KeyringProtection::Biometric => Some(Level::Biometric),
    }
}

// ---- the device ---------------------------------------------------------------------------------------------------

/// What `LAContext.canEvaluatePolicy` says about the device for `level` (never a sheet).
fn device_state_for(level: Level) -> DeviceState {
    let policy = match level {
        Level::UserPresence => LAPolicy(POLICY_WITH_PASSCODE),
        Level::Biometric => LAPolicy(POLICY_BIOMETRICS),
    };
    // SAFETY: a fresh LAContext asked a question on the calling thread.
    let context: Retained<LAContext> = unsafe { LAContext::new() };
    match unsafe { context.canEvaluatePolicy_error(policy) } {
        Ok(()) => DeviceState::Ready,
        Err(error) => logic::device_state(Some(error.code() as i64)),
    }
}

/// Whether this build can bind items to a user check: iOS always (the data-protection keychain is the only one);
/// macOS only with the `keychain-access-groups` entitlement, which a signed app has and an unsigned build lacks.
const fn can_bind_items() -> bool {
    cfg!(target_os = "ios")
}

/// What this device does for `level`: never a sheet.
pub(super) fn support(level: KeyringProtection) -> KeyringSupport {
    let Some(level) = level_of(level) else {
        // An unprotected item is the keychain's own: the OS protects it with the device's data protection.
        return KeyringSupport::Hardware;
    };
    if !can_bind_items() {
        return KeyringSupport::Unavailable;
    }
    match device_state_for(level) {
        DeviceState::Ready | DeviceState::LockedOut => KeyringSupport::Hardware,
        DeviceState::NotEnrolled => KeyringSupport::NotEnrolled,
        DeviceState::Unavailable => KeyringSupport::Unavailable,
    }
}

// ---- the items ----------------------------------------------------------------------------------------------------

/// The meta record of `key`'s protected item, when there is one (an unprotected item has none).
fn read_meta(service: &str, key: &str) -> Option<Level> {
    let bytes = get_generic_password(&meta_service(service), key).ok()?;
    decode_meta(&bytes).map(|(level, _)| level)
}

fn write_meta(service: &str, key: &str, level: Level) -> Result<(), i32> {
    set_generic_password(&meta_service(service), key, &encode_meta(level, None)).map_err(|error| error.code())
}

fn delete_item(service: &str, key: &str) -> Result<(), i32> {
    match delete_generic_password(service, key) {
        Ok(()) => Ok(()),
        // Already absent: a delete is idempotent.
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(error.code()),
    }
}

/// Whether the item is there: asked WITHOUT its secret, so no sheet (the attributes of an item behind a user check are
/// readable). `Err(status)` for a keychain that cannot say.
fn item_state(service: &str, key: &str) -> Result<ItemState, i32> {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(key)
        .load_attributes(true)
        .limit(Limit::Max(1));
    match search.search() {
        Ok(found) if !found.is_empty() => Ok(ItemState::Present),
        Ok(_) => Ok(ItemState::Absent),
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(ItemState::Absent),
        Err(error) => Err(error.code()),
    }
}

/// A secret as text; a non-UTF-8 blob was stored out-of-band, which the `AzString` surface cannot carry.
fn text_of(bytes: Vec<u8>) -> KeyringResult {
    match String::from_utf8(bytes) {
        Ok(s) => KeyringResult::Retrieved(s.into()),
        Err(_) => KeyringResult::Error,
    }
}

fn result_of(outcome: Outcome, stored: KeyringResult) -> KeyringResult {
    match outcome {
        Outcome::Ok => stored,
        Outcome::Cancelled => KeyringResult::Cancelled,
        Outcome::Failed => KeyringResult::Failed,
        Outcome::LockedOut => KeyringResult::LockedOut,
        Outcome::NotEnrolled => KeyringResult::NotEnrolled,
        Outcome::Invalidated => KeyringResult::Invalidated,
        Outcome::Denied => KeyringResult::Denied,
        Outcome::Unavailable => KeyringResult::Unavailable,
        Outcome::WouldPrompt => KeyringResult::WouldPrompt,
        Outcome::NotFound => KeyringResult::NotFound,
        Outcome::Error => KeyringResult::Error,
    }
}

/// What the keychain answered when it was not "fine" or "absent", with the operation that got it: a result carries no status, so
/// this line is the only thing that says WHY an operation ended in `Error` or `Unavailable` (an app on a Simulator without the
/// entitlements of a signed build, a device without a passcode).
fn note_status(what: &str, key: &str, status: i32) {
    if status != 0 && status != ERR_SEC_ITEM_NOT_FOUND {
        crate::log_warn!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[keyring] {} of {:?}: the keychain answered OSStatus {}",
            what,
            key,
            status
        );
    }
}

// ---- the ops ------------------------------------------------------------------------------------------------------

fn store(service: &str, key: &str, secret: &[u8], protection: KeyringProtection, interaction: Interaction) -> KeyringResult {
    let Some(level) = level_of(protection) else {
        // Unprotected: any interaction. An item that was protected once is REPLACED, not updated in place (the update of a
        // bound item can ask for the check, and a delete never does); its meta record goes after the new item is in:
        // it would make the plain item read as a protected one.
        if read_meta(service, key).is_some() {
            let _ = delete_item(service, key);
        }
        return match set_generic_password(service, key, secret) {
            Ok(()) => {
                let _ = delete_item(&meta_service(service), key);
                KeyringResult::Stored
            }
            Err(error) => {
                note_status("store", key, error.code());
                result_of(logic::after_store(error.code(), false, DeviceState::Ready), KeyringResult::Stored)
            }
        };
    };
    // A protected item is made from a user's action only (the dispatcher's `UserCheck`).
    if !interaction.user_check() {
        return KeyringResult::WouldPrompt;
    }
    if !can_bind_items() {
        return KeyringResult::Unavailable;
    }
    // Never a weaker store: where the device cannot do the check, the level is refused (no passcode / biometric).
    match device_state_for(level) {
        DeviceState::NotEnrolled => return KeyringResult::NotEnrolled,
        DeviceState::Unavailable => return KeyringResult::Unavailable,
        DeviceState::Ready | DeviceState::LockedOut => {}
    }
    // An access control cannot be changed in place: the old item (and its meta) go first.
    if let Err(status) = delete_item(service, key) {
        return result_of(logic::after_store(status, true, DeviceState::Ready), KeyringResult::Stored);
    }
    let access = match SecAccessControl::create_with_protection(
        Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
        access_flags(level),
    ) {
        Ok(access) => access,
        Err(error) => {
            return result_of(logic::after_store(error.code(), true, device_state_for(level)), KeyringResult::Stored);
        }
    };
    let mut options = PasswordOptions::new_generic_password(service, key);
    options.set_access_control(access);
    // The data-protection keychain: the only one on iOS; on macOS it needs the keychain-access-groups entitlement
    // (`can_bind_items` is false there, so this point is not reached).
    #[cfg(target_os = "ios")]
    options.use_protected_keychain();
    match set_generic_password_options(secret, options) {
        Ok(()) => match write_meta(service, key, level) {
            Ok(()) => KeyringResult::Stored,
            Err(status) => {
                // No meta record = an item an app could not tell from a plain one: not kept.
                let _ = delete_item(service, key);
                result_of(logic::after_store(status, false, DeviceState::Ready), KeyringResult::Stored)
            }
        },
        Err(error) => {
            note_status("store (protected)", key, error.code());
            result_of(logic::after_store(error.code(), true, device_state_for(level)), KeyringResult::Stored)
        }
    }
}

/// `keyring_get`: the secret of an UNPROTECTED item. A protected one (its meta record is there, or the keychain skips
/// it because it wants a user check) answers `WouldPrompt` with no sheet: the sheet is `unlock`'s.
fn get(service: &str, key: &str) -> KeyringResult {
    let meta = read_meta(service, key);
    if meta.is_some() {
        return match item_state(service, key) {
            Ok(state) => result_of(
                match logic::need_of(meta, state) {
                    Need::Invalidated => Outcome::Invalidated,
                    _ => Outcome::WouldPrompt,
                },
                KeyringResult::Stored,
            ),
            Err(status) => result_of(logic::after_plain_read(status, true), KeyringResult::Stored),
        };
    }
    read_unprotected(service, key)
}

/// A read that can never show a user-check sheet: the search skips an item behind Touch ID or a passcode
/// (`kSecUseAuthenticationUISkip`), so an item that is there but was not read answers `WouldPrompt` (a legacy
/// protected item without a meta record included). A keychain that does not take the skip (`errSecParam`) is read
/// plainly (macOS: the login keychain, with the keychain's own prompts off under a quiet op).
fn read_unprotected(service: &str, key: &str) -> KeyringResult {
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(key)
        .load_data(true)
        .limit(Limit::Max(1))
        .skip_authenticated_items(true);
    let status = match search.search() {
        Ok(found) => {
            let data = found.into_iter().find_map(|item| match item {
                SearchResult::Data(bytes) => Some(bytes),
                _ => None,
            });
            match data {
                Some(bytes) => return text_of(bytes),
                // Skipped (it wants a user check) or not there: the attributes tell which.
                None => ERR_SEC_ITEM_NOT_FOUND,
            }
        }
        Err(error) if error.code() == ERR_SEC_PARAM => {
            return match get_generic_password(service, key) {
                Ok(bytes) => text_of(bytes),
                Err(error) => {
                    note_status("read (plain)", key, error.code());
                    result_of(logic::after_plain_read(error.code(), false), KeyringResult::Stored)
                }
            };
        }
        Err(error) => error.code(),
    };
    if status == ERR_SEC_ITEM_NOT_FOUND {
        return match item_state(service, key) {
            // There, and not handed over: a user check (or the keychain's own dialog) was needed.
            Ok(ItemState::Present) => KeyringResult::WouldPrompt,
            Ok(ItemState::Absent) => KeyringResult::NotFound,
            Err(status) => result_of(logic::after_plain_read(status, false), KeyringResult::Stored),
        };
    }
    note_status("read", key, status);
    result_of(logic::after_plain_read(status, false), KeyringResult::Stored)
}

/// `keyring_unlock`: the ONLY read that may show the sheet. An unprotected item is read plainly.
fn unlock(service: &str, key: &str, prompt: &KeyringPrompt, interaction: Interaction) -> KeyringResult {
    let meta = read_meta(service, key);
    if meta.is_none() {
        // No meta record: a plain item, read as it is - unless the keychain says it is bound to a check anyway (an item an
        // older azul made with an access control, before the records existed): the sheet below reads that one.
        let plain = read_unprotected(service, key);
        if plain != KeyringResult::WouldPrompt {
            return plain;
        }
    }
    let level = meta.unwrap_or(Level::UserPresence);
    if !interaction.user_check() {
        return KeyringResult::WouldPrompt;
    }
    let context: Retained<LAContext> = unsafe { LAContext::new() };
    let reason = crate::desktop::extra::info_plist::prompt_reason_or_default(prompt.reason.as_str());
    // SAFETY: plain property setters on the context this function owns.
    unsafe {
        context.setLocalizedReason(&NSString::from_str(&reason));
        if !prompt.cancel_label.as_str().is_empty() {
            context.setLocalizedCancelTitle(Some(&NSString::from_str(prompt.cancel_label.as_str())));
        }
    }
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(service)
        .account(key)
        .load_data(true)
        .limit(Limit::Max(1));
    // The context carries the reason and the cancel title of the sheet. The typed setter wants a `core-foundation`
    // object, a crate this one does not depend on; the raw one takes the retained context as it is.
    #[allow(deprecated)]
    // SAFETY: `into_raw` hands over the +1 the search options release when they are dropped.
    unsafe {
        search.authentication_context(Retained::into_raw(context).cast());
    }
    let (status, data) = match search.search() {
        Ok(found) => {
            let data = found.into_iter().find_map(|item| match item {
                SearchResult::Data(bytes) => Some(bytes),
                _ => None,
            });
            match data {
                Some(bytes) => (0, Some(bytes)),
                None => (ERR_SEC_ITEM_NOT_FOUND, None),
            }
        }
        Err(error) => (error.code(), None),
    };
    if let Some(bytes) = data {
        return text_of(bytes);
    }
    let device = if matches!(status, logic::ERR_SEC_AUTH_FAILED | ERR_SEC_PARAM) {
        device_state_for(level)
    } else {
        DeviceState::Ready
    };
    if status != 0 && status != ERR_SEC_ITEM_NOT_FOUND {
        crate::log_warn!(
            crate::desktop::shell2::common::debug_server::LogCategory::Platform,
            "[keyring] unlock of {:?}: the keychain answered OSStatus {}",
            key,
            status
        );
    }
    result_of(logic::after_unlock(status, device, true), KeyringResult::Stored)
}

/// `keyring_delete`: the item and its meta record; deleting what is not there is a success.
fn delete(service: &str, key: &str) -> KeyringResult {
    let item = delete_item(service, key);
    let meta = delete_item(&meta_service(service), key);
    match (item, meta) {
        (Ok(()), Ok(())) => KeyringResult::Deleted,
        (Err(status), _) | (_, Err(status)) => result_of(logic::after_plain_read(status, false), KeyringResult::Deleted),
    }
}

/// `Keyring::needs_prompt`: what a `keyring_unlock` of `key` would do, from the meta record and the item's attributes
/// (never its secret): never a sheet.
pub(super) fn needs_prompt(service: &str, key: &str) -> KeyringNeed {
    let meta = read_meta(service, key);
    match item_state(service, key) {
        Ok(state) => match logic::need_of(meta, state) {
            Need::NotFound => KeyringNeed::NotFound,
            Need::Nothing => KeyringNeed::Nothing,
            Need::UserPresence => KeyringNeed::UserPresence,
            Need::Biometric => KeyringNeed::Biometric,
            Need::Invalidated => KeyringNeed::Invalidated,
        },
        Err(status) => match logic::after_plain_read(status, meta.is_some()) {
            Outcome::Unavailable => KeyringNeed::Unavailable,
            _ => KeyringNeed::Error,
        },
    }
}

/// Moves the item `key` from the service `from` to `to` by changing its service in place: its
/// secret is never read (no prompt) and a protected item stays bound; its meta record moves with it. When `to` holds
/// the entry already, that one is the newer: the old item goes. `true` when nothing is left
/// in `from` (also when there was nothing).
pub(super) fn move_entry(key: &str, from: &str, to: &str) -> bool {
    let moved = move_one(key, from, to);
    let meta_moved = move_one(key, &meta_service(from), &meta_service(to));
    moved && meta_moved
}

fn move_one(key: &str, from: &str, to: &str) -> bool {
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
