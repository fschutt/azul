//! AzMaps' road routing, worked out on the client from static tiles
//! (`examples/azul-maps/ROUTING.md`). No azul types: the app drives it on a
//! worker thread and hands it a [`TileSource`] that downloads and caches.
//!
//! One route, start to destination:
//!
//! 1. [`corridor`] names the tiles along the straight line: zoom 14 (every
//!    road) near the ends, zoom 11 (secondary roads and up) further out,
//!    zoom 8 (motorway, trunk, primary) in the middle of a long trip;
//! 2. the [`TileSource`] fetches the ones not loaded yet, in parallel
//!    ([`fetch_parallel`]), and each is read ([`format::parse_tile`]) into one
//!    [`Graph`] for the travel mode - a road shared by two levels meets itself
//!    at the same OSM node ids, which is how A* changes level;
//! 3. both ends snap to the nearest usable road and [`astar`] searches the
//!    graph for the fastest way, in milliseconds of travel time;
//! 4. without a way the corridor widens and only the new tiles are fetched,
//!    twice, before the route is given up.
//!
//! [`route`] runs all four and reports its progress; the `routing-tiles`
//! binary cuts an OpenStreetMap extract into the tiles ([`generate`]).
//! [`source`] has the tile sources: a folder, a cache in memory and on disk,
//! and (feature `http`) a download from any static file hosting.

pub mod astar;
pub mod corridor;
pub mod format;
pub mod generate;
pub mod graph;
pub mod mode;
pub mod pbf;
pub mod router;
pub mod source;
pub mod tile;

#[cfg(test)]
mod tests;

pub use astar::astar;
pub use generate::generate;
pub use corridor::corridor;
pub use graph::Graph;
pub use mode::Mode;
pub use router::{fetch_parallel, route, Progress, Route, RouteError, Stage, TileSource};
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
