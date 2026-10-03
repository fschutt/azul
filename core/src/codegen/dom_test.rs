//! Unit tests of the lowering's decisions: the CSS matcher and the
//! constructor an element gets (moved from `xml_test.rs` with the code).

#![allow(clippy::all, clippy::pedantic, clippy::nursery)]

use azul_css::{
    css::{CssNthChildPattern, CssNthChildSelector},
    AzString,
};

use super::*;
use crate::{
    window::{AzStringPair, StringPairVec},
    xml::XmlAttributeMap,
};

// ----------------------------------------------------------------- helpers

fn attrs(kv: &[(&str, &str)]) -> XmlAttributeMap {
    XmlAttributeMap::from(StringPairVec::from_vec(
        kv.iter()
            .map(|(k, v)| AzStringPair {
                key: AzString::from(*k),
                value: AzString::from(*v),
            })
            .collect::<Vec<_>>(),
    ))
}

fn node(tag: &str, kv: &[(&str, &str)], children: Vec<XmlNodeChild>) -> XmlNode {
    XmlNode {
        node_type: tag.into(),
        attributes: attrs(kv),
        children: children.into(),
    }
}

fn txt(s: &str) -> XmlNodeChild {
    XmlNodeChild::Text(AzString::from(s))
}

fn elem(n: XmlNode) -> XmlNodeChild {
    XmlNodeChild::Element(n)
}

fn sem(suffix: &str, args: Vec<CtorArg>, consumes_text: bool) -> NodeCtor {
    NodeCtor::Semantic {
        suffix: suffix.to_string(),
        args,
        consumes_text,
        skip_caption: false,
    }
}

fn s(v: &str) -> CtorArg {
    CtorArg::Str(v.to_string())
}

// ================================================================
// cap_first / camel_to_snake
// ================================================================

#[test]
fn cap_first_edge_inputs() {
    assert_eq!(cap_first(""), "", "empty input must not panic");
    assert_eq!(cap_first("h1"), "H1");
    assert_eq!(cap_first("button"), "Button");
    assert_eq!(cap_first("A"), "A", "already-uppercase is idempotent");
    assert_eq!(
        cap_first("\u{1F600}x"),
        "\u{1F600}x",
        "emoji has no uppercase form"
    );
    // 'ß' uppercases to TWO chars — the fn must not assume 1:1.
    assert_eq!(cap_first("\u{df}x"), "SSx");
    assert_eq!(cap_first(&"a".repeat(10_000)).len(), 10_000);
}

#[test]
fn camel_to_snake_documented_forms() {
    assert_eq!(camel_to_snake("ButtonNoA11y"), "button_no_a11y");
    assert_eq!(camel_to_snake("PWithText"), "p_with_text");
    assert_eq!(camel_to_snake("ANoA11y"), "a_no_a11y");
    assert_eq!(camel_to_snake("H1WithText"), "h1_with_text");
    assert_eq!(camel_to_snake("Div"), "div");
}

#[test]
fn camel_to_snake_edge_inputs() {
    assert_eq!(camel_to_snake(""), "");
    assert_eq!(camel_to_snake("A"), "a");
    assert_eq!(camel_to_snake("AB"), "ab", "an all-caps run is not split");
    assert_eq!(
        camel_to_snake("ABc"),
        "a_bc",
        "a caps run splits before the last cap"
    );
    assert_eq!(camel_to_snake("\u{1F600}"), "\u{1F600}");
    assert_eq!(camel_to_snake(&"a".repeat(10_000)).len(), 10_000);
}

// ================================================================
// safe_container_tag
// ================================================================

#[test]
fn safe_container_tag_falls_back_to_div_for_arg_taking_widgets() {
    assert_eq!(safe_container_tag(""), "Div");
    assert_eq!(safe_container_tag("Div"), "Div");
    assert_eq!(safe_container_tag("Span"), "Span");
    assert_eq!(safe_container_tag("H1"), "H1");
    // Interactive / arg-taking elements deliberately degrade to a container.
    assert_eq!(safe_container_tag("Button"), "Div");
    assert_eq!(safe_container_tag("Input"), "Div");
    assert_eq!(safe_container_tag("A"), "Div");
    assert_eq!(safe_container_tag("\u{1F600}"), "Div");
    assert_eq!(safe_container_tag(&"z".repeat(10_000)), "Div");
}

/// BUG (reported): `SAFE_CONTAINER_TAGS` is documented as holding the
/// `NodeType`/`NodeTypeTag` **debug names**, and `safe_container_tag` compares
/// against `format!("{:?}", tag_to_node_type(tag))`. But six entries are spelled
/// with a different inner capitalization than the actual variant, so the
/// comparison never matches and `<blockquote>`/`<figcaption>`/`<thead>`/`<tbody>`
/// /`<tfoot>`/`<colgroup>` silently compile down to a plain `div`.
#[test]
fn safe_container_tag_matches_the_real_nodetype_debug_names() {
    for tag in [
        "blockquote",
        "figcaption",
        "thead",
        "tbody",
        "tfoot",
        "colgroup",
    ] {
        let dbg = format!("{:?}", tag_to_node_type(tag));
        assert_ne!(
            safe_container_tag(&dbg),
            "Div",
            "<{tag}> (NodeType debug name {dbg:?}) is a pure container and must keep its own \
             creator instead of degrading to a div"
        );
    }
}

// ================================================================
// node_direct_text / node_aria_label / node_attr_or / node_attr_f32
// first_caption_text
// ================================================================

#[test]
fn node_direct_text_trims_and_skips_elements() {
    assert_eq!(node_direct_text(&XmlNode::default()), "");
    assert_eq!(node_direct_text(&node("p", &[], vec![txt("  Go  ")])), "Go");
    assert_eq!(
        node_direct_text(&node(
            "p",
            &[],
            vec![
                txt("a"),
                elem(node("b", &[], vec![txt("IGNORED")])),
                txt("b")
            ]
        )),
        "a b",
        "direct text children are joined with a single space"
    );
    assert_eq!(
        node_direct_text(&node("p", &[], vec![txt("   "), txt("\t\n")])),
        "",
        "whitespace-only children are dropped"
    );
}

#[test]
fn node_aria_label_ignores_empty_and_whitespace() {
    assert_eq!(node_aria_label(&XmlNode::default()), None);
    assert_eq!(
        node_aria_label(&node("b", &[("aria-label", "")], vec![])),
        None
    );
    assert_eq!(
        node_aria_label(&node("b", &[("aria-label", "   ")], vec![])),
        None
    );
    assert_eq!(
        node_aria_label(&node("b", &[("aria-label", "  Save  ")], vec![])),
        Some("Save".to_string())
    );
}

#[test]
fn node_attr_or_returns_the_default_when_absent() {
    let n = node("a", &[("href", "/x"), ("empty", "")], vec![]);
    assert_eq!(node_attr_or(&n, "href", "FALLBACK"), "/x");
    assert_eq!(node_attr_or(&n, "missing", "FALLBACK"), "FALLBACK");
    assert_eq!(
        node_attr_or(&n, "empty", "FALLBACK"),
        "",
        "a present-but-empty attribute wins over the default"
    );
    assert_eq!(node_attr_or(&XmlNode::default(), "x", ""), "");
}

#[test]
fn node_attr_f32_zero_negative_and_defaults() {
    let n = node(
        "meter",
        &[
            ("zero", "0"),
            ("negzero", "-0"),
            ("neg", "-2.5"),
            ("pad", "  1.5  "),
        ],
        vec![],
    );
    assert_eq!(node_attr_f32(&n, "zero", 9.0), 0.0);
    assert!(node_attr_f32(&n, "negzero", 9.0).is_sign_negative());
    assert_eq!(node_attr_f32(&n, "neg", 9.0), -2.5);
    assert_eq!(
        node_attr_f32(&n, "pad", 9.0),
        1.5,
        "the value is trimmed first"
    );
    assert_eq!(node_attr_f32(&n, "missing", 9.0), 9.0);
}

#[test]
fn node_attr_f32_unparsable_falls_back_and_min_max_saturate() {
    let n = node(
        "meter",
        &[
            ("junk", "abc"),
            ("empty", ""),
            ("huge", "1e400"),
            ("tiny", "-1e400"),
            ("big", "340282350000000000000000000000000000000"),
        ],
        vec![],
    );
    assert_eq!(node_attr_f32(&n, "junk", 7.0), 7.0);
    assert_eq!(node_attr_f32(&n, "empty", 7.0), 7.0);
    assert!(
        node_attr_f32(&n, "huge", 7.0).is_infinite(),
        "an out-of-range literal saturates to inf, it does not panic"
    );
    assert_eq!(node_attr_f32(&n, "tiny", 7.0), f32::NEG_INFINITY);
    assert_eq!(node_attr_f32(&n, "big", 7.0), f32::MAX);
}

#[test]
fn node_attr_f32_accepts_nan_and_inf_spellings() {
    // Rust's f32 FromStr accepts "NaN"/"inf", so hostile markup can inject a
    // non-finite value straight into codegen (see fmt_f32_lit above).
    let n = node("progress", &[("value", "NaN"), ("max", "inf")], vec![]);
    assert!(node_attr_f32(&n, "value", 0.0).is_nan());
    assert_eq!(node_attr_f32(&n, "max", 1.0), f32::INFINITY);
}

#[test]
fn node_attr_f32_nan_default_is_returned_verbatim() {
    assert!(node_attr_f32(&XmlNode::default(), "x", f32::NAN).is_nan());
    assert_eq!(
        node_attr_f32(&XmlNode::default(), "x", f32::INFINITY),
        f32::INFINITY
    );
}

#[test]
fn first_caption_text_edges() {
    assert_eq!(first_caption_text(&XmlNode::default()), None);
    assert_eq!(
        first_caption_text(&node(
            "table",
            &[],
            vec![elem(node("caption", &[], vec![]))]
        )),
        None,
        "an empty caption yields None"
    );
    assert_eq!(
        first_caption_text(&node(
            "table",
            &[],
            vec![elem(node("CAPTION", &[], vec![txt("  Hi  ")]))]
        )),
        Some("Hi".to_string()),
        "the tag match is ASCII-case-insensitive and the text is trimmed"
    );
}

// ================================================================
// group_matches / CssMatcher  (numeric: indices)
// ================================================================

fn refs(v: &[CssPathSelector]) -> Vec<&CssPathSelector> {
    v.iter().collect()
}

#[test]
fn group_matches_global_matches_at_any_index() {
    let a = vec![CssPathSelector::Global];
    assert!(group_matches(&refs(&a), &[], 0, 0));
    assert!(
        group_matches(&refs(&a), &[], usize::MAX, usize::MAX),
        "usize::MAX indices must not overflow"
    );
}

#[test]
fn group_matches_type_class_id() {
    let div = vec![CssPathSelector::Type(NodeTypeTag::Div)];
    let p = vec![CssPathSelector::Type(NodeTypeTag::P)];
    assert!(group_matches(&refs(&div), &refs(&div), 0, 1));
    assert!(!group_matches(&refs(&div), &refs(&p), 0, 1));
    assert!(
        !group_matches(&refs(&div), &[], 0, 1),
        "an empty haystack never matches"
    );

    let cls = vec![CssPathSelector::Class(AzString::from("x"))];
    assert!(group_matches(&refs(&cls), &refs(&cls), 0, 1));
    let id = vec![CssPathSelector::Id(AzString::from("x"))];
    assert!(
        !group_matches(&refs(&id), &refs(&cls), 0, 1),
        "an id is not a class"
    );
}

#[test]
fn group_matches_first_and_last_pseudo_at_boundaries() {
    let first = vec![CssPathSelector::PseudoSelector(
        CssPathPseudoSelector::First,
    )];
    assert!(group_matches(&refs(&first), &[], 0, 10));
    assert!(!group_matches(&refs(&first), &[], 1, 10));

    let last = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::Last)];
    assert!(group_matches(&refs(&last), &[], 9, 10));
    assert!(!group_matches(&refs(&last), &[], 8, 10));
    assert!(
        group_matches(&refs(&last), &[], 0, 0),
        "parent_children == 0 saturates to 0, so index 0 counts as last"
    );
}

#[test]
fn group_matches_nth_child_even_odd_and_number() {
    let even = vec![CssPathSelector::PseudoSelector(
        CssPathPseudoSelector::NthChild(CssNthChildSelector::Even),
    )];
    assert!(group_matches(&refs(&even), &[], 0, 0));
    assert!(!group_matches(&refs(&even), &[], 1, 0));
    assert!(
        !group_matches(&refs(&even), &[], usize::MAX, 0),
        "usize::MAX is odd"
    );

    let odd = vec![CssPathSelector::PseudoSelector(
        CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd),
    )];
    assert!(group_matches(&refs(&odd), &[], 1, 0));
    assert!(!group_matches(&refs(&odd), &[], 2, 0));

    let n = vec![CssPathSelector::PseudoSelector(
        CssPathPseudoSelector::NthChild(CssNthChildSelector::Number(u32::MAX)),
    )];
    assert!(group_matches(&refs(&n), &[], u32::MAX as usize, 0));
    assert!(!group_matches(&refs(&n), &[], 0, 0));
}

#[test]
fn group_matches_nth_child_pattern_zero_repeat_does_not_divide_by_zero() {
    let zero = vec![CssPathSelector::PseudoSelector(
        CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat: 0,
            offset: 0,
        })),
    )];
    // `is_multiple_of(0)` is `self == 0` — no division by zero.
    assert!(group_matches(&refs(&zero), &[], 0, 0));
    assert!(!group_matches(&refs(&zero), &[], 5, 0));

    let offset_past = vec![CssPathSelector::PseudoSelector(
        CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat: 2,
            offset: u32::MAX,
        })),
    )];
    assert!(
        group_matches(&refs(&offset_past), &[], 0, 0),
        "index - offset saturates to 0 rather than underflowing"
    );
}

#[test]
fn group_matches_structural_combinators_never_match() {
    for sel in [
        CssPathSelector::Children,
        CssPathSelector::DirectChildren,
        CssPathSelector::AdjacentSibling,
        CssPathSelector::GeneralSibling,
    ] {
        let a = vec![sel.clone()];
        assert!(
            !group_matches(&refs(&a), &refs(&a), 0, 1),
            "{sel:?} is a combinator, not a matchable group member"
        );
    }
}

#[test]
fn css_matcher_empty_path_never_matches() {
    let m = CssMatcher {
        path: Vec::new(),
        indices_in_parent: vec![0],
        children_length: vec![0],
    };
    let path = CssPath {
        selectors: vec![CssPathSelector::Type(NodeTypeTag::Body)].into(),
    };
    assert!(!m.matches(&path), "an empty matcher path can never match");

    let m2 = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: vec![0],
        children_length: vec![0],
    };
    let empty_path = CssPath {
        selectors: Vec::new().into(),
    };
    assert!(
        !m2.matches(&empty_path),
        "an empty CSS path can never match"
    );
}

#[test]
fn css_matcher_mismatched_bookkeeping_vec_lengths_bail_out() {
    // `indices_in_parent` / `children_length` must be as long as the group list.
    let m = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: Vec::new(),
        children_length: Vec::new(),
    };
    let path = CssPath {
        selectors: vec![CssPathSelector::Type(NodeTypeTag::Body)].into(),
    };
    assert!(
        !m.matches(&path),
        "a desynced matcher must return false, not index out of bounds"
    );
}

#[test]
fn get_css_blocks_and_inline_string_on_empty_css() {
    let m = CssMatcher {
        path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
        indices_in_parent: vec![0],
        children_length: vec![0],
    };
    assert!(get_css_blocks(&Css::empty(), &m).is_empty());
    assert_eq!(css_blocks_to_inline_string(&[]), "");
}

// ================================================================
// analyze_node_ctor / CtorArg / NodeCtor
// ================================================================

#[test]
fn analyze_node_ctor_plain_for_unknown_and_empty_tags() {
    assert_eq!(analyze_node_ctor("div", &XmlNode::default()), NodeCtor::Plain);
    assert_eq!(analyze_node_ctor("", &XmlNode::default()), NodeCtor::Plain);
    assert_eq!(
        analyze_node_ctor("\u{1F600}", &XmlNode::default()),
        NodeCtor::Plain
    );
    let plain = analyze_node_ctor("div", &XmlNode::default());
    assert!(!plain.consumes_text());
    assert!(!plain.skip_caption());
}

#[test]
fn analyze_node_ctor_with_text_tier_requires_actual_text() {
    // Empty <p> stays a plain container (has_only_text_children() is vacuously
    // true for a childless node, so `has_text` is the real gate).
    assert_eq!(analyze_node_ctor("p", &XmlNode::default()), NodeCtor::Plain);
    // <p> with an element child is not "pure text" either.
    let mixed = node("p", &[], vec![txt("a"), elem(XmlNode::create("span"))]);
    assert_eq!(analyze_node_ctor("p", &mixed), NodeCtor::Plain);

    let pure = node("p", &[], vec![txt("  Hello  ")]);
    let ctor = analyze_node_ctor("p", &pure);
    assert!(ctor.consumes_text(), "the text is folded into the constructor");
    assert_eq!(ctor, sem("PWithText", vec![s("Hello")], true));
}

#[test]
fn analyze_node_ctor_button_with_and_without_aria() {
    let plain_btn = node("button", &[], vec![txt("Go")]);
    assert_eq!(
        analyze_node_ctor("button", &plain_btn),
        sem("ButtonNoA11y", vec![s("Go")], true)
    );
    let aria_btn = node("button", &[("aria-label", "Save")], vec![txt("Go")]);
    assert_eq!(
        analyze_node_ctor("button", &aria_btn),
        sem("Button", vec![s("Go"), CtorArg::Aria("Save".to_string())], true)
    );
}

#[test]
fn analyze_node_ctor_keeps_quotes_and_backslashes_raw_for_the_printers_to_escape() {
    let btn = node("button", &[], vec![txt("say \"hi\"\\now")]);
    assert_eq!(
        analyze_node_ctor("button", &btn),
        sem("ButtonNoA11y", vec![s("say \"hi\"\\now")], true)
    );
}

#[test]
fn analyze_node_ctor_anchor_without_aria_takes_its_href_and_its_optional_text() {
    let bare = node("a", &[], vec![]);
    assert_eq!(
        analyze_node_ctor("a", &bare),
        sem("ANoA11y", vec![s(""), CtorArg::OptNone], true),
        "a missing href defaults to an empty string, missing text to no text"
    );
    let full = node("a", &[("href", "/x")], vec![txt("Home")]);
    assert_eq!(
        analyze_node_ctor("a", &full),
        sem("ANoA11y", vec![s("/x"), CtorArg::OptSome("Home".to_string())], true)
    );
}

#[test]
fn analyze_node_ctor_table_aria_form_skips_the_literal_caption() {
    let t = node(
        "table",
        &[("aria-label", "Prices")],
        vec![elem(node("caption", &[], vec![txt("Q1")]))],
    );
    let ctor = analyze_node_ctor("table", &t);
    assert!(ctor.skip_caption(), "the aria form injects its own caption child");
    assert_eq!(
        ctor,
        NodeCtor::Semantic {
            suffix: "Table".to_string(),
            args: vec![s("Q1"), CtorArg::Aria("Prices".to_string())],
            consumes_text: false,
            skip_caption: true,
        }
    );
    let plain = analyze_node_ctor("table", &node("table", &[], vec![]));
    assert!(!plain.skip_caption());
    assert_eq!(plain, sem("TableNoA11y", vec![], false));
}

#[test]
fn analyze_node_ctor_scalar_widgets_use_defaults_and_parse_their_numbers() {
    assert_eq!(
        analyze_node_ctor("progress", &node("progress", &[], vec![])),
        sem(
            "ProgressNoA11y",
            vec![CtorArg::Float(0.0), CtorArg::Float(1.0)],
            false
        ),
        "missing value/max fall back to 0.0 / 1.0"
    );
    let m = node(
        "meter",
        &[("value", "5"), ("min", "-1"), ("max", "10")],
        vec![],
    );
    assert_eq!(
        analyze_node_ctor("meter", &m),
        sem(
            "MeterNoA11y",
            vec![
                CtorArg::Float(5.0),
                CtorArg::Float(-1.0),
                CtorArg::Float(10.0)
            ],
            false
        )
    );
}

/// Non-finite attribute values reach the constructor as they are. Pinned so
/// that a fix (clamping / rejecting them) shows up as a change.
#[test]
fn analyze_node_ctor_non_finite_attributes_stay_non_finite() {
    let p = node("progress", &[("value", "NaN"), ("max", "inf")], vec![]);
    match analyze_node_ctor("progress", &p) {
        NodeCtor::Semantic { args, .. } => {
            assert!(matches!(args[0], CtorArg::Float(f) if f.is_nan()), "{args:?}");
            assert!(
                matches!(args[1], CtorArg::Float(f) if f == f32::INFINITY),
                "{args:?}"
            );
        }
        other => panic!("{other:?}"),
    }
}
