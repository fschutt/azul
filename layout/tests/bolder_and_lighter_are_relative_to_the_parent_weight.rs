//! `font-weight: bolder` / `lighter` are relative to the parent's computed
//! weight (CSS Fonts 4 s2.2, the relative-weight table), and the descendants
//! inherit the computed NUMBER, not the keyword.
//!
//! Both keywords mapped to fixed weights (900 / 300), so every `<b>` and
//! `<strong>` (UA `font-weight: bolder`) in normal text asked for Black
//! instead of Bold, the text inside it inherited the keyword and did too, and
//! a `<b>` inside a `<b>` was no bolder than its parent (MAILENG6, "seen
//! broken").

use azul_core::{
    dom::Dom,
    id::NodeId,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::props::basic::PhysicalSize;
use azul_layout::{solver3::getters, text3::cache::FontStack};
use rust_fontconfig::FcWeight;

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

/// `body(0) > p(1) > [ "a"(2),
///   b(3) > [ "b"(4), b(5) > "c"(6) ],
///   span[600](7) > [ "d"(8), b(9) > "e"(10) ],
///   b[900](11) > [ "f"(12), span[lighter](13) > "g"(14) ],
///   span[lighter](15) > "h"(16) ]`
fn document() -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body().with_child(
            Dom::create_p()
                .with_child(text("a"))
                .with_child(
                    Dom::create_b()
                        .with_child(text("b"))
                        .with_child(Dom::create_b().with_child(text("c"))),
                )
                .with_child(
                    Dom::create_span()
                        .with_css("font-weight: 600")
                        .with_child(text("d"))
                        .with_child(Dom::create_b().with_child(text("e"))),
                )
                .with_child(
                    Dom::create_b()
                        .with_css("font-weight: 900")
                        .with_child(text("f"))
                        .with_child(
                            Dom::create_span()
                                .with_css("font-weight: lighter")
                                .with_child(text("g")),
                        ),
                )
                .with_child(
                    Dom::create_span()
                        .with_css("font-weight: lighter")
                        .with_child(text("h")),
                ),
        ),
    )
}

/// The weight a text node's runs ask the font resolver for.
fn weight(sd: &StyledDom, node: usize, state: &StyledNodeState) -> FcWeight {
    let props = getters::get_style_properties_for_state(
        sd,
        NodeId::new(node),
        None,
        PhysicalSize::new(800.0, 600.0),
        state,
    );
    match &props.font_stack {
        FontStack::Stack(selectors) => selectors.first().expect("a font selector").weight,
        FontStack::Ref(_) => panic!("node {node}: premise, a font-family stack"),
    }
}

fn assert_weights(state: &StyledNodeState, what: &str) {
    let sd = document();
    let cases = [
        (2, FcWeight::Normal, "plain text: 400"),
        (
            4,
            FcWeight::Bold,
            "text in <b>: bolder than 400 is 700, inherited as 700",
        ),
        (
            6,
            FcWeight::Black,
            "text in <b> in <b>: bolder than 700 is 900",
        ),
        (8, FcWeight::SemiBold, "text in a 600 span: 600"),
        (
            10,
            FcWeight::Black,
            "text in <b> in a 600 span: bolder than 600 is 900",
        ),
        (12, FcWeight::Black, "text in a 900 <b>: 900"),
        (14, FcWeight::Bold, "lighter than 900 is 700"),
        (16, FcWeight::Thin, "lighter than 400 is 100"),
    ];
    let got: Vec<(usize, FcWeight)> = cases
        .iter()
        .map(|&(n, ..)| (n, weight(&sd, n, state)))
        .collect();
    for (&(node, want, why), &(_, got_weight)) in cases.iter().zip(&got) {
        assert_eq!(
            got_weight, want,
            "{what}: node {node}, {why} (all: {got:?})"
        );
    }
}

#[test]
fn bolder_and_lighter_resolve_against_the_parent_weight() {
    assert_weights(&StyledNodeState::default(), "resting state (compact cache)");
}

#[test]
fn bolder_and_lighter_resolve_against_the_parent_weight_in_a_pseudo_state_too() {
    // A non-resting state reads the cascade itself (the slow path), not the
    // compact cache: it must come to the same weights.
    let hovered = StyledNodeState {
        hover: true,
        ..StyledNodeState::default()
    };
    assert_weights(&hovered, "hover state (cascade)");
}
