//! The fuzz target of the signed pointer: `PointerV2::from_cbor` on bytes that another device, another
//! member or a hostile server wrote, and `PointerV2::verify` on what is accepted. The entry point is the
//! file the seeded test of the crate runs on 20,000 inputs (`tests/pointer_common/fuzz_entry.rs`, C13):
//! the bytes themselves are tried as a pointer, and as `(position, xor)` triples that damage a valid
//! signed pointer; what is accepted must write back to the bytes that were read. Run it with
//! `cargo +nightly fuzz run pointer_decode` from the `fuzz` folder.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/pointer_common/fuzz_entry.rs"]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::pointer_decode(data);
});
