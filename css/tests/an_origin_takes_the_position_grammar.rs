//! `transform-origin` and `perspective-origin` take a `<position>`: one or two
//! values, keywords in either order, a missing value is `center` - the same
//! grammar `background-position` already parses.
//!
//! `perspective-origin` took exactly two LENGTHS (`left top` was an error),
//! and `transform-origin` exactly two values in x-then-y order (`top left`
//! and a lone `left` were errors).

use azul_css::props::{
    basic::pixel::PixelValue,
    style::transform::{
        parse_style_perspective_origin, parse_style_transform_origin, StylePerspectiveOrigin,
        StyleTransformOrigin,
    },
};

fn perspective(x: PixelValue, y: PixelValue) -> StylePerspectiveOrigin {
    StylePerspectiveOrigin { x, y }
}

fn transform(x: PixelValue, y: PixelValue) -> StyleTransformOrigin {
    StyleTransformOrigin { x, y }
}

#[test]
fn perspective_origin_takes_position_keywords() {
    assert_eq!(
        parse_style_perspective_origin("left top"),
        Ok(perspective(PixelValue::percent(0.0), PixelValue::percent(0.0)))
    );
    assert_eq!(
        parse_style_perspective_origin("right bottom"),
        Ok(perspective(PixelValue::percent(100.0), PixelValue::percent(100.0)))
    );
    assert_eq!(
        parse_style_perspective_origin("center"),
        Ok(perspective(PixelValue::percent(50.0), PixelValue::percent(50.0)))
    );
}

#[test]
fn perspective_origin_still_takes_two_lengths() {
    assert_eq!(
        parse_style_perspective_origin("10px 20px"),
        Ok(perspective(PixelValue::px(10.0), PixelValue::px(20.0)))
    );
}

#[test]
fn a_vertical_keyword_may_come_first() {
    assert_eq!(
        parse_style_transform_origin("top left"),
        Ok(transform(PixelValue::percent(0.0), PixelValue::percent(0.0)))
    );
    assert_eq!(
        parse_style_perspective_origin("bottom right"),
        Ok(perspective(PixelValue::percent(100.0), PixelValue::percent(100.0)))
    );
}

#[test]
fn a_single_value_centres_the_other_axis() {
    assert_eq!(
        parse_style_transform_origin("left"),
        Ok(transform(PixelValue::percent(0.0), PixelValue::percent(50.0)))
    );
    assert_eq!(
        parse_style_transform_origin("top"),
        Ok(transform(PixelValue::percent(50.0), PixelValue::percent(0.0)))
    );
    assert_eq!(
        parse_style_perspective_origin("10px"),
        Ok(perspective(PixelValue::px(10.0), PixelValue::percent(50.0)))
    );
}

#[test]
fn garbage_and_too_many_values_are_still_rejected() {
    assert!(parse_style_perspective_origin("sideways up").is_err());
    assert!(parse_style_perspective_origin("1px 2px 3px 4px").is_err());
    assert!(parse_style_transform_origin("").is_err());
}
