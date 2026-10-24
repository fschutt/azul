//! A pairing's cloud-only files, as the window and the system handle them: "Free up space"
//! (`drive_evict`: the bytes leave this device once the drive holds them whole), bringing them
//! down (`drive_hydrate`), an app that writes one (`drive_write_through_placeholder`: brought
//! down first, and refused when it would eat the disk's reserve), a damaged record of one
//! (`drive_corrupt_placeholder`) and a virus scanner reading the folder (`drive_scan_read`).

use std::{collections::BTreeSet, fs, io::Read, path::Path};

use azcloud_kit::sync::{
    evict::note_problem,
    faults,
    local::LocalIndex,
    problems::{Code, Problem},
    session::{SyncStates, INDEX_FILE},
    DISK_RESERVE,
};
use azul::prelude::*;

use super::{
    files::{content_of, user_files},
    need, problems_in, remote::glob, str_arg, waiting_change, with_harness, Answer, Args,
};
use crate::{sync_jobs, DriveState};

/// The keys `args` names on the device: `path` (a file, or a folder and all in it, `""` the
/// whole folder) or `glob` (over its files' keys), as the states and the folder know them.
fn targets(states: &SyncStates, root: &Path, args: &Args) -> Result<Vec<String>, String> {
    let here: BTreeSet<String> = user_files(root).into_iter().map(|(rel, _)| rel).collect();
    let known = || states.files.keys().cloned().chain(here.iter().cloned());
    if let Some(pattern) = str_arg(args, "glob") {
        let keys: BTreeSet<String> = known().filter(|k| glob(pattern, k)).collect();
        return Ok(keys.into_iter().collect());
    }
    let key = key_of(root, need(args, "path")?)?;
    if key.is_empty() {
        return Ok(known().collect::<BTreeSet<_>>().into_iter().collect());
    }
    let folder = format!("{key}/");
    let under: BTreeSet<String> = known().filter(|k| k.starts_with(&folder)).collect();
    if root.join(&key).is_dir() || (!under.is_empty() && !states.files.contains_key(&key)) {
        return Ok(under.into_iter().collect());
    }
    Ok(vec![key])
}

/// What the window's job is asked for: the folder itself (`dir/`, `""` for the whole folder)
/// when `path` names one - so a folder's own pin goes with it, as the ribbon's command does -
/// else the keys [`targets`] names.
fn job_keys(states: &SyncStates, root: &Path, args: &Args) -> Result<Vec<String>, String> {
    if let Some(path) = str_arg(args, "path") {
        let key = key_of(root, path)?;
        if key.is_empty() {
            return Ok(vec![String::new()]);
        }
        let folder = format!("{key}/");
        let in_states = states.files.keys().any(|k| k.starts_with(&folder));
        if root.join(&key).is_dir() || (in_states && !states.files.contains_key(&key)) {
            return Ok(vec![folder]);
        }
    }
    targets(states, root, args)
}

/// A path argument as the pairing's key (`""`: its whole folder).
fn key_of(root: &Path, path: &str) -> Result<String, String> {
    let rel = if Path::new(path).is_absolute() {
        Path::new(path)
            .strip_prefix(root)
            .map_err(|_| format!("{path} is not in the device's folder"))?
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        path.replace('\\', "/")
    };
    let mut rest = rel.as_str();
    while let Some(after) = rest.strip_prefix("./") {
        rest = after;
    }
    let rest = rest.trim_matches('/');
    Ok(if rest == "." { String::new() } else { rest.to_string() })
}

/// The device's drive id, its folder and its pairing's states as the window holds them.
fn device_states(s: &DriveState, args: &Args) -> Result<(String, std::path::PathBuf, std::sync::Arc<SyncStates>), String> {
    let (drive_id, root) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), d.root.clone()))
    })?;
    let states = s.sync_view.store.states(&drive_id);
    Ok((drive_id, root, states))
}

/// Starts a window's job for the device on the first call (`start` says whether it could),
/// then answers `pending` until it landed: `None` while it waits.
fn job(args: &Args, start: impl FnOnce(&str) -> bool) -> Result<Option<serde_json::Value>, String> {
    let answer = waiting_change(args, |drive_id| {
        if start(drive_id) {
            Ok(())
        } else {
            Err(String::from(
                "AzDrive could not open the device's drive for the job (drive_status says why)",
            ))
        }
    })?;
    Ok(answer.get("pending").is_none().then_some(answer))
}

/// Whether a job of the device was started and has not landed yet.
fn job_waiting(args: &Args) -> Result<bool, String> {
    with_harness(|h| Ok(h.device(args)?.change_wanted.is_some()))
}

/// `drive_evict`: "Free up space" for the files named. Answers `evicted` (the named files in
/// the cloud only now), `refused` (still here) and `reasons` (why each refused one stays).
pub(super) fn evict(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, root, states) = device_states(s, args)?;
    let keys = job_keys(&states, &root, args)?;
    if targets(&states, &root, args)?.is_empty() && !job_waiting(args)? {
        return Ok(serde_json::json!({ "evicted": [], "refused": [], "reasons": {} }));
    }
    if job(args, |id| sync_jobs::free_up(info, app, s, id, keys.clone()))?.is_none() {
        return Ok(super::pending());
    }
    // What it came to, from the states the job answered with.
    let states = s.sync_view.store.states(&drive_id);
    let state_dir = with_harness(|h| Ok(h.device(args)?.state_dir.clone()))?;
    let problems = problems_in(state_dir.as_deref());
    let mut evicted = Vec::new();
    let mut refused = Vec::new();
    let mut reasons = serde_json::Map::new();
    for key in targets(&states, &root, args)? {
        match states.files.get(&key) {
            Some(record) if record.cloud_only => evicted.push(key),
            record => {
                let damaged = problems.iter().find(|p| {
                    p.get("key").and_then(serde_json::Value::as_str) == Some(key.as_str())
                        && p.get("code").and_then(serde_json::Value::as_str) == Some("hash-mismatch")
                });
                let why = match (damaged, record) {
                    (Some(p), _) => p.get("detail").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                    (None, None) => String::from("not synced yet: its only copy is on this device"),
                    (None, Some(_)) => String::from(
                        "the drive does not hold this version whole yet (or it changed here)",
                    ),
                };
                reasons.insert(key.clone(), why.into());
                refused.push(key);
            }
        }
    }
    Ok(serde_json::json!({ "evicted": evicted, "refused": refused, "reasons": reasons }))
}

/// `drive_hydrate`: the named cloud-only files brought down, as opening them does. Answers
/// `hydrated` (here now) and `failed` (still in the cloud only).
pub(super) fn hydrate(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, root, states) = device_states(s, args)?;
    let cloud: Vec<String> = targets(&states, &root, args)?
        .into_iter()
        .filter(|k| states.files.get(k).is_some_and(|r| r.cloud_only))
        .collect();
    let waiting = job_waiting(args)?;
    if cloud.is_empty() && !waiting {
        return Ok(serde_json::json!({ "hydrated": [], "failed": [] }));
    }
    if job(args, |id| sync_jobs::fetch(info, app, s, id, cloud.clone()))?.is_none() {
        return Ok(super::pending());
    }
    let states = s.sync_view.store.states(&drive_id);
    let (hydrated, failed): (Vec<String>, Vec<String>) = targets(&states, &root, args)?
        .into_iter()
        .filter(|k| states.files.contains_key(k))
        .partition(|k| states.files.get(k).is_some_and(|r| !r.cloud_only));
    Ok(serde_json::json!({ "hydrated": hydrated, "failed": failed }))
}

/// `drive_corrupt_placeholder`: what the base keeps of a cloud-only file (its content's name
/// and size) damaged, as a crash or a disk error mid-write leaves it. The next pass must take
/// the drive's word again, without a download and without a delete.
pub(super) fn corrupt_placeholder(args: &Args) -> Answer {
    let (root, state_dir) = with_harness(|h| {
        let d = h.device(args)?;
        let dir = d.state_dir.clone().ok_or("the device is not paired")?;
        Ok((d.root.clone(), dir))
    })?;
    let key = key_of(&root, need(args, "path")?)?;
    let path = state_dir.join(INDEX_FILE);
    let mut index = LocalIndex::load(&path)
        .map_err(|e| e.to_string())?
        .ok_or("the pairing has no base yet")?;
    let entry = index
        .files
        .get_mut(&key)
        .ok_or_else(|| format!("the base has no {key}"))?;
    if !entry.cloud_only {
        return Err(format!("{key} is on this device, not in the cloud only (drive_evict it first)"));
    }
    entry.hash = String::from("0").repeat(64);
    entry.size = 0;
    index.save(&path).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "key": key }))
}

/// `drive_write_through_placeholder`: an app saves a file of an on-demand pairing. A file in
/// the cloud only comes down first (as opening it for writing does); a write that would leave
/// less than the disk's reserve free is refused (`written: false`, listed as `disk-reserve`),
/// so an on-demand folder never fills the disk.
pub(super) fn write_through(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, root, states) = device_states(s, args)?;
    let key = key_of(&root, need(args, "path")?)?;
    let bytes = content_of(args)?;
    // The disk as the sync sees it (a scenario's disk fault is armed under the pairing's folder).
    let pair_folder = with_harness(|h| Ok(h.device(args)?.pair_folder.clone()))?;
    let free = faults::free_bytes(
        &pair_folder,
        azul_storage::beneath::free_space(&root).unwrap_or(u64::MAX),
    );
    if free < (bytes.len() as u64).saturating_add(DISK_RESERVE) {
        let state_dir = with_harness(|h| Ok(h.device(args)?.state_dir.clone()))?;
        let detail = format!(
            "writing {} bytes would leave less than the {DISK_RESERVE} bytes this disk keeps free; \
             nothing was written",
            bytes.len()
        );
        if let Some(dir) = &state_dir {
            note_problem(dir, Problem::new(Code::DiskReserve, &key, detail.clone()));
        }
        return Ok(serde_json::json!({ "written": false, "reason": detail }));
    }
    let cloud_only = states.files.get(&key).is_some_and(|r| r.cloud_only);
    if cloud_only || job_waiting(args)? {
        let keys = vec![key.clone()];
        if job(args, |id| sync_jobs::fetch(info, app, s, id, keys))?.is_none() {
            return Ok(super::pending());
        }
        if s.sync_view.store.states(&drive_id).files.get(&key).is_some_and(|r| r.cloud_only) {
            return Err(format!("{key} could not be brought down first: nothing was written"));
        }
    }
    let path = root.join(&key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(text) = str_arg(args, "text") {
        with_harness(|h| {
            h.texts.insert(text.to_string());
            Ok(())
        })?;
    }
    Ok(serde_json::json!({ "written": true, "key": key }))
}

/// `drive_scan_read`: every file of the folder matching `glob` (default every one) read end to
/// end through the system, as a virus scanner does. Answers `read` (files) and `bytes`.
pub(super) fn scan_read(args: &Args) -> Answer {
    let root = with_harness(|h| Ok(h.device(args)?.root.clone()))?;
    let pattern = str_arg(args, "glob").unwrap_or("*");
    let (mut files, mut bytes) = (0u64, 0u64);
    let mut buf = vec![0u8; 64 * 1024];
    for (rel, path) in user_files(&root) {
        if !glob(pattern, &rel) {
            continue;
        }
        let mut file = fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        loop {
            let n = file.read(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
        }
        files += 1;
    }
    Ok(serde_json::json!({ "read": files, "bytes": bytes }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use azcloud_kit::sync::session::FileRecord;

    fn args(v: serde_json::Value) -> Args {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn a_path_or_a_glob_names_the_pairings_keys_a_folder_with_all_in_it() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-targets");
        let root = dir.path();
        fs::create_dir_all(root.join("media/d000")).unwrap();
        fs::write(root.join("media/d000/a.txt"), b"a").unwrap();
        fs::write(root.join("top.txt"), b"t").unwrap();
        let mut states = SyncStates::default();
        states.files.insert(String::from("media/d000/b.txt"), FileRecord { cloud_only: true, ..FileRecord::default() });
        states.files.insert(String::from("top.txt"), FileRecord::default());
        let all = |v| targets(&states, root, &args(v)).unwrap();
        assert_eq!(all(serde_json::json!({"path": "top.txt"})), vec!["top.txt"]);
        assert_eq!(
            all(serde_json::json!({"path": "media"})),
            vec!["media/d000/a.txt", "media/d000/b.txt"]
        );
        assert_eq!(all(serde_json::json!({"glob": "media/*"})).len(), 2);
        assert_eq!(all(serde_json::json!({"glob": "*"})).len(), 3);
        assert_eq!(all(serde_json::json!({"path": "."})).len(), 3);
        let abs = root.join("top.txt").to_string_lossy().into_owned();
        assert_eq!(all(serde_json::json!({ "path": abs })), vec!["top.txt"]);
        assert!(key_of(root, "/elsewhere/x").is_err());
    }
}
