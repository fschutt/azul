//! Enter, Backspace and Delete act on the caret's INNERMOST block: in
//! `host > blockquote > blockquote > p`, Enter splits the `<p>`, Backspace
//! at the start of a quoted paragraph merges it into the paragraph before
//! it inside the quote, and an `<li>` splits into two `<li>`.
//!
//! What the execCommand spec's `insertParagraph` does (it splits the caret's
//! "editable block" - the nearest block container - and `delete` at the
//! start of a block merges it with the block before it, both inside any
//! number of containers): WPT `editing/data/insertparagraph.js`
//! (`<blockquote><p>foo[]bar</p></blockquote>` ->
//! `<blockquote><p>foo</p><p>{}bar</p></blockquote>`), `delete.js`. A
//! quote's FIRST paragraph is the app's rule (a mail editor lowers the quote
//! level; the spec outdents): the engine records no merge there.
//!
//! Before: the structural edit node was the host's DIRECT child on the
//! caret's path (`LayoutWindow::structural_edit_node`) - the outer quote.
//! Enter cloned the whole quote and moved the rest of it into the clone,
//! and Backspace merged the whole quote into the block before it.

use azul_core::{dom::DomNodeId, events::DefaultAction};
use azul_layout::{
    document_edit::apply_document_operation,
    managers::changeset::{DocumentChangeset, DocumentOperation, NodePosition},
};

use crate::editing_harness::{dnid, host_dom, markup_of_dom, Editor, HOST};

fn record(editor: &mut Editor, action: DefaultAction) -> DocumentChangeset {
    assert!(
        editor
            .lw
            .record_structural_default_action(&action)
            .is_some(),
        "premise: the edit is recorded"
    );
    editor
        .lw
        .get_pending_document_edit()
        .expect("the edit is pending for the app")
        .clone()
}

fn position(at: &NodePosition) -> (u32, Option<u32>) {
    (at.child_index, at.text_byte.into_option())
}

/// Rows: the markup with its caret, the key, the node the recorded edit
/// acts on (its element index; the merge partner for Backspace / Delete),
/// the position, and the resume path from the host.
#[derive(Debug, Clone, Copy)]
enum Key {
    Enter,
    Backspace,
    Delete,
}

fn action_for(key: Key) -> DefaultAction {
    let target: DomNodeId = dnid(HOST);
    match key {
        Key::Enter => DefaultAction::SplitBlockAtCursor { target },
        Key::Backspace => DefaultAction::MergeWithPrevious { target },
        Key::Delete => DefaultAction::MergeWithNext { target },
    }
}

/// `body(0) > host(1) > blockquote(2) > blockquote(3) > p(4) > "abcdef"(5)`
#[test]
fn enter_in_a_nested_quote_splits_the_paragraph() {
    let mut editor =
        Editor::new("<blockquote><blockquote><p>abc[]def</p></blockquote></blockquote>");

    let edit = record(&mut editor, action_for(Key::Enter));

    let DocumentOperation::SplitNode(split) = &edit.operation else {
        panic!("Enter records a split, not {:?}", edit.operation);
    };
    assert_eq!(split.node, dnid(4), "the paragraph, not the outer quote");
    assert_eq!(
        position(&split.at),
        (0, Some(3)),
        "cut inside its text at \"abc|def\""
    );
    assert_eq!(
        edit.resume.node_path.as_ref(),
        &[0, 0, 1],
        "the caret resumes in the NEW paragraph: quote 0 > quote 0 > paragraph 1"
    );

    // The app applies it with the shipped helper, at the paragraph's parent.
    let (mut root, ..) =
        host_dom("<blockquote><blockquote><p>abcdef</p></blockquote></blockquote>");
    apply_document_operation(&mut root, &[0, 0, 0], &edit).expect("the split applies");
    assert_eq!(
        markup_of_dom(&root),
        "<body><div><blockquote><blockquote><p>abc</p><p>def</p></blockquote></blockquote></div></body>"
    );
}

/// `body(0) > host(1) > blockquote(2) > [p(3) > "a"(4), p(5) > "b"(6)]`
#[test]
fn backspace_at_the_start_of_a_quoted_paragraph_merges_it_into_the_one_before() {
    let mut editor = Editor::new("<blockquote><p>a</p><p>[]b</p></blockquote>");

    let edit = record(&mut editor, action_for(Key::Backspace));

    let DocumentOperation::MergeNodes(merge) = &edit.operation else {
        panic!("Backspace records a merge, not {:?}", edit.operation);
    };
    assert_eq!((merge.first, merge.second), (dnid(3), dnid(5)));
    assert_eq!(position(&merge.join), (0, Some(1)), "the seam after \"a\"");
    assert_eq!(
        edit.resume.node_path.as_ref(),
        &[0, 0],
        "quote 0 > paragraph 0"
    );
}

#[test]
fn delete_at_the_end_of_a_quoted_paragraph_merges_the_next_one_into_it() {
    let mut editor = Editor::new("<blockquote><p>a[]</p><p>b</p></blockquote>");

    let edit = record(&mut editor, action_for(Key::Delete));

    let DocumentOperation::MergeNodes(merge) = &edit.operation else {
        panic!("Delete records a merge, not {:?}", edit.operation);
    };
    assert_eq!((merge.first, merge.second), (dnid(3), dnid(5)));
}

/// `body(0) > host(1) > ul(2) > li(3) > "abcd"(4)`
#[test]
fn enter_in_a_list_item_splits_the_item() {
    let mut editor = Editor::new("<ul><li>ab[]cd</li></ul>");

    let edit = record(&mut editor, action_for(Key::Enter));

    let DocumentOperation::SplitNode(split) = &edit.operation else {
        panic!("Enter records a split, not {:?}", edit.operation);
    };
    assert_eq!(split.node, dnid(3), "the list item, not the list");
    assert_eq!(position(&split.at), (0, Some(2)));
    assert_eq!(edit.resume.node_path.as_ref(), &[0, 1], "list 0 > item 1");
}

/// The first paragraph of a quote has no block before it INSIDE the quote:
/// the engine records nothing, and the app decides (a mail editor lowers
/// the quote level).
#[test]
fn backspace_at_the_start_of_a_quotes_first_paragraph_is_the_apps_rule() {
    let mut editor = Editor::new("<p>x</p><blockquote><p>[]a</p></blockquote>");

    assert!(
        editor
            .lw
            .record_structural_default_action(&action_for(Key::Backspace))
            .is_none(),
        "no merge across the quote's edge"
    );
    assert_eq!(editor.markup_of(2), "x", "and nothing else happened");
}

/// A direct child of the host keeps splitting as before (guard).
#[test]
fn enter_in_a_top_level_paragraph_still_splits_it() {
    let mut editor = Editor::new("<p>ab[]cd</p>");

    let edit = record(&mut editor, action_for(Key::Enter));

    let DocumentOperation::SplitNode(split) = &edit.operation else {
        panic!("Enter records a split, not {:?}", edit.operation);
    };
    assert_eq!(split.node, dnid(2));
    assert_eq!(edit.resume.node_path.as_ref(), &[1]);
}
