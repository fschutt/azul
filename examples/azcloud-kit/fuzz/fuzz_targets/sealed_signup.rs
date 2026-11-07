//! A paid drive's sign-up: the text a token server seals to the checkout's claim key. The server
//! knows the key's public half and seals what it likes, so the bundle it holds is read as hostile
//! JSON; the sealed text is opened with a key of the target's own (base64, length, the check).
#![no_main]

use azcloud_kit::{ClaimKey, DriveBundle};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = DriveBundle::parse(text);
    if let Ok(key) = ClaimKey::generate() {
        let _ = key.open("ck_1", text);
    }
});
