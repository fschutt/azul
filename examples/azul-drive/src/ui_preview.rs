//! The preview: azul's `Preview` docked at the window's right on a desktop and at its foot on a
//! phone, showing THE ITEM ITSELF - the picture, the text, the PDF page, the sound, the video - and
//! nothing else. What there is to say about it (its name, kind, size and date, where it is and how
//! it syncs, Open and Copy link) is behind the (?) of the pane's header (the widget's
//! `PreviewDetail`, a popover); a one-line banner stands at the pane's top only when something
//! needs saying (a sync conflict, a failed transfer, only the start of a long text shown: the widget's
//! `PreviewBanner`), its words in short and its own (?) for the rest.
//!
//! The pane shows the selected item (the first one while none is). On a phone the selection IS the
//! row at the TOP EDGE of the folder view while the list scrolls ([`follow_scroll`], from the view's
//! scroll callback; the rules are `follow`'s): the pane is a virtual view of its own, so a change
//! of the row draws that pane again ([`redraw_pane`]) and the rows in place - not the window; a
//! tap pins a row until the list scrolls again. A desktop's scroll leaves the selection alone.
//!
//! The pane's size - its width at the right, its height at the foot - is the user's drag, kept in
//! the settings.

use azcloud_kit::sync::session::FileState;
use azul::{
    callbacks::PreviewOnEventCallbackType,
    dom::{DomId, NodeId},
    prelude::*,
    str::String as AzString,
    widgets::{
        Preview, PreviewBanner, PreviewBannerTone, PreviewDetail, PreviewDock, PreviewEvent,
        PreviewEventKind, SidebarSurface,
    },
};
use azul_appkit::l10n::label;

use crate::{
    actions, follow, ids,
    jobs::PreviewContent,
    model::{clamped_preview_size, Settings},
    save_settings, sync_view, ui_panes, ui_view, with_state, DriveState, PreviewState,
};

// ==== The widget ====

/// The preview for the window now (open or closed: the user's View tab says).
pub(crate) fn preview(s: &DriveState, app: &RefAny, dark: bool) -> Preview {
    let details = s.settings.details_pane && !s.settings.preview_pane;
    let pane = if details {
        ui_view::on_page(ui_panes::details_pane(s))
    } else {
        pane_dom(s, app, dark)
    };
    let mut preview = Preview::create(Dom::create_div(), pane)
        .with_title(label(if details {
            "azdrive-pane-details"
        } else {
            "azdrive-preview-title"
        }))
        .with_open(s.preview_shown())
        .with_size(if s.phone {
            s.settings.preview_height
        } else {
            s.settings.preview_width
        })
        .with_form(s.form)
        .with_surface(SidebarSurface::Theme)
        .with_on_event(app.clone(), on_preview as PreviewOnEventCallbackType);
    if !details {
        // What there is to say about the item: behind the (?), never in the pane.
        if let Some(info) = ui_panes::preview_info(s, app) {
            preview = preview.with_detail(PreviewDetail::create(label("azdrive-preview-details"), info));
        }
        if let Some(banner) = banner(s) {
            preview = preview.with_banner(banner_widget(&banner));
        }
    }
    preview
}

/// The pane's content: a plain DOM on a desktop, a virtual view of its own on a phone (so the item
/// at the list's top edge can change without a new window).
fn pane_dom(s: &DriveState, app: &RefAny, dark: bool) -> Dom {
    if !s.phone {
        return content(s, app, dark);
    }
    Dom::create_virtual_view(RefAny::new(PaneRef { app: app.clone(), dark }), pane_view)
        .with_id(ids::PREVIEW_VIEW)
        .with_css("flex-grow: 1; min-height: 0px; width: 100%;")
}

/// The preview as the window shows it: the item on its stage.
fn content(s: &DriveState, app: &RefAny, dark: bool) -> Dom {
    ui_view::on_page(ui_panes::preview_pane(s, app, dark))
}

/// What the phone's pane carries.
struct PaneRef {
    app: RefAny,
    dark: bool,
}

fn rect(w: f32, h: f32) -> LogicalRect {
    LogicalRect::create(LogicalPosition::create(0.0, 0.0), LogicalSize::create(w, h))
}

/// The phone's pane as a virtual view: the content, read from the state when the view draws.
extern "C" fn pane_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some((app, dark)) = data.downcast_ref::<PaneRef>().map(|r| (r.app.clone(), r.dark)) else {
        return VirtualViewReturn::default();
    };
    let mut state = app.clone();
    let Some(s) = state.downcast_ref::<DriveState>() else {
        return VirtualViewReturn::default();
    };
    let size = info.bounds.get_logical_size();
    let (w, h) = (size.width.max(1.0), size.height.max(1.0));
    VirtualViewReturn::with_dom(content(&s, &app, dark), rect(w, h), rect(w, h))
}

/// Draws the phone's pane again in place (its virtual view reads the state when it draws), without
/// a new window DOM. `false` when there is no such pane (a desktop, the pane closed).
pub(crate) fn redraw_pane(info: &mut CallbackInfo) -> bool {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, ids::PREVIEW_VIEW);
    let Some(raw) = node.into_raw().checked_sub(1) else {
        return false;
    };
    info.trigger_virtual_view_rerender(dom, NodeId { inner: raw });
    true
}

// ==== The item at the top edge ====

/// The key of the item at the top edge of the folder view now, from where it last scrolled to;
/// `None` for no item.
#[must_use]
pub(crate) fn top_key(s: &DriveState) -> Option<String> {
    let first = ui_view::items_in_view(s).start;
    ui_view::shown_order(s).get(first).map(|e| e.key.clone())
}

/// The folder view scrolled to `offset` (`settled`: the scroll ended). On a phone the selection and
/// the preview follow the row at its top edge (`follow`): the row is selected at once, the pane and
/// the rows draw it again in place - only they, not the window - and, once the scroll has settled,
/// the item's own picture or text is asked for, so a long scroll reads no file it passed.
pub(crate) fn follow_scroll(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    offset: f32,
    settled: bool,
) {
    if !follow::follows_scroll(s.phone) || ui_view::uses_icon_grid(s) {
        return;
    }
    let top = top_key(s);
    let selected = s.selection.keys();
    let selected: Vec<&str> = selected.iter().map(String::as_str).collect();
    let step = s.follow.scrolled(offset, top.as_deref(), &selected);
    if let follow::Step::Select(key) = step {
        s.selection.click(&key);
        s.type_ahead.clear();
        s.print_selection();
        redraw_pane(info);
        ui_view::redraw_rows(info);
    }
    if follow::load_due(settled) {
        actions::request_preview(info, app, s);
    }
}

// ==== What the pane's size does ====

/// The size the user dragged the pane to, kept for its dock: the width at the right, the height at
/// the foot.
pub(crate) fn keep_size(settings: &mut Settings, bottom: bool, size: f32) {
    let size = clamped_preview_size(size);
    if bottom {
        settings.preview_height = size;
    } else {
        settings.preview_width = size;
    }
}

/// The pane is dragged (its size now), or let go (where it stays).
extern "C" fn on_preview(mut data: RefAny, mut info: CallbackInfo, event: PreviewEvent) -> Update {
    let bottom = event.dock == PreviewDock::Bottom;
    match event.kind {
        PreviewEventKind::Resized => {
            if let Some(mut s) = data.downcast_mut::<DriveState>() {
                keep_size(&mut s.settings, bottom, event.size);
            }
            Update::DoNothing
        }
        PreviewEventKind::Settled => with_state(&mut data, &mut info, |info, app, s| {
            keep_size(&mut s.settings, bottom, event.size);
            save_settings(info, app, s);
        }),
    }
}

// ==== What needs saying ====

/// How urgent a banner is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    /// A plain remark (only the start of the file is shown).
    Info,
    /// It waits for the user (a sync conflict).
    Warning,
    /// It failed (a transfer).
    Danger,
}

/// What a banner at the pane's top says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Banner {
    /// A Material icon's name.
    pub icon: &'static str,
    /// The words in one line: a key of the resources.
    pub text: &'static str,
    pub tone: Tone,
    /// The rest, behind the banner's own (?): what went wrong, how much of the file is shown.
    pub detail: Option<String>,
}

/// The banner of an item whose sync is in `state`: a conflict waits for an answer, a failed transfer
/// says why; every other state is the item's own business and has none.
#[must_use]
pub(crate) fn banner_of(state: &FileState) -> Option<Banner> {
    match state {
        FileState::Conflict => Some(Banner {
            icon: "warning",
            text: "azdrive-sync-state-conflict",
            tone: Tone::Warning,
            detail: None,
        }),
        FileState::Error(why) => Some(Banner {
            icon: "error",
            text: "azdrive-sync-state-error-short",
            tone: Tone::Danger,
            detail: Some(why.clone()),
        }),
        _ => None,
    }
}

/// The banner of a text file whose preview shows only its start (`size` bytes in all): the pane holds
/// the text and nothing else, so the line over it says so.
#[must_use]
pub(crate) fn cut_banner(size: Option<u64>) -> Banner {
    Banner {
        icon: "content_cut",
        text: "azdrive-preview-cut-banner",
        tone: Tone::Info,
        detail: size.map(|bytes| {
            azul_appkit::l10n::t_args(
                "azdrive-preview-cut-detail",
                &[("size", azul_appkit::l10n::Arg::from(crate::browse::format_size(Some(bytes))))],
            )
        }),
    }
}

/// The banner of the item the preview shows now: its sync's trouble first, else that only the start of
/// its text is shown.
pub(crate) fn banner(s: &DriveState) -> Option<Banner> {
    let entry = s.preview_subject()?;
    if let Some(banner) = sync_view::entry_state(s, entry).and_then(|state| banner_of(&state)) {
        return Some(banner);
    }
    let cut = matches!(
        &s.preview,
        Some(PreviewState {
            key,
            content: Some(PreviewContent::Text { cut: true, .. }),
        }) if *key == entry.key
    );
    cut.then(|| cut_banner(entry.size))
}

/// The widget's banner of `banner`: its words, its tone, the rest behind its own (?).
fn banner_widget(banner: &Banner) -> PreviewBanner {
    let tone = match banner.tone {
        Tone::Info => PreviewBannerTone::Info,
        Tone::Warning => PreviewBannerTone::Warning,
        Tone::Danger => PreviewBannerTone::Danger,
    };
    let mut widget = PreviewBanner::create(label(banner.text), tone).with_icon(AzString::from(banner.icon));
    if let Some(detail) = &banner.detail {
        widget = widget.with_details(
            Dom::create_div()
                .with_css("padding: 10px 12px; font-size: 12px; max-width: 320px;")
                .with_child(Dom::create_span_with_text(AzString::from(detail.as_str()))),
        );
    }
    widget
}

#[cfg(test)]
#[path = "ui_preview_test.rs"]
mod ui_preview_test;
