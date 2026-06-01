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
use azul::image::{Brush, ImageRef, RawImage, RawImageData, RawImageFormat};
use azul::vec::U8VecRef;
use azul::css::PhysicalSizeU32;
use azul::dialog::FileDialog;
use azul::error::{ResultRawImageDecodeImageError, ResultU8VecEncodeImageError};
use azul::option::OptionFileTypeList;

// ───────── Model (the source of truth) ────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct StrokePoint {
    x: f32,
    y: f32,
    /// `0.0..=1.0`, normalized. Finger touches default to `0.5`.
    pressure: f32,
    /// Pen tilt (left/right, fore/aft). Drives the metaball's elongation +
    /// orientation so a tilted pen paints a directional, stretched blob.
    tilt_x: f32,
    tilt_y: f32,
    /// Pen twist (barrel roll) in radians; rotates the elongated dab/metaball.
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
    /// When true, strokes render as 2D **metaballs** (a scalar field summed from
    /// every dab + thresholded, so nearby blobs merge organically) instead of
    /// alpha-over brush dabs. A separate, binary-only effect from the core brush.
    metaball_mode: bool,
    /// An imported image (PNG/JPEG/...) painted underneath the strokes. Part of
    /// the canvas *input*, like the strokes -- not the derived texture.
    background: Option<RawImage>,
    /// A pending Export request: the render callback drains this, reads the
    /// finished canvas back to RGBA8 (`Texture::copy_to_raw_image` on the GPU,
    /// else the CPU `RawImage`), PNG-encodes it and writes it to this path.
    export_path: Option<String>,
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
            metaball_mode: true,
            background: None,
            export_path: None,
            rev: 1,
        }
    }

    fn toggle_metaballs(&mut self) {
        self.metaball_mode = !self.metaball_mode;
        self.rev += 1;
    }

    fn set_background(&mut self, img: RawImage) {
        self.background = Some(img);
        self.rev += 1;
    }

    fn request_export(&mut self, path: String) {
        self.export_path = Some(path);
        // Bump so the render callback re-runs and drains the request even if the
        // strokes are unchanged.
        self.rev += 1;
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
    /// GPU canvas texture (brush mode, when GL is usable); persisted via merge.
    texture: Option<Texture>,
    /// CPU image (metaball mode, or the GL-unusable brush fallback); persisted
    /// via merge so an idle relayout doesn't re-rasterize.
    cpu_image: Option<ImageRef>,
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

/// Smoothstep (Hermite) used to anti-alias the metaball threshold edge.
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).max(0.0).min(1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Fill an RGBA8 canvas buffer with the base layer: the imported background
/// image (nearest-neighbor scaled to fit), else the solid canvas color.
fn composite_base(buf: &mut [u8], w: u32, h: u32, bg: ColorU, background: Option<&RawImage>) {
    if let Some(img) = background {
        if let RawImageData::U8(ref src) = img.pixels {
            let bgr = matches!(img.data_format, RawImageFormat::BGRA8);
            let ok = matches!(img.data_format, RawImageFormat::RGBA8 | RawImageFormat::BGRA8);
            if ok && img.width > 0 && img.height > 0 {
                let src = src.as_ref();
                let (sw, sh) = (img.width, img.height);
                for y in 0..h as usize {
                    let sy = (y * sh) / h as usize;
                    for x in 0..w as usize {
                        let sx = (x * sw) / w as usize;
                        let si = (sy * sw + sx) * 4;
                        let di = (y * w as usize + x) * 4;
                        if si + 3 < src.len() && di + 3 < buf.len() {
                            let (r, g, b) = if bgr {
                                (src[si + 2], src[si + 1], src[si])
                            } else {
                                (src[si], src[si + 1], src[si + 2])
                            };
                            buf[di] = r;
                            buf[di + 1] = g;
                            buf[di + 2] = b;
                            buf[di + 3] = 255;
                        }
                    }
                }
                return;
            }
        }
    }
    for px in buf.chunks_exact_mut(4) {
        px[0] = bg.r;
        px[1] = bg.g;
        px[2] = bg.b;
        px[3] = bg.a;
    }
}

/// CPU brush rasterization into a fresh RGBA8 image over the base layer.
fn render_brush_cpu(strokes: &[Stroke], w: u32, h: u32, bg: ColorU, background: Option<&RawImage>) -> RawImage {
    let mut img = RawImage {
        pixels: RawImageData::U8(vec![0u8; (w as usize) * (h as usize) * 4].into()),
        width: w as usize,
        height: h as usize,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    if let RawImageData::U8(ref mut v) = img.pixels {
        composite_base(v.as_mut(), w, h, bg, background);
    }
    for s in strokes {
        rasterize_stroke(s, bg, |x0, y0, x1, y1, b| img.paint_stroke(x0, y0, x1, y1, b));
    }
    img
}

/// PNG-encode a RawImage and write it to disk (best-effort; logs nothing).
fn export_png(img: &RawImage, path: &str) {
    let encoded = img.encode_png();
    if let ResultU8VecEncodeImageError::Ok(ref bytes) = encoded {
        let _ = std::fs::write(path, bytes.as_ref());
    }
}

/// CPU 2D-metaball renderer (binary-only effect, separate from the core brush).
/// Each stroke point becomes an anisotropic "ball" that reacts to the pen:
/// **pressure -> size**, **tilt -> elongation + orientation**, **barrel-roll ->
/// extra rotation**. A scalar field `Σ 1/((lx/ax)² + (ly/ay)² + ε)` is summed
/// over every ball and thresholded at `1.0`, so nearby blobs grow connecting
/// bridges and merge organically -- the thing alpha-over dabs can't do. Color
/// is field-weighted so overlapping blobs blend. O(Σ per-ball bbox), not
/// O(pixels·balls).
fn render_metaballs(strokes: &[Stroke], w: u32, h: u32, bg: ColorU, background: Option<&RawImage>) -> RawImage {
    let (wu, hu) = (w as usize, h as usize);
    let n = wu.saturating_mul(hu).max(1);
    let mut field = vec![0.0f32; n];
    let mut acc = vec![[0.0f32; 3]; n]; // field-weighted RGB accumulator
    for s in strokes {
        let col = if s.is_eraser { bg } else { s.color };
        let (cr, cg, cb) = (col.r as f32, col.g as f32, col.b as f32);
        for p in &s.points {
            // pressure -> ball size
            let r = (BASE_RADIUS * (0.6 + p.pressure * 2.0)).max(2.0);
            // tilt magnitude -> eccentricity; tilt direction (+ roll) -> angle
            let tilt_mag = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y).sqrt();
            let ecc = (tilt_mag / 60.0).max(0.0).min(0.85);
            let theta = p.tilt_x.atan2(p.tilt_y) + p.barrel_roll_rad;
            let ax = r * (1.0 + ecc * 1.6);
            let ay = (r * (1.0 - ecc * 0.5)).max(r * 0.35);
            let (st, ct) = theta.sin_cos();
            let reach = ax.max(ay) * 2.2;
            let x0 = (p.x - reach).floor().max(0.0) as usize;
            let y0 = (p.y - reach).floor().max(0.0) as usize;
            let x1 = ((p.x + reach).ceil().max(0.0) as usize).min(wu);
            let y1 = ((p.y + reach).ceil().max(0.0) as usize).min(hu);
            for y in y0..y1 {
                for x in x0..x1 {
                    let dx = x as f32 + 0.5 - p.x;
                    let dy = y as f32 + 0.5 - p.y;
                    let lx = dx * ct + dy * st; // into the ball's local frame
                    let ly = -dx * st + dy * ct;
                    let q = (lx / ax) * (lx / ax) + (ly / ay) * (ly / ay);
                    let c = 1.0 / (q + 0.18);
                    let idx = y * wu + x;
                    field[idx] += c;
                    acc[idx][0] += c * cr;
                    acc[idx][1] += c * cg;
                    acc[idx][2] += c * cb;
                }
            }
        }
    }
    let mut buf = vec![0u8; n * 4];
    // Base layer first (imported image or solid bg), then composite the
    // thresholded metaball blobs over it.
    composite_base(&mut buf, w, h, bg, background);
    for i in 0..n {
        let f = field[i];
        let a = smoothstep(0.85, 1.15, f); // AA rim around the 1.0 isosurface
        if a <= 0.0 || f <= 1.0e-4 {
            continue; // keep the base layer
        }
        let (r, g, b) = (acc[i][0] / f, acc[i][1] / f, acc[i][2] / f);
        let o = i * 4;
        buf[o] = (buf[o] as f32 * (1.0 - a) + r * a).round().max(0.0).min(255.0) as u8;
        buf[o + 1] = (buf[o + 1] as f32 * (1.0 - a) + g * a).round().max(0.0).min(255.0) as u8;
        buf[o + 2] = (buf[o + 2] as f32 * (1.0 - a) + b * a).round().max(0.0).min(255.0) as u8;
        buf[o + 3] = 255;
    }
    RawImage {
        pixels: RawImageData::U8(buf.into()),
        width: wu,
        height: hu,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
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

    // Snapshot strokes + rev + mode + imported background + export request.
    let (rev, strokes, metaball_mode, background, export_path) = {
        let paint = cache.paint.downcast_ref::<PaintState>()?;
        let mut all = paint.strokes.clone();
        if let Some(cur) = paint.current.as_ref() {
            all.push(cur.clone());
        }
        (
            paint.rev,
            all,
            paint.metaball_mode,
            paint.background.clone(),
            paint.export_path.clone(),
        )
    };

    let bg = canvas_bg();
    let bg_ref = background.as_ref();

    // GPU is used only for the plain brush with no imported background + a usable
    // GL context. Metaballs and an imported background composite on the CPU.
    let gl = info.get_gl_context().into_option();
    let use_gpu =
        !metaball_mode && background.is_none() && gl.as_ref().map_or(false, |g| g.is_gl_usable());

    if use_gpu {
        let gl = gl.unwrap();
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
        // Export: read the GPU texture back to RGBA8 bytes + PNG-encode it.
        if let Some(path) = export_path.as_ref() {
            if let Some(tex) = cache.texture.as_ref() {
                export_png(&tex.copy_to_raw_image(), path.as_str());
            }
            clear_export(cache);
        }
        return cache.texture.as_ref().map(|t| ImageRef::gl_texture(t.clone()));
    }

    // CPU path: metaballs, or the brush with an imported background / no GL.
    if cache.rendered_rev != rev || cache.cpu_image.is_none() || export_path.is_some() {
        let img = if metaball_mode {
            render_metaballs(&strokes, w, h, bg, bg_ref)
        } else {
            render_brush_cpu(&strokes, w, h, bg, bg_ref)
        };
        if let Some(path) = export_path.as_ref() {
            export_png(&img, path.as_str());
        }
        cache.cpu_image = ImageRef::new_rawimage(img).into_option();
        cache.rendered_rev = rev;
    }
    if export_path.is_some() {
        clear_export(cache);
    }
    cache.cpu_image.clone()
}

/// Drain a pending Export request from the shared PaintState. Best-effort: if
/// the RefAny is momentarily borrowed elsewhere the request survives to the next
/// frame (and just re-writes the same file), which is harmless.
fn clear_export(cache: &mut CanvasCache) {
    if let Some(mut paint) = cache.paint.downcast_mut::<PaintState>() {
        paint.export_path = None;
    }
}

/// Merge callback: carry the GPU texture + rendered_rev from the old canvas
/// node to the new one across DOM rebuilds (so we don't re-allocate/re-paint
/// every relayout). The new node's `paint` (current PaintState) is kept.
extern "C" fn merge_cache(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let (tex, img, rev) = match old_data.downcast_ref::<CanvasCache>() {
        Some(old) => (old.texture.clone(), old.cpu_image.clone(), old.rendered_rev),
        None => return new_data,
    };
    if let Some(mut new) = new_data.downcast_mut::<CanvasCache>() {
        new.texture = tex;
        new.cpu_image = img;
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
    let (n_strokes, n_undone, metaballs) = data
        .downcast_ref::<PaintState>()
        .map(|s| (s.strokes.len(), s.undone.len(), s.metaball_mode))
        .unwrap_or((0, 0, true));
    let _ = n_undone;

    let mode_label = if metaballs { "Effect: Metaballs" } else { "Effect: Brush" };
    let header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_text(format!("AzulPaint · {} strokes", n_strokes).as_str()))
        .with_child(button("Undo", data.clone(), on_undo))
        .with_child(button("Redo", data.clone(), on_redo))
        .with_child(button("Clear", data.clone(), on_clear))
        .with_child(button(mode_label, data.clone(), on_toggle_mode))
        .with_child(button("Import", data.clone(), on_import))
        .with_child(button("Export", data.clone(), on_export));

    // The canvas: a single image driven by render_canvas. Its dataset is a
    // CanvasCache that shares the PaintState; the merge callback persists the
    // texture across rebuilds. Pointer callbacks mutate the PaintState.
    let cache = RefAny::new(CanvasCache {
        paint: data.clone(),
        texture: None,
        cpu_image: None,
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
                    tilt_x: pen.tilt.x_tilt,
                    tilt_y: pen.tilt.y_tilt,
                    barrel_roll_rad: pen.barrel_roll_rad,
                },
                pen.is_eraser,
            ));
        }
    }
    let pos = info.get_cursor_relative_to_node().into_option()?;
    Some((
        StrokePoint {
            x: pos.x,
            y: pos.y,
            pressure: 0.5,
            tilt_x: 0.0,
            tilt_y: 0.0,
            barrel_roll_rad: 0.0,
        },
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

extern "C" fn on_toggle_mode(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.toggle_metaballs(),
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

// Import: pick an image file, decode it (PNG/JPEG/...) and set it as the canvas
// background that strokes/metaballs paint over.
extern "C" fn on_import(mut data: RefAny, _info: CallbackInfo) -> Update {
    let picked = FileDialog::open_file("Import image", OptionString::None, OptionFileTypeList::None);
    let path = match picked.into_option() {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let bytes = match std::fs::read(path.as_str()) {
        Ok(b) => b,
        Err(_) => return Update::DoNothing,
    };
    let decoded = RawImage::decode_image_bytes_any(U8VecRef::from(&bytes[..]));
    let img = match decoded {
        ResultRawImageDecodeImageError::Ok(ref img) => img.clone(),
        _ => return Update::DoNothing,
    };
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.set_background(img),
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

// Export: pick a destination and request a PNG dump of the finished canvas; the
// render callback drains the request (Texture::copy_to_raw_image on GPU, else
// the CPU RawImage) and writes the file.
extern "C" fn on_export(mut data: RefAny, _info: CallbackInfo) -> Update {
    let picked = FileDialog::save_file("Export PNG", OptionString::None);
    let path = match picked.into_option() {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.request_export(path.as_str().to_string()),
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
