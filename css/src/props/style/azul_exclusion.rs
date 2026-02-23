// Azul-specific CSS properties for advanced layout features

use std::num::ParseFloatError;

#[cfg(feature = "parser")]
use crate::macros::*;
use crate::{
    corety::AzString,
    format_rust_code::FormatAsRustCode,
    props::{
        basic::{length::parse_float_value, FloatValue},
        formatter::{FormatAsCssValue, PrintAsCssValue},
    },
};

/// `-azul-exclusion-margin` property: defines margin around shape exclusions
///
/// This property controls the spacing between text and shapes that text flows around.
/// It's similar to `shape-margin` but specifically for exclusions (text wrapping).
///
/// # Example
/// ```css
/// .element {
///     -azul-exclusion-margin: 10.5;
/// }
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleExclusionMargin {
    pub inner: FloatValue,
}

impl Default for StyleExclusionMargin {
    fn default() -> Self {
        Self {
            inner: FloatValue::const_new(0),
        }
    }
}

impl StyleExclusionMargin {
    pub fn is_initial(&self) -> bool {
        self.inner.number == 0
    }
}

impl PrintAsCssValue for StyleExclusionMargin {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner.get())
    }
}

impl FormatAsCssValue for StyleExclusionMargin {
    fn format_as_css_value(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.inner.get())
    }
}

impl FormatAsRustCode for StyleExclusionMargin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleExclusionMargin {{ inner: FloatValue::const_new({}) }}",
            self.inner.get()
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleExclusionMarginParseError {
    FloatValue(ParseFloatError),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleExclusionMarginParseError);

#[cfg(feature = "parser")]
impl_display! { StyleExclusionMarginParseError, {
    FloatValue(e) => format!("Invalid -azul-exclusion-margin value: {}", e),
}}

#[cfg(feature = "parser")]
impl_from!(ParseFloatError, StyleExclusionMarginParseError::FloatValue);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleExclusionMarginParseErrorOwned {
    FloatValue(AzString),
}

#[cfg(feature = "parser")]
impl StyleExclusionMarginParseError {
    pub fn to_contained(&self) -> StyleExclusionMarginParseErrorOwned {
        match self {
            Self::FloatValue(e) => {
                StyleExclusionMarginParseErrorOwned::FloatValue(format!("{}", e).into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleExclusionMarginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleExclusionMarginParseError {
        match self {
            Self::FloatValue(e) => {
                // ParseFloatError doesn't have to_shared, so we recreate it from string
                StyleExclusionMarginParseError::FloatValue(e.parse::<f32>().unwrap_err())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_exclusion_margin(
    input: &str,
) -> Result<StyleExclusionMargin, StyleExclusionMarginParseError> {
    parse_float_value(input)
        .map(|inner| StyleExclusionMargin { inner })
        .map_err(|e| StyleExclusionMarginParseError::FloatValue(e))
}

/// `-azul-hyphenation-language` property: specifies language for hyphenation
///
/// This property defines the language code (BCP 47 format) used for automatic
/// hyphenation. Examples: "en-US", "de-DE", "fr-FR"
///
/// # Example
/// ```css
/// .element {
///     -azul-hyphenation-language: "en-US";
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleHyphenationLanguage {
    pub inner: AzString,
}

impl Default for StyleHyphenationLanguage {
    fn default() -> Self {
        Self {
            inner: AzString::from_const_str("en-US"),
        }
    }
}

impl StyleHyphenationLanguage {
    pub const fn is_initial(&self) -> bool {
        // Cannot compare AzString in const context, so always return false
        false
    }
}

impl PrintAsCssValue for StyleHyphenationLanguage {
    fn print_as_css_value(&self) -> String {
        format!("\"{}\"", self.inner.as_str())
    }
}

impl FormatAsCssValue for StyleHyphenationLanguage {
    fn format_as_css_value(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "\"{}\"", self.inner.as_str())
    }
}

impl FormatAsRustCode for StyleHyphenationLanguage {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "StyleHyphenationLanguage {{ inner: AzString::from_const_str(\"{}\") }}",
            self.inner.as_str()
        )
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum StyleHyphenationLanguageParseError {
    InvalidString(String),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(StyleHyphenationLanguageParseError);

#[cfg(feature = "parser")]
impl_display! { StyleHyphenationLanguageParseError, {
    InvalidString(e) => format!("Invalid -azul-hyphenation-language value: {}", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum StyleHyphenationLanguageParseErrorOwned {
    InvalidString(AzString),
}

#[cfg(feature = "parser")]
impl StyleHyphenationLanguageParseError {
    pub fn to_contained(&self) -> StyleHyphenationLanguageParseErrorOwned {
        match self {
            Self::InvalidString(e) => {
                StyleHyphenationLanguageParseErrorOwned::InvalidString(e.clone().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl StyleHyphenationLanguageParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleHyphenationLanguageParseError {
        match self {
            Self::InvalidString(e) => StyleHyphenationLanguageParseError::InvalidString(e.to_string()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_hyphenation_language(
    input: &str,
) -> Result<StyleHyphenationLanguage, StyleHyphenationLanguageParseError> {
    // Remove quotes if present
    let trimmed = input.trim();
    let unquoted = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    Ok(StyleHyphenationLanguage {
        inner: AzString::from_string(unquoted.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exclusion_margin() {
        let margin = parse_style_exclusion_margin("10.5").unwrap();
        assert_eq!(margin.inner.get(), 10.5);

        let margin = parse_style_exclusion_margin("0").unwrap();
        assert_eq!(margin.inner.get(), 0.0);
    }

    #[test]
    fn test_parse_hyphenation_language() {
        let lang = parse_style_hyphenation_language("\"en-US\"").unwrap();
        assert_eq!(lang.inner.as_str(), "en-US");

        let lang = parse_style_hyphenation_language("'de-DE'").unwrap();
        assert_eq!(lang.inner.as_str(), "de-DE");

        let lang = parse_style_hyphenation_language("fr-FR").unwrap();
        assert_eq!(lang.inner.as_str(), "fr-FR");
    }

    #[test]
    fn test_exclusion_margin_default() {
        let margin = StyleExclusionMargin::default();
        assert_eq!(margin.inner.get(), 0.0);
        assert!(margin.is_initial());
    }

    #[test]
    fn test_hyphenation_language_default() {
        let lang = StyleHyphenationLanguage::default();
        assert_eq!(lang.inner.as_str(), "en-US");
    }
}
