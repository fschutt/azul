//! The committed seed files (planning/architecture.md section 1.4 rule 2: a failing seed is a regression
//! test): every `.seed` file under `tests/seeds/` is replayed against the current code and must do what
//! it says; the canary seeds are the directed restore scenarios; the status of a known gap and its seed
//! file agree.

use std::path::Path;

use azlin_sim::{
    replay_store, DirSeeds, Expect, InvariantId, SeedFile, SeedStore, Status, INVARIANTS,
};

fn store() -> DirSeeds {
    DirSeeds::open(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("seeds"),
    )
}

#[test]
fn every_seed_file_replays_as_it_says() {
    let report = replay_store(&store());
    println!("{} seed files, {} ok", report.checked, report.ok);
    assert!(report.failures.is_empty(), "{:#?}", report.failures);
    assert!(report.looks_fixed.is_empty(), "{:#?}", report.looks_fixed);
}

#[test]
fn the_status_of_a_known_gap_and_its_seed_file_agree() {
    let store = store();
    let Ok(names) = store.names() else {
        panic!("the seed store must be readable");
    };
    let mut open: Vec<InvariantId> = Vec::new();
    for name in names {
        let Ok(text) = store.read(&name) else {
            panic!("{name} must be readable");
        };
        let Ok(file) = SeedFile::parse(&text) else {
            panic!("{name} must parse");
        };
        if file.expect == Expect::Fails {
            open.push(file.invariant);
        }
    }
    for info in INVARIANTS {
        let gap = matches!(info.status, Status::KnownGap(_));
        let has_file = open.contains(&info.id);
        assert_eq!(
            gap, has_file,
            "{}: a known gap needs an `expect: fails` seed and an `expect: fails` seed needs the gap status",
            info.name
        );
    }
}
