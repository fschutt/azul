//! An app's pairing of a folder with a drive ([`SyncSession`]): its settings, its files' states
//! kept between runs, a pass (the on-demand files, pinned files brought down, the conflicts
//! held for a choice), opening a cloud-only file, "Free up space", the size cap, a drive that
//! takes no writes; and an encrypted drive whose local copies stay encrypted ([`ObjectCache`]).

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use azul_storage::{testing::TempDir, ByteRange, Drive, DriveError};

use super::{
    fake_s3::{FakeS3, S3Bucket},
    sync_named::{s3_drive, s3_drive_resuming},
};
use crate::{
    error::{CloudError, CloudResult},
    store::{Conditional, RemoteObject, RemoteStore},
    sync::{
        local::path_of,
        objects::ObjectCache,
        session::{
            is_read_only, AutoDownload, FileRecord, FileState, LocalCopies, Pass, Resolution,
            SyncSession, SyncSetup, SyncStates,
        },
        HeldConflict, SyncEvent,
    },
};

const MB: u64 = 1024 * 1024;

/// One device's pairing: its folder and its state folder.
struct Paired {
    folder: TempDir,
    state: TempDir,
    session: SyncSession,
}

fn setup_of(folder: &TempDir) -> SyncSetup {
    SyncSetup::new("d_test", "Documents/", folder.path())
}

fn paired(name: &str, store: &Arc<S3Bucket>, edit: impl FnOnce(&mut SyncSetup)) -> Paired {
    let folder = TempDir::new(&format!("azcloud-session-{name}"));
    let state = TempDir::new(&format!("azcloud-session-state-{name}"));
    let mut setup = setup_of(&folder);
    edit(&mut setup);
    let remote: Arc<dyn RemoteStore> = store.clone();
    let session = SyncSession::plain(setup, state.path().to_path_buf(), name, remote);
    Paired {
        folder,
        state,
        session,
    }
}

impl Paired {
    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(path_of(self.folder.path(), key)).ok()
    }

    fn pass(&self) -> (SyncStates, Vec<SyncEvent>) {
        let mut heard = Vec::new();
        let stop = AtomicBool::new(false);
        let pass = self.session.pass(&stop, &mut |e| heard.push(e)).unwrap();
        (pass.states, heard)
    }
}

#[test]
fn a_pairing_has_the_plans_defaults_and_a_state_folder_of_its_own() {
    let folder = TempDir::new("azcloud-session-defaults");
    let setup = setup_of(&folder);
    assert_eq!(setup.auto_download, AutoDownload::NewUnder(25));
    assert_eq!(setup.local_copies, LocalCopies::Decrypted);
    assert_eq!(setup.keep_gb, None);
    assert!(!setup.paused);
    let text = serde_json::to_string(&setup).unwrap();
    assert_eq!(serde_json::from_str::<SyncSetup>(&text).unwrap(), setup);
    let other = TempDir::new("azcloud-session-defaults-2");
    let root = PathBuf::from("/cache/sync");
    assert_eq!(setup.state_dir(&root), setup.state_dir(&root), "stable");
    assert_ne!(setup.state_dir(&root), setup_of(&other).state_dir(&root));
    assert!(setup.state_dir(&root).starts_with(&root));
}

#[test]
fn the_auto_download_policy_takes_new_files_by_size() {
    assert!(AutoDownload::Everything.wants(10_000 * MB));
    assert!(AutoDownload::NewUnder(25).wants(25 * MB));
    assert!(!AutoDownload::NewUnder(25).wants(25 * MB + 1));
    assert!(!AutoDownload::PinnedOnly.wants(1));
    assert!(!AutoDownload::Nothing.wants(0));
}

#[test]
fn a_pass_leaves_big_new_files_in_the_cloud_and_says_each_files_state() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| s.auto_download = AutoDownload::NewUnder(1));
    a.write("a.txt", b"tiny");
    a.write("media/big.bin", &vec![7u8; (MB + 1) as usize]);
    let (states, heard) = a.pass();
    assert_eq!(states.state_of("a.txt"), Some(FileState::OnDevice));
    assert!(matches!(heard.first(), Some(SyncEvent::Planned { up_files: 2, .. })));

    let (states, _) = b.pass();
    assert_eq!(states.state_of("a.txt"), Some(FileState::OnDevice));
    assert_eq!(states.state_of("media/big.bin"), Some(FileState::CloudOnly));
    assert_eq!(states.files["media/big.bin"].size, MB + 1);
    assert!(b.read("media/big.bin").is_none());
    assert!(
        b.folder.path().join("media").is_dir(),
        "a cloud-only file's folder is here, so it can be browsed"
    );
    // The states outlive the session: the next start shows them before its first pass.
    assert_eq!(SyncStates::load(b.state.path()), states);
    assert_eq!(b.session.states(), states);
}

#[test]
fn opening_a_cloud_only_file_downloads_it_and_marks_it_used() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| s.auto_download = AutoDownload::Nothing);
    a.write("report.pdf", b"%PDF the report");
    a.pass();
    let (states, _) = b.pass();
    assert_eq!(states.state_of("report.pdf"), Some(FileState::CloudOnly));
    let path = b.session.open("report.pdf").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"%PDF the report");
    let states = b.session.states();
    assert_eq!(states.state_of("report.pdf"), Some(FileState::OnDevice));
    assert!(states.files["report.pdf"].last_used >= crate::now() - 60);
    // An open of a file on this device is no download: the same path.
    assert_eq!(b.session.open("report.pdf").unwrap(), path);
}

#[test]
fn a_pinned_cloud_only_file_comes_down_on_the_next_pass() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| s.auto_download = AutoDownload::PinnedOnly);
    a.write("docs/x.txt", b"x");
    a.write("other.txt", b"o");
    a.pass();
    let (states, _) = b.pass();
    assert_eq!(states.state_of("docs/x.txt"), Some(FileState::CloudOnly));
    b.session.pin(&[String::from("docs/")], true).unwrap();
    let mut heard = Vec::new();
    let pass = b
        .session
        .pass(&AtomicBool::new(false), &mut |e| heard.push(e))
        .unwrap();
    assert_eq!(pass.fetched, vec![String::from("docs/x.txt")]);
    assert_eq!(b.read("docs/x.txt").as_deref(), Some(&b"x"[..]));
    assert_eq!(pass.states.state_of("docs/x.txt"), Some(FileState::Pinned));
    assert_eq!(pass.states.state_of("other.txt"), Some(FileState::CloudOnly));
    assert_eq!(pass.states.folder_state("docs/"), Some(FileState::Pinned));
}

#[test]
fn free_up_space_frees_a_folder_but_never_a_file_kept_on_this_device() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    a.write("photos/1.jpg", b"one");
    a.write("photos/2.jpg", b"two");
    a.write("photos/keep/3.jpg", b"three");
    a.pass();
    a.session.pin(&[String::from("photos/keep/")], true).unwrap();
    let freed = a.session.free_up(&[String::from("photos/")]).unwrap();
    assert_eq!(
        freed.freed,
        vec![String::from("photos/1.jpg"), String::from("photos/2.jpg")]
    );
    assert_eq!(freed.kept.len(), 1, "{:?}", freed.kept);
    assert_eq!(freed.kept[0].0, "photos/keep/3.jpg");
    assert!(a.read("photos/1.jpg").is_none());
    assert!(a.read("photos/keep/3.jpg").is_some());
    let (states, _) = a.pass();
    assert_eq!(states.state_of("photos/1.jpg"), Some(FileState::CloudOnly));
    assert_eq!(states.state_of("photos/keep/3.jpg"), Some(FileState::Pinned));
    assert!(store.read("Documents/.azlin/index.json").is_some());
    // Freeing a pinned folder itself unpins it.
    let freed = a.session.free_up(&[String::from("photos/keep/")]).unwrap();
    assert_eq!(freed.freed, vec![String::from("photos/keep/3.jpg")]);
    assert!(!a.session.states().is_pinned("photos/keep/3.jpg"));
}

#[test]
fn the_size_cap_frees_the_least_recently_used_files_first_never_a_pinned_one() {
    let record = |size: u64, last_used: i64| FileRecord {
        size,
        last_used,
        ..FileRecord::default()
    };
    let mut states = SyncStates::default();
    states.files.insert(String::from("old.bin"), record(400, 10));
    states.files.insert(String::from("pinned.bin"), record(400, 1));
    states.files.insert(String::from("new.bin"), record(400, 30));
    states.files.insert(String::from("mid.bin"), record(400, 20));
    let mut cloud = record(5000, 0);
    cloud.cloud_only = true;
    states.files.insert(String::from("cloud.bin"), cloud);
    let mut torn = record(400, 2);
    torn.conflict = Some(HeldConflict {
        key: String::from("torn.bin"),
        here: String::new(),
        there: String::new(),
        there_size: 0,
        there_device: String::new(),
        there_exec: false,
    });
    states.files.insert(String::from("torn.bin"), torn);
    states.pinned.insert(String::from("pinned.bin"));
    assert_eq!(states.local_bytes(), 2000);
    assert_eq!(
        states.to_free(1300),
        vec![String::from("old.bin"), String::from("mid.bin")],
        "least recently used first; pinned and conflicting files stay"
    );
    assert!(states.to_free(5000).is_empty());
}

#[test]
fn a_conflict_waits_for_a_choice_and_each_choice_does_what_it_says() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |_| {});
    a.write("note.md", b"base");
    a.pass();
    b.pass();

    // Keep mine: b's version goes to the drive.
    a.write("note.md", b"a's edit");
    b.write("note.md", b"b's edit!");
    a.pass();
    let (states, _) = b.pass();
    assert_eq!(states.state_of("note.md"), Some(FileState::Conflict));
    assert_eq!(states.conflicts().len(), 1);
    b.session.resolve("note.md", Resolution::KeepMine).unwrap();
    let (states, _) = b.pass();
    assert_eq!(states.state_of("note.md"), Some(FileState::OnDevice));
    a.pass();
    assert_eq!(a.read("note.md").as_deref(), Some(&b"b's edit!"[..]));

    // Take theirs: a's version comes here.
    a.write("note.md", b"a again");
    b.write("note.md", b"b again!");
    a.pass();
    b.pass();
    b.session.resolve("note.md", Resolution::TakeTheirs).unwrap();
    b.pass();
    assert_eq!(b.read("note.md").as_deref(), Some(&b"a again"[..]));

    // Keep both: the drive's keeps the name, mine a conflict copy - on both devices.
    a.write("note.md", b"a third");
    b.write("note.md", b"b third!");
    a.pass();
    b.pass();
    b.session.resolve("note.md", Resolution::KeepBoth).unwrap();
    let (states, _) = b.pass();
    assert!(states.conflicts().is_empty());
    assert_eq!(b.read("note.md").as_deref(), Some(&b"a third"[..]));
    a.pass();
    let names = |folder: &TempDir| -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            // The sync's own bookkeeping (the folder's marker, its trash) is no file of the user.
            .filter(|name| name != ".azlin")
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(&a.folder).len(), 2);
    assert_eq!(names(&a.folder), names(&b.folder));
}

/// A drive that takes reads but refuses writes (unpaid past its grace period).
struct Refusing(Arc<S3Bucket>);

impl RemoteStore for Refusing {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.0.get_unless(key, etag)
    }
    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        self.0.fetch(key, size)
    }
    fn put(&self, _key: &str, _data: &[u8]) -> CloudResult<String> {
        Err(CloudError::Drive(DriveError::Denied {
            message: String::from("this drive takes no writes"),
        }))
    }
    fn put_if(&self, key: &str, data: &[u8], _: Option<&str>) -> CloudResult<Option<String>> {
        self.put(key, data).map(Some)
    }
    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        self.0.head(key)
    }
    fn delete(&self, key: &str) -> CloudResult<()> {
        self.put(key, b"").map(|_| ())
    }
    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        self.0.list(prefix)
    }
}

#[test]
fn a_drive_that_takes_no_writes_is_read_only() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-session-refused");
    let state = TempDir::new("azcloud-session-refused-state");
    let refusing: Arc<dyn RemoteStore> = Arc::new(Refusing(store.clone()));
    let session = SyncSession::plain(
        setup_of(&folder),
        state.path().to_path_buf(),
        "dev-a",
        refusing,
    );
    fs::write(folder.path().join("new.txt"), b"new").unwrap();
    let failed = session
        .pass(&AtomicBool::new(false), &mut |_| {})
        .unwrap_err();
    assert!(is_read_only(&failed), "{failed}");
    let states = session.states();
    assert!(states.read_only);
    assert!(states.last_error.is_some());
}

#[test]
fn the_states_say_what_a_file_and_a_folder_are() {
    let mut states = SyncStates::default();
    let mut files: BTreeMap<String, FileRecord> = BTreeMap::new();
    files.insert(String::from("a/here.txt"), FileRecord::default());
    let mut cloud = FileRecord::default();
    cloud.cloud_only = true;
    files.insert(String::from("a/cloud.txt"), cloud.clone());
    files.insert(String::from("b/cloud.txt"), cloud);
    let mut broken = FileRecord::default();
    broken.error = Some(String::from("the drive did not answer"));
    files.insert(String::from("c/broken.txt"), broken);
    let mut sealed = FileRecord::default();
    sealed.encrypted_copy = true;
    files.insert(String::from("d/sealed.txt"), sealed);
    states.files = files;
    assert_eq!(states.state_of("a/here.txt"), Some(FileState::OnDevice));
    assert_eq!(states.state_of("a/cloud.txt"), Some(FileState::CloudOnly));
    assert_eq!(
        states.state_of("c/broken.txt"),
        Some(FileState::Error(String::from("the drive did not answer")))
    );
    assert_eq!(states.state_of("d/sealed.txt"), Some(FileState::OnDeviceEncrypted));
    assert_eq!(states.state_of("nope.txt"), None);
    assert_eq!(states.folder_state("a/"), Some(FileState::OnDevice), "partly here");
    assert_eq!(states.folder_state("b/"), Some(FileState::CloudOnly));
    assert!(matches!(states.folder_state("c/"), Some(FileState::Error(_))));
    assert_eq!(states.folder_state("z/"), None);
    assert!(!FileState::CloudOnly.label().is_empty());
}

#[test]
fn an_object_cache_serves_what_it_keeps_and_passes_the_rest_through() {
    let s3 = FakeS3::new();
    let dir = TempDir::new("azcloud-object-cache");
    let cache = ObjectCache::new(s3_drive(&s3), dir.path().to_path_buf());
    s3.write("data/ab/one", b"0123456789".to_vec());
    assert!(!cache.has("data/ab/one"));
    assert_eq!(cache.fill("data/ab/one").unwrap(), 10);
    assert!(cache.has("data/ab/one"));
    s3.clear_log();
    assert_eq!(cache.get("data/ab/one").unwrap(), b"0123456789");
    assert_eq!(
        cache
            .get_range("data/ab/one", ByteRange::new(2, Some(4)))
            .unwrap(),
        b"234"
    );
    assert_eq!(s3.count("GET"), 0, "served from this device");
    assert_eq!(cache.cached(), vec![String::from("data/ab/one")]);
    assert_eq!(cache.bytes(), 10);
    // A delete through the cache takes its copy too.
    cache.delete("data/ab/one").unwrap();
    assert!(!cache.has("data/ab/one"));
    assert!(s3.read("data/ab/one").is_none());
    // Everything else goes to the drive below; a read alone keeps nothing.
    cache.put("other", b"x").unwrap();
    assert_eq!(cache.get("other").unwrap(), b"x");
    assert!(!cache.has("other"));
}

#[test]
fn an_object_cache_hands_a_file_upload_down_and_the_bucket_resumes_it() {
    // AzDrive's Azlin drive is the bucket under this cache and the encryption's wrapper. A
    // file upload must reach the bucket as one: else it is streamed, and a dropped connection,
    // credentials that ran out or a killed app start it again and leave the first upload
    // unfinished in the bucket.
    use azul_storage::checked::{modified_ns, CheckedUpload, FileCheck};

    let s3 = FakeS3::new();
    let resume = TempDir::new("azcloud-object-cache-resume");
    let objects = TempDir::new("azcloud-object-cache-objects");
    let folder = TempDir::new("azcloud-object-cache-file");
    let cache = ObjectCache::new(
        s3_drive_resuming(&s3, resume.path()),
        objects.path().to_path_buf(),
    );
    let bytes: Vec<u8> = (0..12 * MB as usize + 11).map(|i| (i % 251) as u8).collect();
    let path = folder.path().join("video.bin");
    fs::write(&path, &bytes).unwrap();
    let meta = fs::metadata(&path).unwrap();
    let check = FileCheck {
        size: meta.len(),
        modified_ns: modified_ns(&meta),
        blake3: crate::sync::local::hash_bytes(&bytes),
        dev: 0,
        ino: 0,
    };
    // The connection drops after four parts.
    s3.take_parts(Some(4));
    assert!(cache
        .put_file_checked("blobs/video", &path, &check, &|_| {})
        .is_err());
    s3.take_parts(None);
    s3.clear_log();
    assert!(matches!(
        cache.put_file_checked("blobs/video", &path, &check, &|_| {}),
        Ok(CheckedUpload::Sent(_))
    ));
    assert_eq!(s3.read("blobs/video").as_deref(), Some(&bytes[..]));
    assert_eq!(
        s3.count_under("PUT", "blobs/video"),
        9,
        "the nine parts it lacked, never the first four again"
    );
    assert_eq!(
        s3.count_under("POST", "blobs/video"),
        1,
        "one Complete, no new upload started"
    );
}

// ==== The guards' questions (D42, the mass-delete guard), renames ====

/// Text of `n` bytes (low entropy, as the burst guard judges a file).
fn prose(n: usize, seed: u8) -> Vec<u8> {
    b"the quarterly report says the numbers look fine for now "
        .iter()
        .cycle()
        .take(n)
        .map(|b| if *b == b' ' { b' ' } else { b.wrapping_add(seed % 3) })
        .collect()
}

/// Random-looking bytes of `n` (what ransomware leaves): a xorshift, no crate.
fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

/// Ten text files turned into random bytes at once: the burst guard pauses this device's
/// uploads and the states keep why (a restart shows it); "these changes are mine" sends them
/// with the next pass.
#[test]
fn a_burst_pause_waits_for_the_user_and_these_changes_are_mine_sends_them() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    for i in 0..12u8 {
        a.write(&format!("docs/{i}.txt"), &prose(2048, i));
    }
    a.pass();
    a.pass();
    for i in 0..12u8 {
        a.write(&format!("docs/{i}.txt"), &noise(2048, u64::from(i) + 1));
    }
    let (states, _) = a.pass();
    let pause = states.burst.clone().expect("uploads paused");
    assert!(pause.changes >= 10, "{pause:?}");
    assert!(!pause.files.is_empty() && pause.files.len() <= 20);
    assert_eq!(SyncStates::load(a.state.path()).burst, states.burst, "it outlives the app");
    let index = String::from_utf8(store.read("Documents/.azlin/index.json").unwrap()).unwrap();
    assert!(index.contains(&crate::sync::local::hash_bytes(&prose(2048, 0))), "nothing went up");

    a.session.answer_burst().unwrap();
    let (states, _) = a.pass();
    assert!(states.burst.is_none(), "{:?}", states.burst);
    let index = String::from_utf8(store.read("Documents/.azlin/index.json").unwrap()).unwrap();
    assert!(index.contains(&crate::sync::local::hash_bytes(&noise(2048, 1))), "sent");
}

/// A folder emptied here (an unmounted disk?): the pass asks before it deletes most of the
/// drive's files; "keep them" leaves them on the drive (in the cloud only here), "delete them on
/// the drive too" deletes them with the next pass.
#[test]
fn a_mass_delete_waits_for_the_user_and_each_answer_does_what_it_says() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    for i in 0..14 {
        a.write(&format!("keep/{i}.txt"), format!("keep {i}").as_bytes());
    }
    a.pass();
    for i in 0..14 {
        fs::remove_file(path_of(a.folder.path(), &format!("keep/{i}.txt"))).unwrap();
    }
    let (states, _) = a.pass();
    let asked = states.mass_delete.clone().expect("asked first");
    assert!(!asked.here, "deleted here, the drive would lose them");
    assert_eq!((asked.count, asked.keys.len()), (14, 14));
    let index = String::from_utf8(store.read("Documents/.azlin/index.json").unwrap()).unwrap();
    assert!(index.contains("keep/0.txt"), "nothing deleted on the drive");

    a.session.answer_mass_delete(false).unwrap();
    let (states, _) = a.pass();
    assert!(states.mass_delete.is_none());
    assert_eq!(states.state_of("keep/0.txt"), Some(FileState::CloudOnly), "kept on the drive");

    // Again, and this time they go.
    let store = Arc::new(S3Bucket::new());
    let b = paired("dev-b", &store, |_| {});
    for i in 0..14 {
        b.write(&format!("gone/{i}.txt"), format!("gone {i}").as_bytes());
    }
    b.pass();
    for i in 0..14 {
        fs::remove_file(path_of(b.folder.path(), &format!("gone/{i}.txt"))).unwrap();
    }
    let (states, _) = b.pass();
    assert!(states.mass_delete.is_some());
    b.session.answer_mass_delete(true).unwrap();
    let (states, _) = b.pass();
    assert!(states.mass_delete.is_none());
    let index = index_of(&store);
    assert!(!lists(&index, "gone/0.txt"), "{index:#}");
}

/// The drive deleted most of this folder's files elsewhere, and the user confirmed it on the
/// device that deleted them - one of the user's own devices: the confirmation travels in the
/// tombstones, so this device moves its copies into its trash without asking again (D1, D2). A
/// tombstone of any other writer is only a word anyone with access to the drive can write: it
/// asks (AZ-S7, below).
#[test]
fn a_mass_delete_confirmed_on_one_device_goes_into_the_others_trash_without_asking_again() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let mut b = paired("dev-b", &store, |_| {});
    b.session = b.session.with_own_devices(&[String::from("dev-a")]);
    for i in 0..14 {
        a.write(&format!("{i}.txt"), format!("file {i}").as_bytes());
    }
    a.pass();
    b.pass();
    for i in 0..14 {
        fs::remove_file(path_of(a.folder.path(), &format!("{i}.txt"))).unwrap();
    }
    let (states, _) = a.pass();
    assert!(states.mass_delete.is_some(), "a asks first");
    a.session.answer_mass_delete(true).unwrap();
    a.pass();
    let (states, _) = b.pass();
    assert!(states.mass_delete.is_none(), "confirmed once, on the device that deleted them");
    assert!(b.read("0.txt").is_none());
    let trash = b.folder.path().join(".azlin").join("sync").join("trash");
    let mut trashed = Vec::new();
    for pass in fs::read_dir(&trash).unwrap().flatten() {
        for file in fs::read_dir(pass.path()).unwrap().flatten() {
            trashed.push(fs::read(file.path()).unwrap());
        }
    }
    assert_eq!(trashed.len(), 14, "every copy in b's trash");
    assert!(trashed.contains(&b"file 0".to_vec()));
}

/// A drive whose index uses a feature this version does not know: the pass changes nothing and
/// the states say what it does not know.
#[test]
fn a_newer_drive_format_is_kept_in_the_states() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    a.write("a.txt", b"a");
    a.pass();
    let key = "Documents/.azlin/index.json";
    let mut index: serde_json::Value =
        serde_json::from_slice(&store.read(key).unwrap()).unwrap();
    index["features"] = serde_json::json!(["teleport"]);
    store.write(key, serde_json::to_vec(&index).unwrap());
    let (states, _) = a.pass();
    assert_eq!(states.newer_format, vec![String::from("teleport")]);
}

/// A rename through the session (a plain drive's own listing): the copy here moves - a file in
/// the cloud only comes down first - and the next pass moves it on the drive.
#[test]
fn a_rename_through_the_session_moves_the_file_with_the_next_pass() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| s.auto_download = AutoDownload::Nothing);
    a.write("old.txt", b"the text");
    a.write("folder/x.txt", b"x");
    a.pass();
    b.pass();
    b.session.rename("old.txt", "new.txt").unwrap();
    assert_eq!(b.read("new.txt").as_deref(), Some(&b"the text"[..]), "came down, moved");
    b.session.rename("folder/", "renamed/").unwrap();
    assert_eq!(b.read("renamed/x.txt").as_deref(), Some(&b"x"[..]));
    b.pass();
    let index = index_of(&store);
    assert!(lists(&index, "new.txt") && !lists(&index, "old.txt"), "{index:#}");
    assert!(lists(&index, "renamed/x.txt"), "{index:#}");
}

#[cfg(feature = "encryption")]
mod encrypted_copies {
    use std::sync::{atomic::AtomicBool, Arc};

    use azul_storage::{
        crypto::DriveKey,
        encrypted::{EncryptedDrive, MemoryIndex},
        testing::TempDir,
        Drive,
    };

    use super::super::{fake_s3::FakeS3, sync_named::s3_drive};
    use crate::sync::{
        objects::ObjectCache,
        session::{AutoDownload, FileState, LocalCopies, SyncSession, SyncSetup},
    };

    #[test]
    fn an_encrypted_drives_local_copies_can_stay_encrypted_and_open_decrypted() {
        let bucket = FakeS3::new();
        let plain: Arc<dyn Drive> = s3_drive(&bucket);
        let cache_dir = TempDir::new("azcloud-objects");
        let objects = Arc::new(ObjectCache::new(plain, cache_dir.path().to_path_buf()));
        let below: Arc<dyn Drive> = objects.clone();
        let drive: Arc<dyn Drive> = Arc::new(EncryptedDrive::new(
            below,
            DriveKey::generate().unwrap(),
            Arc::new(MemoryIndex::new()),
        ));
        let secret = b"the diary of a very private person";
        drive.put("Documents/diary.txt", secret).unwrap();
        drive.put("Documents/huge.bin", &vec![1u8; 2 * 1024 * 1024]).unwrap();

        let folder = TempDir::new("azcloud-objects-folder");
        let state = TempDir::new("azcloud-objects-state");
        let mut setup = SyncSetup::new("d_test", "Documents/", folder.path());
        setup.local_copies = LocalCopies::Encrypted;
        setup.auto_download = AutoDownload::NewUnder(1);
        let session = SyncSession::encrypted_copies(
            setup,
            state.path().to_path_buf(),
            "dev-a",
            drive.clone(),
            objects.clone(),
        );
        let pass = session.pass(&AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(pass.states.state_of("diary.txt"), Some(FileState::OnDeviceEncrypted));
        assert_eq!(pass.states.state_of("huge.bin"), Some(FileState::CloudOnly));
        assert!(
            std::fs::read_dir(folder.path()).map_or(true, |mut d| d.next().is_none()),
            "no plaintext copy in the folder"
        );
        assert_eq!(objects.cached().len(), 1, "one AZL1 object kept");
        for key in objects.cached() {
            let stored = std::fs::read(objects.path_of(&key)).unwrap();
            assert!(!stored.windows(secret.len()).any(|w| w == secret), "ciphertext only");
        }
        bucket.clear_log();
        let path = session.open("diary.txt").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), secret);
        assert_eq!(bucket.count("GET"), 0, "decrypted from the copy on this device");
        let freed = session.free_up(&[String::from("diary.txt")]).unwrap();
        assert_eq!(freed.freed, vec![String::from("diary.txt")]);
        assert!(objects.cached().is_empty());
        assert_eq!(
            session.states().state_of("diary.txt"),
            Some(FileState::CloudOnly)
        );
    }

    /// Small files an encrypted drive packed together: the copy kept here is their pack, and
    /// each of them opens from it without a download.
    #[test]
    fn a_packed_files_local_copy_is_its_pack_and_opens_without_a_download() {
        use azul_storage::{packing::PackOptions, BatchPut};
        let bucket = FakeS3::new();
        let plain: Arc<dyn Drive> = s3_drive(&bucket);
        let cache_dir = TempDir::new("azcloud-objects-packed");
        let objects = Arc::new(ObjectCache::new(plain, cache_dir.path().to_path_buf()));
        let below: Arc<dyn Drive> = objects.clone();
        let drive: Arc<dyn Drive> = Arc::new(
            EncryptedDrive::new(below, DriveKey::generate().unwrap(), Arc::new(MemoryIndex::new()))
                .with_packing(PackOptions::default()),
        );
        let secret = b"the diary of a very private person";
        let items = [
            BatchPut::new("Documents/diary.txt", secret),
            BatchPut::new("Documents/list.txt", b"milk, bread, stamps"),
        ];
        for result in drive.put_many(&items) {
            result.unwrap();
        }
        let folder = TempDir::new("azcloud-objects-packed-folder");
        let state = TempDir::new("azcloud-objects-packed-state");
        let mut setup = SyncSetup::new("d_test", "Documents/", folder.path());
        setup.local_copies = LocalCopies::Encrypted;
        setup.auto_download = AutoDownload::NewUnder(1);
        let session = SyncSession::encrypted_copies(
            setup,
            state.path().to_path_buf(),
            "dev-a",
            drive.clone(),
            objects.clone(),
        );
        let pass = session.pass(&AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(pass.states.state_of("diary.txt"), Some(FileState::OnDeviceEncrypted));
        assert_eq!(pass.states.state_of("list.txt"), Some(FileState::OnDeviceEncrypted));
        assert_eq!(objects.cached().len(), 1, "one pack kept for both files");
        bucket.clear_log();
        let path = session.open("diary.txt").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), secret);
        let path = session.open("list.txt").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"milk, bread, stamps");
        assert_eq!(bucket.count("GET"), 0, "decrypted from the pack on this device");
    }
}

/// Deleting through the session (a synced folder's file kept in the cloud only; a file of a
/// plain drive's own listing, which shows the sync's names): the next pass deletes it on the
/// drive - and here, for a file that was here.
#[test]
fn a_file_deleted_through_the_session_goes_from_the_drive_with_the_next_pass() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| s.auto_download = AutoDownload::Nothing);
    a.write("cloud.txt", b"only in the cloud for b");
    a.write("here.txt", b"on a");
    a.pass();
    let (states, _) = b.pass();
    assert_eq!(states.state_of("cloud.txt"), Some(FileState::CloudOnly));
    let deleted = b.session.delete(&[String::from("cloud.txt")]).unwrap();
    assert_eq!(deleted, vec![String::from("cloud.txt")]);
    assert_eq!(b.session.states().state_of("cloud.txt"), None, "gone from the states at once");
    b.pass();
    let index: serde_json::Value =
        serde_json::from_slice(&store.read("Documents/.azlin/index.json").unwrap()).unwrap();
    assert!(index["files"].get("cloud.txt").is_none(), "{index:#}");
    assert!(index["deleted"].get("cloud.txt").is_some(), "its tombstone: {index:#}");
    let (states, _) = a.pass();
    assert!(a.read("cloud.txt").is_none(), "the other device deletes it too");
    assert_eq!(states.state_of("cloud.txt"), None);

    let deleted = a.session.delete(&[String::from("here.txt")]).unwrap();
    assert_eq!(deleted, vec![String::from("here.txt")]);
    assert!(a.read("here.txt").is_none(), "its copy here goes at once");
    a.pass();
    let index = index_of(&store);
    assert!(!lists(&index, "here.txt"), "{index:#}");
}

/// Opening a file that is on this device already marks it used: the size cap frees it last.
#[test]
fn touching_a_file_marks_it_used_now() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    a.write("old.txt", b"old");
    a.pass();
    let before = a.session.states().files["old.txt"].last_used;
    a.session.touch("old.txt").unwrap();
    let after = a.session.states().files["old.txt"].last_used;
    assert!(after >= crate::now() - 60 && after >= before, "{before} -> {after}");
    assert!(a.session.touch("missing.txt").is_ok(), "a file the drive does not have: nothing");
}

/// The drive's sync index of `Documents/`, parsed.
fn index_of(store: &S3Bucket) -> serde_json::Value {
    serde_json::from_slice(&store.read("Documents/.azlin/index.json").unwrap()).unwrap()
}

/// Whether the index lists `key` as a live file (a deleted one keeps a tombstone under
/// `deleted`, so a plain text search finds its name there).
fn lists(index: &serde_json::Value, key: &str) -> bool {
    index["files"].get(key).is_some()
}

// ==== Fewer transfers at once (a weak computer) ====

#[test]
fn a_session_moves_as_many_small_files_at_once_as_the_app_says() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-gentle", &store, |_| {});
    assert_eq!(a.session.parallel(), 4, "the run's default");
    let gentle = |transfers: usize| {
        let remote: Arc<dyn RemoteStore> = store.clone();
        SyncSession::plain(
            a.session.setup().clone(),
            a.state.path().to_path_buf(),
            "dev-gentle",
            remote,
        )
        .with_parallel(transfers)
    };
    assert_eq!(gentle(1).parallel(), 1);
    assert_eq!(gentle(2).parallel(), 2);
    assert_eq!(gentle(0).parallel(), 1, "at least one");
    for i in 0..5 {
        a.write(&format!("note-{i}.txt"), b"small");
    }
    let pass = pass_of(&gentle(1));
    assert_eq!(
        pass.report.as_ref().map(|r| r.files_up),
        Some(5),
        "one at a time, all of them"
    );
}

// ==== A limit for big transfers (a metered network) ====

/// `p`'s pairing in a session whose passes hold transfers over `limit` bytes back.
fn limited(p: &Paired, name: &str, store: &Arc<S3Bucket>, limit: u64) -> SyncSession {
    let remote: Arc<dyn RemoteStore> = store.clone();
    SyncSession::plain(
        p.session.setup().clone(),
        p.state.path().to_path_buf(),
        name,
        remote,
    )
    .with_transfer_limit(Some(limit))
}

fn pass_of(session: &SyncSession) -> Pass {
    let stop = AtomicBool::new(false);
    session.pass(&stop, &mut |_| {}).unwrap()
}

/// A file of `MB + 1` bytes: over a 1 MB limit.
fn big(fill: u8) -> Vec<u8> {
    vec![fill; (MB + 1) as usize]
}

#[test]
fn a_limited_pass_sends_small_files_and_keeps_big_ones_here_for_a_free_pass() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    a.write("notes.txt", b"small");
    a.write("video.mp4", &big(7));
    let pass = pass_of(&limited(&a, "dev-a", &store, MB));
    assert_eq!(pass.held, vec![String::from("video.mp4")]);
    assert!(pass.states.files.contains_key("notes.txt"), "the small file went up");
    assert!(!pass.states.files.contains_key("video.mp4"), "the big one did not");
    assert!(lists(&index_of(&store), "notes.txt"));
    assert!(!lists(&index_of(&store), "video.mp4"));
    // A change of the small file goes up on the limited network too.
    a.write("notes.txt", b"small, changed");
    let pass = pass_of(&limited(&a, "dev-a", &store, MB));
    assert_eq!(pass.held, vec![String::from("video.mp4")], "still waiting");
    assert_eq!(pass.report.as_ref().map(|r| r.files_up), Some(1));
    // A free network: the next pass sends it.
    let (states, _) = a.pass();
    assert_eq!(states.state_of("video.mp4"), Some(FileState::OnDevice));
    assert!(lists(&index_of(&store), "video.mp4"));
}

#[test]
fn a_limited_pass_brings_small_files_down_and_leaves_big_ones_on_the_drive_for_a_free_pass() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| s.auto_download = AutoDownload::Everything);
    a.write("notes.txt", b"small");
    a.write("video.mp4", &big(7));
    a.pass();
    let pass = pass_of(&limited(&b, "dev-b", &store, MB));
    assert_eq!(pass.held, vec![String::from("video.mp4")]);
    assert_eq!(b.read("notes.txt").as_deref(), Some(&b"small"[..]));
    assert!(b.read("video.mp4").is_none(), "held back on the drive");
    assert_eq!(
        pass.states.state_of("video.mp4"),
        Some(FileState::CloudOnly),
        "its row says it is not here yet"
    );
    let (states, _) = b.pass();
    assert_eq!(b.read("video.mp4"), Some(big(7)), "a free pass brings it");
    assert_eq!(states.state_of("video.mp4"), Some(FileState::OnDevice));
    // A new version of a big file on the drive waits there too; this device keeps the one it
    // has, and sends nothing of it back.
    a.write("video.mp4", &big(8));
    a.pass();
    let pass = pass_of(&limited(&b, "dev-b", &store, MB));
    assert_eq!(pass.held, vec![String::from("video.mp4")]);
    assert_eq!(b.read("video.mp4"), Some(big(7)), "the old version stays");
    let (states, _) = b.pass();
    assert_eq!(b.read("video.mp4"), Some(big(8)));
    assert_eq!(states.state_of("video.mp4"), Some(FileState::OnDevice));
    a.pass();
    assert_eq!(a.read("video.mp4"), Some(big(8)), "the old version never went back up");
}

#[test]
fn a_limited_pass_leaves_a_big_pinned_file_in_the_cloud_for_a_free_pass() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| s.auto_download = AutoDownload::Nothing);
    a.write("video.mp4", &big(7));
    a.write("small.txt", b"small");
    a.pass();
    b.pass();
    b.session
        .pin(&[String::from("video.mp4"), String::from("small.txt")], true)
        .unwrap();
    let pass = pass_of(&limited(&b, "dev-b", &store, MB));
    assert_eq!(pass.held, vec![String::from("video.mp4")]);
    assert_eq!(pass.fetched, vec![String::from("small.txt")], "the small pin came");
    assert!(b.read("video.mp4").is_none());
    let pass = pass_of(&b.session);
    assert!(pass.held.is_empty());
    assert_eq!(pass.fetched, vec![String::from("video.mp4")]);
    assert_eq!(b.read("video.mp4"), Some(big(7)));
}

/// AZ-S11. A rename is the new name's upload and the old name's delete. The limit held the
/// upload and let the delete through: the drive had neither name until a free network, and the
/// other devices trashed their copy. RED not run: the old code deletes `video.mp4` on the drive
/// in the limited pass, so `lists(&index, "video.mp4")` fails.
#[test]
fn az_s11_a_limited_pass_holds_a_renamed_big_file_whole_the_old_name_stays_on_the_drive() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    a.write("video.mp4", &big(7));
    a.pass();
    fs::rename(
        path_of(a.folder.path(), "video.mp4"),
        path_of(a.folder.path(), "holiday.mp4"),
    )
    .unwrap();
    let pass = pass_of(&limited(&a, "dev-a", &store, MB));
    assert_eq!(pass.held, vec![String::from("holiday.mp4")]);
    let index = index_of(&store);
    assert!(
        lists(&index, "video.mp4"),
        "the old name stays on the drive"
    );
    assert!(!lists(&index, "holiday.mp4"), "the new name waits");
    assert!(
        index["deleted"].get("video.mp4").is_none(),
        "nothing is deleted on the drive: {index}"
    );
    // A free network: the rename goes through whole.
    a.pass();
    let index = index_of(&store);
    assert!(lists(&index, "holiday.mp4"));
    assert!(!lists(&index, "video.mp4"));
}

/// AZ-S11, the other side: the drive renamed a big file. That moves no bytes here - the old name
/// is renamed - so the limit has nothing to hold; the old code held the "download" of the new
/// name and trashed the old one: this folder had neither. RED not run: the old code trashes
/// `video.mp4` and `holiday.mp4` is not there.
#[test]
fn az_s11_a_limited_pass_renames_here_what_the_drive_renamed_it_is_no_transfer() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |s| {
        s.auto_download = AutoDownload::Everything
    });
    a.write("video.mp4", &big(7));
    a.pass();
    b.pass();
    assert_eq!(b.read("video.mp4"), Some(big(7)));
    fs::rename(
        path_of(a.folder.path(), "video.mp4"),
        path_of(a.folder.path(), "holiday.mp4"),
    )
    .unwrap();
    a.pass();
    let pass = pass_of(&limited(&b, "dev-b", &store, MB));
    assert!(pass.held.is_empty(), "no bytes move: {:?}", pass.held);
    assert!(b.read("video.mp4").is_none(), "the old name is gone here");
    assert_eq!(b.read("holiday.mp4"), Some(big(7)), "renamed, not fetched");
}

#[test]
fn a_pairing_kept_before_the_metered_setting_reads_it_as_off() {
    let folder = TempDir::new("azcloud-session-metered-setting");
    let mut setup = setup_of(&folder);
    assert!(!setup.sync_on_metered, "big files wait on a metered network by default");
    let text = serde_json::to_string(&setup).unwrap();
    let older = text.replace(",\"sync_on_metered\":false", "");
    assert!(!older.contains("sync_on_metered"), "{older}");
    assert_eq!(serde_json::from_str::<SyncSetup>(&older).unwrap(), setup);
    setup.sync_on_metered = true;
    let text = serde_json::to_string(&setup).unwrap();
    assert_eq!(serde_json::from_str::<SyncSetup>(&text).unwrap(), setup);
}

// ==== Free up space, backups, on-demand switched off ====

#[test]
fn free_up_space_keeps_a_file_whose_copy_on_the_drive_is_damaged_or_not_there_yet() {
    use crate::sync::problems::Code;
    let store = Arc::new(S3Bucket::new());
    let a = paired("evict-checked", &store, |_| {});
    a.write("good.txt", b"good bytes");
    a.write("bad.txt", b"bad bytes");
    a.pass();
    let blob = crate::sync::remote::blob_key(
        "Documents/",
        &crate::sync::local::hash_bytes(b"bad bytes"),
    );
    RemoteStore::put(store.as_ref(), &blob, b"rotten").unwrap();
    a.write("later.txt", b"not synced yet");
    let keys = [
        String::from("good.txt"),
        String::from("bad.txt"),
        String::from("later.txt"),
    ];
    let freed = a.session.free_up(&keys).unwrap();
    assert_eq!(freed.freed, vec![String::from("good.txt")]);
    assert_eq!(freed.kept.len(), 2, "{:?}", freed.kept);
    assert!(a.read("good.txt").is_none());
    assert_eq!(a.read("bad.txt").as_deref(), Some(&b"bad bytes"[..]), "the only good copy stays");
    assert!(a.read("later.txt").is_some());
    let problems = a.session.problems();
    assert!(
        problems
            .problems
            .iter()
            .any(|p| p.key == "bad.txt" && p.code == Code::HashMismatch),
        "{:?}",
        problems.problems
    );
}

#[test]
fn a_backup_pairing_keeps_the_drives_copy_of_a_file_deleted_here_and_never_deletes_here() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("backup-a", &store, |s| s.backup = true);
    let b = paired("backup-b", &store, |_| {});
    a.write("photo.jpg", b"a photo");
    a.write("notes.txt", b"notes");
    a.pass();
    b.pass();
    // Gone here: the drive keeps it, this device shows it in the cloud only.
    fs::remove_file(path_of(a.folder.path(), "photo.jpg")).unwrap();
    let (states, _) = a.pass();
    assert!(lists(&index_of(&store), "photo.jpg"), "a backup deletes nothing on the drive");
    assert_eq!(states.state_of("photo.jpg"), Some(FileState::CloudOnly));
    // Gone on the drive (another device deleted it): the source keeps it and sends it again.
    fs::remove_file(path_of(b.folder.path(), "notes.txt")).unwrap();
    b.pass();
    assert!(!lists(&index_of(&store), "notes.txt"));
    a.pass();
    assert_eq!(a.read("notes.txt").as_deref(), Some(&b"notes"[..]));
    assert!(lists(&index_of(&store), "notes.txt"), "sent again from the source");
    // Changed on the drive: the source's version wins and nothing here is overwritten.
    b.pass();
    b.write("notes.txt", b"changed elsewhere");
    b.pass();
    a.pass();
    assert_eq!(a.read("notes.txt").as_deref(), Some(&b"notes"[..]));
}

#[test]
fn a_pairing_that_downloads_everything_brings_cloud_only_files_down_but_not_freed_ones() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("everything-a", &store, |_| {});
    let b = paired("everything-b", &store, |s| s.auto_download = AutoDownload::Nothing);
    a.write("one.txt", b"one");
    a.write("two.txt", b"two");
    a.pass();
    let (states, _) = b.pass();
    assert_eq!(states.state_of("one.txt"), Some(FileState::CloudOnly));
    // On-demand switched off: every file comes down.
    let mut setup = b.session.setup().clone();
    setup.auto_download = AutoDownload::Everything;
    let b = Paired {
        session: SyncSession::plain(
            setup,
            b.state.path().to_path_buf(),
            "everything-b",
            store.clone() as Arc<dyn RemoteStore>,
        ),
        ..b
    };
    let (states, _) = b.pass();
    assert_eq!(b.read("one.txt").as_deref(), Some(&b"one"[..]));
    assert_eq!(states.state_of("two.txt"), Some(FileState::OnDevice));
    // A file the user freed up stays in the cloud.
    b.session.free_up(&[String::from("one.txt")]).unwrap();
    let (states, _) = b.pass();
    assert!(b.read("one.txt").is_none());
    assert_eq!(states.state_of("one.txt"), Some(FileState::CloudOnly));
}

#[test]
fn the_read_back_heals_a_damaged_drive_copy_and_puts_back_a_rotted_one_here() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("verify-a", &store, |_| {});
    a.write("a.bin", b"archive a");
    a.write("b.bin", b"archive b");
    a.pass();
    let blob = |text: &[u8]| {
        crate::sync::remote::blob_key("Documents/", &crate::sync::local::hash_bytes(text))
    };
    RemoteStore::put(store.as_ref(), &blob(b"archive a"), b"rotten").unwrap();
    let report = a.session.verify().unwrap();
    assert_eq!(report.checked, 2);
    assert_eq!(report.mismatches, vec![String::from("a.bin")]);
    assert_eq!(report.healed, vec![String::from("a.bin")], "sent again from the good copy here");
    assert_eq!(store.read(&blob(b"archive a")).as_deref(), Some(&b"archive a"[..]));
    // This device's copy rots: same size, same time, other bytes.
    let path = path_of(a.folder.path(), "b.bin");
    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, b"archive X").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let report = a.session.verify().unwrap();
    assert_eq!(report.rotted, vec![String::from("b.bin")]);
    assert_eq!(report.restored, vec![String::from("b.bin")]);
    assert_eq!(a.read("b.bin").as_deref(), Some(&b"archive b"[..]), "the synced version is back");
    assert!(a
        .session
        .problems()
        .problems
        .iter()
        .any(|p| p.key == "b.bin"));
    // The next pass sends nothing: the folder is as the drive has it.
    a.pass();
    assert_eq!(index_of(&store)["files"]["b.bin"]["hash"], crate::sync::local::hash_bytes(b"archive b"));
}

/// The repair stages the drive's version whole beside the rotted file and swaps the two
/// (`durable::replace_keeping`): the rotted bytes are in the trash, the good ones in place, and
/// no staged file or temporary file is left in the folder.
#[test]
fn the_repair_swaps_in_the_staged_version_and_keeps_the_rotted_bytes_in_the_trash() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("verify-swap", &store, |_| {});
    a.write("docs/b.bin", b"archive b");
    a.pass();
    let path = path_of(a.folder.path(), "docs/b.bin");
    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, b"archive X").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let report = a.session.verify().unwrap();
    assert_eq!(
        report.restored,
        vec![String::from("docs/b.bin")],
        "{report:?}"
    );
    assert_eq!(a.read("docs/b.bin").as_deref(), Some(&b"archive b"[..]));
    let trash = a.folder.path().join(".azlin").join("sync").join("trash");
    let mut trashed = Vec::new();
    let mut stack = vec![trash];
    while let Some(folder) = stack.pop() {
        for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
            if entry.path().is_dir() {
                stack.push(entry.path());
            } else {
                trashed.push(entry.path());
            }
        }
    }
    assert_eq!(trashed.len(), 1, "{trashed:?}");
    assert_eq!(
        fs::read(&trashed[0]).unwrap(),
        b"archive X",
        "the rotted bytes are kept"
    );
    let left: Vec<String> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        left,
        vec![String::from("b.bin")],
        "no staged or temporary file: {left:?}"
    );
}

/// A store whose reads stop answering after a number of fetches: a connection that drops, or a
/// disk that fills, in the middle of a repair.
struct DropsAfter {
    inner: Arc<S3Bucket>,
    fetches_left: AtomicUsize,
}

impl DropsAfter {
    /// Whether this fetch is one that still gets an answer.
    fn answers(&self) -> bool {
        self.fetches_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
    }
}

impl RemoteStore for DropsAfter {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.inner.get_unless(key, etag)
    }
    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        if !self.answers() {
            return Err(CloudError::failed("connection reset by peer"));
        }
        self.inner.fetch(key, size)
    }
    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        self.inner.put(key, data)
    }
    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        self.inner.put_if(key, data, if_match)
    }
    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        self.inner.head(key)
    }
    fn delete(&self, key: &str) -> CloudResult<()> {
        self.inner.delete(key)
    }
    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        self.inner.list(prefix)
    }
}

/// AZ-S4. The read-back put the drive's version over a rotted file by moving the rotted file to
/// the trash FIRST and fetching after. A fetch that failed then left the folder without the
/// file; the next pass read "gone here, unchanged on the drive" as a delete, wrote a tombstone
/// and trashed the good copy on every device. The replacement is fetched and checked first.
#[test]
fn az_s4_a_rotted_file_stays_where_it_is_when_the_replacement_cannot_be_fetched() {
    let store = Arc::new(S3Bucket::new());
    let drops = Arc::new(DropsAfter {
        inner: store.clone(),
        fetches_left: AtomicUsize::new(usize::MAX),
    });
    let folder = TempDir::new("azcloud-session-verify-s4");
    let state = TempDir::new("azcloud-session-state-verify-s4");
    let remote: Arc<dyn RemoteStore> = drops.clone();
    let a = Paired {
        session: SyncSession::plain(
            setup_of(&folder),
            state.path().to_path_buf(),
            "verify-s4",
            remote,
        ),
        folder,
        state,
    };
    a.write("b.bin", b"archive b");
    a.pass();
    // This device's copy rots: same size, same time, other bytes.
    let path = path_of(a.folder.path(), "b.bin");
    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, b"archive X").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    // The read-back reads the drive's copy once (whole), then the repair's fetch gets no answer.
    drops.fetches_left.store(1, Ordering::SeqCst);
    let report = a.session.verify().unwrap();
    assert_eq!(report.rotted, vec![String::from("b.bin")], "{report:?}");
    assert!(report.restored.is_empty(), "the fetch dropped: {report:?}");
    assert_eq!(
        a.read("b.bin").as_deref(),
        Some(&b"archive X"[..]),
        "the file is where it was: not moved to the trash before its replacement was here"
    );
    // The next pass finds nothing gone: no tombstone on the drive, no delete for the others.
    drops.fetches_left.store(usize::MAX, Ordering::SeqCst);
    a.pass();
    assert!(
        lists(&index_of(&store), "b.bin"),
        "the drive still lists the file: {}",
        index_of(&store)
    );
    assert!(a.read("b.bin").is_some());
}

// ==== The guards' answers are for the question they were given to (AZ-S7, AZ-S8, AZ-S9) ====

/// AZ-S7. The breaker took a tombstone's `confirmed` as the user's word whoever wrote it: a share
/// participant (or a buggy device) that deletes most of a folder and writes `confirmed: true`
/// had every other device move its copies into the trash without a question. A tombstone counts
/// as confirmed only when this device or one of the user's own devices wrote it.
#[test]
fn az_s7_a_confirmed_tombstone_of_a_stranger_is_counted_by_the_breaker() {
    let store = Arc::new(S3Bucket::new());
    let stranger = paired("someone-else", &store, |_| {});
    let b = paired("dev-b", &store, |_| {});
    for i in 0..14 {
        stranger.write(&format!("{i}.txt"), format!("file {i}").as_bytes());
    }
    stranger.pass();
    b.pass();
    for i in 0..14 {
        fs::remove_file(path_of(stranger.folder.path(), &format!("{i}.txt"))).unwrap();
    }
    stranger.pass();
    stranger.session.answer_mass_delete(true).unwrap();
    stranger.pass();
    let index = index_of(&store);
    assert!(
        index["deleted"]
            .as_object()
            .unwrap()
            .values()
            .all(|t| t["confirmed"] == true),
        "the stranger's tombstones say the user confirmed: {index:#}"
    );
    let (states, _) = b.pass();
    assert!(
        states.mass_delete.is_some(),
        "b asks: the word of a stranger is not enough"
    );
    assert_eq!(
        b.read("0.txt").as_deref(),
        Some(&b"file 0"[..]),
        "nothing was moved to the trash"
    );
}

/// AZ-S8. "Delete them too" was a flag in the states that the next pass read whatever it
/// planned: after the answer, anything else the folder lost (an unmounted disk, a second
/// accident) was deleted on the drive without a question. The answer is for the keys that were
/// asked about; a plan that deletes another set asks again, and a pass that reached its end has
/// used the answer up.
#[test]
fn az_s8_delete_them_too_is_for_the_files_it_was_asked_about() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    for i in 0..14 {
        a.write(&format!("old/{i}.txt"), format!("old {i}").as_bytes());
        a.write(&format!("more/{i}.txt"), format!("more {i}").as_bytes());
    }
    a.pass();
    for i in 0..14 {
        fs::remove_file(path_of(a.folder.path(), &format!("old/{i}.txt"))).unwrap();
    }
    // 19 of the 28 files: a mass delete. The rest of the second batch goes only after the answer.
    for i in 0..5 {
        fs::remove_file(path_of(a.folder.path(), &format!("more/{i}.txt"))).unwrap();
    }
    let (states, _) = a.pass();
    let asked = states.mass_delete.clone().expect("asked");
    assert_eq!(asked.keys.len(), 19, "{asked:?}");
    a.session.answer_mass_delete(true).unwrap();
    // Before the pass that would use it, the folder loses more: the answer was for 19 files.
    for i in 5..14 {
        fs::remove_file(path_of(a.folder.path(), &format!("more/{i}.txt"))).unwrap();
    }
    let (states, _) = a.pass();
    let again = states.mass_delete.clone().expect("a bigger set asks again");
    assert_eq!(again.keys.len(), 28, "{again:?}");
    let index = index_of(&store);
    assert!(
        lists(&index, "old/0.txt") && lists(&index, "more/13.txt"),
        "nothing went: {index:#}"
    );
    assert!(
        states.consents.is_empty(),
        "a pass that reached its end used the answer up: {:?}",
        states.consents
    );
    // The answer to the question asked now covers exactly it, and is used up by the pass that
    // sends the deletes.
    a.session.answer_mass_delete(true).unwrap();
    let (states, _) = a.pass();
    assert!(states.mass_delete.is_none());
    assert!(states.consents.is_empty(), "{:?}", states.consents);
    let index = index_of(&store);
    assert!(
        !lists(&index, "old/0.txt") && !lists(&index, "more/13.txt"),
        "{index:#}"
    );
}

/// AZ-S8. A pass that failed before it used the answer leaves it where it was - and it still
/// covers only the files it was given for.
#[test]
fn az_s8_a_pass_that_fails_keeps_the_answer_for_the_next_one() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    for i in 0..14 {
        a.write(&format!("{i}.txt"), format!("file {i}").as_bytes());
    }
    a.pass();
    for i in 0..14 {
        fs::remove_file(path_of(a.folder.path(), &format!("{i}.txt"))).unwrap();
    }
    let (states, _) = a.pass();
    assert!(states.mass_delete.is_some());
    a.session.answer_mass_delete(true).unwrap();
    // The folder is gone (an unplugged disk): the pass is refused and changes nothing.
    let held = a.folder.path().with_extension("away");
    fs::rename(a.folder.path(), &held).unwrap();
    assert!(a
        .session
        .pass(&AtomicBool::new(false), &mut |_| {})
        .is_err());
    assert_eq!(a.session.states().consents.len(), 1, "the answer waits");
    fs::rename(&held, a.folder.path()).unwrap();
    let (states, _) = a.pass();
    assert!(
        states.mass_delete.is_none() && states.consents.is_empty(),
        "{states:?}"
    );
}

/// AZ-S9. One question can hold both directions: files gone from this folder (the drive would
/// lose them) and files the drive deleted elsewhere that this folder still holds. "Keep" marked
/// them all by the majority's direction, and the minority stayed planned for the delete: the
/// next pass moved them into the trash although the user had said keep.
#[test]
fn az_s9_keep_on_a_mixed_direction_mass_delete_keeps_the_minority_files_too() {
    let store = Arc::new(S3Bucket::new());
    let a = paired("dev-a", &store, |_| {});
    let b = paired("dev-b", &store, |_| {});
    for i in 0..20 {
        a.write(&format!("f/{i}.txt"), format!("file {i}").as_bytes());
    }
    a.pass();
    b.pass();
    // b deletes ten files (the drive would lose them), a deletes four others elsewhere.
    for i in 0..10 {
        fs::remove_file(path_of(b.folder.path(), &format!("f/{i}.txt"))).unwrap();
    }
    for i in 10..14 {
        fs::remove_file(path_of(a.folder.path(), &format!("f/{i}.txt"))).unwrap();
    }
    a.pass();
    let (states, _) = b.pass();
    let asked = states
        .mass_delete
        .clone()
        .expect("14 of 20 in both directions");
    assert_eq!(asked.keys.len(), 14, "{asked:?}");
    assert!(!asked.here, "the majority is the files gone from here");
    b.session.answer_mass_delete(false).unwrap();
    let (states, _) = b.pass();
    assert!(states.mass_delete.is_none(), "{states:?}");
    for i in 10..14 {
        assert_eq!(
            b.read(&format!("f/{i}.txt")).as_deref(),
            Some(format!("file {i}").as_bytes()),
            "kept, not moved into the trash"
        );
    }
    for i in 0..10 {
        assert_eq!(
            states.state_of(&format!("f/{i}.txt")),
            Some(FileState::CloudOnly),
            "kept on the drive"
        );
    }
    let trash = b.folder.path().join(".azlin/sync/trash");
    let trashed = fs::read_dir(&trash)
        .map(|d| d.flatten().count())
        .unwrap_or(0);
    assert_eq!(trashed, 0, "nothing was trashed");
}
