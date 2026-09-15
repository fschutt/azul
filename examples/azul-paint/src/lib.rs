use azul::{
    callbacks::{CallbackType, DatasetMergeCallbackType, RenderImageCallbackInfo},
    css::PhysicalSizeU32,
    dialog::{FileDialog, FileOpenResult, SaveTargetResult},
    dom::{DatasetMergeCallback, RenderImageCallback},
    error::{ResultRawImageDecodeImageError, ResultU8VecEncodeImageError, ResultU8VecFileError},
    file::FileReadBytesResult,
    gl::{GlContextPtr, Texture},
    image::{Brush, ImageRef, RawImage, RawImageData, RawImageFormat},
    option::OptionFileTypeList,
    prelude::*,
    vec::{F32VecRef, StringVec, U8VecRef},
};

#[derive(Debug, Clone, Copy)]
struct StrokePoint {
    x: f32,
    y: f32,
    pressure: f32,
    tilt_x: f32,
    tilt_y: f32,
    barrel_roll_rad: f32,
}

#[derive(Clone)]
struct Stroke {
    points: Vec<StrokePoint>,
    color: ColorU,
    is_eraser: bool,
}

const BASE_RADIUS: f32 = 6.0;

const METABALL_SUPPORT: f32 = 2.0;
const METABALL_ISO: f32 = 0.5;
const METABALL_AA: f32 = 0.05;

fn metaball_kernel(q: f32) -> f32 {
    let s2 = METABALL_SUPPORT * METABALL_SUPPORT;
    if q.is_nan() || q >= s2 {
        return 0.0;
    }
    let t = 1.0 - q / s2;
    t * t * t
}
fn canvas_bg() -> ColorU {
    ColorU {
        r: 250,
        g: 250,
        b: 246,
        a: 255,
    }
}

fn dbg_ms() -> u128 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PenHud {
    rate_hz5: u16,
    tilt_x_deg: i8,
    tilt_y_deg: i8,
    twist_deg: i16,
    is_eraser: bool,
    barrel: bool,
    in_contact: bool,
    device_id: u64,
}

struct PaintState {
    strokes: Vec<Stroke>,
    undone: Vec<Stroke>,
    current: Option<Stroke>,
    color: ColorU,
    hud: Option<PenHud>,
    device_line: Option<String>,
    metaball_mode: bool,
    background: Option<RawImage>,
    export_path: Option<String>,
    rev: u64,
    pressure_marker: String,
    canvas_marker: String,
    last_pressure: f32,
}

impl PaintState {
    fn new() -> Self {
        Self {
            strokes: Vec::new(),
            undone: Vec::new(),
            current: None,
            color: ColorU {
                r: 30,
                g: 30,
                b: 40,
                a: 255,
            },
            hud: None,
            device_line: None,
            metaball_mode: true,
            background: None,
            export_path: None,
            rev: 1,
            pressure_marker: String::new(),
            canvas_marker: String::new(),
            last_pressure: 0.0,
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
        self.rev += 1;
    }

    fn begin_stroke(&mut self, p: StrokePoint, is_eraser: bool) {
        if let Some(active) = self.current.take() {
            if !active.points.is_empty() {
                self.strokes.push(active);
            }
        }
        self.undone.clear();
        self.current = Some(Stroke {
            points: vec![p],
            color: self.color,
            is_eraser,
        });
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
            self.undone.append(&mut self.strokes);
        }
        self.current = None;
        self.rev += 1;
    }
}

struct CanvasCache {
    paint: RefAny,
    texture: Option<Texture>,
    cpu_image: Option<ImageRef>,
    metaball_gpu: Option<MetaballGpu>,
    rendered_rev: u64,
    mb: MetaballField,
}

#[derive(Default)]
struct MetaballField {
    field: Vec<f32>,
    acc: Vec<[f32; 3]>,
    buf: Vec<u8>,
    size: (u32, u32),
    dabs: usize,
    strokes: usize,
    base_sig: (usize, usize),
}

fn brush_for(color: ColorU, pressure: f32) -> Brush {
    let mut b = Brush::new(color, BASE_RADIUS * pressure.max(0.05).min(1.0));
    b.hardness = 0.6;
    b.flow = 0.9;
    b.spacing = 0.2;
    b
}

fn rasterize_stroke<F: FnMut(f32, f32, f32, f32, Brush)>(
    stroke: &Stroke,
    bg: ColorU,
    mut paint: F,
) {
    let color = if stroke.is_eraser { bg } else { stroke.color };
    if stroke.points.len() == 1 {
        let p = stroke.points[0];
        paint(p.x, p.y, p.x, p.y, brush_for(color, p.pressure));
        return;
    }
    for seg in stroke.points.windows(2) {
        let (a, c) = (seg[0], seg[1]);
        paint(
            a.x,
            a.y,
            c.x,
            c.y,
            brush_for(color, (a.pressure + c.pressure) * 0.5),
        );
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).max(0.0).min(1.0);
    t * t * (3.0 - 2.0 * t)
}

fn composite_base(buf: &mut [u8], w: u32, h: u32, bg: ColorU, background: Option<&RawImage>) {
    if let Some(img) = background {
        if let RawImageData::U8(ref src) = img.pixels {
            let bgr = matches!(img.data_format, RawImageFormat::BGRA8);
            let ok = matches!(
                img.data_format,
                RawImageFormat::RGBA8 | RawImageFormat::BGRA8
            );
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

fn render_brush_cpu(
    strokes: &[Stroke],
    w: u32,
    h: u32,
    bg: ColorU,
    background: Option<&RawImage>,
) -> RawImage {
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
        rasterize_stroke(s, bg, |x0, y0, x1, y1, b| {
            img.paint_stroke(x0, y0, x1, y1, b)
        });
    }
    img
}

fn export_png(img: &RawImage, path: &str) {
    let encoded = img.encode_png();
    if let ResultU8VecEncodeImageError::Ok(ref bytes) = encoded {
        let _ = std::fs::write(path, bytes.as_ref());
    }
}

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
                let tilt = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y)
                    .sqrt()
                    .clamp(0.0, 1.0);
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
                    "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" \
                     stroke-width=\"{:.1}\" stroke-linecap=\"round\" />",
                    a.x, a.y, b.x, b.y, rgb, width
                );
            }
        }
    }
    out.push_str("</svg>");
    out
}

fn apply_metaball_dab(
    field: &mut [f32],
    acc: &mut [[f32; 3]],
    wu: usize,
    hu: usize,
    (cr, cg, cb): (f32, f32, f32),
    p: &StrokePoint,
) -> (usize, usize, usize, usize) {
    let r = (BASE_RADIUS * (0.6 + p.pressure * 2.0)).max(2.0);
    let tilt_mag = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y).sqrt();
    let ecc = (tilt_mag / 60.0).max(0.0).min(0.85);
    let theta = p.tilt_y.atan2(p.tilt_x) + p.barrel_roll_rad;
    let ax = r * (1.0 + ecc * 1.6);
    let ay = (r * (1.0 - ecc * 0.5)).max(r * 0.35);
    let (st, ct) = theta.sin_cos();
    let reach = ax.max(ay) * METABALL_SUPPORT;
    let x0 = (p.x - reach).floor().max(0.0) as usize;
    let y0 = (p.y - reach).floor().max(0.0) as usize;
    let x1 = ((p.x + reach).ceil().max(0.0) as usize).min(wu);
    let y1 = ((p.y + reach).ceil().max(0.0) as usize).min(hu);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - p.x;
            let dy = y as f32 + 0.5 - p.y;
            let lx = dx * ct + dy * st;
            let ly = -dx * st + dy * ct;
            let q = (lx / ax) * (lx / ax) + (ly / ay) * (ly / ay);
            let c = metaball_kernel(q);
            if c <= 0.0 {
                continue;
            }
            let idx = y * wu + x;
            field[idx] += c;
            acc[idx][0] += c * cr;
            acc[idx][1] += c * cg;
            acc[idx][2] += c * cb;
        }
    }
    (x0, y0, x1, y1)
}

fn composite_metaball_region(
    buf: &mut [u8],
    field: &[f32],
    acc: &[[f32; 3]],
    w: u32,
    h: u32,
    bg: ColorU,
    background: Option<&RawImage>,
    bx: (usize, usize, usize, usize),
) {
    let (x0, y0, x1, y1) = bx;
    let wu = w as usize;
    let bg_img = background.and_then(|img| {
        if let RawImageData::U8(ref src) = img.pixels {
            let bgr = matches!(img.data_format, RawImageFormat::BGRA8);
            let ok = matches!(
                img.data_format,
                RawImageFormat::RGBA8 | RawImageFormat::BGRA8
            );
            if ok && img.width > 0 && img.height > 0 {
                return Some((src.as_ref(), img.width, img.height, bgr));
            }
        }
        None
    });
    for y in y0..y1 {
        for x in x0..x1 {
            let i = y * wu + x;
            let o = i * 4;
            let (mut pr, mut pg, mut pb) = (bg.r, bg.g, bg.b);
            if let Some((src, sw, sh, bgr)) = bg_img {
                let sy = (y * sh) / h as usize;
                let sx = (x * sw) / w as usize;
                let si = (sy * sw + sx) * 4;
                if si + 3 < src.len() {
                    (pr, pg, pb) = if bgr {
                        (src[si + 2], src[si + 1], src[si])
                    } else {
                        (src[si], src[si + 1], src[si + 2])
                    };
                }
            }
            let f = field[i];
            let a = smoothstep(METABALL_ISO - METABALL_AA, METABALL_ISO + METABALL_AA, f);
            if a > 0.0 {
                let (r, g, b) = (acc[i][0] / f, acc[i][1] / f, acc[i][2] / f);
                buf[o] = (pr as f32 * (1.0 - a) + r * a).round().max(0.0).min(255.0) as u8;
                buf[o + 1] = (pg as f32 * (1.0 - a) + g * a).round().max(0.0).min(255.0) as u8;
                buf[o + 2] = (pb as f32 * (1.0 - a) + b * a).round().max(0.0).min(255.0) as u8;
            } else {
                buf[o] = pr;
                buf[o + 1] = pg;
                buf[o + 2] = pb;
            }
            buf[o + 3] = 255;
        }
    }
}

fn metaball_image(
    mb: &mut MetaballField,
    strokes: &[Stroke],
    w: u32,
    h: u32,
    bg: ColorU,
    background: Option<&RawImage>,
) -> RawImage {
    let (wu, hu) = (w as usize, h as usize);
    let n = wu.saturating_mul(hu).max(1);
    let total_dabs: usize = strokes.iter().map(|s| s.points.len()).sum();
    let base_sig = background.map_or((0, 0), |img| match img.pixels {
        RawImageData::U8(ref v) => (v.as_ref().as_ptr() as usize, v.as_ref().len()),
        _ => (1, 0),
    });

    let rebuild = mb.size != (w, h)
        || mb.buf.len() != n * 4
        || total_dabs < mb.dabs
        || strokes.len() < mb.strokes
        || mb.base_sig != base_sig;
    if rebuild {
        mb.field = vec![0.0f32; n];
        mb.acc = vec![[0.0f32; 3]; n];
        mb.buf = vec![0u8; n * 4];
        mb.size = (w, h);
        mb.dabs = 0;
        mb.strokes = 0;
        mb.base_sig = base_sig;
    }

    let mut skip = mb.dabs;
    let mut boxes: Vec<(usize, usize, usize, usize)> = Vec::new();
    for st in strokes {
        if skip >= st.points.len() {
            skip -= st.points.len();
            continue;
        }
        let col = if st.is_eraser { bg } else { st.color };
        let col = (col.r as f32, col.g as f32, col.b as f32);
        for p in &st.points[skip..] {
            boxes.push(apply_metaball_dab(
                &mut mb.field,
                &mut mb.acc,
                wu,
                hu,
                col,
                p,
            ));
        }
        skip = 0;
    }

    if rebuild {
        composite_metaball_region(
            &mut mb.buf,
            &mb.field,
            &mb.acc,
            w,
            h,
            bg,
            background,
            (0, 0, wu, hu),
        );
    } else {
        for bx in boxes {
            composite_metaball_region(&mut mb.buf, &mb.field, &mb.acc, w, h, bg, background, bx);
        }
    }
    mb.dabs = total_dabs;
    mb.strokes = strokes.len();

    RawImage {
        pixels: RawImageData::U8(mb.buf.clone().into()),
        width: wu,
        height: hu,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    }
}

const MAX_GPU_BALLS: usize = 128;

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
        // Wyvill kernel: compact support, exactly zero at q = S^2.
        // Same constants as metaball_kernel() / METABALL_* on the CPU path.
        float t = max(0.0, 1.0 - q / 4.0);
        float c = t * t * t;
        field += c;
        col += c * uBalls2[i].yzw;
    }
    float a = clamp((field - 0.45) / 0.10, 0.0, 1.0);
    a = a * a * (3.0 - 2.0 * a);
    vec3 blob = (field > 0.0001) ? (col / field) : uBg;
    oFragColor = vec4(mix(uBg, blob, a), 1.0);
}";

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
        let (cr, cg, cb) = (
            col.r as f32 / 255.0,
            col.g as f32 / 255.0,
            col.b as f32 / 255.0,
        );
        for p in &s.points {
            let r = (BASE_RADIUS * (0.6 + p.pressure * 2.0)).max(2.0);
            let tilt = (p.tilt_x * p.tilt_x + p.tilt_y * p.tilt_y).sqrt();
            let ecc = (tilt / 60.0).max(0.0).min(0.85);
            let ang = p.tilt_y.atan2(p.tilt_x) + p.barrel_roll_rad;
            balls.extend_from_slice(&[p.x, p.y, r, ang]);
            balls2.extend_from_slice(&[ecc, cr, cg, cb]);
        }
    }
    let mut count = balls.len() / 4;
    if count > MAX_GPU_BALLS {
        let drop = (count - MAX_GPU_BALLS) * 4;
        balls.drain(0..drop);
        balls2.drain(0..drop);
        count = MAX_GPU_BALLS;
    }

    let fbo = gl.gen_framebuffers(1).get(0).into_option().unwrap_or(0);
    if fbo == 0 || texture_id == 0 || mgpu.program == 0 {
        return;
    }
    gl.bind_framebuffer(GL_FRAMEBUFFER, fbo);
    gl.framebuffer_texture_2d(
        GL_FRAMEBUFFER,
        GL_COLOR_ATTACHMENT0,
        GL_TEXTURE_2D,
        texture_id,
        0,
    );
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
        F32VecRef::from(
            &[
                bg.r as f32 / 255.0,
                bg.g as f32 / 255.0,
                bg.b as f32 / 255.0,
            ][..],
        ),
    );
    gl.draw_arrays(GL_TRIANGLE_STRIP, 0, 4);
    gl.bind_framebuffer(GL_FRAMEBUFFER, 0u32);
    gl.delete_framebuffers((&[fbo][..]).into());
}

extern "C" fn render_canvas(mut data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_logical_size();
    let (w, h) = (size.width.max(1.0) as u32, size.height.max(1.0) as u32);
    let dbg = std::env::var("AZ_PAINT_DEBUG").is_ok();
    let t0 = dbg.then(std::time::Instant::now);
    let rev_before = dbg
        .then(|| {
            data.downcast_ref::<CanvasCache>()
                .map(|c| c.rendered_rev)
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let placeholder = ImageRef::null_image(
        w as usize,
        h as usize,
        RawImageFormat::RGBA8,
        U8VecRef::from(&[][..]),
    );
    let out = render_canvas_inner(&mut data, &mut info, w, h).unwrap_or(placeholder);
    if let Some(t0) = t0 {
        let rev_after = data
            .downcast_ref::<CanvasCache>()
            .map(|c| c.rendered_rev)
            .unwrap_or(0);
        eprintln!(
            "[paint] t={}ms render_canvas {}x{} took {:?} ({})",
            dbg_ms(),
            w,
            h,
            t0.elapsed(),
            if rev_after != rev_before {
                "RASTER"
            } else {
                "cache-hit"
            },
        );
    }
    out
}

fn render_canvas_inner(
    data: &mut RefAny,
    info: &mut RenderImageCallbackInfo,
    w: u32,
    h: u32,
) -> Option<ImageRef> {
    let mut cache = data.downcast_mut::<CanvasCache>()?;
    let cache = &mut *cache;

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

    let gl = info.get_gl_context().into_option();
    let gl_usable = gl.as_ref().map_or(false, |g| g.is_gl_usable());
    if metaball_mode && gl_usable && background.is_none() && cache.metaball_gpu.is_none() {
        if let Some(g) = gl.as_ref() {
            cache.metaball_gpu = compile_metaball_gpu(g);
        }
    }
    let use_gpu =
        background.is_none() && gl_usable && (!metaball_mode || cache.metaball_gpu.is_some());

    if use_gpu {
        let gl = gl.unwrap();
        let need_alloc = match cache.texture.as_ref() {
            Some(t) => t.size.width != w || t.size.height != h,
            None => true,
        };
        if need_alloc {
            let tex = Texture::allocate_rgba8(
                gl.clone(),
                PhysicalSizeU32 {
                    width: w,
                    height: h,
                },
                bg,
            );
            cache.texture = Some(tex);
            cache.rendered_rev = 0;
        }
        if cache.rendered_rev != rev {
            if metaball_mode {
                let tid = cache.texture.as_ref().map(|t| t.texture_id).unwrap_or(0);
                if let Some(mgpu) = cache.metaball_gpu.as_ref() {
                    render_metaballs_gpu(mgpu, &gl, tid, w, h, &strokes, bg);
                }
            } else if let Some(tex) = cache.texture.as_mut() {
                tex.clear();
                for s in &strokes {
                    rasterize_stroke(s, bg, |x0, y0, x1, y1, b| {
                        tex.paint_stroke(x0, y0, x1, y1, b)
                    });
                }
            }
            cache.rendered_rev = rev;
        }
        if let Some(path) = export_path.as_ref() {
            if let Some(tex) = cache.texture.as_ref() {
                export_png(&tex.copy_to_raw_image(), path.as_str());
            }
            clear_export(cache);
        }
        return cache
            .texture
            .as_ref()
            .map(|t| ImageRef::gl_texture(t.clone()));
    }

    let cached = cache.cpu_image.as_ref().map(|img| {
        let s = img.get_size();
        (s.width as u32, s.height as u32)
    });
    if cpu_canvas_needs_raster(
        cache.rendered_rev,
        rev,
        cached,
        (w, h),
        export_path.is_some(),
    ) {
        let img = if metaball_mode {
            metaball_image(&mut cache.mb, &strokes, w, h, bg, bg_ref)
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

fn cpu_canvas_needs_raster(
    rendered_rev: u64,
    rev: u64,
    cached: Option<(u32, u32)>,
    target: (u32, u32),
    exporting: bool,
) -> bool {
    rendered_rev != rev || cached != Some(target) || exporting
}

fn clear_export(cache: &mut CanvasCache) {
    if let Some(mut paint) = cache.paint.downcast_mut::<PaintState>() {
        paint.export_path = None;
    }
}

extern "C" fn merge_cache(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let (tex, img, rev, mgpu, mb) = match old_data.downcast_mut::<CanvasCache>() {
        Some(mut old) => (
            old.texture.clone(),
            old.cpu_image.clone(),
            old.rendered_rev,
            old.metaball_gpu.take(),
            core::mem::take(&mut old.mb),
        ),
        None => return new_data,
    };
    if let Some(mut new) = new_data.downcast_mut::<CanvasCache>() {
        new.texture = tex;
        new.cpu_image = img;
        new.rendered_rev = rev;
        new.metaball_gpu = mgpu;
        new.mb = mb;
    }
    new_data
}

const HEADER: &str = "display: flex; background: #2b2b2b; color: white; padding: 12px 20px; \
                      flex-direction: row; align-items: center; font-family: sans-serif; \
                      font-size: 16px; user-select: none;";
const CANVAS: &str = "flex-grow: 1; position: relative; overflow: hidden;";
const ROOT: &str = "display: flex; flex-direction: column; height: 100%;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (pressure_marker, canvas_marker) = match data.downcast_mut::<PaintState>() {
        Some(mut s) => {
            if s.pressure_marker.is_empty() {
                s.pressure_marker = azul::uuid::Uuid::short().as_str().to_string();
            }
            if s.canvas_marker.is_empty() {
                s.canvas_marker = azul::uuid::Uuid::short().as_str().to_string();
            }
            (s.pressure_marker.clone(), s.canvas_marker.clone())
        }
        None => (String::new(), String::new()),
    };
    let (n_strokes, n_undone, metaballs, hud, device_line, last_pressure) = data
        .downcast_ref::<PaintState>()
        .map(|s| {
            (
                s.strokes.len(),
                s.undone.len(),
                s.metaball_mode,
                s.hud,
                s.device_line.clone(),
                s.last_pressure,
            )
        })
        .unwrap_or((0, 0, true, None, None, 0.0));
    let _ = n_undone;

    let mode_label = if metaballs { "Metaballs" } else { "Brush" };
    let title = match device_line {
        Some(dev) => format!(
            "AzPaint  ·  {} strokes  ·  Effect: {}  ·  {}",
            n_strokes, mode_label, dev
        ),
        None => format!(
            "AzPaint  ·  {} strokes  ·  Effect: {}",
            n_strokes, mode_label
        ),
    };
    let mut header = Dom::create_div()
        .with_css(HEADER)
        .with_child(Dom::create_p_with_text(title.as_str()));
    header.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    match hud {
        Some(h) => {
            header.add_child(Dom::create_p_with_text(
                format!(
                    "Pen {}  ·  {}Hz  ·  tilt {:+}° / {:+}°  twist {:+}°{}{}{}",
                    h.device_id,
                    h.rate_hz5,
                    h.tilt_x_deg,
                    h.tilt_y_deg,
                    h.twist_deg,
                    if h.in_contact {
                        "  [contact]"
                    } else {
                        "  [hover]"
                    },
                    if h.is_eraser { "  [ERASER]" } else { "" },
                    if h.barrel { "  [BARREL]" } else { "" },
                )
                .as_str(),
            ));
        }
        None => {
            header.add_child(Dom::create_p_with_text(
                "Pen: none in proximity (hover the stylus to see live values)",
            ));
        }
    }
    header.add_child(
        Dom::create_div()
            .with_css("width: 140px; margin-left: 12px;")
            .with_child(
                azul::widgets::ProgressBar::create(last_pressure)
                    .with_height(azul::css::PixelValue::px(12.0))
                    .dom()
                    .with_marker(azul::option::OptionString::Some(
                        pressure_marker.as_str().into(),
                    )),
            ),
    );

    let cache = RefAny::new(CanvasCache {
        paint: data.clone(),
        texture: None,
        cpu_image: None,
        metaball_gpu: None,
        rendered_rev: 0,
        mb: MetaballField::default(),
    });

    let canvas = Dom::create_image(ImageRef::callback(
        RenderImageCallback::create(render_canvas).to_core(),
        cache.clone(),
    ))
    .with_css(CANVAS)
    .with_marker(azul::option::OptionString::Some(canvas_marker.as_str().into()))
    .with_dataset(OptionRefAny::Some(cache))
    .with_merge_callback(DatasetMergeCallback::from(
        merge_cache as DatasetMergeCallbackType,
    ))
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseDown),
        data.clone(),
        on_pointer_down,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseOver),
        data.clone(),
        on_pointer_move,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        data.clone(),
        on_pointer_up,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::TouchStart),
        data.clone(),
        on_pointer_down,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::TouchMove),
        data.clone(),
        on_pointer_move,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::TouchEnd),
        data.clone(),
        on_pointer_up,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseLeave),
        data.clone(),
        on_pointer_gone,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::PenLeave),
        data.clone(),
        on_pointer_gone,
    );

    use azul::menu::{Menu, MenuItem, StringMenuItem};
    let action = |label: &str, cb: CallbackType| {
        MenuItem::string(StringMenuItem::create(label).with_callback(data.clone(), cb))
    };
    let action_with_accel = |label: &str, cb: CallbackType, keys: &[azul::dom::VirtualKeyCode]| {
        let mut item = StringMenuItem::create(label).with_callback(data.clone(), cb);
        item.accelerator =
            azul::option::OptionVirtualKeyCodeCombo::Some(azul::dom::VirtualKeyCodeCombo {
                keys: keys.to_vec().into(),
            });
        MenuItem::string(item)
    };
    use azul::dom::VirtualKeyCode as K;
    let menu = Menu::create(vec![
        MenuItem::string(StringMenuItem::create("File").with_children(vec![
            action_with_accel("Import image…", on_import, &[K::LWin, K::O]),
            action_with_accel("Export PNG…", on_export, &[K::LWin, K::S]),
            action_with_accel("Export SVG…", on_export_svg, &[K::LWin, K::LShift, K::S]),
        ])),
        MenuItem::string(StringMenuItem::create("Edit").with_children(vec![
            action("Undo", on_undo),
            action("Redo", on_redo),
            action("Clear", on_clear),
        ])),
        MenuItem::string(StringMenuItem::create("View").with_children(vec![action(
            "Toggle effect (Brush / Metaballs)",
            on_toggle_mode,
        )])),
    ]);

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

fn extract_point(info: &CallbackInfo) -> Option<(StrokePoint, bool)> {
    let pos_opt = info.get_cursor_relative_to_node().into_option();
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        match &pos_opt {
            Some(p) => eprintln!("[paint] cursor_relative_to_node = Some({}, {})", p.x, p.y),
            None => eprintln!("[paint] cursor_relative_to_node = None"),
        }
    }
    let pos = pos_opt?;
    if let Some(pen) = info.get_pen_state().into_option() {
        if pen.in_contact {
            return Some((
                StrokePoint {
                    x: pos.x,
                    y: pos.y,
                    pressure: pen.pressure.max(0.05).min(1.0),
                    tilt_x: pen.tilt.x_tilt,
                    tilt_y: pen.tilt.y_tilt,
                    barrel_roll_rad: pen.barrel_roll_rad,
                },
                pen.is_eraser,
            ));
        }
    }
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

fn hud_from(info: &CallbackInfo) -> Option<PenHud> {
    info.get_pen_state().into_option().map(|p| PenHud {
        rate_hz5: ((p.report_rate_hz / 5.0).round() * 5.0) as u16,
        tilt_x_deg: p.tilt.x_tilt.clamp(-90.0, 90.0).round() as i8,
        tilt_y_deg: p.tilt.y_tilt.clamp(-90.0, 90.0).round() as i8,
        twist_deg: p.barrel_roll_rad.to_degrees().round() as i16,
        is_eraser: p.is_eraser,
        barrel: p.barrel_button_pressed,
        in_contact: p.in_contact,
        device_id: p.device_id,
    })
}

fn update_hud(state: &mut PaintState, hud: Option<PenHud>) -> bool {
    if state.hud == hud {
        false
    } else {
        state.hud = hud;
        true
    }
}

fn tablet_device_line(info: &CallbackInfo) -> Option<String> {
    let devices = info.get_tablet_devices();
    let devices = devices.as_slice();
    let d = devices.iter().max_by_key(|d| d.capabilities.count_ones())?;
    let vendor = d.vendor_name.as_str();
    let vendor = if vendor.is_empty() {
        format!("vendor {:04x}", d.vendor_id)
    } else {
        vendor.to_string()
    };
    let size = if d.physical_width_mm > 0.0 {
        format!(
            ", {:.0}x{:.0} mm",
            d.physical_width_mm, d.physical_height_mm
        )
    } else {
        String::new()
    };
    Some(format!(
        "{} ({}{}, {} device(s), caps 0x{:x}, pmax {:.0})",
        d.name.as_str(),
        vendor,
        size,
        devices.len(),
        d.capabilities,
        d.pressure_max,
    ))
}

fn push_pressure_to_meter(info: &mut CallbackInfo, marker: &str) -> Option<f32> {
    let pct = match info.get_pen_state().into_option() {
        Some(pen) => pen.pressure.clamp(0.0, 1.0) * 100.0,
        None => {
            let ms = info.get_current_mouse_state();
            if ms.left_down {
                50.0
            } else {
                0.0
            }
        }
    };
    let node_id = info.get_node_id_by_marker(marker.to_string()).into_option();
    let ok = node_id
        .map(|n| azul::widgets::ProgressBar::update_progress(*info, n, pct))
        .unwrap_or(false);
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        eprintln!(
            "[paint] t={}ms fast-path: marker={marker:?} node={:?} pct={pct} update_progress={ok}",
            dbg_ms(),
            node_id.map(|n| (n.dom.inner, n.node.inner as i64 - 1)),
        );
    }
    ok.then_some(pct)
}

fn poke_canvas(info: &mut CallbackInfo, marker: &str) {
    if let Some(node) = info.get_node_id_by_marker(marker.to_string()).into_option() {
        let raw = node.node.into_raw();
        if raw != 0 {
            info.update_image_callback(node.dom, azul::dom::NodeId { inner: raw });
        }
    }
}

extern "C" fn on_pointer_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        eprintln!("[paint] t={}ms on_pointer_down FIRED", dbg_ms());
    }
    {
        let ms = info.get_current_mouse_state();
        if ms.right_down || ms.middle_down {
            return Update::DoNothing;
        }
    }
    let hud = hud_from(&info);
    let (point, is_eraser) = match extract_point(&info) {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    update_hud(&mut state, hud);
    state.begin_stroke(point, is_eraser);
    let marker = state.pressure_marker.clone();
    let canvas = state.canvas_marker.clone();
    drop(state);
    if let Some(pct) = push_pressure_to_meter(&mut info, &marker) {
        if let Some(mut s) = data.downcast_mut::<PaintState>() {
            s.last_pressure = pct;
        }
    }
    poke_canvas(&mut info, &canvas);
    Update::DoNothing
}

extern "C" fn on_pointer_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hud = hud_from(&info);
    let point = extract_point(&info);
    let pushed = data
        .downcast_ref::<PaintState>()
        .map(|s| s.pressure_marker.clone())
        .and_then(|m| push_pressure_to_meter(&mut info, &m));
    let device_line = data
        .downcast_ref::<PaintState>()
        .is_some_and(|s| s.device_line.is_none())
        .then(|| tablet_device_line(&info))
        .flatten();
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    if let Some(pct) = pushed {
        state.last_pressure = pct;
    }
    let mut hud_changed = false;
    if device_line.is_some() {
        state.device_line = device_line;
        hud_changed = true;
    }
    let hud_changed = update_hud(&mut state, hud) || hud_changed;
    if state.current.is_none() {
        return if hud_changed {
            Update::RefreshDom
        } else {
            Update::DoNothing
        };
    }
    match point {
        Some((p, _)) => {
            state.extend_stroke(p);
            let canvas = state.canvas_marker.clone();
            drop(state);
            poke_canvas(&mut info, &canvas);
            Update::DoNothing
        }
        None if hud_changed => Update::RefreshDom,
        None => Update::DoNothing,
    }
}

extern "C" fn on_pointer_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        let ms = info.get_current_mouse_state();
        eprintln!(
            "[paint] t={}ms on_pointer_up FIRED (left_down={})",
            dbg_ms(),
            ms.left_down
        );
    }
    let hud = hud_from(&info);
    let marker = match data.downcast_mut::<PaintState>() {
        Some(mut s) => {
            update_hud(&mut s, hud);
            s.end_stroke();
            s.pressure_marker.clone()
        }
        None => return Update::DoNothing,
    };
    if let Some(pct) = push_pressure_to_meter(&mut info, &marker) {
        if let Some(mut s) = data.downcast_mut::<PaintState>() {
            s.last_pressure = pct;
        }
    }
    Update::RefreshDom
}

extern "C" fn on_pointer_gone(mut data: RefAny, _info: CallbackInfo) -> Update {
    if std::env::var("AZ_PAINT_DEBUG").is_ok() {
        eprintln!("[paint] on_pointer_gone FIRED");
    }
    let mut state = match data.downcast_mut::<PaintState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    let had_hud = state.hud.take().is_some();
    let had_stroke = state.current.is_some();
    if had_stroke {
        state.end_stroke();
    }
    if had_hud || had_stroke {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
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

extern "C" fn on_import(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        "Import image",
        OptionString::None,
        OptionFileTypeList::None,
        data,
        on_import_picked,
    );
    Update::DoNothing
}

extern "C" fn on_import_picked(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let _request = path.read_bytes(data, on_import_bytes);
    Update::DoNothing
}

extern "C" fn on_import_bytes(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(read) = FileReadBytesResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let bytes = match read.result {
        ResultU8VecFileError::Ok(b) => b,
        ResultU8VecFileError::Err(_) => return Update::DoNothing,
    };
    let decoded = RawImage::decode_image_bytes_any(U8VecRef::from(bytes.as_ref()));
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

extern "C" fn on_export(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::save_file("Export PNG", "canvas.png", data, on_export_target);
    Update::DoNothing
}

extern "C" fn on_export_target(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = SaveTargetResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(target) = picked.target.into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = target.as_path().into_option() else {
        return Update::DoNothing;
    };
    match data.downcast_mut::<PaintState>() {
        Some(mut s) => s.request_export(path.as_string().as_str().to_string()),
        None => return Update::DoNothing,
    }
    Update::RefreshDom
}

extern "C" fn on_export_svg(mut data: RefAny, _info: CallbackInfo) -> Update {
    let svg = match data.downcast_ref::<PaintState>() {
        Some(s) => strokes_to_svg(&s.strokes, s.metaball_mode),
        None => return Update::DoNothing,
    };
    let _scheduled = FileDialog::save_bytes("strokes.svg", "image/svg+xml", svg.into_bytes());
    Update::DoNothing
}

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
        StrokePoint {
            x,
            y,
            pressure,
            tilt_x: 0.0,
            tilt_y: 0.0,
            barrel_roll_rad: 0.0,
        }
    }

    fn row_coverage(img: &RawImage, y: usize, bg: ColorU) -> Vec<u8> {
        let RawImageData::U8(ref px) = img.pixels else {
            panic!("U8 raster")
        };
        let px = px.as_ref();
        (0..img.width)
            .map(|x| {
                let o = (y * img.width + x) * 4;
                let r = px[o] as i32;
                ((bg.r as i32 - r).max(0) * 255 / bg.r.max(1) as i32) as u8
            })
            .collect()
    }

    fn rising_edges(profile: &[u8]) -> usize {
        let inside = |c: u8| c >= 128;
        profile
            .windows(2)
            .filter(|w| !inside(w[0]) && inside(w[1]))
            .count()
    }

    fn black_dab(x: f32, y: f32) -> Stroke {
        Stroke {
            points: vec![pt(x, y, 0.5)],
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            is_eraser: false,
        }
    }

    #[test]
    fn metaball_merges_have_no_box_edges_and_dabs_keep_their_size() {
        let bg = ColorU {
            r: 250,
            g: 250,
            b: 246,
            a: 255,
        };
        let (w, h) = (100u32, 60u32);
        let alone = metaball_image(
            &mut MetaballField::default(),
            &[black_dab(30.0, 30.0)],
            w,
            h,
            bg,
            None,
        );
        let pair = metaball_image(
            &mut MetaballField::default(),
            &[black_dab(30.0, 30.0), black_dab(60.0, 30.0)],
            w,
            h,
            bg,
            None,
        );

        let alone_row = row_coverage(&alone, 30, bg);
        let pair_row = row_coverage(&pair, 30, bg);
        assert_eq!(rising_edges(&alone_row), 1, "{alone_row:?}");
        assert_eq!(
            rising_edges(&pair_row),
            2,
            "two separate blobs must show exactly two rising edges on the row              \
             through their centres — a notch or spur adds one: {pair_row:?}"
        );
        for y in 0..h as usize {
            let row = row_coverage(&pair, y, bg);
            assert!(
                rising_edges(&row) <= 2,
                "row {y} has a tear/notch (more than one edge per blob): {row:?}"
            );
        }

        let left = |row: &[u8]| (0..30).map(|x| row[x]).collect::<Vec<_>>();
        assert_eq!(
            left(&alone_row),
            left(&pair_row),
            "a neighbour 30 px away changed a dab's rim"
        );

        let first_inside = alone_row.iter().position(|c| *c >= 128).expect("blob");
        let radius = 30.0 - first_inside as f32;
        assert!(
            (7.5..=9.5).contains(&radius),
            "visible radius drifted: {radius}"
        );
    }

    #[test]
    fn the_cpu_canvas_re_rasterises_when_its_box_changes() {
        assert!(cpu_canvas_needs_raster(
            3,
            3,
            Some((400, 300)),
            (500, 300),
            false
        ));
        assert!(
            cpu_canvas_needs_raster(3, 3, None, (500, 300), false),
            "no bitmap yet"
        );
        assert!(
            cpu_canvas_needs_raster(2, 3, Some((500, 300)), (500, 300), false),
            "strokes changed"
        );
        assert!(
            cpu_canvas_needs_raster(3, 3, Some((500, 300)), (500, 300), true),
            "export pending"
        );
        assert!(
            !cpu_canvas_needs_raster(3, 3, Some((500, 300)), (500, 300), false),
            "nothing changed"
        );
    }

    #[test]
    fn the_gpu_metaball_shader_uses_the_cpu_constants() {
        let support_sq = format!("1.0 - q / {:.1}", METABALL_SUPPORT * METABALL_SUPPORT);
        assert!(
            METABALL_FS_BODY.contains(&support_sq),
            "shader support radius: {support_sq}"
        );
        let band = format!(
            "(field - {:.2}) / {:.2}",
            METABALL_ISO - METABALL_AA,
            2.0 * METABALL_AA
        );
        assert!(METABALL_FS_BODY.contains(&band), "shader AA band: {band}");
        assert!(
            METABALL_FS_BODY.contains("t * t * t"),
            "shader must use the cubic Wyvill kernel"
        );
        assert!(
            !METABALL_FS_BODY.contains("1.0 / (q + 0.18)"),
            "the infinite-support kernel is gone"
        );
    }

    #[test]
    fn metaball_kernel_is_compact_and_continuous() {
        assert_eq!(metaball_kernel(0.0), 1.0);
        let s2 = METABALL_SUPPORT * METABALL_SUPPORT;
        assert_eq!(metaball_kernel(s2), 0.0, "exactly zero at the support edge");
        assert_eq!(metaball_kernel(s2 * 4.0), 0.0, "and beyond it");
        assert_eq!(metaball_kernel(f32::NAN), 0.0);
        let mut prev = 1.0f32;
        for i in 1..=400 {
            let q = s2 * (i as f32 / 400.0);
            let c = metaball_kernel(q);
            assert!(c <= prev && c >= 0.0, "q={q}: {c} > {prev}");
            assert!(prev - c < 0.02, "jump at q={q}: {prev} → {c}");
            prev = c;
        }
        assert!(prev < 1e-6, "the last sample before the edge is ~0: {prev}");
    }

    #[test]
    fn svg_export_brush_strokes_as_lines() {
        let strokes = vec![Stroke {
            points: vec![pt(10.0, 20.0, 0.5), pt(40.0, 60.0, 1.0)],
            color: ColorU {
                r: 200,
                g: 30,
                b: 40,
                a: 255,
            },
            is_eraser: false,
        }];
        let svg = strokes_to_svg(&strokes, false);
        assert!(svg.starts_with("<svg"), "{svg}");
        assert!(svg.ends_with("</svg>"), "{svg}");
        assert!(
            svg.contains("<line"),
            "brush stroke must serialize as line segments: {svg}"
        );
        assert!(
            svg.contains("rgb(200,30,40)"),
            "stroke colour must survive: {svg}"
        );
        assert!(svg.contains("stroke-linecap=\"round\""), "{svg}");
        assert!(svg.contains("viewBox=\""), "{svg}");
    }

    #[test]
    fn svg_export_metaball_strokes_as_ellipses_and_eraser_uses_bg() {
        let strokes = vec![Stroke {
            points: vec![pt(5.0, 5.0, 0.8)],
            color: ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 255,
            },
            is_eraser: true,
        }];
        let svg = strokes_to_svg(&strokes, true);
        let bg = canvas_bg();
        assert!(
            svg.contains("<ellipse"),
            "metaball stroke must serialize as ellipses: {svg}"
        );
        assert!(
            svg.contains(&format!("rgb({},{},{})", bg.r, bg.g, bg.b)),
            "eraser must use the canvas background colour: {svg}"
        );
        assert!(
            !svg.contains("rgb(1,2,3)"),
            "eraser must NOT use the stroke colour: {svg}"
        );
    }

    #[test]
    fn svg_export_empty_model_is_valid() {
        let svg = strokes_to_svg(&[], true);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"), "{svg}");
        assert!(!svg.contains("inf") && !svg.contains("NaN"), "{svg}");
    }
}
