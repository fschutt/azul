//! A pass or a read-back over a hundred thousand files that fail tells the first thousand of each kind
//! one by one and counts the rest (X-EDGE-A-5): the problems of the report itself - not only the list
//! the engine publishes from them - are bounded where they are told, so a tree whose files all fail
//! is no hundred thousand sentences, lines and problems in memory and no `verify.json` of tens of
//! megabytes. Every test enters through what production calls (the engine's pass over a drive, the
//! read-back) and counts what the report says and what the published list says.

use std::{fs, path::PathBuf, sync::Arc};

use azul_storage::{testing::TempDir, AzlinError, DriveError, ServiceError};

use super::fake_s3::S3Bucket;
use crate::{
    error::{CloudError, CloudResult},
    store::{Conditional, RemoteObject, RemoteStore},
    sync::{
        engine::{Engine, PassSummary},
        faults,
        local::{hash_bytes, path_of, BaseEntry},
        problems::{from_report, Code, Problem, ProblemList, LISTED_PER_CODE},
        quarantine::OwnDevices,
        remote::{blob_key, blobs_prefix},
        sync_to,
        verify::{verify, VerifyReport},
        LocalIndex, LocalRoot, RunHooks, SyncOptions, SyncReport, Target,
    },
};

const PREFIX: &str = "ea5/";

/// Past the cap by half of it: a thousand told one by one, five hundred counted.
const OVER: usize = LISTED_PER_CODE + 500;

/// What the write of a blob gets.
#[derive(Clone, Copy)]
enum BlobPuts {
    /// What the bucket says.
    Taken,
    /// 413 for every one: the drive takes no file this big.
    TooLarge,
    /// An answer as if it were stored, and nothing kept: a drive that lost its blobs.
    Lost,
}

/// A drive that fails file by file: the write of a blob and its read are told what to do, every
/// other call (the index, the listings) goes to the bucket.
struct FileByFile {
    inner: Arc<S3Bucket>,
    puts: BlobPuts,
    /// What the read of a blob answers instead of the bucket (`None`: the bucket's answer).
    reads: Option<DriveError>,
}

impl FileByFile {
    fn over(inner: &Arc<S3Bucket>, puts: BlobPuts, reads: Option<DriveError>) -> FileByFile {
        FileByFile {
            inner: Arc::clone(inner),
            puts,
            reads,
        }
    }

    fn is_blob(key: &str) -> bool {
        key.starts_with(&blobs_prefix(PREFIX))
    }
}

impl RemoteStore for FileByFile {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        self.inner.get_unless(key, etag)
    }
    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        match &self.reads {
            Some(refusal) if Self::is_blob(key) => Err(CloudError::Drive(refusal.clone())),
            _ => self.inner.fetch(key, size),
        }
    }
    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        if Self::is_blob(key) {
            match self.puts {
                BlobPuts::Taken => {}
                BlobPuts::TooLarge => {
                    return Err(CloudError::Drive(DriveError::Service(ServiceError {
                        status: 413,
                        code: String::from("EntityTooLarge"),
                        message: String::from("the drive takes no file this big"),
                        azlin_error: Some(AzlinError::ObjectTooLarge),
                        ..ServiceError::default()
                    })))
                }
                BlobPuts::Lost => return Ok(String::from("\"lost\"")),
            }
        }
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

/// One device: its folder, its state folder and the name its writes carry.
struct Side {
    folder: TempDir,
    state: TempDir,
    device: &'static str,
}

impl Side {
    fn new(name: &str, device: &'static str) -> Side {
        Side {
            folder: TempDir::new(&format!("azcloud-ea5-{name}")),
            state: TempDir::new(&format!("azcloud-ea5-{name}-state")),
            device,
        }
    }

    fn root(&self) -> LocalRoot {
        LocalRoot::folder(self.folder.path())
    }

    fn index(&self) -> PathBuf {
        self.state.path().join("index.json")
    }

    fn opts(&self) -> SyncOptions {
        SyncOptions::new(PREFIX, "d-test", self.device).unwrap()
    }

    /// `count` small files in forty folders, each with bytes of its own.
    fn write_files(&self, count: usize) {
        for i in 0..count {
            let path = path_of(self.folder.path(), &key_of(i));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content_of(i)).unwrap();
            // An hour old: nothing about it is racy.
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
                .unwrap();
        }
    }

    /// One pass of the engine over `store`: the report, and the list it published.
    fn pass(&self, store: &dyn RemoteStore, opts: &SyncOptions) -> (SyncReport, PassSummary) {
        let engine = Engine::new(
            self.state.path(),
            self.folder.path(),
            Box::new(OwnDevices(vec![self.device.to_string()])),
        );
        let root = self.root();
        let index = self.index();
        let (result, summary) = engine.pass(RunHooks::default(), |hooks| {
            sync_to(Target::Index(store), &root, &index, opts, hooks)
        });
        (result.expect("the pass succeeds"), summary)
    }

    /// The first pass of a folder of `count` files, to a drive that takes the blobs as `puts` says (not
    /// `TooLarge`: nothing goes up then).
    fn sent(&self, count: usize, drive: &Arc<S3Bucket>, puts: BlobPuts) -> SyncReport {
        self.write_files(count);
        let (report, _) = self.pass(&FileByFile::over(drive, puts, None), &self.opts());
        assert_eq!(
            report.files_up,
            count,
            "the premise of the test: {}",
            report.summary()
        );
        report
    }
}

fn key_of(i: usize) -> String {
    format!("many/d{:02}/f{i:05}.txt", i % 40)
}

fn content_of(i: usize) -> Vec<u8> {
    format!("file {i}").into_bytes()
}

/// The one row of `problems` that stands for the problems of its code that are not listed one by
/// one: its code and how many.
fn the_row_for_the_rest(problems: &[Problem]) -> (Code, u64) {
    let rows: Vec<&Problem> = problems.iter().filter(|p| p.more > 0).collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].key, "", "it names no file");
    (rows[0].code, rows[0].more)
}

/// X-EDGE-A-5, `listed_upload_failure`. 10,000 files whose upload the drive refuses (413, for each of
/// them): each was one sentence, one failed file and one problem in the report - the list the engine
/// published was the only thing bounded. Now the report keeps a thousand of each and one row that
/// says how many more, `from_report` finds nothing of the counted ones to list a second time, and the
/// published list counts every file once.
#[test]
fn x_edge_a_5_ten_thousand_uploads_that_fail_are_a_thousand_problems_and_one_row_for_the_rest() {
    const FILES: usize = 10_000;
    let a = Side::new("up", "dev-a");
    a.write_files(FILES);
    let drive = Arc::new(S3Bucket::new());
    let (report, summary) = a.pass(
        &FileByFile::over(&drive, BlobPuts::TooLarge, None),
        &a.opts(),
    );
    assert_eq!(
        report.problems.len(),
        LISTED_PER_CODE + 1,
        "{} problems for {FILES} refused files: {}",
        report.problems.len(),
        report.summary()
    );
    assert_eq!(
        the_row_for_the_rest(&report.problems),
        (Code::TooLarge, (FILES - LISTED_PER_CODE) as u64)
    );
    assert_eq!(report.file_errors.len(), LISTED_PER_CODE);
    assert_eq!(report.errors.len(), LISTED_PER_CODE);
    assert_eq!(
        from_report(&report).len(),
        LISTED_PER_CODE + 1,
        "no sentence of a failure that was only counted is listed a second time"
    );
    assert_eq!(summary.problems.problems.len(), LISTED_PER_CODE + 1);
    assert_eq!(
        summary.problems.total(),
        FILES as u64,
        "every refused file is counted once in the published list"
    );
}

/// X-EDGE-A-5, the download that fails (`mod.rs` `write_download`). A drive that lost its blobs:
/// another device's first pass finds every file's blob missing, and told each one.
#[test]
fn x_edge_a_5_downloads_that_fail_are_told_a_thousand_at_a_time_and_counted_beyond() {
    let drive = Arc::new(S3Bucket::new());
    Side::new("down-a", "dev-a").sent(OVER, &drive, BlobPuts::Lost);
    let b = Side::new("down-b", "dev-b");
    let (report, summary) = b.pass(&*drive, &b.opts());
    assert_eq!(
        report.problems.len(),
        LISTED_PER_CODE + 1,
        "{} problems for {OVER} blobs the drive lost: {}",
        report.problems.len(),
        report.summary()
    );
    assert_eq!(
        the_row_for_the_rest(&report.problems),
        (Code::RemoteEntryUnreadable, (OVER - LISTED_PER_CODE) as u64)
    );
    assert_eq!(report.file_errors.len(), LISTED_PER_CODE);
    assert_eq!(from_report(&report).len(), LISTED_PER_CODE + 1);
    assert_eq!(summary.problems.total(), OVER as u64);
}

/// X-EDGE-A-5, `failed_here`. Every write of another device's files into this folder fails (the files
/// are held by a scanner, a permission): one `locked` problem for each file, in the report.
#[test]
fn x_edge_a_5_writes_here_that_fail_are_told_a_thousand_at_a_time_and_counted_beyond() {
    let drive = Arc::new(S3Bucket::new());
    Side::new("here-a", "dev-a").sent(OVER, &drive, BlobPuts::Taken);
    let b = Side::new("here-b", "dev-b");
    let denied = faults::errno_of("EACCES").expect("a number for EACCES");
    faults::fail_replace(b.folder.path(), "*", denied, 1);
    let (report, summary) = b.pass(&*drive, &b.opts());
    faults::clear(b.folder.path());
    assert_eq!(
        report.problems.len(),
        LISTED_PER_CODE + 1,
        "{} problems for {OVER} locked files: {}",
        report.problems.len(),
        report.summary()
    );
    assert_eq!(
        the_row_for_the_rest(&report.problems),
        (Code::Locked, (OVER - LISTED_PER_CODE) as u64)
    );
    assert_eq!(report.file_errors.len(), LISTED_PER_CODE);
    assert_eq!(report.errors.len(), LISTED_PER_CODE);
    assert_eq!(from_report(&report).len(), LISTED_PER_CODE + 1);
    assert_eq!(summary.problems.total(), OVER as u64);
}

/// X-EDGE-A-5, the disk's reserve. A disk that keeps more free than the drive's files need leaves every
/// download on the drive, said: the problem was capped (`tell`) and the failed file was not, so
/// `from_report` listed the files past the cap a second time under the code of their sentence, and the
/// published list counted them twice.
#[test]
fn x_edge_a_5_downloads_a_disk_cannot_keep_free_for_are_counted_once() {
    let drive = Arc::new(S3Bucket::new());
    Side::new("reserve-a", "dev-a").sent(OVER, &drive, BlobPuts::Taken);
    let b = Side::new("reserve-b", "dev-b");
    let mut opts = b.opts();
    opts.disk_reserve = u64::MAX / 2;
    let (report, summary) = b.pass(&*drive, &opts);
    assert_eq!(
        from_report(&report).len(),
        LISTED_PER_CODE + 1,
        "{} problems for {OVER} files that stay on the drive: {}",
        from_report(&report).len(),
        report.summary()
    );
    assert_eq!(
        the_row_for_the_rest(&report.problems),
        (Code::DiskReserve, (OVER - LISTED_PER_CODE) as u64)
    );
    assert_eq!(report.file_errors.len(), LISTED_PER_CODE);
    assert_eq!(
        summary.problems.total(),
        OVER as u64,
        "each file is listed once"
    );
}

/// A read-back that is answered for every file by the same refusal of a read: nothing is wrong with
/// the copies, none could be checked. One `unchecked` problem for each, in the report.
#[test]
fn x_edge_a_5_a_read_back_that_cannot_read_is_a_thousand_problems_and_one_row_for_the_rest() {
    let drive = Arc::new(S3Bucket::new());
    let a = Side::new("unread", "dev-a");
    a.sent(OVER, &drive, BlobPuts::Taken);
    let cut = DriveError::Transport(String::from("connection reset by peer"));
    let store = FileByFile::over(&drive, BlobPuts::Taken, Some(cut));
    let found = verify(Target::Index(&store), &a.root(), &a.index(), &a.opts()).unwrap();
    assert_eq!(found.unread.len(), OVER, "the premise: no copy was read");
    assert_read_back_is_bounded(&found, Code::Unchecked);
}

/// A read-back that finds every copy on the drive damaged (the bytes under each blob's name are other
/// bytes) and sends each one again from here: one `hash-mismatch` problem for each, in the report.
#[test]
fn x_edge_a_5_a_read_back_that_finds_damage_is_a_thousand_problems_and_one_row_for_the_rest() {
    let drive = Arc::new(S3Bucket::new());
    let a = Side::new("damaged", "dev-a");
    a.sent(OVER, &drive, BlobPuts::Taken);
    for i in 0..OVER {
        drive.write(
            &blob_key(PREFIX, &hash_bytes(&content_of(i))),
            b"rotten".to_vec(),
        );
    }
    let found = verify(Target::Index(&*drive), &a.root(), &a.index(), &a.opts()).unwrap();
    assert_eq!(
        found.mismatches.len(),
        OVER,
        "the premise: every copy is damaged"
    );
    assert_eq!(found.healed.len(), OVER, "and sent again from here");
    assert_read_back_is_bounded(&found, Code::HashMismatch);
}

/// A read-back that finds this device's own copies rotted (other bytes of the same size under the same
/// times, as a failing disk leaves them) and puts each synced version back: one `hash-mismatch`
/// problem for each, in the report.
#[test]
fn x_edge_a_5_a_read_back_that_finds_rot_here_is_a_thousand_problems_and_one_row_for_the_rest() {
    let drive = Arc::new(S3Bucket::new());
    let a = Side::new("rotted", "dev-a");
    a.sent(OVER, &drive, BlobPuts::Taken);
    // The same size, the same modification time, other bytes; then the base learns the change time the
    // write left (a write through the file system always moves it, a failing disk's does not).
    let mut base = LocalIndex::load(&a.index()).unwrap().expect("a base");
    for i in 0..OVER {
        let path = path_of(a.folder.path(), &key_of(i));
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, format!("FILE {i}")).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        let st = azul_storage::beneath::stat_path(&path).unwrap();
        let entry: &mut BaseEntry = base.files.get_mut(&key_of(i)).expect("synced");
        entry.ctime_ns = st.ctime_ns;
    }
    base.save(&a.index()).unwrap();
    let found = verify(Target::Index(&*drive), &a.root(), &a.index(), &a.opts()).unwrap();
    assert_eq!(
        found.rotted.len(),
        OVER,
        "the premise: every copy here rotted"
    );
    assert_eq!(found.restored.len(), OVER, "and the synced version is back");
    assert_read_back_is_bounded(&found, Code::HashMismatch);
}

/// What every read-back above says of its problems: a thousand rows of one code and the row that
/// counts the rest.
fn assert_read_back_is_bounded(found: &VerifyReport, code: Code) {
    assert_eq!(
        found.problems.len(),
        LISTED_PER_CODE + 1,
        "{} problems for {OVER} findings",
        found.problems.len()
    );
    assert_eq!(
        the_row_for_the_rest(&found.problems),
        (code, (OVER - LISTED_PER_CODE) as u64)
    );
    assert!(found
        .problems
        .iter()
        .filter(|p| p.more == 0)
        .all(|p| p.code == code));
}

/// X-EDGE-A-5, the list's side of the read-back: a row that stands for others reaches the list a check
/// notes into (`evict::note_problem`) and is counted into the code's own row - noted as an ordinary
/// problem it was one more in the row of a code at its cap, and nothing at all where the row existed.
#[test]
fn x_edge_a_5_a_row_that_stands_for_others_is_counted_into_the_codes_row_of_the_list() {
    let mut list = ProblemList::default();
    for i in 0..LISTED_PER_CODE {
        list.note(
            Problem::new(Code::HashMismatch, &format!("big/{i:04}.bin"), "rotted"),
            100,
        );
    }
    list.note(Problem::left_out(Code::HashMismatch, 9_000), 200);
    list.note(Problem::left_out(Code::HashMismatch, 5), 300);
    assert_eq!(list.problems.len(), LISTED_PER_CODE + 1);
    assert_eq!(list.total(), LISTED_PER_CODE as u64 + 9_005);
    assert_eq!(
        the_row_for_the_rest(&list.problems),
        (Code::HashMismatch, 9_005)
    );
    // A code the list has no row for yet gets the row as it came, and the next one joins it.
    list.note(Problem::left_out(Code::Unchecked, 7), 400);
    list.note(Problem::left_out(Code::Unchecked, 3), 500);
    let unchecked: Vec<&Problem> = list
        .problems
        .iter()
        .filter(|p| p.code == Code::Unchecked)
        .collect();
    assert_eq!(unchecked.len(), 1, "{unchecked:?}");
    assert_eq!(unchecked[0].more, 10);
    assert_eq!(unchecked[0].since, 400);
    let on_disk = TempDir::new("ea5-list");
    list.save(on_disk.path()).unwrap();
    crate::sync::evict::note_problem(on_disk.path(), Problem::left_out(Code::Unchecked, 2));
    let again = ProblemList::load(on_disk.path()).unwrap();
    assert_eq!(again.total(), list.total() + 2);
}
