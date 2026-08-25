//! The sheet: file browser, toolbar, paginated code, ink overlay, margin.
//!
//! Light theme and print-quality typography on purpose — the surface should
//! look like the printouts it replaces, because that is the mental model that
//! already works.

use azul::prelude::*;
use azul::callbacks::RenderImageCallbackInfo;
use azul::dom::{IdOrClass, RenderImageCallback};
use azul::vec::IdOrClassVec;
use azul::image::{ImageRef, RawImageFormat};
use azul::vec::U8VecRef;
use azul::image::{RawImage, RawImageData};

use crate::{code, ink, model::Semantic, AppState};

/// Line height in logical px. Shared by the renderer and by the code that
/// infers WHICH LINES a stroke covers — if these two ever disagree, every
/// annotation silently anchors to the wrong place.
pub const LINE_H: f32 = 15.0;
/// Page width, sized for ~110 columns of the mono face.
const PAGE_W: f32 = 760.0;
/// Left gutter for line numbers; ink to the left of this is margin commentary.
const GUTTER_W: f32 = 52.0;
/// Gap between sheets in the strip. Wide enough that the shadow of one page
/// does not read as part of the next.
const PAGE_GAP: f32 = 24.0;

/// One "packet" for the level meter.
///
/// The mic delivers frames far faster than anything can be looked at, so the
/// meter advances per block of samples rather than per frame — the bar moves at
/// a readable rate and the DOM is not rebuilt a hundred times a second.
pub const METER_PACKET_SAMPLES: usize = 4096;

/// How many packets fill the meter before it wraps. Roughly 20 s at 48 kHz -
/// long enough for a spoken remark, short enough that the bar visibly moves.
const METER_SPAN_PACKETS: usize = 240;

fn page_h() -> f32 {
    code::LINES_PER_PAGE as f32 * LINE_H + 40.0
}

/// One page plus its gap: the column stride of the horizontal sheet strip.
fn page_stride() -> f32 {
    PAGE_W + PAGE_GAP
}

/// Which page a pointer event landed on, from the node's dataset.
///
/// The page index is carried on the ink node rather than computed from scroll
/// position: the VirtualView materialises an arbitrary window of pages, so
/// "third page from the top of the viewport" is not stable.
pub fn page_of(info: &mut CallbackInfo) -> Option<usize> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<PageTag>().map(|t| t.page))
}

/// Sample the pointer as an ink point, preferring the pen.
///
/// Falls back to the plain cursor so a finger or a mouse still draws — the app
/// must not be dead on a machine with no tablet attached.
pub fn sample(info: &mut CallbackInfo) -> Option<(crate::model::InkPoint, bool)> {
    if let Some(pen) = info.get_pen_state().into_option() {
        if pen.in_contact {
            return Some((
                ink::point_from(
                    pen.position.x,
                    pen.position.y,
                    pen.pressure,
                    pen.tilt.x_tilt,
                    pen.tilt.y_tilt,
                ),
                pen.is_eraser,
            ));
        }
    }
    let p = info.get_cursor_relative_to_node().into_option()?;
    Some((ink::point_from(p.x, p.y, 0.0, 0.0, 0.0), false))
}

/// Index carried on a clickable row, read back in the callback.
///
/// azul callbacks are plain `extern "C" fn`s and cannot close over a value, so
/// "which item was clicked" travels as a dataset on the node itself.
#[derive(Debug, Clone, Copy)]
pub struct IndexTag {
    pub index: usize,
}

/// The index of the clicked row, if it carries one.
pub fn index_of(info: &mut CallbackInfo) -> Option<usize> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<IndexTag>().map(|t| t.index))
}

/// Marks an ink layer with the page it belongs to.
#[derive(Debug, Clone, Copy)]
pub struct PageTag {
    pub page: usize,
}

pub extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let mut d = data.clone();
    let Some(s) = d.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };

    let mut root = Dom::create_body().with_css(
        "display: flex; flex-direction: row; background: #e9e7e2; font-family: sans-serif;",
    );
    root.add_child(sidebar(&s, &data));

    let mut center = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;");
    center.add_child(toolbar(&s, &data));
    center.add_child(page_rail(&s, &data));
    center.add_child(sheet(&s, &data));
    center.add_child(status_bar(&s));
    root.add_child(center);
    root.add_child(margin_panel(&s, &data));
    root
}

/// File browser. Works against a whole repo or a single directory — the only
/// difference is what `load_tree` skipped.
fn sidebar(s: &AppState, data: &RefAny) -> Dom {
    let mut col = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 250px; flex-shrink: 0; \
         background: #f7f6f3; border-right: 1px solid #cfcbc4; overflow: auto;",
    );
    col.add_child(
        Dom::create_div_with_text("Files")
            .with_css("font-weight: bold; font-size: 12px; padding: 10px; color: #55514a;"),
    );
    for (i, f) in s.files.iter().enumerate() {
        let active = s.current == Some(i);
        let css = if active {
            "padding: 4px 10px; font-size: 11px; background: #dfe7f5; color: #1a1a1a;"
        } else {
            "padding: 4px 10px; font-size: 11px; color: #3c3a36;"
        };
        col.add_child(
            Dom::create_div_with_text(f.display.as_str())
                .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
                .with_css(css)
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    // index is carried by the closure-free indexed callback
                    crate::on_pick_file,
                ),
        );
    }
    col
}

/// Semantic palette. Pad ExpressKeys select the same five, in this order.
fn toolbar(s: &AppState, data: &RefAny) -> Dom {
    let mut bar = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 8px; \
         padding: 8px 12px; background: #f7f6f3; border-bottom: 1px solid #cfcbc4;",
    );
    for (i, sem) in Semantic::ALL.iter().enumerate() {
        let c = sem.color();
        let selected = *sem == s.active;
        let css = format!(
            "padding: 5px 12px; font-size: 12px; border-radius: 4px; \
             background: rgba({},{},{},{}); color: {}; border: {};",
            c.r,
            c.g,
            c.b,
            if selected { "1.0" } else { "0.18" },
            if selected { "#ffffff" } else { "#2b2b2b" },
            if selected { "2px solid #2b2b2b" } else { "1px solid #cfcbc4" },
        );
        bar.add_child(
            Dom::create_div_with_text(format!("{}  [{}]", sem.label(), i + 1).as_str())
                .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
                .with_css(css.as_str())
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    crate::on_pick_semantic,
                ),
        );
    }
    // Which nib is in hand. A readout, not a control: the tool changes by
    // clicking on the PAGE, so that the hand never leaves it. Showing it in the
    // toolbar anyway is the only way to know what a click just switched to.
    bar.add_child(
        Dom::create_div_with_text(format!("nib: {}  (click page to cycle)", s.tool.label()).as_str())
            .with_css(
                "margin-left: 16px; padding: 5px 12px; font-size: 12px; border-radius: 4px; \
                 background: #ffffff; color: #2b2b2b; border: 1px dashed #a9a49b;",
            ),
    );

    let rec = s.recording.is_some();
    if rec {
        bar.add_child(meter(s));
    }
    bar.add_child(
        Dom::create_div_with_text(if rec { "◉ recording" } else { "○ record" })
            .with_css(if rec {
                "margin-left: 10px; padding: 5px 12px; font-size: 12px; border-radius: 4px; \
                 background: #d62d20; color: white;"
            } else {
                "margin-left: auto; padding: 5px 12px; font-size: 12px; border-radius: 4px; \
                 background: #ffffff; color: #2b2b2b; border: 1px solid #cfcbc4;"
            })
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                crate::on_toggle_record,
            ),
    );
    bar
}

/// PCM packets arriving, as a bar.
///
/// Deliberately a COUNT of packets rather than an amplitude meter: what needs
/// confirming while speaking is that audio is still being captured, and a level
/// meter answers that only while there is sound — it reads identically for
/// "silent" and "the mic died", which is the failure this exists to catch.
fn meter(s: &AppState) -> Dom {
    let packets = s.level_samples / METER_PACKET_SAMPLES;
    let filled = (packets % METER_SPAN_PACKETS) as f32 / METER_SPAN_PACKETS as f32;
    let mut wrap = Dom::create_div().with_css(
        "margin-left: auto; display: flex; flex-direction: row; align-items: center; gap: 6px;",
    );
    wrap.add_child(
        Dom::create_div_with_text(format!("{packets} pkt").as_str())
            .with_css("font-size: 11px; color: #6b665e; font-family: monospace;"),
    );
    let mut holder = Dom::create_div().with_css("width: 160px;");
    holder.add_child(ProgressBar::create(filled * 100.0).dom());
    wrap.add_child(holder);
    wrap
}

/// DOM id of the sheet strip, so a page button can scroll it.
///
/// A jump has to move the SCROLL CONTAINER, and the button is nowhere near it
/// in the tree — an id is how a callback names a node it did not hit.
pub const STRIP_ID: &str = "sheet-strip";

/// Page numbers as a horizontal button row: `… 49 50 51 52 …`.
///
/// # Why numbers and not a scrollbar
///
/// The thing worth remembering about a review is "FooItem was on page 50, near
/// the top". A scrollbar gives a fraction, which nobody memorises; a page
/// number is a stable name for a place, and printing it above the sheet is what
/// makes that recall possible at all.
///
/// The whole row is built — a number is a few glyphs, and unlike the sheets
/// there is nothing here worth virtualising.
fn page_rail(s: &AppState, data: &RefAny) -> Dom {
    let mut rail = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 3px; \
         padding: 4px 12px; background: #efede8; border-bottom: 1px solid #cfcbc4; \
         overflow-x: auto; overflow-y: hidden; flex-shrink: 0;",
    );
    let Some(file) = s.file() else { return rail };
    for page in 0..file.page_count() {
        let here = page == s.visible_page;
        let css = if here {
            "padding: 2px 9px; font-size: 11px; font-family: monospace; border-radius: 3px; \
             background: #2b2b2b; color: #ffffff; flex-shrink: 0;"
        } else {
            "padding: 2px 9px; font-size: 11px; font-family: monospace; border-radius: 3px; \
             background: #ffffff; color: #55514a; border: 1px solid #d8d4cd; flex-shrink: 0;"
        };
        rail.add_child(
            // Page numbers are 1-based on the button, 0-based in the model —
            // the printout it imitates has no page zero.
            Dom::create_div_with_text(format!("{}", page + 1).as_str())
                .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: page })))
                .with_css(css)
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    crate::on_jump_to_page,
                ),
        );
    }
    rail
}

/// Scroll the strip so `page` is the leftmost sheet.
pub fn scroll_to_page(info: &mut CallbackInfo, page: usize) {
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, STRIP_ID);
    info.scroll_to(
        dom,
        node,
        LogicalPosition::create(page as f32 * page_stride(), 0.0),
    );
}

/// The paginated code as a HORIZONTAL strip of sheets.
///
/// # Why sideways
///
/// Code is already tall and narrow; stacking pages vertically means the same
/// scroll gesture serves two different jobs — moving within a page and moving
/// between pages — and the boundary between them is invisible. Laid out
/// sideways a page is one screen, and paging is a distinct movement, which is
/// what makes a stack of printouts navigable in a way a scroll never is.
///
/// # Why a VirtualView
///
/// Same reason AzWriter uses one: a 4000-line file is ~90 sheets, and building
/// them all to show three would style and lay out every line of the file on
/// every pointer move. The callback materialises the window around the scroll
/// position and declares the rest as scrollbar geometry.
fn sheet(s: &AppState, data: &RefAny) -> Dom {
    let mut area = Dom::create_div().with_css(
        // `hidden`, not `auto`: the VirtualView node below IS the scroll
        // container. An `auto` wrapper would also claim a scroll id and eat the
        // wheel events the VirtualView needs to page.
        "flex-grow: 1; min-height: 0px; background: #e9e7e2; display: flex; \
         flex-direction: column; overflow: hidden; padding: 18px;",
    );
    if s.file().is_none() {
        area.add_child(
            Dom::create_div_with_text("Open a file to begin")
                .with_css("color: #7a756c; padding: 40px;"),
        );
        return area;
    }
    area.add_child(
        Dom::create_virtual_view(
            RefAny::new(SheetStrip { app: data.clone() }),
            sheets_virtual_view,
        )
        .with_ids_and_classes(IdOrClassVec::from_item(IdOrClass::id(STRIP_ID))),
    );
    area
}

/// Payload of the sheet strip's `VirtualView`.
///
/// Only the app handle: everything the callback needs (which file, which ink)
/// already lives in `AppState`, and a second copy here could go stale between
/// a DOM rebuild and the next materialisation.
struct SheetStrip {
    app: RefAny,
}

/// Materialise the pages around the horizontal scroll position.
extern "C" fn sheets_virtual_view(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    // Two handles to the same state, on purpose: reading it needs a borrow that
    // outlives the loop, and the page callbacks need a handle to clone from
    // while that borrow is live. One variable cannot be both.
    let mut app = {
        let Some(strip) = data.downcast_ref::<SheetStrip>() else {
            return VirtualViewReturn::default();
        };
        strip.app.clone()
    };
    let cb_app = app.clone();

    let stride = page_stride();
    let strip_h = page_h();

    let (dom, first, count, total, leftmost) = {
        let Some(s) = app.downcast_ref::<AppState>() else {
            return VirtualViewReturn::default();
        };
        let Some(file) = s.file() else {
            return VirtualViewReturn::default();
        };
        let total = file.page_count().max(1);

        // One page of overscan on each side: paging is a fast gesture, and a
        // sheet that only starts existing once its edge is on screen arrives
        // visibly late.
        let viewport_w = info.bounds.get_logical_size().width;
        // Round, not truncate: the page filling most of the viewport is the one
        // the reader would call "the page they are on", and truncating names
        // the page that is one pixel from having scrolled away.
        let leftmost = ((info.scroll_offset.x.max(0.0) / stride).round() as usize).min(total - 1);
        let first_visible = (info.scroll_offset.x.max(0.0) / stride) as usize;
        let first = first_visible.saturating_sub(1);
        let visible = (viewport_w / stride).ceil() as usize + 2;
        let count = visible.max(2).min(total.saturating_sub(first));

        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: flex-start;");
        for page in first..first + count {
            row.add_child(page_sheet(&s, &cb_app, file, page));
        }
        (row, first, count, total, leftmost)
    };

    // Tell the rail which number to highlight. Written here rather than derived
    // in `layout` because the scroll offset lives in the engine: this callback
    // is the only place in the app that ever sees it.
    //
    // The rail therefore lags the strip by one DOM build. Forcing a rebuild
    // from here would be worse — every frame of a scroll would regenerate the
    // whole tree to move one highlight.
    if let Some(mut s) = app.downcast_mut::<AppState>() {
        s.visible_page = leftmost;
    }

    VirtualViewReturn::with_dom(
        dom,
        // Where the window we just built sits in the strip.
        LogicalRect::create(
            LogicalPosition::create(first as f32 * stride, 0.0),
            LogicalSize::create(count as f32 * stride, strip_h),
        ),
        // The whole file — scrollbar geometry only.
        LogicalRect::create(
            LogicalPosition::create(0.0, 0.0),
            LogicalSize::create(total as f32 * stride, strip_h),
        ),
    )
}

fn page_sheet(s: &AppState, data: &RefAny, file: &code::SourceFile, page: usize) -> Dom {
    let (first_line, lines) = file.page(page);
    let mut sheet = Dom::create_div().with_css(format!(
        "position: relative; width: {}px; height: {}px; background: #ffffff; \
         border: 1px solid #b9b4ab; box-shadow: 0px 1px 4px #00000030; \
         margin-right: {}px; flex-shrink: 0; box-sizing: border-box; \
         padding: 20px; overflow: hidden;",
        PAGE_W as isize,
        page_h() as isize,
        PAGE_GAP as isize,
    ).as_str());

    // Code, one row per line, gutter then text.
    let mut col = Dom::create_div().with_css("display: flex; flex-direction: column;");
    for (i, line) in lines.iter().enumerate() {
        let mut row = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: row; height: {LINE_H}px;"
        ).as_str());
        row.add_child(
            Dom::create_div_with_text(format!("{}", first_line + i).as_str()).with_css(format!(
                "width: {}px; flex-shrink: 0; text-align: right; padding-right: 10px; \
                 font-family: monospace; font-size: 11px; color: #b0aaa0;",
                GUTTER_W as isize - 10,
            ).as_str()),
        );
        row.add_child(
            Dom::create_div_with_text(line.as_str()).with_css(
                "font-family: monospace; font-size: 11px; color: #1f1f1f; white-space: pre;",
            ),
        );
        col.add_child(row);
    }
    sheet.add_child(col);

    // The ink layer sits ON TOP of the code and receives the pointer, so the
    // whole page is drawable including the margins — marginalia is where half
    // the review lives.
    let page_strokes: Vec<_> = s.strokes.iter().filter(|st| st.page == page).collect();
    let has_live = s.live.as_ref().is_some_and(|l| l.page == page);
    let cache = RefAny::new(InkLayer {
        strokes: page_strokes.into_iter().cloned().collect(),
        live: if has_live { s.live.clone() } else { None },
    });
    sheet.add_child(
        Dom::create_image(ImageRef::callback(
            RenderImageCallback::create(render_ink).to_core(),
            cache,
        ))
        .with_dataset(OptionRefAny::Some(RefAny::new(PageTag { page })))
        .with_css(format!(
            "position: absolute; top: 0px; left: 0px; width: {}px; height: {}px;",
            PAGE_W as isize,
            page_h() as isize,
        ).as_str())
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            data.clone(),
            crate::on_ink_down,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseOver),
            data.clone(),
            crate::on_ink_move,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            data.clone(),
            crate::on_ink_up,
        ),
    );
    sheet
}

/// Per-page ink handed to the render callback.
struct InkLayer {
    strokes: Vec<crate::model::Stroke>,
    live: Option<crate::model::Stroke>,
}

extern "C" fn render_ink(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    // Logical, not physical: the stroke model is in logical px (that is what
    // `PenState.position` reports), so rasterising at physical size would
    // paint every dab at half coordinates on a 2x display.
    let size = info.get_bounds().get_logical_size();
    let (w, h) = (size.width.max(1.0) as u32, size.height.max(1.0) as u32);
    let Some(layer) = data.downcast_ref::<InkLayer>() else {
        return ImageRef::null_image(w as usize, h as usize, RawImageFormat::RGBA8, U8VecRef::from(&[][..]));
    };
    let mut all: Vec<&crate::model::Stroke> = layer.strokes.iter().collect();
    if let Some(l) = layer.live.as_ref() {
        all.push(l);
    }
    let buf = ink::rasterize_page(&all, w, h);
    let img = RawImage {
        pixels: RawImageData::U8(buf.into()),
        width: w as usize,
        height: h as usize,
        // The ink is composited source-over onto a transparent sheet, so the
        // colour channels are NOT premultiplied.
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    ImageRef::new_rawimage(img).into_option().unwrap_or_else(|| {
        ImageRef::null_image(w as usize, h as usize, RawImageFormat::RGBA8, U8VecRef::from(&[][..]))
    })
}

/// Derived findings, plus the open-questions queue.
fn margin_panel(s: &AppState, _data: &RefAny) -> Dom {
    let mut col = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 300px; flex-shrink: 0; \
         background: #f7f6f3; border-left: 1px solid #cfcbc4; overflow: auto;",
    );
    col.add_child(
        Dom::create_div_with_text("Derived findings")
            .with_css("font-weight: bold; font-size: 12px; padding: 10px; color: #55514a;"),
    );
    if s.findings.is_empty() {
        col.add_child(
            Dom::create_div_with_text("Draw on the code — findings appear here.")
                .with_css("font-size: 11px; color: #8a857c; padding: 0px 10px 10px 10px;"),
        );
    }
    for f in &s.findings {
        let c = f.semantic.color();
        let mut card = Dom::create_div().with_css(format!(
            "margin: 6px 10px; padding: 8px; background: white; border-radius: 4px; \
             border-left: 4px solid rgb({},{},{});",
            c.r, c.g, c.b
        ).as_str());
        card.add_child(
            Dom::create_div_with_text(format!(
                "{}  L{}-{}",
                f.semantic.label(),
                f.first_line,
                f.last_line
            ).as_str())
            .with_css("font-size: 11px; font-weight: bold; color: #2b2b2b;"),
        );
        if let Some(v) = &f.voice_note {
            card.add_child(
                Dom::create_div_with_text(v.as_str())
                    .with_css("font-size: 10px; color: #6b665e; margin-top: 4px;"),
            );
        }
        col.add_child(card);
    }

    // The "?" queue: every question-shaped mark, gathered so none is lost.
    let questions: Vec<_> = s
        .findings
        .iter()
        .filter(|f| f.semantic == Semantic::Question)
        .collect();
    if !questions.is_empty() {
        col.add_child(
            Dom::create_div_with_text(format!("Open questions ({})", questions.len()).as_str())
                .with_css("font-weight: bold; font-size: 12px; padding: 12px 10px 4px 10px; color: #55514a;"),
        );
        for q in questions {
            col.add_child(
                Dom::create_div_with_text(format!("? {}:{}", q.file, q.first_line).as_str())
                    .with_css("font-size: 11px; color: #205cd6; padding: 2px 10px;"),
            );
        }
    }
    col
}

fn status_bar(s: &AppState) -> Dom {
    let pages = s.file().map_or(0, code::SourceFile::page_count);
    let clips = s.clips.len() + usize::from(s.recording.is_some());
    let mut bar = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 16px; \
         padding: 5px 12px; font-size: 11px; color: #55514a; background: #f7f6f3; \
         border-top: 1px solid #cfcbc4;",
    );
    bar.add_child(Dom::create_div_with_text(
        format!("{pages} sheets  ·  {} strokes  ·  {clips} clips", s.strokes.len()).as_str(),
    ));
    bar.add_child(Dom::create_div_with_text(s.status.as_str()).with_css("margin-left: auto;"));
    bar
}
