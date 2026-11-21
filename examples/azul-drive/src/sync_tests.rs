//! AzDrive's sync: the window's side as plain data (`sync_view`: the drive's status line, a
//! file's state icon, the cloud-only files as rows of the synced folder, where a path lies in
//! the pairing, the settings) and the worker's side (`sync_jobs`: a pass, opening a cloud-only
//! file, pinning and freeing up space, against a drive in memory).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use azcloud_kit::sync::{
    session::{AutoDownload, FileRecord, FileState, SyncSetup, SyncStates},
    HeldConflict,
};
use azul::sensor::{NetworkKind, NetworkState};
use azul_storage::{
    testing::TempDir, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo,
    Precondition,
};

use crate::{
    jobs::Outcome,
    model::Settings,
    sync_jobs::{self, PassProgress, SyncChange, SyncJob, SyncOutcome, SyncWork},
    sync_store::SyncStore,
    sync_view::{self, Running},
};

const MB: u64 = 1024 * 1024;

// ==== The window's side ====

fn setup() -> SyncSetup {
    SyncSetup::new("d_photos", "", Path::new("/home/me/AzDrive/Photos"))
}

/// The key a status line says (no network holding anything back).
fn status_key(
    paired: &SyncSetup,
    states: &SyncStates,
    running: Option<&Running>,
    azlin: bool,
    payment_due: bool,
) -> String {
    sync_view::status_text(paired, states, running, azlin, payment_due, false).key
}

#[test]
fn the_status_line_says_up_to_date_syncing_paused_read_only_and_conflicts() {
    use azul_appkit::l10n::Arg;

    let mut states = SyncStates::default();
    let mut paired = setup();
    assert_eq!(
        status_key(&paired, &states, None, true, false),
        "azdrive-sync-status-never"
    );
    states.last_pass = Some(1);
    assert_eq!(
        status_key(&paired, &states, None, true, false),
        "azdrive-sync-status-up-to-date"
    );
    let running = Running {
        cancel: Arc::new(AtomicBool::new(false)),
        progress: PassProgress {
            files_done: 3,
            files_total: 15,
            bytes_done: 0,
            bytes_total: 340 * MB,
            moving: None,
        },
    };
    let syncing = sync_view::status_text(&paired, &states, Some(&running), true, false, false);
    assert_eq!(syncing.key, "azdrive-sync-status-syncing-files");
    assert_eq!(syncing.get("count"), Some(&Arg::Int(12)));
    assert!(
        syncing.get("size").is_some_and(|size| size.to_string().contains("340")),
        "{syncing:?}"
    );
    states.files.insert(
        String::from("a.jpg"),
        FileRecord {
            conflict: Some(HeldConflict {
                key: String::from("a.jpg"),
                here: String::new(),
                there: String::new(),
                there_size: 1,
                there_device: String::from("laptop"),
                there_exec: false,
            }),
            ..FileRecord::default()
        },
    );
    let waiting = sync_view::status_text(&paired, &states, None, true, false, false);
    assert_eq!(waiting.key, "azdrive-sync-status-conflicts");
    assert_eq!(waiting.get("count"), Some(&Arg::Int(1)));
    // An Azlin drive is read-only when its token server says so (its drive status), not
    // because a write was refused; another drive when it refuses writes.
    states.read_only = true;
    assert_ne!(
        status_key(&paired, &states, None, true, false),
        "azdrive-sync-status-payment-due"
    );
    assert_eq!(
        status_key(&paired, &states, None, true, true),
        "azdrive-sync-status-payment-due"
    );
    assert_eq!(
        status_key(&paired, &states, None, false, false),
        "azdrive-sync-status-read-only"
    );
    paired.paused = true;
    assert_eq!(
        status_key(&paired, &states, None, true, false),
        "azdrive-sync-status-paused"
    );
}

/// The store the window and the search share: a synced folder's file through a drive on this
/// computer and through an encrypted drive's own listing; its plain local copy only while it
/// is on this device.
#[test]
fn the_sync_store_finds_a_file_through_the_folder_and_says_where_its_copy_is() {
    let folder = TempDir::new("azdrive-sync-store");
    let home = folder.path().parent().unwrap().to_path_buf();
    let name = folder.path().file_name().unwrap().to_string_lossy().into_owned();
    fs::write(folder.path().join("here.txt"), b"here").unwrap();
    let store = SyncStore::default();
    let paired = SyncSetup::new("d_photos", "Photos/", folder.path());
    store.set_pairs(
        &[paired],
        &|_drive: &str| false,
        vec![(String::from("home"), home.clone())],
    );
    assert!(store.any());
    let mut states = SyncStates::default();
    states.files.insert(String::from("here.txt"), FileRecord::default());
    states.files.insert(
        String::from("cloud.txt"),
        FileRecord {
            cloud_only: true,
            ..FileRecord::default()
        },
    );
    store.set_states("d_photos", states);
    let key = |file: &str| format!("{name}/{file}");
    assert_eq!(
        store.locate("home", &key("here.txt")),
        Some((String::from("d_photos"), String::from("here.txt")))
    );
    assert_eq!(store.file_state("home", &key("here.txt")), Some(FileState::OnDevice));
    assert_eq!(store.file_state("home", &key("cloud.txt")), Some(FileState::CloudOnly));
    assert_eq!(store.file_state("home", "elsewhere.txt"), None);
    assert_eq!(
        store.local_copy("home", &key("here.txt")),
        Some(folder.path().join("here.txt"))
    );
    assert_eq!(store.local_copy("home", &key("cloud.txt")), None, "no bytes here");
    // A plain drive's own listing shows the sync's files too (by the index's names).
    assert_eq!(store.file_state("d_photos", "Photos/here.txt"), Some(FileState::OnDevice));
    assert_eq!(store.file_state("d_photos", "Other/here.txt"), None, "outside the pairing");
    // An encrypted drive's own listing names its files.
    store.set_pairs(
        &[SyncSetup::new("d_photos", "Photos/", folder.path())],
        &|drive: &str| drive == "d_photos",
        vec![(String::from("home"), home)],
    );
    assert_eq!(store.file_state("d_photos", "Photos/here.txt"), Some(FileState::OnDevice));
    assert_eq!(
        store.local_copy("d_photos", "Photos/here.txt"),
        Some(folder.path().join("here.txt"))
    );
    store.set_moving("d_photos", Some((String::from("here.txt"), true)));
    assert!(matches!(
        store.file_state("d_photos", "Photos/here.txt"),
        Some(FileState::Uploading { .. })
    ));
    store.set_pairs(&[], &|_drive: &str| false, Vec::new());
    assert!(!store.any());
}

#[test]
fn each_file_state_has_an_icon_of_the_icon_set_and_a_word_for_scripts() {
    let states = [
        FileState::CloudOnly,
        FileState::Downloading { done: 0, total: 1 },
        FileState::Uploading { done: 0, total: 1 },
        FileState::OnDevice,
        FileState::OnDeviceEncrypted,
        FileState::Pinned,
        FileState::Conflict,
        FileState::Error(String::from("no answer")),
    ];
    let icons: Vec<&str> = states.iter().map(sync_view::state_icon).collect();
    assert_eq!(
        icons,
        [
            "cloud_queue",
            "cloud_download",
            "cloud_upload",
            "check_circle",
            "lock",
            "push_pin",
            "warning",
            "error"
        ]
    );
    let words: Vec<&str> = states.iter().map(sync_view::state_word).collect();
    assert_eq!(
        words,
        [
            "cloud-only",
            "downloading",
            "uploading",
            "on-device",
            "on-device-encrypted",
            "pinned",
            "conflict",
            "error"
        ]
    );
}

#[test]
fn a_synced_folder_lists_its_cloud_only_files_as_rows() {
    let mut states = SyncStates::default();
    let cloud = |size: u64| FileRecord {
        size,
        modified: 1_700_000_000,
        cloud_only: true,
        ..FileRecord::default()
    };
    states.files.insert(String::from("docs/big.pdf"), cloud(30 * MB));
    states.files.insert(String::from("docs/here.txt"), FileRecord::default());
    states.files.insert(String::from("docs/sub/deep.pdf"), cloud(1));
    states.files.insert(String::from("top.bin"), cloud(2));
    let rows = sync_view::placeholders(&states, "docs/", "AzDrive/Photos/docs/");
    assert_eq!(rows.len(), 1, "only the cloud-only files directly in it: {rows:?}");
    assert_eq!(rows[0].key, "AzDrive/Photos/docs/big.pdf");
    assert_eq!(rows[0].name, "big.pdf");
    assert_eq!(rows[0].size, Some(30 * MB));
    assert_eq!(rows[0].modified, Some(1_700_000_000));
    assert!(!rows[0].is_folder && rows[0].known);
    let top = sync_view::placeholders(&states, "", "AzDrive/Photos/");
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].key, "AzDrive/Photos/top.bin");
}

#[test]
fn a_path_inside_the_synced_folder_has_its_key_under_it() {
    let folder = Path::new("/home/me/AzDrive/Photos");
    assert_eq!(
        sync_view::key_under(folder, Path::new("/home/me/AzDrive/Photos/2026/a.jpg"), false),
        Some(String::from("2026/a.jpg"))
    );
    assert_eq!(
        sync_view::key_under(folder, Path::new("/home/me/AzDrive/Photos/2026"), true),
        Some(String::from("2026/"))
    );
    assert_eq!(
        sync_view::key_under(folder, folder, true),
        Some(String::new()),
        "the folder itself"
    );
    assert_eq!(
        sync_view::key_under(folder, Path::new("/home/me/AzDrive/Photos2/a.jpg"), false),
        None
    );
    assert_eq!(sync_view::key_under(folder, Path::new("/home/me"), true), None);
}

#[test]
fn a_new_pairing_is_a_folder_under_home_named_after_the_drive() {
    assert_eq!(
        sync_view::default_folder(Path::new("/home/me"), "Photos 2026"),
        PathBuf::from("/home/me/AzDrive/Photos 2026")
    );
    assert_eq!(
        sync_view::default_folder(Path::new("/home/me"), "a/b:c"),
        PathBuf::from("/home/me/AzDrive/a-b-c")
    );
}

/// X-DELTA-AZDRIVE-FILES-32 (c): a drive called `..` gave `~/AzDrive/..` - Home itself - for the
/// pairing; the old rule replaced three characters. The one name policy makes the folder name.
#[test]
fn x_delta_azdrive_files_32_no_drive_name_makes_the_pairing_folder_leave_azdrive() {
    let home = Path::new("/home/me");
    let inside = |name: &str| {
        let folder = sync_view::default_folder(home, name);
        assert_eq!(
            folder.parent(),
            Some(Path::new("/home/me/AzDrive")),
            "{name:?} gave {folder:?}"
        );
        folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    for name in ["..", ".", " .. ", "../..", "...", "", "   "] {
        let leaf = inside(name);
        assert!(
            !leaf.is_empty() && leaf != ".." && leaf != ".",
            "{name:?} gave {leaf:?}"
        );
    }
    // A device name and a protected name are not taken as they are.
    assert_ne!(inside("CON"), "CON");
    assert_ne!(inside(".git"), ".git");
    assert_ne!(inside(".azlin"), ".azlin");
    assert_eq!(inside("Photos 2026"), "Photos 2026");
}

#[test]
fn the_settings_keep_the_synced_drives() {
    let mut settings = Settings::default();
    assert!(settings.synced.is_empty());
    let mut paired = setup();
    paired.auto_download = AutoDownload::Nothing;
    paired.keep_gb = Some(20);
    settings.synced.push(paired.clone());
    let read = Settings::from_json(&settings.to_json());
    assert_eq!(read.synced, vec![paired]);
}

// ==== The worker's side ====

/// A bucket in memory with conditional writes: what a sync needs of a drive.
#[derive(Default)]
struct MemDrive {
    objects: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    next: AtomicU64,
}

impl MemDrive {
    fn tag(&self) -> String {
        format!("v{}", self.next.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn not_found(key: &str) -> DriveError {
        DriveError::NotFound {
            key: key.to_string(),
        }
    }
}

impl Drive for MemDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let objects = self.objects.lock().unwrap();
        Ok(ListPage {
            folders: Vec::new(),
            objects: objects
                .iter()
                .filter(|(key, _)| key.starts_with(&request.prefix))
                .map(|(key, (bytes, tag))| ObjectInfo {
                    key: key.clone(),
                    size: bytes.len() as u64,
                    modified: Some(1_700_000_000),
                    etag: Some(tag.clone()),
                })
                .collect(),
            next: None,
        })
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let objects = self.objects.lock().unwrap();
        objects
            .get(key)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| Self::not_found(key))
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.get(key)?;
        let start = usize::try_from(range.start).unwrap();
        let end = range
            .end
            .map_or(bytes.len(), |e| usize::try_from(e).unwrap() + 1)
            .min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        let tag = self.tag();
        self.objects
            .lock()
            .unwrap()
            .insert(key.to_string(), (bytes.to_vec(), tag));
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let objects = self.objects.lock().unwrap();
        let (bytes, tag) = objects.get(key).ok_or_else(|| Self::not_found(key))?;
        Ok(ObjectInfo {
            key: key.to_string(),
            size: bytes.len() as u64,
            modified: Some(1_700_000_000),
            etag: Some(tag.clone()),
        })
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let mut objects = self.objects.lock().unwrap();
        let holds = match (condition, objects.get(key)) {
            (Precondition::Absent, None) => true,
            (Precondition::Matches(want), Some((_, tag))) => want == tag,
            _ => false,
        };
        if !holds {
            return Err(DriveError::Conflict {
                key: key.to_string(),
            });
        }
        let tag = self.tag();
        objects.insert(key.to_string(), (bytes.to_vec(), tag.clone()));
        Ok(Some(tag))
    }
}

/// One device's pairing with the drive.
struct Device {
    folder: TempDir,
    state: TempDir,
    drive: Arc<dyn Drive>,
    auto: AutoDownload,
}

impl Device {
    fn new(name: &str, drive: &Arc<dyn Drive>, auto: AutoDownload) -> Device {
        Device {
            folder: TempDir::new(&format!("azdrive-sync-{name}")),
            state: TempDir::new(&format!("azdrive-sync-state-{name}")),
            drive: drive.clone(),
            auto,
        }
    }

    fn work(&self) -> SyncWork {
        let mut setup = SyncSetup::new("d_photos", "Photos/", self.folder.path());
        setup.auto_download = self.auto;
        SyncWork::new(setup, self.state.path().to_path_buf(), self.drive.clone())
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        fs::write(self.folder.path().join(name), bytes).unwrap();
    }

    /// A pass; what it told while it ran and its answer.
    fn pass(&self) -> (Vec<Outcome>, Outcome) {
        let mut told = Vec::new();
        let answer = sync_jobs::run(
            SyncJob::Pass {
                work: self.work(),
                cancel: Arc::new(AtomicBool::new(false)),
            },
            &mut |outcome| told.push(outcome),
        );
        (told, answer)
    }
}

fn states_of(outcome: &Outcome) -> &SyncStates {
    match outcome {
        Outcome::Sync(
            SyncOutcome::Passed { states, .. }
            | SyncOutcome::Opened { states, .. }
            | SyncOutcome::Changed { states, .. },
        ) => states,
        _ => panic!("not a sync answer"),
    }
}

#[test]
fn a_pass_on_a_worker_syncs_the_folder_and_answers_its_states() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("pass-a", &drive, AutoDownload::Everything);
    a.write("a.jpg", b"picture a");
    a.write("b.jpg", b"picture b");
    let (told, answer) = a.pass();
    assert!(
        told.iter()
            .any(|o| matches!(o, Outcome::Sync(SyncOutcome::Progress { .. }))),
        "progress while it runs"
    );
    match &answer {
        Outcome::Sync(SyncOutcome::Passed {
            drive_id, result, ..
        }) => {
            assert_eq!(drive_id, "d_photos");
            let done = result.as_ref().expect("the pass ran");
            assert_eq!(done.up, 2);
            assert_eq!(done.down, 0);
        }
        _ => panic!("not a pass's answer"),
    }
    assert_eq!(states_of(&answer).state_of("a.jpg"), Some(FileState::OnDevice));
    assert!(
        drive
            .list(&ListRequest::recursive("Photos/"))
            .unwrap()
            .objects
            .iter()
            .any(|o| o.key.ends_with("index.json")),
        "a plain drive keeps the sync's index under the folder"
    );
}

#[test]
fn opening_a_cloud_only_file_brings_it_down_first() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("open-a", &drive, AutoDownload::Everything);
    let b = Device::new("open-b", &drive, AutoDownload::Nothing);
    a.write("report.pdf", b"%PDF-1.7 the report");
    a.pass();
    let (_, answer) = b.pass();
    assert_eq!(
        states_of(&answer).state_of("report.pdf"),
        Some(FileState::CloudOnly)
    );
    let opened = sync_jobs::run(
        SyncJob::Open {
            work: b.work(),
            key: String::from("report.pdf"),
        },
        &mut |_| {},
    );
    match &opened {
        Outcome::Sync(SyncOutcome::Opened { key, result, .. }) => {
            assert_eq!(key, "report.pdf");
            let path = result.as_ref().expect("opened");
            assert_eq!(fs::read(path).unwrap(), b"%PDF-1.7 the report");
        }
        _ => panic!("not an open's answer"),
    }
    assert_eq!(
        states_of(&opened).state_of("report.pdf"),
        Some(FileState::OnDevice)
    );
}

#[test]
fn pinning_and_freeing_up_space_answer_the_new_states() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("pin-a", &drive, AutoDownload::Everything);
    a.write("keep.txt", b"keep");
    a.write("free.txt", b"free");
    a.pass();
    let pinned = sync_jobs::run(
        SyncJob::Pin {
            work: a.work(),
            keys: vec![String::from("keep.txt")],
            on: true,
        },
        &mut |_| {},
    );
    assert!(matches!(
        &pinned,
        Outcome::Sync(SyncOutcome::Changed {
            done: SyncChange::Pinned(true),
            result: Ok(_),
            ..
        })
    ));
    assert_eq!(states_of(&pinned).state_of("keep.txt"), Some(FileState::Pinned));
    let freed = sync_jobs::run(
        SyncJob::FreeUp {
            work: a.work(),
            keys: vec![String::from("")],
        },
        &mut |_| {},
    );
    assert!(matches!(
        &freed,
        Outcome::Sync(SyncOutcome::Changed {
            done: SyncChange::Freed,
            result: Ok(_),
            ..
        })
    ));
    let states = states_of(&freed);
    assert_eq!(states.state_of("free.txt"), Some(FileState::CloudOnly));
    assert_eq!(states.state_of("keep.txt"), Some(FileState::Pinned));
    assert!(!a.folder.path().join("free.txt").exists());
    assert!(a.folder.path().join("keep.txt").exists());
}

/// The search sees the sync store through its seam (`SyncLookup`): no Status column while
/// nothing syncs; a file's state in the search's words; the plain local copy of a file on this
/// device, none of one in the cloud only.
#[test]
fn the_search_asks_the_sync_store_through_its_seam() {
    use crate::sync_lookup::{SyncLookup, SyncState};

    let folder = TempDir::new("azdrive-sync-seam");
    let home = folder.path().parent().unwrap().to_path_buf();
    let name = folder.path().file_name().unwrap().to_string_lossy().into_owned();
    fs::write(folder.path().join("a.txt"), b"a").unwrap();
    let store = SyncStore::default();
    let seam: &dyn SyncLookup = &store;
    assert!(!seam.syncs(), "nothing syncs yet");
    store.set_pairs(
        &[SyncSetup::new("d_photos", "", folder.path())],
        &|_drive: &str| false,
        vec![(String::from("home"), home)],
    );
    assert!(seam.syncs());
    let mut states = SyncStates::default();
    states.files.insert(String::from("a.txt"), FileRecord::default());
    states.files.insert(
        String::from("b.txt"),
        FileRecord {
            cloud_only: true,
            ..FileRecord::default()
        },
    );
    states.files.insert(
        String::from("c.txt"),
        FileRecord {
            error: Some(String::from("the drive did not answer")),
            ..FileRecord::default()
        },
    );
    store.set_states("d_photos", states);
    let key = |file: &str| format!("{name}/{file}");
    assert_eq!(seam.sync_state("home", &key("a.txt")), Some(SyncState::OnThisDevice));
    assert_eq!(seam.sync_state("home", &key("b.txt")), Some(SyncState::OnlineOnly));
    assert_eq!(seam.sync_state("home", &key("c.txt")), Some(SyncState::Problem));
    assert_eq!(seam.sync_state("home", "elsewhere.txt"), None);
    assert_eq!(
        seam.local_copy("home", &key("a.txt")),
        Some(folder.path().join("a.txt"))
    );
    assert_eq!(seam.local_copy("home", &key("b.txt")), None);
}

/// §13.7's overlays from a drive's search index: a magnifier on a file the index read as it is
/// now, a slashed one on a file it never reads (no text, too big), nothing on one not read yet;
/// a row without its size and date (not stat'ed yet) is not asked.
#[test]
fn an_indexed_drives_rows_say_whether_the_index_holds_them() {
    use azul_search_index::FileIndexing;

    assert_eq!(
        sync_view::index_overlay(FileIndexing::Indexed),
        Some(("manage_search", "azdrive-index-overlay-indexed"))
    );
    assert_eq!(
        sync_view::index_overlay(FileIndexing::NotIndexable),
        Some(("search_off", "azdrive-index-overlay-not-indexable"))
    );
    assert_eq!(sync_view::index_overlay(FileIndexing::Unread), None);
    let row = |known: bool| crate::browse::Entry {
        key: String::from("docs/a.txt"),
        name: String::from("a.txt"),
        is_folder: false,
        size: known.then_some(3),
        modified: known.then_some(1_700_000_000),
        etag: None,
        known,
    };
    let file = sync_view::index_entry(&row(true)).expect("a file with its size and date");
    assert_eq!(
        (file.path.as_str(), file.size, file.modified),
        ("docs/a.txt", 3, Some(1_700_000_000))
    );
    assert!(sync_view::index_entry(&row(false)).is_none());
}

/// A plain synced drive's own listing shows the sync index's files and folders - its bucket
/// holds only the sync's bookkeeping (`.azlin/`) under them.
#[test]
fn a_plain_synced_drive_lists_the_files_of_its_sync_index() {
    let mut states = SyncStates::default();
    let file = |size: u64, cloud_only: bool| FileRecord {
        size,
        modified: 1_700_000_000,
        cloud_only,
        ..FileRecord::default()
    };
    states.files.insert(String::from("a.txt"), file(1, false));
    states.files.insert(String::from("docs/b.txt"), file(2, true));
    states.files.insert(String::from("docs/deep/c.txt"), file(3, false));
    states.files.insert(String::from("pics/d.png"), file(4, false));
    let top = sync_view::index_rows(&states, "", "Photos/");
    let keys: Vec<(&str, bool)> = top.iter().map(|e| (e.key.as_str(), e.is_folder)).collect();
    assert_eq!(
        keys,
        [("Photos/a.txt", false), ("Photos/docs/", true), ("Photos/pics/", true)]
    );
    assert_eq!(top[0].size, Some(1));
    assert_eq!(top[1].name, "docs");
    let docs = sync_view::index_rows(&states, "docs/", "Photos/docs/");
    let keys: Vec<&str> = docs.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["Photos/docs/b.txt", "Photos/docs/deep/"]);
}

/// A cloud-only row previews as a sentence (its bytes are not here); a file shown from a plain
/// drive's sync index previews from the synced folder, so its own listing says where.
#[test]
fn a_cloud_only_row_previews_as_a_sentence() {
    assert_eq!(
        sync_view::preview_note(&FileState::CloudOnly, false),
        Some("azdrive-preview-cloud-only")
    );
    assert_eq!(
        sync_view::preview_note(&FileState::OnDevice, true),
        Some("azdrive-preview-synced")
    );
    assert_eq!(sync_view::preview_note(&FileState::OnDevice, false), None);
    assert_eq!(sync_view::preview_note(&FileState::Pinned, false), None);
}

#[test]
fn deleting_and_touching_through_a_job_answer_the_new_states() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("delete-a", &drive, AutoDownload::Everything);
    a.write("gone.txt", b"to be deleted");
    a.write("kept.txt", b"to be kept");
    a.pass();
    let touched = sync_jobs::run(
        SyncJob::Touch {
            work: a.work(),
            key: String::from("kept.txt"),
        },
        &mut |_| {},
    );
    assert!(matches!(
        &touched,
        Outcome::Sync(SyncOutcome::Changed {
            done: SyncChange::Touched,
            result: Ok(_),
            ..
        })
    ));
    let deleted = sync_jobs::run(
        SyncJob::Delete {
            work: a.work(),
            keys: vec![String::from("gone.txt")],
        },
        &mut |_| {},
    );
    assert!(matches!(
        &deleted,
        Outcome::Sync(SyncOutcome::Changed {
            done: SyncChange::Deleted,
            result: Ok(_),
            ..
        })
    ));
    assert_eq!(states_of(&deleted).state_of("gone.txt"), None);
    assert!(!a.folder.path().join("gone.txt").exists());
    let (_, answer) = a.pass();
    match &answer {
        Outcome::Sync(SyncOutcome::Passed { result: Ok(done), .. }) => {
            assert_eq!(done.deleted, 1, "deleted on the drive with the next pass");
        }
        _ => panic!("not a pass's answer"),
    }
}

/// The guards' pauses and the newer format say themselves on the status line; the mass-delete
/// question is AzDrive's own words, without the command line's switch.
#[test]
fn the_guards_pauses_say_themselves_and_ask_in_azdrives_words() {
    use azcloud_kit::sync::{
        guard::{Pause, PauseReason},
        MassDelete,
    };

    let paired = setup();
    let mut states = SyncStates::default();
    states.last_pass = Some(1);
    states.burst = Some(Pause {
        reason: PauseReason::Encryption,
        since: 1,
        changes: 12,
        files: vec![String::from("docs/a.txt")],
    });
    let text = sync_view::status_text(&paired, &states, None, false, false, false);
    assert_eq!(text.key, "azdrive-sync-status-encrypted", "{text:?}");
    states.burst = None;
    let asked = MassDelete {
        here: false,
        count: 14,
        of: 14,
        keys: vec![String::from("keep/0.txt")],
    };
    states.mass_delete = Some(asked.clone());
    let text = sync_view::status_text(&paired, &states, None, false, false, false);
    assert_eq!(text.key, "azdrive-sync-status-mass-delete", "{text:?}");
    let question = sync_view::mass_delete_text(&asked);
    assert_eq!(question.key, "azdrive-sync-mass-delete-here");
    assert_eq!(question.get("count"), Some(&azul_appkit::l10n::Arg::Int(14)));
    let english = crate::l10n::EN;
    let said = english
        .lines()
        .find(|line| line.starts_with("azdrive-sync-mass-delete-here ="))
        .expect("the question in English");
    assert!(!said.contains("--allow"), "{said}");
    states.mass_delete = None;
    states.newer_format = vec![String::from("teleport")];
    let text = sync_view::status_text(&paired, &states, None, false, false, false);
    assert_eq!(text.key, "azdrive-sync-status-newer-format", "{text:?}");
}

/// AZ-U12: the mass-delete question is asked because a lot of files vanished; its main button -
/// the one Enter answers - keeps them. "Delete them on the other side too" is there, once, and is
/// never the main button, on either side.
#[test]
fn the_mass_delete_question_is_answered_keep_by_enter_never_delete() {
    for here in [false, true] {
        let choices = sync_view::mass_delete_choices(here);
        assert_eq!(choices.len(), 2);
        let primary: Vec<_> = choices.iter().filter(|c| c.primary).collect();
        assert_eq!(primary.len(), 1, "exactly one main button");
        assert!(!primary[0].deletes, "Enter must not delete {here}");
        assert_eq!(
            choices.iter().filter(|c| c.deletes).count(),
            1,
            "one answer deletes"
        );
        let delete = choices.iter().find(|c| c.deletes).unwrap();
        assert!(!delete.primary);
        let words = if here {
            "azdrive-sync-mass-delete-here-too"
        } else {
            "azdrive-sync-mass-delete-there-too"
        };
        assert_eq!(delete.text, words);
        for (lang, ftl) in [("en", crate::l10n::EN), ("de", crate::l10n::DE)] {
            for choice in &choices {
                assert!(
                    ftl.lines()
                        .any(|l| l.starts_with(&format!("{} =", choice.text))),
                    "{lang}.ftl lacks {}",
                    choice.text
                );
            }
        }
    }
}

/// A rename in a plain synced drive's own listing goes through the sync: the copy in the synced
/// folder moves, and the next pass moves it on the drive.
#[test]
fn a_rename_through_a_job_moves_the_synced_copy() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("rename-a", &drive, AutoDownload::Everything);
    a.write("old.txt", b"text");
    a.pass();
    let renamed = sync_jobs::run(
        SyncJob::Rename {
            work: a.work(),
            from: String::from("old.txt"),
            to: String::from("new.txt"),
        },
        &mut |_| {},
    );
    assert!(matches!(
        &renamed,
        Outcome::Sync(SyncOutcome::Changed {
            done: SyncChange::Renamed,
            result: Ok(_),
            ..
        })
    ));
    assert!(a.folder.path().join("new.txt").exists());
    assert!(!a.folder.path().join("old.txt").exists());
    let fetched = sync_jobs::run(
        SyncJob::Fetch {
            work: a.work(),
            keys: vec![String::from("new.txt")],
        },
        &mut |_| {},
    );
    assert!(matches!(
        &fetched,
        Outcome::Sync(SyncOutcome::Changed {
            done: SyncChange::Fetched,
            ..
        })
    ));
}

// ==== A metered network ====

fn network(kind: NetworkKind, metered: bool, constrained: bool) -> NetworkState {
    NetworkState {
        kind,
        connected: true,
        metered,
        constrained,
        hotspot: false,
    }
}

/// A computer on battery at `percent`, on `network` (azul-appkit's client health parts).
fn computer(percent: u8, network: &NetworkState) -> azul_appkit::client_health::Device {
    azul_appkit::client_health::Device {
        on_mains: false,
        battery_percent: Some(percent),
        connected: network.connected,
        metered: network.metered,
        constrained: network.constrained,
        hotspot: network.hotspot,
        ..azul_appkit::client_health::Device::DESK
    }
}

#[test]
fn a_weak_computer_holds_big_files_back_and_sync_anyway_lifts_only_the_networks_part() {
    let mut paired = setup();
    let wifi = network(NetworkKind::WiFi, false, false);
    let hotspot = NetworkState {
        hotspot: true,
        ..network(NetworkKind::WiFi, true, false)
    };
    // 23 % on a free Wi-Fi: 70, gently, nothing held.
    assert_eq!(sync_view::health_hold(Some(&computer(23, &wifi)), &paired), None);
    // 8 %: 40, still gently. 5 % in Low Power Mode: under it.
    assert_eq!(sync_view::health_hold(Some(&computer(8, &wifi)), &paired), None);
    let drained = azul_appkit::client_health::Device {
        low_power_mode: true,
        ..computer(5, &wifi)
    };
    assert_eq!(
        sync_view::health_hold(Some(&drained), &paired),
        Some(25 * MB),
        "the auto-download size"
    );
    assert_eq!(sync_view::health_hold(None, &paired), None, "not read yet");
    // 23 % on a phone's hotspot: 35 - held by the computer too.
    let on_hotspot = computer(23, &hotspot);
    assert_eq!(sync_view::health_hold(Some(&on_hotspot), &paired), Some(25 * MB));
    // Sync anyway on this network: the hotspot no longer counts (70), the battery would.
    paired.sync_on_metered = true;
    assert_eq!(sync_view::health_hold(Some(&on_hotspot), &paired), None);
    assert_eq!(
        sync_view::transfer_hold(Some(&hotspot), Some(&on_hotspot), &paired),
        None,
        "sync anyway on this network: nothing waits"
    );
    assert_eq!(
        sync_view::health_hold(Some(&drained), &paired),
        Some(25 * MB),
        "a drained battery still holds big files back"
    );
    paired.sync_on_metered = false;
    // The pass takes the smaller limit of the two.
    paired.auto_download = AutoDownload::NewUnder(4);
    assert_eq!(
        sync_view::transfer_hold(Some(&wifi), Some(&drained), &paired),
        Some(4 * MB),
        "the computer's limit on a free network"
    );
    assert_eq!(
        sync_view::transfer_hold(Some(&hotspot), Some(&computer(90, &hotspot)), &paired),
        Some(4 * MB),
        "the network's limit on a healthy battery"
    );
    assert_eq!(
        sync_view::transfer_hold(Some(&wifi), Some(&computer(90, &wifi)), &paired),
        None
    );
    let offline = azul_appkit::client_health::Device {
        connected: false,
        ..drained
    };
    assert_eq!(
        sync_view::health_hold(Some(&offline), &paired),
        None,
        "offline: the pass says why it failed"
    );
}

#[test]
fn a_pass_on_a_weak_computer_moves_one_small_file_at_a_time() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("gentle-a", &drive, AutoDownload::NewUnder(1));
    let mut work = a.work();
    assert_eq!(work.parallel, 4, "the default");
    work.parallel = crate::health::transfers(35);
    assert_eq!(work.parallel, 1);
    assert_eq!(work.session().parallel(), 1, "the session takes it");
}

/// What a pass answered.
fn done_of(outcome: &Outcome) -> sync_jobs::PassDone {
    match outcome {
        Outcome::Sync(SyncOutcome::Passed { result, .. }) => result.clone().expect("the pass ran"),
        _ => panic!("not a pass's answer"),
    }
}

#[test]
fn a_metered_or_low_data_network_holds_back_files_over_the_auto_download_size() {
    let mut paired = setup();
    let wifi = network(NetworkKind::WiFi, false, false);
    let hotspot = network(NetworkKind::Cellular, true, false);
    let low_data = network(NetworkKind::WiFi, false, true);
    assert_eq!(sync_view::network_hold(Some(&wifi), &paired), None);
    assert_eq!(
        sync_view::network_hold(None, &paired),
        None,
        "not read yet: as a free network"
    );
    assert_eq!(
        sync_view::network_hold(Some(&hotspot), &paired),
        Some(25 * MB),
        "the default auto-download size"
    );
    assert_eq!(sync_view::network_hold(Some(&low_data), &paired), Some(25 * MB));
    paired.auto_download = AutoDownload::NewUnder(4);
    assert_eq!(sync_view::network_hold(Some(&hotspot), &paired), Some(4 * MB));
    paired.auto_download = AutoDownload::Everything;
    assert_eq!(
        sync_view::network_hold(Some(&hotspot), &paired),
        Some(25 * MB),
        "another choice: the default's size"
    );
    let offline = NetworkState {
        connected: false,
        ..hotspot
    };
    assert_eq!(
        sync_view::network_hold(Some(&offline), &paired),
        None,
        "offline: the pass says why it failed"
    );
    paired.sync_on_metered = true;
    assert_eq!(
        sync_view::network_hold(Some(&hotspot), &paired),
        None,
        "Sync anyway on this network"
    );
}

#[test]
fn the_status_line_says_paused_metered_network_while_big_transfers_wait() {
    let mut states = SyncStates::default();
    states.last_pass = Some(1);
    let mut paired = setup();
    assert_eq!(
        sync_view::status_text(&paired, &states, None, true, false, true).key,
        "azdrive-sync-status-metered"
    );
    assert_eq!(
        sync_view::status_text(&paired, &states, None, true, false, false).key,
        "azdrive-sync-status-up-to-date"
    );
    // While a pass of the small files runs, the line still says why the big ones wait.
    let running = Running {
        cancel: Arc::new(AtomicBool::new(false)),
        progress: PassProgress {
            files_done: 0,
            files_total: 2,
            ..PassProgress::default()
        },
    };
    assert_eq!(
        sync_view::status_text(&paired, &states, Some(&running), true, false, true).key,
        "azdrive-sync-status-metered"
    );
    // What the user has to act on says so first.
    assert_eq!(
        sync_view::status_text(&paired, &states, None, true, true, true).key,
        "azdrive-sync-status-payment-due"
    );
    paired.paused = true;
    assert_eq!(
        sync_view::status_text(&paired, &states, None, true, false, true).key,
        "azdrive-sync-status-paused"
    );
}

#[test]
fn a_pass_on_a_metered_network_sends_small_files_and_keeps_big_ones_for_a_free_network() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("metered-a", &drive, AutoDownload::NewUnder(1));
    a.write("small.txt", b"small");
    a.write("big.bin", &vec![7u8; (MB + 1) as usize]);
    let mut work = a.work();
    let hotspot = network(NetworkKind::Cellular, true, false);
    work.transfer_limit = sync_view::network_hold(Some(&hotspot), &work.setup);
    assert_eq!(work.transfer_limit, Some(MB));
    let answer = sync_jobs::run(
        SyncJob::Pass {
            work,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &mut |_| {},
    );
    let done = done_of(&answer);
    assert_eq!((done.up, done.held), (1, 1), "{done:?}");
    assert_eq!(
        states_of(&answer).state_of("small.txt"),
        Some(FileState::OnDevice)
    );
    assert_eq!(
        states_of(&answer).state_of("big.bin"),
        None,
        "not on the drive yet"
    );
    // A free network: the next pass sends it.
    let (_, answer) = a.pass();
    let done = done_of(&answer);
    assert_eq!((done.up, done.held), (1, 0), "{done:?}");
    assert_eq!(
        states_of(&answer).state_of("big.bin"),
        Some(FileState::OnDevice)
    );
}

/// SYNC25A gave the session its restore notice (`with_restore`), but AzDrive's passes never
/// passed the Azlin drive's announcement on: a drive the server restored from its lagging
/// copy was refused as a rollback, and the files committed after that copy were not sent
/// again. A work's notice reaches the pass: it heals from here instead.
#[test]
fn an_announced_restore_reaches_the_pass_and_the_files_after_the_copy_go_up_again() {
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("restore-a", &drive, AutoDownload::Everything);
    a.write("a.jpg", b"picture a");
    a.pass();
    let key = azcloud_kit::sync::remote::index_key("Photos/");
    let backup = drive.get(&key).expect("the index after the first pass");
    a.write("lag.jpg", b"committed after the copy");
    a.pass();
    // The server put its older copy back and announced it.
    drive.put(&key, &backup).expect("the restore");
    let mut work = a.work();
    work.restore = Some(azcloud_kit::sync::RestoreNotice {
        epoch: 1,
        prefix: String::new(),
        ..azcloud_kit::sync::RestoreNotice::default()
    });
    let answer = sync_jobs::run(
        SyncJob::Pass {
            work,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &mut |_| {},
    );
    match &answer {
        Outcome::Sync(SyncOutcome::Passed { result, .. }) => {
            assert!(result.is_ok(), "the restore is taken, not refused: {result:?}");
        }
        _ => panic!("not a pass's answer"),
    }
    let index = azcloud_kit::sync::RemoteIndex::parse(&drive.get(&key).expect("an index"))
        .expect("the index reads");
    assert!(index.files.contains_key("lag.jpg"), "sent again: {:?}", index.files.keys());
    assert!(index.files.contains_key("a.jpg"));
}

/// SYNC25A keeps a file the drive deleted before this pairing knew it, lists it
/// (`deleted-elsewhere-kept`) and sends it only when the user says so - but AzDrive had no
/// way to say so. "Send them again" takes the listed keys out of the kept set; the next pass
/// sends them.
#[test]
fn send_them_again_releases_the_kept_files_for_the_next_pass() {
    use azcloud_kit::sync::{
        local::LocalIndex,
        session::{kept_files, INDEX_FILE},
    };
    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("send-kept-a", &drive, AutoDownload::Everything);
    a.write("report.pdf", b"kept here");
    a.pass();
    let index_path = a.work().dir.join(INDEX_FILE);
    let mut index = LocalIndex::load(&index_path).unwrap().expect("the first pass's base");
    index.kept.insert(String::from("report.pdf"));
    index.save(&index_path).unwrap();
    // The keys are the base's own set (X-DELTA-SYNC4-31), not the problem list's capped rows.
    let keys = kept_files(&a.work().dir).unwrap();
    assert_eq!(keys, ["report.pdf"]);

    let answer = sync_jobs::run(SyncJob::SendKept { work: a.work(), keys }, &mut |_| {});

    assert!(matches!(
        &answer,
        Outcome::Sync(SyncOutcome::Changed {
            done: SyncChange::SentAgain(1),
            result: Ok(_),
            ..
        })
    ));
    let index = LocalIndex::load(&index_path).unwrap().expect("the base");
    assert!(index.kept.is_empty(), "{:?}", index.kept);
}

/// X-DELTA-SYNC7-8: the sync view is built again on every refresh of the page, and the visibility of "Send
/// them again" asked `kept_keys` - which loads and parses the pairing's whole base (every file it knows: tens of
/// megabytes on a big drive) for each pairing, on the UI thread, to learn whether the set is empty. The button
/// is offered from the problem list the last pass left, a bounded file the view loads anyway; the base is read
/// by the click alone. The base here is a FOLDER named like the base file: any read of it fails - the old code
/// found no kept keys in it and hid the button, though the list names a kept file.
#[test]
fn x_delta_sync7_8_the_button_is_offered_from_the_problem_list_and_the_base_is_not_opened() {
    use azcloud_kit::sync::{
        problems::{Code, Problem, ProblemList},
        session::INDEX_FILE,
    };
    let state = TempDir::new("azdrive-kept-offered");
    fs::create_dir_all(state.path().join(INDEX_FILE)).unwrap();
    assert!(!crate::sync_view::kept_listed(state.path()), "no list: no button");

    let mut list = ProblemList::default();
    list.publish(
        vec![Problem::new(Code::DeletedElsewhereKept, "report.pdf", "kept here")],
        true,
        1,
    );
    list.save(state.path()).unwrap();
    assert!(
        crate::sync_view::kept_listed(state.path()),
        "the list names a kept file: the button is offered, whatever the base is"
    );
    // The click's own read of the base still fails here, as it did: it is not the view's business.
    assert!(azcloud_kit::sync::session::kept_files(state.path()).is_err());

    // A list the last pass left empty offers nothing.
    ProblemList::default().save(state.path()).unwrap();
    assert!(!crate::sync_view::kept_listed(state.path()));
}

/// X-DELTA-SYNC8-5: the click on "Send them again" reads the base, and a base that cannot be read is no list of nothing: the
/// answer is the sentence of what is wrong - the click used to find no keys, send nothing and say nothing. The base here is a
/// FOLDER named like the base file: any read of it fails (for every user, root too). A base that reads gives its keys.
/// RED on the old `kept_keys`: `unwrap_or_default()` - no error to tell.
#[test]
fn x_delta_sync8_5_a_base_that_cannot_be_read_is_told_to_the_click_and_is_no_empty_list() {
    use azcloud_kit::sync::session::INDEX_FILE;
    let state = TempDir::new("azdrive-kept-unreadable");
    fs::create_dir_all(state.path().join(INDEX_FILE)).unwrap();
    let told = crate::sync_view::kept_keys_in(state.path());
    let why = told.expect_err("a base nobody can read says so");
    assert!(!why.to_string().is_empty(), "{why:?}");
    // The sentence the click shows has its words in both languages.
    let phrase =
        azul_appkit::l10n::Phrase::new("azdrive-sync-kept-unreadable").arg("why", why.to_string());
    assert_eq!(phrase.key, "azdrive-sync-kept-unreadable");

    // No base at all (a pairing that never synced) is an empty list, not an error.
    let none = TempDir::new("azdrive-kept-none");
    assert_eq!(
        crate::sync_view::kept_keys_in(none.path()).unwrap(),
        Vec::<String>::new()
    );
}

/// The sync trash's rows: each file by its key, the day it was deleted and the day the trash
/// lets it go, in the window's language.
#[test]
fn a_trashed_file_says_when_it_was_deleted_and_until_when_it_can_be_restored() {
    crate::l10n::in_english();
    let item = azcloud_kit::sync::trash::Trashed {
        key: String::from("photos/a.jpg"),
        trashed_at: Some(1_792_839_600),
        bytes: 6,
    };
    let row = azul_appkit::l10n::t_phrase(&crate::sync_view::trash_row(&item));
    assert!(row.contains("photos/a.jpg"), "{row}");
    assert!(row.contains("2026-10-24"), "deleted: {row}");
    assert!(row.contains("2026-11-23"), "kept until: {row}");
    for key in [
        "azdrive-sync-menu-trash",
        "azdrive-sync-trash-title",
        "azdrive-sync-trash-empty",
        "azdrive-sync-trash-restore",
        "azdrive-sync-trash-restore-all",
    ] {
        assert_ne!(azul_appkit::l10n::t(key), key, "{key} has its words");
    }
}

/// X-AR5b-3 (part): the three readers in lib.rs asked the old two-valued `is_encrypted()` and read
/// its `None` each their own way; they ask the state's policies now. A listing may be kept
/// only where the bucket's keys are all there is to the names, the names come from the drive
/// index only for an encrypted drive, and a drive that is not decided is neither.
#[cfg(feature = "encryption")]
#[test]
fn x_ar5b_3_the_listing_cache_and_the_drive_index_follow_the_encryption_state() {
    use azul_storage::trust::{EncState, Via, Why};

    // A drive without an encryption seam: its listing holds the bucket's own keys.
    assert!(crate::keeps_listing_in(None));
    assert!(!crate::names_from_drive_index_in(None));
    let encrypted = EncState::Encrypted {
        key_id: None,
        via: Via::KeyInKeyring,
    };
    for (state, keeps, from_index) in [
        (EncState::Undecided, false, false),
        (EncState::SetupPending, true, false),
        (EncState::Unencrypted(Why::New), true, false),
        (EncState::Unencrypted(Why::HiddenOrAbsent), true, false),
        (encrypted, false, true),
    ] {
        assert_eq!(crate::keeps_listing_in(Some(state)), keeps, "{state:?}");
        assert_eq!(
            crate::names_from_drive_index_in(Some(state)),
            from_index,
            "{state:?}"
        );
    }
}

// ==== A drive's session follows its state ====

/// X-AR5b-3: the session of an Azlin drive for every state it can be in. Through the drive's names
/// only when it is encrypted (with its files' encrypted copies when the pairing keeps them); in
/// every other state - not decided, a setup that stopped, a new drive, a bucket that hides its key
/// files - no session reads or writes: the pass answers why. There is no plain one.
#[cfg(feature = "encryption")]
#[test]
fn x_ar5b_3_an_azlin_drive_is_synced_through_its_names_only_when_it_is_encrypted() {
    use azul_storage::trust::{EncState, Via, Why};

    use crate::sync_jobs::{session_plan, SessionPlan};

    for copies in [false, true] {
        for state in [
            EncState::Undecided,
            EncState::SetupPending,
            EncState::Unencrypted(Why::New),
            EncState::Unencrypted(Why::HiddenOrAbsent),
        ] {
            assert_eq!(session_plan(state, copies), SessionPlan::Unopened, "{state:?}");
        }
        for via in [Via::KeyInKeyring, Via::Record, Via::KeyFilesListed] {
            let state = EncState::Encrypted { key_id: None, via };
            let expected = if copies {
                SessionPlan::EncryptedCopies
            } else {
                SessionPlan::Named
            };
            assert_eq!(session_plan(state, copies), expected, "{state:?}");
        }
    }
}

/// A drive handle that counts the calls made through it and hands them on: what a pass did with
/// the drive it was given, not only what came of it.
#[cfg(feature = "encryption")]
struct CountedCalls {
    inner: Arc<dyn Drive>,
    calls: AtomicU64,
}

#[cfg(feature = "encryption")]
impl CountedCalls {
    fn new(inner: Arc<dyn Drive>) -> CountedCalls {
        CountedCalls {
            inner,
            calls: AtomicU64::new(0),
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }

    fn counted(&self) -> &Arc<dyn Drive> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        &self.inner
    }
}

#[cfg(feature = "encryption")]
impl Drive for CountedCalls {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.counted().list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.counted().get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.counted().get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.counted().put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.counted().delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.counted().head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.counted().put_if(key, bytes, condition)
    }
}

/// X-AR6-26802cb54-1: "the sync does not start". A pass on an Azlin drive whose bucket shows a
/// plain object and no key files (a drive made before the encryption, or a bucket that hides
/// them: nothing here tells the two apart) starts no session that could read or write the bucket
/// as a plain one. It fails with the drive's own refusal, and the bucket is as it was.
///
/// The refusal and the untouched bucket alone prove nothing about the session: the drive the
/// pass is given is the encryption's gate, and a session that had been started anyway would be
/// refused by the gate in the same words (X-DELTA-AZDRIVE-KEYS-40). What tells them apart is the
/// number of calls the pass makes through that handle: the one probe that lets the gate decide,
/// and nothing after it - a plain session would list the drive, and be refused, at least once more.
#[cfg(feature = "encryption")]
#[test]
fn x_ar6_26802cb54_1_a_pass_on_a_drive_that_is_not_encrypted_starts_no_plain_session() {
    use azul_storage::{keyring::MemoryKeyring, AutoEncrypted};

    let bucket = Arc::new(MemDrive::default());
    bucket.put("Photos/old.jpg", b"plain").unwrap();
    let auto = Arc::new(AutoEncrypted::new(
        bucket.clone(),
        "d_photos",
        Arc::new(MemoryKeyring::new()),
        None,
    ));
    let gate: Arc<dyn Drive> = auto.clone();
    let counted = Arc::new(CountedCalls::new(gate));
    let drive: Arc<dyn Drive> = counted.clone();
    let device = Device::new("not-encrypted", &drive, AutoDownload::Everything);
    device.write("new.jpg", b"picture");
    let mut work = device.work();
    work.auto = Some(auto);

    let answer = sync_jobs::run(
        SyncJob::Pass {
            work,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        &mut |_| {},
    );
    match answer {
        Outcome::Sync(SyncOutcome::Passed {
            result: Err(why), ..
        }) => assert!(
            why.contains("cannot tell"),
            "the pass says the drive cannot be told from an encrypted one: {why}"
        ),
        Outcome::Sync(SyncOutcome::Passed {
            result: Ok(done), ..
        }) => {
            panic!("a drive that is not encrypted was synced: {} up", done.up)
        }
        _ => panic!("not a pass's answer"),
    }
    let keys: Vec<String> = bucket.objects.lock().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        vec![String::from("Photos/old.jpg")],
        "nothing was written"
    );
    assert_eq!(
        counted.calls(),
        1,
        "the pass asked the drive once, to decide what it is, and started no session on it"
    );
}

// ==== A slot opens through the encryption, or not at all ====

/// X-AR6-26802cb54-1: an Azlin drive is encrypted, always. A build made without the encryption (a
/// build for the tests of the plain paths, never shipped) refuses it at the opening, with its
/// name, and takes no other drive for one; a build with the encryption does not refuse it for
/// that. The last part enters through `Slot::open`, in the build the test runs in.
#[test]
fn x_ar6_26802cb54_1_a_build_without_the_encryption_refuses_an_azlin_drive_at_the_opening() {
    use azul_storage::{
        config::{DriveAuth, DriveEntry, DriveLocation},
        keyring::{KeyringNamespace, MemoryKeyring},
    };

    let azlin = DriveEntry {
        id: String::from("slot-1"),
        name: String::from("Photos"),
        location: DriveLocation::S3 {
            endpoint: String::from("https://s3.example.test"),
            region: String::from("us-east-1"),
            bucket: String::from("photos"),
            path_style: true,
            auth: DriveAuth::Azlin {
                drive_id: String::from("d_photos"),
                account_url: String::new(),
                encrypted: false,
                key_id: None,
            },
        },
    };
    let local = DriveEntry {
        id: String::from("slot-2"),
        name: String::from("Bucket"),
        location: DriveLocation::Local {
            root: String::from("/tmp/bucket"),
        },
    };

    match crate::azlin_needs_encryption(&azlin, false) {
        Some(DriveError::Unsupported(why)) => {
            assert!(why.contains("\"Photos\""), "{why}");
            assert!(why.contains("without the encryption"), "{why}");
        }
        other => panic!("an Azlin drive in a build without the encryption: {other:?}"),
    }
    assert!(crate::azlin_needs_encryption(&azlin, true).is_none());
    assert!(
        crate::azlin_needs_encryption(&local, false).is_none(),
        "a folder of this computer is no Azlin drive"
    );
    assert!(crate::azlin_needs_encryption(&local, true).is_none());

    // Through the function production calls. The slot has no session: with the encryption that
    // is its own refusal (Denied), without it the build's.
    let dir = TempDir::new("slot-open");
    let keyring = azcloud_kit::SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        dir.path(),
    );
    let mut slot = crate::Slot::new(azlin);
    let refused = slot
        .open(
            &crate::RotatedSessions::default(),
            &crate::KeyRecords::default(),
            None,
            None,
            &keyring,
        )
        .err();
    if cfg!(feature = "encryption") {
        assert!(
            matches!(refused, Some(DriveError::Denied { .. })),
            "{refused:?}"
        );
    } else {
        assert!(
            matches!(refused, Some(DriveError::Unsupported(_))),
            "{refused:?}"
        );
    }
}

/// X-AR5b-24 (the wiring): a shipped build (without `test-hooks`) polls every synced drive every 30
/// seconds and waits two seconds for a folder to be quiet, whatever `$AZDRIVE_SYNC_POLL` and
/// `$AZDRIVE_SYNC_SETTLE_MS` say; only a test build reads them. The switches are set for the test:
/// a build that does not read them lets nothing else see them.
#[cfg(not(feature = "test-hooks"))]
#[test]
fn x_ar5b_24_a_shipped_build_polls_and_settles_at_its_own_pace() {
    std::env::set_var("AZDRIVE_SYNC_POLL", "1");
    std::env::set_var("AZDRIVE_SYNC_SETTLE_MS", "1");
    let poll = sync_jobs::poll_secs();
    let settle = sync_jobs::settle();
    std::env::remove_var("AZDRIVE_SYNC_POLL");
    std::env::remove_var("AZDRIVE_SYNC_SETTLE_MS");
    assert_eq!(poll, 30);
    assert_eq!(settle, std::time::Duration::from_millis(2_000));
}

// ==== AZ-CN4: a source that cannot commit a sync is not offered one ====

fn drive_entry(location: azul_storage::config::DriveLocation) -> azul_storage::config::DriveEntry {
    azul_storage::config::DriveEntry {
        id: String::from("x-1"),
        name: String::from("X"),
        location,
    }
}

/// The review's example: a Dropbox, Google Drive, GCS or Redis drive was offered "Sync with a
/// folder", the folder went up as plain files and the commit then said "writing .. only if it is
/// unchanged" is unsupported. The pairing's one funnel and the menu and the ribbon ask the same
/// function; a bucket and an Azlin drive are not refused.
#[test]
fn az_cn4_an_opendal_or_database_drive_is_refused_the_folder_sync_and_a_bucket_is_not() {
    use azul_storage::config::{DatabaseEngine, DriveAuth, DriveLocation};

    let why = Some("azdrive-sync-why-no-conditional-writes");
    for scheme in [
        "dropbox", "gdrive", "onedrive", "gcs", "redis", "webdav", "ftp",
    ] {
        let entry = drive_entry(DriveLocation::Opendal {
            scheme: scheme.to_string(),
            options: BTreeMap::new(),
            keyring: false,
        });
        assert_eq!(sync_view::pair_refusal(&entry), why, "{scheme}");
    }
    let database = drive_entry(DriveLocation::Database {
        engine: DatabaseEngine::Postgres,
        options: BTreeMap::new(),
        keyring: false,
    });
    assert_eq!(sync_view::pair_refusal(&database), why);
    let bucket = |auth| {
        drive_entry(DriveLocation::S3 {
            endpoint: String::from("https://s3.example"),
            region: String::from("us-east-1"),
            bucket: String::from("b"),
            path_style: true,
            auth,
        })
    };
    assert_eq!(sync_view::pair_refusal(&bucket(DriveAuth::Keyring)), None);
    let azlin = bucket(DriveAuth::Azlin {
        drive_id: String::from("d_k3f9"),
        account_url: String::new(),
        encrypted: true,
        key_id: None,
    });
    assert_eq!(sync_view::pair_refusal(&azlin), None);
}

/// The sentence the ribbon and the menu show exists in both languages and says what is missing.
#[test]
fn az_cn4_the_refusal_is_said_in_english_and_german() {
    crate::l10n::in_english();
    let said = azul_appkit::l10n::t("azdrive-sync-why-no-conditional-writes");
    assert!(said.contains("only if unchanged"), "{said}");
    for (tag, source) in crate::l10n::sources() {
        assert!(
            source.contains("azdrive-sync-why-no-conditional-writes = "),
            "{tag} lacks the refusal"
        );
    }
}

/// X-DELTA-AZDRIVE-FILES-30: the sync tasks that change files of the folder one after the other
/// outside a pass - a delete, a rename, a conflict's answer, "Free up space", files moved in - are
/// work a close waits for. `Job::Sync(_)` was exempt wholesale, and a close went over them.
#[test]
fn x_delta_azdrive_files_30_the_sync_tasks_that_change_files_hold_a_close_back() {
    use azcloud_kit::sync::session::Resolution;

    use crate::jobs::Job;

    let drive: Arc<dyn Drive> = Arc::new(MemDrive::default());
    let a = Device::new("cuts-a", &drive, AutoDownload::Everything);
    let key = || String::from("a.txt");
    let cuts = vec![
        SyncJob::Delete {
            work: a.work(),
            keys: vec![key()],
        },
        SyncJob::Rename {
            work: a.work(),
            from: key(),
            to: String::from("b.txt"),
        },
        SyncJob::MoveIn {
            work: a.work(),
            from: PathBuf::from("/nowhere/a.txt"),
            key: key(),
        },
        SyncJob::Resolve {
            work: a.work(),
            key: key(),
            choice: Resolution::KeepBoth,
        },
        SyncJob::FreeUp {
            work: a.work(),
            keys: vec![key()],
        },
    ];
    for job in cuts {
        assert!(Job::Sync(job).cuts_data(), "a task that changes files");
    }
    // A pass is the sync view's to count; the rest cut nothing in two.
    let quiet = vec![
        SyncJob::Pass {
            work: a.work(),
            cancel: Arc::new(AtomicBool::new(false)),
        },
        SyncJob::Open {
            work: a.work(),
            key: key(),
        },
        SyncJob::Touch {
            work: a.work(),
            key: key(),
        },
        SyncJob::Fetch {
            work: a.work(),
            keys: vec![key()],
        },
        SyncJob::AnswerBurst { work: a.work() },
        SyncJob::Verify { work: a.work() },
    ];
    for job in quiet {
        assert!(!Job::Sync(job).cuts_data());
    }
}

/// X-DELTA-AZDRIVE-FILES-32 (c): a typed pairing folder is a whole path with no `..` in it; the
/// checks after it compare names, so `~/AzDrive/../../etc` was "inside Home".
#[test]
fn x_delta_azdrive_files_32_a_pairing_folder_with_dots_in_its_path_is_refused() {
    use crate::sync_jobs::pair_folder;
    assert!(pair_folder("").is_err());
    #[cfg(unix)]
    {
        assert_eq!(
            pair_folder("/home/me/AzDrive/Photos").unwrap(),
            PathBuf::from("/home/me/AzDrive/Photos")
        );
        for bad in [
            "AzDrive/Photos",
            "/home/me/AzDrive/../../etc",
            "/home/me/..",
        ] {
            assert!(pair_folder(bad).is_err(), "{bad:?}");
        }
    }
}

/// X-DELTA-AZDRIVE-FILES-34: the answer that deletes from the drive (here and on the other devices)
/// is never the main button of its dialog; the mass-delete question's main button loses nothing.
#[test]
fn x_delta_azdrive_files_34_the_answer_that_deletes_is_not_the_main_button() {
    use azul::widgets::ButtonType;
    assert_ne!(sync_view::DELETE_FACE, ButtonType::Primary);
    assert_ne!(sync_view::DELETE_FACE, ButtonType::Default);
    for here in [true, false] {
        for choice in sync_view::mass_delete_choices(here) {
            assert_eq!(choice.primary, !choice.deletes, "{}", choice.text);
        }
    }
}
