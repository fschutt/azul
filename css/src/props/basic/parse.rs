use crate::props::basic::CssImageParseError;
use crate::corety::AzString;

/// Splits a string by commas, but respects parentheses/braces
///
/// E.g. `url(something,else), url(another,thing)` becomes `["url(something,else)",
/// "url(another,thing)"]` whereas a normal split by comma would yield `["url(something", "else)",
/// "url(another", "thing)"]`
pub fn split_string_respect_comma<'a>(input: &'a str) -> Vec<&'a str> {
    split_string_by_char(input, ',')
}

/// Splits a string by whitespace, but respects parentheses/braces
///
/// E.g. `translateX(10px) rotate(90deg)` becomes `["translateX(10px)", "rotate(90deg)"]`
pub fn split_string_respect_whitespace<'a>(input: &'a str) -> Vec<&'a str> {
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

fn split_string_by_char<'a>(input: &'a str, target_char: char) -> Vec<&'a str> {
    let mut comma_separated_items = Vec::<&str>::new();
    let mut current_input = &input[..];

    'outer: loop {
        let (skip_next_braces_result, character_was_found) =
            match skip_next_braces(&current_input, target_char) {
                Some(s) => s,
                None => break 'outer,
            };
        let new_push_item = if character_was_found {
            &current_input[..skip_next_braces_result]
        } else {
            &current_input[..]
        };
        let new_current_input = &current_input[(skip_next_braces_result + 1)..];
        comma_separated_items.push(new_push_item);
        current_input = new_current_input;
        if !character_was_found {
            break 'outer;
        }
    }

    comma_separated_items
}

/// Given a string, returns how many characters need to be skipped
pub fn skip_next_braces(input: &str, target_char: char) -> Option<(usize, bool)> {
    let mut depth = 0;
    let mut last_character = 0;
    let mut character_was_found = false;

    if input.is_empty() {
        return None;
    }

    for (idx, ch) in input.char_indices() {
        last_character = idx;
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

    if last_character == 0 {
        // No more split by `,`
        None
    } else {
        Some((last_character, character_was_found))
    }
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

/// Owned version of ParenthesisParseError.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum ParenthesisParseErrorOwned {
    UnclosedBraces,
    NoOpeningBraceFound,
    NoClosingBraceFound,
    StopWordNotFound(AzString),
    EmptyInput,
}

impl<'a> ParenthesisParseError<'a> {
    pub fn to_contained(&self) -> ParenthesisParseErrorOwned {
        match self {
            ParenthesisParseError::UnclosedBraces => ParenthesisParseErrorOwned::UnclosedBraces,
            ParenthesisParseError::NoOpeningBraceFound => {
                ParenthesisParseErrorOwned::NoOpeningBraceFound
            }
            ParenthesisParseError::NoClosingBraceFound => {
                ParenthesisParseErrorOwned::NoClosingBraceFound
            }
            ParenthesisParseError::StopWordNotFound(s) => {
                ParenthesisParseErrorOwned::StopWordNotFound(s.to_string().into())
            }
            ParenthesisParseError::EmptyInput => ParenthesisParseErrorOwned::EmptyInput,
        }
    }
}

impl ParenthesisParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> ParenthesisParseError<'a> {
        match self {
            ParenthesisParseErrorOwned::UnclosedBraces => ParenthesisParseError::UnclosedBraces,
            ParenthesisParseErrorOwned::NoOpeningBraceFound => {
                ParenthesisParseError::NoOpeningBraceFound
            }
            ParenthesisParseErrorOwned::NoClosingBraceFound => {
                ParenthesisParseError::NoClosingBraceFound
            }
            ParenthesisParseErrorOwned::StopWordNotFound(s) => {
                ParenthesisParseError::StopWordNotFound(s.as_str())
            }
            ParenthesisParseErrorOwned::EmptyInput => ParenthesisParseError::EmptyInput,
        }
    }
}

/// Checks wheter a given input is enclosed in parentheses, prefixed
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
pub fn parse_parentheses<'a>(
    input: &'a str,
    stopwords: &[&'static str],
) -> Result<(&'static str, &'a str), ParenthesisParseError<'a>> {
    use self::ParenthesisParseError::*;

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
pub fn strip_quotes<'a>(input: &'a str) -> Result<QuoteStripped<'a>, UnclosedQuotesError<'a>> {
    let mut double_quote_iter = input.splitn(2, '"');
    double_quote_iter.next();
    let mut single_quote_iter = input.splitn(2, '\'');
    single_quote_iter.next();

    let first_double_quote = double_quote_iter.next();
    let first_single_quote = single_quote_iter.next();
    if first_double_quote.is_some() && first_single_quote.is_some() {
        return Err(UnclosedQuotesError(input));
    }
    if first_double_quote.is_some() {
        let quote_contents = first_double_quote.unwrap();
        if !quote_contents.ends_with('"') {
            return Err(UnclosedQuotesError(quote_contents));
        }
        Ok(QuoteStripped(quote_contents.trim_end_matches("\"")))
    } else if first_single_quote.is_some() {
        let quote_contents = first_single_quote.unwrap();
        if !quote_contents.ends_with('\'') {
            return Err(UnclosedQuotesError(input));
        }
        Ok(QuoteStripped(quote_contents.trim_end_matches("'")))
    } else {
        Err(UnclosedQuotesError(input))
    }
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
