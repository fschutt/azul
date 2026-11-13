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
        let shift = self.lists.len();
        self.lists.extend(other.lists);
        self.unplaced.extend(other.unplaced.into_iter().map(|mut u| {
            u.list += shift;
            u
        }));
        self.skipped += other.skipped;
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
    let lower = file_name.to_ascii_lowercase();
    let by_name = [
        (".gpx", Format::Gpx),
        (".kml", Format::Kml),
        (".kmz", Format::Kmz),
        (".geojson", Format::GeoJson),
        (".json", Format::GeoJson),
        (".csv", Format::Csv),
    ];
    if let Some((_, format)) = by_name.iter().find(|(ext, _)| lower.ends_with(ext)) {
        return Some(*format);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some(Format::Kmz);
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
    if head.contains("<gpx") {
        Some(Format::Gpx)
    } else if head.contains("<kml") {
        Some(Format::Kml)
    } else if head.trim_start().starts_with('{') && head.contains("featurecollection") {
        Some(Format::GeoJson)
    } else {
        None
    }
}

/// Reads one file: its lists are named after the file where it names none.
///
/// # Errors
///
/// The file is none of the formats, or broken.
pub fn import_bytes(file_name: &str, bytes: &[u8]) -> Result<Imported, String> {
    let name = stem(file_name);
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let text = || String::from_utf8_lossy(bytes).trim_start_matches('\u{feff}').to_string();
    match detect(file_name, bytes) {
        Some(Format::Gpx) => gpx::parse_gpx(&text(), &name),
        Some(Format::Kml) => kml::parse_kml(&text(), &name),
        Some(Format::Kmz) => kml::parse_kmz(bytes, &name),
        Some(Format::Csv) => table::parse_csv(&text(), &name),
        Some(Format::GeoJson) => match takeout::takeout_file(base) {
            Some(takeout::TakeoutFile::SavedPlaces) => takeout::parse_saved_places(&text()),
            Some(takeout::TakeoutFile::LabeledPlaces) => takeout::parse_labeled_places(&text()),
            None => {
                let mut list = PlaceList::from_geojson(&crate::saved::slug(&name), &text())?;
                if list.name == list.id {
                    list.name = name;
                }
                Ok(Imported {
                    lists: vec![list],
                    ..Imported::default()
                })
            }
        },
        None => Err(format!("{base}: not a file of places AzMaps reads (GPX, KML, KMZ, GeoJSON, CSV)")),
    }
}

/// Reads a file or, for a folder (an unpacked Google Takeout), every file AzMaps can read in it.
///
/// # Errors
///
/// Nothing in it could be read.
pub fn import_path(path: &std::path::Path) -> Result<Imported, String> {
    if !path.is_dir() {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return import_bytes(&path.to_string_lossy(), &bytes);
    }
    let mut files = Vec::new();
    collect(path, &mut files, 6);
    files.sort();
    let mut all = Imported::default();
    let mut problems = Vec::new();
    for file in files {
        let name = file.to_string_lossy().into_owned();
        // Takeout has many JSON files; only its place files are read.
        let base = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
        let lower = base.to_ascii_lowercase();
        if lower.ends_with(".json") && takeout::takeout_file(&base).is_none() {
            continue;
        }
        if detect(&base, &[]).is_none() {
            continue;
        }
        match std::fs::read(&file).map_err(|e| e.to_string()).and_then(|b| import_bytes(&name, &b)) {
            Ok(imported) => all.extend(imported),
            Err(why) => problems.push(why),
        }
    }
    if all.lists.is_empty() && all.unplaced.is_empty() {
        return Err(problems
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("{}: no places in it", path.display())));
    }
    Ok(all)
}

/// Every file under `dir`, `depth` folders deep at most.
fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect(&path, out, depth - 1);
            }
        } else {
            out.push(path);
        }
    }
}

/// A file's name without its folder and extension, as a list's name.
#[must_use]
pub fn stem(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests;
