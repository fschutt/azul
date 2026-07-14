//! CSS `text-justify` property.
//!
//! Defines [`LayoutTextJustify`] and its parser [`parse_layout_text_justify`],
//! used by the CSS property parsing pipeline.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{codegen::format::FormatAsRustCode, props::formatter::PrintAsCssValue};

/// CSS `text-justify` property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutTextJustify {
    #[default]
    Auto,
    None,
    InterWord,
    InterCharacter,
    /// Legacy value; the parser maps `"distribute"` to `InterCharacter` per spec.
    /// Retained for `#[repr(C)]` FFI backward compatibility.
    Distribute,
}


impl PrintAsCssValue for LayoutTextJustify {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::InterWord => "inter-word",
            Self::InterCharacter => "inter-character",
            Self::Distribute => "distribute",
        }
        .to_string()
    }
}

impl FormatAsRustCode for LayoutTextJustify {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("LayoutTextJustify::{self:?}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextJustifyParseError<'a> {
    InvalidValue(&'a str),
}

impl fmt::Display for TextJustifyParseError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextJustifyParseError::InvalidValue(s) => {
                write!(f, "Invalid text-justify value: '{s}'.")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum TextJustifyParseErrorOwned {
    InvalidValue(AzString),
}

impl TextJustifyParseError<'_> {
    #[must_use] pub fn to_owned(&self) -> TextJustifyParseErrorOwned {
        match self {
            TextJustifyParseError::InvalidValue(s) => {
                TextJustifyParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl TextJustifyParseErrorOwned {
    #[must_use] pub fn to_borrowed(&self) -> TextJustifyParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                TextJustifyParseError::InvalidValue(s.as_str())
            }
        }
    }
}

/// Parses a `text-justify` CSS value string into a [`LayoutTextJustify`].
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `text-justify` value.
pub fn parse_layout_text_justify(
    input: &str,
) -> Result<LayoutTextJustify, TextJustifyParseError<'_>> {
    match input.trim() {
        "auto" => Ok(LayoutTextJustify::Auto),
        "none" => Ok(LayoutTextJustify::None),
        "inter-word" => Ok(LayoutTextJustify::InterWord),
        // "distribute" is a legacy alias that computes to inter-character:
        // +spec:text-alignment-spacing:4a88c2  +spec:text-alignment-spacing:58c33f
        "inter-character" | "distribute" => Ok(LayoutTextJustify::InterCharacter),
        other => Err(TextJustifyParseError::InvalidValue(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_layout_text_justify() {
        assert_eq!(
            parse_layout_text_justify("auto"),
            Ok(LayoutTextJustify::Auto)
        );
        assert_eq!(
            parse_layout_text_justify("none"),
            Ok(LayoutTextJustify::None)
        );
        assert_eq!(
            parse_layout_text_justify("inter-word"),
            Ok(LayoutTextJustify::InterWord)
        );
        assert_eq!(
            parse_layout_text_justify("inter-character"),
            Ok(LayoutTextJustify::InterCharacter)
        );
        assert_eq!(
            parse_layout_text_justify("distribute"),
            Ok(LayoutTextJustify::InterCharacter)
        );
        assert!(parse_layout_text_justify("invalid").is_err());
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // Variant table, kept honest by `justify_variant_index`: the match below
    // is deliberately exhaustive (no `_` arm), so adding a variant to
    // `LayoutTextJustify` is a compile error here rather than a silently
    // untested variant in every loop of this module.
    // ---------------------------------------------------------------------

    const ALL_JUSTIFY: [LayoutTextJustify; 5] = [
        LayoutTextJustify::Auto,
        LayoutTextJustify::None,
        LayoutTextJustify::InterWord,
        LayoutTextJustify::InterCharacter,
        LayoutTextJustify::Distribute,
    ];

    const fn justify_variant_index(j: LayoutTextJustify) -> usize {
        match j {
            LayoutTextJustify::Auto => 0,
            LayoutTextJustify::None => 1,
            LayoutTextJustify::InterWord => 2,
            LayoutTextJustify::InterCharacter => 3,
            LayoutTextJustify::Distribute => 4,
        }
    }

    #[test]
    fn all_justify_lists_every_variant_exactly_once() {
        for (i, j) in ALL_JUSTIFY.iter().enumerate() {
            assert_eq!(
                justify_variant_index(*j),
                i,
                "ALL_JUSTIFY is out of sync at index {i} ({j:?})"
            );
        }
    }

    // ---------------------------------------------------------------------
    // parse_layout_text_justify — positive controls
    // ---------------------------------------------------------------------

    #[test]
    fn every_documented_keyword_parses_to_its_variant() {
        assert_eq!(
            parse_layout_text_justify("auto"),
            Ok(LayoutTextJustify::Auto)
        );
        assert_eq!(
            parse_layout_text_justify("none"),
            Ok(LayoutTextJustify::None)
        );
        assert_eq!(
            parse_layout_text_justify("inter-word"),
            Ok(LayoutTextJustify::InterWord)
        );
        assert_eq!(
            parse_layout_text_justify("inter-character"),
            Ok(LayoutTextJustify::InterCharacter)
        );
        // Legacy alias: "distribute" computes to inter-character.
        assert_eq!(
            parse_layout_text_justify("distribute"),
            Ok(LayoutTextJustify::InterCharacter)
        );
    }

    /// `Distribute` exists only so the `#[repr(C)]` discriminants stay
    /// backwards compatible; no input string may ever produce it, otherwise
    /// two distinct values would represent the same computed style.
    #[test]
    fn no_input_ever_parses_to_the_legacy_distribute_variant() {
        let candidates = [
            "distribute",
            "  distribute  ",
            "inter-character",
            "auto",
            "none",
            "inter-word",
        ];
        for input in candidates {
            assert_ne!(
                parse_layout_text_justify(input),
                Ok(LayoutTextJustify::Distribute),
                "{input:?} parsed to the FFI-only Distribute variant"
            );
        }
    }

    // ---------------------------------------------------------------------
    // parse_layout_text_justify — empty / whitespace / trimming
    // ---------------------------------------------------------------------

    #[test]
    fn empty_and_whitespace_only_input_is_rejected_with_an_empty_payload() {
        // Everything here trims down to "", so the error must carry "" (not
        // the original padding) and must not panic on the empty slice.
        for input in ["", " ", "   ", "\t", "\n", "\r\n", " \t\r\n\u{b}\u{c} "] {
            assert_eq!(
                parse_layout_text_justify(input),
                Err(TextJustifyParseError::InvalidValue("")),
                "whitespace-only input {input:?}"
            );
        }
    }

    #[test]
    fn surrounding_ascii_whitespace_is_trimmed_before_matching() {
        for (input, expected) in [
            ("  auto  ", LayoutTextJustify::Auto),
            ("\tnone\t", LayoutTextJustify::None),
            ("\n inter-word \r\n", LayoutTextJustify::InterWord),
            ("\r\n\tdistribute\r\n\t", LayoutTextJustify::InterCharacter),
        ] {
            assert_eq!(parse_layout_text_justify(input), Ok(expected), "{input:?}");
        }
    }

    /// `str::trim` is Unicode-aware, so keywords padded with Unicode
    /// `White_Space` characters (NBSP, ideographic space, NEL) are accepted
    /// even though CSS whitespace is only space/tab/LF/CR/FF. Pinned as the
    /// *current* behaviour — it is more lenient than the spec, never stricter.
    #[test]
    fn unicode_whitespace_padding_is_also_trimmed() {
        assert_eq!(
            parse_layout_text_justify("\u{a0}auto\u{a0}"),
            Ok(LayoutTextJustify::Auto),
            "NBSP-padded keyword"
        );
        assert_eq!(
            parse_layout_text_justify("\u{3000}none\u{3000}"),
            Ok(LayoutTextJustify::None),
            "ideographic-space-padded keyword"
        );
        assert_eq!(
            parse_layout_text_justify("\u{85}inter-word\u{85}"),
            Ok(LayoutTextJustify::InterWord),
            "NEL-padded keyword"
        );
        // ...but zero-width characters are NOT `White_Space`, so they survive
        // the trim and must be rejected rather than silently stripped.
        assert_eq!(
            parse_layout_text_justify("\u{200b}auto"),
            Err(TextJustifyParseError::InvalidValue("\u{200b}auto")),
            "zero-width space is not whitespace"
        );
        assert_eq!(
            parse_layout_text_justify("\u{feff}auto"),
            Err(TextJustifyParseError::InvalidValue("\u{feff}auto")),
            "BOM is not whitespace"
        );
    }

    #[test]
    fn interior_whitespace_is_never_collapsed() {
        for input in [
            "inter word",
            "inter -word",
            "inter- word",
            "inter - character",
            "auto auto",
            "auto none",
            "au to",
        ] {
            assert_eq!(
                parse_layout_text_justify(input),
                Err(TextJustifyParseError::InvalidValue(input.trim())),
                "interior whitespace in {input:?} must not be collapsed away"
            );
        }
    }

    // ---------------------------------------------------------------------
    // parse_layout_text_justify — malformed input
    // ---------------------------------------------------------------------

    /// CSS keywords are ASCII case-insensitive (css-values-4 §3.1), but this
    /// parser matches case-sensitively and `parse_css_property` only trims
    /// (it does not lower-case) before dispatching here. Pinned as the current
    /// behaviour; see the report accompanying these tests.
    #[test]
    fn keyword_matching_is_case_sensitive() {
        for input in [
            "AUTO",
            "Auto",
            "aUtO",
            "NONE",
            "Inter-Word",
            "INTER-CHARACTER",
            "Distribute",
        ] {
            assert!(
                parse_layout_text_justify(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[test]
    fn garbage_and_near_miss_input_is_rejected_without_panicking() {
        for input in [
            "invalid",
            "interword",
            "inter_word",
            "inter–word",     // en dash, not hyphen-minus
            "inter-",
            "-character",
            "inter-characters",
            "autos",
            "aut",
            "auto;",
            "auto;garbage",
            "auto !important",
            "auto/**/",
            "/*auto*/",
            "url(auto)",
            "attr(auto)",
            "\"auto\"",
            "'auto'",
            ";",
            "{}",
            "\\",
            "-",
            "--",
            "\0",
            "auto\0",
            "\0auto",
            "initial",
            "inherit",
            "unset",
            "revert",
        ] {
            assert_eq!(
                parse_layout_text_justify(input),
                Err(TextJustifyParseError::InvalidValue(input.trim())),
                "{input:?} must be rejected verbatim"
            );
        }
    }

    #[test]
    fn boundary_numeric_strings_are_rejected() {
        let big = i64::MAX.to_string();
        let small = i64::MIN.to_string();
        let umax = u64::MAX.to_string();
        let fmax = f64::MAX.to_string();
        let ftiny = f64::MIN_POSITIVE.to_string();
        let inputs = [
            "0",
            "-0",
            "+0",
            "0.0",
            "1",
            "1e400",
            "-1e-400",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "Infinity",
            big.as_str(),
            small.as_str(),
            umax.as_str(),
            fmax.as_str(),
            ftiny.as_str(),
        ];
        for input in inputs {
            assert_eq!(
                parse_layout_text_justify(input),
                Err(TextJustifyParseError::InvalidValue(input)),
                "numeric-looking input {input:?} must not parse as a keyword"
            );
        }
    }

    #[test]
    fn unicode_input_is_rejected_and_preserved_verbatim_in_the_error() {
        for input in [
            "\u{1F600}",              // emoji
            "auto\u{301}",            // combining acute on the last char
            "\u{301}\u{301}\u{301}",  // lone combining marks
            "ａｕｔｏ",               // fullwidth latin
            "аuto",                   // Cyrillic homoglyph 'а'
            "\u{202e}auto",           // RTL override
            "🙂🙃🙂",
            "日本語",
            "\u{fffd}",               // replacement char
        ] {
            assert_eq!(
                parse_layout_text_justify(input),
                Err(TextJustifyParseError::InvalidValue(input)),
                "unicode input {input:?}"
            );
            // The error must round-trip through the owned form untouched, i.e.
            // no char boundary was sliced through.
            let err = parse_layout_text_justify(input).unwrap_err();
            assert_eq!(err.to_owned().to_borrowed(), err);
        }
    }

    #[test]
    fn extremely_long_input_neither_panics_nor_hangs() {
        let million = "a".repeat(1_000_000);
        assert_eq!(
            parse_layout_text_justify(&million),
            Err(TextJustifyParseError::InvalidValue(million.as_str()))
        );

        // A valid keyword repeated until it is no longer a keyword.
        let repeated = "auto".repeat(250_000);
        assert!(parse_layout_text_justify(&repeated).is_err());

        // Huge padding around a valid keyword: trimming must still find it.
        let padded = format!("{}auto{}", " ".repeat(500_000), "\n".repeat(500_000));
        assert_eq!(
            parse_layout_text_justify(&padded),
            Ok(LayoutTextJustify::Auto)
        );

        // Huge padding around garbage: the error payload is the *trimmed*
        // slice, so it must not carry the megabyte of padding along.
        let padded_garbage = format!("{}garbage{}", " ".repeat(500_000), " ".repeat(500_000));
        let err = parse_layout_text_justify(&padded_garbage).unwrap_err();
        assert_eq!(err, TextJustifyParseError::InvalidValue("garbage"));
    }

    #[test]
    fn deeply_nested_brackets_do_not_stack_overflow() {
        // The parser is a flat match, but a future rewrite that recurses on
        // nested constructs would blow the stack here instead of shipping.
        let nested = format!("{}auto{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_layout_text_justify(&nested).is_err());

        let braces = "{".repeat(50_000);
        assert_eq!(
            parse_layout_text_justify(&braces),
            Err(TextJustifyParseError::InvalidValue(braces.as_str()))
        );
    }

    /// The error borrows *into* the caller's string rather than copying it:
    /// the returned slice must point inside `input` and equal `input.trim()`.
    #[test]
    fn error_payload_is_a_borrowed_subslice_of_the_input() {
        let input = "   bogus-value   ";
        let Err(TextJustifyParseError::InvalidValue(slice)) = parse_layout_text_justify(input)
        else {
            panic!("expected an error for {input:?}");
        };
        assert_eq!(slice, "bogus-value");
        assert_eq!(slice, input.trim());

        let base = input.as_ptr() as usize;
        let borrowed = slice.as_ptr() as usize;
        assert!(
            borrowed >= base && borrowed + slice.len() <= base + input.len(),
            "error payload does not point into the input buffer"
        );
    }

    // ---------------------------------------------------------------------
    // Round trip: print_as_css_value <-> parse_layout_text_justify
    // ---------------------------------------------------------------------

    #[test]
    fn every_printed_value_parses_back_to_the_same_variant() {
        for j in ALL_JUSTIFY {
            let printed = j.print_as_css_value();
            let reparsed = parse_layout_text_justify(&printed);
            let expected = if j == LayoutTextJustify::Distribute {
                // Documented, deliberate asymmetry: "distribute" is a legacy
                // alias that computes to inter-character, so the round trip is
                // lossy for exactly this one (FFI-only) variant.
                LayoutTextJustify::InterCharacter
            } else {
                j
            };
            assert_eq!(reparsed, Ok(expected), "round trip of {j:?} via {printed:?}");
        }
    }

    #[test]
    fn distribute_round_trip_is_lossy_by_design() {
        let printed = LayoutTextJustify::Distribute.print_as_css_value();
        assert_eq!(printed, "distribute");
        assert_eq!(
            parse_layout_text_justify(&printed),
            Ok(LayoutTextJustify::InterCharacter)
        );
        assert_ne!(
            parse_layout_text_justify(&printed),
            Ok(LayoutTextJustify::Distribute)
        );
    }

    #[test]
    fn printed_values_are_distinct_well_formed_css_idents() {
        let mut seen: Vec<String> = Vec::new();
        for j in ALL_JUSTIFY {
            let printed = j.print_as_css_value();
            assert!(!printed.is_empty(), "{j:?} printed an empty value");
            assert_eq!(printed.trim(), printed, "{j:?} printed padded value");
            assert!(
                !printed.contains(char::is_whitespace),
                "{j:?} printed interior whitespace: {printed:?}"
            );
            assert!(
                printed
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{j:?} printed a non-ident value: {printed:?}"
            );
            assert!(
                !seen.contains(&printed),
                "{j:?} printed a value already used by another variant: {printed:?}"
            );
            seen.push(printed);
        }
        assert_eq!(seen.len(), ALL_JUSTIFY.len());
    }

    // ---------------------------------------------------------------------
    // FormatAsRustCode
    // ---------------------------------------------------------------------

    #[test]
    fn format_as_rust_code_is_a_variant_path_and_ignores_indentation() {
        for j in ALL_JUSTIFY {
            let code = j.format_as_rust_code(0);
            assert_eq!(code, format!("LayoutTextJustify::{j:?}"));
            assert!(code.starts_with("LayoutTextJustify::"));
            assert!(!code.contains(char::is_whitespace), "{code:?}");
            // The `tabs` argument is unused; extreme values must not panic or
            // change the output.
            assert_eq!(j.format_as_rust_code(usize::MAX), code);
            assert_eq!(j.format_as_rust_code(usize::MIN), code);
        }
    }

    // ---------------------------------------------------------------------
    // Enum invariants: Default, FFI discriminants, Ord / Hash
    // ---------------------------------------------------------------------

    #[test]
    fn default_is_auto() {
        assert_eq!(LayoutTextJustify::default(), LayoutTextJustify::Auto);
        assert_eq!(
            parse_layout_text_justify(&LayoutTextJustify::default().print_as_css_value()),
            Ok(LayoutTextJustify::default())
        );
    }

    /// The `Distribute` variant is documented as retained for `#[repr(C)]` FFI
    /// backwards compatibility, which only holds if the discriminants keep
    /// their values. Reordering the enum breaks every compiled C/Python
    /// binding — so it must break this test first.
    #[test]
    fn repr_c_discriminants_are_ffi_stable() {
        assert_eq!(LayoutTextJustify::Auto as u8, 0);
        assert_eq!(LayoutTextJustify::None as u8, 1);
        assert_eq!(LayoutTextJustify::InterWord as u8, 2);
        assert_eq!(LayoutTextJustify::InterCharacter as u8, 3);
        assert_eq!(LayoutTextJustify::Distribute as u8, 4);
    }

    #[test]
    fn derived_ord_follows_declaration_order() {
        for (i, a) in ALL_JUSTIFY.iter().enumerate() {
            for (k, b) in ALL_JUSTIFY.iter().enumerate() {
                assert_eq!(
                    a.cmp(b),
                    i.cmp(&k),
                    "Ord disagrees with declaration order for {a:?} vs {b:?}"
                );
                assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
                assert_eq!(a == b, i == k);
            }
        }
    }

    #[test]
    fn equal_variants_hash_equally_and_distinct_variants_do_not_collide() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of(j: LayoutTextJustify) -> u64 {
            let mut hasher = DefaultHasher::new();
            j.hash(&mut hasher);
            hasher.finish()
        }

        for j in ALL_JUSTIFY {
            assert_eq!(hash_of(j), hash_of(j), "{j:?} hashes unstably");
        }
        let mut hashes: Vec<u64> = ALL_JUSTIFY.iter().copied().map(hash_of).collect();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(
            hashes.len(),
            ALL_JUSTIFY.len(),
            "two variants share a hash — HashMap<LayoutTextJustify, _> would be needlessly slow"
        );
    }

    // ---------------------------------------------------------------------
    // TextJustifyParseError: Display + owned/borrowed conversions
    // ---------------------------------------------------------------------

    /// Nasty payloads reused by the error tests below.
    const NASTY: [&str; 9] = [
        "",
        "auto",
        "\0",
        "line\nbreak",
        "🙂",
        "a\u{301}",
        "{}",
        "{0} {1} {{}}",
        "%s %d %n",
    ];

    #[test]
    fn display_is_non_empty_and_quotes_the_offending_value() {
        for value in NASTY {
            let msg = TextJustifyParseError::InvalidValue(value).to_string();
            assert!(!msg.is_empty(), "empty message for {value:?}");
            assert!(
                msg.starts_with("Invalid text-justify value: '"),
                "unexpected prefix: {msg:?}"
            );
            assert!(msg.ends_with("'."), "unexpected suffix: {msg:?}");
            assert!(
                msg.contains(value),
                "message {msg:?} dropped the offending value {value:?}"
            );
        }
    }

    /// The payload is interpolated as a *value*, never re-parsed as a format
    /// string: braces and printf-style escapes must survive literally.
    #[test]
    fn display_does_not_interpret_braces_in_the_payload() {
        let msg = TextJustifyParseError::InvalidValue("{0} {{}} %s").to_string();
        assert_eq!(msg, "Invalid text-justify value: '{0} {{}} %s'.");
    }

    #[test]
    fn display_survives_empty_and_megabyte_payloads() {
        assert_eq!(
            TextJustifyParseError::InvalidValue("").to_string(),
            "Invalid text-justify value: ''."
        );

        let huge = "x".repeat(1_000_000);
        let msg = TextJustifyParseError::InvalidValue(huge.as_str()).to_string();
        // prefix + payload + suffix, with nothing truncated.
        assert_eq!(msg.len(), "Invalid text-justify value: ''.".len() + huge.len());
        assert!(msg.contains(huge.as_str()));
    }

    #[test]
    fn to_owned_then_to_borrowed_is_the_identity() {
        for value in NASTY {
            let borrowed = TextJustifyParseError::InvalidValue(value);
            let owned = borrowed.to_owned();
            assert_eq!(
                owned,
                TextJustifyParseErrorOwned::InvalidValue(String::from(value).into())
            );
            assert_eq!(owned.to_borrowed(), borrowed, "round trip of {value:?}");
            // The message must survive the detour through the owned form.
            assert_eq!(owned.to_borrowed().to_string(), borrowed.to_string());
        }
    }

    #[test]
    fn to_owned_survives_a_large_multibyte_payload_and_keeps_its_length() {
        let huge = "\u{1F600}".repeat(100_000); // 400 kB of 4-byte chars
        let borrowed = TextJustifyParseError::InvalidValue(huge.as_str());
        let owned = borrowed.to_owned();
        let TextJustifyParseErrorOwned::InvalidValue(s) = &owned;
        assert_eq!(s.as_str().len(), huge.len());
        assert_eq!(owned.to_borrowed(), borrowed);
    }

    /// A parse failure carries the *trimmed* input, and that payload must
    /// survive the borrowed -> owned -> borrowed detour byte for byte.
    #[test]
    fn parse_error_payload_round_trips_through_the_owned_form() {
        for input in ["  💥  ", "\tnot-a-keyword\n", "", "   ", "\0\0"] {
            let err = parse_layout_text_justify(input).unwrap_err();
            let TextJustifyParseError::InvalidValue(borrowed) = &err;
            assert_eq!(*borrowed, input.trim());

            let owned = err.to_owned();
            let TextJustifyParseErrorOwned::InvalidValue(s) = &owned;
            assert_eq!(s.as_str(), input.trim());
            assert_eq!(owned.to_borrowed(), err);
        }
    }

    #[test]
    fn owned_error_is_independent_of_the_input_buffer() {
        // Build the error from a string that is dropped immediately after; the
        // owned form must not dangle or lose data.
        let owned = {
            let scratch = String::from("  transient-garbage  ");
            parse_layout_text_justify(&scratch).unwrap_err().to_owned()
        };
        let TextJustifyParseErrorOwned::InvalidValue(s) = &owned;
        assert_eq!(s.as_str(), "transient-garbage");
        assert_eq!(
            owned.to_borrowed().to_string(),
            "Invalid text-justify value: 'transient-garbage'."
        );
    }
}
