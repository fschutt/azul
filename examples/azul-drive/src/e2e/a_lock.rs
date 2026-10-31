//! Locks, two kinds: another app holding a file of the folder locked (`drive_lock` with a
//! `path`: an exclusive lock, as Office takes on Windows and an app mid-write takes with
//! `flock`), and the drive's key not on the device (`drive_lock` without one: a locked
//! keyring - every pass fails as "sign in", changing nothing, until `drive_unlock` or the
//! recovery code gives the key back).

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

/// The drive's key off the device (`locked`), or back on it: the key's keyring entries (the
/// drive key and this device's member secret) leave the device's keyring - kept aside - or go
/// back, and every device drive over that keyring decides again on its next call, as after
/// the system keyring locked. A joined device shares its keyring with the remote's other
/// joined devices (one user's computers, as one system keyring): they lock together. The
/// recovery code (`drive_recover`) puts the key back too.
#[cfg(feature = "encryption")]
fn reopen(s: &mut DriveState, args: &Args, locked: bool) -> Result<(), String> {
    use azul_storage::{
        crypto::device::{drive_key_entry, member_key_entry},
        keyring::KeyringStore,
    };

    let (name, remote, sharing, keyring) = with_harness(|h| {
        let d = h.device(args)?;
        let remote = h.remote_of(args)?;
        // The keyring the device keeps the key in (its own when it is not joined).
        let keyring = super::c_keys::keyring_of(h, &d.name)?;
        let sharing: Vec<String> = h
            .devices
            .iter()
            .filter(|o| o.remote == d.remote)
            .map(|o| o.drive_id.clone())
            .collect();
        Ok((d.name.clone(), remote, sharing, keyring))
    })?;
    if !remote.encrypted {
        return Err(String::from(
            "a plain remote has no drive key to lock: drive_create it with \"encrypted\": true",
        ));
    }
    let entries = [drive_key_entry(&remote.drive_name), member_key_entry(&remote.drive_name)];
    if locked {
        let mut kept = Vec::new();
        for entry in &entries {
            if let Some(secret) = keyring.get(entry).map_err(|e| e.to_string())? {
                kept.push((entry.clone(), secret));
            }
            keyring.delete(entry).map_err(|e| e.to_string())?;
        }
        if kept.is_empty() {
            return Err(format!("{name} holds no key of the drive to lock"));
        }
        with_a(|a| a.keys_aside.insert(remote.name.clone(), kept))?;
    } else {
        let kept = with_a(|a| a.keys_aside.remove(&remote.name))?.unwrap_or_default();
        for (entry, secret) in &kept {
            keyring.set(entry, secret).map_err(|e| e.to_string())?;
        }
    }
    for drive_id in sharing {
        if let Some(auto) = s.slot_index(&drive_id).and_then(|i| s.slots[i].auto.clone()) {
            auto.reopen();
        }
    }
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
