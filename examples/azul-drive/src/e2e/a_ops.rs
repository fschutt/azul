//! The window's actions around a pass and the computer around AzDrive: Cancel in the
//! transfers window (`drive_cancel`), the operations log as data (`drive_ops`), putting back
//! what a pass trashed (`drive_untrash`), the pairing's settings (`drive_setting`,
//! `drive_settings`), the network AzDrive reads (`drive_network`) and the system waking up
//! (`drive_wake`).

use std::sync::atomic::Ordering;

use azcloud_kit::sync::{
    oplog::{OpKind, OpLog},
    session::SyncSetup,
};
use azul::{
    prelude::*,
    sensor::{NetworkKind, NetworkState},
};

use super::{a_pair::change_setup, need, str_arg, with_harness, Answer, Args};
use crate::{sync_jobs, sync_view, DriveState};

/// `drive_cancel`: Cancel while a pass runs - it stops before its next file, and nothing half
/// done is committed; a pass held at a pause point is let go so it can stop.
pub(super) fn cancel(s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, folder) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), d.pair_folder.clone()))
    })?;
    let cancelled = match s.sync_view.drives.get_mut(&drive_id) {
        Some(sync) => {
            sync.again = false;
            sync.running.as_ref().map(|running| running.cancel.store(true, Ordering::SeqCst)).is_some()
        }
        None => false,
    };
    azcloud_kit::sync::faults::release(&folder);
    Ok(serde_json::json!({ "cancelled": cancelled }))
}

/// Why a destructive step of the log happened, in a sentence (each kind has one cause).
fn reason_of(kind: OpKind) -> &'static str {
    match kind {
        OpKind::Trash => "deleted on the drive: moved into this folder's trash",
        OpKind::Delete => "deleted on the drive: removed (this disk has no trash)",
        OpKind::Overwrite => "a newer version from the drive replaced it",
        OpKind::Conflict => "changed here and on the drive: this device's version was renamed",
        OpKind::Move => "renamed or moved on the drive",
    }
}

/// `drive_ops`: the device's operations log as data, oldest first: `ops`
/// `[{at, op, key, to, hash, reason}]` (`at` in seconds on the pass's clock).
pub(super) fn ops(args: &Args) -> Answer {
    let state_dir = with_harness(|h| {
        h.device(args)?
            .state_dir
            .clone()
            .ok_or_else(|| String::from("the device is not paired"))
    })?;
    let ops: Vec<serde_json::Value> = OpLog::new(&state_dir)
        .read()
        .into_iter()
        .map(|op| {
            serde_json::json!({
                "at": op.t,
                "op": op.op,
                "key": op.key,
                "to": op.to,
                "hash": op.hash,
                "reason": reason_of(op.op),
            })
        })
        .collect();
    Ok(serde_json::json!({ "ops": ops }))
}

/// `drive_untrash`: what the device's passes trashed of `path` (a file or a folder) put back
/// where it was; the next pass sends it up again. Answers `restored` (keys).
pub(super) fn untrash(args: &Args) -> Answer {
    let (root, state_dir) = with_harness(|h| {
        let d = h.device(args)?;
        let dir = d.state_dir.clone().ok_or("the device is not paired")?;
        Ok((d.root.clone(), dir))
    })?;
    let key = str_arg(args, "path").unwrap_or("").trim_matches('/').to_string();
    let restored = azcloud_kit::sync::trash::untrash(&root, &state_dir, &key).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "restored": restored }))
}

/// Sets one of a pairing's settings, by the name the settings file uses (`auto_download`,
/// `keep_gb`, `paused`, `sync_on_metered`, `backup`, `exclude`, `local_copies`), plus `mode`
/// and `remote_deletes` as `drive_pair` takes them. The pairing's identity (its drive, folders)
/// is not a setting.
pub(super) fn set_one(setup: &mut SyncSetup, name: &str, value: &serde_json::Value) -> Result<(), String> {
    let (name, value) = match (name, value.as_str()) {
        ("mode", Some("backup" | "camera-upload")) | ("remote_deletes", Some("keep")) => {
            ("backup", serde_json::Value::Bool(true))
        }
        ("mode", Some("two-way")) | ("remote_deletes", Some("delete")) => ("backup", serde_json::Value::Bool(false)),
        ("mode" | "remote_deletes", _) => return Err(format!("{name} {value} is not a mode")),
        ("drive_id" | "prefix" | "folder", _) => {
            return Err(format!("'{name}' is the pairing itself, not a setting (pair again)"))
        }
        _ => (name, value.clone()),
    };
    let mut json = serde_json::to_value(&*setup).map_err(|e| e.to_string())?;
    let map = json.as_object_mut().ok_or("the settings are no object")?;
    if !map.contains_key(name) && !matches!(name, "backup" | "exclude") {
        return Err(format!("no setting '{name}'"));
    }
    map.insert(name.to_string(), value);
    *setup = serde_json::from_value(json).map_err(|e| format!("setting '{name}': {e}"))?;
    Ok(())
}

/// `drive_setting`: one setting of the device's pairing, kept as the window keeps it.
pub(super) fn setting(s: &mut DriveState, args: &Args) -> Answer {
    let name = need(args, "name")?.to_string();
    let value = args.get("value").cloned().unwrap_or(serde_json::Value::Null);
    let mut failed = None;
    let after = change_setup(s, args, |setup| {
        if let Err(e) = set_one(setup, &name, &value) {
            failed = Some(e);
        }
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    Ok(serde_json::json!({ "settings": after }))
}

/// `drive_settings`: the device's pairing's settings, after the ones in `set` (an object of
/// name: value) were set.
pub(super) fn settings(s: &mut DriveState, args: &Args) -> Answer {
    let set = args.get("set").and_then(serde_json::Value::as_object).cloned().unwrap_or_default();
    let mut failed = None;
    let after = change_setup(s, args, |setup| {
        for (name, value) in &set {
            if let Err(e) = set_one(setup, name, value) {
                failed = Some(e);
                return;
            }
        }
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    Ok(serde_json::json!({ "settings": after }))
}

/// `drive_network`: the network as AzDrive reads it from the system (azul's `NetworkState`):
/// `metered` (bool), `constrained`, `hotspot`, `connected` (default true). Every synced drive
/// says its status again; the next pass holds big transfers back on a costly network.
pub(super) fn network(s: &mut DriveState, args: &Args) -> Answer {
    let flag = |k: &str, default: bool| args.get(k).and_then(serde_json::Value::as_bool).unwrap_or(default);
    let metered = flag("metered", false);
    let network = NetworkState {
        kind: if metered { NetworkKind::Cellular } else { NetworkKind::Wired },
        connected: flag("connected", true),
        metered,
        constrained: flag("constrained", false),
        hotspot: flag("hotspot", false),
    };
    s.sync_view.network = Some(network);
    // The computer's health reads the same network flags.
    if let Some(device) = s.sync_view.device.as_mut() {
        device.connected = network.connected;
        device.metered = network.metered;
        device.constrained = network.constrained;
        device.hotspot = network.hotspot;
    }
    for setup in s.settings.synced.clone() {
        sync_view::say_status(s, &setup.drive_id);
    }
    Ok(serde_json::json!({
        "metered": network.metered, "constrained": network.constrained,
        "hotspot": network.hotspot, "connected": network.connected,
    }))
}

/// `drive_status` as the shared command answers it, plus `unsynced_for_s`: how long the oldest
/// change kept from the drive has waited, by the device's sync clock (azcloud-kit's
/// `ProblemList::unsynced_since`; `null` when nothing waits) - the "not backed up since" alarm.
pub(super) fn status(s: &DriveState, args: &Args) -> Answer {
    let mut answer = super::status(s, args)?;
    let (state_dir, clock) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.state_dir.clone(), d.clock))
    })?;
    let since = state_dir
        .and_then(|dir| azcloud_kit::sync::problems::ProblemList::load(&dir).ok())
        .and_then(|list| list.unsynced_since());
    let now = azcloud_kit::now() as f64 + clock;
    let waited = since.map(|since| (now - since as f64).max(0.0).round() as i64);
    if let serde_json::Value::Object(map) = &mut answer {
        map.insert(String::from("unsynced_for_s"), serde_json::json!(waited));
    }
    Ok(answer)
}

/// `drive_wake`: the system woke from sleep (or its network changed), as AzDrive hears it:
/// every synced drive that is not paused gets a full pass now. Answers `passes` (the drives
/// asked).
pub(super) fn wake(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) -> Answer {
    let asked = sync_jobs::woke(info, app, s);
    Ok(serde_json::json!({ "passes": asked }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use azcloud_kit::sync::session::AutoDownload;

    #[test]
    fn a_setting_is_set_by_its_name_and_the_pairing_itself_is_none() {
        let mut setup = SyncSetup::new("e2e-a", "", std::path::Path::new("/tmp/a"));
        set_one(&mut setup, "remote_deletes", &serde_json::json!("keep")).unwrap();
        assert!(setup.backup);
        set_one(&mut setup, "mode", &serde_json::json!("two-way")).unwrap();
        assert!(!setup.backup);
        set_one(&mut setup, "auto_download", &serde_json::json!({ "new_under": 1 })).unwrap();
        assert_eq!(setup.auto_download, AutoDownload::NewUnder(1));
        set_one(&mut setup, "exclude", &serde_json::json!(["big/*"])).unwrap();
        assert_eq!(setup.exclude, vec![String::from("big/*")]);
        set_one(&mut setup, "keep_gb", &serde_json::json!(2)).unwrap();
        assert_eq!(setup.keep_gb, Some(2));
        assert!(set_one(&mut setup, "folder", &serde_json::json!("/elsewhere")).is_err());
        assert!(set_one(&mut setup, "colour", &serde_json::json!("red")).is_err());
        assert!(set_one(&mut setup, "paused", &serde_json::json!("yes")).is_err());
    }

    #[test]
    fn every_step_of_the_log_says_why() {
        for kind in [OpKind::Trash, OpKind::Delete, OpKind::Overwrite, OpKind::Conflict, OpKind::Move] {
            assert!(!reason_of(kind).is_empty());
        }
    }
}
