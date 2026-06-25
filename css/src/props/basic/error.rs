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
