//! The fuzz target of the AZL2 chunk: `open_chunk` and `SealedChunk::from_stored` on bytes that a
//! node, a peer or a hostile server wrote. The entry point is the file the seeded test of the crate
//! runs on 20,000 inputs (`tests/azl2_common/fuzz_entry.rs`, C18): input `selector (1) || declared raw
//! length (4, little-endian) || bytes`; the bytes are tried as a blob and, sealed by `seal_stored`, as
//! the stored bytes of an authentic one. Run it with `cargo +nightly fuzz run chunk_open` from the
//! `fuzz` folder.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/azl2_common/fuzz_entry.rs"]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::chunk_open(data);
});
