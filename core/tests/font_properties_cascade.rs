//! The font properties through the REAL cascade into the compact cache (CSS Fonts 4):
//! `font-weight` as a number from 1 to 1000 with `bolder` / `lighter` resolved against the
//! parent, `font-style: oblique <angle>`, `font-stretch`, the `font` shorthand, the
//! rarely-declared font properties and the DOM flags that tell a reader they exist, and the
//! typed cascade getters.
//!
//! A `StyledDom` is built from a `Dom` and a CSS string (`StyledDom::create`, as
//! `compact_cache.rs` does), the compact cache it builds is read back through its getters.
//! The codec of the slots has its own tests (`css/src/compact_cache_font_test.rs`).
//!
//! Every DOM is `html > body > ...`, so the content starts at node 2 (pre-order). `chain(n)`
//! is `div.l0 > div.l1 > ... > div.l{n-1} > text`: level `k` is node `2 + k`, the text node
//! `2 + n`.
//!
//! The tests that go over many rows collect every row that reads back wrong
//! (`assert_nothing_wrong`), so a failing run lists all of them, not only the first.

use azul_core::{
    dom::{Dom, IdOrClass, NodeId, NodeType},
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    compact_cache::*,
    css::{Css, CssPropertyValue},
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::{
            font::{StyleFontStyle, StyleFontVariantCaps, StyleFontVariantNumeric, StyleFontWeight},
            length::{FloatValue, SizeMetric},
            StyleFontKerning, StyleFontLigatureChoice, StyleFontOpticalSizing, StyleFontStretch,
            StyleFontSynthesis, StyleFontVariantEastAsian, StyleFontVariantLigatures,
            StyleFontVariationSettings,
        },
        property::CssProperty,
    },
};

/// The six flags of the rarely-declared font properties (bits 32..=37 of `dom_declared_flags`).
const NEW_FONT_FLAGS: u64 = DOM_HAS_FONT_VARIATION_SETTINGS
    | DOM_HAS_FONT_OPTICAL_SIZING
    | DOM_HAS_FONT_KERNING
    | DOM_HAS_FONT_VARIANT_LIGATURES
    | DOM_HAS_FONT_VARIANT_EAST_ASIAN
    | DOM_HAS_FONT_SYNTHESIS;

// ===================================================================
// Helpers
// ===================================================================

/// One line per row of a table test that read back wrong.
type Wrong = Vec<String>;

fn assert_nothing_wrong(wrong: &[String]) {
    assert!(
        wrong.is_empty(),
        "{} wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Create a `StyledDom` from a `Dom` and a CSS string (as `core/tests/compact_cache.rs` does).
fn styled(mut dom: Dom, css: &str) -> StyledDom {
    let css = if css.is_empty() {
        Css::empty()
    } else {
        Css::from_string(css.into())
    };
    StyledDom::create(&mut dom, css)
}

/// The compact cache `StyledDom::create` built.
fn compact(styled: &StyledDom) -> &CompactLayoutCache {
    styled
        .css_property_cache
        .ptr
        .compact_cache
        .as_ref()
        .expect("StyledDom::create builds the compact cache")
}

/// The value the typed (slow cascade) getter `$getter` finds for node `$node` of `$styled`.
macro_rules! declared {
    ($styled:expr, $node:expr, $getter:ident) => {
        $styled
            .css_property_cache
            .ptr
            .$getter(
                &$styled.node_data.as_ref()[$node],
                &NodeId::new($node),
                &StyledNodeState::default(),
            )
            .cloned()
    };
}

/// A row is wrong unless the typed getter found `Exact(expected)`.
fn check_declared<T: PartialEq + std::fmt::Debug>(
    wrong: &mut Wrong,
    what: &str,
    found: Option<CssPropertyValue<T>>,
    expected: T,
) {
    if found != Some(CssPropertyValue::Exact(expected)) {
        wrong.push(format!("{what}: the getter found {found:?}"));
    }
}

fn div(class: &str) -> Dom {
    Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
}

fn text() -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper("t")
}

/// `html > body > content`: the content starts at node 2.
fn page(content: Dom) -> Dom {
    Dom::create_html().with_child(Dom::create_body().with_child(content))
}

/// `html > body > div.l0 > div.l1 > ... > div.l{levels-1} > text`.
fn chain(levels: usize) -> Dom {
    let mut inner = div(&format!("l{}", levels - 1)).with_child(text());
    for level in (0..levels - 1).rev() {
        inner = div(&format!("l{level}")).with_child(inner);
    }
    page(inner)
}

/// One rule per level, `.l{k} { decl }`; an empty declaration leaves the level alone.
fn level_rules(decls: &[&str]) -> String {
    let mut css = String::new();
    for (level, decl) in decls.iter().enumerate() {
        if !decl.is_empty() {
            css.push_str(&format!(".l{level} {{ {decl} }} "));
        }
    }
    css
}

fn weight(w: f32) -> StyleFontWeight {
    StyleFontWeight::from_number(w)
}

fn numbers(values: &[f32]) -> Vec<StyleFontWeight> {
    values.iter().copied().map(weight).collect()
}

fn oblique(degrees: f32) -> StyleFontStyle {
    StyleFontStyle::Oblique(FloatValue::new(degrees))
}

fn stretch(percent: f32) -> StyleFontStretch {
    StyleFontStretch::from_percent(percent)
}

/// The weights of the nodes `first..first + count`.
fn weights(s: &StyledDom, first: usize, count: usize) -> Vec<StyleFontWeight> {
    (first..first + count)
        .map(|node| compact(s).get_font_weight(node))
        .collect()
}

/// The font styles of the nodes `first..first + count`.
fn styles(s: &StyledDom, first: usize, count: usize) -> Vec<StyleFontStyle> {
    (first..first + count)
        .map(|node| compact(s).get_font_style(node))
        .collect()
}

/// The font stretches (in percent) of the nodes `first..first + count`.
fn stretches(s: &StyledDom, first: usize, count: usize) -> Vec<f32> {
    (first..first + count)
        .map(|node| compact(s).get_font_stretch(node).percent())
        .collect()
}

/// The first node of a built cache that still holds `bolder` / `lighter`: the builder resolves
/// every one, so a node's children copy a number.
fn node_holding_a_keyword(s: &StyledDom) -> Option<usize> {
    let cache = compact(s);
    (0..cache.node_count()).find(|node| cache.get_font_weight(*node).is_relative())
}

/// Checks the weights of `levels` nested divs and the text under them (`levels + 1` numbers).
fn check_weights_css(wrong: &mut Wrong, css: &str, levels: usize, expected: &[f32]) {
    let s = styled(chain(levels), css);
    let found = weights(&s, 2, levels + 1);
    if found != numbers(expected) {
        wrong.push(format!(
            "`{css}`: the {levels} levels and the text weigh {found:?}, expected {expected:?}"
        ));
    }
    if let Some(node) = node_holding_a_keyword(&s) {
        wrong.push(format!("`{css}`: node {node} still holds `bolder` / `lighter`"));
    }
}

/// [`check_weights_css`] with one declaration per level (an empty one: none).
fn check_weights(wrong: &mut Wrong, decls: &[&str], expected: &[f32]) {
    check_weights_css(wrong, &level_rules(decls), decls.len(), expected);
}

fn check_styles_css(wrong: &mut Wrong, css: &str, levels: usize, expected: &[StyleFontStyle]) {
    let s = styled(chain(levels), css);
    let found = styles(&s, 2, levels + 1);
    if found != expected {
        wrong.push(format!(
            "`{css}`: the {levels} levels and the text are {found:?}, expected {expected:?}"
        ));
    }
}

fn check_styles(wrong: &mut Wrong, decls: &[&str], expected: &[StyleFontStyle]) {
    check_styles_css(wrong, &level_rules(decls), decls.len(), expected);
}

fn check_stretches_css(wrong: &mut Wrong, css: &str, levels: usize, expected: &[f32]) {
    let s = styled(chain(levels), css);
    let found = stretches(&s, 2, levels + 1);
    if found != expected {
        wrong.push(format!(
            "`{css}`: the {levels} levels and the text are {found:?}%, expected {expected:?}%"
        ));
    }
}

fn check_stretches(wrong: &mut Wrong, decls: &[&str], expected: &[f32]) {
    check_stretches_css(wrong, &level_rules(decls), decls.len(), expected);
}

/// The computed font size in px of a node, `None` when it is not an exact px length.
fn font_size_px(s: &StyledDom, node: usize) -> Option<f32> {
    let length = decode_pixel_value_u32(compact(s).get_font_size_raw(node))?;
    (length.metric == SizeMetric::Px).then(|| length.number.get())
}

/// The six new flags a DOM raised (bits 32..=37 of `dom_declared_flags`), nothing else.
fn new_flags(s: &StyledDom) -> u64 {
    compact(s).dom_declared_flags & NEW_FONT_FLAGS
}

/// The six new flags a `div.a` with the text under it raises for `css`.
fn raised(css: &str) -> u64 {
    new_flags(&styled(page(div("a").with_child(text())), css))
}

// ===================================================================
// font-weight: a number from 1 to 1000
// ===================================================================

#[test]
fn a_numeric_font_weight_reaches_the_text_child_as_the_same_number() {
    let cases: [(&str, f32); 12] = [
        ("350", 350.0),
        ("1000", 1000.0),
        ("1", 1.0),
        ("100", 100.0),
        ("500", 500.0),
        ("900", 900.0),
        ("350.5", 350.5),
        ("12.25", 12.25),
        ("650.125", 650.125),
        ("999.5", 999.5),
        ("bold", 700.0),
        ("normal", 400.0),
    ];
    let mut wrong = Wrong::new();
    for (declared, number) in cases {
        let decl = format!("font-weight: {declared}");
        let css = level_rules(&[decl.as_str()]);
        let s = styled(chain(1), &css);
        // html 0, body 1, div.l0 2, text 3: html and body keep the initial weight, the div
        // and its text have the declared one
        let found = weights(&s, 0, 4);
        if found != numbers(&[400.0, 400.0, number, number]) {
            wrong.push(format!(
                "`{css}`: html, body, div and text weigh {found:?}, expected {number}"
            ));
        }
        if let Some(node) = node_holding_a_keyword(&s) {
            wrong.push(format!("`{css}`: node {node} holds a keyword"));
        }
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn a_weight_between_two_64ths_is_kept_to_a_64th_by_the_cascade() {
    let s = styled(chain(1), ".l0 { font-weight: 350.123 }");
    for w in weights(&s, 2, 2) {
        assert!(
            (w.number() - 350.123).abs() < 0.02,
            "350.123 came back as {w:?}"
        );
    }
}

#[test]
fn bolder_inside_bolder_climbs_to_900_and_stays_there() {
    // 400 -> bolder 700 -> bolder 900 -> bolder 900 (div > div > div > div), and the text.
    let mut wrong = Wrong::new();
    check_weights(
        &mut wrong,
        &[
            "font-weight: 400",
            "font-weight: bolder",
            "font-weight: bolder",
            "font-weight: bolder",
        ],
        &[400.0, 700.0, 900.0, 900.0, 900.0],
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn lighter_inside_lighter_falls_to_100_and_stays_there() {
    // 700 -> lighter 400 -> lighter 100 -> lighter 100.
    let mut wrong = Wrong::new();
    check_weights(
        &mut wrong,
        &[
            "font-weight: 700",
            "font-weight: lighter",
            "font-weight: lighter",
            "font-weight: lighter",
        ],
        &[700.0, 400.0, 100.0, 100.0, 100.0],
    );
    assert_nothing_wrong(&wrong);
}

/// CSS Fonts 4 section 2.2, the relative weights: (the inherited weight, what `bolder` gives,
/// what `lighter` gives), at and around every boundary. `bolder` under 350 is 700, under 100
/// is 400, past 900 it changes nothing.
const RELATIVE_WEIGHTS: &[(f32, f32, f32)] = &[
    (50.0, 400.0, 50.0),
    (99.5, 400.0, 99.5),
    (100.0, 400.0, 100.0),
    (300.0, 400.0, 100.0),
    (349.5, 400.0, 100.0),
    (350.0, 700.0, 100.0),
    (400.0, 700.0, 100.0),
    (549.5, 700.0, 100.0),
    (550.0, 900.0, 400.0),
    (700.0, 900.0, 400.0),
    (749.5, 900.0, 400.0),
    (750.0, 900.0, 700.0),
    (899.5, 900.0, 700.0),
    (900.0, 900.0, 700.0),
    (950.0, 950.0, 700.0),
    (1000.0, 1000.0, 700.0),
];

#[test]
fn bolder_and_lighter_follow_the_css_fonts_4_table_at_every_boundary() {
    let mut wrong = Wrong::new();
    for &(parent, bolder, lighter) in RELATIVE_WEIGHTS {
        for (keyword, expected) in [("bolder", bolder), ("lighter", lighter)] {
            let inherited = format!("font-weight: {parent}");
            let relative = format!("font-weight: {keyword}");
            // The weight under `parent`, and the text under it inherits that number.
            check_weights(
                &mut wrong,
                &[inherited.as_str(), relative.as_str()],
                &[parent, expected, expected],
            );
        }
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn a_child_that_declares_a_weight_restarts_the_chain() {
    // bolder 700, then an explicit 300, then bolder against 300 (400), not against the chain.
    let mut wrong = Wrong::new();
    check_weights(
        &mut wrong,
        &[
            "font-weight: 400",
            "font-weight: bolder",
            "font-weight: 300",
            "font-weight: bolder",
        ],
        &[400.0, 700.0, 300.0, 400.0, 400.0],
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn the_child_of_a_bolder_node_inherits_the_number_not_the_keyword() {
    // The child of (bolder over 400) is 700: were it to inherit the keyword it would be 900.
    let mut wrong = Wrong::new();
    check_weights(
        &mut wrong,
        &["", "font-weight: bolder", "", ""],
        &[400.0, 700.0, 700.0, 700.0, 700.0],
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn bolder_on_the_root_elements_computes_against_the_initial_weight() {
    let mut wrong = Wrong::new();
    // html has no parent: `bolder` is taken against 400.
    for (css, expected) in [
        ("html { font-weight: bolder }", [700.0, 700.0, 700.0, 700.0]),
        (
            "html { font-weight: bolder } body { font-weight: bolder }",
            [700.0, 900.0, 900.0, 900.0],
        ),
        ("body { font-weight: bolder }", [400.0, 700.0, 700.0, 700.0]),
    ] {
        let s = styled(chain(1), css);
        // html 0, body 1, div 2, text 3
        let found = weights(&s, 0, 4);
        if found != numbers(&expected) {
            wrong.push(format!("`{css}`: html, body, div, text weigh {found:?}"));
        }
        if let Some(node) = node_holding_a_keyword(&s) {
            wrong.push(format!("`{css}`: node {node} holds a keyword"));
        }
    }
    assert_nothing_wrong(&wrong);
}

// ===================================================================
// The user-agent defaults land in the same slots
// ===================================================================

#[test]
fn the_ua_weights_and_styles_still_resolve_against_the_parent() {
    let mut wrong = Wrong::new();
    let mut check = |what: &str, s: &StyledDom, first: usize, expected: &[f32]| {
        let found = weights(s, first, expected.len());
        if found != numbers(expected) {
            wrong.push(format!("{what}: the weights are {found:?}, expected {expected:?}"));
        }
        if let Some(node) = node_holding_a_keyword(s) {
            wrong.push(format!("{what}: node {node} still holds a keyword"));
        }
    };

    // h1 is bold.
    let s = styled(page(Dom::create_node(NodeType::H1).with_child(text())), "");
    check("h1 and its text", &s, 2, &[700.0, 700.0]);

    // b in b: bolder over normal (700), then over bold (900); the text inherits the number.
    let b_in_b =
        Dom::create_node(NodeType::B).with_child(Dom::create_node(NodeType::B).with_child(text()));
    check("b in b", &styled(page(b_in_b), ""), 2, &[700.0, 900.0, 900.0]);

    // strong in b, and b in h1: the same step from 700.
    let strong_in_b = Dom::create_node(NodeType::B)
        .with_child(Dom::create_node(NodeType::Strong).with_child(text()));
    check(
        "strong in b",
        &styled(page(strong_in_b), ""),
        2,
        &[700.0, 900.0, 900.0],
    );
    let b_in_h1 = Dom::create_node(NodeType::H1)
        .with_child(Dom::create_node(NodeType::B).with_child(text()));
    check(
        "b in h1",
        &styled(page(b_in_h1), ""),
        2,
        &[700.0, 900.0, 900.0],
    );

    // An author rule beats the user agent's.
    let s = styled(
        page(Dom::create_node(NodeType::H1).with_child(text())),
        "h1 { font-weight: 300 }",
    );
    check("h1 with a rule", &s, 2, &[300.0, 300.0]);

    // em is italic, and so is the text in it.
    let s = styled(page(Dom::create_node(NodeType::Em).with_child(text())), "");
    let found = styles(&s, 2, 2);
    if found != [StyleFontStyle::Italic, StyleFontStyle::Italic] {
        wrong.push(format!("em: the styles are {found:?}"));
    }

    // The italic of em is the node's own rule: it replaces an oblique angle from above.
    let em_in_div = div("l0").with_child(Dom::create_node(NodeType::Em).with_child(text()));
    let s = styled(page(em_in_div), ".l0 { font-style: oblique 10deg }");
    let found = styles(&s, 2, 3);
    if found
        != [
            oblique(10.0),
            StyleFontStyle::Italic,
            StyleFontStyle::Italic,
        ]
    {
        wrong.push(format!("em under an oblique div: the styles are {found:?}"));
    }
    assert_nothing_wrong(&wrong);
}

// ===================================================================
// font-style: oblique <angle>
// ===================================================================

#[test]
fn an_oblique_angle_reaches_the_children_and_italic_or_normal_resets_it() {
    let obl = StyleFontStyle::OBLIQUE;
    let italic = StyleFontStyle::Italic;
    let normal = StyleFontStyle::Normal;
    let mut wrong = Wrong::new();

    // `oblique 10deg` on the node and on every node below it.
    check_styles(
        &mut wrong,
        &["font-style: oblique 10deg", ""],
        &[oblique(10.0), oblique(10.0), oblique(10.0)],
    );
    // `italic` on a child resets the angle, for the child and for its text.
    check_styles(
        &mut wrong,
        &["font-style: oblique 10deg", "font-style: italic"],
        &[oblique(10.0), italic, italic],
    );
    // `normal` clears it.
    check_styles(
        &mut wrong,
        &["font-style: oblique 10deg", "font-style: normal"],
        &[oblique(10.0), normal, normal],
    );
    // A bare `oblique` is 14deg, not the 10deg of the parent.
    check_styles(
        &mut wrong,
        &["font-style: oblique 10deg", "font-style: oblique"],
        &[oblique(10.0), obl, obl],
    );
    check_styles(&mut wrong, &["font-style: oblique", ""], &[obl, obl, obl]);
    // The other way: an oblique under an italic.
    check_styles(
        &mut wrong,
        &["font-style: italic", "font-style: oblique 20deg"],
        &[italic, oblique(20.0), oblique(20.0)],
    );
    // The two ends of the range, and a negative angle.
    check_styles(
        &mut wrong,
        &["font-style: oblique -10deg", ""],
        &[oblique(-10.0), oblique(-10.0), oblique(-10.0)],
    );
    check_styles(
        &mut wrong,
        &["font-style: oblique 90deg", "font-style: oblique -90deg"],
        &[oblique(90.0), oblique(-90.0), oblique(-90.0)],
    );
    // A fraction of a degree.
    check_styles(
        &mut wrong,
        &["font-style: oblique 10.5deg", ""],
        &[oblique(10.5), oblique(10.5), oblique(10.5)],
    );
    // `oblique 0deg` is an oblique, not `normal`.
    check_styles(
        &mut wrong,
        &["font-style: oblique 0deg", ""],
        &[oblique(0.0), oblique(0.0), oblique(0.0)],
    );
    // The plain styles.
    check_styles(
        &mut wrong,
        &["font-style: italic", ""],
        &[italic, italic, italic],
    );
    check_styles(
        &mut wrong,
        &["font-style: normal", ""],
        &[normal, normal, normal],
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn an_angle_does_not_come_back_after_an_italic_or_normal_in_between() {
    let mut wrong = Wrong::new();
    // oblique 10deg > italic > bare oblique: 14deg. The slot the italic left must not hand
    // the old 10deg on to the bare oblique.
    check_styles(
        &mut wrong,
        &[
            "font-style: oblique 10deg",
            "font-style: italic",
            "font-style: oblique",
        ],
        &[
            oblique(10.0),
            StyleFontStyle::Italic,
            StyleFontStyle::OBLIQUE,
            StyleFontStyle::OBLIQUE,
        ],
    );
    // oblique 10deg > normal > oblique 20deg.
    check_styles(
        &mut wrong,
        &[
            "font-style: oblique 10deg",
            "font-style: normal",
            "font-style: oblique 20deg",
        ],
        &[
            oblique(10.0),
            StyleFontStyle::Normal,
            oblique(20.0),
            oblique(20.0),
        ],
    );
    assert_nothing_wrong(&wrong);
}

// ===================================================================
// font-stretch
// ===================================================================

#[test]
fn font_stretch_is_inherited_as_the_declared_percentage() {
    let mut wrong = Wrong::new();
    check_stretches(&mut wrong, &["font-stretch: 87.5%", ""], &[87.5, 87.5, 87.5]);
    check_stretches(
        &mut wrong,
        &["font-stretch: condensed", ""],
        &[75.0, 75.0, 75.0],
    );
    // A child `font-stretch: normal` resets to 100%, for its text as well.
    check_stretches(
        &mut wrong,
        &["font-stretch: condensed", "font-stretch: normal"],
        &[75.0, 100.0, 100.0],
    );
    check_stretches(
        &mut wrong,
        &["font-stretch: semi-condensed", "font-stretch: expanded"],
        &[87.5, 125.0, 125.0],
    );
    check_stretches(
        &mut wrong,
        &["font-stretch: 50%", "font-stretch: 200%"],
        &[50.0, 200.0, 200.0],
    );
    check_stretches(
        &mut wrong,
        &["font-stretch: 62.5%", "font-stretch: 112.5%"],
        &[62.5, 112.5, 112.5],
    );
    // A declared 100% is the initial value.
    check_stretches(
        &mut wrong,
        &["font-stretch: 100%", ""],
        &[100.0, 100.0, 100.0],
    );
    // Only the child declares: the parent keeps the initial 100%.
    check_stretches(
        &mut wrong,
        &["", "font-stretch: 87.5%"],
        &[100.0, 87.5, 87.5],
    );
    // A stretch above the 1000% of an ordinary face still fits the slot.
    check_stretches(
        &mut wrong,
        &["font-stretch: 1000%", ""],
        &[1000.0, 1000.0, 1000.0],
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn the_font_stretch_keywords_are_the_css_fonts_4_percentages() {
    let keywords: [(&str, f32); 9] = [
        ("ultra-condensed", 50.0),
        ("extra-condensed", 62.5),
        ("condensed", 75.0),
        ("semi-condensed", 87.5),
        ("normal", 100.0),
        ("semi-expanded", 112.5),
        ("expanded", 125.0),
        ("extra-expanded", 150.0),
        ("ultra-expanded", 200.0),
    ];
    let mut wrong = Wrong::new();
    for (keyword, percent) in keywords {
        let decl = format!("font-stretch: {keyword}");
        check_stretches(&mut wrong, &[decl.as_str()], &[percent, percent]);
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn font_width_is_another_name_of_font_stretch() {
    let mut wrong = Wrong::new();
    check_stretches(&mut wrong, &["font-width: 87.5%", ""], &[87.5, 87.5, 87.5]);
    assert_nothing_wrong(&wrong);
}

#[test]
fn a_declared_zero_percent_stretch_is_not_the_undeclared_one() {
    let s = styled(chain(1), ".l0 { font-stretch: 0% }");
    for percent in stretches(&s, 2, 2) {
        assert!(
            percent < 1.0,
            "0% read back as {percent}%: the slot says 100% (undeclared)"
        );
    }
}

#[test]
fn a_stretch_between_two_64ths_is_kept_to_a_64th_by_the_cascade() {
    let s = styled(chain(1), ".l0 { font-stretch: 33.3% }");
    for percent in stretches(&s, 2, 2) {
        assert!((percent - 33.3).abs() < 0.02, "33.3% came back as {percent}%");
    }
}

// ===================================================================
// The `font` shorthand
// ===================================================================

#[test]
fn the_font_shorthand_sets_weight_style_and_stretch_and_the_text_inherits_them() {
    let s = styled(
        chain(1),
        ".l0 { font: italic 600 condensed 14px/1.4 Arial }",
    );
    let cache = compact(&s);
    let mut wrong = Wrong::new();
    // div.l0 is node 2, its text 3
    for node in [2, 3] {
        if cache.get_font_weight(node) != weight(600.0) {
            wrong.push(format!("node {node}: weight {:?}", cache.get_font_weight(node)));
        }
        if cache.get_font_style(node) != StyleFontStyle::Italic {
            wrong.push(format!("node {node}: style {:?}", cache.get_font_style(node)));
        }
        if cache.get_font_stretch(node).percent() != 75.0 {
            wrong.push(format!(
                "node {node}: stretch {}%",
                cache.get_font_stretch(node).percent()
            ));
        }
        // The rest of the shorthand arrives too.
        if font_size_px(&s, node) != Some(14.0) {
            wrong.push(format!("node {node}: size {:?}", font_size_px(&s, node)));
        }
        if cache.get_line_height(node) != CompactLineHeight::Factor(1.4) {
            wrong.push(format!(
                "node {node}: line height {:?}",
                cache.get_line_height(node)
            ));
        }
        if cache.get_font_family_hash(node) == 0 {
            wrong.push(format!("node {node}: no font family"));
        }
    }
    // html and body are untouched.
    if weights(&s, 0, 2) != numbers(&[400.0, 400.0]) {
        wrong.push(format!("html and body: weights {:?}", weights(&s, 0, 2)));
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn the_font_shorthand_resets_the_longhands_it_omits() {
    let mut wrong = Wrong::new();
    // `font: 14px Arial` on a child of a bold div resets the weight to 400.
    check_weights_css(
        &mut wrong,
        ".l0 { font-weight: bold } .l1 { font: 14px Arial }",
        2,
        &[700.0, 400.0, 400.0],
    );

    // The style, the stretch and the line height are reset the same way.
    let css = ".l0 { font-weight: 800; font-style: italic; font-stretch: condensed; \
               line-height: 2 } .l1 { font: 14px Arial }";
    let s = styled(chain(2), css);
    // div.l0 is node 2, div.l1 node 3, the text 4
    let cache = compact(&s);
    if weights(&s, 2, 3) != numbers(&[800.0, 400.0, 400.0]) {
        wrong.push(format!("weights {:?}", weights(&s, 2, 3)));
    }
    if styles(&s, 2, 3)
        != [
            StyleFontStyle::Italic,
            StyleFontStyle::Normal,
            StyleFontStyle::Normal,
        ]
    {
        wrong.push(format!("styles {:?}", styles(&s, 2, 3)));
    }
    if stretches(&s, 2, 3) != [75.0, 100.0, 100.0] {
        wrong.push(format!("stretches {:?}%", stretches(&s, 2, 3)));
    }
    if cache.get_line_height(2) != CompactLineHeight::Factor(2.0) {
        wrong.push(format!("div.l0 line height {:?}", cache.get_line_height(2)));
    }
    for node in [3, 4] {
        if cache.get_line_height(node) != CompactLineHeight::Normal {
            wrong.push(format!(
                "node {node}: line height {:?}, the shorthand resets it",
                cache.get_line_height(node)
            ));
        }
        // What the shorthand does give.
        if font_size_px(&s, node) != Some(14.0) {
            wrong.push(format!("node {node}: size {:?}", font_size_px(&s, node)));
        }
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn the_font_shorthand_takes_an_oblique_angle_and_a_numeric_weight() {
    let mut wrong = Wrong::new();
    check_styles(
        &mut wrong,
        &["font: oblique 10deg bold 14px Arial"],
        &[oblique(10.0), oblique(10.0)],
    );
    check_weights(
        &mut wrong,
        &["font: oblique 10deg bold 14px Arial"],
        &[700.0, 700.0],
    );
    check_weights(&mut wrong, &["font: 350 14px sans-serif"], &[350.0, 350.0]);
    check_stretches(&mut wrong, &["font: 350 14px sans-serif"], &[100.0, 100.0]);
    check_weights(&mut wrong, &["font: bold 12pt serif"], &[700.0, 700.0]);
    assert_nothing_wrong(&wrong);
}

#[test]
fn a_longhand_after_the_font_shorthand_wins_and_one_before_it_is_reset() {
    let mut wrong = Wrong::new();
    // The shorthand first: the longhand that follows wins.
    check_weights(
        &mut wrong,
        &["font: 14px Arial; font-weight: bold"],
        &[700.0, 700.0],
    );
    // The longhand first: the shorthand resets it.
    check_weights(
        &mut wrong,
        &["font-weight: bold; font: 14px Arial"],
        &[400.0, 400.0],
    );
    assert_nothing_wrong(&wrong);
}

// ===================================================================
// The DOM flags of the rarely-declared font properties
// ===================================================================

#[test]
fn a_dom_that_declares_none_of_the_rare_font_properties_raises_none_of_their_flags() {
    let css = ".a { font-weight: bold; font-style: oblique 10deg; font-stretch: condensed; \
               font-size: 14px; font-family: Arial; line-height: 1.4; \
               font-variant-caps: small-caps; font-variant-numeric: tabular-nums; \
               font-feature-settings: \"liga\" 1 }";
    let s = styled(page(div("a").with_child(text())), css);
    assert_eq!(new_flags(&s), 0, "bits 32..=37 are clear");
    // and nothing above them is ever set
    assert_eq!(compact(&s).dom_declared_flags >> 38, 0, "no flag past bit 37");

    // A DOM with no CSS at all.
    let s = styled(page(div("a").with_child(text())), "");
    assert_eq!(compact(&s).dom_declared_flags >> 32, 0);
}

#[test]
fn each_rare_font_property_raises_its_own_flag_only_for_a_non_initial_value() {
    let cases: &[(&str, u64)] = &[
        // font-variation-settings: `normal` is the initial value
        (
            r#".a { font-variation-settings: "wght" 700 }"#,
            DOM_HAS_FONT_VARIATION_SETTINGS,
        ),
        (
            r#".a { font-variation-settings: "wght" 700, "wdth" 87.5 }"#,
            DOM_HAS_FONT_VARIATION_SETTINGS,
        ),
        (".a { font-variation-settings: normal }", 0),
        // font-optical-sizing: `auto` is the initial value
        (".a { font-optical-sizing: none }", DOM_HAS_FONT_OPTICAL_SIZING),
        (".a { font-optical-sizing: auto }", 0),
        // font-kerning: `auto` is the initial value, `normal` is not
        (".a { font-kerning: none }", DOM_HAS_FONT_KERNING),
        (".a { font-kerning: normal }", DOM_HAS_FONT_KERNING),
        (".a { font-kerning: auto }", 0),
        // font-variant-ligatures: `normal` is the initial value
        (
            ".a { font-variant-ligatures: none }",
            DOM_HAS_FONT_VARIANT_LIGATURES,
        ),
        (
            ".a { font-variant-ligatures: no-common-ligatures }",
            DOM_HAS_FONT_VARIANT_LIGATURES,
        ),
        (
            ".a { font-variant-ligatures: discretionary-ligatures historical-ligatures }",
            DOM_HAS_FONT_VARIANT_LIGATURES,
        ),
        (".a { font-variant-ligatures: normal }", 0),
        // font-variant-east-asian: `normal` is the initial value
        (
            ".a { font-variant-east-asian: jis78 }",
            DOM_HAS_FONT_VARIANT_EAST_ASIAN,
        ),
        (
            ".a { font-variant-east-asian: ruby }",
            DOM_HAS_FONT_VARIANT_EAST_ASIAN,
        ),
        (".a { font-variant-east-asian: normal }", 0),
        // font-synthesis: ONE flag for the four longhands, `auto` is the initial value
        (".a { font-synthesis: none }", DOM_HAS_FONT_SYNTHESIS),
        (".a { font-synthesis-weight: none }", DOM_HAS_FONT_SYNTHESIS),
        (".a { font-synthesis-style: none }", DOM_HAS_FONT_SYNTHESIS),
        (
            ".a { font-synthesis-small-caps: none }",
            DOM_HAS_FONT_SYNTHESIS,
        ),
        (
            ".a { font-synthesis-position: none }",
            DOM_HAS_FONT_SYNTHESIS,
        ),
        (".a { font-synthesis-weight: auto }", 0),
        (".a { font-synthesis: weight style small-caps position }", 0),
        (".a { font-synthesis: weight style }", DOM_HAS_FONT_SYNTHESIS),
        // the font-variant shorthand sets the ligatures and the east-asian longhands too
        (".a { font-variant: none }", DOM_HAS_FONT_VARIANT_LIGATURES),
        (".a { font-variant: normal }", 0),
        (".a { font-variant: jis78 }", DOM_HAS_FONT_VARIANT_EAST_ASIAN),
        (".a { font-variant: small-caps }", 0),
        // the font shorthand resets all of them with `initial`, which is no value to flag
        (".a { font: italic 600 condensed 14px/1.4 Arial }", 0),
        // several in one DOM
        (
            ".a { font-kerning: none; font-optical-sizing: none }",
            DOM_HAS_FONT_KERNING | DOM_HAS_FONT_OPTICAL_SIZING,
        ),
    ];
    let mut wrong = Wrong::new();
    for &(css, expected) in cases {
        let got = raised(css);
        if got != expected {
            wrong.push(format!("`{css}` raised {got:#x}, expected {expected:#x}"));
        }
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn every_way_to_declare_a_rare_font_property_raises_its_flag() {
    let with_text = |dom: Dom| dom.with_child(text());

    // a stylesheet rule
    let rule = styled(page(with_text(div("a"))), ".a { font-kerning: none }");
    // a `*` rule
    let star = styled(page(with_text(div("a"))), "* { font-kerning: none }");
    // the node's own style
    let inline = styled(
        page(with_text(div("a").with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_kerning(
                StyleFontKerning::None,
            ))]
            .into(),
        ))),
        "",
    );
    // `Dom::with_css`, scoped to the subtree
    let scoped = styled(
        page(with_text(div("a").with_css("font-kerning: none"))),
        "",
    );
    let mut wrong = Wrong::new();
    for (way, s) in [
        ("rule", rule),
        ("star rule", star),
        ("inline style", inline),
        ("scoped with_css", scoped),
    ] {
        if new_flags(&s) != DOM_HAS_FONT_KERNING {
            wrong.push(format!("{way}: raised {:#x}", new_flags(&s)));
        }
    }

    // An inline `initial` value raises nothing, however it is declared.
    let inline_initial = styled(
        page(with_text(div("a").with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_kerning(
                StyleFontKerning::Auto,
            ))]
            .into(),
        ))),
        "",
    );
    if new_flags(&inline_initial) != 0 {
        wrong.push(format!(
            "inline `font-kerning: auto`: raised {:#x}",
            new_flags(&inline_initial)
        ));
    }
    assert_nothing_wrong(&wrong);
}

// ===================================================================
// The typed cascade getters
// ===================================================================

#[test]
fn the_typed_getters_find_the_declaration_of_the_declaring_node() {
    // `none` and `auto` of the properties that have them are tested on their own below.
    let css = r#".l0 {
        font-weight: 350.5;
        font-style: oblique 10deg;
        font-stretch: condensed;
        font-variation-settings: "wght" 700, "wdth" 87.5;
        font-kerning: normal;
        font-variant-ligatures: no-common-ligatures discretionary-ligatures;
        font-variant-east-asian: jis78 full-width ruby;
    }"#;
    let s = styled(chain(1), css);
    // html 0, body 1, div.l0 2
    let mut wrong = Wrong::new();

    // The typed value is as declared: the 64ths of the compact slot are not in it.
    check_declared(
        &mut wrong,
        "font-weight",
        declared!(s, 2, get_font_weight),
        weight(350.5),
    );
    check_declared(
        &mut wrong,
        "font-style",
        declared!(s, 2, get_font_style),
        oblique(10.0),
    );
    check_declared(
        &mut wrong,
        "font-stretch",
        declared!(s, 2, get_font_stretch),
        stretch(75.0),
    );
    check_declared(
        &mut wrong,
        "font-kerning",
        declared!(s, 2, get_font_kerning),
        StyleFontKerning::Normal,
    );
    check_declared(
        &mut wrong,
        "font-variant-ligatures",
        declared!(s, 2, get_font_variant_ligatures),
        StyleFontVariantLigatures {
            common: StyleFontLigatureChoice::Off,
            discretionary: StyleFontLigatureChoice::On,
            ..StyleFontVariantLigatures::NORMAL
        },
    );
    check_declared(
        &mut wrong,
        "font-variant-east-asian",
        declared!(s, 2, get_font_variant_east_asian),
        StyleFontVariantEastAsian {
            jis78: true,
            full_width: true,
            ruby: true,
            ..StyleFontVariantEastAsian::NORMAL
        },
    );
    match declared!(s, 2, get_font_variation_settings) {
        Some(CssPropertyValue::Exact(settings)) => {
            if settings.is_normal()
                || settings.variations() != vec![(*b"wght", 700.0), (*b"wdth", 87.5)]
            {
                wrong.push(format!(
                    "font-variation-settings: the axes are {:?}",
                    settings.variations()
                ));
            }
        }
        other => wrong.push(format!("font-variation-settings: the getter found {other:?}")),
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn none_and_auto_are_values_of_their_own_for_the_font_properties_that_have_them() {
    // `font-kerning: none` is `StyleFontKerning::None`, not the generic "no value" keyword:
    // read back as `CssPropertyValue::None` it would be taken for nothing declared.
    let none = "
        .l0 {
            font-kerning: none;
            font-optical-sizing: none;
            font-variant-ligatures: none;
            font-synthesis-weight: none;
            font-synthesis-style: none;
            font-synthesis-small-caps: none;
            font-synthesis-position: none;
        }";
    let s = styled(chain(1), none);
    let mut wrong = Wrong::new();
    check_declared(
        &mut wrong,
        "font-kerning: none",
        declared!(s, 2, get_font_kerning),
        StyleFontKerning::None,
    );
    check_declared(
        &mut wrong,
        "font-optical-sizing: none",
        declared!(s, 2, get_font_optical_sizing),
        StyleFontOpticalSizing::None,
    );
    check_declared(
        &mut wrong,
        "font-variant-ligatures: none",
        declared!(s, 2, get_font_variant_ligatures),
        StyleFontVariantLigatures::NONE,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-weight: none",
        declared!(s, 2, get_font_synthesis_weight),
        StyleFontSynthesis::None,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-style: none",
        declared!(s, 2, get_font_synthesis_style),
        StyleFontSynthesis::None,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-small-caps: none",
        declared!(s, 2, get_font_synthesis_small_caps),
        StyleFontSynthesis::None,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-position: none",
        declared!(s, 2, get_font_synthesis_position),
        StyleFontSynthesis::None,
    );

    // The same for `auto`.
    let auto = "
        .l0 {
            font-kerning: auto;
            font-optical-sizing: auto;
            font-synthesis-weight: auto;
            font-synthesis-style: auto;
            font-synthesis-small-caps: auto;
            font-synthesis-position: auto;
        }";
    let s = styled(chain(1), auto);
    check_declared(
        &mut wrong,
        "font-kerning: auto",
        declared!(s, 2, get_font_kerning),
        StyleFontKerning::Auto,
    );
    check_declared(
        &mut wrong,
        "font-optical-sizing: auto",
        declared!(s, 2, get_font_optical_sizing),
        StyleFontOpticalSizing::Auto,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-weight: auto",
        declared!(s, 2, get_font_synthesis_weight),
        StyleFontSynthesis::Auto,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-style: auto",
        declared!(s, 2, get_font_synthesis_style),
        StyleFontSynthesis::Auto,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-small-caps: auto",
        declared!(s, 2, get_font_synthesis_small_caps),
        StyleFontSynthesis::Auto,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-position: auto",
        declared!(s, 2, get_font_synthesis_position),
        StyleFontSynthesis::Auto,
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn the_font_synthesis_shorthand_declares_the_four_longhands() {
    let s = styled(chain(1), ".l0 { font-synthesis: weight small-caps }");
    let mut wrong = Wrong::new();
    check_declared(
        &mut wrong,
        "font-synthesis-weight",
        declared!(s, 2, get_font_synthesis_weight),
        StyleFontSynthesis::Auto,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-style",
        declared!(s, 2, get_font_synthesis_style),
        StyleFontSynthesis::None,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-small-caps",
        declared!(s, 2, get_font_synthesis_small_caps),
        StyleFontSynthesis::Auto,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-position",
        declared!(s, 2, get_font_synthesis_position),
        StyleFontSynthesis::None,
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn the_font_variant_shorthand_declares_its_four_longhands() {
    let s = styled(
        chain(1),
        ".l0 { font-variant: small-caps no-common-ligatures jis78 }",
    );
    let mut wrong = Wrong::new();
    check_declared(
        &mut wrong,
        "font-variant-caps",
        declared!(s, 2, get_font_variant_caps),
        StyleFontVariantCaps::SmallCaps,
    );
    check_declared(
        &mut wrong,
        "font-variant-numeric",
        declared!(s, 2, get_font_variant_numeric),
        StyleFontVariantNumeric::NORMAL,
    );
    check_declared(
        &mut wrong,
        "font-variant-ligatures",
        declared!(s, 2, get_font_variant_ligatures),
        StyleFontVariantLigatures {
            common: StyleFontLigatureChoice::Off,
            ..StyleFontVariantLigatures::NORMAL
        },
    );
    check_declared(
        &mut wrong,
        "font-variant-east-asian",
        declared!(s, 2, get_font_variant_east_asian),
        StyleFontVariantEastAsian {
            jis78: true,
            ..StyleFontVariantEastAsian::NORMAL
        },
    );

    // `none` is `font-variant-ligatures: none` with the other three normal.
    let s = styled(chain(1), ".l0 { font-variant: none }");
    check_declared(
        &mut wrong,
        "font-variant: none (ligatures)",
        declared!(s, 2, get_font_variant_ligatures),
        StyleFontVariantLigatures::NONE,
    );
    check_declared(
        &mut wrong,
        "font-variant: none (east-asian)",
        declared!(s, 2, get_font_variant_east_asian),
        StyleFontVariantEastAsian::NORMAL,
    );
    check_declared(
        &mut wrong,
        "font-variant: none (caps)",
        declared!(s, 2, get_font_variant_caps),
        StyleFontVariantCaps::Normal,
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn the_typed_stretch_keeps_what_was_declared_beyond_the_slot() {
    // The slot keeps a 64th of a percent; the typed value is the declaration itself.
    let s = styled(chain(1), ".l0 { font-stretch: 33.3% }");
    let mut wrong = Wrong::new();
    check_declared(
        &mut wrong,
        "font-stretch: 33.3%",
        declared!(s, 2, get_font_stretch),
        stretch(33.3),
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn inline_typed_declarations_are_found_by_the_typed_getters() {
    let settings = StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0)]);
    let dom = div("a")
        .with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::font_stretch(stretch(62.5))),
                CssPropertyWithConditions::simple(CssProperty::font_variation_settings(
                    settings.clone(),
                )),
                CssPropertyWithConditions::simple(CssProperty::font_optical_sizing(
                    StyleFontOpticalSizing::None,
                )),
                CssPropertyWithConditions::simple(CssProperty::font_kerning(
                    StyleFontKerning::Normal,
                )),
                CssPropertyWithConditions::simple(CssProperty::font_variant_ligatures(
                    StyleFontVariantLigatures::NONE,
                )),
                CssPropertyWithConditions::simple(CssProperty::font_variant_east_asian(
                    StyleFontVariantEastAsian {
                        ruby: true,
                        ..StyleFontVariantEastAsian::NORMAL
                    },
                )),
                CssPropertyWithConditions::simple(CssProperty::font_synthesis_weight(
                    StyleFontSynthesis::None,
                )),
                CssPropertyWithConditions::simple(CssProperty::font_synthesis_style(
                    StyleFontSynthesis::None,
                )),
                CssPropertyWithConditions::simple(CssProperty::font_synthesis_small_caps(
                    StyleFontSynthesis::None,
                )),
                CssPropertyWithConditions::simple(CssProperty::font_synthesis_position(
                    StyleFontSynthesis::None,
                )),
            ]
            .into(),
        )
        .with_child(text());
    let s = styled(page(dom), "");
    // html 0, body 1, div.a 2
    let mut wrong = Wrong::new();

    check_declared(
        &mut wrong,
        "font-stretch",
        declared!(s, 2, get_font_stretch),
        stretch(62.5),
    );
    check_declared(
        &mut wrong,
        "font-variation-settings",
        declared!(s, 2, get_font_variation_settings),
        settings,
    );
    check_declared(
        &mut wrong,
        "font-optical-sizing",
        declared!(s, 2, get_font_optical_sizing),
        StyleFontOpticalSizing::None,
    );
    check_declared(
        &mut wrong,
        "font-kerning",
        declared!(s, 2, get_font_kerning),
        StyleFontKerning::Normal,
    );
    check_declared(
        &mut wrong,
        "font-variant-ligatures",
        declared!(s, 2, get_font_variant_ligatures),
        StyleFontVariantLigatures::NONE,
    );
    check_declared(
        &mut wrong,
        "font-variant-east-asian",
        declared!(s, 2, get_font_variant_east_asian),
        StyleFontVariantEastAsian {
            ruby: true,
            ..StyleFontVariantEastAsian::NORMAL
        },
    );
    check_declared(
        &mut wrong,
        "font-synthesis-weight",
        declared!(s, 2, get_font_synthesis_weight),
        StyleFontSynthesis::None,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-style",
        declared!(s, 2, get_font_synthesis_style),
        StyleFontSynthesis::None,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-small-caps",
        declared!(s, 2, get_font_synthesis_small_caps),
        StyleFontSynthesis::None,
    );
    check_declared(
        &mut wrong,
        "font-synthesis-position",
        declared!(s, 2, get_font_synthesis_position),
        StyleFontSynthesis::None,
    );

    // Every one of the six declares a value other than its initial one (`normal` kerning is
    // not the initial `auto`), so all six flags are up.
    if new_flags(&s) != NEW_FONT_FLAGS {
        wrong.push(format!(
            "the flags raised are {:#x}, expected {NEW_FONT_FLAGS:#x}",
            new_flags(&s)
        ));
    }
    assert_nothing_wrong(&wrong);
}

// ===================================================================
// CSS-wide keywords
// ===================================================================

#[test]
fn a_css_wide_keyword_resolves_for_font_stretch() {
    let mut wrong = Wrong::new();
    // `inherit` takes the parent's value, not the one a type rule would give the node (`.l1`
    // outranks `div`).
    check_stretches_css(
        &mut wrong,
        "div { font-stretch: 62.5% } .l0 { font-stretch: 87.5% } .l1 { font-stretch: inherit }",
        2,
        &[87.5, 87.5, 87.5],
    );
    // `initial` is 100%, for the node and for the text under it.
    check_stretches_css(
        &mut wrong,
        ".l0 { font-stretch: 87.5% } .l1 { font-stretch: initial }",
        2,
        &[87.5, 100.0, 100.0],
    );
    // `unset` on an inherited property is `inherit`.
    check_stretches_css(
        &mut wrong,
        "div { font-stretch: 62.5% } .l0 { font-stretch: 87.5% } .l1 { font-stretch: unset }",
        2,
        &[87.5, 87.5, 87.5],
    );
    // `inherit` below a parent that declares nothing is the initial 100%.
    check_stretches_css(
        &mut wrong,
        ".l1 { font-stretch: inherit }",
        2,
        &[100.0, 100.0, 100.0],
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn a_css_wide_keyword_resolves_for_font_weight() {
    let mut wrong = Wrong::new();
    // `inherit` takes the parent's number, not the 800 a type rule would give the node.
    check_weights_css(
        &mut wrong,
        "div { font-weight: 800 } .l0 { font-weight: 350 } .l1 { font-weight: inherit }",
        2,
        &[350.0, 350.0, 350.0],
    );
    // ... and when the parent's weight is `bolder`, it is the NUMBER it computed to (700),
    // not the keyword applied once more (900).
    check_weights_css(
        &mut wrong,
        "div { font-weight: 800 } .l0 { font-weight: bolder } .l1 { font-weight: inherit }",
        2,
        &[700.0, 700.0, 700.0],
    );
    check_weights_css(
        &mut wrong,
        ".l0 { font-weight: 800 } .l1 { font-weight: initial }",
        2,
        &[800.0, 400.0, 400.0],
    );
    check_weights_css(
        &mut wrong,
        "div { font-weight: 300 } .l0 { font-weight: 800 } .l1 { font-weight: unset }",
        2,
        &[800.0, 800.0, 800.0],
    );
    assert_nothing_wrong(&wrong);
}

#[test]
fn a_css_wide_keyword_resolves_for_font_style() {
    let mut wrong = Wrong::new();
    // `inherit` takes the parent's angle too.
    check_styles_css(
        &mut wrong,
        "div { font-style: italic } .l0 { font-style: oblique 10deg } .l1 { font-style: inherit }",
        2,
        &[oblique(10.0), oblique(10.0), oblique(10.0)],
    );
    // `initial` is `normal`, and the angle goes with it.
    check_styles_css(
        &mut wrong,
        ".l0 { font-style: oblique 10deg } .l1 { font-style: initial }",
        2,
        &[
            oblique(10.0),
            StyleFontStyle::Normal,
            StyleFontStyle::Normal,
        ],
    );
    assert_nothing_wrong(&wrong);
}

// ===================================================================
// Other ways to declare: `*` rules, the node's own style, a runtime override
// ===================================================================

#[test]
fn a_star_rule_sets_the_font_slots_of_every_element_and_the_text_inherits() {
    let s = styled(
        page(div("a").with_child(text())),
        "* { font-weight: 350; font-stretch: 87.5%; font-style: oblique 10deg }",
    );
    // html 0, body 1, div 2, text 3: the elements get it from the rule, the text from its parent
    let mut wrong = Wrong::new();
    if weights(&s, 0, 4) != numbers(&[350.0; 4]) {
        wrong.push(format!("weights {:?}", weights(&s, 0, 4)));
    }
    if stretches(&s, 0, 4) != [87.5; 4] {
        wrong.push(format!("stretches {:?}%", stretches(&s, 0, 4)));
    }
    if styles(&s, 0, 4) != [oblique(10.0); 4] {
        wrong.push(format!("styles {:?}", styles(&s, 0, 4)));
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn inline_declarations_reach_the_font_slots_like_stylesheet_rules() {
    let inner = div("inner")
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_weight(
                StyleFontWeight::Bolder,
            ))]
            .into(),
        )
        .with_child(text());
    let outer = div("outer")
        .with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::font_weight(
                    StyleFontWeight::Bolder,
                )),
                CssPropertyWithConditions::simple(CssProperty::font_stretch(stretch(62.5))),
                CssPropertyWithConditions::simple(CssProperty::font_style(oblique(12.5))),
            ]
            .into(),
        )
        .with_child(inner);
    let s = styled(page(outer), "");
    // html 0, body 1, div.outer 2, div.inner 3, text 4
    let mut wrong = Wrong::new();
    // bolder over normal, then bolder over that
    if weights(&s, 2, 3) != numbers(&[700.0, 900.0, 900.0]) {
        wrong.push(format!("weights {:?}", weights(&s, 2, 3)));
    }
    if stretches(&s, 2, 3) != [62.5, 62.5, 62.5] {
        wrong.push(format!("stretches {:?}%", stretches(&s, 2, 3)));
    }
    if styles(&s, 2, 3) != [oblique(12.5); 3] {
        wrong.push(format!("styles {:?}", styles(&s, 2, 3)));
    }
    if let Some(node) = node_holding_a_keyword(&s) {
        wrong.push(format!("node {node} still holds a keyword"));
    }
    assert_nothing_wrong(&wrong);
}

#[test]
fn a_runtime_override_reaches_the_font_slots_of_the_node_and_its_descendants() {
    let mut s = styled(chain(2), ".l0 { font-weight: 300 }");
    // html 0, body 1, div.l0 2, div.l1 3, text 4
    let node = NodeId::new(2);
    let mut wrong = Wrong::new();

    // What an animation of a variable font passes through: weights between the hundreds, a
    // stretch and a slant of a fraction.
    let _ = s.restyle_user_property(
        &node,
        &[
            CssProperty::font_weight(weight(512.5)),
            CssProperty::font_stretch(stretch(81.25)),
            CssProperty::font_style(oblique(7.5)),
        ],
    );
    if weights(&s, 2, 3) != numbers(&[512.5; 3]) {
        wrong.push(format!("weights {:?}", weights(&s, 2, 3)));
    }
    if stretches(&s, 2, 3) != [81.25; 3] {
        wrong.push(format!("stretches {:?}%", stretches(&s, 2, 3)));
    }
    if styles(&s, 2, 3) != [oblique(7.5); 3] {
        wrong.push(format!("styles {:?}", styles(&s, 2, 3)));
    }

    // `bolder` as an override computes against the parent (body, 400), and the children copy
    // the number.
    let _ = s.restyle_user_property(&node, &[CssProperty::font_weight(StyleFontWeight::Bolder)]);
    if weights(&s, 2, 3) != numbers(&[700.0; 3]) {
        wrong.push(format!("after `bolder`: weights {:?}", weights(&s, 2, 3)));
    }
    if let Some(bad) = node_holding_a_keyword(&s) {
        wrong.push(format!("after `bolder`: node {bad} still holds a keyword"));
    }
    assert_nothing_wrong(&wrong);
}
