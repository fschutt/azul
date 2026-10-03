//! Enter at the end of a list item: the app applies the split (a new, empty
//! item after it) and acknowledges it; the caret resumes in the NEW item at
//! the start of its text - past the item's `::marker`, which its layout
//! numbers first (the marker is run 0, the text run 1).
//!
//! The resume point names the new item with no text child to stand in, so
//! `restore_caret_from_resume_point` falls back to "run 0, byte 0" of the
//! block - the marker. Every other way a caret lands in a list item (a
//! click, an arrow, a reset editor) is moved past the marker
//! (`caret_past_markers`); this one was not, so the caret painted on the
//! bullet and Backspace / arrows started from the marker. The editing spec's
//! `insertParagraph` puts the caret at the start of the new item's contents
//! (WPT `editing/data/insertparagraph.js`, `<ul><li>foo[]</li></ul>` ->
//! `<ul><li>foo</li><li>{}<br></li></ul>`).

use azul_core::{dom::NodeId, events::DefaultAction};

use crate::editing_harness::{dnid, host_dom, lay_out_new_generation, Editor, HOST};

/// `body(0) > host(1) > ul(2) > [ li(3) > "alpha"(4), li(5) ]` after the
/// app applied the split.
const NEW_ITEM: usize = 5;

#[test]
fn an_acked_split_of_a_list_item_resumes_past_the_new_items_marker() {
    let mut editor = Editor::new("<ul><li>alpha[]</li></ul>");
    assert!(
        editor
            .lw
            .record_structural_default_action(&DefaultAction::SplitBlockAtCursor {
                target: dnid(HOST)
            })
            .is_some(),
        "premise: Enter at the item's end records a split"
    );
    let id = editor
        .lw
        .get_pending_document_edit()
        .expect("premise: the split waits for the app")
        .id;

    // The app applies it: a new, empty item after the first.
    assert!(editor.lw.mark_document_edit_applied(id), "the app acks the split");
    let (dom, ..) = host_dom("<ul><li>alpha</li><li></li></ul>");
    lay_out_new_generation(&mut editor.lw, dom);

    assert_eq!(
        editor.lw.text_edit_manager.get_editing_node_id(),
        Some(NodeId::new(NEW_ITEM)),
        "the caret resumes in the new item"
    );
    let caret = editor
        .lw
        .text_edit_manager
        .get_primary_cursor()
        .expect("the caret resumed");
    assert_eq!(
        (caret.cluster_id.source_run, caret.cluster_id.start_byte_in_run),
        (1, 0),
        "at the start of the item's text (run 1), not on its marker (run 0)"
    );
}
