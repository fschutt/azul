//! Tests of the ops: the words of the table of planning/architecture.md section 5.9, the app name, the
//! mapping of an op to its name, and the redaction of the invite secret (lane W1-S17c1).

use std::collections::BTreeSet;

use azlin_policy::{AdminQuorum, PolicyPatch};
use azlin_seams::{ByteCount, Hash32, MemberKind};

use super::kit::{at, device, group, key, keys, member};
use crate::ops::{
    AppName, EntitlementState, EntitlementVersion, EscrowReason, InviteSecret, Op, OpName,
    PolicyScope, UnknownOp, APP_NAME_MAX_LEN,
};
use crate::roles::Role;

/// The words of the 19 fixed ops of the table of planning/architecture.md section 5.9, typed here
/// from the plan and not from `OpName::as_str`.
const TABLE_WORDS: [&str; 19] = [
    "genesis",
    "device_add",
    "device_remove",
    "member_invite",
    "member_join",
    "member_role",
    "member_suspend",
    "member_remove",
    "member_managed",
    "group_create",
    "group_member",
    "group_move",
    "policy_set",
    "escrow_set",
    "audit_key_set",
    "admin_quorum_set",
    "escrow_open",
    "delegate",
    "entitlement",
];

#[test]
fn every_word_of_the_table_is_an_op_and_only_those() {
    for word in TABLE_WORDS {
        let op = OpName::from_wire(word).expect("a word of the table");
        assert_eq!(op.as_str(), word);
    }
    // 19 fixed words and `app`: the 20 ops.
    assert_eq!(OpName::ALL.len(), 20);
    let words: BTreeSet<&str> = OpName::ALL.iter().map(|op| op.as_str()).collect();
    assert_eq!(words.len(), 20, "no two ops share a word");
    for word in TABLE_WORDS {
        assert!(words.contains(word));
    }
    assert!(words.contains("app"));
}

#[test]
fn unknown_wire_words_are_refused_not_defaulted() {
    for word in [
        "",
        "Genesis",
        "GENESIS",
        " genesis",
        "genesis ",
        "member-role",
        "member_roles",
        "grant",
        "space_grant",
        "account_delete",
        "app",
        "app:",
        "app:/x",
        "app:Notes/x",
        "app:-notes/x",
        "apps:notes/x",
    ] {
        assert_eq!(OpName::from_wire(word), Err(UnknownOp), "{word:?}");
    }
}

#[test]
fn app_wire_words_are_one_op_whatever_the_app() {
    for word in [
        "app:notes/new",
        "app:notes",
        "app:a.b_c-d/x/y",
        "app:0day/z",
    ] {
        assert_eq!(OpName::from_wire(word), Ok(OpName::App), "{word:?}");
    }
}

#[test]
fn app_names_are_lower_case_ascii_and_bounded() {
    assert!(AppName::new("notes").is_ok());
    assert!(AppName::new("a").is_ok());
    assert!(AppName::new("0a.b_c-d").is_ok());
    assert!(AppName::new(&"a".repeat(APP_NAME_MAX_LEN)).is_ok());
    assert_eq!(APP_NAME_MAX_LEN, 48);
    for bad in [
        "",
        "Notes",
        "n o",
        "n/x",
        "n:x",
        "-n",
        ".n",
        "_n",
        "caf\u{e9}",
        "n\0",
    ] {
        assert!(AppName::new(bad).is_err(), "{bad:?}");
    }
    assert!(AppName::new(&"a".repeat(APP_NAME_MAX_LEN + 1)).is_err());
    assert_eq!(AppName::new("notes").expect("valid").as_str(), "notes");
    assert_eq!(AppName::new("notes").expect("valid").to_string(), "notes");
}

/// One value of every op, for the tests that walk them all.
pub(crate) fn one_of_every_op() -> Vec<Op> {
    let mut ops = BTreeSet::new();
    ops.insert(OpName::MemberSuspend);
    vec![
        Op::Genesis {
            owner: member(1),
            device: device(1),
            member_keys: keys(1),
            device_keys: keys(2),
        },
        Op::DeviceAdd {
            member: member(1),
            device: device(2),
            keys: keys(3),
        },
        Op::DeviceRemove { device: device(2) },
        Op::MemberInvite {
            member: member(2),
            kind: MemberKind::Person,
            role: Role::Member,
            invite_hash: Hash32::ZERO,
            groups: BTreeSet::new(),
            app: None,
        },
        Op::MemberJoin {
            member: member(2),
            device: device(3),
            member_keys: keys(4),
            device_keys: keys(5),
            secret: InviteSecret([7; 32]),
        },
        Op::MemberRole {
            member: member(2),
            role: Role::Admin,
            admin_scope: None,
        },
        Op::MemberSuspend { member: member(2) },
        Op::MemberRemove { member: member(2) },
        Op::MemberManaged {
            member: member(2),
            managed: true,
        },
        Op::GroupCreate {
            group: group(1),
            parent: None,
        },
        Op::GroupMember {
            group: group(1),
            member: member(2),
            present: true,
        },
        Op::GroupMove {
            group: group(1),
            new_parent: group(2),
        },
        Op::PolicySet {
            scope: PolicyScope::Account,
            patch: PolicyPatch::none(),
        },
        Op::EscrowSet {
            escrow_pub: [1; 32],
            holders: BTreeSet::new(),
        },
        Op::AuditKeySet { audit_pub: [2; 32] },
        Op::AdminQuorumSet {
            quorum: AdminQuorum::TWO,
        },
        Op::EscrowOpen {
            member: member(2),
            reason: EscrowReason::LeaverHandover,
        },
        Op::Delegate {
            key: key(1),
            ops,
            expires: Some(at(5)),
        },
        Op::Entitlement {
            version: EntitlementVersion(1),
            state: EntitlementState::Active,
            quota: ByteCount::gib(1),
            seats: 3,
        },
        Op::App {
            name: AppName::new("notes").expect("valid"),
            body: vec![1],
        },
    ]
}

#[test]
fn op_name_of_every_op_is_the_op_of_the_table_in_order() {
    let ops = one_of_every_op();
    assert_eq!(ops.len(), OpName::ALL.len());
    for (op, name) in ops.iter().zip(OpName::ALL) {
        assert_eq!(op.name(), name);
    }
}

#[test]
fn the_invite_secret_never_prints_a_byte() {
    let secret = InviteSecret([0xAB; 32]);
    let text = format!("{secret:?}");
    assert_eq!(text, "InviteSecret(***)");
    assert!(!text.to_lowercase().contains("ab"));
    // Nor does an op that carries it.
    let op = Op::MemberJoin {
        member: member(2),
        device: device(3),
        member_keys: keys(4),
        device_keys: keys(5),
        secret,
    };
    assert!(!format!("{op:?}").contains("171"), "no decimal byte either");
    assert!(format!("{op:?}").contains("InviteSecret(***)"));
}

#[test]
fn words_of_the_small_enums_are_the_plans() {
    assert_eq!(EntitlementState::Active.word(), "active");
    assert_eq!(EntitlementState::Lapsed.word(), "lapsed");
    assert_eq!(EscrowReason::LeaverHandover.word(), "leaver_handover");
    assert_eq!(EscrowReason::LegalRequest.word(), "legal_request");
    assert_eq!(EscrowReason::Other.word(), "other");
    assert!(EntitlementVersion(2) > EntitlementVersion(1));
}
