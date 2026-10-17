//! An editing host INSIDE a paragraph paints its caret.
//!
//! A caret is painted only while its editing session lives in the focused
//! subtree (`LayoutWindow::caret_editable_is_focused`, so a blurred field
//! drops its caret). Sessions are keyed on the caret's TEXT BLOCK - the
//! paragraph whose inline layout the caret indexes - and the check asked
//! whether that block lies inside the focused node. For
//! `<p>Name: <span contenteditable>Ada</span></p>` the block is the `<p>`
//! and the focus is the `<span>` inside it, so the answer was "not focused"
//! and the field showed no caret at all.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > p(1) > ["Name: "(2), span[contenteditable](3) > "Ada"(4)]`
const SPAN: usize = 3;

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

#[test]
fn a_focused_inline_editing_host_paints_its_caret() {
    let mut span = Dom::create_span()
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Ada"));
    span.set_contenteditable(true);
    let mut dom = Dom::create_body().with_child(
        Dom::create_p()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "Name: ",
            ))
            .with_child(span),
    );
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 14px; width: 400px; }",
    );
    let styled = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();

    // Focus the span the way a Tab does: the focus path, then the post-layout
    // finalize that seeds the caret at the end of its text.
    let _ = lw.handle_focus_change_for_cursor_blink(Some(dnid(SPAN)), &ws);
    lw.focus_manager.set_focused_node(Some(dnid(SPAN)));
    assert!(
        lw.finalize_pending_focus_changes(),
        "premise: focusing the span opened an editing session"
    );
    assert!(
        lw.text_edit_manager.get_primary_cursor().is_some(),
        "premise: the session has a caret"
    );

    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    let caret_alpha = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .rev()
        .find_map(|item| match item {
            DisplayListItem::CursorRect { color, .. } => Some(color.a),
            _ => None,
        });
    assert!(
        caret_alpha.is_some_and(|a| a > 0),
        "the focused span's caret is painted, got {caret_alpha:?}"
    );
}
