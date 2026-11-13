//! A pointer blob of the drive index: a member writes it, every device reads it.
#![no_main]

use azul_storage::meta::pointer;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pointer::decode(data);
});
