//! The whiteboard on the pages: a loop drawn with the pen arms the lines it goes round; the armed lines are taken to the board
//! by their grip (pressed and carried to the bar at the right, or pressed inside the loop and held), or by the key beside
//! them that sends them without any dragging; the board draws their picture on a Thread and keeps the source they came from.
//!
//! The pointer's side of it is the whiteboard owner's state machine (`whiteboard_app`, `whiteboard_drag`); this is what
//! the review window's ink handlers do with its answers: the press that is a grip and no stroke, the press that is held
//! and no stroke, the carry over the page and the bar, the drop. The armed loops are drawn on a layer of their own over the
//! page's code and under the pen's layer (so they never take a stroke): the outline, the grip, and the ring round the grip
//! that runs out as the time does. The words and the key beside a loop are over the pen's layer, which they take no stroke from.
//!
//! The geometry is pure ([`overlay_segments`], [`ghost_segments`]): segments in the page's px as it is shown, which a few lines
//! of glue put on an image with the engine's brush.

use std::time::Instant;

use azul::{
    callbacks::{RenderImageCallbackInfo, WriteBackCallbackType},
    css::ColorU,
    dom::{AccessibilityInfo, AccessibilityRole, RenderImageCallback},
    image::{Brush, ImageRef, RawImage, RawImageFormat},
    prelude::*,
    str::String as AzString,
    vec::U8VecRef,
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    layout,
    line_map::LineMap,
    ui,
    whiteboard::{Notice},
    whiteboard_app::SourceFacts,
    whiteboard_drag::{ArmedRegion, Motion, Pointer, Press, RegionId, Release, GRIP_PX},
    whiteboard_geom::{Pt, Size},
    whiteboard_glue::{bar_of, chip_place, page_chars, page_lines, CHIP_W},
    whiteboard_plug::{self as plug, with_glue},
    whiteboard_render::{self, RenderRequest},
    whiteboard_ui, AppState,
};

const ACCENT: [u8; 4] = [32, 92, 214, 255];
const OUTLINE: [u8; 4] = [32, 92, 214, 190];
const RING_TRACK: [u8; 4] = [32, 92, 214, 70];
const GHOST: [u8; 4] = [32, 92, 214, 235];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// The id of a page's layer of armed loops.
#[must_use]
pub fn overlay_id(page: usize) -> String {
    format!("__azreview_wb-overlay-{page}")
}

/// The id of the words and the key by an armed loop.
#[must_use]
pub fn chip_id(region: RegionId) -> String {
    format!("__azreview_wb-chip-{region}")
}

/// The id of the key that sends an armed loop to the board.
#[must_use]
pub fn send_id(region: RegionId) -> String {
    format!("__azreview_wb-send-{region}")
}

// ==== the geometry ====

/// A piece of line to draw: its two ends in the page's px as it is shown, the half width of its nib, its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub a: Pt,
    pub b: Pt,
    pub radius: f32,
    pub rgba: [u8; 4],
}

fn seg(a: Pt, b: Pt, radius: f32, rgba: [u8; 4]) -> Seg {
    Seg { a, b, radius, rgba }
}

fn shown(map: &LineMap, p: Pt) -> Pt {
    Pt::new(p.x, map.to_display(p.y))
}

fn polyline(points: &[Pt], radius: f32, rgba: [u8; 4], out: &mut Vec<Seg>) {
    for w in points.windows(2) {
        out.push(seg(w[0], w[1], radius, rgba));
    }
}

/// An arc of the circle round `c`: `share` (0 to 1) of it from the top, clockwise, in at least two pieces.
#[must_use]
pub fn ring(c: Pt, radius: f32, share: f32, width: f32, rgba: [u8; 4]) -> Vec<Seg> {
    let share = if share.is_finite() { share.clamp(0.0, 1.0) } else { 0.0 };
    if share <= 0.0 {
        return Vec::new();
    }
    let steps = ((share * 48.0).ceil() as usize).max(2);
    let points: Vec<Pt> = (0..=steps)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + share * std::f32::consts::TAU * (i as f32 / steps as f32);
            Pt::new(c.x + radius * a.cos(), c.y + radius * a.sin())
        })
        .collect();
    let mut out = Vec::new();
    polyline(&points, width, rgba, &mut out);
    out
}

/// What an armed loop looks like on its page: its outline, its grip (a disc with an arrow in it), and the ring round the grip
/// that is as much of a circle as there is time left. All through `map`: the lines the loop goes round are where they are shown.
#[must_use]
pub fn overlay_segments(region: &ArmedRegion, now_ms: u64, map: &LineMap) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut outline: Vec<Pt> = region.polygon.iter().map(|p| shown(map, *p)).collect();
    if let Some(first) = outline.first().copied() {
        outline.push(first);
    }
    polyline(&outline, 1.2, OUTLINE, &mut out);
    let c = shown(map, region.grip.center());
    // the grip is a disc: a piece of no length with a fat nib
    out.push(seg(c, c, GRIP_PX * 0.5, ACCENT));
    // an arrow pointing right, in white: "take this along"
    out.push(seg(Pt::new(c.x - 5.0, c.y), Pt::new(c.x + 5.0, c.y), 1.2, WHITE));
    out.push(seg(Pt::new(c.x + 1.0, c.y - 4.0), Pt::new(c.x + 5.0, c.y), 1.2, WHITE));
    out.push(seg(Pt::new(c.x + 1.0, c.y + 4.0), Pt::new(c.x + 5.0, c.y), 1.2, WHITE));
    let radius = GRIP_PX * 0.5 + 5.0;
    out.extend(ring(c, radius, 1.0, 1.4, RING_TRACK));
    out.extend(ring(c, radius, region.fraction_left(now_ms), 1.8, ACCENT));
    out
}

/// The outline a lifted loop is carried by: the loop's outline moved by `offset` (px of the window; the page is shown at the
/// scale of the window).
#[must_use]
pub fn ghost_segments(polygon: &[Pt], offset: Pt, map: &LineMap) -> Vec<Seg> {
    let mut carried: Vec<Pt> = polygon.iter().chain(polygon.first()).map(|p| shown(map, *p)).collect();
    for p in &mut carried {
        *p = Pt::new(p.x + offset.x, p.y + offset.y);
    }
    let mut out = Vec::new();
    polyline(&carried, 2.0, GHOST, &mut out);
    out
}

fn brush(rgba: [u8; 4], radius: f32) -> Brush {
    let mut b = Brush::create(ColorU { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }, radius);
    b.hardness = 0.85;
    b.flow = 1.0;
    b.spacing = 0.5;
    b
}

/// The segments on an image at `k` device pixels to a px.
fn paint(image: &mut RawImage, k: f32, segments: &[Seg]) {
    for s in segments {
        image.paint_stroke(s.a.x * k, s.a.y * k, s.b.x * k, s.b.y * k, brush(s.rgba, s.radius * k));
    }
}

// ==== what is on a page ====

/// Whether a page has anything of the whiteboard to show over its code: a loop that is armed, or one that is carried.
#[must_use]
pub fn page_has_regions(s: &AppState, page: usize) -> bool {
    s.wb.try_borrow().is_ok_and(|g| g.panel.wb.armed.all().iter().any(|r| r.page == page))
}

/// The layer of armed loops of `page`: an image the size of the page, drawn when the window asks (`render_overlay`).
#[must_use]
pub fn overlay(data: &RefAny, page: usize, height: f32) -> Dom {
    let canvas = RefAny::new(OverlayCanvas { app: data.clone(), page });
    Dom::create_image(ImageRef::callback(RenderImageCallback::create(render_overlay).to_core(), canvas))
        .with_id(AzString::from(overlay_id(page)))
        .with_accessibility_info(AccessibilityInfo { role: AccessibilityRole::Nothing, ..AccessibilityInfo::create_default() })
        .with_css(format!("position: absolute; top: 0px; left: 0px; width: {}px; height: {}px;", ui::PAGE_W as isize, height as isize).as_str())
}

struct OverlayCanvas {
    app: RefAny,
    page: usize,
}

extern "C" fn render_overlay(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    let bounds = info.get_bounds();
    let (physical, logical) = (bounds.get_physical_size(), bounds.get_logical_size());
    let (w, h) = (physical.width.max(1), physical.height.max(1));
    let blank = || ImageRef::null_image(w as usize, h as usize, RawImageFormat::RGBA8, U8VecRef::from(&[][..]));
    let Some((mut app, page)) = data.downcast_ref::<OverlayCanvas>().map(|c| (c.app.clone(), c.page)) else {
        return blank();
    };
    let k = if logical.width > 0.0 { w as f32 / logical.width } else { 1.0 };
    let Some(state) = app.downcast_ref::<AppState>() else {
        return blank();
    };
    let Ok(glue) = state.wb.try_borrow() else {
        return blank();
    };
    let map = state.page_map(page);
    let now = glue.panel.now_ms();
    let mut segments = Vec::new();
    for r in glue.panel.wb.armed.all().iter().filter(|r| r.page == page && now < r.expires_at_ms()) {
        segments.extend(overlay_segments(r, now, &map));
    }
    if let Some(ghost) = glue.panel.wb.drag.ghost() {
        if let Some(r) = glue.panel.wb.armed.get(ghost.region).filter(|r| r.page == page) {
            segments.extend(ghost_segments(&r.polygon, ghost.offset(), &map));
        }
    }
    let mut image = RawImage::create_rgba8(w, h, vec![0_u8; w as usize * h as usize * 4], true);
    paint(&mut image, k, &segments);
    ImageRef::create_rawimage(image).into_option().unwrap_or_else(blank)
}

/// The words and the key by each armed loop of `page`, over its code (`height`: the page as it is shown).
#[must_use]
pub fn chips(s: &AppState, data: &RefAny, page: usize, map: &LineMap, height: f32) -> Vec<Dom> {
    let Ok(glue) = s.wb.try_borrow() else {
        return Vec::new();
    };
    let Some(file) = s.file() else {
        return Vec::new();
    };
    let now = glue.panel.now_ms();
    glue.panel
        .wb
        .armed
        .all()
        .iter()
        .filter(|r| r.page == page && now < r.expires_at_ms())
        .map(|r| chip(r, data, &file.display, now, map, height, glue.tip_for == Some(r.id)))
        .collect()
}

/// What the key that sends a loop carries.
struct RegionTag {
    app: RefAny,
    id: RegionId,
}

fn chip(r: &ArmedRegion, data: &RefAny, file: &str, now_ms: u64, map: &LineMap, height: f32, tip: bool) -> Dom {
    let at = chip_place(r.bounds, map, Size::new(ui::PAGE_W, height));
    let (first, last) = (r.first_line as usize, r.last_line as usize);
    let armed_name = t_args(
        "azreview-whiteboard-armed-name",
        &[("first", Arg::from(first)), ("last", Arg::from(last)), ("seconds", Arg::from(r.seconds_left(now_ms) as usize))],
    );
    let send_name = t_args(
        "azreview-whiteboard-send-name",
        &[("file", Arg::from(file.to_string())), ("first", Arg::from(first)), ("last", Arg::from(last))],
    );
    let tag = RefAny::new(RegionTag { app: data.clone(), id: r.id });
    let mut words = Dom::create_div()
        .with_id(AzString::from(chip_id(r.id)))
        .with_accessibility_info(AccessibilityInfo::named(AzString::from(armed_name), AccessibilityRole::Grouping))
        .with_css(
            format!(
                "position: absolute; left: {:.0}px; top: {:.0}px; width: {}px; display: flex; flex-direction: column; gap: 5px; \
                 padding: 6px 8px; box-sizing: border-box; background: #fffbe8; border: 1px solid #d9bd62; border-radius: 6px; \
                 box-shadow: 0px 1px 4px #00000030; font-size: 11px; line-height: 1.3; color: #3d3a30;",
                at.x, at.y, CHIP_W as isize
            )
            .as_str(),
        )
        .with_child(
            Dom::create_span_with_text(format!("{} \u{2192}", t("azreview-whiteboard-grip")).as_str())
                .with_css("font-weight: 600; color: #1f4fb0;"),
        )
        .with_child(
            ui::key(
                // the card is pale in both modes: the key has a face of its own, white on blue
                "align-self: flex-start; padding: 0px 10px; background: #205cd6; border: 1px solid #1a4cb0; color: #ffffff; \
                 font-weight: 600; :hover { background: #1a4cb0; }",
                AzString::from(send_name),
                tag,
                on_send,
            )
            .with_id(AzString::from(send_id(r.id)))
            .with_child(Dom::create_icon("dashboard").with_css("font-size: 13px; color: #ffffff;"))
            .with_child(Dom::create_span_with_text(label("azreview-whiteboard-send")).with_css("color: #ffffff;")),
        );
    if tip {
        words.add_child(
            Dom::create_span_with_text(t("azreview-whiteboard-tip").as_str()).with_css("font-size: 10px; color: #6b6450;"),
        );
    }
    words
}

/// The key of a loop was pressed: the lines go to the board, which opens if it was closed.
extern "C" fn on_send(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data.downcast_ref::<RegionTag>().map(|t| (t.app.clone(), t.id)) else {
        return Update::DoNothing;
    };
    put_on_board(&mut app, &mut info, id, Put::Send);
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

// ==== the pointer ====

/// What a press on a page is over, for the ink handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// The grip of an armed loop: no stroke begins, the loop lifts as the pointer moves.
    Grip,
    /// Inside an armed loop: the stroke begins as it always does, and is let go of when the press is held.
    Inside,
    /// Nothing armed is there.
    Elsewhere,
}

fn window_point(info: &CallbackInfo) -> Option<Pt> {
    info.get_cursor_position().into_option().map(|p| Pt::new(p.x, p.y))
}

fn window_size(info: &CallbackInfo) -> Size {
    let d = info.get_current_window_state().size.dimensions;
    Size::new(d.width, d.height)
}

/// A press on page `page` at `point` (the page's px as it was first drawn: where ink is kept). The pointer is taken by the
/// page while a loop is held, so that the carry over the bar is heard.
pub fn press(data: &mut RefAny, info: &mut CallbackInfo, page: usize, point: Pt) -> Pressed {
    let Some(window) = window_point(info) else {
        return Pressed::Elsewhere;
    };
    let pressed = with_glue(data, |g, _| {
        let now = g.panel.now_ms();
        g.panel.wb.page_press(Pointer { page, in_page: point, in_window: window }, now)
    });
    let pressed = match pressed {
        Some(Press::Grip(_)) => Pressed::Grip,
        Some(Press::Inside(_)) => Pressed::Inside,
        _ => Pressed::Elsewhere,
    };
    if pressed != Pressed::Elsewhere {
        let hit = info.get_hit_node();
        info.capture_pointer(hit);
        with_glue(data, |g, _| g.captured = true);
        plug::ensure_timer(data, info);
        redraw_overlays(info);
    }
    pressed
}

/// The pages' layer of armed loops, drawn again.
fn redraw_overlays(info: &mut CallbackInfo) {
    info.update_all_image_callbacks();
}

/// What the pointer's move did to a press on a loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moved {
    /// No press on a loop is followed, or it was a stroke after all: the move is an ordinary one.
    Ordinary,
    /// The press was held or the loop lifted: the stroke it began is no stroke, and the move is the carry's.
    Lifted,
    /// The loop is on its way.
    Carrying,
}

/// The pointer moved with the button down: what the owner's machine makes of it. A loop carried near the edge of the window
/// asks the bar to open; one carried over the bar shows the bar's frame.
pub fn motion(data: &mut RefAny, info: &mut CallbackInfo) -> (Moved, Update) {
    let Some(window) = window_point(info) else {
        return (Moved::Ordinary, Update::DoNothing);
    };
    let size = window_size(info);
    let answer = with_glue(data, |g, s| {
        if !g.panel.wb.drag.is_active() {
            return None;
        }
        let now = g.panel.now_ms();
        let solved = layout::solve(size.w, &s.panels, &s.slide, 0.0);
        let bar = bar_of(size, &s.panels, &solved);
        let motion = g.panel.wb.page_motion(window, now, &bar);
        let mut refresh = false;
        if let Motion::Dragging { over_bar, .. } = motion {
            // The bar's frame is a node: it comes and goes with a new DOM (twice in a drag, not at every move).
            refresh = g.panel.drag_over != over_bar;
            g.panel.drag_over = over_bar;
        }
        Some((motion, refresh))
    })
    .flatten();
    let Some((motion, refresh)) = answer else {
        return (Moved::Ordinary, Update::DoNothing);
    };
    match motion {
        Motion::Nothing | Motion::Inking => (Moved::Ordinary, Update::DoNothing),
        Motion::Held(_) | Motion::Lifted(_) => {
            println!("AZREVIEW_WB lifted");
            redraw_overlays(info);
            (Moved::Lifted, Update::DoNothing)
        }
        Motion::Dragging { open_bar, .. } => {
            if open_bar {
                plug::open_panel(data, info);
            }
            redraw_overlays(info);
            (Moved::Carrying, if refresh || open_bar { Update::RefreshDom } else { Update::DoNothing })
        }
    }
}

/// The button went up: a loop that is carried is dropped where the pointer is (on the open bar it becomes a cutout; anywhere
/// else it is put down and stays armed). `true` when a loop was being followed.
pub fn release(data: &mut RefAny, info: &mut CallbackInfo) -> bool {
    // the page let go of the pointer it held for a press on a loop, whatever came of the press
    if with_glue(data, |g, _| std::mem::take(&mut g.captured)).unwrap_or(false) {
        info.release_pointer_capture();
    }
    let Some(window) = window_point(info) else {
        return false;
    };
    let size = window_size(info);
    let outcome = with_glue(data, |g, s| {
        if !g.panel.wb.drag.is_active() {
            return None;
        }
        let solved = layout::solve(size.w, &s.panels, &s.slide, 0.0);
        let bar = bar_of(size, &s.panels, &solved);
        g.panel.drag_over = false;
        Some(g.panel.wb.page_release(window, &bar))
    })
    .flatten();
    let Some(outcome) = outcome else {
        return false;
    };
    match outcome {
        Release::Dropped { region, at_bar } => {
            println!("AZREVIEW_WB dropped {region}");
            put_on_board(data, info, region, Put::At(at_bar));
        }
        Release::Cancelled => println!("AZREVIEW_WB put down"),
        // a press inside a loop that was not held and did not move far: an ordinary stroke, which the caller ends
        Release::Nothing => return false,
    }
    redraw_overlays(info);
    info.trigger_all_virtual_view_rerender();
    true
}

/// The window lost the focus (or the pointer's release was never heard): a press on a loop that is followed, or a loop that is
/// carried, is let go of where it lies, and stays armed.
pub fn let_go(data: &mut RefAny, info: &mut CallbackInfo) {
    let (was, held) = with_glue(data, |g, _| {
        let was = g.panel.wb.drag.cancel();
        g.panel.drag_over = false;
        (was, std::mem::take(&mut g.captured))
    })
    .unwrap_or((false, false));
    if held {
        info.release_pointer_capture();
    }
    if was {
        println!("AZREVIEW_WB put down");
        redraw_overlays(info);
    }
}

// ==== from a loop to a cutout ====

/// How a loop gets to the board.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Put {
    /// Dropped on the open bar, here (from the bar's top-left corner, the header included).
    At(Pt),
    /// Sent with its key: the board puts it where there is room.
    Send,
}

/// The last stroke that was kept, when it is a loop round some lines of its page: those lines are armed (their region's id and
/// first and last line). The pure part of [`stroke_kept`].
#[must_use]
pub fn arm_last(state: &AppState) -> Option<(RegionId, u32, u32)> {
    let stroke = state.strokes.last()?;
    let file = state.file()?;
    let (first_line, lines) = file.page(stroke.page);
    if lines.is_empty() {
        return None;
    }
    let chars = page_chars(&lines);
    let points: Vec<Pt> = stroke.points.iter().map(|p| Pt::new(p.x, p.y)).collect();
    let mut glue = state.wb.try_borrow_mut().ok()?;
    let now = glue.panel.now_ms();
    let page_size = Size::new(ui::PAGE_W, ui::page_h());
    let id = glue.panel.wb.arm_stroke(stroke.page, &points, &page_lines(first_line, &chars), page_size, stroke.id, now)?;
    glue.panel.wb.armed.get(id).map(|r| (id, r.first_line, r.last_line))
}

/// A pen was lifted and its stroke kept: when the stroke is a loop round some lines, they are armed, and the first loop there
/// ever was says what to do with it (once: the setting remembers it was said).
pub fn stroke_kept(data: &mut RefAny, info: &mut CallbackInfo) {
    let armed = data.downcast_ref::<AppState>().and_then(|s| arm_last(&s));
    let Some((id, first, last)) = armed else {
        return;
    };
    println!("AZREVIEW_WB armed {id} {first} {last}");
    let tell_tip = with_glue(data, |g, _| {
        if g.tip_shown {
            return false;
        }
        g.tip_for = Some(id);
        g.tip_shown = true;
        true
    })
    .unwrap_or(false);
    if tell_tip {
        let kit_ref = data.downcast_ref::<AppState>().map(|s| s.kit.clone());
        if let Some(kit_ref) = kit_ref {
            azul_appkit::ui::set_value(&kit_ref, info, plug::KEY_TIP_SEEN, "1");
        }
    }
    plug::ensure_timer(data, info);
    info.trigger_all_virtual_view_rerender();
}

/// The facts about the file the cutout comes from: its name, what the whole file hashes to now, the lines the loop covers.
fn facts_of(file: &crate::code::SourceFile, page_first: usize, lines: &[String], first: u32, last: u32) -> SourceFacts {
    let from = (first as usize).saturating_sub(page_first);
    let to = ((last as usize).saturating_sub(page_first) + 1).min(lines.len());
    SourceFacts {
        file: file.display.clone(),
        file_hash: crate::workspace_view::hash_of_file(file),
        covered: lines.get(from.min(to)..to).map(<[String]>::to_vec).unwrap_or_default(),
    }
}

/// The loop `region` goes to the board: dropped on the bar at a place, or sent with its key. Its picture is drawn on a Thread
/// ([`on_rendered`] puts it in); what stands in the way is told in the panel.
pub fn put_on_board(app: &mut RefAny, info: &mut CallbackInfo, region: RegionId, how: Put) {
    // the engine says the window's density as 96 per pixel at 1x
    let dpr = info.get_current_window_state().size.dpi as f32 / 96.0;
    let size = window_size(info);
    let job = {
        let mut handle = app.clone();
        let Some(state) = handle.downcast_ref::<AppState>() else {
            return;
        };
        let Some(file) = state.file() else {
            return;
        };
        let Ok(mut glue) = state.wb.try_borrow_mut() else {
            return;
        };
        if !glue.started {
            glue.panel.hint = Some(whiteboard_ui::say(&Notice::BoardBusy));
            return;
        }
        let Some(r) = glue.panel.wb.armed.get(region).cloned() else {
            glue.panel.hint = Some(whiteboard_ui::say(&Notice::NoSuchItem { id: region }));
            return;
        };
        let (page_first, lines) = file.page(r.page);
        let facts = facts_of(file, page_first, &lines, r.first_line, r.last_line);
        let solved = layout::solve(size.w, &state.panels, &state.slide, 0.0);
        let bar = bar_of(size, &state.panels, &solved);
        let page = Size::new(ui::PAGE_W, ui::page_h());
        let dpr = if dpr.is_finite() && dpr >= 1.0 { dpr } else { 1.0 };
        glue.panel.dpr = dpr;
        let made = match how {
            Put::At(at_bar) => glue.panel.wb.drop_region(region, at_bar, facts, dpr, page),
            Put::Send => glue.panel.wb.send_region_now(region, &bar, facts, dpr, page),
        };
        match made {
            Ok(made) => {
                let request = RenderRequest {
                    plan: made.plan.clone(),
                    first_number: u32::try_from(page_first).unwrap_or(u32::MAX),
                    lines: lines.to_vec(),
                    strokes: state.strokes.iter().filter(|st| st.page == r.page).cloned().collect(),
                    fonts: info.get_font_cache_clone(),
                    images: info.get_image_cache_clone(),
                };
                glue.render_started.push((made.id, Instant::now()));
                println!("AZREVIEW_WB cutout {} lines {}-{} of {}", made.id, r.first_line, r.last_line, file.display);
                Some((made.id, request, glue.panel.wb.bar.open))
            }
            Err(n) => {
                println!("AZREVIEW_WB refused {n:?}");
                glue.panel.hint = Some(whiteboard_ui::say(&n));
                None
            }
        }
    };
    let Some((id, request, bar_open)) = job else {
        return;
    };
    if bar_open {
        plug::open_panel(app, info);
    }
    whiteboard_render::spawn_render(info, app, id, request, on_rendered as WriteBackCallbackType);
}

/// The Thread has drawn the picture of a cutout (or could not): it is put in, and the board is kept.
extern "C" fn on_rendered(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(done) = whiteboard_render::take_rendered(&mut msg) else {
        return Update::DoNothing;
    };
    with_glue(&app, |g, _| {
        let took = g
            .render_started
            .iter()
            .position(|(id, _)| *id == done.id)
            .map(|at| g.render_started.remove(at).1.elapsed().as_millis())
            .map_or_else(String::new, |ms| format!(" {ms}ms"));
        match &done.image {
            Ok(img) => println!("AZREVIEW_WB rendered {}x{}{took}", img.width, img.height),
            Err(why) => println!("AZREVIEW_WB rendered {why}"),
        }
        g.panel.wb.image_ready(done.id, done.image);
        g.panel.collect_notices();
    });
    plug::on_changed(&mut app, &mut info);
    Update::RefreshDom
}

#[cfg(test)]
#[path = "whiteboard_page_test.rs"]
mod whiteboard_page_test;
