//! Numbers somebody else wrote: what an object, a header or a length may be before this device
//! spends memory or time on it (ARCH28 RF50 / AR5b-5; the read side of RF48's `get_limited`).
//!
//! A [`Limit`] is a named maximum. The name is what the error says ("a key file may be at most
//! 65536 bytes"), so a refusal tells which rule held, not only that a number was too big. The
//! types that carry a declared length through a parser (`Declared`, `ByteReader`) build on it
//! in the work order that owns the parsers; [`crate::Drive::get_limited`] needs only this much.

use std::fmt;

use crate::DriveError;

/// A named maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limit {
    /// What the limit is for, as an error names it: `"a key file"`.
    pub name: &'static str,
    /// The most bytes.
    pub max: u64,
}

impl Limit {
    #[must_use]
    pub const fn new(name: &'static str, max: u64) -> Limit {
        Limit { name, max }
    }

    /// `got` when it is within the limit, else the error that names the limit.
    ///
    /// # Errors
    ///
    /// [`LimitError`] when `got` is more than [`Limit::max`].
    pub fn within(self, got: u64) -> Result<u64, LimitError> {
        if got > self.max {
            Err(LimitError { limit: self, got })
        } else {
            Ok(got)
        }
    }

    /// [`Limit::within`] for the object `key` of a drive: the refusal is a [`DriveError`] that
    /// names the key and the limit.
    ///
    /// # Errors
    ///
    /// [`DriveError::Protocol`] when `got` is more than [`Limit::max`].
    pub fn check_object(self, key: &str, got: u64) -> Result<u64, DriveError> {
        self.within(got).map_err(|e| e.of_object(key))
    }
}

/// A length past its [`Limit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitError {
    pub limit: Limit,
    /// The length that was declared or found.
    pub got: u64,
}

impl LimitError {
    /// As the error of a drive call on `key`.
    #[must_use]
    pub fn of_object(self, key: &str) -> DriveError {
        DriveError::Protocol(format!("\"{key}\": {self}"))
    }
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} may be at most {} bytes, this one is {}",
            self.limit.name, self.limit.max, self.got
        )
    }
}

impl std::error::Error for LimitError {}

impl From<LimitError> for DriveError {
    fn from(e: LimitError) -> Self {
        DriveError::Protocol(e.to_string())
    }
}
