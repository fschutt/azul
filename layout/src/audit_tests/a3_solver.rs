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
