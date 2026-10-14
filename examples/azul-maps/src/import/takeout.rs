//! Google Takeout's places: "Saved Places.json" (the starred places, a GeoJSON file with the
//! place under `location` or, in older exports, `Location`) and "Labeled places.json" (Home,
//! Work and the other labels). The CSV of every "Saved" list is read by [`super::table`].
//! A starred place exported without coordinates (`[0, 0]`) is unplaced, looked up by its name.

use serde_json::Value;

use super::{links, Imported, Unplaced};
use crate::saved::{slug, ListKind, PlaceList, SavedPlace};

/// The text at `path` (keys in turn) in `value`.
fn text_at(value: &Value, path: &[&str]) -> String {
    let mut at = value;
    for key in path {
        match at.get(*key) {
            Some(next) => at = next,
            None => return String::new(),
        }
    }
    at.as_str().unwrap_or("").trim().to_string()
}

/// A feature's point as `(lat, lon)`; Takeout's `[0, 0]` (a place it exported without
/// coordinates) is none.
fn point(feature: &Value) -> Option<(f64, f64)> {
    let c = feature.get("geometry")?.get("coordinates")?;
    let lon = c.get(0)?.as_f64()?;
    let lat = c.get(1)?.as_f64()?;
    let real = lat.abs() <= 90.0 && lon.abs() <= 180.0 && (lat != 0.0 || lon != 0.0);
    real.then_some((lat, lon))
}

/// The features of a GeoJSON file.
fn features(text: &str) -> Result<Vec<Value>, String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("not GeoJSON: {e}"))?;
    doc.get("features")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "not a GeoJSON FeatureCollection".to_string())
}

/// The starred places of "Saved Places.json", as the list "Starred places".
///
/// # Errors
///
/// The file is no GeoJSON.
pub fn parse_saved_places(text: &str) -> Result<Imported, String> {
    const NAME: &str = "Saved Places";
    let mut imported = Imported {
        lists: vec![PlaceList::new(&slug(NAME), NAME, ListKind::Named)],
        ..Imported::default()
    };
    for feature in features(text)? {
        let p = feature.get("properties").unwrap_or(&Value::Null);
        // The export of 2023 on keeps the place under `location`; older ones under `Location`
        // with the name as `Title` / `Business Name`.
        let name = [
            text_at(p, &["location", "name"]),
            text_at(p, &["Title"]),
            text_at(p, &["Location", "Business Name"]),
            text_at(p, &["title"]),
        ]
        .into_iter()
        .find(|n| !n.is_empty())
        .unwrap_or_default();
        let address = [text_at(p, &["location", "address"]), text_at(p, &["Location", "Address"])]
            .into_iter()
            .find(|a| !a.is_empty())
            .unwrap_or_default();
        let url = [text_at(p, &["google_maps_url"]), text_at(p, &["Google Maps URL"])]
            .into_iter()
            .find(|u| !u.is_empty())
            .unwrap_or_default();
        let date = [text_at(p, &["date"]), text_at(p, &["Published"]), text_at(p, &["Updated"])]
            .into_iter()
            .find(|d| !d.is_empty())
            .unwrap_or_default();
        let saved_s = azul_routing::transit::parse_time(&date).unwrap_or(0);
        let at = point(&feature).or_else(|| links::google_coordinates(&url));
        match at {
            Some(at) => imported.lists[0].places.push(SavedPlace {
                name,
                at,
                note: String::new(),
                address,
                url,
                saved_s,
            }),
            None if !name.is_empty() => imported.unplaced.push(Unplaced {
                list: 0,
                name,
                note: String::new(),
                address,
                url,
            }),
            None => imported.skipped += 1,
        }
    }
    Ok(imported)
}

/// The labelled places of "Labeled places.json", as the list "Labeled places".
///
/// # Errors
///
/// The file is no GeoJSON.
pub fn parse_labeled_places(text: &str) -> Result<Imported, String> {
    const NAME: &str = "Labeled places";
    let mut imported = Imported {
        lists: vec![PlaceList::new(&slug(NAME), NAME, ListKind::Named)],
        ..Imported::default()
    };
    for feature in features(text)? {
        let p = feature.get("properties").unwrap_or(&Value::Null);
        let name = text_at(p, &["name"]);
        let address = text_at(p, &["address"]);
        match point(&feature) {
            Some(at) => imported.lists[0].places.push(SavedPlace {
                name,
                at,
                address,
                ..SavedPlace::default()
            }),
            None if !name.is_empty() => imported.unplaced.push(Unplaced {
                list: 0,
                name,
                address,
                ..Unplaced::default()
            }),
            None => imported.skipped += 1,
        }
    }
    Ok(imported)
}

/// Whether `file_name` is one of Takeout's place files, and which.
#[must_use]
pub fn takeout_file(file_name: &str) -> Option<TakeoutFile> {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name).trim();
    if base.eq_ignore_ascii_case("Saved Places.json") {
        Some(TakeoutFile::SavedPlaces)
    } else if base.eq_ignore_ascii_case("Labeled places.json") {
        Some(TakeoutFile::LabeledPlaces)
    } else {
        None
    }
}

/// Takeout's place files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeoutFile {
    SavedPlaces,
    LabeledPlaces,
}
