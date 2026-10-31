//! Offline areas: the user picks a rectangle on the map and names it, and
//! the app downloads its routing tiles (and the map's own tiles) ahead of
//! time, so routes there work without the network.
//!
//! The hosting's tile list (`tiles.txt`, named by the manifest) says which
//! routing tiles exist and how big each is, so a download asks only for real
//! tiles and can say beforehand how many megabytes it takes ([`plan`]). The
//! tiles go into the tile cache like any other ([`download`]); the area is
//! then pinned ([`pin`]): a small JSON file per area under the cache folder
//! lists its tiles, and pruning the cache skips them. They still refresh
//! when their keep time has passed and the network is there. Deleting the
//! area ([`unpin`]) lets them go again.

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::{
    router::{fetch_parallel, TileSource},
    tile::{tile_of, TileId, LEVELS},
};

/// A rectangle in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    /// The southern edge, degrees of latitude.
    pub south: f64,
    /// The western edge, degrees of longitude.
    pub west: f64,
    /// The northern edge.
    pub north: f64,
    /// The eastern edge.
    pub east: f64,
}

/// Every tile of zoom `z` that overlaps `area`.
#[must_use]
pub fn tiles_in(area: Area, z: u8) -> Vec<TileId> {
    // The north-west corner has the smallest x and y.
    let top_left = tile_of(area.north, area.west, z);
    let bottom_right = tile_of(area.south, area.east, z);
    let mut out = Vec::new();
    for x in top_left.x..=bottom_right.x {
        for y in top_left.y..=bottom_right.y {
            out.push(TileId::new(z, x, y));
        }
    }
    out
}

/// The routing tiles of `area` on all three zooms.
#[must_use]
pub fn routing_tiles(area: Area) -> Vec<TileId> {
    LEVELS.iter().flat_map(|&z| tiles_in(area, z)).collect()
}

/// The map's own (vector) tiles of `area`, zoom 0 to `max_zoom`: what the
/// map widget needs to draw the area offline.
#[must_use]
pub fn map_tiles(area: Area, max_zoom: u8) -> Vec<TileId> {
    (0..=max_zoom).flat_map(|z| tiles_in(area, z)).collect()
}

/// Reads the hosting's tile list: one `z/x/y bytes` per line.
#[must_use]
pub fn parse_index(text: &str) -> BTreeMap<TileId, u64> {
    text.lines()
        .filter_map(|line| {
            let (path, bytes) = line.trim().split_once(' ')?;
            Some((parse_tile_path(path)?, bytes.trim().parse().ok()?))
        })
        .collect()
}

/// Writes the hosting's tile list (what [`parse_index`] reads).
#[must_use]
pub fn write_index(index: &BTreeMap<TileId, u64>) -> String {
    index.iter().map(|(tile, bytes)| format!("{} {bytes}\n", tile.path())).collect()
}

/// `14/8530/5973` back to its tile.
fn parse_tile_path(path: &str) -> Option<TileId> {
    let mut parts = path.split('/');
    let z = parts.next()?.parse().ok()?;
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some(TileId::new(z, x, y))
}

/// The routing tiles of `area` that exist, and how many bytes they take.
#[must_use]
pub fn plan(area: Area, index: &BTreeMap<TileId, u64>) -> (Vec<TileId>, u64) {
    let tiles: Vec<TileId> = routing_tiles(area).into_iter().filter(|t| index.contains_key(t)).collect();
    let bytes = tiles.iter().map(|t| index[t]).sum();
    (tiles, bytes)
}

/// One offline area as the app keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct OfflineArea {
    /// The name the user gave it, shown in the list of areas; it also names
    /// the area's file, so two areas need two names.
    pub name: String,
    /// The rectangle the user picked.
    pub area: Area,
    /// The routing tiles downloaded for it: pruning the cache never deletes
    /// these while the area is kept.
    pub tiles: Vec<TileId>,
    /// Their size as served, bytes, for the list of areas.
    pub bytes: u64,
}

impl OfflineArea {
    /// The area as JSON (what [`Self::parse`] reads).
    #[must_use]
    pub fn to_json(&self) -> String {
        let a = self.area;
        let doc = json!({
            "name": self.name,
            "area": [a.south, a.west, a.north, a.east],
            "tiles": self.tiles.iter().map(TileId::path).collect::<Vec<_>>(),
            "bytes": self.bytes,
        });
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    }

    /// Reads an area's file.
    ///
    /// # Errors
    /// Not JSON, or without a name.
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("offline area: {e}"))?;
        let name = v["name"].as_str().ok_or("offline area: no name")?.to_string();
        let corner = |i: usize| v["area"][i].as_f64().unwrap_or(0.0);
        Ok(Self {
            name,
            area: Area { south: corner(0), west: corner(1), north: corner(2), east: corner(3) },
            tiles: v["tiles"]
                .as_array()
                .map(|tiles| tiles.iter().filter_map(|t| parse_tile_path(t.as_str()?)).collect())
                .unwrap_or_default(),
            bytes: v["bytes"].as_u64().unwrap_or(0),
        })
    }
}

/// The folder of the offline areas under a tile cache folder.
#[must_use]
pub fn areas_dir(cache: &Path) -> PathBuf {
    cache.join("areas")
}

/// The file of the area named `name`: its name made safe for a file name,
/// plus a hash of the whole name so that names differing only in other
/// characters do not share a file.
fn area_file(cache: &Path, name: &str) -> PathBuf {
    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    areas_dir(cache).join(format!("{}-{:08x}.json", slug.trim_matches('-'), hash & 0xffff_ffff))
}

/// Keeps `area` under the tile cache `cache`: its tiles are pinned from now
/// on. Returns the area's file.
///
/// # Errors
/// The file cannot be written.
pub fn pin(cache: &Path, area: &OfflineArea) -> Result<PathBuf, String> {
    let path = area_file(cache, &area.name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, area.to_json()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Deletes the area named `name`: its tiles may be pruned again. An area
/// that is not there is no error.
///
/// # Errors
/// The file is there but cannot be deleted.
pub fn unpin(cache: &Path, name: &str) -> Result<(), String> {
    let path = area_file(cache, name);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Every area kept under the tile cache `cache`, by name. A file that does
/// not read is left out.
#[must_use]
pub fn areas(cache: &Path) -> Vec<OfflineArea> {
    let Ok(entries) = std::fs::read_dir(areas_dir(cache)) else {
        return Vec::new();
    };
    let mut out: Vec<OfflineArea> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| OfflineArea::parse(&std::fs::read_to_string(e.path()).ok()?).ok())
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The tiles of every area kept under `cache`: the ones pruning skips.
#[must_use]
pub fn pinned(cache: &Path) -> HashSet<TileId> {
    areas(cache).into_iter().flat_map(|a| a.tiles).collect()
}

/// How far a download got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadStep {
    /// The tiles answered so far ...
    pub done: usize,
    /// ... of this many.
    pub total: usize,
    /// The bytes they took.
    pub bytes: u64,
    /// The tiles that could not be had (they are tried again next time).
    pub failed: usize,
}

/// What a whole download did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Downloaded {
    /// The tiles asked for.
    pub tiles: usize,
    /// The bytes of the tiles that came.
    pub bytes: u64,
    /// The tiles that could not be had.
    pub failed: usize,
}

/// Fetches `tiles` through `source` on `threads` threads, so that a cache
/// in front of the download keeps them, and reports each answer to
/// `progress`; `progress` returns `false` to stop.
///
/// # Errors
/// `progress` said to stop.
pub fn download(
    source: &dyn TileSource,
    tiles: &[TileId],
    threads: usize,
    progress: &mut dyn FnMut(DownloadStep) -> bool,
) -> Result<Downloaded, String> {
    let mut step = DownloadStep { done: 0, total: tiles.len(), bytes: 0, failed: 0 };
    let mut stopped = false;
    fetch_parallel(source, tiles, threads, &mut |_, answer| {
        match answer {
            Ok(Some(bytes)) => step.bytes += bytes.len() as u64,
            Ok(None) => {}
            Err(_) => step.failed += 1,
        }
        step.done += 1;
        stopped = !progress(step);
        !stopped
    });
    if stopped {
        return Err("the download was stopped".into());
    }
    Ok(Downloaded { tiles: step.done, bytes: step.bytes, failed: step.failed })
}
