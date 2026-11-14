//! The wire form of the pointer: the one deterministic CBOR encoding of a [`PointerV2`] and the strict
//! reader of it (planning/architecture.md section 5.1 principle 6 and section 5.7 "Pointer v2"; build
//! unit S14, lane W1-S14d). The layout is in the documentation of the `pointer` module.
//!
//! - READING goes through `azlin_cbor::decode_secret` (rule R17): the chunk hashes `full` are key
//!   material, so every byte buffer of the decoded tree is wiped when it drops, and each `full` is
//!   handed to `Full::from_slice` at once. After the fields are read and checked, the pointer is
//!   WRITTEN again and the bytes must be the input: a pointer that is accepted is in its one canonical
//!   encoding, so the signature over the encoding is a signature over the bytes that were received
//!   (DRIFT X-DELTA-W1S14D-10).
//! - WRITING builds a tree and wraps it in a `SecretValue` before it is encoded (its buffers hold the
//!   hashes), and the encoder writes into a buffer of exactly the right size, which is then wrapped in
//!   `Zeroizing`: no growth copies a hash to a second allocation.
//! - A `Vec` of chunk references is always created with its exact capacity (the hashes live inside the
//!   elements, so a growth would leave an unwiped copy behind).

use azlin_cbor::{
    decode_secret, encode, IntMap, IntMapBuilder, Limits, SecretValue, ShapeError, Value,
};
use azlin_keys::{DeviceId, Signature, UnixTime};
use zeroize::Zeroizing;

use super::error::{Limit, Part, PointerError};
use super::{
    ChunkList, ChunkListRef, Decoded, DerivedRef, PointerV2, Preview, StreamRef, TransformRef,
    UnknownFields, INLINE_MAX, MAX_DERIVED, MAX_LABELS, MAX_PREVIEWS, MAX_PREVIEW_CHUNKS,
    MAX_STREAMS, MAX_STREAM_CHUNKS, MAX_XATTRS, VERSION,
};
use crate::error::ChunkError;
use crate::ids::{ChunkRef, Full, PackId};
use crate::units::{ChunkLen, FileSize, FileTime, PackOffset};

// The keys of the pointer's map (the table in the documentation of the `pointer` module).
const K_V: u64 = 1;
const K_SIZE: u64 = 2;
const K_MTIME: u64 = 3;
const K_MODE: u64 = 4;
const K_BLAKE3: u64 = 5;
const K_TRANSFORM: u64 = 6;
const K_CHUNKS: u64 = 7;
const K_AUTHOR: u64 = 8;
const K_PREVIEWS: u64 = 9;
const K_STREAMS: u64 = 10;
const K_XATTRS: u64 = 11;
const K_LABELS: u64 = 12;
const K_DERIVED: u64 = 13;
const K_SIG: u64 = 14;
const K_CREATED_AT: u64 = 15;

// The keys of the chunk-list reference `{1: name, 2: count, 3: total_raw}`.
const R_NAME: u64 = 1;
const R_COUNT: u64 = 2;
const R_TOTAL: u64 = 3;

// ---------------------------------------------------------------------------------------------
// Chunk references
// ---------------------------------------------------------------------------------------------

/// One chunk reference as a CBOR value: `[full, raw_len, pack, offset, stored_len]`. The value holds a
/// copy of the hash: it belongs inside a `SecretValue` before it is encoded.
pub(super) fn ref_to_value(r: &ChunkRef) -> Value {
    Value::Array(vec![
        Value::Bytes(r.full.expose().to_vec()),
        Value::Uint(u64::from(r.raw_len.get())),
        Value::Bytes(r.pack.as_bytes().to_vec()),
        Value::Uint(u64::from(r.offset.get())),
        Value::Uint(u64::from(r.stored_len.get())),
    ])
}

/// A list of chunk references as a CBOR array (the capacity is exact).
fn refs_to_value(refs: &[ChunkRef]) -> Value {
    let mut items = Vec::with_capacity(refs.len());
    for r in refs {
        items.push(ref_to_value(r));
    }
    Value::Array(items)
}

/// The chunk length `v`: an unsigned integer that fits a `ChunkLen`. A zero is the error `zero` names
/// (a chunk has at least one byte), a length above the largest blob is `TooLong`.
fn chunk_len(v: &Value, zero: ChunkError) -> Result<ChunkLen, PointerError> {
    let Value::Uint(n) = v else {
        return Err(PointerError::BadShape(Part::ChunkRef));
    };
    match u32::try_from(*n) {
        Ok(0) => Err(PointerError::BadChunk(zero)),
        Ok(bytes) => ChunkLen::new(bytes).ok_or(PointerError::BadChunk(ChunkError::TooLong)),
        Err(_) => Err(PointerError::BadChunk(ChunkError::TooLong)),
    }
}

/// One chunk reference read from a CBOR value and checked (`ChunkRef::check`). The hash is copied out
/// of the tree into a `Full` at once (rule R17).
pub(super) fn ref_from_value(v: &Value) -> Result<ChunkRef, PointerError> {
    let Value::Array(items) = v else {
        return Err(PointerError::BadShape(Part::ChunkRef));
    };
    let [full, raw_len, pack, offset, stored_len] = items.as_slice() else {
        return Err(PointerError::BadShape(Part::ChunkRef));
    };
    let full = match full {
        Value::Bytes(bytes) => {
            Full::from_slice(bytes).ok_or(PointerError::BadShape(Part::ChunkRef))?
        }
        _ => return Err(PointerError::BadShape(Part::ChunkRef)),
    };
    let raw_len = chunk_len(raw_len, ChunkError::EmptyChunk)?;
    let pack = match pack {
        Value::Bytes(bytes) => match <[u8; PackId::LEN]>::try_from(bytes.as_slice()) {
            Ok(array) => PackId::from_bytes(array),
            Err(_) => return Err(PointerError::BadShape(Part::ChunkRef)),
        },
        _ => return Err(PointerError::BadShape(Part::ChunkRef)),
    };
    let offset = match offset {
        Value::Uint(n) => PackOffset::new(
            u32::try_from(*n).map_err(|_| PointerError::BadChunk(ChunkError::TooLong))?,
        ),
        _ => return Err(PointerError::BadShape(Part::ChunkRef)),
    };
    let stored_len = chunk_len(stored_len, ChunkError::Truncated)?;
    let r = ChunkRef {
        full,
        raw_len,
        pack,
        offset,
        stored_len,
    };
    r.check().map_err(PointerError::BadChunk)?;
    Ok(r)
}

/// The chunk references in a CBOR array, each checked. The vector has its exact capacity.
fn refs_from(items: &[Value]) -> Result<Vec<ChunkRef>, PointerError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(ref_from_value(item)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The small records
// ---------------------------------------------------------------------------------------------

/// `[seconds, nanoseconds]`: the seconds an unsigned integer, or a negative integer before 1970.
fn time_to_value(t: FileTime) -> Result<Value, PointerError> {
    let secs = t.secs();
    // For a negative `secs`, `!secs` is `-1 - secs`: the argument of CBOR's negative integer.
    let secs = match u64::try_from(secs) {
        Ok(n) => Value::Uint(n),
        Err(_) => Value::Nint(u64::try_from(!secs).map_err(|_| PointerError::BadTime)?),
    };
    Ok(Value::Array(vec![secs, Value::Uint(u64::from(t.nanos()))]))
}

fn time_from_value(v: &Value) -> Result<FileTime, PointerError> {
    let Value::Array(items) = v else {
        return Err(PointerError::BadShape(Part::Mtime));
    };
    let [secs, nanos] = items.as_slice() else {
        return Err(PointerError::BadShape(Part::Mtime));
    };
    let secs = match secs {
        Value::Uint(n) => i64::try_from(*n).map_err(|_| PointerError::BadTime)?,
        Value::Nint(n) => !i64::try_from(*n).map_err(|_| PointerError::BadTime)?,
        _ => return Err(PointerError::BadShape(Part::Mtime)),
    };
    let nanos = match nanos {
        Value::Uint(n) => u32::try_from(*n).map_err(|_| PointerError::BadTime)?,
        _ => return Err(PointerError::BadShape(Part::Mtime)),
    };
    FileTime::new(secs, nanos).ok_or(PointerError::BadTime)
}

fn transform_to_value(t: &TransformRef) -> Value {
    Value::Array(vec![
        Value::Uint(u64::from(t.id)),
        Value::Uint(t.out_len.get()),
        t.params.clone(),
    ])
}

fn transform_from_value(v: &Value) -> Result<TransformRef, PointerError> {
    let Value::Array(items) = v else {
        return Err(PointerError::BadShape(Part::Transform));
    };
    let [id, out_len, params] = items.as_slice() else {
        return Err(PointerError::BadShape(Part::Transform));
    };
    let id = match id {
        Value::Uint(n) => u16::try_from(*n).map_err(|_| PointerError::BadShape(Part::Transform))?,
        _ => return Err(PointerError::BadShape(Part::Transform)),
    };
    let out_len = match out_len {
        Value::Uint(n) => FileSize::new(*n),
        _ => return Err(PointerError::BadShape(Part::Transform)),
    };
    Ok(TransformRef {
        id,
        out_len,
        params: params.clone(),
    })
}

/// `{1: name, 2: count, 3: total_raw}`.
fn list_ref_to_value(r: &ChunkListRef) -> Result<Value, PointerError> {
    Ok(IntMapBuilder::new()
        .put(R_NAME, Value::Bytes(r.name.to_vec()))
        .put(R_COUNT, Value::Uint(u64::from(r.count)))
        .put(R_TOTAL, Value::Uint(r.total_raw.get()))
        .build()?)
}

/// The chunk-list reference: a map of exactly the keys 1, 2 and 3 (no unknown key, no `crit`).
fn list_ref_from_value(v: &Value) -> Result<ChunkListRef, PointerError> {
    let Value::Map(map) = v else {
        return Err(PointerError::BadShape(Part::ListRef));
    };
    if map.len() != 3 {
        return Err(PointerError::BadShape(Part::ListRef));
    }
    let name = match map.get(&Value::Uint(R_NAME)) {
        Some(Value::Bytes(bytes)) => <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| PointerError::BadShape(Part::ListRef))?,
        _ => return Err(PointerError::BadShape(Part::ListRef)),
    };
    let count = match map.get(&Value::Uint(R_COUNT)) {
        Some(Value::Uint(n)) => u32::try_from(*n).map_err(|_| PointerError::TooManyChunks)?,
        _ => return Err(PointerError::BadShape(Part::ListRef)),
    };
    let total_raw = match map.get(&Value::Uint(R_TOTAL)) {
        Some(Value::Uint(n)) => FileSize::new(*n),
        _ => return Err(PointerError::BadShape(Part::ListRef)),
    };
    Ok(ChunkListRef {
        name,
        count,
        total_raw,
    })
}

fn chunks_to_value(c: &ChunkList) -> Result<Value, PointerError> {
    match c {
        ChunkList::Inline(refs) => Ok(refs_to_value(refs)),
        ChunkList::Object(r) => list_ref_to_value(r),
    }
}

/// The `chunks` value: an array is the inline list (at most `INLINE_MAX_CHUNKS`), a map the reference
/// to the chunk-list object; anything else is the wrong type of key 7.
fn chunks_from_value(v: &Value) -> Result<ChunkList, PointerError> {
    match v {
        Value::Array(items) => {
            if items.len() > INLINE_MAX {
                return Err(PointerError::TooManyInline);
            }
            Ok(ChunkList::Inline(refs_from(items)?))
        }
        Value::Map(_) => Ok(ChunkList::Object(list_ref_from_value(v)?)),
        _ => Err(PointerError::Shape(ShapeError::WrongType(K_CHUNKS))),
    }
}

/// An unsigned integer that fits a `u8` (a kind), else the shape error of `part`.
fn small_u8(v: &Value, part: Part) -> Result<u8, PointerError> {
    match v {
        Value::Uint(n) => u8::try_from(*n).map_err(|_| PointerError::BadShape(part)),
        _ => Err(PointerError::BadShape(part)),
    }
}

/// An unsigned integer that fits a `u32` (a dimension, a bandwidth), else the shape error of `part`.
fn small_u32(v: &Value, part: Part) -> Result<u32, PointerError> {
    match v {
        Value::Uint(n) => u32::try_from(*n).map_err(|_| PointerError::BadShape(part)),
        _ => Err(PointerError::BadShape(part)),
    }
}

fn preview_to_value(p: &Preview) -> Value {
    Value::Array(vec![
        Value::Uint(u64::from(p.kind)),
        Value::Uint(u64::from(p.w)),
        Value::Uint(u64::from(p.h)),
        refs_to_value(&p.chunks),
    ])
}

fn preview_from_value(v: &Value) -> Result<Preview, PointerError> {
    let Value::Array(items) = v else {
        return Err(PointerError::BadShape(Part::Preview));
    };
    let [kind, w, h, chunks] = items.as_slice() else {
        return Err(PointerError::BadShape(Part::Preview));
    };
    let kind = small_u8(kind, Part::Preview)?;
    let w = small_u32(w, Part::Preview)?;
    let h = small_u32(h, Part::Preview)?;
    let Value::Array(refs) = chunks else {
        return Err(PointerError::BadShape(Part::Preview));
    };
    if refs.is_empty() {
        return Err(PointerError::EmptyList(Part::Preview));
    }
    if refs.len() > MAX_PREVIEW_CHUNKS {
        return Err(PointerError::OverLimit(Limit::PreviewChunks));
    }
    Ok(Preview {
        kind,
        w,
        h,
        chunks: refs_from(refs)?,
    })
}

fn stream_to_value(s: &StreamRef) -> Value {
    Value::Array(vec![
        Value::Uint(u64::from(s.kind)),
        Value::Uint(u64::from(s.w)),
        Value::Uint(u64::from(s.h)),
        Value::Uint(u64::from(s.bandwidth)),
        refs_to_value(&s.chunks),
    ])
}

fn stream_from_value(v: &Value) -> Result<StreamRef, PointerError> {
    let Value::Array(items) = v else {
        return Err(PointerError::BadShape(Part::Stream));
    };
    let [kind, w, h, bandwidth, chunks] = items.as_slice() else {
        return Err(PointerError::BadShape(Part::Stream));
    };
    let kind = small_u8(kind, Part::Stream)?;
    let w = small_u32(w, Part::Stream)?;
    let h = small_u32(h, Part::Stream)?;
    let bandwidth = small_u32(bandwidth, Part::Stream)?;
    let Value::Array(refs) = chunks else {
        return Err(PointerError::BadShape(Part::Stream));
    };
    if refs.is_empty() {
        return Err(PointerError::EmptyList(Part::Stream));
    }
    if refs.len() > MAX_STREAM_CHUNKS {
        return Err(PointerError::OverLimit(Limit::StreamChunks));
    }
    Ok(StreamRef {
        kind,
        w,
        h,
        bandwidth,
        chunks: refs_from(refs)?,
    })
}

fn xattr_to_value(name: &str, value: &[u8]) -> Value {
    Value::Array(vec![
        Value::Text(name.to_owned()),
        Value::Bytes(value.to_vec()),
    ])
}

fn xattr_from_value(v: &Value) -> Result<(String, Vec<u8>), PointerError> {
    let Value::Array(items) = v else {
        return Err(PointerError::BadShape(Part::Xattr));
    };
    let [name, value] = items.as_slice() else {
        return Err(PointerError::BadShape(Part::Xattr));
    };
    match (name, value) {
        (Value::Text(name), Value::Bytes(value)) => Ok((name.clone(), value.clone())),
        _ => Err(PointerError::BadShape(Part::Xattr)),
    }
}

fn label_from_value(v: &Value) -> Result<String, PointerError> {
    match v {
        Value::Text(text) => Ok(text.clone()),
        _ => Err(PointerError::BadShape(Part::Label)),
    }
}

fn derived_to_value(d: &DerivedRef) -> Value {
    Value::Array(vec![
        Value::Text(d.extractor.clone()),
        Value::Text(d.model_id.clone()),
        Value::Text(d.model_version.clone()),
        ref_to_value(&d.record),
    ])
}

fn derived_from_value(v: &Value) -> Result<DerivedRef, PointerError> {
    let Value::Array(items) = v else {
        return Err(PointerError::BadShape(Part::Derived));
    };
    let [extractor, model_id, model_version, record] = items.as_slice() else {
        return Err(PointerError::BadShape(Part::Derived));
    };
    match (extractor, model_id, model_version) {
        (Value::Text(extractor), Value::Text(model_id), Value::Text(model_version)) => {
            Ok(DerivedRef {
                extractor: extractor.clone(),
                model_id: model_id.clone(),
                model_version: model_version.clone(),
                record: ref_from_value(record)?,
            })
        }
        _ => Err(PointerError::BadShape(Part::Derived)),
    }
}

/// The records of an optional array key: absent is an empty list, an array that is present is never
/// empty (an empty list is written as no key at all, the one canonical form), and never longer than
/// `max` (checked before any record is read). The vector has its exact capacity.
fn list_of<T>(
    items: Option<&[Value]>,
    max: usize,
    limit: Limit,
    read: fn(&Value) -> Result<T, PointerError>,
) -> Result<Vec<T>, PointerError> {
    let Some(items) = items else {
        return Ok(Vec::new());
    };
    if items.is_empty() {
        return Err(PointerError::NotCanonical);
    }
    if items.len() > max {
        return Err(PointerError::OverLimit(limit));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(read(item)?);
    }
    Ok(out)
}

/// Encodes a tree that was built from key material: the encoder writes into a buffer of exactly the
/// right size, which is wiped when it drops; the tree is wiped when this returns.
fn encode_tree(tree: &SecretValue) -> Result<Zeroizing<Vec<u8>>, PointerError> {
    Ok(Zeroizing::new(encode(tree.as_value(), &Limits::OBJECT)?))
}

// ---------------------------------------------------------------------------------------------
// The pointer
// ---------------------------------------------------------------------------------------------

impl PointerV2 {
    /// The one canonical encoding of the pointer: deterministic CBOR (RFC 8949 section 4.2.1) of the
    /// map in the documentation of the `pointer` module, `sig` included when there is one, in a buffer
    /// that is wiped when it drops (it holds the chunk hashes, which are key material). The pointer is
    /// validated first ([`PointerV2::validate`]): nothing is written that [`PointerV2::from_cbor`]
    /// would refuse.
    ///
    /// Caller: INT-S14-1 and INT-S14-2 (the repository stores the bytes), INT-S16 (log records),
    /// `PointerV2::from_cbor` (the check that an accepted pointer is canonical).
    pub fn to_cbor(&self) -> Result<Zeroizing<Vec<u8>>, PointerError> {
        self.validate()?;
        encode_tree(&self.to_value(true)?)
    }

    /// The canonical encoding WITHOUT key 14: the bytes the signature covers (after the space id).
    /// Not validated: a verifier checks the signature before it judges the content (a flipped field
    /// is a bad signature, whatever else it breaks).
    pub(super) fn encode_unsigned(&self) -> Result<Zeroizing<Vec<u8>>, PointerError> {
        encode_tree(&self.to_value(false)?)
    }

    /// The tree of the pointer: every field, the unknown ones, `crit` and, when asked for and present,
    /// the signature. The part that can fail and holds no key material is done first.
    fn to_value(&self, with_sig: bool) -> Result<SecretValue, PointerError> {
        if self.crit.contains(&0) {
            return Err(PointerError::BadCrit);
        }
        let mtime = time_to_value(self.mtime)?;
        let chunks = chunks_to_value(&self.chunks)?;

        let mut builder = IntMapBuilder::new();
        for (key, value) in self.unknown.iter() {
            builder = builder.put(key, value.clone());
        }
        builder = builder
            .put(K_V, Value::Uint(u64::from(VERSION)))
            .put(K_SIZE, Value::Uint(self.size.get()))
            .put(K_MTIME, mtime)
            .put(K_MODE, Value::Uint(u64::from(self.mode)))
            .put(K_BLAKE3, Value::Bytes(self.blake3.to_vec()))
            .put(K_CHUNKS, chunks)
            .put(
                K_AUTHOR,
                Value::Bytes(self.author_device.as_bytes().to_vec()),
            )
            .put(K_CREATED_AT, Value::Uint(self.created_at.as_secs()));
        if let Some(t) = &self.transform {
            builder = builder.put(K_TRANSFORM, transform_to_value(t));
        }
        if !self.previews.is_empty() {
            builder = builder.put(
                K_PREVIEWS,
                Value::Array(self.previews.iter().map(preview_to_value).collect()),
            );
        }
        if !self.streams.is_empty() {
            builder = builder.put(
                K_STREAMS,
                Value::Array(self.streams.iter().map(stream_to_value).collect()),
            );
        }
        if !self.xattrs.is_empty() {
            builder = builder.put(
                K_XATTRS,
                Value::Array(
                    self.xattrs
                        .iter()
                        .map(|(name, value)| xattr_to_value(name, value))
                        .collect(),
                ),
            );
        }
        if !self.labels.is_empty() {
            builder = builder.put(
                K_LABELS,
                Value::Array(
                    self.labels
                        .iter()
                        .map(|label| Value::Text(label.clone()))
                        .collect(),
                ),
            );
        }
        if !self.derived.is_empty() {
            builder = builder.put(
                K_DERIVED,
                Value::Array(self.derived.iter().map(derived_to_value).collect()),
            );
        }
        if with_sig {
            if let Some(sig) = &self.sig {
                builder = builder.put(K_SIG, Value::Bytes(sig.as_bytes().to_vec()));
            }
        }
        // `crit` sorts and de-duplicates its keys; a key 0 was refused above, a duplicate key cannot
        // happen (the fields have distinct keys and the unknown ones are 16 and up).
        let tree = builder.crit(&self.crit).build()?;
        Ok(SecretValue::new(tree))
    }

    /// Reads a pointer from its bytes: strict deterministic CBOR under the caller's limits (use
    /// `Limits::OBJECT` for a pointer at rest), the version first, then every field, then the limits
    /// and the sums ([`PointerV2::validate`]), then the check that the pointer writes back to exactly
    /// these bytes.
    ///
    /// - version 2: `Decoded::Current`; the signature is NOT checked (that is [`PointerV2::verify`]);
    /// - version 3 to 255: `Decoded::Newer(version)`, and nothing else of the bytes is read: a newer
    ///   pointer is never damage;
    /// - version 0 or 1: `UnsupportedVersion`; above 255: `BadVersion` (DRIFT X-DELTA-W1S14D-10);
    /// - a key listed in `crit` that this build does not know: refused; an unknown key that is not in
    ///   `crit`: accepted and kept (`PointerV2::unknown`), so a copy still verifies.
    ///
    /// The chunk hashes are read with `azlin_cbor::decode_secret` and copied into `Full` at once
    /// (rule R17); the whole tree is wiped when this returns.
    ///
    /// Caller: INT-S14-1 and INT-S14-2 (every read of a pointer from the repository), INT-S16 (log
    /// records), the fuzz target `pointer_decode`.
    pub fn from_cbor(bytes: &[u8], l: &Limits) -> Result<Decoded, PointerError> {
        let tree = decode_secret(bytes, l)?;
        let mut m = IntMap::new(tree.as_value())?;

        match u8::try_from(m.req_u64(K_V)?) {
            Ok(VERSION) => {}
            Ok(old) if old < VERSION => return Err(PointerError::UnsupportedVersion(old)),
            Ok(newer) => return Ok(Decoded::Newer(newer)),
            Err(_) => return Err(PointerError::BadVersion),
        }

        let size = FileSize::new(m.req_u64(K_SIZE)?);
        let mtime = time_from_value(m.req_value(K_MTIME)?)?;
        let mode = u32::try_from(m.req_u64(K_MODE)?).map_err(|_| PointerError::BadMode)?;
        let blake3: [u8; 32] = m.req_bytes_n(K_BLAKE3)?;
        let transform = match m.opt_value(K_TRANSFORM)? {
            Some(v) => Some(transform_from_value(v)?),
            None => None,
        };
        let chunks = chunks_from_value(m.req_value(K_CHUNKS)?)?;
        let author_device = DeviceId::from_bytes(m.req_bytes_n(K_AUTHOR)?);
        let previews = list_of(
            m.opt_array(K_PREVIEWS)?,
            MAX_PREVIEWS,
            Limit::Previews,
            preview_from_value,
        )?;
        let streams = list_of(
            m.opt_array(K_STREAMS)?,
            MAX_STREAMS,
            Limit::Streams,
            stream_from_value,
        )?;
        let xattrs = list_of(
            m.opt_array(K_XATTRS)?,
            MAX_XATTRS,
            Limit::Xattrs,
            xattr_from_value,
        )?;
        let labels = list_of(
            m.opt_array(K_LABELS)?,
            MAX_LABELS,
            Limit::Labels,
            label_from_value,
        )?;
        let derived = list_of(
            m.opt_array(K_DERIVED)?,
            MAX_DERIVED,
            Limit::Derived,
            derived_from_value,
        )?;
        let sig = m.opt_bytes_n::<64>(K_SIG)?.map(Signature::from_bytes);
        let created_at = UnixTime::from_secs(m.req_u64(K_CREATED_AT)?);

        // Rule R18: the must-understand list is kept, so a copy writes the same bytes.
        let crit = m.crit_keys()?;
        let mut unknown = Vec::new();
        for (key, value) in m.finish()? {
            let Some(key) = key.as_uint() else {
                return Err(PointerError::BadUnknown);
            };
            unknown.push((key, value.clone()));
        }
        let unknown = UnknownFields::from_entries(unknown)?;

        let pointer = PointerV2 {
            size,
            mtime,
            mode,
            blake3,
            transform,
            chunks,
            author_device,
            sig,
            previews,
            streams,
            xattrs,
            labels,
            derived,
            created_at,
            unknown,
            crit,
        };
        pointer.validate()?;
        written_back(&pointer, bytes)?;
        Ok(Decoded::Current(Box::new(pointer)))
    }
}

/// The safety net behind the signature rule: a pointer that was read must WRITE BACK to exactly the
/// bytes it was read from, so the signature, which covers canonical bytes, is a signature over what was
/// received. Every rule of the reader already makes this true (shortest forms, sorted keys, one form for
/// an empty list, the unknown fields and `crit` kept); this refuses a pointer for which it is not, so a
/// field added to the format and forgotten by the writer can never be accepted and then verified over
/// bytes other than the ones that arrived (DRIFT X-DELTA-W1S14D-10). No input reaches the refusal today:
/// the unit test below holds the comparison itself.
fn written_back(pointer: &PointerV2, bytes: &[u8]) -> Result<(), PointerError> {
    let again = pointer.to_cbor()?;
    if again.as_slice() != bytes {
        return Err(PointerError::NotCanonical);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest pointer: an empty file.
    fn minimal() -> PointerV2 {
        PointerV2 {
            size: FileSize::ZERO,
            mtime: FileTime::UNIX_EPOCH,
            mode: 0o644,
            blake3: [0x10; 32],
            transform: None,
            chunks: ChunkList::Inline(Vec::new()),
            author_device: DeviceId::from_bytes([0xA1; 16]),
            sig: None,
            previews: Vec::new(),
            streams: Vec::new(),
            xattrs: Vec::new(),
            labels: Vec::new(),
            derived: Vec::new(),
            created_at: UnixTime::from_secs(1),
            unknown: UnknownFields::none(),
            crit: Vec::new(),
        }
    }

    #[test]
    fn c6_the_reader_refuses_a_pointer_that_does_not_write_back_to_its_bytes() {
        let p = minimal();
        let bytes = p.to_cbor().expect("a valid pointer writes");
        // The bytes the pointer writes are accepted.
        assert_eq!(written_back(&p, &bytes), Ok(()));
        // Any other bytes are not: one byte changed, one byte less, one byte more, nothing at all.
        let mut changed = bytes.to_vec();
        changed[3] ^= 1;
        assert_eq!(written_back(&p, &changed), Err(PointerError::NotCanonical));
        assert_eq!(
            written_back(&p, &bytes[..bytes.len() - 1]),
            Err(PointerError::NotCanonical)
        );
        let mut longer = bytes.to_vec();
        longer.push(0);
        assert_eq!(written_back(&p, &longer), Err(PointerError::NotCanonical));
        assert_eq!(written_back(&p, &[]), Err(PointerError::NotCanonical));
    }
}
