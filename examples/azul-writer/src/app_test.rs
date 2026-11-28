//! The app's state: the open documents and which one is shown, what each row of the sidebar says
//! of its document, the close question, the views and the pages of the backstage.

use azul_appkit::docs::DocSync;

use super::*;

fn state() -> AppState {
    AppState::new(RefAny::new(()), PathBuf::from("data"), false)
}

fn typed(id: &str, markdown: &str) -> DocumentModel {
    // a document with typed text that was never saved: it has changes
    DocumentModel::from_doc(
        id.to_string(),
        azul::widgets::RichTextDoc::create_from_markdown(markdown),
        String::new(),
    )
}

fn saved(id: &str, markdown: &str) -> DocumentModel {
    // a document whose file holds exactly what is on screen (the Markdown it writes)
    let mut doc = DocumentModel::from_markdown(id.to_string(), markdown);
    doc.saved = doc.markdown();
    doc
}

#[test]
fn the_backstage_pages_and_the_views_are_found_by_index() {
    for (i, page) in BACKSTAGE_NAV.iter().enumerate() {
        assert_eq!(page.index(), i);
    }
    assert_eq!(View::Pages.index(), 1);
    assert_eq!(View::default(), View::Flow, "the text flows by default; the page layout is a View option");
    assert_eq!(VIEWS[0], View::Flow);
}

#[test]
fn the_ribbon_has_the_five_bands_of_the_card_after_file_and_every_tab_names_an_icon() {
    assert_eq!(TABS.len(), 6);
    assert_eq!(TABS[0], "azwriter-tab-file");
    assert_eq!(
        &TABS[1..],
        ["azwriter-tab-home", "azwriter-tab-insert", "azwriter-tab-layout", "azwriter-tab-review", "azwriter-tab-view"]
    );
    assert_eq!(TAB_ICONS.len(), TABS.len(), "a phone's pills are icon squares: every tab names one");
    assert!(TAB_ICONS.iter().all(|i| !i.is_empty()));
    assert_eq!(START_TAB, 0, "the window starts on File, the location of the document");
}

#[test]
fn nothing_is_open_in_a_fresh_state() {
    let st = state();
    assert!(st.doc().is_none());
    assert!(!st.is_dirty() && !st.any_dirty() && !st.is_saving());
    assert_eq!(st.title(), "AzWriter");
    assert!(st.doc_rows().is_empty());
    assert_eq!(st.sync_of(0), DocSync::Saved, "no document: nothing to say");
    assert!(st.can_open_more());
}

#[test]
fn the_shown_document_is_the_one_at_active_and_its_title_is_the_windows() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(saved("b", "# Beta\n"));
    st.active = 1;
    assert_eq!(st.doc().map(|d| d.id.as_str()), Some("b"));
    assert_eq!(st.title(), "Beta - AzWriter");
    assert_eq!(st.index_of("a"), Some(0));
    assert_eq!(st.index_of("zzz"), None);
    assert!(st.doc_by_id("a").is_some());
    st.active = 7;
    assert!(st.doc().is_none(), "an index past the list shows nothing instead of panicking");
}

#[test]
fn a_row_says_where_the_save_of_its_document_stands() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(typed("b", "# Beta\n\nunsaved words\n"));
    st.open.push(saved("c", "# Gamma\n"));
    assert_eq!(st.sync_of(0), DocSync::Saved);
    assert_eq!(st.sync_of(1), DocSync::Unsaved, "typed text no file has");
    st.pending_saves.push(("b".to_string(), "# Beta\n".to_string()));
    assert_eq!(st.sync_of(1), DocSync::Saving, "a write on its way wins");
    assert_eq!(st.sync_of(0), DocSync::Saved, "another document's write is not this one's");
    st.pending_saves.clear();
    st.open[2].save_failed = true;
    assert_eq!(st.sync_of(2), DocSync::Failed);
}

#[test]
fn the_sidebar_lists_the_open_documents_with_the_shown_one_selected() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(saved("b", "# Beta\n"));
    st.open[1].collab = true;
    st.active = 1;
    let rows = st.doc_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Alpha");
    assert!(!rows[0].selected && rows[1].selected);
    assert!(!rows[0].collab && rows[1].collab);
}

#[test]
fn at_most_the_most_open_documents_may_be_open() {
    let mut st = state();
    for i in 0..azul_appkit::docs::MAX_OPEN_DOCS {
        assert!(st.can_open_more(), "document {i}");
        st.open.push(DocumentModel::untitled(format!("d{i}")));
    }
    assert!(!st.can_open_more());
}

#[test]
fn some_document_with_changes_is_what_the_close_guard_asks_about() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(typed("b", "# Beta\n\nunsaved words\n"));
    st.active = 0;
    assert!(!st.is_dirty(), "the shown one is clean");
    assert!(st.any_dirty(), "another one is not: closing the window would lose it");
}

#[test]
fn dont_save_drops_the_changes_so_the_close_it_asks_for_is_not_held_again() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(typed("b", "# Typed\n\nNot saved.\n"));
    st.active = 1;
    st.asking_close = true;
    assert!(st.any_dirty());
    st.discard_document();
    assert!(
        !st.any_dirty(),
        "the guard judges the close by the DOM rebuilt after the answer: a dirty document \
         there holds the close and asks again"
    );
    assert!(!st.asking_close, "the question is answered");
    assert!(st.pending_saves.is_empty(), "nothing was written");
    assert_eq!(st.open.len(), 1, "the clean document stays open");
    assert_eq!(st.active, 0, "the shown index stays inside the list");
}

#[test]
fn dont_save_on_the_only_document_leaves_nothing_open() {
    let mut st = state();
    st.open.push(typed("a", "# Typed\n\nNot saved.\n"));
    st.asking_close = true;
    st.discard_document();
    assert!(st.open.is_empty());
    assert!(!st.any_dirty());
    assert_eq!(st.active, 0);
}
