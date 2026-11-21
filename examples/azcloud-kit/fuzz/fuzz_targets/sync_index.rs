//! The index of a synced folder, as the drive holds it: every device of the folder (and every
//! member of a shared one) writes it, every device reads it before it acts on a file.
#![no_main]

use azcloud_kit::sync::remote::RemoteIndex;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(index) = RemoteIndex::parse(data) {
        for stand_ins in [false, true] {
            let _ = index.unreadable(stand_ins);
        }
        let _ = index.unknown_features();
        let _ = index.to_bytes();
    }
});
