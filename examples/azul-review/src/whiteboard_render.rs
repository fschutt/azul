//! The picture of a cutout, drawn by azul's CPU renderer at the scale of the window it is made in.
//!
//! The page's code is not cut out of a screenshot: the pages live in a virtual view (a crop of the
//! window would fall outside the picture of it), a screenshot is of the whole window and would show
//! what is over the page when it is taken, and it is made on the window's thread. The lines of the
//! page are laid out again - in the same box, with the same type, by the same engine (the call the
//! reader draws its pages ahead of time with, `Pdf::render_pages_with_policy`) - on a Thread, at the
//! window's pixel scale, on a clear ground. The ink is painted over it with the engine's own brush at
//! the same scale, and `whiteboard_cutout::compose` puts the layers together in the order the page shows
//! them (paper, the marker's ink under the code, the code, the pen's ink over it) and cuts them out along
//! the lasso.
//!
//! The lines are drawn as `ui::code_row` draws them (keep the two in step; the numbers they share,
//! the line's height and the page's padding, are `ui`'s).

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    css::ColorU,
    dom::{Dom, StyledDom},
    font::FontCacheSnapshot,
    image::{Brush, ImageCacheSnapshot, RawImage, RawImageData},
    pdf::{BreakPolicy, Pdf},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};

use crate::{
    ink::{self, Layer},
    model::Stroke,
    ui::{LINE_H, PAGE_PAD},
    whiteboard::CutoutImage,
    whiteboard_archive::encode_png,
    whiteboard_cutout::{compose, CapturePlan, Raster},
    whiteboard_geom::Pt,
};

/// The width of the line numbers' column, px (`ui`'s gutter).
pub const GUTTER_W: f32 = 52.0;
/// The width of the column of code on a page, px (the page without its margin column and its padding).
pub const COLUMN_W: f32 = 708.0;
/// The paper's colour.
pub const PAPER: [u8; 4] = [255, 255, 255, 255];

/// What is drawn: the plan, the lines of the page the loop is on, and the page's ink.
pub struct RenderRequest {
    pub plan: CapturePlan,
    /// The number (from 1) of the page's first line.
    pub first_number: u32,
    /// The lines of the page.
    pub lines: Vec<String>,
    /// The page's kept strokes (the marker's and the pen's).
    pub strokes: Vec<Stroke>,
    /// The window's fonts and images.
    pub fonts: FontCacheSnapshot,
    pub images: ImageCacheSnapshot,
}

/// One line of code as the page shows it: its number in the gutter, then the line.
pub fn code_row(number: u32, line: &str) -> Dom {
    let mut row = Dom::create_div().with_css(format!("display: flex; flex-direction: row; height: {LINE_H}px;").as_str());
    row.add_child(
        Dom::create_span_with_text(format!("{number}").as_str()).with_css(
            format!(
                "width: {}px; flex-shrink: 0; text-align: right; padding-right: 10px; \
                 font-family: monospace; font-size: 11px; color: #b0aaa0;",
                GUTTER_W as isize - 10
            )
            .as_str(),
        ),
    );
    row.add_child(
        Dom::create_span_with_text(line)
            .with_css("font-family: monospace; font-size: 11px; color: #1f1f1f; white-space: pre;"),
    );
    row
}

/// The part of the page the plan draws: the page's column of code, moved so that the region's corner
/// is the box's own, clipped to the box.
fn region_dom(req: &RenderRequest) -> Dom {
    let r = req.plan.region;
    let mut column = Dom::create_div().with_css(
        format!(
            "position: absolute; left: {}px; top: {}px; width: {COLUMN_W}px; display: flex; \
             flex-direction: column; user-select: none;",
            PAGE_PAD - r.x,
            PAGE_PAD - r.y
        )
        .as_str(),
    );
    for (i, line) in req.lines.iter().enumerate() {
        column.add_child(code_row(req.first_number + u32::try_from(i).unwrap_or(0), line));
    }
    Dom::create_div()
        .with_css(
            format!("position: relative; width: {}px; height: {}px; overflow: hidden; margin: 0px; padding: 0px;", r.w, r.h)
                .as_str(),
        )
        .with_child(column)
}

/// The pixels of an image the renderer made (RGBA8, multiplied by alpha).
fn raster_of(image: &RawImage) -> Result<Raster, String> {
    let RawImageData::U8(bytes) = &image.pixels else {
        return Err("the renderer made a picture of a kind the board does not read".to_string());
    };
    let (w, h) = (
        u32::try_from(image.width).map_err(|_| "the picture is too wide".to_string())?,
        u32::try_from(image.height).map_err(|_| "the picture is too tall".to_string())?,
    );
    Raster::from_premultiplied(w, h, bytes.as_ref().to_vec()).ok_or_else(|| "the picture's bytes do not fit its size".to_string())
}

/// The code, on a clear ground, at the plan's scale.
fn code_layer(req: &mut RenderRequest) -> Result<Raster, String> {
    let r = req.plan.region;
    let dom = region_dom(req);
    let fonts = std::mem::replace(&mut req.fonts, FontCacheSnapshot::empty());
    let images = std::mem::replace(&mut req.images, ImageCacheSnapshot::empty());
    let one_page = BreakPolicy {
        max_push_distance: 0.0,
        honor_break_inside: false,
        widows_orphans: false,
        atomic_lines: false,
        atomic_table_rows: false,
        repeat_table_headers: false,
    };
    let image = Pdf::create()
        .render_pages_with_policy(
            StyledDom::create_from_dom(dom),
            r.w,
            r.h,
            fonts,
            images,
            one_page,
            0,
            1,
            req.plan.scale,
            ColorU { r: 0, g: 0, b: 0, a: 0 },
        )
        .into_option()
        .ok_or_else(|| "the code could not be drawn".to_string())?;
    raster_of(&image)
}

/// `b` for a picture `k` times as sharp: the nib is as wide on the page, so it is `k` times as many pixels.
fn sharper(mut b: Brush, k: f32) -> Brush {
    b.radius *= k;
    b
}

/// Paints stroke `s` (in the page's px) into `image`, `scale` device pixels to a page px, the page's corner
/// at `origin` of the picture's own px (the picture of a region has the region's corner there): the engine's
/// brush as the page's ink layer paints it, its nib `scale` times as wide in pixels.
pub fn paint_stroke(image: &mut RawImage, scale: f32, origin: Pt, s: &Stroke) {
    let px = |x: f32, y: f32| ((x - origin.x) * scale, (y - origin.y) * scale);
    match s.points.as_slice() {
        [] => {}
        [p] => {
            let (x, y) = px(p.x, p.y);
            image.paint_stroke(x, y, x, y, sharper(ink::brush(s.semantic, p.pressure), scale));
        }
        points => {
            for w in points.windows(2) {
                let ((x0, y0), (x1, y1)) = (px(w[0].x, w[0].y), px(w[1].x, w[1].y));
                let b = ink::brush(s.semantic, (w[0].pressure + w[1].pressure) * 0.5);
                image.paint_stroke(x0, y0, x1, y1, sharper(b, scale));
            }
        }
    }
}

/// The strokes of one layer, painted with the engine's brush at the plan's scale on a clear picture.
fn ink_layer(plan: &CapturePlan, strokes: &[&Stroke]) -> Result<Option<Raster>, String> {
    if strokes.is_empty() {
        return Ok(None);
    }
    let mut image = RawImage::create_rgba8(plan.px_w, plan.px_h, vec![0_u8; plan.px_w as usize * plan.px_h as usize * 4], true);
    for s in strokes {
        paint_stroke(&mut image, plan.scale, plan.region.origin(), s);
    }
    raster_of(&image).map(Some)
}

/// Draws the cutout's picture: the layers, composed and cut along the lasso, with its PNG.
///
/// # Errors
/// A sentence for what could not be drawn (the code, the picture's size, the PNG).
pub fn render(request: RenderRequest) -> Result<CutoutImage, String> {
    let mut request = request;
    let code = code_layer(&mut request)?;
    let marker = ink_layer(&request.plan, &on_layer(&request.strokes, Layer::Marker))?;
    let pen = ink_layer(&request.plan, &on_layer(&request.strokes, Layer::Pen))?;
    let picture = compose(&request.plan, PAPER, marker.as_ref(), &code, pen.as_ref())?;
    let png = encode_png(picture.w, picture.h, &picture.to_straight())?;
    Ok(picture.into_image(request.plan.scale, png))
}

/// The strokes of `layer`.
fn on_layer(strokes: &[Stroke], layer: Layer) -> Vec<&Stroke> {
    strokes.iter().filter(|s| Layer::of(s.semantic) == layer).collect()
}

/// What the Thread is given.
struct RenderInit {
    request: Option<RenderRequest>,
    id: u64,
    on_done: WriteBackCallbackType,
}

/// The answer: which cutout the picture is for, and the picture or why not.
pub struct Rendered {
    pub id: u64,
    pub image: Result<CutoutImage, String>,
}

extern "C" fn render_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((request, id, on_done)) = init
        .downcast_mut::<RenderInit>()
        .and_then(|mut i| Some((i.request.take()?, i.id, i.on_done)))
    else {
        return;
    };
    let image = render(request);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(Rendered { id, image })),
    )));
}

/// Draws cutout `id`'s picture on an azul `Thread`; `on_done(app, message, info)` gets it on the UI thread
/// ([`take_rendered`]).
pub fn spawn_render(info: &mut CallbackInfo, app: &RefAny, id: u64, request: RenderRequest, on_done: WriteBackCallbackType) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(RenderInit { request: Some(request), id, on_done }), app.clone(), render_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_rendered(msg: &mut RefAny) -> Option<Rendered> {
    let mut guard = msg.downcast_mut::<Option<Rendered>>()?;
    guard.take()
}

#[cfg(test)]
#[path = "whiteboard_render_test.rs"]
mod whiteboard_render_test;
