//! Google Takeout's places: "Saved Places.json" (the starred places, a GeoJSON file with the
//! place under `location` or, in older exports, `Location`) and "Labeled places.json" (Home,
//! Work and the other labels). The CSV of every "Saved" list is read by [`super::table`].
//! A starred place exported without coordinates (`[0, 0]`) is unplaced, looked up by its name.

use super::Imported;

/// The starred places of "Saved Places.json", as the list "Starred places".
///
/// # Errors
///
/// The file is no GeoJSON.
pub fn parse_saved_places(text: &str) -> Result<Imported, String> {
    let _ = text;
    Err("not built".to_string())
}

/// The labelled places of "Labeled places.json", as the list "Labeled places".
///
/// # Errors
///
/// The file is no GeoJSON.
pub fn parse_labeled_places(text: &str) -> Result<Imported, String> {
    let _ = text;
    Err("not built".to_string())
}

/// Whether `file_name` is one of Takeout's place files, and which.
#[must_use]
pub fn takeout_file(file_name: &str) -> Option<TakeoutFile> {
    let _ = file_name;
    None
}

/// Takeout's place files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeoutFile {
    SavedPlaces,
    LabeledPlaces,
}
