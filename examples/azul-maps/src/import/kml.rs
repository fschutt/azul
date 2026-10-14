//! KML and KMZ: Google My Maps, Organic Maps and MAPS.ME bookmarks. A placemark with a `Point`
//! is a place, one with a `LineString` a line; the placemarks of a `Folder` are a list named by
//! the folder, the others a list named by the document (else by the file). A KMZ is a zip with
//! the KML inside (`doc.kml`, else the first `.kml`).

use super::Imported;

/// The lists of a KML file named `name`.
///
/// # Errors
///
/// The file is no XML, or no KML.
pub fn parse_kml(text: &str, name: &str) -> Result<Imported, String> {
    let _ = (text, name);
    Err("not built".to_string())
}

/// The lists of a KMZ file named `name`.
///
/// # Errors
///
/// The file is no zip, or has no KML in it.
pub fn parse_kmz(bytes: &[u8], name: &str) -> Result<Imported, String> {
    let _ = (bytes, name);
    Err("not built".to_string())
}
