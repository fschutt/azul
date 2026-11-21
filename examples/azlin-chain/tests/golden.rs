//! The golden check of the brief (acceptance check 11, launch rule 9): the hash of a team-log entry
//! equals the one of the fixture of azlin-contract.
//!
//! It is `#[ignore]`d until S5 lands the fixture: `azlin-contract/fixtures/c4/team_log_entries.json`
//! does not exist yet (the CBOR schemas and golden vectors of the families moved to the units, D-W1S5
//! item 1), and the byte-exact canonical encoding of a team-log entry and the preimage of its
//! signature are azlin-cbor's and azlin-contract's: this crate takes the entry's hash as the entry
//! says it (`ChainEntry::hash`). The glue unit CHAIN-GLUE implements `ChainEntry` for the real
//! entry and runs this test with the real encoder; here it only states what that run must show and
//! fails with the reason while the fixture is missing.
//!
//! Implements planning/architecture.md section 5.9 (the team log's entries are hash-chained) and
//! section 2.5 (`azlin-team-log-v1`, deterministic CBOR with golden fixtures).

use std::path::PathBuf;

/// Where the fixture will be: next to the contract's other golden tables.
fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../azlin-contract/fixtures/c4/team_log_entries.json")
}

#[test]
#[ignore = "S5 fixture not landed: azlin-contract/fixtures/c4/team_log_entries.json (CHAIN-GLUE runs this golden)"]
fn team_log_entry_hash_matches_azlin_contract_fixture() {
    let path = fixture_path();
    let fixture = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "the golden fixture {} is not there ({error}): S5 has not landed team_log_entries.json",
            path.display()
        )
    });
    // The fixture lists entries as the glue decodes them: for each, the claimed `hash`, the `prev`,
    // the `seq` and the `at`, and the canonical bytes whose BLAKE3 under `azlin-team-log-v1` is that
    // hash. The glue recomputes every hash with the real encoder and feeds the entries to
    // `azlin_chain::verify_from_genesis`; nothing of that encoding lives in this crate.
    assert!(
        fixture.contains("\"hash\"") && fixture.contains("\"prev\""),
        "the fixture has no entries of the chain's shape"
    );
    panic!(
        "the golden check is run by CHAIN-GLUE against the real encoder: {} entries' bytes cannot be hashed here",
        fixture.matches("\"hash\"").count()
    );
}
