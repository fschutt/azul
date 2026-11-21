//! A3-solver guards, part `sizing` (helpers: `super::{window, lay_out, lay_out_styled, position_of}`).

#[allow(unused_imports)]
use super::*;

/// A3-solver-19. A text has a layout node of its own, and so has the box around it. Under a
/// shrink-to-fit ancestor (an absolutely positioned label) the intrinsic pass measured the
/// text as an inline formatting context (`sizing::calculate_ifc_root_intrinsic_sizes`: collect,
/// white-space runs, the loaded fonts, shaping, the scan) and then the label around it, whose
/// content is that text and nothing else, in the same way again: two measures per label and
/// pass. The label takes the measure of its text.
///
/// The layer of `k` labels: one measure per text, none per label, one collection for the layer
/// (all its children are out of flow: nothing to measure).
///
/// Mutation: `reuse_last_ifc_measure` returning `None` measures `2 k + 1` contexts per pass.
#[test]
fn a_label_and_its_text_are_measured_once_per_intrinsic_pass() {
    use crate::solver3::sizing::IFC_MEASURES_PER_PASS;
    const LABELS: usize = 300;
    let mut lw = window();
    IFC_MEASURES_PER_PASS.with(|passes| passes.borrow_mut().clear());
    lay_out(&mut lw, layer_of_absolute_spans(LABELS));
    let passes = IFC_MEASURES_PER_PASS.with(|passes| passes.borrow().clone());
    let most = passes.iter().copied().max().unwrap_or(0);
    assert!(
        most >= LABELS,
        "harness: an intrinsic pass measured the text of each of the {LABELS} labels ({passes:?})"
    );
    assert!(
        most <= LABELS + 8,
        "an intrinsic pass over {LABELS} labels measured {most} inline formatting contexts \
         (per pass: {passes:?}): the label and its text are measured apart"
    );
}

/// A3-solver-19, the result: a label holds the intrinsic sizes of its text (it has no `width`,
/// `min-width` or `max-width` of its own to change them), whichever of the two was measured.
#[test]
fn a_label_holds_the_intrinsic_sizes_of_its_text() {
    let mut lw = window();
    lay_out(&mut lw, layer_of_absolute_spans(40));
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let sizes_of = |node: usize| {
        let index = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(node))
            .and_then(|v| v.first())
            .expect("the node is laid out");
        lr.layout_tree
            .warm(index)
            .and_then(|w| w.intrinsic_sizes)
            .expect("the node has intrinsic sizes")
    };
    for i in [0_usize, 9, 10, 39] {
        // span(2 + 2 i) > text(3 + 2 i).
        let (label, text) = (sizes_of(2 + 2 * i), sizes_of(3 + 2 * i));
        assert!(
            label.min_content_width.to_bits() == text.min_content_width.to_bits()
                && label.max_content_width.to_bits() == text.max_content_width.to_bits()
                && label.min_content_height.to_bits() == text.min_content_height.to_bits()
                && label.max_content_height.to_bits() == text.max_content_height.to_bits(),
            "label {i} holds {label:?}, its text {text:?}"
        );
    }
}

/// A3-solver-26. The intrinsic pass asks for every node it visits whether the node is dirty or
/// above a dirty one. It asked a `HashSet<usize>` it built for itself on every pass
/// (`sizing::compute_dirty_ancestor_closure`: a SipHash per member to build, one per visited
/// node to ask); it asks one flag per node (`sizing::DirtyAncestors`). The hashed set is still
/// built once per layout, by `layout_document`, which walks it to clear the taffy caches.
///
/// Mutation: `calculate_intrinsic_sizes` building `compute_dirty_ancestor_closure` again
/// builds one more per pass: more builds than passes.
#[test]
fn the_intrinsic_pass_builds_no_hash_set_of_the_dirty_nodes() {
    use crate::solver3::sizing::{DIRTY_CLOSURE_HASH_BUILDS, IFC_MEASURES_PER_PASS};
    let mut lw = window();
    DIRTY_CLOSURE_HASH_BUILDS.with(|n| n.set(0));
    IFC_MEASURES_PER_PASS.with(|passes| passes.borrow_mut().clear());
    lay_out(&mut lw, layer_of_absolute_spans(100));
    let builds = DIRTY_CLOSURE_HASH_BUILDS.with(std::cell::Cell::get);
    let passes = IFC_MEASURES_PER_PASS.with(|passes| passes.borrow().len());
    assert!(
        builds >= 1 && passes >= 1,
        "harness: the layout built the dirty closure ({builds}) and ran an intrinsic pass \
         ({passes})"
    );
    assert!(
        builds <= passes,
        "{passes} intrinsic passes of one layout came with {builds} hash sets of the dirty \
         nodes: the pass builds one of its own"
    );
}

/// A3-solver-32 [reveals]. An absolutely positioned box is out of the flow: it adds nothing to
/// the intrinsic sizes of its parent (CSS 2.2 10.3.5 measures a shrink-to-fit width from the
/// content in flow; css-sizing-3 5.1, CSS Position 3: an out-of-flow box does not affect the
/// size of its parent). `sizing::calculate_intrinsic_recursive` hands the parent zero sizes
/// for such a child, but `calculate_block_intrinsic_sizes` still adds the child's own margin,
/// border and padding to those zeros (`cross_extras`, `main_border_padding`, the margin chain) -
/// and so does `calculate_flex_intrinsic_sizes` (`extra_w`, `extra_h`).
///
/// `body > float > [block 100 x 20, absolutely positioned block]`: the float shrinks to fit its
/// one child in flow, 100px - with the positioned child bare (the harness: 100 today) and with
/// 150px of padding on it (by reading: max(100, 0 + 150) = 150 today).
#[test]
fn an_absolutely_positioned_child_adds_nothing_to_its_parents_intrinsic_width() {
    // body(0) > float(1) > [block(2), absolutely positioned block(3)].
    let width_of_the_float = |padding: u32| {
        let mut lw = window();
        lay_out(
            &mut lw,
            Dom::create_body().with_css("margin: 0px;").with_child(
                Dom::create_div()
                    .with_css("float: left;")
                    .with_child(Dom::create_div().with_css("width: 100px; height: 20px;"))
                    .with_child(Dom::create_div().with_css(&format!(
                        "position: absolute; width: 10px; height: 10px; padding-left: {padding}px;"
                    ))),
            ),
        );
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let index = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(1))
            .and_then(|v| v.first())
            .expect("the float is laid out");
        lr.layout_tree
            .get(index)
            .and_then(|n| n.used_size)
            .expect("the float has a size")
            .width
    };
    let bare = width_of_the_float(0);
    assert!(
        (bare - 100.0).abs() < 0.5,
        "harness: a float around a 100px block and a bare positioned box is {bare}px wide"
    );
    let padded = width_of_the_float(150);
    assert!(
        (padded - 100.0).abs() < 0.5,
        "the padding of an absolutely positioned child widened its shrink-to-fit parent to \
         {padded}px (the child in flow is 100px wide)"
    );
}

/// `body > div > n x [span > text]`: a paragraph of `n` inline boxes in flow, a word each.
fn paragraph_of_spans(n: usize) -> Dom {
    let mut paragraph = Dom::create_div().with_css("width: 600px;");
    for i in 0..n {
        paragraph = paragraph.with_child(Dom::create_span_with_text(format!("w{i} ")));
    }
    Dom::create_body().with_css("margin: 0px;").with_child(paragraph)
}

/// A3-solver-08. The collection of an inline box (`fc::collect_inline_span_recursive`) looked
/// its own layout box up among the layout children of the box it sits in with a scan from the
/// first one (`parent_children.iter().find(..)`) - the box its caller had just found - and
/// each of its element children among its own layout children with another: the i-th inline
/// box of a paragraph after i steps, s * s / 2 for a paragraph of s. The caller hands the box
/// over, and the element children are found in a list sorted by DOM node (`fc::BoxesByDom`).
///
/// One collection of the paragraph: n layout children listed once and a bisection per inline
/// box by the root's loop (A3-solver-07), nothing more by the boxes.
///
/// Mutation: the scan for its own box in `collect_inline_span_recursive` (counting each layout
/// child it passes) grows 16x.
#[test]
fn a_paragraph_of_inline_boxes_finds_the_box_of_each_without_a_scan() {
    use crate::solver3::fc::{IFC_CHILD_PROBES, IFC_CHILD_PROBES_MOST};
    let probes = |n: usize| {
        let mut lw = window();
        IFC_CHILD_PROBES.with(|c| c.set(0));
        IFC_CHILD_PROBES_MOST.with(|c| c.set(0));
        lay_out(&mut lw, paragraph_of_spans(n));
        IFC_CHILD_PROBES_MOST.with(std::cell::Cell::get)
    };
    let (small, large) = (probes(500), probes(2_000));
    assert!(
        small >= 500,
        "harness: one collection of the paragraph looked for the boxes of its 500 spans ({small})"
    );
    assert!(
        large < small * 6,
        "one collection of four times the inline boxes took {large} steps for {small}: more \
         than n log n"
    );
}

/// A3-solver-08, the result: the atomic inlines inside an inline box and inside an inline box
/// nested in it are each laid out as the box of their own DOM node, at the size their own CSS
/// gives them - whichever way their layout boxes were found.
#[test]
fn atomic_inlines_inside_nested_inline_boxes_keep_their_own_boxes() {
    let inline_block = |width: u32| {
        Dom::create_div().with_css(&format!(
            "display: inline-block; width: {width}px; height: 10px;"
        ))
    };
    // body(0) > div(1) > span(2) > [inline-block(3), span(4) > [inline-block(5)], inline-block(6)].
    let mut lw = window();
    lay_out(
        &mut lw,
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div().with_css("width: 600px;").with_child(
                Dom::create_span()
                    .with_child(inline_block(30))
                    .with_child(Dom::create_span().with_child(inline_block(40)))
                    .with_child(inline_block(50)),
            ),
        ),
    );
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    for (node, width) in [(3_usize, 30.0_f32), (5, 40.0), (6, 50.0)] {
        let index = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(node))
            .and_then(|v| v.first())
            .expect("the inline-block has a box");
        let size = lr
            .layout_tree
            .get(index)
            .and_then(|n| n.used_size)
            .expect("the inline-block is laid out");
        assert!(
            (size.width - width).abs() < 0.5 && (size.height - 10.0).abs() < 0.5,
            "the inline-block of DOM node {node} is {width} x 10 by its CSS, laid out {size:?}"
        );
    }
}

/// The intrinsic sizes the layout stored on the box of DOM node `node`.
fn intrinsic_sizes_of(
    lw: &LayoutWindow,
    node: usize,
) -> crate::solver3::geometry::IntrinsicSizes {
    let lr = &lw.layout_results[&DomId::ROOT_ID];
    let index = *lr
        .layout_tree
        .dom_to_layout
        .get(&NodeId::new(node))
        .and_then(|v| v.first())
        .expect("the node is laid out");
    lr.layout_tree
        .warm(index)
        .and_then(|w| w.intrinsic_sizes)
        .expect("the node has intrinsic sizes")
}

/// A3-solver-32, the block fold's other axis. An absolutely positioned child took a place in
/// the chain of its siblings' collapsing margins: as the first child it used up the "first
/// child" whose top margin does not count, its own vertical border and padding were stacked,
/// and its bottom margin was collapsed with the next sibling's top one.
///
/// `body > float > [absolute 10 x 10 with 30px padding above and below and a 5px bottom margin,
/// block 100 x 20 with a 10px top margin]`. The float's intrinsic sizes are its one child in
/// flow's: 100 wide, 20 high (a first child's top margin is not counted). They were 100 wide
/// (max(0 + 0, 100)) and 0 + 60 + max(5, 10) + 20 = 90 high.
///
/// Mutation: no `continue` for a child that adds nothing in `calculate_block_intrinsic_sizes`
/// stores a height of 90.
#[test]
fn an_absolutely_positioned_child_takes_no_place_among_its_siblings_margins() {
    // body(0) > float(1) > [absolutely positioned block(2), block(3)].
    let mut lw = window();
    lay_out(
        &mut lw,
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css("float: left;")
                .with_child(Dom::create_div().with_css(
                    "position: absolute; width: 10px; height: 10px; padding-top: 30px; \
                     padding-bottom: 30px; margin-bottom: 5px;",
                ))
                .with_child(
                    Dom::create_div().with_css("width: 100px; height: 20px; margin-top: 10px;"),
                ),
        ),
    );
    let float = intrinsic_sizes_of(&lw, 1);
    assert!(
        (float.min_content_width - 100.0).abs() < 0.01
            && (float.max_content_width - 100.0).abs() < 0.01,
        "the float is as wide as its child in flow, 100px: {float:?}"
    );
    assert!(
        (float.min_content_height - 20.0).abs() < 0.01
            && (float.max_content_height - 20.0).abs() < 0.01,
        "the float is as high as its child in flow, 20px (its positioned child stacked its \
         padding and a margin on top): {float:?}"
    );
}

/// A3-solver-32, the flex fold. An absolutely positioned child of a flex container is no flex
/// item (CSS Flexbox 4.1): the container's intrinsic sizes are its items'.
///
/// `body > flex row > [item 30 x 10, absolute 10 x 10 with 50px left and 40px top padding]`:
/// 30 wide and 10 high. They were 30 + (0 + 50) = 80 wide and max(10, 0 + 40) = 40 high.
///
/// Mutation: no `continue` for a child that adds nothing in `calculate_flex_intrinsic_sizes`
/// stores 80 x 40.
#[test]
fn an_absolutely_positioned_child_of_a_flex_container_adds_nothing_to_its_intrinsic_sizes() {
    // body(0) > flex row(1) > [item(2), absolutely positioned block(3)].
    let mut lw = window();
    lay_out(
        &mut lw,
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row;")
                .with_child(
                    Dom::create_div().with_css("width: 30px; height: 10px; flex-shrink: 0;"),
                )
                .with_child(Dom::create_div().with_css(
                    "position: absolute; width: 10px; height: 10px; padding-left: 50px; \
                     padding-top: 40px;",
                )),
        ),
    );
    let row = intrinsic_sizes_of(&lw, 1);
    assert!(
        (row.min_content_width - 30.0).abs() < 0.01 && (row.max_content_width - 30.0).abs() < 0.01,
        "the flex row is as wide as its one item, 30px: {row:?}"
    );
    assert!(
        (row.min_content_height - 10.0).abs() < 0.01
            && (row.max_content_height - 10.0).abs() < 0.01,
        "the flex row is as high as its one item, 10px: {row:?}"
    );
}
