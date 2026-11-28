//! The `text-decoration` shorthand (CSS Text Decoration 3, 2.4) is
//! `<'text-decoration-line'> || <'text-decoration-style'> || <'text-decoration-color'> ||
//! <'text-decoration-thickness'>`: `underline wavy red 2px` sets the four longhands, in any
//! order, and the ones a value does not name are reset to their initial value. The line part takes
//! any of `underline`, `overline` and `line-through` (`underline overline`), or `none`;
//! `text-decoration-line` is the longhand of it and `text-decoration-style` is a typed property
//! of its own (`solid`, `double`, `dotted`, `dashed`, `wavy`) that is not inherited.

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::ColorU,
        property::{
            get_css_key_map, parse_combined_css_property, parse_css_property,
            CombinedCssPropertyType, CssProperty, CssPropertyType,
        },
        style::text::{
            StyleTextDecoration, StyleTextDecorationColor, StyleTextDecorationStyle,
            StyleTextDecorationThickness,
        },
    },
};

fn px(value: f32) -> StyleTextDecorationThickness {
    StyleTextDecorationThickness::Length(azul_css::props::basic::PixelValue::px(value))
}

fn expand(value: &str) -> Vec<CssProperty> {
    parse_combined_css_property(CombinedCssPropertyType::TextDecoration, value)
        .unwrap_or_else(|e| panic!("`{value}` is a text-decoration: {e:?}"))
}

/// The four longhands as the shorthand lists them: line, style, colour, thickness. `None` is the
/// initial value, which the shorthand writes for a part it does not name.
fn parts(
    value: &str,
) -> (
    Option<StyleTextDecoration>,
    Option<StyleTextDecorationStyle>,
    Option<StyleTextDecorationColor>,
    Option<StyleTextDecorationThickness>,
) {
    let props = expand(value);
    assert_eq!(props.len(), 4, "`{value}`: four longhands: {props:?}");
    let line = match &props[0] {
        CssProperty::TextDecoration(CssPropertyValue::Exact(line)) => Some(*line),
        CssProperty::TextDecoration(CssPropertyValue::Initial) => None,
        other => panic!("`{value}`: the line: {other:?}"),
    };
    let style = match &props[1] {
        CssProperty::TextDecorationStyle(CssPropertyValue::Exact(style)) => Some(*style),
        CssProperty::TextDecorationStyle(CssPropertyValue::Initial) => None,
        other => panic!("`{value}`: the style: {other:?}"),
    };
    let color = match &props[2] {
        CssProperty::TextDecorationColor(CssPropertyValue::Exact(color)) => Some(*color),
        CssProperty::TextDecorationColor(CssPropertyValue::Initial) => None,
        other => panic!("`{value}`: the colour: {other:?}"),
    };
    let thickness = match &props[3] {
        CssProperty::TextDecorationThickness(CssPropertyValue::Exact(thickness)) => {
            Some(*thickness)
        }
        CssProperty::TextDecorationThickness(CssPropertyValue::Initial) => None,
        other => panic!("`{value}`: the thickness: {other:?}"),
    };
    (line, style, color, thickness)
}

const RED: StyleTextDecorationColor =
    StyleTextDecorationColor::Color(ColorU { r: 255, g: 0, b: 0, a: 255 });

#[test]
fn a_value_sets_the_four_longhands() {
    let (line, style, color, thickness) = parts("underline wavy red 2px");
    assert_eq!(line, Some(StyleTextDecoration::Underline));
    assert_eq!(style, Some(StyleTextDecorationStyle::Wavy));
    assert_eq!(color, Some(RED));
    assert_eq!(thickness, Some(px(2.0)));
}

#[test]
fn the_parts_come_in_any_order() {
    for value in [
        "2px red wavy underline",
        "wavy 2px underline red",
        "red underline 2px wavy",
        "UNDERLINE Wavy RED 2px",
    ] {
        assert_eq!(
            parts(value),
            (
                Some(StyleTextDecoration::Underline),
                Some(StyleTextDecorationStyle::Wavy),
                Some(RED),
                Some(px(2.0))
            ),
            "{value}"
        );
    }
}

/// A part a value does not name is reset: `text-decoration: underline` after a rule that set the
/// colour and the style ends them.
#[test]
fn a_part_that_is_not_named_is_reset_to_its_initial_value() {
    assert_eq!(parts("underline"), (Some(StyleTextDecoration::Underline), None, None, None));
    assert_eq!(parts("dotted"), (None, Some(StyleTextDecorationStyle::Dotted), None, None));
    assert_eq!(parts("red"), (None, None, Some(RED), None));
    assert_eq!(parts("3px"), (None, None, None, Some(px(3.0))));
    assert_eq!(
        parts("underline double"),
        (Some(StyleTextDecoration::Underline), Some(StyleTextDecorationStyle::Double), None, None)
    );
}

#[test]
fn the_line_part_is_none_or_any_of_the_three_lines() {
    for (value, line) in [
        ("none", StyleTextDecoration::None),
        ("underline", StyleTextDecoration::Underline),
        ("overline", StyleTextDecoration::Overline),
        ("line-through", StyleTextDecoration::LineThrough),
        ("underline overline", StyleTextDecoration::UnderlineOverline),
        ("overline underline", StyleTextDecoration::UnderlineOverline),
        ("underline line-through", StyleTextDecoration::UnderlineLineThrough),
        ("line-through underline", StyleTextDecoration::UnderlineLineThrough),
        ("overline line-through", StyleTextDecoration::OverlineLineThrough),
        ("underline overline line-through", StyleTextDecoration::UnderlineOverlineLineThrough),
        ("line-through overline underline", StyleTextDecoration::UnderlineOverlineLineThrough),
    ] {
        assert_eq!(parts(value).0, Some(line), "{value}");
    }
    // `blink` is a line a UA may leave undrawn: read, and it draws nothing
    assert_eq!(parts("blink").0, Some(StyleTextDecoration::None));
    assert_eq!(parts("underline blink").0, Some(StyleTextDecoration::Underline));
    // `none` names the line part: the style, the colour and the thickness are reset
    assert_eq!(parts("none"), (Some(StyleTextDecoration::None), None, None, None));
}

#[test]
fn a_colour_may_be_a_function_a_hex_a_name_or_currentcolor() {
    assert_eq!(parts("rgb(255, 0, 0) underline").2, Some(RED));
    assert_eq!(parts("underline #f00").2, Some(RED));
    assert_eq!(parts("underline rgba(255, 0, 0, 1)").2, Some(RED));
    assert_eq!(parts("underline currentcolor").2, Some(StyleTextDecorationColor::CurrentColor));
    assert_eq!(parts("underline CurrentColor wavy").2, Some(StyleTextDecorationColor::CurrentColor));
    assert!(matches!(parts("underline system:accent").2, Some(StyleTextDecorationColor::Color(_))));
}

#[test]
fn the_thickness_is_auto_from_font_a_length_or_a_percentage() {
    assert_eq!(parts("underline auto").3, Some(StyleTextDecorationThickness::Auto));
    assert_eq!(parts("auto").3, Some(StyleTextDecorationThickness::Auto));
    assert_eq!(parts("underline from-font").3, Some(StyleTextDecorationThickness::FromFont));
    assert_eq!(
        parts("underline 0.25em").3,
        Some(StyleTextDecorationThickness::Length(azul_css::props::basic::PixelValue::em(0.25)))
    );
    assert_eq!(
        parts("underline 50%").3,
        Some(StyleTextDecorationThickness::Length(azul_css::props::basic::PixelValue::percent(50.0)))
    );
}

#[test]
fn what_is_no_part_twice_or_no_value_is_an_error() {
    for bad in [
        "",
        "   ",
        "underline underline",
        "none underline",
        "underline none",
        "none none",
        "none blink",
        "red blue",
        "2px 3px",
        "wavy double",
        "underline foo",
        "foo",
        "-2px underline",
        "underline, overline",
        "underline / red",
        "underline auto from-font",
    ] {
        assert!(
            parse_combined_css_property(CombinedCssPropertyType::TextDecoration, bad).is_err(),
            "`{bad}` is no text-decoration"
        );
    }
}

#[test]
fn the_generic_keywords_set_the_four_longhands() {
    for generic in ["inherit", "initial", "unset"] {
        let props = expand(generic);
        let types: Vec<CssPropertyType> = props.iter().map(CssProperty::get_type).collect();
        assert_eq!(
            types,
            vec![
                CssPropertyType::TextDecoration,
                CssPropertyType::TextDecorationStyle,
                CssPropertyType::TextDecorationColor,
                CssPropertyType::TextDecorationThickness,
            ],
            "{generic}"
        );
    }
}

fn line(value: &str) -> StyleTextDecoration {
    match parse_css_property(CssPropertyType::TextDecoration, value)
        .unwrap_or_else(|e| panic!("`{value}` is a text-decoration-line: {e:?}"))
    {
        CssProperty::TextDecoration(CssPropertyValue::Exact(line)) => line,
        other => panic!("`{value}`: {other:?}"),
    }
}

#[test]
fn the_longhand_of_the_lines_takes_several_lines() {
    assert_eq!(line("underline"), StyleTextDecoration::Underline);
    assert_eq!(line("underline overline"), StyleTextDecoration::UnderlineOverline);
    assert_eq!(line("line-through  underline"), StyleTextDecoration::UnderlineLineThrough);
    assert_eq!(
        line("overline line-through underline"),
        StyleTextDecoration::UnderlineOverlineLineThrough
    );
    assert_eq!(line("none"), StyleTextDecoration::None);
    assert_eq!(StyleTextDecoration::default(), StyleTextDecoration::None);
    for bad in ["", "underline underline", "none underline", "underline red", "wavy"] {
        assert!(parse_css_property(CssPropertyType::TextDecoration, bad).is_err(), "`{bad}`");
    }
}

#[test]
fn every_value_of_the_lines_prints_as_the_css_it_was() {
    use azul_css::props::formatter::PrintAsCssValue;
    for value in [
        "none",
        "underline",
        "overline",
        "line-through",
        "underline overline",
        "underline line-through",
        "overline line-through",
        "underline overline line-through",
    ] {
        let parsed = line(value);
        assert_eq!(parsed.print_as_css_value(), value);
        assert_eq!(line(&parsed.print_as_css_value()), parsed, "{value}");
        let (underline, overline, line_through) = parsed.lines();
        assert_eq!(StyleTextDecoration::from_lines(underline, overline, line_through), parsed);
    }
}

fn style(value: &str) -> StyleTextDecorationStyle {
    match parse_css_property(CssPropertyType::TextDecorationStyle, value)
        .unwrap_or_else(|e| panic!("`{value}` is a text-decoration-style: {e:?}"))
    {
        CssProperty::TextDecorationStyle(CssPropertyValue::Exact(style)) => style,
        other => panic!("`{value}`: {other:?}"),
    }
}

#[test]
fn the_style_is_one_of_five_keywords_and_solid_is_the_initial_value() {
    for (value, expected) in [
        ("solid", StyleTextDecorationStyle::Solid),
        ("double", StyleTextDecorationStyle::Double),
        ("dotted", StyleTextDecorationStyle::Dotted),
        ("dashed", StyleTextDecorationStyle::Dashed),
        ("wavy", StyleTextDecorationStyle::Wavy),
        ("WAVY", StyleTextDecorationStyle::Wavy),
        ("  Dashed ", StyleTextDecorationStyle::Dashed),
    ] {
        assert_eq!(style(value), expected, "{value:?}");
    }
    assert_eq!(StyleTextDecorationStyle::default(), StyleTextDecorationStyle::Solid);
    for bad in ["", "wave", "solid solid", "groove", "1px", "red"] {
        assert!(parse_css_property(CssPropertyType::TextDecorationStyle, bad).is_err(), "`{bad}`");
    }
}

#[test]
fn the_style_prints_as_the_css_it_was_and_is_not_inherited() {
    use azul_css::props::formatter::PrintAsCssValue;
    for value in ["solid", "double", "dotted", "dashed", "wavy"] {
        assert_eq!(style(value).print_as_css_value(), value);
    }
    assert!(!CssPropertyType::TextDecorationStyle.is_inheritable());
}

/// `text-decoration` is the shorthand and `text-decoration-line` the longhand of the lines: the
/// typed property keeps its type (`CssPropertyType::TextDecoration`) under the longhand's name.
#[test]
fn the_names_find_the_shorthand_and_the_longhands() {
    let map = get_css_key_map();
    assert_eq!(
        CombinedCssPropertyType::from_str("text-decoration", &map),
        Some(CombinedCssPropertyType::TextDecoration)
    );
    assert_eq!(CssPropertyType::from_str("text-decoration", &map), None, "no longhand of that name");
    for (name, ty) in [
        ("text-decoration-line", CssPropertyType::TextDecoration),
        ("text-decoration-style", CssPropertyType::TextDecorationStyle),
        ("text-decoration-color", CssPropertyType::TextDecorationColor),
        ("text-decoration-thickness", CssPropertyType::TextDecorationThickness),
    ] {
        assert_eq!(CssPropertyType::from_str(name, &map), Some(ty), "{name}");
        assert_eq!(ty.to_str(), name);
    }
    assert_eq!(CombinedCssPropertyType::TextDecoration.to_string(), "text-decoration");
}
