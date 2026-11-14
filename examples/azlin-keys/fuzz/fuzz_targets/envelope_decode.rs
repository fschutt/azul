//! Envelope v2 on arbitrary bytes: the strict reader (`Envelope::from_cbor`), the node-side shape
//! check (`check_shape`), the signature check (`verify_signature`) and `open` (planning/
//! architecture.md section 1.4 rule 3: every parser has a fuzz target). Never a panic, and every
//! input the reader accepts writes back to exactly the same bytes (the signature rule).
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/envelope_common/fuzz_entry.rs"]
#[allow(dead_code)]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::envelope_decode_entry(data);
});
