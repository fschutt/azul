//! KML and KMZ: Google My Maps, Organic Maps and MAPS.ME bookmarks. A placemark with a `Point`
//! is a place, one with a `LineString` a line; the placemarks of a `Folder` are a list named by
//! the folder, the others a list named by the document (else by the file). A KMZ is a zip with
//! the KML inside (`doc.kml`, else the first `.kml`).

use std::io::Read;

use roxmltree::{Document, Node};

use super::{gpx::child_text, Imported};
use crate::saved::{slug, ListKind, PlaceList, SavedPlace, SavedRoute};

/// KML's coordinates (`lon,lat[,alt]` tuples split by white space) as `(lat, lon)`.
fn coordinates(text: &str) -> Vec<(f64, f64)> {
    text.split_whitespace()
        .filter_map(|tuple| {
            let mut parts = tuple.split(',');
            let lon: f64 = parts.next()?.trim().parse().ok()?;
            let lat: f64 = parts.next()?.trim().parse().ok()?;
            (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
        })
        .collect()
}

/// The placemarks directly in `container` (a Document or a Folder) into `list`; its folders
/// into lists of their own.
fn read_container(container: Node<'_, '_>, list: usize, imported: &mut Imported) {
    for node in container.children().filter(Node::is_element) {
        if node.has_tag_name("Folder") {
            let name = child_text(node, "name");
            let name = if name.is_empty() { imported.lists[list].name.clone() } else { name };
            imported
                .lists
                .push(PlaceList::new(&slug(&name), &name, ListKind::Named));
            let folder = imported.lists.len() - 1;
            read_container(node, folder, imported);
        } else if node.has_tag_name("Document") {
            read_container(node, list, imported);
        } else if node.has_tag_name("Placemark") {
            read_placemark(node, list, imported);
        }
    }
}

/// A placemark: its point is a place, its line a route; anything else is skipped.
fn read_placemark(node: Node<'_, '_>, list: usize, imported: &mut Imported) {
    let name = child_text(node, "name");
    let geometry_text = |tag: &str| {
        node.descendants()
            .find(|d| d.has_tag_name(tag))
            .and_then(|g| g.children().find(|c| c.has_tag_name("coordinates")))
            .and_then(|c| c.text())
            .map(coordinates)
    };
    if let Some(points) = geometry_text("Point") {
        let Some(&at) = points.first() else {
            imported.skipped += 1;
            return;
        };
        imported.lists[list].places.push(SavedPlace {
            name,
            at,
            note: child_text(node, "description"),
            address: child_text(node, "address"),
            ..SavedPlace::default()
        });
    } else if let Some(points) = geometry_text("LineString").filter(|p| p.len() >= 2) {
        imported.lists[list].routes.push(SavedRoute { name, points });
    } else {
        imported.skipped += 1;
    }
}

/// The lists of a KML file named `name`.
///
/// # Errors
///
/// The file is no XML, or no KML.
pub fn parse_kml(text: &str, name: &str) -> Result<Imported, String> {
    let doc = Document::parse(text).map_err(|e| format!("not XML: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("kml") {
        return Err("not a KML file".to_string());
    }
    let document = root.children().find(|c| c.has_tag_name("Document"));
    let title = document
        .map(|d| child_text(d, "name"))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| name.to_string());
    let mut imported = Imported {
        lists: vec![PlaceList::new(&slug(&title), &title, ListKind::Named)],
        ..Imported::default()
    };
    read_container(document.unwrap_or(root), 0, &mut imported);
    // A document whose placemarks are all in folders leaves its own list empty.
    if imported.lists.len() > 1 && imported.lists[0].places.is_empty() && imported.lists[0].routes.is_empty() {
        imported.lists.remove(0);
    }
    Ok(imported)
}

/// The lists of a KMZ file named `name`.
///
/// # Errors
///
/// The file is no zip, or has no KML in it.
pub fn parse_kmz(bytes: &[u8], name: &str) -> Result<Imported, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("not a KMZ: {e}"))?;
    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    let kml = names
        .iter()
        .find(|n| n.eq_ignore_ascii_case("doc.kml"))
        .or_else(|| names.iter().find(|n| n.to_ascii_lowercase().ends_with(".kml")))
        .ok_or_else(|| "the KMZ has no KML in it".to_string())?
        .clone();
    let mut text = String::new();
    archive
        .by_name(&kml)
        .map_err(|e| format!("{kml}: {e}"))?
        .read_to_string(&mut text)
        .map_err(|e| format!("{kml}: {e}"))?;
    parse_kml(&text, name)
}
