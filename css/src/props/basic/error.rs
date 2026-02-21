/// Simple "invalid value" error, used for basic parsing failures
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

/// Owned version of InvalidValueErr with String.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct InvalidValueErrOwned {
    pub value: String,
}

/// Wrapper for a ParseFloatError paired with the input string that failed.
/// Used by multiple Owned error enums that need to store both the error and input.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ParseFloatErrorWithInput {
    pub error: core::num::ParseFloatError,
    pub input: String,
}

/// Wrapper for WrongNumberOfComponents errors in CSS filter/transform parsing.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WrongComponentCountError {
    pub expected: usize,
    pub got: usize,
    pub input: String,
}

impl<'a> InvalidValueErr<'a> {
    pub fn to_contained(&self) -> InvalidValueErrOwned {
        InvalidValueErrOwned { value: self.0.to_string() }
    }
}

impl InvalidValueErrOwned {
    pub fn to_shared<'a>(&'a self) -> InvalidValueErr<'a> {
        InvalidValueErr(&self.value)
    }
}
