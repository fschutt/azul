//! CSS string parsing utilities.
//!
//! Parenthesized expressions, quote stripping,
//! comma/whitespace-aware splitting that respects nesting depth, and CSS
//! image/url path parsing.

use crate::corety::AzString;

/// Splits a string by commas, but respects parentheses/braces
///
/// E.g. `url(something,else), url(another,thing)` becomes `["url(something,else)",
/// "url(another,thing)"]` whereas a normal split by comma would yield `["url(something", "else)",
/// "url(another", "thing)"]`
#[must_use] pub fn split_string_respect_comma(input: &str) -> Vec<&str> {
    split_string_by_char(input, ',')
}

/// Splits a string by whitespace, but respects parentheses/braces
///
/// E.g. `translateX(10px) rotate(90deg)` becomes `["translateX(10px)", "rotate(90deg)"]`
#[must_use] pub fn split_string_respect_whitespace(input: &str) -> Vec<&str> {
    let mut items = Vec::<&str>::new();
    let mut current_start = 0;
    let mut depth = 0;
    let input_bytes = input.as_bytes();

    for (idx, &ch) in input_bytes.iter().enumerate() {
        match ch {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b' ' | b'\t' | b'\n' | b'\r' if depth == 0 => {
                if current_start < idx {
                    items.push(&input[current_start..idx]);
                }
                current_start = idx + 1;
            }
            _ => {}
        }
    }

    // Add the last segment
    if current_start < input.len() {
        items.push(&input[current_start..]);
    }

    items
}

fn split_string_by_char(input: &str, target_char: char) -> Vec<&str> {
    let mut comma_separated_items = Vec::<&str>::new();
    let mut current_input = input;

    'outer: loop {
        let Some((skip_next_braces_result, character_was_found)) =
            skip_next_braces(current_input, target_char)
        else {
            break 'outer;
        };
        if character_was_found {
            comma_separated_items.push(&current_input[..skip_next_braces_result]);
            current_input = &current_input[(skip_next_braces_result + 1)..];
        } else {
            comma_separated_items.push(current_input);
            break 'outer;
        }
    }

    comma_separated_items
}

/// Given a string, returns how many characters need to be skipped
fn skip_next_braces(input: &str, target_char: char) -> Option<(usize, bool)> {
    let mut depth = 0;
    let mut last_character: Option<usize> = None;
    let mut character_was_found = false;

    if input.is_empty() {
        return None;
    }

    for (idx, ch) in input.char_indices() {
        last_character = Some(idx);
        match ch {
            '(' => {
                depth += 1;
            }
            ')' => {
                depth -= 1;
            }
            c => {
                if c == target_char && depth == 0 {
                    character_was_found = true;
                    break;
                }
            }
        }
    }

    last_character.map(|lc| (lc, character_was_found))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub enum ParenthesisParseError<'a> {
    UnclosedBraces,
    NoOpeningBraceFound,
    NoClosingBraceFound,
    StopWordNotFound(&'a str),
    EmptyInput,
}

impl_display! { ParenthesisParseError<'a>, {
    UnclosedBraces => format!("Unclosed parenthesis"),
    NoOpeningBraceFound => format!("Expected value in parenthesis (missing \"(\")"),
    NoClosingBraceFound => format!("Missing closing parenthesis (missing \")\")"),
    StopWordNotFound(e) => format!("Stopword not found, found: \"{}\"", e),
    EmptyInput => format!("Empty parenthesis"),
}}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Owned version of `ParenthesisParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum ParenthesisParseErrorOwned {
    UnclosedBraces,
    NoOpeningBraceFound,
    NoClosingBraceFound,
    StopWordNotFound(AzString),
    EmptyInput,
}

impl ParenthesisParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> ParenthesisParseErrorOwned {
        match self {
            ParenthesisParseError::UnclosedBraces => ParenthesisParseErrorOwned::UnclosedBraces,
            ParenthesisParseError::NoOpeningBraceFound => {
                ParenthesisParseErrorOwned::NoOpeningBraceFound
            }
            ParenthesisParseError::NoClosingBraceFound => {
                ParenthesisParseErrorOwned::NoClosingBraceFound
            }
            ParenthesisParseError::StopWordNotFound(s) => {
                ParenthesisParseErrorOwned::StopWordNotFound((*s).to_string().into())
            }
            ParenthesisParseError::EmptyInput => ParenthesisParseErrorOwned::EmptyInput,
        }
    }
}

impl ParenthesisParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> ParenthesisParseError<'_> {
        match self {
            Self::UnclosedBraces => ParenthesisParseError::UnclosedBraces,
            Self::NoOpeningBraceFound => {
                ParenthesisParseError::NoOpeningBraceFound
            }
            Self::NoClosingBraceFound => {
                ParenthesisParseError::NoClosingBraceFound
            }
            Self::StopWordNotFound(s) => {
                ParenthesisParseError::StopWordNotFound(s.as_str())
            }
            Self::EmptyInput => ParenthesisParseError::EmptyInput,
        }
    }
}

/// Checks whether a given input is enclosed in parentheses, prefixed
/// by a certain number of stopwords.
///
/// On success, returns what the stopword was + the string inside the braces
/// on failure returns None.
///
/// ```rust
/// # use azul_css::props::basic::parse::{parse_parentheses, ParenthesisParseError::*};
/// // Search for the nearest "abc()" brace
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["abc"]),
///     Ok(("abc", "def(g)"))
/// );
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["def"]),
///     Err(StopWordNotFound("abc"))
/// );
/// assert_eq!(
///     parse_parentheses("def(ghi(j))", &["def"]),
///     Ok(("def", "ghi(j)"))
/// );
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["abc", "def"]),
///     Ok(("abc", "def(g)"))
/// );
/// ```
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `parentheses` value.
pub fn parse_parentheses<'a>(
    input: &'a str,
    stopwords: &[&'static str],
) -> Result<(&'static str, &'a str), ParenthesisParseError<'a>> {
    use self::ParenthesisParseError::{EmptyInput, NoOpeningBraceFound, StopWordNotFound, NoClosingBraceFound};

    let input = input.trim();
    if input.is_empty() {
        return Err(EmptyInput);
    }

    let first_open_brace = input.find('(').ok_or(NoOpeningBraceFound)?;
    let found_stopword = &input[..first_open_brace];

    // CSS does not allow for space between the ( and the stopword, so no .trim() here
    let mut validated_stopword = None;
    for stopword in stopwords {
        if found_stopword == *stopword {
            validated_stopword = Some(stopword);
            break;
        }
    }

    let validated_stopword = validated_stopword.ok_or(StopWordNotFound(found_stopword))?;
    let last_closing_brace = input.rfind(')').ok_or(NoClosingBraceFound)?;

    Ok((
        validated_stopword,
        &input[(first_open_brace + 1)..last_closing_brace],
    ))
}

/// String has unbalanced `'` or `"` quotation marks
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct UnclosedQuotesError<'a>(pub &'a str);

impl<'a> From<UnclosedQuotesError<'a>> for CssImageParseError<'a> {
    fn from(err: UnclosedQuotesError<'a>) -> Self {
        CssImageParseError::UnclosedQuotes(err.0)
    }
}

/// A string that has been stripped of the beginning and ending quote
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct QuoteStripped<'a>(pub &'a str);

/// Strip quotes from an input, given that both quotes use either `"` or `'`, but not both.
///
/// # Example
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css::props::basic::parse::{strip_quotes, QuoteStripped, UnclosedQuotesError};
/// assert_eq!(
///     strip_quotes("\"Helvetica\""),
///     Ok(QuoteStripped("Helvetica"))
/// );
/// assert_eq!(strip_quotes("'Arial'"), Ok(QuoteStripped("Arial")));
/// assert_eq!(
///     strip_quotes("\"Arial'"),
///     Err(UnclosedQuotesError("\"Arial'"))
/// );
/// ```
/// # Errors
///
/// Returns an error if `input` has an opening quote with no matching closing quote.
pub fn strip_quotes(input: &str) -> Result<QuoteStripped<'_>, UnclosedQuotesError<'_>> {
    let mut double_quote_iter = input.splitn(2, '"');
    double_quote_iter.next();
    let mut single_quote_iter = input.splitn(2, '\'');
    single_quote_iter.next();

    let first_double_quote = double_quote_iter.next();
    let first_single_quote = single_quote_iter.next();
    if first_double_quote.is_some() && first_single_quote.is_some() {
        return Err(UnclosedQuotesError(input));
    }
    if let Some(quote_contents) = first_double_quote {
        if !quote_contents.ends_with('"') {
            return Err(UnclosedQuotesError(quote_contents));
        }
        Ok(QuoteStripped(quote_contents.trim_end_matches('"')))
    } else if let Some(quote_contents) = first_single_quote {
        if !quote_contents.ends_with('\'') {
            return Err(UnclosedQuotesError(input));
        }
        Ok(QuoteStripped(quote_contents.trim_end_matches('\'')))
    } else {
        Err(UnclosedQuotesError(input))
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum CssImageParseError<'a> {
    UnclosedQuotes(&'a str),
}

impl_debug_as_display!(CssImageParseError<'a>);
impl_display! {CssImageParseError<'a>, {
    UnclosedQuotes(e) => format!("Unclosed quotes: \"{}\"", e),
}}

/// Owned version of `CssImageParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssImageParseErrorOwned {
    UnclosedQuotes(AzString),
}

impl CssImageParseError<'_> {
    /// Converts to the owned variant.
    #[must_use] pub fn to_contained(&self) -> CssImageParseErrorOwned {
        match self {
            CssImageParseError::UnclosedQuotes(s) => {
                CssImageParseErrorOwned::UnclosedQuotes((*s).to_string().into())
            }
        }
    }
}

impl CssImageParseErrorOwned {
    /// Converts to the borrowed variant.
    #[must_use] pub fn to_shared(&self) -> CssImageParseError<'_> {
        match self {
            Self::UnclosedQuotes(s) => {
                CssImageParseError::UnclosedQuotes(s.as_str())
            }
        }
    }
}

/// A string slice that has been stripped of its quotes.
/// In CSS, quotes are optional in `url()` so we accept both quoted and unquoted strings.
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `image` value.
pub fn parse_image(input: &str) -> Result<AzString, CssImageParseError<'_>> {
    Ok(strip_quotes(input).map_or_else(|_| input.trim().into(), |stripped| stripped.0.into()))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("'hello'").unwrap(), QuoteStripped("hello"));
        assert_eq!(strip_quotes("\"world\"").unwrap(), QuoteStripped("world"));
        assert_eq!(
            strip_quotes("\"  spaced  \"").unwrap(),
            QuoteStripped("  spaced  ")
        );
        assert!(strip_quotes("'unclosed").is_err());
        assert!(strip_quotes("\"mismatched'").is_err());
        assert!(strip_quotes("no-quotes").is_err());
    }

    #[test]
    fn test_parse_parentheses() {
        assert_eq!(
            parse_parentheses("url(image.png)", &["url"]),
            Ok(("url", "image.png"))
        );
        assert_eq!(
            parse_parentheses("linear-gradient(red, blue)", &["linear-gradient"]),
            Ok(("linear-gradient", "red, blue"))
        );
        assert_eq!(
            parse_parentheses("var(--my-var, 10px)", &["var"]),
            Ok(("var", "--my-var, 10px"))
        );
        assert_eq!(
            parse_parentheses("  rgb( 255, 0, 0 )  ", &["rgb", "rgba"]),
            Ok(("rgb", " 255, 0, 0 "))
        );
    }

    #[test]
    fn test_parse_parentheses_errors() {
        // Stopword not found
        assert!(parse_parentheses("rgba(255,0,0,1)", &["rgb"]).is_err());
        // No opening brace
        assert!(parse_parentheses("url'image.png'", &["url"]).is_err());
        // No closing brace
        assert!(parse_parentheses("url(image.png", &["url"]).is_err());
    }

    #[test]
    fn test_split_string_respect_comma() {
        // Simple case
        let simple = "one, two, three";
        assert_eq!(
            split_string_respect_comma(simple),
            vec!["one", " two", " three"]
        );

        // With parentheses
        let with_parens = "rgba(255, 0, 0, 1), #ff00ff";
        assert_eq!(
            split_string_respect_comma(with_parens),
            vec!["rgba(255, 0, 0, 1)", " #ff00ff"]
        );

        // Multiple parentheses
        let multi_parens =
            "linear-gradient(to right, rgba(0,0,0,0), rgba(0,0,0,1)), url(image.png)";
        assert_eq!(
            split_string_respect_comma(multi_parens),
            vec![
                "linear-gradient(to right, rgba(0,0,0,0), rgba(0,0,0,1))",
                " url(image.png)"
            ]
        );

        // No commas
        let no_commas = "rgb(0,0,0)";
        assert_eq!(split_string_respect_comma(no_commas), vec!["rgb(0,0,0)"]);
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // skip_next_braces (private, parser)
    // ---------------------------------------------------------------------

    #[test]
    fn skip_next_braces_empty_input_returns_none() {
        assert_eq!(skip_next_braces("", ','), None);
        assert_eq!(skip_next_braces("", '('), None);
        assert_eq!(skip_next_braces("", '\0'), None);
    }

    #[test]
    fn skip_next_braces_not_found_yields_last_char_start_not_len() {
        // NOTE: the returned index is the byte offset of the *last char*, not the
        // string length. Callers must not treat it as an exclusive end bound.
        assert_eq!(skip_next_braces("abc", ','), Some((2, false)));
        assert_eq!(skip_next_braces("a", ','), Some((0, false)));
        // 4-byte emoji: index is the char start (0), never a mid-char offset.
        assert_eq!(skip_next_braces("\u{1F600}", ','), Some((0, false)));
    }

    #[test]
    fn skip_next_braces_finds_target_only_at_depth_zero() {
        assert_eq!(skip_next_braces("a,b", ','), Some((1, true)));
        // Comma nested inside parens is invisible; falls through to "not found".
        assert_eq!(skip_next_braces("(a,b)", ','), Some((4, false)));
        // First depth-0 comma, after the group closes.
        assert_eq!(skip_next_braces("(a,b),c", ','), Some((5, true)));
    }

    #[test]
    fn skip_next_braces_unbalanced_closing_paren_drives_depth_negative() {
        // A stray ')' makes depth == -1, so no later comma is ever "at depth 0".
        // Deterministic + no panic, but the separator is silently swallowed.
        assert_eq!(skip_next_braces(")a,b", ','), Some((3, false)));
        assert_eq!(skip_next_braces("))))", ','), Some((3, false)));
    }

    #[test]
    fn skip_next_braces_paren_as_target_char_can_never_match() {
        // The '(' / ')' match arms shadow the target-char arm, so asking for a
        // parenthesis as the separator always reports "not found".
        assert_eq!(skip_next_braces("a(b", '('), Some((2, false)));
        assert_eq!(skip_next_braces("a)b", ')'), Some((2, false)));
    }

    #[test]
    fn skip_next_braces_whitespace_only() {
        assert_eq!(skip_next_braces("   ", ','), Some((2, false)));
        assert_eq!(skip_next_braces("\t\n", ','), Some((1, false)));
        assert_eq!(skip_next_braces("   ", ' '), Some((0, true)));
    }

    #[test]
    fn skip_next_braces_boundary_number_strings() {
        assert_eq!(skip_next_braces("0", ','), Some((0, false)));
        assert_eq!(skip_next_braces("-0", ','), Some((1, false)));
        assert_eq!(
            skip_next_braces("9223372036854775807", ','),
            Some((18, false))
        );
        assert_eq!(skip_next_braces("NaN,inf", ','), Some((3, true)));
        assert_eq!(skip_next_braces("1e309,-1e309", ','), Some((5, true)));
    }

    #[test]
    fn skip_next_braces_unicode_indices_stay_on_char_boundaries() {
        // "e" + combining acute (2 bytes) => comma sits at byte 3.
        assert_eq!(skip_next_braces("e\u{0301},x", ','), Some((3, true)));
        // 4-byte emoji then comma at byte 4.
        assert_eq!(skip_next_braces("\u{1F600},x", ','), Some((4, true)));
        let s = "\u{1F600}\u{0301}\u{4E2D}";
        let (idx, found) = skip_next_braces(s, ',').expect("non-empty input");
        assert!(!found);
        assert!(s.is_char_boundary(idx));
    }

    #[test]
    fn skip_next_braces_extremely_long_input_terminates() {
        let mut input = "a".repeat(1_000_000);
        input.push(',');
        assert_eq!(skip_next_braces(&input, ','), Some((1_000_000, true)));
    }

    #[test]
    fn skip_next_braces_deeply_nested_does_not_stack_overflow() {
        let input = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        // Iterative, not recursive: 20k parens, no target found.
        assert_eq!(skip_next_braces(&input, ','), Some((19_999, false)));
    }

    // ---------------------------------------------------------------------
    // split_string_by_char (private, other)
    // ---------------------------------------------------------------------

    #[test]
    fn split_string_by_char_empty_input_yields_empty_vec() {
        // NOTE: differs from str::split, which yields [""] for an empty input.
        assert!(split_string_by_char("", ',').is_empty());
        assert!(split_string_by_char("", ';').is_empty());
    }

    #[test]
    fn split_string_by_char_respects_nesting_for_any_ascii_separator() {
        assert_eq!(
            split_string_by_char("a;b(c;d);e", ';'),
            vec!["a", "b(c;d)", "e"]
        );
        assert_eq!(split_string_by_char("a b(c d) e", ' '), vec!["a", "b(c d)", "e"]);
    }

    #[test]
    fn split_string_by_char_paren_separator_never_splits() {
        assert_eq!(split_string_by_char("a(b)c", '('), vec!["a(b)c"]);
        assert_eq!(split_string_by_char("a(b)c", ')'), vec!["a(b)c"]);
    }

    // ---------------------------------------------------------------------
    // split_string_respect_comma (other)
    // ---------------------------------------------------------------------

    #[test]
    fn split_comma_empty_and_separator_only_inputs() {
        assert!(split_string_respect_comma("").is_empty());
        assert_eq!(split_string_respect_comma(","), vec![""]);
        assert_eq!(split_string_respect_comma(",,"), vec!["", ""]);
        assert_eq!(split_string_respect_comma("a,,b"), vec!["a", "", "b"]);
    }

    #[test]
    fn split_comma_trailing_separator_drops_the_empty_tail() {
        // Asymmetric: a leading comma keeps its empty segment, a trailing one
        // does not. Pinned so a future refactor has to acknowledge the change.
        assert_eq!(split_string_respect_comma("a,"), vec!["a"]);
        assert_eq!(split_string_respect_comma(",a"), vec!["", "a"]);
    }

    #[test]
    fn split_comma_unbalanced_closing_paren_swallows_separators() {
        // Stray ')' => depth goes negative => nothing splits. No panic.
        assert_eq!(split_string_respect_comma("a),b"), vec!["a),b"]);
        assert_eq!(split_string_respect_comma("a(b,c"), vec!["a(b,c"]);
    }

    #[test]
    fn split_comma_respects_balanced_nesting() {
        assert_eq!(
            split_string_respect_comma("rgba(1,2,3),url(a,b)"),
            vec!["rgba(1,2,3)", "url(a,b)"]
        );
        assert_eq!(
            split_string_respect_comma("f(g(h(1,2),3),4),5"),
            vec!["f(g(h(1,2),3),4)", "5"]
        );
    }

    #[test]
    fn split_comma_unicode_segments_are_valid_utf8() {
        assert_eq!(
            split_string_respect_comma("\u{1F600},h\u{E9}llo,\u{FC}"),
            vec!["\u{1F600}", "h\u{E9}llo", "\u{FC}"]
        );
        // Combining marks must not be sliced apart.
        assert_eq!(
            split_string_respect_comma("e\u{0301},a\u{0308}"),
            vec!["e\u{0301}", "a\u{0308}"]
        );
    }

    #[test]
    fn split_comma_garbage_input_never_panics() {
        for garbage in [
            "\0", "\u{FFFD}", ";;;", "((((", "))))", "()", ",()", "()," , "\\\"'`",
            "\u{200B},\u{200B}", "--,--", "\t,\n,\r",
        ] {
            let parts = split_string_respect_comma(garbage);
            // Every returned slice must be a real substring of the input.
            for p in &parts {
                assert!(garbage.contains(p));
            }
        }
    }

    #[test]
    fn split_comma_extremely_long_inputs_do_not_hang() {
        let no_comma = "a".repeat(1_000_000);
        assert_eq!(split_string_respect_comma(&no_comma), vec![no_comma.as_str()]);

        let all_commas = ",".repeat(100_000);
        let parts = split_string_respect_comma(&all_commas);
        assert_eq!(parts.len(), 100_000);
        assert!(parts.iter().all(|p| p.is_empty()));
    }

    #[test]
    fn split_comma_deeply_nested_does_not_stack_overflow() {
        let nested = format!("{}1,2{}", "(".repeat(10_000), ")".repeat(10_000));
        // Every comma is nested => a single segment.
        assert_eq!(split_string_respect_comma(&nested), vec![nested.as_str()]);
    }

    #[test]
    fn split_comma_round_trips_via_join_when_no_trailing_separator() {
        for input in [
            "a,b,c",
            "one, two, three",
            "rgba(1,2,3),x",
            "a,,b",
            ",a",
            "rgb(0,0,0)",
        ] {
            assert_eq!(split_string_respect_comma(input).join(","), input);
        }
    }

    // ---------------------------------------------------------------------
    // split_string_respect_whitespace (other)
    // ---------------------------------------------------------------------

    #[test]
    fn split_whitespace_empty_and_blank_inputs_yield_nothing() {
        assert!(split_string_respect_whitespace("").is_empty());
        assert!(split_string_respect_whitespace("   ").is_empty());
        assert!(split_string_respect_whitespace("\t\n\r").is_empty());
    }

    #[test]
    fn split_whitespace_valid_minimal_and_run_collapsing() {
        assert_eq!(
            split_string_respect_whitespace("translateX(10px) rotate(90deg)"),
            vec!["translateX(10px)", "rotate(90deg)"]
        );
        assert_eq!(split_string_respect_whitespace("  a\t\tb\n"), vec!["a", "b"]);
    }

    #[test]
    fn split_whitespace_respects_balanced_nesting() {
        assert_eq!(
            split_string_respect_whitespace("translate( 10px , 20px ) scale(2)"),
            vec!["translate( 10px , 20px )", "scale(2)"]
        );
    }

    #[test]
    fn split_whitespace_unbalanced_closing_paren_disables_splitting() {
        assert_eq!(split_string_respect_whitespace("a) b"), vec!["a) b"]);
        assert_eq!(split_string_respect_whitespace("a( b"), vec!["a( b"]);
    }

    #[test]
    fn split_whitespace_unicode_is_split_on_ascii_bytes_only() {
        // Scanning is byte-wise; UTF-8 continuation bytes are >= 0x80 so they can
        // never be mistaken for a space/paren => slices stay on char boundaries.
        assert_eq!(
            split_string_respect_whitespace("h\u{E9}llo w\u{F6}rld \u{1F600}"),
            vec!["h\u{E9}llo", "w\u{F6}rld", "\u{1F600}"]
        );
        // U+00A0 NBSP is *not* an ASCII space => not a separator.
        assert_eq!(
            split_string_respect_whitespace("a\u{A0}b"),
            vec!["a\u{A0}b"]
        );
    }

    #[test]
    fn split_whitespace_garbage_input_never_panics() {
        for garbage in ["\0", "((((", "))))", ")(", "\u{FFFD} \u{FFFD}", "  ) (  "] {
            for p in &split_string_respect_whitespace(garbage) {
                assert!(garbage.contains(p));
            }
        }
    }

    #[test]
    fn split_whitespace_extremely_long_inputs_do_not_hang() {
        let blanks = " ".repeat(1_000_000);
        assert!(split_string_respect_whitespace(&blanks).is_empty());

        let word = "a".repeat(1_000_000);
        assert_eq!(split_string_respect_whitespace(&word), vec![word.as_str()]);
    }

    #[test]
    fn split_whitespace_deeply_nested_does_not_stack_overflow() {
        let nested = format!("{}a{}", "(".repeat(10_000), ")".repeat(10_000));
        let input = format!("{nested} z");
        assert_eq!(
            split_string_respect_whitespace(&input),
            vec![nested.as_str(), "z"]
        );
    }

    // ---------------------------------------------------------------------
    // parse_parentheses (parser)
    // ---------------------------------------------------------------------

    #[test]
    fn parse_parentheses_empty_and_whitespace_only_input() {
        assert_eq!(
            parse_parentheses("", &["url"]),
            Err(ParenthesisParseError::EmptyInput)
        );
        assert_eq!(
            parse_parentheses("   ", &["url"]),
            Err(ParenthesisParseError::EmptyInput)
        );
        assert_eq!(
            parse_parentheses("\t\n", &["url"]),
            Err(ParenthesisParseError::EmptyInput)
        );
        // Empty stopword list can never validate.
        assert_eq!(
            parse_parentheses("url(a)", &[]),
            Err(ParenthesisParseError::StopWordNotFound("url"))
        );
    }

    #[test]
    fn parse_parentheses_valid_minimal_positive_control() {
        assert_eq!(parse_parentheses("a(b)", &["a"]), Ok(("a", "b")));
        assert_eq!(parse_parentheses("abc()", &["abc"]), Ok(("abc", "")));
        assert_eq!(
            parse_parentheses("abc(def(g))", &["abc", "def"]),
            Ok(("abc", "def(g)"))
        );
    }

    #[test]
    fn parse_parentheses_missing_braces_and_stopword() {
        assert_eq!(
            parse_parentheses("abc", &["abc"]),
            Err(ParenthesisParseError::NoOpeningBraceFound)
        );
        assert_eq!(
            parse_parentheses("url(image.png", &["url"]),
            Err(ParenthesisParseError::NoClosingBraceFound)
        );
        assert_eq!(
            parse_parentheses("rgba(1,2,3,4)", &["rgb"]),
            Err(ParenthesisParseError::StopWordNotFound("rgba"))
        );
    }

    #[test]
    fn parse_parentheses_stopword_must_directly_abut_the_brace() {
        // CSS forbids whitespace between the function name and '('; the inner
        // whitespace survives the outer trim, so this must not validate.
        assert_eq!(
            parse_parentheses("url (x)", &["url"]),
            Err(ParenthesisParseError::StopWordNotFound("url "))
        );
        assert_eq!(
            parse_parentheses("URL(x)", &["url"]),
            Err(ParenthesisParseError::StopWordNotFound("URL"))
        );
    }

    #[test]
    fn parse_parentheses_uses_last_closing_brace_and_drops_trailing_junk() {
        // rfind(')') => everything after the *last* ')' is silently discarded.
        assert_eq!(parse_parentheses("url(a)b)", &["url"]), Ok(("url", "a)b")));
        assert_eq!(
            parse_parentheses("url(a);garbage", &["url"]),
            Ok(("url", "a"))
        );
        // Outer whitespace is trimmed, inner whitespace is preserved verbatim.
        assert_eq!(
            parse_parentheses("  rgb( 1 )  ", &["rgb", "rgba"]),
            Ok(("rgb", " 1 "))
        );
    }

    #[test]
    fn parse_parentheses_boundary_number_strings_pass_through_verbatim() {
        for n in [
            "0",
            "-0",
            "NaN",
            "inf",
            "-inf",
            "9223372036854775807",
            "-9223372036854775808",
            "1e309",
            "0.0000000000000000000001",
        ] {
            let input = format!("translate({n})");
            assert_eq!(parse_parentheses(&input, &["translate"]), Ok(("translate", n)));
        }
        // A bare number has no brace at all.
        assert_eq!(
            parse_parentheses("9223372036854775807", &["translate"]),
            Err(ParenthesisParseError::NoOpeningBraceFound)
        );
    }

    #[test]
    fn parse_parentheses_unicode_stopword_and_payload() {
        // Braces are ASCII, so the byte offsets always land on char boundaries.
        assert_eq!(
            parse_parentheses("url(\u{1F600}.png)", &["url"]),
            Ok(("url", "\u{1F600}.png"))
        );
        assert_eq!(
            parse_parentheses("\u{FC}(\u{1F600})", &["\u{FC}"]),
            Ok(("\u{FC}", "\u{1F600}"))
        );
        assert_eq!(
            parse_parentheses("\u{1F600}(x)", &["url"]),
            Err(ParenthesisParseError::StopWordNotFound("\u{1F600}"))
        );
    }

    #[test]
    fn parse_parentheses_garbage_never_panics() {
        for garbage in ["(", ")", ")(", "()", "((((", "))))", "\0(\0)", "\u{FFFD}"] {
            // Either it parses or it errors — it must not panic, and any Ok
            // payload must be a substring of the (trimmed) input.
            if let Ok((_, inner)) = parse_parentheses(garbage, &["", "\u{FFFD}"]) {
                assert!(garbage.contains(inner));
            }
        }
    }

    #[test]
    fn parse_parentheses_extremely_long_input_does_not_hang() {
        let payload = "a".repeat(1_000_000);
        let input = format!("url({payload})");
        assert_eq!(
            parse_parentheses(&input, &["url"]),
            Ok(("url", payload.as_str()))
        );
    }

    #[test]
    fn parse_parentheses_deeply_nested_does_not_stack_overflow() {
        let inner = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        let input = format!("abc({inner})");
        // find/rfind based, not recursive-descent.
        assert_eq!(parse_parentheses(&input, &["abc"]), Ok(("abc", inner.as_str())));
    }

    // ---------------------------------------------------------------------
    // ParenthesisParseError <-> Owned (getters / round-trip)
    // ---------------------------------------------------------------------

    #[test]
    fn parenthesis_error_to_contained_maps_each_variant() {
        assert_eq!(
            ParenthesisParseError::UnclosedBraces.to_contained(),
            ParenthesisParseErrorOwned::UnclosedBraces
        );
        assert_eq!(
            ParenthesisParseError::NoOpeningBraceFound.to_contained(),
            ParenthesisParseErrorOwned::NoOpeningBraceFound
        );
        assert_eq!(
            ParenthesisParseError::NoClosingBraceFound.to_contained(),
            ParenthesisParseErrorOwned::NoClosingBraceFound
        );
        assert_eq!(
            ParenthesisParseError::EmptyInput.to_contained(),
            ParenthesisParseErrorOwned::EmptyInput
        );
        assert_eq!(
            ParenthesisParseError::StopWordNotFound("abc").to_contained(),
            ParenthesisParseErrorOwned::StopWordNotFound("abc".into())
        );
    }

    #[test]
    fn parenthesis_error_round_trips_through_owned() {
        let huge = "x".repeat(100_000);
        let cases = [
            ParenthesisParseError::UnclosedBraces,
            ParenthesisParseError::NoOpeningBraceFound,
            ParenthesisParseError::NoClosingBraceFound,
            ParenthesisParseError::EmptyInput,
            ParenthesisParseError::StopWordNotFound(""),
            ParenthesisParseError::StopWordNotFound("url"),
            ParenthesisParseError::StopWordNotFound("\u{1F600}\u{0301}"),
            ParenthesisParseError::StopWordNotFound("\0"),
            ParenthesisParseError::StopWordNotFound(huge.as_str()),
        ];
        for case in cases {
            let owned = case.to_contained();
            assert_eq!(owned.to_shared(), case, "round-trip must be lossless");
            // to_shared -> to_contained must also be stable.
            assert_eq!(owned.to_shared().to_contained(), owned);
        }
    }

    #[test]
    fn parenthesis_error_owned_to_shared_borrows_the_payload() {
        let owned = ParenthesisParseErrorOwned::StopWordNotFound("linear-gradient".into());
        match owned.to_shared() {
            ParenthesisParseError::StopWordNotFound(s) => assert_eq!(s, "linear-gradient"),
            other => panic!("expected StopWordNotFound, got {other:?}"),
        }
    }

    #[test]
    fn parenthesis_error_display_never_panics_on_extreme_payloads() {
        for e in [
            ParenthesisParseError::EmptyInput,
            ParenthesisParseError::StopWordNotFound(""),
            ParenthesisParseError::StopWordNotFound("\u{1F600}"),
        ] {
            assert!(!format!("{e}").is_empty());
        }
    }

    // ---------------------------------------------------------------------
    // strip_quotes (parser)
    // ---------------------------------------------------------------------

    #[test]
    fn strip_quotes_valid_minimal_positive_control() {
        assert_eq!(strip_quotes("\"Helvetica\""), Ok(QuoteStripped("Helvetica")));
        assert_eq!(strip_quotes("'Arial'"), Ok(QuoteStripped("Arial")));
        // Empty quoted string is legal and yields an empty payload.
        assert_eq!(strip_quotes("\"\""), Ok(QuoteStripped("")));
        assert_eq!(strip_quotes("''"), Ok(QuoteStripped("")));
    }

    #[test]
    fn strip_quotes_empty_blank_and_unquoted_inputs_error() {
        assert_eq!(strip_quotes(""), Err(UnclosedQuotesError("")));
        assert_eq!(strip_quotes("   "), Err(UnclosedQuotesError("   ")));
        assert_eq!(strip_quotes("\t\n"), Err(UnclosedQuotesError("\t\n")));
        assert_eq!(strip_quotes("no-quotes"), Err(UnclosedQuotesError("no-quotes")));
    }

    #[test]
    fn strip_quotes_mixed_quote_kinds_are_rejected() {
        assert_eq!(strip_quotes("\"Arial'"), Err(UnclosedQuotesError("\"Arial'")));
        // A legitimate apostrophe inside a double-quoted name is *also* rejected.
        assert_eq!(
            strip_quotes("\"Bob's Font\""),
            Err(UnclosedQuotesError("\"Bob's Font\""))
        );
    }

    #[test]
    fn strip_quotes_unclosed_error_payload_is_asymmetric_between_branches() {
        // The single-quote branch reports the whole input...
        assert_eq!(strip_quotes("'unclosed"), Err(UnclosedQuotesError("'unclosed")));
        assert_eq!(strip_quotes("'"), Err(UnclosedQuotesError("'")));
        // ...but the double-quote branch reports only the text *after* the quote,
        // losing the leading '"'. Pinned as-is; the two branches disagree.
        assert_eq!(strip_quotes("\"unclosed"), Err(UnclosedQuotesError("unclosed")));
        assert_eq!(strip_quotes("\""), Err(UnclosedQuotesError("")));
    }

    #[test]
    fn strip_quotes_surrounding_whitespace_defeats_stripping() {
        // strip_quotes does not trim, so a padded input is "unclosed".
        assert_eq!(
            strip_quotes(" \"Arial\" "),
            Err(UnclosedQuotesError("Arial\" "))
        );
        assert_eq!(
            strip_quotes(" 'Arial' "),
            Err(UnclosedQuotesError(" 'Arial' "))
        );
        // Inner whitespace, however, is preserved exactly.
        assert_eq!(strip_quotes("\"  spaced  \""), Ok(QuoteStripped("  spaced  ")));
    }

    #[test]
    fn strip_quotes_trims_the_entire_trailing_quote_run() {
        // trim_end_matches strips *all* trailing quotes, not just one.
        assert_eq!(strip_quotes("\"\"\""), Ok(QuoteStripped("")));
        assert_eq!(strip_quotes("\"ab\"\"\""), Ok(QuoteStripped("ab")));
        // An interior quote survives, so the result can still contain a quote.
        assert_eq!(strip_quotes("\"a\"b\""), Ok(QuoteStripped("a\"b")));
    }

    #[test]
    fn strip_quotes_unicode_payload() {
        assert_eq!(
            strip_quotes("\"\u{1F600}\u{E9}\""),
            Ok(QuoteStripped("\u{1F600}\u{E9}"))
        );
        assert_eq!(
            strip_quotes("'e\u{0301}\u{4E2D}'"),
            Ok(QuoteStripped("e\u{0301}\u{4E2D}"))
        );
    }

    #[test]
    fn strip_quotes_boundary_number_strings() {
        for n in ["0", "-0", "NaN", "inf", "9223372036854775807", "1e309"] {
            assert_eq!(strip_quotes(&format!("\"{n}\"")), Ok(QuoteStripped(n)));
        }
    }

    #[test]
    fn strip_quotes_garbage_never_panics() {
        for garbage in ["\0", "\\", "`", "\u{FFFD}", "\"\0\"", "((\"))"] {
            let _ = strip_quotes(garbage);
        }
        assert_eq!(strip_quotes("\"\0\""), Ok(QuoteStripped("\0")));
    }

    #[test]
    fn strip_quotes_extremely_long_and_deeply_nested_inputs() {
        let payload = "a".repeat(1_000_000);
        let input = format!("\"{payload}\"");
        assert_eq!(strip_quotes(&input), Ok(QuoteStripped(payload.as_str())));

        let nested = format!("{}x{}", "(".repeat(10_000), ")".repeat(10_000));
        let quoted = format!("'{nested}'");
        assert_eq!(strip_quotes(&quoted), Ok(QuoteStripped(nested.as_str())));
    }

    #[test]
    fn strip_quotes_round_trips_quote_free_payloads() {
        for payload in [
            "Helvetica",
            "",
            "  spaced  ",
            "url(a,b)",
            "\u{1F600}",
            "0",
            "a\nb",
        ] {
            assert_eq!(
                strip_quotes(&format!("\"{payload}\"")),
                Ok(QuoteStripped(payload)),
                "double-quote round-trip"
            );
            assert_eq!(
                strip_quotes(&format!("'{payload}'")),
                Ok(QuoteStripped(payload)),
                "single-quote round-trip"
            );
        }
    }

    // ---------------------------------------------------------------------
    // CssImageParseError <-> Owned (getters / round-trip)
    // ---------------------------------------------------------------------

    #[test]
    fn css_image_error_round_trips_through_owned() {
        let huge = "x".repeat(100_000);
        for payload in ["", "a.png", "\u{1F600}\u{0301}", "\0", huge.as_str()] {
            let shared = CssImageParseError::UnclosedQuotes(payload);
            let owned = shared.to_contained();
            assert_eq!(owned, CssImageParseErrorOwned::UnclosedQuotes(payload.into()));
            assert_eq!(owned.to_shared(), shared, "round-trip must be lossless");
            assert_eq!(owned.to_shared().to_contained(), owned);
        }
    }

    #[test]
    fn css_image_error_display_includes_the_payload() {
        let e = CssImageParseError::UnclosedQuotes("\u{1F600}");
        assert!(format!("{e}").contains('\u{1F600}'));
        // Debug is routed through Display; it must not panic on an empty payload.
        assert!(!format!("{:?}", CssImageParseError::UnclosedQuotes("")).is_empty());
    }

    #[test]
    fn unclosed_quotes_error_converts_into_css_image_error() {
        let e: CssImageParseError<'_> = UnclosedQuotesError("bad").into();
        assert_eq!(e, CssImageParseError::UnclosedQuotes("bad"));
    }

    // ---------------------------------------------------------------------
    // parse_image (parser)
    // ---------------------------------------------------------------------

    #[test]
    fn parse_image_is_infallible_for_every_adversarial_input() {
        // The signature returns Result, but the body swallows the strip_quotes
        // error and falls back to the trimmed input — it can never be Err.
        let huge = "a".repeat(1_000_000);
        let nested = format!("{}x{}", "(".repeat(10_000), ")".repeat(10_000));
        for input in [
            "",
            "   ",
            "\t\n",
            "\0",
            "\"",
            "'",
            "\"mixed'",
            "no-quotes",
            "url(a.png)",
            "9223372036854775807",
            "NaN",
            "\u{1F600}",
            huge.as_str(),
            nested.as_str(),
        ] {
            assert!(parse_image(input).is_ok(), "parse_image({input:?}) must be Ok");
        }
    }

    #[test]
    fn parse_image_valid_minimal_positive_control() {
        assert_eq!(parse_image("\"image.png\"").unwrap().as_str(), "image.png");
        assert_eq!(parse_image("'image.png'").unwrap().as_str(), "image.png");
        // Unquoted input is accepted and trimmed.
        assert_eq!(parse_image("  image.png  ").unwrap().as_str(), "image.png");
        assert_eq!(parse_image("").unwrap().as_str(), "");
        assert_eq!(parse_image("   ").unwrap().as_str(), "");
    }

    #[test]
    fn parse_image_quoted_payload_is_not_trimmed() {
        // The successful strip_quotes path preserves inner whitespace verbatim,
        // while the fallback path trims — the two paths differ.
        assert_eq!(parse_image("\"  a  \"").unwrap().as_str(), "  a  ");
        assert_eq!(parse_image("  a  ").unwrap().as_str(), "a");
    }

    #[test]
    fn parse_image_malformed_quotes_fall_back_to_the_raw_trimmed_input() {
        // Quotes are *retained* when stripping fails — an unbalanced quote is
        // silently accepted as part of the path rather than rejected.
        assert_eq!(parse_image("\"unclosed").unwrap().as_str(), "\"unclosed");
        assert_eq!(parse_image("'unclosed").unwrap().as_str(), "'unclosed");
        assert_eq!(parse_image("\"mixed'").unwrap().as_str(), "\"mixed'");
        // Padding around the quotes defeats strip_quotes, so the quotes survive.
        assert_eq!(parse_image("  \"a\"  ").unwrap().as_str(), "\"a\"");
    }

    #[test]
    fn parse_image_does_not_unwrap_url_functions() {
        // parse_image only strips quotes; it is not a url() parser.
        assert_eq!(parse_image("url(a.png)").unwrap().as_str(), "url(a.png)");
    }

    #[test]
    fn parse_image_unicode_and_extremely_long_inputs() {
        assert_eq!(
            parse_image("'\u{1F600}.png'").unwrap().as_str(),
            "\u{1F600}.png"
        );
        let payload = "a".repeat(1_000_000);
        let input = format!("\"{payload}\"");
        assert_eq!(parse_image(&input).unwrap().as_str().len(), 1_000_000);
    }

    #[test]
    fn parse_image_round_trips_quote_free_payloads() {
        for payload in ["a.png", "", "\u{1F600}", "some/deep/path.jpeg", "0"] {
            assert_eq!(
                parse_image(&format!("\"{payload}\"")).unwrap().as_str(),
                payload
            );
            assert_eq!(
                parse_image(&format!("'{payload}'")).unwrap().as_str(),
                payload
            );
        }
    }
}
