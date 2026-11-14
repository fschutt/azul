//! The text parsers of the vocabulary (the five 16-byte ids, `KeyId`, `Epoch`) on arbitrary
//! bytes: never a panic, and a text that is accepted is the canonical text of its value
//! (planning/architecture.md section 1.4 rule 3).
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/common/fuzz_entry.rs"]
#[allow(dead_code)]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::id_text_entry(data);
});
