//! The routing tiles' format: the Routable Tiles shape (openplannerteam), a
//! JSON-LD document per tile whose `@graph` lists the tile's OpenStreetMap
//! nodes (`osm:Node`, `geo:lat` / `geo:long`) and the routable ways through
//! it (`osm:Way`, `osm:hasNodes` and the tags routing reads). A way is in
//! every tile it has a node in; a tile also lists the nodes just outside it
//! that its ways step to, so every edge of the tile can be built from the
//! tile alone. A tile may be gzipped.

use std::io::Read;

use serde_json::{json, Map, Value};

use crate::tile::TileId;

/// The kind of road (`highway=*`), the ones routing knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Highway {
    Motorway,
    MotorwayLink,
    Trunk,
    TrunkLink,
    Primary,
    PrimaryLink,
    Secondary,
    SecondaryLink,
    Tertiary,
    TertiaryLink,
    Unclassified,
    Residential,
    LivingStreet,
    Service,
    Road,
    Track,
    Path,
    Footway,
    Cycleway,
    Bridleway,
    Pedestrian,
    Steps,
}

impl Highway {
    const ALL: [(Self, &'static str); 22] = [
        (Self::Motorway, "motorway"),
        (Self::MotorwayLink, "motorway_link"),
        (Self::Trunk, "trunk"),
        (Self::TrunkLink, "trunk_link"),
        (Self::Primary, "primary"),
        (Self::PrimaryLink, "primary_link"),
        (Self::Secondary, "secondary"),
        (Self::SecondaryLink, "secondary_link"),
        (Self::Tertiary, "tertiary"),
        (Self::TertiaryLink, "tertiary_link"),
        (Self::Unclassified, "unclassified"),
        (Self::Residential, "residential"),
        (Self::LivingStreet, "living_street"),
        (Self::Service, "service"),
        (Self::Road, "road"),
        (Self::Track, "track"),
        (Self::Path, "path"),
        (Self::Footway, "footway"),
        (Self::Cycleway, "cycleway"),
        (Self::Bridleway, "bridleway"),
        (Self::Pedestrian, "pedestrian"),
        (Self::Steps, "steps"),
    ];

    /// The OSM value (`motorway_link`) - also what [`Self::parse`] reads in
    /// any case, with or without the `osm:` prefix and with `CamelCase`
    /// (`osm:MotorwayLink`, as Routable Tiles writes it).
    #[must_use]
    pub fn key(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(h, _)| *h == self)
            .map_or("road", |(_, k)| k)
    }

    /// The kind a tag value names, `None` for a way that is not a road.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let bare: String = strip_osm(value)
            .chars()
            .filter(|c| *c != '_')
            .flat_map(char::to_lowercase)
            .collect();
        Self::ALL
            .iter()
            .find(|(_, k)| k.replace('_', "") == bare)
            .map(|(h, _)| *h)
    }

    /// The lowest routing level the road is on: 8 for motorway, trunk and
    /// primary roads, 11 for secondary ones, 14 for the rest.
    #[must_use]
    pub const fn level(self) -> u8 {
        match self {
            Self::Motorway
            | Self::MotorwayLink
            | Self::Trunk
            | Self::TrunkLink
            | Self::Primary
            | Self::PrimaryLink => 8,
            Self::Secondary | Self::SecondaryLink => 11,
            _ => 14,
        }
    }
}

/// Which way traffic may go along a way's nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Oneway {
    /// Both ways.
    #[default]
    No,
    /// In the order of the nodes only.
    Forward,
    /// Against it only (`oneway=-1`).
    Backward,
}

/// Who may use a way, from `access` and its per-mode refinements; `None`
/// where the way says nothing (the road kind decides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Access {
    pub motor: Option<bool>,
    pub bicycle: Option<bool>,
    pub foot: Option<bool>,
}

/// One routable way of a tile.
#[derive(Debug, Clone, PartialEq)]
pub struct Way {
    /// The OSM way id.
    pub id: i64,
    /// Its nodes' OSM ids, in order.
    pub nodes: Vec<i64>,
    pub highway: Highway,
    pub oneway: Oneway,
    /// Bicycles may ride against a car one-way (`oneway:bicycle=no`).
    pub bicycle_both_ways: bool,
    /// `maxspeed` in km/h, when it gives one.
    pub maxspeed_kmh: Option<f32>,
    pub access: Access,
    /// The road's name, for the route's description.
    pub name: Option<String>,
}

/// One tile, read: its nodes `(id, lat, lon)` and its routable ways.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TileData {
    pub nodes: Vec<(i64, f64, f64)>,
    pub ways: Vec<Way>,
}

/// `osm:yes` -> `yes`.
fn strip_osm(value: &str) -> &str {
    value.strip_prefix("osm:").unwrap_or(value)
}

/// The OSM id at the end of an `@id` (`http://www.openstreetmap.org/node/42`
/// -> 42); a bare number too.
fn osm_id(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.rsplit('/').find(|p| !p.is_empty())?.parse().ok(),
        _ => None,
    }
}

fn tag<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object
        .get(&format!("osm:{key}"))
        .or_else(|| object.get(key))
        .and_then(Value::as_str)
}

/// `yes` / `true` / `1` / `designated` / `permissive` -> may; `no` /
/// `private` / `0` -> may not; anything else says nothing.
fn allowed(value: Option<&str>) -> Option<bool> {
    match strip_osm(value?).to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" | "designated" | "permissive" | "destination" => Some(true),
        "no" | "private" | "false" | "0" | "dismount" | "use_sidepath" => Some(false),
        _ => None,
    }
}

/// `maxspeed` in km/h: `50`, `30 mph`, the country words for town and land
/// (`DE:urban`), `walk`, `none` (no limit: the car's top speed).
#[must_use]
pub fn parse_maxspeed(value: &str) -> Option<f32> {
    let v = strip_osm(value).trim().to_ascii_lowercase();
    if let Some(mph) = v.strip_suffix("mph") {
        return mph.trim().parse::<f32>().ok().map(|s| s * 1.609_344);
    }
    if let Ok(kmh) = v.trim_end_matches("km/h").trim().parse::<f32>() {
        return (kmh > 0.0).then_some(kmh);
    }
    match v.rsplit(':').next()? {
        "urban" => Some(50.0),
        "rural" => Some(100.0),
        "motorway" => Some(130.0),
        "trunk" => Some(100.0),
        "living_street" | "walk" => Some(7.0),
        "zone30" | "zone:30" => Some(30.0),
        "none" | "signals" => Some(130.0),
        _ => None,
    }
}

/// Reads one tile (JSON, or gzipped JSON).
///
/// # Errors
/// The bytes are not a gzip stream that unpacks, or not JSON with an
/// `@graph`.
pub fn parse_tile(bytes: &[u8]) -> Result<TileData, String> {
    let unpacked;
    let text = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut out)
            .map_err(|e| format!("gzip: {e}"))?;
        unpacked = out;
        &unpacked[..]
    } else {
        bytes
    };
    let doc: Value = serde_json::from_slice(text).map_err(|e| format!("json: {e}"))?;
    let graph = doc
        .get("@graph")
        .and_then(Value::as_array)
        .ok_or_else(|| "no @graph".to_string())?;
    let mut tile = TileData::default();
    for item in graph {
        let Some(object) = item.as_object() else {
            continue;
        };
        let kind = object.get("@type").and_then(Value::as_str).unwrap_or("");
        let Some(id) = object.get("@id").and_then(osm_id) else {
            continue;
        };
        match strip_osm(kind) {
            "Node" => {
                let lat = object.get("geo:lat").and_then(Value::as_f64);
                let lon = object.get("geo:long").and_then(Value::as_f64);
                if let (Some(lat), Some(lon)) = (lat, lon) {
                    tile.nodes.push((id, lat, lon));
                }
            }
            "Way" => {
                if let Some(way) = read_way(id, object) {
                    tile.ways.push(way);
                }
            }
            _ => {}
        }
    }
    Ok(tile)
}

fn read_way(id: i64, object: &Map<String, Value>) -> Option<Way> {
    let highway = Highway::parse(tag(object, "highway")?)?;
    let nodes: Vec<i64> = object
        .get("osm:hasNodes")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(osm_id)
        .collect();
    if nodes.len() < 2 {
        return None;
    }
    let roundabout = tag(object, "junction")
        .is_some_and(|j| matches!(strip_osm(j), "roundabout" | "circular"));
    let oneway = match tag(object, "oneway").map(strip_osm) {
        Some("yes" | "true" | "1") => Oneway::Forward,
        Some("-1" | "reverse") => Oneway::Backward,
        Some("no" | "false" | "0") => Oneway::No,
        _ if roundabout || matches!(highway, Highway::Motorway) => Oneway::Forward,
        _ => Oneway::No,
    };
    let access = allowed(tag(object, "access"));
    Some(Way {
        id,
        nodes,
        highway,
        oneway,
        bicycle_both_ways: tag(object, "oneway:bicycle").map(strip_osm) == Some("no"),
        maxspeed_kmh: tag(object, "maxspeed").and_then(parse_maxspeed),
        access: Access {
            motor: allowed(tag(object, "motor_vehicle"))
                .or_else(|| allowed(tag(object, "motorcar")))
                .or(access),
            bicycle: allowed(tag(object, "bicycle")).or(access),
            foot: allowed(tag(object, "foot")).or(access),
        },
        name: tag(object, "name").map(str::to_string),
    })
}

/// One tile as JSON in the Routable Tiles shape (what [`parse_tile`] reads).
#[must_use]
pub fn write_tile(tile: TileId, data: &TileData) -> String {
    let mut graph: Vec<Value> = data
        .nodes
        .iter()
        .map(|&(id, lat, lon)| {
            json!({
                "@id": format!("http://www.openstreetmap.org/node/{id}"),
                "@type": "osm:Node",
                "geo:lat": lat,
                "geo:long": lon,
            })
        })
        .collect();
    let word = |may: Option<bool>| may.map(|m| if m { "osm:yes" } else { "osm:no" });
    for way in &data.ways {
        let mut object = Map::new();
        object.insert(
            "@id".into(),
            Value::String(format!("http://www.openstreetmap.org/way/{}", way.id)),
        );
        object.insert("@type".into(), Value::String("osm:Way".into()));
        object.insert(
            "osm:highway".into(),
            Value::String(format!("osm:{}", way.highway.key())),
        );
        object.insert(
            "osm:hasNodes".into(),
            Value::Array(
                way.nodes
                    .iter()
                    .map(|n| Value::String(format!("http://www.openstreetmap.org/node/{n}")))
                    .collect(),
            ),
        );
        match way.oneway {
            Oneway::No => {}
            Oneway::Forward => {
                object.insert("osm:oneway".into(), Value::String("osm:yes".into()));
            }
            Oneway::Backward => {
                object.insert("osm:oneway".into(), Value::String("osm:-1".into()));
            }
        }
        if way.bicycle_both_ways {
            object.insert("osm:oneway:bicycle".into(), Value::String("osm:no".into()));
        }
        if let Some(speed) = way.maxspeed_kmh {
            object.insert("osm:maxspeed".into(), Value::String(format!("{speed:.0}")));
        }
        for (key, may) in [
            ("osm:motor_vehicle", way.access.motor),
            ("osm:bicycle", way.access.bicycle),
            ("osm:foot", way.access.foot),
        ] {
            if let Some(w) = word(may) {
                object.insert(key.into(), Value::String(w.into()));
            }
        }
        if let Some(name) = &way.name {
            object.insert("osm:name".into(), Value::String(name.clone()));
        }
        graph.push(Value::Object(object));
    }
    json!({
        "@context": {
            "osm": "https://w3id.org/openstreetmap/terms#",
            "geo": "http://www.w3.org/2003/01/geo/wgs84_pos#",
            "tiles": "https://w3id.org/tree/terms#",
            "osm:hasNodes": { "@container": "@list", "@type": "@id" },
        },
        "@id": format!("{}/", tile.path()),
        "tiles:zoom": tile.z,
        "tiles:longitudeTile": tile.x,
        "tiles:latitudeTile": tile.y,
        "@graph": graph,
    })
    .to_string()
}
