//! The font properties CSS Fonts 4 adds, through the property machinery: `font-stretch` (spelled
//! `font-width` too), `font-variation-settings`, `font-optical-sizing`, `font-kerning`,
//! `font-variant-ligatures`, `font-variant-east-asian` and the four `font-synthesis-*` longhands;
//! the shorthands `font-synthesis`, `font-variant` and `font` that set them; and the numeric
//! `font-weight` and the angled `font-style` reaching the parser.
//!
//! The value types are tested next to their definitions. This file pins what `property.rs` makes
//! of them: the key map and the categories, the parser, the constructors and accessors, the
//! CSS-wide keywords, the shorthands.

use super::*;
use crate::css::CssWideKeyword;

// ---- helpers -----------------------------------------------------------------------------------

/// One sample of every new longhand.
struct Sample {
    /// The property type.
    ty: CssPropertyType,
    /// The name the key map has for it.
    name: &'static str,
    /// The CSS text of `prop`'s value.
    printed: &'static str,
    /// A value that is not the property's initial one.
    prop: CssProperty,
}

/// The ten new longhands, each with a value that is not its initial one.
fn samples() -> Vec<Sample> {
    vec![
        Sample {
            ty: CssPropertyType::FontStretch,
            name: "font-stretch",
            printed: "87.5%",
            prop: CssProperty::font_stretch(StyleFontStretch::from_percent(87.5)),
        },
        Sample {
            ty: CssPropertyType::FontVariationSettings,
            name: "font-variation-settings",
            printed: r#""wght" 650, "wdth" 87.5"#,
            prop: CssProperty::font_variation_settings(
                StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0), (*b"wdth", 87.5)]),
            ),
        },
        Sample {
            ty: CssPropertyType::FontOpticalSizing,
            name: "font-optical-sizing",
            printed: "none",
            prop: CssProperty::font_optical_sizing(StyleFontOpticalSizing::None),
        },
        Sample {
            ty: CssPropertyType::FontKerning,
            name: "font-kerning",
            printed: "normal",
            prop: CssProperty::font_kerning(StyleFontKerning::Normal),
        },
        Sample {
            ty: CssPropertyType::FontVariantLigatures,
            name: "font-variant-ligatures",
            printed: "common-ligatures no-contextual",
            prop: CssProperty::font_variant_ligatures(StyleFontVariantLigatures {
                common: StyleFontLigatureChoice::On,
                contextual: StyleFontLigatureChoice::Off,
                ..StyleFontVariantLigatures::NORMAL
            }),
        },
        Sample {
            ty: CssPropertyType::FontVariantEastAsian,
            name: "font-variant-east-asian",
            printed: "jis78 ruby",
            prop: CssProperty::font_variant_east_asian(StyleFontVariantEastAsian {
                jis78: true,
                ruby: true,
                ..StyleFontVariantEastAsian::NORMAL
            }),
        },
        Sample {
            ty: CssPropertyType::FontSynthesisWeight,
            name: "font-synthesis-weight",
            printed: "none",
            prop: CssProperty::font_synthesis_weight(StyleFontSynthesis::None),
        },
        Sample {
            ty: CssPropertyType::FontSynthesisStyle,
            name: "font-synthesis-style",
            printed: "none",
            prop: CssProperty::font_synthesis_style(StyleFontSynthesis::None),
        },
        Sample {
            ty: CssPropertyType::FontSynthesisSmallCaps,
            name: "font-synthesis-small-caps",
            printed: "none",
            prop: CssProperty::font_synthesis_small_caps(StyleFontSynthesis::None),
        },
        Sample {
            ty: CssPropertyType::FontSynthesisPosition,
            name: "font-synthesis-position",
            printed: "none",
            prop: CssProperty::font_synthesis_position(StyleFontSynthesis::None),
        },
    ]
}

/// The CSS-wide keywords as written, with what `css_wide_keyword()` says of them.
static CSS_WIDE_KEYWORDS: [(&str, CssWideKeyword); 4] = [
    ("initial", CssWideKeyword::Initial),
    ("inherit", CssWideKeyword::Inherit),
    ("unset", CssWideKeyword::Unset),
    ("revert", CssWideKeyword::Revert),
];

/// `ty` with `keyword` for its value, made the ways the code makes it.
fn with_keyword(ty: CssPropertyType, keyword: CssWideKeyword) -> CssProperty {
    match keyword {
        CssWideKeyword::Initial => CssProperty::initial(ty),
        CssWideKeyword::Inherit => CssProperty::inherit(ty),
        CssWideKeyword::Unset => css_property_from_type!(ty, Unset),
        CssWideKeyword::Revert => css_property_from_type!(ty, Revert),
    }
}

/// The accessors of the new longhands, each asked whether it holds `prop`, in the order of
/// [`samples`].
fn accessor_hits(prop: &CssProperty) -> [bool; 10] {
    [
        prop.as_font_stretch().is_some(),
        prop.as_font_variation_settings().is_some(),
        prop.as_font_optical_sizing().is_some(),
        prop.as_font_kerning().is_some(),
        prop.as_font_variant_ligatures().is_some(),
        prop.as_font_variant_east_asian().is_some(),
        prop.as_font_synthesis_weight().is_some(),
        prop.as_font_synthesis_style().is_some(),
        prop.as_font_synthesis_small_caps().is_some(),
        prop.as_font_synthesis_position().is_some(),
    ]
}

/// The types of `props`, in order.
fn types_of(props: &[CssProperty]) -> Vec<CssPropertyType> {
    props.iter().map(CssProperty::get_type).collect()
}

/// `ty` parsed from `input`, which must parse.
#[cfg(feature = "parser")]
fn parsed(ty: CssPropertyType, input: &str) -> CssProperty {
    parse_css_property(ty, input)
        .unwrap_or_else(|e| panic!("`{}: {input}` must parse: {e:?}", ty.to_str()))
}

/// Whether the parser turns `ty: input` down.
#[cfg(feature = "parser")]
fn is_rejected(ty: CssPropertyType, input: &str) -> bool {
    parse_css_property(ty, input).is_err()
}

/// The shorthand named `shorthand` expanded from `value`; the error is what the parser said, not
/// its text.
#[cfg(feature = "parser")]
fn expand(shorthand: &str, value: &str) -> Result<Vec<CssProperty>, ()> {
    let key = CombinedCssPropertyType::from_str(shorthand, &CssKeyMap::get())
        .unwrap_or_else(|| panic!("`{shorthand}` is a shorthand"));
    parse_combined_css_property(key, value).map_err(|_| ())
}

/// The families of a `font-family` value.
#[cfg(feature = "parser")]
fn family(text: &str) -> StyleFontFamilyVec {
    parse_style_font_family(text).unwrap_or_else(|e| panic!("`{text}` is a font-family: {e:?}"))
}

/// A `line-height` value.
#[cfg(feature = "parser")]
fn line_height(text: &str) -> StyleLineHeight {
    parse_style_line_height(text).unwrap_or_else(|e| panic!("`{text}` is a line-height: {e:?}"))
}

/// The four longhands of `font-synthesis`, in order: weight, style, small capitals, position.
fn synthesis(
    weight: StyleFontSynthesis,
    style: StyleFontSynthesis,
    small_caps: StyleFontSynthesis,
    position: StyleFontSynthesis,
) -> Vec<CssProperty> {
    vec![
        CssProperty::font_synthesis_weight(weight),
        CssProperty::font_synthesis_style(style),
        CssProperty::font_synthesis_small_caps(small_caps),
        CssProperty::font_synthesis_position(position),
    ]
}

/// The four longhands of `font-variant`, in order: numeric, caps, ligatures, East Asian.
fn variant(
    numeric: StyleFontVariantNumeric,
    caps: StyleFontVariantCaps,
    ligatures: StyleFontVariantLigatures,
    east_asian: StyleFontVariantEastAsian,
) -> Vec<CssProperty> {
    vec![
        CssProperty::font_variant_numeric(numeric),
        CssProperty::font_variant_caps(caps),
        CssProperty::font_variant_ligatures(ligatures),
        CssProperty::font_variant_east_asian(east_asian),
    ]
}

/// The ligature groups as a value, in the grammar's order.
fn ligatures(
    common: StyleFontLigatureChoice,
    discretionary: StyleFontLigatureChoice,
    historical: StyleFontLigatureChoice,
    contextual: StyleFontLigatureChoice,
) -> StyleFontVariantLigatures {
    StyleFontVariantLigatures {
        common,
        discretionary,
        historical,
        contextual,
    }
}

/// The East Asian value with the whitespace separated `keywords` set (`normal` for none).
fn east_asian(keywords: &str) -> StyleFontVariantEastAsian {
    let mut value = StyleFontVariantEastAsian::NORMAL;
    for keyword in keywords.split_whitespace() {
        match keyword {
            "jis78" => value.jis78 = true,
            "jis83" => value.jis83 = true,
            "jis90" => value.jis90 = true,
            "jis04" => value.jis04 = true,
            "simplified" => value.simplified = true,
            "traditional" => value.traditional = true,
            "full-width" => value.full_width = true,
            "proportional-width" => value.proportional_width = true,
            "ruby" => value.ruby = true,
            other => panic!("`{other}` is no font-variant-east-asian keyword"),
        }
    }
    value
}

/// The seven longhands `font` names (the ones it can set from its value), in the order it returns
/// them.
static FONT_NAMED: [CssPropertyType; 7] = [
    CssPropertyType::FontStyle,
    CssPropertyType::FontVariantCaps,
    CssPropertyType::FontWeight,
    CssPropertyType::FontStretch,
    CssPropertyType::FontSize,
    CssPropertyType::LineHeight,
    CssPropertyType::FontFamily,
];

/// The longhands `font` resets to their initial value, after the seven it names, in order.
static FONT_RESETS: [CssPropertyType; 11] = [
    CssPropertyType::FontVariantNumeric,
    CssPropertyType::FontVariantLigatures,
    CssPropertyType::FontVariantEastAsian,
    CssPropertyType::FontKerning,
    CssPropertyType::FontOpticalSizing,
    CssPropertyType::FontVariationSettings,
    CssPropertyType::FontFeatureSettings,
    CssPropertyType::FontSynthesisWeight,
    CssPropertyType::FontSynthesisStyle,
    CssPropertyType::FontSynthesisSmallCaps,
    CssPropertyType::FontSynthesisPosition,
];

/// The longhands `font-variant` sets, in order.
static FONT_VARIANT_ORDER: [CssPropertyType; 4] = [
    CssPropertyType::FontVariantNumeric,
    CssPropertyType::FontVariantCaps,
    CssPropertyType::FontVariantLigatures,
    CssPropertyType::FontVariantEastAsian,
];

/// The longhands `font-synthesis` sets, in order.
static FONT_SYNTHESIS_ORDER: [CssPropertyType; 4] = [
    CssPropertyType::FontSynthesisWeight,
    CssPropertyType::FontSynthesisStyle,
    CssPropertyType::FontSynthesisSmallCaps,
    CssPropertyType::FontSynthesisPosition,
];

/// All eighteen longhands of `font`, in the order it returns them.
fn font_order() -> Vec<CssPropertyType> {
    FONT_NAMED.iter().chain(FONT_RESETS.iter()).copied().collect()
}

/// What `font` expands to: the seven longhands it names (an omitted one is the CSS-wide `initial`)
/// and then the eleven it resets.
fn font_expansion(
    style: Option<StyleFontStyle>,
    caps: Option<StyleFontVariantCaps>,
    weight: Option<StyleFontWeight>,
    stretch: Option<StyleFontStretch>,
    size: StyleFontSize,
    line_height: Option<StyleLineHeight>,
    family: StyleFontFamilyVec,
) -> Vec<CssProperty> {
    let mut props = vec![
        style.map_or(
            CssProperty::initial(CssPropertyType::FontStyle),
            CssProperty::font_style,
        ),
        caps.map_or(
            CssProperty::initial(CssPropertyType::FontVariantCaps),
            CssProperty::font_variant_caps,
        ),
        weight.map_or(
            CssProperty::initial(CssPropertyType::FontWeight),
            CssProperty::font_weight,
        ),
        stretch.map_or(
            CssProperty::initial(CssPropertyType::FontStretch),
            CssProperty::font_stretch,
        ),
        CssProperty::font_size(size),
        line_height.map_or(
            CssProperty::initial(CssPropertyType::LineHeight),
            CssProperty::line_height,
        ),
        CssProperty::font_family(family),
    ];
    props.extend(FONT_RESETS.iter().map(|ty| CssProperty::initial(*ty)));
    props
}

// ---- the key map -------------------------------------------------------------------------------

/// Every new longhand is a property type with a name, and the name finds the type again.
#[test]
fn every_new_longhand_is_in_the_key_map_under_its_name() {
    let map = CssKeyMap::get();
    for s in samples() {
        assert_eq!(
            CssPropertyType::from_str(s.name, &map),
            Some(s.ty),
            "`{}`",
            s.name
        );
        assert_eq!(s.ty.to_str(), s.name);
        assert_eq!(format!("{}", s.ty), s.name);
        assert_eq!(format!("{:?}", s.ty), s.name);
        assert_eq!(s.prop.get_type(), s.ty, "`{}`", s.name);
        assert_eq!(s.prop.key(), s.name);
    }
}

/// `font-width` is the new spelling of `font-stretch`: the same type, which prints as the old
/// name.
#[test]
fn font_width_is_another_name_of_font_stretch() {
    let map = CssKeyMap::get();
    assert_eq!(
        CssPropertyType::from_str("font-width", &map),
        Some(CssPropertyType::FontStretch)
    );
    assert_eq!(
        CssPropertyType::from_str("font-stretch", &map),
        Some(CssPropertyType::FontStretch)
    );
    assert_eq!(CssPropertyType::FontStretch.to_str(), "font-stretch");
}

#[cfg(feature = "parser")]
#[test]
fn a_font_width_declaration_is_a_font_stretch_value() {
    let map = CssKeyMap::get();
    let ty = CssPropertyType::from_str("font-width", &map).expect("`font-width` is a property");
    let prop = parsed(ty, "semi-condensed");
    assert_eq!(
        prop,
        CssProperty::font_stretch(StyleFontStretch::from_percent(87.5))
    );
    assert_eq!(
        prop.format_css(),
        "font-stretch: 87.5%;",
        "it prints under the name `font-stretch`"
    );
}

/// The loops over `CssPropertyType::ALL` below only see what the list has.
#[test]
fn the_new_longhands_are_listed_in_all() {
    for s in samples() {
        assert!(
            CssPropertyType::ALL.contains(&s.ty),
            "{:?} is missing from CssPropertyType::ALL",
            s.ty
        );
    }
}

/// Every property type has a name, and the key map finds the type by it.
#[test]
fn every_property_type_has_a_name_that_the_key_map_resolves_back() {
    let map = CssKeyMap::get();
    for &ty in CssPropertyType::ALL {
        let name = ty.to_str();
        assert!(!name.is_empty(), "{ty:?} has no name");
        assert_eq!(
            CssPropertyType::from_str(name, &map),
            Some(ty),
            "`{name}` is not in the key map for {ty:?}"
        );
    }
}

// ---- the categories ----------------------------------------------------------------------------

#[test]
fn every_new_longhand_is_inherited() {
    for s in samples() {
        assert!(s.ty.is_inheritable(), "`{}` is inherited", s.name);
    }
    for ty in [
        CssPropertyType::FontWeight,
        CssPropertyType::FontStyle,
        CssPropertyType::FontVariantCaps,
        CssPropertyType::FontVariantNumeric,
        CssPropertyType::FontFeatureSettings,
    ] {
        assert!(ty.is_inheritable(), "`{}` is inherited", ty.to_str());
    }
}

/// `font-stretch` is a number a node carries in the compact cache; the other nine are rare
/// enough to be read from the property cache.
#[test]
fn only_font_stretch_has_a_compact_encoding_among_the_new_longhands() {
    for s in samples() {
        assert_eq!(
            s.ty.has_compact_encoding(),
            s.ty == CssPropertyType::FontStretch,
            "`{}`",
            s.name
        );
    }
    assert!(CssPropertyType::FontWeight.has_compact_encoding());
    assert!(CssPropertyType::FontStyle.has_compact_encoding());
}

/// A font longhand reshapes the inline context its text is in, and nothing else.
#[test]
fn a_font_longhand_reshapes_only_the_inline_context_it_is_in() {
    for s in samples() {
        assert_eq!(
            s.ty.relayout_scope(true),
            RelayoutScope::IfcOnly,
            "`{}` in an inline context",
            s.name
        );
        assert_eq!(
            s.ty.relayout_scope(false),
            RelayoutScope::None,
            "`{}` outside one",
            s.name
        );
        assert!(s.ty.can_trigger_relayout(), "`{}`", s.name);
        assert_eq!(
            s.ty.get_category(),
            CssPropertyCategory::InheritedLayout,
            "`{}`",
            s.name
        );
    }
    assert_eq!(
        CssPropertyType::FontFeatureSettings.relayout_scope(true),
        RelayoutScope::IfcOnly
    );
    assert_eq!(
        CssPropertyType::FontFeatureSettings.relayout_scope(false),
        RelayoutScope::None
    );
}

// ---- parsing: the numeric weight and the angled style ------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn font_weight_takes_any_number_from_1_to_1000() {
    let ty = CssPropertyType::FontWeight;
    for (input, weight, printed) in [
        ("350", StyleFontWeight::from_number(350.0), "350"),
        ("350.5", StyleFontWeight::from_number(350.5), "350.5"),
        ("1", StyleFontWeight::from_number(1.0), "1"),
        ("1000", StyleFontWeight::from_number(1000.0), "1000"),
        ("400", StyleFontWeight::NORMAL, "normal"),
        ("700", StyleFontWeight::BOLD, "bold"),
        ("normal", StyleFontWeight::NORMAL, "normal"),
        ("bold", StyleFontWeight::BOLD, "bold"),
        ("bolder", StyleFontWeight::Bolder, "bolder"),
        ("lighter", StyleFontWeight::Lighter, "lighter"),
        ("  600  ", StyleFontWeight::W600, "600"),
    ] {
        let prop = parsed(ty, input);
        assert_eq!(
            prop,
            CssProperty::FontWeight(CssPropertyValue::Exact(weight)),
            "`font-weight: {input}`"
        );
        assert_eq!(prop.value(), printed, "`font-weight: {input}`");
        assert_eq!(prop.format_css(), format!("font-weight: {printed};"));
    }
    for bad in ["0", "1001", "-1", "1e3", "4e2", "abc", "350 400", ""] {
        assert!(is_rejected(ty, bad), "`font-weight: {bad}` is no weight");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_weight_350_is_the_number_350() {
    assert_eq!(
        parsed(CssPropertyType::FontWeight, "350"),
        CssProperty::FontWeight(CssPropertyValue::Exact(StyleFontWeight::from_number(350.0)))
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_style_takes_an_oblique_angle() {
    let ty = CssPropertyType::FontStyle;
    for (input, style, printed) in [
        ("normal", StyleFontStyle::Normal, "normal"),
        ("italic", StyleFontStyle::Italic, "italic"),
        ("oblique", StyleFontStyle::OBLIQUE, "oblique"),
        (
            "oblique 12deg",
            StyleFontStyle::Oblique(FloatValue::const_new(12)),
            "oblique 12deg",
        ),
        (
            "oblique -10deg",
            StyleFontStyle::Oblique(FloatValue::const_new(-10)),
            "oblique -10deg",
        ),
        ("oblique 14deg", StyleFontStyle::OBLIQUE, "oblique"),
        (
            "oblique 0.1turn",
            StyleFontStyle::Oblique(FloatValue::const_new(36)),
            "oblique 36deg",
        ),
        (
            "  oblique   20deg ",
            StyleFontStyle::Oblique(FloatValue::const_new(20)),
            "oblique 20deg",
        ),
    ] {
        let prop = parsed(ty, input);
        assert_eq!(
            prop,
            CssProperty::FontStyle(CssPropertyValue::Exact(style)),
            "`font-style: {input}`"
        );
        assert_eq!(prop.value(), printed, "`font-style: {input}`");
        assert_eq!(prop.format_css(), format!("font-style: {printed};"));
    }
    for bad in [
        "oblique 91deg",
        "oblique -91deg",
        "italic 10deg",
        "oblique 10",
        "oblique 10%",
        "slanted",
        "",
    ] {
        assert!(is_rejected(ty, bad), "`font-style: {bad}` is no style");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_style_oblique_12deg_is_an_oblique_of_12_degrees() {
    assert_eq!(
        parsed(CssPropertyType::FontStyle, "oblique 12deg"),
        CssProperty::FontStyle(CssPropertyValue::Exact(StyleFontStyle::Oblique(
            FloatValue::const_new(12)
        )))
    );
}

// ---- parsing: the new longhands ----------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn font_stretch_parses_keywords_and_percentages_to_the_percentage() {
    let ty = CssPropertyType::FontStretch;
    for (input, percent, printed) in [
        ("ultra-condensed", 50.0, "50%"),
        ("extra-condensed", 62.5, "62.5%"),
        ("condensed", 75.0, "75%"),
        ("semi-condensed", 87.5, "87.5%"),
        ("normal", 100.0, "100%"),
        ("semi-expanded", 112.5, "112.5%"),
        ("expanded", 125.0, "125%"),
        ("extra-expanded", 150.0, "150%"),
        ("ultra-expanded", 200.0, "200%"),
        ("87.5%", 87.5, "87.5%"),
        ("100%", 100.0, "100%"),
        ("0%", 0.0, "0%"),
        ("33.3%", 33.3, "33.3%"),
        ("  150%  ", 150.0, "150%"),
    ] {
        let prop = parsed(ty, input);
        assert_eq!(
            prop,
            CssProperty::FontStretch(CssPropertyValue::Exact(StyleFontStretch::from_percent(
                percent
            ))),
            "`font-stretch: {input}`"
        );
        assert_eq!(prop.value(), printed, "`font-stretch: {input}`");
        assert_eq!(prop.format_css(), format!("font-stretch: {printed};"));
    }
    assert_eq!(
        parsed(ty, "normal"),
        CssProperty::font_stretch(StyleFontStretch::NORMAL)
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_stretch_rejects_what_is_not_a_width() {
    let ty = CssPropertyType::FontStretch;
    for bad in [
        "-5%",
        "100",
        "0.5",
        "%",
        "abc%",
        "NaN%",
        "inf%",
        "50px",
        "bold",
        "condensed expanded",
        "50% 60%",
        "auto",
        "",
    ] {
        assert!(is_rejected(ty, bad), "`font-stretch: {bad}` is no width");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variation_settings_parses_the_axes_and_prints_them_canonically() {
    let ty = CssPropertyType::FontVariationSettings;
    let cases: Vec<(&str, &str, Vec<([u8; 4], f32)>)> = vec![
        ("normal", "normal", vec![]),
        ("  NORMAL ", "normal", vec![]),
        (r#""wght" 650"#, r#""wght" 650"#, vec![(*b"wght", 650.0)]),
        (r#"'wght' 650"#, r#""wght" 650"#, vec![(*b"wght", 650.0)]),
        (
            r#""wght" 650, "wdth" 87.5"#,
            r#""wght" 650, "wdth" 87.5"#,
            vec![(*b"wght", 650.0), (*b"wdth", 87.5)],
        ),
        (
            r#""wght" 650,"wdth" 87.5"#,
            r#""wght" 650, "wdth" 87.5"#,
            vec![(*b"wght", 650.0), (*b"wdth", 87.5)],
        ),
        (r#""slnt" -15"#, r#""slnt" -15"#, vec![(*b"slnt", -15.0)]),
        (r#""opsz" .25"#, r#""opsz" 0.25"#, vec![(*b"opsz", 0.25)]),
        (r#""wght" +650"#, r#""wght" 650"#, vec![(*b"wght", 650.0)]),
        (r#""wght"   650"#, r#""wght" 650"#, vec![(*b"wght", 650.0)]),
        (r#""GRAD" 88"#, r#""GRAD" 88"#, vec![(*b"GRAD", 88.0)]),
        (
            r#""wght" 400, "wght" 700"#,
            r#""wght" 700"#,
            vec![(*b"wght", 700.0)],
        ),
        (
            r#""wght" 400, "wdth" 80, "wght" 700"#,
            r#""wdth" 80, "wght" 700"#,
            vec![(*b"wdth", 80.0), (*b"wght", 700.0)],
        ),
    ];
    for (input, printed, axes) in cases {
        let prop = parsed(ty, input);
        assert_eq!(prop.value(), printed, "`font-variation-settings: {input}`");
        let Some(CssPropertyValue::Exact(settings)) = prop.as_font_variation_settings() else {
            panic!("`font-variation-settings: {input}` is an exact value, not {prop:?}");
        };
        assert_eq!(settings.variations(), axes, "`{input}`");
        assert_eq!(settings.is_normal(), axes.is_empty(), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variation_settings_rejects_what_is_not_a_list_of_tagged_numbers() {
    let ty = CssPropertyType::FontVariationSettings;
    for bad in [
        "wght 650",
        r#""wght""#,
        r#""wght" on"#,
        r#""wght" off"#,
        r#""wgh" 1"#,
        r#""weight" 1"#,
        "\"w\u{e9}ht\" 1",
        r#""wght" 1e3"#,
        r#""wght" inf"#,
        r#""wght" NaN"#,
        r#""wght" 650."#,
        r#""wght" 650px"#,
        r#""wght" 650%"#,
        r#""wght" 1 2"#,
        r#""wght" 650,"#,
        r#", "wght" 650"#,
        r#""wght" 650,, "wdth" 1"#,
        r#""wght" 650 "wdth" 1"#,
        r#"normal, "wght" 1"#,
        r#"'wght" 650"#,
        r#""wght 650"#,
        "",
    ] {
        assert!(
            is_rejected(ty, bad),
            "`font-variation-settings: {bad}` is no list of axes"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_optical_sizing_parses_auto_and_none() {
    let ty = CssPropertyType::FontOpticalSizing;
    assert_eq!(
        parsed(ty, "auto"),
        CssProperty::font_optical_sizing(StyleFontOpticalSizing::Auto)
    );
    assert_eq!(
        parsed(ty, "none"),
        CssProperty::font_optical_sizing(StyleFontOpticalSizing::None)
    );
    assert_eq!(parsed(ty, "auto").value(), "auto");
    assert_eq!(parsed(ty, "none").value(), "none");
    for bad in ["normal", "on", "off", "auto none", "bold", ""] {
        assert!(is_rejected(ty, bad), "`font-optical-sizing: {bad}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_kerning_parses_auto_normal_and_none() {
    let ty = CssPropertyType::FontKerning;
    for (input, kerning) in [
        ("auto", StyleFontKerning::Auto),
        ("normal", StyleFontKerning::Normal),
        ("none", StyleFontKerning::None),
    ] {
        assert_eq!(
            parsed(ty, input),
            CssProperty::font_kerning(kerning),
            "`font-kerning: {input}`"
        );
        assert_eq!(parsed(ty, input).value(), input);
    }
    for bad in ["on", "off", "auto normal", "bold", ""] {
        assert!(is_rejected(ty, bad), "`font-kerning: {bad}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_ligatures_parses_the_groups_in_any_order_and_prints_them_in_the_grammars() {
    use StyleFontLigatureChoice::{Off, On, Unset};
    let ty = CssPropertyType::FontVariantLigatures;
    for (input, expected, printed) in [
        ("normal", StyleFontVariantLigatures::NORMAL, "normal"),
        ("none", StyleFontVariantLigatures::NONE, "none"),
        (
            "common-ligatures",
            ligatures(On, Unset, Unset, Unset),
            "common-ligatures",
        ),
        (
            "no-common-ligatures",
            ligatures(Off, Unset, Unset, Unset),
            "no-common-ligatures",
        ),
        (
            "discretionary-ligatures",
            ligatures(Unset, On, Unset, Unset),
            "discretionary-ligatures",
        ),
        (
            "no-discretionary-ligatures",
            ligatures(Unset, Off, Unset, Unset),
            "no-discretionary-ligatures",
        ),
        (
            "historical-ligatures",
            ligatures(Unset, Unset, On, Unset),
            "historical-ligatures",
        ),
        (
            "no-historical-ligatures",
            ligatures(Unset, Unset, Off, Unset),
            "no-historical-ligatures",
        ),
        ("contextual", ligatures(Unset, Unset, Unset, On), "contextual"),
        (
            "no-contextual",
            ligatures(Unset, Unset, Unset, Off),
            "no-contextual",
        ),
        (
            "no-contextual common-ligatures",
            ligatures(On, Unset, Unset, Off),
            "common-ligatures no-contextual",
        ),
        (
            "historical-ligatures discretionary-ligatures no-common-ligatures",
            ligatures(Off, On, On, Unset),
            "no-common-ligatures discretionary-ligatures historical-ligatures",
        ),
        (
            "common-ligatures discretionary-ligatures historical-ligatures contextual",
            ligatures(On, On, On, On),
            "common-ligatures discretionary-ligatures historical-ligatures contextual",
        ),
        (
            "no-common-ligatures no-discretionary-ligatures no-historical-ligatures no-contextual",
            StyleFontVariantLigatures::NONE,
            "none",
        ),
        (
            "  Common-Ligatures   NO-CONTEXTUAL ",
            ligatures(On, Unset, Unset, Off),
            "common-ligatures no-contextual",
        ),
    ] {
        let prop = parsed(ty, input);
        assert_eq!(
            prop,
            CssProperty::font_variant_ligatures(expected),
            "`font-variant-ligatures: {input}`"
        );
        assert_eq!(prop.value(), printed, "`font-variant-ligatures: {input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_ligatures_rejects_two_values_of_one_group_and_normal_or_none_with_company() {
    let ty = CssPropertyType::FontVariantLigatures;
    for bad in [
        "common-ligatures no-common-ligatures",
        "common-ligatures common-ligatures",
        "contextual no-contextual",
        "normal common-ligatures",
        "common-ligatures normal",
        "none common-ligatures",
        "common-ligatures none",
        "none none",
        "normal normal",
        "liga",
        "ligatures",
        "common",
        "no-ligatures",
        "common-ligatures,contextual",
        "",
    ] {
        assert!(
            is_rejected(ty, bad),
            "`font-variant-ligatures: {bad}` is no ligature value"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_east_asian_parses_the_keywords_in_any_order_and_prints_them_in_the_grammars() {
    let ty = CssPropertyType::FontVariantEastAsian;
    for (input, keywords, printed) in [
        ("normal", "", "normal"),
        ("jis78", "jis78", "jis78"),
        ("jis83", "jis83", "jis83"),
        ("jis90", "jis90", "jis90"),
        ("jis04", "jis04", "jis04"),
        ("simplified", "simplified", "simplified"),
        ("traditional", "traditional", "traditional"),
        ("full-width", "full-width", "full-width"),
        ("proportional-width", "proportional-width", "proportional-width"),
        ("ruby", "ruby", "ruby"),
        ("ruby jis78", "jis78 ruby", "jis78 ruby"),
        (
            "proportional-width traditional ruby",
            "traditional proportional-width ruby",
            "traditional proportional-width ruby",
        ),
        ("full-width jis04", "jis04 full-width", "jis04 full-width"),
        (
            "  JIS90   Full-Width  RUBY ",
            "jis90 full-width ruby",
            "jis90 full-width ruby",
        ),
    ] {
        let prop = parsed(ty, input);
        assert_eq!(
            prop,
            CssProperty::font_variant_east_asian(east_asian(keywords)),
            "`font-variant-east-asian: {input}`"
        );
        assert_eq!(prop.value(), printed, "`font-variant-east-asian: {input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_east_asian_rejects_two_values_of_one_group_and_normal_with_company() {
    let ty = CssPropertyType::FontVariantEastAsian;
    for bad in [
        "jis78 jis83",
        "jis78 jis78",
        "simplified traditional",
        "jis04 simplified",
        "full-width proportional-width",
        "ruby ruby",
        "normal ruby",
        "ruby normal",
        "jis",
        "full width",
        "bold",
        "",
    ] {
        assert!(
            is_rejected(ty, bad),
            "`font-variant-east-asian: {bad}` is no East Asian value"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn the_font_synthesis_longhands_take_auto_and_none() {
    for (ty, name) in [
        (CssPropertyType::FontSynthesisWeight, "font-synthesis-weight"),
        (CssPropertyType::FontSynthesisStyle, "font-synthesis-style"),
        (
            CssPropertyType::FontSynthesisSmallCaps,
            "font-synthesis-small-caps",
        ),
        (
            CssPropertyType::FontSynthesisPosition,
            "font-synthesis-position",
        ),
    ] {
        for (input, expected) in [
            ("auto", StyleFontSynthesis::Auto),
            ("none", StyleFontSynthesis::None),
        ] {
            let prop = parsed(ty, input);
            assert_eq!(prop.get_type(), ty, "`{name}: {input}`");
            let as_value = match ty {
                CssPropertyType::FontSynthesisWeight => prop.as_font_synthesis_weight(),
                CssPropertyType::FontSynthesisStyle => prop.as_font_synthesis_style(),
                CssPropertyType::FontSynthesisSmallCaps => prop.as_font_synthesis_small_caps(),
                _ => prop.as_font_synthesis_position(),
            };
            assert_eq!(
                as_value,
                Some(&CssPropertyValue::Exact(expected)),
                "`{name}: {input}`"
            );
            assert_eq!(prop.value(), input, "`{name}: {input}`");
        }
        for bad in ["weight", "normal", "auto none", "none auto", "auto auto", "on", ""] {
            assert!(is_rejected(ty, bad), "`{name}: {bad}`");
        }
    }
}

/// `auto` and `none` are VALUES of these properties (the property's initial value and the off
/// switch), not the generic keywords: read back as "no value" they would give the property's
/// initial value, or the parent's, in place of what was asked for.
#[cfg(feature = "parser")]
#[test]
fn typed_auto_and_none_are_exact_values_and_not_the_generic_keywords() {
    assert_eq!(
        parsed(CssPropertyType::FontOpticalSizing, "auto"),
        CssProperty::font_optical_sizing(StyleFontOpticalSizing::Auto)
    );
    assert_eq!(
        parsed(CssPropertyType::FontOpticalSizing, "none"),
        CssProperty::font_optical_sizing(StyleFontOpticalSizing::None)
    );
    assert_eq!(
        parsed(CssPropertyType::FontKerning, "auto"),
        CssProperty::font_kerning(StyleFontKerning::Auto)
    );
    assert_eq!(
        parsed(CssPropertyType::FontKerning, "none"),
        CssProperty::font_kerning(StyleFontKerning::None)
    );
    assert_eq!(
        parsed(CssPropertyType::FontVariantLigatures, "none"),
        CssProperty::font_variant_ligatures(StyleFontVariantLigatures::NONE),
        "`font-variant-ligatures: none` turns the ligatures off"
    );
    let makers: [(CssPropertyType, fn(StyleFontSynthesis) -> CssProperty); 4] = [
        (
            CssPropertyType::FontSynthesisWeight,
            CssProperty::font_synthesis_weight,
        ),
        (
            CssPropertyType::FontSynthesisStyle,
            CssProperty::font_synthesis_style,
        ),
        (
            CssPropertyType::FontSynthesisSmallCaps,
            CssProperty::font_synthesis_small_caps,
        ),
        (
            CssPropertyType::FontSynthesisPosition,
            CssProperty::font_synthesis_position,
        ),
    ];
    for (ty, make) in makers {
        assert_eq!(
            parsed(ty, "auto"),
            make(StyleFontSynthesis::Auto),
            "`{}: auto`",
            ty.to_str()
        );
        assert_eq!(
            parsed(ty, "none"),
            make(StyleFontSynthesis::None),
            "`{}: none`",
            ty.to_str()
        );
    }
}

/// Keywords of the new longhands ignore ASCII case, the value is trimmed.
#[cfg(feature = "parser")]
#[test]
fn the_keywords_of_the_new_longhands_ignore_ascii_case_and_the_value_is_trimmed() {
    for (ty, input, printed) in [
        (CssPropertyType::FontStretch, "  Semi-Condensed ", "87.5%"),
        (CssPropertyType::FontVariationSettings, " NORMAL ", "normal"),
        (CssPropertyType::FontOpticalSizing, " AUTO ", "auto"),
        (CssPropertyType::FontOpticalSizing, "None", "none"),
        (CssPropertyType::FontKerning, "NORMAL", "normal"),
        (CssPropertyType::FontKerning, "None", "none"),
        (
            CssPropertyType::FontVariantLigatures,
            " No-Common-Ligatures ",
            "no-common-ligatures",
        ),
        (CssPropertyType::FontVariantLigatures, "NONE", "none"),
        (
            CssPropertyType::FontVariantEastAsian,
            " JIS78 Ruby ",
            "jis78 ruby",
        ),
        (CssPropertyType::FontSynthesisWeight, "AUTO", "auto"),
        (CssPropertyType::FontSynthesisStyle, "None", "none"),
        (CssPropertyType::FontSynthesisSmallCaps, "NONE", "none"),
        (CssPropertyType::FontSynthesisPosition, "Auto", "auto"),
    ] {
        let prop = parsed(ty, input);
        assert_eq!(prop.get_type(), ty, "`{}: {input}`", ty.to_str());
        assert_eq!(prop.value(), printed, "`{}: {input}`", ty.to_str());
    }
}

// ---- values: print, parse, constructors, accessors ---------------------------------------------

/// A value prints as the CSS the property takes, and that parses back to the same value.
#[cfg(feature = "parser")]
#[test]
fn a_new_longhand_value_prints_as_css_and_parses_back_to_itself() {
    for s in samples() {
        assert_eq!(s.prop.value(), s.printed, "`{}`", s.name);
        assert_eq!(
            s.prop.format_css(),
            format!("{}: {};", s.name, s.printed),
            "`{}`",
            s.name
        );
        assert_eq!(
            parsed(s.ty, &s.prop.value()),
            s.prop,
            "`{}: {}` parses back to the value it came from",
            s.name,
            s.printed
        );
    }
}

#[test]
fn a_new_longhand_value_prints_as_css() {
    for s in samples() {
        assert_eq!(s.prop.value(), s.printed, "`{}`", s.name);
        assert_eq!(
            s.prop.format_css(),
            format!("{}: {};", s.name, s.printed),
            "`{}`",
            s.name
        );
    }
    assert_eq!(
        CssProperty::font_stretch(StyleFontStretch::from_percent(87.5)).format_css(),
        "font-stretch: 87.5%;"
    );
}

/// Each constructor builds its own property type with an exact value, and only its own accessor
/// hands the value back.
#[test]
fn every_constructor_builds_one_property_and_only_its_accessor_finds_it() {
    let all = samples();
    assert_eq!(all.len(), 10);
    for (index, s) in all.iter().enumerate() {
        assert_eq!(s.prop.get_type(), s.ty, "`{}`", s.name);
        let mut expected = [false; 10];
        expected[index] = true;
        assert_eq!(accessor_hits(&s.prop), expected, "`{}`", s.name);
        // A CSS-wide keyword is a value of the same variant: the same accessor holds it.
        let inherited = CssProperty::inherit(s.ty);
        assert_eq!(accessor_hits(&inherited), expected, "`{}: inherit`", s.name);
    }
}

#[test]
fn the_accessors_hand_back_the_value_they_were_built_with() {
    let stretch = StyleFontStretch::from_percent(87.5);
    assert_eq!(
        CssProperty::font_stretch(stretch).as_font_stretch(),
        Some(&CssPropertyValue::Exact(stretch))
    );
    let settings = StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0)]);
    assert_eq!(
        CssProperty::font_variation_settings(settings.clone()).as_font_variation_settings(),
        Some(&CssPropertyValue::Exact(settings))
    );
    assert_eq!(
        CssProperty::font_optical_sizing(StyleFontOpticalSizing::None).as_font_optical_sizing(),
        Some(&CssPropertyValue::Exact(StyleFontOpticalSizing::None))
    );
    assert_eq!(
        CssProperty::font_kerning(StyleFontKerning::Normal).as_font_kerning(),
        Some(&CssPropertyValue::Exact(StyleFontKerning::Normal))
    );
    assert_eq!(
        CssProperty::font_variant_ligatures(StyleFontVariantLigatures::NONE)
            .as_font_variant_ligatures(),
        Some(&CssPropertyValue::Exact(StyleFontVariantLigatures::NONE))
    );
    let ruby = east_asian("ruby");
    assert_eq!(
        CssProperty::font_variant_east_asian(ruby).as_font_variant_east_asian(),
        Some(&CssPropertyValue::Exact(ruby))
    );
    assert_eq!(
        CssProperty::font_synthesis_weight(StyleFontSynthesis::None).as_font_synthesis_weight(),
        Some(&CssPropertyValue::Exact(StyleFontSynthesis::None))
    );
    assert_eq!(
        CssProperty::font_synthesis_style(StyleFontSynthesis::None).as_font_synthesis_style(),
        Some(&CssPropertyValue::Exact(StyleFontSynthesis::None))
    );
    assert_eq!(
        CssProperty::font_synthesis_small_caps(StyleFontSynthesis::None)
            .as_font_synthesis_small_caps(),
        Some(&CssPropertyValue::Exact(StyleFontSynthesis::None))
    );
    assert_eq!(
        CssProperty::font_synthesis_position(StyleFontSynthesis::None)
            .as_font_synthesis_position(),
        Some(&CssPropertyValue::Exact(StyleFontSynthesis::None))
    );
}

/// The four `font-synthesis-*` longhands share one value type but are four properties: the same
/// value under two of them is two different properties.
#[test]
fn the_four_synthesis_longhands_are_four_properties_of_one_value_type() {
    let weight = CssProperty::font_synthesis_weight(StyleFontSynthesis::None);
    let style = CssProperty::font_synthesis_style(StyleFontSynthesis::None);
    let small_caps = CssProperty::font_synthesis_small_caps(StyleFontSynthesis::None);
    let position = CssProperty::font_synthesis_position(StyleFontSynthesis::None);
    for (a, b) in [
        (&weight, &style),
        (&weight, &small_caps),
        (&weight, &position),
        (&style, &small_caps),
        (&style, &position),
        (&small_caps, &position),
    ] {
        assert_ne!(a, b);
        assert_ne!(a.get_type(), b.get_type());
    }
}

// ---- CSS-wide keywords -------------------------------------------------------------------------

/// `initial`, `inherit`, `unset` and `revert` are values of every new longhand, and read back as
/// the keyword they are.
#[test]
fn the_css_wide_keywords_resolve_on_every_new_longhand() {
    for s in samples() {
        for &(text, keyword) in CSS_WIDE_KEYWORDS.iter() {
            let prop = with_keyword(s.ty, keyword);
            assert_eq!(prop.get_type(), s.ty, "{text} on `{}`", s.name);
            assert_eq!(
                prop.css_wide_keyword(),
                Some(keyword),
                "{text} on `{}`",
                s.name
            );
            assert_eq!(prop.value(), text, "{text} on `{}`", s.name);
            assert_eq!(
                prop.format_css(),
                format!("{}: {text};", s.name),
                "{text} on `{}`",
                s.name
            );
        }
        // An exact value is no keyword, and neither are `none` and `auto`: they name values of
        // the property's own grammar.
        assert_eq!(s.prop.css_wide_keyword(), None, "`{}`", s.name);
        assert_eq!(
            CssProperty::none(s.ty).css_wide_keyword(),
            None,
            "none on `{}`",
            s.name
        );
        assert_eq!(
            CssProperty::auto(s.ty).css_wide_keyword(),
            None,
            "auto on `{}`",
            s.name
        );
        assert!(CssProperty::initial(s.ty).is_initial(), "`{}`", s.name);
        assert!(!CssProperty::inherit(s.ty).is_initial(), "`{}`", s.name);
        assert!(!s.prop.is_initial(), "`{}`", s.name);
    }
}

/// Every property type takes the CSS-wide keywords, and the property made from a type and a
/// keyword is of that type and reads back as that keyword: a slip in the type-to-property table
/// (`css_property_from_type!`) shows here, for the old types and the new ones alike.
#[test]
fn a_css_wide_keyword_resolves_on_every_property_type() {
    for &ty in CssPropertyType::ALL {
        for &(text, keyword) in CSS_WIDE_KEYWORDS.iter() {
            let prop = with_keyword(ty, keyword);
            assert_eq!(prop.get_type(), ty, "{text} on {ty:?}");
            assert_eq!(prop.css_wide_keyword(), Some(keyword), "{text} on {ty:?}");
            assert_eq!(prop.value(), text, "{text} on {ty:?}");
        }
        let initial = css_property_from_type!(ty, Initial);
        assert_eq!(initial.get_type(), ty, "{ty:?}");
        assert_eq!(
            initial.css_wide_keyword(),
            Some(CssWideKeyword::Initial),
            "{ty:?}"
        );
        for prop in [CssProperty::none(ty), CssProperty::auto(ty)] {
            assert_eq!(prop.get_type(), ty, "{ty:?}");
            assert_eq!(prop.css_wide_keyword(), None, "{ty:?}");
        }
    }
}

/// The parser answers the CSS-wide keywords before the property's own grammar is asked, for every
/// property type.
#[cfg(feature = "parser")]
#[test]
fn the_css_wide_keywords_parse_on_every_property_type() {
    for &ty in CssPropertyType::ALL {
        for &(text, keyword) in CSS_WIDE_KEYWORDS.iter() {
            let prop = parsed(ty, text);
            assert_eq!(prop.get_type(), ty, "`{}: {text}`", ty.to_str());
            assert_eq!(
                prop.css_wide_keyword(),
                Some(keyword),
                "`{}: {text}`",
                ty.to_str()
            );
        }
    }
}

/// The same for every shorthand: each of its longhands gets the keyword.
#[cfg(feature = "parser")]
#[test]
fn the_css_wide_keywords_resolve_on_every_shorthand() {
    let map = CssKeyMap::get();
    for (name, &shorthand) in &map.shorthands {
        for &(text, keyword) in CSS_WIDE_KEYWORDS.iter() {
            let props = parse_combined_css_property(shorthand, text)
                .unwrap_or_else(|e| panic!("`{name}: {text}` must parse: {e:?}"));
            assert!(!props.is_empty(), "`{name}: {text}` sets nothing");
            for prop in &props {
                assert_eq!(
                    prop.css_wide_keyword(),
                    Some(keyword),
                    "`{name}: {text}` on {:?}",
                    prop.get_type()
                );
            }
        }
    }
}

// ---- the shorthands ----------------------------------------------------------------------------

#[test]
fn font_synthesis_is_a_shorthand_named_font_synthesis() {
    let map = CssKeyMap::get();
    assert_eq!(
        CombinedCssPropertyType::from_str("font-synthesis", &map),
        Some(CombinedCssPropertyType::FontSynthesis)
    );
    assert_eq!(
        CombinedCssPropertyType::FontSynthesis.to_str(&map),
        "font-synthesis"
    );
    assert_eq!(
        format!("{}", CombinedCssPropertyType::FontSynthesis),
        "font-synthesis"
    );
    for name in ["font", "font-variant", "font-synthesis"] {
        assert!(
            CombinedCssPropertyType::from_str(name, &map).is_some(),
            "`{name}` is a shorthand"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_none_turns_all_four_kinds_off() {
    let off = StyleFontSynthesis::None;
    assert_eq!(
        expand("font-synthesis", "none"),
        Ok(synthesis(off, off, off, off))
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_allows_the_kinds_it_lists_and_forbids_the_rest() {
    let on = StyleFontSynthesis::Auto;
    let off = StyleFontSynthesis::None;
    for (input, [weight, style, small_caps, position]) in [
        ("weight", [on, off, off, off]),
        ("style", [off, on, off, off]),
        ("small-caps", [off, off, on, off]),
        ("position", [off, off, off, on]),
        ("weight style", [on, on, off, off]),
        ("position weight", [on, off, off, on]),
        ("small-caps style weight position", [on, on, on, on]),
        ("  WEIGHT   Style ", [on, on, off, off]),
    ] {
        assert_eq!(
            expand("font-synthesis", input),
            Ok(synthesis(weight, style, small_caps, position)),
            "`font-synthesis: {input}`"
        );
    }
    for bad in [
        "",
        "   ",
        "weight weight",
        "none weight",
        "weight none",
        "bold",
        "normal",
        "weight, style",
        "small caps",
    ] {
        assert!(
            expand("font-synthesis", bad).is_err(),
            "`font-synthesis: {bad}` is no value"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_synthesis_with_a_css_wide_keyword_reaches_its_four_longhands() {
    for &(text, keyword) in CSS_WIDE_KEYWORDS.iter() {
        let props = expand("font-synthesis", text)
            .unwrap_or_else(|()| panic!("`font-synthesis: {text}` must parse"));
        assert_eq!(
            types_of(&props),
            FONT_SYNTHESIS_ORDER,
            "`font-synthesis: {text}`"
        );
        for prop in &props {
            assert_eq!(prop.css_wide_keyword(), Some(keyword), "`{text}`");
        }
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_with_a_css_wide_keyword_reaches_its_four_longhands() {
    for &(text, keyword) in CSS_WIDE_KEYWORDS.iter() {
        let props = expand("font-variant", text)
            .unwrap_or_else(|()| panic!("`font-variant: {text}` must parse"));
        assert_eq!(
            types_of(&props),
            FONT_VARIANT_ORDER,
            "`font-variant: {text}`"
        );
        for prop in &props {
            assert_eq!(prop.css_wide_keyword(), Some(keyword), "`{text}`");
        }
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_normal_resets_all_four_longhands_and_none_turns_the_ligatures_off() {
    assert_eq!(
        expand("font-variant", "normal"),
        Ok(variant(
            StyleFontVariantNumeric::NORMAL,
            StyleFontVariantCaps::Normal,
            StyleFontVariantLigatures::NORMAL,
            StyleFontVariantEastAsian::NORMAL,
        ))
    );
    assert_eq!(
        expand("font-variant", "none"),
        Ok(variant(
            StyleFontVariantNumeric::NORMAL,
            StyleFontVariantCaps::Normal,
            StyleFontVariantLigatures::NONE,
            StyleFontVariantEastAsian::NORMAL,
        ))
    );
}

/// The ligature words go to `font-variant-ligatures` together, the East Asian ones to
/// `font-variant-east-asian`, the rest as before; a longhand left out is reset.
#[cfg(feature = "parser")]
#[test]
fn font_variant_sets_the_ligatures_and_the_east_asian_forms_along_with_the_rest() {
    use StyleFontLigatureChoice::{Off, On, Unset};
    assert_eq!(
        expand(
            "font-variant",
            "small-caps no-common-ligatures tabular-nums jis78 contextual"
        ),
        Ok(variant(
            StyleFontVariantNumeric {
                tabular_nums: true,
                ..StyleFontVariantNumeric::NORMAL
            },
            StyleFontVariantCaps::SmallCaps,
            ligatures(Off, Unset, Unset, On),
            east_asian("jis78"),
        ))
    );
    assert_eq!(
        expand("font-variant", "common-ligatures ruby"),
        Ok(variant(
            StyleFontVariantNumeric::NORMAL,
            StyleFontVariantCaps::Normal,
            ligatures(On, Unset, Unset, Unset),
            east_asian("ruby"),
        ))
    );
    assert_eq!(
        expand("font-variant", "full-width"),
        Ok(variant(
            StyleFontVariantNumeric::NORMAL,
            StyleFontVariantCaps::Normal,
            StyleFontVariantLigatures::NORMAL,
            east_asian("full-width"),
        ))
    );
    assert_eq!(
        expand("font-variant", "Small-Caps   NO-CONTEXTUAL"),
        Ok(variant(
            StyleFontVariantNumeric::NORMAL,
            StyleFontVariantCaps::SmallCaps,
            ligatures(Unset, Unset, Unset, Off),
            StyleFontVariantEastAsian::NORMAL,
        ))
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_variant_rejects_two_values_of_one_group_of_the_new_longhands() {
    for bad in [
        "common-ligatures no-common-ligatures",
        "contextual contextual",
        "jis78 jis83",
        "full-width proportional-width",
        "ruby ruby",
        "normal common-ligatures",
        "normal jis78",
        "none jis78",
        "none common-ligatures",
        "small-caps small-caps",
        "bold",
        "",
    ] {
        assert!(
            expand("font-variant", bad).is_err(),
            "`font-variant: {bad}` is no value"
        );
    }
}

/// `font: inherit` (and `initial`, `unset`, `revert`) sets all eighteen longhands the shorthand
/// reaches, in the order it expands in.
#[cfg(feature = "parser")]
#[test]
fn font_with_a_css_wide_keyword_reaches_every_longhand_it_sets() {
    for &(text, keyword) in CSS_WIDE_KEYWORDS.iter() {
        let props =
            expand("font", text).unwrap_or_else(|()| panic!("`font: {text}` must parse"));
        assert_eq!(props.len(), 18, "`font: {text}`");
        assert_eq!(types_of(&props), font_order(), "`font: {text}`");
        for prop in &props {
            assert_eq!(
                prop.css_wide_keyword(),
                Some(keyword),
                "`font: {text}` on {:?}",
                prop.get_type()
            );
        }
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_inherit_is_inherit_on_all_eighteen_longhands() {
    let props = expand("font", "inherit").expect("`font: inherit` parses");
    let expected: Vec<CssProperty> = [
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
        CssPropertyType::FontKerning,
        CssPropertyType::FontOpticalSizing,
        CssPropertyType::FontVariationSettings,
        CssPropertyType::FontFeatureSettings,
        CssPropertyType::FontSynthesisWeight,
        CssPropertyType::FontSynthesisStyle,
        CssPropertyType::FontSynthesisSmallCaps,
        CssPropertyType::FontSynthesisPosition,
    ]
    .iter()
    .map(|ty| CssProperty::inherit(*ty))
    .collect();
    assert_eq!(props, expected);
}

/// A size and a family are all a `font` needs; everything else is reset to its initial value, as
/// the CSS-wide keyword `initial`.
#[cfg(feature = "parser")]
#[test]
fn font_names_a_size_and_a_family_and_resets_everything_else_to_initial() {
    let props = expand("font", "12px Arial").expect("`font: 12px Arial` parses");
    assert_eq!(types_of(&props), font_order());
    let of = |ty: CssPropertyType| -> CssProperty {
        props
            .iter()
            .find(|prop| prop.get_type() == ty)
            .cloned()
            .unwrap_or_else(|| panic!("no {ty:?} in {props:?}"))
    };
    assert_eq!(
        of(CssPropertyType::FontSize),
        CssProperty::font_size(StyleFontSize::px(12.0))
    );
    assert_eq!(
        of(CssPropertyType::FontFamily),
        CssProperty::font_family(family("Arial"))
    );
    assert_eq!(
        of(CssPropertyType::FontWeight),
        CssProperty::FontWeight(CssPropertyValue::Initial)
    );
    assert_eq!(
        props,
        font_expansion(
            None,
            None,
            None,
            None,
            StyleFontSize::px(12.0),
            None,
            family("Arial")
        )
    );
    for ty in FONT_RESETS.iter() {
        assert_eq!(
            of(*ty).css_wide_keyword(),
            Some(CssWideKeyword::Initial),
            "{ty:?} is reset"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_sets_every_longhand_its_value_names() {
    assert_eq!(
        expand(
            "font",
            r#"italic small-caps bold condensed 12px/1.5 "Helvetica Neue", Arial, sans-serif"#
        ),
        Ok(font_expansion(
            Some(StyleFontStyle::Italic),
            Some(StyleFontVariantCaps::SmallCaps),
            Some(StyleFontWeight::BOLD),
            Some(StyleFontStretch::from_percent(75.0)),
            StyleFontSize::px(12.0),
            Some(line_height("1.5")),
            family(r#""Helvetica Neue", Arial, sans-serif"#),
        ))
    );
    // The words before the size go to the first longhand that takes them, in any order.
    assert_eq!(
        expand("font", "condensed bold small-caps italic 12px a"),
        Ok(font_expansion(
            Some(StyleFontStyle::Italic),
            Some(StyleFontVariantCaps::SmallCaps),
            Some(StyleFontWeight::BOLD),
            Some(StyleFontStretch::from_percent(75.0)),
            StyleFontSize::px(12.0),
            None,
            family("a"),
        ))
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_takes_the_line_height_after_a_slash_with_or_without_spaces() {
    for value in ["12px/1.5 Arial", "12px / 1.5 Arial", "12px /1.5 Arial"] {
        assert_eq!(
            expand("font", value),
            Ok(font_expansion(
                None,
                None,
                None,
                None,
                StyleFontSize::px(12.0),
                Some(line_height("1.5")),
                family("Arial"),
            )),
            "`font: {value}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_takes_the_sizes_a_font_size_takes_but_never_a_bare_number() {
    assert_eq!(
        expand("font", "bold 12pt serif"),
        Ok(font_expansion(
            None,
            None,
            Some(StyleFontWeight::BOLD),
            None,
            StyleFontSize::pt(12.0),
            None,
            family("serif"),
        ))
    );
    assert_eq!(
        expand("font", "80% a"),
        Ok(font_expansion(
            None,
            None,
            None,
            None,
            StyleFontSize::percent(80.0),
            None,
            family("a"),
        ))
    );
    assert_eq!(
        expand("font", "large a"),
        Ok(font_expansion(
            None,
            None,
            None,
            None,
            StyleFontSize::px(18.0),
            None,
            family("a"),
        ))
    );
    // `600` is a weight, never a size: without a size after it this is no `font` value.
    assert!(expand("font", "600 Arial").is_err());
    assert!(expand("font", "12 Arial").is_err());
}

/// Any weight number and an oblique angle are part of `font`.
#[cfg(feature = "parser")]
#[test]
fn font_takes_a_numeric_weight_and_an_oblique_angle() {
    assert_eq!(
        expand("font", "350 14px sans-serif"),
        Ok(font_expansion(
            None,
            None,
            Some(StyleFontWeight::from_number(350.0)),
            None,
            StyleFontSize::px(14.0),
            None,
            family("sans-serif"),
        ))
    );
    assert_eq!(
        expand("font", "oblique 10deg 14px x"),
        Ok(font_expansion(
            Some(StyleFontStyle::Oblique(FloatValue::const_new(10))),
            None,
            None,
            None,
            StyleFontSize::px(14.0),
            None,
            family("x"),
        ))
    );
    assert_eq!(
        expand("font", "oblique 14px x"),
        Ok(font_expansion(
            Some(StyleFontStyle::OBLIQUE),
            None,
            None,
            None,
            StyleFontSize::px(14.0),
            None,
            family("x"),
        ))
    );
}

/// `normal` stands for whichever of the four words is still unset.
#[cfg(feature = "parser")]
#[test]
fn font_takes_up_to_four_normals_before_the_size() {
    assert_eq!(
        expand("font", "normal normal normal normal 10px a"),
        Ok(font_expansion(
            Some(StyleFontStyle::Normal),
            Some(StyleFontVariantCaps::Normal),
            Some(StyleFontWeight::NORMAL),
            Some(StyleFontStretch::NORMAL),
            StyleFontSize::px(10.0),
            None,
            family("a"),
        ))
    );
    assert!(expand("font", "normal normal normal normal normal 10px a").is_err());
}

/// A system font keyword is no grammar of the shorthand: it keeps the family-only value.
#[cfg(feature = "parser")]
#[test]
fn a_system_font_keyword_keeps_the_family_only_value() {
    for value in [
        "caption",
        "icon",
        "menu",
        "message-box",
        "small-caption",
        "status-bar",
        "Menu",
        "system:ui",
        "  caption  ",
    ] {
        assert_eq!(
            expand("font", value),
            Ok(vec![CssProperty::font(family(value))]),
            "`font: {value}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_rejects_what_is_not_a_size_and_a_family() {
    for bad in [
        "Arial",
        "bold",
        "12px",
        "bold bold 12px a",
        "small-caps small-caps 12px a",
        "italic oblique 12px a",
        "12px/ a",
        "12px/1.5",
        "condensed 12px",
        "",
        "   ",
    ] {
        assert!(expand("font", bad).is_err(), "`font: {bad}` is no value");
    }
}
