//! The triage view (never part of the gate): one collecting run that does not stop at the first
//! disagreement, groups every disagreement by its signature and prints one example of each, and prints how
//! every near-miss kind did. Run it with
//! `cargo test -p azlin-escalate-e1 --features azlin-seams/mutants e1_triage -- --ignored --nocapture`.

use std::collections::BTreeMap;

use azlin_escalate_e1::compare::{differential_collect, Mismatch, Outcome, Plan, Stats};
use azlin_escalate_e1::gen::NearMiss;

fn collect(seeds: std::ops::Range<u64>, plan: Plan) -> (Stats, Vec<String>) {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 4) as u64;
    let results: Vec<(Stats, Vec<String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let seeds = seeds.clone();
                scope.spawn(move || {
                    let mut stats = Stats::default();
                    let mut hard: Vec<String> = Vec::new();
                    for seed in seeds {
                        if seed % threads != t {
                            continue;
                        }
                        match differential_collect(seed, 28, plan) {
                            Outcome::Pass(s) => stats.merge(&s),
                            Outcome::Fail(f) => hard.push(f.message()),
                        }
                    }
                    (stats, hard)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("worker"))
            .collect()
    });
    let mut stats = Stats::default();
    let mut hard = Vec::new();
    for (s, h) in results {
        stats.merge(&s);
        hard.extend(h);
    }
    (stats, hard)
}

fn show(mismatches: &[Mismatch], hard: &[String]) {
    let mut groups: BTreeMap<String, Vec<&Mismatch>> = BTreeMap::new();
    for m in mismatches {
        groups.entry(m.signature()).or_default().push(m);
    }
    let mut ordered: Vec<(&String, &Vec<&Mismatch>)> = groups.iter().collect();
    ordered.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    println!(
        "TRIAGE: {} disagreements in {} groups, {} hard failures",
        mismatches.len(),
        ordered.len(),
        hard.len()
    );
    for (sig, list) in ordered {
        println!("---- {} x {}", list.len(), sig);
        if let Some(first) = list.first() {
            let entry = format!("{:?}", first.step.entry);
            let cut: String = entry.chars().take(900).collect();
            println!(
                "     seed {} step {} at {}s: {}",
                first.seed,
                first.index,
                first.step.now.as_secs(),
                cut
            );
            for p in &first.report.problems {
                println!("     problem: {p}");
            }
            if let Some(r) = &first.report.prod_refusal {
                println!("     production: {r}");
            }
        }
    }
    for h in hard.iter().take(5) {
        println!("---- HARD FAILURE\n{h}");
    }
}

#[test]
#[ignore = "triage view: run with -- --ignored --nocapture"]
fn e1_triage() {
    let seeds: u64 = std::env::var("AZLIN_ESCALATE_TRIAGE_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_500);
    let (stats, hard) = collect(0..seeds, Plan::Mixed);
    println!(
        "TRIAGE mixed: {} sequences, {} entries, {} admitted, per class {:?}",
        stats.sequences, stats.entries, stats.admitted, stats.per_class
    );
    println!("TRIAGE per op: {:?}", stats.per_op);
    show(&stats.mismatches, &hard);

    let focus_seeds = (seeds / 15).clamp(20, 120);
    let mut all: Vec<Mismatch> = Vec::new();
    let mut all_hard: Vec<String> = Vec::new();
    for nm in NearMiss::ALL {
        let (s, h) = collect(0..focus_seeds, Plan::Focus(nm));
        let n = s.near.get(&nm).copied().unwrap_or_default();
        println!(
            "TRIAGE near miss {:<46} cases {:>4} both refused {:>4} exact class {:>4} ({:?}), mismatches {}",
            nm.name(),
            n.cases,
            n.both_refused,
            n.exact_class,
            nm.expected(),
            s.mismatches.len()
        );
        all.extend(s.mismatches);
        all_hard.extend(h);
    }
    show(&all, &all_hard);
}
