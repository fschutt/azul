//! The open-documents vocabulary: the sync state a pie shows, the names of the rows, the tags
//! of the sidebar's report and the rules of the list (what is shown after a close, where a new
//! document opens).

use super::*;

#[test]
fn a_write_on_its_way_wins_then_a_failed_write_then_unwritten_changes() {
    assert_eq!(DocSync::of(false, false, false), DocSync::Saved);
    assert_eq!(DocSync::of(true, false, false), DocSync::Unsaved);
    assert_eq!(DocSync::of(true, false, true), DocSync::Failed, "a failed write is louder than the changes");
    assert_eq!(DocSync::of(true, true, true), DocSync::Saving, "a write on its way wins: it may well succeed");
    assert_eq!(DocSync::of(false, true, false), DocSync::Saving);
}

#[test]
fn the_pie_is_full_when_settled_and_a_wedge_while_something_is_still_to_be_written() {
    assert_eq!(DocSync::Saved.fraction(), 1.0);
    assert_eq!(DocSync::Failed.fraction(), 1.0);
    assert!(DocSync::Saving.fraction() > DocSync::Unsaved.fraction());
    for sync in [DocSync::Saved, DocSync::Saving, DocSync::Unsaved, DocSync::Failed] {
        let f = sync.fraction();
        assert!((0.0..=1.0).contains(&f), "{sync:?}: {f}");
    }
    assert_eq!(DocSync::Saved.tone(), Tone::Good);
    assert_eq!(DocSync::Saving.tone(), Tone::Busy);
    assert_eq!(DocSync::Unsaved.tone(), Tone::Warning);
    assert_eq!(DocSync::Failed.tone(), Tone::Danger);
}

#[test]
fn every_state_names_its_word_and_its_sentence_with_the_kits_prefix() {
    let mut seen = std::collections::BTreeSet::new();
    for sync in [DocSync::Saved, DocSync::Saving, DocSync::Unsaved, DocSync::Failed] {
        for key in [sync.key(), sync.detail_key()] {
            assert!(key.starts_with("kit-doc-"), "{key}");
            assert!(seen.insert(key), "{key} is named twice");
        }
    }
    assert!(COLLAB_NOTE_KEY.starts_with("kit-doc-"));
}

#[test]
fn a_row_shows_the_first_line_of_a_name_cut_with_an_ellipsis() {
    assert_eq!(shown_name("report.docx", "Untitled"), "report.docx");
    assert_eq!(shown_name("  report.docx \n second line", "Untitled"), "report.docx");
    assert_eq!(shown_name("", "Untitled"), "Untitled");
    assert_eq!(shown_name("   \n  ", "Untitled"), "Untitled");
    let long = "x".repeat(NAME_CHARS + 20);
    let cut = shown_name(&long, "Untitled");
    assert_eq!(cut.chars().count(), NAME_CHARS);
    assert!(cut.ends_with('\u{2026}'));
    let exact = "y".repeat(NAME_CHARS);
    assert_eq!(shown_name(&exact, "Untitled"), exact, "a name of exactly the limit is kept whole");
    // characters, not bytes: a name of two-byte letters is cut at a character
    let umlauts = "\u{e4}".repeat(NAME_CHARS + 5);
    assert_eq!(shown_name(&umlauts, "Untitled").chars().count(), NAME_CHARS);
}

#[test]
fn a_row_is_saved_unselected_and_without_collaboration_until_the_app_says_otherwise() {
    let row = DocRow::new("book1.xlsx");
    assert_eq!(row.sync, DocSync::Saved);
    assert!(!row.collab && !row.selected);
    assert_eq!(row.shown_name("x"), "book1.xlsx");
    assert_eq!(DocRow::new("").shown_name("Untitled"), "Untitled");
}

#[test]
fn a_tag_names_an_open_document_only_inside_its_range() {
    assert_eq!(doc_tag(0), DOC_TAG_BASE);
    for index in [0, 1, MAX_OPEN_DOCS - 1] {
        assert_eq!(doc_of_tag(doc_tag(index)), Some(index));
    }
    assert_eq!(doc_of_tag(DOC_TAG_BASE + MAX_OPEN_DOCS), None, "past the last document");
    assert_eq!(doc_of_tag(DOC_TAG_BASE - 1), None);
    assert_eq!(doc_of_tag(NEW_TAG), None, "the New row is no document");
    assert_eq!(doc_of_tag(APP_TAG_BASE), None, "an app's own rows are no document");
    assert_eq!(doc_of_tag(0), None);
    assert_eq!(doc_of_tag(usize::MAX), None);
    // the ranges do not meet
    assert!(NEW_TAG < DOC_TAG_BASE);
    assert!(DOC_TAG_BASE + MAX_OPEN_DOCS <= APP_TAG_BASE);
}

#[test]
fn closing_the_shown_document_shows_the_one_that_slid_into_its_place_or_the_last() {
    // [a, b, c] with b shown, b closed: c slides into index 1
    assert_eq!(active_after_close(3, 1, 1), Some(1));
    // the last one shown and closed: its predecessor
    assert_eq!(active_after_close(3, 2, 2), Some(1));
    // the first shown and closed: the next one, now at index 0
    assert_eq!(active_after_close(3, 0, 0), Some(0));
    // the only document: nothing is open any more
    assert_eq!(active_after_close(1, 0, 0), None);
    assert_eq!(active_after_close(0, 0, 0), None);
}

#[test]
fn closing_another_document_keeps_the_shown_one_shown() {
    // a closed one before the shown one moves its index down
    assert_eq!(active_after_close(4, 2, 0), Some(1));
    assert_eq!(active_after_close(4, 3, 2), Some(2));
    // a closed one after it leaves its index
    assert_eq!(active_after_close(4, 1, 3), Some(1));
    assert_eq!(active_after_close(4, 0, 1), Some(0));
}

#[test]
fn an_index_past_the_list_closes_nothing_and_never_leaves_the_list() {
    assert_eq!(active_after_close(3, 1, 7), Some(1));
    assert_eq!(active_after_close(3, 9, 7), Some(2), "a stale active index is held inside the list");
    assert_eq!(active_after_close(2, 5, 0), Some(0));
}

#[test]
fn a_new_document_opens_right_after_the_shown_one() {
    assert_eq!(insert_at(0, 0), 0);
    assert_eq!(insert_at(1, 0), 1);
    assert_eq!(insert_at(3, 0), 1);
    assert_eq!(insert_at(3, 1), 2);
    assert_eq!(insert_at(3, 2), 3, "after the last: at the end");
    assert_eq!(insert_at(3, 9), 3, "a stale index opens at the end, never past it");
}
