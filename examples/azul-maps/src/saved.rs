//! Saved places: the recent destinations (kept on their own, clearable), the favourites, and
//! lists the user names or imports. Each list is one GeoJSON file in the data tree
//! (`maps/lists/<id>.geojson`), a `FeatureCollection` any map tool reads: a place is a `Point`
//! feature with its name, note, address and link; an imported route or track a `LineString`.
//! An export is the same file. Recent searches are kept beside them (`maps/searches.json`).

use serde_json::{json, Value};

use crate::places::fold;

/// The id of the recent destinations' list.
pub const RECENTS: &str = "recents";

/// The id of the favourites' list.
pub const FAVOURITES: &str = "favourites";

/// The folder of the lists in AzMaps' part of the data tree.
pub const LISTS_DIR: &str = "lists";

/// The file of the recent searches in AzMaps' part of the data tree.
pub const SEARCHES_FILE: &str = "searches.json";

/// How many recent destinations are kept.
pub const RECENTS_LIMIT: usize = 50;

/// How many recent searches are kept.
pub const SEARCHES_LIMIT: usize = 20;

/// How close two places are to be the same place, metres.
pub const SAME_PLACE_M: f64 = 30.0;

/// A saved place.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SavedPlace {
    pub name: String,
    pub at: (f64, f64),
    /// What the user wrote about it, or the description an import had.
    pub note: String,
    /// Its address, where known.
    pub address: String,
    /// The link it came from (a Google Maps URL), where it came from one.
    pub url: String,
    /// When it was saved, unix seconds (0: unknown).
    pub saved_s: i64,
}

/// A route or a track saved with a list (an imported GPX route, a KML line).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SavedRoute {
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

/// What a list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// The recent destinations, kept on their own.
    Recents,
    Favourites,
    /// A list the user named or imported.
    Named,
}

impl ListKind {
    /// The word in the file (`azmaps:kind`).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Recents => "recents",
            Self::Favourites => "favourites",
            Self::Named => "list",
        }
    }
}

/// A list of places.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceList {
    /// Its file's name without `.geojson`: `recents`, `favourites`, or a slug of its name.
    pub id: String,
    pub name: String,
    pub kind: ListKind,
    /// Newest first for the recents; in the user's order otherwise.
    pub places: Vec<SavedPlace>,
    pub routes: Vec<SavedRoute>,
}

impl PlaceList {
    /// An empty list.
    #[must_use]
    pub fn new(id: &str, name: &str, kind: ListKind) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            places: Vec::new(),
            routes: Vec::new(),
        }
    }

    /// The list as a GeoJSON `FeatureCollection`.
    #[must_use]
    pub fn to_geojson(&self) -> String {
        let mut features: Vec<Value> = self
            .places
            .iter()
            .map(|p| {
                json!({
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [p.at.1, p.at.0] },
                    "properties": {
                        "name": p.name,
                        "note": p.note,
                        "address": p.address,
                        "url": p.url,
                        "saved": p.saved_s,
                    },
                })
            })
            .collect();
        features.extend(self.routes.iter().map(|r| {
            let line: Vec<Value> = r.points.iter().map(|&(lat, lon)| json!([lon, lat])).collect();
            json!({
                "type": "Feature",
                "geometry": { "type": "LineString", "coordinates": line },
                "properties": { "name": r.name },
            })
        }));
        let doc = json!({
            "type": "FeatureCollection",
            "name": self.name,
            "azmaps:kind": self.kind.key(),
            "features": features,
        });
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    }

    /// The list a GeoJSON file holds (its id is the file's name); points are places, lines
    /// routes, anything else is skipped.
    ///
    /// # Errors
    ///
    /// The file is no GeoJSON `FeatureCollection`.
    pub fn from_geojson(id: &str, text: &str) -> Result<Self, String> {
        let doc: Value = serde_json::from_str(text).map_err(|e| format!("not GeoJSON: {e}"))?;
        if doc.get("type").and_then(Value::as_str) != Some("FeatureCollection") {
            return Err("not a GeoJSON FeatureCollection".to_string());
        }
        let kind = match doc.get("azmaps:kind").and_then(Value::as_str) {
            Some("recents") => ListKind::Recents,
            Some("favourites") => ListKind::Favourites,
            _ => ListKind::Named,
        };
        let name = doc
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(id);
        let mut list = Self::new(id, name, kind);
        for feature in doc.get("features").and_then(Value::as_array).into_iter().flatten() {
            let geometry = feature.get("geometry").unwrap_or(&Value::Null);
            let properties = feature.get("properties").unwrap_or(&Value::Null);
            let coordinates = geometry.get("coordinates");
            match geometry.get("type").and_then(Value::as_str) {
                Some("Point") => {
                    let Some(at) = coordinates.and_then(lat_lon) else {
                        continue;
                    };
                    list.places.push(SavedPlace {
                        name: property(properties, &["name", "title", "Name", "Title"]),
                        at,
                        note: property(properties, &["note", "description", "desc", "Note", "Description"]),
                        address: property(properties, &["address", "Address"]),
                        url: property(properties, &["url", "google_maps_url", "Google Maps URL"]),
                        saved_s: properties.get("saved").and_then(Value::as_i64).unwrap_or(0),
                    });
                }
                Some("LineString") => {
                    let points: Vec<(f64, f64)> = coordinates
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(lat_lon)
                        .collect();
                    if points.len() >= 2 {
                        list.routes.push(SavedRoute {
                            name: property(properties, &["name", "title", "Name", "Title"]),
                            points,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(list)
    }

    /// Puts `place` first, as the most recent; the same place further down goes, and the list
    /// keeps at most `limit`.
    pub fn remember(&mut self, place: SavedPlace, limit: usize) {
        self.places
            .retain(|p| azul_routing::distance_m(p.at, place.at) > SAME_PLACE_M);
        self.places.insert(0, place);
        self.places.truncate(limit);
    }

    /// Whether the list has a place within [`SAME_PLACE_M`] of `at`.
    #[must_use]
    pub fn has_place_at(&self, at: (f64, f64)) -> bool {
        self.places
            .iter()
            .any(|p| azul_routing::distance_m(p.at, at) <= SAME_PLACE_M)
    }
}

/// A GeoJSON position (`[lon, lat]` or `[lon, lat, alt]`) as `(lat, lon)`, when it is a place
/// on Earth.
fn lat_lon(position: &Value) -> Option<(f64, f64)> {
    let lon = position.get(0)?.as_f64()?;
    let lat = position.get(1)?.as_f64()?;
    (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

/// The first of `names` a feature's properties have as text.
fn property(properties: &Value, names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| properties.get(*name).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

/// A slug of a list's name, for its file: `My Trip to Rome` is `my-trip-to-rome`; never empty.
#[must_use]
pub fn slug(name: &str) -> String {
    let words: Vec<String> = fold(name)
        .split(' ')
        .map(|w| w.chars().filter(char::is_ascii_alphanumeric).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        "list".to_string()
    } else {
        words.join("-")
    }
}

/// A place found in the saved lists by a search: the list's index and the place's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedHit {
    pub list: usize,
    pub place: usize,
}

/// The saved places whose name, address or note starts a word with `query` (without case or
/// accents), favourites first, then the named lists, then the recents; at most `limit`.
#[must_use]
pub fn search(lists: &[PlaceList], query: &str, limit: usize) -> Vec<SavedHit> {
    let query = fold(query);
    if query.is_empty() {
        return Vec::new();
    }
    let matches = |text: &str| {
        let text = fold(text);
        text.starts_with(&query) || text.contains(&format!(" {query}"))
    };
    let rank = |kind: ListKind| match kind {
        ListKind::Favourites => 0,
        ListKind::Named => 1,
        ListKind::Recents => 2,
    };
    let mut order: Vec<usize> = (0..lists.len()).collect();
    order.sort_by_key(|&i| rank(lists[i].kind));
    let mut hits = Vec::new();
    for list in order {
        for (place, p) in lists[list].places.iter().enumerate() {
            if hits.len() >= limit {
                return hits;
            }
            if matches(&p.name) || matches(&p.address) || matches(&p.note) {
                hits.push(SavedHit { list, place });
            }
        }
    }
    hits
}

/// Puts `query` first among the recent searches (once), keeping at most [`SEARCHES_LIMIT`].
pub fn remember_search(searches: &mut Vec<String>, query: &str) {
    let query = query.trim();
    if query.is_empty() {
        return;
    }
    let folded = fold(query);
    searches.retain(|s| fold(s) != folded);
    searches.insert(0, query.to_string());
    searches.truncate(SEARCHES_LIMIT);
}

/// The recent searches as their file holds them.
#[must_use]
pub fn searches_to_json(searches: &[String]) -> String {
    serde_json::to_string_pretty(searches).unwrap_or_default()
}

/// The recent searches of their file; a damaged file is no searches.
#[must_use]
pub fn searches_from_json(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(name: &str, at: (f64, f64)) -> SavedPlace {
        SavedPlace {
            name: name.to_string(),
            at,
            ..SavedPlace::default()
        }
    }

    #[test]
    fn a_list_survives_the_trip_through_its_geojson_file() {
        let mut list = PlaceList::new("my-trip", "My trip", ListKind::Named);
        list.places.push(SavedPlace {
            name: "Brandenburger Tor".to_string(),
            at: (52.516_275, 13.377_704),
            note: "meet at noon".to_string(),
            address: "Pariser Platz, 10117 Berlin".to_string(),
            url: "https://www.openstreetmap.org/node/2".to_string(),
            saved_s: 1_792_050_900,
        });
        list.routes.push(SavedRoute {
            name: "Walk".to_string(),
            points: vec![(52.5163, 13.3777), (52.5170, 13.3888)],
        });
        let text = list.to_geojson();
        let doc: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(doc["type"], "FeatureCollection");
        assert_eq!(doc["features"][0]["geometry"]["type"], "Point");
        assert_eq!(doc["features"][0]["geometry"]["coordinates"][0], 13.377_704, "GeoJSON is lon, lat");
        assert_eq!(doc["features"][1]["geometry"]["type"], "LineString");
        let back = PlaceList::from_geojson("my-trip", &text).expect("a list");
        assert_eq!(back, list);
    }

    #[test]
    fn any_geojson_points_are_places_named_by_their_common_properties() {
        let text = r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[13.4132,52.5219]},"properties":{"title":"Alexanderplatz","description":"the clock"}},
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[]},"properties":{"name":"an area"}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[12.3731,51.3397,120]},"properties":{}}
        ]}"#;
        let list = PlaceList::from_geojson("imported", text).expect("a list");
        assert_eq!(list.kind, ListKind::Named);
        assert_eq!(list.places.len(), 2, "the polygon is skipped");
        assert_eq!(list.places[0].name, "Alexanderplatz");
        assert_eq!(list.places[0].note, "the clock");
        assert_eq!(list.places[0].at, (52.5219, 13.4132));
        assert_eq!(list.places[1].name, "", "a point without a name is kept, unnamed");
        assert!(PlaceList::from_geojson("x", "[]").is_err());
        assert!(PlaceList::from_geojson("x", "not json").is_err());
    }

    #[test]
    fn the_kind_of_a_list_is_kept_in_its_file() {
        let favourites = PlaceList::new(FAVOURITES, "Favourites", ListKind::Favourites);
        let back = PlaceList::from_geojson(FAVOURITES, &favourites.to_geojson()).expect("a list");
        assert_eq!(back.kind, ListKind::Favourites);
        assert_eq!(back.name, "Favourites");
        let recents = PlaceList::new(RECENTS, "Recent", ListKind::Recents);
        assert_eq!(PlaceList::from_geojson(RECENTS, &recents.to_geojson()).expect("a list").kind, ListKind::Recents);
    }

    #[test]
    fn a_recent_destination_goes_first_once_and_the_list_is_capped() {
        let mut recents = PlaceList::new(RECENTS, "Recent", ListKind::Recents);
        recents.remember(place("Leipzig Hbf", (51.3455, 12.3821)), 3);
        recents.remember(place("Berlin Hbf", (52.5250, 13.3691)), 3);
        recents.remember(place("Leipzig Hbf", (51.34551, 12.38212)), 3);
        let names: Vec<&str> = recents.places.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Leipzig Hbf", "Berlin Hbf"], "the same place once, newest first");
        recents.remember(place("Dresden Hbf", (51.0404, 13.7320)), 3);
        recents.remember(place("Halle Hbf", (51.4776, 11.9871)), 3);
        assert_eq!(recents.places.len(), 3);
        assert_eq!(recents.places[0].name, "Halle Hbf");
        assert!(recents.has_place_at((51.0404, 13.7321)));
        assert!(!recents.has_place_at((52.5250, 13.3691)), "Berlin fell off the end");
    }

    #[test]
    fn a_slug_names_a_lists_file() {
        assert_eq!(slug("My Trip to Rome"), "my-trip-to-rome");
        assert_eq!(slug("Cafés in Wien!"), "cafes-in-wien");
        assert_eq!(slug("  "), "list");
        assert_eq!(slug("北京"), "list", "a name without latin letters still makes a file name");
    }

    #[test]
    fn a_search_finds_saved_places_favourites_first() {
        let mut favourites = PlaceList::new(FAVOURITES, "Favourites", ListKind::Favourites);
        favourites.places.push(place("Café Einstein", (52.5, 13.35)));
        let mut trip = PlaceList::new("trip", "Trip", ListKind::Named);
        trip.places.push(SavedPlace {
            address: "Unter den Linden 42".to_string(),
            ..place("Einstein Kaffee", (52.51, 13.39))
        });
        trip.places.push(place("Museum", (52.52, 13.40)));
        let mut recents = PlaceList::new(RECENTS, "Recent", ListKind::Recents);
        recents.places.push(place("Einsteinufer", (52.517, 13.32)));
        let lists = vec![recents, trip, favourites];
        let hits = search(&lists, "einst", 10);
        let names: Vec<&str> = hits.iter().map(|h| lists[h.list].places[h.place].name.as_str()).collect();
        assert_eq!(names, ["Café Einstein", "Einstein Kaffee", "Einsteinufer"]);
        assert_eq!(search(&lists, "linden", 10).len(), 1, "by the address too");
        assert_eq!(search(&lists, "cafe", 10).len(), 1, "without accents");
        assert!(search(&lists, "", 10).is_empty());
        assert_eq!(search(&lists, "e", 2).len(), 2, "at most the limit");
    }

    #[test]
    fn recent_searches_are_kept_newest_first_once() {
        let mut searches = Vec::new();
        remember_search(&mut searches, "Leipzig Hbf");
        remember_search(&mut searches, "Berlin Hbf");
        remember_search(&mut searches, " leipzig hbf ");
        assert_eq!(searches, ["leipzig hbf", "Berlin Hbf"], "the newest wording, once");
        remember_search(&mut searches, "  ");
        assert_eq!(searches.len(), 2, "nothing typed is no search");
        for i in 0..30 {
            remember_search(&mut searches, &format!("place {i}"));
        }
        assert_eq!(searches.len(), SEARCHES_LIMIT);
        assert_eq!(searches_from_json(&searches_to_json(&searches)), searches);
        assert!(searches_from_json("{broken").is_empty());
    }
}
