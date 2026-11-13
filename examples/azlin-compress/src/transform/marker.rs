//! The poison pill (ARCH28 RF50 / AR5b-5): what makes `panic = "abort"` survivable.
//!
//! A transform analyses a file's deflate streams and JPEG images with code that other people's
//! files steer (the vendored preflate port, Lepton). The shipped profile aborts on a panic, so a
//! file that makes the analysis panic ends the whole application - and the next idle pass reads
//! the same file again: a crash loop. Before the analysis starts, a marker named by the file's
//! BLAKE3 is written (and synced) into a folder the application keeps; when the analysis returns,
//! the marker goes. A marker found when an analysis is about to start means the last one did not
//! come back: after [`ANALYSIS_TRIES`] of those the file is stored as it is and the program does
//! not enter the code that ended it again.
//!
//! The marker holds no part of the file, only its hash. A folder that cannot be written is no
//! protection and no obstacle: the analysis runs as before.
//!
//! A marker found does not mean "poison" at once (X-DELTA-AZDRIVE-KEYS-44): a quit, a kill, a
//! shutdown or an out-of-memory during an analysis that cannot be cancelled leaves one too, and a
//! file that was only interrupted must not be out of the transform for ever. The file counts the
//! analyses of its content that did not come back (`analysing 2`); a content is refused after
//! [`ANALYSIS_TRIES`] of them, and one that comes back clean erases its record (it proves the
//! content does not end the program). A record older than [`MARKER_TTL`] is not one any more, so
//! a crash loop is bounded - [`ANALYSIS_TRIES`] aborts a month at the worst - and nothing is shut
//! out for ever; [`sweep`] drops the records that expired (the application runs it when it names
//! the folder), so the folder does not collect the trace of every kill. The records are plain
//! files, `<BLAKE3 of the content>.analysing` holding `analysing <tries>`, for a person to list or
//! to delete. Two analyses of one content under way in one process (two threads of a sync) are one
//! try, and the record goes when the LAST of them returns. A program that is quit on purpose does
//! not take its markers back: a thread of it can still be inside the very analysis that ends
//! programs, and the count already forgives a quit.
//!
//! The READ path has the same hole (X-DELTA-AZDRIVE-KEYS-15): decoding a stored transform runs
//! Lepton's decoder and the preflate rebuild over bytes that any member of the drive wrote (it holds
//! the file key, so "authenticated" says nothing of what is inside), and `catch_unwind` around them
//! is dead under `panic = "abort"`. A member who stores a malformed stream ends every device that
//! reads it, and the sync reads it again after each restart. The decode is bracketed by a marker
//! of the stream's hash too, with a count: the first try that does not come back leaves `1`, and
//! a read that finds the marker counts up and goes on - an app that was only quit or killed
//! meanwhile reads the file all the same, and removes the marker - until a stream has ended the
//! program [`DECODE_TRIES`] times: then it is refused by name instead of looped on. The first abort
//! is not prevented: that needs a process of its own for the decoder, with a time limit - AR4's
//! option 2, which ARCH28 Q24 leaves unbuilt ("not now: process isolation is a product-sized
//! change; panics are removed first"). That residual risk stands: ONE crash per malformed stream
//! (and a stream is as many streams as a member cares to vary its bytes), then at most
//! [`DECODE_TRIES`] in all for each of them.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, SystemTime},
};

/// What entering the analysis of a file came to.
pub(crate) enum Entered {
    /// The marker is written: the analysis may run, and the marker goes with `Drop`.
    Clear(Marker),
    /// The content has ended the program [`ANALYSIS_TRIES`] (or [`DECODE_TRIES`]) times: it is not
    /// entered again.
    Left,
    /// No marker could be written (no folder, no room): the analysis runs without one.
    Unavailable,
}

/// How many decodes of the same stream may begin without having come back before the next is
/// refused ([`Marker::enter_decoding`]).
pub(crate) const DECODE_TRIES: u32 = 3;

/// How many analyses of the same content may begin without having come back before the next is
/// refused ([`Marker::enter`]): one that was only quit or killed is one try, not a verdict.
pub(crate) const ANALYSIS_TRIES: u32 = 3;

/// How long a record of aborted tries stands: an abort of a month ago was a kill, a shutdown or an
/// update and not a pattern, so the count begins again. Bounds a crash loop to [`ANALYSIS_TRIES`]
/// aborts a month, and keeps a content from being shut out for ever.
pub(crate) const MARKER_TTL: Duration = Duration::from_secs(30 * 24 * 3600);

/// How far ahead of the clock a marker's stamp may be before it is no record (a clock that was
/// set back, a file system with another clock): a stamp from the future is stale, never fresh.
const FUTURE_SLACK: Duration = Duration::from_secs(24 * 3600);

/// How much of a marker file is read: `analysing 3` is a dozen bytes, anything longer is a stray
/// file of the same name.
const RECORD_LIMIT: u64 = 64;

/// How many entries of the folder one [`sweep`] looks at (the folder holds the trace of an abort
/// each, a handful; a folder with more than this is not one the pill made).
const SWEEP_LIMIT: usize = 10_000;

/// The kind of work a marker stands for; its file name's suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The analysis of a file for a transform (Lepton's encoder, the deflate analysis).
    Analysing,
    /// The decode of a stored transform.
    Decoding,
}

impl Kind {
    fn suffix(self) -> &'static str {
        match self {
            Kind::Analysing => "analysing",
            Kind::Decoding => "decoding",
        }
    }

    fn tries(self) -> u32 {
        match self {
            Kind::Analysing => ANALYSIS_TRIES,
            Kind::Decoding => DECODE_TRIES,
        }
    }
}

/// The marker of an analysis (or a decode) under way; the record goes when the last one of this
/// process for its content is dropped (it came back).
pub(crate) struct Marker {
    path: PathBuf,
}

/// The markers this process holds, by file, and how many of its analyses run under each: they are
/// one try, and the last one to return takes the file away. A file this process does not hold is
/// the record of an earlier run (or of another process) and is not its to remove.
static HELD: Mutex<BTreeMap<PathBuf, u32>> = Mutex::new(BTreeMap::new());

fn held_markers() -> MutexGuard<'static, BTreeMap<PathBuf, u32>> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The marker's file name for the analysis of `input`.
pub(crate) fn marker_name(input: &[u8]) -> String {
    format!(
        "{}.{}",
        blake3::hash(input).to_hex(),
        Kind::Analysing.suffix()
    )
}

/// The marker's file name for the decode of `input`.
pub(crate) fn decoding_marker_name(input: &[u8]) -> String {
    format!(
        "{}.{}",
        blake3::hash(input).to_hex(),
        Kind::Decoding.suffix()
    )
}

/// Whether a record stamped `modified` still stands at `now`: not older than [`MARKER_TTL`], and
/// not stamped from the future beyond [`FUTURE_SLACK`] (a stamp a blink ahead is clocks that
/// differ and is fresh; one far ahead is a clock that was set back and is stale).
fn is_fresh(modified: SystemTime, now: SystemTime) -> bool {
    match now.duration_since(modified) {
        Ok(age) => age <= MARKER_TTL,
        Err(ahead) => ahead.duration() <= FUTURE_SLACK,
    }
}

/// The tries on record in the marker file at `path`: `Ok(0)` when there is none, or it is older
/// than [`MARKER_TTL`] (or from the future); a file that is not a count is one try. An error:
/// the file cannot be read.
fn recorded_tries(path: &Path, kind: Kind, now: SystemTime) -> std::io::Result<u32> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    // A stamp that cannot be read is no fresh record either: the content is counted anew.
    if !file.metadata()?.modified().is_ok_and(|m| is_fresh(m, now)) {
        return Ok(0);
    }
    let mut bytes = Vec::new();
    file.take(RECORD_LIMIT).read_to_end(&mut bytes)?;
    // collapse-ok: bytes that are not a count (a stray file of the same name, the record of the
    // first build, which held the file's name) are one try on record, never "no record".
    Ok(std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| text.trim().strip_prefix(kind.suffix()))
        .and_then(|n| n.trim().parse::<u32>().ok())
        .unwrap_or(1))
}

impl Marker {
    /// Writes (or counts up) the marker of the analysis of `input` into `dir` before it runs.
    pub(crate) fn enter(dir: &Path, input: &[u8]) -> Entered {
        Marker::enter_kind(dir, marker_name(input), Kind::Analysing)
    }

    /// Writes (or counts up) the marker of the DECODE of the stream `input` into `dir`. The file
    /// says how many decodes of this stream began without coming back; one more is let in until
    /// [`DECODE_TRIES`] have, then `Left`. A marker that cannot be read or written is no
    /// protection and no obstacle (`Unavailable`).
    pub(crate) fn enter_decoding(dir: &Path, input: &[u8]) -> Entered {
        Marker::enter_kind(dir, decoding_marker_name(input), Kind::Decoding)
    }

    fn enter_kind(dir: &Path, name: String, kind: Kind) -> Entered {
        if fs::create_dir_all(dir).is_err() {
            return Entered::Unavailable;
        }
        let path = dir.join(name);
        let mut held = held_markers();
        if let Some(running) = held.get_mut(&path) {
            // Another analysis of this very content runs in this process (two threads of one
            // sync): the same try, whose record is on disk already.
            *running += 1;
            return Entered::Clear(Marker { path });
        }
        let before = match recorded_tries(&path, kind, SystemTime::now()) {
            Ok(tries) => tries,
            Err(_) => return Entered::Unavailable,
        };
        if before >= kind.tries() {
            return Entered::Left;
        }
        let written = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .and_then(|mut file| {
                file.write_all(format!("{} {}\n", kind.suffix(), before + 1).as_bytes())?;
                file.sync_all()
            });
        match written {
            Ok(()) => {
                *held.entry(path.clone()).or_insert(0) += 1;
                Entered::Clear(Marker { path })
            }
            Err(_) => Entered::Unavailable,
        }
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        let mut held = held_markers();
        let last = match held.get_mut(&self.path) {
            Some(running) if *running > 1 => {
                *running -= 1;
                false
            }
            // The last of this process's analyses of the content returned: the record goes.
            Some(_) => true,
            // Not held: the record is not this process's to remove.
            None => false,
        };
        if last {
            held.remove(&self.path);
            // collapse-ok: a marker that cannot be removed only keeps one try counted.
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Whether `name` is the name of a marker file: the BLAKE3 of a content in hex and a kind.
fn is_marker_file(name: &str) -> bool {
    [Kind::Analysing, Kind::Decoding].into_iter().any(|kind| {
        name.strip_suffix(kind.suffix())
            .and_then(|stem| stem.strip_suffix('.'))
            .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
    })
}

/// Removes the markers in `dir` that expired ([`MARKER_TTL`]); how many went. The application runs
/// it when it names the folder, so the folder does not collect the records of every kill for
/// ever. Only files named like a marker are looked at.
pub fn sweep(dir: &Path) -> usize {
    sweep_at(dir, SystemTime::now())
}

fn sweep_at(dir: &Path, now: SystemTime) -> usize {
    // collapse-ok: a folder that cannot be read has nothing to sweep, and an entry that cannot be
    // read is looked at by the next start.
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten().take(SWEEP_LIMIT) {
        if !is_marker_file(&entry.file_name().to_string_lossy()) {
            continue;
        }
        // A stamp that cannot be read is no fresh record either.
        let fresh = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| is_fresh(modified, now));
        // collapse-ok: a marker that cannot be removed is swept by the next start.
        if !fresh && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("azlin-marker-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Sets the stamp of the marker file `path` to `when`.
    fn stamp(path: &Path, when: SystemTime) {
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    /// The process ends inside the analysis (an abort, a kill): its marker stays on disk and the
    /// process's own memory of it goes with the process.
    fn die_inside(marker: Marker) {
        held_markers().remove(&marker.path);
        std::mem::forget(marker);
    }

    fn on_disk(dir: &Path, name: &str) -> String {
        fs::read_to_string(dir.join(name))
            .unwrap()
            .trim()
            .to_string()
    }

    /// The analysis that did not come back (the process ended inside it) leaves its marker with a
    /// count; one that came back leaves none. A content that has ended the program ANALYSIS_TRIES
    /// times is not analysed again - one that was only interrupted once is (X-DELTA-AZDRIVE-KEYS-44:
    /// a marker meant "poison" at the first, and a quit shut the file out of the transform for
    /// ever).
    #[test]
    fn a_content_that_ended_the_program_three_times_is_not_analysed_a_fourth_and_one_quit_is_forgiven(
    ) {
        let dir = folder("left");
        let file = b"a file that ends the program";
        let name = marker_name(file);
        let Entered::Clear(marker) = Marker::enter(&dir, file) else {
            panic!("the first analysis is let in");
        };
        assert_eq!(
            on_disk(&dir, &name),
            "analysing 1",
            "written before the analysis"
        );
        die_inside(marker);
        // One try on record is no verdict: the next run analyses the file again, and the analysis
        // that comes back erases the record - the content does not end the program.
        let Entered::Clear(second) = Marker::enter(&dir, file) else {
            panic!("a file that was interrupted once is let in");
        };
        assert_eq!(on_disk(&dir, &name), "analysing 2");
        drop(second);
        assert!(
            !dir.join(&name).exists(),
            "it came back: the record of the earlier abort goes with it"
        );

        // A content that ended the program ANALYSIS_TRIES times is out of the analysis.
        for tries in 1..=ANALYSIS_TRIES {
            let Entered::Clear(marker) = Marker::enter(&dir, file) else {
                panic!("try {tries} is let in");
            };
            die_inside(marker);
        }
        assert!(matches!(Marker::enter(&dir, file), Entered::Left));
        // Another file is let in.
        assert!(matches!(
            Marker::enter(&dir, b"another file"),
            Entered::Clear(_)
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_cannot_be_made_is_no_protection_and_no_obstacle() {
        let dir = folder("blocked");
        fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("file");
        fs::write(&blocker, b"x").unwrap();
        // A folder below a file cannot be made.
        assert!(matches!(
            Marker::enter(&blocker.join("below"), b"data"),
            Entered::Unavailable
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    /// X-DELTA-AZDRIVE-KEYS-15: a decode that returned leaves nothing; one that did not leaves a
    /// count, the next try counts it up and goes on, and a stream that ended the program
    /// DECODE_TRIES times is refused - a crash loop stops, an app that was only quit once does
    /// not lock its file out.
    #[test]
    fn a_stream_that_ended_the_program_three_times_is_not_decoded_a_fourth() {
        let dir = folder("decoding");
        let stream = b"a stream that ends the program";
        // A decode that comes back leaves no marker, however often it runs.
        for _ in 0..5 {
            let Entered::Clear(marker) = Marker::enter_decoding(&dir, stream) else {
                panic!("a decode that came back is let in again");
            };
            drop(marker);
            assert!(!dir.join(decoding_marker_name(stream)).exists());
        }
        // Decodes that do not come back (the process ends inside them, as an abort does).
        for tries in 1..=DECODE_TRIES {
            let Entered::Clear(marker) = Marker::enter_decoding(&dir, stream) else {
                panic!("try {tries} is let in");
            };
            die_inside(marker);
            assert_eq!(
                on_disk(&dir, &decoding_marker_name(stream)),
                format!("decoding {tries}")
            );
        }
        assert!(matches!(
            Marker::enter_decoding(&dir, stream),
            Entered::Left
        ));
        // Another stream is let in, and the analysis marker of the same bytes is another file.
        assert!(matches!(
            Marker::enter_decoding(&dir, b"another"),
            Entered::Clear(_)
        ));
        assert!(matches!(Marker::enter(&dir, stream), Entered::Clear(_)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stray_file_of_the_markers_name_counts_as_one_try_and_a_folder_that_cannot_be_made_blocks_nothing(
    ) {
        let dir = folder("decoding-stray");
        let stream = b"stream";
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(decoding_marker_name(stream)), "not a count").unwrap();
        let Entered::Clear(marker) = Marker::enter_decoding(&dir, stream) else {
            panic!("a stray file is one try, not a refusal");
        };
        die_inside(marker);
        assert_eq!(on_disk(&dir, &decoding_marker_name(stream)), "decoding 2");
        let blocker = dir.join("file");
        fs::write(&blocker, b"x").unwrap();
        assert!(matches!(
            Marker::enter_decoding(&blocker.join("below"), b"data"),
            Entered::Unavailable
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    /// X-DELTA-AZDRIVE-KEYS-44: a record an older build wrote (`analysing <name>`, which meant
    /// "poison" at once) is one try now, not a refusal for ever; so is a file of the marker's
    /// name that is not text (a stray file is one try, never "no record" and never an error).
    #[test]
    fn the_record_of_the_older_build_and_a_stray_file_are_one_try() {
        let dir = folder("older");
        let file = b"a file the first build marked";
        fs::create_dir_all(&dir).unwrap();
        let name = marker_name(file);
        fs::write(dir.join(&name), format!("analysing {name}\n")).unwrap();
        let Entered::Clear(marker) = Marker::enter(&dir, file) else {
            panic!("the old build's marker is one try");
        };
        assert_eq!(on_disk(&dir, &name), "analysing 2");
        drop(marker);

        let binary = b"a binary stray file";
        fs::write(dir.join(marker_name(binary)), [0xffu8, 0xfe, 0x00, 0x80]).unwrap();
        let Entered::Clear(marker) = Marker::enter(&dir, binary) else {
            panic!("a stray binary file is one try, not a refusal and not no protection");
        };
        assert_eq!(on_disk(&dir, &marker_name(binary)), "analysing 2");
        drop(marker);
        let _ = fs::remove_dir_all(&dir);
    }

    /// X-DELTA-AZDRIVE-KEYS-44: two analyses of one content under way in this process (two
    /// threads of one sync) are ONE try: the file stays until the LAST of them returns, so the
    /// second still has its marker when the first is done, and when the process ends inside them
    /// it says one try and not two. The old code removed the file when the FIRST dropped.
    #[test]
    fn the_last_of_two_overlapping_analyses_of_one_content_takes_the_marker_away() {
        let dir = folder("overlap");
        let file = b"one content, two threads";
        let name = marker_name(file);
        let Entered::Clear(first) = Marker::enter(&dir, file) else {
            panic!("let in");
        };
        let Entered::Clear(second) = Marker::enter(&dir, file) else {
            panic!("let in");
        };
        assert_eq!(on_disk(&dir, &name), "analysing 1", "one try, two threads");
        drop(first);
        assert!(dir.join(&name).exists(), "the second is still under way");
        drop(second);
        assert!(!dir.join(&name).exists());

        // The process ends inside both: one try on record.
        let Entered::Clear(first) = Marker::enter(&dir, file) else {
            panic!("let in");
        };
        let Entered::Clear(second) = Marker::enter(&dir, file) else {
            panic!("let in");
        };
        die_inside(first);
        die_inside(second);
        assert_eq!(on_disk(&dir, &name), "analysing 1");
        let _ = fs::remove_dir_all(&dir);
    }

    /// X-DELTA-AZDRIVE-KEYS-44: a marker this process does not hold is the record of an earlier
    /// run and not its to remove: a `Marker` whose file was counted up by the next run (or whose
    /// memory of it is gone) leaves the file alone when it is dropped.
    #[test]
    fn a_marker_that_is_no_longer_held_leaves_the_file_alone() {
        let dir = folder("not-held");
        let file = b"a file whose record belongs to someone else";
        let name = marker_name(file);
        let Entered::Clear(marker) = Marker::enter(&dir, file) else {
            panic!("let in");
        };
        held_markers().remove(&marker.path);
        drop(marker);
        assert_eq!(
            on_disk(&dir, &name),
            "analysing 1",
            "the record of a run that is no longer this one stays"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// X-DELTA-AZDRIVE-KEYS-44: a record older than MARKER_TTL is no record - a kill of a month ago
    /// does not count - and one stamped from the future (a clock set back) is stale as well, but a
    /// stamp a blink ahead (another clock) is fresh.
    #[test]
    fn a_record_that_expired_or_comes_from_the_future_is_not_one() {
        let dir = folder("expiry");
        let file = b"an old abort";
        let path = dir.join(marker_name(file));
        let now = SystemTime::now();
        // Used up, and fresh: refused.
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, format!("analysing {ANALYSIS_TRIES}\n")).unwrap();
        assert!(matches!(Marker::enter(&dir, file), Entered::Left));
        // The same record a month and a day old: the count begins again.
        stamp(&path, now - MARKER_TTL - Duration::from_secs(24 * 3600));
        let Entered::Clear(marker) = Marker::enter(&dir, file) else {
            panic!("an expired record is let in");
        };
        assert_eq!(
            fs::read_to_string(&path).unwrap().trim(),
            "analysing 1",
            "counted anew"
        );
        die_inside(marker);
        // Used up, stamped far ahead of the clock: stale. A little ahead: fresh.
        fs::write(&path, format!("analysing {ANALYSIS_TRIES}\n")).unwrap();
        stamp(&path, now + FUTURE_SLACK + Duration::from_secs(3600));
        let Entered::Clear(marker) = Marker::enter(&dir, file) else {
            panic!("a record from the far future is stale");
        };
        die_inside(marker);
        fs::write(&path, format!("analysing {ANALYSIS_TRIES}\n")).unwrap();
        stamp(&path, now + Duration::from_secs(5));
        assert!(matches!(Marker::enter(&dir, file), Entered::Left));
        let _ = fs::remove_dir_all(&dir);
    }

    /// X-DELTA-AZDRIVE-KEYS-44: the sweep drops the markers that expired (or come from the far
    /// future) of both kinds and nothing else: not a fresh record, not a file that is no marker
    /// (however old), not a marker that is held; a folder that is not there has nothing to sweep.
    #[test]
    fn the_sweep_drops_the_markers_that_expired_and_nothing_else() {
        let dir = folder("sweep");
        fs::create_dir_all(&dir).unwrap();
        let now = SystemTime::now();
        let old = now - MARKER_TTL - Duration::from_secs(3600);
        let used_up = marker_name(b"used up");
        let once = decoding_marker_name(b"once");
        let expired = marker_name(b"expired");
        let expired_decode = decoding_marker_name(b"expired decode");
        let ahead = marker_name(b"ahead");
        fs::write(dir.join(&used_up), format!("analysing {ANALYSIS_TRIES}\n")).unwrap();
        fs::write(dir.join(&once), "decoding 1\n").unwrap();
        for name in [&expired, &expired_decode] {
            fs::write(dir.join(name), "analysing 2\n").unwrap();
            stamp(&dir.join(name), old);
        }
        fs::write(dir.join(&ahead), "analysing 1\n").unwrap();
        stamp(
            &dir.join(&ahead),
            now + FUTURE_SLACK + Duration::from_secs(3600),
        );
        // Old, and named almost like a marker: not one.
        for name in [
            "something-else.txt",
            "short.analysing",
            "not-a-hash.decoding",
        ] {
            fs::write(dir.join(name), "not a marker").unwrap();
            stamp(&dir.join(name), old);
        }

        assert_eq!(sweep_at(&dir, now), 3);
        for gone in [&expired, &expired_decode, &ahead] {
            assert!(!dir.join(gone).exists(), "{gone} expired");
        }
        for kept in [&used_up, &once] {
            assert!(dir.join(kept).exists(), "{kept} is a fresh record");
        }
        for kept in [
            "something-else.txt",
            "short.analysing",
            "not-a-hash.decoding",
        ] {
            assert!(
                dir.join(kept).exists(),
                "{kept} is no marker: only markers go"
            );
        }
        assert_eq!(sweep(&dir.join("no-such-folder")), 0);

        // `sweep` itself (the clock of the process): the fresh marker an analysis under way holds
        // stays.
        let Entered::Clear(marker) = Marker::enter(&dir, b"under way") else {
            panic!("let in");
        };
        assert_eq!(sweep(&dir), 0);
        assert!(dir.join(marker_name(b"under way")).exists());
        drop(marker);
        let _ = fs::remove_dir_all(&dir);
    }
}
