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
use azul_review_link::frame::{Context, FileState, ReplyStatus, ThumbFormat};
use azul_review_link::view::Session;

use super::glue::{self, with_action};
use super::kit::{column, div, icon, px, row, shorten_middle, tap, text, text_clip, Cx};
use super::{Action, Screen, DONE_SHOWN_MS};
use crate::layout::{pad_geometry, Zone, DEFAULT_ASPECT, STRIP_W};
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

fn portrait(cx: &Cx) -> Dom {
    let z = &cx.z;
    column(&format!("width: {}; height: {};", px(z.safe.w), px(z.safe.h)))
        .with_child(header(cx, z.header, false))
        .with_child(file_bar(cx, z.filebar))
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

/// The path of the file under review, the page and the line, as two pieces of words.
fn file_words(cx: &Cx, path_chars: usize, basename: bool) -> (String, String) {
    match &cx.view().pen {
        Some(pen) if !pen.file.is_empty() => {
            let place = cx.tf("remote-page", &[("n", &(pen.page_index + 1).to_string()), ("m", &pen.page_count.max(1).to_string())]);
            let place = if pen.line > 0 { format!("{place}  ·  {}", cx.tf("remote-line", &[("n", &pen.line.to_string())])) } else { place };
            let shown = if basename { pen.file.rsplit('/').next().unwrap_or(&pen.file) } else { pen.file.as_str() };
            (shorten_middle(shown, path_chars), place)
        }
        _ => (cx.t("remote-no-file"), String::new()),
    }
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
        .with_child(text_clip(&name, "font-size: 17px; font-weight: 600; flex-shrink: 1; min-width: 0px;"))
        .with_child(text(&latency, &format!("font-size: 13px; color: {}; white-space: nowrap; flex-shrink: 0;", th::TEXT_DIM)))
        .with_child(div("flex-grow: 1;"));
    if with_file {
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
    let chars = ((zone.w - 2.0 * cx.z.gutter - 150.0) / 7.2).max(10.0) as usize;
    let (path, place) = file_words(cx, chars, false);
    let mut bar = row(&format!("width: {}; height: {}; padding: 0px {}; gap: 8px;", px(zone.w), px(zone.h), px(cx.z.gutter)))
        .with_child(icon("description", 18.0, th::TEXT_DIM))
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

/// The pad: the page with the viewport rectangle and the pen on it, and the strip that turns pages.
fn pad(cx: &Cx, zone: Zone, margin_left: f32) -> Dom {
    let view = cx.view();
    let aspect = view.pen.as_ref().filter(|p| p.page_w > 0.0 && p.page_h > 0.0).map(|p| p.page_w / p.page_h);
    let g = pad_geometry(zone.w, zone.h, aspect);
    let lost = matches!(view.session, Session::Lost { .. });
    let mut page = div(&format!(
        "position: absolute; left: {}; top: {}; width: {}; height: {}; background: #fbfaf6; border-radius: 3px; overflow: hidden; opacity: {};",
        px(g.page.x),
        px(g.page.y),
        px(g.page.w),
        px(g.page.h),
        if lost { "0.4" } else { "1.0" }
    ));
    if let Some(image) = page_picture(cx) {
        page.add_child(
            Dom::create_image(image)
                .with_css("position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;")
                .with_accessibility_info(AccessibilityInfo::named(cx.t("remote-picture").as_str(), AccessibilityRole::Graphic)),
        );
    }
    if let Some(shown) = cx.root.shown_view() {
        let (x0, y0) = (shown.x.clamp(0.0, 1.0), shown.y.clamp(0.0, 1.0));
        let (x1, y1) = ((shown.x + shown.w).clamp(0.0, 1.0), (shown.y + shown.h).clamp(0.0, 1.0));
        let (w, h) = (((x1 - x0) * g.page.w).max(8.0), ((y1 - y0) * g.page.h).max(8.0));
        page.add_child(div(&format!(
            "position: absolute; left: {}; top: {}; width: {}; height: {}; border: 2px solid {}; background: {}; border-radius: 3px;",
            px(x0 * g.page.w),
            px(y0 * g.page.h),
            px(w),
            px(h),
            th::ACCENT,
            th::tint(th::ACCENT, 0.16)
        )));
    }
    if let Some(mark) = view.pen.as_ref().and_then(|p| p.pen) {
        if (0.0..=1.0).contains(&mark.x) && (0.0..=1.0).contains(&mark.y) {
            let size = if mark.down { 16.0 } else { 12.0 };
            page.add_child(div(&format!(
                "position: absolute; left: {}; top: {}; width: {}; height: {}; border-radius: {}; background: {}; border: 2px solid #ffffff;",
                px(mark.x * g.page.w - size / 2.0),
                px(mark.y * g.page.h - size / 2.0),
                px(size),
                px(size),
                px(size / 2.0),
                th::PEN_DOT
            )));
        }
    }
    let strip = column(&format!(
        "position: absolute; left: {}; top: 0px; width: {}; height: {}; align-items: center; justify-content: space-between; \
         padding: 6px 0px; border-left: 1px solid {}; background: {};",
        px(g.strip.x),
        px(g.strip.w),
        px(g.strip.h),
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
    .with_child(page)
    .with_child(strip);
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
    let width = (zone.w - STRIP_W - 40.0).max(120.0);
    div(&format!(
        "position: absolute; left: {}; bottom: 22px; width: {}; padding: 10px 14px; border-radius: 12px; background: {}; \
         color: #ffffff; font-size: 13px; text-align: center;",
        px(20.0),
        px(width),
        th::tint("#000000", 0.66)
    ))
    .with_child(text(&cx.t("remote-hint"), "font-size: 13px; text-align: center; color: #ffffff;"))
}

fn round_key(cx: &Cx, name: String, glyph: &str, action: Action) -> Dom {
    tap(&name, &format!("width: 44px; height: 44px; border-radius: 22px; background: {};", th::SURFACE), cx, action).with_child(icon(glyph, 24.0, th::TEXT))
}

/// Fit, reset, the pages: what a pinch and a drag do, for hands that would rather tap.
fn controls(cx: &Cx, zone: Zone) -> Dom {
    let zoom = cx.view().pen.as_ref().map_or(1.0, |p| p.zoom);
    row(&format!("width: {}; height: {}; padding: 0px {}; gap: 8px;", px(zone.w), px(zone.h), px(cx.z.gutter)))
        .with_child(round_key(cx, cx.t("remote-fit"), "fit_screen", Action::Fit))
        .with_child(round_key(cx, cx.t("remote-reset"), "restart_alt", Action::ResetView))
        .with_child(round_key(cx, cx.t("remote-prev"), "navigate_before", Action::PageStep(-1)))
        .with_child(round_key(cx, cx.t("remote-next"), "navigate_next", Action::PageStep(1)))
        .with_child(div("flex-grow: 1;"))
        .with_child(text(
            &cx.tf("remote-zoom", &[("percent", &format!("{:.0}", zoom * 100.0))]),
            &format!("font-size: 15px; font-weight: 600; color: {}; white-space: nowrap;", th::TEXT_DIM),
        ))
}

// ---- the buttons

/// What the buttons act on, in words.
fn context_words(cx: &Cx) -> String {
    match &cx.view().pen {
        Some(pen) if !pen.file.is_empty() => match pen.context {
            Context::Region => cx.t("remote-context-region"),
            Context::Line => cx.tf("remote-context-line", &[("n", &pen.line.to_string())]),
            Context::Page => cx.tf("remote-context-page", &[("n", &(pen.page_index + 1).to_string())]),
        },
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

/// The page's shape when the Mac has not said.
pub const FALLBACK_ASPECT: f32 = DEFAULT_ASPECT;
