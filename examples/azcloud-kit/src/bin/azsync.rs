//! `azsync`: the sync engine's command line, the client the sync test harness (azul-apps
//! `local/sync-chaos`, adapter `engine`) drives. One folder here against a drive in a local
//! folder in the harness's layout ([`azcloud_kit::sync::folder_remote`]), through the same
//! engine AzDrive runs: the journal, the loop detector, the operations log, the problem list,
//! bounded retries, quarantine marks.
//!
//! ```text
//! azsync pair   --root DIR --state DIR --remote DIR --device NAME [--links skip|in-root]
//!               [--reserve BYTES] [--max-file BYTES] [--here SYSTEM] [--settle-ms MS]
//!               [--own-devices a,b]
//! azsync sync   --state DIR [--confirm-deletes]
//! azsync watch  --state DIR [--seconds N]
//! azsync problems --state DIR
//! azsync unpair --state DIR
//! ```
//!
//! A pass writes, in the state folder: `report.json` (what it did), `problems.json` (the
//! list), `problems-harness.json` (the list as the harness reads it), `ops.jsonl` (the
//! operations log). Its exit: 0 done (problems may be listed), 1 failed, 2 refused (nothing
//! changed), 3 held for the user (a mass delete: `--confirm-deletes`).
//!
//! The harness's faults reach it through the environment: `AZSYNC_CRASH_AT=<point>[:n]` kills
//! the process at the n-th time a pass reaches the point, `AZSYNC_PAUSE_AT=<point>` stops it
//! there once (SIGSTOP, a sleep), `AZSYNC_CLOCK_OFFSET=<seconds>` moves its clock.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use azcloud_kit::{
    state::{read_json, write_json},
    sync::{
        engine::{Engine, PassSummary},
        folder_remote::{self, FolderRemote},
        problems::{harness_json, ProblemList},
        quarantine::OwnDevices,
        retry::PassBackoff,
        LocalRoot, RunHooks, SyncOptions, SyncReport,
    },
};
use serde::{Deserialize, Serialize};

/// The pairing, in the state folder's `config.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Config {
    root: PathBuf,
    remote: PathBuf,
    device: String,
    /// What a link in the folder is: `skip` (listed, never followed) or `in-root` (relative
    /// links inside the folder sync as links, once the engine takes link objects).
    links: String,
    /// Bytes a download must leave free on the disk.
    reserve: u64,
    max_file: u64,
    /// The system whose name rules apply here (`macos`, `linux`, `windows`).
    here: String,
    settle_ms: u64,
    /// The user's own devices: files from any other get the quarantine mark.
    own_devices: Vec<String>,
}

/// The harness's own ignore file, read besides the engine's.
const HARNESS_IGNORE: &str = ".azsyncignore";
/// The longest a pass the user asked for waits for the drive's Retry-After before it gives up
/// for now (the next one waits for the rest).
const MAX_SERVER_WAIT: Duration = Duration::from_secs(45);

fn usage() -> ! {
    eprintln!(
        "usage: azsync pair --root DIR --state DIR --remote DIR --device NAME [options]\n       \
         azsync sync --state DIR [--confirm-deletes]\n       azsync watch --state DIR [--seconds N]\n       \
         azsync problems --state DIR\n       azsync unpair --state DIR"
    );
    std::process::exit(64)
}

/// `--name value` pairs and `--flag`s.
fn flags(args: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let Some(name) = args[i].strip_prefix("--") else {
            usage()
        };
        match args.get(i + 1).filter(|v| !v.starts_with("--")) {
            Some(value) => {
                out.insert(name.to_string(), value.clone());
                i += 2;
            }
            None => {
                out.insert(name.to_string(), String::new());
                i += 1;
            }
        }
    }
    out
}

fn need<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> &'a str {
    match flags.get(name).filter(|v| !v.is_empty()) {
        Some(v) => v,
        None => {
            eprintln!("azsync: --{name} is needed");
            usage()
        }
    }
}

fn number(flags: &BTreeMap<String, String>, name: &str, default: u64) -> u64 {
    flags
        .get(name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("azsync: {message}");
    std::process::exit(1)
}

fn private_dir(dir: &Path) {
    if let Err(e) = fs::create_dir_all(dir) {
        fail(format!("{}: {e}", dir.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
}

fn config(state: &Path) -> Config {
    match read_json::<Config>(&state.join("config.json")) {
        Ok(Some(config)) => config,
        Ok(None) => {
            eprintln!("azsync: {} is not paired", state.display());
            std::process::exit(2)
        }
        Err(e) => fail(e),
    }
}

fn clock_offset() -> f64 {
    std::env::var("AZSYNC_CLOCK_OFFSET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// The harness's stops and kills at the engine's named points.
fn points() -> Box<dyn Fn(&str) + Sync> {
    let crash = std::env::var("AZSYNC_CRASH_AT").unwrap_or_default();
    let pause = std::env::var("AZSYNC_PAUSE_AT").unwrap_or_default();
    let (crash_point, nth) = match crash.split_once(':') {
        Some((point, n)) => (point.to_string(), n.parse::<u32>().unwrap_or(1)),
        None => (crash.clone(), 1),
    };
    let hits: Mutex<BTreeMap<String, u32>> = Mutex::new(BTreeMap::new());
    let paused = AtomicBool::new(false);
    Box::new(move |name: &str| {
        if !crash_point.is_empty() && name == crash_point {
            let mut hits = hits.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let n = hits.entry(name.to_string()).or_insert(0);
            *n += 1;
            if *n >= nth {
                kill_self();
            }
        }
        if !pause.is_empty()
            && name == pause
            && !paused.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            stop_self();
        }
    })
}

#[cfg(unix)]
fn kill_self() {
    // SAFETY: signals to this process itself.
    unsafe {
        libc::kill(libc::getpid(), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_self() {
    std::process::abort();
}

#[cfg(unix)]
fn stop_self() {
    // SAFETY: signals to this process itself; the harness sends SIGCONT.
    unsafe {
        libc::kill(libc::getpid(), libc::SIGSTOP);
    }
}

#[cfg(not(unix))]
fn stop_self() {}

fn pair(flags: &BTreeMap<String, String>) {
    let state = PathBuf::from(need(flags, "state"));
    let root = PathBuf::from(need(flags, "root"));
    let remote = PathBuf::from(need(flags, "remote"));
    let device = need(flags, "device").to_string();
    private_dir(&state);
    if let Err(e) = fs::create_dir_all(&root) {
        fail(format!("{}: {e}", root.display()));
    }
    let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut own_devices = vec![device.clone()];
    own_devices.extend(
        flags
            .get("own-devices")
            .map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    let config = Config {
        root: abs(&root),
        remote: abs(&remote),
        device,
        links: flags.get("links").cloned().unwrap_or_else(|| String::from("skip")),
        reserve: number(flags, "reserve", 0),
        max_file: number(flags, "max-file", azcloud_kit::sync::MAX_FILE_BYTES),
        here: flags.get("here").cloned().unwrap_or_else(|| std::env::consts::OS.to_string()),
        settle_ms: number(flags, "settle-ms", 0),
        own_devices,
    };
    if let Err(e) = write_json(&state.join("config.json"), &config, true) {
        fail(e);
    }
}

/// The report a pass leaves for the harness.
#[derive(Serialize)]
struct HarnessReport<'a> {
    uploaded: usize,
    downloaded: usize,
    trashed: usize,
    deleted_remote: usize,
    conflicts: usize,
    moved: usize,
    held: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(flatten)]
    pass: &'a PassSummary,
}

fn root_of(config: &Config) -> LocalRoot {
    let mut root = LocalRoot::folder(&config.root);
    if let Ok(text) = fs::read_to_string(config.root.join(HARNESS_IGNORE)) {
        root.rules.add_ignore_file(&text);
    }
    root
}

fn options(config: &Config, confirm: bool) -> SyncOptions {
    let mut opts = SyncOptions::new("", "harness", &config.device).unwrap_or_else(|e| fail(e));
    opts.max_file_bytes = config.max_file;
    opts.allow_mass_delete = confirm;
    opts.settle = Duration::from_millis(config.settle_ms);
    // A file written within the settle time waits for a later pass; downloads keep the disk's
    // reserve the harness gives the device (the engine's default is 1 GiB).
    opts.file_settle = Duration::from_millis(config.settle_ms);
    opts.disk_reserve = config.reserve;
    opts
}

fn engine(state: &Path, config: &Config) -> Engine {
    Engine::new(state, &config.root, Box::new(OwnDevices(config.own_devices.clone())))
        .with_clock(clock_offset())
        .with_points(points())
}

/// One pass; its exit.
fn one_pass(state: &Path, config: &Config, engine: &Engine, remote: &FolderRemote, confirm: bool) -> (Option<SyncReport>, PassSummary) {
    let root = root_of(config);
    let opts = options(config, confirm);
    let index = state.join("index.db");
    let hooks = RunHooks {
        hold_mass_delete: !confirm,
        ..RunHooks::default()
    };
    let (result, summary) =
        engine.pass(hooks, |hooks| folder_remote::sync(remote, &root, &index, &opts, hooks));
    // A Retry-After the pass met is kept for the next one, which must not ask early either.
    if let Some(left) = remote.gate.remaining() {
        let mut backoff = PassBackoff::load(state);
        backoff.hold_until(engine.clock.now() + left.as_secs_f64());
        let _ = backoff.save(state);
    }
    (result.ok(), summary)
}

fn write_outputs(state: &Path, report: Option<&SyncReport>, summary: &PassSummary) {
    let out = HarnessReport {
        uploaded: report.map_or(0, |r| r.files_up),
        downloaded: report.map_or(0, |r| r.files_down),
        trashed: report.map_or(0, |r| r.deleted_here),
        deleted_remote: report.map_or(0, |r| r.deleted_there),
        conflicts: report.map_or(0, |r| r.conflicts.len()),
        moved: 0,
        held: report.is_some_and(|r| r.mass_delete.is_some()),
        summary: report.map(SyncReport::summary),
        pass: summary,
    };
    let _ = write_json(&state.join("report.json"), &out, true);
    let _ = write_json(&state.join("problems-harness.json"), &harness_json(&summary.problems), true);
    if let Ok(text) = serde_json::to_string(&out) {
        println!("{text}");
    }
}

fn sync(flags: &BTreeMap<String, String>) -> i32 {
    let state = PathBuf::from(need(flags, "state"));
    let config = config(&state);
    let engine = engine(&state, &config);
    let now = engine.clock.now();
    let wait = PassBackoff::load(&state).server_wait(now);
    if wait > MAX_SERVER_WAIT {
        eprintln!(
            "azsync: the drive asked to wait {} s more; this pass sends nothing",
            wait.as_secs()
        );
        return 0;
    }
    std::thread::sleep(wait);
    let offset = clock_offset();
    let clock = move || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64())
            + offset
    };
    let remote = FolderRemote::new(&config.remote, &config.device, Box::new(clock)).unwrap_or_else(|e| fail(e));
    let confirm = flags.contains_key("confirm-deletes");
    let (report, summary) = one_pass(&state, &config, &engine, &remote, confirm);
    write_outputs(&state, report.as_ref(), &summary);
    if let Some(error) = &summary.error {
        eprintln!("azsync: {error}");
    }
    summary.exit
}

fn watch(flags: &BTreeMap<String, String>) -> i32 {
    let state = PathBuf::from(need(flags, "state"));
    let config = config(&state);
    let engine = engine(&state, &config);
    let offset = clock_offset();
    let clock = move || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64())
            + offset
    };
    let remote = FolderRemote::new(&config.remote, &config.device, Box::new(clock)).unwrap_or_else(|e| fail(e));
    let stop = Arc::new(AtomicBool::new(false));
    if let Some(seconds) = flags.get("seconds").and_then(|s| s.parse::<u64>().ok()) {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds));
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
        });
    }
    engine.watch(&stop, Duration::from_millis(config.settle_ms.max(500)), |why| {
        let (report, summary) = one_pass(&state, &config, &engine, &remote, false);
        write_outputs(&state, report.as_ref(), &summary);
        eprintln!("azsync: pass ({why}): exit {}", summary.exit);
        summary
    });
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else { usage() };
    let flags = flags(&args[1..]);
    let code = match command.as_str() {
        "pair" => {
            pair(&flags);
            0
        }
        "sync" => sync(&flags),
        "watch" => watch(&flags),
        "problems" => {
            let state = PathBuf::from(need(&flags, "state"));
            let list = ProblemList::load(&state).unwrap_or_default();
            println!("{}", harness_json(&list));
            0
        }
        "unpair" => {
            // Unpairing forgets this device's state only; the folder and the drive stay.
            let state = PathBuf::from(need(&flags, "state"));
            let _ = fs::remove_dir_all(&state);
            0
        }
        _ => usage(),
    };
    std::process::exit(code);
}
