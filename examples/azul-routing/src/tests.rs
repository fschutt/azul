//! The crate's tests, one file per part: the tile format, the manifest and
//! offline areas, the tile cache, the costing, the router on small made-up
//! road networks, and the generator. The router on real tiles (Valhalla's
//! Monaco) is in `tests/monaco.rs`.

mod format;
mod manifest;
mod offline;
mod costing;
mod router;
mod build;
mod cache;
mod generate;
