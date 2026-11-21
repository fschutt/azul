//! S1-06: the restore story is green (planning/architecture.md section 2.17, section 6.10 V3, section
//! 12 E4). Each directed scenario of the restore path runs green on the committed code; the mutation
//! round (`tests/mutation.rs`) shows each is red under its guard.

use azlin_sim::{directed_named, run_directed, DirectedExpect, Verdict};

fn green(name: &str) {
    let Some(d) = directed_named(name) else {
        panic!("no directed scenario {name}");
    };
    assert_eq!(d.expect, DirectedExpect::Green);
    let Some(outcome) = run_directed(d) else {
        panic!("the scenario {name} could not be built");
    };
    match &outcome.verdict {
        Verdict::Green => {}
        Verdict::Red(f) => panic!(
            "{name}: {} at step {}: {}\n{}\nstate:\n{}",
            f.invariant.name(),
            f.at_step,
            f.message,
            f.trace_tail.join("\n"),
            f.state.join("\n")
        ),
    }
    assert!(outcome.tallies.steps > 3);
}

#[test]
fn s1_06_a_record_signed_by_the_map_key_reanchors_every_listed_chain_and_held_entries_are_sent_again(
) {
    green("restore_story");
}

#[test]
fn s1_06_hostile_records_and_heads_after_a_restore_are_refused() {
    green("restore_then_hostile");
}

#[test]
fn s1_06_a_device_offline_during_the_whole_restore_heals() {
    green("offline_device_heals");
}

#[test]
fn s1_06_a_device_born_after_the_restore_starts_at_the_epoch() {
    green("new_device_after_restore");
}

#[test]
fn s1_06_a_late_record_keeps_progress() {
    green("restore_late_record_keeps_progress");
}

#[test]
fn s1_06_the_restore_epoch_counter_refuses_the_restore_it_cannot_count() {
    green("restore_epoch_exhaustion");
}
