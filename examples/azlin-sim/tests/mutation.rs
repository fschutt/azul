//! The mutation round (planning/architecture.md section 1.4 rule 1; COMMON-W1R R19). Run it with
//! `AZLIN_SIM_MUTATION=1 cargo test -p azlin-sim --features azlin-seams/mutants --test mutation`: it
//! re-executes its own test binary as a child for each guard (`--exact mutation_child`, the guard in
//! `AZLIN_MUTANT`, the killer in `AZLIN_SIM_CHILD`: the pattern of azlin-seams/tests/mutants.rs), parses
//! the child's one result line and requires RED with an allowed invariant for every row, and a GREEN
//! control (no `AZLIN_MUTANT`) of the same killer. Without `AZLIN_SIM_MUTATION=1` the round prints
//! SKIPPED and passes.

use std::process::Command;

use azlin_sim::{run_child, Killer, MUTANT_TABLE, WAIVERS};

#[test]
fn mutation_child() {
    // gate:env allow: the mutation round re-executes this test with the killer in the environment
    if let Ok(spec) = std::env::var("AZLIN_SIM_CHILD") {
        println!("{}", run_child(&spec));
    }
}

/// What a child printed: red with an invariant, or green, and the seeds it ran.
#[derive(Debug)]
struct ChildResult {
    red: Option<String>,
    seeds: u64,
}

fn child(guard: Option<&str>, spec: &str) -> Result<ChildResult, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = Command::new(exe);
    command
        .args([
            "--exact",
            "mutation_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AZLIN_SIM_CHILD", spec)
        .env_remove("AZLIN_MUTANT");
    if let Some(guard) = guard {
        command.env("AZLIN_MUTANT", guard);
    }
    let output = command.output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let line = text
        .lines()
        .find(|l| l.starts_with("RESULT "))
        .ok_or_else(|| {
            format!(
                "no result line in the child's output: {text} {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
    let parts: Vec<&str> = line.split(' ').collect();
    let seeds = parts
        .iter()
        .find_map(|p| p.strip_prefix("seeds="))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    match parts.as_slice() {
        ["RESULT", "red", invariant, ..] => Ok(ChildResult {
            red: Some((*invariant).to_string()),
            seeds,
        }),
        ["RESULT", "green", ..] => Ok(ChildResult { red: None, seeds }),
        _ => Err(format!("unreadable result line: {line}")),
    }
}

fn mutation_on() -> bool {
    // gate:env allow: the mutation round is opt-in
    std::env::var("AZLIN_SIM_MUTATION").as_deref() == Ok("1")
}

#[test]
fn mutation_round() {
    if !mutation_on() {
        println!("SKIPPED: set AZLIN_SIM_MUTATION=1 and build with --features azlin-seams/mutants");
        return;
    }
    let mut failures = Vec::new();
    let mut seeds_to_kill: Vec<(String, u64)> = Vec::new();
    for row in MUTANT_TABLE {
        let spec = row.killer.spec();
        let control = child(None, &spec);
        match &control {
            Ok(result) if result.red.is_none() => {}
            Ok(result) => {
                failures.push(format!("{}: the CONTROL (no guard off) is red with {:?}: the killer is not green on the committed code", row.guard, result.red));
                continue;
            }
            Err(error) => {
                failures.push(format!("{}: control failed: {error}", row.guard));
                continue;
            }
        }
        match child(Some(row.guard), &spec) {
            Ok(result) => match &result.red {
                Some(invariant) if row.expect.contains(&invariant.as_str()) => {
                    println!(
                        "KILLED {} by {} in {} seeds",
                        row.guard, invariant, result.seeds
                    );
                    seeds_to_kill.push((row.guard.to_string(), result.seeds));
                }
                Some(invariant) => failures.push(format!(
                    "{}: red with {invariant}, which is not in the allowed set {:?}",
                    row.guard, row.expect
                )),
                None => failures.push(format!(
                    "{}: SURVIVED the killer {spec} (was the feature azlin-seams/mutants on?)",
                    row.guard
                )),
            },
            Err(error) => failures.push(format!("{}: {error}", row.guard)),
        }
    }
    let mut counts: Vec<u64> = seeds_to_kill.iter().map(|(_, n)| *n).collect();
    counts.sort_unstable();
    let median = counts.get(counts.len() / 2).copied().unwrap_or(0);
    println!("median seeds to kill: {median}; waived: {}", WAIVERS.len());
    for (guard, seeds) in &seeds_to_kill {
        println!("seeds_to_kill {guard} {seeds}");
    }
    let guards = MUTANT_TABLE.len() + WAIVERS.len();
    if WAIVERS.len() * 5 > guards {
        failures.push(format!(
            "{} of {guards} guards are waived: over 20 %",
            WAIVERS.len()
        ));
    }
    for (guard, reason) in WAIVERS {
        if reason.len() < 20 {
            failures.push(format!("the waiver of {guard} has no reason"));
        }
    }
    for row in MUTANT_TABLE {
        if let Killer::Random { seeds, .. } = row.killer {
            if seeds > 400 {
                failures.push(format!("{}: a random killer of {seeds} seeds", row.guard));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The ids of the guards in the source of a crate next to this one (`mutant!("id")`, outside comments).
fn guards_of(crate_dir: &str) -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_dir)
        .join("src");
    let mut found = Vec::new();
    let mut folders = vec![root];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for line in text.lines() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("//") {
                        continue;
                    }
                    let mut rest = line;
                    while let Some(at) = rest.find("mutant!(\"") {
                        let after = &rest[at + 9..];
                        if let Some(end) = after.find('"') {
                            found.push(after[..end].to_string());
                            rest = &after[end..];
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found.retain(|id| id != "id" && id != "chain_checks_prev_example");
    found
}

#[test]
fn every_guard_of_the_composed_crates_has_a_killer_or_a_waiver() {
    let mut uncovered = Vec::new();
    for crate_dir in ["azlin-chain", "azlin-policy", "azlin-seams"] {
        for guard in guards_of(crate_dir) {
            let covered = MUTANT_TABLE.iter().any(|row| row.guard == guard)
                || WAIVERS.iter().any(|(g, _)| *g == guard);
            if !covered {
                uncovered.push(format!("{crate_dir}: {guard}"));
            }
        }
    }
    if mutation_on() {
        assert!(uncovered.is_empty(), "guards no row covers: {uncovered:?}");
    } else if !uncovered.is_empty() {
        println!(
            "WARNING: guards that no row covers (a failure in the mutation hold): {uncovered:?}"
        );
    }
    // Every row names a guard that exists.
    let all: Vec<String> = ["azlin-chain", "azlin-policy", "azlin-seams"]
        .iter()
        .flat_map(|c| guards_of(c))
        .collect();
    for row in MUTANT_TABLE {
        assert!(
            all.iter().any(|g| g == row.guard),
            "the row of {} names no guard of the composed crates",
            row.guard
        );
    }
}
