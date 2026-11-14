//! The entry points of the fuzz target `chunk_open` (`fuzz/fuzz_targets/chunk_open.rs` includes this
//! file by path) and of the seeded run of C18: the same code, so what the fuzzer is told to find, the
//! ordinary tests look for on a few thousand fixed inputs.
//!
//! It uses only the public API of the crate, and holds one rule against every input: nothing here may
//! panic, and a blob that `seal_stored` made never fails authentication.
#![allow(dead_code)]

use azlin_content::{
    identity, open_chunk, seal_stored, ChunkError, ChunkLen, Codec, CodecError, Compressed,
    Compressor, KChunk, OpenAs, SealedChunk,
};
use azlin_keys::{Secret, SpaceId};
use zeroize::Zeroizing;

/// A compressor for the entry: reads any codec as "the stored bytes", and calls the stream damaged
/// when its first byte is odd (so the entry reaches the compressor's failure path too).
struct Fuzzy;

impl Compressor for Fuzzy {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(None)
    }

    fn decompress(
        &self,
        _codec: Codec,
        stored: &[u8],
        max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        if stored.len() > max_len.as_usize() {
            return Err(CodecError::TooLong);
        }
        match stored.first() {
            Some(first) if !first.is_multiple_of(2) => Err(CodecError::Damaged),
            _ => Ok(Zeroizing::new(stored.to_vec())),
        }
    }
}

/// One fuzz input: `selector (1) || declared raw length (4, little-endian) || bytes`. The bytes are
/// handed to `open_chunk` as a blob (they never authenticate) and, sealed by `seal_stored` as stored
/// bytes, as an authentic blob (so the checks after the tag are reached).
pub fn chunk_open(data: &[u8]) {
    let space = SpaceId::from_bytes([1; 16]);
    let k = KChunk::from_secret(Secret::from_bytes([2; 32]));
    let (full, id) = identity(&k, b"the chunk of the fuzz target");

    let Some((selector, rest)) = data.split_first() else {
        return;
    };
    let Some((len_bytes, bytes)) = rest.split_first_chunk::<4>() else {
        return;
    };
    let declared = u32::from_le_bytes(*len_bytes);
    let Some(raw_len) = ChunkLen::new((declared % 5_000_000).saturating_add(1)) else {
        return;
    };
    let who = if selector.is_multiple_of(2) {
        OpenAs::Member(&k)
    } else {
        OpenAs::Share
    };

    // 1. The bytes as a blob: never authentic, never a panic.
    let _ = open_chunk(&space, &full, &id, bytes, raw_len, who, &Fuzzy);
    let _ = SealedChunk::from_stored(bytes);

    // 2. The bytes as stored bytes of an authentic blob, under each codec the selector names.
    let codec = match (selector >> 1) & 3 {
        0 => Codec::Stored,
        1 => Codec::Zstd,
        2 => Codec::Brotli,
        _ => Codec::ZstdDict,
    };
    if let Ok(blob) = seal_stored(&space, &full, &id, codec, bytes) {
        let result = open_chunk(&space, &full, &id, blob.as_bytes(), raw_len, who, &Fuzzy);
        assert!(
            !matches!(
                result,
                Err(ChunkError::Auth) | Err(ChunkError::Truncated) | Err(ChunkError::BadCodec(_))
            ),
            "a blob that seal_stored made failed authentication or its shape: {result:?}"
        );
    }
}
