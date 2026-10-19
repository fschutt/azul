//! CSS properties for generated content (`content`, `counter-reset`,
//! `counter-increment`, `counter-set`, `string-set`).
//!
//! Defines [`Content`], [`CounterReset`], [`CounterIncrement`],
//! [`CounterSet`] (each a list of [`CounterEntry`]) and [`StringSet`], which
//! are registered as [`CssProperty`] variants.

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

/// One counter of a `counter-reset`, `counter-increment` or `counter-set`
/// list: its name and the value it is reset to, stepped by or set to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CounterEntry {
    pub counter_name: AzString,
    pub value: i32,
}

impl CounterEntry {
    #[must_use]
    pub const fn new(counter_name: AzString, value: i32) -> Self {
        Self {
            counter_name,
            value,
        }
    }
}

impl_option!(
    CounterEntry,
    OptionCounterEntry,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(
    CounterEntry,
    CounterEntryVec,
    CounterEntryVecDestructor,
    CounterEntryVecDestructorType,
    CounterEntryVecSlice,
    OptionCounterEntry
);
impl_vec_debug!(CounterEntry, CounterEntryVec);
impl_vec_partialord!(CounterEntry, CounterEntryVec);
impl_vec_ord!(CounterEntry, CounterEntryVec);
impl_vec_clone!(CounterEntry, CounterEntryVec, CounterEntryVecDestructor);
impl_vec_partialeq!(CounterEntry, CounterEntryVec);
impl_vec_eq!(CounterEntry, CounterEntryVec);
impl_vec_hash!(CounterEntry, CounterEntryVec);

/// `none` for no counters, else every counter with its value, in order.
impl PrintAsCssValue for CounterEntryVec {
    fn print_as_css_value(&self) -> String {
        if self.is_empty() {
            return "none".to_string();
        }
        self.iter()
            .map(|c| alloc::format!("{} {}", c.counter_name.as_str(), c.value))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The counters the `list-item` constants name.
const LIST_ITEM_AT_ZERO: &[CounterEntry] =
    &[CounterEntry::new(AzString::from_const_str("list-item"), 0)];
const LIST_ITEM_BY_ONE: &[CounterEntry] =
    &[CounterEntry::new(AzString::from_const_str("list-item"), 1)];

/// CSS `counter-reset` (CSS Lists 3 s4.2): the counters an element makes,
/// each with its start value. Empty is `none`. A counter named twice takes
/// its last value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CounterReset {
    pub counters: CounterEntryVec,
}

/// CSS `counter-increment` (CSS Lists 3 s4.3): the counters an element
/// steps, each by its value. Empty is `none`. A counter named twice steps
/// once per name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CounterIncrement {
    pub counters: CounterEntryVec,
}

/// CSS `counter-set` (CSS Lists 3 s4.3): the counters an element gives a
/// value, after its resets and increments, without making new ones. Empty is
/// `none`. A counter named twice takes its last value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CounterSet {
    pub counters: CounterEntryVec,
}

macro_rules! impl_counter_list {
    ($name:ident, $list_item:expr) => {
        impl $name {
            /// One counter with its value.
            #[must_use]
            pub fn new(counter_name: AzString, value: i32) -> Self {
                Self {
                    counters: CounterEntryVec::from_vec(alloc::vec![CounterEntry::new(
                        counter_name,
                        value
                    )]),
                }
            }

            #[must_use]
            pub const fn from_counters(counters: CounterEntryVec) -> Self {
                Self { counters }
            }

            /// `none`: no counters.
            #[must_use]
            pub const fn none() -> Self {
                Self {
                    counters: CounterEntryVec::new(),
                }
            }

            /// The `list-item` counter alone, as the UA style sheet names it.
            #[must_use]
            pub const fn list_item() -> Self {
                Self {
                    counters: CounterEntryVec::from_const_slice($list_item),
                }
            }

            #[must_use]
            pub const fn is_none(&self) -> bool {
                self.counters.is_empty()
            }

            /// Whether the list names the counter `name`.
            #[must_use]
            pub fn names(&self, name: &str) -> bool {
                self.counters.iter().any(|c| c.counter_name.as_str() == name)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::none()
            }
        }

        impl PrintAsCssValue for $name {
            fn print_as_css_value(&self) -> String {
                self.counters.print_as_css_value()
            }
        }
    };
}

impl_counter_list!(CounterReset, LIST_ITEM_AT_ZERO);
impl_counter_list!(CounterIncrement, LIST_ITEM_BY_ONE);
impl_counter_list!(CounterSet, LIST_ITEM_AT_ZERO);

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
#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for Content {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Content {{ inner: String::from({:?}) }}", self.inner)
    }
}

#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for CounterEntry {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        alloc::format!(
            "CounterEntry {{ counter_name: AzString::from_const_str({:?}), value: {} }}",
            self.counter_name.as_str(),
            self.value
        )
    }
}

// The entries are emitted once as a const slice (see `VecContents`), which
// the vec then borrows, as every vec of a compiled style sheet does.
#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for CounterEntryVec {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use crate::hash::GetHash;
        if self.is_empty() {
            return "CounterEntryVec::from_const_slice(&[])".to_string();
        }
        alloc::format!(
            "CounterEntryVec::from_const_slice(COUNTER_ENTRY_{}_ITEMS)",
            self.get_hash()
        )
    }
}

#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for CounterReset {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        alloc::format!(
            "CounterReset {{ counters: {} }}",
            self.counters.format_as_rust_code(tabs)
        )
    }
}

#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for CounterIncrement {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        alloc::format!(
            "CounterIncrement {{ counters: {} }}",
            self.counters.format_as_rust_code(tabs)
        )
    }
}

#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for CounterSet {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        alloc::format!(
            "CounterSet {{ counters: {} }}",
            self.counters.format_as_rust_code(tabs)
        )
    }
}

#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for StringSet {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StringSet {{ inner: String::from({:?}) }}", self.inner)
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)]
    // parser submodule reuses the parent module's value types
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

    /// `none | [ <counter-name> <integer>? ]+` (CSS Lists 3 s4.2), every
    /// counter without a value taking `default_value`: the counters in order,
    /// none for `none`. A name is an identifier other than `none` that does
    /// not start with a digit, so a number where a name belongs, a value out
    /// of the integer range and an empty list are errors.
    fn parse_counter_list(input: &str, default_value: i32) -> Result<CounterEntryVec, ()> {
        let trimmed = input.trim();
        if trimmed.eq_ignore_ascii_case("none") {
            return Ok(CounterEntryVec::new());
        }
        let mut counters = Vec::new();
        let mut words = trimmed.split_whitespace().peekable();
        while let Some(name) = words.next() {
            if !is_counter_name(name) {
                return Err(());
            }
            let value = match words.peek() {
                Some(word) if looks_like_integer(word) => {
                    let value = word.parse::<i32>().map_err(|_| ())?;
                    words.next();
                    value
                }
                _ => default_value,
            };
            counters.push(CounterEntry::new(name.into(), value));
        }
        if counters.is_empty() {
            return Err(());
        }
        Ok(CounterEntryVec::from_vec(counters))
    }

    /// A word that is meant as a number: a digit, or a sign and a digit.
    fn looks_like_integer(word: &str) -> bool {
        let digits = word.strip_prefix(['+', '-']).unwrap_or(word);
        digits.starts_with(|c: char| c.is_ascii_digit())
    }

    /// A `<custom-ident>` that may name a counter: not `none` (in any case)
    /// and not a number.
    fn is_counter_name(word: &str) -> bool {
        !word.eq_ignore_ascii_case("none") && !looks_like_integer(word) && word != "+" && word != "-"
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `counter-reset` value.
    pub fn parse_counter_reset(input: &str) -> Result<CounterReset, ()> {
        parse_counter_list(input, 0).map(CounterReset::from_counters)
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `counter-increment` value.
    pub fn parse_counter_increment(input: &str) -> Result<CounterIncrement, ()> {
        parse_counter_list(input, 1).map(CounterIncrement::from_counters)
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `counter-set` value.
    pub fn parse_counter_set(input: &str) -> Result<CounterSet, ()> {
        parse_counter_list(input, 0).map(CounterSet::from_counters)
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
fn names_and_values(counters: &CounterEntryVec) -> Vec<(&str, i32)> {
    counters
        .iter()
        .map(|c| (c.counter_name.as_str(), c.value))
        .collect()
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_simple_content_parser() {
        assert_eq!(parse_content("'Hello'").unwrap().inner.as_str(), "'Hello'");

        // counter-reset, counter-increment and counter-set take a list.
        let reset = parse_counter_reset("page 1").unwrap();
        assert_eq!(reset, CounterReset::new("page".into(), 1));
        assert_eq!(parse_counter_reset("list-item 0").unwrap(), CounterReset::list_item());
        assert!(parse_counter_reset("none").unwrap().is_none());

        let inc = parse_counter_increment("section").unwrap();
        assert_eq!(inc, CounterIncrement::new("section".into(), 1));
        let inc = parse_counter_increment("list-item 2").unwrap();
        assert_eq!(inc, CounterIncrement::new("list-item".into(), 2));

        let set = parse_counter_set("a 3 b").unwrap();
        assert_eq!(names_and_values(&set.counters), [("a", 3), ("b", 0)]);

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
    #[cfg(feature = "codegen")]
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
            "\u{a0}".to_string(),   // NBSP: Unicode White_Space
            "\u{200b}".to_string(), // ZWSP: NOT White_Space
            "\0".to_string(),       // embedded NUL
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
    // Constructors: CounterReset / CounterIncrement / CounterSet ::{new,none,list_item}
    // ---------------------------------------------------------------

    #[test]
    fn counter_constructors_preserve_fields_at_i32_extremes() {
        for value in [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX] {
            let r = CounterReset::new(AzString::from_const_str("c"), value);
            assert_eq!(names_and_values(&r.counters), [("c", value)]);

            let i = CounterIncrement::new(AzString::from_const_str("c"), value);
            assert_eq!(names_and_values(&i.counters), [("c", value)]);

            let s = CounterSet::new(AzString::from_const_str("c"), value);
            assert_eq!(names_and_values(&s.counters), [("c", value)]);
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
            assert_eq!(names_and_values(&r.counters), [(name, i32::MIN)]);

            let i = CounterIncrement::new(name.into(), i32::MAX);
            assert_eq!(names_and_values(&i.counters), [(name, i32::MAX)]);
        }
    }

    #[test]
    fn counter_none_and_list_item_constants() {
        assert!(CounterReset::none().is_none());
        assert_eq!(CounterReset::default(), CounterReset::none());
        assert!(CounterIncrement::none().is_none());
        assert_eq!(CounterIncrement::default(), CounterIncrement::none());
        assert!(CounterSet::none().is_none());
        assert_eq!(CounterSet::default(), CounterSet::none());

        // Per CSS, `counter-reset: list-item` starts at 0 while
        // `counter-increment: list-item` steps by 1 -- the asymmetry is intended.
        assert_eq!(names_and_values(&CounterReset::list_item().counters), [("list-item", 0)]);
        assert_eq!(
            names_and_values(&CounterIncrement::list_item().counters),
            [("list-item", 1)]
        );
        assert!(CounterReset::list_item().names("list-item"));
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
            parse_string_set("chapter content()")
                .unwrap()
                .inner
                .as_str(),
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
    // parse_counter_reset / parse_counter_increment / parse_counter_set
    // (these fully exercise the private `parse_counter_list`:
    //  default_value = 0 for reset and set, 1 for increment)
    // ---------------------------------------------------------------

    fn single(counters: &CounterEntryVec) -> (&str, i32) {
        assert_eq!(counters.len(), 1, "one counter expected in {counters:?}");
        let c = &counters.as_slice()[0];
        (c.counter_name.as_str(), c.value)
    }

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
            assert!(
                parse_counter_set(input).is_err(),
                "counter-set accepted whitespace-only {input:?}"
            );
        }
    }

    #[test]
    fn counter_missing_value_uses_per_property_default() {
        assert_eq!(single(&parse_counter_reset("section").unwrap().counters), ("section", 0));
        assert_eq!(
            single(&parse_counter_increment("section").unwrap().counters),
            ("section", 1)
        );
        assert_eq!(single(&parse_counter_set("section").unwrap().counters), ("section", 0));
    }

    #[test]
    fn counter_none_keyword_is_ascii_case_insensitive() {
        // CSS keywords are ASCII case-insensitive: every spelling of `none`
        // is the keyword, an empty list.
        for none in ["none", "NONE", "None", "  none  "] {
            assert!(parse_counter_reset(none).unwrap().is_none(), "{none:?}");
            assert!(parse_counter_increment(none).unwrap().is_none(), "{none:?}");
            assert!(parse_counter_set(none).unwrap().is_none(), "{none:?}");
        }
    }

    #[test]
    fn counter_none_inside_a_list_is_not_a_counter_name() {
        // `none` is excluded from <counter-name>, so a list naming it is
        // invalid and the declaration is dropped.
        for bad in ["none 5", "a none", "a 1 NONE 2"] {
            assert!(parse_counter_reset(bad).is_err(), "{bad:?}");
            assert!(parse_counter_increment(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn counter_value_at_i32_boundaries_parses_exactly() {
        let value = |input: &str| single(&parse_counter_reset(input).unwrap().counters).1;
        assert_eq!(value("c 2147483647"), i32::MAX);
        assert_eq!(value("c -2147483648"), i32::MIN);
        assert_eq!(value("c 0"), 0);
        assert_eq!(value("c -0"), 0);
        assert_eq!(value("c +7"), 7);
        assert_eq!(value("c 007"), 7);
        assert_eq!(
            single(&parse_counter_increment("c -2147483648").unwrap().counters).1,
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
            assert!(parse_counter_set(over).is_err());
        }
    }

    #[test]
    fn counter_number_like_values_that_are_not_integers_are_rejected() {
        // A word that starts like a number stands where the value goes; one
        // that is not an integer makes the whole list invalid.
        for bad in [
            "c 1.0", "c 1.5", "c 1e3", "c 0x10", "c 1_000", "c 1,", "c -", "c +", "c 1\u{200b}",
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
    fn counter_a_word_after_a_name_is_the_next_counter() {
        // `c one` is two counters, each with the default value; so is a word
        // in another script's digits, which is no CSS integer.
        let names = |input: &str| {
            names_and_values(&parse_counter_increment(input).unwrap().counters)
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect::<Vec<_>>()
        };
        assert_eq!(names("c one"), [("c".to_string(), 1), ("one".to_string(), 1)]);
        assert_eq!(names("c NaN"), [("c".to_string(), 1), ("NaN".to_string(), 1)]);
        assert_eq!(names("c ٣"), [("c".to_string(), 1), ("٣".to_string(), 1)]);
    }

    #[test]
    fn counter_a_number_where_a_name_belongs_is_rejected() {
        for bad in ["5", "-3", "+1", "a 1 2", "1a"] {
            assert!(parse_counter_reset(bad).is_err(), "{bad:?}");
            assert!(parse_counter_increment(bad).is_err(), "{bad:?}");
            assert!(parse_counter_set(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn counter_every_name_value_pair_of_a_list_is_kept_in_order() {
        let r = parse_counter_reset("a 1 b 2 c 3").unwrap();
        assert_eq!(names_and_values(&r.counters), [("a", 1), ("b", 2), ("c", 3)]);

        // A name without a value takes the property's default, wherever it
        // stands in the list.
        let i = parse_counter_increment("a b 5 c").unwrap();
        assert_eq!(names_and_values(&i.counters), [("a", 1), ("b", 5), ("c", 1)]);

        // A repeated name is kept as written; the counting decides what it
        // means (the last value for a reset or set, every step for an
        // increment).
        let twice = parse_counter_increment("list-item 3 list-item -1").unwrap();
        assert_eq!(names_and_values(&twice.counters), [("list-item", 3), ("list-item", -1)]);
        assert!(twice.names("list-item"));
        assert!(!twice.names("other"));
    }

    #[test]
    fn counter_arbitrary_whitespace_forms_are_normalized() {
        for input in [
            "\t page \n 42 \r",
            "page\u{a0}42",    // NBSP separates under split_whitespace
            "page    42",      // runs of spaces
            "\u{2003}page 42", // em-space
        ] {
            let r = parse_counter_reset(input).unwrap();
            assert_eq!(single(&r.counters), ("page", 42), "for {input:?}");
        }
    }

    #[test]
    fn counter_unicode_names_are_preserved() {
        let r = parse_counter_reset("\u{7ae0}\u{8282} 3").unwrap();
        assert_eq!(single(&r.counters), ("\u{7ae0}\u{8282}", 3));

        let i = parse_counter_increment("\u{1F600}").unwrap();
        assert_eq!(single(&i.counters), ("\u{1F600}", 1));

        // A zero-width space is not whitespace: it becomes a counter name.
        let z = parse_counter_reset("\u{200b}").unwrap();
        assert_eq!(single(&z.counters), ("\u{200b}", 0));
    }

    #[test]
    fn counter_hostile_corpus_never_panics() {
        for input in hostile_corpus() {
            let _ = parse_counter_reset(&input);
            let _ = parse_counter_increment(&input);
            let _ = parse_counter_set(&input);
        }
    }

    #[test]
    fn counter_extremely_long_input_does_not_hang() {
        let long_name = "n".repeat(1_000_000);
        let r = parse_counter_reset(&long_name).unwrap();
        assert_eq!(single(&r.counters).0.len(), 1_000_000);
        assert_eq!(single(&r.counters).1, 0);

        let with_value = alloc::format!("{long_name} 5");
        assert_eq!(single(&parse_counter_increment(&with_value).unwrap().counters).1, 5);

        // A million digits must be rejected, not truncated or wrapped.
        let long_number = alloc::format!("c {}", "9".repeat(1_000_000));
        assert!(parse_counter_reset(&long_number).is_err());

        // Whitespace-only input of the same size is still just an error.
        assert!(parse_counter_reset(&" ".repeat(1_000_000)).is_err());

        // A long list stays linear.
        let many = "c 1 ".repeat(100_000);
        assert_eq!(parse_counter_increment(&many).unwrap().counters.len(), 100_000);
    }

    #[test]
    fn counter_deeply_nested_input_does_not_stack_overflow() {
        let nested = alloc::format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        let r = parse_counter_reset(&nested).unwrap();
        assert_eq!(single(&r.counters).0.len(), 20_000);

        let nested_with_value = alloc::format!("{nested} 1");
        assert_eq!(
            single(&parse_counter_increment(&nested_with_value).unwrap().counters).1,
            1
        );
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

            let s = CounterSet::new(name.into(), value);
            assert_eq!(parse_counter_set(&s.print_as_css_value()).unwrap(), s);
        }

        let list = parse_counter_reset("a 1 b -2 a 3").unwrap();
        assert_eq!(list.print_as_css_value(), "a 1 b -2 a 3");
        assert_eq!(parse_counter_reset(&list.print_as_css_value()).unwrap(), list);

        // `none`/`list_item` constants also survive a full round-trip.
        let n = CounterReset::none();
        assert_eq!(n.print_as_css_value(), "none");
        assert_eq!(parse_counter_reset(&n.print_as_css_value()).unwrap(), n);

        let li = CounterIncrement::list_item();
        assert_eq!(li.print_as_css_value(), "list-item 1");
        assert_eq!(
            parse_counter_increment(&li.print_as_css_value()).unwrap(),
            li
        );

        assert_eq!(
            CounterReset::list_item().print_as_css_value(),
            "list-item 0"
        );
        assert_eq!(CounterIncrement::none().print_as_css_value(), "none");
        assert_eq!(CounterSet::none().print_as_css_value(), "none");
    }

    #[test]
    fn counter_print_of_name_containing_space_does_not_reparse_into_itself() {
        // A name the parser can never make ("a b") prints as "a b 5", which
        // reads back as two counters.
        let r = CounterReset::new("a b".into(), 5);
        assert_eq!(r.print_as_css_value(), "a b 5");
        let reparsed = parse_counter_reset(&r.print_as_css_value()).unwrap();
        assert_eq!(names_and_values(&reparsed.counters), [("a", 0), ("b", 5)]);
        assert_ne!(reparsed, r);
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

        // The constant list equals the parsed one.
        assert_eq!(CounterReset::list_item(), parse_counter_reset("list-item").unwrap());
        assert_eq!(
            hash_of(&CounterReset::list_item()),
            hash_of(&parse_counter_reset("list-item").unwrap())
        );

        assert_eq!(
            hash_of(&Content { inner: "x".into() }),
            hash_of(&parse_content(" x ").unwrap())
        );
    }

    #[cfg(feature = "codegen")]
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

        let entry = CounterEntry::new("a\"b".into(), i32::MIN);
        assert_eq!(
            entry.format_as_rust_code(0),
            r#"CounterEntry { counter_name: AzString::from_const_str("a\"b"), value: -2147483648 }"#
        );

        // A list borrows its entries from the const slice the compiled sheet
        // declares for it; `none` borrows nothing.
        let r = CounterReset::new("a".into(), 1);
        assert_eq!(
            r.format_as_rust_code(0),
            alloc::format!(
                "CounterReset {{ counters: CounterEntryVec::from_const_slice(COUNTER_ENTRY_{}_ITEMS) }}",
                crate::hash::GetHash::get_hash(&r.counters)
            )
        );
        assert_eq!(
            CounterIncrement::none().format_as_rust_code(0),
            "CounterIncrement { counters: CounterEntryVec::from_const_slice(&[]) }"
        );
    }

    #[cfg(feature = "codegen")]
    #[test]
    fn format_as_rust_code_ignores_the_tab_argument() {
        let c = Content::default();
        assert_eq!(c.format_as_rust_code(0), c.format_as_rust_code(usize::MAX));

        let r = CounterReset::list_item();
        assert_eq!(r.format_as_rust_code(0), r.format_as_rust_code(usize::MAX));
    }
}
