//! Error types

use crate::binary::read::ReadEof;
use crate::tag::DisplayTag;
use std::fmt;

/// Error returned from font shaping functions
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ShapingError {
    ComplexScript(ComplexScriptError),
    Parse(ParseError),
}

impl From<ComplexScriptError> for ShapingError {
    fn from(error: ComplexScriptError) -> Self {
        ShapingError::ComplexScript(error)
    }
}

impl From<ParseError> for ShapingError {
    fn from(error: ParseError) -> Self {
        ShapingError::Parse(error)
    }
}

impl From<std::num::TryFromIntError> for ShapingError {
    fn from(_error: std::num::TryFromIntError) -> Self {
        ShapingError::Parse(ParseError::BadValue)
    }
}

impl fmt::Display for ShapingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapingError::ComplexScript(err) => write!(f, "indic shaping: {}", err),
            ShapingError::Parse(err) => write!(f, "shaping parse: {}", err),
        }
    }
}

impl std::error::Error for ShapingError {}

/// Error returned from font shaping complex scripts
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ComplexScriptError {
    EmptyBuffer,
    MissingBaseConsonant,
    MissingDottedCircle,
    MissingTags,
    UnexpectedGlyphOrigin,
}

/// Errors that originate when parsing binary data
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ParseError {
    BadEof,
    BadValue,
    BadVersion,
    BadOffset,
    BadIndex,
    LimitExceeded,
    MissingValue,
    MissingTable(u32),
    CompressionError,
    UnsuitableCmap,
    NotImplemented,
}

impl From<ReadEof> for ParseError {
    fn from(_error: ReadEof) -> Self {
        ParseError::BadEof
    }
}

impl From<std::num::TryFromIntError> for ParseError {
    fn from(_error: std::num::TryFromIntError) -> Self {
        ParseError::BadValue
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::BadEof => write!(f, "end of data reached unexpectedly"),
            ParseError::BadValue => write!(f, "invalid value"),
            ParseError::BadVersion => write!(f, "unexpected data version"),
            ParseError::BadOffset => write!(f, "invalid data offset"),
            ParseError::BadIndex => write!(f, "invalid data index"),
            ParseError::LimitExceeded => write!(f, "limit exceeded"),
            ParseError::MissingValue => write!(f, "an expected data value was missing"),
            ParseError::MissingTable(tag) => {
                write!(f, "font is missing '{}' table", DisplayTag(*tag))
            }
            ParseError::CompressionError => write!(f, "compression error"),
            ParseError::UnsuitableCmap => write!(f, "no suitable cmap subtable"),
            ParseError::NotImplemented => write!(f, "feature not implemented"),
        }
    }
}

impl std::error::Error for ParseError {}

impl fmt::Display for ComplexScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComplexScriptError::EmptyBuffer => write!(f, "empty buffer"),
            ComplexScriptError::MissingBaseConsonant => write!(f, "missing base consonant"),
            ComplexScriptError::MissingDottedCircle => write!(f, "missing dotted circle"),
            ComplexScriptError::MissingTags => write!(f, "missing tags"),
            ComplexScriptError::UnexpectedGlyphOrigin => write!(f, "unexpected glyph origin"),
        }
    }
}

impl std::error::Error for ComplexScriptError {}

/// Errors that originate when writing binary data
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum WriteError {
    BadValue,
    NotImplemented,
    PlaceholderMismatch,
}

impl From<std::num::TryFromIntError> for WriteError {
    fn from(_error: std::num::TryFromIntError) -> Self {
        WriteError::BadValue
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::BadValue => write!(f, "write: bad value"),
            WriteError::NotImplemented => write!(f, "writing in this format is not implemented"),
            WriteError::PlaceholderMismatch => {
                write!(f, "data written to placeholder did not match expected size")
            }
        }
    }
}

impl std::error::Error for WriteError {}

/// Enum that can hold read (`ParseError`) and write errors
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ReadWriteError {
    Read(ParseError),
    Write(WriteError),
}

impl From<ParseError> for ReadWriteError {
    fn from(error: ParseError) -> Self {
        ReadWriteError::Read(error)
    }
}

impl From<WriteError> for ReadWriteError {
    fn from(error: WriteError) -> Self {
        ReadWriteError::Write(error)
    }
}

impl fmt::Display for ReadWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadWriteError::Read(err) => write!(f, "read error: {}", err),
            ReadWriteError::Write(err) => write!(f, "write error: {}", err),
        }
    }
}

impl std::error::Error for ReadWriteError {}
