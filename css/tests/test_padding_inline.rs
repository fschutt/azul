//! Test für padding-inline-start und padding-inline-end Properties

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{pixel::PixelValue, SizeMetric},
        layout::spacing::{LayoutPaddingInlineEnd, LayoutPaddingInlineStart},
        property::{CssProperty, CssPropertyType},
    },
};

#[test]
fn test_padding_inline_start_const() {
    let padding = LayoutPaddingInlineStart::const_px(40);
    assert_eq!(padding.inner.metric, SizeMetric::Px);
    assert_eq!(padding.inner.number.get(), 40.0);
}

#[test]
fn test_padding_inline_end_const() {
    let padding = LayoutPaddingInlineEnd::const_px(20);
    assert_eq!(padding.inner.metric, SizeMetric::Px);
    assert_eq!(padding.inner.number.get(), 20.0);
}

#[test]
fn test_padding_inline_start_runtime() {
    let padding = LayoutPaddingInlineStart::px(25.5);
    assert_eq!(padding.inner.metric, SizeMetric::Px);
    assert_eq!(padding.inner.number.get(), 25.5);
}

#[test]
fn test_padding_inline_end_em() {
    let padding = LayoutPaddingInlineEnd::em(2.0);
    assert_eq!(padding.inner.metric, SizeMetric::Em);
    assert_eq!(padding.inner.number.get(), 2.0);
}

#[test]
fn test_padding_inline_start_percent() {
    let padding = LayoutPaddingInlineStart::percent(50.0);
    assert_eq!(padding.inner.metric, SizeMetric::Percent);
    assert_eq!(padding.inner.number.get(), 50.0);
}

#[cfg(feature = "parser")]
#[test]
fn test_parse_padding_inline_start() {
    use azul_css::props::layout::spacing::parse_layout_padding_inline_start;

    let result = parse_layout_padding_inline_start("40px").unwrap();
    assert_eq!(result.inner.metric, SizeMetric::Px);
    assert_eq!(result.inner.number.get(), 40.0);

    let result = parse_layout_padding_inline_start("2em").unwrap();
    assert_eq!(result.inner.metric, SizeMetric::Em);
    assert_eq!(result.inner.number.get(), 2.0);

    let result = parse_layout_padding_inline_start("50%").unwrap();
    assert_eq!(result.inner.metric, SizeMetric::Percent);
    assert_eq!(result.inner.number.get(), 50.0);
}

#[cfg(feature = "parser")]
#[test]
fn test_parse_padding_inline_end() {
    use azul_css::props::layout::spacing::parse_layout_padding_inline_end;

    let result = parse_layout_padding_inline_end("20px").unwrap();
    assert_eq!(result.inner.metric, SizeMetric::Px);
    assert_eq!(result.inner.number.get(), 20.0);

    let result = parse_layout_padding_inline_end("1.5em").unwrap();
    assert_eq!(result.inner.metric, SizeMetric::Em);
    assert_eq!(result.inner.number.get(), 1.5);
}

#[cfg(feature = "parser")]
#[test]
fn test_css_property_from_type() {
    let prop = CssProperty::PaddingInlineStart(CssPropertyValue::Exact(
        LayoutPaddingInlineStart::const_px(40),
    ));

    match prop {
        CssProperty::PaddingInlineStart(val) => {
            if let CssPropertyValue::Exact(padding) = val {
                assert_eq!(padding.inner.metric, SizeMetric::Px);
                assert_eq!(padding.inner.number.get(), 40.0);
            } else {
                panic!("Expected Exact value");
            }
        }
        _ => panic!("Expected PaddingInlineStart"),
    }
}

#[test]
fn test_property_type_enum() {
    // Verify the enum variants exist
    let _type1 = CssPropertyType::PaddingInlineStart;
    let _type2 = CssPropertyType::PaddingInlineEnd;

    // Verify they can be compared
    assert_ne!(_type1, _type2);
}

#[cfg(feature = "parser")]
#[test]
fn test_from_trait() {
    let padding = LayoutPaddingInlineStart::const_px(30);
    let prop: CssProperty = padding.into();

    match prop {
        CssProperty::PaddingInlineStart(val) => {
            if let CssPropertyValue::Exact(p) = val {
                assert_eq!(p.inner.metric, SizeMetric::Px);
                assert_eq!(p.inner.number.get(), 30.0);
            }
        }
        _ => panic!("Expected PaddingInlineStart"),
    }
}
