//! The defaults of planning/architecture.md section 5.10 written out by hand, one column for each
//! kind, so that the table is held even where the plan is not at hand (the doc-table test of
//! `tests/doc_table.rs` reads the plan itself). The readings of the cells that are choices are the
//! rows of the drift register of lane W1-S17b.

mod common;

use azlin_policy::{
    AccountKind, AdminQuorum, AudienceSet, AuditLevel, DeviceApproval, ExternalSharing, LinkPolicy,
    ManagedDefault, PaddingPolicy, Policy, RequestPolicy, UpdatePolicy,
};
use azlin_seams::Span;
use common::KINDS;

#[test]
fn personal_defaults_follow_the_table() {
    let p = Policy::defaults(AccountKind::Personal);
    assert_eq!(p.credential_ttl, Span::hours(12));
    assert_eq!(p.device_approval, DeviceApproval::SelfOnly);
    assert_eq!(p.managed_default, ManagedDefault::No);
    assert!(p.passkey_recovery);
    assert_eq!(p.audit_level, AuditLevel::None);
    assert_eq!(p.admin_quorum, AdminQuorum::ONE);
    assert_eq!(p.external_sharing, ExternalSharing::Allowed);
    assert_eq!(p.updates, UpdatePolicy::Auto);
}

#[test]
fn family_defaults_follow_the_table() {
    let p = Policy::defaults(AccountKind::Family);
    assert_eq!(p.credential_ttl, Span::hours(12));
    assert_eq!(p.device_approval, DeviceApproval::SelfOnly);
    assert_eq!(p.managed_default, ManagedDefault::No);
    assert!(p.passkey_recovery);
    assert_eq!(p.audit_level, AuditLevel::None);
    assert_eq!(p.admin_quorum, AdminQuorum::ONE);
    assert_eq!(p.external_sharing, ExternalSharing::Allowed);
    assert_eq!(p.updates, UpdatePolicy::Auto);
}

#[test]
fn team_defaults_follow_the_table() {
    let p = Policy::defaults(AccountKind::Team);
    assert_eq!(p.credential_ttl, Span::hours(12));
    assert_eq!(p.device_approval, DeviceApproval::SelfOrQuorum);
    assert_eq!(p.managed_default, ManagedDefault::No);
    assert!(!p.passkey_recovery);
    assert_eq!(p.audit_level, AuditLevel::Admin);
    // The table gives no admin_quorum for a team: the signer alone (drift register).
    assert_eq!(p.admin_quorum, AdminQuorum::ONE);
    assert_eq!(p.external_sharing, ExternalSharing::Allowed);
    assert_eq!(p.updates, UpdatePolicy::Auto);
}

#[test]
fn business_defaults_follow_the_table() {
    let p = Policy::defaults(AccountKind::Business);
    assert_eq!(p.credential_ttl, Span::hours(1));
    assert_eq!(p.device_approval, DeviceApproval::SelfOrQuorum);
    assert_eq!(p.managed_default, ManagedDefault::Yes);
    assert!(p.managed_default.is_managed());
    assert!(!p.passkey_recovery);
    assert_eq!(p.audit_level, AuditLevel::AccessSummary);
    assert_eq!(p.admin_quorum, AdminQuorum::TWO);
    assert_eq!(p.external_sharing, ExternalSharing::SameWorkspaceOnly);
    assert_eq!(p.updates, UpdatePolicy::ManagedByMdm);
}

#[test]
fn the_rows_with_one_value_are_the_same_for_every_kind() {
    for kind in KINDS {
        let p = Policy::defaults(kind);
        assert_eq!(p.max_devices_per_member, 10, "{kind}");
        assert_eq!(p.enroll_hold, Span::hours(48), "{kind}");
        assert_eq!(p.escrow_open_wait, Span::hours(72), "{kind}");
        assert!(p.browser_devices, "{kind}: allowed in all four");
        // "optional" is a choice: off, with no maximum age, until an admin turns it on.
        assert!(!p.sso_required, "{kind}");
        assert_eq!(p.sso_max_age, None, "{kind}");
        // The table gives no link_policy or request_policy value: nothing is bounded.
        assert_eq!(p.link_policy, LinkPolicy::allow_all(), "{kind}");
        assert_eq!(p.request_policy, RequestPolicy::unbounded(), "{kind}");
        assert_eq!(p.link_policy.audiences, AudienceSet::ALL, "{kind}");
        // Section 5.15 M4: buckets is the default of `padding`.
        assert_eq!(p.padding, PaddingPolicy::Buckets, "{kind}");
        // And a default is a policy that works: nothing is zero that must not be.
        assert!(
            p.credential_ttl > Span::ZERO && p.enroll_hold > Span::ZERO,
            "{kind}"
        );
        assert!(
            p.max_devices_per_member >= 1 && p.admin_quorum.get() >= 1,
            "{kind}"
        );
    }
}

#[test]
fn the_kinds_differ_in_the_rows_of_the_table_that_have_four_values_only() {
    // Two kinds differ only in the rows that the table gives per kind.
    for (a, b) in [
        (AccountKind::Personal, AccountKind::Family),
        (AccountKind::Family, AccountKind::Team),
        (AccountKind::Team, AccountKind::Business),
    ] {
        let (pa, pb) = (Policy::defaults(a), Policy::defaults(b));
        // Neutralise the per-kind rows in a copy of `pb`: what is left must be equal.
        let mut neutral = pb.clone();
        neutral.credential_ttl = pa.credential_ttl;
        neutral.device_approval = pa.device_approval;
        neutral.managed_default = pa.managed_default;
        neutral.passkey_recovery = pa.passkey_recovery;
        neutral.audit_level = pa.audit_level;
        neutral.admin_quorum = pa.admin_quorum;
        neutral.external_sharing = pa.external_sharing;
        neutral.updates = pa.updates;
        assert_eq!(
            neutral, pa,
            "{a} and {b} differ in a row that has one value in the table"
        );
    }
}

#[test]
fn the_business_is_tighter_than_the_team_in_credentials_and_audit() {
    // A reading of the table, not a rule: a business asks for more than a team.
    let team = Policy::defaults(AccountKind::Team);
    let business = Policy::defaults(AccountKind::Business);
    assert!(business.credential_ttl < team.credential_ttl);
    assert!(business.admin_quorum > team.admin_quorum);
    assert_ne!(business.audit_level, team.audit_level);
}
