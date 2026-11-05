//! A sealed pack index of the drive index (`PackIndex::open` ties its counts to the real length
//! before it reserves).
#![no_main]

use azul_storage::meta::{PackIndex, TestSealer};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let sealer = TestSealer::new([5; 32]);
    let _ = PackIndex::open(&sealer, "pack", data);
});
