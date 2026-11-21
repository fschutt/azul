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
            DisplayList, DisplayListItem, EmitPhase, INLINE_LAYOUT_COPIES, SCROLL_CHAIN_WALKS,
            TAG_LIST_READS,
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
    lay_out_in(&mut lw, dom);
    lw
}

/// Lays `dom` out in `lw` and builds its display list.
fn lay_out_in(lw: &mut LayoutWindow, dom: Dom) {
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

/// A3-core-10: the accessibility pass walks every exposed node of every DOM after every layout
/// pass, read by nobody while no assistive technology is attached. A shell whose platform
/// adapter builds its update in a closure it runs only while one is active asks for the tree on
/// demand: a layout then only notes that a pass is due, and the pass runs when the update is
/// asked for. Without that setting everything is as it was.
#[cfg(feature = "a11y")]
#[test]
fn an_a11y_pass_on_demand_runs_when_the_update_is_asked_for() {
    // The default: the pass runs with the layout.
    let eager = laid_out(label_layer(50, 0));
    assert!(eager.a11y_manager.last_pass.nodes > 50, "the default builds with the layout");
    assert!(!eager.a11y_manager.stale);
    assert!(eager.a11y_tree_updates >= 1);

    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.a11y_manager.on_demand = true;
    assert!(!lw.a11y_update_pending(), "nothing is owed before a layout");
    lay_out_in(&mut lw, label_layer(50, 0));
    assert_eq!(lw.a11y_tree_updates, 0, "no pass ran with the layout");
    assert_eq!(lw.a11y_manager.last_pass.nodes, 0, "no node was visited");
    assert!(lw.a11y_manager.stale, "a pass is due");
    assert!(lw.a11y_update_pending());

    let update = lw.a11y_update_on_demand(true);
    assert!(update.tree.is_some(), "the whole tree");
    assert!(update.nodes.len() > 50, "{} nodes", update.nodes.len());
    assert!(!lw.a11y_manager.stale);
    assert!(!lw.a11y_update_pending());
    assert_eq!(lw.a11y_tree_updates, 1, "one pass");
    let nothing = lw.a11y_update_on_demand(false);
    assert!(nothing.tree.is_none() && nothing.nodes.is_empty(), "nothing changed since");
    assert_eq!(nothing.focus, update.focus, "the focus the adapter holds");
    assert_eq!(lw.a11y_tree_updates, 1, "and no second pass");
}

/// The ids of an update's nodes, sorted.
#[cfg(feature = "a11y")]
fn a11y_ids(update: &accesskit::TreeUpdate) -> Vec<u64> {
    let mut ids: Vec<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
    ids.sort_unstable();
    ids
}

/// A3-core-10: while no assistive technology is active the adapter's closure never runs, so
/// nothing is built however often the page is laid out, and what the layout side holds as "the
/// tree it sent" does not advance. The first update after an activation is asked for whole and
/// is the tree of the page as it is THEN, equal to the one an eager window publishes for that
/// page: not a patch against a tree the adapter was never given.
#[cfg(feature = "a11y")]
#[test]
fn nothing_is_built_while_no_reader_is_active_and_the_first_update_is_the_whole_tree() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.a11y_manager.on_demand = true;
    // Three layouts of three different pages, no closure run in between (no reader).
    for k in [50, 20, 30] {
        lay_out_in(&mut lw, label_layer(k, 0));
        assert_eq!(lw.a11y_tree_updates, 0, "no pass while nobody reads");
        assert!(lw.a11y_manager.retained.nodes.is_empty(), "the sent tree did not advance");
        assert!(lw.a11y_manager.last_tree_update.is_none(), "and nothing is parked");
        assert!(lw.a11y_update_pending());
    }

    // The reader arrives: the shell asks for the whole tree.
    let first = lw.a11y_update_on_demand(true);
    assert!(first.tree.is_some(), "the first update after an activation is a whole tree");
    let eager = laid_out(label_layer(30, 0));
    assert_eq!(
        a11y_ids(&first),
        a11y_ids(&eager.a11y_manager.full_tree()),
        "the tree of the page as it is now"
    );
    assert_eq!(lw.a11y_tree_updates, 1, "built once, when it was asked for");

    // From here on: patches.
    lay_out_in(&mut lw, label_layer(31, 0));
    assert_eq!(lw.a11y_tree_updates, 1, "a layout only notes that a pass is due");
    let patch = lw.a11y_update_on_demand(false);
    assert!(patch.tree.is_none(), "an active adapter gets what changed");
    assert!(!patch.nodes.is_empty() && patch.nodes.len() < first.nodes.len());

    // An adapter that missed an update (or a second activation) is handed the whole tree again,
    // parked patch or not, and without building a node twice.
    lay_out_in(&mut lw, label_layer(32, 0));
    let again = lw.a11y_update_on_demand(true);
    assert!(again.tree.is_some());
    let eager = laid_out(label_layer(32, 0));
    assert_eq!(a11y_ids(&again), a11y_ids(&eager.a11y_manager.full_tree()));
    assert!(lw.a11y_manager.last_tree_update.is_none(), "the patch is part of the whole tree");
}

/// A3-core-10: the closure has to return an update whatever happened. With no tree to hand out
/// (the pass was refused) an adapter that needs a whole tree gets the window alone, which is one;
/// an active one keeps what it has (a patch without a node, the focus it already holds).
#[cfg(feature = "a11y")]
#[test]
fn an_adapter_is_always_handed_an_update_it_can_take() {
    use crate::managers::a11y::A11yManager;

    let mut manager = A11yManager::new();
    let whole = manager.hand_to_adapter(true);
    assert!(whole.tree.is_some());
    assert_eq!(whole.nodes.len(), 1, "the window alone");
    assert_eq!(whole.nodes[0].0, manager.root_id);
    assert_eq!(whole.focus, manager.root_id);

    let patch = manager.hand_to_adapter(false);
    assert!(patch.tree.is_none() && patch.nodes.is_empty());
    assert_eq!(patch.focus, manager.root_id, "the focus handed out before");
}

/// A3-core-17 (A3-window-11): the rects of a selection were painted without a layout tag, so a
/// patch could not find the old highlight of a block. They carry the block's layout index and
/// `EmitPhase::Selection` now; nothing else carries that phase, and a list without a selection
/// has no such item.
#[test]
fn the_rects_of_a_selection_carry_their_blocks_layout_index_and_the_selection_phase() {
    use azul_core::{dom::DomNodeId, styled_dom::NodeHierarchyItemId};

    let page = Dom::create_body()
        .with_css("margin: 0px;")
        .with_child(Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "a line of text to select",
        )))
        .with_child(Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "and a second paragraph",
        )));
    let mut lw = laid_out_with(page, FcFontCache::build());
    let selection_tags = |lw: &LayoutWindow| -> Vec<(usize, usize)> {
        let list = &lw.layout_results.get(&DomId::ROOT_ID).expect("the root's layout").display_list;
        list.layout_node_mapping
            .iter()
            .enumerate()
            .filter_map(|(i, tag)| match tag {
                Some((index, EmitPhase::Selection)) => Some((i, *index)),
                _ => None,
            })
            .collect()
    };
    assert!(selection_tags(&lw).is_empty(), "no selection, no item of the selection phase");

    let body = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
    };
    assert!(lw.select_all_text(body), "harness: select-all made a selection");
    let result = lw.layout_results.get(&DomId::ROOT_ID).expect("the root's layout");
    let list = &result.display_list;
    let rects: Vec<usize> = list
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, DisplayListItem::SelectionRect { .. }))
        .map(|(i, _)| i)
        .collect();
    assert!(rects.len() >= 2, "harness: both paragraphs are highlighted ({} rects)", rects.len());

    let tagged = selection_tags(&lw);
    for i in &rects {
        assert!(
            tagged.iter().any(|(item, _)| item == i),
            "selection rect {i} carries no selection tag: {:?}",
            list.layout_node_mapping[*i]
        );
    }
    // Each run belongs to one box, and that box is a text block's root: the content run that
    // follows the highlight is tagged with the same layout index.
    let blocks: std::collections::BTreeSet<usize> = tagged.iter().map(|(_, index)| *index).collect();
    assert!(blocks.len() >= 2, "one run per highlighted block: {blocks:?}");
    for index in &blocks {
        let last = tagged.iter().filter(|(_, at)| at == index).map(|(i, _)| *i).max().expect("a run");
        let content = list.layout_node_mapping[last + 1..]
            .iter()
            .find(|tag| !matches!(tag, Some((_, EmitPhase::Selection))))
            .copied()
            .flatten();
        assert_eq!(
            content,
            Some((*index, EmitPhase::Content)),
            "the highlight of box {index} is followed by that box's content run"
        );
    }
    // Only what `paint_selections` pushes has the phase.
    for (i, _) in &tagged {
        assert!(
            matches!(list.items[*i], DisplayListItem::SelectionRect { .. }),
            "item {i} of the selection phase is {:?}",
            list.items[*i]
        );
    }
}

/// One accessibility pass of `m` over `results` (a nested DOM under its host in `hosts`), with
/// the DOMs' generations or without, the caret in `caret_in` if any: what the pass did and what
/// it published.
#[cfg(feature = "a11y")]
fn a11y_pass(
    m: &mut crate::managers::a11y::A11yManager,
    results: &std::collections::BTreeMap<DomId, crate::window::DomLayoutResult>,
    hosts: &std::collections::BTreeMap<DomId, (DomId, NodeId)>,
    generations: Option<&std::collections::BTreeMap<DomId, u64>>,
    caret_in: Option<DomId>,
) -> (crate::managers::a11y::A11yPassStats, Option<accesskit::TreeUpdate>) {
    use crate::managers::{
        a11y::{A11yTreeInputs, CursorA11yInfo},
        scroll_state::ScrollManager,
    };

    let scroll_manager = ScrollManager::new();
    let overrides = std::collections::BTreeMap::new();
    let title = azul_css::AzString::from("t");
    let inputs = A11yTreeInputs {
        layout_results: results,
        scroll_manager: &scroll_manager,
        window_title: &title,
        window_size: LogicalSize::new(640.0, 480.0),
        focused_node: None,
        hidpi_factor: 1.0,
        dirty_text_overrides: &overrides,
        cursor_info: caret_in.map(|dom_id| CursorA11yInfo {
            dom_id,
            node_id: NodeId::new(0),
            anchor_offset: 0,
            focus_offset: 0,
        }),
        nested_dom_hosts: hosts,
        language: "",
        language_is_rtl: false,
        translate: None,
        dom_generations: generations,
    };
    let stats = m.refresh(&inputs);
    (stats, m.take_pending())
}

/// An update as something two managers can be compared by: whole tree or patch, the focus, and
/// every node with its id, sorted.
#[cfg(feature = "a11y")]
fn a11y_update_key(update: Option<&accesskit::TreeUpdate>) -> Option<(bool, u64, Vec<(u64, accesskit::Node)>)> {
    update.map(|update| {
        let mut nodes: Vec<(u64, accesskit::Node)> =
            update.nodes.iter().map(|(id, node)| (id.0, node.clone())).collect();
        nodes.sort_by_key(|(id, _)| *id);
        (update.tree.is_some(), update.focus.0, nodes)
    })
}

/// A3-core-10 (a): a pass told the DOMs' generations puts a DOM that is still the one it was
/// back into the tree without reading a node of it (`walked` counts the nodes read, `replayed`
/// the ones put back), whatever happened to the other DOMs, and publishes exactly what a pass
/// that walks everything publishes. The map view under a window whose sidebar changed is the
/// case: `k` labels are replayed, the window's own few nodes are walked, at `k` and at `4k`.
#[cfg(feature = "a11y")]
#[test]
fn an_unchanged_dom_is_put_back_into_the_accessibility_tree_unread() {
    use std::collections::BTreeMap;

    use crate::managers::a11y::A11yManager;

    // 0 body, 1 a sidebar's div, 2 its text, 3 the view's host.
    let window = |sidebar: &str, host_left: usize| {
        let dom = Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(
                Dom::create_div()
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(sidebar)),
            )
            .with_child(Dom::create_div().with_css(&format!(
                "position: absolute; left: {host_left}px; top: 40px; width: 300px; height: 200px;"
            )));
        laid_out(dom).layout_results.remove(&DomId::ROOT_ID).expect("the window's layout")
    };
    let view_dom = DomId { inner: 1 };
    let mut hosts = BTreeMap::new();
    hosts.insert(view_dom, (DomId::ROOT_ID, NodeId::new(3)));
    let gens = |window: u64, view: u64| {
        let mut g = BTreeMap::new();
        g.insert(DomId::ROOT_ID, window);
        g.insert(view_dom, view);
        g
    };

    let mut walked_with_the_window_changed = Vec::new();
    for k in [50usize, 200] {
        let view = laid_out(label_layer(k, 0))
            .layout_results
            .remove(&DomId::ROOT_ID)
            .expect("the view's layout");
        let mut told = A11yManager::new();
        let mut untold = A11yManager::new();
        let mut results = BTreeMap::new();
        results.insert(DomId::ROOT_ID, window("layers", 10));
        results.insert(view_dom, view);

        // Both managers see the same passes; `told` knows the generations.
        let mut step = |results: &BTreeMap<_, _>, g: &BTreeMap<DomId, u64>, caret: Option<DomId>| {
            let (stats, update) = a11y_pass(&mut told, results, &hosts, Some(g), caret);
            let (reference, expected) = a11y_pass(&mut untold, results, &hosts, None, caret);
            assert_eq!(
                a11y_update_key(update.as_ref()),
                a11y_update_key(expected.as_ref()),
                "a pass that replays publishes what a pass that walks publishes (k = {k})"
            );
            assert_eq!(reference.replayed, 0, "without generations every DOM is walked");
            assert_eq!(stats.walked + stats.replayed, stats.nodes - 1, "every node, once: {stats:?}");
            assert_eq!(reference.walked, reference.nodes - 1);
            stats
        };

        // 1. The first pass walks everything.
        let first = step(&results, &gens(1, 1), None);
        assert_eq!(first.replayed, 0);
        let in_view = first.nodes - 1 - 4;
        assert!(in_view > k, "harness: the view's labels are in the tree ({in_view} of {k})");

        // 2. Nothing changed: no node of either DOM is read.
        let second = step(&results, &gens(1, 1), None);
        assert_eq!((second.walked, second.replayed), (0, first.nodes - 1));
        assert!(!second.published);

        // 3. The window is another DOM (its sidebar says something else), the view is not, and
        //    its host is where it was: the window's 4 nodes are read, the view's none.
        results.insert(DomId::ROOT_ID, window("styles", 10));
        let third = step(&results, &gens(2, 1), None);
        assert_eq!((third.walked, third.replayed), (4, in_view), "k = {k}");
        assert!(third.published, "the sidebar's new text is published");
        walked_with_the_window_changed.push(third.walked);

        // 4. The host moved: the view's top-level node is placed against it, the view is
        //    walked (and the comparison above holds its new place to the walking pass's).
        results.insert(DomId::ROOT_ID, window("styles", 30));
        let fourth = step(&results, &gens(3, 1), None);
        assert_eq!(fourth.replayed, 0, "a view whose host moved is walked");

        // 5. The caret is in the view: its nodes may carry what is not the DOM's. Walked, the
        //    window (unchanged) replayed.
        let fifth = step(&results, &gens(3, 1), Some(view_dom));
        assert_eq!((fifth.walked, fifth.replayed), (in_view, 4));
        // 6. The caret left: walked once more (what it left behind is rebuilt), 7. then not.
        let sixth = step(&results, &gens(3, 1), None);
        assert_eq!((sixth.walked, sixth.replayed), (in_view, 4));
        let seventh = step(&results, &gens(3, 1), None);
        assert_eq!((seventh.walked, seventh.replayed), (0, first.nodes - 1));

        // 8. The view is another DOM with the same content: walked, nothing to publish.
        let eighth = step(&results, &gens(3, 2), None);
        assert_eq!((eighth.walked, eighth.replayed), (in_view, 4));
        assert!(!eighth.published);
    }
    assert_eq!(
        walked_with_the_window_changed,
        vec![4, 4],
        "the nodes read when the window changes do not grow with the view"
    );
}
