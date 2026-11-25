// The module docs start with an empty line: it ends the paragraph of the `///` summary the
// declaration in `mod.rs` carries (clippy::too_long_first_doc_paragraph).
//!
//! The shorthands of the font properties (CSS Fonts 4): `font`, `font-variant` and
//! `font-synthesis`.
//!
//! Each parser takes the value of a declaration (the CSS-wide keywords are the caller's business)
//! and returns the longhands the shorthand sets, always in the same order, each as a property of
//! its own:
//!
//! - `font` sets the style, the caps variant, the weight, the stretch, the size, the line height
//!   and the family, and resets every other font longhand to `initial`. A system font keyword
//!   (`caption`, `menu`, `system:ui`) sets the family-only `CssProperty::Font` instead.
//! - `font-variant` sets the numeric, caps, ligature and East Asian longhands.
//! - `font-synthesis` sets the four `font-synthesis-*` longhands.

#[cfg(feature = "parser")]
use alloc::vec::Vec;

#[cfg(feature = "parser")]
use crate::{
    css::CssPropertyValue,
    props::{
        basic::{
            error::InvalidValueErr,
            font::{
                parse_font_style, parse_font_weight, parse_style_font_family,
                parse_style_font_size, parse_style_font_variant_caps,
                parse_style_font_variant_numeric, StyleFontFamilyVec, StyleFontSize,
                StyleFontStyle, StyleFontVariantCaps, StyleFontVariantNumeric, StyleFontWeight,
            },
            font_variation::{
                parse_style_font_stretch, parse_style_font_variant_east_asian,
                parse_style_font_variant_ligatures, StyleFontStretch, StyleFontSynthesis,
                StyleFontVariantEastAsian, StyleFontVariantLigatures,
            },
            parse::split_top_level,
        },
        property::CssProperty,
        style::text::{parse_style_line_height, StyleLineHeight},
    },
};

/// The system font keywords of `font` (CSS 2.1 section 15.8). The value is then a family, not a
/// list of longhands.
#[cfg(feature = "parser")]
static SYSTEM_FONT_KEYWORDS: [&str; 6] = [
    "caption",
    "icon",
    "menu",
    "message-box",
    "small-caption",
    "status-bar",
];

/// The keywords of `font-synthesis`, in the order of its longhands: weight, style, small caps,
/// position.
#[cfg(feature = "parser")]
static FONT_SYNTHESIS_KEYWORDS: [&str; 4] = ["weight", "style", "small-caps", "position"];

/// The keywords of the `font-variant-*` longhands azul has not, each with the group of the
/// `font-variant` grammar it stands in: a group may stand once in a value (`sub super` is no
/// value). They are accepted and set nothing.
#[cfg(feature = "parser")]
static IGNORED_VARIANT_KEYWORDS: [(&str, &str); 6] = [
    ("sub", "position"),
    ("super", "position"),
    ("historical-forms", "historical-forms"),
    ("text", "emoji"),
    ("emoji", "emoji"),
    ("unicode", "emoji"),
];

/// The functions of `font-variant-alternates`, which azul has not: each may stand once in a value,
/// is accepted and sets nothing.
#[cfg(feature = "parser")]
static IGNORED_VARIANT_FUNCTIONS: [&str; 6] = [
    "stylistic",
    "styleset",
    "character-variant",
    "swash",
    "ornaments",
    "annotation",
];

// --- Shared helpers ---

/// Splits the first token (a run of characters up to the next ASCII white space) off `text`: the
/// token and what follows it. The token is empty when `text` is only white space.
#[cfg(feature = "parser")]
fn split_token(text: &str) -> (&str, &str) {
    let text = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let end = text
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(text.len());
    text.split_at(end)
}

// --- font ---

/// Whether `input` (trimmed) is a system font: one of the keywords of CSS 2.1, or a family of
/// azul's own `system:ui` kind.
#[cfg(feature = "parser")]
fn is_system_font(input: &str) -> bool {
    SYSTEM_FONT_KEYWORDS
        .iter()
        .any(|keyword| input.eq_ignore_ascii_case(keyword))
        || input
            .get(..7)
            .is_some_and(|start| start.eq_ignore_ascii_case("system:"))
}

/// What a token meant to the prefix of a `font` value.
#[cfg(feature = "parser")]
enum PrefixStep {
    /// The token is no part of the prefix: the font size starts here.
    End,
    /// The prefix took the token.
    Token,
    /// The prefix took the token and the one after it (`oblique` and its angle).
    TokenAndNext,
    /// The token is one of a property the prefix has already: the value is invalid.
    Duplicate,
}

/// What stands before the font size of a `font` value: at most one of each of `font-style`,
/// `font-variant` (`normal` or `small-caps` only), `font-weight` and `font-stretch` (the keywords
/// only, no percentage), in any order, and `normal`, which can be any of the four.
#[cfg(feature = "parser")]
#[derive(Default)]
struct FontPrefix {
    style: Option<StyleFontStyle>,
    caps: Option<StyleFontVariantCaps>,
    weight: Option<StyleFontWeight>,
    stretch: Option<StyleFontStretch>,
    /// How many `normal` tokens stand. Each is one of the four properties that no other token
    /// names: see [`Self::settle_normals`].
    normals: usize,
}

/// Puts `value` into `slot`, unless the slot has a value already.
#[cfg(feature = "parser")]
fn set_once<T>(slot: &mut Option<T>, value: T) -> PrefixStep {
    if slot.is_some() {
        PrefixStep::Duplicate
    } else {
        *slot = Some(value);
        PrefixStep::Token
    }
}

#[cfg(feature = "parser")]
impl FontPrefix {
    /// Takes `token` into the prefix when it is a value of one of its properties; `following` is
    /// the text after the token (the angle of `oblique 10deg` is a token of its own).
    ///
    /// The keywords are ASCII case-insensitive: the parsers of the longhands are not asked with
    /// the case the author wrote.
    fn take(&mut self, token: &str, following: &str) -> PrefixStep {
        let word = token.to_ascii_lowercase();
        if word == "normal" {
            self.normals += 1;
            return PrefixStep::Token;
        }
        if word == "italic" || word == "oblique" {
            return self.take_style(&word, following);
        }
        if word == "small-caps" {
            return set_once(&mut self.caps, StyleFontVariantCaps::SmallCaps);
        }
        // `font-stretch` takes keywords here: a percentage is a font size.
        if !word.ends_with('%') {
            if let Ok(stretch) = parse_style_font_stretch(&word) {
                return set_once(&mut self.stretch, stretch);
            }
        }
        parse_font_weight(&word).map_or(PrefixStep::End, |weight| {
            set_once(&mut self.weight, weight)
        })
    }

    /// Takes `italic`, `oblique` or `oblique <angle>`.
    fn take_style(&mut self, word: &str, following: &str) -> PrefixStep {
        let angle = split_token(following).0;
        if word == "oblique" && !angle.is_empty() {
            let with_angle = format!("oblique {}", angle.to_ascii_lowercase());
            if let Ok(style) = parse_font_style(&with_angle) {
                return match set_once(&mut self.style, style) {
                    PrefixStep::Token => PrefixStep::TokenAndNext,
                    other => other,
                };
            }
        }
        parse_font_style(word).map_or(PrefixStep::End, |style| set_once(&mut self.style, style))
    }

    /// Hands each `normal` to the first of the four properties no other token names, in the order
    /// style, caps, weight, stretch: all four values are `normal`, so which of them it is makes
    /// no difference to the value, and `normal italic` and `italic normal` are the same.
    ///
    /// False when there are more `normal`s than free properties.
    fn settle_normals(&mut self) -> bool {
        for _ in 0..self.normals {
            if self.style.is_none() {
                self.style = Some(StyleFontStyle::Normal);
            } else if self.caps.is_none() {
                self.caps = Some(StyleFontVariantCaps::Normal);
            } else if self.weight.is_none() {
                self.weight = Some(StyleFontWeight::NORMAL);
            } else if self.stretch.is_none() {
                self.stretch = Some(StyleFontStretch::NORMAL);
            } else {
                return false;
            }
        }
        true
    }
}

/// Splits `input` into the prefix and the rest, which starts at the first token that is no part of
/// the prefix (the font size). `None` when a property is given twice, or when no token is left
/// for the size.
#[cfg(feature = "parser")]
fn split_font_prefix(input: &str) -> Option<(FontPrefix, &str)> {
    let mut prefix = FontPrefix::default();
    let mut rest = input;
    loop {
        let (token, after) = split_token(rest);
        if token.is_empty() {
            return None;
        }
        match prefix.take(token, after) {
            PrefixStep::End => return Some((prefix, rest)),
            PrefixStep::Duplicate => return None,
            PrefixStep::Token => rest = after,
            PrefixStep::TokenAndNext => rest = split_token(after).1,
        }
    }
}

/// The font size, the line height and the family list of a `font` value: what follows the prefix.
#[cfg(feature = "parser")]
struct FontTail {
    size: StyleFontSize,
    line_height: Option<StyleLineHeight>,
    family: StyleFontFamilyVec,
}

/// The font size in `text`, one token. A bare number is no size (it is a weight; the size parser
/// would take it for pixels), and a size is not negative.
#[cfg(feature = "parser")]
fn parse_font_size_token(text: &str) -> Option<StyleFontSize> {
    if text.is_empty() || text.parse::<f32>().is_ok() {
        return None;
    }
    let size = parse_style_font_size(text).ok()?;
    (size.inner.number.get() >= 0.0).then_some(size)
}

/// Whether `text` starts like a number, a dimension or a percentage: a digit, or a sign and/or a
/// dot before one.
#[cfg(feature = "parser")]
fn starts_like_number(text: &str) -> bool {
    let mut chars = text.chars();
    let mut first = chars.next();
    if matches!(first, Some('+' | '-')) {
        first = chars.next();
    }
    if first == Some('.') {
        first = chars.next();
    }
    first.is_some_and(|c| c.is_ascii_digit())
}

/// Whether `item`, one of the comma separated items of a family list, can be a family name: a
/// quoted string, or an unquoted name, which is a sequence of identifiers (CSS Fonts 4 section
/// 3.1): not empty and not starting like a number.
#[cfg(feature = "parser")]
fn is_family_name(item: &str) -> bool {
    let item = item.trim();
    item.starts_with(['"', '\'']) || (!item.is_empty() && !starts_like_number(item))
}

/// The family list `text` of a `font` value: not empty, with a name for every item (so a trailing
/// comma is an error). `87.5% 12px a` is no stretch and a size, it is a size and a family named
/// `12px a`, which is none.
#[cfg(feature = "parser")]
fn parse_font_family_list(text: &str) -> Option<StyleFontFamilyVec> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if !split_top_level(text, |byte| byte == b',')
        .into_iter()
        .all(is_family_name)
    {
        return None;
    }
    parse_style_font_family(text).ok()
}

/// The size, the optional `/ <line-height>` and the family list of the text `rest`, which starts
/// at the font size. White space around the `/` is free: `12px/1.5`, `12px / 1.5`, `12px /1.5`.
#[cfg(feature = "parser")]
fn parse_font_tail(rest: &str) -> Option<FontTail> {
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let size_end = rest
        .find(|c: char| c.is_ascii_whitespace() || c == '/')
        .unwrap_or(rest.len());
    let (size_text, after_size) = rest.split_at(size_end);
    let size = parse_font_size_token(size_text)?;
    let after_size = after_size.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (line_height, family_text) = match after_size.strip_prefix('/') {
        Some(after_slash) => {
            let (token, family_text) = split_token(after_slash);
            (Some(parse_style_line_height(token).ok()?), family_text)
        }
        None => (None, after_size),
    };
    Some(FontTail {
        size,
        line_height,
        family: parse_font_family_list(family_text)?,
    })
}

/// `value` as a parsed value, or `initial` when the value left the property out.
#[cfg(feature = "parser")]
fn exact_or_initial<T>(value: Option<T>) -> CssPropertyValue<T> {
    value.map_or(CssPropertyValue::Initial, CssPropertyValue::Exact)
}

/// The longhands `font` sets, in this order: the style, the caps variant, the weight, the stretch,
/// the size, the line height and the family, then the eleven longhands it resets. What the value
/// left out, and every reset, is `initial`.
#[cfg(feature = "parser")]
fn font_properties(prefix: &FontPrefix, tail: FontTail) -> Vec<CssProperty> {
    vec![
        CssProperty::FontStyle(exact_or_initial(prefix.style)),
        CssProperty::FontVariantCaps(exact_or_initial(prefix.caps)),
        CssProperty::FontWeight(exact_or_initial(prefix.weight)),
        CssProperty::FontStretch(exact_or_initial(prefix.stretch)),
        CssProperty::FontSize(CssPropertyValue::Exact(tail.size)),
        CssProperty::LineHeight(exact_or_initial(tail.line_height)),
        CssProperty::FontFamily(CssPropertyValue::Exact(tail.family)),
        CssProperty::FontVariantNumeric(CssPropertyValue::Initial),
        CssProperty::FontVariantLigatures(CssPropertyValue::Initial),
        CssProperty::FontVariantEastAsian(CssPropertyValue::Initial),
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

/// Parses the value of the `font` shorthand: `[ [ <font-style> || <font-variant-css2> ||
/// <font-weight> || <font-stretch-css3> ]? <font-size> [ / <line-height> ]? <font-family># ] |
/// <system-family-name>`.
///
/// Returns the eighteen longhands in a fixed order: `font-style`, `font-variant-caps`,
/// `font-weight`, `font-stretch`, `font-size`, `line-height`, `font-family`, then the resets
/// `font-variant-numeric`, `-ligatures` and `-east-asian`, `font-kerning`, `font-optical-sizing`,
/// `font-variation-settings`, `font-feature-settings` and the four `font-synthesis-*`. What the
/// value names is `Exact`; what it leaves out (a style, a variant, a weight or a stretch not in
/// the prefix, a line height without `/`) and every reset is `initial`.
///
/// The prefix is the tokens before the font size, in any order, each property at most once;
/// `normal` can stand for any property no other token names, up to four times. A bare number is a
/// weight, never a size. The rest after the size and the line height is the family list.
///
/// A system font (`caption`, `icon`, `menu`, `message-box`, `small-caption`, `status-bar`, or a
/// family starting with `system:`) returns the family-only `CssProperty::Font` alone.
///
/// # Errors
///
/// Returns an error, carrying the trimmed value, if `input` is not a `font` value: no font size,
/// no family, a property given twice, a bare number as the size, or anything else the grammar
/// does not have.
#[cfg(feature = "parser")]
#[allow(clippy::module_name_repetitions)] // the function is the parser of the shorthand
pub fn parse_font_shorthand(input: &str) -> Result<Vec<CssProperty>, InvalidValueErr<'_>> {
    let input = input.trim();
    let invalid = InvalidValueErr(input);
    if is_system_font(input) {
        let family = parse_style_font_family(input).map_err(|_| invalid)?;
        return Ok(vec![CssProperty::Font(CssPropertyValue::Exact(family))]);
    }
    let (mut prefix, rest) = split_font_prefix(input).ok_or(invalid)?;
    if !prefix.settle_normals() {
        return Err(invalid);
    }
    let tail = parse_font_tail(rest).ok_or(invalid)?;
    Ok(font_properties(&prefix, tail))
}

// --- font-variant ---

/// The four values of the `font-variant` longhands azul has.
#[cfg(feature = "parser")]
struct VariantValues {
    numeric: StyleFontVariantNumeric,
    caps: StyleFontVariantCaps,
    ligatures: StyleFontVariantLigatures,
    east_asian: StyleFontVariantEastAsian,
}

#[cfg(feature = "parser")]
impl VariantValues {
    /// Every longhand `normal`.
    const fn normal() -> Self {
        Self {
            numeric: StyleFontVariantNumeric::NORMAL,
            caps: StyleFontVariantCaps::Normal,
            ligatures: StyleFontVariantLigatures::NORMAL,
            east_asian: StyleFontVariantEastAsian::NORMAL,
        }
    }

    /// The longhands, in the order numeric, caps, ligatures, East Asian.
    fn properties(&self) -> Vec<CssProperty> {
        vec![
            CssProperty::FontVariantNumeric(CssPropertyValue::Exact(self.numeric)),
            CssProperty::FontVariantCaps(CssPropertyValue::Exact(self.caps)),
            CssProperty::FontVariantLigatures(CssPropertyValue::Exact(self.ligatures)),
            CssProperty::FontVariantEastAsian(CssPropertyValue::Exact(self.east_asian)),
        ]
    }
}

/// The words of `value`, split at the white space outside parentheses, so that the arguments of a
/// function (`styleset(ss01, ss02)`) stay one word.
#[cfg(feature = "parser")]
fn words_outside_parentheses(value: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    for (i, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if c.is_ascii_whitespace() && depth == 0 {
            if let Some(s) = start.take() {
                words.push(&value[s..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        words.push(&value[s..]);
    }
    words
}

/// Whether the parentheses of `text` are balanced: none is closed before it is opened.
#[cfg(feature = "parser")]
fn parentheses_balanced(text: &str) -> bool {
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    depth == 0
}

/// Whether `word` is a call of the function `name` (ASCII case-insensitive): `name(arguments)`,
/// with arguments that are not empty and whose parentheses are balanced.
#[cfg(feature = "parser")]
fn is_function_call(word: &str, name: &str) -> bool {
    let Some((head, rest)) = word.split_at_checked(name.len()) else {
        return false;
    };
    if !head.eq_ignore_ascii_case(name) {
        return false;
    }
    let Some(arguments) = rest
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
    else {
        return false;
    };
    !arguments.trim().is_empty() && parentheses_balanced(arguments)
}

/// The group of the keyword or function `word` of a `font-variant-*` longhand azul has not, or
/// `None` when `word` is none of them.
#[cfg(feature = "parser")]
fn ignored_group(word: &str) -> Option<&'static str> {
    IGNORED_VARIANT_KEYWORDS
        .iter()
        .find_map(|&(keyword, group)| word.eq_ignore_ascii_case(keyword).then_some(group))
        .or_else(|| {
            IGNORED_VARIANT_FUNCTIONS
                .iter()
                .copied()
                .find(|name| is_function_call(word, name))
        })
}

/// The words of a `font-variant` value, filed under the longhand each belongs to.
#[cfg(feature = "parser")]
#[derive(Default)]
struct VariantWords<'a> {
    numeric: Vec<&'a str>,
    ligatures: Vec<&'a str>,
    east_asian: Vec<&'a str>,
    caps: Option<StyleFontVariantCaps>,
    /// The groups of the keywords without a longhand that have stood already.
    ignored: Vec<&'static str>,
}

/// The value of one longhand: `normal` when no word asked for it, else the words parsed together
/// (the longhand's own parser says whether they are one value, so two of one group are an error).
#[cfg(feature = "parser")]
fn parse_group<T>(
    words: &[&str],
    normal: T,
    parse: fn(&str) -> Result<T, InvalidValueErr<'_>>,
) -> Option<T> {
    if words.is_empty() {
        return Some(normal);
    }
    parse(&words.join(" ")).ok()
}

#[cfg(feature = "parser")]
impl<'a> VariantWords<'a> {
    /// Files `word` under its longhand. False when it belongs to none, or to a longhand that has
    /// its value already.
    fn take(&mut self, word: &'a str) -> bool {
        if let Ok(caps) = parse_style_font_variant_caps(word) {
            // `normal` is the whole value or no part of it.
            let usable = self.caps.is_none() && caps != StyleFontVariantCaps::Normal;
            if usable {
                self.caps = Some(caps);
            }
            return usable;
        }
        if parse_style_font_variant_numeric(word).is_ok() {
            self.numeric.push(word);
            return true;
        }
        // `none` is the whole value or no part of it.
        if !word.eq_ignore_ascii_case("none") && parse_style_font_variant_ligatures(word).is_ok() {
            self.ligatures.push(word);
            return true;
        }
        if parse_style_font_variant_east_asian(word).is_ok() {
            self.east_asian.push(word);
            return true;
        }
        match ignored_group(word) {
            Some(group) if !self.ignored.contains(&group) => {
                self.ignored.push(group);
                true
            }
            _ => false,
        }
    }

    /// The values the words ask for, or `None` when a longhand does not take its words.
    fn values(&self) -> Option<VariantValues> {
        let normal = VariantValues::normal();
        Some(VariantValues {
            numeric: parse_group(
                &self.numeric,
                normal.numeric,
                parse_style_font_variant_numeric,
            )?,
            caps: self.caps.unwrap_or(normal.caps),
            ligatures: parse_group(
                &self.ligatures,
                normal.ligatures,
                parse_style_font_variant_ligatures,
            )?,
            east_asian: parse_group(
                &self.east_asian,
                normal.east_asian,
                parse_style_font_variant_east_asian,
            )?,
        })
    }
}

/// Parses the value of the `font-variant` shorthand: `normal | none | [ <common-lig-values> ||
/// <discretionary-lig-values> || <historical-lig-values> || <contextual-alt-values> ||
/// stylistic() || historical-forms || styleset() || character-variant() || swash() ||
/// ornaments() || annotation() || <caps-keyword> || <numeric-figure-values> ||
/// <numeric-spacing-values> || <numeric-fraction-values> || ordinal || slashed-zero ||
/// <east-asian-variant-values> || <east-asian-width-values> || ruby || [ sub | super ] || [ text |
/// emoji | unicode ] ]`.
///
/// Returns four longhands, in this order: `font-variant-numeric`, `font-variant-caps`,
/// `font-variant-ligatures` and `font-variant-east-asian`, all `Exact`: the ones the value does
/// not name are `normal`. `none` is `none` for the ligatures and `normal` for the others.
///
/// The keywords of the longhands azul has not (`sub`, `super`, `historical-forms`, `text`,
/// `emoji`, `unicode` and the functions `stylistic()`, `styleset()`, `character-variant()`,
/// `swash()`, `ornaments()`, `annotation()`) are accepted and set nothing; of each group of them
/// one may stand.
///
/// # Errors
///
/// Returns an error, carrying the trimmed value, if `input` is not a `font-variant` value: an
/// unknown word, two values of one longhand (`small-caps all-small-caps`, `common-ligatures
/// no-common-ligatures`, `jis78 jis83`), `normal` or `none` together with another word, or
/// nothing at all.
#[cfg(feature = "parser")]
pub fn parse_font_variant_shorthand(input: &str) -> Result<Vec<CssProperty>, InvalidValueErr<'_>> {
    let input = input.trim();
    let invalid = InvalidValueErr(input);
    if input.eq_ignore_ascii_case("normal") {
        return Ok(VariantValues::normal().properties());
    }
    if input.eq_ignore_ascii_case("none") {
        let none = VariantValues {
            ligatures: StyleFontVariantLigatures::NONE,
            ..VariantValues::normal()
        };
        return Ok(none.properties());
    }
    let words = words_outside_parentheses(input);
    if words.is_empty() {
        return Err(invalid);
    }
    let mut filed = VariantWords::default();
    for word in words {
        if !filed.take(word) {
            return Err(invalid);
        }
    }
    let values = filed.values().ok_or(invalid)?;
    Ok(values.properties())
}

// --- font-synthesis ---

/// The four `font-synthesis-*` longhands, in the order weight, style, small caps, position:
/// `auto` for the ones `listed` says are named, `none` for the others.
#[cfg(feature = "parser")]
fn font_synthesis_properties(listed: [bool; 4]) -> Vec<CssProperty> {
    let [weight, style, small_caps, position] = listed.map(|on| {
        CssPropertyValue::Exact(if on {
            StyleFontSynthesis::Auto
        } else {
            StyleFontSynthesis::None
        })
    });
    vec![
        CssProperty::FontSynthesisWeight(weight),
        CssProperty::FontSynthesisStyle(style),
        CssProperty::FontSynthesisSmallCaps(small_caps),
        CssProperty::FontSynthesisPosition(position),
    ]
}

/// Parses the value of the `font-synthesis` shorthand: `none | [ weight || style || small-caps ||
/// position ]`.
///
/// Returns four longhands, in this order: `font-synthesis-weight`, `-style`, `-small-caps` and
/// `-position`, all `Exact`: `none` is `none` for all four, a list is `auto` for the keywords it
/// names and `none` for the others. The keywords are ASCII case-insensitive.
///
/// # Errors
///
/// Returns an error, carrying the trimmed value, if `input` is not a `font-synthesis` value: an
/// unknown keyword (`auto` is none), a keyword given twice, `none` together with another
/// keyword, or nothing at all.
#[cfg(feature = "parser")]
pub fn parse_font_synthesis_shorthand(
    input: &str,
) -> Result<Vec<CssProperty>, InvalidValueErr<'_>> {
    let input = input.trim();
    let invalid = InvalidValueErr(input);
    if input.eq_ignore_ascii_case("none") {
        return Ok(font_synthesis_properties([false; 4]));
    }
    let mut listed = [false; 4];
    for word in input.split_ascii_whitespace() {
        let slot = FONT_SYNTHESIS_KEYWORDS
            .iter()
            .position(|keyword| word.eq_ignore_ascii_case(keyword))
            .ok_or(invalid)?;
        if listed[slot] {
            return Err(invalid);
        }
        listed[slot] = true;
    }
    if !listed.contains(&true) {
        return Err(invalid);
    }
    Ok(font_synthesis_properties(listed))
}

#[cfg(test)]
#[path = "font_shorthand_test.rs"]
mod font_shorthand_test;
