//! Checks 5 and 7 of the brief and the self-checks of the harness: the model imports nothing from
//! `azlin-teamlog`; the twelve production guards have their killers; the class table is total and the model's
//! constants are the plan's (planning/architecture.md section 12 E1, section 5.10).

use std::fs;
use std::path::PathBuf;

use azlin_escalate_e1::compare::{class_of, production_app_body_cap, production_defaults};
use azlin_escalate_e1::model::{AcctKind, Class, MPolicy, APP_BODY_CAP};
use azlin_policy::DeviceApproval;
use azlin_teamlog::{Invalid, PolicyReason, Refusal, MUTANT_IDS};

fn src(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src");
    path.push(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The independence rule: the model is written from the plan and the table, never from the production
/// check. Its file, and the files that only know the model, do not name the production crate.
#[test]
fn model_has_no_import_from_azlin_teamlog() {
    for name in ["model.rs", "universe.rs", "gen.rs"] {
        let text = src(name);
        assert!(
            !text.contains("azlin_teamlog"),
            "src/{name} names azlin_teamlog: the model side must not import the production"
        );
        assert!(
            !text.contains("use azlin_teamlog"),
            "src/{name} imports azlin_teamlog"
        );
    }
}

/// The twelve guards of `azlin-teamlog`, each with the test of this crate that must fail when the guard is
/// turned off: `AZLIN_MUTANT=<id> cargo test -p azlin-escalate-e1 --features azlin-seams/mutants` (the hold
/// script runs it for every id, every run must be red, and green without the variable).
const KILLERS: [(&str, &str); 12] = [
    (
        "teamlog_quorum_counts_devices",
        "near_miss_quorum_met_by_two_devices_of_one_person",
    ),
    (
        "teamlog_scope_ignores_subtree",
        "near_miss_signer_scope_off_by_one_group",
    ),
    (
        "teamlog_role_assign_unbounded",
        "near_miss_signer_one_role_too_low",
    ),
    (
        "teamlog_last_owner_check_off",
        "near_miss_last_owner_demoted_removed_or_suspended",
    ),
    (
        "teamlog_helpdesk_may_approve",
        "near_miss_helpdesk_adds_a_device_alone",
    ),
    (
        "teamlog_delegate_any_op",
        "near_miss_delegate_op_outside_the_named_set",
    ),
    (
        "teamlog_pending_device_may_sign",
        "near_miss_pending_device_signs",
    ),
    (
        "teamlog_suspended_may_sign",
        "near_miss_suspended_member_signs",
    ),
    (
        "teamlog_group_cycle_check_off",
        "near_miss_group_move_into_own_subtree",
    ),
    (
        "teamlog_policy_loosen_allowed",
        "near_miss_policy_set_that_loosens_a_group",
    ),
    ("teamlog_prev_unchecked", "near_miss_wrong_prev_or_seq"),
    (
        "teamlog_cosig_hash_unchecked",
        "near_miss_cosig_over_another_entry_hash",
    ),
];

/// The table of killers lists exactly the production's guards, and every killer is a test of this crate.
/// (The hold script runs the twelve mutants; this test is the part that needs no mutant.)
#[test]
fn e1_catches_every_teamlog_mutant() {
    let listed: Vec<&str> = KILLERS.iter().map(|(id, _)| *id).collect();
    let mut production: Vec<&str> = MUTANT_IDS.to_vec();
    let mut table = listed.clone();
    production.sort_unstable();
    table.sort_unstable();
    assert_eq!(
        table, production,
        "the killer table must list exactly azlin_teamlog::MUTANT_IDS"
    );
    let tests = src_of_tests();
    for (id, killer) in KILLERS {
        assert!(
            tests.contains(&format!("{killer} =>")) || tests.contains(&format!("fn {killer}")),
            "the killer {killer} of {id} is no test of this crate"
        );
    }
}

/// All the text of the integration tests of this crate.
fn src_of_tests() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    let mut all = String::new();
    if let Ok(entries) = fs::read_dir(&path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = fs::read_to_string(&p) {
                    all.push_str(&text);
                }
            }
        }
    }
    all
}

/// Every refusal of the production is classed, and the split by reason is the one the table in `compare.rs`
/// says (the match is exhaustive, so a new refusal does not compile; this test checks the answers).
#[test]
fn class_table_maps_every_refusal_to_one_class() {
    use azlin_chain::ChainRefusal;
    use azlin_seams::{ChainSeq, Hash32};
    use azlin_teamlog::{DeviceState, MemberState, OpName, Role};
    let cases: Vec<(Refusal, Class)> = vec![
        (Refusal::Chain(ChainRefusal::WrongChain), Class::Chain),
        (Refusal::Chain(ChainRefusal::NotGenesis), Class::Chain),
        (
            Refusal::Chain(ChainRefusal::Gap {
                expected: ChainSeq::new(2),
                got: ChainSeq::new(5),
            }),
            Class::Chain,
        ),
        (
            Refusal::Chain(ChainRefusal::WrongPrev {
                expected: Hash32::ZERO,
                got: Hash32::ZERO,
            }),
            Class::Chain,
        ),
        (Refusal::Chain(ChainRefusal::StampBackwards), Class::Chain),
        (Refusal::Chain(ChainRefusal::Corrupt), Class::Signature),
        (Refusal::BadSignature, Class::Signature),
        (Refusal::SignerUnknown, Class::SignerNotActive),
        (
            Refusal::SignerNotActive {
                state: DeviceState::Revoked,
            },
            Class::SignerNotActive,
        ),
        (
            Refusal::MemberNotActive {
                state: MemberState::Suspended,
            },
            Class::SignerNotActive,
        ),
        (
            Refusal::RoleMaySignNo {
                op: OpName::MemberRole,
                role: Some(Role::Guest),
            },
            Class::RoleOrScope,
        ),
        (
            Refusal::OutOfScope {
                op: OpName::GroupMove,
            },
            Class::RoleOrScope,
        ),
        (
            Refusal::QuorumShort { have: 1, need: 2 },
            Class::QuorumShort,
        ),
        (Refusal::AssertionMissing, Class::AssertionMissing),
        (
            Refusal::PolicyForbids(PolicyReason::Loosens(Vec::new())),
            Class::PolicyLoosens,
        ),
        (
            Refusal::PolicyForbids(PolicyReason::DeviceApprovalSelfOnly),
            Class::PolicyForbids,
        ),
        (
            Refusal::PolicyForbids(PolicyReason::MaxDevices { max: 10 }),
            Class::PolicyForbids,
        ),
        (
            Refusal::PolicyForbids(PolicyReason::EntitlementNotNewer),
            Class::PolicyForbids,
        ),
        (
            Refusal::PolicyForbids(PolicyReason::NotManaged),
            Class::PolicyForbids,
        ),
        (Refusal::LastOwner, Class::LastOwner),
        (Refusal::GroupCycle, Class::GroupCycle),
        (
            Refusal::DelegationDoesNotCover {
                op: OpName::MemberRole,
            },
            Class::DelegationDoesNotCover,
        ),
        (Refusal::DelegationExpired, Class::DelegationExpired),
        (Refusal::InviteInvalid, Class::InviteInvalid),
        (Refusal::UnknownOp, Class::UnknownOp),
        (Refusal::SubjectUnknown, Class::SubjectUnknown),
        (Refusal::ConsentMissing, Class::ConsentMissing),
        (Refusal::Invalid(Invalid::IdTaken), Class::Invalid),
        (Refusal::Invalid(Invalid::RootGroup), Class::Invalid),
        (Refusal::Invalid(Invalid::Limit("x")), Class::Invalid),
    ];
    for (refusal, class) in cases {
        assert_eq!(class_of(&refusal), class, "{refusal:?}");
    }
}

/// The model's defaults are the plan's table of planning/architecture.md section 5.10: the tracked fields of
/// `Policy::defaults(kind)` equal the model's, for the four kinds.
#[test]
fn model_defaults_equal_the_policy_defaults_of_the_plan() {
    for kind in AcctKind::ALL {
        let model: MPolicy = MPolicy::defaults(kind);
        let production = production_defaults(kind);
        assert_eq!(model, production, "{kind:?}");
    }
    // the cell the plan leaves to a reading: device_approval of team and business is `self or quorum`
    let team = azlin_policy::Policy::defaults(azlin_seams::AccountKind::Team);
    assert_eq!(team.device_approval, DeviceApproval::SelfOrQuorum);
}

/// The size cap of an app body is an estimate that the model adopts from the production; they must stay
/// equal.
#[test]
fn app_body_cap_equals_the_production_constant() {
    assert_eq!(APP_BODY_CAP as u64, production_app_body_cap());
}

/// The depth limit of the group tree is an estimate that the model adopts from the production; they must
/// stay equal.
#[test]
fn group_depth_limit_equals_the_production_constant() {
    assert_eq!(
        azlin_escalate_e1::model::MAX_GROUP_DEPTH,
        azlin_teamlog::MAX_GROUP_DEPTH
    );
}
