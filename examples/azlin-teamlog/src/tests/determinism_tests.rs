//! The mutation-guard registry, the authority table against the brief, the golden fixtures of S5 (still to
//! come) and the hygiene of the crate (checks 15 and 16 of the brief and the launch rules; lane
//! W1-S17c2). Everything here reads the crate's own source or its data: no entry is checked.

use std::collections::BTreeSet;

use azlin_policy::QuorumClass;

use crate::admit::MUTANT_IDS;
use crate::authority::{
    role_assignable, role_touchable, row, QuorumFacts, QuorumRule, Reach, Row, ROLE_ASSIGNMENT,
    TABLE,
};
use crate::ops::OpName;
use crate::roles::Role;

/// Every source file of the crate that is not a test, with its text.
const SOURCES: [(&str, &str); 19] = [
    ("admit.rs", include_str!("../admit.rs")),
    ("apply.rs", include_str!("../apply.rs")),
    ("authority.rs", include_str!("../authority.rs")),
    ("decode.rs", include_str!("../decode.rs")),
    ("delegate.rs", include_str!("../delegate.rs")),
    ("devices.rs", include_str!("../devices.rs")),
    ("effects.rs", include_str!("../effects.rs")),
    ("groups.rs", include_str!("../groups.rs")),
    ("hash.rs", include_str!("../hash.rs")),
    ("invite.rs", include_str!("../invite.rs")),
    ("lib.rs", include_str!("../lib.rs")),
    ("members.rs", include_str!("../members.rs")),
    ("ops.rs", include_str!("../ops.rs")),
    ("proposal.rs", include_str!("../proposal.rs")),
    ("replay.rs", include_str!("../replay.rs")),
    ("roles.rs", include_str!("../roles.rs")),
    ("sealed.rs", include_str!("../sealed.rs")),
    ("state.rs", include_str!("../state.rs")),
    ("verify.rs", include_str!("../verify.rs")),
];

/// The test files of the crate that carry the killers, with their text.
const TEST_SOURCES: [(&str, &str); 9] = [
    ("admit_tests.rs", include_str!("admit_tests.rs")),
    ("delegate_tests.rs", include_str!("delegate_tests.rs")),
    ("escalation_tests.rs", include_str!("escalation_tests.rs")),
    ("invite_tests.rs", include_str!("invite_tests.rs")),
    ("matrix_tests.rs", include_str!("matrix_tests.rs")),
    ("replay_tests.rs", include_str!("replay_tests.rs")),
    ("sealed_tests.rs", include_str!("sealed_tests.rs")),
    ("determinism_tests.rs", include_str!("determinism_tests.rs")),
    ("apply_tests.rs", include_str!("apply_tests.rs")),
];

/// The code lines of a source (a line that is a comment is not code).
fn code_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.is_empty()
        })
        .collect()
}

/// The ids of the guards a source names: the text between `mutant!("` and the next `"`.
fn guards_of(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in code_lines(text) {
        let mut rest = line;
        while let Some(start) = rest.find("mutant!(\"") {
            let after = &rest[start + "mutant!(\"".len()..];
            match after.find('"') {
                Some(end) => {
                    found.push(after[..end].to_string());
                    rest = &after[end..];
                }
                None => break,
            }
        }
    }
    found
}

/// The twelve guards of check 15 of the brief, each with the killer tests of this crate (typed here
/// from the brief and the documentation of `MUTANT_IDS`).
const KILLERS: [(&str, &[&str]); 12] = [
    (
        "teamlog_quorum_counts_devices",
        &["quorum_of_two_devices_of_one_person_refused"],
    ),
    (
        "teamlog_scope_ignores_subtree",
        &[
            "group_admin_cannot_leave_its_subtree",
            "scope_flows_down_nested_groups_only",
        ],
    ),
    (
        "teamlog_role_assign_unbounded",
        &["admin_cannot_make_or_touch_an_owner"],
    ),
    (
        "teamlog_last_owner_check_off",
        &["last_owner_cannot_be_demoted_removed_or_suspended"],
    ),
    (
        "teamlog_helpdesk_may_approve",
        &["helpdesk_cannot_approve_a_device_alone"],
    ),
    (
        "teamlog_delegate_any_op",
        &["delegate_key_signs_only_its_ops"],
    ),
    (
        "teamlog_pending_device_may_sign",
        &["pending_device_signs_nothing"],
    ),
    (
        "teamlog_suspended_may_sign",
        &["suspended_member_signs_nothing"],
    ),
    (
        "teamlog_group_cycle_check_off",
        &["group_move_cycle_refused"],
    ),
    (
        "teamlog_policy_loosen_allowed",
        &["policy_set_loosening_a_group_refused"],
    ),
    (
        "teamlog_prev_unchecked",
        &["wrong_prev_refused_with_reason"],
    ),
    (
        "teamlog_cosig_hash_unchecked",
        &["cosig_over_another_hash_not_counted"],
    ),
];

#[test]
fn mutant_ids_are_the_guards_in_the_source() {
    // The twelve ids of MUTANT_IDS are unique and are exactly the ids of the `mutant!(` sites of the
    // crate's source: a guard that is not listed, or a listed id with no guard, fails.
    let listed: BTreeSet<&str> = MUTANT_IDS.iter().copied().collect();
    assert_eq!(listed.len(), 12, "twelve distinct ids");
    let mut sites: Vec<String> = Vec::new();
    for (name, text) in SOURCES {
        for id in guards_of(text) {
            assert!(
                listed.contains(id.as_str()),
                "{name}: the guard {id} is not in MUTANT_IDS"
            );
            sites.push(id);
        }
    }
    let placed: BTreeSet<&str> = sites.iter().map(String::as_str).collect();
    assert_eq!(placed, listed, "every listed id has a guard in the source");
    assert!(sites.len() >= 12);
    for id in MUTANT_IDS {
        assert!(id.starts_with("teamlog_"), "{id}");
        assert!(
            id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}"
        );
    }
    // The killers table of this file names the same twelve.
    let killed: BTreeSet<&str> = KILLERS.iter().map(|(id, _)| *id).collect();
    assert_eq!(killed, listed);
}

#[test]
fn every_killer_names_a_test_that_exists() {
    for (id, killers) in KILLERS {
        assert!(!killers.is_empty(), "{id} has a killer");
        for killer in killers {
            let declared = format!("fn {killer}(");
            let found = TEST_SOURCES.iter().any(|(_, text)| {
                text.lines()
                    .any(|line| line.trim_start().starts_with(&declared))
            });
            assert!(found, "{id}: the killer test {killer} does not exist");
        }
    }
    // The documentation table of MUTANT_IDS names the same killers.
    let documented = include_str!("../admit.rs");
    for (id, killers) in KILLERS {
        let line = documented
            .lines()
            .find(|line| line.starts_with("/// | `") && line.contains(&format!("`{id}`")))
            .unwrap_or_else(|| panic!("{id} is documented in MUTANT_IDS"));
        for killer in killers.iter().take(1) {
            assert!(
                line.contains(killer),
                "{id}: the documentation names {killer}"
            );
        }
    }
}

#[test]
fn the_table_has_one_row_for_each_op_in_order() {
    assert_eq!(TABLE.len(), 20);
    for (row_of_table, op) in TABLE.iter().zip(OpName::ALL) {
        assert_eq!(row_of_table.op, op);
        assert_eq!(row(op), row_of_table);
    }
    for op in OpName::ALL {
        assert_eq!(row(op).op, op);
    }
}

/// The row as one line of words: the roles that may sign (with `@scope` for a role that acts in the
/// subtree of its scope group), whether the subject's own device may, whether a delegate key may, and
/// the quorum rule.
fn describe(row: &Row) -> String {
    let roles: Vec<String> = row
        .signers
        .iter()
        .map(|grant| match grant.reach {
            Reach::Account => grant.role.as_str().to_string(),
            Reach::InScope => format!("{}@scope", grant.role.as_str()),
        })
        .collect();
    let quorum = match row.quorum {
        QuorumRule::Never => "none".to_string(),
        QuorumRule::Always(class) => format!("always:{class}"),
        QuorumRule::SubjectIsAdminClass(class) => format!("subject_admin:{class}"),
        QuorumRule::RoleChangeIsAdminClass(class) => format!("role_change_admin:{class}"),
        QuorumRule::NotOwnDevice(class) => format!("not_own_device:{class}"),
        QuorumRule::ManagedToPrivate(class) => format!("managed_to_private:{class}"),
        QuorumRule::MoveLoosensPolicy(class) => format!("move_loosens_policy:{class}"),
    };
    format!(
        "{} | own:{} | delegable:{} | {}",
        roles.join(" "),
        row.own_member,
        row.delegable,
        quorum
    )
}

#[test]
fn the_table_says_what_the_brief_says() {
    // The authority table v0 of the brief of lane S17c, typed here as words, column by column: who may
    // sign (a group admin and helpdesk "in scope"), the member's own device, a delegate key, the quorum.
    let brief: [(OpName, &str); 20] = [
        (OpName::Genesis, " | own:false | delegable:false | none"),
        (
            OpName::DeviceAdd,
            "owner admin | own:true | delegable:false | not_own_device:quorum_enrollment",
        ),
        (
            OpName::DeviceRemove,
            "owner admin | own:true | delegable:false | subject_admin:admin_change",
        ),
        (
            OpName::MemberInvite,
            "owner admin group_admin@scope | own:false | delegable:true | none",
        ),
        (OpName::MemberJoin, " | own:false | delegable:false | none"),
        (
            OpName::MemberRole,
            "owner admin group_admin@scope | own:false | delegable:false | role_change_admin:admin_change",
        ),
        (
            OpName::MemberSuspend,
            "owner admin group_admin@scope helpdesk@scope | own:false | delegable:true | subject_admin:admin_change",
        ),
        (
            OpName::MemberRemove,
            "owner admin group_admin@scope | own:false | delegable:false | subject_admin:admin_change",
        ),
        (
            OpName::MemberManaged,
            "owner admin | own:false | delegable:false | managed_to_private:escrow_change",
        ),
        (
            OpName::GroupCreate,
            "owner admin group_admin@scope | own:false | delegable:false | none",
        ),
        (
            OpName::GroupMember,
            "owner admin group_admin@scope | own:false | delegable:false | none",
        ),
        // The one cell that lane W1-S17c3 added to the brief's table (X-DELTA-W1S47A-2, D-W1S17C3): a move
        // that would loosen the effective policy of a group of the moved subtree asks the quorum of a
        // policy change (and is for an owner or an admin: a group admin only tightens).
        (
            OpName::GroupMove,
            "owner admin group_admin@scope | own:false | delegable:false | move_loosens_policy:policy_change",
        ),
        (
            OpName::PolicySet,
            "owner admin group_admin@scope | own:false | delegable:false | always:policy_change",
        ),
        (
            OpName::EscrowSet,
            "owner admin | own:false | delegable:false | always:escrow_change",
        ),
        (
            OpName::AuditKeySet,
            "owner admin | own:false | delegable:false | always:policy_change",
        ),
        (
            OpName::AdminQuorumSet,
            "owner admin | own:false | delegable:false | always:policy_change",
        ),
        (
            OpName::EscrowOpen,
            "owner admin | own:false | delegable:false | always:escrow_open",
        ),
        (
            OpName::Delegate,
            "owner admin | own:false | delegable:false | always:admin_change",
        ),
        (OpName::Entitlement, " | own:false | delegable:false | none"),
        (
            OpName::App,
            "owner admin group_admin helpdesk member | own:false | delegable:false | none",
        ),
    ];
    for (op, words) in brief {
        assert_eq!(describe(row(op)), words, "{op}");
    }
    // The delegable ops are exactly the two the plan allows (7.3).
    let delegable: Vec<OpName> = TABLE.iter().filter(|r| r.delegable).map(|r| r.op).collect();
    assert_eq!(delegable, vec![OpName::MemberInvite, OpName::MemberSuspend]);
}

#[test]
fn a_quorum_rule_asks_for_its_class_exactly_when_its_condition_holds() {
    let none = QuorumFacts::default();
    let admin = QuorumFacts {
        subject_is_admin_class: true,
        ..none
    };
    let role_change = QuorumFacts {
        role_change_is_admin_class: true,
        ..none
    };
    let own = QuorumFacts {
        own_device: true,
        ..none
    };
    let to_private = QuorumFacts {
        managed_to_private: true,
        ..none
    };
    let loosens = QuorumFacts {
        move_loosens_policy: true,
        ..none
    };
    let class = QuorumClass::AdminChange;
    assert_eq!(QuorumRule::Never.class(admin), None);
    assert_eq!(QuorumRule::Always(class).class(none), Some(class));
    assert_eq!(QuorumRule::SubjectIsAdminClass(class).class(none), None);
    assert_eq!(
        QuorumRule::SubjectIsAdminClass(class).class(admin),
        Some(class)
    );
    assert_eq!(QuorumRule::RoleChangeIsAdminClass(class).class(none), None);
    assert_eq!(
        QuorumRule::RoleChangeIsAdminClass(class).class(role_change),
        Some(class)
    );
    assert_eq!(QuorumRule::NotOwnDevice(class).class(none), Some(class));
    assert_eq!(QuorumRule::NotOwnDevice(class).class(own), None);
    assert_eq!(QuorumRule::ManagedToPrivate(class).class(none), None);
    assert_eq!(
        QuorumRule::ManagedToPrivate(class).class(to_private),
        Some(class)
    );
    assert_eq!(QuorumRule::MoveLoosensPolicy(class).class(none), None);
    assert_eq!(QuorumRule::MoveLoosensPolicy(class).class(own), None);
    assert_eq!(
        QuorumRule::MoveLoosensPolicy(class).class(loosens),
        Some(class)
    );
}

/// The order of privilege, typed here from the brief.
fn rank(role: Role) -> u8 {
    match role {
        Role::Owner => 5,
        Role::Admin => 4,
        Role::GroupAdmin => 3,
        Role::Helpdesk => 2,
        Role::Member => 1,
        Role::Guest => 0,
    }
}

const ROLES: [Role; 6] = [
    Role::Owner,
    Role::Admin,
    Role::GroupAdmin,
    Role::Helpdesk,
    Role::Member,
    Role::Guest,
];

#[test]
fn the_role_assignment_table_says_what_the_brief_says() {
    // member_role | owner: any role incl. owner/admin; admin: group_admin, helpdesk, member, guest for
    // subjects below admin; group_admin: member, guest for subjects below group admin.
    for signer in ROLES {
        for new_role in ROLES {
            let want = match signer {
                Role::Owner => true,
                Role::Admin => matches!(
                    new_role,
                    Role::GroupAdmin | Role::Helpdesk | Role::Member | Role::Guest
                ),
                Role::GroupAdmin => matches!(new_role, Role::Member | Role::Guest),
                _ => false,
            };
            assert_eq!(
                role_assignable(signer, new_role),
                want,
                "{signer} assigns {new_role}"
            );
        }
        for subject in ROLES {
            let want = match signer {
                Role::Owner => true,
                Role::Admin => rank(subject) < rank(Role::Admin),
                Role::GroupAdmin => rank(subject) < rank(Role::GroupAdmin),
                _ => false,
            };
            assert_eq!(
                role_touchable(signer, subject),
                want,
                "{signer} touches {subject}"
            );
        }
    }
    assert_eq!(
        ROLE_ASSIGNMENT.len(),
        3,
        "only owner, admin and group admin change a role"
    );
}

#[test]
fn the_privilege_order_is_the_briefs() {
    for a in ROLES {
        for b in ROLES {
            assert_eq!(
                crate::roles::rank(a) > crate::roles::rank(b),
                rank(a) > rank(b)
            );
            assert_eq!(crate::roles::outranks(a, b), rank(a) > rank(b));
            assert_eq!(crate::roles::at_most(a, b), rank(a) <= rank(b));
        }
    }
    assert!(crate::roles::is_admin_class(Role::Owner) && crate::roles::is_admin_class(Role::Admin));
    assert!(!crate::roles::is_admin_class(Role::GroupAdmin));
    assert!(
        crate::roles::needs_scope(Role::GroupAdmin) && crate::roles::needs_scope(Role::Helpdesk)
    );
    assert!(!crate::roles::needs_scope(Role::Member));
}

// ---- the golden fixtures of S5 ---------------------------------------------------------------------

#[test]
#[ignore = "X-DELTA-W1SIM-9: S5 fixture not landed"]
fn team_log_entries_match_azlin_contract_fixture() {
    // Rule 9 of the brief: until S5 lands `team_log_entries.json` in azlin-contract's fixtures, this test
    // is ignored. It reads the fixture the day it exists: the file must be there, and then the entries
    // of the fixture (their preimages, hashes and signatures, made by the glue) must verify and replay
    // to the state the fixture records.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../azlin-contract/fixtures/c4/team_log_entries.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|why| panic!("S5 fixture team_log_entries.json is not landed: {why}"));
    assert!(
        text.contains("\"genesis\""),
        "the fixture holds a genesis entry"
    );
    panic!("write this test against {path:?}: the fixture has landed");
}

#[test]
#[ignore = "X-DELTA-W1SIM-9: S5 fixture not landed"]
fn genesis_hash_matches_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../azlin-contract/fixtures/c4/team_log_entries.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|why| panic!("S5 fixture team_log_entries.json is not landed: {why}"));
    assert!(
        text.contains("genesis_hash"),
        "the fixture pins the genesis hash"
    );
    panic!("write this test against {path:?}: the fixture has landed");
}

// ---- hygiene: the launch rules a test can hold ---------------------------------------------------

#[test]
fn no_function_of_this_crate_reads_a_clock_or_the_environment() {
    let forbidden = [
        "SystemTime",
        "Instant",
        "std::time",
        "std::env",
        "env::var",
        "std::fs",
        "std::net",
        "std::thread",
        "thread::spawn",
        "std::process",
        "rand::",
        "getrandom",
        "HashMap",
        "HashSet",
        "tokio",
    ];
    for (name, text) in SOURCES {
        for line in code_lines(text) {
            for word in forbidden {
                assert!(!line.contains(word), "{name}: `{word}` in `{line}`");
            }
        }
    }
}

#[test]
fn the_library_names_no_panicking_call_and_no_lenient_default() {
    let forbidden = [
        ".unwrap()",
        ".expect(",
        "panic!(",
        "unreachable!(",
        "todo!(",
        "unimplemented!(",
        "unwrap_or_default",
        "let _ =",
        ".ok()",
        "map_or(0",
        "unwrap_or(false",
    ];
    for (name, text) in SOURCES {
        // The test-only helpers of a source file sit under `#[cfg(test)]`: they are not the library.
        let library = text.split("#[cfg(test)]").next().unwrap_or("");
        for line in code_lines(library) {
            for word in forbidden {
                assert!(!line.contains(word), "{name}: `{word}` in `{line}`");
            }
        }
    }
}

#[test]
fn every_source_file_is_declared_and_cites_the_plan() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let lib = include_str!("../lib.rs");
    let mut files = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).expect("the src folder") {
        let entry = entry.expect("an entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(module) = name.strip_suffix(".rs") {
            if module != "lib" {
                files.insert(module.to_string());
            }
        }
    }
    for module in &files {
        assert!(
            lib.contains(&format!("pub mod {module};")),
            "src/{module}.rs is not declared in lib.rs"
        );
    }
    for (name, text) in SOURCES {
        assert!(
            text.contains("planning/architecture.md"),
            "{name} cites the plan"
        );
    }
    // Every test file of the folder is declared too.
    let tests_dir = dir.join("tests");
    let tests_lib = lib
        .split("mod tests {")
        .nth(1)
        .expect("the tests module of lib.rs");
    for entry in std::fs::read_dir(&tests_dir).expect("the tests folder") {
        let entry = entry.expect("an entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(module) = name.strip_suffix(".rs") {
            assert!(
                tests_lib.contains(&format!("mod {module};")),
                "src/tests/{module}.rs is not declared in lib.rs"
            );
        }
    }
}

#[test]
fn the_manifest_lists_the_dependencies_of_the_brief_and_no_forbidden_one() {
    let manifest = include_str!("../../Cargo.toml");
    for forbidden in [
        "tokio", "rusqlite", "rand", "proptest", "serde", "chrono", "reqwest",
    ] {
        assert!(
            !manifest.contains(&format!("{forbidden} =")),
            "{forbidden} is forbidden"
        );
    }
    for wanted in [
        "azlin-seams",
        "azlin-chain",
        "azlin-policy",
        "blake3",
        "chacha20poly1305",
        "zeroize",
        "thiserror",
    ] {
        assert!(
            manifest.contains(&format!("{wanted} =")),
            "{wanted} is a dependency"
        );
    }
}
