//! An open document: reading and writing its Markdown, when it is dirty, what a change does to its
//! pages, and what the sidebar's row says of it.

use super::*;

const SAMPLE: &str = "# Quarterly report\n\nSales grew **12%** in the *third* quarter.\n\n- North\n- South\n\n| Region | Sales |\n| --- | --- |\n| North | 120 |\n\n---\n\nThe end.\n";

#[test]
fn a_document_reads_its_markdown_and_writes_the_same_back() {
    let doc = DocumentModel::from_markdown("id".to_string(), SAMPLE);
    assert_eq!(doc.title(), "Quarterly report");
    let again = DocumentModel::from_markdown("id".to_string(), &doc.markdown());
    assert_eq!(again.doc(), doc.doc(), "Markdown -> document -> Markdown -> the same document");
    assert!(doc.word_count() >= 12);
}

#[test]
fn a_document_is_dirty_when_it_differs_from_its_file() {
    let fresh = DocumentModel::untitled("a".to_string());
    assert!(!fresh.is_dirty(), "an empty new document has nothing to save");
    assert_eq!(fresh.title(), UNTITLED);
    let mut doc = DocumentModel::from_markdown("b".to_string(), SAMPLE);
    let saved = doc.markdown();
    doc.saved = saved;
    assert!(!doc.is_dirty());
    let mut edited = doc.editor.clone();
    edited.doc = RichTextDoc::create_from_markdown("# Other\n");
    let before = doc.generation;
    doc.adopt(edited);
    assert!(doc.is_dirty());
    assert_ne!(doc.generation, before, "a change re-paginates");
    assert_eq!(doc.title(), "Other");
}

#[test]
fn a_state_that_changed_nothing_leaves_the_generation_alone() {
    let mut doc = DocumentModel::from_markdown("b".to_string(), SAMPLE);
    let before = doc.generation;
    let same = doc.editor.clone();
    doc.adopt(same);
    assert_eq!(doc.generation, before, "the caret moved, the text did not: no new pagination");
}

#[test]
fn every_document_counts_its_own_changes_from_one() {
    let mut a = DocumentModel::untitled("a".to_string());
    let b = DocumentModel::untitled("b".to_string());
    assert_eq!(a.generation, 1);
    assert_eq!(b.generation, 1, "no counter is shared between documents");
    a.touch();
    a.touch();
    assert_eq!(a.generation, 3);
    assert_eq!(b.generation, 1);
    assert_eq!(a.pages.generation, 0, "no pagination has run for it yet");
}

#[test]
fn a_new_document_is_not_collaborating_and_its_last_save_did_not_fail() {
    let doc = DocumentModel::untitled("c".to_string());
    assert!(!doc.collab);
    assert!(!doc.save_failed);
    assert!(doc.paginating.is_none());
}

#[test]
fn the_editor_of_a_document_is_named_with_the_apps_prefix() {
    let doc = DocumentModel::untitled("c".to_string());
    assert!(doc.editor.host_id.as_str().starts_with("__azwriter_"));
}

#[test]
fn a_new_document_id_is_a_uuid() {
    let id = new_document_id();
    assert_eq!(id.len(), 36, "{id}");
    assert_eq!(id.matches('-').count(), 4);
}

#[test]
fn a_title_is_cut_at_sixty_characters() {
    let long = format!("# {}\n", "w".repeat(80));
    let doc = DocumentModel::from_markdown("d".to_string(), &long);
    assert_eq!(doc.title().chars().count(), 60);
    assert!(doc.title().ends_with('\u{2026}'));
}
