//! Settings and about (screen E): which Mac this is, the quick buttons' editor, forgetting the Mac, what this app is.

use azul::prelude::*;
use azul_review_link::limits::PROTOCOL_VERSION;
use azul_review_link::view::Session;

use super::kit::{card, column, div, hairline, icon, px, row, tap, text, Cx};
// (`row` is for the rows with a picture; the log's lines are columns so that their text wraps)
use super::Action;
use crate::theme as th;

/// The version of this app.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn section_title(words: &str, gutter: f32) -> Dom {
    text(words, &format!("font-size: 13px; font-weight: 600; color: {}; padding: 18px {} 8px {};", th::TEXT_DIM, px(gutter + 4.0), px(gutter + 4.0)))
}

fn nav_header(cx: &Cx, title: &str, back: Action, back_name: &str) -> Dom {
    let z = &cx.z;
    row(&format!(
        "width: {}; height: {}; padding: 0px {}; gap: 6px; border-bottom: 1px solid {}; flex-shrink: 0;",
        px(z.safe.w),
        px(if cx.env.landscape() { 48.0 } else { z.header.h }),
        px(z.gutter - 6.0),
        th::SEPARATOR
    ))
    .with_child(tap(back_name, "width: 44px; height: 44px; border-radius: 22px;", cx, back).with_child(icon("arrow_back", 24.0, th::ACCENT)))
    .with_child(text(title, "font-size: 17px; font-weight: 600; flex-grow: 1;"))
}

/// A row that goes somewhere, with a line under its words when there is one to say.
fn link_row(cx: &Cx, glyph: &str, words: &str, hint: Option<&str>, action: Action) -> Dom {
    let mut label = column("flex-grow: 1; min-width: 0px; align-items: flex-start; gap: 2px;").with_child(text(words, "text-align: left; font-size: 17px;"));
    if let Some(hint) = hint {
        label.add_child(text(hint, &format!("text-align: left; font-size: 12px; color: {};", th::TEXT_DIM)));
    }
    tap(words, "width: 100%; min-height: 56px; flex-direction: row; justify-content: flex-start; gap: 14px; padding: 8px 16px;", cx, action)
        .with_child(icon(glyph, 24.0, th::ACCENT))
        .with_child(label)
        .with_child(icon("chevron_right", 22.0, th::TEXT_DIM))
}

fn info_row(words: &str, colour: &str) -> Dom {
    row("width: 100%; min-height: 40px; padding: 0px 16px;").with_child(text(words, &format!("font-size: 15px; color: {colour};")))
}

/// The Settings screen.
#[must_use]
pub fn dom(cx: &Cx) -> Dom {
    let z = &cx.z;
    let view = cx.view();
    let (colour, mac_words, status_words) = match &view.session {
        Session::Connected { mac_name } => (
            th::STATE_DONE,
            mac_name.clone(),
            match view.latency_ms {
                Some(ms) => cx.tf("settings-connected", &[("ms", &ms.to_string())]),
                None => cx.t("conn-connected"),
            },
        ),
        Session::Lost { mac_name, .. } => (th::WARN, mac_name.clone(), cx.t("conn-lost")),
        _ => (th::DANGER, view.mac_name.clone(), cx.t("settings-not-connected")),
    };
    let mac_name = if mac_words.is_empty() { cx.t("settings-no-mac") } else { mac_words };
    let gutter = z.gutter;
    let mut mac = card("width: 100%; overflow: hidden;")
        .with_child(
            row("width: 100%; height: 64px; padding: 0px 16px; gap: 12px;")
                .with_child(div(&format!("width: 12px; height: 12px; border-radius: 6px; background: {colour}; flex-shrink: 0;")))
                .with_child(
                    column("flex-grow: 1; min-width: 0px;")
                        .with_child(text(&mac_name, "font-size: 17px; font-weight: 600; white-space: nowrap; overflow: hidden;"))
                        .with_child(text(&status_words, &format!("font-size: 13px; color: {};", th::TEXT_DIM))),
                ),
        )
        .with_child(hairline("height: 1px; width: 100%;"))
        .with_child(link_row(cx, "tune", &cx.t("settings-quick-buttons"), Some(&cx.t("settings-quick-buttons-hint")), Action::EditButtons(None)));
    if cx.root.ui.forget_ask {
        mac.add_child(hairline("height: 1px; width: 100%;"));
        mac.add_child(
            column("width: 100%; padding: 14px 16px; gap: 12px;")
                .with_child(text(&cx.tf("settings-forget-ask", &[("mac", &mac_name)]), &format!("font-size: 15px; color: {};", th::TEXT)))
                .with_child(
                    row("gap: 10px;")
                        .with_child(
                            tap(&cx.t("settings-forget-no"), &format!("flex-grow: 1; flex-basis: 0px; height: 48px; border-radius: 12px; background: {};", th::SURFACE_HI), cx, Action::ForgetNo)
                                .with_child(text(&cx.t("settings-forget-no"), "font-size: 16px; font-weight: 600;")),
                        )
                        .with_child(
                            tap(&cx.t("settings-forget-yes"), &format!("flex-grow: 1; flex-basis: 0px; height: 48px; border-radius: 12px; background: {};", th::DANGER), cx, Action::ForgetYes)
                                .with_child(text(&cx.t("settings-forget-yes"), &format!("font-size: 16px; font-weight: 600; color: {};", th::ON_ACCENT))),
                        ),
                ),
        );
    } else if cx.root.demo {
        mac.add_child(hairline("height: 1px; width: 100%;"));
        mac.add_child(
            column("width: 100%; padding: 12px 16px 4px 16px; gap: 2px;")
                .with_child(text(&cx.t("settings-demo-note"), &format!("font-size: 13px; color: {}; line-height: 1.3;", th::TEXT_DIM))),
        );
        mac.add_child(
            tap(&cx.t("settings-leave-demo"), "width: 100%; height: 56px; flex-direction: row; justify-content: flex-start; gap: 14px; padding: 0px 16px;", cx, Action::LeaveDemo)
                .with_child(icon("logout", 24.0, th::ACCENT))
                .with_child(text(&cx.t("settings-leave-demo"), &format!("flex-grow: 1; text-align: left; font-size: 17px; color: {};", th::ACCENT))),
        );
    } else if !matches!(view.session, Session::Unpaired) {
        mac.add_child(hairline("height: 1px; width: 100%;"));
        mac.add_child(
            tap(&cx.t("settings-forget"), "width: 100%; height: 56px; flex-direction: row; justify-content: flex-start; gap: 14px; padding: 0px 16px;", cx, Action::ForgetAsk)
                .with_child(icon("link_off", 24.0, th::DANGER))
                .with_child(text(&cx.t("settings-forget"), &format!("flex-grow: 1; text-align: left; font-size: 17px; color: {};", th::DANGER))),
        );
    }
    let about = card("width: 100%; padding: 4px 0px 8px 0px;")
        .with_child(row("width: 100%; height: 48px; padding: 0px 16px; gap: 12px;").with_child(icon("devices", 24.0, th::TEXT_DIM)).with_child(text(&cx.t("app-name"), "font-size: 17px; font-weight: 600;")))
        .with_child(info_row(&cx.tf("settings-version", &[("v", APP_VERSION)]), th::TEXT_DIM))
        .with_child(info_row(&cx.tf("settings-protocol", &[("p", &PROTOCOL_VERSION.to_string())]), th::TEXT_DIM))
        .with_child(info_row(&cx.t("settings-licence"), th::TEXT_DIM));
    let log = cx.root.log();
    let mut report = card("width: 100%; padding: 6px 0px 6px 0px;");
    if log.is_empty() {
        report.add_child(info_row(&cx.t("settings-log-empty"), th::TEXT_DIM));
    }
    for line in log.iter().rev().take(LOG_LINES).rev() {
        report.add_child(
            column("width: 100%; padding: 3px 16px;").with_child(text(&shorten(line, LOG_LINE_CHARS), &format!("font-size: 11px; line-height: 1.3; color: {}; text-align: left;", th::TEXT_DIM))),
        );
    }
    let list_h = (z.safe.h - if cx.env.landscape() { 48.0 } else { z.header.h }).max(0.0);
    column(&format!("width: {}; height: {};", px(z.safe.w), px(z.safe.h)))
        .with_child(nav_header(cx, &cx.t("settings-title"), Action::Back, &cx.t("nav-back")))
        .with_child(
            column(&format!("width: {}; height: {}; overflow-y: auto; padding: 0px {} 24px {};", px(z.safe.w), px(list_h), px(gutter), px(gutter)))
                .with_class("az-scroll")
                .with_child(section_title(&cx.t("settings-mac"), 0.0))
                .with_child(mac)
                .with_child(section_title(&cx.t("settings-about"), 0.0))
                .with_child(about)
                .with_child(section_title(&cx.t("settings-log"), 0.0))
                .with_child(report),
        )
}

/// How many lines of the log the screen shows (the newest).
pub const LOG_LINES: usize = 8;
/// How long a line of the log may be on the screen, in characters.
pub const LOG_LINE_CHARS: usize = 56;

/// `line` cut to `max` characters, with an ellipsis when it was cut.
#[must_use]
pub fn shorten(line: &str, max: usize) -> String {
    if line.chars().count() <= max {
        return line.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = line.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod settings_test;

/// The header of a screen with a back arrow (shared with the editor).
#[must_use]
pub fn header(cx: &Cx, title: &str, back: Action, back_name: &str) -> Dom {
    nav_header(cx, title, back, back_name)
}
