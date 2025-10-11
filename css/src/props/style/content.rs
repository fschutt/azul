//! CSS properties for generated content.

use alloc::string::{String, ToString};

use crate::{corety::AzString, props::formatter::PrintAsCssValue};

// A full implementation would have an enum for ContentPart with variants for
// strings, counters, attributes, etc., and Content would be a Vec<ContentPart>.
// For now, we'll just store the raw string value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Content {
    pub inner: String,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            inner: "normal".to_string(),
        }
    }
}

impl PrintAsCssValue for Content {
    fn print_as_css_value(&self) -> String {
        self.inner.clone()
    }
}

// Placeholder structs for other properties
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CounterReset {
    pub inner: String,
}
impl Default for CounterReset {
    fn default() -> Self {
        Self {
            inner: "none".to_string(),
        }
    }
}
impl PrintAsCssValue for CounterReset {
    fn print_as_css_value(&self) -> String {
        self.inner.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CounterIncrement {
    pub inner: String,
}
impl Default for CounterIncrement {
    fn default() -> Self {
        Self {
            inner: "none".to_string(),
        }
    }
}
impl PrintAsCssValue for CounterIncrement {
    fn print_as_css_value(&self) -> String {
        self.inner.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StringSet {
    pub inner: String,
}
impl Default for StringSet {
    fn default() -> Self {
        Self {
            inner: "none".to_string(),
        }
    }
}
impl PrintAsCssValue for StringSet {
    fn print_as_css_value(&self) -> String {
        self.inner.clone()
    }
}

// Formatting to Rust code
impl crate::format_rust_code::FormatAsRustCode for Content {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Content {{ inner: String::from({:?}) }}", self.inner)
    }
}

impl crate::format_rust_code::FormatAsRustCode for CounterReset {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("CounterReset {{ inner: String::from({:?}) }}", self.inner)
    }
}

impl crate::format_rust_code::FormatAsRustCode for CounterIncrement {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "CounterIncrement {{ inner: String::from({:?}) }}",
            self.inner
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for StringSet {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StringSet {{ inner: String::from({:?}) }}", self.inner)
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
mod parser {
    use super::*;

    // Simplified parsers that just take the raw string value.
    pub fn parse_content(input: &str) -> Result<Content, ()> {
        Ok(Content {
            inner: input.trim().to_string(),
        })
    }

    pub fn parse_counter_reset(input: &str) -> Result<CounterReset, ()> {
        Ok(CounterReset {
            inner: input.trim().to_string(),
        })
    }

    pub fn parse_counter_increment(input: &str) -> Result<CounterIncrement, ()> {
        Ok(CounterIncrement {
            inner: input.trim().to_string(),
        })
    }

    pub fn parse_string_set(input: &str) -> Result<StringSet, ()> {
        Ok(StringSet {
            inner: input.trim().to_string(),
        })
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_simple_content_parser() {
        assert_eq!(parse_content("'Hello'").unwrap().inner, "'Hello'");
        assert_eq!(parse_counter_reset("page 1").unwrap().inner, "page 1");
        assert_eq!(parse_counter_increment("section").unwrap().inner, "section");
        assert_eq!(
            parse_string_set("chapter-title content()").unwrap().inner,
            "chapter-title content()"
        );
    }
}
