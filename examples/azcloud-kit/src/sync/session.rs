//! An app's pairing of a folder on this device with a drive's folder: AzDrive's sync. The kit's
//! loop ([`super::sync_to`]) with an app's hooks, and what an app keeps around it:
//!
//! - **The setup** ([`SyncSetup`], kept in the app's settings): the drive and its folder, the
//!   folder here, and the drive's settings - auto-download ("everything", "new files under N
//!   MB" - 25 by default -, "only pinned folders", "nothing (on demand)"), the local copies
//!   of an encrypted drive (decrypted, or encrypted and decrypted when opened), "keep local
//!   copies at most N GB", paused.
//! - **The states** ([`SyncStates`], `files.json` in the pairing's state folder - the app's
//!   cache folder; they outlive the app): per file its size and date, whether its bytes are
//!   here (cloud only, an encrypted copy), when it was last used, a conflict held for the
//!   user, an error; the pinned files and folders ("always keep on this device"); the drive's
//!   last error and whether it takes writes. [`SyncStates::state_of`] says §13.7's state.
//! - **A pass** ([`SyncSession::pass`]): the run (new files by the auto-download policy, the
//!   rest cloud only; conflicts held - D52), each file's move told as it happens, then the
//!   pinned files kept in the cloud brought down, the folders of cloud-only files made here
//!   (the folder browses as the drive does) and the size cap kept by freeing the least
//!   recently used files - never a pinned one.
//! - [`SyncSession::open`] brings a cloud-only file down first (an encrypted copy: decrypted
//!   into a temporary file); [`SyncSession::pin`], [`SyncSession::free_up`] ("Free up
//!   space"), [`SyncSession::resolve`] (keep mine / take theirs / keep both).
//!
//! The drive side is a plain drive's index and blobs ([`SyncSession::plain`]), an encrypted
//! drive's files by name ([`SyncSession::named`]), or - with `encryption` - an encrypted drive
//! whose local copies stay encrypted ([`SyncSession::encrypted_copies`]: AZL1 objects kept in
//! an [`super::objects::ObjectCache`], no plaintext folder). Every call blocks: call it from
//! an azul `Thread`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};

use azul_storage::{Drive, DriveError, ListRequest};
use serde::{Deserialize, Serialize};

#[cfg(feature = "encryption")]
use super::objects::ObjectCache;
use super::{
    evict_file, fetch_file,
    local::{self, BaseEntry, LocalIndex},
    lock, remote, sync_to, HeldConflict, LocalRoot, RunHooks, SyncEvent, SyncOptions,
    SyncReport, Target,
};
use crate::{
    error::{fail, CloudError, CloudResult, Context},
    state::{read_json, write_json},
    store::RemoteStore,
};

/// The states of a pairing's files, in its state folder.
pub const STATES_FILE: &str = "files.json";
/// The pairing's local index (the sync's base), in its state folder.
pub const INDEX_FILE: &str = "index.json";

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

/// What comes down to this device without being opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoDownload {
    /// Every file.
    Everything,
    /// New files up to this many MB.
    NewUnder(u64),
    /// Only pinned files and the files of pinned folders.
    PinnedOnly,
    /// Nothing: a file comes down when it is opened.
    Nothing,
}

impl Default for AutoDownload {
    fn default() -> Self {
        AutoDownload::NewUnder(AutoDownload::DEFAULT_MB)
    }
}

impl AutoDownload {
    /// "New files under N MB"'s N by default.
    pub const DEFAULT_MB: u64 = 25;

    /// Whether a new file of `size` bytes comes down.
    #[must_use]
    pub fn wants(self, size: u64) -> bool {
        match self {
            AutoDownload::Everything => true,
            AutoDownload::NewUnder(mb) => size <= mb.saturating_mul(MB),
            AutoDownload::PinnedOnly | AutoDownload::Nothing => false,
        }
    }

    /// The choice as the settings say it.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            AutoDownload::Everything => String::from("Everything"),
            AutoDownload::NewUnder(mb) => format!("New files under {mb} MB"),
            AutoDownload::PinnedOnly => String::from("Only pinned folders"),
            AutoDownload::Nothing => String::from("Nothing (on demand)"),
        }
    }
}

/// How an encrypted drive's files are kept on this device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalCopies {
    /// Plaintext in the folder: fast to open and search.
    #[default]
    Decrypted,
    /// The AZL1 ciphertext only, decrypted into a temporary file when opened.
    Encrypted,
}

impl LocalCopies {
    /// The choice as the settings say it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            LocalCopies::Decrypted => "Decrypted (fast to open and search)",
            LocalCopies::Encrypted => "Encrypted, decrypted when opened",
        }
    }
}

/// A folder here paired with a drive's folder, and the drive's sync settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSetup {
    /// The drive (its id in the drives file).
    pub drive_id: String,
    /// The drive's folder (`""`: the whole drive; else ending in `/`).
    #[serde(default)]
    pub prefix: String,
    /// The folder on this device.
    pub folder: PathBuf,
    #[serde(default)]
    pub auto_download: AutoDownload,
    /// An encrypted drive's local copies (a plain drive's are its files).
    #[serde(default)]
    pub local_copies: LocalCopies,
    /// Keep local copies at most this many GB: the least recently used files are freed first,
    /// never a pinned one. `None`: no limit.
    #[serde(default)]
    pub keep_gb: Option<u64>,
    /// No pass runs while it is paused.
    #[serde(default)]
    pub paused: bool,
}

impl SyncSetup {
    /// `folder` paired with `prefix` of the drive `drive_id`, with the default settings.
    #[must_use]
    pub fn new(drive_id: &str, prefix: &str, folder: &Path) -> SyncSetup {
        SyncSetup {
            drive_id: drive_id.to_string(),
            prefix: prefix.to_string(),
            folder: folder.to_path_buf(),
            auto_download: AutoDownload::default(),
            local_copies: LocalCopies::default(),
            keep_gb: None,
            paused: false,
        }
    }

    /// The pairing's state folder under `root` (an app's cache folder for its syncs): one per
    /// drive, drive folder and folder here.
    #[must_use]
    pub fn state_dir(&self, root: &Path) -> PathBuf {
        let id = blake3::hash(
            format!(
                "{}\n{}\n{}",
                self.drive_id,
                self.prefix,
                self.folder.display()
            )
            .as_bytes(),
        )
        .to_hex();
        let name: String = self
            .drive_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        root.join(format!("{name}-{}", &id.as_str()[..12]))
    }

    /// The size cap in bytes.
    #[must_use]
    pub fn keep_bytes(&self) -> Option<u64> {
        self.keep_gb.map(|gb| gb.saturating_mul(GB))
    }
}

/// A file's state as AzDrive shows it (§13.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileState {
    /// Known from the drive, its bytes not on this device.
    CloudOnly,
    /// In transfer: bytes so far of all.
    Downloading { done: u64, total: u64 },
    Uploading { done: u64, total: u64 },
    /// Plaintext on this device.
    OnDevice,
    /// The AZL1 ciphertext on this device, decrypted when opened.
    OnDeviceEncrypted,
    /// Always kept on this device, never freed.
    Pinned,
    /// Changed here and on the drive: waits for keep mine / take theirs / keep both.
    Conflict,
    /// A transfer failed, and why ("Retry" tries again).
    Error(String),
}

impl FileState {
    /// What the state says, in a sentence.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            FileState::CloudOnly => String::from("Cloud only: downloaded when opened"),
            FileState::Downloading { .. } => String::from("Downloading"),
            FileState::Uploading { .. } => String::from("Uploading"),
            FileState::OnDevice => String::from("On this device"),
            FileState::OnDeviceEncrypted => {
                String::from("On this device (encrypted): decrypted when opened")
            }
            FileState::Pinned => String::from("Always kept on this device"),
            FileState::Conflict => String::from("Changed here and on the drive"),
            FileState::Error(why) => format!("Not synced: {why}"),
        }
    }
}

/// What a pairing keeps of one file of the drive.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// Bytes, and when it was last changed (seconds).
    pub size: u64,
    #[serde(default)]
    pub modified: i64,
    /// Its bytes are not on this device.
    #[serde(default)]
    pub cloud_only: bool,
    /// Its AZL1 ciphertext is on this device (an encrypted drive's encrypted local copies),
    /// as the object `object` of the bucket.
    #[serde(default)]
    pub encrypted_copy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// When it was last opened or moved (seconds): the size cap frees the oldest first.
    #[serde(default)]
    pub last_used: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<HeldConflict>,
}

impl FileRecord {
    /// Its bytes (or its ciphertext) are on this device.
    fn here(&self) -> bool {
        !self.cloud_only || self.encrypted_copy
    }
}

/// What a pairing knows of its files between passes and runs of the app.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStates {
    /// Every file of the drive's folder, by its key under it.
    #[serde(default)]
    pub files: BTreeMap<String, FileRecord>,
    /// Always kept on this device: files, and folders (ending in `/`) with all they hold.
    #[serde(default)]
    pub pinned: BTreeSet<String>,
    /// Conflicts the user answered "keep both": the next pass makes the conflict copy.
    #[serde(default)]
    pub keep_both: BTreeSet<String>,
    /// When the last pass ended (seconds), and why the last one failed.
    #[serde(default)]
    pub last_pass: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
    /// The drive refused a write (unpaid, locked down): it is read-only.
    #[serde(default)]
    pub read_only: bool,
}

impl SyncStates {
    /// The states kept in the state folder `dir` (none yet: empty).
    #[must_use]
    pub fn load(dir: &Path) -> SyncStates {
        read_json::<SyncStates>(&dir.join(STATES_FILE))
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Keeps them in the state folder `dir`.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn save(&self, dir: &Path) -> CloudResult<()> {
        write_json(&dir.join(STATES_FILE), self, false)
    }

    /// Whether `key` (a file, or a folder ending in `/`) is always kept on this device: it is
    /// pinned, or a folder it lies in is.
    #[must_use]
    pub fn is_pinned(&self, key: &str) -> bool {
        self.pinned.contains(key)
            || self
                .pinned
                .iter()
                .any(|p| p.ends_with('/') && key.len() > p.len() && key.starts_with(p.as_str()))
    }

    /// The state of the file `key`; `None` when the drive has no such file.
    #[must_use]
    pub fn state_of(&self, key: &str) -> Option<FileState> {
        let record = self.files.get(key)?;
        Some(if record.conflict.is_some() {
            FileState::Conflict
        } else if let Some(why) = &record.error {
            FileState::Error(why.clone())
        } else if record.encrypted_copy {
            if self.is_pinned(key) {
                FileState::Pinned
            } else {
                FileState::OnDeviceEncrypted
            }
        } else if record.cloud_only {
            FileState::CloudOnly
        } else if self.is_pinned(key) {
            FileState::Pinned
        } else {
            FileState::OnDevice
        })
    }

    /// The state of the folder `folder` (ending in `/`): a conflict or an error of a file in it
    /// first, then pinned, then cloud only when none of its files is here, else on this
    /// device. `None` when the drive has no file in it.
    #[must_use]
    pub fn folder_state(&self, folder: &str) -> Option<FileState> {
        let mut any = false;
        let mut all_in_cloud = true;
        let mut error = None;
        for (_, record) in self
            .files
            .range(folder.to_string()..)
            .take_while(|(key, _)| key.starts_with(folder))
        {
            any = true;
            if record.conflict.is_some() {
                return Some(FileState::Conflict);
            }
            if error.is_none() {
                error.clone_from(&record.error);
            }
            if record.here() {
                all_in_cloud = false;
            }
        }
        if !any {
            return None;
        }
        if let Some(why) = error {
            return Some(FileState::Error(why));
        }
        if self.is_pinned(folder) {
            return Some(FileState::Pinned);
        }
        Some(if all_in_cloud {
            FileState::CloudOnly
        } else {
            FileState::OnDevice
        })
    }

    /// The bytes of the files on this device.
    #[must_use]
    pub fn local_bytes(&self) -> u64 {
        self.files
            .values()
            .filter(|r| r.here())
            .map(|r| r.size)
            .sum()
    }

    /// The files to free to get under `cap` bytes: the least recently used first; never a
    /// pinned file, one in conflict or one whose transfer failed.
    #[must_use]
    pub fn to_free(&self, cap: u64) -> Vec<String> {
        let mut total = self.local_bytes();
        if total <= cap {
            return Vec::new();
        }
        let mut candidates: Vec<(&String, &FileRecord)> = self
            .files
            .iter()
            .filter(|(key, r)| {
                r.here() && r.conflict.is_none() && r.error.is_none() && !self.is_pinned(key)
            })
            .collect();
        candidates.sort_by(|a, b| a.1.last_used.cmp(&b.1.last_used).then(a.0.cmp(b.0)));
        let mut out = Vec::new();
        for (key, record) in candidates {
            if total <= cap {
                break;
            }
            total = total.saturating_sub(record.size);
            out.push(key.clone());
        }
        out
    }

    /// The conflicts waiting for the user.
    #[must_use]
    pub fn conflicts(&self) -> Vec<&HeldConflict> {
        self.files
            .values()
            .filter_map(|r| r.conflict.as_ref())
            .collect()
    }

    /// The files of `keys` (folders ending in `/` - `""` the whole drive - with every file in
    /// them) that are on this device, in order.
    fn here_under(&self, keys: &[String]) -> Vec<String> {
        let mut out = BTreeSet::new();
        for key in keys {
            if key.is_empty() || key.ends_with('/') {
                out.extend(
                    self.files
                        .range(key.clone()..)
                        .take_while(|(k, _)| k.starts_with(key.as_str()))
                        .filter(|(_, r)| r.here())
                        .map(|(k, _)| k.clone()),
                );
            } else {
                out.insert(key.clone());
            }
        }
        out.into_iter().collect()
    }
}

/// What a pass did.
#[derive(Clone, Debug)]
pub struct Pass {
    /// The run's report (`None`: an encrypted-copies pass, which runs no sync).
    pub report: Option<SyncReport>,
    /// The states after it (kept in the state folder).
    pub states: SyncStates,
    /// Cloud-only files brought down (pinned ones), and files freed for the size cap.
    pub fetched: Vec<String>,
    pub freed: Vec<String>,
}

/// What "Free up space" did: the files freed, the ones kept and why.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Freed {
    pub freed: Vec<String>,
    pub kept: Vec<(String, String)>,
}

/// The answer to a conflict (D52).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// This device's version goes to the drive.
    KeepMine,
    /// The drive's version comes here.
    TakeTheirs,
    /// The drive's keeps the name, this device's becomes a conflict copy.
    KeepBoth,
}

/// Whether `error` says the drive takes no writes (refused: unpaid past its grace period,
/// locked down, a read-only key).
#[must_use]
pub fn is_read_only(error: &CloudError) -> bool {
    match error.root() {
        CloudError::Drive(DriveError::Denied { .. }) => true,
        CloudError::Drive(DriveError::Service(e)) => e.status == 403 || e.status == 402,
        _ => false,
    }
}

/// This device's name in conflict copies: `$AZCLOUD_DEVICE`, the host's name, else "this
/// device".
#[must_use]
pub fn device_name() -> String {
    ["AZCLOUD_DEVICE", "HOSTNAME", "COMPUTERNAME"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .map(|name| name.trim().to_string())
        .find(|name| !name.is_empty())
        .unwrap_or_else(|| String::from("this device"))
}

/// The drive side of a pairing.
enum SessionRemote {
    /// A plain drive: the plain index and blobs.
    Index(Arc<dyn RemoteStore>),
    /// An encrypted drive: its files by name.
    Named(Arc<dyn Drive>),
    /// An encrypted drive whose local copies stay encrypted.
    #[cfg(feature = "encryption")]
    Encrypted {
        drive: Arc<dyn Drive>,
        objects: Arc<ObjectCache>,
    },
}

/// A folder here paired with a drive's folder.
pub struct SyncSession {
    setup: SyncSetup,
    dir: PathBuf,
    device: String,
    remote: SessionRemote,
}

impl SyncSession {
    /// A plain drive (`store`: its bucket, [`super::drive_store::DriveStore`] over the app's
    /// drive), its pairing's state in `dir`, this device named `device` in conflict copies.
    #[must_use]
    pub fn plain(
        setup: SyncSetup,
        dir: PathBuf,
        device: &str,
        store: Arc<dyn RemoteStore>,
    ) -> SyncSession {
        Self::with(setup, dir, device, SessionRemote::Index(store))
    }

    /// An encrypted drive (`drive`: the drive the app reads, names from its index) whose
    /// local copies are plaintext in the folder.
    #[must_use]
    pub fn named(setup: SyncSetup, dir: PathBuf, device: &str, drive: Arc<dyn Drive>) -> SyncSession {
        Self::with(setup, dir, device, SessionRemote::Named(drive))
    }

    /// An encrypted drive whose local copies stay encrypted: `objects` is the cache below the
    /// encryption of `drive`; nothing is written to the folder.
    #[cfg(feature = "encryption")]
    #[must_use]
    pub fn encrypted_copies(
        setup: SyncSetup,
        dir: PathBuf,
        device: &str,
        drive: Arc<dyn Drive>,
        objects: Arc<ObjectCache>,
    ) -> SyncSession {
        Self::with(setup, dir, device, SessionRemote::Encrypted { drive, objects })
    }

    fn with(setup: SyncSetup, dir: PathBuf, device: &str, remote: SessionRemote) -> SyncSession {
        SyncSession {
            setup,
            dir,
            device: device.to_string(),
            remote,
        }
    }

    #[must_use]
    pub fn setup(&self) -> &SyncSetup {
        &self.setup
    }

    /// The pairing's state folder.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The states as the last pass (or call) left them.
    #[must_use]
    pub fn states(&self) -> SyncStates {
        SyncStates::load(&self.dir)
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join(INDEX_FILE)
    }

    fn options(&self) -> CloudResult<SyncOptions> {
        SyncOptions::new(&self.setup.prefix, &self.setup.drive_id, &self.device)
    }

    fn root(&self) -> LocalRoot {
        let mut root = LocalRoot::folder(&self.setup.folder);
        root.rules
            .add("/.azdrive-trash/", "AzDrive's trash of a folder added as a drive");
        root
    }

    fn target(&self) -> Target<'_> {
        match &self.remote {
            SessionRemote::Index(store) => Target::Index(store.as_ref()),
            SessionRemote::Named(drive) => Target::Named(drive.as_ref()),
            #[cfg(feature = "encryption")]
            SessionRemote::Encrypted { drive, .. } => Target::Named(drive.as_ref()),
        }
    }

    /// Whether the drive answers a read (a refused write then means read-only).
    fn readable(&self) -> bool {
        match &self.remote {
            SessionRemote::Index(store) => self
                .options()
                .and_then(|opts| store.get_unless(&remote::index_key(&opts.prefix), None))
                .is_ok(),
            SessionRemote::Named(drive) => listable(drive.as_ref(), &self.setup.prefix),
            #[cfg(feature = "encryption")]
            SessionRemote::Encrypted { drive, .. } => listable(drive.as_ref(), &self.setup.prefix),
        }
    }

    /// One pass: the sync (or, for encrypted copies, the drive's listing and the copies), told
    /// to `events` as it moves files, stopped by `cancel`.
    ///
    /// # Errors
    ///
    /// When the run fails (the states keep why, and whether the drive is read-only).
    pub fn pass(
        &self,
        cancel: &AtomicBool,
        events: &mut dyn FnMut(SyncEvent),
    ) -> CloudResult<Pass> {
        let mut states = self.states();
        #[cfg(feature = "encryption")]
        {
            if let SessionRemote::Encrypted { drive, objects } = &self.remote {
                let result = self.cache_pass(drive.as_ref(), objects, &mut states, cancel, events);
                return self.after_failure(result, &mut states);
            }
        }
        let result = self.sync_pass(&mut states, cancel, events);
        self.after_failure(result, &mut states)
    }

    /// A failed pass keeps why in the states.
    fn after_failure(&self, result: CloudResult<Pass>, states: &mut SyncStates) -> CloudResult<Pass> {
        if let Err(e) = &result {
            states.last_error = Some(e.to_string());
            states.read_only = is_read_only(e) && self.readable();
            let _ = states.save(&self.dir);
        }
        result
    }

    fn sync_pass(
        &self,
        states: &mut SyncStates,
        cancel: &AtomicBool,
        events: &mut dyn FnMut(SyncEvent),
    ) -> CloudResult<Pass> {
        fs::create_dir_all(&self.setup.folder)
            .with_context(|| format!("{}", self.setup.folder.display()))?;
        let root = self.root();
        let opts = self.options()?;
        let index_path = self.index_path();
        let policy = self.setup.auto_download;
        let pins = states.clone();
        let fetch = |key: &str, size: u64| pins.is_pinned(key) || policy.wants(size);
        let keep_both = states.keep_both.clone();
        let target = self.target();
        let mut moved: BTreeSet<String> = BTreeSet::new();
        let (sender, heard) = mpsc::channel::<SyncEvent>();
        let (fetch, keep_both, root_ref, opts_ref, index_ref) =
            (&fetch, &keep_both, &root, &opts, &index_path);
        let outcome = std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let sender = Mutex::new(sender);
                let hear = |event: SyncEvent| {
                    let _ = lock(&sender).send(event);
                };
                let hooks = RunHooks {
                    fetch: Some(fetch),
                    hold_conflicts: true,
                    keep_both: Some(keep_both),
                    progress: Some(&hear),
                    cancel: Some(cancel),
                };
                sync_to(target, root_ref, index_ref, opts_ref, &hooks)
            });
            for event in heard {
                if let SyncEvent::Finished {
                    key, error: None, ..
                } = &event
                {
                    moved.insert(key.clone());
                }
                events(event);
            }
            worker.join()
        });
        let report = match outcome {
            Ok(result) => result?,
            Err(_) => fail!("the sync of {} stopped unexpectedly", self.setup.folder.display()),
        };
        record(states, &report, &moved);
        // Pinned files kept in the cloud come down now.
        let mut fetched = Vec::new();
        let pinned: Vec<(String, u64)> = report
            .cloud_only
            .iter()
            .filter(|key| states.is_pinned(key))
            .map(|key| (key.clone(), states.files.get(key).map_or(0, |r| r.size)))
            .collect();
        for (key, size) in pinned {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            events(SyncEvent::Started {
                key: key.clone(),
                up: false,
                bytes: size,
            });
            let error = fetch_file(target, &root, &index_path, &opts, &key)
                .err()
                .map(|e| e.to_string());
            if let Some(record) = states.files.get_mut(&key) {
                match &error {
                    None => {
                        record.cloud_only = false;
                        record.last_used = crate::now();
                    }
                    Some(why) => record.error = Some(why.clone()),
                }
            }
            if error.is_none() {
                fetched.push(key.clone());
            }
            events(SyncEvent::Finished {
                key,
                up: false,
                bytes: size,
                error,
            });
        }
        // The folders of cloud-only files are here: the folder browses as the drive does.
        for (key, _) in states.files.iter().filter(|(_, r)| r.cloud_only) {
            if let Some(parent) = local::path_of(&self.setup.folder, key).parent() {
                let _ = fs::create_dir_all(parent);
            }
        }
        let freed = self.keep_cap(states);
        states.last_pass = Some(crate::now());
        states.last_error = None;
        states.read_only = false;
        states.save(&self.dir)?;
        Ok(Pass {
            report: Some(report),
            states: states.clone(),
            fetched,
            freed,
        })
    }

    /// Frees the least recently used files until the size cap holds.
    fn keep_cap(&self, states: &mut SyncStates) -> Vec<String> {
        let Some(cap) = self.setup.keep_bytes() else {
            return Vec::new();
        };
        let mut freed = Vec::new();
        for key in states.to_free(cap) {
            if self.evict_one(&key, states).is_ok() {
                freed.push(key);
            }
        }
        freed
    }

    /// Frees `key`'s bytes on this device.
    fn evict_one(&self, key: &str, states: &mut SyncStates) -> CloudResult<()> {
        #[cfg(feature = "encryption")]
        {
            if let SessionRemote::Encrypted { objects, .. } = &self.remote {
                if let Some(record) = states.files.get_mut(key) {
                    if let Some(object) = record.object.take() {
                        objects.evict(&object)?;
                    }
                    record.encrypted_copy = false;
                    record.cloud_only = true;
                }
                return Ok(());
            }
        }
        evict_file(&self.root(), &self.index_path(), key)?;
        if let Some(record) = states.files.get_mut(key) {
            record.cloud_only = true;
        }
        Ok(())
    }

    /// The file `key` on this device to open: brought down first when it is in the cloud only
    /// (an encrypted copy: decrypted into a temporary file); marked used.
    ///
    /// # Errors
    ///
    /// When it cannot be brought down or decrypted.
    pub fn open(&self, key: &str) -> CloudResult<PathBuf> {
        let mut states = self.states();
        let path = self.open_path(key, &mut states)?;
        if let Some(record) = states.files.get_mut(key) {
            record.last_used = crate::now();
            record.error = None;
        }
        states.save(&self.dir)?;
        Ok(path)
    }

    fn open_path(&self, key: &str, states: &mut SyncStates) -> CloudResult<PathBuf> {
        #[cfg(feature = "encryption")]
        {
            if let SessionRemote::Encrypted { drive, objects } = &self.remote {
                return self.decrypt(drive.as_ref(), objects, key, states);
            }
        }
        let path = fetch_file(
            self.target(),
            &self.root(),
            &self.index_path(),
            &self.options()?,
            key,
        )?;
        if let Some(record) = states.files.get_mut(key) {
            record.cloud_only = false;
        }
        Ok(path)
    }

    /// Pins (`on`) or unpins `keys` (files, and folders ending in `/`): a pinned file is never
    /// freed, and the next pass brings it down when it is in the cloud only. The states.
    ///
    /// # Errors
    ///
    /// When the states cannot be kept.
    pub fn pin(&self, keys: &[String], on: bool) -> CloudResult<SyncStates> {
        let mut states = self.states();
        for key in keys {
            if on {
                states.pinned.insert(key.clone());
            } else {
                states.pinned.remove(key);
            }
        }
        states.save(&self.dir)?;
        Ok(states)
    }

    /// "Free up space": the bytes of `keys` (files, folders ending in `/`) leave this device;
    /// the drive keeps them. Freeing a pinned file or folder unpins it; a file in a folder
    /// that stays pinned is kept, and so is one with changes not synced yet.
    ///
    /// # Errors
    ///
    /// When the states cannot be kept.
    pub fn free_up(&self, keys: &[String]) -> CloudResult<Freed> {
        let mut states = self.states();
        for key in keys {
            states.pinned.remove(key);
        }
        let mut out = Freed::default();
        for key in states.here_under(keys) {
            if states.is_pinned(&key) {
                out.kept.push((
                    key,
                    String::from("it is in a folder that is always kept on this device"),
                ));
                continue;
            }
            match self.evict_one(&key, &mut states) {
                Ok(()) => out.freed.push(key),
                Err(e) => out.kept.push((key, e.to_string())),
            }
        }
        states.save(&self.dir)?;
        Ok(out)
    }

    /// Answers the conflict of `key` (D52); the next pass does it.
    ///
    /// # Errors
    ///
    /// When `key` has no conflict, or the base cannot be changed.
    pub fn resolve(&self, key: &str, choice: Resolution) -> CloudResult<()> {
        let mut states = self.states();
        let Some(held) = states.files.get(key).and_then(|r| r.conflict.clone()) else {
            fail!("{key} has no conflict to answer");
        };
        match choice {
            Resolution::KeepBoth => {
                states.keep_both.insert(key.to_string());
            }
            // The base takes the drive's version: mine is the change, and goes up.
            Resolution::KeepMine => self.set_base(
                key,
                BaseEntry {
                    hash: held.there,
                    size: held.there_size,
                    mtime_ns: 0,
                    cloud_only: false,
                },
            )?,
            // The base takes mine as it is now: the drive's is the change, and comes here.
            Resolution::TakeTheirs => {
                let path = local::path_of(&self.setup.folder, key);
                let meta = fs::metadata(&path).with_context(|| format!("{}", path.display()))?;
                let hash = local::hash_file(&path).with_context(|| format!("{}", path.display()))?;
                self.set_base(
                    key,
                    BaseEntry {
                        hash,
                        size: meta.len(),
                        mtime_ns: local::mtime_ns(&meta),
                        cloud_only: false,
                    },
                )?;
            }
        }
        if let Some(record) = states.files.get_mut(key) {
            record.conflict = None;
        }
        states.save(&self.dir)
    }

    fn set_base(&self, key: &str, entry: BaseEntry) -> CloudResult<()> {
        let path = self.index_path();
        let Some(mut index) = LocalIndex::load(&path)? else {
            fail!("{} was never synced", self.setup.folder.display());
        };
        index.files.insert(key.to_string(), entry);
        index.save(&path)
    }

    /// An encrypted-copies pass: the drive's files from its index, the ones the policy wants
    /// (pinned, new and small enough) kept as ciphertext, the size cap kept.
    #[cfg(feature = "encryption")]
    fn cache_pass(
        &self,
        drive: &dyn Drive,
        objects: &ObjectCache,
        states: &mut SyncStates,
        cancel: &AtomicBool,
        events: &mut dyn FnMut(SyncEvent),
    ) -> CloudResult<Pass> {
        let listing = azul_storage::ops::list_all(drive, &self.setup.prefix)?;
        let now = crate::now();
        let mut files = BTreeMap::new();
        let mut wanted: Vec<(String, String, u64)> = Vec::new();
        for info in listing {
            let Some(key) = info.key.strip_prefix(&self.setup.prefix) else {
                continue;
            };
            if key.is_empty() || key.ends_with('/') || remote::check_key(key).is_err() {
                continue;
            }
            let etag = info.etag.as_deref().unwrap_or("").trim_matches('"');
            let object = bucket_key_of(etag);
            let kept = object.as_deref().is_some_and(|o| objects.has(o));
            let old = states.files.get(key);
            let modified = info.modified.and_then(|m| i64::try_from(m).ok()).unwrap_or(0);
            let wants = states.is_pinned(key)
                || (old.is_none() && self.setup.auto_download.wants(info.size));
            if wants && !kept {
                if let Some(object) = &object {
                    wanted.push((key.to_string(), object.clone(), info.size));
                }
            }
            files.insert(
                key.to_string(),
                FileRecord {
                    size: info.size,
                    modified,
                    cloud_only: !kept,
                    encrypted_copy: kept,
                    object: if kept { object } else { None },
                    last_used: old.map_or(modified, |r| r.last_used),
                    error: None,
                    conflict: None,
                },
            );
        }
        events(SyncEvent::Planned {
            up_files: 0,
            up_bytes: 0,
            down_files: wanted.len(),
            down_bytes: wanted.iter().map(|w| w.2).sum(),
        });
        let mut fetched = Vec::new();
        for (key, object, size) in wanted {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            events(SyncEvent::Started {
                key: key.clone(),
                up: false,
                bytes: size,
            });
            let error = objects.fill(&object).err().map(|e| e.to_string());
            if let Some(record) = files.get_mut(&key) {
                match &error {
                    None => {
                        record.cloud_only = false;
                        record.encrypted_copy = true;
                        record.object = Some(object.clone());
                        record.last_used = now;
                    }
                    Some(why) => record.error = Some(why.clone()),
                }
            }
            if error.is_none() {
                fetched.push(key.clone());
            }
            events(SyncEvent::Finished {
                key,
                up: false,
                bytes: size,
                error,
            });
        }
        states.files = files;
        let freed = self.keep_cap(states);
        states.last_pass = Some(now);
        states.last_error = None;
        states.read_only = false;
        states.save(&self.dir)?;
        Ok(Pass {
            report: None,
            states: states.clone(),
            fetched,
            freed,
        })
    }

    /// An encrypted copy opened: kept here first (when it is in the cloud only), then decrypted
    /// into a temporary file of the pairing's state folder (readable by this user only).
    #[cfg(feature = "encryption")]
    fn decrypt(
        &self,
        drive: &dyn Drive,
        objects: &ObjectCache,
        key: &str,
        states: &mut SyncStates,
    ) -> CloudResult<PathBuf> {
        let full = format!("{}{key}", self.setup.prefix);
        let kept = states.files.get(key).is_some_and(|r| r.encrypted_copy);
        if !kept {
            let etag = drive.head(&full)?.etag.unwrap_or_default();
            if let Some(object) = bucket_key_of(etag.trim_matches('"')) {
                objects.fill(&object)?;
                if let Some(record) = states.files.get_mut(key) {
                    record.cloud_only = false;
                    record.encrypted_copy = true;
                    record.object = Some(object);
                }
            }
        }
        let bytes = drive.get(&full)?;
        let path = local::path_of(&self.dir.join("open"), key);
        crate::state::write_atomic(&path, &bytes, true)
            .with_context(|| format!("{}", path.display()))?;
        Ok(path)
    }
}

/// The bucket key of the AZL1 object an encrypted drive's entity tag names.
#[cfg(feature = "encryption")]
fn bucket_key_of(etag: &str) -> Option<String> {
    azul_storage::crypto::ObjectId::from_hex(etag).map(|id| id.bucket_key())
}

/// Whether `drive` lists `prefix`.
fn listable(drive: &dyn Drive, prefix: &str) -> bool {
    drive
        .list(&ListRequest::folder(prefix).with_max_keys(1))
        .is_ok()
}

/// The states after a run: a record per file of the drive (its bytes here or not, used when it
/// moved), the conflicts held, the files that failed; "keep both" answered for the conflicts
/// the run copied.
fn record(states: &mut SyncStates, report: &SyncReport, moved: &BTreeSet<String>) {
    let now = crate::now();
    let cloud_only: BTreeSet<&str> = report.cloud_only.iter().map(String::as_str).collect();
    let mut files = match &report.remote {
        Some(remote) => remote
            .files
            .iter()
            .map(|(key, file)| {
                let old = states.files.get(key);
                let last_used = if moved.contains(key) {
                    now
                } else {
                    old.map_or(file.mtime, |r| r.last_used)
                };
                let record = FileRecord {
                    size: file.size,
                    modified: file.mtime,
                    cloud_only: cloud_only.contains(key.as_str()),
                    last_used,
                    ..FileRecord::default()
                };
                (key.clone(), record)
            })
            .collect(),
        // A stopped run: the files are as they were.
        None => states.files.clone(),
    };
    for held in &report.held {
        files.entry(held.key.clone()).or_default().conflict = Some(held.clone());
    }
    for (key, why) in &report.file_errors {
        files.entry(key.clone()).or_default().error = Some(why.clone());
    }
    states.files = files;
    states
        .keep_both
        .retain(|key| report.held.iter().any(|h| h.key == *key));
}
