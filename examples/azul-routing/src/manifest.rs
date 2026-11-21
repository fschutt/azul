//! The tile set's manifest, `manifest.json` at the root of the hosting: what
//! the tiles cover, where they are, which build they come from, how long a
//! client may keep them, and the licence the data comes under.
//!
//! AzMaps' `endpoints.routing` (and `--routing`, `AZMAPS_ROUTING`) names the
//! manifest's URL or path. The tile template in it is relative to the
//! manifest, so the whole folder can move to another host (GitHub Pages, a
//! bucket, a local folder) unchanged.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::tile::TileId;

/// What the manifest's `format` says.
pub const FORMAT: &str = "azul-routing-tiles";

/// Where a tile set keeps its tiles, relative to its manifest: what the
/// builds write, and what a folder without a manifest holds.
pub const DEFAULT_TILES: &str = "tiles/{z}/{x}/{y}.pbf";

/// The attribution OpenStreetMap's licence asks for next to every route
/// (a route is a Produced Work under the ODbL) ...
pub const OSM_ATTRIBUTION: &str = "© OpenStreetMap contributors";
/// ... and the page it links to.
pub const OSM_COPYRIGHT_URL: &str = "https://www.openstreetmap.org/copyright";
/// The licence of every tile set built from OpenStreetMap ...
pub const ODBL: &str = "ODbL-1.0";
/// ... and its text.
pub const ODBL_URL: &str = "https://opendatacommons.org/licenses/odbl/1-0/";

/// One source extract the tiles were built from.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// A short name, `monaco`.
    pub id: String,
    /// The extract's URL.
    pub source: String,
    /// The time of the newest OpenStreetMap edit in it, RFC 3339.
    pub osm_timestamp: String,
    /// `[west, south, east, north]` degrees.
    pub bbox: [f64; 4],
}

/// How many tiles one zoom has and how many bytes they take as served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelStats {
    /// The zoom: 8, 11 or 14.
    pub z: u8,
    /// How many tiles it has.
    pub tiles: u64,
    /// Their size as served (gzipped), bytes.
    pub bytes: u64,
}

/// The manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    /// The format version of the tiles, [`crate::format::VERSION`].
    pub version: u32,
    /// Names this build, e.g. `2026-10-14-monaco`. Shown, never needed to
    /// read a tile: keys stay the same across builds.
    pub build: String,
    /// The URL of a tile relative to the manifest, with `{z}`, `{x}`, `{y}`.
    pub tiles: String,
    /// The tile list (`z/x/y bytes` per line) relative to the manifest: an
    /// offline download asks only for tiles that exist.
    pub index: Option<String>,
    /// `[west, south, east, north]` of everything covered.
    pub bbox: [f64; 4],
    /// The extracts the tiles were built from, each with its own coverage.
    pub regions: Vec<Region>,
    /// When the tiles were built, RFC 3339.
    pub built_at: String,
    /// The Valhalla version the graph comes from (`3.9.1`); empty for tiles
    /// azul-routing's own generator wrote.
    pub valhalla: String,
    /// The Valhalla image (or wheel) that built it, so a build can be redone.
    pub valhalla_image: String,
    /// The attribution a client shows next to every route: the ODbL notice
    /// of a Produced Work, "© OpenStreetMap contributors".
    pub attribution: String,
    /// The link the attribution leads to.
    pub attribution_url: String,
    /// The data licence (`ODbL-1.0`).
    pub license: String,
    /// The licence text's URL.
    pub license_url: String,
    /// How many days the hosting suggests keeping a tile, for zoom 8, 11 and
    /// 14. The user's settings win over these.
    pub keep_days: [u32; 3],
    /// How many tiles and bytes each zoom has: the size of a full download.
    pub levels: Vec<LevelStats>,
}

impl Manifest {
    /// Reads the manifest at `path`, or the `manifest.json` in the folder
    /// `path`.
    ///
    /// # Errors
    /// The file cannot be read, or [`Self::parse`] refuses it.
    pub fn read_file(path: &Path) -> Result<Self, String> {
        let path = manifest_path(path);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }

    /// Reads `manifest.json`.
    ///
    /// # Errors
    /// Not JSON, not this format, or a version this crate does not read.
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("manifest: {e}"))?;
        if v["format"].as_str() != Some(FORMAT) {
            return Err("manifest: not an azul-routing-tiles manifest".into());
        }
        let version = u32::try_from(v["version"].as_u64().unwrap_or(0)).unwrap_or(0);
        if version != crate::format::VERSION {
            return Err(format!("manifest: tile format version {version} is not read here"));
        }
        let text = |value: &Value| value.as_str().unwrap_or_default().to_string();
        let bbox = |value: &Value| {
            let mut out = [0.0; 4];
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = value[i].as_f64().unwrap_or(0.0);
            }
            out
        };
        let tiles = text(&v["tiles"]);
        if !(tiles.contains("{z}") && tiles.contains("{x}") && tiles.contains("{y}")) {
            return Err("manifest: the tile template lacks {z}, {x} or {y}".into());
        }
        let days = |z: &str, default: u32| {
            v["keep_days"][z]
                .as_u64()
                .and_then(|d| u32::try_from(d).ok())
                .unwrap_or(default)
        };
        Ok(Self {
            version,
            build: text(&v["build"]),
            tiles,
            index: v["index"].as_str().map(str::to_string),
            bbox: bbox(&v["bbox"]),
            regions: v["regions"]
                .as_array()
                .map(|regions| {
                    regions
                        .iter()
                        .map(|r| Region {
                            id: text(&r["id"]),
                            source: text(&r["source"]),
                            osm_timestamp: text(&r["osm_timestamp"]),
                            bbox: bbox(&r["bbox"]),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            built_at: text(&v["built_at"]),
            valhalla: text(&v["generator"]["valhalla"]),
            valhalla_image: text(&v["generator"]["image"]),
            attribution: text(&v["attribution"]["text"]),
            attribution_url: text(&v["attribution"]["url"]),
            license: text(&v["license"]["id"]),
            license_url: text(&v["license"]["url"]),
            keep_days: [days("8", 56), days("11", 28), days("14", 14)],
            levels: v["levels"]
                .as_array()
                .map(|levels| {
                    levels
                        .iter()
                        .map(|l| LevelStats {
                            z: u8::try_from(l["z"].as_u64().unwrap_or(0)).unwrap_or(0),
                            tiles: l["tiles"].as_u64().unwrap_or(0),
                            bytes: l["bytes"].as_u64().unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// The manifest as JSON (what [`Self::parse`] reads).
    #[must_use]
    pub fn to_json(&self) -> String {
        let regions: Vec<Value> = self
            .regions
            .iter()
            .map(|r| json!({"id": r.id, "source": r.source, "osm_timestamp": r.osm_timestamp, "bbox": r.bbox}))
            .collect();
        let levels: Vec<Value> = self
            .levels
            .iter()
            .map(|l| json!({"z": l.z, "tiles": l.tiles, "bytes": l.bytes}))
            .collect();
        let doc = json!({
            "format": FORMAT,
            "version": self.version,
            "build": self.build,
            "tiles": self.tiles,
            "index": self.index,
            "encoding": "gzip",
            "bbox": self.bbox,
            "regions": regions,
            "built_at": self.built_at,
            "generator": {"valhalla": self.valhalla, "image": self.valhalla_image},
            "attribution": {"text": self.attribution, "url": self.attribution_url},
            "license": {"id": self.license, "url": self.license_url},
            "keep_days": {"8": self.keep_days[0], "11": self.keep_days[1], "14": self.keep_days[2]},
            "levels": levels,
        });
        serde_json::to_string_pretty(&doc).unwrap_or_default()
    }

    /// The URL (or path) of `tile`, given the manifest's own URL (or path).
    #[must_use]
    pub fn tile_url(&self, manifest_url: &str, tile: TileId) -> String {
        resolve(manifest_url, &fill(&self.tiles, tile))
    }

    /// The URL (or path) of the tile list, given the manifest's own URL (or
    /// path); `None` when the hosting has none.
    #[must_use]
    pub fn index_url(&self, manifest_url: &str) -> Option<String> {
        self.index.as_deref().map(|index| resolve(manifest_url, index))
    }

    /// Whether `(lat, lon)` lies in one of the regions built.
    #[must_use]
    pub fn covers(&self, lat: f64, lon: f64) -> bool {
        let inside = |b: &[f64; 4]| b[0] <= lon && lon <= b[2] && b[1] <= lat && lat <= b[3];
        if self.regions.is_empty() {
            inside(&self.bbox)
        } else {
            self.regions.iter().any(|r| inside(&r.bbox))
        }
    }
}

/// `relative` resolved against the folder `base` (a URL or a path) is in.
/// An absolute `relative` (`https://...` or `/...`) stays as it is.
#[must_use]
pub fn resolve(base: &str, relative: &str) -> String {
    if relative.contains("://") || relative.starts_with('/') {
        return relative.to_string();
    }
    match base.rfind('/') {
        Some(slash) => format!("{}{relative}", &base[..=slash]),
        None => relative.to_string(),
    }
}

/// `template` with `{z}`, `{x}` and `{y}` of `tile` filled in.
#[must_use]
pub fn fill(template: &str, tile: TileId) -> String {
    template
        .replace("{z}", &tile.z.to_string())
        .replace("{x}", &tile.x.to_string())
        .replace("{y}", &tile.y.to_string())
}

/// Whether a routing value (`--routing`, `AZMAPS_ROUTING`,
/// `endpoints.routing`) names a manifest - a URL, a file or a folder holding
/// `manifest.json` - rather than a bare tile template with `{z}` in it.
#[must_use]
pub fn is_manifest(value: &str) -> bool {
    !value.contains("{z}")
}

/// The manifest file a path names: the path itself, or the `manifest.json`
/// in it when it is a folder.
#[must_use]
pub fn manifest_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("manifest.json")
    } else {
        path.to_path_buf()
    }
}
