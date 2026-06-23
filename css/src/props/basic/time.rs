//! CSS property types for time durations (s, ms).

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

/// A CSS time duration, stored internally in milliseconds.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub struct CssDuration {
    /// Duration in milliseconds.
    pub inner: u32,
}


impl PrintAsCssValue for CssDuration {
    fn print_as_css_value(&self) -> String {
        format!("{}ms", self.inner)
    }
}

impl crate::codegen::format::FormatAsRustCode for CssDuration {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("CssDuration {{ inner: {} }}", self.inner)
    }
}

/// Error returned when parsing a CSS duration string fails.
#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
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

/// Owned version of [`DurationParseError`] for FFI and storage.
#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum DurationParseErrorOwned {
    InvalidValue(AzString),
    ParseFloat(AzString),
}

#[cfg(feature = "parser")]
impl DurationParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> DurationParseErrorOwned {
        match self {
            Self::InvalidValue(s) => DurationParseErrorOwned::InvalidValue((*s).to_string().into()),
            Self::ParseFloat(e) => DurationParseErrorOwned::ParseFloat(e.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl DurationParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> DurationParseError<'_> {
        match self {
            Self::InvalidValue(s) => DurationParseError::InvalidValue(s),
            Self::ParseFloat(s) => DurationParseError::InvalidValue(s.as_str()),
        }
    }
}

/// Parses a CSS duration string (e.g. `"200ms"`, `"1.5s"`) into a [`CssDuration`].
#[cfg(feature = "parser")]
pub fn parse_duration(input: &str) -> Result<CssDuration, DurationParseError<'_>> {
    let trimmed = input.trim().to_lowercase();
    if trimmed == "0" {
        return Ok(CssDuration { inner: 0 });
    }
    if let Some(num_str) = trimmed.strip_suffix("ms") {
        let ms = num_str
            .parse::<f32>()
            .map_err(DurationParseError::ParseFloat)?;
        if ms < 0.0 {
            return Err(DurationParseError::InvalidValue(input));
        }
        Ok(CssDuration { inner: ms as u32 })
    } else if let Some(num_str) = trimmed.strip_suffix('s') {
        let s = num_str
            .parse::<f32>()
            .map_err(DurationParseError::ParseFloat)?;
        if s < 0.0 {
            return Err(DurationParseError::InvalidValue(input));
        }
        Ok(CssDuration {
            inner: (s * 1000.0) as u32,
        })
    } else {
        Err(DurationParseError::InvalidValue(input))
    }
}
