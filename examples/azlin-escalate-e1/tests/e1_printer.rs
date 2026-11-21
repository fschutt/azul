//! The text a failing run prints for a step IS the Rust that builds it (planning/architecture.md section 12
//! E1: "the failure message prints the seed AND the minimal sequence as a pasteable test body"). Each step
//! below is written once as code (so it compiles with the imports of the printed test) and once as the text
//! the printer must make: a typo in a field name of the printer shows as a difference.

use azlin_escalate_e1::gen::Step;
use azlin_escalate_e1::model::*;
use azlin_seams::{Span, Stamp};

/// Three steps with every kind of value: claims with numbers, signers other than a device, cosignatures,
/// assertions, sets, an app principal with its scopes and expiry, a policy patch, a delegation.
#[test]
fn a_step_prints_as_the_rust_that_builds_it() {
    let invite = Step {
        entry: ModelEntry {
            eid: 7,
            seq: SeqSpec::Off(-1),
            prev: PrevSpec::Garbage(1099511627777),
            epoch: EpochSpec::At(3),
            account: AccountSpec::Own,
            at: Stamp::from_secs(5),
            signer: MSigner::Delegate(Key(1)),
            sig: SigClaim::Valid,
            cosigs: vec![(Dev(11), CosigClaim::OverOtherEntry)],
            assertions: vec![MAssertion {
                dev: Dev(12),
                user_verified: true,
                challenge_ok: false,
                rp_ok: true,
            }],
            corrupt: false,
            op: MOp::MemberInvite {
                member: Mem(104),
                kind: MKind::App,
                role: Role::Guest,
                secret: 55,
                groups: [Grp(1), Grp(2)].into_iter().collect(),
                app: Some(MApp {
                    scopes: [AppId(2)].into_iter().collect(),
                    expires: Some(Stamp::from_secs(99)),
                }),
            },
        },
        now: Stamp::from_secs(100),
    };
    assert_eq!(
        invite.rust(),
        "Step { entry: ModelEntry { eid: 7, seq: SeqSpec::Off(-1), prev: PrevSpec::Garbage(1099511627777), epoch: EpochSpec::At(3), account: AccountSpec::Own, at: Stamp::from_secs(5), signer: MSigner::Delegate(Key(1)), sig: SigClaim::Valid, cosigs: vec![(Dev(11), CosigClaim::OverOtherEntry)], assertions: vec![MAssertion { dev: Dev(12), user_verified: true, challenge_ok: false, rp_ok: true }], corrupt: false, op: MOp::MemberInvite { member: Mem(104), kind: MKind::App, role: Role::Guest, secret: 55, groups: [Grp(1), Grp(2)].into_iter().collect(), app: Some(MApp { scopes: [AppId(2)].into_iter().collect(), expires: Some(Stamp::from_secs(99)) }) } }, now: Stamp::from_secs(100) }"
    );
    let policy = Step {
        entry: ModelEntry {
            eid: 8,
            seq: SeqSpec::Next,
            prev: PrevSpec::Head,
            epoch: EpochSpec::Current,
            account: AccountSpec::Own,
            at: Stamp::from_secs(0),
            signer: MSigner::Device(Dev(21)),
            sig: SigClaim::Invalid,
            cosigs: vec![],
            assertions: vec![],
            corrupt: true,
            op: MOp::PolicySet {
                scope: PScope::Group(Grp(2)),
                patch: MPatch {
                    credential_ttl: Some(Span::secs(3600)),
                    external_sharing: Some(ExtSharing::SameWorkspaceOnly),
                    ..MPatch::default()
                },
            },
        },
        now: Stamp::from_secs(101),
    };
    assert_eq!(
        policy.rust(),
        "Step { entry: ModelEntry { eid: 8, seq: SeqSpec::Next, prev: PrevSpec::Head, epoch: EpochSpec::Current, account: AccountSpec::Own, at: Stamp::from_secs(0), signer: MSigner::Device(Dev(21)), sig: SigClaim::Invalid, cosigs: vec![], assertions: vec![], corrupt: true, op: MOp::PolicySet { scope: PScope::Group(Grp(2)), patch: MPatch { credential_ttl: Some(Span::secs(3600)), external_sharing: Some(ExtSharing::SameWorkspaceOnly), ..MPatch::default() } } }, now: Stamp::from_secs(101) }"
    );
    let delegate = Step {
        entry: ModelEntry {
            eid: 9,
            seq: SeqSpec::Off(1),
            prev: PrevSpec::Stale(1),
            epoch: EpochSpec::Far,
            account: AccountSpec::Other,
            at: Stamp::from_secs(1),
            signer: MSigner::Payments,
            sig: SigClaim::Valid,
            cosigs: vec![],
            assertions: vec![],
            corrupt: false,
            op: MOp::Delegate {
                key: Key(2),
                ops: [OpKind::MemberInvite, OpKind::MemberSuspend]
                    .into_iter()
                    .collect(),
                expires: None,
            },
        },
        now: Stamp::from_secs(102),
    };
    assert_eq!(
        delegate.rust(),
        "Step { entry: ModelEntry { eid: 9, seq: SeqSpec::Off(1), prev: PrevSpec::Stale(1), epoch: EpochSpec::Far, account: AccountSpec::Other, at: Stamp::from_secs(1), signer: MSigner::Payments, sig: SigClaim::Valid, cosigs: vec![], assertions: vec![], corrupt: false, op: MOp::Delegate { key: Key(2), ops: [OpKind::MemberInvite, OpKind::MemberSuspend].into_iter().collect(), expires: None } }, now: Stamp::from_secs(102) }"
    );
}

/// A plain entry (every claim valid, no cosignature, stamp zero) of the device `Dev(1)`, offered at 10.
fn plain(op: MOp) -> Step {
    Step {
        entry: ModelEntry::new(1, MSigner::Device(Dev(1)), op),
        now: Stamp::from_secs(10),
    }
}

/// The text the printer makes for [`plain`] around the text of an op.
fn plain_text(op: &str) -> String {
    format!(
        "Step {{ entry: ModelEntry {{ eid: 1, seq: SeqSpec::Next, prev: PrevSpec::Head, epoch: EpochSpec::Current, account: AccountSpec::Own, at: Stamp::from_secs(0), signer: MSigner::Device(Dev(1)), sig: SigClaim::Valid, cosigs: vec![], assertions: vec![], corrupt: false, op: {op} }}, now: Stamp::from_secs(10) }}"
    )
}

/// Every op of the model (the twenty of the table and the unknown word), and a patch with every field
/// set: the code on the left, the text the printer must make on the right.
#[test]
#[allow(clippy::needless_update)]
fn every_op_prints_as_the_rust_that_builds_it() {
    let every_field = MPatch {
        credential_ttl: Some(Span::secs(60)),
        device_approval: Some(DevApproval::SelfOrQuorum),
        max_devices: Some(5),
        managed_default: Some(true),
        sso_required: Some(false),
        enroll_hold: Some(Span::secs(120)),
        passkey_recovery: Some(true),
        escrow_open_wait: Some(Span::secs(180)),
        audit_level: Some(2),
        admin_quorum: Some(3),
        browser_devices: Some(false),
        external_sharing: Some(ExtSharing::Allowed),
        updates: Some(1),
        padding: Some(4),
        ..MPatch::default()
    };
    let cases: Vec<(MOp, &str)> = vec![
        (
            MOp::Genesis {
                owner: Mem(1),
                device: Dev(2),
            },
            "MOp::Genesis { owner: Mem(1), device: Dev(2) }",
        ),
        (
            MOp::DeviceAdd {
                member: Mem(3),
                device: Dev(4),
            },
            "MOp::DeviceAdd { member: Mem(3), device: Dev(4) }",
        ),
        (
            MOp::DeviceRemove { device: Dev(5) },
            "MOp::DeviceRemove { device: Dev(5) }",
        ),
        (
            MOp::MemberInvite {
                member: Mem(6),
                kind: MKind::Person,
                role: Role::Member,
                secret: 7,
                groups: Default::default(),
                app: None,
            },
            "MOp::MemberInvite { member: Mem(6), kind: MKind::Person, role: Role::Member, secret: 7, groups: [].into_iter().collect(), app: None }",
        ),
        (
            MOp::MemberJoin {
                member: Mem(8),
                device: Dev(9),
                secret: 7,
            },
            "MOp::MemberJoin { member: Mem(8), device: Dev(9), secret: 7 }",
        ),
        (
            MOp::MemberRole {
                member: Mem(8),
                role: Role::GroupAdmin,
                scope: Some(Grp(2)),
            },
            "MOp::MemberRole { member: Mem(8), role: Role::GroupAdmin, scope: Some(Grp(2)) }",
        ),
        (
            MOp::MemberRole {
                member: Mem(8),
                role: Role::Owner,
                scope: None,
            },
            "MOp::MemberRole { member: Mem(8), role: Role::Owner, scope: None }",
        ),
        (
            MOp::MemberSuspend { member: Mem(8) },
            "MOp::MemberSuspend { member: Mem(8) }",
        ),
        (
            MOp::MemberRemove { member: Mem(8) },
            "MOp::MemberRemove { member: Mem(8) }",
        ),
        (
            MOp::MemberManaged {
                member: Mem(8),
                managed: true,
            },
            "MOp::MemberManaged { member: Mem(8), managed: true }",
        ),
        (
            MOp::GroupCreate {
                group: Grp(3),
                parent: Some(Grp(1)),
            },
            "MOp::GroupCreate { group: Grp(3), parent: Some(Grp(1)) }",
        ),
        (
            MOp::GroupCreate {
                group: Grp(4),
                parent: None,
            },
            "MOp::GroupCreate { group: Grp(4), parent: None }",
        ),
        (
            MOp::GroupMember {
                group: Grp(3),
                member: Mem(8),
                present: false,
            },
            "MOp::GroupMember { group: Grp(3), member: Mem(8), present: false }",
        ),
        (
            MOp::GroupMove {
                group: Grp(3),
                new_parent: Grp(1),
            },
            "MOp::GroupMove { group: Grp(3), new_parent: Grp(1) }",
        ),
        (
            MOp::PolicySet {
                scope: PScope::Account,
                patch: every_field,
            },
            "MOp::PolicySet { scope: PScope::Account, patch: MPatch { credential_ttl: Some(Span::secs(60)), device_approval: Some(DevApproval::SelfOrQuorum), max_devices: Some(5), managed_default: Some(true), sso_required: Some(false), enroll_hold: Some(Span::secs(120)), passkey_recovery: Some(true), escrow_open_wait: Some(Span::secs(180)), audit_level: Some(2), admin_quorum: Some(3), browser_devices: Some(false), external_sharing: Some(ExtSharing::Allowed), updates: Some(1), padding: Some(4), ..MPatch::default() } }",
        ),
        (
            MOp::PolicySet {
                scope: PScope::Account,
                patch: MPatch::default(),
            },
            "MOp::PolicySet { scope: PScope::Account, patch: MPatch { ..MPatch::default() } }",
        ),
        (
            MOp::EscrowSet {
                key: 9,
                holders: [Mem(1), Mem(2)].into_iter().collect(),
            },
            "MOp::EscrowSet { key: 9, holders: [Mem(1), Mem(2)].into_iter().collect() }",
        ),
        (
            MOp::AuditKeySet { key: 10 },
            "MOp::AuditKeySet { key: 10 }",
        ),
        (
            MOp::AdminQuorumSet { quorum: 2 },
            "MOp::AdminQuorumSet { quorum: 2 }",
        ),
        (
            MOp::EscrowOpen {
                member: Mem(8),
                reason: Reason::LegalRequest,
            },
            "MOp::EscrowOpen { member: Mem(8), reason: Reason::LegalRequest }",
        ),
        (
            MOp::Delegate {
                key: Key(2),
                ops: [OpKind::MemberSuspend].into_iter().collect(),
                expires: Some(Stamp::from_secs(77)),
            },
            "MOp::Delegate { key: Key(2), ops: [OpKind::MemberSuspend].into_iter().collect(), expires: Some(Stamp::from_secs(77)) }",
        ),
        (
            MOp::Entitlement {
                version: 4,
                active: true,
                quota: 1000,
                seats: 12,
            },
            "MOp::Entitlement { version: 4, active: true, quota: 1000, seats: 12 }",
        ),
        (
            MOp::App {
                name: AppId(3),
                body_len: 99,
            },
            "MOp::App { name: AppId(3), body_len: 99 }",
        ),
        (
            MOp::Unknown { word: 200 },
            "MOp::Unknown { word: 200 }",
        ),
    ];
    // every variant of the enum is in the table (a new variant is a compile error in the printer, and a
    // missing row here is a failure of this test)
    let kinds: std::collections::BTreeSet<Option<OpKind>> =
        cases.iter().map(|(op, _)| op.kind()).collect();
    assert_eq!(
        kinds.len(),
        21,
        "the twenty ops and the unknown word are each in the table"
    );
    for (op, text) in cases {
        assert_eq!(plain(op).rust(), plain_text(text));
    }
}
