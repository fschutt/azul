//! `text-wrap`, `text-rendering` and `font-variant-position` are typed, inherited properties:
//! each keyword is its value, a name finds its type, `auto` of `text-rendering` is its own
//! initial value (not the generic keyword, which an inherited `optimizeSpeed` would survive), and
//! a keyword of another property is no value.

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::font_variation::{StyleFontVariantPosition, StyleTextRendering, StyleTextWrap},
        property::{get_css_key_map, parse_css_property, CssProperty, CssPropertyType},
    },
};

fn parsed(ty: CssPropertyType, value: &str) -> CssProperty {
    parse_css_property(ty, value).unwrap_or_else(|e| panic!("{ty:?}: {value:?}: {e:?}"))
}

#[test]
fn every_keyword_of_text_wrap_is_its_value() {
    for (keyword, value) in [
        ("wrap", StyleTextWrap::Wrap),
        ("nowrap", StyleTextWrap::Nowrap),
        ("balance", StyleTextWrap::Balance),
        ("stable", StyleTextWrap::Stable),
        ("pretty", StyleTextWrap::Pretty),
    ] {
        assert_eq!(
            parsed(CssPropertyType::TextWrap, keyword),
            CssProperty::TextWrap(CssPropertyValue::Exact(value)),
            "{keyword}"
        );
    }
}

#[test]
fn every_keyword_of_font_variant_position_is_its_value() {
    for (keyword, value) in [
        ("normal", StyleFontVariantPosition::Normal),
        ("sub", StyleFontVariantPosition::Sub),
        ("super", StyleFontVariantPosition::Super),
    ] {
        assert_eq!(
            parsed(CssPropertyType::FontVariantPosition, keyword),
            CssProperty::FontVariantPosition(CssPropertyValue::Exact(value)),
            "{keyword}"
        );
    }
}

#[test]
fn every_keyword_of_text_rendering_is_its_value_in_any_case() {
    for (keyword, value) in [
        ("auto", StyleTextRendering::Auto),
        ("optimizeSpeed", StyleTextRendering::OptimizeSpeed),
        ("optimizespeed", StyleTextRendering::OptimizeSpeed),
        ("optimizeLegibility", StyleTextRendering::OptimizeLegibility),
        ("geometricPrecision", StyleTextRendering::GeometricPrecision),
    ] {
        assert_eq!(
            parsed(CssPropertyType::TextRendering, keyword),
            CssProperty::TextRendering(CssPropertyValue::Exact(value)),
            "{keyword}"
        );
    }
}

/// `auto` is a value of `text-rendering`, not the generic keyword: read as the generic one it
/// would be "no value", and a child could not end the `optimizeSpeed` of its parent with it.
#[test]
fn text_rendering_auto_is_a_typed_value() {
    assert!(matches!(
        parsed(CssPropertyType::TextRendering, "auto"),
        CssProperty::TextRendering(CssPropertyValue::Exact(StyleTextRendering::Auto))
    ));
}

#[test]
fn the_names_find_the_types_and_the_types_print_their_names() {
    let map = get_css_key_map();
    for (name, ty) in [
        ("text-wrap", CssPropertyType::TextWrap),
        ("text-rendering", CssPropertyType::TextRendering),
        ("font-variant-position", CssPropertyType::FontVariantPosition),
    ] {
        assert_eq!(CssPropertyType::from_str(name, &map), Some(ty), "{name}");
        assert_eq!(ty.to_str(), name);
    }
}

#[test]
fn the_three_are_inherited() {
    for ty in [
        CssPropertyType::TextWrap,
        CssPropertyType::TextRendering,
        CssPropertyType::FontVariantPosition,
    ] {
        assert!(ty.is_inheritable(), "{ty:?}");
    }
}

#[test]
fn a_keyword_of_another_property_is_no_value() {
    for (ty, bad) in [
        (CssPropertyType::TextWrap, "auto"),
        (CssPropertyType::TextWrap, "none"),
        (CssPropertyType::TextWrap, "wrap balance"),
        (CssPropertyType::TextWrap, "normal"),
        (CssPropertyType::FontVariantPosition, "superscript"),
        (CssPropertyType::FontVariantPosition, "auto"),
        (CssPropertyType::TextRendering, "optimize-speed"),
        (CssPropertyType::TextRendering, "normal"),
    ] {
        assert!(parse_css_property(ty, bad).is_err(), "{ty:?}: {bad:?}");
    }
}
