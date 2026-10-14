//! Places from elsewhere, into AzMaps' lists (Settings > Import), and AzMaps' lists out again.
//!
//! | From | What | Read by |
//! |------|------|---------|
//! | GPX (any app; OsmAnd's favourites) | waypoints, routes, tracks; OsmAnd's categories as lists | [`gpx`] |
//! | KML / KMZ (Google My Maps, Organic Maps, MAPS.ME bookmarks) | placemarks by folder, lines | [`kml`] |
//! | GeoJSON | points, lines | [`crate::saved::PlaceList::from_geojson`] |
//! | CSV with latitude / longitude columns | rows | [`table`] |
//! | Google Takeout | "Saved Places.json", "Labeled places.json", the CSV of every "Saved" list | [`takeout`] |
//! | Apple Maps | no bulk export exists; a shared link, pasted one at a time | [`links`] |
//!
//! Google's saved-list CSV files hold a title, a note and a Google Maps URL, and the URL often
//! has no coordinates. Those places are [`Unplaced`]: their title is looked up with the search
//! (cached), a single clear match is taken, and the rest wait in the review for the user to
//! confirm or fix ([`review`]).
//!
//! Out: a list's GeoJSON file as it is, or GPX ([`gpx::to_gpx`]).

pub mod gpx;
pub mod kml;
pub mod links;
pub mod review;
pub mod table;
pub mod takeout;

use crate::saved::PlaceList;

/// A place an import found without coordinates: its title is looked up.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Unplaced {
    /// The list it belongs to: an index into [`Imported::lists`].
    pub list: usize,
    pub name: String,
    pub note: String,
    pub address: String,
    pub url: String,
}

/// What an import read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Imported {
    pub lists: Vec<PlaceList>,
    /// The places still to be placed.
    pub unplaced: Vec<Unplaced>,
    /// How many entries were skipped (no coordinates and no title, a broken line).
    pub skipped: usize,
}

impl Imported {
    /// How many places it has, placed or not.
    #[must_use]
    pub fn places(&self) -> usize {
        self.lists.iter().map(|l| l.places.len()).sum::<usize>() + self.unplaced.len()
    }

    /// Adds `other`'s lists and unplaced places (their list indexes moved along).
    pub fn extend(&mut self, other: Imported) {
        let _ = other;
    }
}

/// The kind of file an import got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Gpx,
    Kml,
    Kmz,
    GeoJson,
    Csv,
}

/// The format of a file by its name, else by its first bytes.
#[must_use]
pub fn detect(file_name: &str, bytes: &[u8]) -> Option<Format> {
    let _ = (file_name, bytes);
    None
}

/// Reads one file: its lists are named after the file where it names none.
///
/// # Errors
///
/// The file is none of the formats, or broken.
pub fn import_bytes(file_name: &str, bytes: &[u8]) -> Result<Imported, String> {
    let _ = (file_name, bytes);
    Err("not built".to_string())
}

/// Reads a file or, for a folder (an unpacked Google Takeout), every file AzMaps can read in it.
///
/// # Errors
///
/// Nothing in it could be read.
pub fn import_path(path: &std::path::Path) -> Result<Imported, String> {
    let _ = path;
    Err("not built".to_string())
}

/// A file's name without its folder and extension, as a list's name.
#[must_use]
pub fn stem(file_name: &str) -> String {
    let _ = file_name;
    String::new()
}

#[cfg(test)]
mod tests;
