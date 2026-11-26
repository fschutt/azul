//! The model of the built-in recognizer, and how it gets onto the disk: nothing of it is bundled, the
//! program fetches it when it is first needed, once, if the app and the user allow it.
//!
//! * the model is a pinned file ([`ModelSource`]: where it is, how large, which SHA-256), kept in a
//!   folder of its own (the user's cache folder, or the one the app names) under a name that carries
//!   the start of the checksum, so another model never reads the file of the first;
//! * [`DownloadPolicy`] is the app's switch (the user's, through the app): `Never` fetches nothing, `Ask`
//!   (the default) fetches nothing until the app has [allowed](ModelStore::allow) it, which is the
//!   consent step an app shows its user ("the speech model, 16.9 MB from huggingface.co: download?"),
//!   `Automatic` fetches at the first need. An offline program fetches nothing in any of them;
//! * the download is resumable and windowed (`azul_layout::ranged_download`): it runs on a thread of its own,
//!   its progress is read with [`ModelStore::state`] without waiting, [`ModelStore::cancel`] stops it at
//!   the next window and keeps what came, and the next allow resumes from there;
//! * what came is trusted only when its size and its checksum are the pinned ones; then, and only then,
//!   the file gets its name (one `rename`: a program that dies half way leaves a `.partial` file, never a
//!   model that is half a model).
//!
//! The transport is a seam ([`ModelFetch`]) so the rules above are tested without a network.

// The items are `pub` for the tests and for the parts of the module that are re-exported.
#![allow(unreachable_pub)]

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, PoisonError,
    },
};

/// A model the recognizer can fetch: where it is, how large it is and the checksum it must have.
#[derive(Debug, Clone, Copy)]
pub struct ModelSource {
    /// The name of the file in the model folder: the model and the start of its checksum.
    pub file_name: &'static str,
    /// Where it is fetched from (pinned to a revision: the bytes behind it do not change).
    pub url: &'static str,
    /// The SHA-256 of the file, 64 lowercase hex digits.
    pub sha256: &'static str,
    /// The size of the file, bytes.
    pub bytes: u64,
    /// The licence the model's own card names, for the sentence that asks the user.
    pub licence: &'static str,
}

/// Whistle, the default model (`Cactus-Compute/whistle`, 16.9 MB): the revision, the size and the checksum
/// are those `scripts/fetch_whistle.sh` pins, and a test keeps the two in step.
pub static WHISTLE: ModelSource = ModelSource {
    file_name: "whistle-b6e02f048568.cact",
    url: "https://huggingface.co/Cactus-Compute/whistle/resolve/88cb00eeb8928ddbcd410b491584f938ba030b07/whistle.cact",
    sha256: "b6e02f048568ac5d01a2042556c658061e699acbc0aa2a1439f52f3d461dffeb",
    bytes: 16_919_407,
    licence: "Apache-2.0 by its model card",
};

/// Whether, and when, the program fetches the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DownloadPolicy {
    /// Never: a model that is not there stays missing, even when the app allows the download.
    Never,
    /// Not until the app has allowed it ([`ModelStore::allow`]): the consent step. The default.
    #[default]
    Ask,
    /// At the first need, without asking.
    Automatic,
}

/// Where the model is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPhase {
    /// The file is there and complete: nothing to do.
    Present,
    /// It is being fetched: [`ModelState::bytes_done`] of [`ModelState::bytes_total`].
    Downloading,
    /// It is not there, the policy is `Ask` and nobody has allowed it: the app asks its user.
    NeedsConsent,
    /// It is not there and nothing will fetch it (the policy is `Never`, or `Automatic` before the first need).
    Missing,
    /// The last attempt failed: [`ModelState::message`] says how; allowing again tries again.
    Failed,
    /// It is not there and the program is offline: nothing is fetched.
    Offline,
    /// There is no folder to keep it in: the app names one.
    NoFolder,
}

/// What a store says about its model: a snapshot, read without waiting for the download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelState {
    pub phase: ModelPhase,
    /// Bytes of the file that are on disk (while it downloads).
    pub bytes_done: u64,
    /// The size of the file, bytes.
    pub bytes_total: u64,
    /// Where the file is, once it is [`ModelPhase::Present`].
    pub path: Option<PathBuf>,
    /// A sentence for the user or a status line.
    pub message: String,
    /// Where the model is fetched from.
    pub url: String,
}

/// Why the transport did not bring the whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// It was stopped; the partial file stays.
    Cancelled,
    /// The server or the size limit refused it; the partial file is gone.
    Refused(String),
    /// The transport or the disk failed; the partial file stays.
    Failed(String),
}

/// How the bytes arrive, and how a file is checked: the seam the rules of the store are tested through.
pub trait ModelFetch: Send + Sync {
    /// Downloads `source` into `partial`, appending to what is there (an earlier, stopped attempt), and
    /// calls `progress` with the bytes that are on disk and the size when it is known. `cancel` is asked
    /// between the windows: true stops the download there.
    ///
    /// # Errors
    ///
    /// [`FetchError`] when it is stopped, refused or broke.
    fn download(
        &self,
        source: &ModelSource,
        partial: &Path,
        progress: &dyn Fn(u64, Option<u64>),
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError>;

    /// The SHA-256 of the file at `path`, 64 lowercase hex digits.
    ///
    /// # Errors
    ///
    /// The sentence of the I/O error.
    fn sha256_hex(&self, path: &Path) -> Result<String, String>;
}

/// A build with no way to the network: every download fails with the reason.
#[derive(Debug, Clone, Copy)]
pub struct NoNetwork;

impl ModelFetch for NoNetwork {
    fn download(
        &self,
        _source: &ModelSource,
        _partial: &Path,
        _progress: &dyn Fn(u64, Option<u64>),
        _cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError> {
        Err(FetchError::Failed("this build has no HTTP client".to_string()))
    }

    fn sha256_hex(&self, _path: &Path) -> Result<String, String> {
        Err("this build cannot read a checksum".to_string())
    }
}

/// What the app and the user chose, and where the folder is.
#[derive(Debug, Clone)]
struct Config {
    /// The folder the app named, if it did.
    chosen_dir: Option<PathBuf>,
    /// The folder of the platform's cache, when it has one.
    default_dir: Option<PathBuf>,
    policy: DownloadPolicy,
    offline: bool,
    /// The app allowed the download ([`ModelStore::allow`]) and nobody has withdrawn it.
    consent: bool,
}

/// What the worker thread is doing.
#[derive(Debug, Clone, Default)]
struct Live {
    running: bool,
    /// [`ModelStore::allow`] came while a stopped download was still ending: it goes on, or begins again.
    restart: bool,
    /// Why the last attempt failed.
    failure: Option<String>,
}

/// Why an attempt ended without the model.
enum Stop {
    Cancelled,
    Failed(String),
}

struct Shared {
    source: &'static ModelSource,
    fetch: Arc<dyn ModelFetch>,
    config: Mutex<Config>,
    live: Mutex<Live>,
    /// Bytes on disk and the size, for [`ModelStore::state`] to read without a lock.
    done: AtomicU64,
    total: AtomicU64,
    cancel: AtomicBool,
    /// Called once the file is in place, from the worker's thread.
    ready: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

/// The model of a recognizer on the disk: a handle that can be cloned and shared between threads.
#[derive(Clone)]
pub struct ModelStore {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for ModelStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelStore").field("source", &self.shared.source.file_name).finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `bytes` as megabytes with one decimal.
#[allow(clippy::cast_precision_loss)]
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// The host of `url`.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

impl ModelStore {
    /// A store for `source`, fetched through `fetch`, kept in `default_dir` (the platform's cache folder
    /// for the program: `None` where the platform has none, and the app then names one). The policy is
    /// `Ask`, and the program is online.
    #[must_use]
    pub fn new(source: &'static ModelSource, fetch: Arc<dyn ModelFetch>, default_dir: Option<PathBuf>) -> Self {
        Self {
            shared: Arc::new(Shared {
                source,
                fetch,
                config: Mutex::new(Config {
                    chosen_dir: None,
                    default_dir,
                    policy: DownloadPolicy::default(),
                    offline: false,
                    consent: false,
                }),
                live: Mutex::new(Live::default()),
                done: AtomicU64::new(0),
                total: AtomicU64::new(source.bytes),
                cancel: AtomicBool::new(false),
                ready: Mutex::new(None),
            }),
        }
    }

    /// The folder the model is kept in: the one the app named, or the platform's.
    #[must_use]
    pub fn folder(&self) -> Option<PathBuf> {
        let config = lock(&self.shared.config);
        config.chosen_dir.clone().or_else(|| config.default_dir.clone())
    }

    /// Names the folder the model is kept in (`None`: the platform's again).
    pub fn set_folder(&self, folder: Option<PathBuf>) {
        lock(&self.shared.config).chosen_dir = folder;
    }

    pub fn set_policy(&self, policy: DownloadPolicy) {
        lock(&self.shared.config).policy = policy;
    }

    #[must_use]
    pub fn policy(&self) -> DownloadPolicy {
        lock(&self.shared.config).policy
    }

    /// An offline program fetches nothing, whatever the policy and whoever allowed it.
    pub fn set_offline(&self, offline: bool) {
        lock(&self.shared.config).offline = offline;
    }

    /// What to do once the file is in place (on the worker's thread): the session takes up the model.
    pub fn set_ready_hook(&self, hook: Arc<dyn Fn() + Send + Sync>) {
        *lock(&self.shared.ready) = Some(hook);
    }

    /// The model's file, when it is there and complete.
    #[must_use]
    pub fn path(&self) -> Option<PathBuf> {
        let file = self.folder()?.join(self.shared.source.file_name);
        self.shared.complete(&file).then_some(file)
    }

    /// Where the model is, without waiting for anything.
    #[must_use]
    pub fn state(&self) -> ModelState {
        let source = self.shared.source;
        let config = lock(&self.shared.config).clone();
        let live = lock(&self.shared.live).clone();
        let folder = config.chosen_dir.clone().or_else(|| config.default_dir.clone());
        let mut state = ModelState {
            phase: ModelPhase::Missing,
            bytes_done: 0,
            bytes_total: source.bytes,
            path: None,
            message: String::new(),
            url: source.url.to_string(),
        };
        let say = |state: &mut ModelState, phase: ModelPhase, message: String| {
            state.phase = phase;
            state.message = message;
        };
        if let Some(file) = folder.as_ref().map(|dir| dir.join(source.file_name)).filter(|file| self.shared.complete(file)) {
            state.bytes_done = source.bytes;
            say(&mut state, ModelPhase::Present, format!("the speech model is in {}", file.display()));
            state.path = Some(file);
            return state;
        }
        if live.running && !self.shared.cancel.load(Ordering::SeqCst) {
            state.bytes_done = self.shared.done.load(Ordering::SeqCst);
            state.bytes_total = self.shared.total.load(Ordering::SeqCst).max(state.bytes_done);
            let message = format!(
                "the speech model is downloading: {} of {}",
                megabytes(state.bytes_done),
                megabytes(state.bytes_total)
            );
            say(&mut state, ModelPhase::Downloading, message);
            return state;
        }
        let place = folder.as_ref().map_or_else(|| "no folder".to_string(), |dir| dir.display().to_string());
        if let Some(why) = &live.failure {
            let message = format!("the speech model could not be fetched: {why} (allowing the download tries again)");
            say(&mut state, ModelPhase::Failed, message);
        } else if config.offline {
            let message = format!("the speech model is not in {place} and the program is offline: nothing is fetched");
            say(&mut state, ModelPhase::Offline, message);
        } else if folder.is_none() {
            let message = "there is no folder to keep the speech model in: name one with set_model_folder".to_string();
            say(&mut state, ModelPhase::NoFolder, message);
        } else {
            match config.policy {
                DownloadPolicy::Never => {
                    let message = format!("the speech model is not in {place} and downloads are off");
                    say(&mut state, ModelPhase::Missing, message);
                }
                DownloadPolicy::Ask if !config.consent => {
                    let message = format!(
                        "the built-in recognizer needs its speech model ({} from {}, {}): allow the download \
                         (allow_model_download) or register the files with set_builtin_files",
                        megabytes(source.bytes),
                        host_of(source.url),
                        source.licence
                    );
                    say(&mut state, ModelPhase::NeedsConsent, message);
                }
                DownloadPolicy::Ask | DownloadPolicy::Automatic => {
                    let message = format!("the speech model is not in {place}: it is fetched when it is first needed");
                    say(&mut state, ModelPhase::Missing, message);
                }
            }
        }
        state
    }

    /// The recognizer needs the model now: what the policy says happens. Present, offline, failed and
    /// `Never` do nothing; `Automatic` (and `Ask` once allowed) starts the download. Answers the state.
    pub fn request(&self) -> ModelState {
        let state = self.state();
        if state.phase == ModelPhase::Missing {
            let config = lock(&self.shared.config).clone();
            if config.policy == DownloadPolicy::Automatic || (config.policy == DownloadPolicy::Ask && config.consent) {
                self.start();
                return self.state();
            }
        }
        state
    }

    /// The app has asked its user and the user said yes (or the app decided so): fetches the model now,
    /// unless it is there, the program is offline or there is no folder. Allowing again after a failure
    /// tries again. Answers the state.
    pub fn allow(&self) -> ModelState {
        {
            let mut config = lock(&self.shared.config);
            config.consent = true;
        }
        lock(&self.shared.live).failure = None;
        let state = self.state();
        // `Never` is absolute: nothing is fetched, whoever allows it.
        if self.policy() != DownloadPolicy::Never
            && matches!(state.phase, ModelPhase::Missing | ModelPhase::NeedsConsent | ModelPhase::Downloading)
        {
            self.start();
        }
        self.state()
    }

    /// Stops a download at its next window; what came stays for the next allow to resume from. The
    /// consent is withdrawn: with `Ask` the model needs allowing again.
    pub fn cancel(&self) {
        lock(&self.shared.config).consent = false;
        self.shared.cancel.store(true, Ordering::SeqCst);
    }

    fn start(&self) {
        let mut live = lock(&self.shared.live);
        if live.running {
            // A download that was told to stop and is still ending goes on instead.
            if self.shared.cancel.swap(false, Ordering::SeqCst) {
                live.restart = true;
            }
            return;
        }
        live.running = true;
        live.failure = None;
        self.shared.cancel.store(false, Ordering::SeqCst);
        self.shared.done.store(0, Ordering::SeqCst);
        self.shared.total.store(self.shared.source.bytes, Ordering::SeqCst);
        drop(live);
        let shared = Arc::clone(&self.shared);
        let spawned = std::thread::Builder::new().name("azul-speech-model".to_string()).spawn(move || shared.run());
        if let Err(e) = spawned {
            let mut live = lock(&self.shared.live);
            live.running = false;
            live.failure = Some(format!("no thread to fetch it on: {e}"));
        }
    }
}

impl Shared {
    /// The file is the model: it has the pinned size (it only gets its name after its checksum is checked).
    fn complete(&self, file: &Path) -> bool {
        std::fs::metadata(file).is_ok_and(|m| m.is_file() && m.len() == self.source.bytes)
    }

    /// The worker: attempts until one ends the way the app wants (a stopped download that was allowed again
    /// goes on).
    fn run(&self) {
        loop {
            let result = self.attempt();
            let mut live = lock(&self.live);
            if live.restart {
                live.restart = false;
                self.cancel.store(false, Ordering::SeqCst);
                continue;
            }
            live.running = false;
            match result {
                Ok(()) => {
                    drop(live);
                    let hook = lock(&self.ready).clone();
                    if let Some(hook) = hook {
                        hook();
                    }
                }
                Err(Stop::Cancelled) => {}
                Err(Stop::Failed(why)) => live.failure = Some(why),
            }
            return;
        }
    }

    fn attempt(&self) -> Result<(), Stop> {
        let folder = {
            let config = lock(&self.config);
            config.chosen_dir.clone().or_else(|| config.default_dir.clone())
        }
        .ok_or_else(|| Stop::Failed("there is no folder to keep the model in".to_string()))?;
        let file = folder.join(self.source.file_name);
        if self.complete(&file) {
            return Ok(());
        }
        std::fs::create_dir_all(&folder).map_err(|e| Stop::Failed(format!("{}: {e}", folder.display())))?;
        let partial = folder.join(format!("{}.partial", self.source.file_name));
        self.fetch
            .download(
                self.source,
                &partial,
                &|done, total| {
                    self.done.store(done, Ordering::SeqCst);
                    if let Some(total) = total {
                        self.total.store(total, Ordering::SeqCst);
                    }
                },
                &|| self.cancel.load(Ordering::SeqCst),
            )
            .map_err(|e| match e {
                FetchError::Cancelled => Stop::Cancelled,
                FetchError::Refused(why) | FetchError::Failed(why) => Stop::Failed(why),
            })?;
        // What came is the model only when its size and its checksum are the pinned ones.
        let came = std::fs::metadata(&partial).map(|m| m.len()).map_err(|e| Stop::Failed(format!("{}: {e}", partial.display())))?;
        if came != self.source.bytes {
            drop(std::fs::remove_file(&partial));
            return Err(Stop::Failed(format!(
                "the file that came is {came} bytes, the model is {} (it was removed)",
                self.source.bytes
            )));
        }
        let checksum = self.fetch.sha256_hex(&partial).map_err(Stop::Failed)?;
        if !checksum.eq_ignore_ascii_case(self.source.sha256) {
            drop(std::fs::remove_file(&partial));
            return Err(Stop::Failed(format!(
                "the file that came has the checksum {checksum}, not the model's {} (it was removed)",
                self.source.sha256
            )));
        }
        std::fs::rename(&partial, &file).map_err(|e| Stop::Failed(format!("{}: {e}", file.display())))
    }
}

#[cfg(test)]
#[path = "model_test.rs"]
pub(crate) mod model_test;
