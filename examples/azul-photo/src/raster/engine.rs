//! The raster engine: every edit of a document, its composite and its undo
//! history behind one trait.
//!
//! [`RasterEngine`] is the seam the app talks to: an [`Op`] per command, a
//! brush stroke as begin / points / end, `take_dirty` for the rect the
//! canvas must redraw, undo / redo / jump for the History panel. Graphite (or
//! a GPU engine) can implement it later; [`TileEngine`] is the CPU tile store.

use std::{fmt, sync::Arc};

use azul_appkit::UndoHistory;

use super::{
    adjust::Adjustment,
    blend::{self, BlendMode},
    brush::{BrushSettings, Stroke, StrokePoint},
    document::{Composite, Document},
    filter,
    geom::IRect,
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

/// The History panel keeps this many steps.
pub const HISTORY_LIMIT: usize = 60;

/// The CPU tile store: the document, its composite, the History, the stroke
/// in progress.
pub struct TileEngine {
    doc: Document,
    active: Option<LayerId>,
    composite: Composite,
    /// The documents before each recorded edit (appkit's shared undo stack;
    /// a document clone shares its tiles).
    history: UndoHistory<Document>,
    stroke: Option<Stroke>,
    /// The document before the stroke in progress: the stroke's undo step.
    stroke_before: Option<Document>,
    /// Composited by the engine itself (an export, a merged sample) but not
    /// handed to the canvas yet.
    pending: IRect,
}

/// The topmost layer of the document.
fn top_layer(layers: &[Layer]) -> Option<LayerId> {
    layers.last().map(|l| l.id)
}

/// Apply `f` to every pixel layer of the tree (canvas-wide edits).
fn map_rasters(layers: &mut [Layer], f: &dyn Fn(&TileGrid) -> TileGrid) {
    for l in layers {
        match &mut l.content {
            LayerContent::Raster(g) => *g = f(g),
            LayerContent::Group(children) => map_rasters(children, f),
            LayerContent::Adjustment(_) => {}
        }
    }
}

/// New ids for a layer and its subtree (a duplicate).
fn fresh_ids(l: &mut Layer, next: &mut LayerId) {
    l.id = *next;
    *next += 1;
    if let LayerContent::Group(children) = &mut l.content {
        for c in children {
            fresh_ids(c, next);
        }
    }
}

/// The tiles in which two grids differ (by tile identity).
fn changed_tiles(a: &TileGrid, b: &TileGrid) -> Option<IRect> {
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return None;
    }
    let mut out = IRect::default();
    for ty in 0..a.rows() {
        for tx in 0..a.cols() {
            let same = match (a.tile(tx, ty), b.tile(tx, ty)) {
                (None, None) => true,
                (Some(x), Some(y)) => Arc::ptr_eq(x, y),
                _ => false,
            };
            if !same {
                out = out.union(&a.tile_rect(tx, ty));
            }
        }
    }
    Some(out)
}

/// What differs between two states of a document: the rect of the changed
/// tiles when only pixels changed, `None` (everything) when the layers or
/// their settings did.
fn changed_between(a: &Document, b: &Document) -> Option<IRect> {
    fn walk(x: &[Layer], y: &[Layer], acc: &mut IRect) -> bool {
        if x.len() != y.len() {
            return false;
        }
        for (l, m) in x.iter().zip(y) {
            if l.id != m.id || l.visible != m.visible || l.opacity != m.opacity || l.blend != m.blend {
                return false;
            }
            match (&l.content, &m.content) {
                (LayerContent::Raster(g), LayerContent::Raster(h)) => match changed_tiles(g, h) {
                    Some(r) => *acc = acc.union(&r),
                    None => return false,
                },
                (LayerContent::Adjustment(p), LayerContent::Adjustment(q)) => {
                    if p != q {
                        return false;
                    }
                }
                (LayerContent::Group(c), LayerContent::Group(d)) => {
                    if !walk(c, d, acc) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
    if (a.width, a.height) != (b.width, b.height) {
        return None;
    }
    let mut acc = IRect::default();
    walk(&a.layers, &b.layers, &mut acc).then_some(acc)
}

/// `color` painted over `grid` with the mask's coverage.
fn paint_mask(grid: &TileGrid, mask: &Mask, color: [u8; 4]) -> TileGrid {
    let mut out = grid.clone();
    let Some(r) = mask.bounds() else {
        return out;
    };
    let mut px = out.read_rect(r);
    for y in 0..r.h {
        for x in 0..r.w {
            let c = mask.coverage((r.x + x) as u32, (r.y + y) as u32);
            if c <= 0.0 {
                continue;
            }
            let i = ((y * r.w + x) * 4) as usize;
            let under = [px[i], px[i + 1], px[i + 2], px[i + 3]];
            px[i..i + 4].copy_from_slice(&blend::paint_over(under, color, c));
        }
    }
    out.write_rect(r, &px);
    out.prune();
    out
}

/// `grid` with the mask's coverage erased.
fn erase_mask(grid: &TileGrid, mask: &Mask) -> TileGrid {
    let mut out = grid.clone();
    let Some(r) = mask.bounds() else {
        return out;
    };
    let mut px = out.read_rect(r);
    for y in 0..r.h {
        for x in 0..r.w {
            let c = mask.coverage((r.x + x) as u32, (r.y + y) as u32);
            let i = ((y * r.w + x) * 4) as usize;
            let a = (f32::from(px[i + 3]) * (1.0 - c)).round() as u8;
            px[i + 3] = a;
            if a == 0 {
                px[i..i + 3].fill(0);
            }
        }
    }
    out.write_rect(r, &px);
    out.prune();
    out
}

impl TileEngine {
    /// An engine over `doc`; its History starts with "Open".
    #[must_use]
    pub fn new(doc: Document) -> Self {
        Self::with_label(doc, "Open")
    }

    /// An engine over `doc` whose first History state is `label`.
    #[must_use]
    pub fn with_label(doc: Document, label: &str) -> Self {
        Self {
            active: top_layer(&doc.layers),
            composite: Composite::new(doc.width, doc.height),
            history: UndoHistory::new(label).with_limit(HISTORY_LIMIT),
            doc,
            stroke: None,
            stroke_before: None,
            pending: IRect::default(),
        }
    }

    /// The active layer, when it takes pixel edits.
    fn raster_target(&self) -> Result<LayerId, EngineError> {
        let id = self.active.ok_or(EngineError::NoActiveLayer)?;
        let l = self.doc.layer(id).ok_or(EngineError::NoSuchLayer)?;
        if l.locked {
            return Err(EngineError::Locked);
        }
        if l.grid().is_none() {
            return Err(EngineError::NotRaster);
        }
        Ok(id)
    }

    /// Replace the active layer's pixels with `f(pixels, selection)`.
    fn edit_raster(&mut self, f: impl FnOnce(&TileGrid, Option<&Mask>) -> TileGrid) -> Result<bool, EngineError> {
        let id = self.raster_target()?;
        let selection = self.doc.selection.clone();
        let grid = layer::find_mut(&mut self.doc.layers, id)
            .and_then(Layer::grid_mut)
            .ok_or(EngineError::NotRaster)?;
        let new = f(grid, selection.as_deref());
        let dirty = changed_tiles(grid, &new);
        *grid = new;
        self.mark(dirty);
        Ok(true)
    }

    /// The composite must be redone in `dirty` (`None`: everywhere).
    fn mark(&mut self, dirty: Option<IRect>) {
        match dirty {
            Some(r) if r.is_empty() => {}
            Some(r) => self.composite.mark(&r),
            None => self.composite.mark_all(),
        }
    }

    /// Where a layer has pixels (`None`: it can touch every pixel).
    fn extent(&self, id: LayerId) -> Option<IRect> {
        let grid = self.doc.layer(id)?.grid()?;
        Some(
            grid.non_empty_tiles()
                .into_iter()
                .fold(IRect::default(), |acc, (tx, ty)| acc.union(&grid.tile_rect(tx, ty))),
        )
    }

    /// The new selection met with the current one in `mode`.
    fn set_selection(&mut self, new: Mask, mode: SelectMode) {
        let result = match (self.doc.selection.as_deref(), mode) {
            (_, SelectMode::Replace) | (None, SelectMode::Add) => new,
            (None, _) => Mask::empty(self.doc.width, self.doc.height),
            (Some(current), mode) => current.combine(&new, mode),
        };
        self.doc.selection = (!result.is_empty()).then(|| Arc::new(result));
    }

    /// Bring the composite up to date without handing the rect away.
    fn settle(&mut self) {
        if let Some(r) = self.composite.update(&self.doc) {
            self.pending = self.pending.union(&r);
        }
    }

    /// Show `doc` (an undo state): composite only what differs.
    fn restore(&mut self, doc: Document) {
        let dirty = changed_between(&self.doc, &doc);
        self.doc = doc;
        self.mark(dirty);
        if self.active.is_none_or(|id| self.doc.layer(id).is_none()) {
            self.active = top_layer(&self.doc.layers);
        }
    }

    /// Run `op`; `Ok(false)` = nothing to record.
    #[allow(clippy::too_many_lines)]
    fn run(&mut self, op: Op) -> Result<bool, EngineError> {
        let (w, h) = (self.doc.width, self.doc.height);
        match op {
            Op::NewLayer { name } => {
                let id = self.doc.mint_id();
                layer::insert_above(&mut self.doc.layers, self.active, Layer::raster(id, name, TileGrid::new(w, h)));
                self.active = Some(id);
                Ok(true)
            }
            Op::NewAdjustment(adjustment) => {
                let id = self.doc.mint_id();
                layer::insert_above(&mut self.doc.layers, self.active, Layer::adjustment(id, adjustment));
                self.active = Some(id);
                self.mark(None);
                Ok(true)
            }
            Op::NewGroup => {
                let id = self.doc.mint_id();
                let name = format!("Group {id}");
                layer::insert_above(&mut self.doc.layers, self.active, Layer::group(id, name, Vec::new()));
                self.active = Some(id);
                Ok(true)
            }
            Op::DeleteLayer(id) => {
                let gone = self.doc.layer(id).ok_or(EngineError::NoSuchLayer)?.ids();
                let dirty = self.extent(id);
                let next = layer::below(&self.doc.layers, id);
                layer::remove(&mut self.doc.layers, id);
                if self.active.is_none_or(|a| gone.contains(&a)) {
                    self.active = next.or_else(|| top_layer(&self.doc.layers));
                }
                self.mark(dirty);
                Ok(true)
            }
            Op::DuplicateLayer(id) => {
                let mut copy = self.doc.layer(id).ok_or(EngineError::NoSuchLayer)?.clone();
                fresh_ids(&mut copy, &mut self.doc.next_id);
                copy.name = format!("{} copy", copy.name);
                let new_id = copy.id;
                let dirty = self.extent(id);
                layer::insert_above(&mut self.doc.layers, Some(id), copy);
                self.active = Some(new_id);
                self.mark(dirty);
                Ok(true)
            }
            Op::MergeDown(id) => {
                let below = layer::below(&self.doc.layers, id).ok_or(EngineError::Invalid)?;
                let top = self.doc.layer(id).ok_or(EngineError::NoSuchLayer)?.clone();
                let bottom = self.doc.layer(below).ok_or(EngineError::NoSuchLayer)?.clone();
                if top.locked || bottom.locked {
                    return Err(EngineError::Locked);
                }
                if matches!(bottom.content, LayerContent::Adjustment(_)) {
                    return Err(EngineError::NotRaster);
                }
                let mut pair = Document::new(w, h);
                let name = bottom.name.clone();
                pair.layers = vec![bottom, top];
                let merged = Layer::raster(below, name, pair.flatten());
                if let Some(slot) = layer::find_mut(&mut self.doc.layers, below) {
                    *slot = merged;
                }
                layer::remove(&mut self.doc.layers, id);
                self.active = Some(below);
                self.mark(None);
                Ok(true)
            }
            Op::MoveLayer { id, to } => {
                if !layer::move_layer(&mut self.doc.layers, id, to) {
                    return Err(EngineError::Invalid);
                }
                self.mark(None);
                Ok(true)
            }
            Op::SetVisible(id, visible) => {
                let dirty = self.extent(id);
                let l = layer::find_mut(&mut self.doc.layers, id).ok_or(EngineError::NoSuchLayer)?;
                if l.visible == visible {
                    return Ok(false);
                }
                l.visible = visible;
                self.mark(dirty);
                Ok(true)
            }
            Op::SetLocked(id, locked) => {
                let l = layer::find_mut(&mut self.doc.layers, id).ok_or(EngineError::NoSuchLayer)?;
                if l.locked == locked {
                    return Ok(false);
                }
                l.locked = locked;
                Ok(true)
            }
            Op::SetOpacity(id, opacity) => {
                let dirty = self.extent(id);
                let l = layer::find_mut(&mut self.doc.layers, id).ok_or(EngineError::NoSuchLayer)?;
                l.opacity = opacity.clamp(0.0, 1.0);
                self.mark(dirty);
                Ok(true)
            }
            Op::SetBlend(id, blend) => {
                let dirty = self.extent(id);
                let l = layer::find_mut(&mut self.doc.layers, id).ok_or(EngineError::NoSuchLayer)?;
                if l.blend == blend {
                    return Ok(false);
                }
                l.blend = blend;
                self.mark(dirty);
                Ok(true)
            }
            Op::Rename(id, name) => {
                let l = layer::find_mut(&mut self.doc.layers, id).ok_or(EngineError::NoSuchLayer)?;
                l.name = name;
                Ok(true)
            }
            Op::SetAdjustment(id, adjustment) => {
                let l = layer::find_mut(&mut self.doc.layers, id).ok_or(EngineError::NoSuchLayer)?;
                match &mut l.content {
                    LayerContent::Adjustment(a) => *a = adjustment,
                    _ => return Err(EngineError::Invalid),
                }
                self.mark(None);
                Ok(true)
            }
            Op::SetExpanded(id, expanded) => {
                let l = layer::find_mut(&mut self.doc.layers, id).ok_or(EngineError::NoSuchLayer)?;
                l.expanded = expanded;
                Ok(false)
            }
            Op::Select(shape, mode) => {
                let mask = Mask::from_shape(w, h, &shape);
                self.set_selection(mask, mode);
                Ok(true)
            }
            Op::SelectMagicWand {
                x,
                y,
                tolerance,
                contiguous,
                mode,
                sample_merged,
            } => {
                let mask = if sample_merged {
                    self.settle();
                    Mask::magic_wand(self.composite.grid(), x, y, tolerance, contiguous)
                } else {
                    let id = self.active.ok_or(EngineError::NoActiveLayer)?;
                    let grid = self.doc.layer(id).and_then(Layer::grid).ok_or(EngineError::NotRaster)?;
                    Mask::magic_wand(grid, x, y, tolerance, contiguous)
                };
                self.set_selection(mask, mode);
                Ok(true)
            }
            Op::SelectAll => {
                self.doc.selection = Some(Arc::new(Mask::full(w, h)));
                Ok(true)
            }
            Op::Deselect => {
                if self.doc.selection.is_none() {
                    return Ok(false);
                }
                self.doc.selection = None;
                Ok(true)
            }
            Op::InvertSelection => {
                let inverted = match self.doc.selection.as_deref() {
                    Some(m) => m.invert(),
                    None => Mask::full(w, h),
                };
                self.doc.selection = (!inverted.is_empty()).then(|| Arc::new(inverted));
                Ok(true)
            }
            Op::Feather(radius) => {
                let Some(m) = self.doc.selection.as_deref() else {
                    return Ok(false);
                };
                let soft = m.feather(radius);
                self.doc.selection = (!soft.is_empty()).then(|| Arc::new(soft));
                Ok(true)
            }
            Op::FloodFill {
                x,
                y,
                color,
                tolerance,
                contiguous,
            } => self.edit_raster(|g, sel| {
                let region = Mask::magic_wand(g, x, y, tolerance, contiguous);
                let region = match sel {
                    Some(s) => region.combine(s, SelectMode::Intersect),
                    None => region,
                };
                paint_mask(g, &region, color)
            }),
            Op::FillSelection(color) => self.edit_raster(|g, sel| match sel {
                None if color[3] == 255 => TileGrid::filled(w, h, color),
                None => paint_mask(g, &Mask::full(w, h), color),
                Some(m) => paint_mask(g, m, color),
            }),
            Op::ClearSelection => self.edit_raster(|g, sel| match sel {
                None => TileGrid::new(w, h),
                Some(m) => erase_mask(g, m),
            }),
            Op::Gradient { from, to, start, end } => self.edit_raster(|g, sel| {
                let full = Mask::full(w, h);
                let mask = sel.unwrap_or(&full);
                let (vx, vy) = (to.0 - from.0, to.1 - from.1);
                let len2 = (vx * vx + vy * vy).max(1e-6);
                let mut out = g.clone();
                let Some(r) = mask.bounds() else {
                    return out;
                };
                let mut px = out.read_rect(r);
                let s = blend::to_f32(start);
                let e = blend::to_f32(end);
                for y in 0..r.h {
                    for x in 0..r.w {
                        let (dx, dy) = ((r.x + x) as f32 + 0.5, (r.y + y) as f32 + 0.5);
                        let c = mask.coverage((r.x + x) as u32, (r.y + y) as u32);
                        if c <= 0.0 {
                            continue;
                        }
                        let t = (((dx - from.0) * vx + (dy - from.1) * vy) / len2).clamp(0.0, 1.0);
                        let color = [0, 1, 2, 3].map(|k| blend::unit_to_u8(s[k] + (e[k] - s[k]) * t));
                        let i = ((y * r.w + x) * 4) as usize;
                        let under = [px[i], px[i + 1], px[i + 2], px[i + 3]];
                        px[i..i + 4].copy_from_slice(&blend::paint_over(under, color, c));
                    }
                }
                out.write_rect(r, &px);
                out
            }),
            Op::DrawShape { shape, color } => self.edit_raster(|g, sel| {
                let mask = Mask::from_shape(w, h, &shape);
                let mask = match sel {
                    Some(s) => mask.combine(s, SelectMode::Intersect),
                    None => mask,
                };
                paint_mask(g, &mask, color)
            }),
            Op::Filter(f) => self.edit_raster(|g, sel| match f {
                Filter::GaussianBlur { sigma } => filter::gaussian_blur(g, sigma, sel),
                Filter::Sharpen { amount, radius } => filter::sharpen(g, amount, radius, sel),
            }),
            Op::Offset { dx, dy } => self.edit_raster(|g, _| transform::offset(g, dx, dy)),
            Op::TransformLayer { m, interp } => {
                self.edit_raster(|g, _| transform::transform_grid(g, &m, interp, w, h))
            }
            Op::FlipLayer { horizontal } => self.edit_raster(|g, _| transform::flip(g, horizontal)),
            Op::FlipCanvas { horizontal } => {
                map_rasters(&mut self.doc.layers, &|g| transform::flip(g, horizontal));
                self.doc.selection = None;
                self.mark(None);
                Ok(true)
            }
            Op::RotateCanvas90 { clockwise } => {
                map_rasters(&mut self.doc.layers, &|g| transform::rotate90(g, clockwise));
                self.doc.width = h;
                self.doc.height = w;
                self.doc.selection = None;
                self.mark(None);
                Ok(true)
            }
            Op::Crop(r) => {
                let r = r.intersect(&self.doc.bounds()).ok_or(EngineError::Empty)?;
                map_rasters(&mut self.doc.layers, &|g| transform::crop(g, r));
                self.doc.width = r.w as u32;
                self.doc.height = r.h as u32;
                self.doc.selection = self
                    .doc
                    .selection
                    .as_deref()
                    .map(|m| m.crop(r))
                    .filter(|m| !m.is_empty())
                    .map(Arc::new);
                self.mark(None);
                Ok(true)
            }
            Op::ResizeImage { width, height, interp } => {
                if width == 0 || height == 0 {
                    return Err(EngineError::Empty);
                }
                map_rasters(&mut self.doc.layers, &|g| transform::resize(g, width, height, interp));
                self.doc.width = width;
                self.doc.height = height;
                self.doc.selection = None;
                self.mark(None);
                Ok(true)
            }
            Op::ResizeCanvas { width, height, x, y } => {
                if width == 0 || height == 0 {
                    return Err(EngineError::Empty);
                }
                map_rasters(&mut self.doc.layers, &|g| transform::place(g, width, height, x, y));
                self.doc.width = width;
                self.doc.height = height;
                self.doc.selection = None;
                self.mark(None);
                Ok(true)
            }
            Op::AddRasterLayer {
                name,
                width,
                height,
                rgba,
                x,
                y,
            } => {
                let at = IRect::new(x, y, width as i32, height as i32);
                let mut grid = TileGrid::new(w, h);
                grid.write_rect(at, &rgba);
                grid.prune();
                let id = self.doc.mint_id();
                layer::insert_above(&mut self.doc.layers, self.active, Layer::raster(id, name, grid));
                self.active = Some(id);
                self.mark(Some(at));
                Ok(true)
            }
        }
    }
}

impl RasterEngine for TileEngine {
    fn document(&self) -> &Document {
        &self.doc
    }

    fn size(&self) -> (u32, u32) {
        (self.doc.width, self.doc.height)
    }

    fn active_layer(&self) -> Option<LayerId> {
        self.active
    }

    fn set_active_layer(&mut self, id: LayerId) -> bool {
        if self.doc.layer(id).is_none() {
            return false;
        }
        self.active = Some(id);
        true
    }

    fn apply(&mut self, op: Op) -> Result<(), EngineError> {
        if self.stroke.is_some() {
            self.end_stroke();
        }
        let label = op.label();
        let key = op.coalesce_key();
        let before = self.doc.clone();
        if self.run(op)? {
            // A run of the same key (a dragged slider) is one step.
            self.history
                .checkpoint_with(&label, key.as_deref(), || before);
        }
        Ok(())
    }

    fn begin_stroke(&mut self, settings: BrushSettings, at: StrokePoint) -> Result<(), EngineError> {
        if self.stroke.is_some() {
            self.end_stroke();
        }
        let id = self.raster_target()?;
        let base = self
            .doc
            .layer(id)
            .and_then(Layer::grid)
            .cloned()
            .ok_or(EngineError::NotRaster)?;
        self.stroke_before = Some(self.doc.clone());
        self.stroke = Some(Stroke::begin(settings, id, base));
        self.stroke_to(at);
        Ok(())
    }

    fn stroke_to(&mut self, p: StrokePoint) {
        let Self {
            doc,
            stroke,
            composite,
            ..
        } = self;
        let Some(stroke) = stroke.as_mut() else {
            return;
        };
        let selection = doc.selection.clone();
        let Some(grid) = layer::find_mut(&mut doc.layers, stroke.layer).and_then(Layer::grid_mut) else {
            return;
        };
        let changed = stroke.add_point(p, grid, selection.as_deref());
        if !changed.is_empty() {
            composite.mark(&changed);
        }
    }

    fn end_stroke(&mut self) {
        let before = self.stroke_before.take();
        if let (Some(stroke), Some(before)) = (self.stroke.take(), before) {
            if !stroke.dirty.is_empty() {
                self.history.checkpoint(stroke.settings.tool.label(), before);
            }
        }
    }

    fn is_stroking(&self) -> bool {
        self.stroke.is_some()
    }

    fn take_dirty(&mut self) -> Option<IRect> {
        let updated = self.composite.update(&self.doc);
        let pending = std::mem::take(&mut self.pending);
        match updated {
            Some(r) => Some(r.union(&pending)),
            None => (!pending.is_empty()).then_some(pending),
        }
    }

    fn composite(&self) -> &TileGrid {
        self.composite.grid()
    }

    fn sample(&self, x: u32, y: u32) -> [u8; 4] {
        self.composite.grid().pixel(x, y)
    }

    fn undo(&mut self) -> bool {
        if self.stroke.is_some() {
            self.end_stroke();
        }
        let mut doc = self.doc.clone();
        if !self.history.undo(&mut doc) {
            return false;
        }
        self.restore(doc);
        true
    }

    fn redo(&mut self) -> bool {
        let mut doc = self.doc.clone();
        if !self.history.redo(&mut doc) {
            return false;
        }
        self.restore(doc);
        true
    }

    fn jump_to(&mut self, index: usize) -> bool {
        if self.stroke.is_some() {
            self.end_stroke();
        }
        let mut doc = self.doc.clone();
        if !self.history.jump(index, &mut doc) {
            return false;
        }
        self.restore(doc);
        true
    }

    fn history(&self) -> (Vec<String>, usize) {
        (self.history.labels(), self.history.current_index())
    }

    fn flatten_rgba(&mut self) -> (u32, u32, Vec<u8>) {
        self.settle();
        let grid = self.composite.grid();
        (grid.width(), grid.height(), grid.to_rgba())
    }

    fn replace_document(&mut self, doc: Document, label: &str) {
        *self = Self::with_label(doc, label);
    }
}
