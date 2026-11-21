//! The fuzz target of the AZP2 header: `PackHeader::parse`, the node's `check_header` and
//! `check_new_pack`, and the randomness test, on bytes that a client, a peer or a hostile server wrote.
//! The entry point is the file the seeded test of the crate runs on 20,000 inputs
//! (`tests/pack_common/fuzz_entry.rs`, C15): input `declared length (4, little-endian) || bytes`; a header
//! that is accepted must be the 24 bytes read and have a trailer range that ends at the declared length.
//! Run it with `cargo +nightly fuzz run pack_header` from the `fuzz` folder.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/pack_common/fuzz_entry.rs"]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::pack_header(data);
});
