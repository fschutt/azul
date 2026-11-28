//! The folders a person granted that the APP keeps a list of: bookmarks, for a platform whose system keeps no list of grants for it.
//!
//! Android lists the grants itself (`ContentResolver.getPersistedUriPermissions`); iOS gives a picked folder as a security-scoped URL that
//! is good until the process ends, and the way to use it again after a restart is a BOOKMARK (opaque bytes the system made from the URL).
//! The app keeps them in a small file of its own, in the app's storage: this module is that file. It knows nothing of bookmarks beyond
//! "bytes": making them and resolving them is the platform's (`azul-dll`, `desktop/extra/documents/apple.rs`).
//!
//! The file is rewritten whole next to itself and renamed over the old one, so a power cut leaves the old list or the new one; a line
//! that does not parse is skipped (and the rest is kept), never a reason to lose the others; the list is capped ([`MAX_PLACES`]).

use alloc::{string::String, vec::Vec};
use core::fmt::Write as _;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// How many folders the app keeps: a person has a handful. Past this the OLDEST goes (and is returned, so that the platform releases it).
pub const MAX_PLACES: usize = 64;

/// One kept folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPlace {
    /// What the person sees ("Downloads").
    pub name: String,
    /// The platform's bookmark of the folder.
    pub bookmark: Vec<u8>,
    /// When the folder was granted, milliseconds since the epoch (0 when unknown).
    pub granted_ms: u64,
}

/// The file of the kept folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacesStore {
    path: PathBuf,
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok())
        .collect()
}

impl PlacesStore {
    /// The store in `path` (the file is made by the first [`PlacesStore::add`]).
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The kept folders, oldest first; a missing file is an empty list and a line that does not parse is left out.
    #[must_use]
    pub fn load(&self) -> Vec<StoredPlace> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        text.lines().filter_map(parse_line).collect()
    }

    fn save(&self, places: &[StoredPlace]) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = String::new();
        for place in places {
            let _ = writeln!(
                text,
                "v1\t{}\t{}\t{}",
                hex(place.name.as_bytes()),
                place.granted_ms,
                hex(&place.bookmark)
            );
        }
        let temp = self.path.with_extension("tmp");
        let mut file = fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, &self.path)
    }

    /// Keep `place`: a folder with the same bookmark is replaced (granted again), else it is added; past [`MAX_PLACES`] the oldest are
    /// dropped and returned.
    ///
    /// # Errors
    ///
    /// What writing the file said; the old list is intact then.
    pub fn add(&self, place: StoredPlace) -> std::io::Result<Vec<StoredPlace>> {
        let mut places = self.load();
        places.retain(|kept| kept.bookmark != place.bookmark);
        places.push(place);
        let dropped = if places.len() > MAX_PLACES {
            places.drain(..places.len() - MAX_PLACES).collect()
        } else {
            Vec::new()
        };
        self.save(&places)?;
        Ok(dropped)
    }

    /// Forget the folder whose bookmark is `bookmark`; whether there was one.
    ///
    /// # Errors
    ///
    /// What writing the file said; the old list is intact then.
    pub fn remove(&self, bookmark: &[u8]) -> std::io::Result<bool> {
        let mut places = self.load();
        let before = places.len();
        places.retain(|kept| kept.bookmark != bookmark);
        if places.len() == before {
            return Ok(false);
        }
        self.save(&places)?;
        Ok(true)
    }
}

fn parse_line(line: &str) -> Option<StoredPlace> {
    let mut fields = line.split('\t');
    if fields.next()? != "v1" {
        return None;
    }
    let name = String::from_utf8(unhex(fields.next()?)?).ok()?;
    let granted_ms = fields.next()?.parse().ok()?;
    let bookmark = unhex(fields.next()?).filter(|b| !b.is_empty())?;
    fields.next().is_none().then_some(StoredPlace { name, bookmark, granted_ms })
}

#[cfg(test)]
#[path = "places_store_test.rs"]
mod places_store_test;
