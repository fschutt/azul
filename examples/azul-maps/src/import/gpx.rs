//! GPX 1.0 / 1.1: waypoints (`wpt`) are places, routes (`rte`) and tracks (`trk`) lines.
//! OsmAnd keeps its favourites as GPX with a `type` per waypoint, the favourite's category:
//! each category becomes a list of its own.

use super::Imported;
use crate::saved::PlaceList;

/// The lists of a GPX file named `name`.
///
/// # Errors
///
/// The file is no XML, or no GPX.
pub fn parse_gpx(text: &str, name: &str) -> Result<Imported, String> {
    let _ = (text, name);
    Err("not built".to_string())
}

/// `lists` as one GPX file: every place a waypoint (its list as its `type`), every route a
/// route.
#[must_use]
pub fn to_gpx(lists: &[PlaceList]) -> String {
    let _ = lists;
    String::new()
}
