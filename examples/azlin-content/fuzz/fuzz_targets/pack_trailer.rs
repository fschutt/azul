//! The fuzz target of the AZP2 trailer and of the whole pack: `read_trailer` and `rebuild_entries` on
//! bytes that a hostile server wrote, and on real packs that the builder makes of the input and that are
//! then flipped, cut and extended (every change is refused, nothing panics). The entry point is the file
//! the seeded test of the crate runs on 20,000 inputs (`tests/pack_common/fuzz_entry.rs`, C15): input
//! `selector (1) || bytes`. Run it with `cargo +nightly fuzz run pack_trailer` from the `fuzz` folder.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/pack_common/fuzz_entry.rs"]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::pack_trailer(data);
});
