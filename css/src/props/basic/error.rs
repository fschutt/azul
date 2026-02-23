use crate::corety::AzString;

/// Simple "invalid value" error, used for basic parsing failures
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

/// Owned version of InvalidValueErr with AzString.
#[derive(Debug, Clone, PartialEq)]
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
    /// Convert from `core::num::ParseFloatError` by matching on the Display output.
    pub fn from_std(e: &core::num::ParseFloatError) -> Self {
        // core::num::ParseFloatError Display:
        //   Empty   => "cannot parse float from empty string"
        //   Invalid => "invalid float literal"
        let s = alloc::format!("{}", e);
        if s.contains("empty") {
            ParseFloatError::Empty
        } else {
            ParseFloatError::Invalid
        }
    }

    /// Reconstruct a `core::num::ParseFloatError` from our C-compatible variant.
    pub fn to_std(&self) -> core::num::ParseFloatError {
        match self {
            ParseFloatError::Empty => "".parse::<f32>().unwrap_err(),
            ParseFloatError::Invalid => "x".parse::<f32>().unwrap_err(),
        }
    }
}

impl core::fmt::Display for ParseFloatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseFloatError::Empty => write!(f, "cannot parse float from empty string"),
            ParseFloatError::Invalid => write!(f, "invalid float literal"),
        }
    }
}

impl From<core::num::ParseFloatError> for ParseFloatError {
    fn from(e: core::num::ParseFloatError) -> Self {
        ParseFloatError::from_std(&e)
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
    pub fn from_std(e: &core::num::ParseIntError) -> Self {
        use core::num::IntErrorKind;
        match e.kind() {
            IntErrorKind::Empty => ParseIntError::Empty,
            IntErrorKind::InvalidDigit => ParseIntError::InvalidDigit,
            IntErrorKind::PosOverflow => ParseIntError::PosOverflow,
            IntErrorKind::NegOverflow => ParseIntError::NegOverflow,
            IntErrorKind::Zero => ParseIntError::Zero,
            _ => ParseIntError::InvalidDigit, // future-proofing
        }
    }

    /// Reconstruct a `core::num::ParseIntError` from our C-compatible variant.
    pub fn to_std(&self) -> core::num::ParseIntError {
        match self {
            ParseIntError::Empty => "".parse::<i32>().unwrap_err(),
            ParseIntError::InvalidDigit => "x".parse::<i32>().unwrap_err(),
            ParseIntError::PosOverflow => "99999999999999999999".parse::<i32>().unwrap_err(),
            ParseIntError::NegOverflow => "-99999999999999999999".parse::<i32>().unwrap_err(),
            ParseIntError::Zero => {
                // Zero variant is rarely triggered; fallback to InvalidDigit
                "x".parse::<i32>().unwrap_err()
            }
        }
    }
}

impl core::fmt::Display for ParseIntError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ParseIntError::Empty => write!(f, "cannot parse integer from empty string"),
            ParseIntError::InvalidDigit => write!(f, "invalid digit found in string"),
            ParseIntError::PosOverflow => write!(f, "number too large to fit in target type"),
            ParseIntError::NegOverflow => write!(f, "number too small to fit in target type"),
            ParseIntError::Zero => write!(f, "number would be zero for non-zero type"),
        }
    }
}

impl From<core::num::ParseIntError> for ParseIntError {
    fn from(e: core::num::ParseIntError) -> Self {
        ParseIntError::from_std(&e)
    }
}

/// Wrapper for a ParseFloatError paired with the input string that failed.
/// Used by multiple Owned error enums that need to store both the error and input.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ParseFloatErrorWithInput {
    pub error: ParseFloatError,
    pub input: AzString,
}

/// Wrapper for WrongNumberOfComponents errors in CSS filter/transform parsing.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WrongComponentCountError {
    pub expected: usize,
    pub got: usize,
    pub input: AzString,
}

impl<'a> InvalidValueErr<'a> {
    pub fn to_contained(&self) -> InvalidValueErrOwned {
        InvalidValueErrOwned { value: self.0.to_string().into() }
    }
}

impl InvalidValueErrOwned {
    pub fn to_shared<'a>(&'a self) -> InvalidValueErr<'a> {
        InvalidValueErr(self.value.as_str())
    }
}
