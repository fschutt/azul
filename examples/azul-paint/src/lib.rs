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
use azul::gl::{GlContextPtr, Texture};
use azul::image::{Brush, ImageRef, RawImage, RawImageData, RawImageFormat};
use azul::vec::{F32VecRef, StringVec, U8VecRef};
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
    /// Compiled GPU metaball program (lazy; persisted via merge). `None` until
    /// first compiled, or if the shader couldn't be built (-> CPU fallback).
    metaball_gpu: Option<MetaballGpu>,
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

/// Serialize committed strokes to a standalone SVG document. Unlike the PNG
/// export (a readback of the rasterized canvas), this writes the *model*: every
/// stroke as vector primitives, so the result scales losslessly. The viewBox is
/// the strokes' bounding box plus pen radius (the model has no canvas size).
/// Brush strokes become round-capped line segments (width from pressure);
/// metaball strokes become one ellipse per dab (pressure -> size, tilt ->
/// elongation + rotation) -- the field-merge between dabs is raster-only and is
/// approximated by the overlapping ellipses.
fn strokes_to_svg(strokes: &[Stroke], metaball_mode: bool) -> String {
    use std::fmt::Write;

    let bg = canvas_bg();
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for s in strokes {
        for p in &s.points {
            let r = BASE_RADIUS * (0.4 + 0.6 * p.pressure.clamp(0.0, 1.0)) + 2.0;
            min_x = min_x.min(p.x - r);
            min_y = min_y.min(p.y - r);
            max_x = max_x.max(p.x + r);
            max_y = max_y.max(p.y + r);
        }
    }
    if min_x > max_x {
        // No points at all: emit a small empty page.
        min_x = 0.0;
        min_y = 0.0;
        max_x = 64.0;
        max_y = 64.0;
    }
    let (w, h) = ((max_x - min_x).max(1.0), (max_y - min_y).max(1.0));

    let mut out = String::with_capacity(strokes.len() * 128 + 256);
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{:.1} {:.1} {:.1} {:.1}\">",
        min_x, min_y, w, h
    );
    let _ = write!(
        out,
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"rgb({},{},{})\" />",
        min_x, min_y, w, h, bg.r, bg.g, bg.b
    );

    for s in strokes {
        let col = if s.is_eraser { bg } else { s.color };
        let rgb = format!("rgb({},{},{})", col.r, col.g, col.b);
        if metaball_mode {
            for p in &s.points {
                let pr = p.pressure.clamp(0.0, 1.0);
                let r = BASE_RADIUS * (0.4 + 0.6 * pr);
                let tilt = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y).sqrt().clamp(0.0, 1.0);
                let (rx, ry) = (r * (1.0 + tilt), (r * (1.0 - 0.5 * tilt)).max(0.2));
                let angle_deg = (p.tilt_y.atan2(p.tilt_x) + p.barrel_roll_rad).to_degrees();
                let _ = write!(
                    out,
                    "<ellipse cx=\"{:.1}\" cy=\"{:.1}\" rx=\"{:.1}\" ry=\"{:.1}\" \
                     transform=\"rotate({:.1} {:.1} {:.1})\" fill=\"{}\" />",
                    p.x, p.y, rx, ry, angle_deg, p.x, p.y, rgb
                );
            }
        } else if s.points.len() == 1 {
            let p = &s.points[0];
            let r = BASE_RADIUS * (0.4 + 0.6 * p.pressure.clamp(0.0, 1.0));
            let _ = write!(
                out,
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" fill=\"{}\" />",
                p.x, p.y, r, rgb
            );
        } else {
            for seg in s.points.windows(2) {
                let (a, b) = (&seg[0], &seg[1]);
                let p_avg = ((a.pressure + b.pressure) * 0.5).clamp(0.0, 1.0);
                let width = 2.0 * BASE_RADIUS * (0.4 + 0.6 * p_avg);
                let _ = write!(
                    out,
                    "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                     stroke=\"{}\" stroke-width=\"{:.1}\" stroke-linecap=\"round\" />",
                    a.x, a.y, b.x, b.y, rgb, width
                );
            }
        }
    }
    out.push_str("</svg>");
    out
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

// ───────── GPU metaballs (custom shader, mirrors the CPU render_metaballs) ──

/// Max balls uploaded to the GPU shader as a uniform array (most-recent N when a
/// drawing exceeds this; the CPU path is exact/unbounded).
const MAX_GPU_BALLS: usize = 128;

// Bodies have no `#version` line -- it is prepended at compile time from
// `get_usable_glsl_version()` so the shader matches the context (desktop 150 vs
// GLES "300 es"); the body is valid in both.
static METABALL_VS_BODY: &str = "
void main() {
    float x = (gl_VertexID >= 2) ? 1.0 : -1.0;
    float y = (gl_VertexID == 1 || gl_VertexID == 3) ? 1.0 : -1.0;
    gl_Position = vec4(x, y, 0.0, 1.0);
}";

static METABALL_FS_BODY: &str = "
precision highp float;
out vec4 oFragColor;
uniform vec2 uRes;
uniform int uCount;
uniform vec4 uBalls[128];   // xy = center (px, top-left), z = radius, w = angle
uniform vec4 uBalls2[128];  // x = eccentricity, yzw = color (0..1)
uniform vec3 uBg;
void main() {
    vec2 p = vec2(gl_FragCoord.x, uRes.y - gl_FragCoord.y);
    float field = 0.0;
    vec3 col = vec3(0.0);
    for (int i = 0; i < 128; i++) {
        if (i >= uCount) break;
        vec2 d = p - uBalls[i].xy;
        float r = uBalls[i].z;
        float ecc = uBalls2[i].x;
        float ax = r * (1.0 + ecc * 1.6);
        float ay = max(r * (1.0 - ecc * 0.5), r * 0.35);
        float ct = cos(uBalls[i].w);
        float st = sin(uBalls[i].w);
        vec2 l = vec2(d.x * ct + d.y * st, -d.x * st + d.y * ct);
        float q = (l.x / ax) * (l.x / ax) + (l.y / ay) * (l.y / ay);
        float c = 1.0 / (q + 0.18);
        field += c;
        col += c * uBalls2[i].yzw;
    }
    float a = clamp((field - 0.85) / 0.30, 0.0, 1.0);
    a = a * a * (3.0 - 2.0 * a);
    vec3 blob = (field > 0.0001) ? (col / field) : uBg;
    oFragColor = vec4(mix(uBg, blob, a), 1.0);
}";

/// Compiled metaball program + uniform locations; cached in `CanvasCache`.
struct MetaballGpu {
    program: u32,
    u_res: i32,
    u_count: i32,
    u_balls: i32,
    u_balls2: i32,
    u_bg: i32,
}

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TRIANGLE_STRIP: u32 = 0x0005;

/// Compile + link the metaball program once (mirrors core's try_compile_program).
fn compile_metaball_gpu(gl: &GlContextPtr) -> Option<MetaballGpu> {
    let ver = gl.get_usable_glsl_version();
    let ver = ver.as_str();
    if ver.is_empty() {
        return None;
    }
    let vs_src = format!("#version {}\n{}", ver, METABALL_VS_BODY);
    let fs_src = format!("#version {}\n{}", ver, METABALL_FS_BODY);
    let vs = gl.create_shader(GL_VERTEX_SHADER);
    gl.shader_source(vs, StringVec::from_item(vs_src.as_str()));
    gl.compile_shader(vs);
    let fs = gl.create_shader(GL_FRAGMENT_SHADER);
    gl.shader_source(fs, StringVec::from_item(fs_src.as_str()));
    gl.compile_shader(fs);
    let program = gl.create_program();
    if program == 0 {
        return None;
    }
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    Some(MetaballGpu {
        program,
        u_res: gl.get_uniform_location(program, "uRes"),
        u_count: gl.get_uniform_location(program, "uCount"),
        u_balls: gl.get_uniform_location(program, "uBalls"),
        u_balls2: gl.get_uniform_location(program, "uBalls2"),
        u_bg: gl.get_uniform_location(program, "uBg"),
    })
}

/// Render the strokes' metaball field into the texture via the shader + an FBO.
fn render_metaballs_gpu(
    mgpu: &MetaballGpu,
    gl: &GlContextPtr,
    texture_id: u32,
    tw: u32,
    th: u32,
    strokes: &[Stroke],
    bg: ColorU,
) {
    let mut balls: Vec<f32> = Vec::new();
    let mut balls2: Vec<f32> = Vec::new();
    for s in strokes {
        let col = if s.is_eraser { bg } else { s.color };
        let (cr, cg, cb) = (col.r as f32 / 255.0, col.g as f32 / 255.0, col.b as f32 / 255.0);
        for p in &s.points {
            let r = (BASE_RADIUS * (0.6 + p.pressure * 2.0)).max(2.0);
            let tilt = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y).sqrt();
            let ecc = (tilt / 60.0).max(0.0).min(0.85);
            let ang = p.tilt_x.atan2(p.tilt_y) + p.barrel_roll_rad;
            balls.extend_from_slice(&[p.x, p.y, r, ang]);
            balls2.extend_from_slice(&[ecc, cr, cg, cb]);
        }
    }
    let mut count = balls.len() / 4;
    if count > MAX_GPU_BALLS {
        let drop = (count - MAX_GPU_BALLS) * 4; // keep the most-recent balls
        balls.drain(0..drop);
        balls2.drain(0..drop);
        count = MAX_GPU_BALLS;
    }

    let fbo = gl.gen_framebuffers(1).get(0).into_option().unwrap_or(0);
    if fbo == 0 || texture_id == 0 || mgpu.program == 0 {
        return;
    }
    gl.bind_framebuffer(GL_FRAMEBUFFER, fbo);
    gl.framebuffer_texture_2d(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, texture_id, 0);
    gl.viewport(0, 0, tw as i32, th as i32);
    gl.use_program(mgpu.program);
    gl.uniform_2fv(mgpu.u_res, F32VecRef::from(&[tw as f32, th as f32][..]));
    gl.uniform_1i(mgpu.u_count, count as i32);
    if count > 0 {
        gl.uniform_4fv(mgpu.u_balls, F32VecRef::from(&balls[..]));
        gl.uniform_4fv(mgpu.u_balls2, F32VecRef::from(&balls2[..]));
    }
    gl.uniform_3fv(
        mgpu.u_bg,
        F32VecRef::from(&[bg.r as f32 / 255.0, bg.g as f32 / 255.0, bg.b as f32 / 255.0][..]),
    );
    gl.draw_arrays(GL_TRIANGLE_STRIP, 0, 4);
    gl.bind_framebuffer(GL_FRAMEBUFFER, 0u32);
    gl.delete_framebuffers((&[fbo][..]).into());
}

/// RenderImageCallback: produce the canvas image, re-rasterizing strokes only
/// when `PaintState.rev` differs from the cache's `rendered_rev`.
extern "C" fn render_canvas(mut data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_physical_size();
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        eprintln!("[paint] render_canvas bounds = {}x{}", size.width, size.height);
    }
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
    let gl_usable = gl.as_ref().map_or(false, |g| g.is_gl_usable());
    // Lazy-compile the GPU metaball shader on first use (when GL is usable).
    if metaball_mode && gl_usable && background.is_none() && cache.metaball_gpu.is_none() {
        if let Some(g) = gl.as_ref() {
            cache.metaball_gpu = compile_metaball_gpu(g);
        }
    }
    // GPU for the brush whenever usable + no imported background; for metaballs
    // only if the shader compiled (else fall through to the CPU metaball path).
    let use_gpu = background.is_none()
        && gl_usable
        && (!metaball_mode || cache.metaball_gpu.is_some());

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
            if metaball_mode {
                // GPU metaballs: render the thresholded field into the texture.
                let tid = cache.texture.as_ref().map(|t| t.texture_id).unwrap_or(0);
                if let Some(mgpu) = cache.metaball_gpu.as_ref() {
                    render_metaballs_gpu(mgpu, &gl, tid, w, h, &strokes, bg);
                }
            } else if let Some(tex) = cache.texture.as_mut() {
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
    // Move the (non-Clone) compiled GPU program out of the old cache; clone the
    // refcounted texture/image handles.
    let (tex, img, rev, mgpu) = match old_data.downcast_mut::<CanvasCache>() {
        Some(mut old) => (
            old.texture.clone(),
            old.cpu_image.clone(),
            old.rendered_rev,
            old.metaball_gpu.take(),
        ),
        None => return new_data,
    };
    if let Some(mut new) = new_data.downcast_mut::<CanvasCache>() {
        new.texture = tex;
        new.cpu_image = img;
        new.rendered_rev = rev;
        new.metaball_gpu = mgpu;
    }
    new_data
}

// ───────── Layout ──────────────────────────────────────────────────────

// NOTE: `display: flex` is REQUIRED for `flex-direction: row` to take effect —
// azul's default display is `block` (taffy_bridge: LayoutDisplay::default ->
// Display::Block), so a div with only `flex-direction: row` lays its children
// out as block boxes (full-width, stacked vertically). The header used to omit
// `display: flex`, which is why the toolbar buttons stacked on top of each other.
const HEADER: &str = "display: flex; background: #2b2b2b; color: white; padding: 12px 20px; \
    flex-direction: row; align-items: center; font-family: sans-serif; font-size: 16px;";
const CANVAS: &str = "flex-grow: 1; position: relative; overflow: hidden;";
const ROOT: &str = "display: flex; flex-direction: column; height: 100%;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (n_strokes, n_undone, metaballs) = data
        .downcast_ref::<PaintState>()
        .map(|s| (s.strokes.len(), s.undone.len(), s.metaball_mode))
        .unwrap_or((0, 0, true));
    let _ = n_undone;

    // Actions (Undo/Redo/Clear/Import/Export/effect toggle) live in the menu bar
    // below — not as inline buttons. The header is just the title + live status.
    let mode_label = if metaballs { "Metaballs" } else { "Brush" };
    let header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_text(
            format!("AzulPaint  ·  {} strokes  ·  Effect: {}", n_strokes, mode_label).as_str(),
        ));

    // The canvas: a single image driven by render_canvas. Its dataset is a
    // CanvasCache that shares the PaintState; the merge callback persists the
    // texture across rebuilds. Pointer callbacks mutate the PaintState.
    let cache = RefAny::new(CanvasCache {
        paint: data.clone(),
        texture: None,
        cpu_image: None,
        metaball_gpu: None,
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
    .with_callback(EventFilter::Hover(HoverEventFilter::TouchEnd), data.clone(), on_pointer_up);

    // Window menu bar. On Windows this resolves to a native HMENU, on macOS to the
    // app menu, and on Linux to the GNOME/DBus global menu (X11) or the CPU-rendered
    // popup fallback (Wayland/KDE — menus aren't render-intensive so software is fine).
    // Sub-menus exercise the popup path; click actions (with_callback) are a follow-up.
    use azul::menu::{Menu, MenuItem, StringMenuItem};
    // Functional menu items: each carries the same callback the old inline
    // toolbar buttons used, so File/Edit/View actually drive the app.
    let action = |label: &str, cb: CallbackType| {
        MenuItem::string(StringMenuItem::create(label).with_callback(data.clone(), cb))
    };
    let menu = Menu::create(vec![
        MenuItem::string(StringMenuItem::create("File").with_children(vec![
            action("Import image…", on_import),
            action("Export PNG…", on_export),
            action("Export SVG…", on_export_svg),
        ])),
        MenuItem::string(StringMenuItem::create("Edit").with_children(vec![
            action("Undo", on_undo),
            action("Redo", on_redo),
            action("Clear", on_clear),
        ])),
        MenuItem::string(StringMenuItem::create("View").with_children(vec![
            action("Toggle effect (Brush / Metaballs)", on_toggle_mode),
        ])),
    ]);

    // Right-click context menu on the canvas: switch the paint effect. This is the
    // runtime test for the context-menu popup path (try_show_context_menu -> show_menu),
    // positioned at the cursor and clamped on-screen.
    let ctx_menu = Menu::create(vec![
        MenuItem::string(
            StringMenuItem::create("Metaballs mode").with_callback(data.clone(), on_set_metaballs),
        ),
        MenuItem::string(
            StringMenuItem::create("Normal paint mode").with_callback(data.clone(), on_set_brush),
        ),
    ]);
    let canvas = canvas.with_context_menu(ctx_menu.clone());

    Dom::create_body()
        .with_css(ROOT)
        .with_menu_bar(menu)
        .with_context_menu(ctx_menu)
        .with_child(header)
        .with_child(canvas)
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
    let pos_opt = info.get_cursor_relative_to_node().into_option();
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        match &pos_opt {
            Some(p) => eprintln!("[paint] cursor_relative_to_node = Some({}, {})", p.x, p.y),
            None => eprintln!("[paint] cursor_relative_to_node = None"),
        }
    }
    let pos = pos_opt?;
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
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        eprintln!("[paint] on_pointer_down FIRED");
    }
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

// Context-menu actions: set the paint effect explicitly (right-click the canvas).
extern "C" fn on_set_metaballs(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.metaball_mode = true,
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

extern "C" fn on_set_brush(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.metaball_mode = false,
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

// Export SVG: serialize the stroke MODEL (vector data) directly — no readback
// of the rasterized canvas needed, so this writes synchronously here instead
// of round-tripping through the render callback like the PNG export.
extern "C" fn on_export_svg(mut data: RefAny, _info: CallbackInfo) -> Update {
    let picked = FileDialog::save_file("Export SVG", OptionString::None);
    let path = match picked.into_option() {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let svg = match data.downcast_ref::<PaintState>() {
        Some(s) => strokes_to_svg(&s.strokes, s.metaball_mode),
        None => return Update::DoNothing,
    };
    let mut path = path.as_str().to_string();
    if !path.to_ascii_lowercase().ends_with(".svg") {
        path.push_str(".svg");
    }
    let _ = std::fs::write(&path, svg);
    Update::DoNothing
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32, pressure: f32) -> StrokePoint {
        StrokePoint { x, y, pressure, tilt_x: 0.0, tilt_y: 0.0, barrel_roll_rad: 0.0 }
    }

    #[test]
    fn svg_export_brush_strokes_as_lines() {
        let strokes = vec![Stroke {
            points: vec![pt(10.0, 20.0, 0.5), pt(40.0, 60.0, 1.0)],
            color: ColorU { r: 200, g: 30, b: 40, a: 255 },
            is_eraser: false,
        }];
        let svg = strokes_to_svg(&strokes, false);
        assert!(svg.starts_with("<svg"), "{svg}");
        assert!(svg.ends_with("</svg>"), "{svg}");
        assert!(svg.contains("<line"), "brush stroke must serialize as line segments: {svg}");
        assert!(svg.contains("rgb(200,30,40)"), "stroke colour must survive: {svg}");
        assert!(svg.contains("stroke-linecap=\"round\""), "{svg}");
        // viewBox must cover the points (plus radius padding).
        assert!(svg.contains("viewBox=\""), "{svg}");
    }

    #[test]
    fn svg_export_metaball_strokes_as_ellipses_and_eraser_uses_bg() {
        let strokes = vec![Stroke {
            points: vec![pt(5.0, 5.0, 0.8)],
            color: ColorU { r: 1, g: 2, b: 3, a: 255 },
            is_eraser: true,
        }];
        let svg = strokes_to_svg(&strokes, true);
        let bg = canvas_bg();
        assert!(svg.contains("<ellipse"), "metaball stroke must serialize as ellipses: {svg}");
        assert!(
            svg.contains(&format!("rgb({},{},{})", bg.r, bg.g, bg.b)),
            "eraser must use the canvas background colour: {svg}"
        );
        assert!(!svg.contains("rgb(1,2,3)"), "eraser must NOT use the stroke colour: {svg}");
    }

    #[test]
    fn svg_export_empty_model_is_valid() {
        let svg = strokes_to_svg(&[], true);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"), "{svg}");
        assert!(!svg.contains("inf") && !svg.contains("NaN"), "{svg}");
    }
}
