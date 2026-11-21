//! The errors of the codec and of its readers: one small `Copy` enum per stage, no strings, so a
//! caller maps each to its own status (`bad_request`, a damaged object, a refused signature).
//!
//! Implements planning/architecture.md section 5.1 principle 6 (strict, extensible formats) and
//! section 1.4 rule 4 (every limit refuses with a typed error).

use core::fmt;

/// Why `decode` refused its input. STRICT is the only mode: every variant but `Truncated` and the
/// limit variants names a way the input is not the one deterministic encoding of its value
/// (RFC 8949 section 4.2.1), so no refused input can be signed over or re-signed by accident.
///
/// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...) maps it to its own
/// error; INT-WIRE (azlin-proto contract4) maps it to `bad_request`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecodeError {
    /// The input ends inside an item (also: the empty input).
    Truncated,
    /// A complete item is followed by more bytes: the whole input must be one item.
    TrailingBytes,
    /// An integer, or the length of a string, array or map, uses a longer form than its value needs
    /// (`0x1800`, `0x190018`, `0x5800`, `0x9800`): the shortest form is the only one.
    NotShortest,
    /// An indefinite-length string, array or map (`0x5f`, `0x7f`, `0x9f`, `0xbf`).
    IndefiniteLength,
    /// The keys of a map are not in ascending order of their encoded bytes (RFC 8949 section 4.2.1).
    UnsortedMapKeys,
    /// A map holds the same key twice (its encoded bytes are equal).
    DuplicateMapKey,
    /// An item outside the profile: a float, a tag, `undefined`, another simple value, or an
    /// initial byte that the RFC reserves (additional information 28 to 30, or 31 on an integer).
    DisallowedType,
    /// A text string is not valid UTF-8 (overlong forms, surrogate halves, values above U+10FFFF,
    /// truncated sequences). No normalization is applied: NFC and NFD text stay distinct values.
    InvalidUtf8,
    /// More arrays and maps are nested than `Limits::max_depth` allows.
    DepthExceeded,
    /// An array or map has more items than `Limits::max_items`, or the item count of the whole
    /// input exceeds `Limits::max_total_items`.
    TooManyItems,
    /// The input is longer than `Limits::max_input`, or one string claims more than
    /// `Limits::max_text` bytes.
    InputTooLarge,
    /// A lone `break` (`0xff`) where an item is expected.
    ReservedBreak,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DecodeError::Truncated => "the input ends inside an item",
            DecodeError::TrailingBytes => "bytes follow the item",
            DecodeError::NotShortest => "an integer or length is not in its shortest form",
            DecodeError::IndefiniteLength => "indefinite lengths are not in the profile",
            DecodeError::UnsortedMapKeys => "map keys are not in encoded-byte order",
            DecodeError::DuplicateMapKey => "a map key appears twice",
            DecodeError::DisallowedType => "a float, tag or simple value is not in the profile",
            DecodeError::InvalidUtf8 => "a text string is not valid UTF-8",
            DecodeError::DepthExceeded => "nesting is deeper than the limit",
            DecodeError::TooManyItems => "more items than the limit",
            DecodeError::InputTooLarge => "the input or a string is larger than the limit",
            DecodeError::ReservedBreak => "a break where an item is expected",
        })
    }
}

impl std::error::Error for DecodeError {}

/// Why `encode` refused a value: the value breaks a `Limits` field, so it would not be accepted
/// by `decode` under the same limits. The variants mirror the limit variants of [`DecodeError`].
///
/// Caller: every format crate that writes a format (azlin-keys, azlin-content, azlin-creds, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EncodeError {
    /// Arrays and maps are nested deeper than `Limits::max_depth`.
    DepthExceeded,
    /// An array or map has more items than `Limits::max_items`, or the value has more items in
    /// all than `Limits::max_total_items`.
    TooManyItems,
    /// The encoding would be longer than `Limits::max_input`, or one string is longer than
    /// `Limits::max_text`.
    InputTooLarge,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EncodeError::DepthExceeded => "nesting is deeper than the limit",
            EncodeError::TooManyItems => "more items than the limit",
            EncodeError::InputTooLarge => "the encoding or a string is larger than the limit",
        })
    }
}

impl std::error::Error for EncodeError {}

/// Why a [`Map`](crate::Map) or an [`IntMapBuilder`](crate::IntMapBuilder) could not be built.
///
/// Caller: every format crate that writes a format (through `IntMapBuilder::build` and
/// `Map::from_entries`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapError {
    /// Two entries have the same key (the same encoded bytes).
    DuplicateKey,
    /// `IntMapBuilder::put(0, ..)`: key 0 is `crit` and is written only through `crit()`.
    ReservedKey,
    /// `IntMapBuilder::crit` named key 0: `crit` cannot list itself.
    BadCrit,
    /// A key is nested deeper than 255 levels, the most any `Limits` allows: no encoding exists
    /// to order it by.
    KeyTooDeep,
    /// A key is larger than any `Limits` allows (more than `u32::MAX` bytes or items).
    KeyTooLarge,
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MapError::DuplicateKey => "a map key appears twice",
            MapError::ReservedKey => "key 0 is `crit` and is set only through crit()",
            MapError::BadCrit => "`crit` cannot name key 0",
            MapError::KeyTooDeep => "a map key is nested deeper than any limit allows",
            MapError::KeyTooLarge => "a map key is larger than any limit allows",
        })
    }
}

impl std::error::Error for MapError {}

/// Why an [`IntMap`](crate::IntMap) refused a value: the shape of a struct of section 5 is wrong.
/// `KeyReadTwice` and `ReservedKey` are programming errors of the reader, returned (never a panic)
/// so that a fuzzer or a test finds them.
///
/// Caller: every format crate that reads a format (azlin-keys, azlin-content, azlin-creds, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeError {
    /// Not a map, or a map with a key that is not an unsigned integer.
    NotAnIntMap,
    /// Programming error: an accessor was called for key 0, which is `crit` and is read by
    /// `finish` only.
    ReservedKey,
    /// Programming error: one key was read through the same `IntMap` twice.
    KeyReadTwice,
    /// A required key is absent.
    Missing(u64),
    /// The key holds another type than the accessor asks for (text for `req_u64`, a negative
    /// integer for `req_u64`, ...).
    WrongType(u64),
    /// A byte string has another length than `req_bytes_n::<N>` asks for.
    WrongLength(u64),
    /// Key 0 is not a valid `crit`: not an array, empty, not ascending, a duplicate, a non-integer,
    /// or it lists 0.
    BadCrit,
    /// A key listed in `crit` was never asked for by the reader: the sender requires something this
    /// receiver does not understand, so the message is refused.
    CritUnknown(u64),
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeError::NotAnIntMap => f.write_str("not a map with unsigned-integer keys"),
            ShapeError::ReservedKey => f.write_str("key 0 is `crit`, read by finish() only"),
            ShapeError::KeyReadTwice => f.write_str("a key was read twice"),
            ShapeError::Missing(k) => write!(f, "key {k} is missing"),
            ShapeError::WrongType(k) => write!(f, "key {k} has the wrong type"),
            ShapeError::WrongLength(k) => write!(f, "key {k} has the wrong length"),
            ShapeError::BadCrit => {
                f.write_str("`crit` (key 0) is not a valid must-understand list")
            }
            ShapeError::CritUnknown(k) => {
                write!(f, "key {k} is required by `crit` and not understood")
            }
        }
    }
}

impl std::error::Error for ShapeError {}
