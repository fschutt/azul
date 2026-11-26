//! The inline shift of `position: relative` (`inline_relative_offset`) for the node a run of text came
//! from: a node the DOM does not have (the run of a text edit that outlived the DOM it was typed in names
//! one: E22, `node_data[260]` of a DOM of 260 nodes panicked the layout pass of the main thread) shifts
//! nothing, and the lookup never panics.

use azul_core::{
    dom::{Dom, NodeId},
    geom::{LogicalPosition, LogicalSize},
    styled_dom::StyledDom,
};

use super::*;

/// body(0) > div(1) > [p(2) > text(3)]
fn page() -> StyledDom {
    let mut dom = Dom::create_body().with_child(Dom::create_div().with_child(
        Dom::create_p().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("x")),
    ));
    let (css, _) = azul_css::parser2::new_from_str("p { display: block; }");
    StyledDom::create(&mut dom, css)
}

#[test]
fn a_node_of_the_dom_in_no_relative_box_shifts_nothing() {
    let styled_dom = page();
    let shift = inline_relative_offset(
        &styled_dom,
        NodeId::new(3),
        NodeId::new(2),
        LogicalSize::zero(),
        LogicalSize::zero(),
    );
    assert_eq!(shift, LogicalPosition::zero());
}

#[test]
fn a_node_the_dom_does_not_have_shifts_nothing_and_does_not_panic() {
    let styled_dom = page();
    let count = styled_dom.node_data.as_ref().len();
    assert_eq!(count, 4, "premise: the page has four nodes");
    for stale in [count, count + 1, 260, usize::MAX / 2] {
        let shift = inline_relative_offset(
            &styled_dom,
            NodeId::new(stale),
            NodeId::new(2),
            LogicalSize::zero(),
            LogicalSize::zero(),
        );
        assert_eq!(shift, LogicalPosition::zero(), "node {stale} of {count}");
    }
}
