//! CSS property types for time durations (s, ms).

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CssDuration {
    /// Duration in milliseconds.
    pub inner: u32,
}

impl Default for CssDuration {
    fn default() -> Self {
        Self { inner: 0 }
    }
}

impl PrintAsCssValue for CssDuration {
    fn print_as_css_value(&self) -> String {
        format!("{}ms", self.inner)
    }
}

impl crate::format_rust_code::FormatAsRustCode for CssDuration {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("CssDuration {{ inner: {} }}", self.inner)
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum DurationParseError<'a> {
    InvalidValue(&'a str),
    ParseFloat(core::num::ParseFloatError),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(DurationParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { DurationParseError<'a>, {
    InvalidValue(v) => format!("Invalid time value: \"{}\"", v),
    ParseFloat(e) => format!("Invalid number for time value: {}", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum DurationParseErrorOwned {
    InvalidValue(AzString),
    ParseFloat(AzString),
}

#[cfg(feature = "parser")]
impl<'a> DurationParseError<'a> {
    pub fn to_contained(&self) -> DurationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => DurationParseErrorOwned::InvalidValue(s.to_string().into()),
            Self::ParseFloat(e) => DurationParseErrorOwned::ParseFloat(e.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl DurationParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> DurationParseError<'a> {
        match self {
            Self::InvalidValue(s) => DurationParseError::InvalidValue(s),
            Self::ParseFloat(s) => DurationParseError::InvalidValue("invalid float"), /* Can't reconstruct */
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_duration<'a>(input: &'a str) -> Result<CssDuration, DurationParseError<'a>> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.ends_with("ms") {
        let num_str = &trimmed[..trimmed.len() - 2];
        let ms = num_str
            .parse::<f32>()
            .map_err(|e| DurationParseError::ParseFloat(e))?;
        Ok(CssDuration { inner: ms as u32 })
    } else if trimmed.ends_with('s') {
        let num_str = &trimmed[..trimmed.len() - 1];
        let s = num_str
            .parse::<f32>()
            .map_err(|e| DurationParseError::ParseFloat(e))?;
        Ok(CssDuration {
            inner: (s * 1000.0) as u32,
        })
    } else {
        Err(DurationParseError::InvalidValue(input))
    }
}
