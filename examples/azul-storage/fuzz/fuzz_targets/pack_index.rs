//! A pack index of the drive index (`PackIndex::open` ties its counts to the real length before
//! it reserves). The input is the index's PLAINTEXT: the target seals it first, because the
//! sealer's magic and tag are checked before the parser sees a byte, and raw bytes never get
//! past them (X-DELTA-AZDRIVE-KEYS-42) - a member who holds the drive key seals what it likes.
#![no_main]

use azul_storage::meta::{keys, PackIndex, Sealer, TestSealer};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let sealer = TestSealer::new([5; 32]);
    let Ok(sealed) = sealer.seal(keys::idx("pack").as_bytes(), data) else {
        return;
    };
    let _ = PackIndex::open(&sealer, "pack", &sealed);
});
