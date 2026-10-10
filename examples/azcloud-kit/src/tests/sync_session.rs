//! An app's pairing of a folder with a drive ([`SyncSession`]): its settings, its files' states
//! kept between runs, a pass (the on-demand files, pinned files brought down, the conflicts
//! held for a choice), opening a cloud-only file, "Free up space", the size cap, a drive that
//! takes no writes; and an encrypted drive whose local copies stay encrypted ([`ObjectCache`]).

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use azul_storage::{testing::TempDir, ByteRange, Drive, DriveError};

use super::{
    fake_s3::{FakeS3, S3Bucket},
    sync_named::s3_drive,
};
use crate::{
    error::{CloudError, CloudResult},
    store::{Conditional, RemoteObject, RemoteStore},
    sync::{
        local::path_of,
        objects::ObjectCache,
        session::{
            is_read_only, AutoDownload, FileRecord, FileState, LocalCopies, Resolution,
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
}
