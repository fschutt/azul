//! GPX 1.0 / 1.1: waypoints (`wpt`) are places, routes (`rte`) and tracks (`trk`) lines.
//! OsmAnd keeps its favourites as GPX with a `type` per waypoint, the favourite's category:
//! each category becomes a list of its own.

use roxmltree::{Document, Node};

use super::Imported;
use crate::saved::{slug, ListKind, PlaceList, SavedPlace, SavedRoute};

/// The text of `node`'s child element `name`, trimmed.
pub(super) fn child_text(node: Node<'_, '_>, name: &str) -> String {
    node.children()
        .find(|c| c.has_tag_name(name))
        .and_then(|c| c.text())
        .map_or_else(String::new, |t| t.trim().to_string())
}

/// A point's `lat` / `lon` attributes, when they are a place on Earth.
fn point_of(node: Node<'_, '_>) -> Option<(f64, f64)> {
    let lat: f64 = node.attribute("lat")?.trim().parse().ok()?;
    let lon: f64 = node.attribute("lon")?.trim().parse().ok()?;
    (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

/// `text` with XML's special characters escaped.
pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The lists of a GPX file named `name`.
///
/// # Errors
///
/// The file is no XML, or no GPX.
pub fn parse_gpx(text: &str, name: &str) -> Result<Imported, String> {
    let doc = Document::parse(text).map_err(|e| format!("not XML: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("gpx") {
        return Err("not a GPX file".to_string());
    }
    let title = root
        .children()
        .find(|c| c.has_tag_name("metadata"))
        .map(|m| child_text(m, "name"))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| name.to_string());
    let mut imported = Imported::default();
    // The list of a waypoint: its `type` (OsmAnd's category, AzMaps' own list on export), else
    // the file's.
    let list_of = |imported: &mut Imported, list_name: &str| -> usize {
        if let Some(at) = imported.lists.iter().position(|l| l.name == list_name) {
            return at;
        }
        imported
            .lists
            .push(PlaceList::new(&slug(list_name), list_name, ListKind::Named));
        imported.lists.len() - 1
    };
    for node in root.children().filter(Node::is_element) {
        if node.has_tag_name("wpt") {
            let Some(at) = point_of(node) else {
                imported.skipped += 1;
                continue;
            };
            let category = child_text(node, "type");
            let list = list_of(&mut imported, if category.is_empty() { &title } else { &category });
            let mut note = child_text(node, "desc");
            if note.is_empty() {
                note = child_text(node, "cmt");
            }
            imported.lists[list].places.push(SavedPlace {
                name: child_text(node, "name"),
                at,
                note,
                url: node
                    .children()
                    .find(|c| c.has_tag_name("link"))
                    .and_then(|l| l.attribute("href"))
                    .unwrap_or("")
                    .to_string(),
                ..SavedPlace::default()
            });
        } else if node.has_tag_name("rte") || node.has_tag_name("trk") {
            // A route's points are `rtept`s; a track's `trkpt`s in its segments, joined.
            let points: Vec<(f64, f64)> = node
                .descendants()
                .filter(|p| p.has_tag_name("rtept") || p.has_tag_name("trkpt"))
                .filter_map(point_of)
                .collect();
            if points.len() < 2 {
                imported.skipped += 1;
                continue;
            }
            let list = list_of(&mut imported, &title);
            imported.lists[list].routes.push(SavedRoute {
                name: child_text(node, "name"),
                points,
            });
        }
    }
    if imported.lists.is_empty() {
        imported.lists.push(PlaceList::new(&slug(&title), &title, ListKind::Named));
    }
    Ok(imported)
}

/// `lists` as one GPX file: every place a waypoint (its list as its `type`), every route a
/// route.
#[must_use]
pub fn to_gpx(lists: &[PlaceList]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"AzMaps\" \
         xmlns=\"http://www.topografix.com/GPX/1/1\">\n",
    );
    for list in lists {
        for place in &list.places {
            out.push_str(&format!(
                "  <wpt lat=\"{:.7}\" lon=\"{:.7}\"><name>{}</name>",
                place.at.0,
                place.at.1,
                escape(&place.name)
            ));
            if !place.note.is_empty() {
                out.push_str(&format!("<desc>{}</desc>", escape(&place.note)));
            }
            if !place.url.is_empty() {
                out.push_str(&format!("<link href=\"{}\"/>", escape(&place.url)));
            }
            out.push_str(&format!("<type>{}</type></wpt>\n", escape(&list.name)));
        }
    }
    for list in lists {
        for route in &list.routes {
            out.push_str(&format!("  <rte><name>{}</name>", escape(&route.name)));
            for (lat, lon) in &route.points {
                out.push_str(&format!("<rtept lat=\"{lat:.7}\" lon=\"{lon:.7}\"/>"));
            }
            out.push_str("</rte>\n");
        }
    }
    out.push_str("</gpx>\n");
    out
}
