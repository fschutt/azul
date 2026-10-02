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

    /// A digit key (Photoshop's): `1`..`9` = 10 %..90 %, `0` = 100 % - the
    /// paint opacity with a painting tool, the active layer's opacity with
    /// any other tool.
    pub fn digit_opacity(&mut self, digit: u8) -> Effects {
        let value = if digit == 0 { 1.0 } else { f32::from(digit.min(9)) / 10.0 };
        if matches!(self.tool, Tool::Brush | Tool::Pencil | Tool::Eraser | Tool::CloneStamp) {
            self.opts.opacity = value;
            return Effects::dom();
        }
        match self.engine.active_layer() {
            Some(id) => self.apply(Op::SetOpacity(id, value)),
            None => Effects::default(),
        }
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

// ==== The pointer ====

impl PhotoState {
    /// The pointer went down at view point (`vx`, `vy`).
    pub fn pointer_down(&mut self, vx: f32, vy: f32, pressure: f32, mods: Mods) -> Effects {
        let (dx, dy) = self.view.view_to_doc(vx, vy);
        let doc = (dx, dy);
        self.drag = None;
        match self.tool {
            Tool::Brush | Tool::Pencil | Tool::Eraser | Tool::CloneStamp => {
                if self.tool == Tool::CloneStamp {
                    if mods.alt {
                        self.opts.clone_source = Some(doc);
                        self.opts.clone_offset = None;
                        self.status = "Clone source set.".into();
                        return Effects::dom();
                    }
                    let Some(source) = self.opts.clone_source else {
                        self.status = "Alt-click to set where the clone stamp copies from.".into();
                        return Effects::dom();
                    };
                    if self.opts.clone_offset.is_none() {
                        self.opts.clone_offset = Some((source.0 - dx, source.1 - dy));
                    }
                }
                let settings = self.brush_settings();
                let at = StrokePoint {
                    x: dx,
                    y: dy,
                    pressure: pressure.clamp(0.0, 1.0),
                };
                if let Err(e) = self.engine.begin_stroke(settings, at) {
                    return self.refused(e);
                }
                self.modified = true;
                self.drag = Some(Drag::Stroke);
                self.refresh()
            }
            Tool::MarqueeRect | Tool::MarqueeEllipse => {
                self.drag = Some(Drag::Marquee {
                    start: doc,
                    ellipse: self.tool == Tool::MarqueeEllipse,
                });
                Effects::default()
            }
            Tool::Lasso => {
                self.drag = Some(Drag::Lasso { points: vec![doc] });
                Effects::default()
            }
            Tool::MagicWand => match self.doc_pixel(vx, vy) {
                Some((x, y)) => self.apply(Op::SelectMagicWand {
                    x,
                    y,
                    tolerance: self.opts.wand_tolerance,
                    contiguous: self.opts.wand_contiguous,
                    mode: self.select_mode(mods),
                    sample_merged: self.opts.sample_merged,
                }),
                None => Effects::default(),
            },
            Tool::Crop => {
                self.drag = Some(Drag::Crop { start: doc, end: doc });
                Effects::default()
            }
            Tool::Eyedropper => match self.doc_pixel(vx, vy) {
                Some((x, y)) => {
                    let _ = self.engine.take_dirty();
                    let mut c = self.engine.sample(x, y);
                    c[3] = 255;
                    if mods.alt {
                        self.bg = c;
                    } else {
                        self.fg = c;
                    }
                    let mut e = self.refresh();
                    e.dom = true;
                    e
                }
                None => Effects::default(),
            },
            Tool::Bucket => match self.doc_pixel(vx, vy) {
                Some((x, y)) => self.apply(Op::FloodFill {
                    x,
                    y,
                    color: self.fg,
                    tolerance: self.opts.bucket_tolerance,
                    contiguous: self.opts.bucket_contiguous,
                }),
                None => Effects::default(),
            },
            Tool::Gradient => {
                self.drag = Some(Drag::Gradient { start: doc, end: doc });
                Effects::default()
            }
            Tool::Shape => {
                self.drag = Some(Drag::Shape { start: doc, end: doc });
                Effects::default()
            }
            Tool::Move => {
                self.drag = Some(Drag::Move { start: doc, end: doc });
                Effects::default()
            }
            Tool::Hand => {
                self.drag = Some(Drag::Pan {
                    at: (vx, vy),
                    pan: (self.view.pan_x, self.view.pan_y),
                });
                Effects::default()
            }
            Tool::Zoom => self.zoom_step(if mods.alt { -1 } else { 1 }, Some((vx, vy))),
            Tool::Text => {
                self.status = TEXT_TOOL_NOTE.to_string();
                Effects::dom()
            }
        }
    }

    /// The pointer moved to view point (`vx`, `vy`) (pressed or not).
    pub fn pointer_move(&mut self, vx: f32, vy: f32, pressure: f32, mods: Mods) -> Effects {
        let (dx, dy) = self.view.view_to_doc(vx, vy);
        let doc = (dx, dy);
        let cursor = Some((dx.floor() as i32, dy.floor() as i32));
        let cursor_moved = cursor != self.cursor;
        self.cursor = cursor;
        let mut e = match self.drag.as_mut() {
            None => Effects::default(),
            Some(Drag::Stroke) => {
                self.engine.stroke_to(StrokePoint {
                    x: dx,
                    y: dy,
                    pressure: pressure.clamp(0.0, 1.0),
                });
                self.refresh()
            }
            Some(Drag::Marquee { start, ellipse }) => {
                let (start, ellipse) = (*start, *ellipse);
                let r = drag_rect(start, doc, mods.shift);
                let (a, b) = ((r.x as f32, r.y as f32), (r.right() as f32, r.bottom() as f32));
                self.overlays.outline = if ellipse { ellipse_outline(a, b) } else { rect_outline(a, b) };
                self.overlays.closed = true;
                self.refresh()
            }
            Some(Drag::Lasso { points }) => {
                let last = points.last().copied().unwrap_or(doc);
                let far = ((last.0 - doc.0).powi(2) + (last.1 - doc.1).powi(2)).sqrt() * self.view.zoom >= 2.0;
                if far {
                    points.push(doc);
                }
                self.overlays.outline = points.clone();
                self.overlays.closed = false;
                self.refresh()
            }
            Some(Drag::Crop { start, end }) => {
                *end = doc;
                let r = drag_rect(*start, doc, mods.shift);
                self.overlays.crop = Some(r);
                self.refresh()
            }
            Some(Drag::Gradient { start, end }) | Some(Drag::Move { start, end }) => {
                *end = doc;
                self.overlays.guide = Some((*start, doc));
                self.refresh()
            }
            Some(Drag::Shape { start, end }) => {
                *end = doc;
                let r = drag_rect(*start, doc, mods.shift);
                let (a, b) = ((r.x as f32, r.y as f32), (r.right() as f32, r.bottom() as f32));
                self.overlays.outline = if self.opts.shape_ellipse { ellipse_outline(a, b) } else { rect_outline(a, b) };
                self.overlays.closed = true;
                self.refresh()
            }
            Some(Drag::Pan { at, pan }) => {
                self.view.pan_x = (pan.0 + vx - at.0).round();
                self.view.pan_y = (pan.1 + vy - at.1).round();
                self.render_all()
            }
        };
        e.cursor = e.cursor || cursor_moved;
        e
    }

    /// The pointer went up at view point (`vx`, `vy`).
    pub fn pointer_up(&mut self, vx: f32, vy: f32, mods: Mods) -> Effects {
        let (dx, dy) = self.view.view_to_doc(vx, vy);
        let doc = (dx, dy);
        let Some(drag) = self.drag.take() else {
            return Effects::default();
        };
        self.overlays = Overlays::default();
        let cleared = self.refresh();
        let e = match drag {
            Drag::Stroke => {
                self.engine.end_stroke();
                Effects::dom()
            }
            Drag::Marquee { start, ellipse } => {
                let r = drag_rect(start, doc, mods.shift);
                if r.w < 2 && r.h < 2 {
                    // A click without a drag drops the selection.
                    self.apply(Op::Deselect)
                } else {
                    let shape = if ellipse { Shape::Ellipse(r) } else { Shape::Rect(r) };
                    let mut e = self.apply(Op::Select(shape, self.select_mode(mods)));
                    if self.opts.feather > 0.0 {
                        e = e.merge(self.apply(Op::Feather(self.opts.feather)));
                    }
                    e
                }
            }
            Drag::Lasso { mut points } => {
                points.push(doc);
                if points.len() < 3 {
                    self.apply(Op::Deselect)
                } else {
                    let mut e = self.apply(Op::Select(Shape::Polygon(points), self.select_mode(mods)));
                    if self.opts.feather > 0.0 {
                        e = e.merge(self.apply(Op::Feather(self.opts.feather)));
                    }
                    e
                }
            }
            Drag::Crop { start, .. } => {
                let r = drag_rect(start, doc, mods.shift);
                if r.w < 2 || r.h < 2 {
                    Effects::default()
                } else {
                    let e = self.apply(Op::Crop(r));
                    e.merge(self.fit())
                }
            }
            Drag::Gradient { start, .. } => {
                if (start.0 - doc.0).abs() < 0.5 && (start.1 - doc.1).abs() < 0.5 {
                    Effects::default()
                } else {
                    self.apply(Op::Gradient {
                        from: start,
                        to: doc,
                        start: self.fg,
                        end: self.bg,
                    })
                }
            }
            Drag::Shape { start, .. } => {
                let r = drag_rect(start, doc, mods.shift);
                if r.w < 1 || r.h < 1 {
                    Effects::default()
                } else {
                    let shape = if self.opts.shape_ellipse { Shape::Ellipse(r) } else { Shape::Rect(r) };
                    self.apply(Op::DrawShape { shape, color: self.fg })
                }
            }
            Drag::Move { start, .. } => {
                let ox = (doc.0 - start.0).round() as i32;
                let oy = (doc.1 - start.1).round() as i32;
                if ox == 0 && oy == 0 {
                    Effects::default()
                } else {
                    self.apply(Op::Offset { dx: ox, dy: oy })
                }
            }
            Drag::Pan { .. } => Effects::dom(),
        };
        cleared.merge(e)
    }

    /// The pointer left the canvas: finish a stroke, keep other drags.
    pub fn pointer_left(&mut self) -> Effects {
        self.cursor = None;
        if self.drag == Some(Drag::Stroke) {
            self.drag = None;
            self.engine.end_stroke();
            return Effects {
                dom: true,
                cursor: true,
                ..Effects::default()
            };
        }
        Effects {
            cursor: true,
            ..Effects::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{layer, LayerContent};

    const NO: Mods = Mods {
        shift: false,
        alt: false,
        cmd: false,
    };

    /// A 400 x 300 white document shown at 100 % in a 400 x 300 view.
    fn editor() -> PhotoState {
        let mut s = PhotoState::new(Document::with_background(400, 300, [255, 255, 255, 255]), "Test", "u");
        let _ = s.set_view_size(400, 300);
        s.view = View {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            width: 400,
            height: 300,
        };
        let _ = s.render_all();
        s
    }

    fn active_pixel(s: &PhotoState, x: u32, y: u32) -> [u8; 4] {
        let id = s.engine.active_layer().unwrap();
        match &layer::find(&s.engine.document().layers, id).unwrap().content {
            LayerContent::Raster(g) => g.pixel(x, y),
            _ => panic!("not raster"),
        }
    }

    #[test]
    fn a_brush_drag_paints_and_redraws_only_around_the_stroke() {
        let mut s = editor();
        s.fg = [255, 0, 0, 255];
        s.opts.brush_size = 10.0;
        let down = s.pointer_down(50.0, 50.0, 1.0, NO);
        let moved = s.pointer_move(90.0, 50.0, 1.0, NO);
        let up = s.pointer_up(90.0, 50.0, NO);
        assert_eq!(active_pixel(&s, 70, 50), [255, 0, 0, 255]);
        let r = down.merge(moved).view.expect("the stroke redraws its rect");
        assert!(r.w < 400 && r.h < 300, "not the whole view: {r:?}");
        assert!(r.contains(70, 50));
        assert!(up.dom, "the History panel shows the stroke");
        assert_eq!(s.buf.rgba(70, 50), [255, 0, 0, 255], "the view shows it");
        assert_eq!(s.engine.history().0.last().map(String::as_str), Some("Brush"));
    }

    #[test]
    fn a_marquee_drag_selects_and_a_click_deselects() {
        let mut s = editor();
        s.tool = Tool::MarqueeRect;
        s.pointer_down(10.0, 10.0, 1.0, NO);
        s.pointer_move(60.0, 40.0, 1.0, NO);
        assert!(!s.overlays.outline.is_empty(), "the marquee shows while dragging");
        s.pointer_up(60.0, 40.0, NO);
        let m = s.engine.document().selection.clone().expect("a selection");
        assert_eq!(m.bounds(), Some(IRect::new(10, 10, 50, 30)));
        assert!(s.overlays.outline.is_empty());
        // Shift adds a second rect.
        let shift = Mods { shift: true, ..NO };
        s.pointer_down(100.0, 100.0, 1.0, shift);
        s.pointer_up(120.0, 120.0, shift);
        let m = s.engine.document().selection.clone().unwrap();
        assert_eq!(m.bounds(), Some(IRect::new(10, 10, 110, 110)));
        // A click drops it.
        s.pointer_down(5.0, 5.0, 1.0, NO);
        s.pointer_up(5.0, 5.0, NO);
        assert!(s.engine.document().selection.is_none());
    }

    #[test]
    fn the_hand_pans_the_view_and_leaves_the_document_alone() {
        let mut s = editor();
        s.tool = Tool::Hand;
        s.pointer_down(100.0, 100.0, 1.0, NO);
        let e = s.pointer_move(130.0, 90.0, 1.0, NO);
        s.pointer_up(130.0, 90.0, NO);
        assert!(e.view_all);
        assert_eq!((s.view.pan_x, s.view.pan_y), (30.0, -10.0));
        assert_eq!(s.engine.history().0.len(), 1, "no document change");
    }

    #[test]
    fn the_zoom_tool_zooms_in_about_the_click_and_alt_zooms_out() {
        let mut s = editor();
        s.tool = Tool::Zoom;
        let before = s.view.view_to_doc(200.0, 150.0);
        s.pointer_down(200.0, 150.0, 1.0, NO);
        assert_eq!(s.view.zoom, 2.0);
        let after = s.view.view_to_doc(200.0, 150.0);
        assert!((before.0 - after.0).abs() < 1.0 && (before.1 - after.1).abs() < 1.0);
        s.pointer_down(200.0, 150.0, 1.0, Mods { alt: true, ..NO });
        assert_eq!(s.view.zoom, 1.0);
    }

    #[test]
    fn the_eyedropper_picks_the_composite_colour() {
        let mut s = editor();
        s.engine.apply(Op::FillSelection([10, 200, 30, 255])).unwrap();
        s.tool = Tool::Eyedropper;
        s.pointer_down(5.0, 5.0, 1.0, NO);
        assert_eq!(s.fg, [10, 200, 30, 255]);
        s.pointer_down(5.0, 5.0, 1.0, Mods { alt: true, ..NO });
        assert_eq!(s.bg, [10, 200, 30, 255], "Alt picks the background colour");
    }

    #[test]
    fn the_bucket_and_the_wand_act_where_clicked() {
        let mut s = editor();
        s.fg = [0, 0, 255, 255];
        s.tool = Tool::Bucket;
        s.pointer_down(20.0, 20.0, 1.0, NO);
        assert_eq!(active_pixel(&s, 399, 299), [0, 0, 255, 255]);
        s.tool = Tool::MagicWand;
        s.pointer_down(20.0, 20.0, 1.0, NO);
        let m = s.engine.document().selection.clone().unwrap();
        assert_eq!(m.bounds(), Some(IRect::new(0, 0, 400, 300)));
    }

    #[test]
    fn a_crop_drag_crops_and_fits_the_view() {
        let mut s = editor();
        s.tool = Tool::Crop;
        s.pointer_down(100.0, 50.0, 1.0, NO);
        s.pointer_move(300.0, 250.0, 1.0, NO);
        assert!(s.overlays.crop.is_some());
        s.pointer_up(300.0, 250.0, NO);
        assert_eq!(s.engine.size(), (200, 200));
        assert!(s.overlays.crop.is_none());
    }

    #[test]
    fn a_gradient_drag_fills_from_the_foreground_to_the_background_colour() {
        let mut s = editor();
        s.fg = [0, 0, 0, 255];
        s.bg = [255, 255, 255, 255];
        s.tool = Tool::Gradient;
        s.pointer_down(0.0, 10.0, 1.0, NO);
        s.pointer_move(200.0, 10.0, 1.0, NO);
        assert!(s.overlays.guide.is_some());
        s.pointer_up(200.0, 10.0, NO);
        assert!(active_pixel(&s, 0, 10)[0] <= 2, "black at the start");
        assert!(active_pixel(&s, 100, 10)[0].abs_diff(128) <= 2, "grey half way");
    }

    #[test]
    fn the_clone_stamp_wants_a_source_first() {
        let mut s = editor();
        s.tool = Tool::CloneStamp;
        let e = s.pointer_down(50.0, 50.0, 1.0, NO);
        assert!(e.dom && !s.status.is_empty(), "it explains the Alt-click");
        assert!(!s.dragging());
        s.pointer_down(10.0, 10.0, 1.0, Mods { alt: true, ..NO });
        assert_eq!(s.opts.clone_source, Some((10.0, 10.0)));
        s.pointer_down(50.0, 50.0, 1.0, NO);
        assert!(s.dragging());
        assert_eq!(s.opts.clone_offset, Some((-40.0, -40.0)));
        s.pointer_up(50.0, 50.0, NO);
    }

    #[test]
    fn the_text_tool_explains_why_it_is_off() {
        let mut s = editor();
        let e = s.set_tool(Tool::Text);
        assert!(e.dom);
        assert_eq!(s.tool, Tool::Brush, "the tool did not change");
        assert_eq!(s.status, TEXT_TOOL_NOTE);
    }

    #[test]
    fn undo_after_a_stroke_redraws_the_stroke_rect_only() {
        let mut s = editor();
        s.fg = [255, 0, 0, 255];
        s.pointer_down(50.0, 50.0, 1.0, NO);
        s.pointer_up(50.0, 50.0, NO);
        let e = s.undo();
        let r = e.view.expect("the undo redraws");
        assert!(r.w <= 256 + 4 && r.h <= 256 + 4, "one tile's worth: {r:?}");
        assert_eq!(s.buf.rgba(50, 50), [255, 255, 255, 255]);
    }

    #[test]
    fn a_tool_key_cycles_through_the_tools_that_share_it() {
        assert_eq!(Tool::for_key('M', Tool::Brush), Some(Tool::MarqueeRect));
        assert_eq!(Tool::for_key('M', Tool::MarqueeRect), Some(Tool::MarqueeEllipse));
        assert_eq!(Tool::for_key('M', Tool::MarqueeEllipse), Some(Tool::MarqueeRect));
        assert_eq!(Tool::for_key('Q', Tool::Brush), None);
        let ids: std::collections::HashSet<&str> = Tool::ALL.iter().map(|t| t.dom_id()).collect();
        assert_eq!(ids.len(), Tool::ALL.len(), "every tool button has its own id");
    }

    #[test]
    fn the_marching_ants_redraw_only_the_selection() {
        let mut s = editor();
        s.engine.apply(Op::Select(Shape::Rect(IRect::new(10, 10, 20, 20)), SelectMode::Replace)).unwrap();
        let _ = s.refresh();
        let e = s.tick_ants();
        let r = e.view.expect("the ants move");
        assert!(r.w <= 26 && r.h <= 26, "{r:?}");
    }

    #[test]
    fn a_digit_sets_the_layer_opacity_with_the_move_tool_and_the_paint_opacity_with_a_brush() {
        let mut s = editor();
        let bg = s.engine.active_layer().unwrap();
        s.tool = Tool::Move;
        let e = s.digit_opacity(5);
        assert!(e.dom);
        assert_eq!(s.engine.document().layer(bg).unwrap().opacity, 0.5);
        let _ = s.digit_opacity(0);
        assert_eq!(s.engine.document().layer(bg).unwrap().opacity, 1.0, "0 is 100 %");
        s.tool = Tool::Brush;
        let _ = s.digit_opacity(3);
        assert!((s.opts.opacity - 0.3).abs() < 1e-6);
        assert_eq!(s.engine.document().layer(bg).unwrap().opacity, 1.0, "the layer kept its opacity");
    }
}
