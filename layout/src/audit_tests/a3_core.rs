//! A3-core, quadratics in core, css and the display list builder.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    hit_test::TAG_TYPE_DOM_NODE,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    solver3::{
        display_list::{DisplayListItem, INLINE_LAYOUT_COPIES, TAG_LIST_READS},
        layout_tree::LayoutNodeId,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};

/// A label layer as a map view builds it: `k` absolutely positioned spans with a text, each a
/// hit-test target (a span with a text child carries a tag), in groups of `group` plain divs
/// (`0`: all of them children of the one layer).
fn label_layer(k: usize, group: usize) -> Dom {
    let label = |i: usize| {
        Dom::create_span_with_text(format!("label {i}")).with_css(&format!(
            "position: absolute; left: {}px; top: {}px;",
            (i % 40) * 14,
            (i / 40) * 12
        ))
    };
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    if group == 0 {
        for i in 0..k {
            layer = layer.with_child(label(i));
        }
    } else {
        let mut i = 0;
        while i < k {
            let mut part = Dom::create_div();
            for j in i..(i + group).min(k) {
                part = part.with_child(label(j));
            }
            layer = layer.with_child(part);
            i += group;
        }
    }
    Dom::create_body().with_css("margin: 0px;").with_child(layer)
}

/// `dom` laid out in a window, its display list built.
fn laid_out(dom: Dom) -> LayoutWindow {
    laid_out_with(dom, FcFontCache::default())
}

/// [`laid_out`] with the fonts of `fonts` (the system's, for a test that needs its text set).
fn laid_out_with(dom: Dom, fonts: FcFontCache) -> LayoutWindow {
    let mut lw = LayoutWindow::new(fonts).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// How many entries of the DOM's tag list were read while the display list of `k` labels was
/// built.
fn tag_list_reads_for(k: usize, group: usize) -> usize {
    let dom = label_layer(k, group);
    TAG_LIST_READS.with(|n| n.set(0));
    drop(laid_out(dom));
    TAG_LIST_READS.with(std::cell::Cell::get)
}

/// A3-core-01: every box asked for its hit-test tag by scanning the DOM's whole tag list (a box
/// without a tag to the end): boxes x tags entries per display list, 54 million at the 12,470
/// nodes of the Berlin view. The list is read once per display list now.
#[test]
fn a_display_list_reads_the_tag_list_once_not_once_per_box() {
    for group in [0, 64] {
        let (small, large) = (tag_list_reads_for(250, group), tag_list_reads_for(1_000, group));
        assert!(small >= 250, "harness: the tag list was read ({small}, groups of {group})");
        assert!(
            large <= small * 5,
            "four times the labels read {large} tag entries for {small} (groups of {group}): \
             more than linear"
        );
    }
}

/// A3-core-01: the tag of every hit-test area is the one a scan of the tag list finds for the
/// node the area belongs to, and every label has its area.
#[test]
fn every_hit_test_area_carries_the_tag_of_its_node() {
    let k = 300;
    let lw = laid_out(label_layer(k, 64));
    let result = lw.layout_results.get(&DomId::ROOT_ID).expect("the root's layout");
    let tags = result.styled_dom.tag_ids_to_node_ids.as_ref();
    let list = &result.display_list;
    let mut areas = 0usize;
    for (i, item) in list.items.iter().enumerate() {
        let DisplayListItem::HitTestArea { tag, .. } = item else {
            continue;
        };
        if tag.1 != TAG_TYPE_DOM_NODE {
            continue;
        }
        let node = list.node_mapping[i].expect("a hit-test area belongs to a node");
        let scanned = tags
            .iter()
            .find(|m| m.node_id.into_crate_internal() == Some(node))
            .map(|m| m.tag_id.inner);
        assert_eq!(Some(tag.0), scanned, "item {i}: the tag of node {node:?}");
        areas += 1;
    }
    assert!(areas >= k, "{areas} hit-test areas for {k} labels");
}

/// A3-core-03: the selection and caret painters asked for a copy of every text block's lines
/// (made up from the dense text: one allocation and a walk of the block's clusters) before
/// looking whether a selection or a caret is in the block. A page without either copies none.
#[test]
fn a_block_without_a_selection_or_a_caret_copies_no_lines() {
    let k = 200;
    let dom = label_layer(k, 64);
    INLINE_LAYOUT_COPIES.with(|n| n.set(0));
    let lw = laid_out_with(dom, FcFontCache::build());
    let copies = INLINE_LAYOUT_COPIES.with(std::cell::Cell::get);
    // The labels are text blocks: each has lines the painters could have asked for.
    let result = lw.layout_results.get(&DomId::ROOT_ID).expect("the root's layout");
    let tree = &result.layout_tree;
    let blocks = (0..tree.nodes.len())
        .filter(|i| {
            tree.warm(LayoutNodeId::new(*i))
                .is_some_and(|w| w.inline_layout_result.is_some())
        })
        .count();
    assert!(blocks >= k, "harness: {blocks} text blocks for {k} labels");
    assert_eq!(copies, 0, "{copies} copies of a text block's lines for no selection and no caret");
}
