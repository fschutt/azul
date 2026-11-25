//! Check 12 of the brief (rule 9 of the launch rules): the space-chain entries against the golden fixture
//! of `azlin-contract` (S5's `space_chain_entries.json`). The fixture does not exist yet (the crate has no
//! codec: azlin-cbor and the contract types are the glue's, INT-WIRE), so the test stays `#[ignore]`d until
//! INT-WIRE lands it. The one byte layout this crate can pin by itself, the entry hash over the preimage,
//! is pinned in `tests/chain.rs` and `tests/capability.rs`; the signed layout of an entry is the glue's.
//!
//! Implements planning/architecture.md section 2.5 (signed structures, each with its own domain string:
//! `azlin-space-chain-v1`) and section 5.9 (the fields of a chain entry).

use std::fs;
use std::path::PathBuf;

#[test]
#[ignore = "X-DELTA-W1S16B-16: S5 fixture not landed"]
fn space_chain_entries_match_azlin_contract_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../azlin-contract/fixtures/c4");
    let fixture = fs::read_to_string(dir.join("space_chain_entries.json")).expect(
        "S5's fixture space_chain_entries.json (INT-WIRE copies it into azlin-contract/fixtures/c4)",
    );
    // The fields of a space-chain entry (section 5.9 and the ops of `SpaceOp`).
    for field in [
        "seq",
        "prev",
        "signer",
        "space_create",
        "space_grant",
        "space_revoke",
        "space_rekey",
        "space_history",
        "sig",
    ] {
        assert!(
            fixture.contains(field),
            "space_chain_entries.json lacks `{field}`"
        );
    }
}
