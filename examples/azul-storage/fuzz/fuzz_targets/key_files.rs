//! The key files of `.azlin/keys/`: the recovery wrap (with its Argon2id cost), a member's wrap,
//! the key chain. A bucket writes them; a device reads them before it holds any key.
#![no_main]

use azul_storage::crypto::keys::{KeyChain, MemberWrap, RecoveryWrap};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = RecoveryWrap::parse(data);
    let _ = MemberWrap::parse(data);
    let _ = KeyChain::parse(data);
});
