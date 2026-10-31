//! A functional value such as `rgba(16, 24, 40, 0.1)` is ONE component of a
//! space-separated CSS value, whatever whitespace sits inside its parentheses.
//!
//! Every parser here used to cut its input with `str::split_whitespace`, which
//! tore `rgba(16, 24, 40, 0.1)` into four tokens, so the component count came
//! out wrong and the whole declaration was dropped. One test per parser.

use azul_css::{
    css::{Css, CssDeclaration},
    props::{
        basic::color::parse_css_color,
        property::{parse_css_property, CssProperty, CssPropertyType},
        style::{
            border::{parse_style_border, parse_style_border_color},
            box_shadow::parse_style_box_shadow,
            scrollbar::{parse_style_scrollbar_color, ScrollbarColorCustom, StyleScrollbarColor},
        },
    },
};

#[test]
fn a_box_shadow_whose_colour_has_spaces_after_its_commas_parses() {
    let shadow = parse_style_box_shadow("0 2px 8px rgba(16, 24, 40, 0.1)")
        .expect("the spaced rgba() colour must be one shadow component");
    assert_eq!(
        shadow.color,
        parse_css_color("rgba(16, 24, 40, 0.1)").unwrap()
    );
    assert_eq!(
        shadow,
        parse_style_box_shadow("0 2px 8px rgba(16,24,40,0.1)").unwrap(),
        "the spaced and the compact spelling must parse to the same shadow"
    );
}

#[test]
fn a_box_shadow_declaration_whose_colour_has_spaces_is_kept_by_the_stylesheet() {
    let (css, warnings) = Css::from_string_with_warnings(
        ".card { box-shadow: 0 2px 8px rgba(16, 24, 40, 0.1); }"
            .to_string()
            .into(),
    );
    let shadow_sides = css
        .rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter())
        .filter(|decl| {
            matches!(
                decl,
                CssDeclaration::Static(
                    CssProperty::BoxShadowTop(_)
                        | CssProperty::BoxShadowRight(_)
                        | CssProperty::BoxShadowBottom(_)
                        | CssProperty::BoxShadowLeft(_)
                )
            )
        })
        .count();
    assert_eq!(
        shadow_sides, 4,
        "the box-shadow shorthand was dropped; warnings: {warnings:?}"
    );
}

#[test]
fn a_text_shadow_whose_colour_has_spaces_after_its_commas_parses() {
    let spaced = parse_css_property(CssPropertyType::TextShadow, "0 1px 2px rgba(0, 0, 0, 0.5)")
        .expect("text-shadow with a spaced rgba() colour must parse");
    let compact =
        parse_css_property(CssPropertyType::TextShadow, "0 1px 2px rgba(0,0,0,0.5)").unwrap();
    assert_eq!(spaced, compact);
}

#[test]
fn a_drop_shadow_filter_whose_colour_has_spaces_after_its_commas_parses() {
    let spaced = parse_css_property(
        CssPropertyType::Filter,
        "drop-shadow(0 2px 8px rgba(16, 24, 40, 0.1))",
    )
    .expect("drop-shadow() with a spaced rgba() colour must parse");
    let compact = parse_css_property(
        CssPropertyType::Filter,
        "drop-shadow(0 2px 8px rgba(16,24,40,0.1))",
    )
    .unwrap();
    assert_eq!(spaced, compact);
}

#[test]
fn a_border_shorthand_whose_colour_has_spaces_after_its_commas_parses() {
    let side = parse_style_border("1px solid rgba(0, 0, 0, 0.1)")
        .expect("the border shorthand with a spaced rgba() colour must parse");
    assert_eq!(
        side.border_color,
        parse_css_color("rgba(0, 0, 0, 0.1)").unwrap()
    );
    assert_eq!(side, parse_style_border("1px solid rgba(0,0,0,0.1)").unwrap());
}

#[test]
fn a_border_color_shorthand_splits_between_functional_colours_not_inside_them() {
    let colors = parse_style_border_color("rgb(255, 0, 0) rgba(0, 0, 255, 0.5)")
        .expect("two spaced functional colours are TWO border-color values");
    let red = parse_css_color("rgb(255, 0, 0)").unwrap();
    let blue = parse_css_color("rgba(0, 0, 255, 0.5)").unwrap();
    assert_eq!(colors.top, red);
    assert_eq!(colors.bottom, red);
    assert_eq!(colors.left, blue);
    assert_eq!(colors.right, blue);
}

#[test]
fn a_scrollbar_color_whose_colours_have_spaces_after_their_commas_parses() {
    let parsed = parse_style_scrollbar_color("rgb(255, 0, 0) rgba(0, 0, 0, 0.1)")
        .expect("two spaced functional colours are the thumb and the track");
    assert_eq!(
        parsed,
        StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: parse_css_color("rgb(255, 0, 0)").unwrap(),
            track: parse_css_color("rgba(0, 0, 0, 0.1)").unwrap(),
        })
    );
}
