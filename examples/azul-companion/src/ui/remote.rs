//! The remote (screen B): what the user lives in. A page of the file under review as a picture with the rectangle the Mac
//! shows and the pen on it; drag to pan, pinch to zoom around the pen, double tap to fit the width, a strip along the
//! edge to turn pages; the four quick buttons within thumb's reach; the tabs. In landscape the pad stays on the left and the
//! right side is a panel with the buttons or the files.

use azul::{
    dom::{AccessibilityInfo, AccessibilityRole},
    image::{ImageRef, RawImage},
    prelude::*,
    vec::U8Vec,
    widgets::{Spinner, SpinnerStyle},
};
use azul_review_link::frame::{Context, FileState, PenState, ReplyStatus, ReturnPoint, Surface, ThumbFormat};
use azul_review_link::view::Session;

use super::glue::{self, with_action};
use super::kit::{column, div, icon, px, row, shorten_middle, tap, text, text_clip, Cx};
use super::{Action, Screen, DONE_SHOWN_MS};
use crate::layout::{Zone, STRIP_W};
use crate::padscene;
use crate::theme as th;

/// The id of the pad's block (the pad's events measure themselves against it).
pub const PAD_ID: &str = "pad";


/// The remote in the shape the phone is held.
#[must_use]
pub fn dom(cx: &Cx) -> Dom {
    if cx.env.landscape() {
        landscape(cx)
    } else {
        portrait(cx)
    }
}

/// The return point to offer, if the Mac has one.
fn return_point<'a>(cx: &'a Cx<'_>) -> Option<&'a ReturnPoint> {
    cx.view().pen.as_ref().and_then(|p| p.return_point.as_ref())
}

fn portrait(cx: &Cx) -> Dom {
    let z = &cx.z;
    let mut screen = column(&format!("width: {}; height: {};", px(z.safe.w), px(z.safe.h)))
        .with_child(header(cx, z.header, false))
        .with_child(file_bar(cx, z.filebar));
    if let Some(point) = return_point(cx) {
        screen.add_child(ribbon(cx, point, z.ribbon));
    }
    screen
        .with_child(pad(cx, z.pad, z.pad.x - z.safe.x))
        .with_child(controls(cx, z.controls))
        .with_child(context_line(cx, z.context))
        .with_child(dock(cx, false))
        .with_child(tab_bar(cx, z.tabs))
}

fn landscape(cx: &Cx) -> Dom {
    let z = &cx.z;
    let column_w = z.header.w;
    row(&format!("align-items: stretch; width: {}; height: {};", px(z.safe.w), px(z.safe.h)))
        .with_child(
            column(&format!("width: {}; height: {}; flex-shrink: 0;", px(column_w), px(z.safe.h)))
                .with_child(header(cx, z.header, true))
                .with_child(pad(cx, z.pad, z.pad.x - z.safe.x))
                .with_child(controls(cx, z.controls)),
        )
        .with_child(side_panel(cx))
}

/// What a return point says in a few words: the Mac's label, or the file, page and line.
fn return_words(cx: &Cx, point: &ReturnPoint) -> String {
    if !point.label.is_empty() {
        return point.label.clone();
    }
    let file = point.file.rsplit('/').next().unwrap_or(&point.file).to_string();
    let page = (point.page_index + 1).to_string();
    if point.line > 0 {
        cx.tf("return-where", &[("file", &file), ("page", &page), ("line", &point.line.to_string())])
    } else {
        cx.tf("return-where-nolines", &[("file", &file), ("page", &page)])
    }
}

/// "Back to fc.rs p3 l1337" and the button that goes there: over the pad (portrait) or at the top of the side panel (landscape)
/// while AzReview holds a return point. `zone` is `Zones::ribbon`.
fn ribbon(cx: &Cx, point: &ReturnPoint, zone: Zone) -> Dom {
    let words = return_words(cx, point);
    let inner = zone.w - 2.0 * cx.z.gutter;
    row(&format!("width: {}; height: {}; padding: 0px {}; flex-shrink: 0;", px(zone.w), px(zone.h), px(cx.z.gutter)))
        .with_child(
            row(&format!(
                "width: {}; height: 50px; padding: 0px 3px 0px 12px; gap: 8px; border-radius: 13px; background: {};",
                px(inner),
                th::tint(th::ACCENT, 0.18)
            ))
            .with_class("az-row")
            .with_child(icon("undo", 20.0, th::ACCENT))
            .with_child(text_clip(&cx.tf("return-back", &[("where", &words)]), &format!("flex-grow: 1; min-width: 0px; font-size: 14px; color: {};", th::TEXT)))
            .with_child(
                tap(
                    &cx.tf("return-name", &[("where", &words)]),
                    &format!("height: 44px; padding: 0px 14px; border-radius: 10px; background: {}; flex-shrink: 0;", th::ACCENT),
                    cx,
                    Action::ContinueReviewing,
                )
                .with_child(text(&cx.t("return-continue"), "font-size: 14px; font-weight: 600; color: #ffffff; white-space: nowrap;")),
            ),
        )
}

// ---- the lines above the pad

/// The colour and the words of the connection.
fn connection(cx: &Cx) -> (&'static str, String) {
    match &cx.view().session {
        Session::Connected { .. } => (th::STATE_DONE, cx.t("conn-connected")),
        Session::Searching { .. } | Session::Pairing { .. } => (th::WARN, cx.t("conn-searching")),
        Session::Lost { .. } => (th::WARN, cx.t("conn-lost")),
        _ => (th::DANGER, cx.t("conn-offline")),
    }
}

/// The file under review and where in it (over the pages), or the board and how much is on it (over the whiteboard), as two
/// pieces of words.
fn file_words(cx: &Cx, path_chars: usize, basename: bool) -> (String, String) {
    match &cx.view().pen {
        Some(PenState { surface: Surface::Page(page), .. }) if !page.file.is_empty() => {
            let place = cx.tf("remote-page", &[("n", &(page.page_index + 1).to_string()), ("m", &page.page_count.max(1).to_string())]);
            let place = if page.line > 0 { format!("{place}  ·  {}", cx.tf("remote-line", &[("n", &page.line.to_string())])) } else { place };
            let shown = if basename { page.file.rsplit('/').next().unwrap_or(&page.file) } else { page.file.as_str() };
            (shorten_middle(shown, path_chars), place)
        }
        Some(PenState { surface: Surface::Whiteboard(board), .. }) => {
            let items = match board.item_count {
                0 => cx.t("board-empty"),
                1 => cx.t("board-item"),
                n => cx.tf("board-items", &[("n", &n.to_string())]),
            };
            let _ = path_chars;
            (String::new(), items)
        }
        _ => (cx.t("remote-no-file"), String::new()),
    }
}

/// Which surface the pen is over: "Code review", or "Whiteboard" and the board's name.
fn surface_chip(cx: &Cx) -> Dom {
    let (glyph, label) = match cx.view().pen.as_ref().map(|p| &p.surface) {
        Some(Surface::Whiteboard(board)) if !board.name.is_empty() => {
            ("dashboard", cx.tf("surface-board-named", &[("name", &shorten_middle(&board.name, 16))]))
        }
        Some(Surface::Whiteboard(_)) => ("dashboard", cx.t("surface-board")),
        _ => ("description", cx.t("surface-code")),
    };
    row(&format!(
        "height: 26px; padding: 0px 10px 0px 8px; gap: 6px; border-radius: 13px; background: {}; flex-shrink: 1; min-width: 0px;",
        th::tint(th::ACCENT, 0.18)
    ))
    .with_class("az-row")
    .with_child(icon(glyph, 16.0, th::ACCENT))
    .with_child(text_clip(&label, &format!("font-size: 12px; font-weight: 600; color: {}; min-width: 0px;", th::ACCENT)))
}

fn gear(cx: &Cx) -> Dom {
    tap(&cx.t("nav-settings"), "width: 44px; height: 44px; border-radius: 22px;", cx, Action::OpenSettings).with_child(icon("settings", 24.0, th::TEXT_DIM))
}

fn header(cx: &Cx, zone: Zone, with_file: bool) -> Dom {
    let (colour, state_words) = connection(cx);
    let view = cx.view();
    let name = if view.mac_name.is_empty() { state_words.clone() } else { view.mac_name.clone() };
    let latency = match (view.is_connected(), view.latency_ms) {
        (true, Some(ms)) => cx.tf("conn-latency", &[("ms", &ms.to_string())]),
        _ => state_words,
    };
    let mut bar = row(&format!("width: {}; height: {}; padding: 0px {}; gap: 10px;", px(zone.w), px(zone.h), px(cx.z.gutter)))
        .with_child(div(&format!("width: 10px; height: 10px; border-radius: 5px; background: {colour}; flex-shrink: 0;")))
        .with_child(text_clip(&name, "font-size: 17px; font-weight: 600; flex-shrink: 1; min-width: 0px;"));
    // sideways the row is crowded (the chip, the file, the place): the latency is left out there, the dot says the rest
    if !with_file {
        bar.add_child(text(&latency, &format!("font-size: 13px; color: {}; white-space: nowrap; flex-shrink: 0;", th::TEXT_DIM)));
    }
    let mut bar = bar.with_child(div("flex-grow: 1;"));
    if with_file {
        bar.add_child(surface_chip(cx));
        let (path, place) = file_words(cx, 22, true);
        bar.add_child(text_clip(&path, &format!("font-size: 13px; color: {}; flex-shrink: 1; min-width: 0px;", th::TEXT_DIM)));
        if !place.is_empty() {
            bar.add_child(text(&place, &format!("font-size: 13px; color: {}; white-space: nowrap; flex-shrink: 0;", th::TEXT)));
        }
    } else {
        bar.add_child(gear(cx));
    }
    bar
}

fn file_bar(cx: &Cx, zone: Zone) -> Dom {
    let chars = ((zone.w - 2.0 * cx.z.gutter - 270.0) / 7.2).max(8.0) as usize;
    let (path, place) = file_words(cx, chars, true);
    let mut bar = row(&format!("width: {}; height: {}; padding: 0px {}; gap: 8px;", px(zone.w), px(zone.h), px(cx.z.gutter)))
        .with_child(surface_chip(cx))
        .with_child(text_clip(&path, &format!("font-size: 14px; color: {}; flex-shrink: 1; min-width: 0px;", th::TEXT)))
        .with_child(div("flex-grow: 1;"));
    if !place.is_empty() {
        bar.add_child(text(&place, &format!("font-size: 13px; color: {}; white-space: nowrap; flex-shrink: 0;", th::TEXT_DIM)));
    }
    bar
}

// ---- the pad

/// The picture of the page the pen state names (raw pixels), if one has come.
fn page_picture(cx: &Cx) -> Option<ImageRef> {
    let thumb = cx.view().thumb.as_ref()?;
    if thumb.format != ThumbFormat::Rgba8 {
        return None;
    }
    let raw = RawImage::create_rgba8(u32::from(thumb.width), u32::from(thumb.height), U8Vec::from_vec(thumb.bytes.clone()), false);
    ImageRef::raw_image(raw).into_option()
}

/// A positioned box in the pad's pixels.
fn boxed(zone: Zone, css: &str) -> Dom {
    div(&format!(
        "position: absolute; left: {}; top: {}; width: {}; height: {}; {css}",
        px(zone.x),
        px(zone.y),
        px(zone.w.max(0.0)),
        px(zone.h.max(0.0))
    ))
}

/// The picture of the surface stretched over its box.
fn stretched(cx: &Cx, image: ImageRef, name_key: &str) -> Dom {
    Dom::create_image(image)
        .with_css("position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;")
        .with_accessibility_info(AccessibilityInfo::named(cx.t(name_key).as_str(), AccessibilityRole::Graphic))
}

/// The pad: the sheets of the pages (or the board) under the frame that stands for what the Mac shows, the pen on it, and
/// the strip that turns pages.
fn pad(cx: &Cx, zone: Zone, margin_left: f32) -> Dom {
    let view = cx.view();
    let lost = matches!(view.session, Session::Lost { .. });
    let placeholder = PenState::empty();
    let state = view.pen.as_ref().unwrap_or(&placeholder);
    let shown = cx.root.shown_view().unwrap_or(state.view);
    let scene = padscene::scene(&state.surface, shown, state.pen, (zone.w, zone.h));
    // the sheets reach beyond the pad and are clipped by it: the layout check measures them through this clip
    let mut surface = div(&format!(
        "position: absolute; left: 0px; top: 0px; width: {}; height: {}; overflow: hidden; opacity: {};",
        px(scene.content.w),
        px(scene.content.h),
        if lost { "0.4" } else { "1.0" }
    ))
    .with_class("az-scroll");
    let picture = if view.thumb_is_current() { page_picture(cx) } else { None };
    for sheet in &scene.sheets {
        let mut node = boxed(
            sheet.rect,
            &format!(
                "background: {}; border: 1px solid #d9d7cf; border-radius: 3px; overflow: hidden;",
                if sheet.current { "#fbfaf6" } else { "#e9e7e0" }
            ),
        );
        if sheet.current {
            if let Some(image) = picture.clone() {
                node.add_child(stretched(cx, image, "remote-picture"));
            }
        }
        if sheet.rect.h >= 36.0 {
            node.add_child(text(
                &(sheet.index + 1).to_string(),
                "position: absolute; left: 6px; top: 3px; font-size: 11px; font-weight: 600; color: #8a8a90;",
            ));
        }
        surface.add_child(node);
    }
    if let Some(board) = scene.board {
        let mut node = boxed(board, "background: #22232a; border: 1px solid #3a3c46; border-radius: 6px; overflow: hidden;");
        if let Some(image) = picture.clone() {
            node.add_child(stretched(cx, image, "remote-picture-board"));
        }
        surface.add_child(node);
    }
    surface.add_child(boxed(
        scene.frame,
        &format!("border: 2px solid {}; background: {}; border-radius: 3px;", th::ACCENT, th::tint(th::ACCENT, 0.16)),
    ));
    if let Some((x, y, down)) = scene.pen {
        let size = if down { 16.0 } else { 12.0 };
        surface.add_child(boxed(
            Zone { x: x - size / 2.0, y: y - size / 2.0, w: size, h: size },
            &format!("border-radius: {}; background: {}; border: 2px solid #ffffff;", px(size / 2.0), th::PEN_DOT),
        ));
    }
    if let Some(mini) = scene.minimap {
        let mut inset = boxed(mini.board, "background: #1a1b20; border: 1px solid #5a5c68; border-radius: 4px; overflow: hidden;");
        if let Some(image) = picture {
            inset.add_child(stretched(cx, image, "remote-overview"));
        }
        surface.add_child(inset);
        surface.add_child(boxed(mini.frame, &format!("border: 1.5px solid {}; background: {};", th::ACCENT, th::tint(th::ACCENT, 0.25))));
    }
    let strip = column(&format!(
        "position: absolute; left: {}; top: 0px; width: {}; height: {}; align-items: center; justify-content: space-between; \
         padding: 6px 0px; border-left: 1px solid {}; background: {};",
        px(zone.w - STRIP_W.min(zone.w)),
        px(STRIP_W.min(zone.w)),
        px(zone.h),
        th::SEPARATOR,
        th::tint("#ffffff", 0.03)
    ))
    .with_accessibility_name(cx.t("remote-page-strip").as_str())
    .with_child(icon("keyboard_arrow_up", 28.0, th::TEXT_DIM))
    .with_child(icon("keyboard_arrow_down", 28.0, th::TEXT_DIM));
    let mut node = div(&format!(
        "position: relative; width: {}; height: {}; margin-left: {}; background: {}; border-radius: 16px; overflow: hidden; flex-shrink: 0;",
        px(zone.w),
        px(zone.h),
        px(margin_left),
        th::SURFACE
    ))
    .with_id(PAD_ID)
    .with_class("az-surface")
    .with_accessibility_info(AccessibilityInfo::named(cx.t("remote-pad").as_str(), AccessibilityRole::Application))
    .with_child(surface);
    if scene.board.is_none() {
        node.add_child(strip);
    }
    if cx.root.ui.hint && view.pen.is_some() && !lost {
        node.add_child(hint(cx, zone));
    }
    if lost {
        node.add_child(lost_banner(cx, zone));
    }
    node.with_callback(EventFilter::Hover(HoverEventFilter::LeftMouseDown), cx.data.clone(), glue::on_pad_down)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), cx.data.clone(), glue::on_pad_move)
        .with_callback(EventFilter::Hover(HoverEventFilter::LeftMouseUp), cx.data.clone(), glue::on_pad_up)
        .with_callback(EventFilter::Hover(HoverEventFilter::RightMouseDown), cx.data.clone(), glue::on_pad_secondary)
        .with_callback(EventFilter::Hover(HoverEventFilter::PinchIn), cx.data.clone(), glue::on_pad_pinch)
        .with_callback(EventFilter::Hover(HoverEventFilter::PinchOut), cx.data.clone(), glue::on_pad_pinch)
}

/// Over the pad while the link is down: the phone is trying again.
fn lost_banner(cx: &Cx, zone: Zone) -> Dom {
    let width = (zone.w - STRIP_W - 40.0).max(160.0);
    row(&format!(
        "position: absolute; left: 20px; top: 18px; width: {}; height: 60px; padding: 0px 12px; gap: 10px; border-radius: 12px; background: {}; border: 1px solid {};",
        px(width),
        "#2a230f",
        th::tint(th::WARN, 0.7)
    ))
    .with_child(icon("wifi_off", 20.0, th::WARN))
    .with_child(text(&cx.t("remote-lost"), &format!("font-size: 14px; font-weight: 600; color: {};", th::WARN)))
}

/// The gestures, said once over the page until the first one is made.
fn hint(cx: &Cx, zone: Zone) -> Dom {
    let board = cx.view().pen.as_ref().is_some_and(|p| p.board().is_some());
    let width = (zone.w - if board { 0.0 } else { STRIP_W } - 40.0).max(120.0);
    div(&format!(
        "position: absolute; left: {}; bottom: 22px; width: {}; padding: 10px 14px; border-radius: 12px; background: {}; \
         color: #ffffff; font-size: 13px; text-align: center;",
        px(20.0),
        px(width),
        th::tint("#000000", 0.66)
    ))
    .with_child(text(&cx.t(if board { "remote-hint-board" } else { "remote-hint" }), "font-size: 13px; text-align: center; color: #ffffff;"))
}

fn round_key(cx: &Cx, name: String, glyph: &str, action: Action) -> Dom {
    tap(&name, &format!("width: 44px; height: 44px; border-radius: 22px; background: {};", th::SURFACE), cx, action).with_child(icon(glyph, 24.0, th::TEXT))
}

/// Sideways, where the return point has no bar of its own: a few words and the button, in place of the zoom.
fn return_inline(cx: &Cx, point: &ReturnPoint) -> Dom {
    let words = return_words(cx, point);
    row("flex-grow: 1; min-width: 0px; justify-content: flex-end; gap: 6px;")
        .with_child(icon("undo", 16.0, th::TEXT_DIM))
        .with_child(text_clip(&words, &format!("flex-shrink: 1; min-width: 0px; font-size: 13px; color: {};", th::TEXT_DIM)))
        .with_child(
            tap(
                &cx.tf("return-name", &[("where", &words)]),
                &format!("height: 44px; padding: 0px 12px; border-radius: 10px; background: {}; flex-shrink: 0;", th::ACCENT),
                cx,
                Action::ContinueReviewing,
            )
            .with_child(text(&cx.t("return-continue"), "font-size: 14px; font-weight: 600; color: #ffffff; white-space: nowrap;")),
        )
}

/// Fit, reset, the pages: what a pinch and a drag do, for hands that would rather tap.
fn controls(cx: &Cx, zone: Zone) -> Dom {
    let zoom = cx.view().pen.as_ref().map_or(1.0, |p| p.zoom);
    let pages = cx.view().pen.as_ref().is_none_or(|p| p.page().is_some());
    let mut keys = row(&format!("width: {}; height: {}; padding: 0px {}; gap: 8px;", px(zone.w), px(zone.h), px(cx.z.gutter)))
        .with_child(round_key(cx, cx.t("remote-fit"), "fit_screen", Action::Fit))
        .with_child(round_key(cx, cx.t("remote-reset"), "restart_alt", Action::ResetView));
    if pages {
        keys.add_child(round_key(cx, cx.t("remote-prev"), "navigate_before", Action::PageStep(-1)));
        keys.add_child(round_key(cx, cx.t("remote-next"), "navigate_next", Action::PageStep(1)));
    }
    match return_point(cx).filter(|_| cx.env.landscape()) {
        Some(point) => keys.with_child(return_inline(cx, point)),
        None => keys.with_child(div("flex-grow: 1;")).with_child(text(
            &cx.tf("remote-zoom", &[("percent", &format!("{:.0}", zoom * 100.0))]),
            &format!("font-size: 15px; font-weight: 600; color: {}; white-space: nowrap;", th::TEXT_DIM),
        )),
    }
}

// ---- the buttons

/// What the buttons act on, in words.
fn context_words(cx: &Cx) -> String {
    match &cx.view().pen {
        Some(PenState { surface: Surface::Page(page), context, .. }) if !page.file.is_empty() => match context {
            Context::Region => cx.t("remote-context-region"),
            Context::Line => cx.tf("remote-context-line", &[("n", &page.line.to_string())]),
            Context::Page => cx.tf("remote-context-page", &[("n", &(page.page_index + 1).to_string())]),
        },
        Some(PenState { surface: Surface::Whiteboard(_), .. }) => cx.t("remote-context-board"),
        _ => cx.t("remote-context-none"),
    }
}

fn context_line(cx: &Cx, zone: Zone) -> Dom {
    row(&format!("width: {}; height: {}; padding: 0px {}; gap: 8px;", px(zone.w), px(zone.h), px(cx.z.gutter)))
        .with_child(icon("touch_app", 16.0, th::TEXT_DIM))
        .with_child(text_clip(&context_words(cx), &format!("font-size: 13px; color: {}; min-width: 0px;", th::TEXT_DIM)))
}

/// What a quick button shows of its last press: the words, their colour, and the little mark before them.
fn press_state(cx: &Cx, index: usize) -> Option<(String, &'static str, Dom)> {
    let view = cx.view();
    if view.button_request[index] == 0 {
        return None;
    }
    let spinner = |colour: ColorU| Spinner::with_size(14).with_color(colour).with_indicator(SpinnerStyle::Spokes).dom();
    let status = match (view.button_status[index], cx.root.ui.status_seen[index]) {
        (Some(ReplyStatus::Done), Some((_, at))) if cx.root.now().saturating_sub(at) > DONE_SHOWN_MS => return None,
        (status, _) => status,
    };
    Some(match status {
        None => (cx.t("button-state-sending"), th::TEXT_DIM, spinner(ColorU { r: 163, g: 163, b: 173, a: 255 })),
        Some(ReplyStatus::Received) => (cx.t("button-state-received"), th::TEXT_DIM, icon("check", 14.0, th::TEXT_DIM)),
        Some(ReplyStatus::Working) => (cx.t("button-state-working"), th::STATE_WORKING, spinner(ColorU { r: 90, g: 175, b: 255, a: 255 })),
        Some(ReplyStatus::Suggested) => (
            cx.t("button-state-suggested"),
            th::STATE_SUGGESTED,
            div(&format!("width: 10px; height: 10px; border-radius: 5px; background: {}; margin: 2px;", th::STATE_SUGGESTED)),
        ),
        Some(ReplyStatus::Done) => (cx.t("button-state-done"), th::STATE_DONE, icon("check_circle", 14.0, th::STATE_DONE)),
        Some(ReplyStatus::Failed) => (cx.t("button-state-failed"), th::DANGER, icon("error", 14.0, th::DANGER)),
    })
}

fn quick_button(cx: &Cx, index: usize, height: f32, stacked: bool) -> Dom {
    let button = &cx.view().buttons.buttons[index];
    let label = if button.label.is_empty() { cx.tf("editor-slot", &[("n", &(index + 1).to_string())]) } else { button.label.clone() };
    let mut words = column("flex-grow: 1; min-width: 0px; align-items: flex-start; justify-content: center; padding: 0px 8px 0px 12px; gap: 2px;")
        .with_child(text(&label, "font-size: 17px; font-weight: 600; white-space: nowrap;"));
    if let Some((state, colour, mark)) = press_state(cx, index) {
        words.add_child(
            row("gap: 6px; height: 16px;")
                .with_child(div("width: 14px; height: 14px; display: flex; align-items: center; justify-content: center; flex-shrink: 0;").with_child(mark))
                .with_child(text(&state, &format!("font-size: 12px; color: {colour}; white-space: nowrap;"))),
        );
    }
    let key = tap(
        &cx.tf("button-name", &[("label", &label)]),
        &format!(
            "flex-direction: row; justify-content: flex-start; {} min-width: 0px; height: {}; \
             background: {}; border-radius: 14px; overflow: hidden;",
            if stacked { "width: 100%;" } else { "flex-grow: 1; flex-basis: 0px;" },
            px(height),
            th::SURFACE
        ),
        cx,
        Action::Button(index),
    )
    .with_class("az-quick")
    .with_child(div(&format!("width: 5px; align-self: stretch; background: {}; flex-shrink: 0;", th::button_colour(index))))
    .with_child(words);
    with_action(key, HoverEventFilter::LongPress, cx.data, Action::EditButtons(Some(index)))
}

/// The four quick buttons: two by two under the pad, or one under the other in the side panel.
fn dock(cx: &Cx, stacked: bool) -> Dom {
    let z = &cx.z;
    if stacked {
        let mut list = column(&format!(
            "width: {}; height: {}; padding: 0px 12px 8px 12px; gap: {};",
            px(z.dock.w),
            px(z.dock.h),
            px(z.button_gap)
        ));
        for i in 0..4 {
            list.add_child(quick_button(cx, i, z.button_h, true));
        }
        return list;
    }
    let pair = |a: usize, b: usize| {
        row(&format!("gap: {}; width: 100%;", px(z.button_gap)))
            .with_child(quick_button(cx, a, z.button_h, false))
            .with_child(quick_button(cx, b, z.button_h, false))
    };
    column(&format!("width: {}; height: {}; padding: 0px {} 8px {}; gap: {};", px(z.dock.w), px(z.dock.h), px(z.gutter), px(z.gutter), px(z.button_gap)))
        .with_child(pair(0, 1))
        .with_child(pair(2, 3))
}

// ---- the tabs

fn tab(cx: &Cx, screen: Screen, glyph: &str, key: &str) -> Dom {
    let on = cx.root.ui.screen == screen;
    let colour = if on { th::ACCENT } else { th::TEXT_DIM };
    tap(&cx.t(key), "flex-grow: 1; flex-basis: 0px; height: 100%; flex-direction: column; gap: 2px;", cx, Action::Tab(screen))
        .with_child(icon(glyph, 24.0, colour))
        .with_child(text(&cx.t(key), &format!("font-size: 11px; font-weight: 600; color: {colour};")))
}

/// The tab bar under the portrait screens.
#[must_use]
pub fn tab_bar(cx: &Cx, zone: Zone) -> Dom {
    row(&format!("width: {}; height: {}; border-top: 1px solid {}; background: {};", px(zone.w), px(zone.h), th::SEPARATOR, th::BG))
        .with_accessibility_name(cx.t("tabs-name").as_str())
        .with_child(tab(cx, Screen::Remote, "pan_tool", "tab-remote"))
        .with_child(tab(cx, Screen::Files, "folder_open", "tab-files"))
}

// ---- the side panel (landscape)

fn segment(cx: &Cx, screen: Screen, key: &str) -> Dom {
    let on = cx.root.ui.screen == screen;
    let (bg, fg) = if on { (th::ACCENT, th::ON_ACCENT) } else { (th::SURFACE, th::TEXT_DIM) };
    tap(&cx.t(key), &format!("flex-grow: 1; flex-basis: 0px; height: 44px; border-radius: 10px; background: {bg};"), cx, Action::Tab(screen))
        .with_child(text(&cx.t(key), &format!("font-size: 14px; font-weight: 600; color: {fg};")))
}

/// The side panel: a switch between the buttons and the files, and the gear; below it what the switch chose.
fn side_panel(cx: &Cx) -> Dom {
    let z = &cx.z;
    let switch = row(&format!("width: {}; height: {}; padding: 0px 10px; gap: 8px; flex-shrink: 0;", px(z.tabs.w), px(z.tabs.h)))
        .with_child(segment(cx, Screen::Remote, "tab-buttons"))
        .with_child(segment(cx, Screen::Files, "tab-files"))
        .with_child(gear(cx));
    let body = if cx.root.ui.screen == Screen::Files {
        super::files::panel(cx, z.side_body)
    } else {
        column(&format!("width: {}; height: {};", px(z.side_body.w), px(z.side_body.h)))
            .with_child(context_line(cx, z.context))
            .with_child(dock(cx, true))
    };
    column(&format!(
        "width: {}; height: {}; flex-shrink: 0; border-left: 1px solid {}; background: {};",
        px(z.side.w),
        px(z.side.h),
        th::SEPARATOR,
        "#202023"
    ))
    .with_child(switch)
    .with_child(body)
}

/// The colour a file's state has (`None`: no mark).
#[must_use]
pub fn state_colour(state: FileState) -> Option<&'static str> {
    match state {
        FileState::None => None,
        FileState::Clean => Some(th::STATE_DONE),
        FileState::Unresolved => Some(th::STATE_SUGGESTED),
        FileState::Working => Some(th::STATE_WORKING),
    }
}

