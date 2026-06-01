//! AzulPaint — a simple drawing app built on the azul painting API.
//!
//! Architecture (the "dumb widget" / video-widget pattern):
//!   * **App data** (`PaintState`) holds ONLY the source of truth: the list of
//!     strokes + their config (color / eraser), the undo + redo stacks, the
//!     in-flight stroke, and a `rev` counter that bumps on every change.
//!   * The **canvas** is a single `<img>` node whose pixels come from a
//!     `RenderImageCallback`. The GPU `Texture` (or a CPU `RawImage`) is a
//!     *derived cache* living in the node's own dataset (`CanvasCache`); a
//!     **merge callback** carries that cache across DOM rebuilds, and the
//!     render callback re-rasterizes the strokes only when `rev` changed
//!     (reconciling the cached texture against the current strokes/config).
//!   * Pen pressure scales the brush radius; barrel-roll is reserved for later.
//!
//! Undo/redo just move strokes between `strokes` and `undone` and bump `rev`;
//! the texture is recreated from the strokes on the next frame.

use azul::prelude::*;
use azul::callbacks::{CallbackType, DatasetMergeCallbackType, RenderImageCallbackInfo};
use azul::dom::{DatasetMergeCallback, RenderImageCallback};
use azul::gl::Texture;
use azul::image::{ImageRef, RawImage, RawImageData, RawImageFormat};
use azul::misc::Brush;
use azul::vec::U8VecRef;
use azul::css::PhysicalSizeU32;

// ───────── Model (the source of truth) ────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct StrokePoint {
    x: f32,
    y: f32,
    /// `0.0..=1.0`, normalized. Finger touches default to `0.5`.
    pressure: f32,
    /// Barrel roll in radians (reserved; not used by the round brush yet).
    barrel_roll_rad: f32,
}

#[derive(Clone)]
struct Stroke {
    points: Vec<StrokePoint>,
    /// Brush color for this stroke (config travels with the stroke).
    color: ColorU,
    /// Eraser strokes paint the canvas background instead of `color`.
    is_eraser: bool,
}

/// Base brush radius (px) at full pressure.
const BASE_RADIUS: f32 = 6.0;
/// Canvas background — also the eraser color.
fn canvas_bg() -> ColorU {
    ColorU { r: 250, g: 250, b: 246, a: 255 }
}

struct PaintState {
    /// Committed strokes — the source of truth (re-rasterized into the cache).
    strokes: Vec<Stroke>,
    /// Redo stack (strokes popped by undo).
    undone: Vec<Stroke>,
    /// The stroke currently being drawn.
    current: Option<Stroke>,
    /// Current brush color (config).
    color: ColorU,
    /// Bumps on every change so the canvas cache knows to re-rasterize.
    rev: u64,
}

impl PaintState {
    fn new() -> Self {
        Self {
            strokes: Vec::new(),
            undone: Vec::new(),
            current: None,
            color: ColorU { r: 30, g: 30, b: 40, a: 255 },
            rev: 1,
        }
    }

    fn begin_stroke(&mut self, p: StrokePoint, is_eraser: bool) {
        if let Some(active) = self.current.take() {
            if !active.points.is_empty() {
                self.strokes.push(active);
            }
        }
        self.undone.clear(); // a new stroke invalidates the redo stack
        self.current = Some(Stroke { points: vec![p], color: self.color, is_eraser });
        self.rev += 1;
    }

    fn extend_stroke(&mut self, p: StrokePoint) {
        if let Some(s) = self.current.as_mut() {
            s.points.push(p);
            self.rev += 1;
        }
    }

    fn end_stroke(&mut self) {
        if let Some(active) = self.current.take() {
            if !active.points.is_empty() {
                self.strokes.push(active);
            }
        }
        self.rev += 1;
    }

    fn undo(&mut self) {
        if let Some(s) = self.strokes.pop() {
            self.undone.push(s);
            self.rev += 1;
        }
    }

    fn redo(&mut self) {
        if let Some(s) = self.undone.pop() {
            self.strokes.push(s);
            self.rev += 1;
        }
    }

    fn clear_all(&mut self) {
        if !self.strokes.is_empty() {
            // keep the cleared strokes on the redo stack so Clear is undoable
            self.undone.append(&mut self.strokes);
        }
        self.current = None;
        self.rev += 1;
    }
}

// ───────── Canvas cache (derived; reconciled by the merge callback) ─────

/// The canvas node's dataset: a derived GPU/CPU image of `paint`'s strokes.
/// `paint` is a shared clone of the app `PaintState` (so the render callback
/// can reach the strokes — `info.get_ctx()` is NOT the app data). The merge
/// callback carries `texture`/`rendered_rev` across DOM rebuilds.
struct CanvasCache {
    /// Shared handle to the app's PaintState (source of the strokes).
    paint: RefAny,
    /// GPU canvas texture (when GL is usable); persisted via the merge callback.
    texture: Option<Texture>,
    /// `PaintState.rev` the cache was last rasterized at.
    rendered_rev: u64,
}

/// A round brush for a stroke point at the given pressure.
fn brush_for(color: ColorU, pressure: f32) -> Brush {
    let mut b = Brush::new(color, BASE_RADIUS * pressure.max(0.05).min(1.0));
    b.hardness = 0.6;
    b.flow = 0.9;
    b.spacing = 0.2;
    b
}

/// Rasterize a stroke into a target via a `paint_stroke` closure between
/// consecutive points (shared by the GPU + CPU paths).
fn rasterize_stroke<F: FnMut(f32, f32, f32, f32, Brush)>(stroke: &Stroke, bg: ColorU, mut paint: F) {
    let color = if stroke.is_eraser { bg } else { stroke.color };
    if stroke.points.len() == 1 {
        let p = stroke.points[0];
        paint(p.x, p.y, p.x, p.y, brush_for(color, p.pressure));
        return;
    }
    for seg in stroke.points.windows(2) {
        let (a, c) = (seg[0], seg[1]);
        paint(a.x, a.y, c.x, c.y, brush_for(color, (a.pressure + c.pressure) * 0.5));
    }
}

/// RenderImageCallback: produce the canvas image, re-rasterizing strokes only
/// when `PaintState.rev` differs from the cache's `rendered_rev`.
extern "C" fn render_canvas(mut data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_physical_size();
    let (w, h) = (size.width.max(1), size.height.max(1));
    let placeholder = ImageRef::null_image(w as usize, h as usize, RawImageFormat::RGBA8, U8VecRef::from(&[][..]));
    render_canvas_inner(&mut data, &mut info, w, h).unwrap_or(placeholder)
}

fn render_canvas_inner(
    data: &mut RefAny,
    info: &mut RenderImageCallbackInfo,
    w: u32,
    h: u32,
) -> Option<ImageRef> {
    let mut cache = data.downcast_mut::<CanvasCache>()?;
    let cache = &mut *cache;

    // Snapshot the strokes + rev from the shared PaintState.
    let (rev, strokes): (u64, Vec<Stroke>) = {
        let paint = cache.paint.downcast_ref::<PaintState>()?;
        let mut all = paint.strokes.clone();
        if let Some(cur) = paint.current.as_ref() {
            all.push(cur.clone());
        }
        (paint.rev, all)
    };

    let bg = canvas_bg();
    let gl = info.get_gl_context().into_option();

    if let Some(gl) = gl {
        if gl.is_gl_usable() {
            // (Re)allocate the texture if missing or the canvas size changed.
            let need_alloc = match cache.texture.as_ref() {
                Some(t) => t.size.width != w || t.size.height != h,
                None => true,
            };
            if need_alloc {
                let tex = Texture::allocate_rgba8(gl.clone(), PhysicalSizeU32 { width: w, height: h }, bg);
                cache.texture = Some(tex);
                cache.rendered_rev = 0; // force a full re-rasterize
            }
            if cache.rendered_rev != rev {
                if let Some(tex) = cache.texture.as_mut() {
                    tex.clear();
                    for s in &strokes {
                        rasterize_stroke(s, bg, |x0, y0, x1, y1, b| tex.paint_stroke(x0, y0, x1, y1, b));
                    }
                }
                cache.rendered_rev = rev;
            }
            return cache.texture.as_ref().map(|t| ImageRef::gl_texture(t.clone()));
        }
    }

    // CPU fallback: rasterize the strokes into a fresh RawImage each change.
    let mut img = RawImage {
        pixels: RawImageData::U8(vec![0u8; (w as usize) * (h as usize) * 4].into()),
        width: w as usize,
        height: h as usize,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    // fill background
    if let RawImageData::U8(ref mut v) = img.pixels {
        let buf = v.as_mut();
        for px in buf.chunks_exact_mut(4) {
            px[0] = bg.r;
            px[1] = bg.g;
            px[2] = bg.b;
            px[3] = bg.a;
        }
    }
    for s in &strokes {
        rasterize_stroke(s, bg, |x0, y0, x1, y1, b| img.paint_stroke(x0, y0, x1, y1, b));
    }
    cache.rendered_rev = rev;
    ImageRef::new_rawimage(img).into_option()
}

/// Merge callback: carry the GPU texture + rendered_rev from the old canvas
/// node to the new one across DOM rebuilds (so we don't re-allocate/re-paint
/// every relayout). The new node's `paint` (current PaintState) is kept.
extern "C" fn merge_cache(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let (tex, rev) = match old_data.downcast_ref::<CanvasCache>() {
        Some(old) => (old.texture.clone(), old.rendered_rev),
        None => return new_data,
    };
    if let Some(mut new) = new_data.downcast_mut::<CanvasCache>() {
        new.texture = tex;
        new.rendered_rev = rev;
    }
    new_data
}

// ───────── Layout ──────────────────────────────────────────────────────

const HEADER: &str = "background: #2b2b2b; color: white; padding: 12px 20px; \
    flex-direction: row; align-items: center; font-family: sans-serif; font-size: 16px;";
const BTN: &str = "background: #3a3a3a; color: white; padding: 8px 14px; margin-right: 8px; \
    border-radius: 6px; font-size: 14px; cursor: pointer;";
const CANVAS: &str = "flex-grow: 1; position: relative; overflow: hidden;";
const ROOT: &str = "display: flex; flex-direction: column; height: 100%;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (n_strokes, n_undone) = data
        .downcast_ref::<PaintState>()
        .map(|s| (s.strokes.len(), s.undone.len()))
        .unwrap_or((0, 0));

    let header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_text(format!("AzulPaint · {} strokes", n_strokes).as_str()))
        .with_child(button("Undo", data.clone(), on_undo))
        .with_child(button("Redo", data.clone(), on_redo))
        .with_child(button("Clear", data.clone(), on_clear));

    let _ = n_undone;

    // The canvas: a single image driven by render_canvas. Its dataset is a
    // CanvasCache that shares the PaintState; the merge callback persists the
    // texture across rebuilds. Pointer callbacks mutate the PaintState.
    let cache = RefAny::new(CanvasCache {
        paint: data.clone(),
        texture: None,
        rendered_rev: 0,
    });

    let canvas = Dom::create_image(ImageRef::callback(
        RenderImageCallback::create(render_canvas).to_core(),
        cache.clone(),
    ))
    .with_css(CANVAS)
    .with_dataset(OptionRefAny::Some(cache))
    .with_merge_callback(DatasetMergeCallback::from(merge_cache as DatasetMergeCallbackType))
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), data.clone(), on_pointer_down)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseOver), data.clone(), on_pointer_move)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data.clone(), on_pointer_up)
    .with_callback(EventFilter::Hover(HoverEventFilter::TouchStart), data.clone(), on_pointer_down)
    .with_callback(EventFilter::Hover(HoverEventFilter::TouchMove), data.clone(), on_pointer_move)
    .with_callback(EventFilter::Hover(HoverEventFilter::TouchEnd), data, on_pointer_up);

    Dom::create_body().with_css(ROOT).with_child(header).with_child(canvas)
}

fn button(label: &str, data: RefAny, cb: CallbackType) -> Dom {
    Dom::create_div()
        .with_css(BTN)
        .with_child(Dom::create_text(label))
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data, cb)
}

// ───────── Input ────────────────────────────────────────────────────────

fn extract_point(info: &CallbackInfo) -> Option<(StrokePoint, bool)> {
    if let Some(pen) = info.get_pen_state().into_option() {
        if pen.in_contact {
            return Some((
                StrokePoint {
                    x: pen.position.x,
                    y: pen.position.y,
                    pressure: pen.pressure.max(0.05).min(1.0),
                    barrel_roll_rad: pen.barrel_roll_rad,
                },
                pen.is_eraser,
            ));
        }
    }
    let pos = info.get_cursor_relative_to_node().into_option()?;
    Some((
        StrokePoint { x: pos.x, y: pos.y, pressure: 0.5, barrel_roll_rad: 0.0 },
        false,
    ))
}

extern "C" fn on_pointer_down(mut data: RefAny, info: CallbackInfo) -> Update {
    let (point, is_eraser) = match extract_point(&info) {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state.begin_stroke(point, is_eraser);
    Update::RefreshDom
}

extern "C" fn on_pointer_move(mut data: RefAny, info: CallbackInfo) -> Update {
    {
        let read = match data.downcast_ref::<PaintState>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        if read.current.is_none() {
            return Update::DoNothing;
        }
    }
    let (point, _) = match extract_point(&info) {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state.extend_stroke(point);
    Update::RefreshDom
}

extern "C" fn on_pointer_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.end_stroke(),
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

extern "C" fn on_undo(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.undo(),
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

extern "C" fn on_redo(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.redo(),
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

extern "C" fn on_clear(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.clear_all(),
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

/// Start the app. Desktop/iOS: blocks. Android: stashes window options.
pub fn start() {
    let data = RefAny::new(PaintState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn android_ctor() {
    start();
}
