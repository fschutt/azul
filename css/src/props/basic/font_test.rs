//! The numeric `font-weight` and the angled `font-style` (CSS Fonts 4 s2.2 and s2.3).
//!
//! `font-weight` is any number from 1 to 1000 (`350`, `650.5`) or `bolder` / `lighter`, which are
//! worked out against the parent's weight by the table of s2.2; `font-style` is `oblique` with an
//! optional angle from -90 to 90 degrees. The weight is a number held in thousandths, `normal` and
//! `bold` are the numbers 400 and 700 under a name, and a bare `oblique` is 14 degrees.

use super::*;

/// `oblique <degrees>`, the way the parser holds the angle.
fn oblique(degrees: f32) -> StyleFontStyle {
    StyleFontStyle::Oblique(FloatValue::new(degrees))
}

/// What a `font-weight` that does not parse says: the trimmed input.
#[cfg(feature = "parser")]
fn weight_error(input: &str) -> CssFontWeightParseError<'_> {
    CssFontWeightParseError::InvalidValue(InvalidValueErr(input.trim()))
}

/// What a `font-style` that does not parse says: the trimmed input.
#[cfg(feature = "parser")]
fn style_error(input: &str) -> CssFontStyleParseError<'_> {
    CssFontStyleParseError::InvalidValue(InvalidValueErr(input.trim()))
}

// ---- StyleFontWeight: the number ---------------------------------------------------------------

/// `normal` is 400, `bold` is 700, and the hundreds are constants too.
#[test]
fn the_named_weights_are_their_numbers() {
    for (weight, number) in [
        (StyleFontWeight::W100, 100.0),
        (StyleFontWeight::W200, 200.0),
        (StyleFontWeight::W300, 300.0),
        (StyleFontWeight::NORMAL, 400.0),
        (StyleFontWeight::W500, 500.0),
        (StyleFontWeight::W600, 600.0),
        (StyleFontWeight::BOLD, 700.0),
        (StyleFontWeight::W800, 800.0),
        (StyleFontWeight::W900, 900.0),
    ] {
        assert_eq!(weight.number(), number, "{weight:?}");
        assert_eq!(
            weight,
            StyleFontWeight::from_number(number),
            "{weight:?} is the number {number}"
        );
    }
}

#[test]
fn the_default_weight_is_normal() {
    assert_eq!(StyleFontWeight::default(), StyleFontWeight::NORMAL);
    assert_eq!(StyleFontWeight::default().number(), 400.0);
}

/// A weight is a `FloatValue`: thousandths, so it can be compared and hashed.
#[test]
fn a_weight_is_a_number_held_in_thousandths() {
    assert_eq!(
        StyleFontWeight::from_number(350.0),
        StyleFontWeight::Number(FloatValue::const_new(350))
    );
    assert_eq!(
        StyleFontWeight::from_number(350.5),
        StyleFontWeight::Number(FloatValue::new(350.5))
    );
    assert_ne!(
        StyleFontWeight::from_number(350.0),
        StyleFontWeight::from_number(350.5)
    );
}

#[test]
fn from_number_keeps_every_weight_from_1_to_1000() {
    for number in [
        1.0, 2.0, 99.0, 100.0, 350.0, 350.5, 399.0, 400.0, 401.0, 650.25, 650.5, 999.0, 1000.0,
    ] {
        assert_eq!(
            StyleFontWeight::from_number(number).number(),
            number,
            "{number}"
        );
    }
}

#[test]
fn from_number_clamps_to_1_and_1000() {
    for (given, kept) in [
        (0.0, 1.0),
        (-5.0, 1.0),
        (0.5, 1.0),
        (0.999, 1.0),
        (f32::MIN, 1.0),
        (f32::NEG_INFINITY, 1.0),
        (1000.5, 1000.0),
        (5000.0, 1000.0),
        (f32::MAX, 1000.0),
        (f32::INFINITY, 1000.0),
    ] {
        assert_eq!(
            StyleFontWeight::from_number(given),
            StyleFontWeight::from_number(kept),
            "{given} is held as {kept}"
        );
        assert_eq!(StyleFontWeight::from_number(given).number(), kept, "{given}");
    }
}

/// `NaN` is no weight, but a node must never end up with one outside 1 to 1000.
#[test]
fn from_number_of_nan_is_still_a_weight_in_range() {
    let number = StyleFontWeight::from_number(f32::NAN).number();
    assert!((1.0..=1000.0).contains(&number), "{number}");
}

/// `lighter` / `bolder` are only known against the parent's weight: 400 where a number is asked
/// for.
#[test]
fn a_relative_keyword_counts_as_400_where_a_number_is_asked_for() {
    assert_eq!(StyleFontWeight::Lighter.number(), 400.0);
    assert_eq!(StyleFontWeight::Bolder.number(), 400.0);
}

#[test]
fn weights_order_by_their_number() {
    assert!(StyleFontWeight::W100 < StyleFontWeight::NORMAL);
    assert!(StyleFontWeight::NORMAL < StyleFontWeight::BOLD);
    assert!(StyleFontWeight::BOLD < StyleFontWeight::W900);
    assert!(StyleFontWeight::from_number(350.5) < StyleFontWeight::from_number(351.0));
    assert!(StyleFontWeight::from_number(1.0) < StyleFontWeight::from_number(1000.0));
}

/// The shortest CSS: 400 and 700 are keywords however the weight was made, any other number is
/// the number.
#[test]
fn weights_print_as_the_shortest_css() {
    for (weight, css) in [
        (StyleFontWeight::NORMAL, "normal"),
        (StyleFontWeight::BOLD, "bold"),
        (StyleFontWeight::Lighter, "lighter"),
        (StyleFontWeight::Bolder, "bolder"),
        (StyleFontWeight::W100, "100"),
        (StyleFontWeight::W200, "200"),
        (StyleFontWeight::W300, "300"),
        (StyleFontWeight::W500, "500"),
        (StyleFontWeight::W600, "600"),
        (StyleFontWeight::W800, "800"),
        (StyleFontWeight::W900, "900"),
        (StyleFontWeight::from_number(1.0), "1"),
        (StyleFontWeight::from_number(350.0), "350"),
        (StyleFontWeight::from_number(350.5), "350.5"),
        (StyleFontWeight::from_number(650.25), "650.25"),
        (StyleFontWeight::from_number(1000.0), "1000"),
        (StyleFontWeight::from_number(400.0), "normal"),
        (StyleFontWeight::from_number(700.0), "bold"),
    ] {
        assert_eq!(weight.print_as_css_value(), css, "{weight:?}");
    }
}

// ---- StyleFontWeight: bolder / lighter ---------------------------------------------------------

/// `bolder` against a parent of the given weight, as CSS Fonts 4 s2.2 has it: the weight it makes,
/// or `None` where the parent's own weight stays (`900` and up cannot get bolder).
static BOLDER_THAN: [(f32, Option<f32>); 16] = [
    (1.0, Some(400.0)),
    (99.0, Some(400.0)),
    (100.0, Some(400.0)),
    (349.0, Some(400.0)),
    (349.999, Some(400.0)),
    (350.0, Some(700.0)),
    (549.0, Some(700.0)),
    (549.999, Some(700.0)),
    (550.0, Some(900.0)),
    (749.0, Some(900.0)),
    (750.0, Some(900.0)),
    (899.0, Some(900.0)),
    (899.999, Some(900.0)),
    (900.0, None),
    (950.0, None),
    (1000.0, None),
];

/// `lighter` against a parent of the given weight: the weight it makes, or `None` where the
/// parent's own weight stays (below `100` there is nothing lighter to pick).
static LIGHTER_THAN: [(f32, Option<f32>); 17] = [
    (1.0, None),
    (50.0, None),
    (99.0, None),
    (99.999, None),
    (100.0, Some(100.0)),
    (349.0, Some(100.0)),
    (350.0, Some(100.0)),
    (549.0, Some(100.0)),
    (549.999, Some(100.0)),
    (550.0, Some(400.0)),
    (749.0, Some(400.0)),
    (749.999, Some(400.0)),
    (750.0, Some(700.0)),
    (899.0, Some(700.0)),
    (900.0, Some(700.0)),
    (950.0, Some(700.0)),
    (1000.0, Some(700.0)),
];

#[test]
fn bolder_follows_the_css_fonts_4_table_at_every_boundary() {
    for &(parent, made) in BOLDER_THAN.iter() {
        let parent_weight = StyleFontWeight::from_number(parent);
        let expected = made.map_or(parent_weight, StyleFontWeight::from_number);
        assert_eq!(
            StyleFontWeight::Bolder.computed(parent_weight),
            expected,
            "bolder than {parent}"
        );
    }
}

#[test]
fn lighter_follows_the_css_fonts_4_table_at_every_boundary() {
    for &(parent, made) in LIGHTER_THAN.iter() {
        let parent_weight = StyleFontWeight::from_number(parent);
        let expected = made.map_or(parent_weight, StyleFontWeight::from_number);
        assert_eq!(
            StyleFontWeight::Lighter.computed(parent_weight),
            expected,
            "lighter than {parent}"
        );
    }
}

/// The table every designer knows, for the nine hundreds: (parent, bolder, lighter).
#[test]
fn bolder_and_lighter_of_the_hundreds_are_the_familiar_table() {
    for (parent, bolder, lighter) in [
        (100.0, 400.0, 100.0),
        (200.0, 400.0, 100.0),
        (300.0, 400.0, 100.0),
        (400.0, 700.0, 100.0),
        (500.0, 700.0, 100.0),
        (600.0, 900.0, 400.0),
        (700.0, 900.0, 400.0),
        (800.0, 900.0, 700.0),
        (900.0, 900.0, 700.0),
    ] {
        let parent = StyleFontWeight::from_number(parent);
        assert_eq!(
            StyleFontWeight::Bolder.computed(parent).number(),
            bolder,
            "bolder than {parent:?}"
        );
        assert_eq!(
            StyleFontWeight::Lighter.computed(parent).number(),
            lighter,
            "lighter than {parent:?}"
        );
    }
}

/// A parent that is still a keyword counts as 400.
#[test]
fn a_parent_that_is_still_a_keyword_counts_as_400() {
    for parent in [StyleFontWeight::Lighter, StyleFontWeight::Bolder] {
        assert_eq!(
            StyleFontWeight::Bolder.computed(parent),
            StyleFontWeight::BOLD,
            "bolder than {parent:?}"
        );
        assert_eq!(
            StyleFontWeight::Lighter.computed(parent),
            StyleFontWeight::W100,
            "lighter than {parent:?}"
        );
    }
}

/// Anything but `bolder` / `lighter` is its own computed value, whatever the parent is.
#[test]
fn a_weight_that_is_not_relative_is_its_own_computed_value() {
    for own in [
        StyleFontWeight::W100,
        StyleFontWeight::NORMAL,
        StyleFontWeight::BOLD,
        StyleFontWeight::W900,
        StyleFontWeight::from_number(1.0),
        StyleFontWeight::from_number(350.5),
        StyleFontWeight::from_number(1000.0),
    ] {
        for parent in [
            StyleFontWeight::W100,
            StyleFontWeight::NORMAL,
            StyleFontWeight::BOLD,
            StyleFontWeight::W900,
            StyleFontWeight::from_number(1000.0),
            StyleFontWeight::Lighter,
            StyleFontWeight::Bolder,
        ] {
            assert_eq!(own.computed(parent), own, "{own:?} under {parent:?}");
        }
    }
}

/// The descendants inherit the number a keyword came to, so a chain of `bolder` climbs the ladder
/// and stops at its end.
#[test]
fn a_chain_of_bolder_or_lighter_climbs_the_ladder_and_stops_at_its_ends() {
    fn walk(start: StyleFontWeight, step: StyleFontWeight, times: usize) -> Vec<f32> {
        let mut weight = start;
        let mut seen = Vec::new();
        for _ in 0..times {
            weight = step.computed(weight);
            seen.push(weight.number());
        }
        seen
    }
    assert_eq!(
        walk(StyleFontWeight::NORMAL, StyleFontWeight::Bolder, 4),
        [700.0, 900.0, 900.0, 900.0]
    );
    assert_eq!(
        walk(StyleFontWeight::W100, StyleFontWeight::Bolder, 5),
        [400.0, 700.0, 900.0, 900.0, 900.0]
    );
    assert_eq!(
        walk(StyleFontWeight::from_number(1000.0), StyleFontWeight::Bolder, 2),
        [1000.0, 1000.0],
        "a weight past 900 stays where it is"
    );
    assert_eq!(
        walk(StyleFontWeight::W900, StyleFontWeight::Lighter, 4),
        [700.0, 400.0, 100.0, 100.0]
    );
    assert_eq!(
        walk(StyleFontWeight::from_number(1000.0), StyleFontWeight::Lighter, 4),
        [700.0, 400.0, 100.0, 100.0]
    );
    assert_eq!(
        walk(StyleFontWeight::from_number(50.0), StyleFontWeight::Lighter, 2),
        [50.0, 50.0],
        "a weight under 100 stays where it is"
    );
}

#[test]
fn only_lighter_and_bolder_are_relative() {
    assert!(StyleFontWeight::Lighter.is_relative());
    assert!(StyleFontWeight::Bolder.is_relative());
    for weight in [
        StyleFontWeight::NORMAL,
        StyleFontWeight::BOLD,
        StyleFontWeight::W100,
        StyleFontWeight::W900,
        StyleFontWeight::from_number(1.0),
        StyleFontWeight::from_number(350.5),
        StyleFontWeight::from_number(1000.0),
    ] {
        assert!(!weight.is_relative(), "{weight:?}");
    }
}

/// The cascade evaluates these where the value is a constant, so they are `const fn`s.
#[test]
fn computed_and_is_relative_work_in_const_context() {
    const BOLDER_THAN_NORMAL: StyleFontWeight =
        StyleFontWeight::Bolder.computed(StyleFontWeight::NORMAL);
    const LIGHTER_IS_RELATIVE: bool = StyleFontWeight::Lighter.is_relative();
    const BOLD_IS_RELATIVE: bool = StyleFontWeight::BOLD.is_relative();
    assert_eq!(BOLDER_THAN_NORMAL, StyleFontWeight::BOLD);
    assert!(LIGHTER_IS_RELATIVE);
    assert!(!BOLD_IS_RELATIVE);
}

// ---- parse_font_weight -------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn parse_font_weight_takes_the_keywords() {
    assert_eq!(parse_font_weight("normal"), Ok(StyleFontWeight::NORMAL));
    assert_eq!(parse_font_weight("bold"), Ok(StyleFontWeight::BOLD));
    assert_eq!(parse_font_weight("bolder"), Ok(StyleFontWeight::Bolder));
    assert_eq!(parse_font_weight("lighter"), Ok(StyleFontWeight::Lighter));
}

/// Any number from 1 to 1000, fractions included, in the spellings a CSS number has.
#[cfg(feature = "parser")]
#[test]
fn parse_font_weight_takes_every_number_from_1_to_1000() {
    for (input, number) in [
        ("1", 1.0),
        ("100", 100.0),
        ("350", 350.0),
        ("350.5", 350.5),
        ("400", 400.0),
        ("650.25", 650.25),
        ("700", 700.0),
        ("900", 900.0),
        ("999.5", 999.5),
        ("1000", 1000.0),
        ("0400", 400.0),
        ("+400", 400.0),
        ("400.0", 400.0),
        ("1000.0", 1000.0),
    ] {
        assert_eq!(
            parse_font_weight(input),
            Ok(StyleFontWeight::from_number(number)),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn the_constants_are_what_the_parser_makes_of_their_numbers() {
    for (input, constant) in [
        ("100", StyleFontWeight::W100),
        ("200", StyleFontWeight::W200),
        ("300", StyleFontWeight::W300),
        ("400", StyleFontWeight::NORMAL),
        ("500", StyleFontWeight::W500),
        ("600", StyleFontWeight::W600),
        ("700", StyleFontWeight::BOLD),
        ("800", StyleFontWeight::W800),
        ("900", StyleFontWeight::W900),
        ("normal", StyleFontWeight::NORMAL),
        ("bold", StyleFontWeight::BOLD),
    ] {
        assert_eq!(parse_font_weight(input), Ok(constant), "`{input}`");
    }
}

/// A parsed weight prints as the shortest CSS and that parses back to the same weight.
#[cfg(feature = "parser")]
#[test]
fn a_parsed_weight_prints_back_and_parses_again() {
    for (input, printed) in [
        ("normal", "normal"),
        ("bold", "bold"),
        ("bolder", "bolder"),
        ("lighter", "lighter"),
        ("1", "1"),
        ("100", "100"),
        ("350", "350"),
        ("350.5", "350.5"),
        ("400", "normal"),
        ("700", "bold"),
        ("900", "900"),
        ("1000", "1000"),
        ("0400", "normal"),
        ("+400", "normal"),
        ("400.0", "normal"),
        ("700.0", "bold"),
    ] {
        let weight = parse_font_weight(input)
            .unwrap_or_else(|e| panic!("`{input}` must parse: {e:?}"));
        assert_eq!(weight.print_as_css_value(), printed, "`{input}`");
        assert_eq!(
            parse_font_weight(printed),
            Ok(weight),
            "`{printed}` is the same weight as `{input}`"
        );
    }
}

/// Out of range, no number, or a number the grammar here does not take (no exponent): the error
/// carries the trimmed input.
#[cfg(feature = "parser")]
#[test]
fn parse_font_weight_rejects_what_is_not_a_weight_from_1_to_1000() {
    for input in [
        "0",
        "-0",
        "0.999",
        "0.5",
        ".5",
        "1001",
        "1000.5",
        "-1",
        "-400",
        "5000",
        "9223372036854775807",
        "-9223372036854775808",
        "1e3",
        "4e2",
        "1E3",
        "1e+3",
        "NaN",
        "nan",
        "inf",
        "-inf",
        "infinity",
        "abc",
        "thin",
        "boldest",
        "",
        "   ",
        "\t\n",
        "bold;x",
        "400px",
        "400 700",
        "bold 700",
        "400%",
        "+-400",
        "--400",
        "0x190",
        "1,000",
        "-",
        "+",
        ".",
        "4.",
        "100_0",
    ] {
        assert_eq!(
            parse_font_weight(input),
            Err(weight_error(input)),
            "`{input}`"
        );
    }
}

#[cfg(feature = "parser")]
#[test]
fn parse_font_weight_trims_the_input_and_the_error_carries_the_trimmed_text() {
    assert_eq!(
        parse_font_weight("  350  "),
        Ok(StyleFontWeight::from_number(350.0))
    );
    assert_eq!(parse_font_weight("\t bold \n"), Ok(StyleFontWeight::BOLD));
    assert_eq!(
        parse_font_weight("  1001  "),
        Err(CssFontWeightParseError::InvalidValue(InvalidValueErr("1001")))
    );
    assert_eq!(
        parse_font_weight(" bold;x "),
        Err(CssFontWeightParseError::InvalidValue(InvalidValueErr("bold;x")))
    );
}

/// A number of any length is a number (too big or too small), never a panic.
#[cfg(feature = "parser")]
#[test]
fn parse_font_weight_survives_numbers_of_any_length() {
    assert!(parse_font_weight(&"9".repeat(100_000)).is_err());
    assert!(parse_font_weight(&format!("0.{}1", "0".repeat(100_000))).is_err());
    assert!(parse_font_weight(&format!("-{}", "9".repeat(100_000))).is_err());
}

// ---- StyleFontStyle ----------------------------------------------------------------------------

#[test]
fn the_default_style_is_normal() {
    assert_eq!(StyleFontStyle::default(), StyleFontStyle::Normal);
}

/// A bare `oblique` is `oblique 14deg`.
#[test]
fn a_bare_oblique_is_14_degrees() {
    assert_eq!(
        StyleFontStyle::OBLIQUE,
        StyleFontStyle::Oblique(FloatValue::const_new(14))
    );
    assert_eq!(StyleFontStyle::OBLIQUE, oblique(14.0));
    assert_eq!(StyleFontStyle::OBLIQUE.oblique_degrees(), 14.0);
    assert_ne!(StyleFontStyle::OBLIQUE, StyleFontStyle::Normal);
    assert_ne!(StyleFontStyle::OBLIQUE, StyleFontStyle::Italic);
    assert_ne!(StyleFontStyle::OBLIQUE, oblique(0.0));
}

/// The slant of an `oblique`, 0 for the styles without one.
#[test]
fn oblique_degrees_is_the_angle_and_0_for_normal_and_italic() {
    assert_eq!(StyleFontStyle::Normal.oblique_degrees(), 0.0);
    assert_eq!(StyleFontStyle::Italic.oblique_degrees(), 0.0);
    for degrees in [0.0, 1.0, 12.5, 14.0, 20.0, 90.0, -10.0, -90.0] {
        assert_eq!(oblique(degrees).oblique_degrees(), degrees, "{degrees}");
    }
}

/// `oblique` alone is 14 degrees and prints as `oblique`; any other angle is spelled out.
#[test]
fn styles_print_as_css() {
    for (style, css) in [
        (StyleFontStyle::Normal, "normal"),
        (StyleFontStyle::Italic, "italic"),
        (StyleFontStyle::OBLIQUE, "oblique"),
        (oblique(14.0), "oblique"),
        (oblique(20.0), "oblique 20deg"),
        (oblique(-10.0), "oblique -10deg"),
        (oblique(0.0), "oblique 0deg"),
        (oblique(12.5), "oblique 12.5deg"),
        (oblique(90.0), "oblique 90deg"),
        (oblique(-90.0), "oblique -90deg"),
    ] {
        assert_eq!(style.print_as_css_value(), css, "{style:?}");
    }
}

// ---- parse_font_style --------------------------------------------------------------------------

#[cfg(feature = "parser")]
#[test]
fn parse_font_style_takes_the_three_keywords() {
    assert_eq!(parse_font_style("normal"), Ok(StyleFontStyle::Normal));
    assert_eq!(parse_font_style("italic"), Ok(StyleFontStyle::Italic));
    assert_eq!(parse_font_style("oblique"), Ok(StyleFontStyle::OBLIQUE));
    assert_eq!(parse_font_style("oblique"), Ok(oblique(14.0)));
}

/// `oblique <angle>`: the angle is the slant in degrees, from -90 to 90, in `deg`, `grad`, `rad`
/// or `turn`; the extra spaces and any CSS whitespace between the two are fine.
#[cfg(feature = "parser")]
#[test]
fn parse_font_style_takes_an_angle_after_oblique() {
    for (input, degrees) in [
        ("oblique 20deg", 20.0),
        ("oblique -10deg", -10.0),
        ("oblique 0deg", 0.0),
        ("oblique 90deg", 90.0),
        ("oblique -90deg", -90.0),
        ("oblique 14deg", 14.0),
        ("oblique 14.0deg", 14.0),
        ("oblique 12.5deg", 12.5),
        ("oblique 0.1turn", 36.0),
        ("oblique 100grad", 90.0),
        ("oblique -100grad", -90.0),
        ("oblique 50grad", 45.0),
        ("oblique 0.25turn", 90.0),
        ("oblique -0.25turn", -90.0),
        ("  oblique   20deg  ", 20.0),
        ("oblique\t20deg", 20.0),
    ] {
        assert_eq!(
            parse_font_style(input),
            Ok(oblique(degrees)),
            "`{input}`"
        );
    }
    assert_eq!(parse_font_style("oblique 14deg"), Ok(StyleFontStyle::OBLIQUE));
}

/// Radians are not exact degrees: the angle is the right slant to a hundredth of a degree.
#[cfg(feature = "parser")]
#[test]
fn parse_font_style_converts_radians() {
    let style = parse_font_style("oblique 0.5rad").expect("0.5rad is an angle");
    assert!((style.oblique_degrees() - 28.648).abs() < 0.01, "{style:?}");
    let style = parse_font_style("oblique -0.5rad").expect("-0.5rad is an angle");
    assert!((style.oblique_degrees() + 28.648).abs() < 0.01, "{style:?}");
}

/// An angle past 90 degrees either way, a value that is not an angle (a bare number, a
/// percentage, no unit), an angle after another keyword, anything after the angle, and the
/// keywords as the spec has them (lowercase, exactly): the error carries the trimmed input.
#[cfg(feature = "parser")]
#[test]
fn parse_font_style_rejects_what_is_not_a_style() {
    for input in [
        "",
        "   ",
        "slanted",
        "italics",
        "ITALIC",
        "oblique 91deg",
        "oblique -91deg",
        "oblique 90.01deg",
        "oblique -90.01deg",
        "oblique 0.5turn",
        "oblique -0.5turn",
        "oblique 101grad",
        "oblique 2rad",
        "italic 10deg",
        "normal 10deg",
        "oblique 10",
        "oblique 10%",
        "oblique deg",
        "oblique 10deg 20deg",
        "oblique 10 deg",
        "oblique 10degs",
        "oblique20deg",
        "oblique, 10deg",
        "oblique NaNdeg",
        "oblique infdeg",
        "oblique 20deg italic",
        "oblique italic",
        "oblique oblique",
        "italic oblique",
        "italic;",
        "oblique;",
        "0",
        "-0",
        "NaN",
        "inf",
        "9223372036854775807",
    ] {
        assert_eq!(parse_font_style(input), Err(style_error(input)), "`{input}`");
    }
}

#[cfg(feature = "parser")]
#[test]
fn parse_font_style_error_carries_the_trimmed_input() {
    assert_eq!(
        parse_font_style("  oblique 91deg  "),
        Err(CssFontStyleParseError::InvalidValue(InvalidValueErr(
            "oblique 91deg"
        )))
    );
    assert_eq!(
        parse_font_style(" slanted "),
        Err(CssFontStyleParseError::InvalidValue(InvalidValueErr("slanted")))
    );
}

/// A style prints as CSS, and that parses back to the same style.
#[cfg(feature = "parser")]
#[test]
fn a_style_prints_back_and_parses_again() {
    for style in [
        StyleFontStyle::Normal,
        StyleFontStyle::Italic,
        StyleFontStyle::OBLIQUE,
        oblique(20.0),
        oblique(-10.0),
        oblique(0.0),
        oblique(90.0),
        oblique(-90.0),
        oblique(12.5),
        oblique(33.333),
    ] {
        let css = style.print_as_css_value();
        assert_eq!(
            parse_font_style(&css),
            Ok(style),
            "`{css}` is the style {style:?}"
        );
    }
}

/// An angle of any length is an angle too big to be a slant, never a panic.
#[cfg(feature = "parser")]
#[test]
fn parse_font_style_survives_angles_of_any_length() {
    assert!(parse_font_style(&format!("oblique {}deg", "9".repeat(100_000))).is_err());
    assert!(parse_font_style(&format!("oblique {}", "(".repeat(10_000))).is_err());
    assert!(parse_font_style(&"oblique ".repeat(10_000)).is_err());
}
