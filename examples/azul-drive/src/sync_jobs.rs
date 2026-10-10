//! AzDrive's folder sync on worker threads (azcloud-kit's `sync::session`) and what starts it.
//!
//! - One [`SyncJob`] per azul `Thread`: a PASS of a synced drive (its progress streamed back
//!   at most ten times a second while it runs), a cloud-only file OPENED (downloaded - and
//!   decrypted - first), files PINNED or FREED ("Free up space"), a conflict ANSWERED. The
//!   answers come back as [`SyncOutcome`]s ([`on_outcome`], on the UI thread).
//! - At the window's start every synced drive shows the states it kept (they outlive the
//!   app) and gets a pass; then a timer polls every synced drive that is not paused every
//!   `$AZDRIVE_SYNC_POLL` seconds (30 by default): a pass that finds nothing new is one
//!   conditional read of the drive (a HEAD of the plain index; an encrypted drive's own
//!   index polls its manifest) and a scan of the folder.
//! - A drive's calls go through the drive the window holds: an Azlin drive refreshes its
//!   credentials under the drive's keyring lock (azcloud-kit's `AzlinDrive`, as
//!   `SharedKeyring::with_drive_token` does), so two AzDrive windows never spend one drive
//!   token twice. No call here takes the drive token itself.
//! - Plain drives sync through the plain index and blobs (`DriveStore`); an encrypted drive
//!   by name through its own index, or - "local copies: encrypted" - into the AZL1 object cache
//!   below its encryption (feature `encryption`).

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration as StdDuration, Instant},
};

use azcloud_kit::sync::{
    drive_store::DriveStore,
    session::{device_name, Freed, Pass, Resolution, SyncSession, SyncSetup, SyncStates},
    SyncEvent,
};
use azul::{
    callbacks::{TimerCallbackInfo, TimerCallbackReturn},
    prelude::*,
    sensor::NetworkState,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use azul_appkit::l10n::{Phrase, Text};
use azul_storage::{key, Drive};

use crate::{
    browse::{self, Place},
    go,
    jobs::{Job, Outcome},
    l10n::cloud_error_text,
    spawn,
    sync_view::{self, DriveSync, Running, SyncAction, SyncDialog},
    DriveState, Popup,
};

// ==== The worker's side ====

/// What a pass has done so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PassProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The file moving now (its key under the pairing), and whether it goes up.
    pub moving: Option<(String, bool)>,
}

impl PassProgress {
    /// Takes in an event of the pass; whether it should be told at once (the plan).
    fn hear(&mut self, event: &SyncEvent) -> bool {
        match event {
            SyncEvent::Planned {
                up_files,
                up_bytes,
                down_files,
                down_bytes,
            } => {
                self.files_total += up_files + down_files;
                self.bytes_total += up_bytes + down_bytes;
                true
            }
            SyncEvent::Started { key, up, .. } => {
                self.moving = Some((key.clone(), *up));
                false
            }
            SyncEvent::Finished { bytes, .. } => {
                self.files_done += 1;
                self.bytes_done += bytes;
                self.moving = None;
                false
            }
        }
    }
}

/// A synced drive as a job takes it: its pairing, its state folder and the drive.
pub(crate) struct SyncWork {
    pub setup: SyncSetup,
    pub dir: PathBuf,
    pub drive: Arc<dyn Drive>,
    /// An Azlin drive's encryption seam: whether it is encrypted.
    #[cfg(feature = "encryption")]
    pub auto: Option<Arc<azul_storage::AutoEncrypted>>,
    /// Its AZL1 objects kept on this computer (encrypted local copies).
    #[cfg(feature = "encryption")]
    pub objects: Option<Arc<azcloud_kit::sync::objects::ObjectCache>>,
    /// Files over this many bytes wait for a pass on a free network, either way (a metered
    /// network: [`sync_view::network_hold`]).
    pub transfer_limit: Option<u64>,
}

impl SyncWork {
    pub(crate) fn new(setup: SyncSetup, dir: PathBuf, drive: Arc<dyn Drive>) -> SyncWork {
        SyncWork {
            setup,
            dir,
            drive,
            #[cfg(feature = "encryption")]
            auto: None,
            #[cfg(feature = "encryption")]
            objects: None,
            transfer_limit: None,
        }
    }

    fn drive_id(&self) -> String {
        self.setup.drive_id.clone()
    }

    /// The session (on the worker: an Azlin drive decides on its first call whether it is
    /// encrypted), with the work's transfer limit.
    fn session(&self) -> SyncSession {
        self.drive_session().with_transfer_limit(self.transfer_limit)
    }

    /// The session of the drive's kind.
    fn drive_session(&self) -> SyncSession {
        let device = device_name();
        #[cfg(feature = "encryption")]
        {
            if let Some(auto) = &self.auto {
                if auto.is_encrypted().is_none() {
                    let probe = azul_storage::ListRequest::folder("").with_max_keys(1);
                    let _ = self.drive.list(&probe);
                }
                if auto.is_encrypted() == Some(true) {
                    let encrypted = self.setup.local_copies
                        == azcloud_kit::sync::session::LocalCopies::Encrypted;
                    if let (true, Some(objects)) = (encrypted, &self.objects) {
                        return SyncSession::encrypted_copies(
                            self.setup.clone(),
                            self.dir.clone(),
                            &device,
                            self.drive.clone(),
                            objects.clone(),
                        );
                    }
                    return SyncSession::named(
                        self.setup.clone(),
                        self.dir.clone(),
                        &device,
                        self.drive.clone(),
                    );
                }
            }
        }
        SyncSession::plain(
            self.setup.clone(),
            self.dir.clone(),
            &device,
            Arc::new(DriveStore::new(self.drive.clone())),
        )
    }
}

/// A sync task on a worker thread.
pub(crate) enum SyncJob {
    /// One pass of a synced drive, until it ends or `cancel`.
    Pass {
        work: SyncWork,
        cancel: Arc<AtomicBool>,
    },
    /// A file opened (its key under the pairing): brought down first when it is in the cloud.
    Open { work: SyncWork, key: String },
    /// Files and folders always kept on this device (`on`), or no longer.
    Pin {
        work: SyncWork,
        keys: Vec<String>,
        on: bool,
    },
    /// "Free up space" for files and folders.
    FreeUp { work: SyncWork, keys: Vec<String> },
    /// A conflict answered.
    Resolve {
        work: SyncWork,
        key: String,
        choice: Resolution,
    },
    /// Files and folders deleted from the drive with the next pass (kept in the cloud only, or
    /// shown from a plain drive's sync index).
    Delete { work: SyncWork, keys: Vec<String> },
    /// A file on this device opened: used now (the size cap frees it last).
    Touch { work: SyncWork, key: String },
    /// A rename in a plain synced drive's own listing: in the synced folder (cloud-only files
    /// come down first); the next pass moves them on the drive.
    Rename {
        work: SyncWork,
        from: String,
        to: String,
    },
    /// Cloud-only files brought down (a copy, move or download of them waits for it).
    Fetch { work: SyncWork, keys: Vec<String> },
    /// The burst guard's question answered: "these changes are mine".
    AnswerBurst { work: SyncWork },
    /// The mass delete's question answered: delete them too, or keep them.
    AnswerMassDelete { work: SyncWork, delete_too: bool },
}

/// What a pass did, in counts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PassDone {
    pub up: usize,
    pub down: usize,
    pub deleted: usize,
    pub conflicts: usize,
    pub cloud_only: usize,
    pub freed: usize,
    /// The burst guard's pause (azcloud-kit's `guard`): uploads held back, downloads went on.
    pub paused: Option<azcloud_kit::sync::guard::Pause>,
    /// What the drive's index uses that this version does not know (D43).
    pub newer_format: Vec<String>,
    /// Files whose transfer waited for a free network (a metered one held them back).
    pub held: usize,
}

impl PassDone {
    fn of(pass: &Pass) -> PassDone {
        let mut done = PassDone {
            down: pass.fetched.len(),
            freed: pass.freed.len(),
            conflicts: pass.states.conflicts().len(),
            cloud_only: pass
                .states
                .files
                .values()
                .filter(|r| r.cloud_only)
                .count(),
            ..PassDone::default()
        };
        if let Some(report) = &pass.report {
            done.up = report.files_up;
            done.down += report.files_down;
            done.deleted = report.deleted_here + report.deleted_there;
            done.paused.clone_from(&report.paused);
            done.newer_format.clone_from(&report.newer_format);
        }
        done.held = pass.held.len();
        done
    }
}

/// What a pin, a "Free up space" or an answer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncChange {
    Pinned(bool),
    Freed,
    Resolved(Resolution),
    Deleted,
    Touched,
    Renamed,
    Fetched,
    /// A guard's question answered (the next pass does it).
    Answered,
}

/// A sync job's answer, on the UI thread.
pub(crate) enum SyncOutcome {
    /// How far a pass got (its thread still runs).
    Progress {
        drive_id: String,
        progress: PassProgress,
    },
    /// A pass ended: what it did, or why it failed; the states either way.
    Passed {
        drive_id: String,
        result: Result<PassDone, String>,
        states: SyncStates,
    },
    /// A file opened: where it is on this computer, or why not.
    Opened {
        drive_id: String,
        key: String,
        result: Result<PathBuf, Text>,
        states: SyncStates,
    },
    /// A pin, a "Free up space", an answer: what to say (or why not), the states after.
    Changed {
        drive_id: String,
        done: SyncChange,
        result: Result<Text, Text>,
        states: SyncStates,
    },
}

/// What "Free up space" says.
fn freed_text(freed: &Freed) -> Text {
    let text = Text::from(Phrase::new("azdrive-sync-freed").arg("count", freed.freed.len()));
    match freed.kept.first() {
        Some((key, why)) => text.then(" ").then(
            Phrase::new("azdrive-sync-freed-kept")
                .arg("name", key::last_segment(key))
                .arg("why", why.as_str()),
        ),
        None => text,
    }
}

/// Runs `job`; a pass tells its progress through `emit` while it runs.
pub(crate) fn run(job: SyncJob, emit: &mut dyn FnMut(Outcome)) -> Outcome {
    let outcome = match job {
        SyncJob::Pass { work, cancel } => {
            let session = work.session();
            let drive_id = work.drive_id();
            let mut progress = PassProgress::default();
            let mut last: Option<Instant> = None;
            let result = session.pass(&cancel, &mut |event| {
                let now = progress.hear(&event);
                let due = last.is_none_or(|at| at.elapsed() >= StdDuration::from_millis(100));
                if now || due {
                    last = Some(Instant::now());
                    emit(Outcome::Sync(SyncOutcome::Progress {
                        drive_id: drive_id.clone(),
                        progress: progress.clone(),
                    }));
                }
            });
            match result {
                Ok(pass) => SyncOutcome::Passed {
                    drive_id,
                    result: Ok(PassDone::of(&pass)),
                    states: pass.states,
                },
                Err(e) => SyncOutcome::Passed {
                    drive_id,
                    result: Err(e.to_string()),
                    states: session.states(),
                },
            }
        }
        SyncJob::Open { work, key } => {
            let session = work.session();
            let result = session.open(&key).map_err(|e| cloud_error_text(&e));
            SyncOutcome::Opened {
                drive_id: work.drive_id(),
                key,
                result,
                states: session.states(),
            }
        }
        SyncJob::Pin { work, keys, on } => {
            let session = work.session();
            let result = session
                .pin(&keys, on)
                .map(|_| {
                    let said = if on {
                        "azdrive-sync-pinned"
                    } else {
                        "azdrive-sync-unpinned"
                    };
                    Text::from(Phrase::new(said).arg("count", keys.len()))
                })
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Pinned(on),
                result,
                states: session.states(),
            }
        }
        SyncJob::FreeUp { work, keys } => {
            let session = work.session();
            let result = session
                .free_up(&keys)
                .map(|freed| freed_text(&freed))
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Freed,
                result,
                states: session.states(),
            }
        }
        SyncJob::Delete { work, keys } => {
            let session = work.session();
            let result = session
                .delete(&keys)
                .map(|deleted| {
                    Text::from(Phrase::new("azdrive-sync-deleted").arg("count", deleted.len()))
                })
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Deleted,
                result,
                states: session.states(),
            }
        }
        SyncJob::Touch { work, key } => {
            let session = work.session();
            let result = session
                .touch(&key)
                .map(|()| Text::plain(key))
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Touched,
                result,
                states: session.states(),
            }
        }
        SyncJob::Rename { work, from, to } => {
            let session = work.session();
            let result = session
                .rename(&from, &to)
                .map(|()| Text::plain(to.clone()))
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Renamed,
                result,
                states: session.states(),
            }
        }
        SyncJob::Fetch { work, keys } => {
            let session = work.session();
            let result = keys
                .iter()
                .try_for_each(|key| session.open(key).map(|_| ()))
                .map(|()| Text::from(Phrase::new("azdrive-sync-fetched").arg("count", keys.len())))
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Fetched,
                result,
                states: session.states(),
            }
        }
        SyncJob::AnswerBurst { work } => {
            let session = work.session();
            let result = session
                .answer_burst()
                .map(|()| Text::plain("burst mine"))
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Answered,
                result,
                states: session.states(),
            }
        }
        SyncJob::AnswerMassDelete { work, delete_too } => {
            let session = work.session();
            let result = session
                .answer_mass_delete(delete_too)
                .map(|()| {
                    Text::plain(if delete_too {
                        "mass-delete delete"
                    } else {
                        "mass-delete keep"
                    })
                })
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Answered,
                result,
                states: session.states(),
            }
        }
        SyncJob::Resolve { work, key, choice } => {
            let session = work.session();
            let result = session
                .resolve(&key, choice)
                .map(|()| Text::plain(key.clone()))
                .map_err(|e| cloud_error_text(&e));
            SyncOutcome::Changed {
                drive_id: work.drive_id(),
                done: SyncChange::Resolved(choice),
                result,
                states: session.states(),
            }
        }
    };
    Outcome::Sync(outcome)
}

// ==== The UI thread's side ====

/// How often the timer polls every synced drive: `$AZDRIVE_SYNC_POLL` seconds (scripts), else
/// 30.
fn poll_secs() -> u64 {
    std::env::var("AZDRIVE_SYNC_POLL")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(30)
}

/// The store takes the pairings as the settings have them now, which drives name their files
/// and the drives on this computer.
pub(crate) fn publish(s: &DriveState) {
    let roots: Vec<(String, PathBuf)> = (0..s.slots.len())
        .filter_map(|i| Some((s.slots[i].entry.id.clone(), s.local_root(i)?)))
        .collect();
    s.sync_view.store.set_pairs(
        &s.settings.synced,
        &|drive_id: &str| sync_view::names_its_files(s, drive_id),
        roots,
    );
}

/// The folder the pairings' states are kept in: `<cache>/sync`.
fn state_root(s: &DriveState) -> Option<PathBuf> {
    s.cache_dir.as_ref().map(|dir| dir.join("sync"))
}

/// At the window's start: every synced drive's states as they were kept, the poll timer, a
/// first pass of each drive that is not paused.
pub(crate) fn start(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    // The search asks the store (its SyncLookup) from now on: no Status column while nothing
    // syncs.
    s.sync = Arc::new(s.sync_view.store.clone());
    note_network(s, NetworkState::query());
    let Some(root) = state_root(s) else {
        return;
    };
    publish(s);
    for setup in s.settings.synced.clone() {
        let states = SyncStates::load(&setup.state_dir(&root));
        s.sync_view.store.set_states(&setup.drive_id, states);
        s.sync_view.drives.entry(setup.drive_id.clone()).or_default();
        sync_view::say_status(s, &setup.drive_id);
    }
    if s.settings.synced.is_empty() {
        return;
    }
    start_timer(info, app, s);
    for setup in s.settings.synced.clone() {
        if !setup.paused {
            request_pass(info, app, s, &setup.drive_id);
        }
    }
}

fn start_timer(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.sync_view.timer {
        return;
    }
    s.sync_view.timer = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_sync_timer, get_time).with_interval(Duration::System(
            SystemTimeDiff::from_millis(poll_secs() * 1000),
        )),
    );
}

extern "C" fn on_sync_timer(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut callback_info = info.callback_info;
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let changed = network_changed(&mut *s, NetworkState::query());
    let due: Vec<String> = s
        .settings
        .synced
        .iter()
        .filter(|p| !p.paused)
        .map(|p| p.drive_id.clone())
        .collect();
    for drive_id in due {
        request_pass(&mut callback_info, &app, &mut *s, &drive_id);
    }
    if changed {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// Keeps `network` as the network now; a change prints `AZDRIVE_NETWORK <kind> connected=..
/// metered=.. constrained=..`. Whether it changed.
fn note_network(s: &mut DriveState, network: NetworkState) -> bool {
    if s.sync_view.network == Some(network) {
        return false;
    }
    s.sync_view.network = Some(network);
    println!(
        "AZDRIVE_NETWORK {:?} connected={} metered={} constrained={}",
        network.kind, network.connected, network.metered, network.constrained
    );
    true
}

/// The poll's reading of the network: on a change every synced drive says its status again,
/// and a pass that runs without the limit the new network sets stops before its next file (and
/// runs again with it). Whether it changed.
fn network_changed(s: &mut DriveState, network: NetworkState) -> bool {
    let before = s.sync_view.network;
    if !note_network(s, network) {
        return false;
    }
    for setup in s.settings.synced.clone() {
        let held = sync_view::network_hold(Some(&network), &setup);
        let was = sync_view::network_hold(before.as_ref(), &setup);
        if held.is_some() && held != was {
            if let Some(sync) = s.sync_view.drives.get_mut(&setup.drive_id) {
                if let Some(running) = &sync.running {
                    running.cancel.store(true, Ordering::SeqCst);
                    sync.again = true;
                }
            }
        }
        sync_view::say_status(s, &setup.drive_id);
    }
    true
}

/// Synced drive `drive_id` as a job takes it; `None` (with a message, or the keyring asked
/// for its keys - the next poll syncs) when it cannot be opened now.
fn work_of(info: &mut CallbackInfo, s: &mut DriveState, drive_id: &str) -> Option<SyncWork> {
    let setup = sync_view::setup_of(s, drive_id)?.clone();
    let Some(root) = state_root(s) else {
        s.warn(Text::key("azdrive-sync-why-no-cache"));
        return None;
    };
    let index = s.slot_index(drive_id)?;
    if s.slots[index].locked() {
        crate::unlock(info, s, index);
        return None;
    }
    let drive = crate::open_slot(s, index)?;
    #[cfg_attr(not(feature = "encryption"), allow(unused_mut))]
    let mut work = SyncWork::new(setup.clone(), setup.state_dir(&root), drive);
    #[cfg(feature = "encryption")]
    {
        work.auto = s.slots[index].auto.clone();
        work.objects = s.slots[index].objects.clone();
    }
    Some(work)
}

/// A pass of synced drive `drive_id` now - or, while one runs, right after it.
pub(crate) fn request_pass(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    // A banned drive takes no uploads: its sync pauses, the status line says why.
    if crate::ban::ban_of(s, drive_id).is_some() {
        sync_view::say_status(s, drive_id);
        return;
    }
    if let Some(sync) = s.sync_view.drives.get_mut(drive_id) {
        if sync.running.is_some() {
            sync.again = true;
            return;
        }
    }
    let Some(mut work) = work_of(info, s, drive_id) else {
        return;
    };
    // A metered or low-data network holds the big files back ("Paused (metered network)").
    work.transfer_limit = sync_view::network_hold(s.sync_view.network.as_ref(), &work.setup);
    let cancel = Arc::new(AtomicBool::new(false));
    s.sync_view.drives.entry(drive_id.to_string()).or_default().running = Some(Running {
        cancel: cancel.clone(),
        progress: PassProgress::default(),
    });
    println!("AZDRIVE_SYNC_STARTED {drive_id}");
    sync_view::say_status(s, drive_id);
    spawn(info, app, s, Job::Sync(SyncJob::Pass { work, cancel }));
}

/// Pairs drive `drive_id`'s folder `prefix` with `folder` on this computer, which must lie in
/// a drive this window shows (Home, a folder added as a drive); the first pass starts and the
/// folder opens. `Err` says what is wrong.
pub(crate) fn pair(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    folder: &str,
    prefix: &str,
) -> Result<(), Text> {
    if folder.is_empty() {
        return Err(Text::key("azdrive-sync-pair-type-folder"));
    }
    let folder = PathBuf::from(folder);
    if !folder.is_absolute() {
        return Err(Text::key("azdrive-sync-pair-whole-path"));
    }
    let prefix = azcloud_kit::sync::remote::normalize_prefix(prefix)
        .map_err(|e| Text::plain(e.to_string()))?;
    if sync_view::setup_of(s, drive_id).is_some() {
        return Err(Text::key("azdrive-sync-pair-synced"));
    }
    if let Some(other) = s
        .settings
        .synced
        .iter()
        .find(|p| folder.starts_with(&p.folder) || p.folder.starts_with(&folder))
    {
        return Err(Phrase::new("azdrive-sync-pair-taken")
            .arg("folder", other.folder.display().to_string())
            .into());
    }
    let Some(place) = local_place(s, &folder) else {
        return Err(Text::key("azdrive-sync-pair-not-shown"));
    };
    if s.cache_dir.is_none() {
        return Err(Text::key("azdrive-sync-why-no-cache"));
    }
    std::fs::create_dir_all(&folder)
        .map_err(|e| Text::plain(format!("{}: {e}", folder.display())))?;
    s.settings
        .synced
        .push(SyncSetup::new(drive_id, &prefix, &folder));
    publish(s);
    crate::save_settings(info, app, s);
    s.sync_view.drives.entry(drive_id.to_string()).or_default();
    println!("AZDRIVE_SYNC_PAIRED {drive_id} {}", folder.display());
    start_timer(info, app, s);
    request_pass(info, app, s, drive_id);
    go(info, app, s, place, true);
    Ok(())
}

/// The place of `folder` in a drive on this computer.
fn local_place(s: &DriveState, folder: &Path) -> Option<Place> {
    let drives: Vec<(String, PathBuf)> = (0..s.slots.len())
        .filter_map(|i| Some((s.slots[i].entry.id.clone(), s.local_root(i)?)))
        .collect();
    browse::place_of_path(folder, &drives)
}

/// "Stop syncing": the pairing is forgotten (a pass running stops); the files stay on both
/// sides, the states in the cache folder.
pub(crate) fn stop(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    if let Some(sync) = s.sync_view.drives.remove(drive_id) {
        if let Some(running) = sync.running {
            running.cancel.store(true, Ordering::SeqCst);
        }
    }
    s.settings.synced.retain(|p| p.drive_id != drive_id);
    publish(s);
    crate::save_settings(info, app, s);
    println!("AZDRIVE_SYNC_STOPPED {drive_id}");
    s.info(Text::key("azdrive-sync-stopped"));
}

/// Answers the conflict of `key` (D52), on a worker thread; a pass follows.
pub(crate) fn resolve(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    key: &str,
    choice: Resolution,
) {
    if let Some(work) = work_of(info, s, drive_id) {
        spawn(
            info,
            app,
            s,
            Job::Sync(SyncJob::Resolve {
                work,
                key: key.to_string(),
                choice,
            }),
        );
    }
}

/// Opens `entry` of drive `drive` through its pairing when the sync must bring it first: a
/// cloud-only file, an encrypted copy, or - in an encrypted drive's own listing - a file whose
/// copy is in the synced folder. Whether it did (else the window opens it as usual).
pub(crate) fn open_if_synced(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive: &str,
    entry: &browse::Entry,
) -> bool {
    if entry.is_folder {
        return false;
    }
    let Some((drive_id, rel)) = sync_view::pair_at(s, drive, &entry.key) else {
        return false;
    };
    let Some(state) = s.sync_view.store.states(&drive_id).state_of(&rel) else {
        return false;
    };
    // A file changed here and on the drive asks which version to keep.
    if state == azcloud_kit::sync::session::FileState::Conflict {
        sync_view::ask_conflict(s, &drive_id, &rel);
        return true;
    }
    let local_view = s.is_local_drive(drive);
    let through_sync = match state {
        azcloud_kit::sync::session::FileState::CloudOnly
        | azcloud_kit::sync::session::FileState::OnDeviceEncrypted => true,
        // The drive's own listing: the synced copy opens (it may hold changes not up yet).
        _ => !local_view,
    };
    if !through_sync {
        // On this device already: it opens as it is, and is used now (the size cap frees the
        // least recently used first) - quietly: a drive whose keys are still in the keyring is
        // not unlocked for it.
        let locked = s
            .slot_index(&drive_id)
            .is_none_or(|index| s.slots[index].locked());
        if !locked {
            if let Some(work) = work_of(info, s, &drive_id) {
                spawn(info, app, s, Job::Sync(SyncJob::Touch { work, key: rel }));
            }
        }
        return false;
    }
    let Some(work) = work_of(info, s, &drive_id) else {
        return true;
    };
    s.info(Phrase::new("azdrive-opening").arg("name", entry.name.as_str()));
    spawn(info, app, s, Job::Sync(SyncJob::Open { work, key: rel }));
    true
}

/// What a pass's guards said, against the states `before` it: a new burst pause or mass delete
/// is printed (`AZDRIVE_SYNC_BURST <drive> burst|encryption <changes>`, `AZDRIVE_SYNC_MASS_DELETE
/// <drive> here|there <count>`) and asked once (the next question); a newer drive format is the
/// drive's problem "Update the app..." (azcloud-kit's table: the status line, one notification).
fn guards_seen(
    info: &mut CallbackInfo,
    s: &mut DriveState,
    drive_id: &str,
    before: &SyncStates,
) {
    let after = s.sync_view.store.states(drive_id);
    if let Some(pause) = after.burst.as_ref().filter(|_| before.burst.is_none()) {
        let reason = match pause.reason {
            azcloud_kit::sync::guard::PauseReason::Burst => "burst",
            azcloud_kit::sync::guard::PauseReason::Encryption => "encryption",
        };
        println!("AZDRIVE_SYNC_BURST {drive_id} {reason} {}", pause.changes);
        s.sync_view.asked.remove(&format!("{drive_id}\nburst"));
    }
    if let Some(asked) = after
        .mass_delete
        .as_ref()
        .filter(|_| before.mass_delete.is_none())
    {
        println!(
            "AZDRIVE_SYNC_MASS_DELETE {drive_id} {} {}",
            if asked.here { "here" } else { "there" },
            asked.count
        );
        s.sync_view.asked.remove(&format!("{drive_id}\nmass"));
    }
    if !after.newer_format.is_empty() && after.newer_format != before.newer_format {
        let problem = azcloud_kit::UserError {
            code: azcloud_kit::user_errors::Code::NewerFormat,
            retry_after: None,
            request_id: None,
            detail: after.newer_format.join(", "),
        };
        crate::problems::drive_problem_for(info, s, drive_id, problem);
    }
}

/// The answer to the burst guard's question: "these changes are mine" (the next pass sends
/// them), on a worker thread.
pub(crate) fn answer_burst(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    if let Some(work) = work_of(info, s, drive_id) {
        spawn(info, app, s, Job::Sync(SyncJob::AnswerBurst { work }));
    }
}

/// The answer to the mass delete's question: delete them too, or keep them.
pub(crate) fn answer_mass_delete(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    delete_too: bool,
) {
    if let Some(work) = work_of(info, s, drive_id) {
        spawn(
            info,
            app,
            s,
            Job::Sync(SyncJob::AnswerMassDelete { work, delete_too }),
        );
    }
}

/// A rename of `key` (a file, or a folder ending in `/`) to `to` in a plain synced drive's own
/// listing of `drive`: through the sync - whether it did (else the drive renames it).
pub(crate) fn rename_if_synced(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive: &str,
    key: &str,
    to: &str,
) -> bool {
    if !sync_view::from_index(s, drive, key) {
        return false;
    }
    let (Some((drive_id, from)), Some((_, to))) =
        (sync_view::pair_at(s, drive, key), sync_view::pair_at(s, drive, to))
    else {
        return false;
    };
    if let Some(work) = work_of(info, s, &drive_id) {
        spawn(info, app, s, Job::Sync(SyncJob::Rename { work, from, to }));
    }
    true
}

/// A copy, move or download (an upload too) as it is queued.
pub(crate) struct Transfer {
    pub kind: crate::fileops::TransferKind,
    pub source: (String, Arc<dyn Drive>),
    pub items: Vec<crate::fileops::SourceItem>,
    pub target: (String, Arc<dyn Drive>),
    pub target_prefix: String,
    pub target_name: String,
    pub auto: Option<crate::fileops::ConflictChoice>,
}

/// The synced folder of synced drive `drive_id`.
fn synced_folder(s: &DriveState, drive_id: &str) -> Option<PathBuf> {
    sync_view::setup_of(s, drive_id).map(|p| p.folder.clone())
}

/// A transfer whose source or target is a plain synced drive's own listing (its sync index's
/// files, which the bucket does not hold under their names) reads from - and writes to - the
/// synced folder on this computer; the next pass takes the change to the drive. Cloud-only
/// files of its source come down first: the transfer waits for them (`None`; it is queued again
/// when they are here).
pub(crate) fn route_transfer(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    mut t: Transfer,
) -> Option<Transfer> {
    let source_synced = t
        .items
        .first()
        .is_some_and(|item| sync_view::from_index(s, &t.source.0, &item.key));
    if source_synced {
        let found: Vec<(String, String)> = t
            .items
            .iter()
            .map_while(|item| sync_view::pair_at(s, &t.source.0, &item.key))
            .collect();
        let pair = found.first().map(|(d, _)| d.clone());
        let one = found.len() == t.items.len() && found.windows(2).all(|w| w[0].0 == w[1].0);
        if let (Some(drive_id), true) = (pair, one) {
            let states = s.sync_view.store.states(&drive_id);
            let cloud: Vec<String> = found
                .iter()
                .flat_map(|(_, rel)| {
                    states
                        .files
                        .iter()
                        .filter(|(key, record)| {
                            record.cloud_only
                                && (*key == rel || (rel.ends_with('/') && key.starts_with(rel.as_str())))
                        })
                        .map(|(key, _)| key.clone())
                        .collect::<Vec<_>>()
                })
                .collect();
            if !cloud.is_empty() {
                let Some(work) = work_of(info, s, &drive_id) else {
                    return None;
                };
                s.info(Phrase::new("azdrive-sync-downloading-first").arg("count", cloud.len()));
                s.sync_view.waiting_transfer = Some(t);
                spawn(info, app, s, Job::Sync(SyncJob::Fetch { work, keys: cloud }));
                return None;
            }
            let Some(folder) = synced_folder(s, &drive_id) else {
                return Some(t);
            };
            t.items = t
                .items
                .iter()
                .zip(found)
                .map(|(item, (_, rel))| crate::fileops::SourceItem {
                    key: rel,
                    ..item.clone()
                })
                .collect();
            t.source = (
                format!("sync:{drive_id}"),
                Arc::new(azul_storage::LocalDrive::without_manifest(folder)),
            );
        }
    }
    if sync_view::from_index(s, &t.target.0, &t.target_prefix) {
        if let Some((drive_id, rel)) = sync_view::pair_at(s, &t.target.0, &t.target_prefix) {
            if let Some(folder) = synced_folder(s, &drive_id) {
                t.target = (
                    format!("sync:{drive_id}"),
                    Arc::new(azul_storage::LocalDrive::without_manifest(folder)),
                );
                t.target_prefix = rel;
            }
        }
    }
    Some(t)
}

/// A transfer ended: one that went through a synced folder (`sync:<drive>`) is taken to the
/// drive by a pass now.
pub(crate) fn transfer_done(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    source_id: &str,
    target_id: &str,
) {
    for id in [source_id, target_id] {
        if let Some(drive_id) = id.strip_prefix("sync:") {
            let drive_id = drive_id.to_string();
            request_pass(info, app, s, &drive_id);
            refresh_if_showing(info, app, s, &drive_id);
        }
    }
}

/// The selected `items` of drive `drive` that the sync deletes (asked first: "Delete from the
/// drive?"): every item of a pairing when one of them is in the cloud only (its bytes are not
/// here to move to the trash), or a plain synced drive's own listing (the sync index's names).
/// The items left for the usual delete (none when the sync took them).
pub(crate) fn delete_through_sync(
    s: &mut DriveState,
    drive: &str,
    items: Vec<crate::fileops::SourceItem>,
) -> Vec<crate::fileops::SourceItem> {
    // Every item in one pairing, or the usual delete takes them all.
    let found: Vec<(String, String)> = items
        .iter()
        .map_while(|item| sync_view::pair_at(s, drive, &item.key))
        .collect();
    let one_pairing = found.len() == items.len() && found.windows(2).all(|w| w[0].0 == w[1].0);
    let Some(drive_id) = found.first().map(|(d, _)| d.clone()).filter(|_| one_pairing) else {
        return items;
    };
    let states = s.sync_view.store.states(&drive_id);
    let cloud_only = found.iter().any(|(_, rel)| {
        states.state_of(rel) == Some(azcloud_kit::sync::session::FileState::CloudOnly)
    });
    let keys: Vec<String> = found.into_iter().map(|(_, rel)| rel).collect();
    let from_index = items
        .first()
        .is_some_and(|item| sync_view::from_index(s, drive, &item.key));
    if !cloud_only && !from_index {
        return items;
    }
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup = Some(Popup::Sync(SyncDialog::Delete { drive_id, keys }));
    }
    Vec::new()
}

/// "Delete from the drive?" said yes: `keys` of the pairing of `drive_id` go, on a worker
/// thread; a pass follows.
pub(crate) fn delete(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    keys: Vec<String>,
) {
    if let Some(work) = work_of(info, s, drive_id) {
        spawn(info, app, s, Job::Sync(SyncJob::Delete { work, keys }));
    }
}

/// What a look at an Azlin drive's status found (the token server's word): the synced drives
/// of `azlin_id` are "Read-only (payment due)" while it says they take no writes.
pub(crate) fn drive_status_seen(s: &mut DriveState, azlin_id: &str, read_only: bool) {
    let ids: Vec<String> = s
        .slots
        .iter()
        .filter(|slot| slot.entry.azlin().is_some_and(|(id, _)| id == azlin_id))
        .map(|slot| slot.entry.id.clone())
        .collect();
    for id in ids {
        if read_only {
            s.sync_view.payment_due.insert(id.clone());
        } else {
            s.sync_view.payment_due.remove(&id);
        }
        if sync_view::setup_of(s, &id).is_some() {
            sync_view::say_status(s, &id);
        }
    }
}

/// How long a refused write waits before it asks the token server about its drive again.
const STATUS_ASK_SECS: u64 = 600;

/// A pass of an Azlin drive was refused a write: its token server is asked for the drive's
/// status (at most every ten minutes) - the status line says "Read-only (payment due)" by its
/// word, never by a guess.
fn ask_drive_status(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    let Some(index) = s.slot_index(drive_id) else {
        return;
    };
    let Some(azlin_id) = s.slots[index].entry.azlin().map(|(id, _)| id.to_string()) else {
        return;
    };
    let now = crate::actions::now_secs();
    let asked = s.sync_view.asked_status.get(drive_id).copied();
    if asked.is_some_and(|at| now.saturating_sub(at) < STATUS_ASK_SECS) {
        return;
    }
    s.sync_view.asked_status.insert(drive_id.to_string(), now);
    crate::periods::start_redemptions(info, app, s, Some(&azlin_id));
}

/// Runs a sync command of the ribbon, a menu or the Options for `drive` (`None`: the drive the
/// window shows).
pub(crate) fn run_action(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive: Option<String>,
    what: SyncAction,
) {
    let selected = matches!(what, SyncAction::KeepOnDevice | SyncAction::FreeUpSpace)
        .then(|| sync_view::selected_keys(s))
        .flatten();
    let Some(drive_id) = drive
        .or_else(|| selected.as_ref().map(|(d, _)| d.clone()))
        .or_else(|| sync_view::target_drive(s))
    else {
        return;
    };
    match what {
        SyncAction::Pair => sync_view::ask_pair(s, &drive_id),
        SyncAction::Now => {
            // A guard's question waiting: asked again.
            let states = s.sync_view.store.states(&drive_id);
            if states.burst.is_some() || states.mass_delete.is_some() {
                s.sync_view.asked.remove(&format!("{drive_id}\nburst"));
                s.sync_view.asked.remove(&format!("{drive_id}\nmass"));
                sync_view::ask_next_question(s);
            }
            request_pass(info, app, s, &drive_id);
        }
        SyncAction::KeepOnDevice => {
            let Some((_, keys)) = selected else { return };
            let states = s.sync_view.store.states(&drive_id);
            let all_kept = keys.iter().all(|k| states.is_pinned(k));
            if let Some(work) = work_of(info, s, &drive_id) {
                spawn(
                    info,
                    app,
                    s,
                    Job::Sync(SyncJob::Pin {
                        work,
                        keys,
                        on: !all_kept,
                    }),
                );
            }
        }
        SyncAction::FreeUpSpace => {
            let Some((_, keys)) = selected else { return };
            if let Some(work) = work_of(info, s, &drive_id) {
                spawn(info, app, s, Job::Sync(SyncJob::FreeUp { work, keys }));
            }
        }
        SyncAction::Pause => {
            let Some(setup) = s.settings.synced.iter_mut().find(|p| p.drive_id == drive_id) else {
                return;
            };
            setup.paused = !setup.paused;
            let paused = setup.paused;
            if paused {
                if let Some(running) = s.sync_view.drives.get(&drive_id).and_then(|d| d.running.as_ref())
                {
                    running.cancel.store(true, Ordering::SeqCst);
                }
            }
            publish(s);
            crate::save_settings(info, app, s);
            println!(
                "AZDRIVE_SYNC_{} {drive_id}",
                if paused { "PAUSED" } else { "RESUMED" }
            );
            sync_view::say_status(s, &drive_id);
            if !paused {
                request_pass(info, app, s, &drive_id);
            }
        }
        SyncAction::Stop => {
            if s.popup.is_none() {
                s.popups_opened += 1;
                s.popup = Some(Popup::Sync(SyncDialog::Stop { drive_id }));
            }
        }
        SyncAction::OpenFolder => {
            let folder = sync_view::setup_of(s, &drive_id).map(|p| p.folder.clone());
            match folder.and_then(|f| local_place(s, &f)) {
                Some(place) => go(info, app, s, place, true),
                None => s.warn(Text::key("azdrive-sync-folder-not-shown")),
            }
        }
    }
}

/// The folder the window shows lies in pairing `drive_id`: it is listed again (new files, the
/// states of its rows).
fn refresh_if_showing(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    if s.find.is_some() {
        return;
    }
    if sync_view::place_in_pair(s).is_some_and(|(d, _)| d == drive_id) {
        crate::start_listing(info, app, s, true);
    }
}

/// A sync job's answer.
pub(crate) fn on_outcome(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, outcome: SyncOutcome) {
    match outcome {
        SyncOutcome::Progress { drive_id, progress } => {
            s.sync_view
                .store
                .set_moving(&drive_id, progress.moving.clone());
            if let Some(running) = s
                .sync_view
                .drives
                .get_mut(&drive_id)
                .and_then(|d| d.running.as_mut())
            {
                running.progress = progress;
            }
            sync_view::say_status(s, &drive_id);
        }
        SyncOutcome::Passed {
            drive_id,
            result,
            states,
        } => {
            let sync: &mut DriveSync = s.sync_view.drives.entry(drive_id.clone()).or_default();
            sync.running = None;
            let again = std::mem::take(&mut sync.again);
            // An Azlin drive decided by now whether it is encrypted: its own listing may name
            // its files.
            publish(s);
            let before = s.sync_view.store.states(&drive_id);
            s.sync_view.store.set_moving(&drive_id, None);
            s.sync_view.store.set_states(&drive_id, states);
            match &result {
                Ok(done) => println!(
                    "AZDRIVE_SYNC_DONE {drive_id} up={} down={} deleted={} conflicts={} \
                     cloud_only={} freed={} paused={} newer_format={} held={}",
                    done.up,
                    done.down,
                    done.deleted,
                    done.conflicts,
                    done.cloud_only,
                    done.freed,
                    done.paused.is_some(),
                    done.newer_format.len(),
                    done.held
                ),
                Err(e) => println!("AZDRIVE_SYNC_FAILED {drive_id} {e}"),
            }
            guards_seen(info, s, &drive_id, &before);
            // A refused write: the token server says whether the drive is read-only.
            if result.is_err() && s.sync_view.store.states(&drive_id).read_only {
                ask_drive_status(info, app, s, &drive_id);
            }
            let after = s.sync_view.store.states(&drive_id);
            sync_view::print_changes(&drive_id, &before, &after);
            sync_view::say_status(s, &drive_id);
            refresh_if_showing(info, app, s, &drive_id);
            sync_view::ask_next_question(s);
            let paused = sync_view::setup_of(s, &drive_id).is_none_or(|p| p.paused);
            if again && !paused {
                request_pass(info, app, s, &drive_id);
            }
        }
        SyncOutcome::Opened {
            drive_id,
            key,
            result,
            states,
        } => {
            replace_states(s, &drive_id, states);
            match result {
                Ok(path) => {
                    println!("AZDRIVE_SYNC_OPENED {drive_id} {key}");
                    s.clear_notice();
                    if let Err(e) = crate::open_with_os(&path) {
                        s.error(e);
                    }
                }
                Err(e) => s.error(
                    Phrase::new("azdrive-sync-open-failed")
                        .arg("name", key::last_segment(&key))
                        .then(" ")
                        .then(e),
                ),
            }
            refresh_if_showing(info, app, s, &drive_id);
        }
        SyncOutcome::Changed {
            drive_id,
            done,
            result,
            states,
        } => {
            replace_states(s, &drive_id, states);
            match (done, result) {
                (_, Err(e)) => s.error(e),
                (SyncChange::Pinned(on), Ok(text)) => {
                    println!("AZDRIVE_SYNC_PINNED {drive_id} {}", if on { "on" } else { "off" });
                    s.info(text);
                    // A pinned file in the cloud comes down with the next pass.
                    if on {
                        request_pass(info, app, s, &drive_id);
                    }
                }
                (SyncChange::Freed, Ok(text)) => {
                    println!("AZDRIVE_SYNC_FREED {drive_id}");
                    s.info(text);
                }
                (SyncChange::Deleted, Ok(text)) => {
                    println!("AZDRIVE_SYNC_DELETED {drive_id}");
                    s.info(text);
                    request_pass(info, app, s, &drive_id);
                }
                (SyncChange::Touched, Ok(_)) => {}
                (SyncChange::Renamed, Ok(to)) => {
                    println!("AZDRIVE_SYNC_RENAMED {drive_id} {to}");
                    request_pass(info, app, s, &drive_id);
                }
                (SyncChange::Fetched, Ok(text)) => {
                    println!("AZDRIVE_SYNC_FETCHED {drive_id}");
                    s.info(text);
                    // The copy, move or download that waited for them.
                    if let Some(waiting) = s.sync_view.waiting_transfer.take() {
                        crate::actions::enqueue_routed(info, app, s, waiting);
                    }
                }
                (SyncChange::Answered, Ok(what)) => {
                    println!("AZDRIVE_SYNC_ANSWERED {drive_id} {what}");
                    request_pass(info, app, s, &drive_id);
                }
                (SyncChange::Resolved(choice), Ok(key)) => {
                    let word = match choice {
                        Resolution::KeepMine => "mine",
                        Resolution::TakeTheirs => "theirs",
                        Resolution::KeepBoth => "both",
                    };
                    println!("AZDRIVE_SYNC_RESOLVED {drive_id} {word} {key}");
                    request_pass(info, app, s, &drive_id);
                }
            }
            refresh_if_showing(info, app, s, &drive_id);
        }
    }
}

/// The states a job answered with take the place of the drive's (the changes printed).
fn replace_states(s: &mut DriveState, drive_id: &str, states: SyncStates) {
    let before = s.sync_view.store.states(drive_id);
    s.sync_view.store.set_states(drive_id, states);
    let after = s.sync_view.store.states(drive_id);
    sync_view::print_changes(drive_id, &before, &after);
    sync_view::say_status(s, drive_id);
}
