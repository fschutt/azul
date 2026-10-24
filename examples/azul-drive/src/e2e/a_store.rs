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

/// `drive_uninstall`: AzDrive's "Remove drive" for the device's drive, confirmed - the drive
/// leaves the source list and its keys the keyring; its pairing stops and its state folder
/// goes. Every file of the folder stays as it is (a cloud-only one stays on the drive).
pub(super) fn uninstall(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let drive_id = with_harness(|h| Ok(h.device(args)?.drive_id.clone()))?;
    if s.slot_index(&drive_id).is_none() {
        return Err(format!("AzDrive has no drive {drive_id} (removed already?)"));
    }
    s.popup = Some(Popup::ConfirmForget {
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

/// The checks group A answers (`drive_check` with one of these names).
pub(super) const CHECKS: &[&str] = &["no_registration"];

/// Runs group A's check `name`: `ok` and `detail`, `success` as `ok` (as every check).
pub(super) fn check(s: &DriveState, args: &Args) -> Answer {
    let name = str_arg(args, "check").unwrap_or_default().to_string();
    let verdict = match name.as_str() {
        "no_registration" => no_registration(s, args),
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
}
