//! CSV: a table with latitude and longitude columns (`lat` / `latitude` / `y`, `lon` / `lng` /
//! `long` / `longitude` / `x`), named by a `name` / `title` column, noted by a `note` /
//! `description` / `comment` one. A table without coordinate columns but with a URL column - a
//! list of Google's "Saved" export (`Title,Note,URL,...`) - has its places' coordinates taken
//! from their links where the links have them; the others are unplaced, to be looked up.

use super::{links, Imported, Unplaced};
use crate::saved::{slug, ListKind, PlaceList, SavedPlace};

/// The column of a header whose name is one of `names` (without case or surrounding space).
fn column(headers: &[String], names: &[&str]) -> Option<usize> {
    headers
        .iter()
        .position(|h| names.iter().any(|n| h.trim().eq_ignore_ascii_case(n)))
}

/// A coordinate as a table writes it, with a decimal point or a decimal comma.
fn number(text: &str) -> Option<f64> {
    let text = text.trim();
    text.parse()
        .ok()
        .or_else(|| text.replace(',', ".").parse().ok())
        .filter(|v: &f64| v.is_finite())
}

/// The list of a CSV file named `name`.
///
/// # Errors
///
/// The file has neither coordinate columns nor a URL column.
pub fn parse_csv(text: &str, name: &str) -> Result<Imported, String> {
    let text = text.trim_start_matches('\u{feff}');
    // European tables separate with semicolons (the comma is their decimal point).
    let first = text.lines().next().unwrap_or("");
    let delimiter = if first.contains(';') && !first.contains(',') { b';' } else { b',' };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| format!("not a table: {e}"))?
        .iter()
        .map(str::to_string)
        .collect();
    let lat = column(&headers, &["lat", "latitude", "y"]);
    let lon = column(&headers, &["lon", "lng", "long", "longitude", "x"]);
    let url = column(&headers, &["url", "link", "google maps url"]);
    let title = column(&headers, &["name", "title"]);
    let note = column(&headers, &["note", "description", "desc", "comment"]);
    let address = column(&headers, &["address"]);
    if (lat.is_none() || lon.is_none()) && url.is_none() {
        return Err("the table has no latitude / longitude columns and no links".to_string());
    }
    let mut imported = Imported {
        lists: vec![PlaceList::new(&slug(name), name, ListKind::Named)],
        ..Imported::default()
    };
    for record in reader.records() {
        let Ok(record) = record else {
            imported.skipped += 1;
            continue;
        };
        let field = |at: Option<usize>| {
            at.and_then(|i| record.get(i))
                .map(str::trim)
                .unwrap_or("")
                .to_string()
        };
        let link = field(url);
        let mut place_name = field(title);
        if place_name.is_empty() {
            place_name = links::google_place_name(&link).unwrap_or_default();
        }
        let at = match (lat, lon) {
            (Some(lat), Some(lon)) => match (number(&field(Some(lat))), number(&field(Some(lon)))) {
                (Some(a), Some(b)) if a.abs() <= 90.0 && b.abs() <= 180.0 => Some((a, b)),
                _ => None,
            },
            _ => links::google_coordinates(&link),
        };
        match at {
            Some(at) => imported.lists[0].places.push(SavedPlace {
                name: place_name,
                at,
                note: field(note),
                address: field(address),
                url: link,
                saved_s: 0,
            }),
            // A row of a list of links, without coordinates in its link: looked up by name.
            None if lat.is_none() && !place_name.is_empty() => imported.unplaced.push(Unplaced {
                list: 0,
                name: place_name,
                note: field(note),
                address: field(address),
                url: link,
            }),
            None => imported.skipped += 1,
        }
    }
    Ok(imported)
}
