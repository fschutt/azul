//! The text forms of the recovery module on arbitrary bytes: the recovery code, the contact key, the
//! request key, the sealed share, the reply and the printed share, `combine` on shares made of the
//! bytes, and the three unwraps and the share open on the bytes as an envelope (planning/
//! architecture.md section 1.4 rule 3: every parser has a fuzz target). Never a panic, and every
//! text that parses prints back to a text that parses to the same value.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/recovery_common/fuzz_entry.rs"]
#[allow(dead_code)]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::contact_text_entry(data);
});
