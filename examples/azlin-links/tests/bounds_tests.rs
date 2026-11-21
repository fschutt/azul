//! Check 3: the bounds of the account's `link_policy`. The policy forces `preview_only`, the watermark
//! and `app_only`, and refuses the rest: audiences it does not allow, `anyone` without a password,
//! an expiry beyond its longest life, and uploads, edit, comments, open logs and slugs.
//!
//! Implements planning/architecture.md section 5.12 ("Admin bounds") and section 5.10 (`link_policy`).

mod common;

use azlin_links::{
    apply_policy, validate, Audience, Bounded, LinkKind, LinkOptions, OptionName, PolicyViolation,
    Rights, Slug,
};
use azlin_policy::{AudienceClass, AudienceSet, LinkPolicy};
use azlin_seams::{Span, Stamp};
use common::{every_audience, facts_for, opts, refusals_under, DAY, NOW};

/// The policy that bounds nothing, as a starting point.
fn open() -> LinkPolicy {
    LinkPolicy::allow_all()
}

/// Runs `validate` and `apply_policy` over `o`.
fn bound(o: &LinkOptions, policy: &LinkPolicy) -> Result<Bounded, Vec<PolicyViolation>> {
    let validated = validate(o, &facts_for(o)).expect("valid options");
    apply_policy(validated, policy, NOW)
}

/// The stamp `secs` seconds after `NOW`.
fn after(secs: u64) -> Stamp {
    NOW.checked_add(Span::secs(secs)).expect("no overflow")
}

#[test]
fn policy_forces_preview_only_watermark_and_app_only() {
    let o = opts(Audience::SignedIn);
    assert_eq!(o.rights, Rights::Download);
    assert!(!o.watermark && !o.app_only);

    // Each one alone.
    let policy = LinkPolicy {
        force_preview_only: true,
        ..open()
    };
    let bounded = bound(&o, &policy).expect("forced, not refused");
    assert_eq!(bounded.options().rights, Rights::PreviewOnly);
    assert!(!bounded.options().watermark && !bounded.options().app_only);
    assert_eq!(bounded.forced(), [OptionName::Rights]);

    let policy = LinkPolicy {
        force_watermark: true,
        ..open()
    };
    let bounded = bound(&o, &policy).expect("forced, not refused");
    assert!(bounded.options().watermark);
    assert_eq!(bounded.options().rights, Rights::Download);
    assert!(!bounded.options().app_only);
    assert_eq!(bounded.forced(), [OptionName::Watermark]);

    let policy = LinkPolicy {
        force_app_only: true,
        ..open()
    };
    let bounded = bound(&o, &policy).expect("forced, not refused");
    assert!(bounded.options().app_only);
    assert_eq!(bounded.options().rights, Rights::Download);
    assert!(!bounded.options().watermark);
    assert_eq!(bounded.forced(), [OptionName::AppOnly]);

    // All three, in the order of the table.
    let policy = LinkPolicy {
        force_preview_only: true,
        force_watermark: true,
        force_app_only: true,
        ..open()
    };
    let bounded = bound(&o, &policy).expect("forced, not refused");
    assert_eq!(bounded.options().rights, Rights::PreviewOnly);
    assert!(bounded.options().watermark && bounded.options().app_only);
    assert_eq!(
        bounded.forced(),
        [
            OptionName::Rights,
            OptionName::Watermark,
            OptionName::AppOnly
        ]
    );

    // What the creator had set is not "forced": nothing to list.
    let mut set = opts(Audience::SignedIn);
    set.rights = Rights::PreviewOnly;
    set.watermark = true;
    set.app_only = true;
    let bounded = bound(&set, &policy).expect("already as forced");
    assert!(bounded.forced().is_empty());
    assert_eq!(bounded.options(), &set);

    // Without the policy a download stays a download.
    let bounded = bound(&o, &open()).expect("no bounds");
    assert_eq!(bounded.options(), &o);
    assert!(bounded.forced().is_empty());
}

#[test]
fn policy_refuses_each_audience_it_does_not_allow() {
    // One test row for each audience of the table: the policy that allows every audience BUT that
    // one refuses it, and lets the others through.
    assert_eq!(AudienceClass::ALL.len(), 5);
    for class in AudienceClass::ALL {
        let policy = LinkPolicy {
            audiences: AudienceSet::ALL.without(*class),
            ..open()
        };
        for audience in every_audience() {
            let o = opts(audience.clone());
            let result = refusals_under(&o, &policy);
            if audience.class() == *class {
                assert_eq!(
                    result,
                    [PolicyViolation::AudienceNotAllowed(*class)],
                    "{audience:?} under a policy without {class}"
                );
            } else {
                assert!(
                    result.is_empty(),
                    "{audience:?} under a policy without {class}: {result:?}"
                );
            }
        }
    }
    // A policy that allows no audience refuses every link.
    let none = LinkPolicy {
        audiences: AudienceSet::NONE,
        ..open()
    };
    for audience in every_audience() {
        let result = refusals_under(&opts(audience.clone()), &none);
        assert_eq!(
            result,
            [PolicyViolation::AudienceNotAllowed(audience.class())],
            "{audience:?}"
        );
    }
}

#[test]
fn a_workspace_can_switch_public_links_off() {
    // "A workspace can switch public links off for its members": the audiences without `anyone`.
    let policy = LinkPolicy {
        audiences: AudienceSet::ALL.without(AudienceClass::Anyone),
        ..open()
    };
    assert_eq!(
        refusals_under(&opts(Audience::Anyone), &policy),
        [PolicyViolation::AudienceNotAllowed(AudienceClass::Anyone)]
    );
    // A password does not make a public link allowed.
    let mut protected = opts(Audience::Anyone);
    protected.password = true;
    assert_eq!(
        refusals_under(&protected, &policy),
        [PolicyViolation::AudienceNotAllowed(AudienceClass::Anyone)]
    );
    // Sharing with named people and the account itself keeps working.
    for audience in every_audience() {
        if audience.class() != AudienceClass::Anyone {
            assert!(
                refusals_under(&opts(audience.clone()), &policy).is_empty(),
                "{audience:?}"
            );
        }
    }
}

#[test]
fn policy_requires_a_password_for_anyone() {
    let policy = LinkPolicy {
        password_for_anyone: true,
        ..open()
    };
    // `anyone` without a password: refused.
    assert_eq!(
        refusals_under(&opts(Audience::Anyone), &policy),
        [PolicyViolation::PasswordRequiredForAnyone]
    );
    // With a password: through.
    let mut o = opts(Audience::Anyone);
    o.password = true;
    assert!(refusals_under(&o, &policy).is_empty());
    // The rule is about `anyone`: every other audience needs none.
    for audience in every_audience() {
        if audience.class() != AudienceClass::Anyone {
            assert!(
                refusals_under(&opts(audience.clone()), &policy).is_empty(),
                "{audience:?}"
            );
        }
    }
    // Without the policy `anyone` needs no password.
    assert!(refusals_under(&opts(Audience::Anyone), &open()).is_empty());
}

#[test]
fn policy_max_expiry() {
    let month = Span::days(30);
    let policy = LinkPolicy {
        max_expiry: Some(month),
        ..open()
    };
    let latest = NOW.checked_add(month).expect("no overflow");
    let with_expiry = |at: Option<Stamp>| {
        let mut o = opts(Audience::SignedIn);
        o.expires_at = at;
        o
    };
    // A longest life asks for an expiry.
    assert_eq!(
        refusals_under(&with_expiry(None), &policy),
        [PolicyViolation::ExpiryRequired { longest: month }]
    );
    // From the creation: the last moment is allowed, one second more is not.
    assert!(refusals_under(&with_expiry(Some(latest)), &policy).is_empty());
    assert_eq!(
        refusals_under(
            &with_expiry(Some(
                latest.checked_add(Span::secs(1)).expect("no overflow")
            )),
            &policy
        ),
        [PolicyViolation::ExpiryTooLate { latest }]
    );
    assert!(refusals_under(&with_expiry(Some(after(DAY.as_secs()))), &policy).is_empty());
    assert!(refusals_under(&with_expiry(Some(after(1))), &policy).is_empty());
    // The expiry is the link's own: `expires_after_open` and `idle_expiry` do not stand in for it.
    let mut o = with_expiry(None);
    o.expires_after_open = Some(DAY);
    o.idle_expiry = Some(DAY);
    assert_eq!(
        refusals_under(&o, &policy),
        [PolicyViolation::ExpiryRequired { longest: month }]
    );
    // Without a longest life any expiry, and none, is fine.
    assert!(refusals_under(&with_expiry(None), &open()).is_empty());
    assert!(refusals_under(&with_expiry(Some(after(10 * 365 * 86_400))), &open()).is_empty());
}

#[test]
fn policy_forbids_uploads_edit_comments_open_logs_slugs() {
    // One option each, with the parts it needs (`facts_for`), refused under the policy that forbids it
    // and through under the others.
    let slug = Slug::parse("a-name").expect("a slug");
    let cases: Vec<(&str, LinkOptions, PolicyViolation, LinkPolicy)> = vec![
        (
            "uploads",
            {
                let mut o = opts(Audience::SignedIn);
                o.allow_upload = true;
                o
            },
            PolicyViolation::UploadsNotAllowed,
            LinkPolicy {
                uploads: false,
                ..open()
            },
        ),
        (
            "edit",
            {
                let mut o = opts(common::people(&[10]));
                o.edit = true;
                o
            },
            PolicyViolation::EditNotAllowed,
            LinkPolicy {
                edit: false,
                ..open()
            },
        ),
        (
            "comments",
            {
                let mut o = opts(Audience::SignedIn);
                o.comment = true;
                o
            },
            PolicyViolation::CommentsNotAllowed,
            LinkPolicy {
                comments: false,
                ..open()
            },
        ),
        (
            "open logs",
            {
                let mut o = opts(Audience::SignedIn);
                o.open_log = true;
                o
            },
            PolicyViolation::OpenLogsNotAllowed,
            LinkPolicy {
                open_logs: false,
                ..open()
            },
        ),
        (
            "slugs",
            {
                let mut o = opts(common::people(&[10]));
                o.slug = Some(slug);
                o
            },
            PolicyViolation::SlugsNotAllowed,
            LinkPolicy {
                slugs: false,
                ..open()
            },
        ),
    ];
    // The policy that forbids all five.
    let forbids_all = LinkPolicy {
        uploads: false,
        edit: false,
        comments: false,
        open_logs: false,
        slugs: false,
        ..open()
    };
    for (what, options, violation, policy) in &cases {
        assert_eq!(
            refusals_under(options, policy),
            std::slice::from_ref(violation),
            "{what}"
        );
        assert_eq!(
            refusals_under(options, &forbids_all),
            std::slice::from_ref(violation),
            "{what} under the policy that forbids every one"
        );
        assert!(refusals_under(options, &open()).is_empty(), "{what}");
        // The other four allowed: the same options go through.
        for (other, _, _, other_policy) in &cases {
            if other != what {
                assert!(
                    refusals_under(options, other_policy).is_empty(),
                    "{what} under the policy that forbids {other}"
                );
            }
        }
    }
    // Options without any of the five pass the policy that forbids them all.
    assert!(refusals_under(&opts(Audience::SignedIn), &forbids_all).is_empty());
}

#[test]
fn allow_all_bounds_nothing() {
    for audience in every_audience() {
        let mut o = opts(audience.clone());
        o.expires_at = Some(after(10 * 365 * 86_400));
        let bounded = bound(&o, &open()).expect("no bounds");
        assert_eq!(bounded.options(), &o, "{audience:?}");
        assert!(bounded.forced().is_empty());
    }
}

#[test]
fn every_violation_is_reported_together_and_names_its_options() {
    let policy = LinkPolicy {
        audiences: AudienceSet::NONE.with(AudienceClass::SignedIn),
        max_expiry: Some(Span::days(1)),
        password_for_anyone: true,
        uploads: false,
        edit: false,
        comments: false,
        open_logs: false,
        slugs: false,
        ..open()
    };
    // `anyone` with everything the policy forbids (and a slug with the password it asks for).
    let mut o = LinkOptions::new(Audience::Anyone, LinkKind::File);
    o.allow_upload = true;
    o.expires_at = Some(after(3 * 86_400));
    o.slug = Some(Slug::parse("a-name").expect("a slug"));
    o.password = true;
    let violations = refusals_under(&o, &policy);
    assert_eq!(
        violations,
        [
            PolicyViolation::AudienceNotAllowed(AudienceClass::Anyone),
            PolicyViolation::ExpiryTooLate {
                latest: after(86_400)
            },
            PolicyViolation::UploadsNotAllowed,
            PolicyViolation::SlugsNotAllowed,
        ]
    );
    // Each names the options it is about.
    let named: Vec<Vec<OptionName>> = violations.iter().map(|v| v.options()).collect();
    assert_eq!(
        named,
        [
            vec![OptionName::Audience],
            vec![OptionName::ExpiresAt],
            vec![OptionName::AllowUpload],
            vec![OptionName::Slug],
        ]
    );
    // And the rest of them.
    assert_eq!(
        PolicyViolation::PasswordRequiredForAnyone.options(),
        [OptionName::Audience, OptionName::Password]
    );
    assert_eq!(
        PolicyViolation::EditNotAllowed.options(),
        [OptionName::Edit]
    );
    assert_eq!(
        PolicyViolation::CommentsNotAllowed.options(),
        [OptionName::Comment]
    );
    assert_eq!(
        PolicyViolation::OpenLogsNotAllowed.options(),
        [OptionName::OpenLog]
    );
    assert_eq!(
        PolicyViolation::ExpiryRequired { longest: DAY }.options(),
        [OptionName::ExpiresAt]
    );
    assert_eq!(
        PolicyViolation::Forced(OptionName::Watermark).options(),
        [OptionName::Watermark]
    );
    for violation in [
        PolicyViolation::UploadsNotAllowed,
        PolicyViolation::Forced(OptionName::Rights),
        PolicyViolation::ExpiryRequired { longest: DAY },
    ] {
        assert!(!violation.to_string().is_empty());
    }
}

#[test]
fn a_refusal_comes_before_any_forcing() {
    // A policy that forces AND refuses refuses: no `Bounded` with half the options forced.
    let policy = LinkPolicy {
        force_watermark: true,
        audiences: AudienceSet::NONE,
        ..open()
    };
    let o = opts(Audience::SignedIn);
    assert_eq!(
        refusals_under(&o, &policy),
        [PolicyViolation::AudienceNotAllowed(AudienceClass::SignedIn)]
    );
}
