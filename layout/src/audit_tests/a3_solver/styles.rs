//! A3-solver guards, part `styles`: the second key of the style memo, by the values the build
//! reads (`getters::StyleInputs`; helpers: `super::{window, lay_out, layer_of_absolute_spans}`).
//!
//! The counters are thread-locals next to the memo (`getters::STYLE_VALUE_HITS`,
//! `STYLE_VALUE_HIT_NODES`, `STYLE_BUILDS`). In a test build every node that takes a style by
//! its inputs also builds it and asserts the two equal (`getters::get_style_properties_cached`),
//! so each test here - and every other test of the crate that lays a page out - checks the key
//! as it goes; `STYLE_BUILDS` counts only the builds a release build makes as well.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::props::basic::PhysicalSize;

use super::{lay_out, layer_of_absolute_spans, window};
use crate::{
    font_traits::StyleProperties,
    solver3::getters::{
        get_style_properties, get_style_properties_cached, StyleCache, STYLE_BUILDS,
        STYLE_VALUE_HITS, STYLE_VALUE_HIT_NODES,
    },
    text3::cache::TextDecoration,
};

/// The viewport the styles below are asked at.
fn viewport() -> PhysicalSize {
    PhysicalSize::new(800.0, 600.0)
}

/// Clears the counters of the second key on this thread.
fn reset_counters() {
    STYLE_VALUE_HITS.with(|n| n.set(0));
    STYLE_BUILDS.with(|n| n.set(0));
    STYLE_VALUE_HIT_NODES.with(|nodes| nodes.borrow_mut().clear());
}

/// Whether DOM node `node` took its style by its inputs since the counters were cleared.
fn taken_by_inputs(node: usize) -> bool {
    let node = u32::try_from(node).expect("a small node index");
    STYLE_VALUE_HIT_NODES.with(|nodes| nodes.borrow().contains(&node))
}

/// The style of every node of `styled`, asked in document order from ONE memo.
fn styles_of(styled: &StyledDom) -> Vec<Arc<StyleProperties>> {
    let mut memo = StyleCache::default();
    (0..styled.node_data.len())
        .map(|i| get_style_properties_cached(&mut memo, styled, NodeId::new(i), None, viewport()))
        .collect()
}

/// Every style of `styles` is the one its node builds (`get_style_properties`, no memo).
fn assert_each_is_the_style_its_node_builds(styled: &StyledDom, styles: &[Arc<StyleProperties>]) {
    for (node, style) in styles.iter().enumerate() {
        let built = get_style_properties(styled, NodeId::new(node), None, viewport());
        assert!(
            **style == built,
            "DOM node {node} was given a style that is not the one it builds:\n  given: \
             {style:?}\n  built: {built:?}"
        );
    }
}

/// `body(0) > css.len() x [span(1 + 2 i) > text(2 + 2 i)]`: labels, the i-th span under the
/// declarations `css[i]` (an empty string: none).
fn labels(css: &[&str]) -> StyledDom {
    let mut body = Dom::create_body();
    for declarations in css {
        let label = Dom::create_span_with_text("label");
        body = body.with_child(if declarations.is_empty() {
            label
        } else {
            label.with_css(declarations)
        });
    }
    StyledDom::create_from_dom(body)
}

/// The span of label `i` of [`labels`].
const fn span(i: usize) -> usize {
    1 + 2 * i
}

/// The text of label `i` of [`labels`].
const fn text(i: usize) -> usize {
    2 + 2 * i
}

/// Raises a pseudo-state on DOM node `node`, flag by flag as the window does before a layout.
fn raise(styled: &mut StyledDom, node: usize, flag: fn(&mut StyledNodeState)) {
    let mut nodes = styled.styled_nodes.as_container_mut();
    let styled_node = nodes.get_mut(NodeId::new(node)).expect("the node exists");
    flag(&mut styled_node.styled_node_state);
}

/// A3-solver-20b. The style memo shared equal styles only AFTER building them: every label of
/// a map view and every text in one built its style (the family list cloned, the font stack
/// cloned out of its memo, the result hashed, compared and dropped) to find that an earlier
/// node had made the same one. A node whose inputs equal an earlier node's takes that node's
/// style: a layer of 200 equal labels - 200 spans, 200 texts - builds a handful of styles.
///
/// Mutation: without the lookup in `by_inputs` (or with `style_inputs_of` answering `None`)
/// no node is served by its inputs and the page builds a style per span and per text.
#[test]
fn equal_labels_take_their_style_by_its_inputs_instead_of_building_it() {
    let k = 200;
    let mut lw = window();
    reset_counters();
    lay_out(&mut lw, layer_of_absolute_spans(k));
    let hits = STYLE_VALUE_HITS.with(std::cell::Cell::get);
    let builds = STYLE_BUILDS.with(std::cell::Cell::get);
    // The text of label i is DOM node 3 + 2 i (`layer_of_absolute_spans`).
    let texts = (0..k).filter(|i| taken_by_inputs(3 + 2 * i)).count();
    assert!(
        texts >= k - 1,
        "{texts} of the {k} equal texts took their style by its inputs: all but the first do"
    );
    assert!(
        hits >= k,
        "a layer of {k} equal labels took a style by its inputs {hits} times ({builds} built)"
    );
    // One for the spans, one for their texts, one each for the layer and the body - per memo
    // (a pass has one); without the key, one per span and per text: 2 k.
    assert!(
        builds <= 16,
        "a layer of {k} equal labels built {builds} styles: a handful is enough"
    );
}

/// A3-solver-20b, the result. Labels that differ in ONE input of their style - each through
/// one declaration - get a style of their own with that input in it, never an earlier label's;
/// equal labels share one allocation, also a label that comes after all the others.
///
/// Mutation: a field left out of `getters::StyleInputs` (`font_size`, `text_color`,
/// `font_family_hash`, `tier1`, `letter_spacing`, `word_spacing`, `line_height`,
/// `text_transform`, `tab_size`) serves that label the plain label's style: it is "taken by
/// its inputs" and the field does not differ (and the memo's own check panics). A keyword of
/// `vertical-align` is in the key twice, in the tier-1 enums and as the cascade answers it.
///
/// A label that differs in something its style does NOT hold (`text-align`, one of the tier-1
/// enums) has other inputs and is built: a field too many in the key costs a build, not the
/// allocation - `by_value` still gives it the plain label's. Mutation: without the lookup in
/// the `by_value` bucket (always `Arc::new`) the two are two allocations.
#[test]
#[allow(clippy::float_cmp)] // sizes read from declarations, not computed
fn labels_that_differ_in_one_input_get_their_own_style_and_equal_ones_share_one() {
    let css = [
        "",
        "",
        "font-size: 20px;",
        "color: #ff0000;",
        "font-family: monospace;",
        "font-weight: bold;",
        "font-style: italic;",
        "letter-spacing: 2px;",
        "word-spacing: 3px;",
        "line-height: 30px;",
        "text-transform: uppercase;",
        "vertical-align: super;",
        "tab-size: 4px;",
        "text-align: center;",
        "",
    ];
    let styled = labels(&css);
    reset_counters();
    let styles = styles_of(&styled);
    assert_each_is_the_style_its_node_builds(&styled, &styles);

    // The plain labels: 0, 1 and - after all the others - 14.
    for i in [1_usize, 14] {
        assert!(
            taken_by_inputs(span(i)) && taken_by_inputs(text(i)),
            "harness: the plain label {i} and its text take their style by its inputs"
        );
        assert!(
            Arc::ptr_eq(&styles[span(0)], &styles[span(i)])
                && Arc::ptr_eq(&styles[text(0)], &styles[text(i)]),
            "the plain labels 0 and {i} do not share one style"
        );
    }

    // The others: one input changed each, read off the span that declares it.
    let plain = &styles[span(0)];
    for i in 2..13_usize {
        let own = &styles[span(i)];
        let (property, differs) = match i {
            2 => ("font-size", own.font_size_px != plain.font_size_px),
            3 => ("color", own.color != plain.color),
            4 => ("font-family", own.font_stack != plain.font_stack),
            5 => ("font-weight", own.font_stack != plain.font_stack),
            6 => ("font-style", own.font_stack != plain.font_stack),
            7 => ("letter-spacing", own.letter_spacing != plain.letter_spacing),
            8 => ("word-spacing", own.word_spacing != plain.word_spacing),
            9 => ("line-height", own.line_height != plain.line_height),
            10 => ("text-transform", own.text_transform != plain.text_transform),
            11 => ("vertical-align", own.vertical_align != plain.vertical_align),
            _ => ("tab-size", own.tab_size != plain.tab_size),
        };
        assert!(
            !taken_by_inputs(span(i)),
            "label {i} declares another {property} and took the style of an earlier label"
        );
        assert!(
            !Arc::ptr_eq(own, plain) && differs,
            "label {i} declares another {property}; its style has the plain label's:\n  \
             plain: {plain:?}\n  label: {own:?}"
        );
    }
    assert!(
        (styles[span(2)].font_size_px - 20.0).abs() < 0.01,
        "`font-size: 20px` gave a label {} px",
        styles[span(2)].font_size_px
    );

    // Label 13: other inputs (`text-align`), the same style.
    assert!(
        !taken_by_inputs(span(13)),
        "harness: a label with another `text-align` has other inputs than the plain one"
    );
    assert!(
        Arc::ptr_eq(&styles[span(13)], plain),
        "a label built to the plain label's style does not share its allocation"
    );
}

/// A3-solver-20b, the guard. A node whose style has an input that is not in the key is not
/// served by its inputs, although every slot the key holds equals a plain label's: a span
/// under the pointer or around the focus (any state but the normal one sends the getters to
/// the cascade), an inline box with a background, a border, padding or a horizontal margin
/// (all of it is in the box's style), a span with a `text-decoration` of its own.
///
/// Mutation: without `!node_state.is_normal()` in `getters::style_inputs_of` the hovered and
/// the focus-within span are served; without its inline-box block the four decorated boxes
/// are; without `HOT_FLAG_HAS_TEXT_DECORATION` the underlined span is.
#[test]
fn a_label_whose_style_has_an_input_outside_the_key_is_not_served_by_its_inputs() {
    let css = [
        "",
        "",
        "",
        "",
        "background-color: #ffff00;",
        "text-decoration: underline;",
        "padding-left: 4px;",
        "border: 1px solid #000000;",
        "margin-right: 3px;",
        "",
    ];
    let mut styled = labels(&css);
    raise(&mut styled, span(2), |state| state.hover = true);
    raise(&mut styled, span(3), |state| state.focus_within = true);
    reset_counters();
    let styles = styles_of(&styled);
    assert_each_is_the_style_its_node_builds(&styled, &styles);

    assert!(
        taken_by_inputs(span(1)) && taken_by_inputs(span(9)),
        "harness: a plain span before and after the others takes its style by its inputs"
    );
    for (i, what) in [
        (2_usize, "under the pointer"),
        (3, "around the focus"),
        (4, "with a background"),
        (5, "with an underline of its own"),
        (6, "with padding"),
        (7, "with a border"),
        (8, "with a horizontal margin"),
    ] {
        assert!(
            !taken_by_inputs(span(i)),
            "the span {what} (label {i}) was served the style of a plain span"
        );
    }

    // What the plain span's style would have dropped.
    let plain = &styles[span(0)];
    assert!(
        plain.background_color.is_none()
            && plain.border.is_none()
            && plain.text_decoration == TextDecoration::default(),
        "harness: a plain span paints nothing around its text: {plain:?}"
    );
    assert!(
        styles[span(4)].background_color.is_some(),
        "the background of an inline box is in its style"
    );
    assert!(
        styles[span(5)].text_decoration != TextDecoration::default(),
        "the underline of a span is in its style"
    );
    for i in [6_usize, 7, 8] {
        assert!(
            styles[span(i)].border.is_some(),
            "the edges of the inline box of label {i} are in its style"
        );
    }
}

/// A3-solver-20b, the guard, a box that is no inline box: an underlined block is not served a
/// plain block's style. The labels UNDER it get the style they build, whichever way: what a
/// box hands down to the text in it is no input of that text's own style (`text-decoration` is
/// not inherited; the inline formatting context adds what the boxes around a text propagate,
/// `getters::propagated_text_decoration`).
///
/// `body(0) > [div(1) > span(2) > text(3), div(4) > span(5) > text(6), div(7) > span(8) >
/// text(9)]`, the second and the third block underlined.
///
/// Mutation: without `HOT_FLAG_HAS_TEXT_DECORATION` in `getters::style_inputs_of` the
/// underlined blocks take the plain block's style, without its underline.
#[test]
fn an_underlined_block_keeps_its_underline_and_the_labels_under_it_get_the_style_they_build() {
    let block = || Dom::create_div().with_child(Dom::create_span_with_text("label"));
    let styled = StyledDom::create_from_dom(
        Dom::create_body()
            .with_child(block())
            .with_child(block().with_css("text-decoration: underline;"))
            .with_child(block().with_css("text-decoration: underline;")),
    );
    reset_counters();
    let styles = styles_of(&styled);
    assert_each_is_the_style_its_node_builds(&styled, &styles);
    assert!(
        styles[1].text_decoration == TextDecoration::default(),
        "harness: the plain block declares no decoration"
    );
    for node in [4_usize, 7] {
        assert!(
            !taken_by_inputs(node),
            "the underlined block (DOM node {node}) was served the style of an earlier block"
        );
        assert!(
            styles[node].text_decoration != TextDecoration::default(),
            "the underlined block (DOM node {node}) lost its underline"
        );
    }
}
