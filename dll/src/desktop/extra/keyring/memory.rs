//! The keyring's stand-in: in a headless / E2E run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`) one map
//! for the process - what a callback's request and a worker thread's blocking call both reach -
//! and, in any run where `AZ_KEYRING_FILE` names a file (a test stack's windows too), that file
//! (JSON: each namespace's entries under its service name, those of the service every app shared
//! before namespaces flat at the top), read, changed and written in place under its own OS lock
//! for every op, so the keyring outlives the run (a test that restarts an app) and the processes of
//! one test share it. Plaintext: a test's keyring only - such a run never touches the real one.
//! Entries live per namespace (keyring service), as in the real keyrings.

use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Mutex, PoisonError},
};

use azul_core::keyring::{KeyringNamespace, KeyringRequest, KeyringResult};

/// One namespace's entries: entry name to secret.
type Entries = BTreeMap<String, String>;
/// Every namespace's entries, by service.
type Services = BTreeMap<String, Entries>;

/// The stand-in of this process.
static STORE: Mutex<Services> = Mutex::new(BTreeMap::new());
/// The threads of this process take turns on a keyring file (the file's lock lines up the
/// processes).
static FILE_TURN: Mutex<()> = Mutex::new(());

/// One op in `service` on the process's stand-in.
pub(super) fn answer(service: &str, req: &KeyringRequest) -> KeyringResult {
    let mut store = STORE.lock().unwrap_or_else(PoisonError::into_inner);
    answer_in(req, store.entry(service.to_string()).or_default())
}

/// One op on `store`: what the platform backends answer (a delete of what is not there is
/// `Deleted` too).
fn answer_in(req: &KeyringRequest, store: &mut Entries) -> KeyringResult {
    match req {
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
    }
}

/// One op in `service` on the stand-in kept in the file `path` (made when there is none);
/// `Error` when it cannot be read, parsed or written.
pub(super) fn answer_in_file(service: &str, req: &KeyringRequest, path: &Path) -> KeyringResult {
    let _turn = FILE_TURN.lock().unwrap_or_else(PoisonError::into_inner);
    in_file(service, req, path).unwrap_or(KeyringResult::Error)
}

/// The file's namespaces. A text value at the top is an entry of the service every app shared
/// before namespaces (what the test stacks seed); an object is one namespace's entries.
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
                    .insert(name, secret);
            }
            serde_json::Value::Object(entries) => {
                let namespace = services.entry(name).or_default();
                for (key, secret) in entries {
                    let serde_json::Value::String(secret) = secret else {
                        return Err(io::Error::other("a keyring entry that is no text"));
                    };
                    namespace.insert(key, secret);
                }
            }
            _ => return Err(io::Error::other("a keyring entry that is no text")),
        }
    }
    Ok(services)
}

/// The file's text: the old service's entries flat at the top (as they were read), every other
/// namespace as an object; empty namespaces are left out.
fn file_text(services: &Services) -> io::Result<String> {
    let mut top = serde_json::Map::new();
    for (service, entries) in services {
        if service == KeyringNamespace::LEGACY_SERVICE {
            for (key, secret) in entries {
                top.insert(key.clone(), serde_json::Value::String(secret.clone()));
            }
        } else if !entries.is_empty() {
            let entries = entries
                .iter()
                .map(|(key, secret)| (key.clone(), serde_json::Value::String(secret.clone())))
                .collect();
            top.insert(service.clone(), serde_json::Value::Object(entries));
        }
    }
    serde_json::to_string(&top).map_err(io::Error::other)
}

fn in_file(service: &str, req: &KeyringRequest, path: &Path) -> io::Result<KeyringResult> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    // Held until `file` closes, at the end of this op.
    file.lock()?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let mut services = parse_file(&text)?;
    let result = answer_in(req, services.entry(service.to_string()).or_default());
    if !matches!(req, KeyringRequest::Get { .. }) {
        // In place, never through a rename: a process waiting for the lock holds THIS file open.
        let text = file_text(&services)?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    Ok(result)
}
