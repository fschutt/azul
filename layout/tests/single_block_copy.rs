//! Copying a selection inside ONE block copies the characters it covers.
//!
//! A `TextCursor` names a run of the inline content `solver3::fc` built for
//! the block and a byte in that run's SHAPED text. That content puts a list
//! item's `::marker` in front of its text (so the text is run 1), and it
//! collapses white space in `white-space: normal` text before shaping. The
//! single-block copy indexed a different vector - the DOM-child walk of
//! `get_text_before_textinput`, which has no marker and keeps the raw white
//! space - with those cursors. e1b0099e3 moved the MULTI-block copy onto the
//! layout's own runs (`selection_runs_for_node`); the single-block copy is the
//! same question.

use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{
        CursorAffinity, GraphemeClusterId, MultiCursorState, SelectionRange, TextCursor,
    },
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, text3::cache::ShapedItem, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > div.p(1) > text(2)`, the block styled by `block_css`.
const BLOCK: usize = 1;

fn one_block(block_css: &str, text: &str) -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .p { display: block; }
    "#;
    let mut block =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("p".into())].into());
    if !block_css.is_empty() {
        block = block.with_css(block_css);
    }
    let mut dom = Dom::create_body().with_child(
        block.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            text,
        )),
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
    lw
}

/// The block's TEXT clusters, as the layout shaped them (generated content -
/// a list marker - carries no source node).
fn text_clusters(lw: &LayoutWindow) -> Vec<GraphemeClusterId> {
    let layout = lw
        .materialized_inline_layout_for_node(DomId::ROOT_ID, NodeId::new(BLOCK))
        .expect("the block is laid out");
    layout
        .items
        .iter()
        .filter_map(|item| match &item.item {
            ShapedItem::Cluster(c) if c.source_node_id.is_some() => Some(c.source_cluster_id),
            _ => None,
        })
        .collect()
}

fn at(cluster: GraphemeClusterId, affinity: CursorAffinity) -> TextCursor {
    TextCursor {
        cluster_id: cluster,
        affinity,
    }
}

/// An editing session on the block holding `range`, as a drag inside it
/// leaves one.
fn select(lw: &mut LayoutWindow, range: SelectionRange) {
    let block = lw
        .text_block_of(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(BLOCK))),
        })
        .expect("the block is a text block");
    let mut mc = MultiCursorState::new_with_cursor(range.start, block, 0);
    mc.set_single_range(range);
    lw.text_edit_manager.multi_cursor = Some(mc);
}

fn copied(lw: &LayoutWindow) -> Option<String> {
    lw.get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string())
}

/// "a|lph|a" in a list item: the marker is run 0, so the text is run 1 -
/// a run the DOM walk does not have.
#[test]
fn a_selection_in_a_list_item_copies_its_text() {
    let mut lw = one_block("display: list-item;", "alpha");
    let clusters = text_clusters(&lw);
    let l = *clusters
        .iter()
        .find(|c| c.start_byte_in_run == 1)
        .expect("a cluster at byte 1");
    let last_a = *clusters
        .iter()
        .find(|c| c.start_byte_in_run == 4)
        .expect("a cluster at byte 4");
    assert_eq!(
        l.source_run, 1,
        "premise: the marker is run 0 of the layout's content, so 'alpha' is run 1"
    );

    select(
        &mut lw,
        SelectionRange {
            start: at(l, CursorAffinity::Leading),
            end: at(last_a, CursorAffinity::Leading),
        },
    );
    assert_eq!(copied(&lw).as_deref(), Some("lph"));
}

/// "a   b c" in `white-space: normal` is SHAPED as "a b c": the 'c' starts
/// at byte 4 of the shaped run - where the raw DOM text has the 'b'.
#[test]
fn a_selection_after_collapsed_white_space_copies_what_it_covers() {
    let mut lw = one_block("", "a   b c");
    let last = *text_clusters(&lw).last().expect("the text has clusters");
    assert_eq!(
        last.start_byte_in_run, 4,
        "premise: the layout collapsed the spaces, so 'c' starts at byte 4 of \"a b c\""
    );

    select(
        &mut lw,
        SelectionRange {
            start: at(last, CursorAffinity::Leading),
            end: at(last, CursorAffinity::Trailing),
        },
    );
    assert_eq!(copied(&lw).as_deref(), Some("c"));
}

/// A range dragged BACKWARD (anchor after focus) copies the same text.
#[test]
fn a_backward_selection_copies_the_same_text() {
    let mut lw = one_block("display: list-item;", "alpha");
    let clusters = text_clusters(&lw);
    let l = *clusters
        .iter()
        .find(|c| c.start_byte_in_run == 1)
        .expect("a cluster at byte 1");
    let last_a = *clusters
        .iter()
        .find(|c| c.start_byte_in_run == 4)
        .expect("a cluster at byte 4");

    select(
        &mut lw,
        SelectionRange {
            start: at(last_a, CursorAffinity::Leading),
            end: at(l, CursorAffinity::Leading),
        },
    );
    assert_eq!(copied(&lw).as_deref(), Some("lph"));
}
