//! The window's sidebar (azul's `Sidebar`), the one AzWriter shares with the other document apps:
//! the OPEN DOCUMENTS (the tabs: each row with its sync pie and its collaboration icon, a "New or
//! open" row after them), then the OUTLINE of the shown document, its PAGES (in the page layout)
//! and its STYLES. Collapsed it is the 18 px rail: the open documents' icons at the top, the
//! formatting quick access (undo, the size, italic, a list, bold LAST, nearest to the thumb) at the
//! bottom, then the status slot (the shown document's save, the page count) and the toggle.
//!
//! What the rows are and how their tags are told apart is `nav.rs` and azul-appkit's `docs`; this
//! file turns it into widgets and runs what the sidebar reports. UI only for the sync and the
//! collaboration: the sync kit that would fill them is a TODO.

use azul::{
    callbacks::{CallbackInfo, LayoutCallbackInfo, RefAny, SidebarOnEventCallbackType, Update},
    dom::{DomId, DomNodeId, ScrollIntoViewOptions},
    str::String as AzString,
    widgets::{
        PieTone, Sidebar, SidebarEvent, SidebarEventKind, SidebarRow, SidebarSection, SidebarState,
        SidebarStatus, SidebarSurface,
    },
};
use azul_appkit::{
    docs::{self, DocSync},
    docs_ui::{self, DocEvent},
    flow::{self, Quick, WRITER_QUICK},
    l10n::{label, t, t_args, Arg},
};

use crate::{
    app::{AppState, BackstagePage, Command, View},
    commands, ids, model, nav,
    ribbon::{current_style, style_commands, STYLES},
};

/// The icon of an open document's row: azul's Haiku pack draws a page of text for it under flora.
const DOC_ICON: &str = "description";

/// The sidebar of the window, as the app's state asks for it (what the widgets need to know of the
/// window itself is added by [`sidebar`]).
#[must_use]
pub(crate) fn build(st: &AppState, app: &RefAny) -> Sidebar {
    let mut sidebar = Sidebar::create()
        .with_section(docs_ui::documents_section(&st.doc_rows(), DOC_ICON, model::UNTITLED))
        .with_quick_access(flow::quick_access(st.doc().map(|d| &d.editor), &WRITER_QUICK))
        .with_label(label("azwriter-sidebar-label"))
        .with_state(st.sidebar)
        .with_surface(SidebarSurface::Theme)
        .with_dom_id(ids::SIDEBAR)
        .with_on_event(app.clone(), on_sidebar as SidebarOnEventCallbackType);
    if let Some(doc) = st.doc() {
        if let Some(section) = outline_section(doc) {
            sidebar = sidebar.with_section(section);
        }
        if st.view == View::Pages {
            sidebar = sidebar.with_section(pages_section(doc));
        }
        sidebar = sidebar.with_section(styles_section(doc));
        for status in statuses(st, doc) {
            sidebar = sidebar.with_status(status);
        }
    }
    sidebar
}

/// The sidebar of the window now, with the window's width and the room of its controls.
#[must_use]
pub(crate) fn sidebar(st: &AppState, app: &RefAny, info: &LayoutCallbackInfo) -> Sidebar {
    azul_appkit::frame::sidebar_in_window(build(st, app), info)
}

/// The OUTLINE section: the headings of the shown document, a click scrolls to one. `None`
/// without a heading.
fn outline_section(doc: &model::DocumentModel) -> Option<SidebarSection> {
    let outline = nav::outline(doc.doc(), model::UNTITLED);
    if outline.rows.is_empty() {
        return None;
    }
    let mut rows: Vec<SidebarRow> = outline
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            SidebarRow::create(row.text.clone())
                .with_tag(nav::outline_tag(i))
                .with_depth(row.depth)
        })
        .collect();
    if outline.hidden > 0 {
        // the cap left some out: say how many (a row nobody can choose)
        rows.push(SidebarRow::create(t_args(
            "azwriter-outline-more",
            &[("count", Arg::from(outline.hidden))],
        )));
    }
    Some(SidebarSection::create(label("azwriter-outline-title")).with_rows(rows))
}

/// The PAGES section (the page layout): "Page 1", "Page 2" ..., the caret's page selected.
fn pages_section(doc: &model::DocumentModel) -> SidebarSection {
    let starts = doc.pages.starts_for(doc.doc().block_count());
    let shown = doc
        .pages
        .page_of(doc.editor.caret_block, doc.editor.caret_byte)
        .min(starts.len().saturating_sub(1));
    let rows: Vec<SidebarRow> = (0..starts.len().min(nav::MAX_PAGE_ROWS))
        .map(|i| {
            SidebarRow::create(t_args("azwriter-page-row", &[("page", Arg::from(i + 1))]))
                .with_tag(nav::page_tag(i))
                .with_selected(i == shown)
        })
        .collect();
    SidebarSection::create(label("azwriter-pages-title")).with_rows(rows)
}

/// The STYLES section: Normal, Heading 1 - 3, Quote, Code; the caret's block's style selected.
fn styles_section(doc: &model::DocumentModel) -> SidebarSection {
    let current = current_style(&doc.editor);
    let rows: Vec<SidebarRow> = STYLES
        .iter()
        .enumerate()
        .map(|(i, (key, _))| {
            SidebarRow::create(label(key))
                .with_tag(nav::style_tag(i))
                .with_selected(i == current)
        })
        .collect();
    SidebarSection::create(label("azwriter-styles-title")).with_rows(rows)
}

/// What the status slot holds: the shown document's save (a pie) and its page count (a badge). The
/// slot is the app's data: nothing here is the widget's.
fn statuses(st: &AppState, doc: &model::DocumentModel) -> Vec<SidebarStatus> {
    let pages = doc.pages.starts_for(doc.doc().block_count()).len();
    vec![
        docs_ui::sync_status(st.sync_of(st.active)),
        SidebarStatus::badge(label("azwriter-status-pages-label"), pages.to_string(), PieTone::Accent)
            .with_detail(t_args("azwriter-status-pages-detail", &[("pages", Arg::from(pages))])),
    ]
}

/// The sentence the notice says when a document's sync pie is pressed: what the state means, and
/// where collaboration stands (UI only, until the sync kit exists).
#[must_use]
pub(crate) fn status_notice(title: &str, sync: DocSync, collab: bool) -> String {
    let mut said = format!("{title}: {}", t(sync.detail_key()));
    if collab {
        said.push(' ');
        said.push_str(&t(docs::COLLAB_NOTE_KEY));
    }
    said
}

/// Scrolls the document so that block `block` stands at the top of its page.
fn scroll_to_block(info: &mut CallbackInfo, block: usize) {
    let dom = DomId { inner: 0 };
    let id = format!("{}{block}", ids::BLOCK_PREFIX);
    let node = info.get_node_id_by_id_attribute(dom, AzString::from(id.as_str()));
    if node.into_raw() != 0 {
        info.scroll_node_into_view(DomNodeId { dom, node }, ScrollIntoViewOptions::start());
    }
}

/// The drawer of a phone closes once something was chosen in it; a desktop keeps its sidebar.
fn after_choice(st: &mut AppState) {
    if st.phone && st.sidebar == SidebarState::Expanded {
        st.sidebar = SidebarState::Rail;
    }
}

/// What the report of the open documents asks of the app.
fn on_doc_event(data: &mut RefAny, info: &mut CallbackInfo, event: DocEvent) -> Update {
    let update = match event {
        DocEvent::Switch(index) => commands::run(data, Command::Switch(index), info),
        DocEvent::New => commands::run(data, Command::OpenBackstage(BackstagePage::Open), info),
        DocEvent::Collab { index, on } => commands::run(data, Command::Collab(index, on), info),
        DocEvent::Status(index) => {
            let Some(mut st) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            let Some(doc) = st.open.get(index) else {
                return Update::DoNothing;
            };
            let said = status_notice(&doc.title(), st.sync_of(index), doc.collab);
            st.notice = said;
            Update::RefreshDom
        }
        DocEvent::Menu(index) => {
            let Some(mut st) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            if index >= st.open.len() {
                return Update::DoNothing;
            }
            st.sheet_doc = Some(index);
            Update::RefreshDom
        }
    };
    if matches!(event, DocEvent::Switch(_) | DocEvent::New) {
        if let Some(mut st) = data.downcast_mut::<AppState>() {
            after_choice(&mut st);
        }
    }
    update
}

/// A row of the app's own was chosen: a style is applied, an outline row or a page scrolls the
/// text there.
fn on_own_row(data: &mut RefAny, info: &mut CallbackInfo, tag: usize) -> Update {
    if let Some(index) = nav::style_of_tag(tag, STYLES.len()) {
        let picked = match data.downcast_ref::<AppState>() {
            Some(st) => match st.doc() {
                Some(doc) => style_commands(&doc.editor, index),
                None => return Update::DoNothing,
            },
            None => return Update::DoNothing,
        };
        let mut update = Update::DoNothing;
        for cmd in picked {
            update.max_self(commands::run(data, Command::Rich(cmd), info));
        }
        return update;
    }
    let block = match data.downcast_ref::<AppState>() {
        Some(st) => match st.doc() {
            Some(doc) => {
                if let Some(index) = nav::outline_of_tag(tag) {
                    nav::outline(doc.doc(), model::UNTITLED).rows.get(index).map(|row| row.block)
                } else if let Some(index) = nav::page_of_tag(tag) {
                    let starts = doc.pages.starts_for(doc.doc().block_count());
                    nav::page_block(&starts, index)
                } else {
                    None
                }
            }
            None => None,
        },
        None => None,
    };
    let Some(block) = block else {
        return Update::DoNothing;
    };
    scroll_to_block(info, block);
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        after_choice(&mut st);
    }
    Update::RefreshDom
}

/// Every report of the sidebar: a document chosen, its collaboration icon or its pie, a long press,
/// a style, an outline row, the toggle, a quick-access button.
extern "C" fn on_sidebar(mut data: RefAny, mut info: CallbackInfo, event: SidebarEvent) -> Update {
    if event.kind == SidebarEventKind::QuickAction {
        return match Quick::of_tag(event.tag) {
            Some(quick) => commands::run(&mut data, Command::Quick(quick), &mut info),
            None => Update::DoNothing,
        };
    }
    if let Some(doc_event) = docs_ui::doc_event(&event) {
        return on_doc_event(&mut data, &mut info, doc_event);
    }
    match event.kind {
        SidebarEventKind::RowClicked => on_own_row(&mut data, &mut info, event.tag),
        // the toggle, the handle, a swipe from the rail, a tap on the pushed-aside view
        SidebarEventKind::StateChanged => {
            let Some(mut st) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            st.sidebar = event.state;
            Update::RefreshDom
        }
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
#[path = "ui_sidebar_test.rs"]
mod ui_sidebar_test;
