//! The errors of the vocabulary and of the chunk format: one small `Copy` enum per kind of failure,
//! no strings as control flow and no data that could carry a secret, so a caller maps each to its
//! own status and a `Display` can be logged as it is (build unit S14, lane W1-S14b).
//!
//! UNKNOWN IS A STATE: a compressor that failed, a blob that does not authenticate and a length
//! that does not match are errors that stop the operation, never a default (planning/architecture.md
//! section 1.4 rule 11). The order in which `open_chunk` reports them is part of its contract: a
//! blob is authenticated first, so nothing but [`ChunkError::Auth`], [`ChunkError::Truncated`] and
//! [`ChunkError::TooLong`] can come out of bytes the server made up.

use core::fmt;

/// Why a codec cannot be used: a codec byte this build does not read, or a stream the codec
/// refuses. The seam [`crate::Compressor`] answers with it.
///
/// Caller: INT-S14-1 (azlin-compress's adapter implements the seam), `Codec::from_byte`,
/// `open_chunk`, `seal_chunk`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodecError {
    /// The codec byte is reserved (3, JPEG XL) or no codec uses it: refused by name, with the
    /// byte.
    Unsupported(u8),
    /// The stored bytes are no valid stream of their codec.
    Damaged,
    /// The stream would decode to more than the declared length (rule I2: it is stopped at the
    /// pointer's raw length).
    TooLong,
    /// The compressor could not do its work (out of memory, a missing dictionary). A failed
    /// compression is an error that stops the seal; it is never "store it as it is".
    Failed,
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::Unsupported(3) => f.write_str("codec 3 (JPEG XL) is reserved and not read"),
            CodecError::Unsupported(byte) => write!(f, "codec {byte} is not read by this build"),
            CodecError::Damaged => {
                f.write_str("the stored bytes are no valid stream of their codec")
            }
            CodecError::TooLong => {
                f.write_str("the stream decodes to more than the declared length")
            }
            CodecError::Failed => f.write_str("the compressor failed"),
        }
    }
}

impl std::error::Error for CodecError {}

/// Why a chunk cannot be sealed or opened, or a chunk reference is refused.
///
/// Caller: INT-S14-1 (the upload and download pipelines map each to a status and a retry), S14c
/// (`SealedChunk::from_stored`), S14d (`ChunkRef::check`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChunkError {
    /// A chunk of zero bytes: an empty file has an empty chunk list (brief P-5).
    EmptyChunk,
    /// A plaintext, a declared raw length or a blob longer than the format allows
    /// ([`crate::limits::MAX_CHUNK`], [`crate::limits::MAX_BLOB`]).
    TooLong,
    /// A blob shorter than its framing (the codec byte, the nonce and the tag: 41 bytes), or a
    /// stored length that cannot hold one.
    Truncated,
    /// The authenticated encryption refused the blob: a wrong key, another space, another chunk id,
    /// another codec byte, or any flipped bit. One word for all of them, so the refusal says nothing
    /// of which part failed.
    Auth,
    /// The blob authenticated and its codec byte is one this build does not read (3 is reserved).
    /// Never reported before authentication.
    BadCodec(u8),
    /// The decoded plaintext is not as long as the declared raw length (or the stored bytes are
    /// longer than it).
    LengthMismatch,
    /// The blob authenticated, but its plaintext does not hash to the chunk's `full` under
    /// `K_chunk`: a writer that sealed other bytes under this chunk's key (`OpenAs::Member` only).
    ContentMismatch,
    /// The blob authenticated, but the chunk id given is not the id of the `full` given.
    IdMismatch,
    /// The compressor failed or refused the stream.
    Codec(CodecError),
}

impl fmt::Display for ChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChunkError::EmptyChunk => f.write_str("a chunk cannot be empty"),
            ChunkError::TooLong => f.write_str("the chunk is longer than the format allows"),
            ChunkError::Truncated => f.write_str("the chunk blob is shorter than its framing"),
            ChunkError::Auth => f.write_str("the chunk does not authenticate"),
            ChunkError::BadCodec(byte) => {
                write!(f, "the chunk's codec {byte} is not read by this build")
            }
            ChunkError::LengthMismatch => f.write_str("the chunk is not as long as declared"),
            ChunkError::ContentMismatch => {
                f.write_str("the chunk's content is not the content its hash names")
            }
            ChunkError::IdMismatch => f.write_str("the chunk id is not the id of the chunk's hash"),
            ChunkError::Codec(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for ChunkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ChunkError::Codec(e) => Some(e),
            _ => None,
        }
    }
}

impl From<CodecError> for ChunkError {
    fn from(e: CodecError) -> ChunkError {
        ChunkError::Codec(e)
    }
}
