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
    line_map::LineMap,
    model::{InkPoint, Semantic, Stroke},
    AppState,
};

/// The map of a page nothing was put in: for the painting that is not of a page of the window.
static PLAIN: LineMap = LineMap::plain();

/// The pen's radius at full pressure, px.
const PEN_RADIUS: f32 = 1.6;
/// The marker's radius, px: a broad nib, one line of code tall.
const MARKER_RADIUS: f32 = 9.0;
/// How much of its colour the marker leaves on the white paper.
pub const MARKER_ALPHA: f32 = 0.38;
/// The most a layer is magnified by: a display above this ratio is drawn at it (a 4x layer of a
/// 1000 x 651 px page is 4000 x 2604 px, 42 MB).
pub const MAX_RASTER_SCALE: f32 = 4.0;

/// The colour a fresh layer is cleared to: nothing. A layer is composited as premultiplied alpha, so
/// "transparent white" (255, 255, 255, 0) would not be nothing: it ADDS white to everything under the
/// layer, and the code under the pen's layer disappears.
pub const CLEAR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// The scale a layer is rasterised at: the window's device pixel ratio (2 on a Retina display), so
/// a stroke's edge is as sharp as the display's pixels; 1 where the window reports none or nonsense.
#[must_use]
pub fn raster_scale(device_pixel_ratio: f32) -> f32 {
    if device_pixel_ratio.is_finite() && device_pixel_ratio >= 1.0 {
        device_pixel_ratio.min(MAX_RASTER_SCALE)
    } else {
        1.0
    }
}

/// A layer's frame size in px: its `logical` size (px at 1x) at `scale`, whole px, at least one each way.
#[must_use]
pub fn raster_size(logical: (f32, f32), scale: f32) -> (u32, u32) {
    let px = |v: f32| (v * scale).round().max(1.0) as u32;
    (px(logical.0), px(logical.1))
}

/// `b` as it is in a frame rasterised at `scale`: the nib is as wide on the screen as at 1x, so in the
/// frame's px it is `scale` times as wide.
#[must_use]
pub fn brush_at(mut b: Brush, scale: f32) -> Brush {
    b.radius *= scale;
    b
}

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
    /// How the page's lines are drawn (`LineMap::signature`): ink is drawn where its lines are, so a frame drawn for another
    /// map is drawn anew.
    map: u64,
}

/// A page's picture grew by one stroke: its revision went from `from` to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Append {
    pub from: u64,
    pub to: u64,
    pub id: u64,
}

/// How a layer's frame gets to the picture its page has now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatchUp {
    /// It shows it already.
    Same,
    /// It shows the same as before: only its key moves.
    KeyOnly,
    /// It shows the page but for the last segments of the stroke just kept.
    Tail,
    /// Every kept stroke is painted again.
    Whole,
}

/// How the frame that shows `had` (plus the stroke `live_id`, painted as far as it was drawn) gets to
/// the picture `now`. `append` is the page's last growth and `appended_on` the layer that stroke is
/// on; `this` is the frame's own layer.
///
/// A stroke that ends was painted into the frame while it was drawn: when that is the only change since
/// the frame was drawn, nothing is painted again but the last segments the frame had not seen yet. Every
/// other change (an erase, an undo, a file opened, a resize, two strokes between two frames) draws the
/// layer anew from the kept strokes.
#[must_use]
pub fn catch_up(
    had: Option<FrameKey>,
    now: FrameKey,
    append: Option<Append>,
    appended_on: Option<Layer>,
    this: Layer,
    live_id: Option<u64>,
) -> CatchUp {
    if had == Some(now) {
        return CatchUp::Same;
    }
    let (Some(had), Some(append), Some(on)) = (had, append, appended_on) else {
        return CatchUp::Whole;
    };
    let same_picture = had.opened == now.opened && had.page == now.page && had.size == now.size && had.map == now.map;
    if !same_picture || had.kept != append.from || now.kept != append.to {
        return CatchUp::Whole;
    }
    if on != this {
        CatchUp::KeyOnly
    } else if live_id == Some(append.id) {
        CatchUp::Tail
    } else {
        CatchUp::Whole
    }
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
    /// A page whose last change was a stroke kept on it: what that change was.
    appends: BTreeMap<usize, Append>,
}

impl InkRevisions {
    /// A stroke (`id`) was kept on `page`: the page's picture grew by it. A layer whose frame shows
    /// the stroke already needs no redraw of the others ([`catch_up`]).
    pub fn appended(&mut self, page: usize, id: u64) {
        let from = self.pages.get(&page).copied().unwrap_or(0);
        self.clock += 1;
        self.pages.insert(page, self.clock);
        self.appends.insert(
            page,
            Append {
                from,
                to: self.clock,
                id,
            },
        );
    }

    /// The last growth of `page`'s picture, when that is the last thing that happened to its strokes.
    #[must_use]
    pub fn append_of(&self, page: usize) -> Option<Append> {
        self.appends.get(&page).copied()
    }

    /// The kept strokes of `page` changed in a way a frame cannot catch up with: it is drawn anew.
    pub fn touch(&mut self, page: usize) {
        self.clock += 1;
        self.pages.insert(page, self.clock);
        self.appends.remove(&page);
    }

    /// Another file is open: every frame drawn before shows ink that is gone.
    pub fn open(&mut self) {
        self.opened += 1;
        self.pages.clear();
        self.appends.clear();
    }

    /// The key a frame of `page` at `size` must have to be shown as it is, on a page nobody put lines in.
    #[must_use]
    pub fn key(&self, page: usize, size: (u32, u32)) -> FrameKey {
        self.key_with(page, size, 0)
    }

    /// The key a frame of `page` at `size` must have to be shown as it is, its lines drawn as the map with `map` as its
    /// signature draws them.
    #[must_use]
    pub fn key_with(&self, page: usize, size: (u32, u32), map: u64) -> FrameKey {
        FrameKey {
            opened: self.opened,
            page,
            kept: self.pages.get(&page).copied().unwrap_or(0),
            size,
            map,
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
    /// What the frame has painted of a stroke in progress: its id and how many of its points.
    #[must_use]
    pub const fn progress(&self) -> Option<(u64, usize)> {
        match self.id {
            Some(id) => Some((id, self.points)),
            None => None,
        }
    }

    /// Paints what the frame lacks of `live` (`None`: no stroke is being
    /// drawn on this layer) with `paint`, in the px of a frame rasterised at `scale`; whether
    /// anything was painted.
    pub fn paint(
        &mut self,
        live: Option<&Stroke>,
        scale: f32,
        paint: &mut dyn FnMut(f32, f32, f32, f32, Brush),
    ) -> bool {
        self.paint_mapped(live, scale, &PLAIN, paint)
    }

    /// [`Self::paint`] on a page whose lines are drawn as `map` says: a stroke is kept where the page was first drawn and
    /// painted where its lines are now.
    pub fn paint_mapped(
        &mut self,
        live: Option<&Stroke>,
        scale: f32,
        map: &LineMap,
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
        paint_from(live, self.points, scale, map, paint);
        self.points = live.points.len();
        true
    }
}

/// The width of the nib of the pen that draws `semantic`, at full pressure, px: the marker's broad one or the pen's fine one
/// (what an agent is told a stroke was drawn with).
#[must_use]
pub fn nib_width(semantic: Semantic) -> f32 {
    if semantic.is_highlighter() {
        MARKER_RADIUS * 2.0
    } else {
        PEN_RADIUS * 2.0
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
/// point from there (a stroke of one point is a dab). The points and the nib are in the px of a frame
/// rasterised at `scale` (the stroke's own are the page's logical px, as the page was first drawn: `map` carries them to where
/// the page's lines are now).
fn paint_from(
    stroke: &Stroke,
    from: usize,
    scale: f32,
    map: &LineMap,
    paint: &mut dyn FnMut(f32, f32, f32, f32, Brush),
) {
    if let [p] = stroke.points.as_slice() {
        let y = map.to_display(p.y) * scale;
        paint(p.x * scale, y, p.x * scale, y, brush_at(brush(stroke.semantic, p.pressure), scale));
        return;
    }
    for i in from.max(1)..stroke.points.len() {
        let (a, b) = (stroke.points[i - 1], stroke.points[i]);
        paint(
            a.x * scale,
            map.to_display(a.y) * scale,
            b.x * scale,
            map.to_display(b.y) * scale,
            brush_at(brush(stroke.semantic, (a.pressure + b.pressure) * 0.5), scale),
        );
    }
}

/// A transparent `w` x `h` frame (px at `scale` times the page's logical px) with `strokes`
/// painted on the CPU (premultiplied RGBA: painting over transparent black keeps it so).
#[must_use]
pub fn draw_cpu(w: u32, h: u32, scale: f32, strokes: &[Stroke]) -> RawImage {
    draw_cpu_mapped(w, h, scale, &PLAIN, strokes)
}

/// [`draw_cpu`] on a page whose lines are drawn as `map` says.
#[must_use]
pub fn draw_cpu_mapped(w: u32, h: u32, scale: f32, map: &LineMap, strokes: &[Stroke]) -> RawImage {
    let mut img = RawImage::create_rgba8(w, h, vec![0u8; (w as usize) * (h as usize) * 4], true);
    for s in strokes {
        paint_from(s, 0, scale, map, &mut |x0, y0, x1, y1, b| {
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

/// What a frame of `layer` on `page` at `size` has to show now: the key it must have, the stroke being
/// drawn when it is on that layer of that page, how the page last grew, and where the page's lines are drawn (the ink goes down
/// with the line it was drawn on when lines are put in above it).
fn wanted(
    app: &mut RefAny,
    page: usize,
    layer: Layer,
    size: (u32, u32),
) -> Option<(FrameKey, Option<Stroke>, Option<Append>, LineMap)> {
    let s = app.downcast_ref::<AppState>()?;
    let live = s.live.as_ref().filter(|st| on(st, page, layer)).cloned();
    let map = s.page_map(page);
    Some((s.ink.key_with(page, size, map.signature()), live, s.ink.append_of(page), map))
}

/// The layer the kept stroke `id` is on.
fn layer_of_stroke(app: &mut RefAny, id: u64) -> Option<Layer> {
    let s = app.downcast_ref::<AppState>()?;
    s.strokes
        .iter()
        .find(|st| st.id == id)
        .map(|st| Layer::of(st.semantic))
}

/// The kept stroke `id`, for the last segments a frame has yet to paint of it.
fn stroke_by_id(app: &mut RefAny, id: u64) -> Option<Stroke> {
    let s = app.downcast_ref::<AppState>()?;
    s.strokes.iter().find(|st| st.id == id).cloned()
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
    // The frame has the display's pixels: the layer's logical size times the window's device
    // pixel ratio (the engine asks again when the window moves to a display of another ratio).
    let bounds = info.get_bounds();
    let logical = bounds.get_logical_size();
    let scale = raster_scale(bounds.get_hidpi_factor().inner.get());
    let (w, h) = raster_size((logical.width, logical.height), scale);
    draw_frame(&mut data, &mut info, w, h, scale).unwrap_or_else(|| {
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
    scale: f32,
) -> Option<ImageRef> {
    let began = std::time::Instant::now();
    let drawn = draw_frame_untimed(data, info, w, h, scale);
    // `AZREVIEW_PROFILE`: what a frame of an ink layer cost.
    let app = data.downcast_ref::<InkCanvas>().map(|c| c.app.clone());
    if let Some(mut app) = app {
        if let Some(mut s) = app.downcast_mut::<AppState>() {
            if s.perf.is_on() {
                s.perf.drawn(u32::try_from(began.elapsed().as_micros()).unwrap_or(u32::MAX));
            }
        }
    }
    drawn
}

fn draw_frame_untimed(
    data: &mut RefAny,
    info: &mut RenderImageCallbackInfo,
    w: u32,
    h: u32,
    scale: f32,
) -> Option<ImageRef> {
    let mut canvas = data.downcast_mut::<InkCanvas>()?;
    let canvas = &mut *canvas;
    let (mut app, page, layer) = (canvas.app.clone(), canvas.page, canvas.layer);
    let (key, live, append, map) = wanted(&mut app, page, layer, (w, h))?;
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
            frame.texture = Some(Texture::allocate_rgba8(
                gl,
                PhysicalSizeU32 {
                    width: w,
                    height: h,
                },
                CLEAR,
            ));
            frame.key = None;
        }
        let what = how_to_catch_up(frame, key, append, &mut app, layer);
        let tex = frame.texture.as_mut()?;
        match what {
            CatchUp::Same => {}
            CatchUp::KeyOnly => frame.key = Some(key),
            CatchUp::Tail => {
                // The stroke that ended was painted as it was drawn: only the segments the frame has
                // not seen yet are left (a stroke is kept when the pen lifts, the last moves may not
                // have had a frame).
                if let Some(done) = append.and_then(|a| stroke_by_id(&mut app, a.id)) {
                    frame.live.paint_mapped(Some(&done), scale, &map, &mut |x0, y0, x1, y1, b| {
                        tex.paint_stroke(x0, y0, x1, y1, b)
                    });
                }
                frame.key = Some(key);
                frame.live = LiveProgress::default();
            }
            CatchUp::Whole => {
                // A fresh texture holds whatever the driver left in it: clear it first.
                tex.clear();
                for s in &kept(&mut app, page, layer) {
                    paint_from(s, 0, scale, &map, &mut |x0, y0, x1, y1, b| {
                        tex.paint_stroke(x0, y0, x1, y1, b)
                    });
                }
                frame.key = Some(key);
                frame.live = LiveProgress::default();
            }
        }
        frame.live.paint_mapped(live.as_ref(), scale, &map, &mut |x0, y0, x1, y1, b| {
            tex.paint_stroke(x0, y0, x1, y1, b)
        });
        return Some(ImageRef::gl_texture(tex.clone()));
    }

    let mut changed = false;
    let what = if frame.pixels.is_none() {
        CatchUp::Whole
    } else {
        how_to_catch_up(frame, key, append, &mut app, layer)
    };
    match what {
        CatchUp::Same => {}
        CatchUp::KeyOnly => frame.key = Some(key),
        CatchUp::Tail => {
            if let (Some(done), Some(pixels)) = (
                append.and_then(|a| stroke_by_id(&mut app, a.id)),
                frame.pixels.as_mut(),
            ) {
                changed |= frame.live.paint_mapped(Some(&done), scale, &map, &mut |x0, y0, x1, y1, b| {
                    pixels.paint_stroke(x0, y0, x1, y1, b)
                });
            }
            frame.key = Some(key);
            frame.live = LiveProgress::default();
        }
        CatchUp::Whole => {
            frame.pixels = Some(draw_cpu_mapped(w, h, scale, &map, &kept(&mut app, page, layer)));
            frame.key = Some(key);
            frame.live = LiveProgress::default();
            changed = true;
        }
    }
    let pixels = frame.pixels.as_mut()?;
    changed |= frame.live.paint_mapped(live.as_ref(), scale, &map, &mut |x0, y0, x1, y1, b| {
        pixels.paint_stroke(x0, y0, x1, y1, b)
    });
    if changed || frame.shown.is_none() {
        frame.shown = ImageRef::create_rawimage(pixels.clone()).into_option();
    }
    frame.shown.clone()
}

/// What `frame` needs to show the page as it is now ([`catch_up`], with what only the app knows).
fn how_to_catch_up(
    frame: &Frame,
    key: FrameKey,
    append: Option<Append>,
    app: &mut RefAny,
    layer: Layer,
) -> CatchUp {
    if frame.key == Some(key) {
        return CatchUp::Same;
    }
    let appended_on = append.and_then(|a| layer_of_stroke(app, a.id));
    catch_up(
        frame.key,
        key,
        append,
        appended_on,
        layer,
        frame.live.progress().map(|(id, _)| id),
    )
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
        t_ms: 0,
    }
}

#[cfg(test)]
#[path = "ink_test.rs"]
mod ink_test;
