//! What the commands do to the state that needs no window: importing, which documents are open and
//! where a new one goes, the cap, the word count sentence.

use super::*;
use crate::app::View;

fn state() -> AppState {
    AppState::new(RefAny::new(()), std::path::PathBuf::from("data"), false)
}

fn saved(id: &str, markdown: &str) -> DocumentModel {
    let mut doc = DocumentModel::from_markdown(id.to_string(), markdown);
    doc.saved = doc.markdown();
    doc
}

#[test]
fn the_answer_of_an_import_read_is_a_document_or_a_sentence() {
    crate::l10n::in_english();
    let doc = imported("notes.md", Ok(Some(b"# Hello\n\nworld\n".to_vec()))).expect("markdown");
    assert_eq!(crate::model::title_of(&doc), "Hello");
    let docx = include_bytes!("../testdata/sample.docx").to_vec();
    let doc = imported("Report.docx", Ok(Some(docx))).expect("word");
    assert_eq!(crate::model::title_of(&doc), "A Real Heading");
    assert_eq!(
        imported("gone.md", Ok(None)).err().as_deref(),
        Some("gone.md is gone.")
    );
    assert_eq!(
        imported("locked.md", Err("permission denied".to_string())).err().as_deref(),
        Some("locked.md could not be read: permission denied")
    );
    let broken = imported("broken.docx", Ok(Some(b"not a zip".to_vec()))).expect_err("not Word");
    assert!(broken.starts_with("broken.docx could not be imported: "), "{broken}");
}

#[test]
fn an_import_reads_word_by_name_and_markdown_otherwise() {
    let doc = import_bytes("notes.md", b"# Hello\n\nworld\n").expect("markdown");
    assert_eq!(crate::model::title_of(&doc), "Hello");
    let docx = include_bytes!("../testdata/sample.docx");
    let doc = import_bytes("Report.DOCX", docx).expect("word");
    assert_eq!(crate::model::title_of(&doc), "A Real Heading");
}

#[test]
fn a_blank_document_nobody_saved_is_pristine_and_typed_or_saved_ones_are_not() {
    assert!(is_pristine(&DocumentModel::untitled("a".to_string())));
    assert!(!is_pristine(&saved("b", "# Words\n")));
    let typed = DocumentModel::from_doc(
        "c".to_string(),
        azul::widgets::RichTextDoc::create_from_markdown("typed\n"),
        String::new(),
    );
    assert!(!is_pristine(&typed));
}

#[test]
fn opening_a_document_replaces_the_untouched_blank_one() {
    let mut st = state();
    st.open.push(DocumentModel::untitled("blank".to_string()));
    assert!(show_document(&mut st, saved("a", "# Alpha\n")));
    assert_eq!(st.open.len(), 1, "no empty tab is left behind");
    assert_eq!(st.doc().map(|d| d.id.as_str()), Some("a"));
}

#[test]
fn a_blank_document_somebody_typed_in_stays_open() {
    let mut st = state();
    st.open.push(DocumentModel::from_doc(
        "typed".to_string(),
        azul::widgets::RichTextDoc::create_from_markdown("typed words\n"),
        String::new(),
    ));
    assert!(show_document(&mut st, saved("a", "# Alpha\n")));
    assert_eq!(st.open.len(), 2);
}

#[test]
fn a_new_document_opens_right_after_the_shown_one_and_takes_the_screen() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(saved("b", "# Beta\n"));
    st.active = 0;
    assert!(show_document(&mut st, saved("c", "# Gamma\n")));
    let ids: Vec<&str> = st.open.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "c", "b"]);
    assert_eq!(st.active, 1);
    assert_eq!(st.screen, Screen::Editor);
    assert!(st.sheet_doc.is_none());
}

#[test]
fn a_document_that_is_open_already_is_shown_again_not_opened_twice() {
    let mut st = state();
    st.open.push(saved("a", "# Alpha\n"));
    st.open.push(saved("b", "# Beta\n"));
    st.active = 0;
    assert!(show_document(&mut st, saved("b", "# Beta, read again\n")));
    assert_eq!(st.open.len(), 2);
    assert_eq!(st.active, 1);
    assert_eq!(st.open[1].title(), "Beta, read again");
}

#[test]
fn one_document_more_than_the_most_is_refused_with_a_notice_and_nothing_opens() {
    crate::l10n::in_english();
    let mut st = state();
    for i in 0..docs::MAX_OPEN_DOCS {
        assert!(show_document(&mut st, saved(&format!("d{i}"), &format!("# Document {i}\n"))));
    }
    assert_eq!(st.open.len(), docs::MAX_OPEN_DOCS);
    let shown = st.active;
    assert!(!show_document(&mut st, saved("extra", "# One too many\n")));
    assert_eq!(st.open.len(), docs::MAX_OPEN_DOCS);
    assert_eq!(st.active, shown, "the shown document stays shown");
    assert!(st.notice.contains(&docs::MAX_OPEN_DOCS.to_string()), "{}", st.notice);
}

#[test]
fn the_word_count_sentence_names_words_characters_and_pages() {
    crate::l10n::in_english();
    let doc = saved("a", "# Hi\n\nthere you\n");
    let said = word_count_notice(&doc);
    assert!(said.contains("3 words"), "{said}");
    assert!(said.contains("11 characters"), "{said}");
    assert!(said.contains("1 page"), "{said}");
}

#[test]
fn the_views_are_the_flow_and_the_page_layout() {
    // `Command::View` carries the layout; the flow is the default
    assert_eq!(View::default(), View::Flow);
    assert_ne!(Command::View(View::Flow), Command::View(View::Pages));
}
