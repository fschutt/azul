//! A pairing's options, as the window sets them: its mode (two-way, or a one-way backup that
//! never deletes on the drive and never changes the folder), its on-demand files, what this
//! device leaves out (selective sync). `drive_pair` with any of them pairs with them set before
//! the first pass; `drive_select` and `drive_on_demand` change a pairing's afterwards.

use std::path::PathBuf;

use azcloud_kit::sync::{
    faults,
    session::{AutoDownload, SyncSetup},
};
use azul::prelude::*;

use super::{bool_arg, str_arg, with_harness, Answer, Args};
use crate::{sync_jobs, sync_view, DriveState};

/// The options of `drive_pair` that only group A knows.
const OPTIONS: &[&str] = &["mode", "remote_deletes", "on_demand", "exclude", "links"];

pub(super) fn has_options(args: &Args) -> bool {
    OPTIONS.iter().any(|k| args.contains_key(*k))
}

/// What the options make of a new pairing's setup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PairOptions {
    pub backup: bool,
    pub on_demand: bool,
    pub exclude: Vec<String>,
}

impl PairOptions {
    pub(super) fn of(args: &Args) -> Result<PairOptions, String> {
        let mut out = PairOptions::default();
        match str_arg(args, "mode") {
            None | Some("two-way") => {}
            // A phone's camera upload is a backup of its gallery: one way, nothing deleted.
            Some("backup" | "camera-upload") => out.backup = true,
            Some(other) => {
                return Err(format!("mode '{other}' is none of two-way, backup, camera-upload"))
            }
        }
        match str_arg(args, "remote_deletes") {
            None | Some("delete") => {}
            // The drive keeps what is deleted here: a backup's rule.
            Some("keep") => out.backup = true,
            Some(other) => return Err(format!("remote_deletes '{other}' is none of keep, delete")),
        }
        out.on_demand = bool_arg(args, "on_demand");
        out.exclude = globs(args, "exclude")?;
        match str_arg(args, "links") {
            None | Some("skip") => {}
            Some(other) => {
                return Err(format!(
                    "links '{other}': the sync keeps no links yet (every symbolic link is \
                     skipped and listed as symlink-skipped); a link policy needs link entries \
                     in the drive's index"
                ))
            }
        }
        Ok(out)
    }

    pub(super) fn apply(&self, setup: &mut SyncSetup) {
        setup.backup = self.backup;
        if self.on_demand {
            setup.auto_download = AutoDownload::Nothing;
        }
        setup.exclude.clone_from(&self.exclude);
    }
}

/// `args[key]` as a list of strings.
fn globs(args: &Args, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("'{key}' holds a value that is no string"))
            })
            .collect(),
        Some(_) => Err(format!("'{key}' is not a list")),
    }
}

/// `drive_pair` with the pairing's options set before its first pass (the shared
/// `drive_pair`'s steps otherwise).
pub(super) fn pair(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let options = PairOptions::of(args)?;
    let (drive_id, folder, prefix) = with_harness(|h| {
        let d = h.device(args)?;
        let folder = str_arg(args, "folder").map_or_else(|| d.root.clone(), PathBuf::from);
        let prefix = match str_arg(args, "prefix") {
            Some(p) => p.to_string(),
            None => h
                .remotes
                .get(&d.remote)
                .map(|r| r.prefix.clone())
                .ok_or_else(|| format!("no remote '{}'", d.remote))?,
        };
        Ok((d.drive_id.clone(), folder, prefix))
    })?;
    sync_jobs::pair_with(info, app, s, &drive_id, &folder.to_string_lossy(), &prefix, |setup| {
        options.apply(setup);
    })
    .map_err(|text| format!("AzDrive refused the pairing: {text:?}"))?;
    let state_dir = sync_view::setup_of(s, &drive_id)
        .zip(s.cache_dir.clone())
        .map(|(setup, cache): (&SyncSetup, PathBuf)| setup.state_dir(&cache.join("sync")));
    with_harness(|h| {
        let d = h.device_mut(args)?;
        if d.pair_folder != folder {
            faults::clear(&d.pair_folder);
        }
        d.root.clone_from(&folder);
        d.pair_folder.clone_from(&folder);
        d.state_dir.clone_from(&state_dir);
        Ok(())
    })?;
    Ok(serde_json::json!({
        "folder": folder, "state_dir": state_dir,
        "backup": options.backup, "on_demand": options.on_demand, "exclude": options.exclude,
    }))
}

/// Changes the setup of `device`'s pairing as the window's Options do (the settings kept).
pub(super) fn change_setup(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    args: &Args,
    change: impl FnOnce(&mut SyncSetup),
) -> Result<SyncSetup, String> {
    let drive_id = with_harness(|h| Ok(h.device(args)?.drive_id.clone()))?;
    let setup = s
        .settings
        .synced
        .iter_mut()
        .find(|p| p.drive_id == drive_id)
        .ok_or_else(|| String::from("the device is not paired (drive_pair it first)"))?;
    change(setup);
    let after = setup.clone();
    sync_jobs::publish(s);
    crate::save_settings(info, app, s);
    sync_view::say_status(s, &drive_id);
    Ok(after)
}

/// `drive_select`: what this device leaves out, from its next pass on.
pub(super) fn select(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let exclude = globs(args, "exclude")?;
    let after = change_setup(info, app, s, args, |setup| setup.exclude.clone_from(&exclude))?;
    Ok(serde_json::json!({ "exclude": after.exclude }))
}

/// `drive_on_demand`: on, nothing comes down until it is opened; off, every file comes down
/// (except what the user freed up), from the next pass on.
pub(super) fn on_demand(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let on = args
        .get("on")
        .and_then(serde_json::Value::as_bool)
        .ok_or("'on' is missing")?;
    let after = change_setup(info, app, s, args, |setup| {
        setup.auto_download = if on {
            AutoDownload::Nothing
        } else {
            AutoDownload::Everything
        };
    })?;
    Ok(serde_json::json!({ "auto_download": after.auto_download }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: serde_json::Value) -> Args {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn a_pairings_options_come_from_its_mode_and_policies() {
        let backup = PairOptions::of(&args(serde_json::json!({"mode": "backup"}))).unwrap();
        assert!(backup.backup);
        let camera = PairOptions::of(&args(serde_json::json!({"mode": "camera-upload"}))).unwrap();
        assert!(camera.backup);
        let keep = PairOptions::of(&args(serde_json::json!({"remote_deletes": "keep"}))).unwrap();
        assert!(keep.backup);
        let selective = PairOptions::of(&args(serde_json::json!({
            "on_demand": true, "exclude": ["big/*"],
        })))
        .unwrap();
        assert!(selective.on_demand && !selective.backup);
        assert_eq!(selective.exclude, vec![String::from("big/*")]);
        assert!(PairOptions::of(&args(serde_json::json!({"mode": "sideways"}))).is_err());
        assert!(PairOptions::of(&args(serde_json::json!({"links": "in-root"}))).is_err());
        assert!(has_options(&args(serde_json::json!({"mode": "backup"}))));
        assert!(!has_options(&args(serde_json::json!({"folder": "/x"}))));

        let mut setup = SyncSetup::new("e2e-a", "", std::path::Path::new("/tmp/a"));
        selective.apply(&mut setup);
        assert_eq!(setup.auto_download, AutoDownload::Nothing);
        assert_eq!(setup.exclude, vec![String::from("big/*")]);
        backup.apply(&mut setup);
        assert!(setup.backup);
    }
}
