//! The preview: azul's `Inspector` (the Preview) docked at the window's right on a desktop and
//! at its foot on a phone, showing THE ITEM ITSELF - the picture, the text, the PDF page, the
//! sound, the video - and nothing else. What there is to say about it (its name, kind, size and
//! date, where it is and how it syncs, Open and Copy link) is behind the (?) in the pane's
//! header; a one-line banner stands at the pane's top only when something needs saying (a sync
//! conflict, a failed transfer), its words in short and its own (?) for the rest.
//!
//! On a desktop the pane shows the selected item (the first one while none is). On a phone it
//! shows the item at the TOP EDGE of the folder view and follows it while the list scrolls
//! ([`follow_top`], from the view's scroll callback): the pane is a virtual view of its own, so
//! a change of the item draws that pane again ([`redraw_pane`]) and nothing else - not the list,
//! not the window. The selection never follows the scroll; only the preview does.
//!
//! The pane's size - its width at the right, its height at the foot - is the user's drag, kept in
//! the settings.

use azul::{
    callbacks::InspectorOnEventCallbackType,
    dom::{DomId, NodeId, TabIndex},
    misc::TransientAnchor,
    prelude::*,
    str::String as AzString,
    widgets::{
        Dialog, DialogClosedBy, Inspector, InspectorDock, InspectorEvent, InspectorEventKind,
        SidebarSurface,
    },
};
use azul_appkit::l10n::label;
use azcloud_kit::sync::session::FileState;

use crate::{
    actions,
    ids,
    model::{clamped_preview_size, Settings},
    save_settings, sync_view, ui_panes, ui_view, with_state, DriveState,
};

// ==== The widget ====

/// The preview for the window now (open or closed: the user's View tab says).
pub(crate) fn preview(s: &DriveState, app: &RefAny, dark: bool) -> Inspector {
    let details = s.settings.details_pane && !s.settings.preview_pane;
    let pane = if details {
        ui_view::on_page(ui_panes::details_pane(s))
    } else {
        pane_dom(s, app, dark)
    };
    let mut preview = Inspector::create(Dom::create_div(), pane)
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
        .with_on_event(app.clone(), on_preview as InspectorOnEventCallbackType);
    if !details {
        if let Some(mark) = details_mark(s, app) {
            preview = preview.with_header_end(mark);
        }
    }
    preview
}

/// The pane's content: a plain DOM on a desktop, a virtual view of its own on a phone (so the
/// item at the list's top edge can change without a new window).
fn pane_dom(s: &DriveState, app: &RefAny, dark: bool) -> Dom {
    if !s.phone {
        return content(s, app, dark);
    }
    Dom::create_virtual_view(RefAny::new(PaneRef { app: app.clone(), dark }), pane_view)
        .with_id(ids::PREVIEW_VIEW)
        .with_css("flex-grow: 1; min-height: 0px; width: 100%;")
}

/// The preview as the window shows it: the item on its stage, the banner over it.
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

/// Draws the phone's pane again in place (its virtual view reads the state when it draws),
/// without a new window DOM. `false` when there is no such pane (a desktop, the pane closed).
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

/// The folder view scrolled: on a phone the preview follows the item now at its top edge - the
/// pane draws it at once; with `settled` (the scroll ended) the item's own picture or text is
/// asked for too, so a long scroll reads no file it passed.
pub(crate) fn follow_top(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, settled: bool) {
    if !s.phone || !s.settings.preview_pane {
        return;
    }
    let Some(key) = top_key(s) else {
        return;
    };
    let changed = s.top_key.as_deref() != Some(key.as_str());
    if changed {
        s.top_key = Some(key);
        redraw_pane(info);
    }
    if settled {
        actions::request_preview(info, app, s);
    }
}

// ==== What the pane's size does ====

/// The size the user dragged the pane to, kept for its dock: the width at the right, the height
/// at the foot.
pub(crate) fn keep_size(settings: &mut Settings, bottom: bool, size: f32) {
    let size = clamped_preview_size(size);
    if bottom {
        settings.preview_height = size;
    } else {
        settings.preview_width = size;
    }
}

/// The pane is dragged (its size now), or let go (where it stays).
extern "C" fn on_preview(mut data: RefAny, mut info: CallbackInfo, event: InspectorEvent) -> Update {
    let bottom = event.dock == InspectorDock::Bottom;
    match event.kind {
        InspectorEventKind::Resized => {
            if let Some(mut s) = data.downcast_mut::<DriveState>() {
                keep_size(&mut s.settings, bottom, event.size);
            }
            Update::DoNothing
        }
        InspectorEventKind::Settled => with_state(&mut data, &mut info, |info, app, s| {
            keep_size(&mut s.settings, bottom, event.size);
            save_settings(info, app, s);
        }),
    }
}

// ==== What needs saying ====

/// What a banner at the pane's top says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Banner {
    /// A Material icon's name.
    pub icon: &'static str,
    /// The words in one line: a key of the resources.
    pub text: &'static str,
    /// Red where it failed, amber where it waits for the user.
    pub failed: bool,
    /// The rest, behind the banner's own (?): what went wrong, for a failed transfer.
    pub detail: Option<String>,
}

/// The banner of an item whose sync is in `state`: a conflict waits for an answer, a failed
/// transfer says why; every other state is the item's own business and has none.
#[must_use]
pub(crate) fn banner_of(state: &FileState) -> Option<Banner> {
    match state {
        FileState::Conflict => Some(Banner {
            icon: "warning",
            text: "azdrive-sync-state-conflict",
            failed: false,
            detail: None,
        }),
        FileState::Error(why) => Some(Banner {
            icon: "error",
            text: "azdrive-sync-state-error-short",
            failed: true,
            detail: Some(why.clone()),
        }),
        _ => None,
    }
}

/// The banner of the item the preview shows now.
pub(crate) fn banner(s: &DriveState) -> Option<Banner> {
    let entry = s.preview_subject()?;
    banner_of(&sync_view::entry_state(s, entry)?)
}

/// The banner as one line over the stage.
pub(crate) fn banner_dom(banner: &Banner) -> Dom {
    let tone = if banner.failed {
        "color: system:danger;"
    } else {
        "color: system:warning;"
    };
    let mut line = Dom::create_div()
        .with_id(ids::PREVIEW_BANNER)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 4px 10px; \
             font-size: 12px; flex-shrink: 0; border-bottom: 1px solid system:separator; {tone}"
        ))
        .with_child(Dom::create_icon(AzString::from(banner.icon)).with_css("font-size: 16px; flex-shrink: 0;"))
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 1; min-width: 0px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;")
                .with_child(Dom::create_span_with_text(label(banner.text))),
        );
    if let Some(detail) = &banner.detail {
        line.add_child(crate::ui_git::info_mark_text(detail));
    }
    line
}

// ==== The (?) ====

/// The (?) in the pane's header: a popover with what there is to say about the item (its name,
/// kind, size, date, place and sync state, Open and Copy link). `None` with no item.
fn details_mark(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let content = ui_panes::preview_info(s, app)?;
    let invoker = Dom::create_div()
        .with_id(ids::PREVIEW_DETAILS)
        .with_css(
            "display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; \
             border-radius: 12px; cursor: pointer; flex-shrink: 0; \
             :hover { background: rgba(127, 127, 127, 0.22); }",
        )
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(label("azdrive-preview-details"))
        .with_child(Dom::create_icon("help_outline").with_css("font-size: 16px; opacity: 0.8;"));
    Some(
        Dialog::create(content)
            .with_invoker(invoker)
            .with_modal(false)
            .with_anchor(if s.phone {
                TransientAnchor::Top
            } else {
                TransientAnchor::Left
            })
            .with_closed_by(DialogClosedBy::Any)
            .with_close_button(false)
            .dom(),
    )
}

#[cfg(test)]
#[path = "ui_preview_test.rs"]
mod ui_preview_test;
