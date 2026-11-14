//! What a pointer must satisfy beyond its shape: the ranges of its numbers and times, the limits of
//! every collection and text, and the arithmetic of its chunks (planning/architecture.md section 1.4
//! rule 4 "Everything has a limit, stated up front", section 5.7, rule I2 of section 6.10; build unit
//! S14, lane W1-S14d).
//!
//! [`PointerV2::validate`] runs on every pointer that is written, signed or read, so what this build
//! writes it also reads and what it reads it could have written. [`PointerV2::check_consistency`] is
//! its arithmetic half: the lengths of the chunks add up to the declared length, every reference
//! describes a chunk that can exist, and the choice between an inline list and a chunk-list object is
//! the canonical one (rule P-2). All arithmetic on a length is checked (a hostile pointer with the
//! largest numbers is an error, never a wrap).

use azlin_cbor::{encode, Limits};

use super::error::{Limit, Part, PointerError};
use super::{
    ChunkList, DerivedRef, PointerV2, Preview, StreamRef, TransformRef, INLINE_MAX,
    INLINE_MAX_CHUNKS, LAST_KNOWN_KEY, MAX_DERIVED, MAX_DIMENSION, MAX_FILE_SIZE, MAX_LABEL,
    MAX_LABELS, MAX_PREVIEWS, MAX_PREVIEW_CHUNKS, MAX_STREAMS, MAX_STREAM_CHUNKS, MAX_TEXT,
    MAX_TIME_SECS, MAX_TRANSFORM_PARAMS, MAX_XATTRS, MAX_XATTR_NAME, MAX_XATTR_TOTAL,
    MAX_XATTR_VALUE, MIN_TIME_SECS, MODE_MASK,
};
use crate::limits::{MAX_CHUNK, MAX_FILE_CHUNKS};
use crate::units::{FileSize, FileTime};

impl PointerV2 {
    /// Checks everything a pointer must satisfy: the mode and the times, the size, the transform, `crit`,
    /// the limits of the previews, streams, attributes, labels and derived records, and then
    /// [`PointerV2::check_consistency`]. `Ok` means [`PointerV2::to_cbor`] can write the pointer and
    /// [`PointerV2::from_cbor`] reads the bytes back to an equal one.
    ///
    /// Caller: `PointerV2::to_cbor`, `PointerV2::from_cbor`, `PointerV2::sign`, `PointerV2::verify`,
    /// INT-S14-1 (a pointer is checked before an upload).
    pub fn validate(&self) -> Result<(), PointerError> {
        if self.mode > MODE_MASK {
            return Err(PointerError::BadMode);
        }
        check_mtime(self.mtime)?;
        if self.created_at.as_secs() > MAX_TIME_SECS {
            return Err(PointerError::BadTime);
        }
        if self.size > MAX_FILE_SIZE {
            return Err(PointerError::SizeTooLarge);
        }
        if let Some(transform) = &self.transform {
            check_transform(transform)?;
        }
        check_crit(&self.crit)?;
        check_previews(&self.previews)?;
        check_streams(&self.streams)?;
        check_xattrs(&self.xattrs)?;
        check_labels(&self.labels)?;
        check_derived(&self.derived)?;
        self.check_consistency()
    }

    /// The arithmetic of the pointer (check C5):
    ///
    /// - the chunks are inline (at most `INLINE_MAX_CHUNKS`: else `TooManyInline`) or a chunk-list
    ///   object (more than that: else `ObjectTooSmall`; at most `MAX_FILE_CHUNKS`: else
    ///   `TooManyChunks`);
    /// - the raw lengths add up to the transformed length (`transform.out_len`) or, without a
    ///   transform, to `size`: else `SizeMismatch`; an empty file has an empty inline list;
    /// - every chunk reference describes a chunk that can exist (`ChunkRef::check`: lengths, and
    ///   `offset + stored_len` inside a `u32`), the references of the previews, streams and derived
    ///   records included; a repeated reference inside a file is legal (repeated blocks);
    /// - a chunk-list object's declared total is possible for that many chunks (each has 1 to
    ///   `MAX_CHUNK` bytes).
    ///
    /// Caller: `PointerV2::validate`, INT-S14-1 (the upload pipeline checks the pointer it built).
    pub fn check_consistency(&self) -> Result<(), PointerError> {
        let expected = match &self.transform {
            Some(transform) => transform.out_len,
            None => self.size,
        };
        if expected > MAX_FILE_SIZE {
            return Err(PointerError::SizeTooLarge);
        }
        match &self.chunks {
            ChunkList::Inline(refs) => {
                if refs.len() > INLINE_MAX {
                    return Err(PointerError::TooManyInline);
                }
                let mut sum = FileSize::ZERO;
                for r in refs {
                    r.check().map_err(PointerError::BadChunk)?;
                    sum = sum
                        .checked_add_chunk(r.raw_len)
                        .ok_or(PointerError::Overflow)?;
                }
                if sum != expected {
                    return Err(PointerError::SizeMismatch);
                }
            }
            ChunkList::Object(r) => {
                if r.count <= INLINE_MAX_CHUNKS {
                    return Err(PointerError::ObjectTooSmall);
                }
                if r.count > MAX_FILE_CHUNKS {
                    return Err(PointerError::TooManyChunks);
                }
                if r.total_raw != expected {
                    return Err(PointerError::SizeMismatch);
                }
                check_list_total(r.count, r.total_raw).map_err(|_| PointerError::SizeMismatch)?;
            }
        }
        for r in self.side_refs() {
            r.check().map_err(PointerError::BadChunk)?;
        }
        Ok(())
    }

    /// The largest number of bytes the decoded FILE may have: the declared `size` (rule I2 of section
    /// 6.10: "the decoder's `max_decoded_len` is the file's declared size from its sealed pointer, and
    /// the declared size is checked against the local disk (E3) before any download"). The inverse of
    /// the whole-file transform is bound by it (it may produce `size` bytes and not one more); one chunk
    /// is bound by its own `raw_len` (`open_chunk`), which a valid pointer keeps inside the transformed
    /// length (`transform.out_len`, or `size` without a transform).
    ///
    /// Caller: INT-S14-1 and INT-S14-2 (the download pipeline's bound, the free-space check before a
    /// download).
    pub fn max_decoded_len(&self) -> FileSize {
        self.size
    }
}

/// Whether a declared total length is possible for `count` chunks of 1 to `MAX_CHUNK` bytes each:
/// `count <= total <= count * MAX_CHUNK <= 2^50`. Shared with the chunk-list object.
pub(super) fn check_list_total(count: u32, total: FileSize) -> Result<(), PointerError> {
    let low = u64::from(count);
    let high = low
        .checked_mul(u64::from(MAX_CHUNK.get()))
        .ok_or(PointerError::Overflow)?;
    if total > MAX_FILE_SIZE {
        return Err(PointerError::SizeTooLarge);
    }
    if total.get() < low || total.get() > high {
        return Err(PointerError::ListTotal);
    }
    Ok(())
}

/// `mtime` between 0000-01-01 and 9999-12-31 (UTC).
fn check_mtime(t: FileTime) -> Result<(), PointerError> {
    let secs = t.secs();
    if secs < MIN_TIME_SECS {
        return Err(PointerError::BadTime);
    }
    if u64::try_from(secs).is_ok_and(|s| s > MAX_TIME_SECS) {
        return Err(PointerError::BadTime);
    }
    Ok(())
}

/// The transform: its length within the size cap, its parameters within `MAX_TRANSFORM_PARAMS` bytes.
fn check_transform(t: &TransformRef) -> Result<(), PointerError> {
    if t.out_len > MAX_FILE_SIZE {
        return Err(PointerError::SizeTooLarge);
    }
    match encode(&t.params, &Limits::OBJECT) {
        Ok(bytes) if bytes.len() <= MAX_TRANSFORM_PARAMS => Ok(()),
        _ => Err(PointerError::OverLimit(Limit::TransformParams)),
    }
}

/// `crit`: ascending, each a known key (1 to 15). Key 0 is `crit` itself.
fn check_crit(crit: &[u64]) -> Result<(), PointerError> {
    let mut previous = 0u64;
    for key in crit {
        if *key <= previous || *key > LAST_KNOWN_KEY {
            return Err(PointerError::BadCrit);
        }
        previous = *key;
    }
    Ok(())
}

fn check_previews(previews: &[Preview]) -> Result<(), PointerError> {
    if previews.len() > MAX_PREVIEWS {
        return Err(PointerError::OverLimit(Limit::Previews));
    }
    for p in previews {
        if p.w > MAX_DIMENSION || p.h > MAX_DIMENSION {
            return Err(PointerError::BadDimension);
        }
        if p.chunks.is_empty() {
            return Err(PointerError::EmptyList(Part::Preview));
        }
        if p.chunks.len() > MAX_PREVIEW_CHUNKS {
            return Err(PointerError::OverLimit(Limit::PreviewChunks));
        }
    }
    Ok(())
}

fn check_streams(streams: &[StreamRef]) -> Result<(), PointerError> {
    if streams.len() > MAX_STREAMS {
        return Err(PointerError::OverLimit(Limit::Streams));
    }
    for s in streams {
        if s.w > MAX_DIMENSION || s.h > MAX_DIMENSION {
            return Err(PointerError::BadDimension);
        }
        if s.chunks.is_empty() {
            return Err(PointerError::EmptyList(Part::Stream));
        }
        if s.chunks.len() > MAX_STREAM_CHUNKS {
            return Err(PointerError::OverLimit(Limit::StreamChunks));
        }
    }
    Ok(())
}

/// The extended attributes: unique names of 1 to 255 bytes without NUL, values up to 64 KiB, all names
/// and values together up to 256 KiB.
fn check_xattrs(xattrs: &[(String, Vec<u8>)]) -> Result<(), PointerError> {
    if xattrs.len() > MAX_XATTRS {
        return Err(PointerError::OverLimit(Limit::Xattrs));
    }
    let mut total = 0usize;
    for (i, (name, value)) in xattrs.iter().enumerate() {
        if name.is_empty() || name.contains('\0') {
            return Err(PointerError::BadXattrName);
        }
        if name.len() > MAX_XATTR_NAME {
            return Err(PointerError::OverLimit(Limit::XattrName));
        }
        if value.len() > MAX_XATTR_VALUE {
            return Err(PointerError::OverLimit(Limit::XattrValue));
        }
        total = total
            .checked_add(name.len())
            .and_then(|t| t.checked_add(value.len()))
            .ok_or(PointerError::OverLimit(Limit::XattrTotal))?;
        if total > MAX_XATTR_TOTAL {
            return Err(PointerError::OverLimit(Limit::XattrTotal));
        }
        if xattrs
            .iter()
            .take(i)
            .any(|(earlier, _)| earlier.as_str() == name.as_str())
        {
            return Err(PointerError::DuplicateXattr);
        }
    }
    Ok(())
}

/// The labels: texts of 1 to 128 bytes without NUL.
fn check_labels(labels: &[String]) -> Result<(), PointerError> {
    if labels.len() > MAX_LABELS {
        return Err(PointerError::OverLimit(Limit::Labels));
    }
    for label in labels {
        if label.is_empty() || label.contains('\0') {
            return Err(PointerError::BadText);
        }
        if label.len() > MAX_LABEL {
            return Err(PointerError::OverLimit(Limit::Label));
        }
    }
    Ok(())
}

/// The derived records: an extractor of 1 to 256 bytes, a model id and version of up to 256 bytes (they
/// may be empty), none with a NUL.
fn check_derived(derived: &[DerivedRef]) -> Result<(), PointerError> {
    if derived.len() > MAX_DERIVED {
        return Err(PointerError::OverLimit(Limit::Derived));
    }
    for d in derived {
        if d.extractor.is_empty() {
            return Err(PointerError::BadText);
        }
        for text in [&d.extractor, &d.model_id, &d.model_version] {
            if text.len() > MAX_TEXT {
                return Err(PointerError::OverLimit(Limit::Text));
            }
            if text.contains('\0') {
                return Err(PointerError::BadText);
            }
        }
    }
    Ok(())
}
