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
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `content` value.
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

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `counter-reset` value.
    pub fn parse_counter_reset(input: &str) -> Result<CounterReset, ()> {
        let (counter_name, value) = parse_counter_name_value(input, 0)?;
        Ok(CounterReset::new(counter_name, value))
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `counter-increment` value.
    pub fn parse_counter_increment(input: &str) -> Result<CounterIncrement, ()> {
        let (counter_name, value) = parse_counter_name_value(input, 1)?;
        Ok(CounterIncrement::new(counter_name, value))
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `string-set` value.
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

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    use super::*;
    use crate::codegen::format::FormatAsRustCode;

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    /// Nasty inputs that must never panic in any parser in this module.
    fn hostile_corpus() -> Vec<String> {
        alloc::vec![
            String::new(),
            " ".to_string(),
            "\t\n\r\u{b}\u{c}".to_string(),
            "\u{a0}".to_string(),      // NBSP: Unicode White_Space
            "\u{200b}".to_string(),    // ZWSP: NOT White_Space
            "\0".to_string(),          // embedded NUL
            "\0page\0 1\0".to_string(),
            ";".to_string(),
            "}{".to_string(),
            "counter(section) \". \"".to_string(),
            "-".to_string(),
            "--".to_string(),
            "+".to_string(),
            "e".to_string(),
            "NaN".to_string(),
            "inf".to_string(),
            "-inf".to_string(),
            "\u{1F600}".to_string(),
            "a\u{0301}\u{0301}\u{0301}".to_string(), // stacked combining marks
            "\u{202e}reversed".to_string(),          // RTL override
            "page 1 page 2 page 3".to_string(),
            "page \u{fffd}".to_string(),
            "\"unterminated".to_string(),
            "url(".to_string(),
        ]
    }

    // ---------------------------------------------------------------
    // Constructors: CounterReset::{new,none,list_item}
    //               CounterIncrement::{new,none,list_item}
    // ---------------------------------------------------------------

    #[test]
    fn counter_constructors_preserve_fields_at_i32_extremes() {
        for value in [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX] {
            let r = CounterReset::new(AzString::from_const_str("c"), value);
            assert_eq!(r.counter_name.as_str(), "c");
            assert_eq!(r.value, value);

            let i = CounterIncrement::new(AzString::from_const_str("c"), value);
            assert_eq!(i.counter_name.as_str(), "c");
            assert_eq!(i.value, value);
        }
    }

    #[test]
    fn counter_constructors_accept_degenerate_names_verbatim() {
        // Empty, whitespace-laden, unicode and very long names are stored as-is:
        // the constructors perform no validation whatsoever.
        let huge: String = "x".repeat(100_000);
        let names = ["", " ", "a b", "\u{1F600}\u{1F600}", "none", huge.as_str()];

        for name in names {
            let r = CounterReset::new(name.into(), i32::MIN);
            assert_eq!(r.counter_name.as_str(), name);
            assert_eq!(r.counter_name.as_str().len(), name.len());
            assert_eq!(r.value, i32::MIN);

            let i = CounterIncrement::new(name.into(), i32::MAX);
            assert_eq!(i.counter_name.as_str(), name);
            assert_eq!(i.counter_name.as_str().len(), name.len());
            assert_eq!(i.value, i32::MAX);
        }
    }

    #[test]
    fn counter_none_and_list_item_constants() {
        assert_eq!(CounterReset::none().counter_name.as_str(), "none");
        assert_eq!(CounterReset::none().value, 0);
        assert_eq!(CounterReset::default(), CounterReset::none());

        assert_eq!(CounterIncrement::none().counter_name.as_str(), "none");
        assert_eq!(CounterIncrement::none().value, 0);
        assert_eq!(CounterIncrement::default(), CounterIncrement::none());

        // Per CSS, `counter-reset: list-item` starts at 0 while
        // `counter-increment: list-item` steps by 1 -- the asymmetry is intended.
        assert_eq!(CounterReset::list_item().counter_name.as_str(), "list-item");
        assert_eq!(CounterReset::list_item().value, 0);
        assert_eq!(
            CounterIncrement::list_item().counter_name.as_str(),
            "list-item"
        );
        assert_eq!(CounterIncrement::list_item().value, 1);
    }

    #[test]
    fn content_and_string_set_defaults() {
        assert_eq!(Content::default().inner.as_str(), "normal");
        assert_eq!(StringSet::default().inner.as_str(), "none");
    }

    // ---------------------------------------------------------------
    // parse_content / parse_string_set  (raw passthrough parsers)
    // ---------------------------------------------------------------

    #[test]
    fn content_and_string_set_are_pure_trim_and_never_error() {
        // NOTE: both parsers are infallible despite their `# Errors` docs --
        // they accept empty input and arbitrary garbage. `str::trim` is the
        // exact oracle, which also means Unicode whitespace (NBSP) is stripped
        // while ZWSP is not.
        for input in hostile_corpus() {
            let c = parse_content(&input).expect("parse_content never errors");
            assert_eq!(c.inner.as_str(), input.trim());

            let s = parse_string_set(&input).expect("parse_string_set never errors");
            assert_eq!(s.inner.as_str(), input.trim());
        }

        assert_eq!(parse_content("").unwrap().inner.as_str(), "");
        assert_eq!(parse_content("   \t\n  ").unwrap().inner.as_str(), "");
        assert_eq!(parse_string_set("").unwrap().inner.as_str(), "");
        // NBSP is Unicode White_Space, so it is trimmed away entirely:
        assert_eq!(parse_content("\u{a0}x\u{a0}").unwrap().inner.as_str(), "x");
        // ZWSP is not, so it survives as content:
        assert_eq!(
            parse_content(" \u{200b} ").unwrap().inner.as_str(),
            "\u{200b}"
        );
    }

    #[test]
    fn content_positive_control_and_inner_junk_is_kept() {
        assert_eq!(parse_content("'Hi'").unwrap().inner.as_str(), "'Hi'");
        assert_eq!(parse_content("  'Hi'  ").unwrap().inner.as_str(), "'Hi'");
        // Trailing junk is *not* rejected, only outer whitespace is stripped:
        assert_eq!(
            parse_content("'Hi';garbage").unwrap().inner.as_str(),
            "'Hi';garbage"
        );
        assert_eq!(
            parse_string_set("chapter content()").unwrap().inner.as_str(),
            "chapter content()"
        );
    }

    #[test]
    fn content_extremely_long_input_does_not_hang() {
        let huge = "a".repeat(1_000_000);
        let padded = alloc::format!("   {huge}\n");

        let c = parse_content(&padded).unwrap();
        assert_eq!(c.inner.as_str().len(), 1_000_000);
        assert!(c.inner.as_str().starts_with("aa"));

        let s = parse_string_set(&padded).unwrap();
        assert_eq!(s.inner.as_str().len(), 1_000_000);
    }

    #[test]
    fn content_deeply_nested_input_does_not_stack_overflow() {
        // The parser is non-recursive, so 10k nested brackets are just bytes.
        let nested = alloc::format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        let c = parse_content(&nested).unwrap();
        assert_eq!(c.inner.as_str().len(), 20_000);

        let deep = alloc::format!("{}x{}", "counter(".repeat(5_000), ")".repeat(5_000));
        assert!(parse_content(&deep).is_ok());
        assert!(parse_string_set(&deep).is_ok());
    }

    #[test]
    fn content_preserves_multibyte_unicode_byte_for_byte() {
        let input = "  \u{1F600}a\u{0301}\u{4e2d}\u{6587}  ";
        let c = parse_content(input).unwrap();
        assert_eq!(c.inner.as_str(), input.trim());
        assert_eq!(c.inner.as_str().len(), input.trim().len());
        // 5 scalar values, not 4 glyphs: U+0301 COMBINING ACUTE ACCENT is its own
        // `char` (it renders as one grapheme with the preceding 'a', but `chars()`
        // counts scalars).
        assert_eq!(c.inner.as_str().chars().count(), 5);
    }

    #[test]
    fn content_boundary_number_strings_are_accepted_as_plain_text() {
        // `content` has no numeric grammar here: numbers survive verbatim.
        for n in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "1e309",
            "NaN",
            "inf",
        ] {
            assert_eq!(parse_content(n).unwrap().inner.as_str(), n);
            assert_eq!(parse_string_set(n).unwrap().inner.as_str(), n);
        }
    }

    #[test]
    fn content_print_parse_roundtrips_for_parsed_values() {
        for input in hostile_corpus() {
            let c = parse_content(&input).unwrap();
            let reparsed = parse_content(&c.print_as_css_value()).unwrap();
            assert_eq!(reparsed, c, "content round-trip failed for {input:?}");

            let s = parse_string_set(&input).unwrap();
            let re_s = parse_string_set(&s.print_as_css_value()).unwrap();
            assert_eq!(re_s, s, "string-set round-trip failed for {input:?}");
        }

        let d = Content::default();
        assert_eq!(parse_content(&d.print_as_css_value()).unwrap(), d);
        let d = StringSet::default();
        assert_eq!(parse_string_set(&d.print_as_css_value()).unwrap(), d);
    }

    #[test]
    fn content_roundtrip_is_lossy_for_untrimmed_handbuilt_values() {
        // print -> parse is only idempotent for already-trimmed values; a value
        // built directly (not via the parser) loses its padding on re-parse.
        let padded = Content {
            inner: "  x  ".into(),
        };
        assert_eq!(padded.print_as_css_value(), "  x  ");
        assert_ne!(parse_content(&padded.print_as_css_value()).unwrap(), padded);
        assert_eq!(
            parse_content(&padded.print_as_css_value())
                .unwrap()
                .inner
                .as_str(),
            "x"
        );
    }

    // ---------------------------------------------------------------
    // parse_counter_reset / parse_counter_increment
    // (these fully exercise the private `parse_counter_name_value`:
    //  default_value = 0 for reset, 1 for increment)
    // ---------------------------------------------------------------

    #[test]
    fn counter_empty_and_whitespace_only_input_is_rejected() {
        for input in ["", " ", "   ", "\t\n", "\r\n\t ", "\u{a0}", "\u{2003}"] {
            assert!(
                parse_counter_reset(input).is_err(),
                "counter-reset accepted whitespace-only {input:?}"
            );
            assert!(
                parse_counter_increment(input).is_err(),
                "counter-increment accepted whitespace-only {input:?}"
            );
        }
    }

    #[test]
    fn counter_missing_value_uses_per_property_default() {
        let r = parse_counter_reset("section").unwrap();
        assert_eq!(r.counter_name.as_str(), "section");
        assert_eq!(r.value, 0);

        let i = parse_counter_increment("section").unwrap();
        assert_eq!(i.counter_name.as_str(), "section");
        assert_eq!(i.value, 1);
    }

    #[test]
    fn counter_none_keyword_is_case_sensitive() {
        // CSS keywords are ASCII case-insensitive, but only lowercase `none`
        // hits the keyword branch. `NONE` is treated as a *counter name*.
        let r = parse_counter_reset("NONE").unwrap();
        assert_eq!(r.counter_name.as_str(), "NONE");
        assert_eq!(r.value, 0);

        let i = parse_counter_increment("None").unwrap();
        assert_eq!(i.counter_name.as_str(), "None");
        assert_eq!(i.value, 1, "uppercase `None` took the counter-name branch");

        // The lowercase keyword branch ignores the property default entirely.
        assert_eq!(parse_counter_increment("none").unwrap().value, 0);
        assert_eq!(parse_counter_increment("  none  ").unwrap().value, 0);
    }

    #[test]
    fn counter_none_with_value_keeps_value_but_prints_as_bare_none() {
        // "none 5" misses the keyword fast-path (it is not *exactly* "none"),
        // so it parses as a counter literally named "none" with value 5 --
        // but PrintAsCssValue then drops the 5, so print->parse is lossy.
        let r = parse_counter_reset("none 5").unwrap();
        assert_eq!(r.counter_name.as_str(), "none");
        assert_eq!(r.value, 5);
        assert_eq!(r.print_as_css_value(), "none");

        let reparsed = parse_counter_reset(&r.print_as_css_value()).unwrap();
        assert_eq!(reparsed.value, 0);
        assert_ne!(reparsed, r, "value 5 silently vanished across a round-trip");
    }

    #[test]
    fn counter_value_at_i32_boundaries_parses_exactly() {
        assert_eq!(parse_counter_reset("c 2147483647").unwrap().value, i32::MAX);
        assert_eq!(parse_counter_reset("c -2147483648").unwrap().value, i32::MIN);
        assert_eq!(parse_counter_reset("c 0").unwrap().value, 0);
        assert_eq!(parse_counter_reset("c -0").unwrap().value, 0);
        assert_eq!(parse_counter_reset("c +7").unwrap().value, 7);
        assert_eq!(parse_counter_reset("c 007").unwrap().value, 7);
        assert_eq!(
            parse_counter_increment("c -2147483648").unwrap().value,
            i32::MIN
        );
    }

    #[test]
    fn counter_value_overflowing_i32_is_rejected_not_wrapped() {
        for over in [
            "c 2147483648",           // i32::MAX + 1
            "c -2147483649",          // i32::MIN - 1
            "c 9223372036854775807",  // i64::MAX
            "c -9223372036854775808", // i64::MIN
            "c 340282366920938463463374607431768211456",
        ] {
            assert!(
                parse_counter_reset(over).is_err(),
                "overflowing value silently accepted: {over:?}"
            );
            assert!(parse_counter_increment(over).is_err());
        }
    }

    #[test]
    fn counter_non_integer_values_are_rejected() {
        for bad in [
            "c 1.0", "c 1.5", "c 1e3", "c NaN", "c nan", "c inf", "c -inf", "c 0x10", "c 1_000",
            "c 1,", "c one", "c -", "c +", "c ٣", // Arabic-Indic digit three
            "c １",  // fullwidth digit one
            "c 1\u{200b}",
        ] {
            assert!(
                parse_counter_reset(bad).is_err(),
                "counter-reset accepted non-integer {bad:?}"
            );
            assert!(
                parse_counter_increment(bad).is_err(),
                "counter-increment accepted non-integer {bad:?}"
            );
        }
    }

    #[test]
    fn counter_extra_tokens_after_the_first_pair_are_silently_dropped() {
        // CSS allows a *list* of counters; this parser keeps only the first
        // name/value pair and discards the rest without erroring.
        let r = parse_counter_reset("a 1 b 2 c 3").unwrap();
        assert_eq!(r.counter_name.as_str(), "a");
        assert_eq!(r.value, 1);

        // ...the parse errors instead of falling back to a default when the
        // second token is not an integer.
        parse_counter_increment("a b").unwrap_err();
    }

    #[test]
    fn counter_arbitrary_whitespace_forms_are_normalized() {
        for input in [
            "\t page \n 42 \r",
            "page\u{a0}42",   // NBSP separates under split_whitespace
            "page    42",     // runs of spaces
            "\u{2003}page 42", // em-space
        ] {
            let r = parse_counter_reset(input).unwrap();
            assert_eq!(r.counter_name.as_str(), "page", "for {input:?}");
            assert_eq!(r.value, 42, "for {input:?}");
        }
    }

    #[test]
    fn counter_unicode_names_are_preserved() {
        let r = parse_counter_reset("\u{7ae0}\u{8282} 3").unwrap();
        assert_eq!(r.counter_name.as_str(), "\u{7ae0}\u{8282}");
        assert_eq!(r.value, 3);

        let i = parse_counter_increment("\u{1F600}").unwrap();
        assert_eq!(i.counter_name.as_str(), "\u{1F600}");
        assert_eq!(i.value, 1);

        // A zero-width space is not whitespace: it becomes a counter name.
        let z = parse_counter_reset("\u{200b}").unwrap();
        assert_eq!(z.counter_name.as_str(), "\u{200b}");
        assert_eq!(z.value, 0);
    }

    #[test]
    fn counter_hostile_corpus_never_panics() {
        for input in hostile_corpus() {
            let _ = parse_counter_reset(&input);
            let _ = parse_counter_increment(&input);
        }
    }

    #[test]
    fn counter_extremely_long_input_does_not_hang() {
        let long_name = "n".repeat(1_000_000);
        let r = parse_counter_reset(&long_name).unwrap();
        assert_eq!(r.counter_name.as_str().len(), 1_000_000);
        assert_eq!(r.value, 0);

        let with_value = alloc::format!("{long_name} 5");
        assert_eq!(parse_counter_increment(&with_value).unwrap().value, 5);

        // A million digits must be rejected, not truncated or wrapped.
        let long_number = alloc::format!("c {}", "9".repeat(1_000_000));
        assert!(parse_counter_reset(&long_number).is_err());

        // Whitespace-only input of the same size is still just an error.
        assert!(parse_counter_reset(&" ".repeat(1_000_000)).is_err());
    }

    #[test]
    fn counter_deeply_nested_input_does_not_stack_overflow() {
        let nested = alloc::format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        let r = parse_counter_reset(&nested).unwrap();
        assert_eq!(r.counter_name.as_str().len(), 20_000);
        assert_eq!(r.value, 0);

        let nested_with_value = alloc::format!("{nested} 1");
        assert_eq!(parse_counter_increment(&nested_with_value).unwrap().value, 1);
    }

    // ---------------------------------------------------------------
    // PrintAsCssValue <-> parser round-trips
    // ---------------------------------------------------------------

    #[test]
    fn counter_print_parse_roundtrips_for_well_formed_values() {
        for (name, value) in [
            ("page", 0),
            ("page", 1),
            ("section", -1),
            ("list-item", i32::MAX),
            ("list-item", i32::MIN),
            ("\u{7ae0}", 7),
        ] {
            let r = CounterReset::new(name.into(), value);
            assert_eq!(parse_counter_reset(&r.print_as_css_value()).unwrap(), r);

            let i = CounterIncrement::new(name.into(), value);
            assert_eq!(parse_counter_increment(&i.print_as_css_value()).unwrap(), i);
        }

        // `none`/`list_item` constants also survive a full round-trip.
        let n = CounterReset::none();
        assert_eq!(n.print_as_css_value(), "none");
        assert_eq!(parse_counter_reset(&n.print_as_css_value()).unwrap(), n);

        let li = CounterIncrement::list_item();
        assert_eq!(li.print_as_css_value(), "list-item 1");
        assert_eq!(parse_counter_increment(&li.print_as_css_value()).unwrap(), li);

        assert_eq!(CounterReset::list_item().print_as_css_value(), "list-item 0");
        assert_eq!(CounterIncrement::none().print_as_css_value(), "none");
    }

    #[test]
    fn counter_print_of_empty_name_reparses_into_a_different_counter() {
        // An empty name prints as " 5"; re-parsing reads "5" as the *name*
        // and falls back to the default value -- a silent identity change.
        let r = CounterReset::new(AzString::from_const_str(""), 5);
        assert_eq!(r.print_as_css_value(), " 5");

        let reparsed = parse_counter_reset(&r.print_as_css_value()).unwrap();
        assert_eq!(reparsed.counter_name.as_str(), "5");
        assert_eq!(reparsed.value, 0);
        assert_ne!(reparsed, r);
    }

    #[test]
    fn counter_print_of_name_containing_space_fails_to_reparse() {
        // "a b" + " 5" prints as "a b 5"; the second token "b" is not an i32,
        // so the printed form is no longer parseable at all.
        let r = CounterReset::new("a b".into(), 5);
        assert_eq!(r.print_as_css_value(), "a b 5");
        assert!(parse_counter_reset(&r.print_as_css_value()).is_err());

        let i = CounterIncrement::new("a b".into(), 5);
        assert!(parse_counter_increment(&i.print_as_css_value()).is_err());
    }

    // ---------------------------------------------------------------
    // Derived-trait invariants (Eq / Ord / Hash) and codegen formatting
    // ---------------------------------------------------------------

    #[test]
    fn counter_ord_is_name_then_value_and_hash_agrees_with_eq() {
        let a1 = CounterReset::new("a".into(), 1);
        let a2 = CounterReset::new("a".into(), 2);
        let b_min = CounterReset::new("b".into(), i32::MIN);

        assert!(a1 < a2, "equal names must order by value");
        assert!(a2 < b_min, "name must dominate value in the ordering");
        assert!(CounterReset::new("a".into(), i32::MAX) < b_min);

        // Eq/Hash consistency, including across differently-allocated names.
        let owned = CounterReset::new(String::from("a").into(), 1);
        assert_eq!(owned, a1);
        assert_eq!(hash_of(&owned), hash_of(&a1));
        assert_ne!(a1, a2);

        assert_eq!(
            hash_of(&Content {
                inner: "x".into()
            }),
            hash_of(&parse_content(" x ").unwrap())
        );
    }

    #[test]
    fn format_as_rust_code_escapes_quotes_and_control_chars() {
        let c = Content {
            inner: "a\"b\\c\nd".into(),
        };
        assert_eq!(
            c.format_as_rust_code(0),
            r#"Content { inner: String::from("a\"b\\c\nd") }"#
        );

        let s = StringSet {
            inner: "\"q\"".into(),
        };
        assert_eq!(
            s.format_as_rust_code(0),
            r#"StringSet { inner: String::from("\"q\"") }"#
        );

        let r = CounterReset::new("a\"b".into(), i32::MIN);
        assert_eq!(
            r.format_as_rust_code(0),
            r#"CounterReset { counter_name: AzString::from_const_str("a\"b"), value: -2147483648 }"#
        );

        let i = CounterIncrement::new("".into(), i32::MAX);
        assert_eq!(
            i.format_as_rust_code(0),
            r#"CounterIncrement { counter_name: AzString::from_const_str(""), value: 2147483647 }"#
        );
    }

    #[test]
    fn format_as_rust_code_ignores_the_tab_argument() {
        let c = Content::default();
        assert_eq!(c.format_as_rust_code(0), c.format_as_rust_code(usize::MAX));

        let r = CounterReset::list_item();
        assert_eq!(r.format_as_rust_code(0), r.format_as_rust_code(usize::MAX));
    }
}
