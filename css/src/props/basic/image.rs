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
pub enum CssImageParseErrorOwned {
    UnclosedQuotes(String),
}

impl<'a> CssImageParseError<'a> {
    pub fn to_contained(&self) -> CssImageParseErrorOwned {
        match self {
            CssImageParseError::UnclosedQuotes(s) => {
                CssImageParseErrorOwned::UnclosedQuotes(s.to_string())
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
pub fn parse_image<'a>(input: &'a str) -> Result<AzString, CssImageParseError<'a>> {
    Ok(strip_quotes(input)?.0.into())
}
