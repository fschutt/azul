//! What a pairing keeps on this computer, as other versions and other days leave it: its state
//! folder in the previous release's format or a newer one's (`drive_state_format`), a copy of it
//! put back later (`drive_state_save` / `drive_state_load`: a restored backup, a reinstall that
//! keeps an old configuration), and AzDrive's removal of the drive (`drive_uninstall`, checked
//! by `drive_check` `no_registration`).

use std::{fs, path::Path};

use azcloud_kit::sync::{
    local::LOCAL_VERSION,
    session::{INDEX_FILE, STATES_FILE},
};
use azul::prelude::*;

use super::{files::clean, need, str_arg, with_harness, Answer, Args};
use crate::{DriveState, Popup};

/// What the previous release's base did not have yet: the fields added since, each with a
/// default an older base reads as (`#[serde(default)]`).
const NEWER_BASE_FIELDS: &[&str] = &["root_id", "restore_epoch", "volume", "kept", "guard", "remote_etag"];
const NEWER_ENTRY_FIELDS: &[&str] = &["ctime_ns", "ino", "dev"];
const NEWER_STATES_FIELDS: &[&str] = &["freed", "newer_format", "burst", "mass_delete", "skipped", "last_problem"];

fn state_dir_of(args: &Args) -> Result<std::path::PathBuf, String> {
    with_harness(|h| {
        h.device(args)?
            .state_dir
            .clone()
            .ok_or_else(|| String::from("the device is not paired"))
    })
}

fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    text.push(b'\n');
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The base and the states of `dir` as the previous release wrote them (the fields added since
/// left out), or as a newer one would (a format version above this one's).
pub(super) fn rewrite(dir: &Path, version: &str) -> Result<(), String> {
    let base_path = dir.join(INDEX_FILE);
    let mut base = read_json(&base_path)?;
    let map = base.as_object_mut().ok_or("the base is no JSON object")?;
    match version {
        "previous" => {
            for field in NEWER_BASE_FIELDS {
                map.remove(*field);
            }
            map.insert(String::from("version"), 1.into());
            if let Some(files) = map.get_mut("files").and_then(serde_json::Value::as_object_mut) {
                for entry in files.values_mut().filter_map(serde_json::Value::as_object_mut) {
                    for field in NEWER_ENTRY_FIELDS {
                        entry.remove(*field);
                    }
                }
            }
            let states_path = dir.join(STATES_FILE);
            if states_path.exists() {
                let mut states = read_json(&states_path)?;
                if let Some(map) = states.as_object_mut() {
                    for field in NEWER_STATES_FIELDS {
                        map.remove(*field);
                    }
                }
                write_json(&states_path, &states)?;
            }
        }
        "newer" => {
            map.insert(String::from("version"), (LOCAL_VERSION + 1).into());
            map.insert(String::from("state_features"), serde_json::json!(["a-newer-base"]));
        }
        other => return Err(format!("version '{other}' is neither previous nor newer")),
    }
    write_json(&base_path, &base)
}

/// `drive_state_format`: the device's state folder as an upgrade (`previous`) or a downgrade
/// (`newer`) finds it; `drive_restart` reads it.
pub(super) fn state_format(args: &Args) -> Answer {
    let dir = state_dir_of(args)?;
    let version = need(args, "version")?;
    rewrite(&dir, version)?;
    Ok(serde_json::json!({ "version": version }))
}

/// Where `drive_state_save` keeps the device's copies: `<run>/state-copies/<device>/<name>`.
fn copy_dir(args: &Args) -> Result<std::path::PathBuf, String> {
    let name = need(args, "name")?;
    if name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(format!("'{name}' is no name for a copy"));
    }
    with_harness(|h| {
        let d = h.device(args)?;
        Ok(h.run.join("state-copies").join(&d.name).join(name))
    })
}

/// Copies every file under `from` into `to` (made fresh).
fn copy_tree(from: &Path, to: &Path) -> Result<usize, String> {
    if to.exists() {
        clean(to);
    }
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut copied = 0;
    for entry in fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?.flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            copied += copy_tree(&path, &target)?;
        } else if meta.is_file() {
            fs::copy(&path, &target).map_err(|e| format!("{}: {e}", path.display()))?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// `drive_state_save`: a copy of the device's state folder under `name`.
pub(super) fn state_save(args: &Args) -> Answer {
    let dir = state_dir_of(args)?;
    let copy = copy_dir(args)?;
    let files = copy_tree(&dir, &copy)?;
    Ok(serde_json::json!({ "files": files }))
}

/// `drive_state_load`: the copy `name` back as the device's state folder (what is there now
/// goes); `drive_restart` reads it.
pub(super) fn state_load(s: &DriveState, args: &Args) -> Answer {
    let (dir, drive_id) = with_harness(|h| {
        let d = h.device(args)?;
        let dir = d.state_dir.clone().ok_or("the device is not paired")?;
        Ok((dir, d.drive_id.clone()))
    })?;
    if s
        .sync_view
        .drives
        .get(&drive_id)
        .is_some_and(|d| d.running.is_some())
    {
        return Err(String::from("a pass runs: wait for it first (drive_wait_idle)"));
    }
    let copy = copy_dir(args)?;
    if !copy.is_dir() {
        return Err(format!("no copy '{}' (drive_state_save one)", need(args, "name")?));
    }
    let files = copy_tree(&copy, &dir)?;
    Ok(serde_json::json!({ "files": files }))
}

/// `drive_corrupt_settings`: AzDrive's settings file cut off mid-write, as a crash or a full
/// disk leaves it (its last good copy stays); `drive_upgrade` reads it as a start does.
pub(super) fn corrupt_settings(s: &DriveState) -> Answer {
    let drive = s
        .settings_drive
        .clone()
        .ok_or("AzDrive keeps no settings file in this run")?;
    let text = s.settings.to_json();
    let cut = &text[..text.len() / 2];
    azul_storage::Drive::put(&drive, crate::SETTINGS_KEY, cut.as_bytes()).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "bytes": cut.len() }))
}

/// `drive_upgrade`: AzDrive started again (a new version's first start): the settings read
/// from their file - the last good copy when it is damaged - and the device's sync state from
/// its state folder.
pub(super) fn upgrade(s: &mut DriveState, args: &Args) -> Answer {
    let drive = s
        .settings_drive
        .clone()
        .ok_or("AzDrive keeps no settings file in this run")?;
    s.settings = crate::read_settings(&drive);
    crate::sync_jobs::publish(s);
    super::restart(s, args)?;
    Ok(serde_json::json!({ "pairings": s.settings.synced.len() }))
}

/// `drive_unpair` with `fail_state_removal`: "Stop syncing" whose removal of the state folder
/// was cut off half way - the pairing is gone, the base (and its cached index) deleted, the
/// rest of the state left. Pairing the folder again must delete nothing anywhere.
pub(super) fn unpair_cut(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, state_dir) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), d.state_dir.clone()))
    })?;
    crate::sync_jobs::stop(info, app, s, &drive_id);
    let mut removed = Vec::new();
    if let Some(dir) = state_dir {
        for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // The base first, as a removal in name order meets it; what follows stays.
            if name.starts_with("index") {
                fs::remove_file(entry.path()).map_err(|e| format!("{name}: {e}"))?;
                removed.push(name);
            }
        }
    }
    Ok(serde_json::json!({ "removed": removed }))
}

/// `drive_uninstall`: AzDrive's "Remove drive" for the device's drive, confirmed - the drive
/// leaves the source list and its keys the keyring; its pairing stops and its state folder
/// goes. Every file of the folder stays as it is (a cloud-only one stays on the drive).
pub(super) fn uninstall(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let drive_id = with_harness(|h| Ok(h.device(args)?.drive_id.clone()))?;
    if s.slot_index(&drive_id).is_none() {
        return Err(format!("AzDrive has no drive {drive_id} (removed already?)"));
    }
    s.popup.open(Popup::ConfirmForget {
        drive_id: drive_id.clone(),
    });
    crate::actions::forget_drive(info, app, s);
    Ok(serde_json::json!({ "removed": drive_id }))
}

/// `drive_check` `no_registration`: nothing of the device's drive is left in AzDrive - no
/// source-list entry, no pairing, no state folder.
pub(super) fn no_registration(s: &DriveState, args: &Args) -> Result<(), String> {
    let (name, drive_id, state_dir) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.name.clone(), d.drive_id.clone(), d.state_dir.clone()))
    })?;
    let mut left = Vec::new();
    if s.slot_index(&drive_id).is_some() {
        left.push(String::from("its drive in the source list"));
    }
    if s.settings.synced.iter().any(|p| p.drive_id == drive_id) {
        left.push(String::from("its pairing in the settings"));
    }
    if let Some(dir) = state_dir.filter(|d| d.exists()) {
        left.push(format!("its state folder {}", dir.display()));
    }
    if left.is_empty() {
        Ok(())
    } else {
        Err(format!("{name} left behind: {}", left.join(", ")))
    }
}

/// `drive_check` `state_size`: the device's state folder (the base, its journal, the states,
/// the problem list, the operations log) holds at most `max_bytes`.
fn state_size(args: &Args) -> Result<(), String> {
    let dir = state_dir_of(args)?;
    let max = args
        .get("max_bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'max_bytes' is missing")?;
    let total: u64 = super::files::all_files(&dir)
        .iter()
        .filter_map(|(_, p)| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    if total <= max {
        Ok(())
    } else {
        Err(format!("the state folder holds {total} bytes, more than {max}"))
    }
}

/// What the sync engine and the storage layer name their temporary files.
fn engine_temp(name: &str) -> bool {
    name.starts_with(".azsync-tmp-")
        || (name.starts_with("azul-storage-") && name.ends_with(".azl1-spool"))
        || (name.starts_with('.') && name.contains(".azul-storage-") && name.ends_with(".tmp"))
}

/// `drive_check` `system_temp_untouched`: nothing of the sync engine is left in the process's
/// system temporary folder (a download is spooled on the folder's own disk, an upload's spool
/// is removed): no entry of the engine's names made since the sandbox was.
fn system_temp_untouched() -> Result<(), String> {
    let run = with_harness(|h| Ok(h.run.clone()))?;
    let started = fs::metadata(&run)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .map_err(|e| format!("{}: {e}", run.display()))?;
    let tmp = std::env::temp_dir();
    let mut left = Vec::new();
    for entry in fs::read_dir(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !engine_temp(&name) {
            continue;
        }
        let made = entry
            .metadata()
            .and_then(|m| m.created().or_else(|_| m.modified()))
            .ok();
        if made.is_some_and(|t| t >= started) {
            left.push(name);
        }
    }
    if left.is_empty() {
        Ok(())
    } else {
        left.sort();
        left.truncate(5);
        Err(format!("the system temp folder {} holds {left:?}", tmp.display()))
    }
}

/// `drive_check` `peak_memory`: the process took at most `max_mb` MiB at any time so far (its
/// peak resident set: a pass over a burst of files must stream).
fn peak_memory(args: &Args) -> Result<(), String> {
    let max = args
        .get("max_mb")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'max_mb' is missing")?;
    let peak = azcloud_kit::sync::memory::peak_bytes()
        .ok_or("this system does not say a process's peak memory")?;
    let mb = peak / (1024 * 1024);
    if mb <= max {
        Ok(())
    } else {
        Err(format!("the process took {mb} MiB at its peak, more than {max}"))
    }
}

/// The checks group A answers (`drive_check` with one of these names).
pub(super) const CHECKS: &[&str] = &["no_registration", "state_size", "system_temp_untouched", "peak_memory"];

/// Runs group A's check `name`: `ok` and `detail`, `success` as `ok` (as every check).
pub(super) fn check(s: &DriveState, args: &Args) -> Answer {
    let name = str_arg(args, "check").unwrap_or_default().to_string();
    let verdict = match name.as_str() {
        "no_registration" => no_registration(s, args),
        "state_size" => state_size(args),
        "system_temp_untouched" => system_temp_untouched(),
        "peak_memory" => peak_memory(args),
        other => Err(format!("no check '{other}'")),
    };
    Ok(match verdict {
        Ok(()) => serde_json::json!({ "ok": true, "detail": "" }),
        Err(detail) => serde_json::json!({
            "ok": false, "success": false, "detail": detail, "error": format!("{name}: {detail}"),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use azcloud_kit::sync::local::LocalIndex;

    #[test]
    fn the_previous_format_leaves_out_the_newer_fields_and_a_newer_one_is_refused() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-state-format");
        let mut index = LocalIndex::new(dir.path(), "bucket", "");
        index.root_id = Some(String::from("abc"));
        index.files.insert(
            String::from("a.txt"),
            azcloud_kit::sync::local::BaseEntry {
                hash: String::from("h"),
                size: 1,
                mtime_ns: 5,
                ctime_ns: 6,
                ino: 7,
                dev: 8,
                ..Default::default()
            },
        );
        index.save(&dir.path().join(INDEX_FILE)).unwrap();
        rewrite(dir.path(), "previous").unwrap();
        let old = LocalIndex::load(&dir.path().join(INDEX_FILE)).unwrap().unwrap();
        assert_eq!(old.root_id, None);
        assert_eq!((old.files["a.txt"].ino, old.files["a.txt"].mtime_ns), (0, 5));
        rewrite(dir.path(), "newer").unwrap();
        assert!(LocalIndex::load(&dir.path().join(INDEX_FILE)).is_err());
        assert!(rewrite(dir.path(), "sideways").is_err());
    }

    #[test]
    fn a_damaged_settings_file_is_read_from_its_last_good_copy() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-settings");
        let drive = azul_storage::LocalDrive::new(dir.path().to_path_buf());
        let mut settings = crate::model::Settings::default();
        let mut setup = azcloud_kit::sync::session::SyncSetup::new("e2e-b", "", Path::new("/b"));
        setup.backup = true;
        settings.synced.push(setup);
        crate::write_settings(&drive, &settings.to_json()).unwrap();
        assert_eq!(crate::read_settings(&drive), settings);
        let text = settings.to_json();
        azul_storage::Drive::put(&drive, crate::SETTINGS_KEY, &text.as_bytes()[..text.len() / 2]).unwrap();
        let read = crate::read_settings(&drive);
        assert!(read.synced.first().is_some_and(|p| p.backup), "the backup pairing kept");
    }

    #[test]
    fn the_engines_temporary_files_are_told_by_their_names() {
        assert!(engine_temp("azul-storage-00ff00ff00ff00ff.azl1-spool"));
        assert!(engine_temp(".azsync-tmp-1234"));
        assert!(engine_temp(".report.txt.azul-storage-77.tmp"));
        assert!(!engine_temp("azul-storage-period-tokens"));
        assert!(!engine_temp("AzDrive-open"));
    }
}
