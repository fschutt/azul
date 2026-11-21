//! A3-core, quadratics in core, css and the display list builder.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalRect, LogicalSize},
    hit_test::TAG_TYPE_DOM_NODE,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    solver3::{
        display_list::{
            DisplayList, DisplayListItem, INLINE_LAYOUT_COPIES, SCROLL_CHAIN_WALKS, TAG_LIST_READS,
        },
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

/// A3-core-05: every positioned box had its scroll chain walked up the tree
/// (`ScrollChain::of`: a handful of cascade lookups per level) although the chain depends only
/// on the box's parent and on how it is positioned. The labels of one parent share one walk.
#[test]
fn the_positioned_boxes_of_one_parent_share_one_scroll_chain_walk() {
    for (group, budget) in [(0, 8), (64, 16 + 8)] {
        let dom = label_layer(1_000, group);
        SCROLL_CHAIN_WALKS.with(|n| n.set(0));
        drop(laid_out(dom));
        let walks = SCROLL_CHAIN_WALKS.with(std::cell::Cell::get);
        assert!(walks >= 1, "harness: a chain was walked (groups of {group})");
        assert!(
            walks <= budget,
            "{walks} scroll chain walks for 1,000 labels in groups of {group} (budget {budget})"
        );
    }
}

/// The clips open around the hit-test area of DOM node `node` in `list`, outermost first.
fn clips_around(list: &DisplayList, node: usize) -> Vec<LogicalRect> {
    let mut open = Vec::new();
    for (i, item) in list.items.iter().enumerate() {
        match item {
            DisplayListItem::PushClip { bounds, .. } => open.push(*bounds.inner()),
            DisplayListItem::PopClip => {
                open.pop();
            }
            DisplayListItem::HitTestArea { tag, .. }
                if tag.1 == TAG_TYPE_DOM_NODE
                    && list.node_mapping[i] == Some(NodeId::new(node)) =>
            {
                return open;
            }
            _ => {}
        }
    }
    panic!("node {node} has no hit-test area");
}

/// A3-core-05: a chain read from the memo is the chain of that box. The boxes of one parent
/// and one kind of positioning are painted in the same clips; a box of another parent, or an
/// absolutely positioned box beside a relatively positioned one of the same parent (it leaves
/// the clip of a parent that is not its containing block), is not.
#[test]
fn a_shared_scroll_chain_is_the_chain_of_every_box_that_shares_it() {
    let text = |t: &str| Dom::create_text_do_not_use_without_block_level_wrapper(t);
    let boxed = |css: &str| Dom::create_div().with_css(css).with_child(text("x"));
    let abs = |left: usize, top: usize| {
        boxed(&format!(
            "position: absolute; left: {left}px; top: {top}px; width: 20px; height: 20px;"
        ))
    };
    // 0 body, 1 layer, 2 A, 3/5/7 its absolute boxes (4/6/8 their texts), 9 B, 10/12 its
    // relative boxes, 14/16/18 its absolute boxes.
    let a = Dom::create_div()
        .with_css(
            "position: absolute; left: 10px; top: 10px; width: 200px; height: 100px; \
             overflow: hidden;",
        )
        .with_child(abs(5, 5))
        .with_child(abs(40, 5))
        .with_child(abs(80, 5));
    let b = Dom::create_div()
        .with_css("width: 300px; height: 120px; overflow: hidden;")
        .with_child(boxed("position: relative; width: 50px; height: 20px;"))
        .with_child(boxed("position: relative; width: 50px; height: 20px;"))
        .with_child(abs(300, 300))
        .with_child(abs(340, 300))
        .with_child(abs(380, 300));
    let layer = Dom::create_div()
        .with_css("position: relative; width: 600px; height: 400px;")
        .with_child(a)
        .with_child(b);
    let lw = laid_out(Dom::create_body().with_css("margin: 0px;").with_child(layer));
    let result = lw.layout_results.get(&DomId::ROOT_ID).expect("the root's layout");
    assert_eq!(result.styled_dom.node_data.as_ref().len(), 20, "harness: the page's nodes");
    let list = &result.display_list;

    let in_a = clips_around(list, 3);
    assert!(!in_a.is_empty(), "a box positioned in A is painted in A's clip");
    assert_eq!(clips_around(list, 5), in_a, "the second box of A, from the memo");
    assert_eq!(clips_around(list, 7), in_a, "the third box of A, from the memo");

    let relative_in_b = clips_around(list, 10);
    assert!(!relative_in_b.is_empty(), "a relative box of B is painted in B's clip");
    assert_eq!(clips_around(list, 12), relative_in_b, "the second relative box of B");
    assert_ne!(relative_in_b, in_a, "B's clip is not A's");

    let absolute_in_b = clips_around(list, 14);
    assert!(
        absolute_in_b.len() < relative_in_b.len(),
        "an absolute box of B leaves B's clip: its containing block is the layer"
    );
    assert_eq!(clips_around(list, 16), absolute_in_b, "the second absolute box of B");
    assert_eq!(clips_around(list, 18), absolute_in_b, "the third absolute box of B");
}

/// A3-core-12 (A3-solver 36a): an inset, margin, padding or border width is read from a compact
/// slot at rest (tenths of a pixel) and from the cascade in any other state, which gave the
/// declared number: `left: 10.04px` was 10.0 at rest and 10.04 hovered. Both give the slot's
/// number now. And a length the slot cannot hold (beyond +-3276 px) is kept beside the slots
/// when the cache is built: a resting read of it asks the cascade no more.
#[test]
fn an_inset_is_one_number_at_rest_and_hovered_and_costs_no_lookup_beyond_the_slot() {
    use azul_core::styled_dom::StyledNodeState;
    use azul_css::props::{
        basic::pixel::PixelValue,
        property::{CssProperty, CssPropertyType},
    };

    use crate::solver3::getters::get_css_left;

    // 0 body, then the boxes 1..=4.
    let boxed =
        |left: &str| Dom::create_div().with_css(&format!("position: absolute; left: {left};"));
    let mut styled = StyledDom::create_from_dom(
        Dom::create_body()
            .with_child(boxed("10.04px"))
            .with_child(boxed("10.05px"))
            .with_child(boxed("5000.25px"))
            .with_child(boxed("7px")),
    );
    let rest = StyledNodeState::new();
    let mut hovered = StyledNodeState::new();
    hovered.hover = true;
    let left = |styled: &StyledDom, node: usize, state: &StyledNodeState| {
        get_css_left(styled, NodeId::new(node), state).exact()
    };

    // 10.04 px is 100.4 tenths: 100, 10.0 px. Hovered it read 10.04.
    assert_eq!(left(&styled, 1, &rest), Some(PixelValue::px(10.0)));
    assert_eq!(left(&styled, 1, &hovered), Some(PixelValue::px(10.0)), "10.04px, hovered");
    // 10.05 px is 100.5 tenths (exactly, as f32): a half, away from zero: 101, 10.1 px.
    assert_eq!(left(&styled, 2, &rest), Some(PixelValue::px(10.1)));
    assert_eq!(left(&styled, 2, &hovered), Some(PixelValue::px(10.1)), "10.05px, hovered");
    // Beyond the slot (50,002.5 tenths): the declared length, in both states.
    assert_eq!(left(&styled, 3, &rest), Some(PixelValue::px(5000.25)));
    assert_eq!(left(&styled, 3, &hovered), Some(PixelValue::px(5000.25)), "5000.25px, hovered");

    // No cascade lookup for a resting read, in the slot's range or beyond it: an override
    // written into the cascade behind the compact cache's back is seen by the reads that ask
    // the cascade (another state) and by no resting read.
    let seven: CssProperty = styled
        .get_css_property_cache()
        .get_property(
            &styled.node_data.as_container()[NodeId::new(4)],
            &NodeId::new(4),
            &rest,
            &CssPropertyType::Left,
        )
        .cloned()
        .expect("left: 7px");
    {
        let overrides = &mut styled.css_property_cache.downcast_mut().user_overridden_properties;
        if overrides.len() < 5 {
            overrides.resize(5, Vec::new());
        }
        overrides[1] = vec![(CssPropertyType::Left, seven.clone())];
        overrides[3] = vec![(CssPropertyType::Left, seven)];
    }
    assert_eq!(
        left(&styled, 3, &hovered),
        Some(PixelValue::px(7.0)),
        "harness: the cascade answers the override"
    );
    assert_eq!(left(&styled, 1, &rest), Some(PixelValue::px(10.0)), "the slot was read");
    assert_eq!(
        left(&styled, 3, &rest),
        Some(PixelValue::px(5000.25)),
        "the kept length was read: a length beyond the slot cost a cascade lookup"
    );
}
