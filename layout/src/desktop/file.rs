//! File I/O wrapper for the desktop C API layer.
//!
//! Note: `layout/src/file.rs` provides a more complete file API with
//! proper error types (`FileError`) and a `FilePath` wrapper.

use alloc::sync::Arc;
use core::fmt;
use std::{
    fs,
    io::{Read, Write},
    sync::Mutex,
};

use azul_css::{impl_option, impl_option_inner, AzString, U8Vec};

/// Thread-safe file handle with path tracking for the C API.
#[repr(C)]
pub struct File {
    pub ptr: Box<Arc<Mutex<fs::File>>>,
    pub path: AzString,
    pub run_destructor: bool,
}

impl Clone for File {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
            path: self.path.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path.as_str())
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path.as_str())
    }
}

impl PartialEq for File {
    fn eq(&self, other: &Self) -> bool {
        self.path.as_str().eq(other.path.as_str())
    }
}

impl Eq for File {}

impl PartialOrd for File {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.path.as_str().partial_cmp(other.path.as_str())
    }
}

impl_option!(File, OptionFile, copy = false, [Clone, Debug]);

impl File {
    fn new(f: fs::File, path: AzString) -> Self {
        Self {
            ptr: Box::new(Arc::new(Mutex::new(f))),
            path,
            run_destructor: true,
        }
    }
    /// Opens a file in read-only mode, returning `None` on failure.
    #[must_use] pub fn open(path: &str) -> Option<Self> {
        Some(Self::new(
            fs::File::open(path).ok()?,
            path.to_string().into(),
        ))
    }
    /// Creates a file (truncating if it exists), returning `None` on failure.
    #[must_use] pub fn create(path: &str) -> Option<Self> {
        Some(Self::new(
            fs::File::create(path).ok()?,
            path.to_string().into(),
        ))
    }
    /// Reads the file at `self.path` into a string.
    pub fn read_to_string(&mut self) -> Option<AzString> {
        let file_string = fs::read_to_string(self.path.as_str()).ok()?;
        Some(file_string.into())
    }
    /// Reads the file at `self.path` into a byte vector.
    pub fn read_to_bytes(&mut self) -> Option<U8Vec> {
        let file_bytes = fs::read(self.path.as_str()).ok()?;
        Some(file_bytes.into())
    }
    /// Writes a string to the file handle. Returns `false` on failure.
    pub fn write_string(&mut self, string: &str) -> bool {
        self.write_bytes(string.as_bytes())
    }
    /// Writes bytes to the file handle and syncs to disk. Returns `false` on failure.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> bool {
        let Ok(mut lock) = self.ptr.lock() else {
            return false;
        };
        lock.write_all(bytes).is_ok() && lock.sync_all().is_ok()
    }
    /// Closes the file by dropping the handle. Provided for C API symmetry.
    pub fn close(self) {}
}

#[cfg(test)]
mod autotest_generated {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::path::PathBuf;

    use super::*;

    // NOTE ON THE TWO HALVES OF THIS TYPE
    //
    // `File` is really two things glued together:
    //   * a live `fs::File` handle behind `Arc<Mutex<_>>` -- this is what `write_bytes`
    //     (and therefore `write_string`) talks to, and
    //   * a `path: AzString` -- this is what `read_to_string` / `read_to_bytes` /
    //     `Debug` / `Display` / `PartialEq` / `PartialOrd` talk to.
    //
    // The two halves can disagree (path deleted under an open handle, path never
    // pointing at the handle at all -- `File::new` accepts *any* AzString), so most
    // tests below deliberately probe the seam rather than the happy path.

    /// A temp path that unlinks itself on drop, so a failing assert can't leak files.
    struct TempPath(PathBuf);

    impl TempPath {
        fn new(tag: &str) -> Self {
            Self(unique_temp_path(tag))
        }
        fn as_str(&self) -> &str {
            self.0.to_str().expect("temp dir must be valid UTF-8")
        }
        fn exists(&self) -> bool {
            self.0.exists()
        }
    }

    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn unique_temp_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        std::env::temp_dir().join(format!(
            "azul_autotest_file_{}_{}_{}_{}",
            std::process::id(),
            nanos,
            n,
            tag
        ))
    }

    /// A real, open `fs::File` handle whose path is already gone -- lets us build a
    /// `File` with a *completely arbitrary* `path` field for the fmt/cmp tests.
    fn detached_handle() -> fs::File {
        let path = unique_temp_path("detached");
        let handle = fs::File::create(&path).expect("temp dir must be writable");
        let _ = fs::remove_file(&path);
        handle
    }

    fn with_path(path: &str) -> File {
        File::new(detached_handle(), path.to_string().into())
    }

    // ---------------------------------------------------------------- File::new

    #[test]
    fn new_sets_fields_and_arms_the_destructor() {
        let temp = TempPath::new("new_fields");
        let handle = fs::File::create(temp.as_str()).expect("create");
        let file = File::new(handle, temp.as_str().to_string().into());

        assert_eq!(file.path.as_str(), temp.as_str());
        assert!(file.run_destructor, "a freshly built File must own its handle");
        assert_eq!(
            Arc::strong_count(&file.ptr),
            1,
            "an un-cloned File must be the sole owner of the handle"
        );
    }

    #[test]
    fn new_accepts_extreme_paths_without_panicking() {
        // `new` never touches the filesystem, so even a path that could never exist
        // must be stored verbatim rather than validated/normalized/truncated.
        for path in [
            "",
            "   ",
            "\u{0}embedded-nul",
            "\u{1F600}\u{0301}",
            "relative/../../../etc/passwd",
        ] {
            let file = with_path(path);
            assert_eq!(file.path.as_str(), path, "path must be stored verbatim");
            assert!(file.run_destructor);
        }

        let huge = "x".repeat(100_000);
        assert_eq!(with_path(&huge).path.as_str().len(), 100_000);
    }

    // --------------------------------------------------------------- File::open

    #[test]
    fn open_empty_path_returns_none() {
        assert!(File::open("").is_none());
    }

    #[test]
    fn open_whitespace_only_path_is_not_trimmed_into_something_valid() {
        for path in ["   ", "\t\n", " ", "\r\n\t "] {
            assert!(
                File::open(path).is_none(),
                "whitespace-only path {path:?} must not open anything"
            );
        }
    }

    #[test]
    fn open_missing_path_returns_none() {
        let temp = TempPath::new("never_created");
        assert!(!temp.exists());
        assert!(File::open(temp.as_str()).is_none());
    }

    #[test]
    fn open_garbage_path_returns_none_without_panicking() {
        // Interior NUL cannot be turned into a CString -> must surface as None, not a panic.
        for path in [
            "\u{0}",
            "abc\u{0}def",
            // NOTE: "//////" is deliberately NOT here -- POSIX collapses a run of
            // slashes to "/", and opening the root directory read-only legitimately
            // succeeds. It is not garbage.
            "::**?<>|\"",
            "\u{FFFD}\u{202E}\u{200B}",
        ] {
            assert!(
                File::open(path).is_none(),
                "garbage path {path:?} must return None"
            );
        }
    }

    #[test]
    fn open_boundary_number_paths_return_none() {
        for name in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "18446744073709551616",
            "NaN",
            "inf",
            "-inf",
            "1e400",
            "0.0000000000000000000001",
        ] {
            let path = std::env::temp_dir().join(format!("azul_autotest_missing_{name}"));
            let path = path.to_str().expect("utf-8 temp dir");
            assert!(
                File::open(path).is_none(),
                "numeric-looking missing path {path:?} must return None"
            );
        }
    }

    #[test]
    fn open_extremely_long_path_returns_none_and_terminates() {
        let path = "a".repeat(1_000_000);
        assert!(
            File::open(&path).is_none(),
            "a 1M-char path is over every OS limit and must fail cleanly"
        );
    }

    #[test]
    fn open_deeply_nested_path_does_not_stack_overflow() {
        // 10_000 nested segments -- exercises the "recursive input" case for a path parser.
        let mut path = String::with_capacity(20_002);
        for _ in 0..10_000 {
            path.push_str("a/");
        }
        path.push('x');
        assert!(File::open(&path).is_none());
    }

    #[test]
    fn open_directory_never_yields_readable_content() {
        // On unix, open(2) on a directory succeeds, so `File::open` hands back a `File`
        // whose handle is a directory. Reading through it must still fail cleanly.
        let dir = std::env::temp_dir();
        let dir = dir.to_str().expect("utf-8 temp dir");
        if let Some(mut file) = File::open(dir) {
            assert_eq!(file.path.as_str(), dir);
            assert!(
                file.read_to_string().is_none(),
                "a directory must not read back as a string"
            );
            assert!(
                file.read_to_bytes().is_none(),
                "a directory must not read back as bytes"
            );
        }
    }

    #[test]
    fn open_is_read_only_so_writes_fail_and_leave_content_intact() {
        let temp = TempPath::new("readonly");
        {
            let mut file = File::create(temp.as_str()).expect("create");
            assert!(file.write_string("original"));
        }

        let mut file = File::open(temp.as_str()).expect("open existing file");
        assert!(
            !file.write_string("clobbered"),
            "writing through a read-only handle must return false, not panic"
        );
        assert!(!file.write_bytes(b"clobbered"));
        assert_eq!(
            file.read_to_string().expect("read back").as_str(),
            "original",
            "a failed write must not have corrupted the file"
        );
    }

    // ------------------------------------------------------------- File::create

    #[test]
    fn create_empty_path_returns_none() {
        assert!(File::create("").is_none());
    }

    #[test]
    fn create_in_missing_directory_returns_none() {
        let dir = unique_temp_path("missing_dir");
        let path = dir.join("child.txt");
        let path = path.to_str().expect("utf-8 temp dir");
        assert!(File::create(path).is_none());
    }

    #[test]
    fn create_over_a_directory_returns_none() {
        let dir = std::env::temp_dir();
        let dir = dir.to_str().expect("utf-8 temp dir");
        assert!(
            File::create(dir).is_none(),
            "create() must refuse to truncate a directory"
        );
    }

    #[test]
    fn create_garbage_path_returns_none_without_panicking() {
        let over_long = "z".repeat(1_000_000);
        for path in ["\u{0}", "bad\u{0}name", "", over_long.as_str()] {
            assert!(
                File::create(path).is_none(),
                "create() of a {}-char garbage path must return None",
                path.len()
            );
        }
    }

    #[test]
    fn create_truncates_existing_content() {
        let temp = TempPath::new("truncate");
        {
            let mut file = File::create(temp.as_str()).expect("create");
            assert!(file.write_string("a fairly long pre-existing payload"));
        }

        let mut file = File::create(temp.as_str()).expect("re-create");
        assert_eq!(
            file.read_to_string().expect("read back").as_str(),
            "",
            "create() must truncate an existing file to zero bytes"
        );
    }

    // ------------------------------------------------------- write/read round-trips

    #[test]
    fn round_trip_representative_string() {
        let temp = TempPath::new("round_trip");
        let mut file = File::create(temp.as_str()).expect("create");
        assert!(file.write_string("hello world"));
        assert_eq!(file.read_to_string().expect("read").as_str(), "hello world");
        assert_eq!(file.read_to_bytes().expect("read").as_slice(), b"hello world");

        // ...and the same content survives a close + reopen cycle.
        file.close();
        let mut reopened = File::open(temp.as_str()).expect("reopen");
        assert_eq!(reopened.read_to_string().expect("read").as_str(), "hello world");
    }

    #[test]
    fn round_trip_empty_write_produces_an_empty_file() {
        let temp = TempPath::new("empty_write");
        let mut file = File::create(temp.as_str()).expect("create");

        assert!(file.write_bytes(&[]), "writing zero bytes must still succeed");
        assert!(file.write_string(""));
        assert_eq!(file.read_to_string().expect("read").as_str(), "");
        assert_eq!(file.read_to_bytes().expect("read").len(), 0);
    }

    #[test]
    fn round_trip_unicode_content() {
        let content = "\u{1F600} héllo e\u{0301} \u{202E}rtl\u{202C} \u{0}nul \r\n mixed";
        let temp = TempPath::new("unicode_content");
        let mut file = File::create(temp.as_str()).expect("create");

        assert!(file.write_string(content));
        assert_eq!(file.read_to_string().expect("read").as_str(), content);
        assert_eq!(
            file.read_to_bytes().expect("read").as_slice(),
            content.as_bytes(),
            "byte round-trip must be exact for multibyte content"
        );
    }

    #[test]
    fn round_trip_unicode_path() {
        let temp = TempPath::new("p\u{1F600}_h\u{0301}ll\u{00F6}");
        let mut file = File::create(temp.as_str()).expect("create with unicode path");

        assert_eq!(file.path.as_str(), temp.as_str());
        assert!(file.write_string("ok"));
        assert_eq!(file.read_to_string().expect("read").as_str(), "ok");

        let reopened = File::open(temp.as_str()).expect("reopen unicode path");
        assert_eq!(reopened.path.as_str(), temp.as_str());
        assert_eq!(reopened, file, "equality is path-based, so these must match");
    }

    #[test]
    fn round_trip_all_256_byte_values_and_invalid_utf8_reads_as_none() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        let temp = TempPath::new("all_bytes");
        let mut file = File::create(temp.as_str()).expect("create");

        assert!(file.write_bytes(&bytes));
        assert_eq!(
            file.read_to_bytes().expect("read").as_slice(),
            bytes.as_slice(),
            "every byte value must survive the round-trip untouched"
        );
        assert!(
            file.read_to_string().is_none(),
            "non-UTF-8 content must return None, never a lossy string or a panic"
        );
    }

    #[test]
    fn round_trip_one_mib_payload() {
        let bytes = vec![0xABu8; 1024 * 1024];
        let temp = TempPath::new("one_mib");
        let mut file = File::create(temp.as_str()).expect("create");

        assert!(file.write_bytes(&bytes));
        let read = file.read_to_bytes().expect("read");
        assert_eq!(read.len(), bytes.len());
        assert_eq!(read.as_slice(), bytes.as_slice());
    }

    #[test]
    fn writes_append_at_the_handle_cursor_rather_than_overwriting() {
        let temp = TempPath::new("append_cursor");
        let mut file = File::create(temp.as_str()).expect("create");

        assert!(file.write_string("a"));
        assert!(file.write_string("bc"));
        assert!(file.write_bytes(b"d"));
        assert_eq!(
            file.read_to_string().expect("read").as_str(),
            "abcd",
            "consecutive writes advance the cursor; none of them rewind"
        );
    }

    #[test]
    fn reads_follow_the_path_not_the_handle() {
        // The seam: `read_*` re-opens `self.path` from scratch, so out-of-band changes
        // to the path are visible and the handle's cursor position is irrelevant.
        let temp = TempPath::new("path_vs_handle");
        let mut file = File::create(temp.as_str()).expect("create");
        assert!(file.write_string("written through the handle"));

        fs::write(temp.as_str(), "replaced out of band").expect("out-of-band write");
        assert_eq!(
            file.read_to_string().expect("read").as_str(),
            "replaced out of band"
        );
    }

    #[test]
    fn reads_return_none_after_the_path_is_deleted_under_an_open_handle() {
        let temp = TempPath::new("deleted_under_handle");
        let mut file = File::create(temp.as_str()).expect("create");
        assert!(file.write_string("content"));

        fs::remove_file(temp.as_str()).expect("unlink");
        assert!(
            file.read_to_string().is_none(),
            "reads go through the path, which is now gone"
        );
        assert!(file.read_to_bytes().is_none());
        // The handle itself is still alive, so writes to it must still succeed.
        assert!(
            file.write_string("still writable"),
            "an unlinked-but-open handle is still writable on unix"
        );
    }

    #[test]
    fn read_and_write_on_a_file_whose_path_never_existed() {
        // `File::new` is happy to pair a live handle with a bogus path. Reads must then
        // fail cleanly while writes (which use the handle) still work.
        let mut file = with_path("/this/path/does/not/exist\u{1F600}");
        assert!(file.read_to_string().is_none());
        assert!(file.read_to_bytes().is_none());
        assert!(file.write_string("goes to the detached handle"));

        let mut empty_path = with_path("");
        assert!(empty_path.read_to_string().is_none());
        assert!(empty_path.read_to_bytes().is_none());
    }

    #[test]
    fn write_bytes_returns_false_on_a_poisoned_mutex() {
        let temp = TempPath::new("poisoned");
        let mut file = File::create(temp.as_str()).expect("create");

        let arc = (*file.ptr).clone();
        let joined = std::thread::spawn(move || {
            let _guard = arc.lock().expect("first lock cannot be poisoned");
            panic!("intentional panic to poison the file mutex");
        })
        .join();
        assert!(joined.is_err(), "the helper thread must have panicked");

        assert!(
            !file.write_bytes(b"nope"),
            "a poisoned mutex must surface as `false`, not as an unwrap panic"
        );
        assert!(!file.write_string("nope"));
    }

    // --------------------------------------------------- Clone / Drop / close

    #[test]
    fn clone_shares_the_handle_and_the_path() {
        let temp = TempPath::new("clone_shares");
        let mut file = File::create(temp.as_str()).expect("create");
        let mut clone = file.clone();

        assert_eq!(Arc::strong_count(&file.ptr), 2, "clone must share the Arc");
        assert!(clone.run_destructor);
        assert_eq!(clone, file);

        // Both halves write into the same cursor, so the writes interleave in order.
        assert!(file.write_string("a"));
        assert!(clone.write_string("b"));
        assert!(file.write_string("c"));
        assert_eq!(file.read_to_string().expect("read").as_str(), "abc");
    }

    #[test]
    fn close_drops_only_one_owner_and_leaves_the_content_on_disk() {
        let temp = TempPath::new("close");
        let mut file = File::create(temp.as_str()).expect("create");
        assert!(file.write_string("persisted"));

        let clone = file.clone();
        clone.close();
        assert_eq!(
            Arc::strong_count(&file.ptr),
            1,
            "closing a clone must release exactly one Arc reference"
        );

        // The surviving handle still works, and the bytes are still on disk.
        assert!(file.write_string("!"));
        assert_eq!(file.read_to_string().expect("read").as_str(), "persisted!");

        file.close();
        let mut reopened = File::open(temp.as_str()).expect("reopen after close");
        assert_eq!(reopened.read_to_string().expect("read").as_str(), "persisted!");
    }

    #[test]
    fn close_on_an_extreme_file_does_not_panic() {
        with_path("").close();
        with_path(&"q".repeat(100_000)).close();
        with_path("\u{1F600}\u{0}\u{202E}").close();
    }

    // ------------------------------------------------- Debug / Display / Eq / Ord

    #[test]
    fn debug_and_display_render_exactly_the_path() {
        // Includes format-specifier-looking payloads: these must be printed literally,
        // never interpreted, and never truncated.
        for path in [
            "/tmp/plain.txt",
            "",
            "{}",
            "{0} {:?} %s %n",
            "\u{1F600} h\u{00E9}llo e\u{0301}",
            "line\nbreak\ttab",
        ] {
            let file = with_path(path);
            assert_eq!(format!("{file}"), path, "Display must echo the path verbatim");
            assert_eq!(format!("{file:?}"), path, "Debug must echo the path verbatim");
            assert_eq!(
                format!("{file}"),
                format!("{file:?}"),
                "Debug and Display must agree"
            );
        }
    }

    #[test]
    fn display_is_non_empty_for_a_real_file_and_idempotent() {
        let temp = TempPath::new("display_stable");
        let file = File::create(temp.as_str()).expect("create");

        let once = format!("{file}");
        assert!(!once.is_empty());
        assert_eq!(once, temp.as_str());
        // Re-rendering the re-parsed value is stable (serialize . parse . serialize == serialize).
        let reopened = File::open(&once).expect("the rendered path must reopen the same file");
        assert_eq!(format!("{reopened}"), once);
        assert_eq!(reopened, file);
    }

    #[test]
    fn display_of_a_huge_path_is_not_truncated() {
        let path = "w".repeat(300_000);
        let file = with_path(&path);
        assert_eq!(format!("{file}").len(), 300_000);
    }

    #[test]
    fn eq_and_ord_are_purely_path_based() {
        let a1 = with_path("a");
        let a2 = with_path("a"); // different handle, same path
        let b = with_path("b");

        assert_eq!(a1, a2, "equality ignores the handle entirely");
        assert_ne!(a1, b);
        assert_eq!(a1.partial_cmp(&a2), Some(core::cmp::Ordering::Equal));
        assert_eq!(a1.partial_cmp(&b), Some(core::cmp::Ordering::Less));
        assert_eq!(b.partial_cmp(&a1), Some(core::cmp::Ordering::Greater));
        assert!(a1 < b);

        // Ordering must track `str` ordering (byte-wise), including for multibyte paths.
        let empty = with_path("");
        let emoji = with_path("\u{1F600}");
        assert_eq!(empty.partial_cmp(&emoji), "".partial_cmp("\u{1F600}"));
        assert_eq!(emoji.partial_cmp(&empty), "\u{1F600}".partial_cmp(""));
        assert_eq!(emoji.partial_cmp(&emoji.clone()), Some(core::cmp::Ordering::Equal));

        // Reflexive / symmetric on a clone (which shares the handle).
        let cloned = a1.clone();
        assert_eq!(a1, cloned);
        assert_eq!(cloned, a1);
    }

    #[test]
    fn option_file_round_trips_through_the_ffi_option() {
        assert!(matches!(OptionFile::default(), OptionFile::None));
        assert!(OptionFile::default().as_option().is_none());

        let none: OptionFile = Option::<File>::None.into();
        assert!(Option::<File>::from(none).is_none());

        let file = with_path("\u{1F600}/round/trip");
        let ffi: OptionFile = Some(file).into();
        assert_eq!(
            ffi.as_option().expect("Some").path.as_str(),
            "\u{1F600}/round/trip"
        );

        let back = Option::<File>::from(ffi).expect("Some survives the round-trip");
        assert_eq!(back.path.as_str(), "\u{1F600}/round/trip");
        assert!(back.run_destructor);
    }
}
