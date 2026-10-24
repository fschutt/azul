//! The drive itself as the window and an operator see it: the window's listing of a synced
//! drive while a pass runs (`drive_browse`), a drive copied whole to another backend
//! (`drive_migrate`) and a device's drive pointed at it (`drive_repoint`: the same pairing and
//! state, a new address).

use std::{sync::Arc, time::Instant};

use azul::prelude::*;
use azul_storage::{Drive, ListRequest};

use super::{
    remote::{FaultDrive, Remote},
    need, str_arg, with_harness, Answer, Args,
};
use crate::{sync_view, DriveState};

/// `drive_browse`: the window's listing of the device's drive at `path` (`""` its top), as it
/// shows it - a plain synced drive by its sync index's names, an encrypted one by its own
/// listing - answered at once, also while a pass runs. Fails when it took longer than
/// `timeout_ms` (default 60000). Answers `entries` (names, a folder's ending in `/`) and `ms`.
pub(super) fn browse(s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, remote) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), h.remote_of(args)?))
    })?;
    let rel = str_arg(args, "path").unwrap_or("").trim_matches('/').to_string();
    let rel = if rel.is_empty() { rel } else { format!("{rel}/") };
    let limit = args
        .get("timeout_ms")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(60_000);
    let started = Instant::now();
    let entries: Vec<String> = if remote.encrypted {
        let drive = crate::open_drive(s, &drive_id)
            .ok_or_else(|| format!("the drive of {drive_id} cannot be opened"))?;
        let page = drive
            .list(&ListRequest::folder(&format!("{}{rel}", remote.prefix)))
            .map_err(|e| e.to_string())?;
        let strip = |key: &str| key.strip_prefix(&format!("{}{rel}", remote.prefix)).unwrap_or(key).to_string();
        page.folders
            .iter()
            .map(|f| strip(f))
            .chain(page.objects.iter().map(|o| strip(&o.key)))
            .filter(|name| !name.is_empty() && !name.starts_with(".azlin"))
            .collect()
    } else {
        let states = s.sync_view.store.states(&drive_id);
        sync_view::index_rows(&states, &rel, "")
            .into_iter()
            .map(|e| if e.is_folder { format!("{}/", e.name) } else { e.name })
            .collect()
    };
    let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if ms > limit {
        return Err(format!("the listing took {ms} ms, more than {limit}"));
    }
    Ok(serde_json::json!({ "entries": entries, "ms": ms }))
}

/// `drive_migrate`: every stored object of `remote` copied byte for byte to a new remote `to`
/// (its index with the tombstones, its blobs, an encrypted drive's objects and keys), as a
/// backend migration does; the drive's identity and keys stay the same. Answers `objects`.
pub(super) fn migrate(args: &Args) -> Answer {
    let to = need(args, "to")?.to_string();
    let copied = with_harness(|h| {
        let from = h.remote_of(args)?;
        if h.remotes.contains_key(&to) {
            return Err(format!("remote '{to}' exists"));
        }
        let mut target = Remote::create(&to, h.run.join("remote").join(&to), from.encrypted, &from.prefix)
            .map_err(|e| e.to_string())?;
        // The same drive under a new address: its id and its keys stay.
        target.drive_name.clone_from(&from.drive_name);
        target.keyring = from.keyring.clone();
        let copied = copy_objects(&from, &target)?;
        h.remotes.insert(to.clone(), Arc::new(target));
        Ok(copied)
    })?;
    Ok(serde_json::json!({ "remote": to, "objects": copied }))
}

/// Every object of `from` written byte for byte into `to`; how many.
fn copy_objects(from: &Remote, to: &Remote) -> Result<usize, String> {
    let mut copied = 0usize;
    for key in from.keys() {
        if let Some(bytes) = from.read(&key) {
            to.write(&key, &bytes).map_err(|e| format!("{key}: {e}"))?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// `drive_repoint`: the device's drive opened at `remote` instead (the drive moved there): its
/// pairing, its state folder and its folder stay; the next pass finds the drive as it was.
pub(super) fn repoint(s: &mut DriveState, args: &Args) -> Answer {
    let target = need(args, "remote")?.to_string();
    let (name, drive_id, remote, run) = with_harness(|h| {
        let remote = h
            .remotes
            .get(&target)
            .cloned()
            .ok_or_else(|| format!("no remote '{target}'"))?;
        let run = h.run.clone();
        let d = h.device_mut(args)?;
        d.remote.clone_from(&target);
        Ok((d.name.clone(), d.drive_id.clone(), remote, run))
    })?;
    let index = s
        .slot_index(&drive_id)
        .ok_or_else(|| format!("AzDrive has no drive {drive_id}"))?;
    let bucket: Arc<dyn Drive> = Arc::new(FaultDrive {
        remote: remote.clone(),
        device: name.clone(),
    });
    if remote.encrypted {
        open_encrypted(s, index, bucket, &remote, &run.join("devices").join(&name).join("index-cache"))?;
    } else {
        s.slots[index].drive = Some(bucket);
    }
    Ok(serde_json::json!({ "device": name, "remote": target }))
}

#[cfg(feature = "encryption")]
fn open_encrypted(
    s: &mut DriveState,
    index: usize,
    bucket: Arc<dyn Drive>,
    remote: &Remote,
    index_cache: &std::path::Path,
) -> Result<(), String> {
    let provider: Arc<dyn azul_storage::encrypted::IndexProvider> =
        Arc::new(crate::encryption::meta_provider(Some(index_cache.to_path_buf())));
    let auto = Arc::new(azul_storage::encrypted::AutoEncrypted::new(
        bucket,
        &remote.drive_name,
        remote.keyring.clone(),
        Some(provider),
    ));
    s.slots[index].drive = Some(auto.clone() as Arc<dyn Drive>);
    s.slots[index].auto = Some(auto);
    Ok(())
}

#[cfg(not(feature = "encryption"))]
fn open_encrypted(
    _s: &mut DriveState,
    _index: usize,
    _bucket: Arc<dyn Drive>,
    _remote: &Remote,
    _index_cache: &std::path::Path,
) -> Result<(), String> {
    Err(String::from("this AzDrive was built without encryption"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_migration_copies_every_object_byte_for_byte() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-migrate");
        let from = Remote::create("r", dir.path().join("r"), false, "").unwrap();
        let to = Remote::create("r2", dir.path().join("r2"), false, "").unwrap();
        from.write(".azlin/index.json", b"{\"generation\": 3}").unwrap();
        from.write(".azlin/blobs/ab/abcd", b"bytes").unwrap();
        from.write("Caf\u{e9} A.txt", b"named").unwrap();
        assert_eq!(copy_objects(&from, &to).unwrap(), 3);
        assert_eq!(to.keys(), from.keys());
        for key in from.keys() {
            assert_eq!(to.read(&key), from.read(&key), "{key}");
        }
    }
}
