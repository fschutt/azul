//! Check 6 of the brief: determinism and shrinking. A failing seed replays exactly; a planted failure is
//! reduced to a minimal sequence; the failure message prints a test that can be pasted (planning/
//! architecture.md section 12 E1: "the failure message prints the seed AND the minimal sequence").

use azlin_escalate_e1::compare::{
    differential, differential_planted, record, run_steps, run_steps_with, shrink_failure, Outcome,
    Plan,
};
use azlin_escalate_e1::gen::Step;
use azlin_escalate_e1::model::{AppId, Dev, MKind, MOp, MSigner, Mem, ModelEntry, Planted, Role};
use azlin_seams::Stamp;

/// A seed whose run fails with the planted bug in the model (the model forgets that a suspended member's
/// devices sign nothing, so the generator hands them the pen and the production refuses).
fn first_failing_seed(steps: usize) -> Option<(u64, Outcome)> {
    for seed in 0..600u64 {
        let outcome =
            differential_planted(seed, steps, Plan::Mixed, Planted::SuspendedMembersMaySign);
        if matches!(outcome, Outcome::Fail(_)) {
            return Some((seed, outcome));
        }
    }
    None
}

#[test]
fn a_failing_seed_replays_exactly() {
    let (seed, first) =
        first_failing_seed(28).expect("no seed in 0..600 fails with the planted bug");
    let again = differential_planted(seed, 28, Plan::Mixed, Planted::SuspendedMembersMaySign);
    match (first, again) {
        (Outcome::Fail(a), Outcome::Fail(b)) => {
            assert_eq!(a.index, b.index, "the same step fails");
            assert_eq!(a.detail, b.detail, "with the same difference");
            assert_eq!(a.history, b.history, "after the same steps");
            assert_eq!(a.fail, b.fail);
        }
        _ => panic!("the seed did not fail twice"),
    }
    // and a passing seed passes twice with the same counts
    let one = differential(4, 28);
    let two = differential(4, 28);
    match (one, two) {
        (Outcome::Pass(a), Outcome::Pass(b)) => {
            assert_eq!(a.entries, b.entries);
            assert_eq!(a.admitted, b.admitted);
            assert_eq!(a.per_class, b.per_class);
        }
        (Outcome::Fail(f), _) | (_, Outcome::Fail(f)) => panic!("seed 4 fails: {}", f.message()),
    }
}

/// A recorded setup plus four entries that trigger the planted bug: the owner invites a member, the member
/// joins, the owner suspends it, and a device of the suspended member signs an app op. The shrinker must cut
/// the setup away and leave exactly the five entries that matter (the genesis and the four).
#[test]
fn shrink_reduces_a_planted_failure_to_a_minimal_sequence() {
    let (kind, account, mut steps) = record(11, 0);
    assert!(
        steps.len() >= 8,
        "the setup of the seed has {} entries",
        steps.len()
    );
    let last = steps.last().map(|s| s.now.as_secs()).unwrap_or(1_000_000);
    let owner = Dev(11);
    let victim = Mem(120);
    let victim_device = Dev(9001);
    let mut eid = 9_000_000u64;
    let mut push = |signer: Dev, op: MOp, extra: Option<Dev>, at: u64| {
        eid += 1;
        let mut entry = ModelEntry::new(eid, MSigner::Device(signer), op);
        if let Some(d) = extra {
            entry
                .cosigs
                .push((d, azlin_escalate_e1::model::CosigClaim::Valid));
        }
        steps.push(Step {
            entry,
            now: Stamp::from_secs(last + at),
        });
    };
    push(
        owner,
        MOp::MemberInvite {
            member: victim,
            kind: MKind::Person,
            role: Role::Member,
            secret: 4242,
            groups: Default::default(),
            app: None,
        },
        None,
        10,
    );
    push(
        victim_device,
        MOp::MemberJoin {
            member: victim,
            device: victim_device,
            secret: 4242,
        },
        Some(owner),
        20,
    );
    push(owner, MOp::MemberSuspend { member: victim }, None, 30);
    push(
        victim_device,
        MOp::App {
            name: AppId(1),
            body_len: 5,
        },
        None,
        40,
    );
    // production and model agree on the honest sequence
    assert!(
        run_steps(kind, account, &steps).is_none(),
        "the honest sequence must pass"
    );
    // with the bug planted the model admits the suspended member's app op and the production refuses it
    let failure = run_steps_with(
        kind,
        account,
        &steps,
        Some(Planted::SuspendedMembersMaySign),
    )
    .expect("the planted bug must be caught");
    let minimal = shrink_failure(&failure);
    assert_eq!(
        minimal.len(),
        5,
        "the minimal sequence is the genesis and the four entries: {minimal:#?}"
    );
    // 1-minimal: dropping any one entry makes the failure go away
    for skip in 0..minimal.len() {
        let reduced: Vec<Step> = minimal
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, s)| s.clone())
            .collect();
        assert!(
            run_steps_with(
                kind,
                account,
                &reduced,
                Some(Planted::SuspendedMembersMaySign)
            )
            .is_none(),
            "entry {skip} of the minimal sequence is not needed"
        );
    }
}

/// The message of a failure names the seed and ends with a test that replays the minimal sequence.
#[test]
fn failure_message_prints_a_pasteable_test() {
    let (seed, outcome) =
        first_failing_seed(28).expect("no seed in 0..600 fails with the planted bug");
    let Outcome::Fail(f) = outcome else {
        panic!("the seed passed");
    };
    let text = f.message();
    assert!(text.contains(&format!("seed {seed}")), "{text}");
    assert!(text.contains("#[test]"), "{text}");
    assert!(
        text.contains(&format!("fn e1_regression_seed_{seed}()")),
        "{text}"
    );
    assert!(text.contains("run_steps("), "{text}");
    assert!(
        text.contains("Step {"),
        "the body lists the entries: {text}"
    );
    assert!(
        text.contains("Stamp::from_secs("),
        "times are printed as code: {text}"
    );
    // the minimal sequence is much shorter than the run
    let minimal = shrink_failure(&f);
    assert!(
        minimal.len() < f.history.len(),
        "{} of {} entries remain",
        minimal.len(),
        f.history.len()
    );
}
