//! `text-decoration-color` and `text-decoration-thickness` are typed properties that are not
//! inherited: `currentcolor` and a colour are the values of the first, `auto`, `from-font`, a
//! length and a percentage those of the second; `auto` is the typed initial value of the second
//! (not the generic keyword, which a stylesheet cannot tell from an unset property), a thickness
//! below nothing is no value, and a name finds its type.

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{ColorU, PixelValue},
        property::{get_css_key_map, parse_css_property, CssProperty, CssPropertyType},
        style::text::{StyleTextDecorationColor, StyleTextDecorationThickness},
    },
};

fn parsed(ty: CssPropertyType, value: &str) -> CssProperty {
    parse_css_property(ty, value).unwrap_or_else(|e| panic!("{ty:?}: {value:?}: {e:?}"))
}

fn colour(value: &str) -> StyleTextDecorationColor {
    match parsed(CssPropertyType::TextDecorationColor, value) {
        CssProperty::TextDecorationColor(CssPropertyValue::Exact(c)) => c,
        other => panic!("{value:?} is no colour value: {other:?}"),
    }
}

fn thickness(value: &str) -> StyleTextDecorationThickness {
    match parsed(CssPropertyType::TextDecorationThickness, value) {
        CssProperty::TextDecorationThickness(CssPropertyValue::Exact(t)) => t,
        other => panic!("{value:?} is no thickness value: {other:?}"),
    }
}

#[test]
fn the_colour_is_currentcolor_or_a_colour() {
    for keyword in ["currentcolor", "CURRENTCOLOR", "CurrentColor", "  currentcolor "] {
        assert_eq!(colour(keyword), StyleTextDecorationColor::CurrentColor, "{keyword:?}");
    }
    assert_eq!(StyleTextDecorationColor::default(), StyleTextDecorationColor::CurrentColor);
    assert_eq!(
        colour("#ff0000"),
        StyleTextDecorationColor::Color(ColorU { r: 255, g: 0, b: 0, a: 255 })
    );
    match colour("rgba(0, 128, 0, 0.5)") {
        StyleTextDecorationColor::Color(c) => {
            assert_eq!((c.r, c.g, c.b), (0, 128, 0));
            assert!((127..=128).contains(&c.a), "half transparent: {}", c.a);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(colour("transparent"), StyleTextDecorationColor::Color(ColorU::TRANSPARENT));
    // a theme colour stays the token the layout resolves
    assert!(matches!(colour("system:accent"), StyleTextDecorationColor::Color(_)));
}

#[test]
fn the_thickness_is_auto_from_font_a_length_or_a_percentage() {
    assert_eq!(thickness("auto"), StyleTextDecorationThickness::Auto);
    assert_eq!(thickness("AUTO"), StyleTextDecorationThickness::Auto);
    assert_eq!(thickness("from-font"), StyleTextDecorationThickness::FromFont);
    assert_eq!(thickness("From-Font"), StyleTextDecorationThickness::FromFont);
    assert_eq!(StyleTextDecorationThickness::default(), StyleTextDecorationThickness::Auto);
    assert_eq!(thickness("2px"), StyleTextDecorationThickness::Length(PixelValue::px(2.0)));
    assert_eq!(thickness("0.25em"), StyleTextDecorationThickness::Length(PixelValue::em(0.25)));
    assert_eq!(thickness("50%"), StyleTextDecorationThickness::Length(PixelValue::percent(50.0)));
    assert_eq!(thickness("0"), StyleTextDecorationThickness::Length(PixelValue::px(0.0)));
    assert_eq!(thickness("1.5pt"), StyleTextDecorationThickness::Length(PixelValue::pt(1.5)));
}

/// `auto` is a value of the thickness, not the generic keyword: read as the generic one it would
/// be "no value".
#[test]
fn the_thickness_auto_is_a_typed_value() {
    assert!(matches!(
        parsed(CssPropertyType::TextDecorationThickness, "auto"),
        CssProperty::TextDecorationThickness(CssPropertyValue::Exact(
            StyleTextDecorationThickness::Auto
        ))
    ));
}

#[test]
fn the_generic_keywords_are_read_as_such() {
    for ty in [CssPropertyType::TextDecorationColor, CssPropertyType::TextDecorationThickness] {
        for generic in ["inherit", "initial", "unset"] {
            let property = parsed(ty, generic);
            let kept = match &property {
                CssProperty::TextDecorationColor(v) => v.css_wide_keyword().is_some(),
                CssProperty::TextDecorationThickness(v) => v.css_wide_keyword().is_some(),
                other => panic!("{other:?}"),
            };
            assert!(kept, "{ty:?}: {generic}: {property:?}");
        }
    }
}

#[test]
fn what_is_no_colour_and_no_thickness_is_no_value() {
    for bad in ["", "notacolor", "red blue", "#12", "currentcolor red", "none"] {
        assert!(parse_css_property(CssPropertyType::TextDecorationColor, bad).is_err(), "{bad:?}");
    }
    for bad in ["", "thick", "-1px", "-0.5em", "auto auto", "1px 2px", "none", "from font"] {
        assert!(
            parse_css_property(CssPropertyType::TextDecorationThickness, bad).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn the_names_find_the_types_and_the_types_print_their_names() {
    let map = get_css_key_map();
    for (name, ty) in [
        ("text-decoration-color", CssPropertyType::TextDecorationColor),
        ("text-decoration-thickness", CssPropertyType::TextDecorationThickness),
    ] {
        assert_eq!(CssPropertyType::from_str(name, &map), Some(ty), "{name}");
        assert_eq!(ty.to_str(), name);
    }
}

/// Neither is inherited: a decoration is drawn with the properties of the box that sets it
/// (CSS Text Decoration 3, 2.1), and a box inside is not decorated by what its parent set.
#[test]
fn neither_is_inherited() {
    for ty in [CssPropertyType::TextDecorationColor, CssPropertyType::TextDecorationThickness] {
        assert!(!ty.is_inheritable(), "{ty:?}");
    }
}

#[test]
fn the_values_print_as_the_css_they_were() {
    use azul_css::props::formatter::PrintAsCssValue;
    for value in ["currentcolor", "#cc3300", "rgba(0, 128, 0, 0.5)"] {
        let c = colour(value);
        assert_eq!(colour(&c.print_as_css_value()), c, "{value}");
    }
    for value in ["auto", "from-font", "2px", "0.25em", "50%"] {
        let t = thickness(value);
        assert_eq!(thickness(&t.print_as_css_value()), t, "{value}");
    }
}
