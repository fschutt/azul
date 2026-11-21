//! S1-04: a crash at every await point recovers (planning/architecture.md section 1.4 rule 2: a crash
//! at any point; section 6.10 V1). One case for each (handler, await point) of table 4.4 of the
//! brief; the case list is checked against the handler table, so a new handler or phase needs its row.

use std::collections::BTreeSet;

use azlin_sim::sched::{JobKind, HANDLERS};
use azlin_sim::{crash_cases, small_world, Scenario, Verdict, DIRECTED_SEED};

#[test]
fn s1_04_the_case_list_is_the_handler_table() {
    let cases = crash_cases();
    let wanted: BTreeSet<(JobKind, u8)> = JobKind::ALL
        .iter()
        .flat_map(|kind| (1..=kind.phases()).map(move |phase| (*kind, phase)))
        .collect();
    let have: BTreeSet<(JobKind, u8)> = cases.iter().map(|c| (c.kind, c.phase)).collect();
    assert_eq!(have, wanted, "every await point of the table has its case");
    assert_eq!(
        cases.len(),
        HANDLERS.iter().map(|h| h.phases.len()).sum::<usize>()
    );
    assert!(cases.len() >= 39, "{} cases", cases.len());
}

#[test]
fn s1_04_a_crash_at_every_await_point_recovers() {
    let mut failures = Vec::new();
    for case in crash_cases() {
        let Ok(scenario) = Scenario::new(DIRECTED_SEED, small_world()) else {
            panic!("the small world must validate");
        };
        let outcome = scenario.replay(&case.steps);
        if let Verdict::Red(failure) = &outcome.verdict {
            let detail = if failures.is_empty() {
                format!(
                    "\n{}\nstate:\n{}",
                    failure.trace_tail.join("\n"),
                    failure.state.join("\n")
                )
            } else {
                String::new()
            };
            failures.push(format!(
                "{} after phase {}: {} at step {}: {}{detail}",
                case.kind.handler().name,
                case.phase,
                failure.invariant.name(),
                failure.at_step,
                failure.message
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
