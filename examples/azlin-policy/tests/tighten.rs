//! Check 4: "groups inherit and may only tighten" (planning/architecture.md section 5.10 and section
//! 7.2). For every field of the policy: a child that loosens the field is refused, naming the field
//! and both values (`child_that_loosens_<field>_is_refused_naming_field_parent_child`); a child that
//! tightens it is accepted (`child_that_tightens_<field>_is_accepted`); an equal child is accepted
//! (`equal_child_is_accepted`). The three fields of the account only (`managed_default`, `updates`,
//! `padding`) are refused when a group changes them in either direction. The order of each field
//! is the proposed one of the brief (drift row X-DELTA-W1S17B-1), written out here value by value.
//! The rules themselves are checked against their laws over small domains.

mod common;

use std::fmt::Debug;

use azlin_policy::tighten::{
    HigherRankIsTighter, LargerIsTighter, OffIsTighter, OnIsTighter, SameAsParent,
    SmallerIsTighter, SmallerLimitIsTighter, SubsetIsTighter,
};
use azlin_policy::{
    check_tightens, clamp, AccountKind, AdminQuorum, AudienceClass, AudienceSet, AuditLevel,
    DeviceApproval, ExternalSharing, Field, Loosened, ManagedDefault, PaddingPolicy, Policy, Rank,
    Tighter, UpdatePolicy,
};
use azlin_seams::sim::Rng;
use azlin_seams::{ByteCount, Span};
use common::{random_policy, KINDS};

/// Any policy: the fields under test are set by the tests.
fn base() -> Policy {
    Policy::defaults(AccountKind::Team)
}

/// The parent holds `tight` and the child `loose`: refused, with exactly this one entry; the clamp
/// gives back the parent.
fn loosening_is_refused<V: Copy>(
    field: Field,
    set: impl Fn(&mut Policy, V),
    tight: V,
    loose: V,
    tight_text: &str,
    loose_text: &str,
) {
    let mut parent = base();
    set(&mut parent, tight);
    let mut child = parent.clone();
    set(&mut child, loose);
    let refused = check_tightens(&parent, &child).expect_err("a looser child is refused");
    assert_eq!(
        refused,
        vec![Loosened {
            field,
            parent: tight_text.to_string(),
            child: loose_text.to_string()
        }],
        "{field}: the refusal names the field, the parent's and the child's value"
    );
    let text = refused[0].to_string();
    assert!(
        text.contains(field.name()) && text.contains(tight_text) && text.contains(loose_text),
        "{text}"
    );
    assert_eq!(
        clamp(&parent, &child),
        parent,
        "{field}: the clamp gives the parent's value back"
    );
}

/// The parent holds `loose` and the child `tight`: accepted, and the clamp keeps the child.
fn tightening_is_accepted<V: Copy>(field: Field, set: impl Fn(&mut Policy, V), tight: V, loose: V) {
    let mut parent = base();
    set(&mut parent, loose);
    let mut child = parent.clone();
    set(&mut child, tight);
    assert_eq!(
        check_tightens(&parent, &child),
        Ok(()),
        "{field}: a tighter child is accepted"
    );
    assert_eq!(
        clamp(&parent, &child),
        child,
        "{field}: the clamp keeps the tighter child"
    );
}

/// A field of the account only: a group that changes it, either way, is refused.
fn change_is_refused<V: Copy>(
    field: Field,
    set: impl Fn(&mut Policy, V),
    from: V,
    to: V,
    from_text: &str,
    to_text: &str,
) {
    let mut parent = base();
    set(&mut parent, from);
    let mut child = parent.clone();
    set(&mut child, to);
    let refused = check_tightens(&parent, &child).expect_err("a group may not change this field");
    assert_eq!(
        refused,
        vec![Loosened {
            field,
            parent: from_text.to_string(),
            child: to_text.to_string()
        }]
    );
    assert_eq!(
        clamp(&parent, &child),
        parent,
        "{field}: the clamp keeps the parent's value"
    );
}

/// The two tests of a field that a group may tighten.
macro_rules! tightenable {
    ($loosen:ident, $tighten:ident, $field:expr, $set:expr,
     $tight:expr => $tight_text:expr, $loose:expr => $loose_text:expr) => {
        #[test]
        fn $loosen() {
            loosening_is_refused($field, $set, $tight, $loose, $tight_text, $loose_text);
        }

        #[test]
        fn $tighten() {
            tightening_is_accepted($field, $set, $tight, $loose);
        }
    };
}

tightenable!(
    child_that_loosens_credential_ttl_is_refused_naming_field_parent_child,
    child_that_tightens_credential_ttl_is_accepted,
    Field::CredentialTtl,
    |p: &mut Policy, v| p.credential_ttl = v,
    Span::hours(1) => "3600 s",
    Span::hours(12) => "43200 s"
);
tightenable!(
    child_that_loosens_device_approval_is_refused_naming_field_parent_child,
    child_that_tightens_device_approval_is_accepted,
    Field::DeviceApproval,
    |p: &mut Policy, v| p.device_approval = v,
    DeviceApproval::SelfOnly => "self",
    DeviceApproval::SelfOrQuorum => "self_or_quorum"
);
tightenable!(
    child_that_loosens_max_devices_per_member_is_refused_naming_field_parent_child,
    child_that_tightens_max_devices_per_member_is_accepted,
    Field::MaxDevicesPerMember,
    |p: &mut Policy, v| p.max_devices_per_member = v,
    5u8 => "5",
    10u8 => "10"
);
tightenable!(
    child_that_loosens_sso_required_is_refused_naming_field_parent_child,
    child_that_tightens_sso_required_is_accepted,
    Field::SsoRequired,
    |p: &mut Policy, v| p.sso_required = v,
    true => "on",
    false => "off"
);
tightenable!(
    child_that_loosens_sso_max_age_is_refused_naming_field_parent_child,
    child_that_tightens_sso_max_age_is_accepted,
    Field::SsoMaxAge,
    |p: &mut Policy, v| p.sso_max_age = v,
    Some(Span::hours(1)) => "3600 s",
    None => "none"
);
tightenable!(
    child_that_loosens_enroll_hold_is_refused_naming_field_parent_child,
    child_that_tightens_enroll_hold_is_accepted,
    Field::EnrollHold,
    |p: &mut Policy, v| p.enroll_hold = v,
    Span::hours(96) => "345600 s",
    Span::hours(48) => "172800 s"
);
tightenable!(
    child_that_loosens_passkey_recovery_is_refused_naming_field_parent_child,
    child_that_tightens_passkey_recovery_is_accepted,
    Field::PasskeyRecovery,
    |p: &mut Policy, v| p.passkey_recovery = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_escrow_open_wait_is_refused_naming_field_parent_child,
    child_that_tightens_escrow_open_wait_is_accepted,
    Field::EscrowOpenWait,
    |p: &mut Policy, v| p.escrow_open_wait = v,
    Span::hours(96) => "345600 s",
    Span::hours(72) => "259200 s"
);
tightenable!(
    child_that_loosens_audit_level_is_refused_naming_field_parent_child,
    child_that_tightens_audit_level_is_accepted,
    Field::AuditLevel,
    |p: &mut Policy, v| p.audit_level = v,
    AuditLevel::AccessFull => "access_full",
    AuditLevel::Admin => "admin"
);
tightenable!(
    child_that_loosens_admin_quorum_is_refused_naming_field_parent_child,
    child_that_tightens_admin_quorum_is_accepted,
    Field::AdminQuorum,
    |p: &mut Policy, v| p.admin_quorum = v,
    AdminQuorum::new(3).expect("3") => "3",
    AdminQuorum::new(2).expect("2") => "2"
);
tightenable!(
    child_that_loosens_link_audiences_is_refused_naming_field_parent_child,
    child_that_tightens_link_audiences_is_accepted,
    Field::LinkAudiences,
    |p: &mut Policy, v| p.link_policy.audiences = v,
    AudienceSet::NONE.with(AudienceClass::SignedIn).with(AudienceClass::Members)
        => "signed_in,members",
    AudienceSet::NONE
        .with(AudienceClass::SignedIn)
        .with(AudienceClass::Members)
        .with(AudienceClass::People)
        => "signed_in,members,people"
);
tightenable!(
    child_that_loosens_link_max_expiry_is_refused_naming_field_parent_child,
    child_that_tightens_link_max_expiry_is_accepted,
    Field::LinkMaxExpiry,
    |p: &mut Policy, v| p.link_policy.max_expiry = v,
    Some(Span::days(30)) => "2592000 s",
    None => "none"
);
tightenable!(
    child_that_loosens_link_password_for_anyone_is_refused_naming_field_parent_child,
    child_that_tightens_link_password_for_anyone_is_accepted,
    Field::LinkPasswordForAnyone,
    |p: &mut Policy, v| p.link_policy.password_for_anyone = v,
    true => "on",
    false => "off"
);
tightenable!(
    child_that_loosens_link_force_preview_only_is_refused_naming_field_parent_child,
    child_that_tightens_link_force_preview_only_is_accepted,
    Field::LinkForcePreviewOnly,
    |p: &mut Policy, v| p.link_policy.force_preview_only = v,
    true => "on",
    false => "off"
);
tightenable!(
    child_that_loosens_link_force_watermark_is_refused_naming_field_parent_child,
    child_that_tightens_link_force_watermark_is_accepted,
    Field::LinkForceWatermark,
    |p: &mut Policy, v| p.link_policy.force_watermark = v,
    true => "on",
    false => "off"
);
tightenable!(
    child_that_loosens_link_force_app_only_is_refused_naming_field_parent_child,
    child_that_tightens_link_force_app_only_is_accepted,
    Field::LinkForceAppOnly,
    |p: &mut Policy, v| p.link_policy.force_app_only = v,
    true => "on",
    false => "off"
);
tightenable!(
    child_that_loosens_link_uploads_is_refused_naming_field_parent_child,
    child_that_tightens_link_uploads_is_accepted,
    Field::LinkUploads,
    |p: &mut Policy, v| p.link_policy.uploads = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_link_edit_is_refused_naming_field_parent_child,
    child_that_tightens_link_edit_is_accepted,
    Field::LinkEdit,
    |p: &mut Policy, v| p.link_policy.edit = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_link_comments_is_refused_naming_field_parent_child,
    child_that_tightens_link_comments_is_accepted,
    Field::LinkComments,
    |p: &mut Policy, v| p.link_policy.comments = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_link_open_logs_is_refused_naming_field_parent_child,
    child_that_tightens_link_open_logs_is_accepted,
    Field::LinkOpenLogs,
    |p: &mut Policy, v| p.link_policy.open_logs = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_link_slugs_is_refused_naming_field_parent_child,
    child_that_tightens_link_slugs_is_accepted,
    Field::LinkSlugs,
    |p: &mut Policy, v| p.link_policy.slugs = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_request_allowed_is_refused_naming_field_parent_child,
    child_that_tightens_request_allowed_is_accepted,
    Field::RequestAllowed,
    |p: &mut Policy, v| p.request_policy.allowed = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_request_max_expiry_is_refused_naming_field_parent_child,
    child_that_tightens_request_max_expiry_is_accepted,
    Field::RequestMaxExpiry,
    |p: &mut Policy, v| p.request_policy.max_expiry = v,
    Some(Span::days(7)) => "604800 s",
    None => "none"
);
tightenable!(
    child_that_loosens_request_max_files_is_refused_naming_field_parent_child,
    child_that_tightens_request_max_files_is_accepted,
    Field::RequestMaxFiles,
    |p: &mut Policy, v| p.request_policy.max_files = v,
    10u32 => "10",
    1_000u32 => "1000"
);
tightenable!(
    child_that_loosens_request_max_bytes_is_refused_naming_field_parent_child,
    child_that_tightens_request_max_bytes_is_accepted,
    Field::RequestMaxBytes,
    |p: &mut Policy, v| p.request_policy.max_bytes = v,
    ByteCount::mib(1) => "1048576 B",
    ByteCount::gib(1) => "1073741824 B"
);
tightenable!(
    child_that_loosens_browser_devices_is_refused_naming_field_parent_child,
    child_that_tightens_browser_devices_is_accepted,
    Field::BrowserDevices,
    |p: &mut Policy, v| p.browser_devices = v,
    false => "off",
    true => "on"
);
tightenable!(
    child_that_loosens_external_sharing_is_refused_naming_field_parent_child,
    child_that_tightens_external_sharing_is_accepted,
    Field::ExternalSharing,
    |p: &mut Policy, v| p.external_sharing = v,
    ExternalSharing::SameWorkspaceOnly => "same_workspace_only",
    ExternalSharing::Allowed => "allowed"
);

/// The two tests of a field of the account only.
macro_rules! account_only {
    ($loosen:ident, $tighten:ident, $field:expr, $set:expr,
     $from:expr => $from_text:expr, $to:expr => $to_text:expr) => {
        #[test]
        fn $loosen() {
            change_is_refused($field, $set, $from, $to, $from_text, $to_text);
        }

        #[test]
        fn $tighten() {
            change_is_refused($field, $set, $to, $from, $to_text, $from_text);
        }
    };
}

account_only!(
    child_that_loosens_managed_default_is_refused_naming_field_parent_child,
    child_that_tightens_managed_default_is_refused_a_group_may_not_change_it,
    Field::ManagedDefault,
    |p: &mut Policy, v| p.managed_default = v,
    ManagedDefault::No => "no",
    ManagedDefault::Yes => "yes"
);
account_only!(
    child_that_loosens_updates_is_refused_naming_field_parent_child,
    child_that_tightens_updates_is_refused_a_group_may_not_change_it,
    Field::Updates,
    |p: &mut Policy, v| p.updates = v,
    UpdatePolicy::Auto => "auto",
    UpdatePolicy::ManagedByMdm => "managed_by_mdm"
);
account_only!(
    child_that_loosens_padding_is_refused_naming_field_parent_child,
    child_that_tightens_padding_is_refused_a_group_may_not_change_it,
    Field::Padding,
    |p: &mut Policy, v| p.padding = v,
    PaddingPolicy::Buckets => "buckets",
    PaddingPolicy::Full => "full"
);

#[test]
fn equal_child_is_accepted() {
    for kind in KINDS {
        let policy = Policy::defaults(kind);
        assert_eq!(check_tightens(&policy, &policy.clone()), Ok(()), "{kind}");
        assert_eq!(clamp(&policy, &policy), policy);
    }
    let mut rng = Rng::new(0x7167_0001);
    for _ in 0..500 {
        let policy = random_policy(&mut rng);
        assert_eq!(check_tightens(&policy, &policy.clone()), Ok(()));
        assert_eq!(clamp(&policy, &policy), policy);
    }
}

#[test]
fn every_loosened_field_is_listed_in_the_order_of_the_table() {
    let mut parent = base();
    parent.credential_ttl = Span::hours(1);
    parent.link_policy.uploads = false;
    parent.padding = PaddingPolicy::Buckets;
    parent.request_policy.max_files = 10;
    let mut child = parent.clone();
    child.request_policy.max_files = 11; // looser
    child.padding = PaddingPolicy::Full; // changed
    child.link_policy.uploads = true; // looser
    child.credential_ttl = Span::hours(2); // looser
    child.enroll_hold = Span::hours(96); // TIGHTER: not listed
    let refused = check_tightens(&parent, &child).expect_err("loosened");
    let fields: Vec<Field> = refused.iter().map(|l| l.field).collect();
    assert_eq!(
        fields,
        vec![
            Field::CredentialTtl,
            Field::LinkUploads,
            Field::RequestMaxFiles,
            Field::Padding
        ]
    );
}

#[test]
fn a_child_that_tightens_everything_is_accepted_and_the_clamp_keeps_it() {
    // The tightest value of every field, against the loosest parent.
    let mut loose = base();
    loose.credential_ttl = Span::hours(12);
    loose.device_approval = DeviceApproval::SelfOrQuorum;
    loose.max_devices_per_member = 10;
    loose.sso_required = false;
    loose.sso_max_age = None;
    loose.enroll_hold = Span::hours(48);
    loose.passkey_recovery = true;
    loose.escrow_open_wait = Span::hours(72);
    loose.audit_level = AuditLevel::None;
    loose.admin_quorum = AdminQuorum::ONE;
    loose.link_policy = azlin_policy::LinkPolicy::allow_all();
    loose.request_policy = azlin_policy::RequestPolicy::unbounded();
    loose.browser_devices = true;
    loose.external_sharing = ExternalSharing::Allowed;

    let mut tight = loose.clone();
    tight.credential_ttl = Span::minutes(15);
    tight.device_approval = DeviceApproval::SelfOnly;
    tight.max_devices_per_member = 1;
    tight.sso_required = true;
    tight.sso_max_age = Some(Span::hours(1));
    tight.enroll_hold = Span::hours(96);
    tight.passkey_recovery = false;
    tight.escrow_open_wait = Span::hours(96);
    tight.audit_level = AuditLevel::AccessFull;
    tight.admin_quorum = AdminQuorum::new(3).expect("3");
    tight.link_policy.audiences = AudienceSet::NONE;
    tight.link_policy.max_expiry = Some(Span::days(1));
    tight.link_policy.password_for_anyone = true;
    tight.link_policy.force_preview_only = true;
    tight.link_policy.force_watermark = true;
    tight.link_policy.force_app_only = true;
    tight.link_policy.uploads = false;
    tight.link_policy.edit = false;
    tight.link_policy.comments = false;
    tight.link_policy.open_logs = false;
    tight.link_policy.slugs = false;
    tight.request_policy.allowed = false;
    tight.request_policy.max_expiry = Some(Span::days(1));
    tight.request_policy.max_files = 1;
    tight.request_policy.max_bytes = ByteCount::kib(1);
    tight.browser_devices = false;
    tight.external_sharing = ExternalSharing::SameWorkspaceOnly;

    assert_eq!(check_tightens(&loose, &tight), Ok(()));
    assert_eq!(clamp(&loose, &tight), tight);
    // And the other way round: every one of the 27 tightenable fields is refused, once.
    let refused = check_tightens(&tight, &loose).expect_err("everything loosened");
    let fields: Vec<Field> = refused.iter().map(|l| l.field).collect();
    let account_only = [Field::ManagedDefault, Field::Updates, Field::Padding];
    let expected: Vec<Field> = Field::ALL
        .iter()
        .copied()
        .filter(|f| !account_only.contains(f))
        .collect();
    assert_eq!(expected.len(), 27);
    assert_eq!(fields, expected);
    assert_eq!(clamp(&tight, &loose), tight);
}

// ---------------------------------------------------------------------------------------------
// The laws of the rules, over small domains.
// ---------------------------------------------------------------------------------------------

/// What every rule satisfies: tighter-or-equal is reflexive and transitive and is what `tighter`
/// computes, the tighter of two values is never looser than the parent, and (for every rule but
/// `SameAsParent`, which always answers with the parent) the tighter of two values is commutative,
/// associative, idempotent and never looser than the child either.
fn laws<R, T>(pool: &[T], symmetric: bool)
where
    R: Tighter<T>,
    T: Clone + PartialEq + Debug,
{
    for a in pool {
        assert!(R::at_least_as_tight(a, a), "reflexive: {a:?}");
        assert_eq!(R::tighter(a, a), *a, "idempotent: {a:?}");
        for b in pool {
            let meet = R::tighter(a, b);
            assert_eq!(
                R::at_least_as_tight(a, b),
                meet == *b,
                "the check and the clamp read one rule: parent {a:?}, child {b:?}"
            );
            assert!(
                R::at_least_as_tight(a, &meet),
                "never looser than the parent: {a:?} {b:?}"
            );
            if symmetric {
                assert_eq!(meet, R::tighter(b, a), "commutative: {a:?} {b:?}");
                assert!(
                    R::at_least_as_tight(b, &meet),
                    "never looser than the child: {a:?} {b:?}"
                );
            }
            for c in pool {
                if R::at_least_as_tight(a, b) && R::at_least_as_tight(b, c) {
                    assert!(R::at_least_as_tight(a, c), "transitive: {a:?} {b:?} {c:?}");
                }
                if symmetric {
                    assert_eq!(
                        R::tighter(&R::tighter(a, b), c),
                        R::tighter(a, &R::tighter(b, c)),
                        "associative: {a:?} {b:?} {c:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn smaller_and_larger_are_ordered_rules() {
    let spans = [Span::ZERO, Span::hours(1), Span::hours(12), Span::hours(48)];
    laws::<SmallerIsTighter, Span>(&spans, true);
    laws::<LargerIsTighter, Span>(&spans, true);
    laws::<SmallerIsTighter, u8>(&[0, 1, 10, 255], true);
    laws::<SmallerIsTighter, u32>(&[0, 1, 1000, u32::MAX], true);
    laws::<SmallerIsTighter, ByteCount>(
        &[ByteCount::ZERO, ByteCount::kib(1), ByteCount::MAX],
        true,
    );
    let quorums: Vec<AdminQuorum> = (1..=8)
        .map(|n| AdminQuorum::new(n).expect("1 to 8"))
        .collect();
    laws::<LargerIsTighter, AdminQuorum>(&quorums, true);

    // The directions, by hand.
    assert!(SmallerIsTighter::at_least_as_tight(&10u8, &5u8));
    assert!(!SmallerIsTighter::at_least_as_tight(&5u8, &10u8));
    assert!(LargerIsTighter::at_least_as_tight(&5u8, &10u8));
    assert!(!LargerIsTighter::at_least_as_tight(&10u8, &5u8));
    assert_eq!(SmallerIsTighter::tighter(&10u8, &5u8), 5);
    assert_eq!(LargerIsTighter::tighter(&10u8, &5u8), 10);
}

#[test]
fn a_limit_is_tighter_than_no_limit_and_a_smaller_limit_than_a_larger() {
    let pool = [
        None,
        Some(Span::ZERO),
        Some(Span::hours(1)),
        Some(Span::hours(12)),
    ];
    laws::<SmallerLimitIsTighter, Option<Span>>(&pool, true);
    // No limit is the loosest.
    assert!(SmallerLimitIsTighter::at_least_as_tight(
        &None::<Span>,
        &None::<Span>
    ));
    assert!(SmallerLimitIsTighter::at_least_as_tight(
        &None,
        &Some(Span::hours(1))
    ));
    assert!(!SmallerLimitIsTighter::at_least_as_tight(
        &Some(Span::hours(1)),
        &None
    ));
    assert!(SmallerLimitIsTighter::at_least_as_tight(
        &Some(Span::hours(12)),
        &Some(Span::hours(1))
    ));
    assert!(!SmallerLimitIsTighter::at_least_as_tight(
        &Some(Span::hours(1)),
        &Some(Span::hours(12))
    ));
    assert_eq!(
        SmallerLimitIsTighter::tighter(&None, &Some(Span::hours(1))),
        Some(Span::hours(1))
    );
    assert_eq!(
        SmallerLimitIsTighter::tighter(&Some(Span::hours(1)), &None),
        Some(Span::hours(1))
    );
    assert_eq!(
        SmallerLimitIsTighter::tighter(&Some(Span::hours(12)), &Some(Span::hours(1))),
        Some(Span::hours(1))
    );
}

#[test]
fn flags_are_restrictions_or_permissions() {
    laws::<OnIsTighter, bool>(&[false, true], true);
    laws::<OffIsTighter, bool>(&[false, true], true);
    // A restriction: on is tighter.
    assert!(OnIsTighter::at_least_as_tight(&false, &true));
    assert!(!OnIsTighter::at_least_as_tight(&true, &false));
    assert!(OnIsTighter::tighter(&false, &true));
    // A permission: off is tighter.
    assert!(OffIsTighter::at_least_as_tight(&true, &false));
    assert!(!OffIsTighter::at_least_as_tight(&false, &true));
    assert!(!OffIsTighter::tighter(&true, &false));
}

#[test]
fn ranks_follow_the_words_of_the_plan() {
    laws::<HigherRankIsTighter, DeviceApproval>(DeviceApproval::ALL, true);
    laws::<HigherRankIsTighter, AuditLevel>(AuditLevel::ALL, true);
    laws::<HigherRankIsTighter, ExternalSharing>(ExternalSharing::ALL, true);
    // audit_level: none < admin < access_summary < access_full.
    let ranks: Vec<u8> = AuditLevel::ALL.iter().map(Rank::tightness).collect();
    assert_eq!(ranks, vec![0, 1, 2, 3]);
    assert!(DeviceApproval::SelfOnly.tightness() > DeviceApproval::SelfOrQuorum.tightness());
    assert!(ExternalSharing::SameWorkspaceOnly.tightness() > ExternalSharing::Allowed.tightness());
}

#[test]
fn a_subset_of_the_audiences_is_tighter() {
    let sets: Vec<AudienceSet> = (0u8..32)
        .map(|bits| {
            let mut set = AudienceSet::NONE;
            for (i, class) in AudienceClass::ALL.iter().enumerate() {
                if bits & (1 << i) != 0 {
                    set = set.with(*class);
                }
            }
            set
        })
        .collect();
    assert_eq!(sets.len(), 32);
    laws::<SubsetIsTighter, AudienceSet>(&sets, true);
    let all = AudienceSet::ALL;
    let no_public = all.without(AudienceClass::Anyone);
    assert!(SubsetIsTighter::at_least_as_tight(&all, &no_public));
    assert!(!SubsetIsTighter::at_least_as_tight(&no_public, &all));
    assert_eq!(SubsetIsTighter::tighter(&all, &no_public), no_public);
    // Two sets that are not ordered: the tighter of them is what both allow.
    let a = AudienceSet::NONE
        .with(AudienceClass::Anyone)
        .with(AudienceClass::People);
    let b = AudienceSet::NONE
        .with(AudienceClass::People)
        .with(AudienceClass::Members);
    assert!(!SubsetIsTighter::at_least_as_tight(&a, &b));
    assert!(!SubsetIsTighter::at_least_as_tight(&b, &a));
    assert_eq!(
        SubsetIsTighter::tighter(&a, &b),
        AudienceSet::NONE.with(AudienceClass::People)
    );
}

#[test]
fn a_field_of_the_account_only_has_one_acceptable_child() {
    laws::<SameAsParent, ManagedDefault>(ManagedDefault::ALL, false);
    laws::<SameAsParent, UpdatePolicy>(UpdatePolicy::ALL, false);
    laws::<SameAsParent, PaddingPolicy>(PaddingPolicy::ALL, false);
    assert!(SameAsParent::at_least_as_tight(
        &UpdatePolicy::Auto,
        &UpdatePolicy::Auto
    ));
    assert!(!SameAsParent::at_least_as_tight(
        &UpdatePolicy::Auto,
        &UpdatePolicy::Notify
    ));
    assert_eq!(
        SameAsParent::tighter(&UpdatePolicy::Auto, &UpdatePolicy::Notify),
        UpdatePolicy::Auto
    );
}

#[test]
fn the_clamp_is_never_looser_than_the_parent_for_2000_seeded_pairs() {
    let mut rng = Rng::new(0x7167_0002);
    for round in 0..2000 {
        let parent = random_policy(&mut rng);
        let child = random_policy(&mut rng);
        let clamped = clamp(&parent, &child);
        assert_eq!(
            check_tightens(&parent, &clamped),
            Ok(()),
            "round {round}: the clamp is no looser than the parent"
        );
        // What the check accepts, the clamp leaves as it is; what it refuses, the clamp changes.
        match check_tightens(&parent, &child) {
            Ok(()) => assert_eq!(
                clamped, child,
                "round {round}: an accepted child is not changed"
            ),
            Err(_) => assert_ne!(clamped, child, "round {round}: a refused child is changed"),
        }
        // Clamping twice is clamping once.
        assert_eq!(clamp(&parent, &clamped), clamped);
    }
}
