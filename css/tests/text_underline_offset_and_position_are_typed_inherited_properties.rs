//! `text-underline-offset` and `text-underline-position` are typed, inherited properties: `auto`,
//! a length or a percentage are the values of the first; `auto`, or `from-font` or `under` and
//! `left` or `right`, each group once and in either order, those of the second. `auto` is a typed
//! value of both (not the generic keyword, which could not end the value a parent set), and a name
//! finds its type.

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::PixelValue,
        formatter::PrintAsCssValue,
        property::{get_css_key_map, parse_css_property, CssProperty, CssPropertyType},
        style::text::{StyleTextUnderlineOffset, StyleTextUnderlinePosition},
    },
};

fn parsed(ty: CssPropertyType, value: &str) -> CssProperty {
    parse_css_property(ty, value).unwrap_or_else(|e| panic!("{ty:?}: {value:?}: {e:?}"))
}

fn offset(value: &str) -> StyleTextUnderlineOffset {
    match parsed(CssPropertyType::TextUnderlineOffset, value) {
        CssProperty::TextUnderlineOffset(CssPropertyValue::Exact(o)) => o,
        other => panic!("{value:?} is no offset value: {other:?}"),
    }
}

fn position(value: &str) -> StyleTextUnderlinePosition {
    match parsed(CssPropertyType::TextUnderlinePosition, value) {
        CssProperty::TextUnderlinePosition(CssPropertyValue::Exact(p)) => p,
        other => panic!("{value:?} is no position value: {other:?}"),
    }
}

#[test]
fn the_offset_is_auto_a_length_or_a_percentage() {
    assert_eq!(offset("auto"), StyleTextUnderlineOffset::Auto);
    assert_eq!(offset(" AUTO "), StyleTextUnderlineOffset::Auto);
    assert_eq!(StyleTextUnderlineOffset::default(), StyleTextUnderlineOffset::Auto);
    assert_eq!(offset("3px"), StyleTextUnderlineOffset::Length(PixelValue::px(3.0)));
    assert_eq!(offset("-2px"), StyleTextUnderlineOffset::Length(PixelValue::px(-2.0)));
    assert_eq!(offset("0.25em"), StyleTextUnderlineOffset::Length(PixelValue::em(0.25)));
    assert_eq!(offset("50%"), StyleTextUnderlineOffset::Length(PixelValue::percent(50.0)));
    assert_eq!(offset("0"), StyleTextUnderlineOffset::Length(PixelValue::px(0.0)));
}

#[test]
fn the_position_is_auto_or_the_keywords_of_its_two_groups_in_either_order() {
    assert_eq!(StyleTextUnderlinePosition::default(), StyleTextUnderlinePosition::Auto);
    for (input, value) in [
        ("auto", StyleTextUnderlinePosition::Auto),
        ("AUTO", StyleTextUnderlinePosition::Auto),
        ("from-font", StyleTextUnderlinePosition::FromFont),
        ("under", StyleTextUnderlinePosition::Under),
        ("UNDER", StyleTextUnderlinePosition::Under),
        ("left", StyleTextUnderlinePosition::Left),
        ("right", StyleTextUnderlinePosition::Right),
        ("from-font left", StyleTextUnderlinePosition::FromFontLeft),
        ("left from-font", StyleTextUnderlinePosition::FromFontLeft),
        ("from-font right", StyleTextUnderlinePosition::FromFontRight),
        ("right from-font", StyleTextUnderlinePosition::FromFontRight),
        ("under left", StyleTextUnderlinePosition::UnderLeft),
        ("left under", StyleTextUnderlinePosition::UnderLeft),
        ("  Under   Right ", StyleTextUnderlinePosition::UnderRight),
        ("right under", StyleTextUnderlinePosition::UnderRight),
    ] {
        assert_eq!(position(input), value, "{input:?}");
    }
}

/// `auto` is a value of both properties, not the generic keyword: read as the generic one it would
/// be "no value", and a box could not end the offset of its parent with it.
#[test]
fn auto_is_a_typed_value_of_both() {
    assert!(matches!(
        parsed(CssPropertyType::TextUnderlineOffset, "auto"),
        CssProperty::TextUnderlineOffset(CssPropertyValue::Exact(StyleTextUnderlineOffset::Auto))
    ));
    assert!(matches!(
        parsed(CssPropertyType::TextUnderlinePosition, "auto"),
        CssProperty::TextUnderlinePosition(CssPropertyValue::Exact(
            StyleTextUnderlinePosition::Auto
        ))
    ));
}

#[test]
fn what_is_no_offset_and_no_position_is_no_value() {
    for bad in ["", "thick", "1px 2px", "auto auto", "under"] {
        assert!(parse_css_property(CssPropertyType::TextUnderlineOffset, bad).is_err(), "{bad:?}");
    }
    for bad in [
        "",
        "above",
        "under under",
        "left right",
        "from-font under",
        "under from-font",
        "left left",
        "auto left",
        "under auto",
        "under, left",
        "none",
    ] {
        assert!(
            parse_css_property(CssPropertyType::TextUnderlinePosition, bad).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn the_names_find_the_types_and_both_are_inherited() {
    let map = get_css_key_map();
    for (name, ty) in [
        ("text-underline-offset", CssPropertyType::TextUnderlineOffset),
        ("text-underline-position", CssPropertyType::TextUnderlinePosition),
    ] {
        assert_eq!(CssPropertyType::from_str(name, &map), Some(ty), "{name}");
        assert_eq!(ty.to_str(), name);
        assert!(ty.is_inheritable(), "{ty:?}");
    }
}

#[test]
fn the_values_print_as_the_css_they_were() {
    for (value, _) in StyleTextUnderlinePosition::ALL {
        let printed = value.print_as_css_value();
        assert_eq!(position(&printed), value, "{printed}");
    }
    for value in ["auto", "3px", "-0.5em", "50%"] {
        let o = offset(value);
        assert_eq!(offset(&o.print_as_css_value()), o, "{value}");
    }
}
