//! MVT tile decode + projection helpers for `MapWidget`. Opt-in via the
//! `map-tiles` Cargo feature so default builds don't pay for the
//! `td` / `mvt-reader` / `proj4rs` dep tree.
//!
//! Architecture (see `MOBILE_SESSION_LOG.md` for the full design and
//! the user's "MVT + MapCSS = SVG → DOM" pipeline):
//!
//! 1. `MapWidget`'s `VirtualView` callback computes the visible tiles
//!    (Web Mercator XYZ).
//! 2. For each visible tile not in cache, the widget enqueues a fetch.
//!    Fetch lands in a follow-up tick — needs a Thread/async surface.
//! 3. When the fetched PBF bytes arrive, `decode_mvt_tile` parses them
//!    via the `td` crate (`td::parse_mvt_tile`), returning a `Vec` of
//!    GeoJSON `Feature`s with WGS-84 coordinates.
//! 4. The next tick maps each Feature → SVG path string by applying
//!    the user's `MapCSS` stylesheet (parsed via the framework's
//!    existing CSS parser).
//! 5. The widget's `VirtualView` patches the SVG-as-DOM into the tile
//!    `<div>` as a child.
//!
//! This module is the entry point for step 3 only. Steps 1-2 are
//! `MapWidget` internals; steps 4-5 land in later ticks.

#[cfg(feature = "map-tiles")]
pub use td::{parse_mvt_tile, TileCoord};

#[cfg(feature = "map-tiles")]
mod svg;
#[cfg(feature = "map-tiles")]
pub use svg::features_to_svg;

/// Decode the PBF bytes of a single MVT tile into a `Vec` of GeoJSON
/// `Feature`s. Wraps `td::parse_mvt_tile` with the tile-coord
/// conversion from `azul_layout::widgets::map::MapTileId`.
///
/// Returns an error string when the `map-tiles` feature is disabled or
/// when `td` fails to parse the bytes. Callers shouldn't trust the
/// returned features to be in any particular order — `mvt-reader` walks
/// the layers as it finds them.
#[cfg(feature = "map-tiles")]
pub fn decode_mvt_tile(
    bytes: alloc::vec::Vec<u8>,
    tile: azul_layout::widgets::map::MapTileId,
) -> Result<alloc::vec::Vec<geojson::Feature>, alloc::string::String> {
    let coord = TileCoord {
        z: tile.z as u32,
        x: tile.x,
        y: tile.y,
    };
    td::parse_mvt_tile(bytes, &coord).map_err(|e| alloc::format!("{e:?}"))
}

/// Build the `https://host/{z}/{x}/{y}.pbf` URL for a tile, expanding
/// the same `{z}` / `{x}` / `{y}` placeholders Leaflet uses. The
/// substitution is hand-rolled instead of going through
/// `td::tile_coords_to_urls` because we need the result for a single
/// tile, not a batch. Always safe — never returns an error.
pub fn build_tile_url(
    url_template: &str,
    tile: azul_layout::widgets::map::MapTileId,
) -> alloc::string::String {
    use alloc::string::ToString;
    url_template
        .replace("{z}", &tile.z.to_string())
        .replace("{x}", &tile.x.to_string())
        .replace("{y}", &tile.y.to_string())
}

/// Stub used when the `map-tiles` feature is off — decode is a no-op
/// returning an empty `Vec`. Lets the `MapWidget` cache state machine
/// compile and run without dragging in the MVT dep tree.
#[cfg(not(feature = "map-tiles"))]
pub fn decode_mvt_tile(
    _bytes: alloc::vec::Vec<u8>,
    _tile: azul_layout::widgets::map::MapTileId,
) -> Result<alloc::vec::Vec<()>, alloc::string::String> {
    Err(alloc::string::String::from(
        "azul-dll built without `map-tiles` feature — MVT decode unavailable",
    ))
}
