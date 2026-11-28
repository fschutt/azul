//! `scrollbar-width` through the property parser: `auto`, `thin` and `none` are the three typed
//! values of the property, not the CSS-wide keywords. `none` written as CSS text used to come back
//! as the keyword `CssPropertyValue::None`, which reads back as "no value": the node kept the UA's
//! scrollbar and a tab strip, a text field or a panel that said `scrollbar-width: none` still drew
//! one while it scrolled.

use super::*;
use crate::props::style::scrollbar::LayoutScrollbarWidth;

fn parsed(value: &str) -> CssProperty {
    parse_css_property(CssPropertyType::ScrollbarWidth, value).expect("a scrollbar-width value")
}

#[test]
fn none_auto_and_thin_are_the_typed_values() {
    for (text, typed) in [
        ("none", LayoutScrollbarWidth::None),
        ("auto", LayoutScrollbarWidth::Auto),
        ("thin", LayoutScrollbarWidth::Thin),
    ] {
        assert_eq!(
            parsed(text),
            CssProperty::ScrollbarWidth(CssPropertyValue::Exact(typed)),
            "scrollbar-width: {text}"
        );
    }
}

#[test]
fn the_typed_none_reads_back_as_a_value() {
    // The readers ask `get_property()`: the keyword has none, the typed value has one.
    let CssProperty::ScrollbarWidth(value) = parsed("none") else {
        panic!("a scrollbar-width property");
    };
    assert_eq!(value.get_property(), Some(&LayoutScrollbarWidth::None));
}

#[test]
fn the_css_wide_keywords_stay_the_keywords() {
    assert_eq!(
        parsed("initial"),
        CssProperty::initial(CssPropertyType::ScrollbarWidth)
    );
    assert_eq!(
        parsed("inherit"),
        CssProperty::inherit(CssPropertyType::ScrollbarWidth)
    );
}

#[test]
fn a_value_that_is_none_of_them_is_refused() {
    assert!(parse_css_property(CssPropertyType::ScrollbarWidth, "wide").is_err());
}
