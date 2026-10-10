//! The keyring's headless / E2E stand-in (`AZ_BACKEND=headless`, `AZ_E2E_TEST`): one map for
//! the process - what a callback's request and a worker thread's blocking call both reach - or,
//! when `AZ_KEYRING_FILE` names a file, that file (JSON: entry name to secret), read, changed and
//! written in place under its own OS lock for every op, so the keyring outlives the run (a test
//! that restarts an app) and the processes of one test share it. Plaintext: a test's keyring
//! only - a headless run never touches the real one.

use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Mutex, PoisonError},
};

use azul_core::keyring::{KeyringRequest, KeyringResult};

/// The stand-in of this process.
static STORE: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
/// The threads of this process take turns on a keyring file (the file's lock lines up the
/// processes).
static FILE_TURN: Mutex<()> = Mutex::new(());

/// One op on the process's stand-in.
pub(super) fn answer(req: &KeyringRequest) -> KeyringResult {
    let mut store = STORE.lock().unwrap_or_else(PoisonError::into_inner);
    answer_in(req, &mut store)
}

/// One op on `store`: what the platform backends answer (a delete of what is not there is
/// `Deleted` too).
fn answer_in(req: &KeyringRequest, store: &mut BTreeMap<String, String>) -> KeyringResult {
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

/// One op on the stand-in kept in the file `path` (made when there is none); `Error` when it
/// cannot be read, parsed or written.
pub(super) fn answer_in_file(req: &KeyringRequest, path: &Path) -> KeyringResult {
    let _turn = FILE_TURN.lock().unwrap_or_else(PoisonError::into_inner);
    in_file(req, path).unwrap_or(KeyringResult::Error)
}

fn in_file(req: &KeyringRequest, path: &Path) -> io::Result<KeyringResult> {
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
    let mut store: BTreeMap<String, String> = if text.trim().is_empty() {
        BTreeMap::new()
    } else {
        serde_json::from_str(&text).map_err(io::Error::other)?
    };
    let result = answer_in(req, &mut store);
    if !matches!(req, KeyringRequest::Get { .. }) {
        // In place, never through a rename: a process waiting for the lock holds THIS file open.
        let text = serde_json::to_string(&store).map_err(io::Error::other)?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    Ok(result)
}
