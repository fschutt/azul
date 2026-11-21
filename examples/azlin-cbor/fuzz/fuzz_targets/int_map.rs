//! `IntMap` on whatever `decode` accepts, and `IntMapBuilder` round-tripped through `encode`,
//! `decode` and `IntMap` (planning/architecture.md section 5.1 principle 6: `crit`).
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../tests/common/fuzz_entry.rs"]
#[allow(dead_code)]
mod fuzz_entry;

fuzz_target!(|data: &[u8]| {
    fuzz_entry::int_map_entry(data);
});
