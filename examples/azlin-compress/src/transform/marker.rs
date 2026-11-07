//! The poison pill (ARCH28 RF50 / AR5b-5): what makes `panic = "abort"` survivable.
//!
//! A transform analyses a file's deflate streams and JPEG images with code that other people's
//! files steer (the vendored preflate port, Lepton). The shipped profile aborts on a panic, so a
//! file that makes the analysis panic ends the whole application - and the next idle pass reads
//! the same file again: a crash loop. Before the analysis starts, a marker named by the file's
//! BLAKE3 is written (and synced) into a folder the application keeps; when the analysis returns,
//! the marker goes. A marker found when an analysis is about to start means the last one did not
//! come back: that file is stored as it is, from then on, and the program does not enter the code
//! that ended it again.
//!
//! The marker holds no part of the file, only its hash. A folder that cannot be written is no
//! protection and no obstacle: the analysis runs as before.
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
//! is not prevented: that needs a process of its own for the decoder.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// What entering the analysis of a file came to.
pub(crate) enum Entered {
    /// The marker is written: the analysis may run, and the marker goes with `Drop`.
    Clear(Marker),
    /// A marker of this very file is there already: an earlier analysis of it did not return.
    Left,
    /// No marker could be written (no folder, no room): the analysis runs without one.
    Unavailable,
}

/// How many decodes of the same stream may begin without having come back before the next is
/// refused ([`Marker::enter_decoding`]).
pub(crate) const DECODE_TRIES: u32 = 3;

/// The marker of an analysis under way; removed when it is dropped.
pub(crate) struct Marker {
    path: PathBuf,
}

/// The marker's file name for `input`.
pub(crate) fn marker_name(input: &[u8]) -> String {
    format!("{}.analysing", blake3::hash(input).to_hex())
}

impl Marker {
    /// Writes the marker of `input` into `dir` before its analysis.
    pub(crate) fn enter(dir: &Path, input: &[u8]) -> Entered {
        if fs::create_dir_all(dir).is_err() {
            return Entered::Unavailable;
        }
        let name = marker_name(input);
        let path = dir.join(&name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                // collapse-ok: the marker's content is a label for a person; its existence is the
                // protection, and a failed write or sync leaves the file there.
                let _ = file.write_all(format!("analysing {name}\n").as_bytes());
                let _ = file.sync_all();
                Entered::Clear(Marker { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Entered::Left,
            Err(_) => Entered::Unavailable,
        }
    }
}

/// The marker's file name for the decode of `input`.
pub(crate) fn decoding_marker_name(input: &[u8]) -> String {
    format!("{}.decoding", blake3::hash(input).to_hex())
}

impl Marker {
    /// Writes (or counts up) the marker of the DECODE of the stream `input` into `dir`. The file
    /// says how many decodes of this stream began without coming back; one more is let in until
    /// [`DECODE_TRIES`] have, then `Left`. A marker that cannot be read or written is no
    /// protection and no obstacle (`Unavailable`).
    pub(crate) fn enter_decoding(dir: &Path, input: &[u8]) -> Entered {
        if fs::create_dir_all(dir).is_err() {
            return Entered::Unavailable;
        }
        let path = dir.join(decoding_marker_name(input));
        let before = match fs::read_to_string(&path) {
            Ok(text) => text
                .trim()
                .strip_prefix("decoding ")
                .and_then(|n| n.parse::<u32>().ok())
                // Not a count (a stray file of the same name): one try is on record.
                .unwrap_or(1),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(_) => return Entered::Unavailable,
        };
        if before >= DECODE_TRIES {
            return Entered::Left;
        }
        let written = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .and_then(|mut file| {
                file.write_all(format!("decoding {}\n", before + 1).as_bytes())?;
                file.sync_all()
            });
        match written {
            Ok(()) => Entered::Clear(Marker { path }),
            Err(_) => Entered::Unavailable,
        }
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        // collapse-ok: a marker that cannot be removed only keeps this one file untransformed.
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("azlin-marker-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// The analysis that did not come back (the process ended inside it) leaves its marker; the
    /// next one finds it and the file is not analysed again. One that returned leaves none.
    #[test]
    fn a_marker_left_behind_keeps_the_file_out_of_the_analysis_and_a_dropped_one_does_not() {
        let dir = folder("left");
        let file = b"a file that ends the program";
        let Entered::Clear(marker) = Marker::enter(&dir, file) else {
            panic!("the first analysis is let in");
        };
        assert!(
            dir.join(marker_name(file)).exists(),
            "written before the analysis"
        );
        // The same file, while the first is under way (or after it died): left.
        assert!(matches!(Marker::enter(&dir, file), Entered::Left));
        // Another file is let in.
        assert!(matches!(
            Marker::enter(&dir, b"another file"),
            Entered::Clear(_)
        ));
        // An analysis that returned removes its marker: the next one is let in.
        drop(marker);
        assert!(!dir.join(marker_name(file)).exists());
        assert!(matches!(Marker::enter(&dir, file), Entered::Clear(_)));
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
        // Decodes that do not come back (the marker is forgotten, as an abort does).
        for tries in 1..=DECODE_TRIES {
            let Entered::Clear(marker) = Marker::enter_decoding(&dir, stream) else {
                panic!("try {tries} is let in");
            };
            std::mem::forget(marker);
            assert_eq!(
                fs::read_to_string(dir.join(decoding_marker_name(stream)))
                    .unwrap()
                    .trim(),
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
        std::mem::forget(marker);
        assert_eq!(
            fs::read_to_string(dir.join(decoding_marker_name(stream)))
                .unwrap()
                .trim(),
            "decoding 2"
        );
        let blocker = dir.join("file");
        fs::write(&blocker, b"x").unwrap();
        assert!(matches!(
            Marker::enter_decoding(&blocker.join("below"), b"data"),
            Entered::Unavailable
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}
