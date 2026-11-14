//! The mutation round (feature (e) of the brief; COMMON-W1R R19; planning/architecture.md section 1.4
//! rule 1 and the mutation standard of the QUALITY rules).
//!
//! The guards of the other crates (`azlin_seams::mutant!`) are the harness's mutation round:
//! `AZLIN_MUTANT=<id>` (read once per process) turns one guard off, and a NAMED scenario must go
//! red. [`MUTANT_TABLE`] holds, for each guard of the composed crates, the killer: a number of
//! seeds of the generic runner (the first red wins, no shrink) or a directed scenario, and the
//! invariants that may kill it. [`WAIVERS`] holds the guards that composition cannot reach, each with
//! the reason and the crate test that kills it instead. `tests/mutation.rs` runs the round: it
//! re-executes its own test binary as a child for each guard, parses the child's one result line
//! ([`run_child`]) and requires RED with an allowed invariant, and a GREEN control without the guard.

use crate::config::Stage;
use crate::directed::{directed_named, run_directed};
use crate::scenario::{FailureKind, Scenario, Verdict};

/// How a guard is killed.
///
/// Implements planning/architecture.md section 1.4 rule 1 (the guard reads its switch as a
/// parameter of the process).
///
/// Caller: `MutantRow`, `run_child`, tests/mutation.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Killer {
    /// The generic runner: at most `seeds` seeds of the stage, the first red wins.
    Random {
        /// The stage.
        stage: Stage,
        /// The most seeds.
        seeds: u64,
    },
    /// A hand-written scenario of `directed.rs`, by name.
    Directed(&'static str),
}

/// One guard, its killer and the invariants that may kill it.
///
/// Implements planning/architecture.md section 1.4 rule 1 and the mutation standard.
///
/// Caller: `MUTANT_TABLE`, tests/mutation.rs.
#[derive(Clone, Copy, Debug)]
pub struct MutantRow {
    /// The id of the guard.
    pub guard: &'static str,
    /// What kills it.
    pub killer: Killer,
    /// The names of the invariants that may be the one that goes red.
    pub expect: &'static [&'static str],
}

const RANDOM: Killer = Killer::Random {
    stage: Stage::S1,
    seeds: 400,
};

const POLICY_RANDOM: Killer = Killer::Random {
    stage: Stage::S1,
    seeds: 300,
};

/// The guards of the composed crates of stage 1 and their killers.
///
/// Implements planning/architecture.md section 1.4 rule 1.
///
/// Caller: tests/mutation.rs, tests/x08.
pub const MUTANT_TABLE: &[MutantRow] = &[
    MutantRow {
        guard: "chain_checks_chain_id",
        killer: Killer::Directed("hostile_heads_and_entries"),
        expect: &[
            "inv_chain_every_copy_verifies_from_genesis",
            "inv_chain_no_fork_is_ever_accepted",
        ],
    },
    MutantRow {
        guard: "chain_checks_intact",
        killer: Killer::Directed("hostile_heads_and_entries"),
        expect: &[
            "inv_refusal_every_refusal_is_typed_and_classed",
            "inv_chain_every_copy_verifies_from_genesis",
        ],
    },
    MutantRow {
        guard: "chain_second_genesis_check_off",
        killer: Killer::Directed("hostile_heads_and_entries"),
        expect: &[
            "inv_chain_no_fork_is_ever_accepted",
            "inv_chain_every_copy_verifies_from_genesis",
        ],
    },
    MutantRow {
        guard: "chain_checks_seq",
        killer: RANDOM,
        expect: &["inv_chain_every_copy_verifies_from_genesis"],
    },
    MutantRow {
        guard: "chain_checks_prev",
        killer: RANDOM,
        expect: &["inv_chain_every_copy_verifies_from_genesis"],
    },
    MutantRow {
        guard: "chain_checks_stamp",
        killer: RANDOM,
        expect: &["inv_stamp_never_runs_backwards"],
    },
    MutantRow {
        guard: "chain_checks_epoch",
        killer: RANDOM,
        expect: &[
            "inv_restore_the_lost_timeline_stays_lost",
            "inv_chain_every_copy_verifies_from_genesis",
        ],
    },
    MutantRow {
        guard: "chain_fork_check_off",
        killer: Killer::Directed("hostile_heads_and_entries"),
        expect: &["inv_chain_no_fork_is_ever_accepted"],
    },
    MutantRow {
        guard: "chain_pin_accepts_older",
        killer: Killer::Directed("hostile_heads_and_entries"),
        expect: &["inv_pin_never_goes_backwards"],
    },
    MutantRow {
        guard: "chain_pin_fork_unchecked",
        killer: Killer::Directed("hostile_heads_and_entries"),
        expect: &["inv_chain_no_fork_is_ever_accepted"],
    },
    MutantRow {
        guard: "chain_pin_skips_signature_check",
        killer: Killer::Directed("hostile_heads_and_entries"),
        expect: &[
            "inv_pin_only_holds_heads_the_group_signed",
            "inv_pin_never_goes_backwards",
        ],
    },
    MutantRow {
        guard: "chain_pin_epoch_floor_off",
        killer: Killer::Directed("restore_then_hostile"),
        expect: &[
            "inv_restore_the_lost_timeline_stays_lost",
            "inv_pin_the_epoch_never_goes_down",
            "inv_pin_never_goes_backwards",
        ],
    },
    MutantRow {
        guard: "chain_peer_ahead_is_quiet",
        killer: Killer::Directed("peers_and_withheld_heads"),
        expect: &["inv_peer_a_hidden_head_shows"],
    },
    MutantRow {
        guard: "chain_peer_fork_is_quiet",
        killer: Killer::Directed("peers_and_withheld_heads"),
        expect: &[
            "inv_peer_a_hidden_head_shows",
            "inv_chain_no_fork_is_ever_accepted",
        ],
    },
    MutantRow {
        guard: "chain_peer_epoch_ignored",
        killer: RANDOM,
        expect: &[
            "inv_peer_a_hidden_head_shows",
            "inv_peer_a_peers_junk_is_no_alarm",
        ],
    },
    MutantRow {
        guard: "chain_peer_skips_signature_check",
        killer: Killer::Directed("peers_and_withheld_heads"),
        expect: &["inv_peer_a_peers_junk_is_no_alarm"],
    },
    MutantRow {
        guard: "chain_restore_group_alone_accepted",
        killer: Killer::Directed("restore_then_hostile"),
        expect: &["inv_restore_only_through_the_signed_epoch_path"],
    },
    MutantRow {
        guard: "chain_restore_trust_skipped",
        killer: Killer::Directed("restore_then_hostile"),
        expect: &["inv_restore_only_through_the_signed_epoch_path"],
    },
    MutantRow {
        guard: "chain_restore_replay_check_off",
        killer: Killer::Directed("restore_then_hostile"),
        expect: &["inv_restore_only_through_the_signed_epoch_path"],
    },
    MutantRow {
        guard: "chain_restore_late_record_check_off",
        killer: Killer::Directed("restore_late_record_keeps_progress"),
        expect: &["inv_restore_a_late_record_keeps_progress"],
    },
    MutantRow {
        guard: "chain_restore_resend_off",
        killer: Killer::Directed("restore_story"),
        expect: &["inv_restore_held_entries_are_sent_again"],
    },
    MutantRow {
        guard: "chain_checkpoint_age_off",
        killer: Killer::Directed("checkpoints_by_age"),
        expect: &["inv_chain_a_checkpoint_is_written_when_due"],
    },
    MutantRow {
        guard: "chain_checkpoint_entries_off",
        killer: Killer::Directed("thousand_entries"),
        expect: &["inv_chain_a_checkpoint_is_written_when_due"],
    },
    MutantRow {
        guard: "chain_checkpoint_head_unverified",
        killer: Killer::Directed("checkpoints_by_age"),
        expect: &["inv_chain_checkpoint_equals_genesis"],
    },
    MutantRow {
        guard: "policy_effective_skips_clamp",
        killer: POLICY_RANDOM,
        expect: &["inv_policy_a_group_only_tightens"],
    },
    MutantRow {
        guard: "policy_epoch_always_bumps",
        killer: POLICY_RANDOM,
        expect: &["inv_policy_epoch_bumps_only_on_change"],
    },
    MutantRow {
        guard: "policy_quorum_needs_one",
        killer: POLICY_RANDOM,
        expect: &["inv_policy_quorum_counts_distinct_people"],
    },
    MutantRow {
        guard: "policy_quorum_counts_devices",
        killer: POLICY_RANDOM,
        expect: &["inv_policy_quorum_counts_distinct_people"],
    },
    MutantRow {
        guard: "policy_quorum_counts_apps",
        killer: POLICY_RANDOM,
        expect: &["inv_policy_quorum_counts_distinct_people"],
    },
    MutantRow {
        guard: "policy_quorum_off_by_one",
        killer: POLICY_RANDOM,
        expect: &["inv_policy_quorum_counts_distinct_people"],
    },
    MutantRow {
        guard: "seams_restore_epoch_wraps",
        killer: Killer::Directed("restore_epoch_exhaustion"),
        expect: &["inv_restore_only_through_the_signed_epoch_path"],
    },
];

/// The guards that composition cannot reach, each with the reason and the crate test that kills it.
///
/// Implements planning/architecture.md section 1.4 rule 1.
///
/// Caller: tests/mutation.rs, tests/x08.
pub const WAIVERS: &[(&str, &str)] = &[
    (
        "chain_genesis_prev_zero",
        "only an EMPTY chain meets a genesis, and every copy of the world starts with its genesis and only a restart or a wipe empties one, behind the pin rule of glue rule G18; azlin-chain's genesis_must_be_seq_zero_with_zero_prev kills it",
    ),
    (
        "seams_hash32_ignores_domain",
        "the harness hashes under one domain per use and never compares two domains; azlin-seams' hash32_is_domain_separated_and_displays_lowercase_hex kills it",
    ),
    (
        "seams_hash32_no_length_frame",
        "the harness hashes fixed-width parts; azlin-seams' hash32_is_domain_separated_and_displays_lowercase_hex kills it",
    ),
    (
        "seams_byte_count_wraps",
        "the harness has no byte counts but the policy pool's; azlin-seams' byte_count_checked_arithmetic_never_wraps kills it",
    ),
    (
        "seams_verifier_any_alg",
        "the harness signs only Ed25519; azlin-seams' unsupported_alg_is_an_error_not_a_pass kills it",
    ),
    (
        "seams_fake_scheme_ignores_key",
        "the harness checks the key id of a head and a record itself before the scheme; azlin-seams' fake_scheme_verifies_only_its_own_signatures kills it",
    ),
    (
        "seams_rng_below_modulo",
        "a modulo bias of the generator moves no decision of the harness; azlin-seams' sim_rng_is_reproducible_and_unbiased_below_n kills it",
    ),
    (
        "seams_policy_epoch_wraps",
        "the policy epoch of the harness never reaches the largest number; azlin-seams' policy_epoch_counts_with_a_checked_next_and_a_strict_decimal_text kills it",
    ),
];

/// Runs the killer of a spec in this process and returns the one result line the parent parses:
/// `RESULT red <invariant> seeds=<n>` or `RESULT green seeds=<n>`. A spec is `random:<stage>:<seeds>` or
/// `directed:<name>`; the guard under test is the process's own (`AZLIN_MUTANT`).
///
/// Implements planning/architecture.md section 1.4 rule 1.
///
/// Caller: tests/mutation.rs (`mutation_child`).
pub fn run_child(spec: &str) -> String {
    let mut parts = spec.split(':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("random"), Some(stage), Some(seeds)) => {
            let stage = stage
                .parse::<u8>()
                .ok()
                .and_then(Stage::from_number)
                .unwrap_or(Stage::S1);
            let Ok(seeds) = seeds.parse::<u64>() else {
                return String::from("RESULT error bad-seed-count seeds=0");
            };
            for seed in 0..seeds {
                let outcome = Scenario::standard(seed, stage).run();
                if let Verdict::Red(failure) = &outcome.verdict {
                    // A known gap is a defect of the crates the registry already keeps: it is no
                    // kill of a guard.
                    if matches!(failure.kind, FailureKind::KnownGap(_)) {
                        continue;
                    }
                    let name = if failure.kind == FailureKind::Harness {
                        "harness"
                    } else {
                        failure.invariant.name()
                    };
                    return format!("RESULT red {name} seeds={}", seed + 1);
                }
            }
            format!("RESULT green seeds={seeds}")
        }
        (Some("directed"), Some(name), None) => match directed_named(name).and_then(run_directed) {
            Some(outcome) => match &outcome.verdict {
                Verdict::Red(failure) if !matches!(failure.kind, FailureKind::KnownGap(_)) => {
                    format!("RESULT red {} seeds=1", failure.invariant.name())
                }
                Verdict::Red(_) | Verdict::Green => String::from("RESULT green seeds=1"),
            },
            None => format!("RESULT error unknown-directed-{name} seeds=0"),
        },
        _ => String::from("RESULT error bad-spec seeds=0"),
    }
}

impl Killer {
    /// The spec string of this killer, as `run_child` reads it.
    pub fn spec(&self) -> String {
        match self {
            Killer::Random { stage, seeds } => format!("random:{}:{seeds}", stage.number()),
            Killer::Directed(name) => format!("directed:{name}"),
        }
    }
}
