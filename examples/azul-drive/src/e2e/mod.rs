//! AzDrive's E2E commands (`examples/azul-drive/e2e/COMMANDS.md` is their contract): the ops a
//! scenario uses to drive AzDrive's real folder sync - the pairing, the passes, the problem list
//! and the conflict answers of the window - against remotes that are folders in a sandbox, with
//! faults between each device and its remote.
//!
//! The commands are added to the app's config only when a scenario or the debug server can
//! reach them (`AZ_E2E` or `AZ_DEBUG` set). Every path they create lies in the run's sandbox;
//! no command touches the user's drives, keyring or settings.
//!
//! What the commands keep between steps (the sandbox, the remotes, the devices and their pass
//! counts) is the [`Harness`], one per process like the window it drives. The sync's own hooks
//! report into it: [`pass_ended`] when a pass ends, [`change_ended`] when an answer landed.

mod c_keys;
mod c_links;
// Group A: cloud-only files, a pairing's modes, the window's actions and the computer around.
mod a_cmds;
mod a_lock;
mod a_ops;
mod a_pair;
mod a_place;
mod a_state;
mod checks;
mod files;
mod remote;
mod tamper;
// Group B of the catalogue's commands: hostile servers, networks and disks, tampering.
mod b_faults;
mod b_files;
mod b_tamper;
mod b_checks;

// The stand-in remote under azcloud-kit's real folder sync.
#[cfg(test)]
mod remote_tests;
// The hostile server's and network's faults on the stand-in remote's requests.
#[cfg(test)]
mod b_faults_tests;

// The scenario files: every step one the runner and the commands can carry.
#[cfg(test)]
mod c_catalogue_tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use azcloud_kit::sync::{
    faults,
    local::LocalIndex,
    remote::{index_key, RemoteIndex},
    session::{SyncSetup, SyncStates, INDEX_FILE},
};
use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString};
use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    Drive, ListRequest, LocalDrive,
};

use crate::{sync_jobs, sync_view, DriveState, Slot};
use remote::{FaultDrive, Remote};

/// A pass that crashed at a fault point ends with this error text and the point's name.
pub(crate) const CRASHED: &str = "crashed at ";

/// The id of the sandbox's own drive (a local drive whose root is the sandbox, so pairing a
/// folder in it is allowed).
const SANDBOX_ID: &str = "e2e";

/// One op: its name, a line for `list_app_ops`, its arguments (name, JSON type, required).
struct OpDoc {
    name: &'static str,
    summary: &'static str,
    args: &'static [(&'static str, &'static str, bool)],
}

const DEVICE: (&str, &str, bool) = ("device", "string", false);
const PATH: (&str, &str, bool) = ("path", "string", true);

/// Every command (COMMANDS.md has the long form).
const OPS: &[OpDoc] = &[
    OpDoc { name: "drive_harness", summary: "make the run's sandbox", args: &[("work", "string", false), ("keep", "bool", false)] },
    OpDoc { name: "drive_cleanup", summary: "remove the sandbox (unless kept)", args: &[] },
    OpDoc { name: "drive_create", summary: "a new remote", args: &[("remote", "string", true), ("encrypted", "bool", false), ("prefix", "string", false)] },
    OpDoc { name: "drive_open", summary: "a device on a remote", args: &[("device", "string", true), ("remote", "string", true), ("device_name", "string", false), ("settle_ms", "number", false), ("clock", "number", false), ("joined", "bool", false), ("key_from", "string", false), ("recovery_code", "string", false)] },
    OpDoc { name: "drive_pair", summary: "pair the device's folder (AzDrive's Sync with a folder)", args: &[("device", "string", true), ("folder", "string", false), ("prefix", "string", false)] },
    OpDoc { name: "drive_unpair", summary: "stop syncing the device", args: &[("device", "string", true)] },
    OpDoc { name: "drive_restart", summary: "drop the device's in-memory sync state, read it again", args: &[("device", "string", true)] },
    OpDoc { name: "drive_sync", summary: "a pass, answered when it ended", args: &[DEVICE, ("expect", "string", false), ("crash_at", "string", false), ("pause_at", "string", false), ("timeout_ms", "number", false)] },
    OpDoc { name: "drive_sync_bg", summary: "start a pass, answer at once", args: &[DEVICE, ("pause_at", "string", false)] },
    OpDoc { name: "drive_cont", summary: "let a paused pass go on", args: &[DEVICE] },
    OpDoc { name: "drive_wait", summary: "wait for the background pass to end", args: &[DEVICE, ("expect", "string", false), ("timeout_ms", "number", false)] },
    OpDoc { name: "drive_wait_idle", summary: "wait until no pass runs or waits", args: &[DEVICE, ("timeout_ms", "number", false)] },
    OpDoc { name: "drive_status", summary: "the device's sync status", args: &[DEVICE] },
    OpDoc { name: "drive_answer", summary: "answer a held mass delete or burst", args: &[DEVICE, ("question", "string", true), ("delete_too", "bool", false)] },
    OpDoc { name: "drive_resolve", summary: "answer a held conflict", args: &[DEVICE, ("key", "string", true), ("choice", "string", true)] },
    OpDoc { name: "drive_problems", summary: "the device's problem list", args: &[DEVICE] },
    OpDoc { name: "drive_states", summary: "the device's file states", args: &[DEVICE] },
    OpDoc { name: "drive_base", summary: "the device's base (the last synced state)", args: &[DEVICE] },
    OpDoc { name: "drive_index", summary: "the drive's index", args: &[DEVICE, ("remote", "string", false)] },
    OpDoc { name: "drive_counters", summary: "requests and bytes between devices and remote", args: &[DEVICE, ("remote", "string", false)] },
    OpDoc { name: "drive_list", summary: "the device's folder as it is", args: &[DEVICE] },
    OpDoc { name: "drive_read", summary: "one path on the device", args: &[DEVICE, PATH] },
    OpDoc { name: "drive_write", summary: "write a file", args: &[DEVICE, ("path", "string", false), ("path_hex", "string", false), ("text", "string", false), ("hex", "string", false), ("size", "number", false), ("seed", "number", false), ("fill", "string", false), ("mtime", "number", false), ("exec", "bool", false)] },
    OpDoc { name: "drive_overwrite_in_place", summary: "overwrite a file in place", args: &[DEVICE, PATH, ("text", "string", false)] },
    OpDoc { name: "drive_append", summary: "append to a file", args: &[DEVICE, PATH, ("text", "string", false)] },
    OpDoc { name: "drive_mkdir", summary: "make a folder", args: &[DEVICE, PATH] },
    OpDoc { name: "drive_rm", summary: "remove a file (or every file matching a glob)", args: &[DEVICE, ("path", "string", false), ("glob", "string", false)] },
    OpDoc { name: "drive_rmtree", summary: "remove a folder and all in it", args: &[DEVICE, PATH] },
    OpDoc { name: "drive_rename", summary: "rename or move", args: &[DEVICE, ("from", "string", true), ("to", "string", true)] },
    OpDoc { name: "drive_symlink", summary: "make a symbolic link", args: &[DEVICE, PATH, ("target", "string", true)] },
    OpDoc { name: "drive_hardlink", summary: "make a hard link", args: &[DEVICE, PATH, ("target", "string", true)] },
    OpDoc { name: "drive_mkfifo", summary: "make a named pipe", args: &[DEVICE, PATH] },
    OpDoc { name: "drive_chmod", summary: "change a file's mode", args: &[DEVICE, PATH, ("mode", "string", true)] },
    OpDoc { name: "drive_utime", summary: "set a file's modification time", args: &[DEVICE, PATH, ("mtime", "number", false), ("like", "string", false)] },
    OpDoc { name: "drive_many", summary: "many small files", args: &[DEVICE, ("dir", "string", true), ("count", "number", true), ("size", "number", false), ("seed", "number", false), ("text_prefix", "string", false)] },
    OpDoc { name: "drive_office_save", summary: "save as Word does", args: &[DEVICE, PATH, ("text", "string", true), ("keep_lock", "bool", false), ("keep_temp", "bool", false)] },
    OpDoc { name: "drive_editor_save", summary: "save as an editor does (a sibling renamed over)", args: &[DEVICE, PATH, ("text", "string", true), ("style", "string", false), ("keep_swap", "bool", false)] },
    OpDoc { name: "drive_move_root", summary: "move the synced folder", args: &[DEVICE, ("suffix", "string", false)] },
    OpDoc { name: "drive_remove_root", summary: "remove the synced folder", args: &[DEVICE] },
    OpDoc { name: "drive_replace_root", summary: "an empty folder in the synced folder's place", args: &[DEVICE] },
    OpDoc { name: "drive_wipe_state", summary: "lose the pairing's state files", args: &[DEVICE] },
    OpDoc { name: "drive_corrupt_state", summary: "cut the pairing's base off mid-write", args: &[DEVICE] },
    OpDoc { name: "drive_fault", summary: "faults between a device and its remote", args: &[DEVICE, ("remote", "string", false), ("clear", "bool", false), ("partition", "any", false), ("error_rate", "number", false), ("seed", "number", false), ("rate_limit_every", "number", false), ("retry_after", "number", false), ("cut_after_ops", "number", false), ("quota_bytes", "number", false), ("fail_keys", "array", false), ("refuse_ops", "array", false), ("auth_failed", "bool", false), ("latency_ms", "number", false), ("read_only", "any", false), ("moved", "any", false), ("hang", "bool", false), ("hang_ms", "number", false), ("tls_untrusted", "bool", false), ("short_body_after_bytes", "number", false), ("report_ok", "bool", false), ("fail_files", "array", false), ("answer_from", "string", false), ("max_files", "number", false), ("max_object_bytes", "number", false)] },
    OpDoc { name: "drive_crash_at", summary: "crash the device's next pass at a point", args: &[DEVICE, ("point", "string", true)] },
    OpDoc { name: "drive_disk", summary: "the device's disk as the sync sees it", args: &[DEVICE, ("free_bytes", "number", false), ("full", "bool", false), ("clear", "bool", false)] },
    OpDoc { name: "drive_clock", summary: "the device's clock as the sync reads it", args: &[DEVICE, ("offset", "number", false), ("jump", "number", false)] },
    OpDoc { name: "drive_tamper", summary: "change a remote past every device", args: &[("remote", "string", false), ("action", "string", true)] },
    OpDoc { name: "drive_snapshot", summary: "keep the remote's index under a name", args: &[("remote", "string", false), ("name", "string", true)] },
    OpDoc { name: "drive_check", summary: "a sync-chaos check", args: &[("check", "string", true)] },
    // Group C: keys, recovery, share links.
    OpDoc { name: "drive_recover", summary: "unlock the drive with a recovery code", args: &[DEVICE, ("recovery_code", "string", false), ("expect", "string", false)] },
    OpDoc { name: "drive_join", summary: "join with another device's approval", args: &[DEVICE, ("approver", "string", true), ("pause_at", "string", false)] },
    OpDoc { name: "drive_rotate_key", summary: "rotate an encrypted remote's drive key", args: &[("remote", "string", false)] },
    OpDoc { name: "drive_share_link", summary: "share a file or folder as a link", args: &[DEVICE, PATH, ("name", "string", false), ("expires_s", "number", false)] },
    OpDoc { name: "drive_open_link", summary: "visit a share link without an account", args: &[("link", "string", true)] },
    OpDoc { name: "drive_share_expire", summary: "let every link's expiry pass", args: &[("remote", "string", false)] },
    OpDoc { name: "drive_send_again", summary: "send the files kept after the drive deleted them elsewhere", args: &[DEVICE] },
    OpDoc { name: "drive_local_fault", summary: "a folder the scan cannot list for some passes", args: &[DEVICE, ("path", "string", false), ("readdir_error", "string", true), ("passes", "number", false)] },
    OpDoc { name: "drive_xattr", summary: "set an extended attribute", args: &[DEVICE, PATH, ("name", "string", true), ("text", "string", false), ("hex", "string", false), ("size", "number", false), ("seed", "number", false)] },
    OpDoc { name: "drive_chflags", summary: "set BSD flags as chflags does (macOS)", args: &[DEVICE, PATH, ("flags", "string", true)] },
    OpDoc { name: "drive_mark_objects", summary: "record a remote's stored objects under a name", args: &[("remote", "string", false), ("name", "string", true)] },
];

/// Adds the commands to `config` when a scenario or the debug server can reach them.
pub(crate) fn register(config: &mut AppConfig) {
    if std::env::var_os("AZ_E2E").is_none() && std::env::var_os("AZ_DEBUG").is_none() {
        return;
    }
    for op in OPS {
        config.add_e2e_command(AzString::from(op.name.to_string()), op_def(op), on_command);
    }
    for op in a_cmds::OPS {
        config.add_e2e_command(AzString::from(op.name.to_string()), op_def(op), on_command);
    }
}

/// `op` as `list_app_ops` shows it (azul's `E2eOpDef`).
fn op_def(op: &OpDoc) -> Json {
    let args: Vec<serde_json::Value> = op
        .args
        .iter()
        .map(|(name, ty, required)| {
            serde_json::json!({ "name": name, "type": ty, "required": required, "description": "" })
        })
        .collect();
    let def = serde_json::json!({
        "name": op.name,
        "summary": op.summary,
        "description": "See examples/azul-drive/e2e/COMMANDS.md.",
        "args": args,
        "examples": [],
    });
    Json::parse(AzString::from(def.to_string()))
        .into_result()
        .unwrap_or_else(|_| Json::null())
}

/// Every command's entry: the app's state, the window's callback info, the op and its
/// arguments (a JSON object); the answer is a JSON object with `success`.
extern "C" fn on_command(
    mut data: RefAny,
    mut info: CallbackInfo,
    name: AzString,
    args: AzString,
) -> CustomE2eOpResult {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return answer(fail("the app's data is not AzDrive's"));
    };
    let s = &mut *guard;
    let args = match serde_json::from_str::<serde_json::Value>(args.as_str()) {
        Ok(serde_json::Value::Object(map)) => map,
        Ok(serde_json::Value::Null) => serde_json::Map::new(),
        Ok(_) => return answer(fail("the arguments are not a JSON object")),
        Err(e) => return answer(fail(&format!("the arguments are not JSON: {e}"))),
    };
    let result = run(&mut info, &app, s, name.as_str(), &args);
    answer(match result {
        Ok(serde_json::Value::Object(mut map)) => {
            map.entry("success").or_insert(serde_json::Value::Bool(true));
            serde_json::Value::Object(map)
        }
        Ok(other) => serde_json::json!({ "success": true, "value": other }),
        Err(why) => fail(&why),
    })
}

fn answer(json: serde_json::Value) -> CustomE2eOpResult {
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(json.to_string()),
    }
}

fn fail(why: &str) -> serde_json::Value {
    serde_json::json!({ "success": false, "error": why })
}

/// The answer of a command that waits: called again with `_attempt` + 1.
fn pending() -> serde_json::Value {
    serde_json::json!({ "pending": true })
}

type Args = serde_json::Map<String, serde_json::Value>;
pub(crate) type Answer = Result<serde_json::Value, String>;

fn str_arg<'a>(args: &'a Args, key: &str) -> Option<&'a str> {
    args.get(key).and_then(serde_json::Value::as_str)
}

fn need<'a>(args: &'a Args, key: &str) -> Result<&'a str, String> {
    str_arg(args, key).ok_or_else(|| format!("'{key}' is missing"))
}

fn bool_arg(args: &Args, key: &str) -> bool {
    args.get(key).and_then(serde_json::Value::as_bool).unwrap_or(false)
}

fn attempt(args: &Args) -> u64 {
    args.get("_attempt").and_then(serde_json::Value::as_u64).unwrap_or(0)
}

// ==== The harness ====

/// One device: a drive of AzDrive's on a remote, paired with a folder of its own.
pub(crate) struct Device {
    /// The scenario's name for it.
    pub name: String,
    /// Its drive's id in AzDrive (`e2e-<name>`).
    pub drive_id: String,
    pub remote: String,
    /// The device name its conflict copies say.
    pub device_name: String,
    /// Where the device's files are: the synced folder, until a `drive_move_root` moves it.
    pub root: PathBuf,
    /// The folder the pairing names: what the sync's faults are armed under (it stays when
    /// the folder is moved, as AzDrive's pairing does).
    pub pair_folder: PathBuf,
    /// The pairing's state folder, once paired.
    pub state_dir: Option<PathBuf>,
    /// How long its passes wait for a quiet folder.
    pub settle: Duration,
    /// Its sync engine's clock is this many seconds off the system's.
    pub clock: f64,
    /// Passes ended, and crashed, so far.
    pub passes: u64,
    pub crashes: u64,
    /// What the last pass ended with (the `drive_sync` answer).
    pub last: Option<serde_json::Value>,
    /// The pass count a waiting `drive_sync` / `drive_wait` waits for.
    pub waiting_for: Option<u64>,
    /// Answers (a conflict's, a question's) landed so far, and the count one waits for.
    pub changes: u64,
    pub change_wanted: Option<u64>,
}

/// What the commands keep between steps.
pub(crate) struct Harness {
    /// The sandbox, and its parts.
    pub run: PathBuf,
    pub outside: PathBuf,
    /// The text of `outside/secret.txt`: nothing may carry it onto a remote.
    pub secret: String,
    /// The sandbox stays after `drive_cleanup`.
    pub keep: bool,
    /// The outside folder at the start: `outside_untouched` compares with it.
    pub outside_before: BTreeMap<String, String>,
    pub remotes: BTreeMap<String, Arc<Remote>>,
    /// In the order they were opened: the first is every command's default device.
    pub devices: Vec<Device>,
    /// Every text a step wrote (sync-chaos's live texts).
    pub texts: BTreeSet<String>,
    /// Every problem a device's passes listed, in order.
    pub seen_problems: BTreeMap<String, Vec<serde_json::Value>>,
}

static HARNESS: Mutex<Option<Harness>> = Mutex::new(None);

fn harness() -> MutexGuard<'static, Option<Harness>> {
    HARNESS.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Harness {
    pub(crate) fn device(&self, args: &Args) -> Result<&Device, String> {
        match str_arg(args, "device") {
            Some(name) => self
                .devices
                .iter()
                .find(|d| d.name == name)
                .ok_or_else(|| format!("no device '{name}' (drive_open it first)")),
            None => self
                .devices
                .first()
                .ok_or_else(|| String::from("no device yet (drive_open one first)")),
        }
    }

    fn device_mut(&mut self, args: &Args) -> Result<&mut Device, String> {
        let name = self.device(args)?.name.clone();
        self.devices
            .iter_mut()
            .find(|d| d.name == name)
            .ok_or_else(|| format!("no device '{name}'"))
    }

    fn by_drive_mut(&mut self, drive_id: &str) -> Option<&mut Device> {
        self.devices.iter_mut().find(|d| d.drive_id == drive_id)
    }

    pub(crate) fn remote_of(&self, args: &Args) -> Result<Arc<Remote>, String> {
        let name = match str_arg(args, "remote") {
            Some(name) => name.to_string(),
            None => match str_arg(args, "device") {
                Some(_) => self.device(args)?.remote.clone(),
                None => self
                    .remotes
                    .keys()
                    .next()
                    .cloned()
                    .ok_or_else(|| String::from("no remote yet (drive_create one first)"))?,
            },
        };
        self.remotes
            .get(&name)
            .cloned()
            .ok_or_else(|| format!("no remote '{name}'"))
    }

    /// `{run}`, `{outside}`, `{secret}`, `{root:<d>}`, `{state:<d>}`, `{remote:<r>}` in `text`.
    pub(crate) fn subst(&self, text: &str) -> String {
        let mut out = text
            .replace("{run}", &self.run.to_string_lossy())
            .replace("{outside}", &self.outside.to_string_lossy())
            .replace("{secret}", &self.secret);
        for d in &self.devices {
            out = out.replace(&format!("{{root:{}}}", d.name), &d.root.to_string_lossy());
            if let Some(state) = &d.state_dir {
                out = out.replace(&format!("{{state:{}}}", d.name), &state.to_string_lossy());
            }
        }
        for (name, r) in &self.remotes {
            out = out.replace(&format!("{{remote:{name}}}"), &r.dir.to_string_lossy());
        }
        c_keys::subst(&out)
    }

    /// The arguments with every string substituted ([`Self::subst`]).
    fn subst_args(&self, args: &Args) -> Args {
        fn walk(h: &Harness, v: &serde_json::Value) -> serde_json::Value {
            match v {
                serde_json::Value::String(s) => serde_json::Value::String(h.subst(s)),
                serde_json::Value::Array(a) => {
                    serde_json::Value::Array(a.iter().map(|x| walk(h, x)).collect())
                }
                serde_json::Value::Object(o) => serde_json::Value::Object(
                    o.iter().map(|(k, x)| (k.clone(), walk(h, x))).collect(),
                ),
                other => other.clone(),
            }
        }
        args.iter().map(|(k, v)| (k.clone(), walk(self, v))).collect()
    }

    /// `args[key]` as a path: absolute as it is, else under the device's folder.
    pub(crate) fn path(&self, args: &Args, key: &str) -> Result<PathBuf, String> {
        let p = need(args, key)?;
        if Path::new(p).is_absolute() {
            return Ok(PathBuf::from(p));
        }
        Ok(self.device(args)?.root.join(p))
    }
}

/// The sandbox, made with the defaults when no step made it yet.
fn ensure_harness(s: &mut DriveState, args: &Args) -> Result<(), String> {
    if harness().is_some() {
        return Ok(());
    }
    let parent = str_arg(args, "work")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("AZDRIVE_E2E_WORK").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let run = parent.join(format!("azdrive-e2e-{}-{stamp}", std::process::id()));
    let outside = run.join("outside");
    let secret = format!("AZDRIVE-E2E-SECRET-{stamp:032x}");
    files::make_outside(&outside, &secret).map_err(|e| format!("the sandbox: {e}"))?;
    for dir in ["remote", "devices", "cache"] {
        std::fs::create_dir_all(run.join(dir)).map_err(|e| format!("the sandbox: {e}"))?;
    }
    let keep = bool_arg(args, "keep")
        || std::env::var("AZDRIVE_E2E_KEEP").is_ok_and(|v| v.trim() == "1");
    // AzDrive's cache folder for this run: the pairings' state folders lie in the sandbox.
    s.cache_dir = Some(run.join("cache"));
    // The sandbox as a drive on this computer: a folder paired must lie in a drive the window
    // shows.
    if s.slot_index(SANDBOX_ID).is_none() {
        let mut slot = Slot::new(DriveEntry {
            id: SANDBOX_ID.to_string(),
            name: String::from("E2E sandbox"),
            location: DriveLocation::Local {
                root: run.to_string_lossy().into_owned(),
            },
        });
        slot.drive = Some(Arc::new(LocalDrive::new(run.clone())));
        s.slots.push(slot);
        sync_jobs::publish(s);
    }
    let outside_before = files::snapshot(&outside);
    *harness() = Some(Harness {
        run,
        outside,
        secret,
        keep,
        outside_before,
        remotes: BTreeMap::new(),
        devices: Vec::new(),
        texts: BTreeSet::new(),
        seen_problems: BTreeMap::new(),
    });
    Ok(())
}

/// Runs the command `name`.
fn run(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, name: &str, raw: &Args) -> Answer {
    ensure_harness(s, raw)?;
    let args = harness()
        .as_ref()
        .map(|h| h.subst_args(raw))
        .unwrap_or_default();
    let args = &args;
    match name {
        _ if a_cmds::handles(name, args) => a_cmds::run(info, app, s, name, args),
        "drive_harness" => with_harness(|h| {
            Ok(serde_json::json!({
                "run": h.run, "outside": h.outside, "secret": h.secret,
            }))
        }),
        "drive_cleanup" => cleanup(s),
        "drive_create" => create(args),
        "drive_open" => open(s, args),
        "drive_pair" => pair(info, app, s, args),
        "drive_unpair" => {
            let drive_id = with_harness(|h| Ok(h.device(args)?.drive_id.clone()))?;
            sync_jobs::stop(info, app, s, &drive_id);
            Ok(serde_json::json!({}))
        }
        "drive_restart" => restart(s, args),
        "drive_sync" => sync(info, app, s, args, true),
        "drive_sync_bg" => sync(info, app, s, args, false),
        // Group C: a join waiting at its approval goes on with drive_cont.
        "drive_cont" if c_keys::join_waits(args) => c_keys::cont_join(s, args),
        "drive_cont" => with_harness(|h| {
            faults::release(&h.device(args)?.pair_folder);
            Ok(serde_json::json!({}))
        }),
        "drive_wait" => wait_pass(args),
        "drive_wait_idle" => wait_idle(s, args),
        "drive_status" => status(s, args),
        "drive_answer" => answer_question(info, app, s, args),
        "drive_resolve" => resolve(info, app, s, args),
        "drive_problems" => {
            let state_dir = with_harness(|h| Ok(h.device(args)?.state_dir.clone()))?;
            Ok(serde_json::json!({ "problems": problems_in(state_dir.as_deref()) }))
        }
        "drive_states" => states(s, args),
        "drive_base" => base(args),
        "drive_index" => index(s, args),
        "drive_counters" => counters(args),
        "drive_fault" => fault(args),
        "drive_crash_at" => with_harness(|h| {
            faults::arm_crash(&h.device(args)?.pair_folder, need(args, "point")?);
            Ok(serde_json::json!({}))
        }),
        "drive_disk" => with_harness(|h| {
            let root = h.device(args)?.pair_folder.clone();
            if bool_arg(args, "clear") {
                faults::set_disk_full(&root, false);
                faults::set_free_bytes(&root, None);
            }
            if let Some(full) = args.get("full").and_then(serde_json::Value::as_bool) {
                faults::set_disk_full(&root, full);
            }
            if let Some(free) = args.get("free_bytes").and_then(serde_json::Value::as_u64) {
                faults::set_free_bytes(&root, Some(free));
            }
            Ok(serde_json::json!({}))
        }),
        "drive_clock" => with_harness(|h| {
            let d = h.device_mut(args)?;
            if let Some(offset) = args.get("offset").and_then(serde_json::Value::as_f64) {
                d.clock = offset;
            }
            if let Some(jump) = args.get("jump").and_then(serde_json::Value::as_f64) {
                d.clock += jump;
            }
            Ok(serde_json::json!({ "offset": d.clock }))
        }),
        // Group C: the share server's links (a takedown, a visit's check).
        "drive_tamper" if str_arg(args, "action") == Some("takedown_link") => c_links::takedown(args),
        "drive_check" if c_links::is_link_check(args) => c_links::check(s, args),
        "drive_tamper" if b_tamper::owns(args) => b_tamper::tamper(s, args),
        "drive_tamper" => tamper::tamper(s, args),
        "drive_snapshot" => tamper::snapshot(args),
        "drive_check" if b_checks::owns(args) => b_checks::check(s, args),
        "drive_check" => checks::check(s, args),
        // Group C: keys, recovery, share links.
        "drive_recover" => c_keys::recover(s, args),
        "drive_join" => c_keys::join(s, args),
        "drive_rotate_key" => c_keys::rotate_key(s, args),
        "drive_share_link" => c_links::share_link(args),
        "drive_open_link" => c_links::open_link(args),
        "drive_share_expire" => c_links::expire(args),
        // AzDrive's "Send them again" for the files kept after the drive deleted them elsewhere.
        "drive_send_again" => waiting_change(args, |drive_id| {
            if sync_jobs::send_kept(info, app, s, drive_id) {
                Ok(())
            } else {
                Err(String::from(
                    "the device keeps no file the drive deleted elsewhere (deleted-elsewhere-kept)",
                ))
            }
        }),
        "drive_local_fault" => b_files::local_fault(args),
        "drive_xattr" => b_files::xattr(args),
        "drive_chflags" => b_files::chflags(args),
        "drive_mark_objects" => b_tamper::mark_objects(args),
        _ if name.starts_with("drive_") => files::file_op(name, args),
        _ => Err(format!("'{name}' is not one of AzDrive's commands")),
    }
}

/// `f` with the harness (made by [`ensure_harness`] before every command).
pub(crate) fn with_harness<T>(f: impl FnOnce(&mut Harness) -> Result<T, String>) -> Result<T, String> {
    let mut guard = harness();
    let h = guard
        .as_mut()
        .ok_or_else(|| String::from("the sandbox is gone (drive_cleanup ran)"))?;
    f(h)
}

fn cleanup(s: &mut DriveState) -> Answer {
    let Some(h) = harness().take() else {
        return Ok(serde_json::json!({}));
    };
    c_keys::forget();
    c_links::forget();
    for d in &h.devices {
        faults::clear(&d.pair_folder);
    }
    for r in h.remotes.values() {
        b_faults::clear_remote(r);
    }
    // Every device stops syncing, so nothing writes into the folder while it goes.
    for d in &h.devices {
        if let Some(sync) = s.sync_view.drives.get(&d.drive_id) {
            if let Some(running) = &sync.running {
                running.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
    // Group A's locks let go and its volumes detached before the sandbox goes.
    a_cmds::cleanup();
    if !h.keep {
        b_files::clear_flags(&h.run);
        files::clean(&h.run);
    }
    Ok(serde_json::json!({ "removed": !h.keep, "run": h.run }))
}

fn create(args: &Args) -> Answer {
    let name = need(args, "remote")?.to_string();
    let encrypted = args
        .get("encrypted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let prefix = azcloud_kit::sync::remote::normalize_prefix(str_arg(args, "prefix").unwrap_or(""))
        .map_err(|e| e.to_string())?;
    let remote = with_harness(|h| {
        if h.remotes.contains_key(&name) {
            return Err(format!("remote '{name}' exists"));
        }
        let dir = h.run.join("remote").join(&name);
        let remote = Arc::new(
            Remote::create(&name, dir, encrypted, &prefix).map_err(|e| e.to_string())?,
        );
        h.remotes.insert(name.clone(), remote.clone());
        Ok(remote)
    })?;
    if encrypted {
        set_up_keys(&remote)?;
    }
    Ok(serde_json::json!({
        "remote": name, "path": remote.dir, "encrypted": encrypted, "prefix": prefix,
    }))
}

/// The keys of a new encrypted remote, as AzDrive makes them for a new Azlin drive: the drive
/// key in the remote's keyring, the recovery wrap in the bucket.
#[cfg(feature = "encryption")]
fn set_up_keys(remote: &Arc<Remote>) -> Result<(), String> {
    use azul_storage::crypto::{device, keys::RecoveryKdf};
    let bucket = FaultDrive {
        remote: remote.clone(),
        device: String::from("(setup)"),
    };
    let kdf = RecoveryKdf::fresh().map_err(|e| e.to_string())?;
    device::setup_new_drive(&bucket, remote.keyring.as_ref(), &remote.drive_name, kdf)
        // The recovery sheet's code: `{recovery:<remote>}`, drive_recover's default.
        .map(|(_, code)| c_keys::remember_code(&remote.name, code.to_text().to_string()))
        .map_err(|e| format!("the encrypted remote's keys: {e}"))
}

#[cfg(not(feature = "encryption"))]
fn set_up_keys(_remote: &Arc<Remote>) -> Result<(), String> {
    Err(String::from(
        "this AzDrive was built without encryption: drive_create with \"encrypted\": false",
    ))
}

fn open(s: &mut DriveState, args: &Args) -> Answer {
    let name = need(args, "device")?.to_string();
    let remote = with_harness(|h| {
        if h.devices.iter().any(|d| d.name == name) {
            return Err(format!("device '{name}' exists"));
        }
        h.remote_of(args)
    })?;
    let drive_id = format!("e2e-{name}");
    b_files::reset_hashed(&drive_id);
    let device_name = str_arg(args, "device_name").unwrap_or(&name).to_string();
    let run = with_harness(|h| Ok(h.run.clone()))?;
    let root = run.join("devices").join(&name).join("root");
    let bucket: Arc<dyn Drive> = Arc::new(FaultDrive {
        remote: remote.clone(),
        device: name.clone(),
    });
    // The remote's keyring (a joined device), or one of its own (`joined`, `key_from`).
    #[cfg_attr(not(feature = "encryption"), allow(unused_variables))]
    let keyring = c_keys::keyring_for_open(&remote, &name, args, bucket.as_ref())?;
    // A cloud drive in the source list; its drive is set here, so nothing ever opens the
    // address (`.invalid` names no host).
    let mut slot = Slot::new(DriveEntry {
        id: drive_id.clone(),
        name: format!("E2E {name}"),
        location: DriveLocation::S3 {
            endpoint: format!("https://{}.e2e.invalid", remote.name),
            region: String::from("e2e"),
            bucket: remote.name.clone(),
            path_style: true,
            auth: DriveAuth::default(),
        },
    });
    if remote.encrypted {
        #[cfg(feature = "encryption")]
        {
            // Each device has its own copy of the drive index, as two computers do.
            let index_cache = run.join("devices").join(&name).join("index-cache");
            let provider: Arc<dyn azul_storage::encrypted::IndexProvider> =
                Arc::new(crate::encryption::meta_provider(Some(index_cache)));
            let auto = Arc::new(azul_storage::encrypted::AutoEncrypted::new(
                bucket,
                &remote.drive_name,
                keyring,
                Some(provider),
            ));
            slot.drive = Some(auto.clone() as Arc<dyn Drive>);
            slot.auto = Some(auto);
        }
    } else {
        slot.drive = Some(bucket);
    }
    if let Some(i) = s.slot_index(&drive_id) {
        s.slots.remove(i);
    }
    s.slots.push(slot);
    // An encrypted device's calls by file key, where `fail_files` finds them.
    b_faults::wrap_files(s.slots.last_mut(), &remote, &name);
    let settle = Duration::from_millis(
        args.get("settle_ms")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    );
    let clock = args
        .get("clock")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    with_harness(|h| {
        h.devices.push(Device {
            name: name.clone(),
            drive_id: drive_id.clone(),
            remote: remote.name.clone(),
            device_name,
            pair_folder: root.clone(),
            root,
            state_dir: None,
            settle,
            clock,
            passes: 0,
            crashes: 0,
            last: None,
            waiting_for: None,
            changes: 0,
            change_wanted: None,
        });
        Ok(())
    })?;
    Ok(serde_json::json!({ "device": name, "drive_id": drive_id }))
}

fn pair(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, folder, prefix) = with_harness(|h| {
        let d = h.device(args)?;
        let folder = str_arg(args, "folder").map_or_else(|| d.root.clone(), PathBuf::from);
        let prefix = match str_arg(args, "prefix") {
            Some(p) => p.to_string(),
            None => h.remote_of(&Args::from_iter([(
                String::from("remote"),
                serde_json::Value::String(d.remote.clone()),
            )]))?
            .prefix
            .clone(),
        };
        Ok((d.drive_id.clone(), folder, prefix))
    })?;
    sync_jobs::pair(info, app, s, &drive_id, &folder.to_string_lossy(), &prefix)
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
    Ok(serde_json::json!({ "folder": folder, "state_dir": state_dir }))
}

fn restart(s: &mut DriveState, args: &Args) -> Answer {
    let (drive_id, state_dir) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), d.state_dir.clone()))
    })?;
    if s
        .sync_view
        .drives
        .get(&drive_id)
        .is_some_and(|d| d.running.is_some())
    {
        return Err(String::from("a pass runs: wait for it first (drive_wait_idle)"));
    }
    if let Some(dir) = state_dir {
        s.sync_view.store.set_states(&drive_id, SyncStates::load(&dir));
    }
    // A restarted app has hashed nothing yet.
    b_files::reset_hashed(&drive_id);
    if let Some(sync) = s.sync_view.drives.get_mut(&drive_id) {
        sync.again = false;
    }
    sync_view::say_status(s, &drive_id);
    Ok(serde_json::json!({}))
}

/// Whether a pass of `drive_id` runs, and whether another waits for it.
fn running(s: &DriveState, drive_id: &str) -> (bool, bool) {
    s.sync_view
        .drives
        .get(drive_id)
        .map_or((false, false), |d| (d.running.is_some(), d.again))
}

fn sync(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args, wait: bool) -> Answer {
    let (drive_id, root, passes, waiting) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), d.pair_folder.clone(), d.passes, d.waiting_for))
    })?;
    let pause_at = str_arg(args, "pause_at").map(str::to_string);
    if attempt(args) == 0 || waiting.is_none() {
        if let Some(point) = str_arg(args, "crash_at") {
            faults::arm_crash(&root, point);
        }
        if let Some(point) = &pause_at {
            faults::arm_pause(&root, point);
        }
        // The asked pass is the next to start: after the one running, if one runs.
        let (busy, _) = running(s, &drive_id);
        let target = passes + if busy { 2 } else { 1 };
        sync_jobs::request_pass(info, app, s, &drive_id);
        let (started, queued) = running(s, &drive_id);
        if !started && !queued {
            faults::release(&root);
            return Err(String::from(
                "AzDrive started no pass (not paired, paused, banned, or the drive cannot be \
                 opened: drive_status says why)",
            ));
        }
        with_harness(|h| {
            h.device_mut(args)?.waiting_for = Some(target);
            Ok(())
        })?;
        if !wait && pause_at.is_none() {
            return Ok(serde_json::json!({ "started": true }));
        }
        return Ok(pending());
    }
    let target = waiting.unwrap_or(passes);
    if let Some(point) = &pause_at {
        if faults::paused_at(&root).as_deref() == Some(point.as_str()) {
            return Ok(serde_json::json!({ "outcome": "paused", "point": point }));
        }
        if passes >= target {
            return Err(format!("the pass ended before it reached {point}"));
        }
        return Ok(pending());
    }
    if passes < target {
        return Ok(pending());
    }
    finished(args)
}

/// The waited-for pass ended: its answer, `success` as `expect` says.
fn finished(args: &Args) -> Answer {
    with_harness(|h| {
        let d = h.device_mut(args)?;
        d.waiting_for = None;
        let mut last = d.last.clone().unwrap_or_else(|| serde_json::json!({}));
        let outcome = last
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("done")
            .to_string();
        let expect = str_arg(args, "expect").unwrap_or("ok");
        let wanted = match expect {
            "ok" => outcome == "done",
            "fail" => outcome == "failed",
            "crash" => outcome == "crashed",
            "any" => true,
            other => return Err(format!("expect '{other}' is none of ok, fail, crash, any")),
        };
        if let serde_json::Value::Object(map) = &mut last {
            map.insert("success".into(), wanted.into());
            if !wanted {
                let why = map
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                map.insert(
                    "error".into(),
                    format!("the pass of {} ended {outcome}, expected {expect}: {why}", d.name).into(),
                );
            }
        }
        Ok(last)
    })
}

fn wait_pass(args: &Args) -> Answer {
    let (passes, waiting) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.passes, d.waiting_for))
    })?;
    match waiting {
        None => Err(String::from("no pass to wait for (drive_sync_bg starts one)")),
        Some(target) if passes < target => Ok(pending()),
        Some(_) => finished(args),
    }
}

fn wait_idle(s: &DriveState, args: &Args) -> Answer {
    let ids: Vec<(String, u64)> = with_harness(|h| {
        Ok(if str_arg(args, "device").is_some() {
            let d = h.device(args)?;
            vec![(d.drive_id.clone(), d.passes)]
        } else {
            h.devices.iter().map(|d| (d.drive_id.clone(), d.passes)).collect()
        })
    })?;
    let busy = ids.iter().any(|(id, _)| {
        let (run, again) = running(s, id);
        run || again
    });
    if busy {
        return Ok(pending());
    }
    Ok(serde_json::json!({ "passes": ids.iter().map(|(_, n)| n).sum::<u64>() }))
}

fn status(s: &DriveState, args: &Args) -> Answer {
    with_harness(|h| {
        let d = h.device(args)?;
        let (busy, again) = running(s, &d.drive_id);
        let said = s
            .sync_view
            .drives
            .get(&d.drive_id)
            .map(|sync| sync.said.clone())
            .unwrap_or_default();
        Ok(serde_json::json!({
            "idle": !busy && !again,
            "running": busy,
            "queued": again,
            "passes": d.passes,
            "crashes": d.crashes,
            "status": said,
            "paired": sync_view::setup_of(s, &d.drive_id).is_some(),
            "last": d.last,
        }))
    })
}

fn answer_question(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let question = need(args, "question")?.to_string();
    let delete_too = bool_arg(args, "delete_too");
    waiting_change(args, |drive_id| match question.as_str() {
        "mass_delete" => {
            sync_jobs::answer_mass_delete(info, app, s, drive_id, delete_too);
            Ok(())
        }
        "burst" => {
            sync_jobs::answer_burst(info, app, s, drive_id);
            Ok(())
        }
        other => Err(format!("no question '{other}' (mass_delete, burst)")),
    })
}

fn resolve(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    use azcloud_kit::sync::session::Resolution;
    let key = need(args, "key")?.to_string();
    let choice = match need(args, "choice")? {
        "mine" => Resolution::KeepMine,
        "theirs" => Resolution::TakeTheirs,
        "both" => Resolution::KeepBoth,
        other => return Err(format!("no choice '{other}' (mine, theirs, both)")),
    };
    waiting_change(args, |drive_id| {
        sync_jobs::resolve(info, app, s, drive_id, &key, choice);
        Ok(())
    })
}

/// Starts an answer's job on the first call, then waits until it landed ([`change_ended`]).
fn waiting_change(args: &Args, start: impl FnOnce(&str) -> Result<(), String>) -> Answer {
    let (drive_id, changes, wanted) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.drive_id.clone(), d.changes, d.change_wanted))
    })?;
    match wanted {
        None => {
            start(&drive_id)?;
            with_harness(|h| {
                h.device_mut(args)?.change_wanted = Some(changes + 1);
                Ok(())
            })?;
            Ok(pending())
        }
        Some(target) if changes < target => Ok(pending()),
        Some(_) => with_harness(|h| {
            h.device_mut(args)?.change_wanted = None;
            Ok(serde_json::json!({}))
        }),
    }
}

fn states(s: &DriveState, args: &Args) -> Answer {
    let drive_id = with_harness(|h| Ok(h.device(args)?.drive_id.clone()))?;
    let states = s.sync_view.store.states(&drive_id);
    let files: serde_json::Map<String, serde_json::Value> = states
        .files
        .iter()
        .map(|(key, record)| {
            let state = states.state_of(key).map(|st| sync_view::state_word(&st));
            (
                key.clone(),
                serde_json::json!({
                    "state": state,
                    "size": record.size,
                    "modified": record.modified,
                    "cloud_only": record.cloud_only,
                    "pinned": states.is_pinned(key),
                    "error": record.error,
                    "conflict": record.conflict.is_some(),
                }),
            )
        })
        .collect();
    Ok(serde_json::json!({
        "files": files,
        "read_only": states.read_only,
        "last_error": states.last_error,
        "last_problem": states.last_problem.as_ref().map(|p| p.code.clone()),
        "skipped": states.skipped,
    }))
}

fn base(args: &Args) -> Answer {
    let state_dir = with_harness(|h| {
        h.device(args)?
            .state_dir
            .clone()
            .ok_or_else(|| String::from("the device is not paired"))
    })?;
    let index = LocalIndex::load(&state_dir.join(INDEX_FILE)).map_err(|e| e.to_string())?;
    let Some(index) = index else {
        return Ok(serde_json::json!({ "entries": {}, "generation": 0 }));
    };
    let entries: serde_json::Map<String, serde_json::Value> = index
        .files
        .iter()
        .map(|(k, e)| {
            (
                k.clone(),
                serde_json::json!({
                    "hash": e.hash, "size": e.size, "mtime": e.mtime_ns, "cloud_only": e.cloud_only,
                }),
            )
        })
        .collect();
    Ok(serde_json::json!({ "entries": entries, "generation": index.generation }))
}

/// The drive's index: a plain remote's sync index, an encrypted one's files by name.
fn index(s: &mut DriveState, args: &Args) -> Answer {
    let (remote, device_drive) = with_harness(|h| {
        let remote = h.remote_of(args)?;
        let device = if str_arg(args, "device").is_some() {
            Some(h.device(args)?.drive_id.clone())
        } else {
            None
        };
        Ok((remote, device))
    })?;
    if remote.encrypted {
        let drive = drive_reading(s, &remote, device_drive.as_deref())?;
        let files = named_files(drive.as_ref(), &remote.prefix)?;
        return Ok(serde_json::json!({ "files": files }));
    }
    let key = index_key(&remote.prefix);
    let bytes = match device_drive {
        Some(id) => {
            let drive = crate::open_drive(s, &id).ok_or("the device's drive cannot be opened")?;
            match drive.get(&key) {
                Ok(b) => Some(b),
                Err(azul_storage::DriveError::NotFound { .. }) => None,
                Err(e) => return Err(e.to_string()),
            }
        }
        None => remote.read(&key),
    };
    let Some(bytes) = bytes else {
        return Ok(serde_json::json!({ "generation": 0, "features": [], "files": {}, "deleted": {} }));
    };
    let index = RemoteIndex::parse(&bytes).map_err(|e| e.to_string())?;
    serde_json::to_value(&index).map_err(|e| e.to_string())
}

/// The drive a device of `remote` reads it through: the device named, else the remote's first.
pub(crate) fn drive_reading(
    s: &mut DriveState,
    remote: &Remote,
    device_drive: Option<&str>,
) -> Result<Arc<dyn Drive>, String> {
    let id = match device_drive {
        Some(id) => id.to_string(),
        None => with_harness(|h| {
            h.devices
                .iter()
                .find(|d| d.remote == remote.name)
                .map(|d| d.drive_id.clone())
                .ok_or_else(|| format!("remote '{}' has no device to read it", remote.name))
        })?,
    };
    crate::open_drive(s, &id).ok_or_else(|| format!("the drive of {id} cannot be opened"))
}

/// Every file under `prefix` of an encrypted drive, by its key under the prefix: `{size}`.
pub(crate) fn named_files(
    drive: &dyn Drive,
    prefix: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let mut out = serde_json::Map::new();
    let mut request = ListRequest::recursive(prefix);
    loop {
        let page = drive.list(&request).map_err(|e| e.to_string())?;
        for o in &page.objects {
            let key = o.key.strip_prefix(prefix).unwrap_or(&o.key);
            if key.is_empty() || key.ends_with('/') || key.starts_with(".azlin/") {
                continue;
            }
            out.insert(key.to_string(), serde_json::json!({ "size": o.size }));
        }
        match page.next {
            Some(next) => request.continuation = Some(next),
            None => break,
        }
    }
    Ok(out)
}

fn counters(args: &Args) -> Answer {
    with_harness(|h| {
        let remote = h.remote_of(args)?;
        let device = if str_arg(args, "device").is_some() {
            Some(h.device(args)?)
        } else {
            None
        };
        let mut out = remote.counters(device.map(|d| d.name.as_str())).to_json();
        if let (serde_json::Value::Object(map), Some(d)) = (&mut out, device) {
            map.insert("passes".into(), d.passes.into());
            map.insert("crashes".into(), d.crashes.into());
            map.insert("hashed".into(), b_files::hashed_of(&d.drive_id).into());
        }
        Ok(out)
    })
}

fn fault(args: &Args) -> Answer {
    with_harness(|h| {
        let remote = h.remote_of(args)?;
        let clear = bool_arg(args, "clear");
        let mut fields = args.clone();
        for k in ["device", "remote", "clear", "_attempt", "timeout_ms"] {
            fields.remove(k);
        }
        // The hostile server's and network's faults are kept apart (b_faults).
        b_faults::set(h, &remote, args)?;
        for k in b_faults::FIELDS {
            fields.remove(*k);
        }
        if !clear && fields.is_empty() {
            return Ok(serde_json::json!({}));
        }
        // `partition` may name the devices it cuts off.
        if let Some(names) = args.get("partition").and_then(serde_json::Value::as_array) {
            for name in names.iter().filter_map(serde_json::Value::as_str) {
                remote.set_faults(name, clear, &Args::from_iter([(
                    String::from("partition"),
                    serde_json::Value::Bool(true),
                )]));
            }
            fields.remove("partition");
            if fields.is_empty() {
                return Ok(serde_json::json!({}));
            }
        }
        let target = match str_arg(args, "device") {
            Some(_) => h.device(args)?.name.clone(),
            None => String::new(),
        };
        if clear && fields.is_empty() && target.is_empty() {
            remote.clear_faults();
        } else {
            remote.set_faults(&target, clear, &fields);
        }
        Ok(serde_json::json!({}))
    })
}

// ==== The sync's hooks ====

/// The restore an E2E device's passes take as announced by its stand-in server (`None`: none,
/// or not an E2E device).
pub(crate) fn restore_of(drive_id: &str) -> Option<azcloud_kit::sync::RestoreNotice> {
    b_tamper::restore_of(drive_id)
}

/// The device name a device's passes write in conflict copies (`None`: not an E2E device).
pub(crate) fn device_name_of(drive_id: &str) -> Option<String> {
    harness().as_ref().and_then(|h| {
        h.devices
            .iter()
            .find(|d| d.drive_id == drive_id)
            .map(|d| d.device_name.clone())
    })
}

/// The device names of every device on an E2E device's remote: one user's computers, whose
/// downloads need no quarantine mark (`None`: not an E2E device).
pub(crate) fn own_devices_of(drive_id: &str) -> Option<Vec<String>> {
    harness().as_ref().and_then(|h| {
        let remote = &h.devices.iter().find(|d| d.drive_id == drive_id)?.remote;
        Some(
            h.devices
                .iter()
                .filter(|d| &d.remote == remote)
                .map(|d| d.device_name.clone())
                .collect(),
        )
    })
}

/// How far a device's sync clock is off (`None`: not an E2E device).
pub(crate) fn clock_of(drive_id: &str) -> Option<f64> {
    harness().as_ref().and_then(|h| {
        h.devices
            .iter()
            .find(|d| d.drive_id == drive_id)
            .map(|d| d.clock)
    })
}

/// How long a device's passes wait for a quiet folder (`None`: not an E2E device).
pub(crate) fn settle_of(drive_id: &str) -> Option<Duration> {
    harness().as_ref().and_then(|h| {
        h.devices
            .iter()
            .find(|d| d.drive_id == drive_id)
            .map(|d| d.settle)
    })
}

/// A pass of `drive_id` ended (on the UI thread, after its states were taken): the device's
/// count, its last answer and the problems it listed.
pub(crate) fn pass_ended(drive_id: &str, result: &Result<sync_jobs::PassDone, String>) {
    if let Ok(done) = result {
        b_files::note_hashed(drive_id, done.hashed as u64);
    }
    let mut guard = harness();
    let Some(h) = guard.as_mut() else {
        return;
    };
    let Some(d) = h.by_drive_mut(drive_id) else {
        return;
    };
    let problems = problems_in(d.state_dir.as_deref());
    d.passes += 1;
    let last = match result {
        Ok(done) => serde_json::json!({
            "outcome": "done",
            "up": done.up,
            "down": done.down,
            "deleted": done.deleted,
            "conflicts": done.conflicts,
            "cloud_only": done.cloud_only,
            "freed": done.freed,
            "held": done.held,
            "paused": done.paused.is_some(),
            "newer_format": done.newer_format,
            "problems": problems,
        }),
        Err(e) => {
            let crashed = e.strip_prefix(CRASHED);
            if crashed.is_some() {
                d.crashes += 1;
            }
            serde_json::json!({
                "outcome": if crashed.is_some() { "crashed" } else { "failed" },
                "point": crashed,
                "error": e,
                "problems": problems,
            })
        }
    };
    d.last = Some(last);
    let name = d.name.clone();
    h.seen_problems.entry(name).or_default().extend(problems);
}

/// An answer's job for `drive_id` landed (a conflict resolved, a question answered).
pub(crate) fn change_ended(drive_id: &str) {
    if let Some(d) = harness().as_mut().and_then(|h| h.by_drive_mut(drive_id)) {
        d.changes += 1;
    }
}

/// The pairing's problem list as the engine published it (`problems.json` in its state
/// folder): one entry per file a pass skipped, refused, held, froze or failed, and per
/// question or refusal that holds the drive, with the engine's codes (`symlink-skipped`,
/// `conflict-held`, ...).
pub(crate) fn problems_in(state_dir: Option<&Path>) -> Vec<serde_json::Value> {
    let Some(dir) = state_dir else {
        return Vec::new();
    };
    azcloud_kit::sync::problems::ProblemList::load(dir)
        .map(|list| {
            list.problems
                .iter()
                .filter_map(|p| serde_json::to_value(p).ok())
                .collect()
        })
        .unwrap_or_default()
}
