//! Locks, two kinds: another app holding a file of the folder locked (`drive_lock` with a
//! `path`: an exclusive lock, as Office takes on Windows and an app mid-write takes with
//! `flock`), and the drive's key not on the device (`drive_lock` without one: a locked vault -
//! the device's drive is opened again over a keyring that holds nothing for it, so every pass
//! fails as "sign in" until `drive_unlock` gives the key back).

use std::fs;

use azul::prelude::*;

use super::{a_state::with_a, str_arg, with_harness, Answer, Args};
use crate::DriveState;

/// `drive_lock`: with `path`, an exclusive lock on that file held by "another app" until
/// `drive_unlock` with the same path; without, the drive's key leaves the device.
pub(super) fn lock(s: &mut DriveState, args: &Args) -> Answer {
    let (device, root) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.name.clone(), d.root.clone()))
    })?;
    let Some(path) = str_arg(args, "path") else {
        reopen(s, args, true)?;
        return Ok(serde_json::json!({ "key_here": false }));
    };
    let key = path.trim_matches('/').to_string();
    let file = fs::File::options()
        .read(true)
        .write(true)
        .open(root.join(&key))
        .map_err(|e| format!("{key}: {e}"))?;
    file.lock().map_err(|e| format!("{key}: the lock: {e}"))?;
    with_a(|a| {
        a.locks.insert((device, key.clone()), file);
    })?;
    Ok(serde_json::json!({ "locked": key }))
}

/// `drive_unlock`: the file's lock let go (`path`), or the drive's key back on the device.
pub(super) fn unlock(s: &mut DriveState, args: &Args) -> Answer {
    let device = with_harness(|h| Ok(h.device(args)?.name.clone()))?;
    if args.contains_key("passphrase") {
        return Err(String::from(
            "AzDrive keeps the drive key in the system's keyring, unlocked by the login: it has \
             no passphrase of its own to try (a forgotten key is recovered with drive_recover)",
        ));
    }
    let Some(path) = str_arg(args, "path") else {
        reopen(s, args, false)?;
        return Ok(serde_json::json!({ "key_here": true }));
    };
    let key = path.trim_matches('/').to_string();
    let held = with_a(|a| a.locks.remove(&(device, key.clone())))?;
    let file = held.ok_or_else(|| format!("{key} is not locked (drive_lock it first)"))?;
    file.unlock().map_err(|e| format!("{key}: {e}"))?;
    Ok(serde_json::json!({ "unlocked": key }))
}

/// The device's drive opened again: over a keyring without its key (`locked`), or over the
/// remote's (the key back), as AzDrive opens an encrypted drive.
#[cfg(feature = "encryption")]
fn reopen(s: &mut DriveState, args: &Args, locked: bool) -> Result<(), String> {
    use std::sync::Arc;

    use azul_storage::{
        encrypted::{AutoEncrypted, IndexProvider},
        keyring::KeyringStore,
        Drive,
    };

    use super::remote::{FaultDrive, MemKeyring};

    let (name, drive_id, remote, run) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.name.clone(), d.drive_id.clone(), h.remote_of(args)?, h.run.clone()))
    })?;
    if !remote.encrypted {
        return Err(String::from(
            "a plain remote has no drive key to lock: drive_create it with \"encrypted\": true",
        ));
    }
    let index = s
        .slot_index(&drive_id)
        .ok_or_else(|| format!("AzDrive has no drive {drive_id}"))?;
    let bucket: Arc<dyn Drive> = Arc::new(FaultDrive {
        remote: remote.clone(),
        device: name.clone(),
    });
    let keyring: Arc<dyn KeyringStore> = if locked {
        Arc::new(MemKeyring::default())
    } else {
        remote.keyring.clone()
    };
    let index_cache = run.join("devices").join(&name).join("index-cache");
    let provider: Arc<dyn IndexProvider> =
        Arc::new(crate::encryption::meta_provider(Some(index_cache)));
    let auto = Arc::new(AutoEncrypted::new(
        bucket,
        &remote.drive_name,
        keyring,
        Some(provider),
    ));
    let slot = &mut s.slots[index];
    slot.drive = Some(auto.clone() as Arc<dyn Drive>);
    slot.auto = Some(auto);
    Ok(())
}

#[cfg(not(feature = "encryption"))]
fn reopen(_s: &mut DriveState, _args: &Args, _locked: bool) -> Result<(), String> {
    Err(String::from(
        "this AzDrive was built without encryption: no drive key to lock",
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_files_lock_is_exclusive_until_let_go() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-lock");
        let path = dir.path().join("open-in-word.docx");
        std::fs::write(&path, b"v2").unwrap();
        let held = std::fs::File::options().read(true).write(true).open(&path).unwrap();
        held.lock().unwrap();
        let reader = std::fs::File::open(&path).unwrap();
        assert!(matches!(reader.try_lock_shared(), Err(std::fs::TryLockError::WouldBlock)));
        held.unlock().unwrap();
        assert!(reader.try_lock_shared().is_ok());
    }
}
