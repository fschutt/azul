//! The rules of the model store, over a transport that is a few lines of test code: what each policy does,
//! what consent and offline mean, what a stopped download leaves and what resumes it, and what a file that
//! is not the pinned one does. No network, no real model.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use super::{DownloadPolicy, FetchError, ModelFetch, ModelPhase, ModelSource, ModelStore, WHISTLE};

pub(crate) const GOOD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A model of 3000 bytes, in windows of 1000.
pub(crate) static TEST_MODEL: ModelSource = ModelSource {
    file_name: "test-0123456789ab.cact",
    url: "https://models.example.org/path/test.cact",
    sha256: GOOD,
    bytes: 3000,
    licence: "Apache-2.0 by its card",
};

pub(crate) fn body() -> Vec<u8> {
    (0..3000usize).map(|i| (i % 251) as u8).collect()
}

/// A server of `body` that a test can hold at a window, make fail, or make lie about the checksum.
pub(crate) struct Fake {
    body: Vec<u8>,
    window: usize,
    pub(crate) checksum: Mutex<String>,
    calls: AtomicUsize,
    /// The offset each download began at.
    pub(crate) starts: Mutex<Vec<u64>>,
    /// The offset at which the download waits until [`Fake::release`], if any.
    hold: (Mutex<Option<usize>>, Condvar),
    pub(crate) fail_next: Mutex<Option<String>>,
    /// How many downloads ended because they were told to stop.
    cancelled: AtomicUsize,
}

impl Fake {
    pub(crate) fn new(body: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            body,
            window: 1000,
            checksum: Mutex::new(GOOD.to_string()),
            calls: AtomicUsize::new(0),
            starts: Mutex::new(Vec::new()),
            hold: (Mutex::new(None), Condvar::new()),
            fail_next: Mutex::new(None),
            cancelled: AtomicUsize::new(0),
        })
    }

    pub(crate) fn hold_at(&self, offset: usize) {
        *self.hold.0.lock().unwrap() = Some(offset);
    }

    pub(crate) fn release(&self) {
        *self.hold.0.lock().unwrap() = None;
        self.hold.1.notify_all();
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ModelFetch for Fake {
    fn download(
        &self,
        _source: &ModelSource,
        partial: &Path,
        progress: &dyn Fn(u64, Option<u64>),
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError> {
        use std::io::Write as _;
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(why) = self.fail_next.lock().unwrap().take() {
            return Err(FetchError::Failed(why));
        }
        let mut have = std::fs::metadata(partial).map_or(0, |m| m.len() as usize);
        self.starts.lock().unwrap().push(have as u64);
        let total = self.body.len() as u64;
        progress(have as u64, Some(total));
        while have < self.body.len() {
            {
                let mut held = self.hold.0.lock().unwrap();
                let began = Instant::now();
                while *held == Some(have) && began.elapsed() < Duration::from_secs(10) {
                    held = self.hold.1.wait_timeout(held, Duration::from_millis(50)).unwrap().0;
                }
            }
            if cancel() {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
                return Err(FetchError::Cancelled);
            }
            let end = (have + self.window).min(self.body.len());
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(partial).map_err(|e| FetchError::Failed(e.to_string()))?;
            file.write_all(&self.body[have..end]).map_err(|e| FetchError::Failed(e.to_string()))?;
            have = end;
            progress(have as u64, Some(total));
        }
        Ok(())
    }

    fn sha256_hex(&self, _path: &Path) -> Result<String, String> {
        Ok(self.checksum.lock().unwrap().clone())
    }
}

pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-speech-model-{}-{name}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    dir
}

fn store(fake: &Arc<Fake>, dir: &Path) -> ModelStore {
    ModelStore::new(&TEST_MODEL, Arc::clone(fake) as Arc<dyn ModelFetch>, Some(dir.to_path_buf()))
}

pub(crate) fn wait_for(store: &ModelStore, what: &str, done: impl Fn(&super::ModelState) -> bool) -> super::ModelState {
    let began = Instant::now();
    loop {
        let state = store.state();
        if done(&state) {
            return state;
        }
        assert!(began.elapsed() < Duration::from_secs(10), "waited for {what}: {state:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn the_model_file(dir: &Path) -> PathBuf {
    dir.join(TEST_MODEL.file_name)
}

#[test]
fn a_model_that_is_there_is_present_and_nothing_is_fetched() {
    let dir = scratch("present");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(the_model_file(&dir), body()).unwrap();
    let fake = Fake::new(body());
    let store = store(&fake, &dir);
    store.set_policy(DownloadPolicy::Automatic);
    let state = store.request();
    assert_eq!(state.phase, ModelPhase::Present);
    assert_eq!(state.path.as_deref(), Some(the_model_file(&dir).as_path()));
    assert_eq!(store.allow().phase, ModelPhase::Present);
    assert_eq!(fake.calls(), 0, "a model that is there is not fetched again");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_file_of_the_wrong_size_is_not_the_model() {
    let dir = scratch("short");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(the_model_file(&dir), &body()[..2999]).unwrap();
    let store = store(&Fake::new(body()), &dir);
    assert_eq!(store.state().phase, ModelPhase::NeedsConsent);
    assert!(store.path().is_none());
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn with_ask_nothing_is_fetched_until_the_app_allows_it() {
    let dir = scratch("ask");
    let fake = Fake::new(body());
    let store = store(&fake, &dir);
    let ready = Arc::new(AtomicUsize::new(0));
    let told = Arc::clone(&ready);
    store.set_ready_hook(Arc::new(move || {
        told.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(store.policy(), DownloadPolicy::Ask, "the default asks");
    // The recognizer needs it, again and again: nothing happens, the state says what to ask.
    for _ in 0..3 {
        let state = store.request();
        assert_eq!(state.phase, ModelPhase::NeedsConsent);
        assert!(state.message.contains("models.example.org") && state.message.contains("Apache-2.0"), "{}", state.message);
        assert_eq!(state.bytes_total, 3000);
        assert_eq!(state.url, TEST_MODEL.url);
    }
    assert_eq!(fake.calls(), 0);
    // The user said yes.
    store.allow();
    let state = wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    assert_eq!(std::fs::read(state.path.expect("a path")).unwrap(), body());
    assert_eq!(fake.calls(), 1);
    assert!(!dir.join(format!("{}.partial", TEST_MODEL.file_name)).exists(), "the partial file became the model");
    // The hook runs on the worker's thread after the rename; give it a moment.
    let began = Instant::now();
    while ready.load(Ordering::SeqCst) == 0 && began.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(ready.load(Ordering::SeqCst), 1, "the session is told once");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn with_automatic_the_first_need_starts_the_download() {
    let dir = scratch("automatic");
    let fake = Fake::new(body());
    let store = store(&fake, &dir);
    store.set_policy(DownloadPolicy::Automatic);
    assert_eq!(store.state().phase, ModelPhase::Missing, "before the first need it is only missing");
    store.request();
    wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    assert_eq!(fake.calls(), 1);
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn never_fetches_nothing_even_when_allowed() {
    let dir = scratch("never");
    let fake = Fake::new(body());
    let store = store(&fake, &dir);
    store.set_policy(DownloadPolicy::Never);
    assert_eq!(store.request().phase, ModelPhase::Missing);
    let state = store.allow();
    assert_eq!(state.phase, ModelPhase::Missing);
    assert!(state.message.contains("downloads are off"), "{}", state.message);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.calls(), 0);
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn an_offline_program_fetches_nothing_whatever_the_policy_and_the_consent() {
    let dir = scratch("offline");
    let fake = Fake::new(body());
    let store = store(&fake, &dir);
    store.set_offline(true);
    for policy in [DownloadPolicy::Ask, DownloadPolicy::Automatic, DownloadPolicy::Never] {
        store.set_policy(policy);
        assert_eq!(store.request().phase, ModelPhase::Offline, "{policy:?}");
        let state = store.allow();
        assert_eq!(state.phase, ModelPhase::Offline, "{policy:?}");
        assert!(state.message.contains("offline"), "{}", state.message);
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.calls(), 0);
    // Online again, and it is consented: the next need fetches.
    store.set_offline(false);
    store.set_policy(DownloadPolicy::Ask);
    store.request();
    wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn the_progress_is_read_while_it_downloads_without_waiting() {
    let dir = scratch("progress");
    let fake = Fake::new(body());
    fake.hold_at(1000);
    let store = store(&fake, &dir);
    store.allow();
    let state = wait_for(&store, "the first window", |s| s.phase == ModelPhase::Downloading && s.bytes_done == 1000);
    assert_eq!(state.bytes_total, 3000);
    assert!(state.message.contains("downloading"), "{}", state.message);
    assert!(state.path.is_none());
    fake.release();
    wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_cancelled_download_keeps_what_came_needs_consent_again_and_resumes_where_it_stopped() {
    let dir = scratch("cancel");
    let fake = Fake::new(body());
    fake.hold_at(1000);
    let store = store(&fake, &dir);
    store.allow();
    wait_for(&store, "the first window", |s| s.bytes_done == 1000);
    store.cancel();
    // At once the state is no longer a download, and the consent is withdrawn.
    assert_eq!(store.state().phase, ModelPhase::NeedsConsent);
    fake.release();
    // The transport has seen the stop, and what came stays on disk.
    let partial = dir.join(format!("{}.partial", TEST_MODEL.file_name));
    let began = Instant::now();
    while fake.cancelled.load(Ordering::SeqCst) == 0 {
        assert!(began.elapsed() < Duration::from_secs(10), "the download never saw the stop");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::fs::metadata(&partial).expect("the partial file stays").len(), 1000);
    assert_eq!(store.state().phase, ModelPhase::NeedsConsent);
    assert!(!the_model_file(&dir).exists(), "half a model is never the model");
    // Allowed again: it goes on from the 1000 bytes.
    store.allow();
    wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    assert_eq!(*fake.starts.lock().unwrap(), [0, 1000], "the second download began where the first stopped");
    assert_eq!(std::fs::read(the_model_file(&dir)).unwrap(), body());
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn allowing_again_while_a_stopped_download_ends_makes_it_go_on() {
    let dir = scratch("restart");
    let fake = Fake::new(body());
    fake.hold_at(1000);
    let store = store(&fake, &dir);
    store.allow();
    wait_for(&store, "the first window", |s| s.bytes_done == 1000);
    store.cancel();
    store.allow();
    fake.release();
    wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    assert_eq!(fake.calls(), 1, "the one download went on, none began twice");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_failure_is_a_state_with_its_reason_and_allowing_again_tries_again() {
    let dir = scratch("failure");
    let fake = Fake::new(body());
    *fake.fail_next.lock().unwrap() = Some("connection reset by peer".to_string());
    let store = store(&fake, &dir);
    store.set_policy(DownloadPolicy::Automatic);
    store.request();
    let state = wait_for(&store, "the failure", |s| s.phase == ModelPhase::Failed);
    assert!(state.message.contains("connection reset by peer"), "{}", state.message);
    // The recognizer needing it again does not retry by itself: no storm against a server that is down.
    store.request();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.calls(), 1);
    // The app allows again (the user pressed "try again").
    store.allow();
    wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    assert_eq!(fake.calls(), 2);
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_file_with_another_checksum_is_removed_and_is_not_the_model() {
    let dir = scratch("checksum");
    let fake = Fake::new(body());
    *fake.checksum.lock().unwrap() = "ff".repeat(32);
    let store = store(&fake, &dir);
    store.allow();
    let state = wait_for(&store, "the failure", |s| s.phase == ModelPhase::Failed);
    assert!(state.message.contains("checksum") && state.message.contains("removed"), "{}", state.message);
    assert!(!the_model_file(&dir).exists());
    assert!(!dir.join(format!("{}.partial", TEST_MODEL.file_name)).exists(), "a file that is not the model is not kept for a resume");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_file_of_another_size_is_removed_and_is_not_the_model() {
    let dir = scratch("size");
    let fake = Fake::new(body()[..2999].to_vec());
    let store = store(&fake, &dir);
    store.allow();
    let state = wait_for(&store, "the failure", |s| s.phase == ModelPhase::Failed);
    assert!(state.message.contains("2999 bytes"), "{}", state.message);
    assert!(!the_model_file(&dir).exists());
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn no_folder_is_a_state_and_naming_one_lets_the_consent_given_before_count() {
    let dir = scratch("nofolder");
    let fake = Fake::new(body());
    let store = ModelStore::new(&TEST_MODEL, Arc::clone(&fake) as Arc<dyn ModelFetch>, None);
    assert_eq!(store.state().phase, ModelPhase::NoFolder);
    assert_eq!(store.allow().phase, ModelPhase::NoFolder);
    assert_eq!(fake.calls(), 0);
    store.set_folder(Some(dir.clone()));
    store.request();
    wait_for(&store, "the model", |s| s.phase == ModelPhase::Present);
    assert_eq!(store.folder().as_deref(), Some(dir.as_path()));
    store.set_folder(None);
    assert_eq!(store.state().phase, ModelPhase::NoFolder, "no folder of the platform's either");
    drop(std::fs::remove_dir_all(&dir));
}

/// The pin of the model is in two places, the program and the script that fetches the files for a
/// build of one's own; they must say the same.
#[test]
fn the_pinned_model_is_the_one_the_fetch_script_pins() {
    let script = include_str!("../../../../../../scripts/fetch_whistle.sh");
    assert!(script.contains(&format!("MODEL_SHA256=\"{}\"", WHISTLE.sha256)), "the checksum");
    assert!(script.contains(&format!("MODEL_BYTES={}", WHISTLE.bytes)), "the size");
    let revision = WHISTLE.url.split("/resolve/").nth(1).and_then(|rest| rest.split('/').next()).expect("a revision in the url");
    assert!(script.contains(&format!("MODEL_REVISION=\"{revision}\"")), "the revision");
    assert!(WHISTLE.url.ends_with("/whistle.cact"));
    assert_eq!(WHISTLE.file_name, format!("whistle-{}.cact", &WHISTLE.sha256[..12]));
}
