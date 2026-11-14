//! The value model of the profile: unsigned and negative integers, byte strings, text strings,
//! arrays, maps, `true`, `false`, `null`. Nothing else exists, so nothing else can be signed.
//!
//! Implements planning/architecture.md section 5.1 principle 6 (deterministic CBOR, RFC 8949
//! section 4.2). P-1 of the brief: no floats, tags or other simple values: the formats of
//! section 5 are integers, bytes, text, arrays and maps (sizes, times and epochs are integers).

use crate::{
    enc,
    error::{EncodeError, MapError},
    limits::Limits,
};

/// One CBOR data item of the profile.
///
/// Integers: `Uint(n)` is the integer `n`; `Nint(n)` is the integer `-1 - n`, so `Nint(0)` is -1
/// and `Nint(u64::MAX)` is -2^64 (the whole range of CBOR major types 0 and 1).
///
/// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...), INT-WIRE (azlin-proto
/// contract4).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// The integer `n` (major type 0).
    Uint(u64),
    /// The integer `-1 - n` (major type 1).
    Nint(u64),
    /// A byte string (major type 2).
    Bytes(Vec<u8>),
    /// A text string (major type 3): valid UTF-8, never normalized.
    Text(String),
    /// An array (major type 4).
    Array(Vec<Value>),
    /// A map (major type 5) with its keys in canonical order.
    Map(Map),
    /// `true` or `false` (major type 7).
    Bool(bool),
    /// `null` (major type 7).
    Null,
}

impl Value {
    /// The integer, if this is `Uint`.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn as_uint(&self) -> Option<u64> {
        match self {
            Value::Uint(n) => Some(*n),
            _ => None,
        }
    }

    /// The bytes, if this is `Bytes`.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b.as_slice()),
            _ => None,
        }
    }

    /// The text, if this is `Text`.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// The items, if this is `Array`.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// The map, if this is `Map`.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// The boolean, if this is `Bool`.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Whether this is `Null`.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

/// The entries of a map, sorted by the bytewise order of the ENCODED key (RFC 8949 section 4.2.1)
/// and unique. The field is private and the only constructors keep that invariant (`from_entries`
/// sorts and refuses duplicates; the decoder refuses unsorted and duplicate input), so a `Map`
/// cannot be encoded in a non-deterministic way.
///
/// Keys may be any `Value` (P-2 of the brief: a map of xattr names to bytes has text keys; the
/// structs of section 5 have unsigned-integer keys and are read through `IntMap`).
///
/// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...), INT-WIRE.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Map(Vec<(Value, Value)>);

impl Map {
    /// A map of these entries: sorted by the encoded key, in the order every encoder and decoder
    /// of the profile agrees on. The same key twice is refused, however far apart the entries are.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...), `IntMapBuilder`.
    pub fn from_entries(entries: Vec<(Value, Value)>) -> Result<Map, MapError> {
        // The order is the order of the encoded bytes, so the keys are encoded once and sorted by
        // those bytes: the sort cannot disagree with what the decoder compares.
        let mut keyed: Vec<(Vec<u8>, Value, Value)> = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            let bytes = enc::encode(&key, &Limits::KEY).map_err(|e| match e {
                EncodeError::DepthExceeded => MapError::KeyTooDeep,
                EncodeError::TooManyItems | EncodeError::InputTooLarge => MapError::KeyTooLarge,
            })?;
            keyed.push((bytes, key, value));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        for pair in keyed.windows(2) {
            if let [a, b] = pair {
                if a.0 == b.0 {
                    return Err(MapError::DuplicateKey);
                }
            }
        }
        Ok(Map(keyed.into_iter().map(|(_, k, v)| (k, v)).collect()))
    }

    /// The map the decoder built from input it has checked to be sorted and unique.
    pub(crate) fn from_sorted(entries: Vec<(Value, Value)>) -> Map {
        Map(entries)
    }

    /// The value of `key`. A linear search: the maps of section 5 are small structs.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn get(&self, key: &Value) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// The entries in canonical key order.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn iter(&self) -> std::slice::Iter<'_, (Value, Value)> {
        self.0.iter()
    }

    /// The number of entries.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no entries.
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The entries, moved out (a caller that keeps the unknown fields of a structure takes them
    /// without copying their bytes).
    ///
    /// Caller: every format crate (azlin-keys, azlin-content, azlin-creds, ...).
    pub fn into_entries(self) -> Vec<(Value, Value)> {
        self.0
    }

    /// The entries as a slice, in canonical key order.
    pub(crate) fn entries(&self) -> &[(Value, Value)] {
        self.0.as_slice()
    }
}

impl<'a> IntoIterator for &'a Map {
    type Item = &'a (Value, Value);
    type IntoIter = std::slice::Iter<'a, (Value, Value)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
