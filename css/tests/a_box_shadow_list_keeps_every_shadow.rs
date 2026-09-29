//! `box-shadow` takes a comma-separated LIST of shadows - `0 1px 2px red,
//! 0 0 0 1px blue` is an elevation shadow plus a ring - and CSS paints the
//! FIRST one on top. The parser read one shadow only, so a list failed to
//! parse and the whole declaration was dropped.
//!
//! A node keeps its shadows in its four shadow slots
//! (`-azul-box-shadow-left/right/top/bottom`), and the painter paints each
//! distinct one once, in slot order, the last (bottom) on top. A list
//! therefore fills the slots from the bottom up, and up to four shadows fit.

use azul_css::{
    css::{Css, CssDeclaration},
    parser2::CssParseWarnMsgInnerOwned,
    props::{
        property::CssProperty,
        style::box_shadow::{parse_style_box_shadow, StyleBoxShadow},
    },
};

/// The shadows `.x { <declaration> }` puts into the four slots, as `[left,
/// right, top, bottom]` (`None` for a slot it leaves unset), and the
/// parser's warnings.
fn slots(declaration: &str) -> ([Option<StyleBoxShadow>; 4], Vec<CssParseWarnMsgInnerOwned>) {
    let (css, warnings) = Css::from_string_with_warnings(format!(".x {{ {declaration} }}").into());
    let mut slots = [None; 4];
    for decl in css
        .rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter())
    {
        let CssDeclaration::Static(property) = decl else {
            continue;
        };
        let (slot, value) = match property {
            CssProperty::BoxShadowLeft(v) => (0, v),
            CssProperty::BoxShadowRight(v) => (1, v),
            CssProperty::BoxShadowTop(v) => (2, v),
            CssProperty::BoxShadowBottom(v) => (3, v),
            _ => continue,
        };
        slots[slot] = value.get_property().map(|shadow| **shadow);
    }
    (slots, warnings.into_iter().map(|w| w.warning).collect())
}

fn shadow(css: &str) -> Option<StyleBoxShadow> {
    Some(parse_style_box_shadow(css).expect("premise: a valid single shadow"))
}

#[test]
fn a_list_of_two_shadows_is_kept_with_the_first_in_the_slot_painted_on_top() {
    let (got, warnings) = slots("box-shadow: 0 1px 2px red, 0 0 0 1px blue;");
    assert!(warnings.is_empty(), "the list must parse: {warnings:?}");
    let first = shadow("0 1px 2px red");
    let second = shadow("0 0 0 1px blue");
    // [left, right, top, bottom]: bottom paints last, so it holds the first
    // shadow; the slots the list leaves over repeat its last shadow, which
    // paints nothing twice.
    assert_eq!(got, [second, second, second, first]);
}

#[test]
fn four_shadows_take_one_slot_each_from_the_bottom_up() {
    let (got, warnings) = slots("box-shadow: 0 1px red, 0 2px green, 0 3px blue, 0 4px black;");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        got,
        [
            shadow("0 4px black"),
            shadow("0 3px blue"),
            shadow("0 2px green"),
            shadow("0 1px red"),
        ]
    );
}

#[test]
fn one_shadow_still_fills_all_four_slots() {
    let (got, warnings) = slots("box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);");
    assert!(warnings.is_empty(), "{warnings:?}");
    let only = shadow("0 1px 2px rgba(0, 0, 0, 0.5)");
    assert_eq!(got, [only, only, only, only]);
}

#[test]
fn a_list_longer_than_four_keeps_the_first_four_and_warns_once() {
    let (got, warnings) =
        slots("box-shadow: 0 1px red, 0 2px green, 0 3px blue, 0 4px black, 0 5px white;");
    assert_eq!(
        got,
        [
            shadow("0 4px black"),
            shadow("0 3px blue"),
            shadow("0 2px green"),
            shadow("0 1px red"),
        ],
        "the first four shadows are the four on top: they are the ones kept"
    );
    assert_eq!(
        warnings.len(),
        1,
        "dropping the fifth shadow must be said, once: {warnings:?}"
    );
    assert!(
        format!("{:?}", warnings[0]).contains("box-shadow"),
        "the warning names the declaration: {warnings:?}"
    );
}

#[test]
fn one_invalid_shadow_invalidates_the_whole_list() {
    let (got, warnings) = slots("box-shadow: 0 1px red, 1px;");
    assert_eq!(got, [None; 4], "CSS drops a list with an invalid shadow");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}
