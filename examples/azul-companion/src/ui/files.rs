//! The files (screen C): the workspace one folder at a time, so the tree on the Mac can stay closed. A breadcrumb to go
//! back up, the folder's rows (a folder asks the Mac for its rows, a file opens in AzReview), each file in the colour
//! AzReview gives it: green nothing unresolved, yellow an unresolved suggestion, blue an agent at work.

use azul::{
    prelude::*,
    widgets::{Spinner, SpinnerStyle},
};
use azul_review_link::frame::{Entry, EntryKind, FileState};

use super::kit::{column, div, hairline, icon, px, row, tap, text, text_clip, Cx};
use super::remote::{state_colour, tab_bar};
use super::{Action, Screen};
use crate::layout::Zone;
use crate::theme as th;

/// The Files screen in portrait: the breadcrumb, the rows, the tab bar.
#[cfg(test)]
#[path = "files_test.rs"]
mod files_test;

#[must_use]
pub fn dom(cx: &Cx) -> Dom {
    let z = &cx.z;
    if cx.env.landscape() {
        // the remote's own screen has the panel
        return super::remote::dom(cx);
    }
    column(&format!("width: {}; height: {};", px(z.safe.w), px(z.safe.h)))
        .with_child(crumb_bar(cx, z.header, true))
        .with_child(list(cx, z.side_body))
        .with_child(tab_bar(cx, z.tabs))
}

/// The files in the landscape side panel: the breadcrumb, then the rows.
#[must_use]
pub fn panel(cx: &Cx, zone: Zone) -> Dom {
    let bar = Zone { x: zone.x, y: zone.y, w: zone.w, h: 48.0 };
    let rows = Zone { x: zone.x, y: zone.y + bar.h, w: zone.w, h: (zone.h - bar.h).max(0.0) };
    column(&format!("width: {}; height: {};", px(zone.w), px(zone.h)))
        .with_child(crumb_bar(cx, bar, false))
        .with_child(list(cx, rows))
}

/// The folders of the path on screen, from the workspace down.
fn crumb_names(dir: &str) -> Vec<&str> {
    dir.split('/').filter(|s| !s.is_empty()).collect()
}

/// What the breadcrumb shows of a path that is deeper than the room: the workspace, a key for the folders that are left
/// out (it goes to the last of them), and the last `room` folders. `(depth, name)` for each key: depth is how many folders
/// of the path lead to it; the left-out folders' key is named `"…"`.
#[must_use]
pub fn crumb_plan(names: &[&str], room: usize) -> Vec<(usize, String)> {
    let room = room.max(1);
    let mut plan = Vec::new();
    let skipped = names.len().saturating_sub(room);
    if skipped > 0 {
        plan.push((skipped, "…".to_string()));
    }
    for (i, name) in names.iter().enumerate().skip(skipped) {
        plan.push((i + 1, (*name).to_string()));
    }
    plan
}

fn crumb(cx: &Cx, name: &str, depth: usize, last: bool, home: bool) -> Dom {
    let colour = if last { th::TEXT } else { th::ACCENT };
    let mut key = tap(name, "height: 44px; min-width: 44px; padding: 0px 10px; border-radius: 10px; flex-direction: row; gap: 6px;", cx, Action::Crumb(depth));
    if home {
        key.add_child(icon("home", 20.0, colour));
    }
    key.with_child(text(name, &format!("font-size: 15px; font-weight: {}; color: {colour}; white-space: nowrap;", if last { 600 } else { 500 })))
}

/// The folders back to the workspace, each a key; the gear at the end in portrait.
fn crumb_bar(cx: &Cx, zone: Zone, with_gear: bool) -> Dom {
    let view = cx.view();
    let folder = if view.pen.as_ref().is_some_and(|p| !p.folder.is_empty()) {
        view.pen.as_ref().map(|p| p.folder.clone()).unwrap_or_default()
    } else {
        cx.t("files-root")
    };
    let names = crumb_names(&view.files.dir);
    let room = if with_gear { 3 } else { 2 };
    let mut crumbs = row("flex-grow: 1; min-width: 0px; overflow: hidden; gap: 2px;")
        .with_class("az-scroll")
        .with_accessibility_name(cx.t("files-breadcrumb").as_str());
    crumbs.add_child(crumb(cx, &folder, 0, names.is_empty(), true));
    for (depth, name) in crumb_plan(&names, room) {
        crumbs.add_child(icon("chevron_right", 18.0, th::TEXT_DIM));
        crumbs.add_child(crumb(cx, &name, depth, depth == names.len(), false));
    }
    let mut bar = row(&format!(
        "width: {}; height: {}; padding: 0px {}; gap: 8px; border-bottom: 1px solid {}; flex-shrink: 0;",
        px(zone.w),
        px(zone.h),
        px(if with_gear { cx.z.gutter - 6.0 } else { 6.0 }),
        th::SEPARATOR
    ))
    .with_child(crumbs);
    if with_gear {
        bar.add_child(tap(&cx.t("nav-settings"), "width: 44px; height: 44px; border-radius: 22px;", cx, Action::OpenSettings).with_child(icon("settings", 24.0, th::TEXT_DIM)));
    }
    bar
}

/// The path of an entry of the folder on screen.
fn path_of(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// What a state is called for the screen reader.
fn state_words(cx: &Cx, state: FileState) -> Option<String> {
    match state {
        FileState::None => None,
        FileState::Clean => Some(cx.t("files-state-clean")),
        FileState::Unresolved => Some(cx.t("files-state-unresolved")),
        FileState::Working => Some(cx.t("files-state-working")),
    }
}

fn entry_row(cx: &Cx, entry: &Entry, height: f32) -> Dom {
    let view = cx.view();
    let path = path_of(&view.files.dir, &entry.name);
    let is_dir = entry.kind == EntryKind::Dir;
    let under_review = view.pen.as_ref().is_some_and(|p| !is_dir && p.file() == path);
    let just_opened = cx.root.ui.opened.as_ref().is_some_and(|(p, _)| *p == path);
    let (glyph, glyph_colour) = if is_dir { ("folder", th::ACCENT) } else { ("description", th::TEXT_DIM) };
    let background = if just_opened {
        th::tint(th::STATE_DONE, 0.22)
    } else if under_review {
        th::tint(th::ACCENT, 0.20)
    } else {
        String::from("transparent")
    };
    let kind = cx.t(if is_dir { "files-folder" } else { "files-file" });
    let mut name = format!("{}, {kind}", entry.name);
    if let Some(state) = state_words(cx, entry.state) {
        name = format!("{name}, {state}");
    }
    if under_review {
        name = format!("{name}, {}", cx.t("files-under-review"));
    }
    let action = if is_dir { Action::OpenDir(path) } else { Action::OpenFile(path) };
    let mut key = tap(
        &name,
        &format!(
            "width: 100%; height: {}; flex-direction: row; justify-content: flex-start; gap: 12px; padding: 0px {}; \
             background: {background}; border-bottom: 1px solid {};",
            px(height),
            px(cx.z.gutter),
            th::SEPARATOR
        ),
        cx,
        action,
    )
    .with_child(icon(glyph, 24.0, glyph_colour))
    .with_class("az-row")
    .with_child(text_clip(
        &entry.name,
        &format!("flex-grow: 1; min-width: 0px; text-align: left; font-size: 17px; font-weight: {};", if under_review { 600 } else { 400 }),
    ));
    if let Some(colour) = state_colour(entry.state) {
        key.add_child(div(&format!("width: 12px; height: 12px; border-radius: 6px; background: {colour}; flex-shrink: 0;")));
    }
    if is_dir {
        key.add_child(icon("chevron_right", 22.0, th::TEXT_DIM));
    }
    key
}

/// A line that is not a file: loading, an error, an empty folder.
fn note_row(cx: &Cx, height: f32) -> Dom {
    row(&format!("width: 100%; height: {}; padding: 0px {}; gap: 12px;", px(height), px(cx.z.gutter)))
}

/// What went wrong with a folder, in words: the connection names it with a short code (`not-connected`, `no-answer`, ...); a code this
/// build has no words for is shown as it is.
fn why_words(cx: &Cx, why: &str) -> String {
    let key = format!("files-why-{why}");
    if cx.root.strings.template(cx.env.lang, &key).is_some() {
        cx.t(&key)
    } else {
        why.to_string()
    }
}

/// The rows of the folder.
fn list(cx: &Cx, zone: Zone) -> Dom {
    let files = &cx.view().files;
    let height = if cx.env.landscape() { 52.0 } else { 56.0 };
    let mut rows = column(&format!(
        "width: {}; height: {}; overflow-y: auto; overflow-x: hidden; flex-shrink: 0;",
        px(zone.w),
        px(zone.h)
    ))
    .with_class("az-scroll");
    if let Some(why) = &files.error {
        rows.add_child(
            note_row(cx, 64.0)
                .with_child(icon("warning", 22.0, th::WARN))
                .with_child(text(&cx.tf("files-error", &[("why", &why_words(cx, why))]), &format!("flex-grow: 1; font-size: 14px; color: {};", th::WARN)))
                .with_child(tap(&cx.t("files-retry"), &format!("height: 44px; padding: 0px 12px; border-radius: 10px; background: {};", th::SURFACE), cx, Action::RetryFiles).with_child(text(
                    &cx.t("files-retry"),
                    &format!("font-size: 14px; font-weight: 600; color: {};", th::ACCENT),
                ))),
        );
    }
    if files.loading() {
        rows.add_child(
            note_row(cx, 48.0)
                .with_child(Spinner::with_size(18).with_color(ColorU { r: 90, g: 175, b: 255, a: 255 }).with_indicator(SpinnerStyle::Spokes).dom())
                .with_child(text(&cx.t("files-loading"), &format!("font-size: 14px; color: {};", th::TEXT_DIM))),
        );
    }
    for entry in &files.entries {
        rows.add_child(entry_row(cx, entry, height));
    }
    if files.entries.is_empty() && !files.loading() && files.error.is_none() {
        rows.add_child(note_row(cx, 64.0).with_child(text(&cx.t("files-empty"), &format!("font-size: 15px; color: {};", th::TEXT_DIM))));
    }
    if files.more && !files.loading() {
        rows.add_child(
            tap(&cx.t("files-more"), &format!("width: 100%; height: 52px; flex-direction: row; gap: 8px; border-bottom: 1px solid {};", th::SEPARATOR), cx, Action::LoadMore)
                .with_child(icon("expand_more", 22.0, th::ACCENT))
                .with_child(text(&cx.t("files-more"), &format!("font-size: 15px; font-weight: 600; color: {};", th::ACCENT))),
        );
    }
    let _ = (hairline, Screen::Files);
    rows
}
