//! The ink on the pages, drawn as AzPaint draws its canvas: the engine's
//! brush (`Texture::paint_stroke` on the GPU, `RawImage::paint_stroke` on the
//! CPU) into a frame each ink layer keeps. A stroke being drawn adds only its
//! new segments to that frame, and the pointer handlers ask for a new frame
//! of the layer (`update_image_callback`), not for a new DOM. A frame is drawn
//! anew only when the kept strokes of its page change: a stroke ends, the
//! eraser takes one, another file opens.
//!
//! A page has two layers. The marker's lies under the code: a highlighter
//! colours the paper under the letters, so it is drawn opaque, in its colour
//! washed into the white paper. The pen's lies over the code and takes the
//! pointer.

use std::collections::BTreeMap;

use azul::{
    callbacks::RenderImageCallbackInfo,
    css::PhysicalSizeU32,
    gl::Texture,
    image::{Brush, ImageRef, RawImage, RawImageFormat},
    prelude::*,
    vec::U8VecRef,
};

use crate::{
    model::{InkPoint, Semantic, Stroke},
    AppState,
};

/// The pen's radius at full pressure, px.
const PEN_RADIUS: f32 = 1.6;
/// The marker's radius, px: a broad nib, one line of code tall.
const MARKER_RADIUS: f32 = 9.0;
/// How much of its colour the marker leaves on the white paper.
pub const MARKER_ALPHA: f32 = 0.38;

/// Which strokes a layer draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// The highlighter's, under the code.
    Marker,
    /// Every other meaning's, over the code.
    Pen,
}

impl Layer {
    /// The layer a stroke of `semantic` is drawn on.
    #[must_use]
    pub const fn of(semantic: Semantic) -> Self {
        if semantic.is_highlighter() {
            Self::Marker
        } else {
            Self::Pen
        }
    }
}

/// What a layer's frame was drawn from. A frame whose key is not the one its
/// page has now is drawn anew.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameKey {
    /// How many files were opened before: a frame of an earlier file never
    /// matches.
    opened: u64,
    /// The page: the DOM rebuild hands a layer's frame to the layer at the
    /// same place, which may show another page.
    page: usize,
    /// The revision of the page's kept strokes.
    kept: u64,
    /// The layer's size in px.
    size: (u32, u32),
}

/// The revisions of the pages' kept strokes, in [`AppState::ink`]: the
/// handlers touch a page when its kept strokes change, the layers read the
/// key to know whether their frame still shows them.
#[derive(Debug, Default)]
pub struct InkRevisions {
    /// Files opened so far.
    opened: u64,
    /// The last revision handed out (shared by all pages, so a revision is
    /// never handed out twice).
    clock: u64,
    /// Each touched page's revision; an untouched page is at 0.
    pages: BTreeMap<usize, u64>,
}

impl InkRevisions {
    /// The kept strokes of `page` changed.
    pub fn touch(&mut self, page: usize) {
        self.clock += 1;
        self.pages.insert(page, self.clock);
    }

    /// Another file is open: every frame drawn before shows ink that is gone.
    pub fn open(&mut self) {
        self.opened += 1;
        self.pages.clear();
    }

    /// The key a frame of `page` at `size` must have to be shown as it is.
    #[must_use]
    pub fn key(&self, page: usize, size: (u32, u32)) -> FrameKey {
        FrameKey {
            opened: self.opened,
            page,
            kept: self.pages.get(&page).copied().unwrap_or(0),
            size,
        }
    }
}

/// How much of the stroke being drawn a frame shows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LiveProgress {
    /// The stroke's id.
    id: Option<u64>,
    /// How many of its points are painted.
    points: usize,
}

impl LiveProgress {
    /// Paints what the frame lacks of `live` (`None`: no stroke is being
    /// drawn on this layer) with `paint`; whether anything was painted.
    pub fn paint(
        &mut self,
        live: Option<&Stroke>,
        paint: &mut dyn FnMut(f32, f32, f32, f32, Brush),
    ) -> bool {
        let Some(live) = live else {
            *self = Self::default();
            return false;
        };
        if self.id != Some(live.id) {
            *self = Self {
                id: Some(live.id),
                points: 0,
            };
        }
        if live.points.len() <= self.points {
            return false;
        }
        paint_from(live, self.points, paint);
        self.points = live.points.len();
        true
    }
}

/// The engine brush a segment of a `semantic` stroke is drawn with (AzPaint's
/// `brush_for`): the pen's width follows the pressure; the marker is a broad
/// nib in the colour it leaves on the paper.
#[must_use]
pub fn brush(semantic: Semantic, pressure: f32) -> Brush {
    let c = semantic.color();
    let mut b = if semantic.is_highlighter() {
        let washed = |v: u8| (255.0 - (255.0 - f32::from(v)) * MARKER_ALPHA).round() as u8;
        Brush::create(
            ColorU {
                r: washed(c.r),
                g: washed(c.g),
                b: washed(c.b),
                a: 255,
            },
            MARKER_RADIUS,
        )
    } else {
        Brush::create(c, PEN_RADIUS * (0.35 + 0.65 * pressure.clamp(0.0, 1.0)))
    };
    b.hardness = if semantic.is_highlighter() { 0.85 } else { 0.6 };
    b.flow = 1.0;
    b.spacing = 0.2;
    b
}

/// Paints `stroke` from its point `from` on: every segment that ends at a
/// point from there (a stroke of one point is a dab).
fn paint_from(stroke: &Stroke, from: usize, paint: &mut dyn FnMut(f32, f32, f32, f32, Brush)) {
    if let [p] = stroke.points.as_slice() {
        paint(p.x, p.y, p.x, p.y, brush(stroke.semantic, p.pressure));
        return;
    }
    for i in from.max(1)..stroke.points.len() {
        let (a, b) = (stroke.points[i - 1], stroke.points[i]);
        paint(
            a.x,
            a.y,
            b.x,
            b.y,
            brush(stroke.semantic, (a.pressure + b.pressure) * 0.5),
        );
    }
}

/// A transparent `w` x `h` frame with `strokes` painted on the CPU
/// (premultiplied RGBA: painting over transparent black keeps it so).
#[must_use]
pub fn draw_cpu(w: u32, h: u32, strokes: &[Stroke]) -> RawImage {
    let mut img = RawImage::create_rgba8(w, h, vec![0u8; (w as usize) * (h as usize) * 4], true);
    for s in strokes {
        paint_from(s, 0, &mut |x0, y0, x1, y1, b| {
            img.paint_stroke(x0, y0, x1, y1, b)
        });
    }
    img
}

/// The frame a layer keeps between draws.
#[derive(Default)]
struct Frame {
    /// What the frame shows of the kept strokes (`None`: nothing yet).
    key: Option<FrameKey>,
    /// What it shows of the stroke being drawn.
    live: LiveProgress,
    /// CPU: the pixels, and the image last handed out - handed out again
    /// while nothing changed, so an unchanged frame repaints nothing.
    pixels: Option<RawImage>,
    shown: Option<ImageRef>,
    /// GPU: the texture painted into, handed out every time.
    texture: Option<Texture>,
}

/// One ink layer of one page: the image callback's data and the node's
/// dataset (the pointer handlers read the page from it).
pub struct InkCanvas {
    /// The app ([`AppState`]): the strokes are read at draw time, so drawing
    /// needs no new DOM.
    pub app: RefAny,
    /// The page whose ink this is.
    pub page: usize,
    /// Marker or pen.
    pub layer: Layer,
    /// The kept frame, carried over a DOM rebuild by [`merge_ink`].
    frame: Frame,
}

impl InkCanvas {
    /// The `layer` of `page`, nothing drawn yet.
    #[must_use]
    pub fn create(app: RefAny, page: usize, layer: Layer) -> Self {
        Self {
            app,
            page,
            layer,
            frame: Frame::default(),
        }
    }
}

/// The strokes of `layer` on `page` are `st`'s.
fn on(st: &Stroke, page: usize, layer: Layer) -> bool {
    st.page == page && Layer::of(st.semantic) == layer
}

/// The key a frame of `layer` on `page` at `size` must have now, and the
/// stroke being drawn when it is on that layer of that page.
fn wanted(
    app: &mut RefAny,
    page: usize,
    layer: Layer,
    size: (u32, u32),
) -> Option<(FrameKey, Option<Stroke>)> {
    let s = app.downcast_ref::<AppState>()?;
    let live = s.live.as_ref().filter(|st| on(st, page, layer)).cloned();
    Some((s.ink.key(page, size), live))
}

/// The kept strokes of `layer` on `page`, read only when a frame is drawn
/// anew.
fn kept(app: &mut RefAny, page: usize, layer: Layer) -> Vec<Stroke> {
    app.downcast_ref::<AppState>()
        .map(|s| {
            s.strokes
                .iter()
                .filter(|st| on(st, page, layer))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// The ink layer's `RenderImageCallback`: the kept frame, with what changed
/// since it was drawn painted in.
pub extern "C" fn render_ink(mut data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_logical_size();
    let (w, h) = (size.width.max(1.0) as u32, size.height.max(1.0) as u32);
    draw_frame(&mut data, &mut info, w, h).unwrap_or_else(|| {
        ImageRef::null_image(
            w as usize,
            h as usize,
            RawImageFormat::RGBA8,
            U8VecRef::from(&[][..]),
        )
    })
}

fn draw_frame(
    data: &mut RefAny,
    info: &mut RenderImageCallbackInfo,
    w: u32,
    h: u32,
) -> Option<ImageRef> {
    let mut canvas = data.downcast_mut::<InkCanvas>()?;
    let canvas = &mut *canvas;
    let (mut app, page, layer) = (canvas.app.clone(), canvas.page, canvas.layer);
    let (key, live) = wanted(&mut app, page, layer, (w, h))?;
    let frame = &mut canvas.frame;

    let gl = info
        .get_gl_context()
        .into_option()
        .filter(|g| g.is_gl_usable());
    if let Some(gl) = gl {
        let fits = frame
            .texture
            .as_ref()
            .is_some_and(|t| t.size.width == w && t.size.height == h);
        if !fits {
            let clear = ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            };
            frame.texture = Some(Texture::allocate_rgba8(
                gl,
                PhysicalSizeU32 {
                    width: w,
                    height: h,
                },
                clear,
            ));
            frame.key = None;
        }
        let tex = frame.texture.as_mut()?;
        if frame.key != Some(key) {
            // A fresh texture holds whatever the driver left in it: clear it first.
            tex.clear();
            for s in &kept(&mut app, page, layer) {
                paint_from(s, 0, &mut |x0, y0, x1, y1, b| {
                    tex.paint_stroke(x0, y0, x1, y1, b)
                });
            }
            frame.key = Some(key);
            frame.live = LiveProgress::default();
        }
        frame.live.paint(live.as_ref(), &mut |x0, y0, x1, y1, b| {
            tex.paint_stroke(x0, y0, x1, y1, b)
        });
        return Some(ImageRef::gl_texture(tex.clone()));
    }

    let mut changed = false;
    if frame.key != Some(key) || frame.pixels.is_none() {
        frame.pixels = Some(draw_cpu(w, h, &kept(&mut app, page, layer)));
        frame.key = Some(key);
        frame.live = LiveProgress::default();
        changed = true;
    }
    let pixels = frame.pixels.as_mut()?;
    changed |= frame.live.paint(live.as_ref(), &mut |x0, y0, x1, y1, b| {
        pixels.paint_stroke(x0, y0, x1, y1, b)
    });
    if changed || frame.shown.is_none() {
        frame.shown = ImageRef::create_rawimage(pixels.clone()).into_option();
    }
    frame.shown.clone()
}

/// The layer's merge callback: a DOM rebuild hands the old layer's frame to
/// the new one, so a frame that still fits is not drawn again.
pub extern "C" fn merge_ink(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let frame = match old_data.downcast_mut::<InkCanvas>() {
        Some(mut old) => core::mem::take(&mut old.frame),
        None => return new_data,
    };
    if let Some(mut new) = new_data.downcast_mut::<InkCanvas>() {
        new.frame = frame;
    }
    new_data
}

/// A point of a stroke from the pointer: a pen's pressure (a mouse has
/// none, and draws at a middle one) and tilt.
pub fn point_from(x: f32, y: f32, pressure: f32, tilt_x: f32, tilt_y: f32) -> InkPoint {
    InkPoint {
        x,
        y,
        pressure: if pressure <= 0.0 {
            0.55
        } else {
            pressure.clamp(0.05, 1.0)
        },
        tilt_x,
        tilt_y,
    }
}
