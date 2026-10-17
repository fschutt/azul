//! AzPhoto's raster core: the pixels, independent of the UI.
//!
//! A [`Document`] is a tree of layers (raster layers of 256x256 RGBA8 tiles,
//! non-destructive adjustment layers, groups), a selection mask and a size.
//! Tiles are shared (`Arc`) and copied on write, so a document clone costs a
//! pointer per tile: the undo history keeps whole documents and pays only for
//! the tiles an edit touched ("tile snapshots").
//!
//! Every edit goes through the [`RasterEngine`] trait (an [`Op`] or a brush
//! stroke) and marks the tiles it touched dirty; [`RasterEngine::take_dirty`]
//! composites only those tiles (blend modes, opacity, adjustments, groups) on
//! worker threads and answers the document rect that changed, which the canvas
//! turns into ONE dirty-rect update of its image node.
//!
//! [`TileEngine`] is the implementation; Graphite can implement the same trait
//! later (its raster nodes for adjustments and export, this tile store for
//! painting), and the app does not change.
//!
//! Pixels are straight (not premultiplied) RGBA8 in the tiles; compositing
//! runs in premultiplied f32.

pub mod adjust;
pub mod blend;
pub mod brush;
pub mod document;
pub mod engine;
pub mod filter;
pub mod geom;
pub mod layer;
pub mod selection;
pub mod tile;
pub mod transform;

#[cfg(test)]
mod tests;

pub use adjust::Adjustment;
pub use blend::BlendMode;
pub use brush::{BrushSettings, BrushTool, StrokePoint};
pub use document::{Composite, Document};
pub use engine::{EngineError, Filter, Op, RasterEngine, TileEngine};
pub use geom::IRect;
pub use layer::{Layer, LayerContent, LayerId, Placement};
pub use selection::{Mask, SelectMode, Shape};
pub use tile::{Tile, TileGrid, TILE};
pub use transform::{Affine, Interp};
