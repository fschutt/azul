//! What a hostile or broken server, network or disk does to a pass, as the engine names it: a
//! server that moved, a drive gone read-only, an object over the server's size cap, a file made
//! immutable, a folder that cannot be listed for a while, a crash inside a download's body, and
//! attributes that are not synced.

use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use azul_storage::{testing::TempDir, AzlinError, DriveError, ServiceError};

use super::fake_s3::S3Bucket;
use crate::{
    error::CloudError,
    store::RemoteStore,
    sync::{
        faults,
        problems::{code_of_error, of_failed_pass, Code, Severity},
        remote::{index_key, RemoteIndex},
        session::{SyncSession, SyncSetup},
    },
};

const PREFIX: &str = "hostile/";

/// A plain pairing of `folder` whose passes hear the fault seam under it.
fn session(folder: &Path, state: &Path, store: &Arc<S3Bucket>) -> SyncSession {
    let remote: Arc<dyn RemoteStore> = store.clone();
    let root = folder.to_path_buf();
    SyncSession::plain(
        SyncSetup::new("d_hostile", PREFIX, folder),
        state.to_path_buf(),
        "dev",
        remote,
    )
    .with_points(Arc::new(move |name: &str| faults::point(&root, name)))
}

fn remote_keys(store: &S3Bucket) -> Vec<String> {
    store
        .read(&index_key(PREFIX))
        .and_then(|bytes| RemoteIndex::parse(&bytes).ok())
        .map(|index| index.files.keys().cloned().collect())
        .unwrap_or_default()
}

fn service(status: u16, code: &str, azlin: Option<AzlinError>) -> CloudError {
    CloudError::Drive(DriveError::Service(ServiceError {
        status,
        code: code.to_string(),
        message: String::from("the service said no"),
        azlin_error: azlin,
        ..ServiceError::default()
    }))
}

#[test]
fn the_new_codes_are_written_as_the_sync_harness_spells_them() {
    assert_eq!(Code::ServerMoved.as_str(), "server-moved");
    assert_eq!(Code::ReadOnly.as_str(), "read-only");
    assert_eq!(Code::XattrsNotSynced.as_str(), "xattrs-not-synced");
    assert_eq!(Code::XattrsNotSynced.severity(), Severity::Warning);
    assert_eq!(Code::ServerMoved.severity(), Severity::Error);
    assert!(Code::ServerMoved.is_pass_wide());
    assert!(!Code::XattrsNotSynced.is_pass_wide());
}

#[test]
fn a_moved_server_is_its_own_problem_and_not_a_sign_in() {
    let moved = service(301, "PermanentRedirect", None);
    assert_eq!(code_of_error(&moved), Code::ServerMoved);
    assert_eq!(of_failed_pass(&moved).code, Code::ServerMoved);
    assert_eq!(code_of_error(&service(308, "PermanentRedirect", None)), Code::ServerMoved);
}

#[test]
fn a_read_only_drive_says_so_instead_of_asking_to_sign_in_again() {
    for azlin in [
        AzlinError::ReadOnlyUnpaid,
        AzlinError::ReadOnlyLockdown,
        AzlinError::ReadOnlyOperator,
        AzlinError::DriveBanned,
        // locked, its files in offline storage: signing in again changes nothing either
        AzlinError::DriveArchived,
    ] {
        let e = service(403, "AccessDenied", Some(azlin));
        assert_eq!(code_of_error(&e), Code::ReadOnly, "{e}");
    }
    // Revoked keys still ask to sign in.
    assert_eq!(code_of_error(&service(403, "AccessDenied", None)), Code::SignInNeeded);
}

#[test]
fn an_object_over_the_servers_cap_is_too_large_and_a_full_drive_is_quota() {
    assert_eq!(code_of_error(&service(413, "EntityTooLarge", None)), Code::TooLarge);
    assert_eq!(
        code_of_error(&service(400, "EntityTooLarge", Some(AzlinError::ObjectTooLarge))),
        Code::TooLarge
    );
    assert_eq!(
        code_of_error(&service(403, "QuotaExceeded", Some(AzlinError::QuotaExceeded))),
        Code::QuotaExceeded
    );
}

#[test]
fn an_immutable_file_or_folder_is_locked_not_an_io_error() {
    let e = CloudError::Io(String::from("rename a.txt: Operation not permitted (os error 1)"));
    assert_eq!(code_of_error(&e), Code::Locked);
    let e = CloudError::Io(String::from("Permission denied (os error 13)"));
    assert_eq!(code_of_error(&e), Code::Unreadable);
}

#[test]
fn a_folder_that_cannot_be_listed_for_one_scan_deletes_nothing_and_the_next_scan_takes_it_again() {
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-hostile-readdir");
    let state = TempDir::new("azcloud-hostile-readdir-state");
    fs::create_dir_all(folder.path().join("photos")).unwrap();
    for i in 0..3 {
        fs::write(folder.path().join(format!("photos/{i}.jpg")), format!("photo {i}")).unwrap();
    }
    let s = session(folder.path(), state.path(), &store);
    s.pass(&AtomicBool::new(false), &mut |_| {}).expect("the first pass");
    assert_eq!(remote_keys(&store).len(), 3);
    let eio = faults::errno_of("EIO").expect("EIO is known");
    faults::set_readdir_error(folder.path(), "photos", eio, 1);
    s.pass(&AtomicBool::new(false), &mut |_| {})
        .expect("a pass with one unreadable folder still runs");
    assert_eq!(remote_keys(&store).len(), 3, "nothing in the unreadable folder was deleted");
    let listed = s.problems();
    assert!(
        listed.problems.iter().any(|p| p.code == Code::Unreadable && p.key == "photos"),
        "{:?}",
        listed.problems
    );
    s.pass(&AtomicBool::new(false), &mut |_| {}).expect("the clean pass");
    assert!(!s.problems().problems.iter().any(|p| p.code == Code::Unreadable));
    assert_eq!(remote_keys(&store).len(), 3);
    faults::clear(folder.path());
}

#[test]
fn a_played_readdir_error_lasts_the_scans_it_was_set_for() {
    let root = Path::new("/hostile-test/readdir");
    faults::set_readdir_error(root, "a/b/", 5, 2);
    assert!(faults::readdir_error(root, "a/b").is_none(), "nothing before a scan starts");
    faults::scan_started(root);
    assert_eq!(faults::readdir_error(root, "a/b").and_then(|e| e.raw_os_error()), Some(5));
    assert!(faults::readdir_error(root, "a").is_none(), "only that folder");
    faults::scan_started(root);
    assert!(faults::readdir_error(root, "a/b").is_some(), "the second scan");
    faults::scan_started(root);
    assert!(faults::readdir_error(root, "a/b").is_none(), "gone after two scans");
    faults::clear(root);
}

/// One big object in memory, counting its ranged GETs.
struct BigObject {
    bytes: Vec<u8>,
    ranges: std::sync::atomic::AtomicUsize,
}

impl azul_storage::Drive for BigObject {
    fn list(&self, _: &azul_storage::ListRequest) -> Result<azul_storage::ListPage, DriveError> {
        Ok(azul_storage::ListPage {
            folders: Vec::new(),
            objects: Vec::new(),
            next: None,
        })
    }

    fn get(&self, _: &str) -> Result<Vec<u8>, DriveError> {
        Ok(self.bytes.clone())
    }

    fn get_range(&self, _: &str, range: azul_storage::ByteRange) -> Result<Vec<u8>, DriveError> {
        self.ranges.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let start = usize::try_from(range.start).unwrap_or(usize::MAX).min(self.bytes.len());
        let end = range
            .end
            .map_or(self.bytes.len(), |e| usize::try_from(e).unwrap_or(usize::MAX).saturating_add(1))
            .min(self.bytes.len());
        Ok(self.bytes[start..end.max(start)].to_vec())
    }

    fn put(&self, key: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(DriveError::Unsupported(format!("writing {key}")))
    }

    fn delete(&self, _: &str) -> Result<(), DriveError> {
        Ok(())
    }

    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
        Ok(azul_storage::ObjectInfo {
            key: key.to_string(),
            size: self.bytes.len() as u64,
            modified: Some(1),
            etag: Some(String::from("one-version")),
        })
    }
}

fn big_object() -> BigObject {
    // Two parts: one whole part and one byte.
    let len = usize::try_from(azul_storage::transfer::CHUNK).unwrap() + 1;
    BigObject {
        bytes: (0..len).map(|i| (i % 251) as u8).collect(),
        ranges: std::sync::atomic::AtomicUsize::new(0),
    }
}

#[test]
fn a_download_in_parts_tells_its_middle_once_and_completes() {
    let object = big_object();
    let dir = TempDir::new("azcloud-hostile-mid");
    let dest = dir.path().join("big.blob");
    let told = std::sync::atomic::AtomicUsize::new(0);
    let size = object.bytes.len() as u64;
    crate::sync::drive_store::download_told(&object, "big", size, &dest, &|| {
        told.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("the download");
    assert_eq!(told.load(std::sync::atomic::Ordering::SeqCst), 1, "the middle is told once");
    assert_eq!(fs::read(&dest).unwrap(), object.bytes);
}

#[test]
fn a_crash_in_the_middle_of_a_download_stops_it_and_the_next_one_resumes_from_the_parts_kept() {
    let object = big_object();
    let dir = TempDir::new("azcloud-hostile-mid-crash");
    let dest = dir.path().join("big.blob");
    let size = object.bytes.len() as u64;
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::sync::drive_store::download_told(&object, "big", size, &dest, &|| {
            std::panic::resume_unwind(Box::new(faults::Crashed {
                point: String::from("mid-download"),
            }))
        })
    }));
    let payload = crashed.expect_err("the download crashed in its middle");
    assert_eq!(faults::crashed_at(payload.as_ref()).as_deref(), Some("mid-download"));
    assert!(!dest.exists(), "a crashed download leaves no file under its name");
    crate::sync::drive_store::download_told(&object, "big", size, &dest, &|| {})
        .expect("the next download");
    assert_eq!(fs::read(&dest).unwrap(), object.bytes);
    assert_eq!(
        object.ranges.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "every part was fetched once: the second download resumed"
    );
}

#[test]
fn the_systems_own_attributes_are_no_users_and_a_users_are() {
    use crate::sync::attrs::is_system_attribute;
    assert!(is_system_attribute("com.apple.quarantine"));
    assert!(is_system_attribute("com.apple.provenance"));
    assert!(is_system_attribute("security.selinux"));
    assert!(!is_system_attribute("com.apple.FinderInfo"));
    assert!(!is_system_attribute("com.apple.metadata:kMDItemFinderComment"));
    assert!(!is_system_attribute("user.xdg.comment"));
}

#[test]
fn a_folder_whose_files_carry_attributes_is_noted_once_as_a_warning_and_its_content_syncs() {
    use crate::sync::attrs;
    let store = Arc::new(S3Bucket::new());
    let folder = TempDir::new("azcloud-hostile-xattr");
    let state = TempDir::new("azcloud-hostile-xattr-state");
    fs::create_dir_all(folder.path().join("tagged")).unwrap();
    fs::write(folder.path().join("tagged/paper.pdf"), "the paper").unwrap();
    fs::write(folder.path().join("tagged/notes.txt"), "notes").unwrap();
    fs::write(folder.path().join("plain.txt"), "plain").unwrap();
    let name = if cfg!(target_os = "macos") { "com.example.notes" } else { "user.notes" };
    for file in ["tagged/paper.pdf", "tagged/notes.txt"] {
        if let Err(e) = attrs::set_attribute(&folder.path().join(file), name, b"read before Friday") {
            // A disk (or a system) without extended attributes has nothing to note.
            eprintln!("no extended attributes here ({e}): the note cannot be played");
            return;
        }
    }
    let s = session(folder.path(), state.path(), &store);
    s.pass(&AtomicBool::new(false), &mut |_| {}).expect("the pass");
    let list = s.problems();
    let notes: Vec<_> = list
        .problems
        .iter()
        .filter(|p| p.code == Code::XattrsNotSynced)
        .collect();
    assert_eq!(notes.len(), 1, "once per folder: {notes:?}");
    assert_eq!(notes[0].key, "tagged");
    assert_eq!(notes[0].severity, Severity::Warning);
    assert!(list.up_to_date(), "a note leaves the folder up to date");
    assert_eq!(remote_keys(&store).len(), 3, "the content syncs");
}

/// X-DELTA-EDGE-3 (E15a, T12). A hostile (or just broken) drive whose index holds a hundred thousand
/// keys no folder can hold (20,000 here, see below): every one was a problem in the pass's report, a line in the list and a
/// row in `problems.json`, and the list was made by a scan of itself for each - minutes of one core
/// and a file of tens of megabytes every pass. The index below has one good file and 20,000 keys with
/// a name of 260 bytes (the brief's 5,000-byte keys of 10,000 levels are half a gigabyte of JSON: a
/// name past 255 bytes is as much a name no folder holds): one pass on the engine, which publishes
/// the list. The good file arrives, no folder is made, the report names the first thousand and says
/// how many more there are in one row, the list does the same, and the pass ends within a minute.
#[test]
fn a_hostile_index_with_tens_of_thousands_of_over_long_keys_is_listed_once_per_code_and_never_written(
) {
    use super::sync::{Device, PREFIX};
    use crate::sync::{
        engine::Engine, local::hash_bytes, problems::LISTED_PER_CODE, quarantine::OwnDevices,
        remote::blob_key, sync_to, LocalRoot, RemoteFile, RunHooks, SyncOptions, Target,
    };
    // The brief's hundred thousand: a pass over that many takes 22 s unoptimised, and the counts below
    // fail on the old code at any size past a thousand.
    const BAD: usize = 20_000;
    let store = Arc::new(S3Bucket::new());
    let good = b"the one good file";
    let hash = hash_bytes(good);
    store.write(&blob_key(PREFIX, &hash), good.to_vec());
    let entry = |gen: u64| RemoteFile {
        hash: hash.clone(),
        size: good.len() as u64,
        mtime: 1_700_000_000,
        mtime_ns: 0,
        gen,
        device: String::from("someone"),
        exec: false,
        link: None,
    };
    let mut index = RemoteIndex::empty();
    index.generation = 1;
    index.files.insert(String::from("good.txt"), entry(1));
    let long = "x".repeat(260);
    for i in 0..BAD {
        index.files.insert(format!("deep/{i:06}-{long}"), entry(1));
    }
    store.write(&index_key(PREFIX), index.to_bytes());
    drop(index);

    let device = Device::new("hostile-index");
    let folder = device.path().to_path_buf();
    let state = TempDir::new("azcloud-hostile-index-state");
    let state_dir = state.path().to_path_buf();
    let (sent, done) = std::sync::mpsc::channel();
    {
        let store = Arc::clone(&store);
        let folder = folder.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let engine = Engine::new(
                &state_dir,
                &folder,
                Box::new(OwnDevices(vec![String::from("dev")])),
            );
            let root = LocalRoot::folder(&folder);
            let opts = SyncOptions::new(PREFIX, "d-test", "dev").unwrap();
            let index_path = state_dir.join("index.json");
            let (result, summary) = engine.pass(RunHooks::default(), |hooks| {
                sync_to(
                    Target::Index(&*store as &dyn RemoteStore),
                    &root,
                    &index_path,
                    &opts,
                    hooks,
                )
            });
            let _ = sent.send((result, summary, started.elapsed()));
        });
    }
    let (result, summary, took) = done
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the pass over a hostile index takes more than a minute");
    let report = result.expect("the pass succeeds");
    assert_eq!(
        fs::read(folder.join("good.txt")).unwrap(),
        good,
        "the good file arrives"
    );
    assert!(
        !folder.join("deep").exists(),
        "no folder is made for a key no folder holds"
    );
    // The report: the first thousand by name, and one row for the rest.
    let named = |p: &&crate::sync::problems::Problem| p.code == Code::BadRemoteName && p.more == 0;
    assert_eq!(
        report.problems.iter().filter(named).count(),
        LISTED_PER_CODE
    );
    let more: Vec<_> = report.problems.iter().filter(|p| p.more > 0).collect();
    assert_eq!(more.len(), 1, "{more:?}");
    assert_eq!(
        (more[0].code, more[0].more),
        (Code::BadRemoteName, (BAD - LISTED_PER_CODE) as u64)
    );
    assert!(report.problems.len() <= 1_100, "{}", report.problems.len());
    // The list the pass published: the same, and a file a pass can write.
    let list = &summary.problems;
    assert_eq!(list.problems.iter().filter(named).count(), LISTED_PER_CODE);
    assert_eq!(
        list.total(),
        BAD as u64,
        "every one is counted, listed or not"
    );
    let bytes = serde_json::to_vec(list).unwrap();
    assert!(
        bytes.len() < 1 << 20,
        "problems.json is {} bytes",
        bytes.len()
    );
    let saved = crate::sync::problems::ProblemList::load(state.path()).unwrap();
    assert_eq!(
        saved.total(),
        BAD as u64,
        "and it is what the state folder holds"
    );
    assert!(took < std::time::Duration::from_secs(60), "{took:?}");
    // The drive is as it was: nothing of the hostile entries is touched.
    assert_eq!(super::sync::index_of(&store).files.len(), BAD + 1);
}

/// An object another device replaces between two ranges of a download and puts back: it is `a` for the first
/// range, `b` for the second and `a` again afterwards, so the look at the end finds the tag the download began
/// with (X-DELTA-SYNC4-12's A-B-A). A pinned range is served only while the object is the version it names; a
/// plain one is served whatever the object is then. `replace: false` is the same drive that is left alone.
struct ReplacedBetweenRanges {
    a: Vec<u8>,
    b: Vec<u8>,
    replace: bool,
    /// The ranges served so far.
    served: std::sync::Mutex<usize>,
    pinned: std::sync::atomic::AtomicUsize,
    plain: std::sync::atomic::AtomicUsize,
}

impl ReplacedBetweenRanges {
    fn new(replace: bool) -> ReplacedBetweenRanges {
        // Two parts: one whole part and one byte.
        let len = usize::try_from(azul_storage::transfer::CHUNK).unwrap() + 1;
        ReplacedBetweenRanges {
            a: (0..len).map(|i| (i % 251) as u8).collect(),
            b: (0..len).map(|i| ((i + 7) % 251) as u8).collect(),
            replace,
            served: std::sync::Mutex::new(0),
            pinned: std::sync::atomic::AtomicUsize::new(0),
            plain: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// The object as it is after `served` ranges: `b` for the second one.
    fn now(&self, served: usize) -> (&[u8], &'static str) {
        if self.replace && served == 1 {
            (&self.b, "tag-b")
        } else {
            (&self.a, "tag-a")
        }
    }

    fn serve(
        &self,
        key: &str,
        range: azul_storage::ByteRange,
        pinned_to: Option<&str>,
    ) -> Result<Vec<u8>, DriveError> {
        let mut served = self.served.lock().unwrap();
        let (object, tag) = self.now(*served);
        if pinned_to.is_some_and(|want| want != tag) {
            return Err(DriveError::Conflict {
                key: key.to_string(),
            });
        }
        *served += 1;
        let start = usize::try_from(range.start)
            .unwrap_or(usize::MAX)
            .min(object.len());
        let end = range
            .end
            .map_or(object.len(), |e| {
                usize::try_from(e).unwrap_or(usize::MAX).saturating_add(1)
            })
            .min(object.len());
        Ok(object[start..end.max(start)].to_vec())
    }
}

impl azul_storage::Drive for ReplacedBetweenRanges {
    fn list(&self, _: &azul_storage::ListRequest) -> Result<azul_storage::ListPage, DriveError> {
        Ok(azul_storage::ListPage {
            folders: Vec::new(),
            objects: Vec::new(),
            next: None,
        })
    }

    fn get(&self, _: &str) -> Result<Vec<u8>, DriveError> {
        let served = *self.served.lock().unwrap();
        Ok(self.now(served).0.to_vec())
    }

    fn get_range(&self, key: &str, range: azul_storage::ByteRange) -> Result<Vec<u8>, DriveError> {
        self.plain.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.serve(key, range, None)
    }

    fn get_range_if(
        &self,
        key: &str,
        range: azul_storage::ByteRange,
        condition: &azul_storage::Precondition,
    ) -> Result<Vec<u8>, DriveError> {
        match condition {
            azul_storage::Precondition::Matches(tag) => {
                self.pinned
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.serve(key, range, Some(tag))
            }
            azul_storage::Precondition::Absent => Err(DriveError::Unsupported(String::from(
                "a range only if absent",
            ))),
        }
    }

    fn put(&self, key: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(DriveError::Unsupported(format!("writing {key}")))
    }

    fn delete(&self, _: &str) -> Result<(), DriveError> {
        Ok(())
    }

    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
        let served = *self.served.lock().unwrap();
        let (object, tag) = self.now(served);
        Ok(azul_storage::ObjectInfo {
            key: key.to_string(),
            size: object.len() as u64,
            modified: Some(1),
            etag: Some(tag.to_string()),
        })
    }
}

/// X-DELTA-STRUCT3-6. The sync's download is made over `Stoppable`, which wrote seven methods and left the rest to
/// the trait's defaults: `get_range_if` answered `Unsupported`, so the download read its ranges PLAIN and looked once
/// at the end. An object replaced between two ranges and put back (A, B, A) passed that look and the file under the
/// final name was the splice of two versions - for the named pairing, whose blobs carry no content hash, a spliced
/// file the sync then remembers. Every range is pinned to the version the download began with now, so the range
/// that meets B is a `Conflict` and nothing is put in place. Entered through `download_told`, the function the
/// sync's fetch calls. RED against the old `Stoppable` (the download answers `Ok` with the splice).
#[test]
fn x_delta_struct3_6_a_download_in_parts_never_splices_an_object_replaced_between_two_ranges_and_put_back(
) {
    use std::sync::atomic::Ordering::SeqCst;

    let object = ReplacedBetweenRanges::new(true);
    let size = object.a.len() as u64;
    let dir = TempDir::new("azcloud-struct3-6");
    let dest = dir.path().join("big.blob");
    let outcome = crate::sync::drive_store::download_told(&object, "big", size, &dest, &|| {});
    assert!(
        matches!(outcome, Err(DriveError::Conflict { .. })),
        "a range that met the other version was read and kept: {outcome:?}"
    );
    assert!(!dest.exists(), "no file under its name");
    assert_eq!(object.plain.load(SeqCst), 0, "no range was read unpinned");
    assert!(object.pinned.load(SeqCst) >= 1);

    // The control: a drive nobody replaces is downloaded whole through the same pinned reads.
    let steady = ReplacedBetweenRanges::new(false);
    let dest = dir.path().join("steady.blob");
    crate::sync::drive_store::download_told(&steady, "big", size, &dest, &|| {})
        .expect("a steady object");
    assert_eq!(fs::read(&dest).unwrap(), steady.a);
    assert_eq!(steady.pinned.load(SeqCst), 2, "both ranges pinned");
    assert_eq!(steady.plain.load(SeqCst), 0);
}

/// X-DELTA-STRUCT3-6, -7 and -8 for `Stoppable` itself: the pinned read reaches the drive below as ONE `get_range_if`
/// with its condition, every request - the nine calls `delegate_drive!` passes straight on included - is refused once
/// the download's pass is gone and nothing reaches the drive below then.
#[test]
fn x_delta_struct3_8_the_downloads_drive_pins_a_read_and_refuses_every_request_once_its_pass_is_gone(
) {
    use crate::sync::drive_store::Stoppable;
    use azul_storage::testing::{pinned_read_arrives, PinnedRead, Probe, METHOD_NAMES};

    let dir = TempDir::new("azcloud-struct3-8-stoppable");
    let probe = Probe::new();
    let below: Arc<dyn azul_storage::Drive> = Arc::new(probe.clone());
    let stopped = AtomicBool::new(false);
    let drive = Stoppable {
        inner: &*below,
        stopped: &stopped,
    };
    pinned_read_arrives(&drive, &probe, "object").expect("the pinned read arrives as sent");
    let down = super::every_method_of(&drive, dir.path());
    assert_eq!(down.len(), METHOD_NAMES.len());
    for call in &down {
        assert!(
            !matches!(call.outcome, Err(DriveError::Transport(_))),
            "{}: refused with the pass alive: {:?}",
            call.name,
            call.outcome
        );
    }
    let heard = probe.heard().len();

    stopped.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut not_refused = Vec::new();
    for call in super::every_method_of(&drive, dir.path())
        .iter()
        .filter(|call| call.request)
    {
        if !matches!(call.outcome, Err(DriveError::Transport(_))) {
            not_refused.push(format!("{}: {:?}", call.name, call.outcome));
        }
    }
    assert!(
        not_refused.is_empty(),
        "requests the stop did not refuse: {not_refused:?}"
    );
    assert!(matches!(
        pinned_read_arrives(&drive, &probe, "object"),
        Err(PinnedRead::Refused(DriveError::Transport(_)))
    ));
    assert_eq!(
        probe.heard().len(),
        heard,
        "nothing reached the drive below once the pass was gone"
    );
}
