use super::*;
use super::StyleFontLigatureChoice::{Off, On, Unset};
use crate::hash::GetHash;

// These tests pin the contract of the font-variation module: the grammar of
// every property (keywords, ASCII case, trimming, what is rejected), the
// canonical print and its round trip, the defaults and the helper methods.

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// What the parsers trim around a value: `(left, right)`.
static PADDINGS: [(&str, &str); 4] = [
    ("", ""),
    ("  ", " "),
    ("\t", "\n"),
    (" \r\n\t ", "\u{000C}"),
];

#[cfg(feature = "parser")]
type KeywordParser<T> = for<'a> fn(&'a str) -> Result<T, InvalidValueErr<'a>>;

/// The grammar of an `auto | none`-style property: every keyword parses in
/// every spelling (lower, upper and mixed case, padded), prints as itself and
/// reads back; the `rejected` inputs fail with the trimmed input as the error.
#[cfg(feature = "parser")]
fn assert_keyword_property<T>(
    parse: KeywordParser<T>,
    all: &[(T, &'static str)],
    rejected: &[&str],
) where
    T: Copy + PartialEq + core::fmt::Debug + PrintAsCssValue,
{
    assert!(!all.is_empty());
    for (value, keyword) in all {
        let upper = keyword.to_ascii_uppercase();
        let mixed: String = keyword
            .chars()
            .enumerate()
            .map(|(index, c)| if index % 2 == 0 { c.to_ascii_uppercase() } else { c })
            .collect();
        for spelling in [keyword.to_string(), upper, mixed] {
            for (left, right) in &PADDINGS {
                let input = format!("{left}{spelling}{right}");
                assert_eq!(parse(&input), Ok(*value), "{input:?}");
            }
        }
        assert_eq!(value.print_as_css_value(), *keyword);
        assert_eq!(parse(&value.print_as_css_value()), Ok(*value));
    }
    for bad in rejected {
        assert_eq!(parse(bad), Err(InvalidValueErr(bad.trim())), "{bad:?}");
    }
}

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

/// The nine `font-variant-east-asian` keywords with their OpenType tags, in the
/// grammar's order.
static EAST_ASIAN: [(&str, [u8; 4]); 9] = [
    ("jis78", *b"jp78"),
    ("jis83", *b"jp83"),
    ("jis90", *b"jp90"),
    ("jis04", *b"jp04"),
    ("simplified", *b"smpl"),
    ("traditional", *b"trad"),
    ("full-width", *b"fwid"),
    ("proportional-width", *b"pwid"),
    ("ruby", *b"ruby"),
];

/// The value with the keyword `n` set when bit `n` of `mask` is, `n` counting
/// in the grammar's order.
fn east_asian(mask: u32) -> StyleFontVariantEastAsian {
    let bit = |index: usize| ((mask >> index) & 1) == 1;
    StyleFontVariantEastAsian {
        jis78: bit(0),
        jis83: bit(1),
        jis90: bit(2),
        jis04: bit(3),
        simplified: bit(4),
        traditional: bit(5),
        full_width: bit(6),
        proportional_width: bit(7),
        ruby: bit(8),
    }
}

/// Whether the grammar allows the keywords of `mask`: at most one of the
/// variant group (bits 0 to 5) and at most one of the width group (6 and 7);
/// `ruby` (bit 8) stands alone.
fn east_asian_mask_is_valid(mask: u32) -> bool {
    (mask & 0b11_1111).count_ones() <= 1 && ((mask >> 6) & 0b11).count_ones() <= 1
}

// ---------------------------------------------------------------------------
// layout, defaults, ordering, const-ness
// ---------------------------------------------------------------------------

#[test]
fn the_values_have_the_layout_the_c_api_declares() {
    assert_eq!(core::mem::size_of::<StyleFontVariantEastAsian>(), 9);
    assert_eq!(
        core::mem::size_of::<StyleFontStretch>(),
        core::mem::size_of::<PercentageValue>()
    );
    // A fieldless `repr(C)` enum is a C `enum`: as big as a C `int`. The API
    // generator demands `repr(C)` of every fieldless enum, so the four choices
    // of the ligatures are four of those (a size of 4 bytes would take a
    // one-byte enum, which `repr(C)` forbids).
    let int = core::mem::size_of::<core::ffi::c_int>();
    assert_eq!(core::mem::size_of::<StyleFontLigatureChoice>(), int);
    assert_eq!(core::mem::size_of::<StyleFontVariantLigatures>(), 4 * int);
    assert_eq!(core::mem::size_of::<StyleFontOpticalSizing>(), int);
    assert_eq!(core::mem::size_of::<StyleFontKerning>(), int);
    assert_eq!(core::mem::size_of::<StyleFontSynthesis>(), int);
}

#[test]
fn every_property_starts_at_its_initial_value() {
    assert_eq!(StyleFontStretch::default(), StyleFontStretch::NORMAL);
    assert_eq!(
        StyleFontVariationSettings::default().inner.as_str(),
        "normal"
    );
    assert_eq!(
        StyleFontOpticalSizing::default(),
        StyleFontOpticalSizing::Auto
    );
    assert_eq!(StyleFontKerning::default(), StyleFontKerning::Auto);
    assert_eq!(StyleFontSynthesis::default(), StyleFontSynthesis::Auto);
    assert_eq!(StyleFontLigatureChoice::default(), Unset);
    assert_eq!(
        StyleFontVariantLigatures::default(),
        StyleFontVariantLigatures::NORMAL
    );
    assert_eq!(
        StyleFontVariantEastAsian::default(),
        StyleFontVariantEastAsian::NORMAL
    );
}

#[test]
fn values_order_by_declaration_and_by_width() {
    assert!(StyleFontOpticalSizing::Auto < StyleFontOpticalSizing::None);
    assert!(StyleFontKerning::Auto < StyleFontKerning::Normal);
    assert!(StyleFontKerning::Normal < StyleFontKerning::None);
    assert!(StyleFontSynthesis::Auto < StyleFontSynthesis::None);
    assert!(Unset < On);
    assert!(On < Off);
    assert!(StyleFontStretch::from_percent(50.0) < StyleFontStretch::NORMAL);
    assert!(StyleFontStretch::NORMAL < StyleFontStretch::from_percent(200.0));
}

#[test]
fn equal_values_hash_equal() {
    assert_eq!(
        StyleFontStretch::from_percent(87.5).get_hash(),
        StyleFontStretch::from_percent(87.5).get_hash()
    );
    assert_eq!(
        StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0)]).get_hash(),
        StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0)]).get_hash()
    );
    assert_eq!(
        ligatures(On, Unset, Off, Unset).get_hash(),
        ligatures(On, Unset, Off, Unset).get_hash()
    );
    assert_ne!(
        ligatures(On, Unset, Off, Unset).get_hash(),
        ligatures(Off, Unset, On, Unset).get_hash()
    );
    #[cfg(feature = "parser")]
    {
        assert_eq!(
            parse_style_font_stretch("semi-condensed")
                .unwrap()
                .get_hash(),
            parse_style_font_stretch("87.5%").unwrap().get_hash()
        );
    }
}

#[test]
fn the_const_fns_run_at_compile_time() {
    const LIGATURES_NORMAL: bool = StyleFontVariantLigatures::NORMAL.is_normal();
    const LIGATURES_NONE: bool = StyleFontVariantLigatures::NONE.is_none();
    const EAST_ASIAN_NORMAL: bool = StyleFontVariantEastAsian::NORMAL.is_normal();
    const KEYWORDS: [(&str, [u8; 4], bool); 9] = StyleFontVariantEastAsian::NORMAL.keywords();
    const OPTICAL: &str = StyleFontOpticalSizing::None.keyword();
    const KERNING: &str = StyleFontKerning::Normal.keyword();
    const SYNTHESIS: &str = StyleFontSynthesis::Auto.keyword();
    const WIDTH: StyleFontStretch = StyleFontStretch::NORMAL;
    let flags = [LIGATURES_NORMAL, LIGATURES_NONE, EAST_ASIAN_NORMAL];
    assert_eq!(flags, [true, true, true]);
    assert_eq!(KEYWORDS[8], ("ruby", *b"ruby", false));
    assert_eq!((OPTICAL, KERNING, SYNTHESIS), ("none", "normal", "auto"));
    assert_eq!(WIDTH, StyleFontStretch::default());
}

// ---------------------------------------------------------------------------
// font-stretch
// ---------------------------------------------------------------------------

#[test]
fn the_initial_font_stretch_is_one_hundred_percent() {
    assert_eq!(
        StyleFontStretch::NORMAL,
        StyleFontStretch::from_percent(100.0)
    );
    assert_eq!(StyleFontStretch::default().percent(), 100.0);
    assert_eq!(StyleFontStretch::default().print_as_css_value(), "100%");
}

#[test]
fn a_font_stretch_holds_its_percentage_to_the_thousandth() {
    for percent in [
        0.0_f32, 0.5, 12.25, 33.3, 50.0, 62.5, 75.0, 87.5, 99.9, 100.0, 112.5, 125.0, 150.0, 200.0,
        1000.0,
    ] {
        assert_eq!(
            StyleFontStretch::from_percent(percent).percent(),
            percent,
            "{percent}"
        );
    }
}

#[test]
fn a_font_stretch_always_prints_as_a_percentage() {
    static CASES: [(f32, &str); 11] = [
        (50.0, "50%"),
        (62.5, "62.5%"),
        (75.0, "75%"),
        (87.5, "87.5%"),
        (100.0, "100%"),
        (112.5, "112.5%"),
        (125.0, "125%"),
        (150.0, "150%"),
        (200.0, "200%"),
        (0.0, "0%"),
        (33.3, "33.3%"),
    ];
    for (percent, text) in &CASES {
        assert_eq!(
            StyleFontStretch::from_percent(*percent).print_as_css_value(),
            *text
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn every_font_stretch_keyword_is_its_percentage() {
    static KEYWORDS: [(&str, f32); 9] = [
        ("ultra-condensed", 50.0),
        ("extra-condensed", 62.5),
        ("condensed", 75.0),
        ("semi-condensed", 87.5),
        ("normal", 100.0),
        ("semi-expanded", 112.5),
        ("expanded", 125.0),
        ("extra-expanded", 150.0),
        ("ultra-expanded", 200.0),
    ];
    for (keyword, percent) in &KEYWORDS {
        let expected = StyleFontStretch::from_percent(*percent);
        let upper = keyword.to_ascii_uppercase();
        let mixed: String = keyword
            .chars()
            .enumerate()
            .map(|(index, c)| if index % 2 == 0 { c.to_ascii_uppercase() } else { c })
            .collect();
        for spelling in [keyword.to_string(), upper, mixed] {
            for (left, right) in &PADDINGS {
                let input = format!("{left}{spelling}{right}");
                assert_eq!(parse_style_font_stretch(&input), Ok(expected), "{input:?}");
            }
        }
        assert_eq!(expected.percent(), *percent, "{keyword}");
        // a keyword prints as the percentage it stands for
        assert_eq!(expected.print_as_css_value(), format!("{percent}%"));
    }
}

#[cfg(feature = "parser")]
#[test]
fn the_font_stretch_keywords_order_from_narrow_to_wide() {
    let order = [
        "ultra-condensed",
        "extra-condensed",
        "condensed",
        "semi-condensed",
        "normal",
        "semi-expanded",
        "expanded",
        "extra-expanded",
        "ultra-expanded",
    ];
    let widths: Vec<StyleFontStretch> = order
        .iter()
        .map(|keyword| parse_style_font_stretch(keyword).unwrap())
        .collect();
    for pair in widths.windows(2) {
        assert!(pair[0] < pair[1], "{pair:?}");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_font_stretch_percentage_is_a_plain_decimal_of_at_least_zero() {
    static CASES: [(&str, f32); 12] = [
        ("87.5%", 87.5),
        ("100%", 100.0),
        ("0%", 0.0),
        (" 150% ", 150.0),
        ("+50%", 50.0),
        (".5%", 0.5),
        ("12.25%", 12.25),
        ("1000%", 1000.0),
        ("050%", 50.0),
        ("0.0%", 0.0),
        ("62.5%", 62.5),
        ("\t200%\n", 200.0),
    ];
    for (text, percent) in &CASES {
        assert_eq!(
            parse_style_font_stretch(text),
            Ok(StyleFontStretch::from_percent(*percent)),
            "{text:?}"
        );
    }
    // a keyword and the percentage it stands for are the same value
    assert_eq!(
        parse_style_font_stretch("semi-condensed"),
        parse_style_font_stretch("87.5%")
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_font_stretch_that_is_no_keyword_and_no_percentage_is_rejected() {
    let too_long = format!("{}%", "9".repeat(400));
    let rejected = [
        "",
        " ",
        "\t\n",
        "-5%",
        "-0.5%",
        "-100%",
        "100",
        "0",
        "1.5",
        "%",
        " % ",
        "abc%",
        "50 %",
        "5.%",
        "1e2%",
        "1E2%",
        "inf%",
        "nan%",
        "NaN%",
        "+inf%",
        "50%%",
        "%50",
        "50% 60%",
        "50px",
        "50%;",
        "ultra",
        "condensed expanded",
        "normal%",
        "- 5%",
        "--5%",
        "+-5%",
        "50.5.5%",
        "50,5%",
        "5 0%",
        "0x10%",
        "\u{ff15}\u{ff10}%",
        "condensed;",
        "condensed!",
        "semi condensed",
        "ultracondensed",
        "wider",
        too_long.as_str(),
    ];
    for input in rejected {
        assert_eq!(
            parse_style_font_stretch(input),
            Err(InvalidValueErr(input.trim())),
            "{input:?}"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_font_stretch_error_carries_the_trimmed_input() {
    assert_eq!(
        parse_style_font_stretch("  -5%\n"),
        Err(InvalidValueErr("-5%"))
    );
    assert_eq!(parse_style_font_stretch(""), Err(InvalidValueErr("")));
    assert_eq!(parse_style_font_stretch("   "), Err(InvalidValueErr("")));
    assert_eq!(
        parse_style_font_stretch(" abc "),
        Err(InvalidValueErr("abc"))
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_printed_font_stretch_reads_back() {
    for input in [
        "ultra-condensed",
        "semi-condensed",
        "normal",
        "ultra-expanded",
        "0%",
        "33.3%",
        "87.5%",
        "12.25%",
        "999%",
    ] {
        let parsed = parse_style_font_stretch(input).unwrap();
        let text = parsed.print_as_css_value();
        assert!(text.ends_with('%'), "{text}");
        assert_eq!(parse_style_font_stretch(&text), Ok(parsed), "{input}");
    }
}

// ---------------------------------------------------------------------------
// font-variation-settings
// ---------------------------------------------------------------------------

#[test]
fn the_initial_font_variation_settings_is_normal() {
    let normal = StyleFontVariationSettings::default();
    assert_eq!(normal.inner.as_str(), "normal");
    assert!(normal.is_normal());
    assert!(normal.variations().is_empty());
    assert_eq!(normal.print_as_css_value(), "normal");
    assert_eq!(StyleFontVariationSettings::from_variations(&[]), normal);
    let spelled = StyleFontVariationSettings {
        inner: "normal".into(),
    };
    assert!(spelled.is_normal());
    assert_eq!(spelled, normal);
}

#[test]
fn a_variation_list_is_not_normal() {
    let value = StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0)]);
    assert!(!value.is_normal());
    assert_ne!(value, StyleFontVariationSettings::default());
}

#[test]
fn variations_are_written_as_a_quoted_tag_and_a_number_joined_by_comma_and_space() {
    let one = StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0)]);
    assert_eq!(one.inner.as_str(), r#""wght" 650"#);
    assert_eq!(one.print_as_css_value(), r#""wght" 650"#);

    let two = StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0), (*b"wdth", 87.5)]);
    assert_eq!(two.print_as_css_value(), r#""wght" 650, "wdth" 87.5"#);

    let negative = StyleFontVariationSettings::from_variations(&[(*b"slnt", -15.0)]);
    assert_eq!(negative.print_as_css_value(), r#""slnt" -15"#);

    let fraction = StyleFontVariationSettings::from_variations(&[(*b"opsz", 0.25)]);
    assert_eq!(fraction.print_as_css_value(), r#""opsz" 0.25"#);

    let zero = StyleFontVariationSettings::from_variations(&[(*b"wght", 0.0)]);
    assert_eq!(zero.print_as_css_value(), r#""wght" 0"#);

    let four = StyleFontVariationSettings::from_variations(&[
        (*b"wght", 650.0),
        (*b"wdth", 87.5),
        (*b"slnt", -15.0),
        (*b"opsz", 0.25),
    ]);
    assert_eq!(
        four.print_as_css_value(),
        r#""wght" 650, "wdth" 87.5, "slnt" -15, "opsz" 0.25"#
    );
}

#[test]
fn variations_reads_the_tags_back_in_text_order() {
    let value = StyleFontVariationSettings::from_variations(&[
        (*b"wght", 650.0),
        (*b"wdth", 87.5),
        (*b"slnt", -15.0),
        (*b"opsz", 0.25),
    ]);
    assert_eq!(
        value.variations(),
        [
            (*b"wght", 650.0),
            (*b"wdth", 87.5),
            (*b"slnt", -15.0),
            (*b"opsz", 0.25)
        ]
    );
    assert!(StyleFontVariationSettings::default().variations().is_empty());
}

#[test]
fn a_tag_listed_twice_keeps_its_last_value_at_the_place_it_last_stands() {
    let value = StyleFontVariationSettings::from_variations(&[
        (*b"wght", 400.0),
        (*b"wdth", 100.0),
        (*b"wght", 700.0),
    ]);
    assert_eq!(value.print_as_css_value(), r#""wdth" 100, "wght" 700"#);
    assert_eq!(value.variations(), [(*b"wdth", 100.0), (*b"wght", 700.0)]);

    let adjacent =
        StyleFontVariationSettings::from_variations(&[(*b"wght", 400.0), (*b"wght", 700.0)]);
    assert_eq!(adjacent.print_as_css_value(), r#""wght" 700"#);
    assert_eq!(adjacent.variations(), [(*b"wght", 700.0)]);

    let both = StyleFontVariationSettings::from_variations(&[
        (*b"wght", 400.0),
        (*b"wdth", 100.0),
        (*b"wght", 700.0),
        (*b"wdth", 50.0),
    ]);
    assert_eq!(both.print_as_css_value(), r#""wght" 700, "wdth" 50"#);
}

#[test]
fn a_tag_differing_in_case_is_another_tag() {
    let value =
        StyleFontVariationSettings::from_variations(&[(*b"wght", 400.0), (*b"WGHT", 700.0)]);
    assert_eq!(value.print_as_css_value(), r#""wght" 400, "WGHT" 700"#);
    assert_eq!(value.variations(), [(*b"wght", 400.0), (*b"WGHT", 700.0)]);
}

// The grammar does not say what `from_variations` does with an entry it cannot
// write (a tag that is not four printable ASCII characters, a number that is
// not finite): it leaves the entry out, so the text always reads back as what
// it says.
#[test]
fn from_variations_leaves_out_what_the_text_cannot_hold() {
    let value = StyleFontVariationSettings::from_variations(&[
        (*b"w\0ht", 1.0),
        (*b"wght", f32::NAN),
        (*b"wdth", f32::INFINITY),
        (*b"slnt", f32::NEG_INFINITY),
        (*b"\xC3\xA4bc", 1.0),
        (*b"opsz", 14.0),
    ]);
    assert_eq!(value.print_as_css_value(), r#""opsz" 14"#);
    assert_eq!(value.variations(), [(*b"opsz", 14.0)]);

    let nothing = StyleFontVariationSettings::from_variations(&[(*b"w\0ht", 1.0)]);
    assert_eq!(nothing, StyleFontVariationSettings::default());
}

#[test]
fn the_printable_ascii_range_is_what_a_tag_may_hold() {
    let value = StyleFontVariationSettings::from_variations(&[(*b"    ", 5.0), (*b"~~~~", 6.0)]);
    assert_eq!(value.variations(), [(*b"    ", 5.0), (*b"~~~~", 6.0)]);
    assert_eq!(value.print_as_css_value(), r#""    " 5, "~~~~" 6"#);
}

#[cfg(feature = "parser")]
#[test]
fn font_variation_settings_normal_is_a_keyword_in_any_case() {
    for input in ["normal", "NORMAL", "Normal", "  normal ", "\t normal \n"] {
        let parsed = parse_style_font_variation_settings(input).unwrap();
        assert_eq!(parsed, StyleFontVariationSettings::default(), "{input:?}");
        assert!(parsed.is_normal());
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_variation_list_reads_into_its_canonical_text() {
    static CASES: [(&str, &str); 24] = [
        (r#""wght" 650"#, r#""wght" 650"#),
        (r#"'wght' 650"#, r#""wght" 650"#),
        (r#""wght" 650, "wdth" 87.5"#, r#""wght" 650, "wdth" 87.5"#),
        (r#""wght" 650,"wdth" 87.5"#, r#""wght" 650, "wdth" 87.5"#),
        (
            "  \"wght\"   650  ,\t\"wdth\"\n87.5  ",
            r#""wght" 650, "wdth" 87.5"#,
        ),
        (r#""wght" -0.5"#, r#""wght" -0.5"#),
        (r#""wght" +3"#, r#""wght" 3"#),
        (r#""wght" .5"#, r#""wght" 0.5"#),
        (r#""wght" 0"#, r#""wght" 0"#),
        (r#""wght" 007"#, r#""wght" 7"#),
        (r#""wght" 1000000"#, r#""wght" 1000000"#),
        (r#""opsz" 14.5"#, r#""opsz" 14.5"#),
        (r#""slnt" -15"#, r#""slnt" -15"#),
        (
            r#""wght" 400, "wdth" 100, "wght" 700"#,
            r#""wdth" 100, "wght" 700"#,
        ),
        (r#""wght" 400, "wght" 700"#, r#""wght" 700"#),
        (r#""WGHT" 400"#, r#""WGHT" 400"#),
        (
            r#""wght" 400, "WGHT" 700"#,
            r#""wght" 400, "WGHT" 700"#,
        ),
        (r#""    " 5"#, r#""    " 5"#),
        (r#""~~~~" 5"#, r#""~~~~" 5"#),
        (r#""wght"400"#, r#""wght" 400"#),
        (r#""wght" 1, 'wdth' 2"#, r#""wght" 1, "wdth" 2"#),
        (r#"",,,," 5"#, r#"",,,," 5"#),
        (r#""it's" 5"#, r#""it's" 5"#),
        (r#""a b " 5"#, r#""a b " 5"#),
    ];
    for (input, canonical) in &CASES {
        let parsed = parse_style_font_variation_settings(input).unwrap();
        assert_eq!(parsed.print_as_css_value(), *canonical, "{input:?}");
        assert_eq!(parsed.inner.as_str(), *canonical, "{input:?}");
        // the canonical text is a fixed point of print and parse
        let again = parse_style_font_variation_settings(canonical).unwrap();
        assert_eq!(again, parsed, "{input:?}");
        assert_eq!(again.print_as_css_value(), *canonical, "{input:?}");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_parsed_variation_list_reads_back_as_its_tags_and_numbers() {
    let parsed = parse_style_font_variation_settings(
        r#""wght" 650, 'wdth' 87.5, "slnt" -15, "opsz" 0.25"#,
    )
    .unwrap();
    assert_eq!(
        parsed.variations(),
        [
            (*b"wght", 650.0),
            (*b"wdth", 87.5),
            (*b"slnt", -15.0),
            (*b"opsz", 0.25)
        ]
    );
    // a tag given twice keeps the last value and the last place
    let twice =
        parse_style_font_variation_settings(r#""wght" 400, "wdth" 100, "wght" 700"#).unwrap();
    assert_eq!(twice.variations(), [(*b"wdth", 100.0), (*b"wght", 700.0)]);
    // tags are case-sensitive
    let cased = parse_style_font_variation_settings(r#""wght" 400, "WGHT" 700"#).unwrap();
    assert_eq!(cased.variations(), [(*b"wght", 400.0), (*b"WGHT", 700.0)]);
}

#[cfg(feature = "parser")]
fn axis_value(number: &str) -> Option<f32> {
    let parsed = parse_style_font_variation_settings(&format!("\"wght\" {number}")).ok()?;
    Some(parsed.variations()[0].1)
}

#[cfg(feature = "parser")]
#[test]
fn an_axis_number_is_a_plain_decimal() {
    for (text, value) in [
        ("650", 650.0),
        ("-0.5", -0.5),
        ("+3", 3.0),
        (".5", 0.5),
        ("0", 0.0),
        ("-0", 0.0),
        ("007", 7.0),
        ("1.25", 1.25),
        ("-.5", -0.5),
        ("+.25", 0.25),
        ("00.50", 0.5),
        ("12345", 12345.0),
        ("0.0001", 0.0001),
    ] {
        assert_eq!(axis_value(text), Some(value), "{text}");
    }
    let too_long = "9".repeat(400);
    for text in [
        "5.",
        "1e3",
        "1E3",
        "1e-3",
        "1e+3",
        "inf",
        "-inf",
        "+inf",
        "infinity",
        "NaN",
        "nan",
        "",
        "-",
        "+",
        ".",
        "+.",
        "--5",
        "+-5",
        "5..5",
        "1.2.3",
        "0x10",
        "1_000",
        "4 00",
        "400px",
        "4e",
        "e4",
        "400;",
        "1,5",
        "\u{664}\u{660}\u{660}",
        "\u{ff14}\u{ff10}\u{ff10}",
        too_long.as_str(),
    ] {
        assert_eq!(axis_value(text), None, "{text}");
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_tag_with_a_double_quote_in_single_quotes_survives_the_text() {
    let parsed = parse_style_font_variation_settings(r#"'a"bc' 5"#).unwrap();
    assert_eq!(parsed.variations(), [(*b"a\"bc", 5.0)]);
    let again = parse_style_font_variation_settings(&parsed.print_as_css_value()).unwrap();
    assert_eq!(again, parsed);
    assert_eq!(again.variations(), parsed.variations());
}

#[cfg(feature = "parser")]
#[test]
fn a_variation_list_that_breaks_the_grammar_is_rejected() {
    let too_long = format!("\"wght\" {}", "9".repeat(400));
    let rejected = [
        "",
        "   ",
        "\t\n",
        // an unquoted or unbalanced tag
        "wght 400",
        "wght",
        "'wght",
        "\"wght",
        "wght\" 400",
        "\"wght' 400",
        "'wght\" 400",
        "\u{201c}wght\u{201d} 400",
        // a tag of the wrong size or of the wrong characters
        r#""wgh" 400"#,
        r#""wghts" 400"#,
        "\"\" 400",
        r#""w" 400"#,
        "\"wgh\u{e4}\" 400",
        "\"wg\u{e4}\" 400",
        "\"\u{20ac}\" 400",
        "\"w\tht\" 400",
        "\"w\u{7f}ht\" 400",
        "\"w\u{0}ht\" 400",
        // a missing or malformed value
        r#""wght""#,
        r#""wght" on"#,
        r#""wght" off"#,
        r#""wght" 5."#,
        r#""wght" 1e3"#,
        r#""wght" 1E3"#,
        r#""wght" inf"#,
        r#""wght" -inf"#,
        r#""wght" NaN"#,
        r#""wght" nan"#,
        r#""wght" 400px"#,
        r#""wght" 4 00"#,
        r#""wght" 400 500"#,
        r#""wght" 400;"#,
        // an empty item
        r#""wght" 400,"#,
        r#""wght" 400 ,"#,
        r#", "wght" 400"#,
        r#""wght" 400,, "wdth" 50"#,
        r#""wght" 400, "wdth""#,
        // a missing comma
        r#""wght" 400 "wdth" 50"#,
        // normal and the other keywords stand alone
        r#"normal, "wght" 400"#,
        r#""wght" 400, normal"#,
        r#"normal "wght" 400"#,
        "normal normal",
        "none",
        "auto",
        too_long.as_str(),
    ];
    for input in rejected {
        assert_eq!(
            parse_style_font_variation_settings(input),
            Err(InvalidValueErr(input.trim())),
            "{input:?}"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_font_variation_settings_error_carries_the_trimmed_input() {
    assert_eq!(
        parse_style_font_variation_settings("  \"wght\" 5.\n"),
        Err(InvalidValueErr("\"wght\" 5."))
    );
    assert_eq!(
        parse_style_font_variation_settings(""),
        Err(InvalidValueErr(""))
    );
    assert_eq!(
        parse_style_font_variation_settings(" \t "),
        Err(InvalidValueErr(""))
    );
    assert_eq!(
        parse_style_font_variation_settings("  wght 400  "),
        Err(InvalidValueErr("wght 400"))
    );
}

// ---------------------------------------------------------------------------
// font-optical-sizing, font-kerning, font-synthesis-*
// ---------------------------------------------------------------------------

#[test]
fn font_optical_sizing_has_the_two_keywords_in_grammar_order() {
    assert_eq!(
        StyleFontOpticalSizing::ALL,
        [
            (StyleFontOpticalSizing::Auto, "auto"),
            (StyleFontOpticalSizing::None, "none")
        ]
    );
    for (value, keyword) in StyleFontOpticalSizing::ALL {
        assert_eq!(value.keyword(), keyword);
        assert_eq!(value.print_as_css_value(), keyword);
    }
}

#[test]
fn font_kerning_has_the_three_keywords_in_grammar_order() {
    assert_eq!(
        StyleFontKerning::ALL,
        [
            (StyleFontKerning::Auto, "auto"),
            (StyleFontKerning::Normal, "normal"),
            (StyleFontKerning::None, "none")
        ]
    );
    for (value, keyword) in StyleFontKerning::ALL {
        assert_eq!(value.keyword(), keyword);
        assert_eq!(value.print_as_css_value(), keyword);
    }
}

#[test]
fn font_synthesis_has_the_two_keywords_in_grammar_order() {
    assert_eq!(
        StyleFontSynthesis::ALL,
        [
            (StyleFontSynthesis::Auto, "auto"),
            (StyleFontSynthesis::None, "none")
        ]
    );
    for (value, keyword) in StyleFontSynthesis::ALL {
        assert_eq!(value.keyword(), keyword);
        assert_eq!(value.print_as_css_value(), keyword);
    }
}

#[cfg(feature = "parser")]
#[test]
fn font_optical_sizing_is_auto_or_none() {
    assert_keyword_property(
        parse_style_font_optical_sizing,
        &StyleFontOpticalSizing::ALL,
        &[
            "",
            " ",
            "\t\n",
            "normal",
            "auto none",
            "none auto",
            "auto,none",
            "auto;",
            "none!",
            "0",
            "1",
            "on",
            "off",
            "autoo",
            "au to",
            "-auto",
            "weight",
            "  bogus  ",
        ],
    );
}

#[cfg(feature = "parser")]
#[test]
fn font_kerning_is_auto_normal_or_none() {
    assert_keyword_property(
        parse_style_font_kerning,
        &StyleFontKerning::ALL,
        &[
            "",
            " ",
            "\t\n",
            "on",
            "off",
            "auto normal",
            "normal none",
            "none none",
            "normal;",
            "none!",
            "0",
            "kern",
            "-normal",
            "  bogus  ",
        ],
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_font_synthesis_longhand_is_auto_or_none() {
    assert_keyword_property(
        parse_style_font_synthesis,
        &StyleFontSynthesis::ALL,
        &[
            "",
            " ",
            "\t\n",
            "normal",
            "weight",
            "style",
            "small-caps",
            "position",
            "weight style",
            "auto none",
            "none auto",
            "auto weight",
            "auto;",
            "none!",
            "on",
            "off",
            "  bogus  ",
        ],
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_keyword_error_carries_the_trimmed_input() {
    assert_eq!(
        parse_style_font_kerning("  bogus \t"),
        Err(InvalidValueErr("bogus"))
    );
    assert_eq!(parse_style_font_kerning(""), Err(InvalidValueErr("")));
    assert_eq!(
        parse_style_font_optical_sizing(" normal\n"),
        Err(InvalidValueErr("normal"))
    );
    assert_eq!(
        parse_style_font_synthesis("\tauto none  "),
        Err(InvalidValueErr("auto none"))
    );
}

// ---------------------------------------------------------------------------
// font-variant-ligatures
// ---------------------------------------------------------------------------

#[test]
fn normal_and_none_are_the_two_extreme_ligature_values() {
    assert_eq!(
        StyleFontVariantLigatures::NORMAL,
        ligatures(Unset, Unset, Unset, Unset)
    );
    assert_eq!(
        StyleFontVariantLigatures::NONE,
        ligatures(Off, Off, Off, Off)
    );
    assert!(StyleFontVariantLigatures::NORMAL.is_normal());
    assert!(!StyleFontVariantLigatures::NORMAL.is_none());
    assert!(StyleFontVariantLigatures::NONE.is_none());
    assert!(!StyleFontVariantLigatures::NONE.is_normal());
    assert_eq!(StyleFontVariantLigatures::NORMAL.print_as_css_value(), "normal");
    assert_eq!(StyleFontVariantLigatures::NONE.print_as_css_value(), "none");

    // a mix is neither of them, however many groups are set
    for mixed in [
        ligatures(On, Unset, Unset, Unset),
        ligatures(Off, Unset, Unset, Unset),
        ligatures(Off, Off, Off, Unset),
        ligatures(Off, Off, Off, On),
        ligatures(Unset, Unset, Unset, Off),
        ligatures(On, On, On, On),
    ] {
        assert!(!mixed.is_normal(), "{mixed:?}");
        assert!(!mixed.is_none(), "{mixed:?}");
    }
}

#[test]
fn the_opentype_tags_of_the_ligatures_come_in_grammar_order() {
    let normal = StyleFontVariantLigatures::NORMAL;
    assert!(normal.opentype_on().is_empty());
    assert!(normal.opentype_off().is_empty());

    let none = StyleFontVariantLigatures::NONE;
    assert!(none.opentype_on().is_empty());
    assert_eq!(
        none.opentype_off(),
        [*b"liga", *b"clig", *b"dlig", *b"hlig", *b"calt"]
    );

    let on = ligatures(On, On, On, On);
    assert_eq!(
        on.opentype_on(),
        [*b"liga", *b"clig", *b"dlig", *b"hlig", *b"calt"]
    );
    assert!(on.opentype_off().is_empty());

    let mixed = ligatures(Off, On, Unset, On);
    assert_eq!(mixed.opentype_on(), [*b"dlig", *b"calt"]);
    assert_eq!(mixed.opentype_off(), [*b"liga", *b"clig"]);

    let common = ligatures(On, Unset, Unset, Unset);
    assert_eq!(common.opentype_on(), [*b"liga", *b"clig"]);
    assert!(common.opentype_off().is_empty());

    let historical = ligatures(Unset, Unset, Off, Unset);
    assert!(historical.opentype_on().is_empty());
    assert_eq!(historical.opentype_off(), [*b"hlig"]);
}

#[test]
fn every_combination_of_ligature_choices_prints_in_grammar_order() {
    let words = [
        ("common-ligatures", "no-common-ligatures"),
        ("discretionary-ligatures", "no-discretionary-ligatures"),
        ("historical-ligatures", "no-historical-ligatures"),
        ("contextual", "no-contextual"),
    ];
    let tags: [&[[u8; 4]]; 4] = [&[*b"liga", *b"clig"], &[*b"dlig"], &[*b"hlig"], &[*b"calt"]];
    let choices = [Unset, On, Off];
    let mut seen: Vec<StyleFontVariantLigatures> = Vec::new();
    for a in choices {
        for b in choices {
            for c in choices {
                for d in choices {
                    let all = [a, b, c, d];
                    let value = ligatures(a, b, c, d);
                    seen.push(value);

                    let mut expected_words: Vec<&str> = Vec::new();
                    let mut expected_on: Vec<[u8; 4]> = Vec::new();
                    let mut expected_off: Vec<[u8; 4]> = Vec::new();
                    for (group, choice) in all.iter().enumerate() {
                        if *choice == On {
                            expected_words.push(words[group].0);
                            expected_on.extend_from_slice(tags[group]);
                        } else if *choice == Off {
                            expected_words.push(words[group].1);
                            expected_off.extend_from_slice(tags[group]);
                        }
                    }
                    let expected = if all == [Unset; 4] {
                        "normal".to_string()
                    } else if all == [Off; 4] {
                        "none".to_string()
                    } else {
                        expected_words.join(" ")
                    };

                    assert_eq!(value.print_as_css_value(), expected, "{value:?}");
                    assert_eq!(value.opentype_on(), expected_on, "{value:?}");
                    assert_eq!(value.opentype_off(), expected_off, "{value:?}");
                    assert_eq!(value.is_normal(), all == [Unset; 4], "{value:?}");
                    assert_eq!(value.is_none(), all == [Off; 4], "{value:?}");
                    #[cfg(feature = "parser")]
                    assert_eq!(
                        parse_style_font_variant_ligatures(&expected),
                        Ok(value),
                        "{expected}"
                    );
                }
            }
        }
    }
    // three choices in four groups: 81 values, every one of them its own
    assert_eq!(seen.len(), 81);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 81);
}

#[cfg(feature = "parser")]
#[test]
fn every_ligature_keyword_sets_its_group() {
    let cases = [
        ("common-ligatures", ligatures(On, Unset, Unset, Unset)),
        ("no-common-ligatures", ligatures(Off, Unset, Unset, Unset)),
        ("discretionary-ligatures", ligatures(Unset, On, Unset, Unset)),
        (
            "no-discretionary-ligatures",
            ligatures(Unset, Off, Unset, Unset),
        ),
        ("historical-ligatures", ligatures(Unset, Unset, On, Unset)),
        (
            "no-historical-ligatures",
            ligatures(Unset, Unset, Off, Unset),
        ),
        ("contextual", ligatures(Unset, Unset, Unset, On)),
        ("no-contextual", ligatures(Unset, Unset, Unset, Off)),
    ];
    for (keyword, expected) in cases {
        let upper = keyword.to_ascii_uppercase();
        let mixed: String = keyword
            .chars()
            .enumerate()
            .map(|(index, c)| if index % 2 == 0 { c.to_ascii_uppercase() } else { c })
            .collect();
        for spelling in [keyword.to_string(), upper, mixed] {
            for (left, right) in &PADDINGS {
                let input = format!("{left}{spelling}{right}");
                assert_eq!(
                    parse_style_font_variant_ligatures(&input),
                    Ok(expected),
                    "{input:?}"
                );
            }
        }
        assert_eq!(expected.print_as_css_value(), keyword);
    }
}

#[cfg(feature = "parser")]
#[test]
fn normal_and_none_read_as_the_two_extremes() {
    for input in ["normal", "NORMAL", "Normal", "  normal\n"] {
        assert_eq!(
            parse_style_font_variant_ligatures(input),
            Ok(StyleFontVariantLigatures::NORMAL),
            "{input:?}"
        );
    }
    for input in ["none", "NONE", "None", "\tnone "] {
        assert_eq!(
            parse_style_font_variant_ligatures(input),
            Ok(StyleFontVariantLigatures::NONE),
            "{input:?}"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn ligature_keywords_combine_in_any_order_and_print_in_grammar_order() {
    let cases = [
        (
            "common-ligatures discretionary-ligatures",
            ligatures(On, On, Unset, Unset),
            "common-ligatures discretionary-ligatures",
        ),
        (
            "no-common-ligatures discretionary-ligatures",
            ligatures(Off, On, Unset, Unset),
            "no-common-ligatures discretionary-ligatures",
        ),
        (
            "contextual common-ligatures",
            ligatures(On, Unset, Unset, On),
            "common-ligatures contextual",
        ),
        (
            "no-contextual historical-ligatures no-discretionary-ligatures common-ligatures",
            ligatures(On, Off, On, Off),
            "common-ligatures no-discretionary-ligatures historical-ligatures no-contextual",
        ),
        (
            "contextual   historical-ligatures\tdiscretionary-ligatures\ncommon-ligatures",
            ligatures(On, On, On, On),
            "common-ligatures discretionary-ligatures historical-ligatures contextual",
        ),
        (
            "No-Contextual HISTORICAL-ligatures",
            ligatures(Unset, Unset, On, Off),
            "historical-ligatures no-contextual",
        ),
    ];
    for (input, expected, canonical) in cases {
        let parsed = parse_style_font_variant_ligatures(input).unwrap();
        assert_eq!(parsed, expected, "{input:?}");
        assert_eq!(parsed.print_as_css_value(), canonical, "{input:?}");
        assert_eq!(
            parse_style_font_variant_ligatures(canonical),
            Ok(expected),
            "{canonical}"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn all_four_groups_off_by_keyword_is_none() {
    let parsed = parse_style_font_variant_ligatures(
        "no-common-ligatures no-discretionary-ligatures no-historical-ligatures no-contextual",
    )
    .unwrap();
    assert_eq!(parsed, StyleFontVariantLigatures::NONE);
    assert!(parsed.is_none());
    assert_eq!(parsed.print_as_css_value(), "none");
    assert_eq!(
        parse_style_font_variant_ligatures(&parsed.print_as_css_value()),
        Ok(parsed)
    );
}

#[cfg(feature = "parser")]
#[test]
fn a_ligature_value_that_breaks_the_grammar_is_rejected() {
    let rejected = [
        "",
        "  ",
        "\t\n",
        // normal and none only stand alone
        "normal normal",
        "normal none",
        "none normal",
        "none none",
        "none common-ligatures",
        "common-ligatures none",
        "normal contextual",
        "contextual normal",
        "contextual none",
        "none contextual",
        // two keywords of one group
        "common-ligatures no-common-ligatures",
        "no-common-ligatures common-ligatures",
        "discretionary-ligatures no-discretionary-ligatures",
        "no-discretionary-ligatures discretionary-ligatures",
        "historical-ligatures no-historical-ligatures",
        "no-historical-ligatures historical-ligatures",
        "contextual no-contextual",
        "no-contextual contextual",
        "common-ligatures contextual no-common-ligatures",
        // a repeated keyword
        "common-ligatures common-ligatures",
        "no-common-ligatures no-common-ligatures",
        "discretionary-ligatures discretionary-ligatures",
        "contextual contextual",
        "no-contextual no-contextual",
        "historical-ligatures contextual historical-ligatures",
        "common-ligatures contextual common-ligatures",
        // not keywords of this property
        "ligatures",
        "common",
        "no-common",
        "liga",
        "clig",
        "dlig",
        "hlig",
        "calt",
        "contextual-ligatures",
        "no-contextual-ligatures",
        "discretionary",
        "historical",
        "no-historical",
        "no-ligatures",
        "common_ligatures",
        "all",
        "auto",
        "common-ligatures,contextual",
        "common-ligatures;",
        "common-ligatures!",
        "common-ligatures\u{a0}contextual",
    ];
    for input in rejected {
        assert_eq!(
            parse_style_font_variant_ligatures(input),
            Err(InvalidValueErr(input.trim())),
            "{input:?}"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn a_ligature_error_carries_the_trimmed_input() {
    assert_eq!(
        parse_style_font_variant_ligatures("  bogus \n"),
        Err(InvalidValueErr("bogus"))
    );
    assert_eq!(
        parse_style_font_variant_ligatures("  common-ligatures   no-common-ligatures \n"),
        Err(InvalidValueErr("common-ligatures   no-common-ligatures"))
    );
    assert_eq!(
        parse_style_font_variant_ligatures(""),
        Err(InvalidValueErr(""))
    );
    assert_eq!(
        parse_style_font_variant_ligatures("   "),
        Err(InvalidValueErr(""))
    );
}

// ---------------------------------------------------------------------------
// font-variant-east-asian
// ---------------------------------------------------------------------------

#[test]
fn normal_east_asian_asks_for_nothing() {
    let normal = StyleFontVariantEastAsian::NORMAL;
    assert!(normal.is_normal());
    assert!(normal.opentype_features().is_empty());
    assert_eq!(normal.print_as_css_value(), "normal");
    assert_eq!(normal, east_asian(0));
    for (index, (keyword, tag, on)) in normal.keywords().iter().enumerate() {
        assert_eq!(*keyword, EAST_ASIAN[index].0);
        assert_eq!(*tag, EAST_ASIAN[index].1);
        assert!(!*on, "{keyword}");
    }
}

#[test]
fn every_east_asian_keyword_has_its_tag_and_prints_alone() {
    for (index, (keyword, tag)) in EAST_ASIAN.iter().enumerate() {
        let value = east_asian(1 << index);
        assert!(!value.is_normal(), "{keyword}");
        assert_eq!(value.opentype_features(), [*tag], "{keyword}");
        assert_eq!(value.print_as_css_value(), *keyword);
        let keywords = value.keywords();
        for (other, (name, name_tag, on)) in keywords.iter().enumerate() {
            assert_eq!(*name, EAST_ASIAN[other].0);
            assert_eq!(*name_tag, EAST_ASIAN[other].1);
            assert_eq!(*on, other == index, "{keyword} / {name}");
        }
    }
}

#[test]
fn every_set_of_east_asian_flags_prints_and_lists_in_grammar_order() {
    for mask in 0..512_u32 {
        let value = east_asian(mask);
        let mut words: Vec<&str> = Vec::new();
        let mut tags: Vec<[u8; 4]> = Vec::new();
        for (index, (keyword, tag)) in EAST_ASIAN.iter().enumerate() {
            if ((mask >> index) & 1) == 1 {
                words.push(*keyword);
                tags.push(*tag);
            }
        }
        let text = if words.is_empty() {
            "normal".to_string()
        } else {
            words.join(" ")
        };
        assert_eq!(value.print_as_css_value(), text, "mask {mask:#011b}");
        assert_eq!(value.opentype_features(), tags, "mask {mask:#011b}");
        assert_eq!(value.is_normal(), mask == 0, "mask {mask:#011b}");
        for (index, (_, _, on)) in value.keywords().iter().enumerate() {
            assert_eq!(*on, ((mask >> index) & 1) == 1, "mask {mask:#011b}");
        }
    }
}

#[cfg(feature = "parser")]
#[test]
fn the_east_asian_grammar_allows_one_variant_one_width_and_ruby() {
    for mask in 0..512_u32 {
        let value = east_asian(mask);
        let words: Vec<&str> = EAST_ASIAN
            .iter()
            .enumerate()
            .filter(|(index, _)| ((mask >> *index) & 1) == 1)
            .map(|(_, (keyword, _))| *keyword)
            .collect();
        let text = if words.is_empty() {
            "normal".to_string()
        } else {
            words.join(" ")
        };
        let reversed = words.iter().rev().copied().collect::<Vec<_>>().join(" ");
        let upper = text.to_ascii_uppercase();
        let padded = format!("  {}\t\n", words.join("   "));

        if east_asian_mask_is_valid(mask) {
            assert_eq!(
                parse_style_font_variant_east_asian(&text),
                Ok(value),
                "{text}"
            );
            assert_eq!(
                parse_style_font_variant_east_asian(&upper),
                Ok(value),
                "{upper}"
            );
            if !words.is_empty() {
                // any order, any whitespace, printed back in grammar order
                let from_reversed = parse_style_font_variant_east_asian(&reversed).unwrap();
                assert_eq!(from_reversed, value, "{reversed}");
                assert_eq!(from_reversed.print_as_css_value(), text);
                assert_eq!(
                    parse_style_font_variant_east_asian(&padded),
                    Ok(value),
                    "{padded:?}"
                );
            }
        } else {
            assert_eq!(
                parse_style_font_variant_east_asian(&text),
                Err(InvalidValueErr(text.as_str())),
                "{text}"
            );
            assert!(
                parse_style_font_variant_east_asian(&reversed).is_err(),
                "{reversed}"
            );
        }
    }
}

#[cfg(feature = "parser")]
#[test]
fn two_east_asian_variants_or_two_widths_are_rejected() {
    let variants = [
        "jis78",
        "jis83",
        "jis90",
        "jis04",
        "simplified",
        "traditional",
    ];
    for first in variants {
        for second in variants {
            let input = format!("{first} {second}");
            assert!(
                parse_style_font_variant_east_asian(&input).is_err(),
                "{input}"
            );
            // another group's keyword does not make it valid
            let with_ruby = format!("{first} ruby {second}");
            assert!(
                parse_style_font_variant_east_asian(&with_ruby).is_err(),
                "{with_ruby}"
            );
        }
    }
    for input in [
        "full-width proportional-width",
        "proportional-width full-width",
        "full-width full-width",
        "proportional-width proportional-width",
        "jis78 full-width proportional-width",
        "ruby ruby",
        "ruby jis78 ruby",
    ] {
        assert!(
            parse_style_font_variant_east_asian(input).is_err(),
            "{input}"
        );
    }
    // one of each group together is fine
    assert!(parse_style_font_variant_east_asian("jis04 proportional-width ruby").is_ok());
    assert!(parse_style_font_variant_east_asian("traditional full-width").is_ok());
}

#[cfg(feature = "parser")]
#[test]
fn normal_east_asian_and_junk_stand_alone() {
    for input in ["normal", "NORMAL", "  Normal\n"] {
        assert_eq!(
            parse_style_font_variant_east_asian(input),
            Ok(StyleFontVariantEastAsian::NORMAL),
            "{input:?}"
        );
    }
    let rejected = [
        "",
        "  ",
        "\t\n",
        "normal normal",
        "normal ruby",
        "ruby normal",
        "normal jis78",
        "jis78 normal",
        "none",
        "auto",
        "jis",
        "jis-78",
        "jis99",
        "full",
        "width",
        "fullwidth",
        "full_width",
        "proportional",
        "ruby-text",
        "jp78",
        "fwid",
        "jis78,ruby",
        "jis78;",
        "jis78!",
        "jis78\u{a0}ruby",
    ];
    for input in rejected {
        assert_eq!(
            parse_style_font_variant_east_asian(input),
            Err(InvalidValueErr(input.trim())),
            "{input:?}"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn an_east_asian_error_carries_the_trimmed_input() {
    assert_eq!(
        parse_style_font_variant_east_asian("  bogus \n"),
        Err(InvalidValueErr("bogus"))
    );
    assert_eq!(
        parse_style_font_variant_east_asian("\tjis78   jis83  "),
        Err(InvalidValueErr("jis78   jis83"))
    );
    assert_eq!(
        parse_style_font_variant_east_asian(""),
        Err(InvalidValueErr(""))
    );
}

// ---------------------------------------------------------------------------
// the Rust code of the values (the code generator)
// ---------------------------------------------------------------------------

#[cfg(feature = "codegen")]
#[test]
fn the_rust_code_of_the_keyword_enums_names_the_variant() {
    assert_eq!(
        StyleFontOpticalSizing::None.format_as_rust_code(0),
        "StyleFontOpticalSizing::None"
    );
    assert_eq!(
        StyleFontKerning::Normal.format_as_rust_code(0),
        "StyleFontKerning::Normal"
    );
    assert_eq!(
        StyleFontSynthesis::Auto.format_as_rust_code(0),
        "StyleFontSynthesis::Auto"
    );
    assert_eq!(
        StyleFontLigatureChoice::Off.format_as_rust_code(0),
        "StyleFontLigatureChoice::Off"
    );
}

#[cfg(feature = "codegen")]
#[test]
fn the_rust_code_of_the_structs_names_every_field() {
    // the ligature choices are enums: each one has to carry its type, or the
    // generated code does not compile
    let none = StyleFontVariantLigatures::NONE.format_as_rust_code(0);
    assert!(none.starts_with("StyleFontVariantLigatures {"), "{none}");
    assert_eq!(none.matches("StyleFontLigatureChoice::Off").count(), 4, "{none}");
    let mixed = ligatures(On, Unset, Off, Unset).format_as_rust_code(0);
    assert!(mixed.contains("common: StyleFontLigatureChoice::On"), "{mixed}");
    assert!(
        mixed.contains("historical: StyleFontLigatureChoice::Off"),
        "{mixed}"
    );

    let ruby = StyleFontVariantEastAsian {
        ruby: true,
        ..StyleFontVariantEastAsian::NORMAL
    }
    .format_as_rust_code(0);
    assert!(ruby.starts_with("StyleFontVariantEastAsian {"), "{ruby}");
    assert!(ruby.contains("ruby: true"), "{ruby}");
    assert!(ruby.contains("jis78: false"), "{ruby}");

    let stretch = StyleFontStretch::from_percent(87.5).format_as_rust_code(0);
    assert!(
        stretch.starts_with("StyleFontStretch { inner: PercentageValue::"),
        "{stretch}"
    );
    assert!(stretch.contains("87"), "{stretch}");

    let settings =
        StyleFontVariationSettings::from_variations(&[(*b"wght", 650.0)]).format_as_rust_code(0);
    assert!(
        settings.starts_with("StyleFontVariationSettings { inner: AzString::from_const_str("),
        "{settings}"
    );
    assert!(settings.contains("wght"), "{settings}");
}
