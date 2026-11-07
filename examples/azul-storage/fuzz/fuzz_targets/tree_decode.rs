//! The git-shaped objects of the drive index: a tree and a commit, as a pack hands them to the
//! reader after the pack's own checks (`Tree::decode`, `Commit::decode`).
#![no_main]

use azul_storage::meta::{Commit, Tree};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = Tree::decode(data);
    let _ = Commit::decode(data);
});
