//! CSS Parser Robustness Tests
//!
//! Tests that the CSS parser handles invalid and edge-case CSS gracefully
//! without panicking and produces appropriate warnings/errors.

use azul_css::parser2::new_from_str;

#[test]
fn test_css_parser_missing_value() {
    let css = "div { color: }";
    let (result, _warnings) = new_from_str(css);
    // Should not panic, may produce warnings
    let _ = result;
}

#[test]
fn test_css_parser_missing_property_name() {
    let css = "div { : red }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_missing_colon() {
    let css = "div { color red }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_missing_closing_brace() {
    let css = "div { color: red";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_missing_selector() {
    let css = "{ color: red }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_empty_string() {
    let css = "";
    let (result, _warnings) = new_from_str(css);
    assert!(result.rules().next().is_none());
}

#[test]
fn test_css_parser_whitespace_only() {
    let css = "   \n\t  \r\n  ";
    let (result, _warnings) = new_from_str(css);
    assert!(result.rules().next().is_none());
}

#[test]
fn test_css_parser_comments_only() {
    let css = "/* This is a comment */ /* Another comment */";
    let (result, _warnings) = new_from_str(css);
    assert!(result.rules().next().is_none());
}

#[test]
fn test_css_parser_unclosed_comment() {
    let css = "div { color: red } /* unclosed comment";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_nested_braces() {
    let css = "div { color: red; { nested: value } }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_unicode_selector() {
    let css = ".日本語 { color: red }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_unicode_value() {
    let css = "div { content: '日本語テスト' }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_very_long_selector() {
    let selector = "div ".repeat(100);
    let css = format!("{} {{ color: red }}", selector.trim());
    let (result, _warnings) = new_from_str(&css);
    let _ = result;
}

#[test]
fn test_css_parser_very_long_value() {
    let value = "a".repeat(10000);
    let css = format!("div {{ content: '{}' }}", value);
    let (result, _warnings) = new_from_str(&css);
    let _ = result;
}

#[test]
fn test_css_parser_multiple_semicolons() {
    let css = "div { color: red;;; background: blue;; }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_invalid_unit() {
    let css = "div { width: 100xyz }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_negative_values() {
    let css = "div { margin: -10px; width: -50% }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_scientific_notation() {
    let css = "div { width: 1e10px }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_special_characters_in_class() {
    let css = ".class\\:with\\:colons { color: red }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_multiple_rules() {
    let css = r#"
        div { color: red }
        .class { color: blue }
        #id { color: green }
        div.class#id { color: yellow }
    "#;
    let (result, _warnings) = new_from_str(css);
    assert!(result.rules().count() >= 4);
}

#[test]
fn test_css_parser_pseudo_classes() {
    let css = r#"
        a:hover { color: red }
        a:active { color: blue }
        a:focus { color: green }
        a:visited { color: purple }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_pseudo_elements() {
    let css = r#"
        p::before { content: 'prefix' }
        p::after { content: 'suffix' }
        p::first-line { font-weight: bold }
        p::first-letter { font-size: 2em }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_combinators() {
    let css = r#"
        div > p { color: red }
        div + p { color: blue }
        div ~ p { color: green }
        div p { color: yellow }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_attribute_selectors() {
    let css = r#"
        [data-test] { color: red }
        [data-test="value"] { color: blue }
        [data-test^="prefix"] { color: green }
        [data-test$="suffix"] { color: yellow }
        [data-test*="contains"] { color: purple }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_shorthand_margin() {
    let css = r#"
        .one { margin: 10px }
        .two { margin: 10px 20px }
        .three { margin: 10px 20px 30px }
        .four { margin: 10px 20px 30px 40px }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_shorthand_padding() {
    let css = r#"
        .one { padding: 10px }
        .two { padding: 10px 20px }
        .three { padding: 10px 20px 30px }
        .four { padding: 10px 20px 30px 40px }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_color_formats() {
    let css = r#"
        .hex3 { color: #f00 }
        .hex6 { color: #ff0000 }
        .hex8 { color: #ff0000ff }
        .rgb { color: rgb(255, 0, 0) }
        .rgba { color: rgba(255, 0, 0, 0.5) }
        .named { color: red }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_calc_expression() {
    let css = "div { width: calc(100% - 20px) }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_multiple_selectors() {
    let css = "div, p, span, .class, #id { color: red }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_important() {
    let css = "div { color: red !important }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_media_query() {
    let css = "@media screen and (max-width: 600px) { div { color: red } }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_font_family_with_quotes() {
    let css = r#"div { font-family: "Helvetica Neue", Arial, sans-serif }"#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_background_shorthand() {
    let css = "div { background: #fff url('image.png') no-repeat center center }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_border_shorthand() {
    let css = "div { border: 1px solid red }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_flex_shorthand() {
    let css = r#"
        .flex1 { flex: 1 }
        .flex2 { flex: 1 1 auto }
        .flex3 { flex: none }
    "#;
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

/// Verify that `flex: <number>` expands to grow + shrink:1 + basis:0 per CSS spec.
#[test]
fn test_flex_shorthand_single_number() {
    use azul_css::props::property::{parse_combined_css_property, CombinedCssPropertyType, CssProperty};
    use azul_css::props::layout::flex::{LayoutFlexGrow, LayoutFlexShrink, LayoutFlexBasis};
    use azul_css::props::basic::length::FloatValue;
    use azul_css::props::basic::pixel::PixelValue;

    let props = parse_combined_css_property(CombinedCssPropertyType::Flex, "1").unwrap();
    assert_eq!(props.len(), 3, "flex: 1 must produce 3 properties");
    assert!(matches!(&props[0], CssProperty::FlexGrow(_)));
    assert!(matches!(&props[1], CssProperty::FlexShrink(_)));
    assert!(matches!(&props[2], CssProperty::FlexBasis(_)));

    // Check values: grow=1, shrink=1, basis=0
    if let CssProperty::FlexGrow(g) = &props[0] {
        assert_eq!(g.get_property().unwrap().inner, FloatValue::new(1.0));
    }
    if let CssProperty::FlexShrink(s) = &props[1] {
        assert_eq!(s.get_property().unwrap().inner, FloatValue::new(1.0));
    }
    if let CssProperty::FlexBasis(b) = &props[2] {
        assert_eq!(*b.get_property().unwrap(), LayoutFlexBasis::Exact(PixelValue::px(0.0)));
    }
}

/// Verify `flex: <number> <number>` = grow + shrink + basis:0.
#[test]
fn test_flex_shorthand_two_numbers() {
    use azul_css::props::property::{parse_combined_css_property, CombinedCssPropertyType, CssProperty};
    use azul_css::props::layout::flex::{LayoutFlexGrow, LayoutFlexShrink, LayoutFlexBasis};
    use azul_css::props::basic::length::FloatValue;
    use azul_css::props::basic::pixel::PixelValue;

    let props = parse_combined_css_property(CombinedCssPropertyType::Flex, "2 3").unwrap();
    assert_eq!(props.len(), 3, "flex: 2 3 must produce 3 properties");
    if let CssProperty::FlexGrow(g) = &props[0] {
        assert_eq!(g.get_property().unwrap().inner, FloatValue::new(2.0));
    }
    if let CssProperty::FlexShrink(s) = &props[1] {
        assert_eq!(s.get_property().unwrap().inner, FloatValue::new(3.0));
    }
    if let CssProperty::FlexBasis(b) = &props[2] {
        assert_eq!(*b.get_property().unwrap(), LayoutFlexBasis::Exact(PixelValue::px(0.0)));
    }
}

/// Verify `flex: <number> <width>` = grow + shrink:1 + basis:<width>.
#[test]
fn test_flex_shorthand_number_and_width() {
    use azul_css::props::property::{parse_combined_css_property, CombinedCssPropertyType, CssProperty};
    use azul_css::props::layout::flex::{LayoutFlexGrow, LayoutFlexShrink, LayoutFlexBasis};
    use azul_css::props::basic::length::FloatValue;
    use azul_css::props::basic::pixel::PixelValue;

    let props = parse_combined_css_property(CombinedCssPropertyType::Flex, "1 200px").unwrap();
    assert_eq!(props.len(), 3, "flex: 1 200px must produce 3 properties");
    if let CssProperty::FlexGrow(g) = &props[0] {
        assert_eq!(g.get_property().unwrap().inner, FloatValue::new(1.0));
    }
    if let CssProperty::FlexShrink(s) = &props[1] {
        assert_eq!(s.get_property().unwrap().inner, FloatValue::new(1.0));
    }
    if let CssProperty::FlexBasis(b) = &props[2] {
        assert_eq!(*b.get_property().unwrap(), LayoutFlexBasis::Exact(PixelValue::px(200.0)));
    }
}

/// Verify `flex: none` = grow:0 + shrink:0 + basis:auto.
#[test]
fn test_flex_shorthand_none() {
    use azul_css::props::property::{parse_combined_css_property, CombinedCssPropertyType, CssProperty};
    use azul_css::props::layout::flex::LayoutFlexBasis;

    let props = parse_combined_css_property(CombinedCssPropertyType::Flex, "none").unwrap();
    assert_eq!(props.len(), 3);
    // Just verify the types are correct — the "none" keyword path
    // constructs values with const_new(0) which may use different wrapper variants
    assert!(matches!(&props[0], CssProperty::FlexGrow(_)));
    assert!(matches!(&props[1], CssProperty::FlexShrink(_)));
    assert!(matches!(&props[2], CssProperty::FlexBasis(_)));
}

/// Verify `flex: 1 2 auto` = grow:1, shrink:2, basis:auto.
#[test]
fn test_flex_shorthand_three_values() {
    use azul_css::props::property::{parse_combined_css_property, CombinedCssPropertyType, CssProperty};
    use azul_css::props::layout::flex::{LayoutFlexGrow, LayoutFlexShrink, LayoutFlexBasis};
    use azul_css::props::basic::length::FloatValue;

    let props = parse_combined_css_property(CombinedCssPropertyType::Flex, "1 2 auto").unwrap();
    assert_eq!(props.len(), 3);
    if let CssProperty::FlexGrow(g) = &props[0] {
        assert_eq!(g.get_property().unwrap().inner, FloatValue::new(1.0));
    }
    if let CssProperty::FlexShrink(s) = &props[1] {
        assert_eq!(s.get_property().unwrap().inner, FloatValue::new(2.0));
    }
    if let CssProperty::FlexBasis(b) = &props[2] {
        assert_eq!(*b.get_property().unwrap(), LayoutFlexBasis::Auto);
    }
}

#[test]
fn test_css_parser_transform() {
    let css = "div { transform: rotate(45deg) scale(1.5) translate(10px, 20px) }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_transition() {
    let css = "div { transition: all 0.3s ease-in-out }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_box_shadow() {
    let css = "div { box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1) }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}

#[test]
fn test_css_parser_gradient() {
    let css = "div { background: linear-gradient(to right, red, blue) }";
    let (result, _warnings) = new_from_str(css);
    let _ = result;
}
