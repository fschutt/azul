//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to an AzulPixmap using agg-rust.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

pub mod bands;
mod blend;
mod blit_scale;
mod compositor;
pub use crate::cpu_counters as counters;
pub mod embolden;
pub mod hvif;
mod item_grid;
pub mod pixmap;
mod raster;
pub mod retained;
mod shape;
mod svg;
mod text_halo;
pub mod text_raster;

pub use blend::{blend_pixel, blend_premultiplied, blit_pixmap_blend};
pub use compositor::*;
pub use pixmap::*;
pub use raster::*;
pub use retained::RetainedFrames;
pub use shape::*;
pub use svg::*;
pub use text_halo::{
    begin_cpu_halo_frame, halo_scan_changed_pictures, halos_drawn, release_cpu_halos,
    release_text_halos, text_halo_bounds_in_this_thread, text_halo_slots_in_this_thread,
    text_halo_stats, text_halos, text_halos_in_use, text_halos_of_held_list, text_halos_of_list,
    within_halo_budget, TextHalo, TextHaloStats, TextHalosReleased,
};
pub use text_raster::*;
