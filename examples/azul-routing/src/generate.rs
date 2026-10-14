//! The tile generator: an OpenStreetMap extract cut into routing tiles.
//!
//! Two passes over the extract: the first keeps the routable ways (a
//! `highway=*` routing knows) and notes their nodes, the second reads those
//! nodes' positions. Then every level cuts the ways its road kinds are on
//! (motorway, trunk and primary on zoom 8; secondary and up on zoom 11;
//! everything on zoom 14) into the tiles their nodes are in. A tile lists
//! each way that has a node in it, whole, and the positions of its own
//! nodes plus their neighbours along the way, so every stretch of road that
//! touches the tile can be built from the tile alone.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::{Read, Write},
    path::Path,
};

use crate::{
    format::{parse_maxspeed, write_tile, Access, Highway, Oneway, TileData, Way},
    pbf::{read_blocks, OsmWay},
    tile::{tile_of, TileId, LEVELS},
};

/// What the generator wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Generated {
    /// The routable ways it found.
    pub ways: usize,
    /// The nodes they use.
    pub nodes: usize,
    /// The tiles written, per level (zoom, count).
    pub tiles: Vec<(u8, usize)>,
}

/// A routing way read from its OSM tags; `None` for a way routing does not
/// use (not a road, an area, a road under construction).
#[must_use]
pub fn way_from_tags(id: i64, nodes: Vec<i64>, tags: &[(String, String)]) -> Option<Way> {
    let tag = |key: &str| tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let highway = Highway::parse(tag("highway")?)?;
    if tag("area") == Some("yes") || nodes.len() < 2 {
        return None;
    }
    let allowed = |value: Option<&str>| match value? {
        "yes" | "designated" | "permissive" | "destination" => Some(true),
        "no" | "private" | "dismount" | "use_sidepath" => Some(false),
        _ => None,
    };
    let roundabout = matches!(tag("junction"), Some("roundabout" | "circular"));
    let oneway = match tag("oneway") {
        Some("yes" | "true" | "1") => Oneway::Forward,
        Some("-1" | "reverse") => Oneway::Backward,
        Some("no" | "false" | "0") => Oneway::No,
        _ if roundabout || highway == Highway::Motorway => Oneway::Forward,
        _ => Oneway::No,
    };
    let access = allowed(tag("access"));
    Some(Way {
        id,
        nodes,
        highway,
        oneway,
        bicycle_both_ways: tag("oneway:bicycle") == Some("no"),
        maxspeed_kmh: tag("maxspeed").and_then(parse_maxspeed),
        access: Access {
            motor: allowed(tag("motor_vehicle"))
                .or_else(|| allowed(tag("motorcar")))
                .or(access),
            bicycle: allowed(tag("bicycle")).or(access),
            foot: allowed(tag("foot")).or(access),
        },
        name: tag("name").map(str::to_string),
    })
}

/// Cuts the routable ways of an extract into tiles: `open()` opens the
/// extract (it is read twice), `write(tile, bytes)` stores one gzipped tile.
///
/// # Errors
/// The extract does not read ([`read_blocks`]), or `write` fails.
pub fn generate(
    open: &mut dyn FnMut() -> Result<Box<dyn Read>, String>,
    write: &mut dyn FnMut(TileId, &[u8]) -> Result<(), String>,
) -> Result<Generated, String> {
    let mut ways: Vec<Way> = Vec::new();
    let mut wanted: HashSet<i64> = HashSet::new();
    read_blocks(&mut *open()?, true, &mut |block| {
        for OsmWay { id, nodes, tags } in block.ways {
            if let Some(way) = way_from_tags(id, nodes, &tags) {
                wanted.extend(way.nodes.iter().copied());
                ways.push(way);
            }
        }
        Ok(())
    })?;
    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(wanted.len());
    read_blocks(&mut *open()?, false, &mut |block| {
        for (id, lat, lon) in block.nodes {
            if wanted.contains(&id) {
                coords.insert(id, (lat, lon));
            }
        }
        Ok(())
    })?;
    let mut generated = Generated {
        ways: ways.len(),
        nodes: coords.len(),
        tiles: Vec::new(),
    };
    for z in LEVELS {
        let tiles = cut_level(&ways, &coords, z);
        generated.tiles.push((z, tiles.len()));
        for (tile, data) in tiles {
            write(tile, &gzip(write_tile(tile, &data).as_bytes())?)?;
        }
    }
    Ok(generated)
}

/// The tiles of level `z`: each way on the level in every tile it has a
/// node in, with that tile's nodes and their neighbours along the way.
#[must_use]
pub fn cut_level(ways: &[Way], coords: &HashMap<i64, (f64, f64)>, z: u8) -> BTreeMap<TileId, TileData> {
    let mut tile_ways: BTreeMap<TileId, BTreeSet<usize>> = BTreeMap::new();
    let mut tile_nodes: BTreeMap<TileId, BTreeSet<i64>> = BTreeMap::new();
    for (index, way) in ways.iter().enumerate() {
        if way.highway.level() > z {
            continue;
        }
        for (i, node) in way.nodes.iter().enumerate() {
            let Some(&(lat, lon)) = coords.get(node) else {
                continue;
            };
            let tile = tile_of(lat, lon, z);
            tile_ways.entry(tile).or_default().insert(index);
            let nodes = tile_nodes.entry(tile).or_default();
            let from = i.saturating_sub(1);
            let to = (i + 1).min(way.nodes.len() - 1);
            for n in &way.nodes[from..=to] {
                if coords.contains_key(n) {
                    nodes.insert(*n);
                }
            }
        }
    }
    tile_ways
        .into_iter()
        .map(|(tile, indices)| {
            let nodes = tile_nodes
                .get(&tile)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| coords.get(id).map(|&(lat, lon)| (*id, lat, lon)))
                        .collect()
                })
                .unwrap_or_default();
            let ways = indices.into_iter().map(|i| ways[i].clone()).collect();
            (tile, TileData { nodes, ways })
        })
        .collect()
}

/// `bytes` gzipped.
///
/// # Errors
/// Only on an allocation the encoder cannot make.
pub fn gzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

/// Writes tile `tile` under `root` as `<z>/<x>/<y>.json.gz`.
///
/// # Errors
/// The folder or the file cannot be written.
pub fn write_to_folder(root: &Path, tile: TileId, bytes: &[u8]) -> Result<(), String> {
    let dir = root.join(tile.z.to_string()).join(tile.x.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.json.gz", tile.y));
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
