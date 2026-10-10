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

#[test]
fn the_status_line_says_up_to_date_syncing_paused_read_only_and_conflicts() {
    let mut states = SyncStates::default();
    let mut paired = setup();
    assert_eq!(
        sync_view::status_text(&paired, &states, None, true),
        "Not synced yet"
    );
    states.last_pass = Some(1);
    assert_eq!(sync_view::status_text(&paired, &states, None, true), "Up to date");
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
    let syncing = sync_view::status_text(&paired, &states, Some(&running), true);
    assert!(syncing.starts_with("Syncing 12 files ("), "{syncing}");
    assert!(syncing.contains("340"), "{syncing}");
    states.files.insert(
        String::from("a.jpg"),
        FileRecord {
            conflict: Some(HeldConflict {
                key: String::from("a.jpg"),
                here: String::new(),
                there: String::new(),
                there_size: 1,
                there_device: String::from("laptop"),
            }),
            ..FileRecord::default()
        },
    );
    assert_eq!(
        sync_view::status_text(&paired, &states, None, true),
        "Waiting for you: 1 conflict"
    );
    states.read_only = true;
    assert_eq!(
        sync_view::status_text(&paired, &states, None, true),
        "Read-only (payment due)"
    );
    assert_eq!(sync_view::status_text(&paired, &states, None, false), "Read-only");
    paired.paused = true;
    assert_eq!(sync_view::status_text(&paired, &states, None, true), "Paused");
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
    // A plain drive's own listing holds the sync's blobs, not its files.
    assert_eq!(store.file_state("d_photos", "Photos/here.txt"), None);
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
        PathBuf::from("/home/me/AzDrive/a_b_c")
    );
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
        Some(("manage_search", "In the search index"))
    );
    assert_eq!(
        sync_view::index_overlay(FileIndexing::NotIndexable),
        Some(("search_off", "Not indexable: no text, or too big"))
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
