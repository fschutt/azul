//! Check 2 of the brief: the differential. The model, the production check as the metadata group runs it
//! (`verify_signatures` + `admit` + `apply`) and a device's own replay agree on every entry of every seeded
//! sequence (planning/architecture.md section 12 E1).

mod common;

use azlin_escalate_e1::model::{Class, OpKind};

/// THE DIFFERENTIAL. Every seeded sequence (the setup of a universe, then random, near-miss and valid
/// entries) runs on the model, on the metadata group's check and on a device's replay of the same entry: the
/// verdicts agree (the same class when the model finds one, a class of its set when it finds several), the
/// states agree after every admitted entry, and at the end `replay` from the first entry reaches the
/// incremental state's `state_hash`. A failure prints the failing seeds, the first failure and its minimal
/// sequence as a test body.
#[test]
fn e1_differential_3000_seeded_sequences_model_equals_admit_equals_replay() {
    let run = common::main_run();
    assert!(run.failures.is_empty(), "{}", common::fail_report(run));
}

/// A generator that never reaches the interesting branches proves nothing: every one of the 20 ops and every
/// refusal class must have been exercised at least 50 times in the run.
#[test]
fn e1_differential_covers_every_op_and_every_refusal_class() {
    let run = common::main_run();
    let scale = (run.seeds as f64 / 3000.0).min(1.0);
    let need = ((50.0 * scale) as usize).max(1);
    let mut short: Vec<String> = Vec::new();
    for op in OpKind::ALL {
        let seen = run.stats.per_op.get(&op).copied().unwrap_or(0);
        if seen < need {
            short.push(format!("op {} saw {seen} entries (need {need})", op.name()));
        }
    }
    for class in Class::ALL {
        let seen = run.stats.per_class.get(&class).copied().unwrap_or(0);
        if seen < need {
            short.push(format!("class {class:?} saw {seen} refusals (need {need})"));
        }
    }
    assert!(
        short.is_empty(),
        "the generator does not reach: {}\nper op: {:?}\nper class: {:?}",
        short.join("; "),
        run.stats.per_op,
        run.stats.per_class
    );
}

/// The shape of a run, printed for the reviewer (never fails): entries, admitted, the production's refusal
/// classes, the count of refusals whose class is not the model's first.
#[test]
fn e1_differential_run_summary() {
    let run = common::main_run();
    println!(
        "E1 differential: {} sequences, {} entries, {} admitted, {} refused with another class of the model's set than its first, known findings {:?}",
        run.seeds,
        run.stats.entries,
        run.stats.admitted,
        run.stats.class_order_differs,
        run.stats.known
    );
    println!("per op (entries): {:?}", run.stats.per_op);
    println!("per op (admitted): {:?}", run.stats.per_op_admitted);
    println!("per class (refusals): {:?}", run.stats.per_class);
}
