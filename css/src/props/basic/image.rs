use crate::{corety::AzString, props::basic::parse::strip_quotes};

#[derive(Copy, Clone, PartialEq)]
pub enum CssImageParseError<'a> {
    UnclosedQuotes(&'a str),
}

impl_debug_as_display!(CssImageParseError<'a>);
impl_display! {CssImageParseError<'a>, {
    UnclosedQuotes(e) => format!("Unclosed quotes: \"{}\"", e),
}}

/// Owned version of CssImageParseError.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssImageParseErrorOwned {
    UnclosedQuotes(AzString),
}

impl<'a> CssImageParseError<'a> {
    pub fn to_contained(&self) -> CssImageParseErrorOwned {
        match self {
            CssImageParseError::UnclosedQuotes(s) => {
                CssImageParseErrorOwned::UnclosedQuotes(s.to_string().into())
            }
        }
    }
}

impl CssImageParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssImageParseError<'a> {
        match self {
            CssImageParseErrorOwned::UnclosedQuotes(s) => {
                CssImageParseError::UnclosedQuotes(s.as_str())
            }
        }
    }
}

/// A string slice that has been stripped of its quotes.
/// In CSS, quotes are optional in url() so we accept both quoted and unquoted strings.
pub fn parse_image<'a>(input: &'a str) -> Result<AzString, CssImageParseError<'a>> {
    // Try to strip quotes first, but if there are none, use the input as-is
    Ok(match strip_quotes(input) {
        Ok(stripped) => stripped.0.into(),
        Err(_) => input.trim().into(), // No quotes, use as-is (valid in modern CSS)
    })
}
