//! A3-solver, quadratics in solver3.
//!
//! Size sweeps: a page of `n` boxes and one of `4 n` are laid out through a window
//! (`LayoutWindow`, the product path) and a COUNT of what the solver looked at is compared - a
//! test-only thread-local next to the code it counts, as `in_flow_probe_tests` does. A linear
//! step grows 4x, an `n log n` one about 5x, a quadratic one 16x; the bound is 6x. No wall
//! clock: the machine is shared.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};

/// A window without fonts (the sweeps count lookups, no glyph is needed).
fn window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a layout window")
}

/// Lays `dom` out in `lw`, 640 x 480.
fn lay_out(lw: &mut LayoutWindow, dom: Dom) {
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

/// Where the box of DOM node `node` was placed.
fn position_of(lw: &LayoutWindow, node: usize) -> LogicalPosition {
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let index = *lr
        .layout_tree
        .dom_to_layout
        .get(&NodeId::new(node))
        .and_then(|v| v.first())
        .expect("the node is laid out");
    *lr.calculated_positions
        .get(index.index())
        .expect("the node has a position")
}

/// `body(0) > div(1) > k x [div(2 + 2 i) > text]`: a positioned block of `k` absolutely
/// positioned BLOCKS with a text (a map's labels as divs), the i-th at
/// `(10 * (i % 50), 9 * (i / 50))` - 2,000 of them stay inside the layer, so no size of a sweep
/// overflows the window where another does not. All its children being out-of-flow blocks, the
/// layer is a block container of blocks (`layout_tree::in_flow_block_level_mask`), laid out by
/// `fc::layout_bfc`, which records a static position for each of them.
fn layer_of_absolute_blocks(k: usize) -> Dom {
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    for i in 0..k {
        layer = layer.with_child(Dom::create_div_with_text(format!("label {i}")).with_css(
            &format!(
                "position: absolute; left: {}px; top: {}px;",
                (i % 50) * 10,
                (i / 50) * 9
            ),
        ));
    }
    Dom::create_body().with_css("margin: 0px;").with_child(layer)
}

/// A3-solver-01. `cache::process_out_of_flow_children` asked for every recorded static position
/// whether its box is a child of the block, by `children.contains(index)`: k positions x k / 2
/// children. It asks a sorted copy of the children by binary search.
///
/// Mutation: `is_one_of_sorted` as a linear scan (`sorted.iter().any(|c| { count; *c == index })`)
/// grows 16x here.
#[test]
fn a_block_of_absolute_blocks_finds_each_of_them_among_its_children_by_bisection() {
    use crate::solver3::cache::CHILD_LOOKUP_STEPS;
    let steps = |k: usize| {
        let mut lw = window();
        CHILD_LOOKUP_STEPS.with(|n| n.set(0));
        lay_out(&mut lw, layer_of_absolute_blocks(k));
        CHILD_LOOKUP_STEPS.with(std::cell::Cell::get)
    };
    let (small, large) = (steps(500), steps(2_000));
    assert!(
        small >= 500,
        "harness: each of the 500 static positions was looked up among the children ({small})"
    );
    assert!(
        large < small * 6,
        "four times the blocks took {large} comparisons for {small}: more than n log n"
    );
}

/// A3-solver-01, the result: every absolutely positioned block of the layer is laid out and
/// stands at its insets (the layer's padding box starts at the window's origin), whichever way
/// its parent found it among its children.
#[test]
fn absolute_blocks_of_a_block_stand_at_their_insets() {
    let mut lw = window();
    lay_out(&mut lw, layer_of_absolute_blocks(120));
    for i in [0_usize, 1, 49, 50, 51, 119] {
        let p = position_of(&lw, 2 + 2 * i);
        #[allow(clippy::cast_precision_loss)] // small pixel counts
        let (x, y) = (((i % 50) * 10) as f32, ((i / 50) * 9) as f32);
        assert!(
            (p.x - x).abs() < 0.01 && (p.y - y).abs() < 0.01,
            "block {i} stands at ({}, {}), its insets say ({x}, {y})",
            p.x,
            p.y
        );
    }
}

/// `body > div > r x [span > text, div]`: a block whose inline content is cut into `r` runs
/// by `r` blocks. Each run is wrapped in an anonymous block (CSS 2.2 s9.2.1.1), so the layer's
/// box has `2 r` children: `r` anonymous inline wrappers and `r` blocks.
fn block_of_inline_runs(r: usize) -> Dom {
    let mut layer = Dom::create_div().with_css("width: 600px;");
    for i in 0..r {
        layer = layer
            .with_child(Dom::create_span_with_text(format!("run {i}")))
            .with_child(Dom::create_div().with_css("height: 2px;"));
    }
    Dom::create_body().with_css("margin: 0px;").with_child(layer)
}

/// A3-solver-02. When a page is laid out again, `cache::try_reuse_anon_wrapper` matches the
/// i-th anonymous wrapper of a block with the i-th of the block's old box, which it found by
/// counting the old box's children off from the first for every run
/// (`.filter(is_wrapper).nth(i)`): r runs x r children. The old box's wrappers are listed once.
///
/// Mutation: listing them for every run (`*old_wrappers = None` before the listing, or the old
/// `.filter(..).nth(anon_ordinal)`) looks at 16x the children here.
#[test]
fn a_block_of_many_inline_runs_lists_the_wrappers_of_its_old_box_once() {
    use crate::solver3::cache::OLD_WRAPPER_CANDIDATES;
    let looked_at = |r: usize| {
        let mut lw = window();
        lay_out(&mut lw, block_of_inline_runs(r));
        OLD_WRAPPER_CANDIDATES.with(|n| n.set(0));
        // The same page again: every run finds the wrapper it had.
        lay_out(&mut lw, block_of_inline_runs(r));
        OLD_WRAPPER_CANDIDATES.with(std::cell::Cell::get)
    };
    let (small, large) = (looked_at(100), looked_at(400));
    assert!(
        small >= 100,
        "harness: the second layout looked for the wrappers of the old box ({small})"
    );
    assert!(
        large < small * 6,
        "four times the runs looked at {large} old children for {small}: more than linear"
    );
}

/// `body > div > j x [abs div > text] + m x [div > text]`: a block that starts with `j`
/// absolutely positioned blocks (overlays written before the content) followed by `m` blocks in
/// flow, every one with a line of text.
fn overlays_then_blocks(j: usize, m: usize) -> Dom {
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px;");
    for i in 0..j {
        layer = layer.with_child(Dom::create_div_with_text(format!("over {i}")).with_css(
            &format!("position: absolute; left: {}px; top: {}px;", (i % 50) * 10, (i / 50) * 9),
        ));
    }
    for i in 0..m {
        layer = layer.with_child(Dom::create_div_with_text(format!("line {i}")));
    }
    Dom::create_body().with_css("margin: 0px;").with_child(layer)
}

/// A3-solver-03. Every box with a line of text asks whether it starts its parent's first line
/// (`fc::is_first_in_flow_child`). The answer looked at the parent's children from the first
/// one: each of `m` in-flow blocks behind `j` out-of-flow ones passed those `j` again. It
/// looks from the box backwards to the nearest in-flow sibling.
///
/// Mutation: scanning forwards from the parent's first child again (the loop before) grows 16x.
#[test]
fn blocks_behind_out_of_flow_siblings_look_back_to_the_nearest_in_flow_one() {
    use crate::solver3::fc::IN_FLOW_CHECKS;
    let checks = |n: usize| {
        let mut lw = window();
        IN_FLOW_CHECKS.with(|c| c.set(0));
        lay_out(&mut lw, overlays_then_blocks(n, n));
        IN_FLOW_CHECKS.with(std::cell::Cell::get)
    };
    let (small, large) = (checks(150), checks(600));
    assert!(
        small >= 150,
        "harness: the blocks were asked whether they are in flow ({small})"
    );
    assert!(
        large < small * 6,
        "four times the blocks took {large} in-flow checks for {small}: more than linear"
    );
}

/// A3-solver-03, the result: the blocks in flow are stacked from the top of the layer in
/// document order, the overlays before them take no room.
#[test]
fn blocks_behind_out_of_flow_siblings_are_stacked_from_the_top() {
    let mut lw = window();
    // body(0) > layer(1) > 3 x [abs div, text] (2..=7) + 4 x [div, text] (8, 10, 12, 14).
    lay_out(&mut lw, overlays_then_blocks(3, 4));
    let ys: Vec<f32> = [8_usize, 10, 12, 14].iter().map(|&n| position_of(&lw, n).y).collect();
    assert!(ys[0].abs() < 0.01, "the first block in flow starts the layer: {ys:?}");
    assert!(
        ys.windows(2).all(|w| w[1] >= w[0]),
        "the blocks in flow follow each other downwards: {ys:?}"
    );
}

/// A3-solver-04. `getters::get_scrollbar_style` made the part of its answer that reads no node
/// (a clone of the cascade's context, the UA scrollbar rules evaluated against it, the system's
/// scrollbar merged in) for every node of every pass before it found that the node has no
/// scrollbar CSS. It is made once per context and kept.
///
/// Mutation: building it on every ask (no `SCROLLBAR_BASE`) makes `builds` equal `asks`.
#[test]
fn the_scrollbar_style_every_node_starts_from_is_made_once_per_context() {
    use crate::solver3::getters::{SCROLLBAR_BASE_BUILDS, SCROLLBAR_STYLE_ASKS};
    let mut lw = window();
    SCROLLBAR_STYLE_ASKS.with(|n| n.set(0));
    SCROLLBAR_BASE_BUILDS.with(|n| n.set(0));
    lay_out(&mut lw, layer_of_absolute_blocks(300));
    let asks = SCROLLBAR_STYLE_ASKS.with(std::cell::Cell::get);
    let builds = SCROLLBAR_BASE_BUILDS.with(std::cell::Cell::get);
    assert!(
        asks >= 300,
        "harness: the layout asked for the scrollbar style of each of the 300 blocks ({asks})"
    );
    assert!(
        builds <= 4,
        "one page under one context made the node-independent scrollbar style {builds} times \
         for {asks} asks"
    );
}

/// `body(0) > div(1) > k x [abs div(a) > flex row(a + 1) > items x [div > text]]` with
/// `a = 2 + (2 + 2 items) i`: a canvas of `k` absolutely positioned boxes, each holding a flex
/// row of `items` fixed-size items with a text - a node graph in small. Every row is a flex
/// container under a block, so every row is laid out through a taffy bridge of its own.
fn canvas_of_flex_nodes(k: usize, items: usize) -> Dom {
    let mut canvas = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    for i in 0..k {
        let mut row = Dom::create_div().with_css("display: flex; flex-direction: row;");
        for j in 0..items {
            row = row.with_child(
                Dom::create_div_with_text(format!("{i}.{j}"))
                    .with_css("width: 30px; height: 10px; flex-shrink: 0;"),
            );
        }
        canvas = canvas.with_child(
            Dom::create_div()
                .with_css(&format!(
                    "position: absolute; left: {}px; top: {}px;",
                    (i % 10) * 40,
                    (i / 10) * 12
                ))
                .with_child(row),
        );
    }
    Dom::create_body().with_css("margin: 0px;").with_child(canvas)
}

/// A3-solver-05. A taffy bridge is made for every flex or grid container that a block, an
/// inline or the positioning pass lays out, and each bridge holds two style memos with one cell
/// per node of the WHOLE document. A cell was a `OnceCell<taffy::Style>` (several hundred
/// bytes): megabytes allocated, initialised and dropped per bridge. It is a pointer.
///
/// Mutation: an unboxed cell (`OnceCell<Style>`) is as large as a `Style` and fails the bound.
#[test]
fn a_taffy_bridge_keeps_a_pointer_per_node_not_a_style() {
    use crate::solver3::taffy_bridge::{BRIDGES_MADE, STYLE_CELL_BYTES};
    let mut lw = window();
    BRIDGES_MADE.with(|n| n.set(0));
    STYLE_CELL_BYTES.with(|n| n.set(0));
    lay_out(&mut lw, canvas_of_flex_nodes(40, 2));
    let bridges = BRIDGES_MADE.with(std::cell::Cell::get);
    let cell = STYLE_CELL_BYTES.with(std::cell::Cell::get);
    assert!(
        bridges >= 40,
        "harness: each of the 40 flex rows under a block got a bridge ({bridges})"
    );
    assert!(
        cell > 0 && cell <= 2 * core::mem::size_of::<usize>(),
        "a cell of a bridge's style memo takes {cell} bytes, times every node of the document, \
         times {bridges} bridges"
    );
}

/// A3-solver-05 / -06, the result: the flex rows of the canvas place their items side by side
/// at the widths they state, and every box of the canvas stands at its insets.
#[test]
fn flex_rows_in_absolute_boxes_place_their_items_side_by_side() {
    let mut lw = window();
    lay_out(&mut lw, canvas_of_flex_nodes(40, 3));
    for i in [0_usize, 9, 10, 39] {
        // abs div(a), row(a + 1), items a + 2, a + 4, a + 6 (each followed by its text).
        let a = 2 + 8 * i;
        #[allow(clippy::cast_precision_loss)] // small pixel counts
        let (x, y) = (((i % 10) * 40) as f32, ((i / 10) * 12) as f32);
        let node = position_of(&lw, a);
        assert!(
            (node.x - x).abs() < 0.01 && (node.y - y).abs() < 0.01,
            "box {i} stands at ({}, {}), its insets say ({x}, {y})",
            node.x,
            node.y
        );
        let items: Vec<LogicalPosition> =
            [a + 2, a + 4, a + 6].iter().map(|&n| position_of(&lw, n)).collect();
        for (j, item) in items.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let item_x = x + 30.0 * j as f32;
            assert!(
                (item.x - item_x).abs() < 0.01 && (item.y - y).abs() < 0.01,
                "item {j} of row {i} stands at ({}, {}), not ({item_x}, {y})",
                item.x,
                item.y
            );
        }
    }
}

/// A3-solver-06. Taffy asks a container for its children through `child_ids` (once per run),
/// `child_count` and `get_child_id(i)` (for every child, twice in a final run). The bridge
/// built the filtered child list for each of those asks: `2 n + 4` lists of `n` per container
/// and run. `child_count` and `get_child_id` count and step without a list.
///
/// Mutation: `get_child_id` through `get_layout_children(..)[index]` builds a list per child:
/// 8 items per row give 19 lists per bridge and fail the bound.
#[test]
fn taffy_counts_and_indexes_a_containers_children_without_listing_them() {
    use crate::solver3::taffy_bridge::{BRIDGES_MADE, CHILD_LISTS_BUILT};
    let mut lw = window();
    BRIDGES_MADE.with(|n| n.set(0));
    CHILD_LISTS_BUILT.with(|n| n.set(0));
    lay_out(&mut lw, canvas_of_flex_nodes(40, 8));
    let bridges = BRIDGES_MADE.with(std::cell::Cell::get);
    let lists = CHILD_LISTS_BUILT.with(std::cell::Cell::get);
    assert!(
        bridges >= 40 && lists >= 40,
        "harness: 40 flex rows were laid out ({bridges} bridges, {lists} child lists)"
    );
    assert!(
        lists <= 4 * bridges,
        "{bridges} runs of a flex row of 8 items built {lists} child lists"
    );
}

/// `body(0) > div(1) > k x [span(2 + 2 i) > text]`: a positioned block of `k` absolutely
/// positioned SPANS with a text (the map editor's labels), the i-th at
/// `(10 * (i % 50), 9 * (i / 50))`. A span is inline-level, so the layer holds inline content
/// only and is itself the root of an inline formatting context (`layout_ifc`) with `k`
/// out-of-flow children.
fn layer_of_absolute_spans(k: usize) -> Dom {
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    for i in 0..k {
        layer = layer.with_child(Dom::create_span_with_text(format!("label {i}")).with_css(
            &format!(
                "position: absolute; left: {}px; top: {}px;",
                (i % 50) * 10,
                (i / 50) * 9
            ),
        ));
    }
    Dom::create_body().with_css("margin: 0px;").with_child(layer)
}

/// A3-solver-07. Collecting the content of an inline formatting context looked every element
/// child of the root up among the root's layout children with a scan from the first one
/// (`children.iter().find(..)` in `fc::collect_and_measure_inline_content_impl`), before it
/// asked whether the child is out of flow: a layer of n absolutely positioned spans took
/// n * n / 2 steps. The layout children are listed by DOM node once and bisected.
///
/// Mutation: the scan per child (counting each layout child it passes) grows 16x.
#[test]
fn an_inline_root_of_absolute_spans_finds_each_of_them_among_its_children_by_bisection() {
    use crate::solver3::fc::IFC_CHILD_PROBES;
    let probes = |k: usize| {
        let mut lw = window();
        IFC_CHILD_PROBES.with(|n| n.set(0));
        lay_out(&mut lw, layer_of_absolute_spans(k));
        IFC_CHILD_PROBES.with(std::cell::Cell::get)
    };
    let (small, large) = (probes(500), probes(2_000));
    assert!(
        small >= 500,
        "harness: each of the 500 spans was looked up among the layer's children ({small})"
    );
    assert!(
        large < small * 6,
        "four times the spans took {large} steps for {small}: more than n log n"
    );
}

/// A3-solver-07, the result: every absolutely positioned span of the layer is laid out and
/// stands at its insets.
#[test]
fn absolute_spans_of_an_inline_root_stand_at_their_insets() {
    let mut lw = window();
    lay_out(&mut lw, layer_of_absolute_spans(120));
    for i in [0_usize, 1, 49, 50, 51, 119] {
        let p = position_of(&lw, 2 + 2 * i);
        #[allow(clippy::cast_precision_loss)] // small pixel counts
        let (x, y) = (((i % 50) * 10) as f32, ((i / 50) * 9) as f32);
        assert!(
            (p.x - x).abs() < 0.01 && (p.y - y).abs() < 0.01,
            "span {i} stands at ({}, {}), its insets say ({x}, {y})",
            p.x,
            p.y
        );
    }
}

/// A3-solver-09. The intrinsic size of a block (and of a flex container) is folded from its
/// children's, which the recursion hands over as `(child, sizes)` pairs in child order; each
/// child's pair was looked up with a scan of the pairs from the first one
/// (`child_intrinsics.iter().find(..)` in `sizing::calculate_block_intrinsic_sizes`):
/// n * n / 2 comparisons for a block of n children. Each pair is taken where the one before
/// it ended.
///
/// Mutation: the scan from the first pair for every child grows 16x.
#[test]
fn a_block_folds_its_childrens_intrinsic_sizes_in_one_walk() {
    use crate::solver3::sizing::INTRINSIC_CHILD_PROBES;
    let probes = |k: usize| {
        let mut lw = window();
        INTRINSIC_CHILD_PROBES.with(|n| n.set(0));
        lay_out(&mut lw, layer_of_absolute_blocks(k));
        INTRINSIC_CHILD_PROBES.with(std::cell::Cell::get)
    };
    let (small, large) = (probes(500), probes(2_000));
    assert!(
        small >= 500,
        "harness: the layer asked for the intrinsic sizes of its 500 children ({small})"
    );
    assert!(
        large < small * 6,
        "four times the children took {large} comparisons for {small}: more than linear"
    );
}

/// A3-solver-10. The reconcile computes a node's fingerprint to compare it with its old box,
/// and for a node it builds fresh the tree builder computed the same fingerprint again
/// (`LayoutTreeBuilder::create_node_from_dom`). The reconcile hands over the one it computed.
///
/// Mutation: `create_node_from_dom` (no fingerprint handed over) in `cache::reconcile_node`
/// makes the builder compute one per node.
#[test]
fn the_reconcile_hands_the_builder_the_fingerprint_it_computed() {
    use crate::solver3::layout_tree::BUILDER_FINGERPRINTS;
    let mut lw = window();
    BUILDER_FINGERPRINTS.with(|n| n.set(0));
    lay_out(&mut lw, layer_of_absolute_spans(200));
    let by_builder = BUILDER_FINGERPRINTS.with(std::cell::Cell::get);
    let nodes = lw.layout_results[&DomId::ROOT_ID].layout_tree.nodes.len();
    assert!(nodes >= 400, "harness: the 200 labels and their texts have boxes ({nodes})");
    assert!(
        by_builder <= 4,
        "the builder computed {by_builder} fingerprints of its own for a tree of {nodes} boxes \
         the reconcile had fingerprinted"
    );
}

/// [`lay_out`] for a styled DOM (one whose node states were set before the layout).
fn lay_out_styled(lw: &mut LayoutWindow, styled: StyledDom) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// A3-solver-12. The font-size memo answered for the normal state only. A window raises
/// `:focus-within` on the focused node and all its ancestors (and `:hover` on what is under the
/// pointer) before a layout, so the root's font size - asked for every node's rem lengths -
/// and the font size of such a box, asked by each of its children as their parent's, were
/// resolved by a walk to the root on every call. The memo answers for a node in its own state.
///
/// Mutation: `if !node_state.is_normal() { return None; }` in `getters::memoised_font_size`
/// walks at least twice per label of the focused page.
#[test]
fn the_font_sizes_of_a_focused_page_are_answered_by_the_memo() {
    use crate::solver3::getters::FONT_SIZE_WALKS;
    let walks = |focused: bool| {
        let mut styled = StyledDom::create_from_dom(layer_of_absolute_spans(300));
        if focused {
            // The first label: it, the layer and the body are `:focus-within`.
            styled.raise_focus_within(NodeId::new(2));
        }
        let mut lw = window();
        FONT_SIZE_WALKS.with(|n| n.set(0));
        lay_out_styled(&mut lw, styled);
        FONT_SIZE_WALKS.with(std::cell::Cell::get)
    };
    let (at_rest, focused) = (walks(false), walks(true));
    assert!(
        focused <= at_rest + 30,
        "a page of 300 labels walked the ancestors for a font size {focused} times with a \
         focused label, {at_rest} times at rest"
    );
}

/// A3-solver-12, the result: em and rem insets of a box under a focused root resolve against
/// the box's own and the root's font size, as at rest.
#[test]
fn em_and_rem_insets_resolve_alike_at_rest_and_under_a_focused_root() {
    // body(0) > layer(1) > label(2) > text(3).
    let page = || {
        Dom::create_body().with_css("margin: 0px; font-size: 32px;").with_child(
            Dom::create_div()
                .with_css("position: relative; width: 600px; height: 400px; font-size: 20px;")
                .with_child(
                    Dom::create_div_with_text("label")
                        .with_css("position: absolute; left: 2em; top: 1rem; font-size: 10px;"),
                ),
        )
    };
    for focused in [false, true] {
        let mut styled = StyledDom::create_from_dom(page());
        if focused {
            styled.raise_focus_within(NodeId::new(2));
        }
        let mut lw = window();
        lay_out_styled(&mut lw, styled);
        let p = position_of(&lw, 2);
        assert!(
            (p.x - 20.0).abs() < 0.01 && (p.y - 32.0).abs() < 0.01,
            "focused = {focused}: `left: 2em` of a 10px box and `top: 1rem` under a 32px root \
             put it at ({}, {}), not (20, 32)",
            p.x,
            p.y
        );
    }
}

/// `body > div > k x [label > text]`: the map editor's label layer WITHOUT its groups of 64 -
/// `k` absolutely positioned labels under one positioned block, every third one turned by its
/// own `transform` (a reference frame each). `spans`: the labels are spans (the layer is the
/// root of an inline formatting context) or divs (the layer is a block container of blocks).
fn flat_layer_of_turned_labels(k: usize, spans: bool) -> Dom {
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    for i in 0..k {
        let turn = if i % 3 == 0 {
            format!(" transform: rotate({}deg);", (i % 7) * 10 + 5)
        } else {
            String::new()
        };
        let css = format!(
            "position: absolute; left: {}px; top: {}px;{turn}",
            (i % 50) * 10,
            (i / 50) * 9
        );
        let text = format!("label {i}");
        layer = layer.with_child(if spans {
            Dom::create_span_with_text(text).with_css(&css)
        } else {
            Dom::create_div_with_text(text).with_css(&css)
        });
    }
    Dom::create_body().with_css("margin: 0px;").with_child(layer)
}

/// A3-solver-13, the audit's question 1: a FLAT list of absolutely positioned siblings with
/// transforms under one parent is laid out and painted with a number of `position` lookups that
/// grows with the list, not with its square - for labels that are spans and labels that are
/// divs. (`get_position_type` is asked by nearly every walk over children, siblings or
/// ancestors: the scans found so far - Z10, A3-solver-01, -03, -07 - all asked it or stood next
/// to a walk that does. The scans that ask nothing have counters of their own above.)
#[test]
fn a_flat_layer_of_absolute_labels_takes_position_lookups_in_proportion_to_its_labels() {
    use crate::solver3::positioning::POSITION_TYPE_LOOKUPS;
    for spans in [true, false] {
        let lookups = |k: usize| {
            let mut lw = window();
            POSITION_TYPE_LOOKUPS.with(|n| n.set(0));
            lay_out(&mut lw, flat_layer_of_turned_labels(k, spans));
            POSITION_TYPE_LOOKUPS.with(std::cell::Cell::get)
        };
        let (small, large) = (lookups(400), lookups(1_600));
        assert!(
            small >= 400,
            "harness (spans = {spans}): every label's position was looked up ({small})"
        );
        assert!(
            large < small * 6,
            "spans = {spans}: four times the labels took {large} position lookups for {small}: \
             more than linear"
        );
    }
}

/// `dom` under the stylesheet `css` (its nodes are matched by class).
fn styled_page(mut dom: Dom, css: &str) -> StyledDom {
    let (css, _warnings) = azul_css::parser2::new_from_str(css);
    StyledDom::create(&mut dom, css)
}

/// A div of class `class`.
fn div_class(class: &'static str) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class(class.into())].into())
}

/// A window that laid `page` out under `first` and then, as the shell rebuilds a page whose
/// stylesheet changed (the CSS diff first), under `second`: the second pass is an INCREMENTAL
/// one, it lays out again only what the changed rules reach.
fn restyled(page: fn() -> Dom, first: &str, second: &str) -> LayoutWindow {
    let mut lw = window();
    lay_out_styled(&mut lw, styled_page(page(), first));
    let mut next = styled_page(page(), second);
    let _pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now());
    lay_out_styled(&mut lw, next);
    lw
}

/// A window that laid `page` out under `css` once, from nothing.
fn fresh(page: fn() -> Dom, css: &str) -> LayoutWindow {
    let mut lw = window();
    lay_out_styled(&mut lw, styled_page(page(), css));
    lw
}

/// The boxes of the DOM nodes `nodes` stand where a fresh layout of the same page puts them.
fn assert_as_in_a_fresh_layout(incremental: &LayoutWindow, fresh: &LayoutWindow, nodes: &[usize]) {
    for &node in nodes {
        let (was, is) = (position_of(fresh, node), position_of(incremental, node));
        assert!(
            (was.x - is.x).abs() < 0.01 && (was.y - is.y).abs() < 0.01,
            "DOM node {node} stands at ({}, {}) after the incremental pass, a fresh layout of \
             the same page puts it at ({}, {})",
            is.x,
            is.y,
            was.x,
            was.y
        );
    }
}

/// `body(0) > .g(1) > [.a(2) > .x(3), .s(4)]`.
fn two_blocks_in_a_box() -> Dom {
    Dom::create_body().with_child(
        div_class("g")
            .with_child(div_class("a").with_child(div_class("x")))
            .with_child(div_class("s")),
    )
}

/// `body(0) > .g(1) > [.b(2), .a(3) > .x(4), .s(5)]`.
fn three_blocks_in_a_box() -> Dom {
    Dom::create_body().with_child(
        div_class("g")
            .with_child(div_class("b"))
            .with_child(div_class("a").with_child(div_class("x")))
            .with_child(div_class("s")),
    )
}

/// A3-solver-29. `.x` gets another WIDTH: its parent `.a` is laid out again and keeps its
/// size, so nothing may move. `cache::reposition_block_flow_siblings` stacked `.a`'s siblings
/// again from the parent's position + padding, without its BORDER: `.s` moved 5px to the left
/// (fresh: `.g`'s content box starts at (5, 5), `.a` is 20 high, `.s` stands at (5, 25); it
/// was put at (0, 25)).
#[test]
fn a_clean_sibling_keeps_its_place_in_a_bordered_box_when_its_neighbour_is_laid_out_again() {
    let base = "body { margin: 0; } .g { border: 5px solid black; width: 300px; } \
                .x { height: 20px; } .s { height: 10px; }";
    let (first, second) = (
        format!("{base} .x {{ width: 100px; }}"),
        format!("{base} .x {{ width: 150px; }}"),
    );
    let incremental = restyled(two_blocks_in_a_box, &first, &second);
    assert_as_in_a_fresh_layout(&incremental, &fresh(two_blocks_in_a_box, &second), &[2, 3, 4]);
}

/// A3-solver-29. The same in a PADDED box with margins that collapse between the two blocks
/// (`.a`'s bottom 10 and `.s`'s top 10 are one gap of 10): fresh, `.a` stands at (5, 5), 20
/// high, and `.s` at (5, 35). The stacking summed the two margins and put `.s` at (5, 45).
#[test]
fn a_clean_sibling_keeps_its_collapsed_margin_when_its_neighbour_is_laid_out_again() {
    let base = "body { margin: 0; } .g { padding: 5px; width: 300px; } .x { height: 20px; } \
                .a { margin-bottom: 10px; } .s { margin-top: 10px; height: 10px; }";
    let (first, second) = (
        format!("{base} .x {{ width: 100px; }}"),
        format!("{base} .x {{ width: 150px; }}"),
    );
    let incremental = restyled(two_blocks_in_a_box, &first, &second);
    assert_as_in_a_fresh_layout(&incremental, &fresh(two_blocks_in_a_box, &second), &[2, 3, 4]);
}

/// A3-solver-29. Three siblings in a bordered box of a definite height, and the middle one
/// GROWS (`.x` from 20 to 50 high): the one before it stays, the one after it moves down by
/// 30 and keeps its x. Fresh under the second sheet: `.b` (7 high, margin-bottom 4) at (5, 5),
/// `.a` at (5, 16), 50 high, `.s` (margin-top 6) at (5, 72). The stacking put `.b` at (0, 0)
/// and `.s` at (0, 72).
#[test]
fn siblings_of_a_block_that_grew_in_a_bordered_box_stand_as_in_a_fresh_layout() {
    let base = "body { margin: 0; } .g { border: 5px solid black; width: 300px; height: 200px; } \
                .b { height: 7px; margin-bottom: 4px; } .s { height: 10px; margin-top: 6px; }";
    let (first, second) = (
        format!("{base} .x {{ height: 20px; }}"),
        format!("{base} .x {{ height: 50px; }}"),
    );
    let incremental = restyled(three_blocks_in_a_box, &first, &second);
    assert_as_in_a_fresh_layout(
        &incremental,
        &fresh(three_blocks_in_a_box, &second),
        &[2, 3, 4, 5],
    );
}

/// `body(0) > k x [div(1 + 2 i) > div(2 + 2 i)]`: `k` rows, each a block with one block in flow
/// (5 px high) - `k` blocks whose first pass sizes a child.
fn rows_of_nested_blocks(k: usize) -> Dom {
    let mut body = Dom::create_body().with_css("margin: 0px;");
    for _ in 0..k {
        body = body
            .with_child(Dom::create_div().with_child(Dom::create_div().with_css("height: 5px;")));
    }
    body
}

/// A3-solver-15. The first pass of `fc::layout_bfc` made a new position vector for every block
/// with a child in flow, and writing the position of a box at tree index i fills a vector up
/// to i: blocks x tree length / 2 positions per pass. The vectors are lent by the pass
/// (`PassMemo::bfc`) and given back with the written entries unset.
///
/// Mutation: `Vec::new()` per block (no pool) is not counted as "made", but
/// `lend` making a new vector on every call makes one per block and fails the bound; a
/// `take_back` that unsets every entry of the vector grows 16x in `unset`.
#[test]
fn the_first_pass_of_a_block_borrows_its_position_vector_from_the_pass() {
    use crate::solver3::fc::{SCRATCH_ENTRIES_LEFT_SET, SCRATCH_ENTRIES_UNSET, SCRATCH_VECTORS_MADE};
    let counts = |k: usize| {
        let mut lw = window();
        SCRATCH_VECTORS_MADE.with(|n| n.set(0));
        SCRATCH_ENTRIES_UNSET.with(|n| n.set(0));
        SCRATCH_ENTRIES_LEFT_SET.with(|n| n.set(0));
        lay_out(&mut lw, rows_of_nested_blocks(k));
        (
            SCRATCH_VECTORS_MADE.with(std::cell::Cell::get),
            SCRATCH_ENTRIES_UNSET.with(std::cell::Cell::get),
            SCRATCH_ENTRIES_LEFT_SET.with(std::cell::Cell::get),
        )
    };
    let ((made_small, unset_small, left_small), (made_large, unset_large, left_large)) =
        (counts(200), counts(800));
    assert!(
        unset_small >= 200,
        "harness: the first passes of the 200 rows gave their vectors back ({unset_small})"
    );
    assert!(
        made_small <= 8 && made_large <= 8,
        "a page of rows two blocks deep made {made_small} / {made_large} position vectors: one \
         per nesting level is enough"
    );
    assert!(
        unset_large < unset_small * 6,
        "four times the rows unset {unset_large} entries for {unset_small}: more than linear"
    );
    assert_eq!(
        (left_small, left_large),
        (0, 0),
        "a vector given back still held positions: the next block would take them for its own"
    );
}

/// A3-solver-15, the result, on pages whose blocks hold every kind of child the first pass
/// meets (blocks in flow, absolutely positioned boxes with and without insets, a flex row, an
/// inline-block with a block inside): no vector comes back with a position left in it, and the
/// rows stand below each other.
#[test]
fn position_vectors_come_back_empty_and_the_rows_stand_where_they_did() {
    use crate::solver3::fc::SCRATCH_ENTRIES_LEFT_SET;
    let mut lw = window();
    lay_out(&mut lw, rows_of_nested_blocks(40));
    for i in [0_usize, 1, 17, 39] {
        // Row i is DOM node 1 + 2 i, 5 px high like the block in it.
        #[allow(clippy::cast_precision_loss)] // small pixel counts
        let y = (5 * i) as f32;
        let (row, inner) = (position_of(&lw, 1 + 2 * i), position_of(&lw, 2 + 2 * i));
        assert!(
            row.x.abs() < 0.01 && (row.y - y).abs() < 0.01 && (inner.y - y).abs() < 0.01,
            "row {i} stands at ({}, {}), its block at y = {}: both belong at (0, {y})",
            row.x,
            row.y,
            inner.y
        );
    }

    let mixed = || {
        let mut body = Dom::create_body().with_css("margin: 0px;");
        for i in 0..12 {
            body = body.with_child(
                Dom::create_div()
                    .with_css("position: relative;")
                    .with_child(Dom::create_div().with_css("height: 4px;"))
                    .with_child(
                        Dom::create_div()
                            .with_css("position: absolute; left: 3px; top: 1px; width: 5px;")
                            .with_child(Dom::create_div().with_css("height: 2px;")),
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css("position: absolute; width: 5px;")
                            .with_child(Dom::create_div().with_css("height: 2px;")),
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css("display: flex; flex-direction: row;")
                            .with_child(
                                Dom::create_div()
                                    .with_css("width: 20px;")
                                    .with_child(Dom::create_div().with_css("height: 3px;")),
                            ),
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css("display: inline-block; width: 30px;")
                            .with_child(Dom::create_div().with_css(&format!("height: {}px;", 2 + i))),
                    ),
            );
        }
        body
    };
    SCRATCH_ENTRIES_LEFT_SET.with(|n| n.set(0));
    let mut lw = window();
    lay_out(&mut lw, mixed());
    // The same page again: the passes that reuse the first one's tree give theirs back too.
    lay_out(&mut lw, mixed());
    assert_eq!(
        SCRATCH_ENTRIES_LEFT_SET.with(std::cell::Cell::get),
        0,
        "a position vector was given back with a position of a box outside the sized subtrees"
    );
}

/// A3-solver-20. `layout_document` made two contexts, one for the reconcile and one for the
/// layout, each with a style memo of its own: the style of every text node under a restyled box
/// (on a first layout, and for every label of a view rendered again: all of them), which the
/// reconcile resolves to compare it with the one the text was laid out with, was resolved a
/// second time by the layout. The layout's context takes the reconcile's memo over.
///
/// Mutation: `style_cache: Default::default()` for the layout's context rebuilds one style per
/// label.
#[test]
fn the_layout_takes_over_the_styles_the_reconcile_resolved() {
    use crate::solver3::getters::{STYLE_BUILT_NODES, STYLE_REBUILDS};
    let mut lw = window();
    STYLE_BUILT_NODES.with(|built| built.borrow_mut().clear());
    STYLE_REBUILDS.with(|n| n.set(0));
    lay_out(&mut lw, layer_of_absolute_spans(200));
    let built = STYLE_BUILT_NODES.with(|built| built.borrow().len());
    let rebuilt = STYLE_REBUILDS.with(std::cell::Cell::get);
    assert!(
        built >= 200,
        "harness: the text styles of the 200 labels were resolved ({built} nodes)"
    );
    assert!(
        rebuilt * 4 <= built,
        "one layout resolved the style of {built} nodes and {rebuilt} of them a second time"
    );
}

/// `body(0) > div(1) > k x [span(2 + 2 i) > text]`: a map's label layer with every label
/// absolutely positioned at `(10 * (i % 50) + dx, 9 * (i / 50) + dy)` and turned by
/// `turn + i % 7` degrees - the same labels, moved and turned, for every `(dx, dy, turn)`.
fn labels_moved(k: usize, dx: usize, dy: usize, turn: usize) -> Dom {
    let mut layer = Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
    for i in 0..k {
        layer = layer.with_child(Dom::create_span_with_text(format!("label {i}")).with_css(
            &format!(
                "position: absolute; left: {}px; top: {}px; transform: rotate({}deg);",
                (i % 50) * 10 + dx,
                (i / 50) * 9 + dy,
                turn + i % 7
            ),
        ));
    }
    Dom::create_body().with_css("margin: 0px;").with_child(layer)
}

/// A3-solver-41 (the solver's half of A3-window-08). A view rendered again in which only the
/// labels' positions and rotations changed: every label keeps its box (no box is built fresh,
/// no intrinsic size is measured again but the layer's and its ancestors'), and stands where a
/// fresh layout of the moved page puts it. Before, the changed inline style made every label a
/// fresh box: its text measured twice and laid out again.
///
/// Mutation: without the inset arm in `cache::reconcile_node` every label is fresh
/// (`last_reconcile_fresh` = 200) and `INSET_MOVES` stays 0.
#[test]
fn labels_that_only_moved_and_turned_keep_their_boxes_and_stand_as_in_a_fresh_layout() {
    use crate::solver3::cache::INSET_MOVES;
    let k = 200;
    let mut lw = window();
    lay_out(&mut lw, labels_moved(k, 0, 0, 5));
    INSET_MOVES.with(|n| n.set(0));
    lay_out(&mut lw, labels_moved(k, 7, 3, 25));
    let moved = INSET_MOVES.with(std::cell::Cell::get);
    assert!(moved >= k, "harness: the {k} labels were seen as moved ({moved})");
    assert_eq!(
        lw.layout_cache.last_reconcile_fresh, 0,
        "a label whose insets and rotation alone changed was built fresh"
    );
    assert!(
        lw.layout_cache.last_intrinsic_dirty <= 4,
        "{} boxes were sized anew for labels that only moved: more than the layer and its \
         ancestors",
        lw.layout_cache.last_intrinsic_dirty
    );
    let mut fresh = window();
    lay_out(&mut fresh, labels_moved(k, 7, 3, 25));
    // Labels (2 + 2 i) and their texts (3 + 2 i): the first, two in the middle, the last.
    assert_as_in_a_fresh_layout(&lw, &fresh, &[2, 3, 100, 101, 102, 2 * k, 2 * k + 1]);
}

/// A3-solver-41, the product path: the shell hands a rebuilt page to
/// `LayoutWindow::begin_reconciliation` first, whose CSS diff reports the moved insets as css
/// dirt of scope `Full` (`CssPropertyType::relayout_scope`), and a node the css diff names is
/// built fresh whatever its fingerprint says (`ReconciliationResult::css_relayout`). The
/// solver cannot tell "only the insets moved" from that entry; until the css diff says so (see
/// the notes: For area A3-window), the labels are fresh boxes on this path.
#[test]
#[ignore = "needs A3-window: begin_reconciliation must not stage css dirt of scope Full for an \
            out-of-flow box whose inset values alone moved"]
fn labels_that_only_moved_keep_their_boxes_when_the_shell_rebuilds_the_page() {
    let k = 200;
    let mut lw = window();
    lay_out_styled(&mut lw, StyledDom::create_from_dom(labels_moved(k, 0, 0, 5)));
    let mut next = StyledDom::create_from_dom(labels_moved(k, 7, 3, 25));
    let _pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now());
    lay_out_styled(&mut lw, next);
    assert_eq!(
        lw.layout_cache.last_reconcile_fresh, 0,
        "a label whose insets and rotation alone changed was built fresh"
    );
    let mut fresh = window();
    lay_out(&mut fresh, labels_moved(k, 7, 3, 25));
    assert_as_in_a_fresh_layout(&lw, &fresh, &[2, 3, 100, 101, 102, 2 * k, 2 * k + 1]);
}

/// A3-solver-42 (A8-13). Every inline formatting context asked the font manager for the fonts
/// it holds (`FontManager::get_loaded_fonts`: two mutex locks and a fold over every loaded
/// face) when its lines were laid out. A pass asks once and keeps the answer
/// (`LayoutContext::loaded_fonts`).
///
/// Mutation: `loaded_fonts` asking the manager on every call asks once per label.
#[test]
fn a_pass_asks_the_font_manager_for_its_fonts_once() {
    use crate::solver3::LOADED_FONTS_ASKS;
    let mut lw = window();
    LOADED_FONTS_ASKS.with(|n| n.set(0));
    lay_out(&mut lw, layer_of_absolute_spans(200));
    let asks = LOADED_FONTS_ASKS.with(std::cell::Cell::get);
    let nodes = lw.layout_results[&DomId::ROOT_ID].layout_tree.nodes.len();
    assert!(nodes >= 400, "harness: the 200 labels and their texts have boxes ({nodes})");
    assert!(
        (1..=4).contains(&asks),
        "one layout of 200 labels asked the font manager for its fonts {asks} times"
    );
}

/// A3-solver-43. `positioning::adjust_relative_positions` ADDS a relatively positioned box's
/// offset to the position the box has, for every such box of the tree, on every pass. An
/// incremental pass computes positions only below its layout roots and keeps all others from
/// the pass before - with the offset in them already: a relative box beside the re-laid
/// subtree moved by its offset again on every such pass. `.x` gets another width (its parent
/// `.a` is laid out again); `.r` (`top: 10px; left: 4px`) stands at (4, 10) in a fresh layout
/// and was put at (8, 20).
///
/// `body(0) > [.r(1), .a(2) > .x(3)]`.
#[test]
fn a_relatively_positioned_box_beside_a_re_laid_one_is_not_shifted_again() {
    let page = || {
        Dom::create_body()
            .with_child(div_class("r"))
            .with_child(div_class("a").with_child(div_class("x")))
    };
    let base = "body { margin: 0; } .r { position: relative; top: 10px; left: 4px; height: 20px; } \
                .x { height: 20px; }";
    let (first, second) = (
        format!("{base} .x {{ width: 100px; }}"),
        format!("{base} .x {{ width: 150px; }}"),
    );
    let incremental = restyled(page, &first, &second);
    assert_as_in_a_fresh_layout(&incremental, &fresh(page, &second), &[1, 2, 3]);
}

/// A3-solver-43. The re-laid box INSIDE a relatively positioned one: `.a` is laid out where it
/// stood, which is 10 below the flow already, and its content from there. `.r` and what it
/// holds were shifted by 10 again (fresh: all three at y = 10; they were put at 20).
///
/// `body(0) > .r(1) > .a(2) > .x(3)`.
#[test]
fn a_relatively_positioned_box_around_a_re_laid_one_is_not_shifted_again() {
    let page = || {
        Dom::create_body()
            .with_child(div_class("r").with_child(div_class("a").with_child(div_class("x"))))
    };
    let base = "body { margin: 0; } .r { position: relative; top: 10px; } .x { height: 20px; }";
    let (first, second) = (
        format!("{base} .x {{ width: 100px; }}"),
        format!("{base} .x {{ width: 150px; }}"),
    );
    let incremental = restyled(page, &first, &second);
    assert_as_in_a_fresh_layout(&incremental, &fresh(page, &second), &[1, 2, 3]);
}

/// A3-solver-43, the other side: a relatively positioned box BELOW the re-laid one gets its
/// position from the flow in that pass and is shifted, once (`.q`, `top: 7px`, under `.x`'s
/// 20: y = 27 fresh and incremental).
///
/// `body(0) > .a(1) > [.x(2), .q(3)]`.
#[test]
fn a_relatively_positioned_box_inside_a_re_laid_one_is_shifted_once() {
    let page = || {
        Dom::create_body()
            .with_child(div_class("a").with_child(div_class("x")).with_child(div_class("q")))
    };
    let base = "body { margin: 0; } .x { height: 20px; } \
                .q { position: relative; top: 7px; height: 5px; }";
    let (first, second) = (
        format!("{base} .x {{ width: 100px; }}"),
        format!("{base} .x {{ width: 150px; }}"),
    );
    let incremental = restyled(page, &first, &second);
    assert_as_in_a_fresh_layout(&incremental, &fresh(page, &second), &[1, 2, 3]);
}

/// `page` (a body) with one more child after everything else: a block holding `text` - what a
/// pass can change without touching any box the page had.
fn with_a_paragraph(page: Dom, text: &str) -> Dom {
    page.with_child(Dom::create_div_with_text(text.to_string()))
}

/// The interiors `positioning::position_out_of_flow_elements` laid out and the ones it read off
/// their last run, on this thread since the last call.
fn abs_interiors_since_last_asked() -> (usize, usize) {
    use crate::solver3::positioning::{ABS_INTERIORS_LAID_OUT, ABS_INTERIORS_REUSED};
    (
        ABS_INTERIORS_LAID_OUT.with(|n| n.replace(0)),
        ABS_INTERIORS_REUSED.with(|n| n.replace(0)),
    )
}

/// A3-solver-14. The positioning pass solves every absolutely positioned box in every pass and
/// laid the interior of each out again, whatever the pass was for: a paragraph beside a layer
/// of 200 labels changed its text, and 200 label interiors were laid out. A box whose subtree
/// nothing touched and whose size came out the same keeps its interior (`cache::AbsInterior`).
/// In this crate's tests a kept interior is laid out all the same and compared with what was
/// kept (`positioning::checks_interior_reuse`), so a label kept wrongly panics here.
///
/// Mutation: the store in `position_out_of_flow_elements` writing `None` lays out 200.
#[test]
fn a_pass_for_a_paragraph_lays_out_no_interior_of_the_labels_beside_it() {
    let k = 200;
    let mut lw = window();
    lay_out(&mut lw, with_a_paragraph(layer_of_absolute_blocks(k), "one"));
    let (first, _) = abs_interiors_since_last_asked();
    assert!(first >= k, "harness: the first pass laid out the {k} labels' interiors ({first})");
    lay_out(&mut lw, with_a_paragraph(layer_of_absolute_blocks(k), "another, longer text"));
    let (laid_out, kept) = abs_interiors_since_last_asked();
    assert_eq!(
        laid_out, 0,
        "a pass that changed one paragraph laid out {laid_out} interiors of labels beside it"
    );
    assert!(kept >= k, "harness: the {k} labels were solved again ({kept} kept)");
    let mut fresh = window();
    lay_out(&mut fresh, with_a_paragraph(layer_of_absolute_blocks(k), "another, longer text"));
    assert_as_in_a_fresh_layout(&lw, &fresh, &[2, 3, 100, 101, 2 * k, 2 * k + 1]);
}

/// A3-solver-14. One label of a layer gets another text: the layer is laid out again, which
/// visits every label from the flow (`cache::process_out_of_flow_children`). A clean label's
/// visit is a cache hit that writes nothing into the tree, so its interior is still the one the
/// positioning pass left, and only the changed label's is laid out.
///
/// Mutation: `calculate_layout_for_subtree_fragment` dropping `abs_interior` on every visit
/// (`if true` for the comparison of `tree_writes`) lays out all 200.
#[test]
fn a_label_that_changed_is_the_only_one_whose_interior_is_laid_out_again() {
    let k = 200;
    let page = |changed: Option<usize>| {
        let mut layer =
            Dom::create_div().with_css("position: relative; width: 600px; height: 400px;");
        for i in 0..k {
            let text = if changed == Some(i) {
                "renamed".to_string()
            } else {
                format!("label {i}")
            };
            layer = layer.with_child(Dom::create_div_with_text(text).with_css(&format!(
                "position: absolute; left: {}px; top: {}px;",
                (i % 50) * 10,
                (i / 50) * 9
            )));
        }
        Dom::create_body().with_css("margin: 0px;").with_child(layer)
    };
    let mut lw = window();
    lay_out(&mut lw, page(None));
    let _ = abs_interiors_since_last_asked();
    lay_out(&mut lw, page(Some(70)));
    let (laid_out, kept) = abs_interiors_since_last_asked();
    assert!(
        (1..=2).contains(&laid_out),
        "one label changed its text and {laid_out} label interiors were laid out"
    );
    assert!(kept >= k - 2, "harness: the other labels were solved again ({kept} kept)");
    let mut fresh = window();
    lay_out(&mut fresh, page(Some(70)));
    // The changed label (2 + 2 * 70) and its text, and labels around it.
    assert_as_in_a_fresh_layout(&lw, &fresh, &[2, 3, 140, 142, 143, 144, 2 * k]);
}

/// A3-solver-14 with A3-solver-41: a view rendered again in which the labels only moved. Each
/// label keeps its box (41) and is placed again by the positioning pass, which solves its box
/// to the size it had and so keeps its interior: no text is laid out for a label that moved.
///
/// Mutation: `AbsInterior::is_for` comparing the box's position too (or the store writing
/// `None`) lays out all 200.
#[test]
fn labels_that_only_moved_are_placed_again_without_laying_out_their_interiors() {
    let k = 200;
    let mut lw = window();
    lay_out(&mut lw, labels_moved(k, 0, 0, 5));
    let _ = abs_interiors_since_last_asked();
    lay_out(&mut lw, labels_moved(k, 7, 3, 25));
    assert_eq!(
        lw.layout_cache.last_reconcile_fresh, 0,
        "harness: the labels that only moved kept their boxes"
    );
    let (laid_out, kept) = abs_interiors_since_last_asked();
    assert_eq!(laid_out, 0, "{laid_out} interiors of labels that only moved were laid out");
    assert!(kept >= k, "harness: the {k} labels were solved again ({kept} kept)");
    let mut fresh = window();
    lay_out(&mut fresh, labels_moved(k, 7, 3, 25));
    assert_as_in_a_fresh_layout(&lw, &fresh, &[2, 3, 100, 101, 102, 2 * k, 2 * k + 1]);
}

/// A3-solver-14, a node graph in small: absolutely positioned boxes that each hold a flex row.
/// The interior of such a box is a block with a flex container in it - a taffy bridge per box
/// per pass, for a pass that changed a paragraph beside the canvas.
///
/// Mutation: as for the labels (the store writing `None`).
#[test]
fn a_pass_for_a_paragraph_lays_out_no_interior_of_the_flex_boxes_beside_it() {
    use crate::solver3::taffy_bridge::BRIDGES_MADE;
    let k = 40;
    let mut lw = window();
    lay_out(&mut lw, with_a_paragraph(canvas_of_flex_nodes(k, 3), "one"));
    let _ = abs_interiors_since_last_asked();
    BRIDGES_MADE.with(|n| n.set(0));
    lay_out(&mut lw, with_a_paragraph(canvas_of_flex_nodes(k, 3), "another, longer text"));
    let (laid_out, kept) = abs_interiors_since_last_asked();
    assert_eq!(
        laid_out, 0,
        "a pass that changed one paragraph laid out {laid_out} interiors of boxes beside it"
    );
    assert!(kept >= k, "harness: the {k} boxes were solved again ({kept} kept)");
    // A kept interior is laid out once more by the check, and a row whose box is kept is a
    // cache hit of its block: no bridge is made for it.
    let bridges = BRIDGES_MADE.with(std::cell::Cell::get);
    assert!(
        bridges * 4 <= k,
        "{bridges} taffy bridges were made for {k} flex rows that nothing touched"
    );
    let mut fresh = window();
    lay_out(&mut fresh, with_a_paragraph(canvas_of_flex_nodes(k, 3), "another, longer text"));
    // The first box (2), its row (3) and the row's first item (4); the second box (10).
    assert_as_in_a_fresh_layout(&lw, &fresh, &[2, 3, 4, 10, 11]);
}

/// A3-solver-50 (handed over by A8). The intrinsic measure of an inline formatting context and
/// its layout each shaped its content: the second time a hit in every cache of the text
/// layout, which still hashes the content, looks its stages up and expands every cached
/// cluster into a new vector. The layout takes what the measure of the same root shaped, when
/// the content it collected is the measure's (`sizing::MeasuredParagraphs`).
///
/// If the second assertion fails, the content the layout collects for a text label is not
/// equal to the content its measure collected (`sizing::collect_inline_content_for_sizing`
/// against `fc::collect_and_measure_inline_content`): nothing is taken, the layout shapes as
/// before, and the two collections are to be made one.
///
/// Mutation: `MeasuredParagraphs::keep` keeping nothing (or `take` answering `None`) takes 0.
#[test]
fn the_layout_of_a_label_takes_what_its_measure_shaped() {
    use crate::solver3::sizing::{MEASURED_PARAGRAPHS_KEPT, MEASURED_PARAGRAPHS_TAKEN};
    let k = 200;
    let mut lw = window();
    MEASURED_PARAGRAPHS_KEPT.with(|n| n.set(0));
    MEASURED_PARAGRAPHS_TAKEN.with(|n| n.set(0));
    lay_out(&mut lw, layer_of_absolute_blocks(k));
    let kept = MEASURED_PARAGRAPHS_KEPT.with(std::cell::Cell::get);
    let taken = MEASURED_PARAGRAPHS_TAKEN.with(std::cell::Cell::get);
    assert!(kept >= k, "harness: the measure of the {k} labels kept what it shaped ({kept})");
    assert!(
        taken >= k,
        "the layout of {taken} of {k} labels took what the label's measure had shaped"
    );
    // The labels stand at their insets (label i at 10 * (i % 50), 9 * (i / 50)).
    let label = position_of(&lw, 2 + 2 * 57);
    assert!(
        (label.x - 70.0).abs() < 0.01 && (label.y - 9.0).abs() < 0.01,
        "label 57 stands at ({}, {}), not at its insets (70, 9)",
        label.x,
        label.y
    );
}

/// `body(0) > .p(1) > .r(2) > .c(3)`: an absolutely positioned box at (50, 40) holding a
/// relatively positioned one (`left: 7px; top: 5px`) with a child.
fn a_relative_box_in_an_absolute_one() -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css("position: absolute; left: 50px; top: 40px; width: 200px; height: 100px;")
            .with_child(
                Dom::create_div()
                    .with_css("position: relative; left: 7px; top: 5px; width: 20px; height: 10px;")
                    .with_child(Dom::create_div().with_css("width: 5px; height: 5px;")),
            ),
    )
}

/// A3-solver-45. The relative shifts are applied to the positions the flow computed
/// (`adjust_relative_positions`), and after that the positioning pass places every absolutely
/// positioned box and derives everything below it from the flow again
/// (`cache::position_bfc_child_descendants`): a relatively positioned box inside an absolutely
/// positioned one lost its shift, and its content with it.
///
/// Hand-computed: the box at (50, 40), no border, no padding; `.r` is its first child at the
/// content origin (50, 40), shifted by (7, 5): (57, 45); `.c` goes with it: (57, 45). Before:
/// both at (50, 40).
///
/// Mutation: without `shift_relative_descendants` after the box is placed, `.r` stands at
/// (50, 40).
#[test]
fn a_relatively_positioned_box_inside_an_absolute_one_stands_at_its_offset() {
    let mut lw = window();
    lay_out(&mut lw, a_relative_box_in_an_absolute_one());
    let (p, r, c) = (position_of(&lw, 1), position_of(&lw, 2), position_of(&lw, 3));
    assert!(
        (p.x - 50.0).abs() < 0.01 && (p.y - 40.0).abs() < 0.01,
        "harness: the absolute box stands at its insets, ({}, {})",
        p.x,
        p.y
    );
    assert!(
        (r.x - 57.0).abs() < 0.01 && (r.y - 45.0).abs() < 0.01,
        "the relatively positioned box stands at ({}, {}), not at (57, 45)",
        r.x,
        r.y
    );
    assert!(
        (c.x - 57.0).abs() < 0.01 && (c.y - 45.0).abs() < 0.01,
        "its child stands at ({}, {}), not at (57, 45)",
        c.x,
        c.y
    );
}

/// A3-solver-45, a second pass over the same page: the shift is there once, not twice (the
/// positions are derived from the flow and shifted in every pass).
#[test]
fn a_relatively_positioned_box_inside_an_absolute_one_is_shifted_once_per_pass() {
    let mut lw = window();
    lay_out(&mut lw, with_a_paragraph(a_relative_box_in_an_absolute_one(), "one"));
    lay_out(&mut lw, with_a_paragraph(a_relative_box_in_an_absolute_one(), "another"));
    let r = position_of(&lw, 2);
    assert!(
        (r.x - 57.0).abs() < 0.01 && (r.y - 45.0).abs() < 0.01,
        "after a second pass the relatively positioned box stands at ({}, {}), not at (57, 45)",
        r.x,
        r.y
    );
}

/// A3-solver-46. The positioning pass reads a box's static position from its slot of the
/// position vector and writes the box's final position into the same slot. A pass that does not
/// derive that slot from the flow again (an incremental pass that lays out another subtree)
/// solves from last pass's FINAL position. For a box with all insets `auto` in an `rtl`
/// containing block the solve is `x = static.x - margin-right` (`right` is set to the static
/// position, CSS 2.2 10.3.7), so the box moved left by its right margin on every such pass.
///
/// `body(0) > [.c(1) > .p(2), .g(3) > .x(4)]`: `.x` gets another width; its parent `.g` has a
/// definite height, so `.x` alone is laid out again and `.c` with its box `.p` is not touched.
/// Whatever x a fresh layout gives `.p` (F), the incremental pass gave F - 10.
///
/// Mutation: the static position read from the slot in the `Rtl` arm (no `flow_static_position`)
/// puts `.p` 10 px further left than a fresh layout.
#[test]
fn a_box_at_its_static_position_in_an_rtl_block_does_not_drift_by_its_margin() {
    let page = || {
        Dom::create_body()
            .with_child(div_class("c").with_child(div_class("p")))
            .with_child(div_class("g").with_child(div_class("x")))
    };
    let base = "body { margin: 0; } \
                .c { direction: rtl; position: relative; width: 400px; height: 100px; } \
                .p { position: absolute; width: 50px; height: 20px; margin-right: 10px; } \
                .g { height: 50px; } .x { height: 20px; }";
    let (first, second) = (
        format!("{base} .x {{ width: 100px; }}"),
        format!("{base} .x {{ width: 150px; }}"),
    );
    let incremental = restyled(page, &first, &second);
    assert_as_in_a_fresh_layout(&incremental, &fresh(page, &second), &[1, 2, 3, 4]);
}

/// A3-solver-52. An out-of-flow child takes its static position from its parent's layout
/// (`cache::process_out_of_flow_children`), which passes over a child that has a position - the
/// test by which it tells the children it has just placed. A pass begins with the positions of
/// the pass before, so when a layout root is laid out again, its absolutely positioned children
/// all "have a position": they kept the place the positioning pass gave them last time as
/// their static position, whatever the root's new layout says.
///
/// `body(0) > .g(1) > .a(2) > [.x(3), .p(4)]`: `.a` goes from 100 to 200 px high (`.g` has a
/// definite height, so `.a` is the one layout root), `.x` is half of it, `.p` (all insets
/// `auto`) stands below `.x`: at y = 50 before, at y = 100 in a fresh layout of the second
/// stylesheet. The incremental pass left it at 50.
///
/// Mutation: without the emptied slots in `layout_document`'s root loop `.p` stays at y = 50.
#[test]
fn a_box_at_its_static_position_follows_the_flow_of_a_parent_laid_out_again() {
    let page = || {
        Dom::create_body().with_child(
            div_class("g").with_child(
                div_class("a")
                    .with_child(div_class("x"))
                    .with_child(div_class("p")),
            ),
        )
    };
    let base = "body { margin: 0; } .g { height: 400px; } \
                .a { position: relative; width: 300px; } .x { height: 50%; } \
                .p { position: absolute; width: 10px; height: 10px; }";
    let (first, second) = (
        format!("{base} .a {{ height: 100px; }}"),
        format!("{base} .a {{ height: 200px; }}"),
    );
    let incremental = restyled(page, &first, &second);
    let fresh = fresh(page, &second);
    let p = position_of(&fresh, 4);
    assert!(
        (p.y - 100.0).abs() < 0.01,
        "harness: a fresh layout puts the box below the 100 px block, at y = {}",
        p.y
    );
    assert_as_in_a_fresh_layout(&incremental, &fresh, &[2, 3, 4]);
}

/// What the layout kept of the sticky box of DOM node `node` for later scrolls: where normal
/// flow put it (y) and the shift it applied (y).
fn sticky_of(lw: &LayoutWindow, node: usize) -> (f32, f32) {
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let index = *lr
        .layout_tree
        .dom_to_layout
        .get(&NodeId::new(node))
        .and_then(|v| v.first())
        .expect("the node is laid out");
    let kept = lr
        .layout_tree
        .warm(index)
        .and_then(|w| w.sticky.as_deref())
        .expect("the box is sticky");
    (kept.static_rect.origin.y, kept.baked.y)
}

/// A3-solver-44. `positioning::adjust_sticky_positions` takes a sticky box's position for where
/// normal flow put it, shifts it, and keeps both for later scrolls (`StickyConstraint`: the
/// static place and the shift baked in). An incremental pass that lays out another subtree
/// leaves the box's position as the pass before SHIFTED it, and the box was then taken to stand
/// there by the flow: the kept static place became the shifted one and the baked shift 0, so
/// the next scroll moved the box from the wrong place.
///
/// `body(0) > [.c(1) > .s(2), .g(3) > .x(4)]`: `.s` is `sticky; top: 50px` at the top of the
/// 300 px block `.c`, so the unscrolled viewport holds it 50 below its place: static y = 0,
/// shift 50. `.x` gets another width; `.g` has a definite height, so nothing else is laid out.
///
/// Mutation: `static_pos` taken from the slot for a box the pass did not place keeps
/// (50, 0) instead of (0, 50).
#[test]
fn a_sticky_box_beside_a_re_laid_one_keeps_its_static_place_for_later_scrolls() {
    let page = || {
        Dom::create_body()
            .with_child(div_class("c").with_child(div_class("s")))
            .with_child(div_class("g").with_child(div_class("x")))
    };
    let base = "body { margin: 0; } .c { height: 300px; } \
                .s { position: sticky; top: 50px; height: 20px; } \
                .g { height: 50px; } .x { height: 20px; }";
    let (first, second) = (
        format!("{base} .x {{ width: 100px; }}"),
        format!("{base} .x {{ width: 150px; }}"),
    );
    let fresh = fresh(page, &second);
    let (flow_y, shift_y) = sticky_of(&fresh, 2);
    assert!(
        flow_y.abs() < 0.01 && (shift_y - 50.0).abs() < 0.01,
        "harness: a fresh layout keeps the sticky box's place 0 and shift 50, not {flow_y} and \
         {shift_y}"
    );
    let incremental = restyled(page, &first, &second);
    assert_as_in_a_fresh_layout(&incremental, &fresh, &[2]);
    let (flow_y, shift_y) = sticky_of(&incremental, 2);
    assert!(
        flow_y.abs() < 0.01 && (shift_y - 50.0).abs() < 0.01,
        "after the incremental pass the sticky box's kept place is {flow_y} and its baked shift \
         {shift_y}: a fresh layout keeps 0 and 50"
    );
}

/// `body(0) > .layer(1) > 200 x [div.l(2 + 2 i) > text]`, label 70 of class `s` as well: a
/// layer of absolutely positioned labels, one of which a stylesheet can restyle.
fn layer_with_one_marked_label() -> Dom {
    use azul_core::dom::IdOrClass::Class;
    let mut layer = div_class("layer");
    for i in 0..200 {
        let classes = if i == 70 {
            vec![Class("l".into()), Class("s".into())]
        } else {
            vec![Class("l".into())]
        };
        layer = layer.with_child(
            Dom::create_div_with_text(format!("label {i}"))
                .with_ids_and_classes(classes.into())
                .with_css(&format!("left: {}px; top: {}px;", (i % 50) * 10, (i / 50) * 9)),
        );
    }
    Dom::create_body().with_child(layer)
}

/// A3-solver-30. A box whose restyle changes what lays out is laid out by its parent (the
/// css-dirty channel of `layout_document`), and a block that holds only inline content is lifted
/// to it as if it were on one of its lines (`cache::lift_to_slot_container`). For a label of a
/// map's layer that is the layer: one label marked - a wider padding - and the layer was laid
/// out again, the static position of each of its labels found among its children and every
/// label visited. An absolutely positioned box whose size or text alone changed moves no
/// sibling and nothing of its parent: it is laid out as its own root.
///
/// Counted: the lookups `cache::process_out_of_flow_children` makes for the static positions of
/// a block's children (`CHILD_LOOKUP_STEPS`); the layer makes one per label when it is laid
/// out, a label none.
///
/// Mutation: `restyled_out_of_flow_box_stands_alone` answering `false` lays the layer out
/// (more than 200 lookups).
#[test]
fn a_restyled_label_is_laid_out_without_its_layer() {
    use crate::solver3::cache::CHILD_LOOKUP_STEPS;
    let page: fn() -> Dom = layer_with_one_marked_label;
    let base = "body { margin: 0; } \
                .layer { position: relative; width: 600px; height: 400px; } \
                .l { position: absolute; }";
    let (first, second) = (
        format!("{base} .s {{ padding-left: 0px; }}"),
        format!("{base} .s {{ padding-left: 6px; }}"),
    );
    let mut lw = window();
    lay_out_styled(&mut lw, styled_page(page(), &first));
    let mut next = styled_page(page(), &second);
    let _pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut next, azul_core::task::Instant::now());
    CHILD_LOOKUP_STEPS.with(|n| n.set(0));
    lay_out_styled(&mut lw, next);
    let lookups = CHILD_LOOKUP_STEPS.with(std::cell::Cell::get);
    assert_eq!(
        lookups, 0,
        "one restyled label made its layer look {lookups} static positions up among its children"
    );
    // The marked label (2 + 2 * 70 = 142), its text and its neighbours stand as in a fresh
    // layout of the second stylesheet.
    assert_as_in_a_fresh_layout(&lw, &fresh(page, &second), &[2, 140, 141, 142, 143, 144, 400]);
}

/// The absolutely positioned boxes `positioning::position_out_of_flow_elements` solved and the
/// ones it placed from their last solve, on this thread since the last call.
fn abs_boxes_since_last_asked() -> (usize, usize) {
    use crate::solver3::positioning::{ABS_BOXES_PLACED_AS_KEPT, ABS_BOXES_SOLVED};
    (
        ABS_BOXES_SOLVED.with(|n| n.replace(0)),
        ABS_BOXES_PLACED_AS_KEPT.with(|n| n.replace(0)),
    )
}

/// A3-solver-14b. The positioning pass solved every absolutely positioned box in every pass:
/// containing block, used size, insets and the six rules, some forty style reads for a label,
/// 12,000 times for a paragraph that changed beside a map's layer. A box whose cache entry
/// stands and whose containing block, viewport and static position are what they were is
/// placed from its last solve (`cache::AbsPlacement`). In this crate's tests such a box is
/// solved all the same and the result compared, so a box placed wrongly panics here.
///
/// Mutation: the store of `placement` at the end of a box's solve writing `None` solves 200.
#[test]
fn a_pass_for_a_paragraph_solves_none_of_the_labels_beside_it() {
    let k = 200;
    let mut lw = window();
    lay_out(&mut lw, with_a_paragraph(layer_of_absolute_blocks(k), "one"));
    let (first, _) = abs_boxes_since_last_asked();
    assert!(first >= k, "harness: the first pass solved the {k} labels ({first})");
    lay_out(&mut lw, with_a_paragraph(layer_of_absolute_blocks(k), "another, longer text"));
    let (solved, kept) = abs_boxes_since_last_asked();
    assert_eq!(solved, 0, "a pass that changed one paragraph solved {solved} labels beside it");
    assert!(kept >= k, "harness: the {k} labels were placed again ({kept} from their last solve)");
    let mut fresh = window();
    lay_out(&mut fresh, with_a_paragraph(layer_of_absolute_blocks(k), "another, longer text"));
    assert_as_in_a_fresh_layout(&lw, &fresh, &[2, 3, 100, 101, 2 * k, 2 * k + 1]);
}

/// A3-solver-14b, the input that moves without clearing a box's cache entry: its insets
/// (A3-solver-41 keeps the box of a label that only moved). Such a box is solved again - from
/// its last solve it would stay where it was (in this crate's tests: panic, the check finds the
/// new place).
///
/// Mutation: without the loop over `paint_dirty` in `layout_document` the 200 moved labels are
/// placed from their last solve.
#[test]
fn labels_that_moved_are_solved_again() {
    let k = 200;
    let mut lw = window();
    lay_out(&mut lw, labels_moved(k, 0, 0, 5));
    let _ = abs_boxes_since_last_asked();
    lay_out(&mut lw, labels_moved(k, 7, 3, 25));
    let (solved, kept) = abs_boxes_since_last_asked();
    assert_eq!(kept, 0, "{kept} labels that moved were placed from their last solve");
    assert!(solved >= k, "harness: the {k} moved labels were solved ({solved})");
    // Label 57 (DOM node 2 + 2 * 57) at (10 * 7 + 7, 9 * 1 + 3).
    let label = position_of(&lw, 2 + 2 * 57);
    assert!(
        (label.x - 77.0).abs() < 0.01 && (label.y - 12.0).abs() < 0.01,
        "the moved label 57 stands at ({}, {}), not at its insets (77, 12)",
        label.x,
        label.y
    );
}

// The guards of the second round, one file per part of the solver (`a3_solver/*.rs`).
mod ifc;
mod sizing;
mod styles;
mod taffy;
mod widgets;
