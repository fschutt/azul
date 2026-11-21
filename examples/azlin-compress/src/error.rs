//! What can go wrong. Decoding never panics on bad input: every check returns one of these.

use std::fmt;

/// Why an encode or a decode failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The bytes are not what this crate wrote, or they were changed or cut off.
    Damaged(String),
    /// Written by a newer version: an unknown format version, flag, transform, codec or
    /// dictionary. An old reader refuses these by name instead of guessing.
    Unsupported(String),
    /// Over a limit: a size, the memory cap, a decompression bomb.
    Limit(String),
    /// The reader or the writer failed.
    Io(String),
    /// A codec library refused its input (zstd, brotli, Lepton).
    Codec(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Damaged(why) => write!(f, "damaged compressed data: {why}"),
            Error::Unsupported(what) => write!(f, "written by a newer version: {what}"),
            Error::Limit(what) => write!(f, "over a limit: {what}"),
            Error::Io(why) => write!(f, "i/o: {why}"),
            Error::Codec(why) => write!(f, "codec: {why}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e.to_string())
    }
}

pub(crate) fn damaged(why: impl Into<String>) -> Error {
    Error::Damaged(why.into())
}
