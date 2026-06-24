//! CSS properties for generated content (`content`, `counter-reset`,
//! `counter-increment`, `string-set`).
//!
//! Defines [`Content`], [`CounterReset`], [`CounterIncrement`], and
//! [`StringSet`], which are registered as [`CssProperty`] variants.

use alloc::string::{String, ToString};

use crate::{corety::AzString, props::formatter::PrintAsCssValue};

/// CSS `content` property value, stored as a raw string.
///
/// Intentionally simplified: stores the unparsed CSS value rather than
/// a structured `ContentPart` enum. Complex values like `counter(section) ". "`
/// are preserved verbatim but not individually evaluated.
///
/// **Note:** Currently parsed and stored but not yet consumed by the layout
/// engine (e.g., for `::before`/`::after` pseudo-element generated content).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Content {
    pub inner: AzString,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            inner: "normal".into(),
        }
    }
}

impl PrintAsCssValue for Content {
    fn print_as_css_value(&self) -> String {
        self.inner.as_str().to_string()
    }
}

/// CSS `counter-reset` property: resets a named counter to a given value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CounterReset {
    pub counter_name: AzString,
    pub value: i32,
}

impl CounterReset {
    #[must_use] pub const fn new(counter_name: AzString, value: i32) -> Self {
        Self {
            counter_name,
            value,
        }
    }

    #[must_use] pub const fn none() -> Self {
        Self {
            counter_name: AzString::from_const_str("none"),
            value: 0,
        }
    }

    #[must_use] pub const fn list_item() -> Self {
        Self {
            counter_name: AzString::from_const_str("list-item"),
            value: 0,
        }
    }
}

impl Default for CounterReset {
    fn default() -> Self {
        Self::none()
    }
}

impl PrintAsCssValue for CounterReset {
    fn print_as_css_value(&self) -> String {
        if self.counter_name.as_str() == "none" {
            "none".to_string()
        } else {
            alloc::format!("{} {}", self.counter_name.as_str(), self.value)
        }
    }
}

/// CSS `counter-increment` property: increments a named counter by a given value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CounterIncrement {
    pub counter_name: AzString,
    pub value: i32,
}

impl CounterIncrement {
    #[must_use] pub const fn new(counter_name: AzString, value: i32) -> Self {
        Self {
            counter_name,
            value,
        }
    }

    #[must_use] pub const fn none() -> Self {
        Self {
            counter_name: AzString::from_const_str("none"),
            value: 0,
        }
    }

    #[must_use] pub const fn list_item() -> Self {
        Self {
            counter_name: AzString::from_const_str("list-item"),
            value: 1,
        }
    }
}

impl Default for CounterIncrement {
    fn default() -> Self {
        Self::none()
    }
}

impl PrintAsCssValue for CounterIncrement {
    fn print_as_css_value(&self) -> String {
        if self.counter_name.as_str() == "none" {
            "none".to_string()
        } else {
            alloc::format!("{} {}", self.counter_name.as_str(), self.value)
        }
    }
}

/// CSS `string-set` property value, stored as a raw string.
///
/// **Note:** Currently parsed and stored but not yet consumed by the layout engine.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StringSet {
    pub inner: AzString,
}

impl Default for StringSet {
    fn default() -> Self {
        Self {
            inner: "none".into(),
        }
    }
}

impl PrintAsCssValue for StringSet {
    fn print_as_css_value(&self) -> String {
        self.inner.as_str().to_string()
    }
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for Content {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Content {{ inner: String::from({:?}) }}", self.inner)
    }
}

impl crate::codegen::format::FormatAsRustCode for CounterReset {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        alloc::format!(
            "CounterReset {{ counter_name: AzString::from_const_str({:?}), value: {} }}",
            self.counter_name.as_str(),
            self.value
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for CounterIncrement {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        alloc::format!(
            "CounterIncrement {{ counter_name: AzString::from_const_str({:?}), value: {} }}",
            self.counter_name.as_str(),
            self.value
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for StringSet {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StringSet {{ inner: String::from({:?}) }}", self.inner)
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;

    // Simplified parsers that just take the raw string value.
    pub fn parse_content(input: &str) -> Result<Content, ()> {
        Ok(Content {
            inner: input.trim().into(),
        })
    }

    fn parse_counter_name_value(input: &str, default_value: i32) -> Result<(AzString, i32), ()> {
        let trimmed = input.trim();

        if trimmed == "none" {
            return Ok((AzString::from_const_str("none"), 0));
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.is_empty() {
            return Err(());
        }

        let counter_name = parts[0].into();
        let value = if parts.len() > 1 {
            parts[1].parse::<i32>().map_err(|_| ())?
        } else {
            default_value
        };

        Ok((counter_name, value))
    }

    pub fn parse_counter_reset(input: &str) -> Result<CounterReset, ()> {
        let (counter_name, value) = parse_counter_name_value(input, 0)?;
        Ok(CounterReset::new(counter_name, value))
    }

    pub fn parse_counter_increment(input: &str) -> Result<CounterIncrement, ()> {
        let (counter_name, value) = parse_counter_name_value(input, 1)?;
        Ok(CounterIncrement::new(counter_name, value))
    }

    pub fn parse_string_set(input: &str) -> Result<StringSet, ()> {
        Ok(StringSet {
            inner: input.trim().into(),
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
        assert_eq!(parse_content("'Hello'").unwrap().inner.as_str(), "'Hello'");

        // Test counter-reset parsing
        let reset = parse_counter_reset("page 1").unwrap();
        assert_eq!(reset.counter_name.as_str(), "page");
        assert_eq!(reset.value, 1);

        let reset = parse_counter_reset("list-item 0").unwrap();
        assert_eq!(reset.counter_name.as_str(), "list-item");
        assert_eq!(reset.value, 0);

        let reset = parse_counter_reset("none").unwrap();
        assert_eq!(reset.counter_name.as_str(), "none");

        // Test counter-increment parsing
        let inc = parse_counter_increment("section").unwrap();
        assert_eq!(inc.counter_name.as_str(), "section");
        assert_eq!(inc.value, 1); // Default value

        let inc = parse_counter_increment("list-item 2").unwrap();
        assert_eq!(inc.counter_name.as_str(), "list-item");
        assert_eq!(inc.value, 2);

        assert_eq!(
            parse_string_set("chapter-title content()")
                .unwrap()
                .inner
                .as_str(),
            "chapter-title content()"
        );
    }
}
