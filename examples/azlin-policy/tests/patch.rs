//! Check 3: the patch (planning/architecture.md section 5.9 `policy_set`, section 5.10): every field
//! of the policy can be set by a patch, and a patch that sets one field changes that field and
//! nothing else.

mod common;

use azlin_policy::{
    apply, AccountKind, AdminQuorum, AudienceClass, AudienceSet, AuditLevel, DeviceApproval,
    ExternalSharing, Field, ManagedDefault, PaddingPolicy, Policy, PolicyPatch, UpdatePolicy,
};
use azlin_seams::sim::Rng;
use azlin_seams::{ByteCount, Span};
use common::{random_patch, random_policy};

/// One row of the table: the field, a patch that sets it to a value that differs from the business
/// default, and the same change made by hand on a policy.
struct Row {
    field: Field,
    patch: fn(&mut PolicyPatch),
    by_hand: fn(&mut Policy),
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            field: Field::CredentialTtl,
            patch: |p| p.credential_ttl = Some(Span::hours(3)),
            by_hand: |p| p.credential_ttl = Span::hours(3),
        },
        Row {
            field: Field::DeviceApproval,
            patch: |p| p.device_approval = Some(DeviceApproval::SelfOnly),
            by_hand: |p| p.device_approval = DeviceApproval::SelfOnly,
        },
        Row {
            field: Field::MaxDevicesPerMember,
            patch: |p| p.max_devices_per_member = Some(3),
            by_hand: |p| p.max_devices_per_member = 3,
        },
        Row {
            field: Field::ManagedDefault,
            patch: |p| p.managed_default = Some(ManagedDefault::No),
            by_hand: |p| p.managed_default = ManagedDefault::No,
        },
        Row {
            field: Field::SsoRequired,
            patch: |p| p.sso_required = Some(true),
            by_hand: |p| p.sso_required = true,
        },
        Row {
            field: Field::SsoMaxAge,
            patch: |p| p.sso_max_age = Some(Some(Span::hours(8))),
            by_hand: |p| p.sso_max_age = Some(Span::hours(8)),
        },
        Row {
            field: Field::EnrollHold,
            patch: |p| p.enroll_hold = Some(Span::hours(96)),
            by_hand: |p| p.enroll_hold = Span::hours(96),
        },
        Row {
            field: Field::PasskeyRecovery,
            patch: |p| p.passkey_recovery = Some(true),
            by_hand: |p| p.passkey_recovery = true,
        },
        Row {
            field: Field::EscrowOpenWait,
            patch: |p| p.escrow_open_wait = Some(Span::hours(24)),
            by_hand: |p| p.escrow_open_wait = Span::hours(24),
        },
        Row {
            field: Field::AuditLevel,
            patch: |p| p.audit_level = Some(AuditLevel::AccessFull),
            by_hand: |p| p.audit_level = AuditLevel::AccessFull,
        },
        Row {
            field: Field::AdminQuorum,
            patch: |p| p.admin_quorum = Some(AdminQuorum::new(3).expect("3 is a quorum")),
            by_hand: |p| p.admin_quorum = AdminQuorum::new(3).expect("3 is a quorum"),
        },
        Row {
            field: Field::LinkAudiences,
            patch: |p| {
                p.link_policy.audiences = Some(AudienceSet::ALL.without(AudienceClass::Anyone));
            },
            by_hand: |p| p.link_policy.audiences = AudienceSet::ALL.without(AudienceClass::Anyone),
        },
        Row {
            field: Field::LinkMaxExpiry,
            patch: |p| p.link_policy.max_expiry = Some(Some(Span::days(30))),
            by_hand: |p| p.link_policy.max_expiry = Some(Span::days(30)),
        },
        Row {
            field: Field::LinkPasswordForAnyone,
            patch: |p| p.link_policy.password_for_anyone = Some(true),
            by_hand: |p| p.link_policy.password_for_anyone = true,
        },
        Row {
            field: Field::LinkForcePreviewOnly,
            patch: |p| p.link_policy.force_preview_only = Some(true),
            by_hand: |p| p.link_policy.force_preview_only = true,
        },
        Row {
            field: Field::LinkForceWatermark,
            patch: |p| p.link_policy.force_watermark = Some(true),
            by_hand: |p| p.link_policy.force_watermark = true,
        },
        Row {
            field: Field::LinkForceAppOnly,
            patch: |p| p.link_policy.force_app_only = Some(true),
            by_hand: |p| p.link_policy.force_app_only = true,
        },
        Row {
            field: Field::LinkUploads,
            patch: |p| p.link_policy.uploads = Some(false),
            by_hand: |p| p.link_policy.uploads = false,
        },
        Row {
            field: Field::LinkEdit,
            patch: |p| p.link_policy.edit = Some(false),
            by_hand: |p| p.link_policy.edit = false,
        },
        Row {
            field: Field::LinkComments,
            patch: |p| p.link_policy.comments = Some(false),
            by_hand: |p| p.link_policy.comments = false,
        },
        Row {
            field: Field::LinkOpenLogs,
            patch: |p| p.link_policy.open_logs = Some(false),
            by_hand: |p| p.link_policy.open_logs = false,
        },
        Row {
            field: Field::LinkSlugs,
            patch: |p| p.link_policy.slugs = Some(false),
            by_hand: |p| p.link_policy.slugs = false,
        },
        Row {
            field: Field::RequestAllowed,
            patch: |p| p.request_policy.allowed = Some(false),
            by_hand: |p| p.request_policy.allowed = false,
        },
        Row {
            field: Field::RequestMaxExpiry,
            patch: |p| p.request_policy.max_expiry = Some(Some(Span::days(7))),
            by_hand: |p| p.request_policy.max_expiry = Some(Span::days(7)),
        },
        Row {
            field: Field::RequestMaxFiles,
            patch: |p| p.request_policy.max_files = Some(100),
            by_hand: |p| p.request_policy.max_files = 100,
        },
        Row {
            field: Field::RequestMaxBytes,
            patch: |p| p.request_policy.max_bytes = Some(ByteCount::gib(1)),
            by_hand: |p| p.request_policy.max_bytes = ByteCount::gib(1),
        },
        Row {
            field: Field::BrowserDevices,
            patch: |p| p.browser_devices = Some(false),
            by_hand: |p| p.browser_devices = false,
        },
        Row {
            field: Field::ExternalSharing,
            patch: |p| p.external_sharing = Some(ExternalSharing::Allowed),
            by_hand: |p| p.external_sharing = ExternalSharing::Allowed,
        },
        Row {
            field: Field::Updates,
            patch: |p| p.updates = Some(UpdatePolicy::Auto),
            by_hand: |p| p.updates = UpdatePolicy::Auto,
        },
        Row {
            field: Field::Padding,
            patch: |p| p.padding = Some(PaddingPolicy::Full),
            by_hand: |p| p.padding = PaddingPolicy::Full,
        },
    ]
}

#[test]
fn patch_sets_each_field_and_nothing_else() {
    let base = Policy::defaults(AccountKind::Business);
    let table = rows();

    // One row for every field, once, in the order of the table.
    let fields: Vec<Field> = table.iter().map(|row| row.field).collect();
    assert_eq!(
        fields,
        Field::ALL,
        "one row for each field of the table, in its order"
    );
    assert_eq!(Field::ALL.len(), 30);

    for row in &table {
        let mut patch = PolicyPatch::none();
        (row.patch)(&mut patch);
        assert_eq!(
            patch.sets(),
            vec![row.field],
            "{}: the patch sets this field only",
            row.field
        );
        assert!(!patch.is_empty());

        let mut expected = base.clone();
        (row.by_hand)(&mut expected);
        assert_ne!(
            expected, base,
            "{}: the row's value differs from the default",
            row.field
        );

        let applied = apply(&base, &patch);
        assert_eq!(
            applied, expected,
            "{}: the patch changes this field and nothing else",
            row.field
        );
        // The base is not touched, and applying the same patch again changes nothing more.
        assert_eq!(base, Policy::defaults(AccountKind::Business));
        assert_eq!(
            apply(&applied, &patch),
            applied,
            "{}: a patch applied twice",
            row.field
        );
    }
}

#[test]
fn a_patch_that_sets_nothing_changes_nothing() {
    for kind in common::KINDS {
        let base = Policy::defaults(kind);
        let none = PolicyPatch::none();
        assert!(none.is_empty());
        assert_eq!(none.sets(), Vec::<Field>::new());
        assert_eq!(apply(&base, &none), base);
    }
}

#[test]
fn a_patch_can_remove_a_bound() {
    let mut base = Policy::defaults(AccountKind::Business);
    base.sso_max_age = Some(Span::hours(8));
    base.link_policy.max_expiry = Some(Span::days(30));
    base.request_policy.max_expiry = Some(Span::days(7));

    let mut patch = PolicyPatch::none();
    patch.sso_max_age = Some(None);
    patch.link_policy.max_expiry = Some(None);
    patch.request_policy.max_expiry = Some(None);
    let applied = apply(&base, &patch);
    assert_eq!(applied.sso_max_age, None);
    assert_eq!(applied.link_policy.max_expiry, None);
    assert_eq!(applied.request_policy.max_expiry, None);
    assert_eq!(
        patch.sets(),
        vec![
            Field::SsoMaxAge,
            Field::LinkMaxExpiry,
            Field::RequestMaxExpiry
        ]
    );
}

/// Whether `a` and `b` agree on `field`: both are made equal in every other field with the by-hand
/// rows, so they are equal afterwards exactly when they agree on `field`.
fn agree_on(table: &[Row], a: &Policy, b: &Policy, field: Field) -> bool {
    let mut left = a.clone();
    let mut right = b.clone();
    for row in table {
        if row.field != field {
            (row.by_hand)(&mut left);
            (row.by_hand)(&mut right);
        }
    }
    left == right
}

#[test]
fn a_patch_changes_only_the_fields_it_sets_for_500_seeded_patches() {
    let table = rows();
    let mut rng = Rng::new(0x5E70_0001);
    for round in 0..500 {
        let base = random_policy(&mut rng);
        let patch = random_patch(&mut rng);
        let applied = apply(&base, &patch);
        assert_eq!(
            apply(&applied, &patch),
            applied,
            "setting is idempotent (round {round})"
        );
        let sets = patch.sets();
        for field in Field::ALL {
            if !sets.contains(field) {
                assert!(
                    agree_on(&table, &base, &applied, *field),
                    "round {round}: {field} is not set by the patch and still changed"
                );
            }
        }
    }
}
