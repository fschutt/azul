//! Backspace and Delete merge blocks only at a real block boundary.
//!
//! The keyboard asks `build_editing_query_state` whether the caret sits at
//! the very start or the very end of its block: at the start Backspace
//! records `MergeWithPrevious`, at the end Delete records `MergeWithNext`,
//! anywhere else both edit characters. That answer read the caret's raw
//! cluster id against the FOCUSED HOST's flattened text:
//!
//! - "at the start" was `run == 0 && byte == 0`, which is also the caret the layout mints on the
//!   right half of the first glyph (`Trailing` on cluster 0) - AFTER that glyph;
//! - "at the end" compared the cluster's START byte with the host's last run, so the layout's own
//!   end-of-text caret (`Trailing` on the last cluster) never matched, and in a multi-paragraph
//!   host no paragraph but the last could ever be at its end;
//! - a selection was judged by its focus end alone, so Backspace over a selection that reached a
//!   paragraph's start merged instead of deleting it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    events::DefaultAction,
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{KeyboardState, VirtualKeyCode},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > div.host[contenteditable](1) > [p(2) > "first"(3), p(4) > "second"(5)]`
const HOST: usize = 1;
const P1: usize = 2;
const P2: usize = 4;

fn two_paragraphs() -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .host { display: block; }
        .p { display: block; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let para = |text: &str| {
        Dom::create_div()
            .with_ids_and_classes(class("p"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                text,
            ))
    };
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(para("first"))
            .with_child(para("second")),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn at(byte: u32, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity,
    }
}

/// The editing session a click leaves: on the paragraph, at `cursor`.
fn caret_in(lw: &mut LayoutWindow, paragraph: usize, cursor: TextCursor) {
    lw.start_editing_at(cursor, DomId::ROOT_ID, NodeId::new(paragraph), 0);
}

/// What the shell decides for `key` with the focus on the host.
fn decided(lw: &LayoutWindow, key: VirtualKeyCode) -> DefaultAction {
    let focused = Some(dnid(HOST));
    let editing = lw
        .build_editing_query_state(focused)
        .expect("the focus is in a contenteditable host");
    let keys = KeyboardState {
        current_virtual_keycode: Some(key).into(),
        pressed_virtual_keycodes: vec![key].into(),
        ..Default::default()
    };
    azul_layout::default_actions::determine_keyboard_default_action_with_editing(
        &keys,
        focused,
        &lw.layout_results,
        false,
        Some(&editing),
    )
    .action
}

/// A click on the right half of "second"'s 's' puts the caret AFTER it:
/// `Trailing` on cluster 0. Backspace deletes the 's'.
#[test]
fn backspace_after_a_paragraphs_first_character_deletes_it() {
    let mut lw = two_paragraphs();
    caret_in(&mut lw, P2, at(0, CursorAffinity::Trailing));

    let action = decided(&lw, VirtualKeyCode::Back);
    assert!(
        !matches!(action, DefaultAction::MergeWithPrevious { .. }),
        "the caret is after the 's', not at the paragraph's start: {action:?}"
    );
}

/// The layout's own end-of-text caret - `Trailing` on the last cluster, what
/// End and a click past the text produce - is at the paragraph's end: Delete
/// joins the next paragraph onto it.
#[test]
fn delete_at_a_paragraphs_end_merges_the_next_one() {
    let mut lw = two_paragraphs();
    let end = lw
        .materialized_inline_layout_for_node(DomId::ROOT_ID, NodeId::new(P1))
        .and_then(|layout| layout.get_last_cluster_cursor())
        .expect("premise: the first paragraph has a last cluster");
    caret_in(&mut lw, P1, end);

    let action = decided(&lw, VirtualKeyCode::Delete);
    assert!(
        matches!(action, DefaultAction::MergeWithNext { .. }),
        "Delete at the end of 'first' merges 'second' into it: {action:?}"
    );
}

/// "sec|ond" selected backward to the paragraph's start: the caret end is
/// at the start, but Backspace deletes the SELECTION.
#[test]
fn backspace_over_a_selection_reaching_a_paragraphs_start_deletes_it() {
    let mut lw = two_paragraphs();
    caret_in(&mut lw, P2, at(3, CursorAffinity::Leading));
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("the session")
        .set_single_range(SelectionRange {
            start: at(3, CursorAffinity::Leading),
            end: at(0, CursorAffinity::Leading),
        });

    let action = decided(&lw, VirtualKeyCode::Back);
    assert!(
        !matches!(action, DefaultAction::MergeWithPrevious { .. }),
        "a selection is deleted, not merged: {action:?}"
    );
}

/// The boundaries that ARE boundaries still merge (passes before the fix
/// too; pins that the fix does not overcorrect).
#[test]
fn backspace_before_a_paragraphs_first_character_still_merges() {
    let mut lw = two_paragraphs();
    caret_in(&mut lw, P2, at(0, CursorAffinity::Leading));
    assert!(matches!(
        decided(&lw, VirtualKeyCode::Back),
        DefaultAction::MergeWithPrevious { .. }
    ));
}
