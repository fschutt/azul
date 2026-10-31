//! Whole-file transforms: a file is rewritten into another byte stream that compresses better,
//! and back, bit for bit. They run before the file is cut into segments, because what they
//! reach (a whole JPEG image, a whole deflate stream) crosses segment boundaries.
//!
//! | id | transform | versions |
//! |---|---|---|
//! | 0 | none | 0 |
//! | 1 | JPEG images through Lepton ([`jpeg`]) | 1 |
//! | 2 | containers: deflate streams rebuilt by azlin-preflate, JPEG images through Lepton ([`deflate`]) | 1 |
//!
//! Both write the record stream of [`records`]. A forward transform is only ever returned after
//! its output was decoded and compared with the input; anything else (an error, a mismatch,
//! the budget, the memory cap) means no transform, and the file is compressed per segment as
//! it is.

use std::io::{Read, Write};

use zeroize::Zeroizing;

use crate::{
    detect::Kind,
    error::Error,
    options::{Limits, Options},
};

pub(crate) mod deflate;
pub(crate) mod jpeg;
mod lepton;
pub(crate) mod records;
pub(crate) mod scan;
mod work;

/// The id byte of no transform.
pub const TRANSFORM_NONE: u8 = 0;
/// The id byte of the JPEG (Lepton) transform, and the version this build writes.
pub const TRANSFORM_JPEG: u8 = 1;
pub const JPEG_VERSION: u8 = 1;
/// The id byte of the container (deflate and Lepton) transform, and its version.
pub const TRANSFORM_CONTAINERS: u8 = 2;
pub const CONTAINERS_VERSION: u8 = 1;

/// A transform, as a container names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformId {
    None,
    Jpeg,
    Containers,
}

impl TransformId {
    /// The id byte and the version this build writes.
    #[must_use]
    pub fn bytes(self) -> (u8, u8) {
        match self {
            TransformId::None => (TRANSFORM_NONE, 0),
            TransformId::Jpeg => (TRANSFORM_JPEG, JPEG_VERSION),
            TransformId::Containers => (TRANSFORM_CONTAINERS, CONTAINERS_VERSION),
        }
    }

    /// The transform of an id byte and version, if this build reads it.
    pub fn from_bytes(id: u8, version: u8) -> Result<TransformId, Error> {
        match (id, version) {
            (TRANSFORM_NONE, 0) => Ok(TransformId::None),
            (TRANSFORM_JPEG, JPEG_VERSION) => Ok(TransformId::Jpeg),
            (TRANSFORM_CONTAINERS, CONTAINERS_VERSION) => Ok(TransformId::Containers),
            (other, v) => Err(Error::Unsupported(format!("transform {other} version {v}"))),
        }
    }
}

/// What the Lepton part of a transform did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JpegStats {
    /// Images the scan found.
    pub found: u32,
    /// Images stored as Lepton data, and their bytes before and after.
    pub recompressed: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    /// Images left as they were: over the time budget, over the memory cap, or refused by
    /// Lepton (it failed, saved under 5 %, or its output did not decode to the image).
    pub over_budget: u32,
    pub over_memory: u32,
    pub refused: u32,
}

/// What the deflate part of a transform did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeflateStats {
    /// Deflate streams found.
    pub found: u32,
    /// Streams stored as plain text and reconstruction data: their bytes in the file and the
    /// plain text's bytes.
    pub recompressed: u32,
    pub bytes_in: u64,
    pub plain_bytes: u64,
    /// Streams left as they were: over the time budget, over the memory cap, not analysable
    /// (or not rebuilt bit-exact), or not worth it (zstd does not beat the deflate stream).
    pub over_budget: u32,
    pub over_memory: u32,
    pub refused: u32,
    pub not_worth: u32,
}

/// A file's transform, verified.
#[derive(Debug)]
pub struct Transformed {
    pub id: TransformId,
    /// The transformed stream: what gets cut into segments.
    pub bytes: Zeroizing<Vec<u8>>,
    pub jpeg: JpegStats,
    pub deflate: DeflateStats,
}

/// What [`forward`] did.
#[derive(Debug)]
pub enum Forward {
    Applied(Transformed),
    /// No transform: why (none applies, it did not pay, it failed or did not verify, a cap).
    NotApplied(String),
}

/// Whether a file of `kind` and `len` bytes goes through a whole-file transform under
/// `options` (the caller must then hold the whole file in memory), and which.
#[must_use]
pub fn wanted(kind: Kind, len: u64, options: &Options) -> Option<TransformId> {
    if !options.transforms || len > options.limits.max_transform_input {
        return None;
    }
    if kind.holds_deflate_streams() {
        Some(TransformId::Containers)
    } else if kind.may_hold_jpegs() {
        Some(TransformId::Jpeg)
    } else {
        None
    }
}

/// The most bytes the transformed stream of a file of at most `max_decoded_len` bytes may hold
/// under transform `id`: a decoder stops reading segments past it. Without a transform it is the
/// file; Lepton data is smaller than its image (a little more is allowed for the records of a
/// file Lepton barely shrank); the plain text of a deflate stream may be up to 1032 times the
/// stream (deflate's largest ratio).
#[must_use]
pub fn max_transformed_len(id: TransformId, max_decoded_len: u64) -> u64 {
    match id {
        TransformId::None => max_decoded_len,
        TransformId::Jpeg => max_decoded_len.saturating_add(max_decoded_len / 16 + 4096),
        TransformId::Containers => max_decoded_len
            .saturating_mul(records::MAX_DEFLATE_EXPANSION)
            .saturating_add(4096),
    }
}

/// The kind the codec stage treats a transformed stream as. Lepton's output is as dense as the
/// JPEG it came from; the containers transform turns deflated bytes into plain text, which the
/// codecs then compress even when the file's own kind is dense (PNG, gzip).
#[must_use]
pub fn codec_kind(kind: Kind, id: TransformId) -> Kind {
    match id {
        TransformId::Containers if kind.is_dense() => Kind::Binary,
        _ => kind,
    }
}

/// The transform of `input` (a file of `kind`), already decoded and compared with `input`.
#[must_use]
pub fn forward(input: &[u8], kind: Kind, options: &Options) -> Forward {
    let applied = match wanted(kind, input.len() as u64, options) {
        None | Some(TransformId::None) => {
            return Forward::NotApplied(String::from("no transform for this kind or size"))
        }
        Some(TransformId::Jpeg) => jpeg::forward(input, kind, options).map(|(bytes, jpeg)| Transformed {
            id: TransformId::Jpeg,
            bytes,
            jpeg,
            deflate: DeflateStats::default(),
        }),
        Some(TransformId::Containers) => {
            deflate::forward(input, kind, options).map(|(bytes, jpeg, deflate)| Transformed {
                id: TransformId::Containers,
                bytes,
                jpeg,
                deflate,
            })
        }
    };
    match applied {
        Ok(transformed) => Forward::Applied(transformed),
        Err(why) => Forward::NotApplied(why),
    }
}

/// Decodes the transformed stream of `reader` (transform `id`) into `writer`; returns the bytes
/// written. `threads` is what Lepton may use.
pub fn inverse<R: Read, W: Write>(
    id: TransformId,
    reader: &mut R,
    writer: &mut W,
    limits: &Limits,
    threads: usize,
) -> Result<u64, Error> {
    let allowed = match id {
        TransformId::None => {
            let copied = std::io::copy(&mut reader.take(limits.max_decoded_len.saturating_add(1)), writer)?;
            if copied > limits.max_decoded_len {
                return Err(Error::Limit(String::from(
                    "the decoded file would be larger than the cap",
                )));
            }
            return Ok(copied);
        }
        TransformId::Jpeg => records::Allowed::Jpeg,
        TransformId::Containers => records::Allowed::Containers,
    };
    records::decode_records(reader, &mut records::WriterSink { writer }, allowed, limits, threads)
}
