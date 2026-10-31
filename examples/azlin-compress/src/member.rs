//! Small files packed together (AzDrive's packs: files under 512 KiB share 4 MiB objects). Each
//! member is one record, its codec byte and its data, compressed on its own before it goes into
//! the pack. The pack itself is then sealed without compression.
//!
//! A member is never cut into segments: it is one segment of the codec table (0 stored, 1 zstd,
//! 2 brotli, 4 zstd with a dictionary), where the shipped dictionaries help most. A small JPEG
//! (a thumbnail, a sticker) goes through its transform first, and is then stored as a whole AZC1
//! container under codec byte 5. That byte is valid in a member record only, never in a segment.

use zeroize::Zeroizing;

use crate::{
    container::{self, decode},
    detect::{detect, SNIFF_LEN},
    error::{damaged, Error},
    options::Options,
    segment::{decode_segment, Segment, SegmentEncoder},
    transform::{self, TransformId},
};

/// The codec byte of a member stored as a whole AZC1 container (a transformed file).
pub const MEMBER_CONTAINER: u8 = 5;

/// One packed file's record.
#[derive(Debug)]
pub struct Member {
    pub codec: u8,
    /// The bytes after the codec byte.
    pub data: Zeroizing<Vec<u8>>,
}

/// The record of the small file `file`: its transform when it has one that pays, else its
/// best single-segment codec, else stored.
pub fn encode_member(file: &[u8], options: &Options) -> Result<Member, Error> {
    let kind = detect(&file[..file.len().min(SNIFF_LEN)], options.extension.as_deref());
    let mut encoder = SegmentEncoder::new(kind, Some(file.len() as u64), options);
    let plain = match encoder.encode(file)? {
        Segment::Stored => Member {
            codec: 0,
            data: Zeroizing::new(file.to_vec()),
        },
        Segment::Compressed { codec, data } => Member {
            codec: codec.byte(),
            data,
        },
    };
    if transform::wanted(kind, file.len() as u64, options).is_none() {
        return Ok(plain);
    }
    let encoded = container::encode(file, options)?;
    if encoded.report.transform != TransformId::None && encoded.bytes.len() < plain.data.len() {
        return Ok(Member {
            codec: MEMBER_CONTAINER,
            data: encoded.bytes,
        });
    }
    Ok(plain)
}

/// The file of a member record, which must be exactly `expected` bytes.
pub fn decode_member(
    codec: u8,
    data: &[u8],
    expected: usize,
    options: &Options,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    if codec != MEMBER_CONTAINER {
        return decode_segment(codec, data, expected);
    }
    // The entry knows the size: nothing larger is decoded.
    let mut capped = options.clone();
    capped.limits.max_decoded_len = expected as u64;
    let file = decode(data, &capped)?;
    if file.len() != expected {
        return Err(damaged("a packed file decodes to another length than its entry"));
    }
    Ok(file)
}
