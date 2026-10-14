//! AzMaps' road routing, worked out on the client from static tiles. No
//! azul types: the app drives it on a worker thread and hands it a
//! [`TileSource`] that downloads and caches.
//!
//! The tiles are Valhalla's road graph, cut into the map's XYZ tiles at zoom
//! 8, 11 and 14 and served as gzipped protobuf next to a `manifest.json`
//! (the azul-routing-tiles project builds and publishes them):
//!
//! - [`manifest`] reads the manifest: coverage, tile URL, licence, keep times;
//! - [`format`] reads one tile, with [`wire`] as its protobuf reader;
//! - [`graph`] joins the tiles into one graph, by keys every build agrees on;
//! - [`router`] finds a route with an edge-based A* that reads tiles as the
//!   search reaches them, after fetching the likely ones ([`corridor`]) in
//!   parallel; [`mode`] says how a car, a bicycle or a walker uses each edge;
//! - [`source`] has the tile sources: a folder, a download (feature `http`),
//!   and a cache in memory and on disk that keeps each zoom for weeks and
//!   serves old tiles when the network is gone;
//! - [`offline`] plans, downloads and pins the tiles of an area ahead.
//!
//! The `routing-tiles` binary cuts an OpenStreetMap extract into the same
//! tiles where no Valhalla build exists ([`generate`], [`build`], [`pbf`]).
//! Where no tiles are at hand, [`online`] asks a Valhalla server for the route.

pub mod build;
pub mod corridor;
pub mod format;
pub mod generate;
pub mod graph;
pub mod manifest;
pub mod mode;
pub mod offline;
pub mod online;
pub mod pbf;
pub mod router;
pub mod source;
pub mod tile;
pub mod wire;

#[cfg(test)]
mod tests;

pub use corridor::corridor;
pub use generate::generate;
pub use graph::Graph;
pub use manifest::Manifest;
pub use mode::Mode;
pub use router::{fetch_parallel, route, route_with, Options, Progress, Route, RouteError, Stage, TileSource};
pub use tile::TileId;

/// The great-circle distance between two places `(lat, lon)`, in metres.
#[must_use]
pub fn distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.1 - a.1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().min(1.0).asin()
}
