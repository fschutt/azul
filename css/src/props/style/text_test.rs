//! `normal` as a length and `none` as a value: `letter-spacing: normal` and `word-spacing: normal`
//! are the zero length, `text-justify: none` is the typed value that turns the justification off.
//!
//! The two spacings parsed a length only, so `normal` was a declaration that failed: dropped, and
//! the box kept the spacing it inherited. And `none` of `text-justify` was read as the generic
//! keyword `none`, which a property reads back as "no value": the initial `auto`, which justifies.

#[cfg(feature = "parser")]
use super::*;
#[cfg(feature = "parser")]
use crate::{
    css::CssPropertyValue,
    props::{
        layout::text::LayoutTextJustify,
        property::{parse_css_property, CssProperty, CssPropertyType},
    },
};

#[cfg(feature = "parser")]
#[test]
fn normal_is_no_letter_spacing_and_no_word_spacing() {
    for input in ["normal", " NORMAL ", "Normal"] {
        let letters = parse_style_letter_spacing(input).unwrap();
        assert_eq!(letters.inner.number.get(), 0.0, "{input:?}");
        let words = parse_style_word_spacing(input).unwrap();
        assert_eq!(words.inner.number.get(), 0.0, "{input:?}");
    }
    // a length still is one, a word that is no length still is no value
    assert_eq!(parse_style_letter_spacing("3px").unwrap().inner.number.get(), 3.0);
    assert!(parse_style_letter_spacing("normally").is_err());
    assert!(parse_style_word_spacing("").is_err());
}

#[cfg(feature = "parser")]
#[test]
fn the_spacings_print_back_as_the_zero_length() {
    let letters = parse_style_letter_spacing("normal").unwrap();
    assert_eq!(parse_style_letter_spacing(&letters.print_as_css_value()).unwrap(), letters);
}

#[cfg(feature = "parser")]
#[test]
fn text_justify_none_is_a_typed_value() {
    let property = parse_css_property(CssPropertyType::TextJustify, "none").unwrap();
    assert_eq!(
        property,
        CssProperty::TextJustify(CssPropertyValue::Exact(LayoutTextJustify::None)),
        "`none` turns the justification off: a value, not the generic keyword"
    );
    let property = parse_css_property(CssPropertyType::TextJustify, "inter-word").unwrap();
    assert_eq!(
        property,
        CssProperty::TextJustify(CssPropertyValue::Exact(LayoutTextJustify::InterWord))
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_decoration_colour_is_currentcolor_or_a_colour_and_prints_as_either() {
    for input in ["currentcolor", " CurrentColor ", "CURRENTCOLOR"] {
        assert_eq!(
            parse_style_text_decoration_color(input).unwrap(),
            StyleTextDecorationColor::CurrentColor,
            "{input:?}"
        );
    }
    assert_eq!(StyleTextDecorationColor::default(), StyleTextDecorationColor::CurrentColor);
    let red = parse_style_text_decoration_color("#ff0000").unwrap();
    assert_eq!(red, StyleTextDecorationColor::Color(ColorU::new_rgb(255, 0, 0)));
    for value in [StyleTextDecorationColor::CurrentColor, red] {
        let printed = value.print_as_css_value();
        assert_eq!(parse_style_text_decoration_color(&printed).unwrap(), value, "{printed}");
    }
    for bad in ["", "current color", "notacolor", "#12", "red blue"] {
        assert!(parse_style_text_decoration_color(bad).is_err(), "{bad:?}");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_decoration_thickness_is_auto_from_font_a_length_or_a_percentage_and_never_negative() {
    assert_eq!(
        parse_style_text_decoration_thickness(" AUTO ").unwrap(),
        StyleTextDecorationThickness::Auto
    );
    assert_eq!(
        parse_style_text_decoration_thickness("From-Font").unwrap(),
        StyleTextDecorationThickness::FromFont
    );
    assert_eq!(StyleTextDecorationThickness::default(), StyleTextDecorationThickness::Auto);
    for (input, length) in [
        ("3px", PixelValue::px(3.0)),
        ("0.1em", PixelValue::em(0.1)),
        ("12.5%", PixelValue::percent(12.5)),
        ("0", PixelValue::px(0.0)),
        ("-0", PixelValue::px(-0.0)),
    ] {
        assert_eq!(
            parse_style_text_decoration_thickness(input).unwrap(),
            StyleTextDecorationThickness::Length(length),
            "{input:?}"
        );
    }
    for value in [
        StyleTextDecorationThickness::Auto,
        StyleTextDecorationThickness::FromFont,
        StyleTextDecorationThickness::Length(PixelValue::px(2.0)),
        StyleTextDecorationThickness::Length(PixelValue::percent(50.0)),
    ] {
        let printed = value.print_as_css_value();
        assert_eq!(parse_style_text_decoration_thickness(&printed).unwrap(), value, "{printed}");
    }
    for bad in ["", "thick", "-1px", "-5%", "auto auto", "1px 2px", "from font"] {
        assert!(parse_style_text_decoration_thickness(bad).is_err(), "{bad:?}");
    }
}
