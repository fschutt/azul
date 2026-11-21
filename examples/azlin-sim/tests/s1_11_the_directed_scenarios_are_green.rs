//! S1-11: every directed scenario runs green on the committed code (the mutation table names the ones
//! that kill a guard).

use azlin_sim::{run_directed, DirectedExpect, Verdict, DIRECTED};

#[test]
fn s1_11_the_directed_scenarios_are_green() {
    let mut failures = Vec::new();
    for d in DIRECTED {
        let Some(outcome) = run_directed(d) else {
            failures.push(format!("{}: could not be built", d.name));
            continue;
        };
        match (d.expect, &outcome.verdict) {
            (DirectedExpect::Green, Verdict::Green) => {}
            (DirectedExpect::Red(id), Verdict::Red(f)) if f.invariant == id => {}
            (_, Verdict::Red(f)) => failures.push(format!(
                "{}: {} at step {}: {}\n{}\nstate:\n{}",
                d.name,
                f.invariant.name(),
                f.at_step,
                f.message,
                f.trace_tail.join("\n"),
                f.state.join("\n")
            )),
            (_, Verdict::Green) => failures.push(format!("{}: expected red, was green", d.name)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
