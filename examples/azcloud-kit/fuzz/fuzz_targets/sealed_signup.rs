//! A paid drive's sign-up: the text a token server seals to the checkout's claim key. The server
//! knows the key's public half and seals what it likes, so the bundle it holds is read as hostile
//! JSON (`DriveBundle::parse`, fed the bytes directly: it is the parser behind the seal). The
//! sealed text is opened with a key of the target's own: that reaches what comes before the tag -
//! the base64, the lengths, the check for an ephemeral key of small order - and stops at the tag,
//! behind which `ClaimKey::open` holds one `from_utf8` and nothing else to find
//! (X-DELTA-AZDRIVE-KEYS-42: the claim reads no parser behind its authentication).
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
