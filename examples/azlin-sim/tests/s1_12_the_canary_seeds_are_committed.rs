//! S1-12: the canary seeds are committed (planning/architecture.md section 1.4 rule 2: a seed is a
//! regression test; section 2.17 and section 6.10 V3: the restore story). `tests/seeds/` holds at least
//! eight `expect: pass` files, they cover at least eight of the ten directed restore scenarios of S1-06,
//! and each parses, carries the name of an invariant of the registry, and is in the canonical text form
//! (what `SeedFile::to_text` writes). That each replays green is `tests/seeds.rs`.

use std::path::Path;

use azlin_sim::{canary_files, DirSeeds, Expect, InvariantId, SeedFile, SeedStore};

fn store() -> DirSeeds {
    DirSeeds::open(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("seeds"),
    )
}

/// The ten restore scenarios of S1-06 and the canary file that covers each. The ninth (a silent
/// rollback, `announce=0`) is the planted violation of `inv_leader_no_silent_rollback_window`, not a
/// green seed: its planted test is in tests/x12.
const SCENARIOS: [(&str, &str); 10] = [
    (
        "a record signed by the map key re-anchors every listed chain",
        "canary-restore-story",
    ),
    (
        "a record signed by the group alone is refused",
        "canary-restore-then-hostile",
    ),
    (
        "a replayed record is refused",
        "canary-restore-then-hostile",
    ),
    (
        "a record older than the device's newest head is refused",
        "canary-restore-then-hostile",
    ),
    (
        "a validly signed head of the lost timeline is refused",
        "canary-restore-then-hostile",
    ),
    (
        "a late record keeps progress",
        "canary-restore-late-record-keeps-progress",
    ),
    ("resend ranges and re-proposal", "canary-restore-story"),
    (
        "a device offline during the whole restore heals",
        "canary-offline-device-heals",
    ),
    (
        "a silent rollback is caught by the pins",
        "planted: tests/x12",
    ),
    (
        "a device born after the restore starts at the epoch",
        "canary-new-device-after-restore",
    ),
];

fn canaries() -> Vec<SeedFile> {
    let store = store();
    let Ok(names) = store.names() else {
        panic!("the seed store must be readable");
    };
    let mut out = Vec::new();
    for name in names {
        let Ok(text) = store.read(&name) else {
            panic!("{name} must be readable");
        };
        let file = match SeedFile::parse(&text) {
            Ok(file) => file,
            Err(error) => panic!("{name} must parse: {error}"),
        };
        if file.name.starts_with("canary-") {
            assert_eq!(file.to_text(), text, "{name} is not in the canonical form");
            out.push(file);
        }
    }
    out
}

#[test]
fn s1_12_at_least_eight_canary_seeds_are_committed_and_expect_a_pass() {
    let files = canaries();
    assert!(
        files.len() >= 8,
        "only {} canary seed files are committed",
        files.len()
    );
    for file in &files {
        assert_eq!(
            file.expect,
            Expect::Pass,
            "{} must expect a pass",
            file.name
        );
        assert!(!file.steps.is_empty(), "{} has no steps", file.name);
        assert_eq!(
            InvariantId::from_name(file.invariant.name()),
            Some(file.invariant),
            "{} names an invariant the registry does not know",
            file.name
        );
    }
}

#[test]
fn s1_12_the_canaries_cover_at_least_eight_of_the_ten_restore_scenarios() {
    let files = canaries();
    let covered = SCENARIOS
        .iter()
        .filter(|(_, canary)| files.iter().any(|file| file.name == *canary))
        .count();
    assert!(
        covered >= 8,
        "the canaries cover only {covered} of the ten restore scenarios"
    );
}

#[test]
fn s1_12_the_committed_canaries_are_the_directed_scenarios_as_they_are_now() {
    let store = store();
    for wanted in canary_files() {
        let Ok(text) = store.read(&wanted.name) else {
            panic!(
                "{} is not committed: write it with `simrun --write-canaries tests/seeds`",
                wanted.name
            );
        };
        assert_eq!(
            text,
            wanted.to_text(),
            "{} is not the directed scenario as it is now: delete it and write it again with `simrun --write-canaries tests/seeds`",
            wanted.name
        );
    }
}
