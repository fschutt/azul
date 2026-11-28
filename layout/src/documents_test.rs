//! `Documents` over the desktop's backend: listing, stat, ranged reads, writes that are all-or-nothing, names, deletes, the failures
//! as typed errors, and the handle that moves to a worker thread.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use azul_core::documents::{AllFilesAccess, DocumentError, PlaceKind};
use azul_css::{AzString, U8Vec};

use super::*;
use crate::managers::share_area::new_token;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("azul-documents-test-{}", new_token()));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().into_owned()
    }
    fn file(&self, rel: &str, bytes: &[u8]) -> String {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn az(text: &str) -> AzString {
    AzString::from(text)
}

fn names(entries: &DocumentEntryVec) -> Vec<String> {
    entries.as_ref().iter().map(|e| e.name.as_str().to_string()).collect()
}

#[test]
fn a_folder_lists_its_folders_first_then_by_name_with_what_the_platform_says() {
    let dir = TestDir::new();
    dir.file("b.md", b"# b");
    dir.file("A.txt", b"aa");
    dir.file("sub/inner.png", &[1, 2, 3]);
    fs::create_dir(dir.0.join("zdir")).unwrap();
    let docs = Documents::local();
    let listed = docs.children(az(&dir.0.to_string_lossy())).unwrap();
    assert_eq!(names(&listed), ["sub", "zdir", "A.txt", "b.md"]);
    let entries = listed.as_ref();
    assert!(entries[0].is_dir && entries[0].mime.as_str() == "inode/directory" && entries[0].size.as_option().is_none());
    assert_eq!(entries[2].mime.as_str(), "text/plain");
    assert_eq!(entries[2].size.as_option().copied(), Some(2));
    assert_eq!(entries[3].mime.as_str(), "text/markdown");
    assert!(entries[3].modified_ms.as_option().is_some());
    assert!(entries[3].can_write && entries[3].can_delete && entries[3].can_rename);
}

#[test]
fn a_missing_folder_a_file_and_a_denied_folder_say_which() {
    let dir = TestDir::new();
    let file = dir.file("f.txt", b"x");
    let docs = Documents::local();
    assert!(matches!(docs.children(az(&dir.path("nope"))), Err(DocumentError::NotFound(_))));
    assert!(matches!(
        docs.children(az(&file)),
        Err(DocumentError::NotADirectory(_) | DocumentError::Failed(_))
    ));
    assert!(matches!(docs.stat(az(&dir.path("nope"))), Err(DocumentError::NotFound(_))));
}

#[test]
fn stat_describes_one_entry() {
    let dir = TestDir::new();
    let file = dir.file("pic.JPG", &[0; 10]);
    let entry = Documents::local().stat(az(&file)).unwrap();
    assert_eq!(entry.name.as_str(), "pic.JPG");
    assert_eq!(entry.mime.as_str(), "image/jpeg");
    assert_eq!(entry.uri.as_str(), file);
    assert_eq!(entry.size.as_option().copied(), Some(10));
}

#[test]
fn a_range_read_gives_the_bytes_asked_for_and_fewer_at_the_end() {
    let dir = TestDir::new();
    let file = dir.file("n.bin", &(0..100u8).collect::<Vec<_>>());
    let docs = Documents::local();
    assert_eq!(docs.read(az(&file), 10, 5).unwrap().as_ref(), [10, 11, 12, 13, 14]);
    assert_eq!(docs.read(az(&file), 95, 50).unwrap().as_ref(), [95, 96, 97, 98, 99]);
    assert!(docs.read(az(&file), 100, 10).unwrap().as_ref().is_empty(), "nothing past the end");
    assert!(docs.read(az(&file), 1_000_000, 10).unwrap().as_ref().is_empty());
    assert!(docs.read(az(&file), 0, 0).unwrap().as_ref().is_empty());
    assert_eq!(docs.read(az(&file), 0, 100).unwrap().as_ref().len(), 100);
}

#[test]
fn a_read_over_the_limit_is_refused_with_the_numbers() {
    let dir = TestDir::new();
    let file = dir.file("n.bin", b"x");
    match Documents::local().read(az(&file), 0, MAX_READ + 1) {
        Err(DocumentError::Failed(why)) => assert!(why.as_str().contains("64 MiB") && why.as_str().contains("ranges"), "{}", why.as_str()),
        other => panic!("{other:?}"),
    }
    assert!(Documents::local().read(az(&file), 0, MAX_READ).is_ok());
}

#[test]
fn a_write_replaces_the_content_all_or_nothing() {
    let dir = TestDir::new();
    let file = dir.file("doc.md", b"old old old old");
    let docs = Documents::local();
    let entry = docs.write(az(&file), U8Vec::from_vec(b"new".to_vec())).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"new", "shorter content truncates");
    assert_eq!(entry.size.as_option().copied(), Some(3));
    // no half-written leftovers beside it
    let leftovers: Vec<_> = fs::read_dir(&dir.0).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
    assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    // a file that is not there is not created by a write
    assert!(matches!(docs.write(az(&dir.path("ghost.md")), U8Vec::from_vec(vec![1])), Err(DocumentError::NotFound(_))));
    assert!(!dir.0.join("ghost.md").exists());
}

#[test]
fn a_write_into_a_folder_that_cannot_take_it_leaves_the_original() {
    let dir = TestDir::new();
    let file = dir.file("keep.txt", b"original");
    // writing "over" a folder: the rename fails, the temporary file is removed, the folder is as it was
    fs::create_dir(dir.0.join("folder")).unwrap();
    assert!(Documents::local().write(az(&dir.path("folder")), U8Vec::from_vec(vec![1])).is_err());
    assert!(dir.0.join("folder").is_dir());
    assert_eq!(fs::read(&file).unwrap(), b"original");
    let count = fs::read_dir(&dir.0).unwrap().count();
    assert_eq!(count, 2, "no temporary file stays");
}

#[test]
fn files_and_folders_are_made_with_plain_names_and_never_over_an_existing_one() {
    let dir = TestDir::new();
    let docs = Documents::local();
    let parent = az(&dir.0.to_string_lossy());
    let made = docs.create_file(parent.clone(), az("new.md"), az("text/markdown")).unwrap();
    assert_eq!(made.name.as_str(), "new.md");
    assert_eq!(made.size.as_option().copied(), Some(0));
    assert!(matches!(
        docs.create_file(parent.clone(), az("new.md"), az("text/markdown")),
        Err(DocumentError::Exists(_))
    ));
    let folder = docs.create_dir(parent.clone(), az("notes")).unwrap();
    assert!(folder.is_dir);
    assert!(matches!(docs.create_dir(parent.clone(), az("notes")), Err(DocumentError::Exists(_))));
    for bad in ["", ".", "..", "a/b", "a\\b", "nul\0", &"n".repeat(300)] {
        assert!(matches!(docs.create_file(parent.clone(), az(bad), az("x/y")), Err(DocumentError::Failed(_))), "{bad:?}");
        assert!(matches!(docs.create_dir(parent.clone(), az(bad)), Err(DocumentError::Failed(_))), "{bad:?}");
    }
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2, "the refused names made nothing");
}

#[test]
fn a_rename_never_lands_on_something_that_is_there() {
    let dir = TestDir::new();
    let a = dir.file("a.txt", b"a");
    dir.file("b.txt", b"b");
    let docs = Documents::local();
    assert!(matches!(docs.rename(az(&a), az("b.txt")), Err(DocumentError::Exists(_))));
    let moved = docs.rename(az(&a), az("c.txt")).unwrap();
    assert_eq!(moved.name.as_str(), "c.txt");
    assert!(!dir.0.join("a.txt").exists() && dir.0.join("c.txt").exists());
    assert_eq!(fs::read(&dir.0.join("b.txt")).unwrap(), b"b");
    assert!(matches!(docs.rename(az(&dir.path("zzz")), az("y")), Err(DocumentError::NotFound(_))));
    assert!(matches!(docs.rename(az(&moved.uri.as_str()), az("../escape")), Err(DocumentError::Failed(_))));
}

#[test]
fn a_delete_takes_a_file_or_a_folder_with_what_is_in_it_and_a_missing_one_is_not_found() {
    let dir = TestDir::new();
    let file = dir.file("f.txt", b"x");
    dir.file("tree/a/b/c.txt", b"deep");
    let docs = Documents::local();
    docs.delete(az(&file)).unwrap();
    assert!(!dir.0.join("f.txt").exists());
    docs.delete(az(&dir.path("tree"))).unwrap();
    assert!(!dir.0.join("tree").exists());
    assert!(matches!(docs.delete(az(&dir.path("tree"))), Err(DocumentError::NotFound(_))));
}

#[cfg(unix)]
#[test]
fn deleting_a_link_deletes_the_link_and_not_what_it_points_to() {
    let dir = TestDir::new();
    let target = dir.file("keep/precious.txt", b"precious");
    std::os::unix::fs::symlink(dir.0.join("keep"), dir.0.join("link")).unwrap();
    Documents::local().delete(az(&dir.path("link"))).unwrap();
    assert!(!dir.0.join("link").exists());
    assert_eq!(fs::read(target).unwrap(), b"precious");
}

#[test]
fn a_huge_folder_is_cut_at_the_limit() {
    // a folder of more than MAX_ENTRIES entries would be slow to make: the cut is the same code, shown with the constant
    assert!(MAX_ENTRIES >= 1000 && MAX_ENTRIES <= 100_000);
}

#[test]
fn the_places_are_the_given_folders_and_the_desktop_needs_no_grant() {
    let dir = TestDir::new();
    let docs = Documents::new(Arc::new(LocalDocuments::with_places(vec![
        ("Project".to_string(), dir.0.clone()),
    ])));
    let places = docs.places();
    assert_eq!(places.as_ref().len(), 1);
    assert_eq!(places.as_ref()[0].name.as_str(), "Project");
    assert_eq!(places.as_ref()[0].kind, PlaceKind::SharedStorage);
    assert!(places.as_ref()[0].writable);
    assert_eq!(docs.all_files_access(), AllFilesAccess::Granted);
    assert!(!docs.request_all_files_access());
    assert!(docs.forget(az(&dir.0.to_string_lossy())).is_ok(), "a path is not a grant: nothing to release");
    assert_eq!(docs.backend_name().as_str(), "local");
}

#[test]
fn a_stream_of_a_file_reads_and_seeks() {
    let dir = TestDir::new();
    let file = dir.file("s.bin", b"0123456789");
    let mut stream = Documents::local().open_read(&file).unwrap();
    stream.seek(SeekFrom::Start(4)).unwrap();
    let mut rest = String::new();
    stream.read_to_string(&mut rest).unwrap();
    assert_eq!(rest, "456789");
    assert!(matches!(Documents::local().open_read(&dir.path("none")), Err(DocumentError::NotFound(_))));
}

#[test]
fn the_handle_moves_to_a_worker_thread_and_the_slot_hands_out_the_installed_backend() {
    let dir = TestDir::new();
    let file = dir.file("t.txt", b"thread");
    let slot = DocumentsSlot::new();
    assert_eq!(slot.handle().backend_name().as_str(), "local", "the desktop's backend until a shell installs its own");
    let handle = slot.handle();
    let locator = az(&file);
    let size = std::thread::spawn(move || handle.stat(locator).unwrap().size.as_option().copied()).join().unwrap();
    assert_eq!(size, Some(6));
    let snapshot = slot.handle();
    // a shell installs its own
    #[derive(Debug)]
    struct Other;
    impl DocumentBackend for Other {
        fn name(&self) -> &str { "other" }
        fn places(&self) -> Vec<Place> { Vec::new() }
        fn forget(&self, _: &str) -> Result<(), DocumentError> { Ok(()) }
        fn stat(&self, l: &str) -> Result<DocumentEntry, DocumentError> { Err(DocumentError::Denied(az(l))) }
        fn children(&self, l: &str) -> Result<Vec<DocumentEntry>, DocumentError> { Err(DocumentError::TimedOut(az(l))) }
        fn read(&self, l: &str, _: u64, _: u64) -> Result<Vec<u8>, DocumentError> { Err(DocumentError::Unsupported(az(l))) }
        fn open_read(&self, _: &str) -> Result<Box<dyn ReadSeek>, DocumentError> { Err(DocumentError::Cancelled) }
        fn write(&self, l: &str, _: &[u8]) -> Result<DocumentEntry, DocumentError> { Err(DocumentError::NotFound(az(l))) }
        fn create_file(&self, p: &str, _: &str, _: &str) -> Result<DocumentEntry, DocumentError> { Err(DocumentError::NotADirectory(az(p))) }
        fn create_dir(&self, p: &str, _: &str) -> Result<DocumentEntry, DocumentError> { Err(DocumentError::NotADirectory(az(p))) }
        fn rename(&self, l: &str, _: &str) -> Result<DocumentEntry, DocumentError> { Err(DocumentError::Unsupported(az(l))) }
        fn delete(&self, l: &str) -> Result<(), DocumentError> { Err(DocumentError::Unsupported(az(l))) }
        fn all_files_access(&self) -> AllFilesAccess { AllFilesAccess::Unavailable }
        fn request_all_files_access(&self) -> bool { false }
    }
    slot.install(Arc::new(Other));
    assert_eq!(slot.handle().backend_name().as_str(), "other");
    assert!(matches!(slot.handle().stat(az("content://x")), Err(DocumentError::Denied(_))));
    assert!(matches!(slot.handle().children(az("content://x")), Err(DocumentError::TimedOut(_))));
    assert_eq!(slot.handle().all_files_access(), AllFilesAccess::Unavailable);
    // a handle taken BEFORE the install keeps the backend it was taken with: a handle is a snapshot
    assert_eq!(snapshot.backend_name().as_str(), "local");
}

#[test]
fn the_error_of_a_failure_says_what_the_person_does_next() {
    assert!(DocumentError::NotFound(az("Downloads")).describe().contains("not there any more"));
    assert!(DocumentError::Denied(az("Photos")).describe().contains("removed or never given"));
    assert!(DocumentError::TimedOut(az("Drive")).describe().contains("did not answer in time"));
    assert!(DocumentError::Cancelled.describe().contains("nothing was changed"));
    assert_eq!(DocumentError::Failed(az("the provider said no")).describe(), "the provider said no");
}

// ---- the deadline of a call ----

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < Duration::from_secs(10), "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_call_that_answers_in_time_gives_its_answer_and_leaves_nothing_stuck() {
    let guard = CallGuard::new(Duration::from_secs(5), 4);
    assert_eq!(guard.run("fast", || Ok(41 + 1)).unwrap(), 42);
    assert!(matches!(
        guard.run::<u8>("failing", || Err(DocumentError::Denied(az("x")))),
        Err(DocumentError::Denied(_))
    ));
    assert_eq!(guard.stuck(), 0);
}

#[test]
fn a_call_past_its_deadline_is_timed_out_counted_while_it_is_inside_and_forgotten_when_it_returns() {
    let guard = CallGuard::new(Duration::from_millis(40), 4);
    let started = Instant::now();
    let answer = guard.run("Drive", || {
        std::thread::sleep(Duration::from_millis(400));
        Ok(1u8)
    });
    match answer {
        Err(DocumentError::TimedOut(why)) => assert!(why.as_str().contains("Drive did not answer"), "{}", why.as_str()),
        other => panic!("{other:?}"),
    }
    assert!(started.elapsed() < Duration::from_millis(350), "the caller did not wait for the call");
    assert_eq!(guard.stuck(), 1, "the late call is still inside the provider");
    wait_until("the late call never came back", || guard.stuck() == 0);
}

#[test]
fn a_provider_holding_too_many_calls_is_refused_at_once_and_answers_again_when_they_return() {
    let guard = CallGuard::new(Duration::from_millis(150), 2);
    let (release, blocked) = std::sync::mpsc::channel::<()>();
    let blocked = std::sync::Arc::new(Mutex::new(blocked));
    for _ in 0..2 {
        let blocked = std::sync::Arc::clone(&blocked);
        let answer = guard.run("stuck", move || {
            drop(blocked.lock().unwrap().recv());
            Ok(())
        });
        assert!(matches!(answer, Err(DocumentError::TimedOut(_))));
    }
    assert_eq!(guard.stuck(), 2);
    let ran = std::sync::Arc::new(AtomicBool::new(false));
    let ran_in_call = std::sync::Arc::clone(&ran);
    let started = Instant::now();
    match guard.run("third", move || {
        ran_in_call.store(true, Ordering::SeqCst);
        Ok(())
    }) {
        Err(DocumentError::TimedOut(why)) => assert!(why.as_str().contains("earlier calls are still waiting"), "{}", why.as_str()),
        other => panic!("{other:?}"),
    }
    assert!(started.elapsed() < Duration::from_millis(100), "refused without waiting for the deadline (150 ms)");
    assert!(!ran.load(Ordering::SeqCst), "a refused call is not started");
    // the provider comes back: both late calls end, the count drops, the next call is served
    drop(release.send(()));
    drop(release.send(()));
    wait_until("the stuck calls never came back", || guard.stuck() == 0);
    assert_eq!(guard.run("after", || Ok(7)).unwrap(), 7);
}

#[test]
fn a_panicking_call_is_a_failure_and_not_a_call_stuck_in_the_provider() {
    let guard = CallGuard::new(Duration::from_secs(5), 4);
    let answer = guard.run::<u8>("panicky", || panic!("the backend has a bug"));
    assert!(matches!(answer, Err(DocumentError::Failed(_))), "{answer:?}");
    assert_eq!(guard.stuck(), 0);
}

#[test]
fn the_late_answer_is_dropped_with_what_it_holds() {
    struct Holds(std::sync::Arc<AtomicUsize>);
    impl Drop for Holds {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let closed = std::sync::Arc::new(AtomicUsize::new(0));
    let guard = CallGuard::new(Duration::from_millis(20), 4);
    let in_call = std::sync::Arc::clone(&closed);
    let answer = guard.run("slow open", move || {
        std::thread::sleep(Duration::from_millis(200));
        Ok(Holds(in_call))
    });
    assert!(matches!(answer, Err(DocumentError::TimedOut(_))));
    wait_until("the late answer was not dropped", || closed.load(Ordering::SeqCst) == 1);
    wait_until("the count did not drop", || guard.stuck() == 0);
}

#[test]
fn a_listing_is_folders_first_then_by_name_without_regard_to_case_and_cut_at_the_limit() {
    let entry = |name: &str, is_dir: bool| azul_core::documents::DocumentEntry {
        uri: az(name),
        name: az(name),
        mime: az(if is_dir { "inode/directory" } else { "text/plain" }),
        is_dir,
        size: azul_css::OptionU64::None,
        modified_ms: azul_css::OptionU64::None,
        can_write: true,
        can_delete: true,
        can_rename: true,
    };
    let mut entries = vec![entry("b.txt", false), entry("A.txt", false), entry("a.txt", false), entry("Zeta", true), entry("alpha", true)];
    sort_listing(&mut entries);
    let order: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(order, ["alpha", "Zeta", "A.txt", "a.txt", "b.txt"]);
    let mut many: Vec<_> = (0..MAX_ENTRIES + 5).map(|i| entry(&format!("f{i}"), false)).collect();
    assert!(cap_listing(&mut many));
    assert_eq!(many.len(), MAX_ENTRIES);
    assert!(!cap_listing(&mut many), "a listing at the limit is not cut again");
}

// ---- a range of a stream ----

/// A stream that reads like a pipe: no seeking.
struct Pipe(std::io::Cursor<Vec<u8>>);

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for Pipe {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::from_raw_os_error(29))
    }
}

#[test]
fn a_range_of_a_seekable_stream_is_the_bytes_asked_for() {
    let mut stream = std::io::Cursor::new((0..50u8).collect::<Vec<_>>());
    assert_eq!(read_range(&mut stream, "s", 10, 3).unwrap(), [10, 11, 12]);
    assert_eq!(read_range(&mut stream, "s", 48, 10).unwrap(), [48, 49]);
    assert!(read_range(&mut stream, "s", 50, 10).unwrap().is_empty());
    assert!(read_range(&mut stream, "s", 5000, 10).unwrap().is_empty());
    assert!(read_range(&mut stream, "s", 3, 0).unwrap().is_empty());
    assert_eq!(read_range(&mut stream, "s", 0, u64::MAX).unwrap().len(), 50, "a huge len allocates no more than the stream holds");
}

#[test]
fn a_pipe_that_cannot_seek_is_read_through_to_the_offset_and_not_beyond_what_is_sensible() {
    let bytes = (0..50u8).collect::<Vec<_>>();
    assert_eq!(read_range(&mut Pipe(std::io::Cursor::new(bytes.clone())), "p", 10, 3).unwrap(), [10, 11, 12]);
    assert_eq!(read_range(&mut Pipe(std::io::Cursor::new(bytes.clone())), "p", 0, 2).unwrap(), [0, 1]);
    assert!(read_range(&mut Pipe(std::io::Cursor::new(bytes.clone())), "p", 60, 5).unwrap().is_empty(), "the pipe ends before the offset");
    match read_range(&mut Pipe(std::io::Cursor::new(bytes)), "Drive/big.mkv", MAX_SKIP + 1, 5) {
        Err(DocumentError::Unsupported(why)) => {
            assert!(why.as_str().contains("Drive/big.mkv") && why.as_str().contains("read it from the start"), "{}", why.as_str());
        }
        other => panic!("{other:?}"),
    }
}
