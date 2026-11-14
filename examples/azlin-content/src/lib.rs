//! Azlin content: the content-defined chunker, the sealed chunk format AZL2, the pack format AZP2, the
//! signed pointer v2, the chunk index with local deduplication, and the client's prune
//! (planning/architecture.md section 5.7 "Content: chunks, packs, delta sync", section 5.5 "Keys";
//! build unit S14, lanes W1-S14b to W1-S14e2).
//!
//! This crate is the first lane's vocabulary and chunk format plus one pre-declared stub per later
//! module, so that the later lanes build against types and not against each other's logic:
//!
//! | module | what | lane |
//! |---|---|---|
//! | [`units`] | `ChunkLen`, `PackLen`, `PackOffset`, `FileSize`, `FileTime`, `Percent` | S14b |
//! | [`ids`] | `ChunkId`, `Full`, `PackId`, `PackName`, `PackClass`, `SealedChunk`, `PackEntry`, `PackSummary`, `ChunkRef` | S14b |
//! | [`error`] | `ChunkError`, `CodecError` | S14b |
//! | [`limits`] | the chunk and pack sizes, the 5 % rule, the blob layout, the table of derive contexts | S14b |
//! | [`codec`] | `Codec`, the `Compressor` seam, `StoredOnly`, `CompressPolicy` | S14b |
//! | [`chunk`] | `KChunk`, `ChunkMeta`, `OpenAs`, `identity` | S14b |
//! | `pack`, `pointer`, `index`, `dedup`, `prune` | stubs: `//! owned by lane W1-S14x` | S14c, S14d, S14e1, S14e1, S14e2 |
//!
//! The crate is a pure library: no clock, file, network or thread, no `serde` (the formats are
//! deterministic CBOR through `azlin-cbor`), no unsafe code, and no randomness of its own: where a
//! nonce or a key has to be drawn, the caller hands in the entropy seam of `azlin-keys` (the AZL2 chunk
//! needs none: its nonce is synthetic). It builds for `wasm32-unknown-unknown`.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

pub mod chunk;
pub mod codec;
pub mod error;
pub mod ids;
pub mod limits;
pub mod units;

// The modules of the later lanes: each is a stub file that its lane replaces (R2: nobody edits this
// file after the skeleton, apart from the first lane adding its own modules).
pub mod dedup;
pub mod index;
pub mod pack;
pub mod pointer;
pub mod prune;

pub use chunk::{identity, ChunkMeta, KChunk, OpenAs};
pub use codec::{Codec, CompressPolicy, Compressed, Compressor, StoredOnly, CODEC_JPEG_XL};
pub use error::{ChunkError, CodecError};
pub use ids::{
    ChunkId, ChunkRef, Full, PackClass, PackEntry, PackId, PackName, PackSummary, SealedChunk,
};
pub use limits::{ContentContext, CONTEXTS};
pub use units::{ChunkLen, FileSize, FileTime, PackLen, PackOffset, Percent};
