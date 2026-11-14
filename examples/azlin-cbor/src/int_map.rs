//! The reader and the builder of every struct of section 5: a map with unsigned-integer keys,
//! key 0 = `crit`.
//!
//! Implements planning/architecture.md section 5.1 principle 6 ("unknown fields ignored,
//! must-understand fields listed in `crit`") and section 2.5 ("Key 0 (`crit`) lists keys the
//! receiver must understand, or refuse"). The reader returns the unknown fields so that a caller
//! that copies a structure keeps them: that is what makes "ignore unknown fields" compatible with
//! a signature over the bytes (a copy re-encodes byte for byte).

use std::collections::BTreeSet;

use crate::{
    error::{MapError, ShapeError},
    value::{Map, Value},
};

/// The reader of one struct. Every `req_` and `opt_` call registers its key as UNDERSTOOD (also
/// when the key is absent); `finish` then applies `crit` and returns the keys nobody asked for.
///
/// The rule of `crit` (strict; DRIFT X-DELTA-W1S14A-2, P-4 of the brief): key 0, when present, is
/// a non-empty array of distinct unsigned integers in ascending order, none of them 0; anything
/// else is `ShapeError::BadCrit`. A key listed there that the reader never asked for is
/// `CritUnknown`; a listed key that is absent from the map is fine (the receiver must understand
/// the key, not receive it).
///
/// A reader that forgets `finish` skips `crit`: always end with `finish` (it consumes the reader).
///
/// Caller: every format crate that reads a format (azlin-keys, azlin-content, azlin-creds, ...),
/// INT-WIRE (azlin-proto contract4).
#[derive(Debug)]
pub struct IntMap<'a> {
    entries: &'a [(Value, Value)],
    /// The key of each entry, strictly ascending (the order of a `Map` with unsigned keys).
    keys: Vec<u64>,
    /// The keys asked for so far, present or not.
    asked: BTreeSet<u64>,
}

impl<'a> IntMap<'a> {
    /// A reader of `v`, which must be a map whose keys are all unsigned integers.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn new(v: &'a Value) -> Result<IntMap<'a>, ShapeError> {
        let Value::Map(map) = v else {
            return Err(ShapeError::NotAnIntMap);
        };
        let entries = map.entries();
        let mut keys: Vec<u64> = Vec::with_capacity(entries.len());
        for (key, _) in entries {
            match key {
                Value::Uint(n) => keys.push(*n),
                _ => return Err(ShapeError::NotAnIntMap),
            }
        }
        // A `Map` keeps unsigned keys strictly ascending; the search below relies on it, so it is
        // checked here once instead of trusted.
        if !keys.windows(2).all(|w| matches!(w, [a, b] if a < b)) {
            return Err(ShapeError::NotAnIntMap);
        }
        Ok(IntMap {
            entries,
            keys,
            asked: BTreeSet::new(),
        })
    }

    /// The value of key `k`, registering the key as understood.
    fn ask(&mut self, k: u64) -> Result<Option<&'a Value>, ShapeError> {
        if k == 0 {
            return Err(ShapeError::ReservedKey);
        }
        if !self.asked.insert(k) {
            return Err(ShapeError::KeyReadTwice);
        }
        Ok(self.lookup(k))
    }

    fn lookup(&self, k: u64) -> Option<&'a Value> {
        let i = self.keys.binary_search(&k).ok()?;
        self.entries.get(i).map(|(_, v)| v)
    }

    /// A required key: `Missing(k)` when absent.
    fn require(&mut self, k: u64) -> Result<&'a Value, ShapeError> {
        self.ask(k)?.ok_or(ShapeError::Missing(k))
    }

    /// A required unsigned integer. A text, a negative integer or any other type is `WrongType`.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_u64(&mut self, k: u64) -> Result<u64, ShapeError> {
        let v = self.require(k)?;
        v.as_uint().ok_or(ShapeError::WrongType(k))
    }

    /// An optional unsigned integer.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_u64(&mut self, k: u64) -> Result<Option<u64>, ShapeError> {
        match self.ask(k)? {
            None => Ok(None),
            Some(v) => v.as_uint().map(Some).ok_or(ShapeError::WrongType(k)),
        }
    }

    /// A required byte string.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_bytes(&mut self, k: u64) -> Result<&'a [u8], ShapeError> {
        let v = self.require(k)?;
        v.as_bytes().ok_or(ShapeError::WrongType(k))
    }

    /// An optional byte string.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_bytes(&mut self, k: u64) -> Result<Option<&'a [u8]>, ShapeError> {
        match self.ask(k)? {
            None => Ok(None),
            Some(v) => v.as_bytes().map(Some).ok_or(ShapeError::WrongType(k)),
        }
    }

    /// A required byte string of exactly `N` bytes (a hash, a key, a nonce): another length is
    /// `WrongLength`.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_bytes_n<const N: usize>(&mut self, k: u64) -> Result<[u8; N], ShapeError> {
        let bytes = self.req_bytes(k)?;
        <[u8; N]>::try_from(bytes).map_err(|_| ShapeError::WrongLength(k))
    }

    /// An optional byte string of exactly `N` bytes.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_bytes_n<const N: usize>(&mut self, k: u64) -> Result<Option<[u8; N]>, ShapeError> {
        match self.opt_bytes(k)? {
            None => Ok(None),
            Some(bytes) => <[u8; N]>::try_from(bytes)
                .map(Some)
                .map_err(|_| ShapeError::WrongLength(k)),
        }
    }

    /// A required text string.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_text(&mut self, k: u64) -> Result<&'a str, ShapeError> {
        let v = self.require(k)?;
        v.as_text().ok_or(ShapeError::WrongType(k))
    }

    /// An optional text string.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_text(&mut self, k: u64) -> Result<Option<&'a str>, ShapeError> {
        match self.ask(k)? {
            None => Ok(None),
            Some(v) => v.as_text().map(Some).ok_or(ShapeError::WrongType(k)),
        }
    }

    /// A required array.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_array(&mut self, k: u64) -> Result<&'a [Value], ShapeError> {
        let v = self.require(k)?;
        v.as_array().ok_or(ShapeError::WrongType(k))
    }

    /// An optional array.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_array(&mut self, k: u64) -> Result<Option<&'a [Value]>, ShapeError> {
        match self.ask(k)? {
            None => Ok(None),
            Some(v) => v.as_array().map(Some).ok_or(ShapeError::WrongType(k)),
        }
    }

    /// A required map (a table, or a nested struct: read it with `IntMap::new` on `req_value`).
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_map(&mut self, k: u64) -> Result<&'a Map, ShapeError> {
        let v = self.require(k)?;
        v.as_map().ok_or(ShapeError::WrongType(k))
    }

    /// An optional map.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_map(&mut self, k: u64) -> Result<Option<&'a Map>, ShapeError> {
        match self.ask(k)? {
            None => Ok(None),
            Some(v) => v.as_map().map(Some).ok_or(ShapeError::WrongType(k)),
        }
    }

    /// A required boolean.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_bool(&mut self, k: u64) -> Result<bool, ShapeError> {
        let v = self.require(k)?;
        v.as_bool().ok_or(ShapeError::WrongType(k))
    }

    /// An optional boolean.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_bool(&mut self, k: u64) -> Result<Option<bool>, ShapeError> {
        match self.ask(k)? {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or(ShapeError::WrongType(k)),
        }
    }

    /// A required value of any type (the nested struct under key `k`, for `IntMap::new`).
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn req_value(&mut self, k: u64) -> Result<&'a Value, ShapeError> {
        self.require(k)
    }

    /// An optional value of any type.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    pub fn opt_value(&mut self, k: u64) -> Result<Option<&'a Value>, ShapeError> {
        self.ask(k)
    }

    /// The `crit` list of the map: empty when key 0 is absent, `BadCrit` when it is present and
    /// not a non-empty ascending array of distinct unsigned integers above 0. A caller that copies
    /// a structure re-writes it with `IntMapBuilder::crit`, so the copy keeps its must-understand
    /// markers (DRIFT X-DELTA-W1S14A-7: `finish` does not return key 0).
    ///
    /// Caller: every format crate that copies or re-signs a structure (azlin-keys, azlin-content).
    pub fn crit_keys(&self) -> Result<Vec<u64>, ShapeError> {
        // DRIFT X-DELTA-W1S14A-2: the plan gives no shape for `crit`; this is P-4 of the brief.
        let Some(v) = self.lookup(0) else {
            return Ok(Vec::new());
        };
        let Value::Array(items) = v else {
            return Err(ShapeError::BadCrit);
        };
        if items.is_empty() {
            return Err(ShapeError::BadCrit);
        }
        let mut keys: Vec<u64> = Vec::with_capacity(items.len());
        for item in items {
            let Value::Uint(k) = item else {
                return Err(ShapeError::BadCrit);
            };
            let ascending = keys.last().is_none_or(|last| last < k);
            if *k == 0 || !ascending {
                return Err(ShapeError::BadCrit);
            }
            keys.push(*k);
        }
        Ok(keys)
    }

    /// Ends the reading: applies `crit` (`BadCrit` when it is malformed; `CritUnknown(k)` for the
    /// lowest listed key that was never asked for) and returns the entries nobody asked for, in
    /// key order, so the caller can KEEP them (section 5.1 principle 6: unknown keys that are not
    /// in `crit` are ignored). Key 0 itself is not among them: see `crit_keys`.
    ///
    /// Caller: every format crate that reads a format (azlin-keys, azlin-content, ...).
    #[must_use = "an ignored result skips the check of `crit`"]
    pub fn finish(self) -> Result<Vec<(&'a Value, &'a Value)>, ShapeError> {
        for k in self.crit_keys()? {
            if !self.asked.contains(&k) {
                return Err(ShapeError::CritUnknown(k));
            }
        }
        Ok(self
            .entries
            .iter()
            .zip(self.keys.iter())
            .filter(|(_, k)| **k != 0 && !self.asked.contains(*k))
            .map(|((key, value), _)| (key, value))
            .collect())
    }
}

/// The writer of one struct: integer keys and values, `crit` through its own method. The output
/// does not depend on the order of the calls (the map sorts its keys), so two writers that put the
/// same fields write the same bytes.
///
/// Caller: every format crate that writes a format (azlin-keys, azlin-content, azlin-creds, ...),
/// INT-WIRE (azlin-proto contract4).
#[derive(Clone, Debug, Default)]
pub struct IntMapBuilder {
    entries: Vec<(u64, Value)>,
    crit: BTreeSet<u64>,
}

impl IntMapBuilder {
    /// An empty struct.
    ///
    /// Caller: every format crate that writes a format (azlin-keys, azlin-content, ...).
    pub fn new() -> Self {
        IntMapBuilder::default()
    }

    /// Sets key `k`. Key 0 is refused at `build` (it is `crit`'s, set through `crit()`), and so is
    /// a key that is put twice.
    ///
    /// Caller: every format crate that writes a format (azlin-keys, azlin-content, ...).
    #[must_use]
    pub fn put(mut self, k: u64, v: Value) -> Self {
        self.entries.push((k, v));
        self
    }

    /// Lists keys the receiver must understand, or refuse. The keys are sorted and made unique
    /// here, and several calls add up; `crit` is written (as key 0) only when something is listed.
    /// Listing a key that is never put is fine; listing 0 is refused at `build`.
    ///
    /// Caller: every format crate that writes a format (azlin-keys, azlin-content, ...).
    #[must_use]
    pub fn crit(mut self, keys: &[u64]) -> Self {
        self.crit.extend(keys.iter().copied());
        self
    }

    /// The map: `MapError::ReservedKey` for a `put(0, ..)`, `BadCrit` for a `crit` that lists 0,
    /// `DuplicateKey` for a key put twice.
    ///
    /// Caller: every format crate that writes a format (azlin-keys, azlin-content, ...).
    pub fn build(self) -> Result<Value, MapError> {
        let mut entries: Vec<(Value, Value)> =
            Vec::with_capacity(self.entries.len().saturating_add(1));
        for (k, v) in self.entries {
            if k == 0 {
                return Err(MapError::ReservedKey);
            }
            entries.push((Value::Uint(k), v));
        }
        if self.crit.contains(&0) {
            return Err(MapError::BadCrit);
        }
        if !self.crit.is_empty() {
            let listed = self.crit.iter().map(|k| Value::Uint(*k)).collect();
            entries.push((Value::Uint(0), Value::Array(listed)));
        }
        Ok(Value::Map(Map::from_entries(entries)?))
    }
}
