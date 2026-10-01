//! The editor's state and what each tool does with the pointer.
//!
//! No azul types: the canvas glue (`canvas.rs`) feeds view coordinates in and
//! takes [`Effects`] out (which view rect to send to the image node, whether
//! the panels need a rebuild). So every tool is tested without a window.

use std::sync::Arc;

use crate::{
    raster::{
        BrushSettings, BrushTool, Document, EngineError, IRect, Mask, Op, RasterEngine, SelectMode,
        Shape, StrokePoint, TileEngine,
    },
    view::{self, Overlays, View, ViewBuffer, ViewColors},
};

/// The tools of the tools column, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Move,
    MarqueeRect,
    MarqueeEllipse,
    Lasso,
    MagicWand,
    Crop,
    Eyedropper,
    Brush,
    Pencil,
    Eraser,
    Bucket,
    Gradient,
    CloneStamp,
    Text,
    Shape,
    Hand,
    Zoom,
}

impl Tool {
    pub const ALL: [Self; 17] = [
        Self::Move,
        Self::MarqueeRect,
        Self::MarqueeEllipse,
        Self::Lasso,
        Self::MagicWand,
        Self::Crop,
        Self::Eyedropper,
        Self::Brush,
        Self::Pencil,
        Self::Eraser,
        Self::Bucket,
        Self::Gradient,
        Self::CloneStamp,
        Self::Text,
        Self::Shape,
        Self::Hand,
        Self::Zoom,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::MarqueeRect => "Rectangular Marquee",
            Self::MarqueeEllipse => "Elliptical Marquee",
            Self::Lasso => "Lasso",
            Self::MagicWand => "Magic Wand",
            Self::Crop => "Crop",
            Self::Eyedropper => "Eyedropper",
            Self::Brush => "Brush",
            Self::Pencil => "Pencil",
            Self::Eraser => "Eraser",
            Self::Bucket => "Paint Bucket",
            Self::Gradient => "Gradient",
            Self::CloneStamp => "Clone Stamp",
            Self::Text => "Text",
            Self::Shape => "Shape",
            Self::Hand => "Hand",
            Self::Zoom => "Zoom",
        }
    }

    /// The Material icon of the tool's button.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Move => "open_with",
            Self::MarqueeRect => "highlight_alt",
            Self::MarqueeEllipse => "radio_button_unchecked",
            Self::Lasso => "gesture",
            Self::MagicWand => "auto_fix_high",
            Self::Crop => "crop",
            Self::Eyedropper => "colorize",
            Self::Brush => "brush",
            Self::Pencil => "edit",
            Self::Eraser => "cleaning_services",
            Self::Bucket => "format_color_fill",
            Self::Gradient => "gradient",
            Self::CloneStamp => "content_copy",
            Self::Text => "text_fields",
            Self::Shape => "category",
            Self::Hand => "pan_tool",
            Self::Zoom => "zoom_in",
        }
    }

    /// The single-key shortcut.
    #[must_use]
    pub const fn key(self) -> char {
        match self {
            Self::Move => 'V',
            Self::MarqueeRect | Self::MarqueeEllipse => 'M',
            Self::Lasso => 'L',
            Self::MagicWand => 'W',
            Self::Crop => 'C',
            Self::Eyedropper => 'I',
            Self::Brush | Self::Pencil => 'B',
            Self::Eraser => 'E',
            Self::Bucket | Self::Gradient => 'G',
            Self::CloneStamp => 'S',
            Self::Text => 'T',
            Self::Shape => 'U',
            Self::Hand => 'H',
            Self::Zoom => 'Z',
        }
    }

    /// The DOM id of the tool's button (scripts click it).
    #[must_use]
    pub const fn dom_id(self) -> &'static str {
        match self {
            Self::Move => "tool-move",
            Self::MarqueeRect => "tool-marquee-rect",
            Self::MarqueeEllipse => "tool-marquee-ellipse",
            Self::Lasso => "tool-lasso",
            Self::MagicWand => "tool-magic-wand",
            Self::Crop => "tool-crop",
            Self::Eyedropper => "tool-eyedropper",
            Self::Brush => "tool-brush",
            Self::Pencil => "tool-pencil",
            Self::Eraser => "tool-eraser",
            Self::Bucket => "tool-bucket",
            Self::Gradient => "tool-gradient",
            Self::CloneStamp => "tool-clone",
            Self::Text => "tool-text",
            Self::Shape => "tool-shape",
            Self::Hand => "tool-hand",
            Self::Zoom => "tool-zoom",
        }
    }

    /// Text is laid out but needs engine work (azul has no text-to-pixels API).
    #[must_use]
    pub const fn enabled(self) -> bool {
        !matches!(self, Self::Text)
    }

    /// The tool with this shortcut, cycling within a shared key.
    #[must_use]
    pub fn for_key(key: char, current: Self) -> Option<Self> {
        let matching: Vec<Self> = Self::ALL.into_iter().filter(|t| t.key() == key).collect();
        if matching.is_empty() {
            return None;
        }
        let next = match matching.iter().position(|t| *t == current) {
            Some(i) => matching[(i + 1) % matching.len()],
            None => matching[0],
        };
        Some(next)
    }
}

/// Why the Text tool is off, shown when it is chosen.
pub const TEXT_TOOL_NOTE: &str =
    "Text needs engine work: azul has no API that rasterises text into pixels yet.";

/// The options bar's values, per tool family.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolOptions {
    pub brush_size: f32,
    pub pencil_size: f32,
    pub eraser_size: f32,
    pub clone_size: f32,
    pub hardness: f32,
    pub opacity: f32,
    pub flow: f32,
    pub spacing: f32,
    pub pressure_size: bool,
    pub pressure_flow: bool,
    pub select_mode: SelectMode,
    pub feather: f32,
    pub wand_tolerance: u8,
    pub wand_contiguous: bool,
    pub sample_merged: bool,
    pub bucket_tolerance: u8,
    pub bucket_contiguous: bool,
    pub shape_ellipse: bool,
    /// Where the clone stamp samples from (Alt-click), document pixels.
    pub clone_source: Option<(f32, f32)>,
    /// The aligned offset from the first clone stroke on.
    pub clone_offset: Option<(f32, f32)>,
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            brush_size: 24.0,
            pencil_size: 2.0,
            eraser_size: 40.0,
            clone_size: 40.0,
            hardness: 0.8,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.15,
            pressure_size: true,
            pressure_flow: false,
            select_mode: SelectMode::Replace,
            feather: 0.0,
            wand_tolerance: 32,
            wand_contiguous: true,
            sample_merged: false,
            bucket_tolerance: 32,
            bucket_contiguous: true,
            shape_ellipse: false,
            clone_source: None,
            clone_offset: None,
        }
    }
}

/// Modifier keys during a pointer event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

/// What the window must do after a state change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// View pixels that changed: send them to the canvas node.
    pub view: Option<IRect>,
    /// The whole view changed (a zoom, a pan, a new document).
    pub view_all: bool,
    /// Panels or menus show something else now: rebuild the DOM.
    pub dom: bool,
    /// The cursor's document position changed (the status bar's label).
    pub cursor: bool,
}

impl Effects {
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            view: match (self.view, other.view) {
                (Some(a), Some(b)) => Some(a.union(&b)),
                (a, b) => a.or(b),
            },
            view_all: self.view_all || other.view_all,
            dom: self.dom || other.dom,
            cursor: self.cursor || other.cursor,
        }
    }

    const fn dom() -> Self {
        Self {
            view: None,
            view_all: false,
            dom: true,
            cursor: false,
        }
    }
}

/// A pointer drag in progress.
#[derive(Clone, Debug, PartialEq)]
enum Drag {
    Stroke,
    Marquee { start: (f32, f32), ellipse: bool },
    Lasso { points: Vec<(f32, f32)> },
    Move { start: (f32, f32), end: (f32, f32) },
    Crop { start: (f32, f32), end: (f32, f32) },
    Gradient { start: (f32, f32), end: (f32, f32) },
    Shape { start: (f32, f32), end: (f32, f32) },
    Pan { at: (f32, f32), pan: (f32, f32) },
}

/// Everything the editor shows except azul's own objects.
pub struct PhotoState {
    /// The raster engine, behind its trait.
    pub engine: Box<dyn RasterEngine>,
    pub name: String,
    /// The document's folder under `photo/`.
    pub uuid: String,
    /// Unsaved changes.
    pub modified: bool,
    pub tool: Tool,
    pub opts: ToolOptions,
    /// Foreground and background colours (straight RGBA).
    pub fg: [u8; 4],
    pub bg: [u8; 4],
    pub swatches: Vec<[u8; 4]>,
    pub view: View,
    /// The view has been fitted to a real size.
    pub view_ready: bool,
    pub buf: ViewBuffer,
    pub colors: ViewColors,
    pub overlays: Overlays,
    pub ants_phase: u32,
    /// The selection and overlays the view shows now.
    shown_selection: Option<Arc<Mask>>,
    shown_overlays: Overlays,
    drag: Option<Drag>,
    /// The document pixel under the pointer.
    pub cursor: Option<(i32, i32)>,
    /// The last message (an error, a hint).
    pub status: String,
}

/// The swatches of a new window.
pub const SWATCHES: [[u8; 4]; 12] = [
    [0, 0, 0, 255],
    [255, 255, 255, 255],
    [128, 128, 128, 255],
    [220, 50, 47, 255],
    [255, 140, 0, 255],
    [255, 214, 10, 255],
    [76, 175, 80, 255],
    [0, 150, 136, 255],
    [33, 150, 243, 255],
    [63, 81, 181, 255],
    [156, 39, 176, 255],
    [121, 85, 72, 255],
];

impl PhotoState {
    /// The editor over `doc` (not shown until the canvas reports its size).
    #[must_use]
    pub fn new(doc: Document, name: &str, uuid: &str) -> Self {
        Self {
            engine: Box::new(TileEngine::new(doc)),
            name: name.to_string(),
            uuid: uuid.to_string(),
            modified: false,
            tool: Tool::Brush,
            opts: ToolOptions::default(),
            fg: [0, 0, 0, 255],
            bg: [255, 255, 255, 255],
            swatches: SWATCHES.to_vec(),
            view: View {
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
                width: 1,
                height: 1,
            },
            view_ready: false,
            buf: ViewBuffer::new(1, 1),
            colors: ViewColors::for_mode(false),
            overlays: Overlays::default(),
            ants_phase: 0,
            shown_selection: None,
            shown_overlays: Overlays::default(),
            drag: None,
            cursor: None,
            status: String::new(),
        }
    }

    /// Start over with `doc` (open, new, the sample): fitted to the view.
    pub fn replace_document(&mut self, doc: Document, name: &str, uuid: &str, label: &str) -> Effects {
        self.engine.replace_document(doc, label);
        self.name = name.to_string();
        self.uuid = uuid.to_string();
        self.modified = false;
        self.drag = None;
        self.overlays = Overlays::default();
        self.opts.clone_source = None;
        self.opts.clone_offset = None;
        self.view_ready = false;
        let (w, h) = (self.view.width, self.view.height);
        let mut e = self.set_view_size(w, h);
        e.dom = true;
        e
    }

    /// The canvas node is `width` x `height` physical pixels: fit the
    /// document the first time, keep zoom and pan after; redraw all.
    pub fn set_view_size(&mut self, width: u32, height: u32) -> Effects {
        let (width, height) = (width.max(1), height.max(1));
        let (dw, dh) = self.engine.size();
        if !self.view_ready && width > 1 && height > 1 {
            self.view = View::fit(dw, dh, width, height);
            self.view_ready = true;
        } else {
            self.view.width = width;
            self.view.height = height;
        }
        self.render_all()
    }

    /// Redraw the whole view.
    pub fn render_all(&mut self) -> Effects {
        let _ = self.engine.take_dirty();
        if (self.buf.width, self.buf.height) != (self.view.width, self.view.height) {
            self.buf = ViewBuffer::new(self.view.width, self.view.height);
        }
        let selection = self.engine.document().selection.clone();
        view::render(
            &mut self.buf,
            &self.view,
            self.engine.composite(),
            selection.as_deref(),
            self.ants_phase,
            &self.overlays,
            self.colors,
            self.view.bounds(),
        );
        self.shown_selection = selection;
        self.shown_overlays = self.overlays.clone();
        Effects {
            view: Some(self.view.bounds()),
            view_all: true,
            dom: false,
            cursor: false,
        }
    }

    /// Redraw what changed since the last redraw: the composited document
    /// rect, a selection that changed, overlays that moved.
    pub fn refresh(&mut self) -> Effects {
        let mut rect: Option<IRect> = None;
        let mut add = |r: Option<IRect>| {
            if let Some(r) = r {
                rect = Some(rect.map_or(r, |old| old.union(&r)));
            }
        };
        if let Some(doc_rect) = self.engine.take_dirty() {
            add(self.view.doc_rect_to_view(&doc_rect));
        }
        let selection = self.engine.document().selection.clone();
        let same = match (&selection, &self.shown_selection) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            for m in [&selection, &self.shown_selection].into_iter().flatten() {
                if let Some(b) = m.bounds() {
                    add(self.view.doc_rect_to_view(&b.inflate(1)));
                }
            }
        }
        if self.overlays != self.shown_overlays {
            add(self.shown_overlays.view_bounds(&self.view));
            add(self.overlays.view_bounds(&self.view));
        }
        let Some(r) = rect else {
            return Effects::default();
        };
        view::render(
            &mut self.buf,
            &self.view,
            self.engine.composite(),
            selection.as_deref(),
            self.ants_phase,
            &self.overlays,
            self.colors,
            r,
        );
        self.shown_selection = selection;
        self.shown_overlays = self.overlays.clone();
        Effects {
            view: Some(r),
            ..Effects::default()
        }
    }

    /// The marching ants moved one step: redraw the selection's edge.
    pub fn tick_ants(&mut self) -> Effects {
        let Some(bounds) = self.engine.document().selection.as_ref().and_then(|m| m.bounds()) else {
            return Effects::default();
        };
        self.ants_phase = self.ants_phase.wrapping_add(1);
        let Some(r) = self.view.doc_rect_to_view(&bounds.inflate(1)) else {
            return Effects::default();
        };
        let selection = self.engine.document().selection.clone();
        view::render(
            &mut self.buf,
            &self.view,
            self.engine.composite(),
            selection.as_deref(),
            self.ants_phase,
            &self.overlays,
            self.colors,
            r,
        );
        Effects {
            view: Some(r),
            ..Effects::default()
        }
    }

    /// Light or dark mode: the view's own colours.
    pub fn set_dark(&mut self, dark: bool) -> Effects {
        let colors = ViewColors::for_mode(dark);
        if colors == self.colors {
            return Effects::default();
        }
        self.colors = colors;
        self.render_all()
    }

    /// Run a command; a refusal becomes the status line.
    pub fn apply(&mut self, op: Op) -> Effects {
        if let Err(e) = self.engine.apply(op) {
            return self.refused(e);
        }
        self.modified = true;
        self.status.clear();
        let mut e = self.refresh();
        e.dom = true;
        e
    }

    fn refused(&mut self, e: EngineError) -> Effects {
        self.status = e.to_string();
        Effects::dom()
    }

    pub fn undo(&mut self) -> Effects {
        if !self.engine.undo() {
            return Effects::default();
        }
        self.modified = true;
        let mut e = self.refresh();
        e.dom = true;
        e
    }

    pub fn redo(&mut self) -> Effects {
        if !self.engine.redo() {
            return Effects::default();
        }
        self.modified = true;
        let mut e = self.refresh();
        e.dom = true;
        e
    }

    /// Show History state `index`.
    pub fn jump(&mut self, index: usize) -> Effects {
        if !self.engine.jump_to(index) {
            return Effects::default();
        }
        let mut e = self.refresh();
        e.dom = true;
        e
    }

    /// Zoom to `zoom` about the view point `about` (the view's centre when
    /// `None`).
    pub fn zoom_to(&mut self, zoom: f32, about: Option<(f32, f32)>) -> Effects {
        let (vx, vy) = about.unwrap_or((self.view.width as f32 / 2.0, self.view.height as f32 / 2.0));
        self.view.zoom_about(zoom, vx, vy);
        let mut e = self.render_all();
        e.dom = true;
        e
    }

    /// One zoom step in (`dir` > 0) or out.
    pub fn zoom_step(&mut self, dir: i32, about: Option<(f32, f32)>) -> Effects {
        let z = self.view.step(dir);
        self.zoom_to(z, about)
    }

    /// The whole document in the view.
    pub fn fit(&mut self) -> Effects {
        let (dw, dh) = self.engine.size();
        self.view = View::fit(dw, dh, self.view.width, self.view.height);
        let mut e = self.render_all();
        e.dom = true;
        e
    }

    /// 100 %, centred.
    pub fn actual_pixels(&mut self) -> Effects {
        let (dw, dh) = self.engine.size();
        self.view.zoom = 1.0;
        self.view.center(dw, dh);
        let mut e = self.render_all();
        e.dom = true;
        e
    }

    pub fn swap_colors(&mut self) -> Effects {
        std::mem::swap(&mut self.fg, &mut self.bg);
        Effects::dom()
    }

    pub fn default_colors(&mut self) -> Effects {
        self.fg = [0, 0, 0, 255];
        self.bg = [255, 255, 255, 255];
        Effects::dom()
    }

    /// Choose a tool (the Text tool explains itself instead).
    pub fn set_tool(&mut self, tool: Tool) -> Effects {
        if !tool.enabled() {
            self.status = TEXT_TOOL_NOTE.to_string();
            return Effects::dom();
        }
        self.tool = tool;
        self.status.clear();
        self.drag = None;
        self.overlays = Overlays::default();
        let mut e = self.refresh();
        e.dom = true;
        e
    }

    /// The current brush-family tool's size.
    #[must_use]
    pub fn brush_size(&self) -> f32 {
        match self.tool {
            Tool::Pencil => self.opts.pencil_size,
            Tool::Eraser => self.opts.eraser_size,
            Tool::CloneStamp => self.opts.clone_size,
            _ => self.opts.brush_size,
        }
    }

    pub fn set_brush_size(&mut self, size: f32) {
        let size = size.clamp(1.0, 2500.0);
        match self.tool {
            Tool::Pencil => self.opts.pencil_size = size,
            Tool::Eraser => self.opts.eraser_size = size,
            Tool::CloneStamp => self.opts.clone_size = size,
            _ => self.opts.brush_size = size,
        }
    }

    /// The `[` / `]` keys: one step smaller or larger.
    pub fn step_brush_size(&mut self, larger: bool) -> Effects {
        let s = self.brush_size();
        let step = if s < 10.0 { 1.0 } else if s < 100.0 { 5.0 } else { 25.0 };
        self.set_brush_size(if larger { s + step } else { s - step });
        Effects::dom()
    }

    /// The stroke settings of the current tool.
    #[must_use]
    pub fn brush_settings(&self) -> BrushSettings {
        let tool = match self.tool {
            Tool::Pencil => BrushTool::Pencil,
            Tool::Eraser => BrushTool::Eraser,
            Tool::CloneStamp => {
                let (dx, dy) = self.opts.clone_offset.unwrap_or((0.0, 0.0));
                BrushTool::Clone { dx, dy }
            }
            _ => BrushTool::Brush,
        };
        BrushSettings {
            tool,
            color: self.fg,
            size: self.brush_size(),
            hardness: if tool == BrushTool::Pencil { 1.0 } else { self.opts.hardness },
            opacity: self.opts.opacity,
            flow: self.opts.flow,
            spacing: self.opts.spacing,
            pressure_size: self.opts.pressure_size,
            pressure_flow: self.opts.pressure_flow,
        }
    }

    /// The selection mode of a drag: Shift adds, Alt subtracts, both
    /// intersect, otherwise the options bar's mode.
    #[must_use]
    pub fn select_mode(&self, mods: Mods) -> SelectMode {
        match (mods.shift, mods.alt) {
            (true, true) => SelectMode::Intersect,
            (true, false) => SelectMode::Add,
            (false, true) => SelectMode::Subtract,
            (false, false) => self.opts.select_mode,
        }
    }

    /// The document pixel under view point (`vx`, `vy`), inside the document.
    #[must_use]
    pub fn doc_pixel(&self, vx: f32, vy: f32) -> Option<(u32, u32)> {
        let (x, y) = self.view.view_to_doc(vx, vy);
        let (w, h) = self.engine.size();
        (x >= 0.0 && y >= 0.0 && x < w as f32 && y < h as f32).then(|| (x as u32, y as u32))
    }

    /// A drag is in progress.
    #[must_use]
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// The ellipse inscribed in the rect from `a` to `b`, as a polyline.
#[must_use]
pub fn ellipse_outline(a: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    let (cx, cy) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (rx, ry) = ((b.0 - a.0).abs() / 2.0, (b.1 - a.1).abs() / 2.0);
    (0..64)
        .map(|i| {
            let t = i as f32 / 64.0 * std::f32::consts::TAU;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

/// The rect from `a` to `b` as a polyline.
#[must_use]
pub fn rect_outline(a: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    vec![(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1)]
}

/// The whole-pixel rect from `a` to `b`; Shift makes it a square.
#[must_use]
pub fn drag_rect(a: (f32, f32), b: (f32, f32), square: bool) -> IRect {
    let mut b = b;
    if square {
        let side = (b.0 - a.0).abs().max((b.1 - a.1).abs());
        b = (a.0 + side * (b.0 - a.0).signum(), a.1 + side * (b.1 - a.1).signum());
    }
    IRect::from_corners(a.0.round() as i32, a.1.round() as i32, b.0.round() as i32, b.1.round() as i32)
}

/// The shape a marquee or shape drag describes.
#[must_use]
pub fn drag_shape(a: (f32, f32), b: (f32, f32), ellipse: bool, square: bool) -> Shape {
    let r = drag_rect(a, b, square);
    if ellipse {
        Shape::Ellipse(r)
    } else {
        Shape::Rect(r)
    }
}
