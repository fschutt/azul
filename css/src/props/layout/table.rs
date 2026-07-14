//! CSS properties for table layout and styling.
//!
//! This module contains properties specific to CSS table formatting:
//! - `table-layout`: Controls the algorithm used to layout table cells, rows, and columns
//! - `border-collapse`: Specifies whether cell borders are collapsed into a single border or
//!   separated
//! - `border-spacing`: Sets the distance between borders of adjacent cells (separate borders only)
//! - `caption-side`: Specifies the placement of a table caption
//! - `empty-cells`: Specifies whether or not to display borders on empty cells in a table

use alloc::string::{String, ToString};

use crate::{
    codegen::format::FormatAsRustCode,
    props::{
        basic::pixel::{CssPixelValueParseError, PixelValue},
        formatter::PrintAsCssValue,
    },
};

// table-layout

/// Controls the algorithm used to lay out table cells, rows, and columns.
///
/// The `table-layout` property determines whether the browser should use:
/// - **auto**: Column widths are determined by the content (slower but flexible)
/// - **fixed**: Column widths are determined by the first row (faster and predictable)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutTableLayout {
    /// Use automatic table layout algorithm (content-based, default).
    /// Column width is set by the widest unbreakable content in the cells.
    #[default]
    Auto,
    /// Use fixed table layout algorithm (first-row-based).
    /// Column width is set by the width property of the column or first-row cell.
    /// Renders faster than auto.
    Fixed,
}


impl PrintAsCssValue for LayoutTableLayout {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Fixed => "fixed".to_string(),
        }
    }
}

impl FormatAsRustCode for LayoutTableLayout {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => "LayoutTableLayout::Auto".to_string(),
            Self::Fixed => "LayoutTableLayout::Fixed".to_string(),
        }
    }
}

// border-collapse

/// Specifies whether cell borders are collapsed into a single border or separated.
///
/// The `border-collapse` property determines the border rendering model:
/// - **separate**: Each cell has its own border (default, uses border-spacing)
/// - **collapse**: Adjacent cells share borders (ignores border-spacing)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleBorderCollapse {
    /// Borders are separated (default). Each cell has its own border.
    /// The `border-spacing` property defines the distance between borders.
    #[default]
    Separate,
    /// Borders are collapsed. Adjacent cells share a single border.
    /// Border conflict resolution rules apply when borders differ.
    Collapse,
}


impl PrintAsCssValue for StyleBorderCollapse {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Separate => "separate".to_string(),
            Self::Collapse => "collapse".to_string(),
        }
    }
}

impl FormatAsRustCode for StyleBorderCollapse {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Separate => "StyleBorderCollapse::Separate".to_string(),
            Self::Collapse => "StyleBorderCollapse::Collapse".to_string(),
        }
    }
}

// border-spacing

/// Sets the distance between the borders of adjacent cells.
///
/// The `border-spacing` property is only applicable when `border-collapse` is set to `separate`.
/// It can have one or two values:
/// - One value: Sets both horizontal and vertical spacing
/// - Two values: First is horizontal, second is vertical
///
/// This struct represents a single spacing value (either horizontal or vertical).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderSpacing {
    /// Horizontal spacing between cell borders
    pub horizontal: PixelValue,
    /// Vertical spacing between cell borders
    pub vertical: PixelValue,
}

impl Default for LayoutBorderSpacing {
    fn default() -> Self {
        // Default border-spacing is 0 (no spacing)
        Self {
            horizontal: PixelValue::const_px(0),
            vertical: PixelValue::const_px(0),
        }
    }
}

impl LayoutBorderSpacing {
    /// Creates a new border spacing with the same value for horizontal and vertical
    #[must_use] pub const fn new(spacing: PixelValue) -> Self {
        Self {
            horizontal: spacing,
            vertical: spacing,
        }
    }

    /// Creates a new border spacing with different horizontal and vertical values
    #[must_use] pub const fn new_separate(horizontal: PixelValue, vertical: PixelValue) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }
}

impl PrintAsCssValue for LayoutBorderSpacing {
    fn print_as_css_value(&self) -> String {
        if self.horizontal == self.vertical {
            // Single value: same for both dimensions
            self.horizontal.to_string()
        } else {
            // Two values: horizontal vertical
            format!("{} {}", self.horizontal, self.vertical)
        }
    }
}

impl FormatAsRustCode for LayoutBorderSpacing {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use crate::codegen::format::format_pixel_value;
        format!(
            "LayoutBorderSpacing {{ horizontal: {}, vertical: {} }}",
            format_pixel_value(&self.horizontal),
            format_pixel_value(&self.vertical)
        )
    }
}

// caption-side

/// Specifies the placement of a table caption.
///
/// The `caption-side` property positions the caption either above or below the table.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleCaptionSide {
    /// Caption is placed above the table (default)
    #[default]
    Top,
    /// Caption is placed below the table
    Bottom,
}


impl PrintAsCssValue for StyleCaptionSide {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Top => "top".to_string(),
            Self::Bottom => "bottom".to_string(),
        }
    }
}

impl FormatAsRustCode for StyleCaptionSide {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Top => "StyleCaptionSide::Top".to_string(),
            Self::Bottom => "StyleCaptionSide::Bottom".to_string(),
        }
    }
}

// empty-cells

/// Specifies whether or not to display borders and background on empty cells.
///
/// The `empty-cells` property only applies when `border-collapse` is set to `separate`.
/// A cell is considered empty if it contains no visible content.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum StyleEmptyCells {
    /// Show borders and background on empty cells (default)
    #[default]
    Show,
    /// Hide borders and background on empty cells
    Hide,
}


impl PrintAsCssValue for StyleEmptyCells {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Show => "show".to_string(),
            Self::Hide => "hide".to_string(),
        }
    }
}

impl FormatAsRustCode for StyleEmptyCells {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Show => "StyleEmptyCells::Show".to_string(),
            Self::Hide => "StyleEmptyCells::Hide".to_string(),
        }
    }
}

// Parsing Functions

/// Parse errors for table-layout property
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LayoutTableLayoutParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse a table-layout value from a string
pub(crate) fn parse_table_layout(
    input: &str,
) -> Result<LayoutTableLayout, LayoutTableLayoutParseError<'_>> {
    match input.trim() {
        "auto" => Ok(LayoutTableLayout::Auto),
        "fixed" => Ok(LayoutTableLayout::Fixed),
        other => Err(LayoutTableLayoutParseError::InvalidKeyword(other)),
    }
}

/// Parse errors for border-collapse property
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StyleBorderCollapseParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse a border-collapse value from a string
pub(crate) fn parse_border_collapse(
    input: &str,
) -> Result<StyleBorderCollapse, StyleBorderCollapseParseError<'_>> {
    match input.trim() {
        "separate" => Ok(StyleBorderCollapse::Separate),
        "collapse" => Ok(StyleBorderCollapse::Collapse),
        other => Err(StyleBorderCollapseParseError::InvalidKeyword(other)),
    }
}

/// Parse errors for border-spacing property
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LayoutBorderSpacingParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
    InvalidFormat,
}

/// Parse a border-spacing value from a string
/// Accepts: "5px" or "5px 10px"
pub(crate) fn parse_border_spacing(
    input: &str,
) -> Result<LayoutBorderSpacing, LayoutBorderSpacingParseError<'_>> {
    use crate::props::basic::parse_pixel_value;

    let parts: Vec<&str> = input.split_whitespace().collect();

    match parts.len() {
        1 => {
            // Single value: use for both horizontal and vertical
            let value =
                parse_pixel_value(parts[0]).map_err(LayoutBorderSpacingParseError::PixelValue)?;
            Ok(LayoutBorderSpacing::new(value))
        }
        2 => {
            // Two values: horizontal vertical
            let horizontal =
                parse_pixel_value(parts[0]).map_err(LayoutBorderSpacingParseError::PixelValue)?;
            let vertical =
                parse_pixel_value(parts[1]).map_err(LayoutBorderSpacingParseError::PixelValue)?;
            Ok(LayoutBorderSpacing::new_separate(horizontal, vertical))
        }
        _ => Err(LayoutBorderSpacingParseError::InvalidFormat),
    }
}

/// Parse errors for caption-side property
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StyleCaptionSideParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse a caption-side value from a string
pub(crate) fn parse_caption_side(
    input: &str,
) -> Result<StyleCaptionSide, StyleCaptionSideParseError<'_>> {
    match input.trim() {
        "top" => Ok(StyleCaptionSide::Top),
        "bottom" => Ok(StyleCaptionSide::Bottom),
        other => Err(StyleCaptionSideParseError::InvalidKeyword(other)),
    }
}

/// Parse errors for empty-cells property
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StyleEmptyCellsParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse an empty-cells value from a string
pub(crate) fn parse_empty_cells(
    input: &str,
) -> Result<StyleEmptyCells, StyleEmptyCellsParseError<'_>> {
    match input.trim() {
        "show" => Ok(StyleEmptyCells::Show),
        "hide" => Ok(StyleEmptyCells::Hide),
        other => Err(StyleEmptyCellsParseError::InvalidKeyword(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_table_layout() {
        assert_eq!(parse_table_layout("auto").unwrap(), LayoutTableLayout::Auto);
        assert_eq!(
            parse_table_layout("fixed").unwrap(),
            LayoutTableLayout::Fixed
        );
        assert!(parse_table_layout("invalid").is_err());
    }

    #[test]
    fn test_parse_border_collapse() {
        assert_eq!(
            parse_border_collapse("separate").unwrap(),
            StyleBorderCollapse::Separate
        );
        assert_eq!(
            parse_border_collapse("collapse").unwrap(),
            StyleBorderCollapse::Collapse
        );
        assert!(parse_border_collapse("invalid").is_err());
    }

    #[test]
    fn test_parse_border_spacing() {
        let spacing1 = parse_border_spacing("5px").unwrap();
        assert_eq!(spacing1.horizontal, PixelValue::const_px(5));
        assert_eq!(spacing1.vertical, PixelValue::const_px(5));

        let spacing2 = parse_border_spacing("5px 10px").unwrap();
        assert_eq!(spacing2.horizontal, PixelValue::const_px(5));
        assert_eq!(spacing2.vertical, PixelValue::const_px(10));
    }

    #[test]
    fn test_parse_caption_side() {
        assert_eq!(parse_caption_side("top").unwrap(), StyleCaptionSide::Top);
        assert_eq!(
            parse_caption_side("bottom").unwrap(),
            StyleCaptionSide::Bottom
        );
        assert!(parse_caption_side("invalid").is_err());
    }

    #[test]
    fn test_parse_empty_cells() {
        assert_eq!(parse_empty_cells("show").unwrap(), StyleEmptyCells::Show);
        assert_eq!(parse_empty_cells("hide").unwrap(), StyleEmptyCells::Hide);
        assert!(parse_empty_cells("invalid").is_err());
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact comparisons are the point: FloatValue is fixed-point
mod autotest_generated {
    use crate::props::basic::SizeMetric;

    use super::*;

    // `FloatValue` stores millipixels in an `isize` (value * 1000, truncated via
    // `as`). `.number.number()` is that raw integer -- asserting on it keeps the
    // numeric tests exact instead of comparing floats.
    fn raw(p: PixelValue) -> isize {
        p.number.number()
    }

    fn table_layout_ok(s: &str) -> bool {
        parse_table_layout(s).is_ok()
    }
    fn border_collapse_ok(s: &str) -> bool {
        parse_border_collapse(s).is_ok()
    }
    fn caption_side_ok(s: &str) -> bool {
        parse_caption_side(s).is_ok()
    }
    fn empty_cells_ok(s: &str) -> bool {
        parse_empty_cells(s).is_ok()
    }

    /// Every keyword parser, type-erased to `&str -> bool` (is it accepted?), so
    /// the malformed-input cases can be asserted against all four at once.
    const KEYWORD_PARSERS: &[(&str, fn(&str) -> bool)] = &[
        ("table-layout", table_layout_ok),
        ("border-collapse", border_collapse_ok),
        ("caption-side", caption_side_ok),
        ("empty-cells", empty_cells_ok),
    ];

    fn assert_all_keyword_parsers_reject(input: &str, why: &str) {
        for (prop, parse) in KEYWORD_PARSERS {
            assert!(
                !parse(input),
                "{prop}: expected {input:?} to be rejected ({why}), but it parsed"
            );
        }
    }

    // ------------------------------------------------------------------------
    // constructors
    // ------------------------------------------------------------------------

    #[test]
    fn new_applies_the_same_spacing_to_both_axes() {
        let s = LayoutBorderSpacing::new(PixelValue::const_px(7));
        assert_eq!(s.horizontal, PixelValue::const_px(7));
        assert_eq!(s.vertical, PixelValue::const_px(7));
        assert_eq!(s.horizontal, s.vertical);
    }

    #[test]
    fn new_separate_preserves_argument_order() {
        // Asymmetric on purpose: a swapped assignment would still pass if both
        // axes shared a metric *and* a value.
        let s = LayoutBorderSpacing::new_separate(
            PixelValue::const_px(3),
            PixelValue::percent(50.0),
        );
        assert_eq!(s.horizontal, PixelValue::const_px(3));
        assert_eq!(s.vertical, PixelValue::percent(50.0));
        assert_ne!(s.horizontal, s.vertical);
    }

    #[test]
    fn constructors_do_not_panic_on_extreme_floats() {
        let extremes = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            f32::EPSILON,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ];

        for v in extremes {
            let same = LayoutBorderSpacing::new(PixelValue::px(v));
            assert_eq!(same.horizontal, same.vertical, "new({v}) must be symmetric");

            for w in extremes {
                let sep =
                    LayoutBorderSpacing::new_separate(PixelValue::px(v), PixelValue::em(w));
                assert_eq!(sep.horizontal.metric, SizeMetric::Px);
                assert_eq!(sep.vertical.metric, SizeMetric::Em);
                // Whatever the input float was, the stored value is a plain isize,
                // so it can never be NaN/inf once it lands in the struct.
                assert!(sep.horizontal.number.get().is_finite());
                assert!(sep.vertical.number.get().is_finite());
            }
        }
    }

    #[test]
    fn nan_spacing_is_flattened_to_zero_and_stays_comparable() {
        // `FloatValue::new` does `NaN * 1000.0 as isize`, and an `as` cast maps NaN
        // to 0. So a NaN spacing silently becomes 0px rather than poisoning Eq/Ord
        // (which are derived over the isize, not the float).
        let nan = LayoutBorderSpacing::new(PixelValue::px(f32::NAN));
        assert_eq!(raw(nan.horizontal), 0);
        assert_eq!(raw(nan.vertical), 0);

        // Reflexivity would fail here if equality were float-based.
        assert_eq!(nan, nan);
        assert_eq!(nan, LayoutBorderSpacing::new(PixelValue::px(f32::NAN)));
        assert_eq!(nan, LayoutBorderSpacing::new(PixelValue::px(0.0)));
        assert_eq!(nan, LayoutBorderSpacing::default());
    }

    #[test]
    fn infinite_spacing_saturates_instead_of_wrapping() {
        // `inf * 1000.0 as isize` saturates at the isize bounds; ditto any finite
        // f32 whose *1000 scaling overflows (f32::MAX).
        for input in [f32::INFINITY, f32::MAX, 1e38] {
            let s = LayoutBorderSpacing::new(PixelValue::px(input));
            assert_eq!(raw(s.horizontal), isize::MAX, "{input} must saturate high");
            assert!(s.horizontal.number.get().is_finite());
        }
        for input in [f32::NEG_INFINITY, f32::MIN, -1e38] {
            let s = LayoutBorderSpacing::new(PixelValue::px(input));
            assert_eq!(raw(s.horizontal), isize::MIN, "{input} must saturate low");
            assert!(s.horizontal.number.get().is_finite());
        }
    }

    #[test]
    fn equal_border_spacings_hash_equal() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        fn hash(s: LayoutBorderSpacing) -> u64 {
            let mut h = DefaultHasher::new();
            s.hash(&mut h);
            h.finish()
        }

        // NaN and 0.0 collapse to the same stored value, so the Hash/Eq contract
        // must hold across them too.
        assert_eq!(
            hash(LayoutBorderSpacing::new(PixelValue::px(f32::NAN))),
            hash(LayoutBorderSpacing::default())
        );
        assert_eq!(
            hash(LayoutBorderSpacing::new(PixelValue::const_px(4))),
            hash(LayoutBorderSpacing::new_separate(
                PixelValue::const_px(4),
                PixelValue::const_px(4)
            ))
        );
    }

    #[test]
    fn defaults_are_the_css_initial_values() {
        assert_eq!(LayoutTableLayout::default(), LayoutTableLayout::Auto);
        assert_eq!(StyleBorderCollapse::default(), StyleBorderCollapse::Separate);
        assert_eq!(StyleCaptionSide::default(), StyleCaptionSide::Top);
        assert_eq!(StyleEmptyCells::default(), StyleEmptyCells::Show);

        let d = LayoutBorderSpacing::default();
        assert_eq!(d, LayoutBorderSpacing::new(PixelValue::const_px(0)));
        assert_eq!(raw(d.horizontal), 0);
        assert_eq!(raw(d.vertical), 0);
        assert_eq!(d.horizontal.metric, SizeMetric::Px);
    }

    // ------------------------------------------------------------------------
    // keyword parsers: table-layout / border-collapse / caption-side / empty-cells
    // ------------------------------------------------------------------------

    #[test]
    fn keyword_parsers_accept_every_variant() {
        // Positive controls -- exhaustive over the variants of each enum.
        assert_eq!(parse_table_layout("auto").unwrap(), LayoutTableLayout::Auto);
        assert_eq!(parse_table_layout("fixed").unwrap(), LayoutTableLayout::Fixed);
        assert_eq!(
            parse_border_collapse("separate").unwrap(),
            StyleBorderCollapse::Separate
        );
        assert_eq!(
            parse_border_collapse("collapse").unwrap(),
            StyleBorderCollapse::Collapse
        );
        assert_eq!(parse_caption_side("top").unwrap(), StyleCaptionSide::Top);
        assert_eq!(parse_caption_side("bottom").unwrap(), StyleCaptionSide::Bottom);
        assert_eq!(parse_empty_cells("show").unwrap(), StyleEmptyCells::Show);
        assert_eq!(parse_empty_cells("hide").unwrap(), StyleEmptyCells::Hide);
    }

    #[test]
    fn keyword_parsers_reject_empty_input() {
        assert_all_keyword_parsers_reject("", "empty input");
        assert_eq!(
            parse_table_layout(""),
            Err(LayoutTableLayoutParseError::InvalidKeyword(""))
        );
        assert_eq!(
            parse_border_collapse(""),
            Err(StyleBorderCollapseParseError::InvalidKeyword(""))
        );
        assert_eq!(
            parse_caption_side(""),
            Err(StyleCaptionSideParseError::InvalidKeyword(""))
        );
        assert_eq!(
            parse_empty_cells(""),
            Err(StyleEmptyCellsParseError::InvalidKeyword(""))
        );
    }

    #[test]
    fn keyword_parsers_reject_whitespace_only_input() {
        for input in ["   ", "\t", "\n", "\r\n", "\t\n \x0c", "\u{a0}", "\u{2028}"] {
            assert_all_keyword_parsers_reject(input, "whitespace only");
        }
        // The trim happens before the match, so the error payload is the *trimmed*
        // input -- i.e. the empty string, not the original blanks.
        assert_eq!(
            parse_table_layout("  \t\n "),
            Err(LayoutTableLayoutParseError::InvalidKeyword(""))
        );
    }

    #[test]
    fn keyword_parsers_trim_surrounding_whitespace() {
        assert_eq!(
            parse_table_layout("   auto\t\n").unwrap(),
            LayoutTableLayout::Auto
        );
        assert_eq!(
            parse_border_collapse("\r\n collapse  ").unwrap(),
            StyleBorderCollapse::Collapse
        );
        assert_eq!(
            parse_caption_side("\t bottom \t").unwrap(),
            StyleCaptionSide::Bottom
        );
        assert_eq!(parse_empty_cells("\n hide \n").unwrap(), StyleEmptyCells::Hide);
    }

    #[test]
    fn keyword_parsers_also_trim_non_css_unicode_whitespace() {
        // Characterized leniency: `str::trim` strips everything with the Unicode
        // White_Space property, but CSS whitespace is only space/tab/LF/CR/FF. So
        // NBSP- and LINE-SEPARATOR-padded keywords are accepted here even though a
        // conformant tokenizer would reject them.
        assert_eq!(
            parse_table_layout("\u{a0}auto\u{a0}").unwrap(),
            LayoutTableLayout::Auto
        );
        assert_eq!(
            parse_empty_cells("\u{2028}show\u{2029}").unwrap(),
            StyleEmptyCells::Show
        );
    }

    #[test]
    fn keyword_parse_errors_carry_the_trimmed_input() {
        // The error borrows from `input` (lifetime-tied) and reports the trimmed
        // slice, not the raw one.
        assert_eq!(
            parse_table_layout("  bogus  "),
            Err(LayoutTableLayoutParseError::InvalidKeyword("bogus"))
        );
        assert_eq!(
            parse_border_collapse(" separate collapse "),
            Err(StyleBorderCollapseParseError::InvalidKeyword(
                "separate collapse"
            ))
        );
        assert_eq!(
            parse_caption_side("\ttop;\t"),
            Err(StyleCaptionSideParseError::InvalidKeyword("top;"))
        );
        assert_eq!(
            parse_empty_cells(" show hide "),
            Err(StyleEmptyCellsParseError::InvalidKeyword("show hide"))
        );
    }

    #[test]
    fn keyword_parsers_are_case_sensitive() {
        // Conformance gap (characterized, matching `parse_pixel_value`): CSS
        // keywords are ASCII case-insensitive, so `table-layout: AUTO` is valid CSS
        // -- but the match arms only cover the lowercase spelling.
        for input in [
            "AUTO", "Auto", "aUtO", "FIXED", "SEPARATE", "Collapse", "TOP", "Bottom",
            "SHOW", "Hide",
        ] {
            assert_all_keyword_parsers_reject(input, "keyword matching is case-sensitive");
        }
    }

    #[test]
    fn keyword_parsers_reject_garbage() {
        for input in [
            "invalid",
            "auto fixed",
            "auto;",
            "auto;garbage",
            "auto/*c*/",
            "/*auto*/",
            "au to",
            "a\0uto",
            "\0",
            "\u{7f}",
            "-->",
            "<!--",
            "!important",
            "\\61 uto",
            "'auto'",
            "\"auto\"",
            "auto()",
            "url(auto)",
        ] {
            assert_all_keyword_parsers_reject(input, "garbage");
        }
    }

    #[test]
    fn keyword_parsers_reject_boundary_numeric_strings() {
        // None of these properties take a number; the numeric boundary cases must
        // not be coerced into a keyword.
        for input in [
            "0",
            "-0",
            "0.0",
            "1",
            "-1",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "18446744073709551616", // u64::MAX + 1
            "1e400",
            "-1e400",
            "3.4028235e38",
            "1.17549435e-38",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "infinity",
        ] {
            assert_all_keyword_parsers_reject(input, "numeric input for a keyword property");
        }
    }

    #[test]
    fn keyword_parsers_reject_leading_and_trailing_junk() {
        assert_all_keyword_parsers_reject("xauto", "leading junk");
        assert_all_keyword_parsers_reject("autox", "trailing junk");
        assert_all_keyword_parsers_reject("auto!", "trailing junk");
        assert_all_keyword_parsers_reject("(fixed)", "wrapped in parens");
        assert_all_keyword_parsers_reject("collapse,", "trailing comma");
        // Trailing/leading *whitespace* is the one thing that is trimmed away.
        assert_eq!(parse_table_layout("auto ").unwrap(), LayoutTableLayout::Auto);
    }

    #[test]
    fn keyword_parsers_survive_unicode_input() {
        for input in [
            "\u{1F600}",             // emoji
            "auto\u{0301}",          // combining acute on the final char
            "\u{FF41}\u{FF55}\u{FF54}\u{FF4F}", // fullwidth "auto"
            "\u{202E}auto",          // RTL override prefix
            "\u{FEFF}auto",          // BOM prefix (not Unicode whitespace)
            "аuto",                  // leading CYRILLIC A homoglyph
            "🇩🇪",                    // regional indicator pair
            "e\u{0301}\u{0301}\u{0301}",
            "\u{10FFFF}",            // highest scalar value
        ] {
            assert_all_keyword_parsers_reject(input, "non-ASCII input");
        }
        // Slicing must stay on char boundaries: the error payload borrows the input.
        assert_eq!(
            parse_table_layout("  \u{1F600}  "),
            Err(LayoutTableLayoutParseError::InvalidKeyword("\u{1F600}"))
        );
    }

    #[test]
    fn keyword_parsers_survive_extremely_long_input() {
        let long = "a".repeat(1_000_000);
        assert_all_keyword_parsers_reject(&long, "1M-char input");
        assert_eq!(
            parse_table_layout(&long),
            Err(LayoutTableLayoutParseError::InvalidKeyword(long.as_str()))
        );

        // A million repetitions of a *valid* token is still not the token.
        let repeated = "auto".repeat(250_000);
        assert_all_keyword_parsers_reject(&repeated, "repeated valid token");

        // Padding a valid keyword with a megabyte of whitespace still parses.
        let padded = format!("{}auto{}", " ".repeat(500_000), "\n".repeat(500_000));
        assert_eq!(parse_table_layout(&padded).unwrap(), LayoutTableLayout::Auto);
    }

    #[test]
    fn keyword_parsers_survive_deeply_nested_input() {
        // These parsers are non-recursive, so 10k nesting levels must not blow the
        // stack -- assert that explicitly so a future recursive-descent rewrite of
        // the value parser trips here.
        let nested = format!("{}auto{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_all_keyword_parsers_reject(&nested, "10k nested brackets");

        let braces = "{".repeat(50_000);
        assert_all_keyword_parsers_reject(&braces, "50k open braces");
    }

    // ------------------------------------------------------------------------
    // border-spacing
    // ------------------------------------------------------------------------

    #[test]
    fn border_spacing_single_value_applies_to_both_axes() {
        let s = parse_border_spacing("5px").unwrap();
        assert_eq!(s.horizontal, PixelValue::const_px(5));
        assert_eq!(s.vertical, PixelValue::const_px(5));
        assert_eq!(s, LayoutBorderSpacing::new(PixelValue::const_px(5)));
    }

    #[test]
    fn border_spacing_two_values_are_horizontal_then_vertical() {
        let s = parse_border_spacing("5px 10px").unwrap();
        assert_eq!(s.horizontal, PixelValue::const_px(5));
        assert_eq!(s.vertical, PixelValue::const_px(10));
        // Order matters: the reversed input must not compare equal.
        assert_ne!(s, parse_border_spacing("10px 5px").unwrap());
    }

    #[test]
    fn border_spacing_accepts_mixed_metrics_per_axis() {
        let s = parse_border_spacing("1em 50%").unwrap();
        assert_eq!(s.horizontal.metric, SizeMetric::Em);
        assert_eq!(raw(s.horizontal), 1000);
        assert_eq!(s.vertical.metric, SizeMetric::Percent);
        assert_eq!(raw(s.vertical), 50_000);

        // "rem" must win over "em" in the suffix table.
        assert_eq!(
            parse_border_spacing("2rem").unwrap().horizontal.metric,
            SizeMetric::Rem
        );
        // ...and "vmax"/"vmin" over "vw"/"vh".
        assert_eq!(
            parse_border_spacing("2vmax 3vmin").unwrap().horizontal.metric,
            SizeMetric::Vmax
        );
    }

    #[test]
    fn border_spacing_rejects_empty_and_whitespace_only_input() {
        // `split_whitespace` yields zero parts, which falls through to the `_` arm.
        for input in ["", "   ", "\t\n", "\r\n\x0c ", "\u{a0}", "\u{2028}\u{2029}"] {
            assert_eq!(
                parse_border_spacing(input),
                Err(LayoutBorderSpacingParseError::InvalidFormat),
                "expected InvalidFormat for {input:?}"
            );
        }
    }

    #[test]
    fn border_spacing_rejects_more_than_two_components() {
        for input in ["5px 10px 15px", "1px 2px 3px 4px", "0 0 0"] {
            assert_eq!(
                parse_border_spacing(input),
                Err(LayoutBorderSpacingParseError::InvalidFormat),
                "expected InvalidFormat for {input:?}"
            );
        }
    }

    #[test]
    fn border_spacing_collapses_arbitrary_internal_whitespace() {
        let expected = LayoutBorderSpacing::new_separate(
            PixelValue::const_px(5),
            PixelValue::const_px(10),
        );
        for input in [
            "5px 10px",
            "  5px   10px  ",
            "\t5px\n10px\r\n",
            "5px\x0c10px",
        ] {
            assert_eq!(parse_border_spacing(input).unwrap(), expected, "{input:?}");
        }
    }

    #[test]
    fn border_spacing_splits_on_non_css_unicode_whitespace() {
        // Characterized leniency (same root cause as the keyword trim):
        // `split_whitespace` uses the Unicode White_Space property, so a
        // non-breaking space separates the two components even though CSS would
        // tokenize `5px\u{a0}10px` as a single (invalid) dimension token.
        let s = parse_border_spacing("5px\u{a0}10px").unwrap();
        assert_eq!(s.horizontal, PixelValue::const_px(5));
        assert_eq!(s.vertical, PixelValue::const_px(10));
    }

    #[test]
    fn border_spacing_propagates_pixel_value_errors() {
        // Unit with no number.
        assert!(matches!(
            parse_border_spacing("px"),
            Err(LayoutBorderSpacingParseError::PixelValue(
                CssPixelValueParseError::NoValueGiven("px", SizeMetric::Px)
            ))
        ));
        // Garbage, unknown unit, trailing junk.
        for input in ["abc", "5foo", "5px;", "#5px", "5 .. px"] {
            assert!(
                parse_border_spacing(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
        // A separated unit makes the *second* component the failing one.
        assert!(matches!(
            parse_border_spacing("5 px"),
            Err(LayoutBorderSpacingParseError::PixelValue(
                CssPixelValueParseError::NoValueGiven("px", SizeMetric::Px)
            ))
        ));
        // The error payload borrows the offending slice, not the whole input.
        assert!(matches!(
            parse_border_spacing("5px zzz"),
            Err(LayoutBorderSpacingParseError::PixelValue(
                CssPixelValueParseError::InvalidPixelValue("zzz")
            ))
        ));
    }

    #[test]
    fn border_spacing_accepts_unitless_numbers() {
        // Characterized gap: `parse_pixel_value` falls back to a bare f32 and
        // assumes px, so `border-spacing: 5` is accepted as 5px. Per CSS, only a
        // unitless *zero* is legal for a <length>.
        assert_eq!(
            parse_border_spacing("0").unwrap(),
            LayoutBorderSpacing::new(PixelValue::const_px(0))
        );
        assert_eq!(
            parse_border_spacing("5").unwrap(),
            LayoutBorderSpacing::new(PixelValue::const_px(5))
        );
        // -0 must land on the same stored value as +0 (no signed-zero surprises).
        assert_eq!(raw(parse_border_spacing("-0").unwrap().horizontal), 0);
        assert_eq!(
            parse_border_spacing("-0").unwrap(),
            parse_border_spacing("0").unwrap()
        );
    }

    #[test]
    fn border_spacing_accepts_negative_values() {
        // Characterized gap: `border-spacing` is a non-negative <length> in CSS,
        // but nothing here clamps -- the negative value is stored as-is.
        let s = parse_border_spacing("-5px -10px").unwrap();
        assert_eq!(raw(s.horizontal), -5000);
        assert_eq!(raw(s.vertical), -10_000);
    }

    #[test]
    fn border_spacing_flattens_nan_to_zero() {
        // "NaN" is a valid f32 literal, so this reaches `FloatValue::new(NaN)`,
        // where `NaN * 1000.0 as isize` yields 0. The parse succeeds with 0px --
        // it does not panic, and it does not store a NaN.
        let s = parse_border_spacing("NaNpx").unwrap();
        assert_eq!(raw(s.horizontal), 0);
        assert_eq!(raw(s.vertical), 0);
        assert!(s.horizontal.number.get().is_finite());
        assert_eq!(s, LayoutBorderSpacing::default());

        let mixed = parse_border_spacing("NaNpx 4px").unwrap();
        assert_eq!(raw(mixed.horizontal), 0);
        assert_eq!(raw(mixed.vertical), 4000);
    }

    #[test]
    fn border_spacing_saturates_infinite_and_overflowing_values() {
        // Both an explicit infinity and a finite-but-overflowing literal (whose
        // *1000 scaling overflows isize) must saturate rather than wrap.
        for input in ["infpx", "infinitypx", "1e40px", "3.5e38px"] {
            let s = parse_border_spacing(input).unwrap();
            assert_eq!(raw(s.horizontal), isize::MAX, "{input} must saturate high");
            assert!(s.horizontal.number.get().is_finite(), "{input} must be finite");
        }
        for input in ["-infpx", "-1e40px", "-3.5e38px"] {
            let s = parse_border_spacing(input).unwrap();
            assert_eq!(raw(s.horizontal), isize::MIN, "{input} must saturate low");
            assert!(s.horizontal.number.get().is_finite(), "{input} must be finite");
        }
        // Saturation is per-axis, and the high/low bounds do not collapse together.
        let s = parse_border_spacing("infpx -infpx").unwrap();
        assert_eq!(raw(s.horizontal), isize::MAX);
        assert_eq!(raw(s.vertical), isize::MIN);
        assert_ne!(s.horizontal, s.vertical);
    }

    #[test]
    fn border_spacing_truncates_below_milli_precision() {
        // FloatValue is fixed-point with 3 decimals and truncates (does not round)
        // toward zero, so sub-millipixel input silently becomes 0.
        assert_eq!(raw(parse_border_spacing("0.0005px").unwrap().horizontal), 0);
        assert_eq!(raw(parse_border_spacing("0.9999px").unwrap().horizontal), 999);
        assert_eq!(raw(parse_border_spacing("1.9999px").unwrap().horizontal), 1999);
        // Denormal input must not panic or produce a non-finite stored value.
        let denormal = parse_border_spacing("1.17549435e-38px").unwrap();
        assert_eq!(raw(denormal.horizontal), 0);
    }

    #[test]
    fn border_spacing_survives_extremely_long_input() {
        // One 1M-char token: rejected by the pixel parser, and the error borrows
        // the whole slice back out.
        let long_token = "z".repeat(1_000_000);
        assert!(parse_border_spacing(&long_token).is_err());

        // 200k components: hits the `_ => InvalidFormat` arm without hanging.
        let many = "5px ".repeat(200_000);
        assert_eq!(
            parse_border_spacing(&many),
            Err(LayoutBorderSpacingParseError::InvalidFormat)
        );

        // A megabyte of padding around a valid single value still parses.
        let padded = format!("{}5px{}", " ".repeat(500_000), " ".repeat(500_000));
        assert_eq!(
            parse_border_spacing(&padded).unwrap(),
            LayoutBorderSpacing::new(PixelValue::const_px(5))
        );
    }

    #[test]
    fn border_spacing_survives_deeply_nested_input() {
        let nested = format!("{}5px{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(
            parse_border_spacing(&nested).is_err(),
            "10k nested brackets must be rejected, not stack-overflow"
        );
    }

    #[test]
    fn border_spacing_unicode_input_does_not_panic() {
        for input in [
            "\u{1F600}",
            "5\u{1F600}px",
            "5px \u{1F600}",
            "５px",       // fullwidth digit
            "5\u{0301}px", // combining mark between number and unit
            "\u{FEFF}5px", // BOM prefix (not whitespace -> part of the token)
        ] {
            assert!(
                parse_border_spacing(input).is_err(),
                "expected {input:?} to be rejected"
            );
        }
    }

    // ------------------------------------------------------------------------
    // round-trips: print_as_css_value -> parse_* must be the identity
    // ------------------------------------------------------------------------

    #[test]
    fn keyword_enums_roundtrip_through_css() {
        for v in [LayoutTableLayout::Auto, LayoutTableLayout::Fixed] {
            assert_eq!(parse_table_layout(&v.print_as_css_value()).unwrap(), v);
        }
        for v in [StyleBorderCollapse::Separate, StyleBorderCollapse::Collapse] {
            assert_eq!(parse_border_collapse(&v.print_as_css_value()).unwrap(), v);
        }
        for v in [StyleCaptionSide::Top, StyleCaptionSide::Bottom] {
            assert_eq!(parse_caption_side(&v.print_as_css_value()).unwrap(), v);
        }
        for v in [StyleEmptyCells::Show, StyleEmptyCells::Hide] {
            assert_eq!(parse_empty_cells(&v.print_as_css_value()).unwrap(), v);
        }
        // Defaults round-trip too.
        assert_eq!(
            parse_table_layout(&LayoutTableLayout::default().print_as_css_value()).unwrap(),
            LayoutTableLayout::default()
        );
    }

    #[test]
    fn keyword_enum_css_spellings_are_distinct() {
        // A copy-paste in `print_as_css_value` would make two variants print the
        // same string, which the round-trip above would silently accept for one of
        // them.
        assert_ne!(
            LayoutTableLayout::Auto.print_as_css_value(),
            LayoutTableLayout::Fixed.print_as_css_value()
        );
        assert_ne!(
            StyleBorderCollapse::Separate.print_as_css_value(),
            StyleBorderCollapse::Collapse.print_as_css_value()
        );
        assert_ne!(
            StyleCaptionSide::Top.print_as_css_value(),
            StyleCaptionSide::Bottom.print_as_css_value()
        );
        assert_ne!(
            StyleEmptyCells::Show.print_as_css_value(),
            StyleEmptyCells::Hide.print_as_css_value()
        );
    }

    #[test]
    fn border_spacing_roundtrips_through_css() {
        let values = [
            PixelValue::const_px(0),
            PixelValue::const_px(5),
            PixelValue::const_px(-5),
            PixelValue::px(1.5),
            PixelValue::px(-0.125),
            PixelValue::em(2.0),
            PixelValue::rem(0.5),
            PixelValue::percent(50.0),
            PixelValue::from_metric(SizeMetric::Pt, 12.0),
            PixelValue::from_metric(SizeMetric::Vmin, 3.25),
        ];

        for h in values {
            // Single-value form (horizontal == vertical).
            let same = LayoutBorderSpacing::new(h);
            assert_eq!(
                parse_border_spacing(&same.print_as_css_value()).unwrap(),
                same,
                "single-value round-trip failed for {h:?}"
            );

            // Two-value form.
            for v in values {
                let sep = LayoutBorderSpacing::new_separate(h, v);
                assert_eq!(
                    parse_border_spacing(&sep.print_as_css_value()).unwrap(),
                    sep,
                    "two-value round-trip failed for {h:?} / {v:?}"
                );
            }
        }
    }

    #[test]
    fn border_spacing_prints_one_value_only_when_both_axes_match() {
        assert_eq!(
            LayoutBorderSpacing::new(PixelValue::const_px(5)).print_as_css_value(),
            "5px"
        );
        assert_eq!(
            LayoutBorderSpacing::new_separate(
                PixelValue::const_px(5),
                PixelValue::const_px(10)
            )
            .print_as_css_value(),
            "5px 10px"
        );
        // Same number, different unit -> still two values (the metric is part of Eq).
        assert_eq!(
            LayoutBorderSpacing::new_separate(
                PixelValue::const_px(0),
                PixelValue::percent(0.0)
            )
            .print_as_css_value(),
            "0px 0%"
        );
        assert_eq!(
            LayoutBorderSpacing::default().print_as_css_value(),
            "0px",
            "the default must not print as a two-value form"
        );
    }

    #[test]
    fn border_spacing_saturated_value_roundtrips_stably() {
        // The saturated bound prints as a lossy f32 (9223372000000000px), but
        // re-parsing it saturates right back to the same stored bound, so the
        // round-trip is still a fixed point.
        let saturated = parse_border_spacing("infpx").unwrap();
        let printed = saturated.print_as_css_value();
        assert_eq!(parse_border_spacing(&printed).unwrap(), saturated);

        let low = parse_border_spacing("-infpx").unwrap();
        assert_eq!(
            parse_border_spacing(&low.print_as_css_value()).unwrap(),
            low
        );
    }

    #[test]
    fn border_spacing_parse_print_is_idempotent_after_quantization() {
        // Inputs whose precision the fixed-point representation cannot hold must
        // reach a fixed point after a single round-trip, not drift on every pass.
        for input in ["0.0005px", "0.9999px", "0.1px", "1e40px", "NaNpx", "-0"] {
            let once = parse_border_spacing(input).unwrap();
            let printed = once.print_as_css_value();
            let twice = parse_border_spacing(&printed).unwrap();
            assert_eq!(once, twice, "not idempotent for {input:?}");
            assert_eq!(printed, twice.print_as_css_value(), "not stable for {input:?}");
        }
    }

    // ------------------------------------------------------------------------
    // FormatAsRustCode
    // ------------------------------------------------------------------------

    #[test]
    fn format_as_rust_code_emits_the_variant_paths() {
        assert_eq!(
            LayoutTableLayout::Fixed.format_as_rust_code(0),
            "LayoutTableLayout::Fixed"
        );
        assert_eq!(
            StyleBorderCollapse::Collapse.format_as_rust_code(0),
            "StyleBorderCollapse::Collapse"
        );
        assert_eq!(
            StyleCaptionSide::Bottom.format_as_rust_code(0),
            "StyleCaptionSide::Bottom"
        );
        assert_eq!(StyleEmptyCells::Hide.format_as_rust_code(0), "StyleEmptyCells::Hide");

        // The indent argument is ignored -- it must not change the output.
        assert_eq!(
            LayoutTableLayout::Auto.format_as_rust_code(0),
            LayoutTableLayout::Auto.format_as_rust_code(usize::MAX)
        );
    }

    #[test]
    fn format_as_rust_code_for_border_spacing_is_total() {
        assert_eq!(
            LayoutBorderSpacing::new(PixelValue::const_px(5)).format_as_rust_code(0),
            "LayoutBorderSpacing { horizontal: \
             PixelValue::const_from_metric_fractional(SizeMetric::Px, 5, 0), vertical: \
             PixelValue::const_from_metric_fractional(SizeMetric::Px, 5, 0) }"
        );

        // Extreme / degenerate values must still format without panicking.
        for v in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN, -2.5] {
            let code = LayoutBorderSpacing::new(PixelValue::px(v)).format_as_rust_code(0);
            assert!(code.starts_with("LayoutBorderSpacing {"), "{v}: {code}");
            assert!(code.ends_with('}'), "{v}: {code}");
        }
    }
}
