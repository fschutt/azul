//! The raster engine: every edit of a document, its composite and its undo
//! history behind one trait.
//!
//! [`RasterEngine`] is the seam the app talks to: an [`Op`] per command, a
//! brush stroke as begin / points / end, `take_dirty` for the rect the
//! canvas must redraw, undo / redo / jump for the History panel. Graphite (or
//! a GPU engine) can implement it later; [`TileEngine`] is the CPU tile store.

use std::{fmt, sync::Arc};

use super::{
    adjust::Adjustment,
    blend::{self, BlendMode},
    brush::{BrushSettings, Stroke, StrokePoint},
    document::{Composite, Document},
    filter,
    geom::IRect,
    history::History,
    layer::{self, Layer, LayerContent, LayerId, Placement},
    selection::{Mask, SelectMode, Shape},
    tile::TileGrid,
    transform::{self, Affine, Interp},
};

/// A destructive filter on the active layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    GaussianBlur { sigma: f32 },
    Sharpen { amount: f32, radius: f32 },
}

impl Filter {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::GaussianBlur { .. } => "Gaussian Blur",
            Self::Sharpen { .. } => "Sharpen",
        }
    }
}

/// Why an edit was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// No layer is active.
    NoActiveLayer,
    /// The layer id is not in the document.
    NoSuchLayer,
    /// The layer is locked.
    Locked,
    /// The edit needs pixels and the layer is an adjustment or a group.
    NotRaster,
    /// The edit would leave nothing (an empty crop).
    Empty,
    /// The edit makes no sense here (a layer into itself, merge without a
    /// layer below).
    Invalid,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoActiveLayer => "Select a layer first.",
            Self::NoSuchLayer => "That layer is gone.",
            Self::Locked => "The layer is locked.",
            Self::NotRaster => "This needs a pixel layer; the active one is an adjustment or a group.",
            Self::Empty => "Nothing would be left.",
            Self::Invalid => "That does not work here.",
        })
    }
}

/// One command on the document. Each recorded one is a History state.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// An empty pixel layer above the active one.
    NewLayer { name: String },
    /// An adjustment layer above the active one.
    NewAdjustment(Adjustment),
    /// An empty group above the active one.
    NewGroup,
    DeleteLayer(LayerId),
    DuplicateLayer(LayerId),
    /// Flatten the layer into the one below it.
    MergeDown(LayerId),
    MoveLayer { id: LayerId, to: Placement },
    SetVisible(LayerId, bool),
    SetLocked(LayerId, bool),
    SetOpacity(LayerId, f32),
    SetBlend(LayerId, BlendMode),
    Rename(LayerId, String),
    SetAdjustment(LayerId, Adjustment),
    /// Open or close a group in the Layers panel (not an undo step).
    SetExpanded(LayerId, bool),
    Select(Shape, SelectMode),
    /// The magic wand on the active layer (or the composite, `sample_merged`).
    SelectMagicWand {
        x: u32,
        y: u32,
        tolerance: u8,
        contiguous: bool,
        mode: SelectMode,
        sample_merged: bool,
    },
    SelectAll,
    Deselect,
    InvertSelection,
    Feather(f32),
    /// The paint bucket: fill the region like the clicked pixel.
    FloodFill {
        x: u32,
        y: u32,
        color: [u8; 4],
        tolerance: u8,
        contiguous: bool,
    },
    /// Fill the selection (the whole layer without one).
    FillSelection([u8; 4]),
    /// Erase the selection (the whole layer without one).
    ClearSelection,
    /// A linear gradient from `from` (`start`) to `to` (`end`), in the selection.
    Gradient {
        from: (f32, f32),
        to: (f32, f32),
        start: [u8; 4],
        end: [u8; 4],
    },
    /// A filled shape on the active layer, in the selection.
    DrawShape { shape: Shape, color: [u8; 4] },
    Filter(Filter),
    /// Move the active layer by whole pixels.
    Offset { dx: i32, dy: i32 },
    /// Map the active layer through an affine transform.
    TransformLayer { m: Affine, interp: Interp },
    FlipLayer { horizontal: bool },
    FlipCanvas { horizontal: bool },
    RotateCanvas90 { clockwise: bool },
    Crop(IRect),
    ResizeImage { width: u32, height: u32, interp: Interp },
    /// A canvas of `width` x `height`, the old one's top-left at (`x`, `y`).
    ResizeCanvas { width: u32, height: u32, x: i32, y: i32 },
    /// A new pixel layer from RGBA8 rows placed at (`x`, `y`) (paste, place).
    AddRasterLayer {
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        x: i32,
        y: i32,
    },
}

impl Op {
    /// The History panel's label.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::NewLayer { .. } => "New Layer".into(),
            Self::NewAdjustment(a) => a.name().into(),
            Self::NewGroup => "New Group".into(),
            Self::DeleteLayer(_) => "Delete Layer".into(),
            Self::DuplicateLayer(_) => "Duplicate Layer".into(),
            Self::MergeDown(_) => "Merge Down".into(),
            Self::MoveLayer { .. } => "Move Layer".into(),
            Self::SetVisible(_, true) => "Show Layer".into(),
            Self::SetVisible(_, false) => "Hide Layer".into(),
            Self::SetLocked(_, true) => "Lock Layer".into(),
            Self::SetLocked(_, false) => "Unlock Layer".into(),
            Self::SetOpacity(..) => "Opacity".into(),
            Self::SetBlend(..) => "Blend Mode".into(),
            Self::Rename(..) => "Rename Layer".into(),
            Self::SetAdjustment(_, a) => format!("Edit {}", a.name()),
            Self::SetExpanded(..) => "Expand Group".into(),
            Self::Select(Shape::Rect(_), _) => "Rectangular Marquee".into(),
            Self::Select(Shape::Ellipse(_), _) => "Elliptical Marquee".into(),
            Self::Select(Shape::Polygon(_), _) => "Lasso".into(),
            Self::SelectMagicWand { .. } => "Magic Wand".into(),
            Self::SelectAll => "Select All".into(),
            Self::Deselect => "Deselect".into(),
            Self::InvertSelection => "Inverse".into(),
            Self::Feather(_) => "Feather".into(),
            Self::FloodFill { .. } => "Paint Bucket".into(),
            Self::FillSelection(_) => "Fill".into(),
            Self::ClearSelection => "Clear".into(),
            Self::Gradient { .. } => "Gradient".into(),
            Self::DrawShape { .. } => "Shape".into(),
            Self::Filter(f) => f.name().into(),
            Self::Offset { .. } => "Move".into(),
            Self::TransformLayer { .. } => "Transform".into(),
            Self::FlipLayer { .. } => "Flip Layer".into(),
            Self::FlipCanvas { .. } => "Flip Canvas".into(),
            Self::RotateCanvas90 { .. } => "Rotate Canvas".into(),
            Self::Crop(_) => "Crop".into(),
            Self::ResizeImage { .. } => "Image Size".into(),
            Self::ResizeCanvas { .. } => "Canvas Size".into(),
            Self::AddRasterLayer { name, .. } => format!("Place {name}"),
        }
    }

    /// Runs of the same key (a dragged slider) are one History state.
    #[must_use]
    pub fn coalesce_key(&self) -> Option<String> {
        match self {
            Self::SetOpacity(id, _) => Some(format!("opacity {id}")),
            Self::SetAdjustment(id, _) => Some(format!("adjust {id}")),
            Self::Rename(id, _) => Some(format!("rename {id}")),
            Self::Feather(_) => Some("feather".into()),
            _ => None,
        }
    }
}

/// The document engine the app drives.
pub trait RasterEngine {
    /// The document as it is now.
    fn document(&self) -> &Document;
    /// Width and height in pixels.
    fn size(&self) -> (u32, u32);
    /// The layer edits go to.
    fn active_layer(&self) -> Option<LayerId>;
    /// Make `id` the active layer (false when it does not exist).
    fn set_active_layer(&mut self, id: LayerId) -> bool;
    /// Run one command (one History state).
    fn apply(&mut self, op: Op) -> Result<(), EngineError>;
    /// Start a brush stroke on the active layer with its first dab at `at`.
    fn begin_stroke(&mut self, settings: BrushSettings, at: StrokePoint) -> Result<(), EngineError>;
    /// Continue the stroke.
    fn stroke_to(&mut self, p: StrokePoint);
    /// Finish the stroke (one History state).
    fn end_stroke(&mut self);
    fn is_stroking(&self) -> bool;
    /// Composite what changed; the document rect to redraw, `None` when
    /// nothing changed since the last call.
    fn take_dirty(&mut self) -> Option<IRect>;
    /// The flattened image as of the last [`Self::take_dirty`].
    fn composite(&self) -> &TileGrid;
    /// One pixel of the composite (the eyedropper, the info line).
    fn sample(&self, x: u32, y: u32) -> [u8; 4];
    fn undo(&mut self) -> bool;
    fn redo(&mut self) -> bool;
    /// Show History state `index`.
    fn jump_to(&mut self, index: usize) -> bool;
    /// The History labels and the current index.
    fn history(&self) -> (Vec<String>, usize);
    /// The flattened image as RGBA8 rows (export).
    fn flatten_rgba(&mut self) -> (u32, u32, Vec<u8>);
    /// Start over with `doc` (open, new): a fresh History.
    fn replace_document(&mut self, doc: Document, label: &str);
}
