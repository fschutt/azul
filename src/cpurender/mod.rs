//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to an AzulPixmap using agg-rust.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

mod compositor;
mod pixmap;
mod raster;
mod svg;

pub use compositor::*;
pub use pixmap::*;
pub use raster::*;
pub use svg::*;
