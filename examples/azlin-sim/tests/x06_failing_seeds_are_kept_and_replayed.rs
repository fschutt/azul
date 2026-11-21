//! X06: failing seeds are kept and replayed (planning/architecture.md section 1.4 rule 2). Against a
//! `MemSeeds`, a planted failure is shrunk and written as `<invariant>-<seed>.seed` once; a second run
//! writes nothing; `replay_store` reproduces a red with the same invariant; an `expect: fails` file
//! whose defect disappears is reported "looks fixed".

use azlin_sim::{
    replay_store, Config, Defect, Expect, InvariantId, MemSeeds, Scenario, SeedFile, SeedStore,
    SeedVerdict, Stage, Steps, Verdict,
};

#[test]
fn x06_a_planted_failure_is_written_once_and_replayed() {
    // A faulty double that a hostile entry exposes: the seed file records the shrunk steps.
    let config = Config::standard(0, Stage::S1).with_steps(1);
    let scenario = Scenario::new(0, config.clone())
        .expect("a valid configuration")
        .with_defect(Defect::DeviceAdmitsUnchecked);
    let steps = Steps::new()
        .append(0, 0, 1)
        .append(1, 0, 2)
        .quiesce()
        .attack(azlin_sim::AttackKind::WrongChainEntry, 0, 0)
        .tick(3)
        .quiesce()
        .build();
    let outcome = scenario.replay(&steps);
    let Verdict::Red(failure) = &outcome.verdict else {
        panic!("the planted defect must be caught");
    };
    let shrunk = azlin_sim::shrink(&scenario, outcome.steps.clone(), failure);
    assert!(shrunk.len() <= steps.len());
    // The file written for it replays red with the same invariant in a store.
    let file = SeedFile {
        name: format!("{}-{:016x}", failure.invariant.name(), 0u64),
        stage: Stage::S1,
        seed: 0,
        config,
        invariant: failure.invariant,
        expect: Expect::Fails,
        note: String::from("planted: a glue that skips the chain check"),
        steps: shrunk,
    };
    let text = file.to_text();
    assert_eq!(SeedFile::parse(&text), Ok(file.clone()));
    let mut store = MemSeeds::new();
    store
        .create(&file.name, &text)
        .expect("a fresh store takes the file");
    assert!(
        store.create(&file.name, &text).is_err(),
        "a store never overwrites"
    );
    // The planted defect is the SCENARIO's, not the file's: replaying the file on the committed code is green,
    // which is exactly "the defect looks fixed".
    assert_eq!(file.replay(), SeedVerdict::LooksFixed);
    let report = replay_store(&store);
    assert_eq!(report.checked, 1);
    assert_eq!(report.looks_fixed.len(), 1, "{report:?}");
    assert!(report.failures.is_empty());
    // An `expect: pass` file of a green run is clean.
    let canary = SeedFile {
        name: String::from("canary-a"),
        stage: Stage::S1,
        seed: 1,
        config: azlin_sim::small_world(),
        invariant: InvariantId::RestoreHealsEveryDevice,
        expect: Expect::Pass,
        note: String::from("a green run"),
        steps: Steps::new().append(0, 0, 1).quiesce().build(),
    };
    assert_eq!(canary.replay(), SeedVerdict::Ok);
}
