use super::*;
#[cfg(feature = "parser")]
use crate::{
    css::CssPropertyValue,
    props::{
        basic::{
            error::InvalidValueErr,
            font::{
                parse_style_font_family, parse_style_font_size, StyleFontFamily,
                StyleFontFamilyVec, StyleFontSize, StyleFontStyle, StyleFontVariantCaps,
                StyleFontVariantNumeric, StyleFontWeight,
            },
            font_variation::{
                StyleFontLigatureChoice, StyleFontStretch, StyleFontSynthesis,
                StyleFontVariantEastAsian, StyleFontVariantLigatures, StyleFontVariantPosition,
            },
            length::{FloatValue, PercentageValue},
            pixel::PixelValue,
        },
        property::{CssProperty, CssPropertyType},
        style::text::StyleLineHeight,
    },
    system::SystemFontType,
};

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

/// A `font` value that must parse.
#[cfg(feature = "parser")]
fn font(input: &str) -> Vec<CssProperty> {
    parse_font_shorthand(input).unwrap_or_else(|e| panic!("`font: {input}` must parse: {e:?}"))
}

/// A `font-variant` value that must parse.
#[cfg(feature = "parser")]
fn variant(input: &str) -> Vec<CssProperty> {
    parse_font_variant_shorthand(input)
        .unwrap_or_else(|e| panic!("`font-variant: {input}` must parse: {e:?}"))
}

/// A `font-synthesis` value that must parse.
#[cfg(feature = "parser")]
fn synthesis(input: &str) -> Vec<CssProperty> {
    parse_font_synthesis_shorthand(input)
        .unwrap_or_else(|e| panic!("`font-synthesis: {input}` must parse: {e:?}"))
}

#[cfg(feature = "parser")]
fn px(value: f32) -> StyleFontSize {
    StyleFontSize {
        inner: PixelValue::px(value),
    }
}

#[cfg(feature = "parser")]
fn font_size(inner: PixelValue) -> StyleFontSize {
    StyleFontSize { inner }
}

/// A family list of named families.
#[cfg(feature = "parser")]
fn family(names: &[&str]) -> StyleFontFamilyVec {
    names
        .iter()
        .map(|name| StyleFontFamily::System((*name).into()))
        .collect::<Vec<_>>()
        .into()
}

/// What a `font` value is expected to set. A `None` is a longhand the value leaves out: it is
/// `initial`, as are the eleven longhands `font` resets.
#[cfg(feature = "parser")]
struct FontValue {
    style: Option<StyleFontStyle>,
    caps: Option<StyleFontVariantCaps>,
    weight: Option<StyleFontWeight>,
    stretch: Option<StyleFontStretch>,
    size: StyleFontSize,
    line_height: Option<StyleLineHeight>,
    family: StyleFontFamilyVec,
}

#[cfg(feature = "parser")]
impl FontValue {
    /// A value that names a size and a family only.
    fn new(size: StyleFontSize, family: StyleFontFamilyVec) -> Self {
        Self {
            style: None,
            caps: None,
            weight: None,
            stretch: None,
            size,
            line_height: None,
            family,
        }
    }

    /// The nineteen longhands, in the order of the contract.
    fn expansion(self) -> Vec<CssProperty> {
        fn or_initial<T>(value: Option<T>) -> CssPropertyValue<T> {
            match value {
                Some(value) => CssPropertyValue::Exact(value),
                None => CssPropertyValue::Initial,
            }
        }
        vec![
            CssProperty::FontStyle(or_initial(self.style)),
            CssProperty::FontVariantCaps(or_initial(self.caps)),
            CssProperty::FontWeight(or_initial(self.weight)),
            CssProperty::FontStretch(or_initial(self.stretch)),
            CssProperty::FontSize(CssPropertyValue::Exact(self.size)),
            CssProperty::LineHeight(or_initial(self.line_height)),
            CssProperty::FontFamily(CssPropertyValue::Exact(self.family)),
            CssProperty::FontVariantNumeric(CssPropertyValue::Initial),
            CssProperty::FontVariantLigatures(CssPropertyValue::Initial),
            CssProperty::FontVariantEastAsian(CssPropertyValue::Initial),
            CssProperty::FontVariantPosition(CssPropertyValue::Initial),
            CssProperty::FontKerning(CssPropertyValue::Initial),
            CssProperty::FontOpticalSizing(CssPropertyValue::Initial),
            CssProperty::FontVariationSettings(CssPropertyValue::Initial),
            CssProperty::FontFeatureSettings(CssPropertyValue::Initial),
            CssProperty::FontSynthesisWeight(CssPropertyValue::Initial),
            CssProperty::FontSynthesisStyle(CssPropertyValue::Initial),
            CssProperty::FontSynthesisSmallCaps(CssPropertyValue::Initial),
            CssProperty::FontSynthesisPosition(CssPropertyValue::Initial),
        ]
    }
}

/// `12px a` with one more property named.
#[cfg(feature = "parser")]
fn a_12px() -> FontValue {
    FontValue::new(px(12.0), family(&["a"]))
}

/// What a `font-variant` value is expected to set: the five longhands, all `Exact`.
#[cfg(feature = "parser")]
struct VariantValue {
    numeric: StyleFontVariantNumeric,
    caps: StyleFontVariantCaps,
    ligatures: StyleFontVariantLigatures,
    east_asian: StyleFontVariantEastAsian,
}

#[cfg(feature = "parser")]
impl VariantValue {
    /// Every longhand `normal`.
    fn normal() -> Self {
        Self {
            numeric: StyleFontVariantNumeric::NORMAL,
            caps: StyleFontVariantCaps::Normal,
            ligatures: StyleFontVariantLigatures::NORMAL,
            east_asian: StyleFontVariantEastAsian::NORMAL,
        }
    }

    /// The five longhands, in the order of the contract, the position `normal`.
    fn expansion(self) -> Vec<CssProperty> {
        self.expansion_with_position(StyleFontVariantPosition::Normal)
    }

    /// The five longhands, in the order of the contract.
    fn expansion_with_position(self, position: StyleFontVariantPosition) -> Vec<CssProperty> {
        vec![
            CssProperty::FontVariantNumeric(CssPropertyValue::Exact(self.numeric)),
            CssProperty::FontVariantCaps(CssPropertyValue::Exact(self.caps)),
            CssProperty::FontVariantLigatures(CssPropertyValue::Exact(self.ligatures)),
            CssProperty::FontVariantEastAsian(CssPropertyValue::Exact(self.east_asian)),
            CssProperty::FontVariantPosition(CssPropertyValue::Exact(position)),
        ]
    }
}

/// The four `font-synthesis-*` longhands, all `Exact`: `auto` where `on`, else `none`.
#[cfg(feature = "parser")]
fn synthesis_expansion(
    weight: bool,
    style: bool,
    small_caps: bool,
    position: bool,
) -> Vec<CssProperty> {
    let value = |on: bool| {
        CssPropertyValue::Exact(if on {
            StyleFontSynthesis::Auto
        } else {
            StyleFontSynthesis::None
        })
    };
    vec![
        CssProperty::FontSynthesisWeight(value(weight)),
        CssProperty::FontSynthesisStyle(value(style)),
        CssProperty::FontSynthesisSmallCaps(value(small_caps)),
        CssProperty::FontSynthesisPosition(value(position)),
    ]
}

// ---------------------------------------------------------------------------------------------
// font: the expansion
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn font_sets_the_nineteen_longhands_in_the_documented_order() {
    let types: Vec<CssPropertyType> = font("12px a").iter().map(CssProperty::get_type).collect();
    assert_eq!(
        types,
        vec![
            CssPropertyType::FontStyle,
            CssPropertyType::FontVariantCaps,
            CssPropertyType::FontWeight,
            CssPropertyType::FontStretch,
            CssPropertyType::FontSize,
            CssPropertyType::LineHeight,
            CssPropertyType::FontFamily,
            CssPropertyType::FontVariantNumeric,
            CssPropertyType::FontVariantLigatures,
            CssPropertyType::FontVariantEastAsian,
            CssPropertyType::FontVariantPosition,
            CssPropertyType::FontKerning,
            CssPropertyType::FontOpticalSizing,
            CssPropertyType::FontVariationSettings,
            CssPropertyType::FontFeatureSettings,
            CssPropertyType::FontSynthesisWeight,
            CssPropertyType::FontSynthesisStyle,
            CssPropertyType::FontSynthesisSmallCaps,
            CssPropertyType::FontSynthesisPosition,
        ]
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_with_a_size_and_a_family_resets_everything_it_does_not_name_to_initial() {
    // Spelled out, not built by the helper: the value names the size and the family, every other
    // longhand is the CSS-wide keyword `initial`.
    assert_eq!(
        font("12px Arial"),
        vec![
            CssProperty::FontStyle(CssPropertyValue::Initial),
            CssProperty::FontVariantCaps(CssPropertyValue::Initial),
            CssProperty::FontWeight(CssPropertyValue::Initial),
            CssProperty::FontStretch(CssPropertyValue::Initial),
            CssProperty::FontSize(CssPropertyValue::Exact(px(12.0))),
            CssProperty::LineHeight(CssPropertyValue::Initial),
            CssProperty::FontFamily(CssPropertyValue::Exact(family(&["Arial"]))),
            CssProperty::FontVariantNumeric(CssPropertyValue::Initial),
            CssProperty::FontVariantLigatures(CssPropertyValue::Initial),
            CssProperty::FontVariantEastAsian(CssPropertyValue::Initial),
            CssProperty::FontVariantPosition(CssPropertyValue::Initial),
            CssProperty::FontKerning(CssPropertyValue::Initial),
            CssProperty::FontOpticalSizing(CssPropertyValue::Initial),
            CssProperty::FontVariationSettings(CssPropertyValue::Initial),
            CssProperty::FontFeatureSettings(CssPropertyValue::Initial),
            CssProperty::FontSynthesisWeight(CssPropertyValue::Initial),
            CssProperty::FontSynthesisStyle(CssPropertyValue::Initial),
            CssProperty::FontSynthesisSmallCaps(CssPropertyValue::Initial),
            CssProperty::FontSynthesisPosition(CssPropertyValue::Initial),
        ]
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_with_every_part_of_the_grammar() {
    assert_eq!(
        font(r#"italic small-caps bold condensed 12px/1.5 "Helvetica Neue", Arial, sans-serif"#),
        FontValue {
            style: Some(StyleFontStyle::Italic),
            caps: Some(StyleFontVariantCaps::SmallCaps),
            weight: Some(StyleFontWeight::BOLD),
            stretch: Some(StyleFontStretch::from_percent(75.0)),
            size: px(12.0),
            line_height: Some(StyleLineHeight::Number(FloatValue::new(1.5))),
            family: family(&["Helvetica Neue", "Arial", "sans-serif"]),
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_with_a_bold_weight_a_point_size_and_a_generic_family() {
    assert_eq!(
        font("bold 12pt serif"),
        FontValue {
            weight: Some(StyleFontWeight::BOLD),
            ..FontValue::new(font_size(PixelValue::pt(12.0)), family(&["serif"]))
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn the_value_is_trimmed_and_any_ascii_white_space_separates_the_tokens() {
    let expected = FontValue {
        weight: Some(StyleFontWeight::BOLD),
        line_height: Some(StyleLineHeight::Number(FloatValue::new(1.5))),
        family: family(&["A B", "c"]),
        ..a_12px()
    }
    .expansion();
    assert_eq!(font("bold 12px/1.5 \"A B\", c"), expected);
    assert_eq!(font("  bold   12px/1.5   \"A B\"  ,  c  "), expected);
    assert_eq!(font("bold\t12px\t/\t1.5\t\"A B\" ,\tc"), expected);
    assert_eq!(font("\nbold\n12px\n/\n1.5\n\"A B\",\nc\n"), expected);
}

// ---------------------------------------------------------------------------------------------
// font: the prefix
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn the_prefix_properties_come_in_any_order() {
    let expected = FontValue {
        style: Some(StyleFontStyle::Italic),
        caps: Some(StyleFontVariantCaps::SmallCaps),
        weight: Some(StyleFontWeight::BOLD),
        stretch: Some(StyleFontStretch::from_percent(75.0)),
        ..a_12px()
    }
    .expansion();
    for input in [
        "italic small-caps bold condensed 12px a",
        "condensed bold small-caps italic 12px a",
        "bold italic condensed small-caps 12px a",
        "small-caps condensed italic bold 12px a",
        "bold condensed italic small-caps 12px a",
    ] {
        assert_eq!(font(input), expected, "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn the_prefix_keywords_ignore_ascii_case() {
    assert_eq!(
        font("ITALIC Small-Caps BOLD Condensed 12px Arial"),
        FontValue {
            style: Some(StyleFontStyle::Italic),
            caps: Some(StyleFontVariantCaps::SmallCaps),
            weight: Some(StyleFontWeight::BOLD),
            stretch: Some(StyleFontStretch::from_percent(75.0)),
            ..FontValue::new(px(12.0), family(&["Arial"]))
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_style_is_normal_italic_oblique_or_oblique_with_an_angle() {
    for (input, style) in [
        ("italic 12px a", StyleFontStyle::Italic),
        ("oblique 12px a", StyleFontStyle::OBLIQUE),
        ("oblique 14deg 12px a", StyleFontStyle::OBLIQUE),
        (
            "oblique 10deg 12px a",
            StyleFontStyle::Oblique(FloatValue::new(10.0)),
        ),
        (
            "oblique -20deg 12px a",
            StyleFontStyle::Oblique(FloatValue::new(-20.0)),
        ),
        (
            "oblique 90deg 12px a",
            StyleFontStyle::Oblique(FloatValue::new(90.0)),
        ),
        (
            "OBLIQUE 10DEG 12px a",
            StyleFontStyle::Oblique(FloatValue::new(10.0)),
        ),
    ] {
        assert_eq!(
            font(input),
            FontValue {
                style: Some(style),
                ..a_12px()
            }
            .expansion(),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn an_oblique_angle_is_a_token_of_its_own_before_the_size() {
    // `font: oblique 10deg 14px x`
    assert_eq!(
        font("oblique 10deg 14px x"),
        FontValue {
            style: Some(StyleFontStyle::Oblique(FloatValue::new(10.0))),
            ..FontValue::new(px(14.0), family(&["x"]))
        }
        .expansion()
    );
    // The angle goes with `oblique` wherever the style stands in the prefix.
    assert_eq!(
        font("bold oblique 10deg small-caps 14px x"),
        FontValue {
            style: Some(StyleFontStyle::Oblique(FloatValue::new(10.0))),
            caps: Some(StyleFontVariantCaps::SmallCaps),
            weight: Some(StyleFontWeight::BOLD),
            ..FontValue::new(px(14.0), family(&["x"]))
        }
        .expansion()
    );
    // A bare `oblique` before a size takes no angle: the size is no angle.
    assert_eq!(
        font("oblique 14px x"),
        FontValue {
            style: Some(StyleFontStyle::OBLIQUE),
            ..FontValue::new(px(14.0), family(&["x"]))
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn an_angle_out_of_range_is_no_part_of_the_value() {
    for input in [
        "oblique 91deg 12px a",
        "oblique -91deg 12px a",
        "oblique 10px 12px a",
        "italic 10deg 12px a",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_in_the_prefix_is_normal_or_small_caps_only() {
    assert_eq!(
        font("small-caps 12px a"),
        FontValue {
            caps: Some(StyleFontVariantCaps::SmallCaps),
            ..a_12px()
        }
        .expansion()
    );
    for input in [
        "all-small-caps 12px a",
        "petite-caps 12px a",
        "all-petite-caps 12px a",
        "unicase 12px a",
        "titling-caps 12px a",
        "small-cap 12px a",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_weight_is_a_keyword_or_a_number_from_one_to_a_thousand() {
    for (input, weight) in [
        ("bold 12px a", StyleFontWeight::BOLD),
        ("700 12px a", StyleFontWeight::BOLD),
        ("400 12px a", StyleFontWeight::NORMAL),
        ("100 12px a", StyleFontWeight::W100),
        ("900 12px a", StyleFontWeight::W900),
        ("1 12px a", StyleFontWeight::from_number(1.0)),
        ("1000 12px a", StyleFontWeight::from_number(1000.0)),
        ("350 12px a", StyleFontWeight::from_number(350.0)),
        ("650.5 12px a", StyleFontWeight::from_number(650.5)),
        ("lighter 12px a", StyleFontWeight::Lighter),
        ("bolder 12px a", StyleFontWeight::Bolder),
        ("BOLD 12px a", StyleFontWeight::BOLD),
    ] {
        assert_eq!(
            font(input),
            FontValue {
                weight: Some(weight),
                ..a_12px()
            }
            .expansion(),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_weight_that_is_no_multiple_of_a_hundred_is_a_weight() {
    // `font: 350 14px sans-serif`
    assert_eq!(
        font("350 14px sans-serif"),
        FontValue {
            weight: Some(StyleFontWeight::from_number(350.0)),
            ..FontValue::new(px(14.0), family(&["sans-serif"]))
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_number_outside_one_to_a_thousand_is_no_weight_and_a_bare_number_is_no_size() {
    for input in [
        "0 12px a",
        "1001 12px a",
        "-5 12px a",
        "1e3 12px a",
        "inf 12px a",
        "NaN 12px a",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_stretch_is_one_of_the_css3_keywords() {
    for (keyword, percent) in [
        ("ultra-condensed", 50.0),
        ("extra-condensed", 62.5),
        ("condensed", 75.0),
        ("semi-condensed", 87.5),
        ("semi-expanded", 112.5),
        ("expanded", 125.0),
        ("extra-expanded", 150.0),
        ("ultra-expanded", 200.0),
    ] {
        let input = format!("{keyword} 12px a");
        assert_eq!(
            font(&input),
            FontValue {
                stretch: Some(StyleFontStretch::from_percent(percent)),
                ..a_12px()
            }
            .expansion(),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_stretch_in_the_prefix_takes_no_percentage() {
    // A percentage is the font size, and the rest, `12px a`, is no family name: it starts like a
    // number.
    for input in ["87.5% 12px a", "condensed 87.5% 12px a", "100% 12px a", "50% 12px a"] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn normal_four_times_names_all_four_prefix_properties() {
    // `font: normal normal normal normal 10px a`
    assert_eq!(
        font("normal normal normal normal 10px a"),
        FontValue {
            style: Some(StyleFontStyle::Normal),
            caps: Some(StyleFontVariantCaps::Normal),
            weight: Some(StyleFontWeight::NORMAL),
            stretch: Some(StyleFontStretch::NORMAL),
            ..FontValue::new(px(10.0), family(&["a"]))
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_normal_is_the_first_of_the_four_properties_no_other_token_names() {
    // style, caps, weight, stretch
    assert_eq!(
        font("normal 12px a"),
        FontValue {
            style: Some(StyleFontStyle::Normal),
            ..a_12px()
        }
        .expansion()
    );
    // `bold` names the weight, so `normal` is the style: wherever it stands.
    let bold_normal = FontValue {
        style: Some(StyleFontStyle::Normal),
        weight: Some(StyleFontWeight::BOLD),
        ..a_12px()
    }
    .expansion();
    assert_eq!(font("bold normal 12px a"), bold_normal);
    assert_eq!(font("normal bold 12px a"), bold_normal);
    // `italic` names the style, so `normal` is the caps variant.
    let italic_normal = FontValue {
        style: Some(StyleFontStyle::Italic),
        caps: Some(StyleFontVariantCaps::Normal),
        ..a_12px()
    }
    .expansion();
    assert_eq!(font("italic normal 12px a"), italic_normal);
    assert_eq!(font("normal italic 12px a"), italic_normal);
    // Three `normal`s with an `italic` fill the other three.
    let all_set = FontValue {
        style: Some(StyleFontStyle::Italic),
        caps: Some(StyleFontVariantCaps::Normal),
        weight: Some(StyleFontWeight::NORMAL),
        stretch: Some(StyleFontStretch::NORMAL),
        ..a_12px()
    }
    .expansion();
    assert_eq!(font("italic normal normal normal 12px a"), all_set);
    assert_eq!(font("normal italic normal normal 12px a"), all_set);
    assert_eq!(font("normal normal normal italic 12px a"), all_set);
}

#[cfg(feature = "parser")]
#[test]
fn more_normals_than_free_properties_is_an_error() {
    for input in [
        "normal normal normal normal normal 10px a",
        "italic normal normal normal normal 10px a",
        "normal normal normal normal bold 10px a",
        "bold italic small-caps condensed normal 10px a",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_property_given_twice_is_an_error() {
    for input in [
        "bold bold 12px a",
        "bold 700 12px a",
        "bold lighter 12px a",
        "400 700 12px a",
        "small-caps small-caps 12px a",
        "italic italic 12px a",
        "italic oblique 12px a",
        "oblique italic 12px a",
        "oblique 10deg italic 12px a",
        "oblique oblique 12px a",
        "condensed expanded 12px a",
        "condensed condensed 12px a",
        "bold italic bold 12px a",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

// ---------------------------------------------------------------------------------------------
// font: the size and the line height
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn the_size_is_a_length_a_percentage_or_a_keyword() {
    for (input, size) in [
        ("12px a", px(12.0)),
        ("12pt a", font_size(PixelValue::pt(12.0))),
        ("1.5em a", font_size(PixelValue::em(1.5))),
        ("0.75rem a", font_size(PixelValue::rem(0.75))),
        ("80% a", font_size(PixelValue::percent(80.0))),
        ("0px a", px(0.0)),
        ("large a", px(18.0)),
        ("x-small a", px(10.0)),
        ("medium a", px(16.0)),
        (
            "larger a",
            parse_style_font_size("larger").expect("`larger` is a font-size"),
        ),
        (
            "smaller a",
            parse_style_font_size("smaller").expect("`smaller` is a font-size"),
        ),
    ] {
        assert_eq!(
            font(input),
            FontValue::new(size, family(&["a"])).expansion(),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_negative_size_is_no_size() {
    for input in ["-12px a", "-1em a", "-50% a"] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_bare_number_is_never_a_size() {
    // `600` is a weight, so `font: 600 a` has a weight and no size; `12` is not a size either.
    for input in ["600 a", "12 a", "bold 12 a", "12.5 a", "0 a", "1e2 a"] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn the_line_height_follows_the_size_after_a_slash_with_or_without_white_space() {
    let expected = FontValue {
        line_height: Some(StyleLineHeight::Number(FloatValue::new(1.5))),
        ..a_12px()
    }
    .expansion();
    for input in [
        "12px/1.5 a",
        "12px / 1.5 a",
        "12px /1.5 a",
        "12px/ 1.5 a",
        "12px   /   1.5   a",
        "12px\t/\t1.5\ta",
    ] {
        assert_eq!(font(input), expected, "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn the_line_height_is_a_number_a_percentage_a_length_or_normal() {
    for (input, line_height) in [
        ("12px/2 a", StyleLineHeight::Number(FloatValue::new(2.0))),
        (
            "12px/150% a",
            StyleLineHeight::Percentage(PercentageValue::new(150.0)),
        ),
        (
            "12px/20px a",
            StyleLineHeight::Length(PixelValue::px(20.0)),
        ),
        ("12px/normal a", StyleLineHeight::Normal),
        ("12px/NORMAL a", StyleLineHeight::Normal),
    ] {
        assert_eq!(
            font(input),
            FontValue {
                line_height: Some(line_height),
                ..a_12px()
            }
            .expansion(),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn without_a_slash_the_line_height_is_initial_and_with_one_it_is_exact() {
    let without = font("12px a");
    let with = font("12px/normal a");
    assert_eq!(
        without[5],
        CssProperty::LineHeight(CssPropertyValue::Initial)
    );
    assert_eq!(
        with[5],
        CssProperty::LineHeight(CssPropertyValue::Exact(StyleLineHeight::Normal))
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_broken_line_height_is_an_error() {
    for input in [
        "12px/ a",
        "12px/a",
        "12px/",
        "12px /",
        "12px / a",
        "12px/-1 a",
        "12px/abc a",
        "12px/1.5/2 a",
        "12px//1.5 a",
        "/1.5 a",
        "/ 12px a",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

// ---------------------------------------------------------------------------------------------
// font: the family list
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn the_rest_of_the_value_is_the_family_list() {
    for (input, names) in [
        ("12px Arial", vec!["Arial"]),
        ("12px Arial, sans-serif", vec!["Arial", "sans-serif"]),
        ("12px Times New Roman", vec!["Times New Roman"]),
        ("12px \"Times New Roman\"", vec!["Times New Roman"]),
        (
            "12px 'Fira Code', monospace",
            vec!["Fira Code", "monospace"],
        ),
        ("12px \"a, b\", c", vec!["a, b", "c"]),
        ("12px \"3270\"", vec!["3270"]),
        (
            "12px -apple-system, sans-serif",
            vec!["-apple-system", "sans-serif"],
        ),
        (
            "12px \u{5FAE}\u{8F6F}\u{96C5}\u{9ED1}, sans-serif",
            vec!["\u{5FAE}\u{8F6F}\u{96C5}\u{9ED1}", "sans-serif"],
        ),
        // The family may be named like a keyword of the prefix.
        ("12px bold", vec!["bold"]),
        ("12px normal", vec!["normal"]),
    ] {
        assert_eq!(
            font(input),
            FontValue::new(px(12.0), family(&names)).expansion(),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_system_family_name_in_the_family_list_is_a_system_family() {
    assert_eq!(
        font("12px/1.5 system:ui, sans-serif"),
        FontValue {
            line_height: Some(StyleLineHeight::Number(FloatValue::new(1.5))),
            family: StyleFontFamilyVec::from_vec(vec![
                StyleFontFamily::SystemType(SystemFontType::Ui),
                StyleFontFamily::System("sans-serif".into()),
            ]),
            ..a_12px()
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_family_list_must_name_every_family() {
    for input in [
        "12px a,",
        "12px ,a",
        "12px a,,b",
        "12px a, ,b",
        "12px 3270",
        "12px 12px a",
        "12px a, 3d",
        "12px -5",
        "12px .5",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

// ---------------------------------------------------------------------------------------------
// font: what is no value, and system fonts
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn a_value_without_a_size_or_without_a_family_is_an_error() {
    for input in [
        "Arial",
        "bold",
        "12px",
        "bold bold 12px a",
        "small-caps small-caps 12px a",
        "12px/ a",
        "italic",
        "italic 12px",
        "italic bold",
        "normal",
        "12px/1.5",
        "bold Arial",
        "",
        "   ",
        "/",
        "a b c",
    ] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn an_error_carries_the_trimmed_value() {
    assert_eq!(
        parse_font_shorthand("  Arial  "),
        Err(InvalidValueErr("Arial"))
    );
    assert_eq!(
        parse_font_shorthand("\tbold bold 12px a\n"),
        Err(InvalidValueErr("bold bold 12px a"))
    );
    assert_eq!(parse_font_shorthand(""), Err(InvalidValueErr("")));
}

#[cfg(feature = "parser")]
#[test]
fn a_system_font_keyword_keeps_the_family_only_font_property() {
    for keyword in [
        "caption",
        "icon",
        "menu",
        "message-box",
        "small-caption",
        "status-bar",
    ] {
        assert_eq!(
            font(keyword),
            vec![CssProperty::Font(CssPropertyValue::Exact(family(&[
                keyword
            ])))],
            "`{keyword}`"
        );
        // ... with white space around it,
        assert_eq!(font(&format!("  {keyword}\t")), font(keyword), "`{keyword}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_system_font_keyword_ignores_ascii_case_and_keeps_the_name_as_written() {
    assert_eq!(
        font("MESSAGE-BOX"),
        vec![CssProperty::Font(CssPropertyValue::Exact(family(&[
            "MESSAGE-BOX"
        ])))]
    );
    assert_eq!(
        font("Menu"),
        vec![CssProperty::Font(CssPropertyValue::Exact(family(&["Menu"])))]
    );
}

#[cfg(feature = "parser")]
#[test]
fn an_azul_system_font_is_the_family_only_font_property() {
    assert_eq!(
        font("system:ui"),
        vec![CssProperty::Font(CssPropertyValue::Exact(
            StyleFontFamilyVec::from_vec(vec![StyleFontFamily::SystemType(SystemFontType::Ui)])
        ))]
    );
    assert_eq!(
        font("system:monospace:bold, serif"),
        vec![CssProperty::Font(CssPropertyValue::Exact(
            StyleFontFamilyVec::from_vec(vec![
                StyleFontFamily::SystemType(SystemFontType::MonospaceBold),
                StyleFontFamily::System("serif".into()),
            ])
        ))]
    );
    // It is what the family parser makes of the value, as `font` always did.
    for value in ["system:ui", "system:title:bold", "system:menu, sans-serif"] {
        assert_eq!(
            font(value),
            vec![CssProperty::Font(CssPropertyValue::Exact(
                parse_style_font_family(value).expect("a family list")
            ))],
            "`{value}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_system_font_keyword_with_anything_else_is_no_system_font() {
    // `caption 12px` is not the keyword alone: a size is expected where `caption` stands.
    for input in ["caption 12px", "bold caption", "menu 12px/1.5 a", "caption, serif"] {
        assert!(parse_font_shorthand(input).is_err(), "`{input}`");
    }
    // After a size the keyword is a family name like any other.
    assert_eq!(
        font("12px caption extra"),
        FontValue::new(px(12.0), family(&["caption extra"])).expansion()
    );
    assert_eq!(
        font("12px caption"),
        FontValue::new(px(12.0), family(&["caption"])).expansion()
    );
}

// ---------------------------------------------------------------------------------------------
// font-variant
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn font_variant_sets_the_five_longhands_in_the_documented_order() {
    let types: Vec<CssPropertyType> = variant("normal")
        .iter()
        .map(CssProperty::get_type)
        .collect();
    assert_eq!(
        types,
        vec![
            CssPropertyType::FontVariantNumeric,
            CssPropertyType::FontVariantCaps,
            CssPropertyType::FontVariantLigatures,
            CssPropertyType::FontVariantEastAsian,
            CssPropertyType::FontVariantPosition,
        ]
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_normal_is_normal_for_all_five_longhands() {
    assert_eq!(variant("normal"), VariantValue::normal().expansion());
    assert_eq!(variant("  NORMAL "), VariantValue::normal().expansion());
    // Spelled out: every value is `Exact`, none is `initial`.
    assert_eq!(
        variant("normal"),
        vec![
            CssProperty::FontVariantNumeric(CssPropertyValue::Exact(
                StyleFontVariantNumeric::NORMAL
            )),
            CssProperty::FontVariantCaps(CssPropertyValue::Exact(StyleFontVariantCaps::Normal)),
            CssProperty::FontVariantLigatures(CssPropertyValue::Exact(
                StyleFontVariantLigatures::NORMAL
            )),
            CssProperty::FontVariantEastAsian(CssPropertyValue::Exact(
                StyleFontVariantEastAsian::NORMAL
            )),
            CssProperty::FontVariantPosition(CssPropertyValue::Exact(
                StyleFontVariantPosition::Normal
            )),
        ]
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_none_is_none_for_the_ligatures_and_normal_for_the_rest() {
    let expected = VariantValue {
        ligatures: StyleFontVariantLigatures::NONE,
        ..VariantValue::normal()
    }
    .expansion();
    assert_eq!(variant("none"), expected);
    assert_eq!(variant(" None "), expected);
    assert_eq!(variant("NONE"), expected);
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_caps_keywords_go_to_the_caps_longhand() {
    for (input, caps) in [
        ("small-caps", StyleFontVariantCaps::SmallCaps),
        ("Small-Caps", StyleFontVariantCaps::SmallCaps),
        ("all-small-caps", StyleFontVariantCaps::AllSmallCaps),
        ("petite-caps", StyleFontVariantCaps::PetiteCaps),
        ("all-petite-caps", StyleFontVariantCaps::AllPetiteCaps),
        ("unicase", StyleFontVariantCaps::Unicase),
        ("titling-caps", StyleFontVariantCaps::TitlingCaps),
    ] {
        assert_eq!(
            variant(input),
            VariantValue {
                caps,
                ..VariantValue::normal()
            }
            .expansion(),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_numeric_keywords_go_to_the_numeric_longhand() {
    assert_eq!(
        variant("tabular-nums"),
        VariantValue {
            numeric: StyleFontVariantNumeric {
                tabular_nums: true,
                ..StyleFontVariantNumeric::NORMAL
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
    assert_eq!(
        variant("oldstyle-nums slashed-zero diagonal-fractions"),
        VariantValue {
            numeric: StyleFontVariantNumeric {
                oldstyle_nums: true,
                slashed_zero: true,
                diagonal_fractions: true,
                ..StyleFontVariantNumeric::NORMAL
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_ligature_keywords_go_to_the_ligatures_longhand() {
    assert_eq!(
        variant("no-common-ligatures"),
        VariantValue {
            ligatures: StyleFontVariantLigatures {
                common: StyleFontLigatureChoice::Off,
                ..StyleFontVariantLigatures::NORMAL
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
    assert_eq!(
        variant("common-ligatures no-discretionary-ligatures historical-ligatures no-contextual"),
        VariantValue {
            ligatures: StyleFontVariantLigatures {
                common: StyleFontLigatureChoice::On,
                discretionary: StyleFontLigatureChoice::Off,
                historical: StyleFontLigatureChoice::On,
                contextual: StyleFontLigatureChoice::Off,
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
    assert_eq!(
        variant("contextual"),
        VariantValue {
            ligatures: StyleFontVariantLigatures {
                contextual: StyleFontLigatureChoice::On,
                ..StyleFontVariantLigatures::NORMAL
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
    // Every group off is `none`.
    assert_eq!(
        variant(
            "no-common-ligatures no-discretionary-ligatures no-historical-ligatures no-contextual"
        ),
        VariantValue {
            ligatures: StyleFontVariantLigatures::NONE,
            ..VariantValue::normal()
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_east_asian_keywords_go_to_the_east_asian_longhand() {
    // `jis78 full-width ruby`
    assert_eq!(
        variant("jis78 full-width ruby"),
        VariantValue {
            east_asian: StyleFontVariantEastAsian {
                jis78: true,
                full_width: true,
                ruby: true,
                ..StyleFontVariantEastAsian::NORMAL
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
    assert_eq!(
        variant("traditional proportional-width"),
        VariantValue {
            east_asian: StyleFontVariantEastAsian {
                traditional: true,
                proportional_width: true,
                ..StyleFontVariantEastAsian::NORMAL
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_takes_keywords_of_different_longhands_in_any_order() {
    // `common-ligatures tabular-nums`
    let expected = VariantValue {
        numeric: StyleFontVariantNumeric {
            tabular_nums: true,
            ..StyleFontVariantNumeric::NORMAL
        },
        ligatures: StyleFontVariantLigatures {
            common: StyleFontLigatureChoice::On,
            ..StyleFontVariantLigatures::NORMAL
        },
        ..VariantValue::normal()
    }
    .expansion();
    assert_eq!(variant("common-ligatures tabular-nums"), expected);
    assert_eq!(variant("tabular-nums common-ligatures"), expected);

    // `small-caps oldstyle-nums discretionary-ligatures`
    assert_eq!(
        variant("small-caps oldstyle-nums discretionary-ligatures"),
        VariantValue {
            numeric: StyleFontVariantNumeric {
                oldstyle_nums: true,
                ..StyleFontVariantNumeric::NORMAL
            },
            caps: StyleFontVariantCaps::SmallCaps,
            ligatures: StyleFontVariantLigatures {
                discretionary: StyleFontLigatureChoice::On,
                ..StyleFontVariantLigatures::NORMAL
            },
            east_asian: StyleFontVariantEastAsian::NORMAL,
        }
        .expansion()
    );

    // All four longhands in one value, in a scrambled order, with a ligature group of its own.
    assert_eq!(
        variant("ruby no-contextual SMALL-CAPS slashed-zero common-ligatures"),
        VariantValue {
            numeric: StyleFontVariantNumeric {
                slashed_zero: true,
                ..StyleFontVariantNumeric::NORMAL
            },
            caps: StyleFontVariantCaps::SmallCaps,
            ligatures: StyleFontVariantLigatures {
                common: StyleFontLigatureChoice::On,
                contextual: StyleFontLigatureChoice::Off,
                ..StyleFontVariantLigatures::NORMAL
            },
            east_asian: StyleFontVariantEastAsian {
                ruby: true,
                ..StyleFontVariantEastAsian::NORMAL
            },
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_ligatures_of_different_groups_combine_with_on_and_off() {
    assert_eq!(
        variant("no-common-ligatures discretionary-ligatures"),
        VariantValue {
            ligatures: StyleFontVariantLigatures {
                common: StyleFontLigatureChoice::Off,
                discretionary: StyleFontLigatureChoice::On,
                ..StyleFontVariantLigatures::NORMAL
            },
            ..VariantValue::normal()
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_takes_a_function_and_sets_nothing_for_it() {
    // `stylistic(x)` belongs to a longhand azul has not: accepted, ignored.
    for input in [
        "stylistic(x)",
        "styleset(ss01, ss02)",
        "character-variant(cv01)",
        "swash(fancy)",
        "ornaments(o1)",
        "annotation(circled)",
        "STYLISTIC(X)",
        "swash(a b)",
        "swash(f(g))",
    ] {
        assert_eq!(variant(input), VariantValue::normal().expansion(), "`{input}`");
    }
    // ... and the other keywords of the value count as ever.
    assert_eq!(
        variant("stylistic(x) small-caps"),
        VariantValue {
            caps: StyleFontVariantCaps::SmallCaps,
            ..VariantValue::normal()
        }
        .expansion()
    );
    assert_eq!(
        variant("styleset(ss01, ss02) tabular-nums"),
        variant("tabular-nums styleset(ss01, ss02)")
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_takes_the_keywords_without_a_longhand_and_sets_nothing_for_them() {
    // `historical-forms`, `text`, `emoji` and `unicode` are accepted and ignored.
    for input in ["historical-forms", "text", "emoji", "unicode", "HISTORICAL-FORMS"] {
        assert_eq!(variant(input), VariantValue::normal().expansion(), "`{input}`");
    }
    assert_eq!(
        variant("text small-caps"),
        VariantValue {
            caps: StyleFontVariantCaps::SmallCaps,
            ..VariantValue::normal()
        }
        .expansion()
    );
}

/// `sub` and `super` are the keywords of `font-variant-position`: the shorthand sets it.
#[cfg(feature = "parser")]
#[test]
fn font_variant_sub_and_super_set_the_position() {
    for (input, position) in [
        ("sub", StyleFontVariantPosition::Subscript),
        ("SUB", StyleFontVariantPosition::Subscript),
        ("super", StyleFontVariantPosition::Superscript),
        ("Super", StyleFontVariantPosition::Superscript),
    ] {
        assert_eq!(
            variant(input),
            VariantValue::normal().expansion_with_position(position),
            "`{input}`"
        );
    }
    assert_eq!(
        variant("sub small-caps"),
        VariantValue {
            caps: StyleFontVariantCaps::SmallCaps,
            ..VariantValue::normal()
        }
        .expansion_with_position(StyleFontVariantPosition::Subscript)
    );
    // a value that names no position is `normal`
    assert_eq!(
        variant("small-caps"),
        VariantValue {
            caps: StyleFontVariantCaps::SmallCaps,
            ..VariantValue::normal()
        }
        .expansion()
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_with_keywords_of_every_kind() {
    assert_eq!(
        variant("no-common-ligatures all-small-caps jis78 super styleset(ss01, ss02)"),
        VariantValue {
            caps: StyleFontVariantCaps::AllSmallCaps,
            ligatures: StyleFontVariantLigatures {
                common: StyleFontLigatureChoice::Off,
                ..StyleFontVariantLigatures::NORMAL
            },
            east_asian: StyleFontVariantEastAsian {
                jis78: true,
                ..StyleFontVariantEastAsian::NORMAL
            },
            numeric: StyleFontVariantNumeric::NORMAL,
        }
        .expansion_with_position(StyleFontVariantPosition::Superscript)
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_takes_a_keyword_of_a_group_once() {
    // The keywords of the longhands azul has not stand in groups of the grammar: one of each.
    for input in [
        "sub super",
        "super sub",
        "sub sub",
        "text emoji",
        "emoji unicode",
        "unicode unicode",
        "historical-forms historical-forms",
        "swash(a) swash(b)",
        "styleset(a) styleset(a)",
        "stylistic(a) stylistic(b)",
    ] {
        assert!(parse_font_variant_shorthand(input).is_err(), "`{input}`");
    }
    // Different groups are different longhands: one of each stands together.
    assert_eq!(
        variant("sub text historical-forms swash(a) ornaments(b) stylistic(c)"),
        VariantValue::normal().expansion_with_position(StyleFontVariantPosition::Subscript)
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_rejects_two_values_of_one_longhand() {
    for input in [
        // two caps keywords
        "small-caps all-small-caps",
        "small-caps small-caps",
        "petite-caps titling-caps",
        // two ligature keywords of one group, or the same keyword twice
        "common-ligatures no-common-ligatures",
        "common-ligatures common-ligatures",
        "contextual no-contextual",
        "discretionary-ligatures no-discretionary-ligatures",
        "historical-ligatures historical-ligatures",
        // two East Asian keywords of one group
        "jis78 jis83",
        "jis78 jis78",
        "simplified traditional",
        "full-width proportional-width",
        "ruby ruby",
        // two numeric keywords of one group
        "lining-nums oldstyle-nums",
        "tabular-nums proportional-nums",
        "diagonal-fractions stacked-fractions",
        "tabular-nums tabular-nums",
        "ordinal ordinal",
    ] {
        assert!(parse_font_variant_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_normal_and_none_stand_alone() {
    for input in [
        "normal small-caps",
        "small-caps normal",
        "normal normal",
        "none tabular-nums",
        "tabular-nums none",
        "none none",
        "none normal",
        "normal none",
        "none no-common-ligatures",
        "normal sub",
        "none sub",
    ] {
        assert!(parse_font_variant_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_rejects_junk_and_nothing() {
    for input in [
        "",
        "   ",
        "bold",
        "foo bar",
        "small-caps, tabular-nums",
        "small-caps;",
        "small",
        "auto",
        "inherit small-caps",
        "stylistic()",
        "stylistic( )",
        "stylistic(x",
        "stylistic x)",
        "stylistic (x)",
        "swash(a))",
        "swash)a(",
        "unknown(x)",
        "(x)",
        "()",
        "\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}",
        "sub\u{e9}",
    ] {
        assert!(parse_font_variant_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_font_variant_error_carries_the_trimmed_value() {
    assert_eq!(
        parse_font_variant_shorthand("  jis78 jis83 "),
        Err(InvalidValueErr("jis78 jis83"))
    );
    assert_eq!(
        parse_font_variant_shorthand("\tbold\n"),
        Err(InvalidValueErr("bold"))
    );
    assert_eq!(parse_font_variant_shorthand("  "), Err(InvalidValueErr("")));
}

// ---------------------------------------------------------------------------------------------
// font-synthesis
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_sets_the_four_longhands_in_the_documented_order() {
    let types: Vec<CssPropertyType> = synthesis("none")
        .iter()
        .map(CssProperty::get_type)
        .collect();
    assert_eq!(
        types,
        vec![
            CssPropertyType::FontSynthesisWeight,
            CssPropertyType::FontSynthesisStyle,
            CssPropertyType::FontSynthesisSmallCaps,
            CssPropertyType::FontSynthesisPosition,
        ]
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_none_turns_all_four_off() {
    assert_eq!(
        synthesis("none"),
        vec![
            CssProperty::FontSynthesisWeight(CssPropertyValue::Exact(StyleFontSynthesis::None)),
            CssProperty::FontSynthesisStyle(CssPropertyValue::Exact(StyleFontSynthesis::None)),
            CssProperty::FontSynthesisSmallCaps(CssPropertyValue::Exact(
                StyleFontSynthesis::None
            )),
            CssProperty::FontSynthesisPosition(CssPropertyValue::Exact(StyleFontSynthesis::None)),
        ]
    );
    assert_eq!(synthesis("  NONE "), synthesis_expansion(false, false, false, false));
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_keywords_turn_the_listed_longhands_on_and_the_others_off() {
    for (input, expected) in [
        ("weight", synthesis_expansion(true, false, false, false)),
        ("style", synthesis_expansion(false, true, false, false)),
        ("small-caps", synthesis_expansion(false, false, true, false)),
        ("position", synthesis_expansion(false, false, false, true)),
        ("weight style", synthesis_expansion(true, true, false, false)),
        ("small-caps position", synthesis_expansion(false, false, true, true)),
        ("weight position", synthesis_expansion(true, false, false, true)),
        (
            "weight style small-caps position",
            synthesis_expansion(true, true, true, true),
        ),
    ] {
        assert_eq!(synthesis(input), expected, "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_keywords_come_in_any_order() {
    let expected = synthesis_expansion(true, true, false, false);
    assert_eq!(synthesis("weight style"), expected);
    assert_eq!(synthesis("style weight"), expected);
    assert_eq!(
        synthesis("position small-caps style weight"),
        synthesis_expansion(true, true, true, true)
    );
    assert_eq!(
        synthesis("small-caps weight"),
        synthesis_expansion(true, false, true, false)
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_keywords_ignore_ascii_case_and_extra_white_space() {
    let expected = synthesis_expansion(true, true, false, false);
    assert_eq!(synthesis("WEIGHT Style"), expected);
    assert_eq!(synthesis("  weight \t style\n"), expected);
    assert_eq!(
        synthesis("Small-Caps"),
        synthesis_expansion(false, false, true, false)
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_rejects_what_is_not_its_value() {
    for input in [
        "weight weight",
        "style weight style",
        "auto",
        "bold",
        "",
        "   ",
        "none weight",
        "weight none",
        "none none",
        "weight, style",
        "weights",
        "small caps",
        "small-caps small-caps",
        "position;",
        "inherit",
        "\u{91CD}\u{307F}",
    ] {
        assert!(parse_font_synthesis_shorthand(input).is_err(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_font_synthesis_error_carries_the_trimmed_value() {
    assert_eq!(
        parse_font_synthesis_shorthand("  auto "),
        Err(InvalidValueErr("auto"))
    );
    assert_eq!(
        parse_font_synthesis_shorthand("weight weight"),
        Err(InvalidValueErr("weight weight"))
    );
    assert_eq!(parse_font_synthesis_shorthand(""), Err(InvalidValueErr("")));
}

// ---------------------------------------------------------------------------------------------
// every shorthand: odd input is an error, never a panic
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn odd_input_never_panics() {
    let long = "a ".repeat(2000);
    let deep = format!("{}x{}", "(".repeat(500), ")".repeat(500));
    for input in [
        "",
        " ",
        "/",
        "//",
        "///",
        "12px/",
        "/12px",
        "\"",
        "'",
        "\"\"",
        "(",
        ")",
        "((",
        "))",
        ")(",
        ",",
        ",,",
        "\0",
        "\u{1D11E}",
        "\u{1D11E} 12px a",
        "12px \u{1D11E}",
        "12px\u{A0}a",
        "oblique",
        "oblique oblique",
        "oblique \u{1D11E}",
        "normal/",
        "normal normal normal normal normal normal",
        "-",
        "+",
        ".",
        "1e999",
        "12px/1e999 a",
        "swash(",
        "swash()",
        "stylistic(\u{1D11E})",
        "\u{e9}",
        "system:",
        "system:\u{1D11E}",
        long.as_str(),
        deep.as_str(),
    ] {
        let _ = parse_font_shorthand(input);
        let _ = parse_font_variant_shorthand(input);
        let _ = parse_font_synthesis_shorthand(input);
    }
}
