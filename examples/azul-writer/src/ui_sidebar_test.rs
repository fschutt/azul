//! What the sidebar lists: the open documents with their pies and icons, the outline, the pages,
//! the styles, the rail's quick access and the status slot.

use std::path::PathBuf;

use azul::widgets::SidebarCollab;

use super::*;
use crate::model::DocumentModel;

fn state() -> AppState {
    crate::l10n::in_english();
    AppState::new(RefAny::new(()), PathBuf::from("data"), false)
}

fn saved(id: &str, markdown: &str) -> DocumentModel {
    let mut doc = DocumentModel::from_markdown(id.to_string(), markdown);
    doc.saved = doc.markdown();
    doc
}

fn titles(sidebar: &Sidebar) -> Vec<String> {
    sidebar.sections.as_slice().iter().map(|s| s.title.as_str().to_string()).collect()
}

fn rows_of(sidebar: &Sidebar, section: usize) -> Vec<SidebarRow> {
    sidebar.sections.as_slice()[section].rows.as_slice().to_vec()
}

#[test]
fn with_nothing_open_the_sidebar_is_the_list_of_open_documents_and_its_new_row_alone() {
    let sidebar = build(&state(), &RefAny::new(()));
    assert_eq!(titles(&sidebar), vec!["Open documents"]);
    let rows = rows_of(&sidebar, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label.as_str(), "New or open");
    assert_eq!(rows[0].tag, docs::NEW_TAG);
    assert!(sidebar.statuses.as_slice().is_empty(), "no document: no save to show");
}

#[test]
fn every_open_document_is_a_row_with_its_pie_and_its_collaboration_icon() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(saved("b", "# Beta\n"));
    st.open[1].collab = true;
    st.active = 1;
    let sidebar = build(&st, &RefAny::new(()));
    let rows = rows_of(&sidebar, 0);
    assert_eq!(rows.len(), 3, "two documents and the New row");
    assert_eq!(rows[0].label.as_str(), "Alpha");
    assert_eq!(rows[1].label.as_str(), "Beta");
    assert_eq!(rows[0].tag, docs::doc_tag(0));
    assert_eq!(rows[1].tag, docs::doc_tag(1));
    assert!(!rows[0].selected && rows[1].selected, "the shown document is the selected row");
    assert!(rows[0].has_pie() && rows[1].has_pie());
    assert_eq!(rows[0].collab, SidebarCollab::Off);
    assert_eq!(rows[1].collab, SidebarCollab::On);
    assert!(rows[0].is_rail_button() && rows[1].is_rail_button(), "the rail keeps the documents' icons");
}

#[test]
fn a_documents_pie_is_full_and_green_when_it_is_saved_and_a_wedge_when_it_has_changes() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(DocumentModel::from_doc(
        "b".to_string(),
        azul::widgets::RichTextDoc::create_from_markdown("# Typed\n\nwords no file has\n"),
        String::new(),
    ));
    let sidebar = build(&st, &RefAny::new(()));
    let rows = rows_of(&sidebar, 0);
    assert_eq!(rows[0].pie_fraction, 1.0);
    assert!(rows[1].pie_fraction < 1.0, "unsaved changes are a wedge: {}", rows[1].pie_fraction);
}

#[test]
fn the_outline_lists_the_headings_and_a_document_without_one_has_no_outline_section() {
    let mut st = state();
    st.open.push(saved("a", "# Title\n\ntext\n\n## One\n\nmore\n\n## Two\n"));
    let sidebar = build(&st, &RefAny::new(()));
    assert_eq!(titles(&sidebar), vec!["Open documents", "Outline", "Styles"]);
    let outline = rows_of(&sidebar, 1);
    let labels: Vec<&str> = outline.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, vec!["Title", "One", "Two"]);
    assert_eq!(outline[1].depth, 1);
    assert!(outline.iter().all(|r| !r.is_rail_button()), "an outline row is no button of the rail");

    st.open[0] = saved("a", "just words\n");
    assert_eq!(titles(&build(&st, &RefAny::new(()))), vec!["Open documents", "Styles"]);
}

#[test]
fn the_pages_are_listed_in_the_page_layout_only() {
    let mut st = state();
    st.open.push(saved("a", "# Title\n\ntext\n"));
    assert!(!titles(&build(&st, &RefAny::new(()))).contains(&"Pages".to_string()), "the flow has no pages");
    st.view = View::Pages;
    let sidebar = build(&st, &RefAny::new(()));
    assert!(titles(&sidebar).contains(&"Pages".to_string()));
    let at = titles(&sidebar).iter().position(|t| t == "Pages").expect("a Pages section");
    let rows = rows_of(&sidebar, at);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label.as_str(), "Page 1");
    assert!(rows[0].selected);
}

#[test]
fn the_styles_are_the_ribbons_six_with_the_carets_style_selected() {
    let mut st = state();
    st.open.push(saved("a", "## Heading\n"));
    let sidebar = build(&st, &RefAny::new(()));
    let at = titles(&sidebar).iter().position(|t| t == "Styles").expect("a Styles section");
    let rows = rows_of(&sidebar, at);
    let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, vec!["Normal", "Heading 1", "Heading 2", "Heading 3", "Quote", "Code"]);
    assert_eq!(rows.iter().position(|r| r.selected), Some(2), "the caret starts in the heading 2 of block 0");
}

#[test]
fn the_rails_quick_access_ends_with_bold_nearest_the_thumb() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    let sidebar = build(&st, &RefAny::new(()));
    let quick = sidebar.quick_access.as_slice();
    assert_eq!(quick.len(), WRITER_QUICK.len());
    assert_eq!(quick.first().map(|q| q.label.as_str()), Some("Undo"));
    assert_eq!(quick.last().map(|q| q.label.as_str()), Some("Bold"));
    assert_eq!(quick.last().map(|q| q.tag), Some(Quick::Bold.tag()));
    assert!(quick.iter().all(|q| q.tag < flow::TAG_LIMIT));
}

#[test]
fn with_no_document_the_quick_access_is_disabled() {
    let sidebar = build(&state(), &RefAny::new(()));
    assert!(sidebar
        .quick_access
        .as_slice()
        .iter()
        .all(|q| q.state == azul::widgets::SidebarQuickState::Disabled));
}

#[test]
fn the_status_slot_holds_the_shown_documents_save_and_its_page_count() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    let sidebar = build(&st, &RefAny::new(()));
    let statuses = sidebar.statuses.as_slice();
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].text.as_str(), "Saved");
    assert_eq!(statuses[1].text.as_str(), "1", "one page");
}

#[test]
fn the_sidebar_starts_where_the_state_says() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.sidebar = SidebarState::Expanded;
    assert_eq!(build(&st, &RefAny::new(())).state, SidebarState::Expanded);
    st.sidebar = SidebarState::Rail;
    assert_eq!(build(&st, &RefAny::new(())).state, SidebarState::Rail);
}

#[test]
fn a_pressed_pie_says_what_the_save_means_and_where_collaboration_stands() {
    crate::l10n::in_english();
    let saved_note = status_notice("Alpha", DocSync::Saved, false);
    assert_eq!(saved_note, "Alpha: The document is saved on this device.");
    let collab = status_notice("Alpha", DocSync::Unsaved, true);
    assert!(collab.starts_with("Alpha: The changes are written"), "{collab}");
    assert!(collab.contains("sync kit"), "the note says collaboration is not connected yet: {collab}");
}
