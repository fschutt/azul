//! The keyring's stand-in: in a debug build, and in a headless / E2E run of a build with the cargo
//! feature `test-hooks` (`AZ_BACKEND=headless`, `AZ_E2E_TEST`), one map
//! for the process - what a callback's request and a worker thread's blocking call both reach -
//! and, in a build with the hooks where `AZ_KEYRING_FILE` names a file (a test stack's windows
//! too), that file
//! (JSON: each namespace's entries under its service name, those of the service every app shared
//! before namespaces flat at the top), read, changed and written in place under its own OS lock
//! for every op, so the keyring outlives the run (a test that restarts an app) and the processes of
//! one test share it. Plaintext: a test's keyring only - such a run never touches the real one -,
//! and the file is the owner's alone (0600 on Unix, made so also when it exists with broader modes).
//! Entries live per namespace (keyring service), as in the real keyrings.
//!
//! The stand-in has the SAME semantics as the platform backends (the model of
//! `azul_core::keyring`): an item has a protection level and keeps it; a `Get` of a protected
//! item answers `WouldPrompt`, an `Unlock` shows the pretend sheet - the scripted user
//! (`StandInUser`: what is enrolled, what they do at each sheet) decides: `Retrieved`,
//! `Cancelled`, `Failed`, `LockedOut`, `NotEnrolled` - and a `Biometric` item stored under an
//! enrolled set that has changed since answers `Invalidated`. So a flow of a protected secret runs
//! the same on every host, an e2e scenario of it too.

use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Mutex, PoisonError},
};

use azul_core::keyring::{
    KeyringNamespace, KeyringNeed, KeyringProtection, KeyringRequest, KeyringResult, KeyringSupport,
};
use azul_layout::managers::stand_in_user::{SheetOutcome, StandInUser};

use super::Interaction;

/// One entry: its secret, how it is guarded, and the enrolled-set epoch it was stored under.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    secret: String,
    protection: KeyringProtection,
    epoch: u32,
}

impl Entry {
    const fn plain(secret: String) -> Self {
        Self {
            secret,
            protection: KeyringProtection::DeviceUnlocked,
            epoch: 0,
        }
    }
}

/// One namespace's entries: entry name to entry.
type Entries = BTreeMap<String, Entry>;
/// Every namespace's entries, by service.
type Services = BTreeMap<String, Entries>;

/// The stand-in of this process.
static STORE: Mutex<Services> = Mutex::new(BTreeMap::new());
/// The threads of this process take turns on a keyring file (the file's lock lines up the
/// processes).
static FILE_TURN: Mutex<()> = Mutex::new(());

/// One op in `service` on the process's stand-in.
pub(super) fn answer(
    service: &str,
    req: &KeyringRequest,
    interaction: Interaction,
    user: &StandInUser,
) -> KeyringResult {
    let mut store = STORE.lock().unwrap_or_else(PoisonError::into_inner);
    answer_in(req, store.entry(service.to_string()).or_default(), interaction, user)
}

/// Whether the enrolled set changed since the entry was stored: a `Biometric` item is bound to
/// it (a `UserPresence` one survives, the passcode still works).
fn invalidated(entry: &Entry, user: &StandInUser) -> bool {
    entry.protection == KeyringProtection::Biometric && entry.epoch != user.epoch()
}

/// What a read of `entry` needs (see `KeyringNeed`).
fn need_of(entry: Option<&Entry>, user: &StandInUser) -> KeyringNeed {
    match entry {
        None => KeyringNeed::NotFound,
        Some(entry) if !entry.protection.needs_user() => KeyringNeed::Nothing,
        Some(entry) if invalidated(entry, user) => KeyringNeed::Invalidated,
        Some(entry) => match entry.protection {
            KeyringProtection::Biometric => KeyringNeed::Biometric,
            _ => KeyringNeed::UserPresence,
        },
    }
}

/// What a read of `key` in `service` needs, on the process's stand-in.
pub(super) fn need(service: &str, key: &str, user: &StandInUser) -> KeyringNeed {
    let store = STORE.lock().unwrap_or_else(PoisonError::into_inner);
    need_of(store.get(service).and_then(|entries| entries.get(key)), user)
}

/// What the stand-in does for `protection`: it pretends to be hardware when the scripted user
/// has what the level needs, and says `NotEnrolled` when not.
pub(super) fn support(protection: KeyringProtection, user: &StandInUser) -> KeyringSupport {
    if user.meets(protection) {
        KeyringSupport::Hardware
    } else {
        KeyringSupport::NotEnrolled
    }
}

/// One op on `store`: what the platform backends answer (a delete of what is not there is
/// `Deleted` too).
fn answer_in(
    req: &KeyringRequest,
    store: &mut Entries,
    interaction: Interaction,
    user: &StandInUser,
) -> KeyringResult {
    match req {
        KeyringRequest::Store {
            key,
            secret,
            protection,
            ..
        } => {
            if protection.needs_user() {
                // A protected item is made behind the user's action only, and never weaker than
                // asked: a device that cannot meet the level stores nothing.
                if !interaction.user_check() {
                    return KeyringResult::WouldPrompt;
                }
                if !user.meets(*protection) {
                    return KeyringResult::NotEnrolled;
                }
            }
            store.insert(
                key.as_str().to_string(),
                Entry {
                    secret: secret.as_str().to_string(),
                    protection: *protection,
                    epoch: user.epoch(),
                },
            );
            KeyringResult::Stored
        }
        KeyringRequest::Get { key } => match store.get(key.as_str()) {
            None => KeyringResult::NotFound,
            Some(entry) if !entry.protection.needs_user() => {
                KeyringResult::Retrieved(entry.secret.as_str().into())
            }
            Some(entry) if invalidated(entry, user) => KeyringResult::Invalidated,
            Some(_) => KeyringResult::WouldPrompt,
        },
        KeyringRequest::Unlock { key, .. } => match store.get(key.as_str()) {
            None => KeyringResult::NotFound,
            Some(entry) if !entry.protection.needs_user() => {
                KeyringResult::Retrieved(entry.secret.as_str().into())
            }
            Some(_) if !interaction.user_check() => KeyringResult::WouldPrompt,
            Some(entry) if invalidated(entry, user) => KeyringResult::Invalidated,
            Some(entry) => match user.check(entry.protection) {
                SheetOutcome::Passed => KeyringResult::Retrieved(entry.secret.as_str().into()),
                SheetOutcome::Cancelled => KeyringResult::Cancelled,
                SheetOutcome::Failed => KeyringResult::Failed,
                SheetOutcome::LockedOut => KeyringResult::LockedOut,
                SheetOutcome::NotEnrolled => KeyringResult::NotEnrolled,
            },
        },
        KeyringRequest::Delete { key } => {
            store.remove(key.as_str());
            KeyringResult::Deleted
        }
    }
}

/// One op in `service` on the stand-in kept in the file `path` (made when there is none);
/// `Error` when it cannot be read, parsed or written.
pub(super) fn answer_in_file(
    service: &str,
    req: &KeyringRequest,
    path: &Path,
    interaction: Interaction,
    user: &StandInUser,
) -> KeyringResult {
    let _turn = FILE_TURN.lock().unwrap_or_else(PoisonError::into_inner);
    in_file(service, req, path, interaction, user).unwrap_or(KeyringResult::Error)
}

/// What a read of `key` in `service` needs, on the stand-in kept in the file `path`.
pub(super) fn need_in_file(
    service: &str,
    key: &str,
    path: &Path,
    user: &StandInUser,
) -> KeyringNeed {
    let _turn = FILE_TURN.lock().unwrap_or_else(PoisonError::into_inner);
    let read = || -> io::Result<KeyringNeed> {
        let mut file = open_locked(path)?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        let services = parse_file(&text)?;
        Ok(need_of(
            services.get(service).and_then(|entries| entries.get(key)),
            user,
        ))
    };
    read().unwrap_or(KeyringNeed::Error)
}

/// The file's namespaces. A text value at the top is an entry of the service every app shared
/// before namespaces (what the test stacks seed); an object is one namespace's entries: each a
/// text (an unprotected secret) or an object with the `secret`, its `protection` (0, 1, 2 as
/// `KeyringProtection::code`) and the `epoch` it was stored under.
fn parse_file(text: &str) -> io::Result<Services> {
    if text.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let top: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(text).map_err(io::Error::other)?;
    let mut services = Services::new();
    for (name, value) in top {
        match value {
            serde_json::Value::String(secret) => {
                services
                    .entry(KeyringNamespace::LEGACY_SERVICE.to_string())
                    .or_default()
                    .insert(name, Entry::plain(secret));
            }
            serde_json::Value::Object(entries) => {
                let namespace = services.entry(name).or_default();
                for (key, entry) in entries {
                    namespace.insert(key, parse_entry(entry)?);
                }
            }
            _ => return Err(io::Error::other("a keyring entry that is no text")),
        }
    }
    Ok(services)
}

/// One entry of a namespace: a text, or the object of a protected entry.
fn parse_entry(value: serde_json::Value) -> io::Result<Entry> {
    match value {
        serde_json::Value::String(secret) => Ok(Entry::plain(secret)),
        serde_json::Value::Object(fields) => {
            let Some(serde_json::Value::String(secret)) = fields.get("secret") else {
                return Err(io::Error::other("a keyring entry that is no text"));
            };
            let protection = fields
                .get("protection")
                .and_then(serde_json::Value::as_u64)
                .and_then(|code| u8::try_from(code).ok())
                .and_then(KeyringProtection::from_code)
                .ok_or_else(|| io::Error::other("a keyring entry with no protection level"))?;
            let epoch = fields
                .get("epoch")
                .and_then(serde_json::Value::as_u64)
                .and_then(|epoch| u32::try_from(epoch).ok())
                .unwrap_or(0);
            Ok(Entry {
                secret: secret.clone(),
                protection,
                epoch,
            })
        }
        _ => Err(io::Error::other("a keyring entry that is no text")),
    }
}

/// An entry as the file keeps it: a text when it is unprotected, else an object.
fn entry_value(entry: &Entry) -> serde_json::Value {
    if entry.protection.needs_user() {
        serde_json::json!({
            "secret": entry.secret,
            "protection": entry.protection.code(),
            "epoch": entry.epoch,
        })
    } else {
        serde_json::Value::String(entry.secret.clone())
    }
}

/// The file's text: the old service's entries flat at the top (as they were read), every other
/// namespace as an object; empty namespaces are left out.
fn file_text(services: &Services) -> io::Result<String> {
    let mut top = serde_json::Map::new();
    for (service, entries) in services {
        if service == KeyringNamespace::LEGACY_SERVICE {
            // The old shared service never held a protected item: its entries are texts.
            for (key, entry) in entries {
                top.insert(key.clone(), serde_json::Value::String(entry.secret.clone()));
            }
        } else if !entries.is_empty() {
            let entries = entries
                .iter()
                .map(|(key, entry)| (key.clone(), entry_value(entry)))
                .collect();
            top.insert(service.clone(), serde_json::Value::Object(entries));
        }
    }
    serde_json::to_string(&top).map_err(io::Error::other)
}

/// The file, opened for this op and locked until it closes (made when there is none; 0600).
fn open_locked(path: &Path) -> io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    // The secrets in it are for the owner alone, in plain text as they are.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options.open(path)?;
    #[cfg(unix)]
    owner_only(&file)?;
    // Held until `file` closes, at the end of this op.
    file.lock()?;
    Ok(file)
}

fn in_file(
    service: &str,
    req: &KeyringRequest,
    path: &Path,
    interaction: Interaction,
    user: &StandInUser,
) -> io::Result<KeyringResult> {
    // The old shared service keeps texts only (its entries sit flat at the top of the file): a
    // protected item has no place in it, and is not kept there as a weaker one.
    if service == KeyringNamespace::LEGACY_SERVICE
        && matches!(req, KeyringRequest::Store { protection, .. } if protection.needs_user())
    {
        return Ok(KeyringResult::Unavailable);
    }
    let mut file = open_locked(path)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let mut services = parse_file(&text)?;
    let result = answer_in(
        req,
        services.entry(service.to_string()).or_default(),
        interaction,
        user,
    );
    if !matches!(req, KeyringRequest::Get { .. } | KeyringRequest::Unlock { .. }) {
        // In place, never through a rename: a process waiting for the lock holds THIS file open.
        let text = file_text(&services)?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    Ok(result)
}

/// A keyring file made before (or by a tool) with broader modes is tightened to 0600.
#[cfg(unix)]
fn owner_only(file: &std::fs::File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if file.metadata()?.permissions().mode() & 0o077 != 0 {
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
