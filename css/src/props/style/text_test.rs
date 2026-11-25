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
