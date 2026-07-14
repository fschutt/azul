//! C-compatible (`#[repr(C)]`) error types for CSS parsing failures.
//!
//! Mirrors `core::num::ParseFloatError` and `core::num::ParseIntError` for FFI use,
//! and provides generic invalid-value error wrappers.

use crate::corety::AzString;

/// Simple "invalid value" error, used for basic parsing failures
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

/// Owned version of `InvalidValueErr` with `AzString`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct InvalidValueErrOwned {
    pub value: AzString,
}

/// C-compatible enum mirroring `core::num::ParseFloatError` internals.
///
/// `core::num::ParseFloatError` is a 1-byte enum with variants `Empty` and `Invalid`,
/// but its `kind` field is private. We mirror the variants here for FFI compatibility.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum ParseFloatError {
    /// Input string was empty.
    Empty,
    /// Input string was not a valid float literal.
    Invalid,
}

impl ParseFloatError {
    /// Convert from `core::num::ParseFloatError` by comparing against known error instances.
    fn from_std(e: &core::num::ParseFloatError) -> Self {
        // Compare against the known Empty error instance to avoid
        // relying on Display message wording or allocating a format string.
        let empty_err = "".parse::<f32>().unwrap_err();
        if *e == empty_err {
            Self::Empty
        } else {
            Self::Invalid
        }
    }

    /// Reconstruct a `core::num::ParseFloatError` from our C-compatible variant.
    #[must_use] pub fn to_std(&self) -> core::num::ParseFloatError {
        match self {
            Self::Empty => "".parse::<f32>().unwrap_err(),
            Self::Invalid => "x".parse::<f32>().unwrap_err(),
        }
    }
}

impl core::fmt::Display for ParseFloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => write!(f, "cannot parse float from empty string"),
            Self::Invalid => write!(f, "invalid float literal"),
        }
    }
}

impl From<core::num::ParseFloatError> for ParseFloatError {
    fn from(e: core::num::ParseFloatError) -> Self {
        Self::from_std(&e)
    }
}

/// C-compatible enum mirroring `core::num::ParseIntError` internals.
///
/// `core::num::ParseIntError` is a 1-byte enum with variants matching `IntErrorKind`.
/// We mirror them here for FFI compatibility.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum ParseIntError {
    /// Input string was empty.
    Empty,
    /// Input contained an invalid digit.
    InvalidDigit,
    /// Input overflowed the target integer type (positive).
    PosOverflow,
    /// Input overflowed the target integer type (negative).
    NegOverflow,
    /// Input was zero but zero is not allowed (rarely used).
    Zero,
}

impl ParseIntError {
    /// Convert from `core::num::ParseIntError` using the stable `kind()` method.
    const fn from_std(e: &core::num::ParseIntError) -> Self {
        use core::num::IntErrorKind;
        match e.kind() {
            IntErrorKind::Empty => Self::Empty,
            IntErrorKind::PosOverflow => Self::PosOverflow,
            IntErrorKind::NegOverflow => Self::NegOverflow,
            IntErrorKind::Zero => Self::Zero,
            _ => Self::InvalidDigit, // future-proofing
        }
    }

    /// Reconstruct a `core::num::ParseIntError` from our C-compatible variant.
    #[must_use] pub fn to_std(&self) -> core::num::ParseIntError {
        match self {
            Self::Empty => "".parse::<i32>().unwrap_err(),
            Self::InvalidDigit => "x".parse::<i32>().unwrap_err(),
            Self::PosOverflow => "99999999999999999999".parse::<i32>().unwrap_err(),
            Self::NegOverflow => "-99999999999999999999".parse::<i32>().unwrap_err(),
            Self::Zero => {
                // Zero variant cannot be reproduced on stable Rust; falls back to InvalidDigit.
                // Note: round-tripping Zero through to_std() then from_std() yields InvalidDigit.
                "x".parse::<i32>().unwrap_err()
            }
        }
    }
}

impl core::fmt::Display for ParseIntError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => write!(f, "cannot parse integer from empty string"),
            Self::InvalidDigit => write!(f, "invalid digit found in string"),
            Self::PosOverflow => write!(f, "number too large to fit in target type"),
            Self::NegOverflow => write!(f, "number too small to fit in target type"),
            Self::Zero => write!(f, "number would be zero for non-zero type"),
        }
    }
}

impl From<core::num::ParseIntError> for ParseIntError {
    fn from(e: core::num::ParseIntError) -> Self {
        Self::from_std(&e)
    }
}

/// Wrapper for a `ParseFloatError` paired with the input string that failed.
/// Used by multiple Owned error enums that need to store both the error and input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ParseFloatErrorWithInput {
    pub error: ParseFloatError,
    pub input: AzString,
}

/// Wrapper for `WrongNumberOfComponents` errors in CSS filter/transform parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct WrongComponentCountError {
    pub expected: usize,
    pub got: usize,
    pub input: AzString,
}

impl InvalidValueErr<'_> {
    #[must_use] pub fn to_contained(&self) -> InvalidValueErrOwned {
        InvalidValueErrOwned { value: self.0.to_string().into() }
    }
}

impl InvalidValueErrOwned {
    #[must_use] pub fn to_shared(&self) -> InvalidValueErr<'_> {
        InvalidValueErr(self.value.as_str())
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod autotest_generated {
    use core::num::IntErrorKind;
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    use super::*;

    // =====================================================================
    // helpers
    // =====================================================================

    /// Parse `s` as `T`, expecting failure, and funnel the std error through
    /// the private `from_std` constructor under test.
    fn float_kind<T>(s: &str) -> ParseFloatError
    where
        T: core::str::FromStr<Err = core::num::ParseFloatError>,
    {
        match s.parse::<T>() {
            Ok(_) => panic!("expected {s:?} to FAIL to parse as a float"),
            Err(e) => ParseFloatError::from_std(&e),
        }
    }

    fn int_kind<T>(s: &str) -> ParseIntError
    where
        T: core::str::FromStr<Err = core::num::ParseIntError>,
    {
        match s.parse::<T>() {
            Ok(_) => panic!("expected {s:?} to FAIL to parse as an integer"),
            Err(e) => ParseIntError::from_std(&e),
        }
    }

    fn std_float_err(s: &str) -> core::num::ParseFloatError {
        s.parse::<f32>().expect_err("input should not parse")
    }

    fn std_int_err(s: &str) -> core::num::ParseIntError {
        s.parse::<i32>().expect_err("input should not parse")
    }

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    const ALL_FLOAT: [ParseFloatError; 2] = [ParseFloatError::Empty, ParseFloatError::Invalid];

    const ALL_INT: [ParseIntError; 5] = [
        ParseIntError::Empty,
        ParseIntError::InvalidDigit,
        ParseIntError::PosOverflow,
        ParseIntError::NegOverflow,
        ParseIntError::Zero,
    ];

    // =====================================================================
    // ParseFloatError::from_std  (constructor, private)
    // =====================================================================

    #[test]
    fn float_from_std_empty_string_maps_to_empty() {
        assert_eq!(float_kind::<f32>(""), ParseFloatError::Empty);
        // The comparison instance inside `from_std` is built from `f32`; an error
        // produced by an `f64` parse must still classify as `Empty` (std compares
        // the private `kind`, not the source type).
        assert_eq!(float_kind::<f64>(""), ParseFloatError::Empty);
    }

    #[test]
    fn float_from_std_blank_input_is_invalid_not_empty() {
        // A string that *looks* empty but is not: `from_std` must NOT collapse
        // these into `Empty`, because std trims nothing.
        for s in [
            " ",
            "  ",
            "\t",
            "\n",
            "\r\n",
            "\u{a0}",    // NBSP
            "\u{feff}",  // BOM
            "\u{200b}",  // zero-width space
            "\u{0}",     // NUL
        ] {
            assert_eq!(
                float_kind::<f32>(s),
                ParseFloatError::Invalid,
                "blank-ish input {s:?} must be Invalid, not Empty"
            );
        }
    }

    #[test]
    fn float_from_std_malformed_inputs_are_invalid() {
        for s in [
            "x", ".", "-", "+", "e", "e5", "5e", "1.2.3", "0x1f", "1,5", "--1", "++1", "1 ", " 1",
            "1_0", "NaNx", "infinit", "1/2", "abc", "1e", "1e+", "-.",
        ] {
            assert_eq!(
                float_kind::<f32>(s),
                ParseFloatError::Invalid,
                "malformed input {s:?} should be Invalid"
            );
        }
    }

    #[test]
    fn float_from_std_non_ascii_digits_are_invalid() {
        for s in [
            "١٢٣",     // Arabic-Indic digits
            "１２３",  // fullwidth digits
            "½",       // vulgar fraction
            "😀",
            "٣.٥",
            "1\u{301}", // combining acute after a valid digit
            "Ⅻ",        // roman numeral
        ] {
            assert_eq!(
                float_kind::<f32>(s),
                ParseFloatError::Invalid,
                "unicode input {s:?} should be Invalid"
            );
        }
    }

    #[test]
    fn float_from_std_huge_malformed_input_does_not_panic_or_hang() {
        let mut huge = "9".repeat(100_000);
        huge.push('x');
        assert_eq!(float_kind::<f32>(&huge), ParseFloatError::Invalid);

        // 100k leading zeros followed by garbage: still just Invalid.
        let mut zeros = "0".repeat(100_000);
        zeros.push_str("..");
        assert_eq!(float_kind::<f32>(&zeros), ParseFloatError::Invalid);
    }

    #[test]
    fn float_from_impl_agrees_with_from_std() {
        for s in ["", " ", "x", "1.2.3", "😀"] {
            let a: ParseFloatError = std_float_err(s).into();
            let b = ParseFloatError::from_std(&std_float_err(s));
            assert_eq!(a, b, "From<> and from_std disagree for {s:?}");
        }
    }

    // =====================================================================
    // float numeric limits: magnitude overflow never reaches our error type
    // =====================================================================

    #[test]
    fn float_magnitude_overflow_saturates_to_infinity_instead_of_erroring() {
        // No `ParseFloatError` is produced for out-of-range magnitudes — std
        // saturates. Anything relying on an "overflow" variant would be wrong.
        assert!(
            "1e400".parse::<f32>().expect("saturates, does not error").is_infinite(),
            "huge positive exponent should saturate to +inf"
        );
        assert!("-1e400".parse::<f32>().expect("saturates").is_sign_negative());
        assert_eq!("1e-400".parse::<f32>().expect("underflows to zero"), 0.0);

        let huge = "9".repeat(100_000);
        assert!(huge.parse::<f32>().expect("saturates").is_infinite());
    }

    #[test]
    fn float_nan_and_inf_literals_parse_and_never_error() {
        assert!("nan".parse::<f32>().expect("nan is valid").is_nan());
        assert!("NaN".parse::<f32>().expect("NaN is valid").is_nan());
        assert!("inf".parse::<f32>().expect("inf is valid").is_infinite());
        assert!("infinity".parse::<f32>().expect("infinity is valid").is_infinite());
        assert!("-inf".parse::<f32>().expect("-inf is valid").is_sign_negative());
        assert!("-0".parse::<f32>().expect("-0 is valid").is_sign_negative());
    }

    // =====================================================================
    // ParseFloatError::to_std  (getter) + round-trip
    // =====================================================================

    #[test]
    fn float_to_std_returns_the_matching_std_error() {
        assert_eq!(ParseFloatError::Empty.to_std(), std_float_err(""));
        assert_eq!(ParseFloatError::Invalid.to_std(), std_float_err("x"));
    }

    #[test]
    fn float_to_std_variants_stay_distinct() {
        // If these ever collapsed, `from_std` would misclassify every error.
        assert_ne!(ParseFloatError::Empty.to_std(), ParseFloatError::Invalid.to_std());
    }

    #[test]
    fn float_to_std_is_deterministic() {
        for v in ALL_FLOAT {
            assert_eq!(v.to_std(), v.to_std(), "to_std() must be stable for {v:?}");
        }
    }

    #[test]
    fn float_round_trip_encode_decode_is_identity() {
        for v in ALL_FLOAT {
            assert_eq!(ParseFloatError::from_std(&v.to_std()), v, "round-trip lost {v:?}");
            assert_eq!(ParseFloatError::from(v.to_std()), v);
        }
    }

    // =====================================================================
    // ParseIntError::from_std  (constructor, private)
    // =====================================================================

    #[test]
    fn int_from_std_empty_string_maps_to_empty_for_every_width() {
        assert_eq!(int_kind::<i8>(""), ParseIntError::Empty);
        assert_eq!(int_kind::<u8>(""), ParseIntError::Empty);
        assert_eq!(int_kind::<i32>(""), ParseIntError::Empty);
        assert_eq!(int_kind::<u128>(""), ParseIntError::Empty);
        assert_eq!(int_kind::<usize>(""), ParseIntError::Empty);
        assert_eq!(int_kind::<isize>(""), ParseIntError::Empty);
    }

    #[test]
    fn int_from_std_malformed_inputs_are_invalid_digit() {
        for s in [
            "x", " ", "  ", "\t", "+", "-", "+-1", "--1", "1 ", " 1", "1_000", "0x10", "1.0",
            "1e3", "abc", "\u{0}", "1\u{0}", "٣", "１２３", "😀", "½", ",", "1,000",
        ] {
            assert_eq!(
                int_kind::<i32>(s),
                ParseIntError::InvalidDigit,
                "malformed input {s:?} should be InvalidDigit"
            );
        }
    }

    #[test]
    fn int_from_std_negative_into_unsigned_is_invalid_digit_not_neg_overflow() {
        // std rejects the '-' sign as a digit for unsigned types rather than
        // reporting NegOverflow — a classifier that assumed otherwise would be wrong.
        assert_eq!(int_kind::<u32>("-1"), ParseIntError::InvalidDigit);
        assert_eq!(int_kind::<u8>("-0"), ParseIntError::InvalidDigit);
        assert_eq!(int_kind::<u128>("-99999999999999999999999999"), ParseIntError::InvalidDigit);
    }

    #[test]
    fn int_from_std_positive_overflow_boundaries() {
        // exactly MAX parses; MAX + 1 overflows.
        assert_eq!(i32::MAX.to_string().parse::<i32>(), Ok(i32::MAX));
        assert_eq!(int_kind::<i32>("2147483648"), ParseIntError::PosOverflow);
        assert_eq!(u8::MAX.to_string().parse::<u8>(), Ok(u8::MAX));
        assert_eq!(int_kind::<u8>("256"), ParseIntError::PosOverflow);
        assert_eq!(i8::MAX.to_string().parse::<i8>(), Ok(i8::MAX));
        assert_eq!(int_kind::<i8>("128"), ParseIntError::PosOverflow);
        assert_eq!(
            int_kind::<u128>("340282366920938463463374607431768211456"),
            ParseIntError::PosOverflow
        );
    }

    #[test]
    fn int_from_std_negative_overflow_boundaries() {
        assert_eq!(i32::MIN.to_string().parse::<i32>(), Ok(i32::MIN));
        assert_eq!(int_kind::<i32>("-2147483649"), ParseIntError::NegOverflow);
        assert_eq!(i8::MIN.to_string().parse::<i8>(), Ok(i8::MIN));
        assert_eq!(int_kind::<i8>("-129"), ParseIntError::NegOverflow);
        assert_eq!(int_kind::<i128>("-99999999999999999999999999999999999999999"), ParseIntError::NegOverflow);
    }

    #[test]
    fn int_from_std_huge_digit_runs_overflow_without_panic() {
        let huge = "9".repeat(10_000);
        assert_eq!(int_kind::<i32>(&huge), ParseIntError::PosOverflow);
        assert_eq!(int_kind::<u128>(&huge), ParseIntError::PosOverflow);

        let huge_neg = format!("-{huge}");
        assert_eq!(int_kind::<i64>(&huge_neg), ParseIntError::NegOverflow);
    }

    #[test]
    fn int_leading_zeros_do_not_produce_a_false_overflow() {
        // 10k leading zeros: the digit loop multiplies by 10 each step, so a naive
        // overflow check would trip here. It must still parse cleanly.
        let padded = format!("{}5", "0".repeat(10_000));
        assert_eq!(padded.parse::<i32>(), Ok(5));
        assert_eq!("0000000000000000000000000000005".parse::<i32>(), Ok(5));
    }

    #[test]
    fn int_from_std_zero_variant_is_reachable_via_nonzero_types() {
        // Contrary to the note on `to_std`, `IntErrorKind::Zero` IS constructible on
        // stable via the NonZero* parsers — so `from_std` really can return `Zero`.
        assert_eq!(int_kind::<core::num::NonZeroU8>("0"), ParseIntError::Zero);
        assert_eq!(int_kind::<core::num::NonZeroI32>("0"), ParseIntError::Zero);
        assert_eq!(int_kind::<core::num::NonZeroUsize>("0"), ParseIntError::Zero);
        // ...while other failures on the same type keep their own classification.
        assert_eq!(int_kind::<core::num::NonZeroU8>(""), ParseIntError::Empty);
        assert_eq!(int_kind::<core::num::NonZeroU8>("x"), ParseIntError::InvalidDigit);
        assert_eq!(int_kind::<core::num::NonZeroU8>("256"), ParseIntError::PosOverflow);
    }

    #[test]
    fn int_from_impl_agrees_with_from_std() {
        for s in ["", "x", "99999999999999999999", "-99999999999999999999", "😀"] {
            let a: ParseIntError = std_int_err(s).into();
            let b = ParseIntError::from_std(&std_int_err(s));
            assert_eq!(a, b, "From<> and from_std disagree for {s:?}");
        }
    }

    // =====================================================================
    // ParseIntError::to_std  (getter) + round-trip
    // =====================================================================

    #[test]
    fn int_to_std_maps_each_variant_onto_the_expected_std_kind() {
        assert!(matches!(ParseIntError::Empty.to_std().kind(), IntErrorKind::Empty));
        assert!(matches!(ParseIntError::InvalidDigit.to_std().kind(), IntErrorKind::InvalidDigit));
        assert!(matches!(ParseIntError::PosOverflow.to_std().kind(), IntErrorKind::PosOverflow));
        assert!(matches!(ParseIntError::NegOverflow.to_std().kind(), IntErrorKind::NegOverflow));
        // Documented lossy case: `Zero` degrades to an InvalidDigit std error.
        assert!(matches!(ParseIntError::Zero.to_std().kind(), IntErrorKind::InvalidDigit));
    }

    #[test]
    fn int_to_std_is_deterministic() {
        for v in ALL_INT {
            assert_eq!(v.to_std(), v.to_std(), "to_std() must be stable for {v:?}");
        }
    }

    #[test]
    fn int_round_trip_encode_decode_is_identity_except_for_zero() {
        for v in [
            ParseIntError::Empty,
            ParseIntError::InvalidDigit,
            ParseIntError::PosOverflow,
            ParseIntError::NegOverflow,
        ] {
            assert_eq!(ParseIntError::from_std(&v.to_std()), v, "round-trip lost {v:?}");
            assert_eq!(ParseIntError::from(v.to_std()), v);
        }

        // `Zero` is the one variant that does NOT survive to_std() -> from_std(),
        // exactly as the code comments document. (It is *not* an un-representable
        // kind though — see `int_from_std_zero_variant_is_reachable_via_nonzero_types`.)
        assert_eq!(
            ParseIntError::from_std(&ParseIntError::Zero.to_std()),
            ParseIntError::InvalidDigit,
            "Zero round-trip is documented as lossy"
        );
    }

    #[test]
    fn int_to_std_variants_stay_distinct_where_they_must() {
        let empty = ParseIntError::Empty.to_std();
        let invalid = ParseIntError::InvalidDigit.to_std();
        let pos = ParseIntError::PosOverflow.to_std();
        let neg = ParseIntError::NegOverflow.to_std();
        assert_ne!(empty, invalid);
        assert_ne!(invalid, pos);
        assert_ne!(pos, neg);
        assert_ne!(empty, neg);
        // Zero aliases InvalidDigit (documented).
        assert_eq!(ParseIntError::Zero.to_std(), invalid);
    }

    // =====================================================================
    // Display / Debug (serializers)
    // =====================================================================

    #[test]
    fn display_output_is_non_empty_and_unique_per_variant() {
        let float_msgs: Vec<String> = ALL_FLOAT.iter().map(ToString::to_string).collect();
        for m in &float_msgs {
            assert!(!m.is_empty(), "float Display must not be empty");
        }
        assert_ne!(float_msgs[0], float_msgs[1], "float variants must be distinguishable");

        let int_msgs: Vec<String> = ALL_INT.iter().map(ToString::to_string).collect();
        for m in &int_msgs {
            assert!(!m.is_empty(), "int Display must not be empty");
        }
        for i in 0..int_msgs.len() {
            for j in (i + 1)..int_msgs.len() {
                assert_ne!(int_msgs[i], int_msgs[j], "int variants {i}/{j} share a message");
            }
        }
    }

    #[test]
    fn display_mirrors_the_std_error_messages() {
        // The whole point of these types is to be a faithful FFI mirror of the std
        // errors; if std ever reworded a message, this catches the drift.
        assert_eq!(ParseFloatError::Empty.to_string(), std_float_err("").to_string());
        assert_eq!(ParseFloatError::Invalid.to_string(), std_float_err("x").to_string());

        assert_eq!(ParseIntError::Empty.to_string(), std_int_err("").to_string());
        assert_eq!(ParseIntError::InvalidDigit.to_string(), std_int_err("x").to_string());
        assert_eq!(
            ParseIntError::PosOverflow.to_string(),
            std_int_err("99999999999999999999").to_string()
        );
        assert_eq!(
            ParseIntError::NegOverflow.to_string(),
            std_int_err("-99999999999999999999").to_string()
        );
        // `Zero` cannot go through `to_std()` (it aliases InvalidDigit), so compare
        // against a genuine Zero-kind error obtained from a NonZero parse.
        let std_zero = "0"
            .parse::<core::num::NonZeroU8>()
            .expect_err("parsing 0 as NonZeroU8 must fail");
        assert!(matches!(std_zero.kind(), IntErrorKind::Zero));
        assert_eq!(ParseIntError::Zero.to_string(), std_zero.to_string());
    }

    #[test]
    fn display_with_formatter_flags_does_not_panic() {
        for v in ALL_INT {
            let msg = v.to_string();
            let padded = format!("{v:>60}");
            assert!(!padded.is_empty());
            assert!(padded.contains(&msg), "padding must not corrupt the message");
            // precision / fill / alternate flags: no panic, still produces output
            assert!(!format!("{v:.3}").is_empty());
            assert!(!format!("{v:*^10}").is_empty());
            assert!(!format!("{v:#?}").is_empty());
        }
        for v in ALL_FLOAT {
            assert!(!format!("{v:>60}").is_empty());
            assert!(!format!("{v:.1}").is_empty());
            assert!(!format!("{v:?}").is_empty());
        }
    }

    #[test]
    fn debug_output_names_the_variant() {
        assert_eq!(format!("{:?}", ParseFloatError::Empty), "Empty");
        assert_eq!(format!("{:?}", ParseIntError::PosOverflow), "PosOverflow");
        assert_eq!(format!("{:?}", ParseIntError::Zero), "Zero");
    }

    // =====================================================================
    // derived-trait invariants (Eq / Ord / Hash / Copy)
    // =====================================================================

    #[test]
    fn error_enums_have_consistent_eq_hash_and_ord() {
        for (i, a) in ALL_INT.iter().enumerate() {
            assert_eq!(hash_of(a), hash_of(&ALL_INT[i]), "equal values must hash equal");
            for (j, b) in ALL_INT.iter().enumerate() {
                assert_eq!(a == b, i == j, "only identical variants may compare equal");
                assert_eq!(a.cmp(b), i.cmp(&j), "Ord must follow declaration order");
            }
        }
        assert!(ParseFloatError::Empty < ParseFloatError::Invalid);
        assert_eq!(hash_of(&ParseFloatError::Empty), hash_of(&ParseFloatError::Empty));
        assert_ne!(ParseFloatError::Empty, ParseFloatError::Invalid);
    }

    #[test]
    fn error_enums_are_copy_and_survive_a_sort() {
        let mut v = [
            ParseIntError::Zero,
            ParseIntError::Empty,
            ParseIntError::NegOverflow,
            ParseIntError::InvalidDigit,
            ParseIntError::PosOverflow,
        ];
        v.sort_unstable();
        assert_eq!(v, ALL_INT);

        let a = ParseIntError::Zero;
        let b = a; // Copy, not move
        assert_eq!(a, b);
    }

    // =====================================================================
    // InvalidValueErr::to_contained / InvalidValueErrOwned::to_shared
    // =====================================================================

    #[test]
    fn invalid_value_err_round_trips_through_owned() {
        for s in [
            "",
            "a",
            "border-radius",
            "   ",
            "\n\t",
            "börder-radiüs 😀",
            "٣.٥",
            "\u{feff}leading-bom",
            "trailing-nul\u{0}",
            "a\u{0}b",
        ] {
            let shared = InvalidValueErr(s);
            let owned = shared.to_contained();
            assert_eq!(owned.value.as_str(), s, "to_contained lost {s:?}");
            assert_eq!(owned.to_shared(), shared, "round-trip changed {s:?}");
            assert_eq!(owned.to_shared().0, s);
        }
    }

    #[test]
    fn invalid_value_err_empty_string_is_not_confused_with_default() {
        let owned = InvalidValueErr("").to_contained();
        assert_eq!(owned.value, AzString::default());
        assert!(owned.value.as_str().is_empty());
        assert_eq!(owned.to_shared(), InvalidValueErr(""));
        assert_eq!(owned, InvalidValueErrOwned { value: AzString::default() });
    }

    #[test]
    fn invalid_value_err_preserves_interior_nul_bytes() {
        // If the AzString conversion ever went through a C-string, this would
        // truncate at the NUL.
        let s = "a\u{0}b";
        let owned = InvalidValueErr(s).to_contained();
        assert_eq!(owned.value.as_bytes(), b"a\0b");
        assert_eq!(owned.value.as_str().len(), 3);
        assert_eq!(owned.to_shared().0.len(), 3);
    }

    #[test]
    fn to_contained_deep_copies_and_outlives_its_source() {
        let owned = {
            let src = String::from("temporary-buffer");
            let copied = InvalidValueErr(src.as_str()).to_contained();
            assert!(
                !core::ptr::eq(copied.value.as_str().as_ptr(), src.as_str().as_ptr()),
                "to_contained must copy, not alias the borrowed input"
            );
            drop(src);
            copied
        };
        assert_eq!(owned.value.as_str(), "temporary-buffer");
    }

    #[test]
    fn to_shared_borrows_the_owned_buffer_without_copying() {
        let owned = InvalidValueErr("shared-buffer").to_contained();
        let shared = owned.to_shared();
        assert!(
            core::ptr::eq(shared.0.as_ptr(), owned.value.as_str().as_ptr()),
            "to_shared must borrow the existing buffer"
        );
        // calling it twice yields the same view
        assert_eq!(owned.to_shared(), owned.to_shared());
    }

    #[test]
    fn invalid_value_err_handles_a_huge_payload() {
        let big = "ü".repeat(100_000); // 200_000 bytes, non-ASCII
        let owned = InvalidValueErr(big.as_str()).to_contained();
        assert_eq!(owned.value.as_str().len(), big.len());
        assert_eq!(owned.value.as_bytes().len(), 200_000);
        assert_eq!(owned.to_shared().0, big.as_str());
        assert_eq!(owned.clone(), owned);
    }

    #[test]
    fn invalid_value_err_owned_equality_is_by_content() {
        let a = InvalidValueErr("x").to_contained();
        let b = InvalidValueErrOwned { value: AzString::from("x") };
        let c = InvalidValueErrOwned { value: AzString::from("y") };
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.clone(), a);
        assert_eq!(a.to_shared(), b.to_shared());
    }

    // =====================================================================
    // ParseFloatErrorWithInput / WrongComponentCountError
    // =====================================================================

    #[test]
    fn parse_float_error_with_input_keeps_error_and_input_together() {
        let input = "1.2.3";
        let err = ParseFloatErrorWithInput {
            error: ParseFloatError::from(std_float_err(input)),
            input: AzString::from(input),
        };
        assert_eq!(err.error, ParseFloatError::Invalid);
        assert_eq!(err.input.as_str(), input);
        assert_eq!(err.clone(), err);

        let empty = ParseFloatErrorWithInput {
            error: ParseFloatError::from(std_float_err("")),
            input: AzString::default(),
        };
        assert_eq!(empty.error, ParseFloatError::Empty);
        assert!(empty.input.as_str().is_empty());
        assert_ne!(empty, err);
        assert!(!format!("{err:?}").is_empty());
    }

    #[test]
    fn wrong_component_count_error_survives_usize_extremes() {
        let e = WrongComponentCountError {
            expected: usize::MAX,
            got: 0,
            input: AzString::from("rgba(1)"),
        };
        assert_eq!(e.expected, usize::MAX);
        assert_eq!(e.got, 0);
        assert_eq!(e.input.as_str(), "rgba(1)");
        assert_eq!(e.clone(), e);
        assert!(!format!("{e:?}").is_empty());

        let same_but_got_max = WrongComponentCountError {
            expected: usize::MAX,
            got: usize::MAX,
            input: AzString::from("rgba(1)"),
        };
        assert_ne!(e, same_but_got_max, "`got` participates in equality");

        // A 0-expected / 0-got degenerate error is still constructible and inert.
        let zeroed = WrongComponentCountError {
            expected: 0,
            got: 0,
            input: AzString::default(),
        };
        assert_eq!(zeroed.expected, zeroed.got);
        assert!(zeroed.input.as_str().is_empty());
    }
}
