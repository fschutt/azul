//! CSV: a table with latitude and longitude columns (`lat` / `latitude` / `y`, `lon` / `lng` /
//! `long` / `longitude` / `x`), named by a `name` / `title` column, noted by a `note` /
//! `description` / `comment` one. A table without coordinate columns but with a URL column - a
//! list of Google's "Saved" export (`Title,Note,URL,...`) - has its places' coordinates taken
//! from their links where the links have them; the others are unplaced, to be looked up.

use super::Imported;

/// The list of a CSV file named `name`.
///
/// # Errors
///
/// The file has neither coordinate columns nor a URL column.
pub fn parse_csv(text: &str, name: &str) -> Result<Imported, String> {
    let _ = (text, name);
    Err("not built".to_string())
}
