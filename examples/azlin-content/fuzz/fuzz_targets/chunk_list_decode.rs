//! The fuzz target of the chunk-list object of a huge file: `decode_chunk_list` on sealed bytes that a
//! hostile server made up. The entry point is the file the seeded test of the crate runs
//! (`tests/pointer_common/fuzz_entry.rs`, C13): input `selector (1) || count offset (2, big-endian) ||
//! bytes`; the bytes are handed over as a sealed object (the identity sealer makes them the plaintext)
//! under their own name, and a valid object is damaged by the same bytes as `(position, xor)` triples.
//! Run it with `cargo +nightly fuzz run chunk_list_decode` from the `fuzz` folder.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/pointer_common/fuzz_entry.rs"]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::chunk_list_decode(data);
});
