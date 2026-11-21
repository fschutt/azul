//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to an AzulPixmap using agg-rust.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

mod blend;
mod blit_scale;
mod compositor;
pub use crate::cpu_counters as counters;
pub mod embolden;
pub mod hvif;
pub mod pixmap;
mod raster;
mod shape;
mod svg;
mod text_halo;
pub mod text_raster;

pub use blend::{blend_pixel, blend_premultiplied, blit_pixmap_blend};
pub use compositor::*;
pub use pixmap::*;
pub use raster::*;
pub use shape::*;
pub use svg::*;
pub use text_halo::{
    halo_scan_changed_pictures, halos_drawn, text_halo_bounds_in_this_thread,
    text_halo_slots_in_this_thread, text_halo_stats, text_halos, within_halo_budget, TextHalo,
    TextHaloStats,
};
pub use text_raster::*;
