//! ZIP archives over the C ABI.
//!
//! `azul_layout::zip` already implements the format; nothing here re-does it.
//! What was missing is a handle every binding can reach - the Rust functions
//! take `Vec<ZipFileEntry>`, which is not something C, Python or Lua can
//! build. So this is a stateful handle (`Db`'s shape: opaque pointer +
//! `run_destructor`) whose whole surface is primitives: a string and a byte
//! vector at a time.
//!
//! The API is always present; the engine sits behind the `zip` feature, so it
//! flows through normal api.json codegen with no feature-gating at the ABI
//! boundary. Without the feature, entries still accumulate and can be read
//! back - only `to_bytes` / `to_file` / parsing degrade to empty.

use core::ffi::c_void;

use azul_css::{AzString, U8Vec};
use azul_layout::zip::ZipFile;

/// Say once per process that the `zip` feature is compiled out.
///
/// Without this the degradation is silent: `to_bytes` hands back an empty
/// buffer, which is byte-for-byte what "the archive had nothing in it" looks
/// like - the same trap the PDF and video stubs warn about.
#[cfg(not(feature = "zip"))]
fn announce_zip_stub(what: &str) {
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][zip] {what} called, but this build has no `zip` feature - the \
             archive cannot be compressed or parsed, so every result is EMPTY. \
             Rebuild with: cargo build -p azul-dll --features build-dll,zip"
        );
    });
}

/// A ZIP archive held in memory: build one entry at a time, then serialize.
///
/// Owns a boxed [`ZipFile`]. `run_destructor` follows the `App` / `Db` handle
/// convention so codegen's `_delete` frees it exactly once.
#[repr(C)]
#[derive(Debug)]
pub struct Zip {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for Zip {
    /// Deep copy - an archive is data, not a connection. A shallow copy would
    /// leave one of the two handles dangling the moment the other is deleted.
    fn clone(&self) -> Self {
        Self::from_archive(self.archive().cloned().unwrap_or_default())
    }
}

impl Default for Zip {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Zip {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            drop(unsafe { Box::from_raw(self.ptr.cast::<ZipFile>()) });
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Zip {
    fn from_archive(archive: ZipFile) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(archive)).cast::<c_void>(),
            run_destructor: true,
        }
    }

    fn archive(&self) -> Option<&ZipFile> {
        unsafe { self.ptr.cast::<ZipFile>().as_ref() }
    }

    fn archive_mut(&mut self) -> Option<&mut ZipFile> {
        unsafe { self.ptr.cast::<ZipFile>().as_mut() }
    }

    /// An empty archive.
    #[must_use]
    pub fn new() -> Self {
        Self::from_archive(ZipFile::new())
    }

    /// Parse ZIP `bytes`.
    ///
    /// A malformed archive (or a build without the `zip` feature) yields a
    /// valid but EMPTY handle rather than a null one, so callers that forget
    /// to check get nothing instead of a crash. Path traversal (`..`) is
    /// rejected by the underlying reader.
    #[must_use]
    pub fn from_bytes(bytes: U8Vec) -> Self {
        #[cfg(feature = "zip")]
        {
            use azul_layout::zip::ZipReadConfig;
            match ZipFile::from_bytes(bytes.as_ref(), &ZipReadConfig::default()) {
                Ok(z) => Self::from_archive(z),
                Err(_) => Self::new(),
            }
        }
        #[cfg(not(feature = "zip"))]
        {
            let _ = bytes;
            announce_zip_stub("Zip::from_bytes");
            Self::new()
        }
    }

    /// Read and parse a `.zip` off disk. Empty handle if it cannot be read.
    #[must_use]
    pub fn from_file(path: AzString) -> Self {
        #[cfg(feature = "zip")]
        {
            use azul_layout::zip::ZipReadConfig;
            match ZipFile::from_file(
                std::path::Path::new(path.as_str()),
                &ZipReadConfig::default(),
            ) {
                Ok(z) => Self::from_archive(z),
                Err(_) => Self::new(),
            }
        }
        #[cfg(not(feature = "zip"))]
        {
            let _ = path;
            announce_zip_stub("Zip::from_file");
            Self::new()
        }
    }

    /// `true` if this handle owns an archive. False only for a handle whose
    /// memory could not be allocated, or one already deleted.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Append a file entry. A repeated path is kept as-is: ZIP permits
    /// duplicates, and silently dropping one would lose data the caller
    /// believes it stored.
    pub fn add_file(&mut self, path: AzString, data: U8Vec) {
        if let Some(z) = self.archive_mut() {
            z.add_file(path.as_str().to_string(), data.as_ref().to_vec());
        }
    }

    /// Append a directory entry (no data). Only needed for readers that
    /// expect explicit directory records - file paths may contain `/` freely.
    pub fn add_directory(&mut self, path: AzString) {
        if let Some(z) = self.archive_mut() {
            z.add_directory(path.as_str().to_string());
        }
    }

    /// Drop every entry with this exact path.
    pub fn remove(&mut self, path: AzString) {
        if let Some(z) = self.archive_mut() {
            z.remove(path.as_str());
        }
    }

    /// `true` if an entry with this exact path exists.
    #[must_use]
    pub fn contains(&self, path: AzString) -> bool {
        self.archive().is_some_and(|z| z.contains(path.as_str()))
    }

    /// Number of entries, files and directories alike.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.archive().map_or(0, |z| z.entries.len())
    }

    /// Path of entry `index`; empty string when out of range.
    #[must_use]
    pub fn file_path(&self, index: usize) -> AzString {
        self.archive()
            .and_then(|z| z.entries.get(index))
            .map_or_else(
                || AzString::from_const_str(""),
                |e| AzString::from(e.path.clone()),
            )
    }

    /// Contents of entry `index`; empty when out of range or a directory.
    #[must_use]
    pub fn file_data(&self, index: usize) -> U8Vec {
        self.archive()
            .and_then(|z| z.entries.get(index))
            .map_or_else(
                || U8Vec::from_vec(Vec::new()),
                |e| U8Vec::from_vec(e.data.clone()),
            )
    }

    /// `true` if entry `index` is a directory record.
    #[must_use]
    pub fn file_is_directory(&self, index: usize) -> bool {
        self.archive()
            .and_then(|z| z.entries.get(index))
            .is_some_and(|e| e.is_directory)
    }

    /// Contents of the entry at `path`; empty if absent.
    ///
    /// Ambiguous with a real zero-byte entry - use `contains` when the
    /// difference matters.
    #[must_use]
    pub fn get_file(&self, path: AzString) -> U8Vec {
        self.archive()
            .and_then(|z| z.get(path.as_str()))
            .map_or_else(
                || U8Vec::from_vec(Vec::new()),
                |e| U8Vec::from_vec(e.data.clone()),
            )
    }

    /// Compress to ZIP bytes with deflate level 6.
    ///
    /// Empty on failure or without the `zip` feature - test the result rather
    /// than assuming it worked.
    #[must_use]
    pub fn to_bytes(&self) -> U8Vec {
        self.to_bytes_with_level(6)
    }

    /// Compress to ZIP bytes. `level` 0 stores uncompressed (fastest, right
    /// for already-compressed payloads); 1-9 deflate at that level.
    #[must_use]
    pub fn to_bytes_with_level(&self, level: u8) -> U8Vec {
        #[cfg(feature = "zip")]
        {
            use azul_layout::zip::ZipWriteConfig;
            let cfg = if level == 0 {
                ZipWriteConfig::store()
            } else {
                ZipWriteConfig::deflate(level)
            };
            self.archive()
                .and_then(|z| z.to_bytes(&cfg).ok())
                .map_or_else(|| U8Vec::from_vec(Vec::new()), U8Vec::from_vec)
        }
        #[cfg(not(feature = "zip"))]
        {
            let _ = level;
            announce_zip_stub("Zip::to_bytes");
            U8Vec::from_vec(Vec::new())
        }
    }

    /// Write the archive to `path`. `true` on success.
    pub fn to_file(&self, path: AzString) -> bool {
        #[cfg(feature = "zip")]
        {
            use azul_layout::zip::ZipWriteConfig;
            self.archive().is_some_and(|z| {
                z.to_file(
                    std::path::Path::new(path.as_str()),
                    &ZipWriteConfig::default(),
                )
                .is_ok()
            })
        }
        #[cfg(not(feature = "zip"))]
        {
            let _ = path;
            announce_zip_stub("Zip::to_file");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_survive_a_round_trip_through_the_handle() {
        let mut z = Zip::new();
        z.add_file("a.txt".into(), U8Vec::from_vec(b"hello".to_vec()));
        z.add_file("dir/b.bin".into(), U8Vec::from_vec(vec![0u8, 1, 2, 3]));

        assert_eq!(z.file_count(), 2);
        assert_eq!(z.file_path(0).as_str(), "a.txt");
        assert_eq!(z.get_file("dir/b.bin".into()).as_ref(), &[0u8, 1, 2, 3]);
        assert!(z.contains("a.txt".into()));
        assert!(!z.contains("missing".into()));
    }

    #[cfg(feature = "zip")]
    #[test]
    fn bytes_written_by_the_handle_parse_back_to_the_same_entries() {
        let mut z = Zip::new();
        z.add_file("notes.json".into(), U8Vec::from_vec(b"{\"n\":1}".to_vec()));
        z.add_file("audio/clip0.wav".into(), U8Vec::from_vec(vec![7u8; 512]));

        let bytes = z.to_bytes();
        assert!(!bytes.as_ref().is_empty(), "compression produced nothing");

        let back = Zip::from_bytes(bytes);
        assert_eq!(back.file_count(), 2);
        assert_eq!(back.get_file("notes.json".into()).as_ref(), b"{\"n\":1}");
        assert_eq!(back.get_file("audio/clip0.wav".into()).as_ref().len(), 512);
    }

    #[test]
    fn a_cloned_handle_owns_its_own_copy() {
        // A shallow clone would leave `copy` pointing at freed memory here.
        let copy = {
            let mut z = Zip::new();
            z.add_file("x".into(), U8Vec::from_vec(b"y".to_vec()));
            z.clone()
        };
        assert_eq!(copy.file_count(), 1);
        assert_eq!(copy.get_file("x".into()).as_ref(), b"y");
    }
}
