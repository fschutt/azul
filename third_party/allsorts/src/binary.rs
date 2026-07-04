#![deny(missing_docs)]

//! Reading and writing of binary data.

pub mod read;
pub mod write;

/// Calculate the length required to 32-bit (long) align data of length `len`
///
/// Example:
///
/// ```
/// use allsorts::binary::long_align;
///
/// let length = 123;
/// let padded_length = long_align(length);
/// assert_eq!(padded_length, 124);
/// ```
pub const fn long_align(len: usize) -> usize {
    len.div_ceil(4) * 4
}

/// Calculate the length required to 16-bit (word) align data of length `len`
///
/// Example:
///
/// ```
/// use allsorts::binary::word_align;
///
/// let length = 123;
/// let padded_length = word_align(length);
/// assert_eq!(padded_length, 124);
/// ```
pub const fn word_align(len: usize) -> usize {
    len.div_ceil(2) * 2
}

/// Unsigned 8-bit binary type.
#[derive(Copy, Clone)]
pub enum U8 {}

/// Signed 8-bit binary type.
#[derive(Copy, Clone)]
pub enum I8 {}

/// Unsigned 16-bit big endian binary type.
#[derive(Copy, Clone)]
pub enum U16Be {}

/// Signed 16-bit big endian binary type.
#[derive(Copy, Clone)]
pub enum I16Be {}

/// Unsigned 24-bit (3 bytes) big endian binary type.
#[derive(Copy, Clone)]
pub enum U24Be {}

/// Unsigned 32-bit big endian binary type.
#[derive(Copy, Clone)]
pub enum U32Be {}

/// Signed 32-bit big endian binary type.
#[derive(Copy, Clone)]
pub enum I32Be {}

/// Unsigned 64-bit binary type.
#[derive(Copy, Clone)]
pub enum U64Be {}

/// Signed 64-bit binary type.
#[derive(Copy, Clone)]
pub enum I64Be {}
