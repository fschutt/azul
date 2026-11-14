//! `decode` under every set of limits, with the signature rule as the oracle: an accepted input
//! re-encodes to itself byte for byte (planning/architecture.md section 5.1 principle 6).
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/common/fuzz_entry.rs"]
#[allow(dead_code)]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::decode_entry(data);
});
