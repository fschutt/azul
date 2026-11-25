//! CSS properties for the width, the axes and the OpenType features of a font (CSS Fonts 4).
//!
//! `font-stretch`, `font-variation-settings`, `font-optical-sizing`, `font-kerning`,
//! `font-variant-ligatures`, `font-variant-east-asian` and the `font-synthesis-*` longhands.
//!
//! Every keyword is ASCII case-insensitive and a value is trimmed before it is read; a parse
//! error carries the trimmed input.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

#[cfg(feature = "codegen")]
use crate::codegen::format::{format_percentage_value, FormatAsRustCode};
use crate::{
    corety::AzString,
    props::{
        basic::{
            error::InvalidValueErr,
            length::{FloatValue, PercentageValue},
        },
        formatter::PrintAsCssValue,
    },
};

// --- Plain decimal numbers ---

/// A number as the font properties spell it: `[+-]?(digits+(.digits+)?|.digits+)`.
///
/// No exponent, no `inf` or `NaN`, no trailing dot. The value is finite (a digit string too long
/// for an `f32` is not a number here) and `-0` is `0`, so a number has one canonical spelling.
fn parse_decimal(text: &str) -> Option<f32> {
    let bytes = text.as_bytes();
    let unsigned = &bytes[usize::from(matches!(bytes.first(), Some(b'+' | b'-')))..];
    let (mut whole, mut fraction, mut dot) = (0_usize, 0_usize, false);
    for &byte in unsigned {
        match byte {
            b'0'..=b'9' if dot => fraction += 1,
            b'0'..=b'9' => whole += 1,
            b'.' if !dot => dot = true,
            _ => return None,
        }
    }
    // `5.` and `.` are no numbers: a dot needs a digit after it, a number without one a digit
    let well_formed = if dot { fraction > 0 } else { whole > 0 };
    if !well_formed {
        return None;
    }
    let value = text.parse::<f32>().ok()?;
    let value = if value == 0.0 { 0.0 } else { value };
    value.is_finite().then_some(value)
}

/// The value of the `table` entry whose keyword is `input` (trimmed, ASCII case-insensitive).
#[cfg(feature = "parser")]
fn parse_keyword<'a, T: Copy>(
    input: &'a str,
    table: &[(T, &str)],
) -> Result<T, InvalidValueErr<'a>> {
    let word = input.trim();
    table
        .iter()
        .find(|(_, keyword)| word.eq_ignore_ascii_case(keyword))
        .map(|(value, _)| *value)
        .ok_or(InvalidValueErr(word))
}

// --- Font Stretch ---

/// The `font-stretch` property (CSS Fonts 4, also `font-width`): how wide the face is, as a
/// percentage of the normal width.
///
/// `50%` is ultra condensed, `100%` normal, `200%` ultra expanded. The keywords (`condensed`,
/// `semi-expanded`, ...) are the percentages they stand for, and a value always prints as its
/// percentage. A variable font draws any percentage on its `wdth` axis; a family of separate
/// faces picks the face nearest to it. Inherited. The initial value is `100%`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFontStretch {
    /// The width in percent of the normal width (`87.5%` for `semi-condensed`).
    pub inner: PercentageValue,
}

impl Default for StyleFontStretch {
    fn default() -> Self {
        Self::NORMAL
    }
}

impl StyleFontStretch {
    /// `normal`: `100%`, the initial value.
    pub const NORMAL: Self = Self {
        inner: PercentageValue::const_new(100),
    };

    /// The width of `percent` percent of the normal width (`87.5` is `87.5%`).
    #[must_use]
    pub fn from_percent(percent: f32) -> Self {
        Self {
            inner: PercentageValue::new(percent),
        }
    }

    /// The width as a number of percent: `87.5` for `87.5%`.
    #[must_use]
    pub fn percent(&self) -> f32 {
        FloatValue::new(self.inner.normalized() * 100.0).get()
    }
}

impl PrintAsCssValue for StyleFontStretch {
    fn print_as_css_value(&self) -> String {
        format!("{}%", self.percent())
    }
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontStretch {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleFontStretch {{ inner: {} }}",
            format_percentage_value(&self.inner)
        )
    }
}

/// The keywords of `font-stretch` with the percentage each stands for.
#[cfg(feature = "parser")]
static STRETCH_KEYWORDS: [(&str, f32); 9] = [
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

#[cfg(feature = "parser")]
/// Parses a `font-stretch` value: a keyword or a percentage of at least `0%`.
///
/// The keywords are `ultra-condensed` 50%, `extra-condensed` 62.5%, `condensed` 75%,
/// `semi-condensed` 87.5%, `normal` 100%, `semi-expanded` 112.5%, `expanded` 125%,
/// `extra-expanded` 150% and `ultra-expanded` 200%, ASCII case-insensitive.
///
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `font-stretch` value: a negative percentage,
/// a bare number, a percentage that is not a plain decimal (`1e2%`, `inf%`), an unknown word,
/// or nothing at all.
pub fn parse_style_font_stretch(input: &str) -> Result<StyleFontStretch, InvalidValueErr<'_>> {
    let trimmed = input.trim();
    if let Some((_, percent)) = STRETCH_KEYWORDS
        .iter()
        .find(|(keyword, _)| trimmed.eq_ignore_ascii_case(keyword))
    {
        return Ok(StyleFontStretch::from_percent(*percent));
    }
    trimmed
        .strip_suffix('%')
        .and_then(parse_decimal)
        .filter(|percent| *percent >= 0.0)
        .map(StyleFontStretch::from_percent)
        .ok_or(InvalidValueErr(trimmed))
}

// --- Font Variation Settings ---

/// The `font-variation-settings` property (CSS Fonts 4): the value of each design axis of a
/// variable font, by the axis' four-character tag.
///
/// `"wght" 650`, `"wdth" 87.5`, `"opsz" 14`. Inherited. Held as its canonical CSS text (`normal`,
/// or each axis as its tag in double quotes and its number, separated by `, `): one string keeps
/// the value a single C struct. [`Self::variations`] reads the axes back; a tag given twice keeps
/// only its last value, where it last stands. An axis a font does not have is ignored.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleFontVariationSettings {
    /// The canonical CSS text of the value (`normal` for no axis set).
    pub inner: AzString,
}

impl Default for StyleFontVariationSettings {
    fn default() -> Self {
        Self {
            inner: AzString::from_const_str("normal"),
        }
    }
}

impl StyleFontVariationSettings {
    /// The value for `variations`, each an axis tag and its number; `normal` for none. A tag
    /// listed twice keeps its last value, at the place it last stands. An entry the grammar
    /// cannot hold - a tag that is not four printable ASCII characters (or holds both kinds of
    /// quote), a number that is not finite - is left out, so the text always reads back as what
    /// it says.
    #[must_use]
    pub fn from_variations(variations: &[([u8; 4], f32)]) -> Self {
        let mut kept: Vec<([u8; 4], f32)> = Vec::new();
        for (tag, value) in variations {
            if !is_variation_tag(*tag) || !value.is_finite() {
                continue;
            }
            kept.retain(|(other, _)| other != tag);
            kept.push((*tag, if *value == 0.0 { 0.0 } else { *value }));
        }
        if kept.is_empty() {
            return Self::default();
        }
        let text = kept
            .iter()
            .map(|(tag, value)| variation_text(*tag, *value))
            .collect::<Vec<_>>()
            .join(", ");
        Self { inner: text.into() }
    }

    /// Whether this is `normal` (no axis set).
    #[must_use]
    pub fn is_normal(&self) -> bool {
        self.inner.as_str().trim().eq_ignore_ascii_case("normal")
    }

    /// The axes set, in order, each a tag and its number. Empty for `normal`.
    #[must_use]
    pub fn variations(&self) -> Vec<([u8; 4], f32)> {
        if self.is_normal() {
            return Vec::new();
        }
        parse_variation_list(self.inner.as_str()).unwrap_or_default()
    }
}

/// Whether `tag` is an axis tag the grammar can write: four printable ASCII characters (0x20 to
/// 0x7E), not holding both a `"` and a `'` (a tag sits in one kind of quote).
fn is_variation_tag(tag: [u8; 4]) -> bool {
    tag.iter().all(|b| (0x20..=0x7E).contains(b))
        && !(tag.contains(&b'"') && tag.contains(&b'\''))
}

/// One axis as the canonical text has it: the tag in double quotes (in single quotes when the
/// tag itself holds a `"`), a space, the number.
fn variation_text(tag: [u8; 4], value: f32) -> String {
    let name: String = tag.iter().copied().map(char::from).collect();
    let quote = if tag.contains(&b'"') { '\'' } else { '"' };
    format!("{quote}{name}{quote} {value}")
}

/// The `(tag, value)` list of a `font-variation-settings` value that is not `normal`.
///
/// Comma separated, each a quoted four-character printable-ASCII tag (double or single quotes),
/// then a plain decimal number - which is required. A tag given twice keeps its last value,
/// where it last stands. `None` for anything the grammar does not allow.
fn parse_variation_list(input: &str) -> Option<Vec<([u8; 4], f32)>> {
    let mut out: Vec<([u8; 4], f32)> = Vec::new();
    let mut rest = input.trim_start();
    loop {
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let after_quote = &rest[1..];
        let end = after_quote.find(quote)?;
        let name = &after_quote.as_bytes()[..end];
        let tag: [u8; 4] = name.try_into().ok()?;
        if !is_variation_tag(tag) {
            return None;
        }
        let after_tag = &after_quote[end + 1..];
        let (number, next) = after_tag
            .split_once(',')
            .map_or((after_tag, None), |(number, next)| (number, Some(next)));
        let value = parse_decimal(number.trim())?;
        out.retain(|(other, _)| *other != tag);
        out.push((tag, value));
        let Some(next) = next else {
            return Some(out);
        };
        rest = next.trim_start();
    }
}

impl PrintAsCssValue for StyleFontVariationSettings {
    fn print_as_css_value(&self) -> String {
        self.inner.as_str().to_string()
    }
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontVariationSettings {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleFontVariationSettings {{ inner: AzString::from_const_str({:?}) }}",
            self.inner.as_str()
        )
    }
}

#[cfg(feature = "parser")]
/// Parses a `font-variation-settings` value: `normal`, or a comma-separated list of quoted tags,
/// each followed by a number.
///
/// A tag is four printable ASCII characters in double or single quotes; the number is required
/// and plain decimal (`"wght" 650`, `'wdth' -12.5`, `"opsz" .5`, `"slnt" +3`): no exponent, no
/// `inf`, no `NaN`, no `on` / `off`. A tag given twice keeps its last value.
///
/// # Errors
///
/// Returns an error for an unquoted, short, long or non-ASCII tag, a missing or non-numeric
/// value (`"wght"`, `"wght" on`, `"wght" 1e3`, `"wght" 5.`), an empty item (`, "wght" 400`, a
/// trailing comma), `normal` with anything else, or nothing at all.
pub fn parse_style_font_variation_settings(
    input: &str,
) -> Result<StyleFontVariationSettings, InvalidValueErr<'_>> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("normal") {
        return Ok(StyleFontVariationSettings::default());
    }
    parse_variation_list(trimmed)
        .map(|variations| StyleFontVariationSettings::from_variations(&variations))
        .ok_or(InvalidValueErr(trimmed))
}

// --- Font Optical Sizing ---

/// The `font-optical-sizing` property (CSS Fonts 4): whether a variable font's `opsz` axis
/// follows the font size.
///
/// `auto`, the initial value, sets the axis from the font size; `none` leaves it alone.
/// Inherited.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum StyleFontOpticalSizing {
    #[default]
    Auto,
    None,
}

impl StyleFontOpticalSizing {
    /// Every value with its CSS keyword, in the grammar's order.
    pub const ALL: [(Self, &'static str); 2] = [(Self::Auto, "auto"), (Self::None, "none")];

    /// The CSS keyword of this value.
    #[must_use]
    pub const fn keyword(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
        }
    }
}

impl PrintAsCssValue for StyleFontOpticalSizing {
    fn print_as_css_value(&self) -> String {
        self.keyword().to_string()
    }
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontOpticalSizing {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StyleFontOpticalSizing::{self:?}")
    }
}

#[cfg(feature = "parser")]
/// Parses a `font-optical-sizing` value: `auto` or `none`, ASCII case-insensitive.
///
/// # Errors
///
/// Returns an error if `input` is not exactly one of the two keywords.
pub fn parse_style_font_optical_sizing(
    input: &str,
) -> Result<StyleFontOpticalSizing, InvalidValueErr<'_>> {
    parse_keyword(input, &StyleFontOpticalSizing::ALL)
}

// --- Font Kerning ---

/// The `font-kerning` property (CSS Fonts 4): whether the pair adjustments of the font are
/// applied.
///
/// `auto`, the initial value, leaves the choice to the shaper (kerning on), `normal` asks for
/// it, `none` turns it off. Inherited.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum StyleFontKerning {
    #[default]
    Auto,
    Normal,
    None,
}

impl StyleFontKerning {
    /// Every value with its CSS keyword, in the grammar's order.
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Auto, "auto"),
        (Self::Normal, "normal"),
        (Self::None, "none"),
    ];

    /// The CSS keyword of this value.
    #[must_use]
    pub const fn keyword(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Normal => "normal",
            Self::None => "none",
        }
    }
}

impl PrintAsCssValue for StyleFontKerning {
    fn print_as_css_value(&self) -> String {
        self.keyword().to_string()
    }
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontKerning {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StyleFontKerning::{self:?}")
    }
}

#[cfg(feature = "parser")]
/// Parses a `font-kerning` value: `auto`, `normal` or `none`, ASCII case-insensitive.
///
/// # Errors
///
/// Returns an error if `input` is not exactly one of the three keywords.
pub fn parse_style_font_kerning(input: &str) -> Result<StyleFontKerning, InvalidValueErr<'_>> {
    parse_keyword(input, &StyleFontKerning::ALL)
}

// --- Font Variant Ligatures ---

/// What one group of `font-variant-ligatures` asks for: nothing (`Unset`, the group is left to
/// the font and the shaper), the group's ligatures on, or off.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum StyleFontLigatureChoice {
    #[default]
    Unset,
    On,
    Off,
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontLigatureChoice {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StyleFontLigatureChoice::{self:?}")
    }
}

/// The `font-variant-ligatures` property (CSS Fonts 4): which ligatures the text asks the font
/// for, as OpenType features.
///
/// One choice per group, in any combination of groups:
///
/// - common: `common-ligatures` / `no-common-ligatures` (`liga`, `clig`);
/// - discretionary: `discretionary-ligatures` / `no-discretionary-ligatures` (`dlig`);
/// - historical: `historical-ligatures` / `no-historical-ligatures` (`hlig`);
/// - contextual: `contextual` / `no-contextual` (`calt`).
///
/// `normal`, the initial value (every group `Unset`), leaves them to the font; `none` turns
/// every group off. Inherited.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct StyleFontVariantLigatures {
    /// `common-ligatures` / `no-common-ligatures` (`liga`, `clig`).
    pub common: StyleFontLigatureChoice,
    /// `discretionary-ligatures` / `no-discretionary-ligatures` (`dlig`).
    pub discretionary: StyleFontLigatureChoice,
    /// `historical-ligatures` / `no-historical-ligatures` (`hlig`).
    pub historical: StyleFontLigatureChoice,
    /// `contextual` / `no-contextual` (`calt`).
    pub contextual: StyleFontLigatureChoice,
}

/// The four groups of `font-variant-ligatures` in the grammar's order, each with the keyword
/// that turns it on and the one that turns it off.
static LIGATURE_KEYWORDS: [(&str, &str); 4] = [
    ("common-ligatures", "no-common-ligatures"),
    ("discretionary-ligatures", "no-discretionary-ligatures"),
    ("historical-ligatures", "no-historical-ligatures"),
    ("contextual", "no-contextual"),
];

impl StyleFontVariantLigatures {
    /// `normal`: every group `Unset`, the initial value.
    pub const NORMAL: Self = Self {
        common: StyleFontLigatureChoice::Unset,
        discretionary: StyleFontLigatureChoice::Unset,
        historical: StyleFontLigatureChoice::Unset,
        contextual: StyleFontLigatureChoice::Unset,
    };

    /// `none`: every group off.
    pub const NONE: Self = Self {
        common: StyleFontLigatureChoice::Off,
        discretionary: StyleFontLigatureChoice::Off,
        historical: StyleFontLigatureChoice::Off,
        contextual: StyleFontLigatureChoice::Off,
    };

    /// Whether this is `normal` (every group `Unset`).
    #[must_use]
    pub const fn is_normal(&self) -> bool {
        matches!(self.common, StyleFontLigatureChoice::Unset)
            && matches!(self.discretionary, StyleFontLigatureChoice::Unset)
            && matches!(self.historical, StyleFontLigatureChoice::Unset)
            && matches!(self.contextual, StyleFontLigatureChoice::Unset)
    }

    /// Whether this is `none` (every group off).
    #[must_use]
    pub const fn is_none(&self) -> bool {
        matches!(self.common, StyleFontLigatureChoice::Off)
            && matches!(self.discretionary, StyleFontLigatureChoice::Off)
            && matches!(self.historical, StyleFontLigatureChoice::Off)
            && matches!(self.contextual, StyleFontLigatureChoice::Off)
    }

    /// The OpenType feature tags the shaper turns on for this value, in the grammar's order:
    /// common (`liga`, `clig`), discretionary (`dlig`), historical (`hlig`), contextual (`calt`).
    #[must_use]
    pub fn opentype_on(&self) -> Vec<[u8; 4]> {
        self.opentype_tags(StyleFontLigatureChoice::On)
    }

    /// The OpenType feature tags the shaper turns off for this value, in the grammar's order (as
    /// [`Self::opentype_on`]).
    #[must_use]
    pub fn opentype_off(&self) -> Vec<[u8; 4]> {
        self.opentype_tags(StyleFontLigatureChoice::Off)
    }

    /// The tags of the groups that hold `choice`.
    fn opentype_tags(&self, choice: StyleFontLigatureChoice) -> Vec<[u8; 4]> {
        let mut tags = Vec::new();
        if self.common == choice {
            tags.push(*b"liga");
            tags.push(*b"clig");
        }
        if self.discretionary == choice {
            tags.push(*b"dlig");
        }
        if self.historical == choice {
            tags.push(*b"hlig");
        }
        if self.contextual == choice {
            tags.push(*b"calt");
        }
        tags
    }

    /// The four choices in the grammar's order.
    const fn choices(&self) -> [StyleFontLigatureChoice; 4] {
        [
            self.common,
            self.discretionary,
            self.historical,
            self.contextual,
        ]
    }

    /// The value of four choices in the grammar's order.
    #[cfg(feature = "parser")]
    const fn from_choices(choices: [StyleFontLigatureChoice; 4]) -> Self {
        Self {
            common: choices[0],
            discretionary: choices[1],
            historical: choices[2],
            contextual: choices[3],
        }
    }
}

impl PrintAsCssValue for StyleFontVariantLigatures {
    fn print_as_css_value(&self) -> String {
        if self.is_normal() {
            return "normal".to_string();
        }
        if self.is_none() {
            return "none".to_string();
        }
        let mut words: Vec<&str> = Vec::new();
        for (group, choice) in self.choices().iter().enumerate() {
            let (on, off) = LIGATURE_KEYWORDS[group];
            match choice {
                StyleFontLigatureChoice::Unset => {}
                StyleFontLigatureChoice::On => words.push(on),
                StyleFontLigatureChoice::Off => words.push(off),
            }
        }
        words.join(" ")
    }
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontVariantLigatures {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "StyleFontVariantLigatures {{ common: {}, discretionary: {}, historical: {}, \
             contextual: {} }}",
            self.common.format_as_rust_code(tabs),
            self.discretionary.format_as_rust_code(tabs),
            self.historical.format_as_rust_code(tabs),
            self.contextual.format_as_rust_code(tabs),
        )
    }
}

/// The group (0 to 3, the grammar's order) and the choice a `font-variant-ligatures` keyword
/// stands for, ASCII case-insensitive.
#[cfg(feature = "parser")]
fn ligature_keyword(word: &str) -> Option<(usize, StyleFontLigatureChoice)> {
    LIGATURE_KEYWORDS
        .iter()
        .enumerate()
        .find_map(|(group, (on, off))| {
            if word.eq_ignore_ascii_case(on) {
                Some((group, StyleFontLigatureChoice::On))
            } else if word.eq_ignore_ascii_case(off) {
                Some((group, StyleFontLigatureChoice::Off))
            } else {
                None
            }
        })
}

#[cfg(feature = "parser")]
/// Parses a `font-variant-ligatures` value: `normal` or `none` (each only alone), or one or more
/// of the eight keywords in any order, at most one per group.
///
/// Keywords are ASCII case-insensitive.
///
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `font-variant-ligatures` value: an unknown
/// word, two keywords of one group (`common-ligatures no-common-ligatures`), a repeated keyword,
/// `normal` or `none` with anything else, or nothing at all.
pub fn parse_style_font_variant_ligatures(
    input: &str,
) -> Result<StyleFontVariantLigatures, InvalidValueErr<'_>> {
    let input = input.trim();
    if input.eq_ignore_ascii_case("normal") {
        return Ok(StyleFontVariantLigatures::NORMAL);
    }
    if input.eq_ignore_ascii_case("none") {
        return Ok(StyleFontVariantLigatures::NONE);
    }
    let mut choices = [StyleFontLigatureChoice::Unset; 4];
    let mut any = false;
    for word in input.split_ascii_whitespace() {
        let Some((group, choice)) = ligature_keyword(word) else {
            return Err(InvalidValueErr(input));
        };
        if choices[group] != StyleFontLigatureChoice::Unset {
            return Err(InvalidValueErr(input));
        }
        choices[group] = choice;
        any = true;
    }
    if any {
        Ok(StyleFontVariantLigatures::from_choices(choices))
    } else {
        Err(InvalidValueErr(input))
    }
}

// --- Font Variant East Asian ---

/// The `font-variant-east-asian` property (CSS Fonts 4): which East Asian glyph forms the text
/// asks the font for, as OpenType features.
///
/// One choice of the variant group, one of the width group, and `ruby`:
///
/// - variant: `jis78` (`jp78`), `jis83` (`jp83`), `jis90` (`jp90`), `jis04` (`jp04`),
///   `simplified` (`smpl`) or `traditional` (`trad`);
/// - width: `full-width` (`fwid`) or `proportional-width` (`pwid`);
/// - `ruby` (`ruby`): the glyphs made for ruby annotation.
///
/// `normal`, the initial value (every field `false`), asks for none of them. Inherited. A font
/// without a feature shapes the text as if it had not been asked.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
#[allow(clippy::struct_excessive_bools)] // one flag per CSS keyword, as the grammar has them
pub struct StyleFontVariantEastAsian {
    pub jis78: bool,
    pub jis83: bool,
    pub jis90: bool,
    pub jis04: bool,
    pub simplified: bool,
    pub traditional: bool,
    pub full_width: bool,
    pub proportional_width: bool,
    pub ruby: bool,
}

impl StyleFontVariantEastAsian {
    /// `normal`: no East Asian feature asked for.
    pub const NORMAL: Self = Self {
        jis78: false,
        jis83: false,
        jis90: false,
        jis04: false,
        simplified: false,
        traditional: false,
        full_width: false,
        proportional_width: false,
        ruby: false,
    };

    /// Every keyword with its OpenType feature tag and whether `self` asks for it, in the
    /// grammar's order (the order a value is printed in).
    #[must_use]
    pub const fn keywords(&self) -> [(&'static str, [u8; 4], bool); 9] {
        [
            ("jis78", *b"jp78", self.jis78),
            ("jis83", *b"jp83", self.jis83),
            ("jis90", *b"jp90", self.jis90),
            ("jis04", *b"jp04", self.jis04),
            ("simplified", *b"smpl", self.simplified),
            ("traditional", *b"trad", self.traditional),
            ("full-width", *b"fwid", self.full_width),
            ("proportional-width", *b"pwid", self.proportional_width),
            ("ruby", *b"ruby", self.ruby),
        ]
    }

    /// Whether this is `normal` (no feature asked for).
    #[must_use]
    pub const fn is_normal(&self) -> bool {
        !(self.jis78
            || self.jis83
            || self.jis90
            || self.jis04
            || self.simplified
            || self.traditional
            || self.full_width
            || self.proportional_width
            || self.ruby)
    }

    /// The OpenType feature tags the shaper turns on for this value, in the grammar's order
    /// (empty for `normal`).
    #[must_use]
    pub fn opentype_features(&self) -> Vec<[u8; 4]> {
        self.keywords()
            .iter()
            .filter(|(_, _, on)| *on)
            .map(|(_, tag, _)| *tag)
            .collect()
    }

    /// The value whose keywords, in the grammar's order, are set in `flags`.
    #[cfg(feature = "parser")]
    const fn from_flags(flags: [bool; 9]) -> Self {
        Self {
            jis78: flags[0],
            jis83: flags[1],
            jis90: flags[2],
            jis04: flags[3],
            simplified: flags[4],
            traditional: flags[5],
            full_width: flags[6],
            proportional_width: flags[7],
            ruby: flags[8],
        }
    }
}

impl PrintAsCssValue for StyleFontVariantEastAsian {
    fn print_as_css_value(&self) -> String {
        if self.is_normal() {
            return "normal".to_string();
        }
        self.keywords()
            .iter()
            .filter(|(_, _, on)| *on)
            .map(|(keyword, _, _)| *keyword)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontVariantEastAsian {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("{self:?}")
    }
}

#[cfg(feature = "parser")]
/// Parses a `font-variant-east-asian` value: `normal` (only alone), or one or more of the nine
/// keywords in any order.
///
/// At most one of the variant group (`jis78` `jis83` `jis90` `jis04` `simplified`
/// `traditional`), at most one of the width group (`full-width` `proportional-width`), and
/// `ruby` at most once. Keywords are ASCII case-insensitive.
///
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `font-variant-east-asian` value: an unknown
/// word, two values of one group, a repeated keyword, `normal` with anything else, or nothing at
/// all.
pub fn parse_style_font_variant_east_asian(
    input: &str,
) -> Result<StyleFontVariantEastAsian, InvalidValueErr<'_>> {
    let input = input.trim();
    if input.eq_ignore_ascii_case("normal") {
        return Ok(StyleFontVariantEastAsian::NORMAL);
    }
    let keywords = StyleFontVariantEastAsian::NORMAL.keywords();
    let mut flags = [false; 9];
    let mut any = false;
    for word in input.split_ascii_whitespace() {
        let Some(index) = keywords
            .iter()
            .position(|(keyword, _, _)| word.eq_ignore_ascii_case(keyword))
        else {
            return Err(InvalidValueErr(input));
        };
        // The keywords 0 to 5 are the variant group, 6 and 7 the width group, 8 is `ruby`: a
        // group takes one keyword, so a second one (or the same one again) is an error.
        let group: core::ops::Range<usize> = match index {
            0..=5 => 0..6,
            6..=7 => 6..8,
            _ => 8..9,
        };
        if flags[group].iter().any(|taken| *taken) {
            return Err(InvalidValueErr(input));
        }
        flags[index] = true;
        any = true;
    }
    if any {
        Ok(StyleFontVariantEastAsian::from_flags(flags))
    } else {
        Err(InvalidValueErr(input))
    }
}

// --- Font Synthesis ---

/// The value of the four `font-synthesis-*` longhands (`weight`, `style`, `small-caps` and
/// `position`): whether the engine may fake a face the font lacks.
///
/// `auto`, the initial value, lets it fake the bold, the italic, the small capitals or the
/// super- and subscripts; `none` does not. Inherited.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum StyleFontSynthesis {
    #[default]
    Auto,
    None,
}

impl StyleFontSynthesis {
    /// Every value with its CSS keyword, in the grammar's order.
    pub const ALL: [(Self, &'static str); 2] = [(Self::Auto, "auto"), (Self::None, "none")];

    /// The CSS keyword of this value.
    #[must_use]
    pub const fn keyword(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
        }
    }
}

impl PrintAsCssValue for StyleFontSynthesis {
    fn print_as_css_value(&self) -> String {
        self.keyword().to_string()
    }
}

#[cfg(feature = "codegen")]
impl FormatAsRustCode for StyleFontSynthesis {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StyleFontSynthesis::{self:?}")
    }
}

#[cfg(feature = "parser")]
/// Parses the value of a `font-synthesis-weight`, `-style`, `-small-caps` or `-position`
/// longhand: `auto` or `none`, ASCII case-insensitive.
///
/// # Errors
///
/// Returns an error if `input` is not exactly one of the two keywords.
pub fn parse_style_font_synthesis(input: &str) -> Result<StyleFontSynthesis, InvalidValueErr<'_>> {
    parse_keyword(input, &StyleFontSynthesis::ALL)
}

#[cfg(test)]
#[path = "font_variation_test.rs"]
mod font_variation_test;
