//! The entry function of the fuzz target of this crate's text parsers
//! (`examples/azlin-keys/fuzz/fuzz_targets/id_text.rs`), which `tests/fuzz_inputs.rs` also runs on a
//! few thousand fixed inputs: a panic is a failed test here and an abort in the shipped profile.
//! The file is self-contained (it names only `azlin_keys` and std) because the fuzz target
//! includes it by `#[path]`.
//!
//! The oracle is strictness: a text that a parser accepts is the one canonical text of the value
//! it parsed to (lowercase hex of the exact length, decimal without sign or leading zero), so no
//! value has a second spelling.

use azlin_keys::{AccountId, DeviceId, Epoch, GroupId, KeyId, MemberId, SpaceId};

fn accepted_is_canonical<T>(text: &str)
where
    T: std::str::FromStr + std::fmt::Display,
{
    if let Ok(value) = text.parse::<T>() {
        assert_eq!(
            value.to_string(),
            text,
            "a parser accepted a text that is not the canonical text of its value"
        );
    }
}

/// Every text parser of the vocabulary (the five 16-byte ids, `KeyId`, `Epoch`) on arbitrary
/// bytes: never a panic, and every accepted text is canonical.
pub fn id_text_entry(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    accepted_is_canonical::<AccountId>(text);
    accepted_is_canonical::<MemberId>(text);
    accepted_is_canonical::<DeviceId>(text);
    accepted_is_canonical::<SpaceId>(text);
    accepted_is_canonical::<GroupId>(text);
    accepted_is_canonical::<KeyId>(text);
    accepted_is_canonical::<Epoch>(text);
}
