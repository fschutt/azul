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
}
