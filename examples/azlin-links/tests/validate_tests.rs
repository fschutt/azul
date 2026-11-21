//! Check 2: the rules between options, one `#[test]` each (the slug's shape is in `id_tests.rs`).
//! Every rule is a variant of `OptionError`; `validate` collects every error in the order of the
//! rules; every error names the option it is about.
//!
//! Implements planning/architecture.md section 5.12 (the options table: slug, edit, comment, open_log,
//! allow_upload, audience, kind, subfolders, the dates, the counts, title and message).

mod common;

use azlin_links::{
    validate, Audience, Facts, LinkKind, LinkOptions, OptionError, OptionName, Sealed, Slug,
    MAX_EXPIRES_AFTER_OPEN, MAX_GRANTS, MAX_GROUPS, MAX_IDLE_EXPIRY, MAX_PEOPLE, MAX_WRAPS,
    MESSAGE_MAX_LEN, TITLE_MAX_LEN,
};
use azlin_seams::{ByteCount, Span, Stamp};
use common::{
    every_audience, facts_for, groups, link_id, opts, parts_for, people, plain_facts, sealed, DAY,
    NOW,
};

/// Panics unless `o` is valid under `f`.
fn assert_valid(o: &LinkOptions, f: &Facts, why: &str) {
    if let Err(found) = validate(o, f) {
        panic!("{why}: expected valid options, got {found:?}");
    }
}

/// Panics unless `validate` refuses `o` under `f` with exactly `expected`, in that order.
fn assert_errors(o: &LinkOptions, f: &Facts, expected: &[OptionError], why: &str) {
    match validate(o, f) {
        Ok(_) => panic!("{why}: expected {expected:?}, got Ok"),
        Err(found) => assert_eq!(found, expected, "{why}"),
    }
}

/// The stamp `days` days after `NOW`.
fn later(days: u32) -> Stamp {
    NOW.checked_add(Span::days(days)).expect("no overflow")
}

#[test]
fn slug_needs_a_password_or_a_named_audience() {
    let slug = Slug::parse("quarterly-report").expect("a slug");
    for audience in every_audience() {
        let mut o = opts(audience.clone());
        o.slug = Some(slug.clone());
        let f = facts_for(&o);
        // Without a password only a named audience (`people`) may have a slug: a name can be guessed.
        if audience.is_named() {
            assert_valid(&o, &f, &format!("{audience:?} without a password"));
        } else {
            assert_errors(
                &o,
                &f,
                &[OptionError::SlugNeedsPasswordOrNamedAudience],
                &format!("{audience:?} without a password"),
            );
        }
        // With a password any audience may.
        o.password = true;
        assert_valid(&o, &facts_for(&o), &format!("{audience:?} with a password"));
    }
    // No slug is no problem for any audience.
    for audience in every_audience() {
        let o = opts(audience.clone());
        assert_valid(&o, &facts_for(&o), &format!("{audience:?} without a slug"));
    }
}

#[test]
fn edit_needs_a_named_audience() {
    for audience in every_audience() {
        let mut o = opts(audience.clone());
        o.edit = true;
        let f = facts_for(&o);
        if audience.is_named() {
            assert_valid(&o, &f, &format!("{audience:?}"));
        } else {
            assert_errors(
                &o,
                &f,
                &[OptionError::EditNeedsNamedAudience],
                &format!("{audience:?}"),
            );
        }
    }
}

#[test]
fn comment_needs_a_signed_in_audience() {
    for audience in every_audience() {
        let mut o = opts(audience.clone());
        o.comment = true;
        // `facts_for` gives the comment log a link with comments needs.
        let f = facts_for(&o);
        assert!(f.comment_log);
        if audience.is_signed_in() {
            assert_valid(&o, &f, &format!("{audience:?}"));
        } else {
            assert_errors(
                &o,
                &f,
                &[OptionError::CommentNeedsSignedInAudience],
                &format!("{audience:?}"),
            );
        }
        // Comments go to the link's comment log: without it the option is refused.
        let no_log = Facts {
            comment_log: false,
            ..f
        };
        let mut expected = vec![OptionError::CommentNeedsLog];
        if !audience.is_signed_in() {
            expected.insert(0, OptionError::CommentNeedsSignedInAudience);
        }
        assert_errors(
            &o,
            &no_log,
            &expected,
            &format!("{audience:?} without a log"),
        );
    }
}

#[test]
fn open_log_needs_a_signed_in_audience_and_is_off_by_default() {
    for audience in every_audience() {
        // Off by default, for every audience and every kind.
        for kind in LinkKind::ALL {
            assert!(!LinkOptions::new(audience.clone(), *kind).open_log);
        }
        let mut o = opts(audience.clone());
        o.open_log = true;
        let f = facts_for(&o);
        if audience.is_signed_in() {
            assert_valid(&o, &f, &format!("{audience:?}"));
        } else {
            assert_errors(
                &o,
                &f,
                &[OptionError::OpenLogNeedsSignedInAudience],
                &format!("{audience:?}"),
            );
        }
    }
}

#[test]
fn allow_upload_needs_the_paired_request() {
    for audience in every_audience() {
        let mut o = opts(audience.clone());
        o.allow_upload = true;
        let with = facts_for(&o);
        assert!(with.paired_request);
        assert_valid(&o, &with, &format!("{audience:?} with the paired request"));
        let without = Facts {
            paired_request: false,
            ..with
        };
        let mut expected = vec![OptionError::UploadNeedsPairedRequest];
        if matches!(audience, Audience::Approval) {
            // An approval link has no door without it either.
            expected.insert(0, OptionError::ApprovalNeedsPairedRequest);
        }
        assert_errors(&o, &without, &expected, &format!("{audience:?} without it"));
    }
}

#[test]
fn approval_needs_the_paired_request() {
    let o = opts(Audience::Approval);
    let with = Facts {
        paired_request: true,
        ..plain_facts()
    };
    assert_valid(&o, &with, "approval with the paired request");
    assert_errors(
        &o,
        &plain_facts(),
        &[OptionError::ApprovalNeedsPairedRequest],
        "approval without it",
    );
    // An approval link gains a wrap at each approval, up to the limit.
    for wraps in [1, 10, MAX_WRAPS] {
        assert_valid(&o, &Facts { wraps, ..with }, &format!("{wraps} approvals"));
    }
    assert_errors(
        &o,
        &Facts {
            wraps: MAX_WRAPS + 1,
            ..with
        },
        &[OptionError::TooManyWraps {
            wraps: MAX_WRAPS + 1,
        }],
        "one approval too many",
    );
}

#[test]
fn people_needs_one_wrap_per_person() {
    let o = opts(people(&[10, 11, 12]));
    for wraps in 0..=6usize {
        let f = Facts {
            wraps,
            ..facts_for(&o)
        };
        if wraps == 3 {
            assert_valid(&o, &f, "three people, three wraps");
        } else {
            assert_errors(
                &o,
                &f,
                &[OptionError::PeopleNeedOneWrapEach { people: 3, wraps }],
                &format!("three people, {wraps} wraps"),
            );
        }
    }
    // A `people` audience names someone.
    let nobody = opts(people(&[]));
    assert_errors(
        &nobody,
        &facts_for(&nobody),
        &[OptionError::PeopleEmpty],
        "no one named",
    );
    // And at most MAX_PEOPLE.
    let many = |n: usize| -> Vec<u8> { (0..n).map(|i| i as u8).collect() };
    let at_limit = opts(people(&many(MAX_PEOPLE)));
    assert_valid(&at_limit, &facts_for(&at_limit), "the most people");
    let over = opts(people(&many(MAX_PEOPLE + 1)));
    assert_errors(
        &over,
        &facts_for(&over),
        &[OptionError::TooManyPeople {
            people: MAX_PEOPLE + 1,
        }],
        "one person too many",
    );
}

#[test]
fn wraps_are_for_people_and_approval_only() {
    for audience in [Audience::Anyone, Audience::SignedIn, groups(&[1])] {
        let o = opts(audience.clone());
        let f = Facts {
            wraps: 1,
            ..facts_for(&o)
        };
        assert_errors(
            &o,
            &f,
            &[OptionError::WrapsNotAllowed { wraps: 1 }],
            &format!("{audience:?}"),
        );
    }
}

#[test]
fn members_name_at_most_the_limit_of_groups() {
    let many = |n: usize| -> Vec<u8> { (0..n).map(|i| i as u8).collect() };
    let at_limit = opts(groups(&many(MAX_GROUPS)));
    assert_valid(&at_limit, &facts_for(&at_limit), "the most groups");
    let over = opts(groups(&many(MAX_GROUPS + 1)));
    assert_errors(
        &over,
        &facts_for(&over),
        &[OptionError::TooManyGroups {
            groups: MAX_GROUPS + 1,
        }],
        "one group too many",
    );
    // No group at all is the whole account.
    let all = opts(groups(&[]));
    assert_valid(&all, &facts_for(&all), "the whole account");
}

#[test]
fn password_and_its_record_go_together() {
    let mut o = opts(Audience::SignedIn);
    o.password = true;
    assert_errors(
        &o,
        &plain_facts(),
        &[OptionError::PasswordNeedsRecord],
        "a password without its record",
    );
    o.password = false;
    let with_record = Facts {
        opaque_record: true,
        ..plain_facts()
    };
    assert_errors(
        &o,
        &with_record,
        &[OptionError::RecordWithoutPassword],
        "a record without a password",
    );
    o.password = true;
    assert_valid(&o, &with_record, "a password with its record");
}

#[test]
fn subfolders_only_for_folder_kinds() {
    for kind in LinkKind::ALL {
        let mut o = LinkOptions::new(Audience::SignedIn, *kind);
        o.subfolders = true;
        let f = facts_for(&o);
        if kind.is_folder() {
            assert_valid(&o, &f, &format!("{kind} with subfolders"));
        } else {
            assert_errors(
                &o,
                &f,
                &[OptionError::SubfoldersOnlyForFolders { kind: *kind }],
                &format!("{kind} with subfolders"),
            );
        }
        o.subfolders = false;
        assert_valid(&o, &f, &format!("{kind} without subfolders"));
    }
}

#[test]
fn selection_needs_objects() {
    for kind in LinkKind::ALL {
        let o = LinkOptions::new(Audience::SignedIn, *kind);
        let none = Facts {
            grants: 0,
            ..facts_for(&o)
        };
        if kind.needs_objects() {
            assert_errors(
                &o,
                &none,
                &[OptionError::NeedsObjects { kind: *kind }],
                &format!("{kind} with nothing to serve"),
            );
        } else {
            // A live folder may be made over a folder that is still empty.
            assert_eq!(*kind, LinkKind::FolderLive);
            assert_valid(&o, &none, "a live folder with nothing yet");
        }
        for grants in [1, 5, MAX_GRANTS] {
            assert_valid(
                &o,
                &Facts { grants, ..none },
                &format!("{kind}: {grants} grants"),
            );
        }
        assert_errors(
            &o,
            &Facts {
                grants: MAX_GRANTS + 1,
                ..none
            },
            &[OptionError::TooManyGrants {
                grants: MAX_GRANTS + 1,
            }],
            &format!("{kind}: one grant too many"),
        );
    }
    // The selection, by name.
    let selection = LinkOptions::new(Audience::SignedIn, LinkKind::Selection);
    assert_errors(
        &selection,
        &Facts {
            grants: 0,
            ..plain_facts()
        },
        &[OptionError::NeedsObjects {
            kind: LinkKind::Selection,
        }],
        "a selection of nothing",
    );
}

#[test]
fn folder_live_follows_latest() {
    // The record derives `follow_latest` from the kind: a live folder follows, nothing else does.
    for kind in LinkKind::ALL {
        let record = common::build(LinkOptions::new(Audience::SignedIn, *kind));
        assert_eq!(
            record.follow_latest(),
            *kind == LinkKind::FolderLive,
            "{kind}"
        );
        assert_eq!(record.kind(), *kind);
    }
    // A live folder made over an empty folder is a link; `follow_latest` holds.
    let live = LinkOptions::new(Audience::SignedIn, LinkKind::FolderLive);
    let mut parts = parts_for(&live);
    parts.grants = common::grants(0);
    let facts = common::facts_of(&parts);
    let validated = validate(&live, &facts).expect("a live folder with nothing yet");
    let bounded = azlin_links::apply_policy(validated, &azlin_policy::LinkPolicy::allow_all(), NOW)
        .expect("no bounds");
    let record = azlin_links::LinkRecord::create(bounded, parts, link_id(2), NOW).expect("a link");
    assert!(record.follow_latest());
    assert!(record.grants().is_empty());
}

#[test]
fn version_pins_one_version() {
    // A version link is pinned: it never follows the latest version, it is no folder, and it names
    // what it serves.
    let record = common::build(LinkOptions::new(Audience::SignedIn, LinkKind::Version));
    assert!(!record.follow_latest());
    let mut o = LinkOptions::new(Audience::SignedIn, LinkKind::Version);
    o.subfolders = true;
    assert_errors(
        &o,
        &facts_for(&o),
        &[OptionError::SubfoldersOnlyForFolders {
            kind: LinkKind::Version,
        }],
        "a version has no subfolders",
    );
    let o = LinkOptions::new(Audience::SignedIn, LinkKind::Version);
    assert_errors(
        &o,
        &Facts {
            grants: 0,
            ..facts_for(&o)
        },
        &[OptionError::NeedsObjects {
            kind: LinkKind::Version,
        }],
        "a version names what it serves",
    );
}

#[test]
fn not_before_before_expires_at() {
    let mut o = opts(Audience::SignedIn);
    let f = facts_for(&o);
    // Either alone is fine.
    o.not_before = Some(later(1));
    assert_valid(&o, &f, "only not_before");
    o.not_before = None;
    o.expires_at = Some(later(2));
    assert_valid(&o, &f, "only expires_at");
    // Before is fine; equal and after are an empty window.
    o.not_before = Some(later(1));
    assert_valid(&o, &f, "not_before before expires_at");
    o.not_before = Some(later(2));
    assert_errors(
        &o,
        &f,
        &[OptionError::EmptyWindow],
        "not_before equals expires_at",
    );
    o.not_before = Some(later(3));
    assert_errors(
        &o,
        &f,
        &[OptionError::EmptyWindow],
        "not_before after expires_at",
    );
    // One second before is enough.
    o.not_before = Some(
        later(2)
            .checked_sub(Span::secs(1))
            .expect("after the epoch"),
    );
    assert_valid(&o, &f, "one second before");
}

#[test]
fn expires_at_must_lie_in_the_future() {
    let mut o = opts(Audience::SignedIn);
    let f = facts_for(&o);
    o.expires_at = Some(NOW);
    assert_errors(
        &o,
        &f,
        &[OptionError::ExpiresAtNotInTheFuture],
        "expires now",
    );
    o.expires_at = Some(NOW.checked_sub(DAY).expect("after the epoch"));
    assert_errors(&o, &f, &[OptionError::ExpiresAtNotInTheFuture], "expired");
    o.expires_at = Some(NOW.checked_add(Span::secs(1)).expect("no overflow"));
    assert_valid(&o, &f, "expires in a second");
    // `not_before` may be in the past or the future.
    o.not_before = Some(NOW.checked_sub(DAY).expect("after the epoch"));
    assert_valid(&o, &f, "not_before in the past");
}

#[test]
fn counts_and_durations_positive() {
    let f = plain_facts();
    let zero = |option: OptionName| [OptionError::NotPositive { option }];
    let mut o = opts(Audience::SignedIn);

    o.max_downloads = Some(0);
    assert_errors(&o, &f, &zero(OptionName::MaxDownloads), "max_downloads 0");
    o.max_downloads = Some(1);
    assert_valid(&o, &f, "max_downloads 1");
    o.max_downloads = Some(u32::MAX);
    assert_valid(&o, &f, "max_downloads u32::MAX");
    o.max_downloads = None;

    o.bytes_cap = Some(ByteCount::ZERO);
    assert_errors(&o, &f, &zero(OptionName::BytesCap), "bytes_cap 0");
    o.bytes_cap = Some(ByteCount::new(1));
    assert_valid(&o, &f, "bytes_cap 1");
    o.bytes_cap = None;

    o.max_devices = Some(0);
    assert_errors(&o, &f, &zero(OptionName::MaxDevices), "max_devices 0");
    o.max_devices = Some(1);
    assert_valid(&o, &f, "max_devices 1");
    o.max_devices = Some(u16::MAX);
    assert_valid(&o, &f, "max_devices u16::MAX");
    o.max_devices = None;

    o.expires_after_open = Some(Span::ZERO);
    assert_errors(
        &o,
        &f,
        &zero(OptionName::ExpiresAfterOpen),
        "expires_after_open 0",
    );
    o.expires_after_open = Some(Span::secs(1));
    assert_valid(&o, &f, "expires_after_open 1 s");
    o.expires_after_open = Some(MAX_EXPIRES_AFTER_OPEN);
    assert_valid(&o, &f, "expires_after_open at its limit");
    o.expires_after_open = Some(
        MAX_EXPIRES_AFTER_OPEN
            .checked_add(Span::secs(1))
            .expect("no overflow"),
    );
    assert_errors(
        &o,
        &f,
        &[OptionError::TooLong {
            option: OptionName::ExpiresAfterOpen,
        }],
        "expires_after_open beyond its limit",
    );
    o.expires_after_open = None;

    o.idle_expiry = Some(Span::ZERO);
    assert_errors(&o, &f, &zero(OptionName::IdleExpiry), "idle_expiry 0");
    o.idle_expiry = Some(Span::days(1));
    assert_valid(&o, &f, "idle_expiry 1 day");
    o.idle_expiry = Some(MAX_IDLE_EXPIRY);
    assert_valid(&o, &f, "idle_expiry at its limit");
    o.idle_expiry = Some(
        MAX_IDLE_EXPIRY
            .checked_add(Span::secs(1))
            .expect("no overflow"),
    );
    assert_errors(
        &o,
        &f,
        &[OptionError::TooLong {
            option: OptionName::IdleExpiry,
        }],
        "idle_expiry beyond its limit",
    );
    o.idle_expiry = None;

    // Everything at once: every error, in the order of the rules.
    o.max_downloads = Some(0);
    o.bytes_cap = Some(ByteCount::ZERO);
    o.max_devices = Some(0);
    o.expires_after_open = Some(Span::ZERO);
    o.idle_expiry = Some(Span::ZERO);
    assert_errors(
        &o,
        &f,
        &[
            OptionError::NotPositive {
                option: OptionName::MaxDownloads,
            },
            OptionError::NotPositive {
                option: OptionName::BytesCap,
            },
            OptionError::NotPositive {
                option: OptionName::MaxDevices,
            },
            OptionError::NotPositive {
                option: OptionName::ExpiresAfterOpen,
            },
            OptionError::NotPositive {
                option: OptionName::IdleExpiry,
            },
        ],
        "every zero",
    );
}

#[test]
fn title_and_message_are_sealed_and_capped() {
    let f = plain_facts();
    // A plain title is no sealed value: the API takes `Sealed` and a short text is not one.
    assert!(Sealed::new(b"Holiday photos".to_vec()).is_err());
    assert!(Sealed::new(Vec::new()).is_err());

    let mut o = opts(Audience::SignedIn);
    o.title = Some(sealed(1, TITLE_MAX_LEN));
    assert_valid(&o, &f, "a title at its cap");
    o.title = Some(sealed(1, TITLE_MAX_LEN + 1));
    assert_errors(&o, &f, &[OptionError::TitleTooLong], "a title over its cap");
    o.title = None;

    o.message = Some(sealed(2, MESSAGE_MAX_LEN));
    assert_valid(&o, &f, "a message at its cap");
    o.message = Some(sealed(2, MESSAGE_MAX_LEN + 1));
    assert_errors(
        &o,
        &f,
        &[OptionError::MessageTooLong],
        "a message over its cap",
    );

    // Both over: both reported.
    o.title = Some(sealed(1, TITLE_MAX_LEN + 1));
    assert_errors(
        &o,
        &f,
        &[OptionError::TitleTooLong, OptionError::MessageTooLong],
        "both over",
    );
}

#[test]
fn validate_reports_every_broken_rule_at_once() {
    let slug = Slug::parse("a-name").expect("a slug");
    let mut o = opts(Audience::Anyone);
    o.slug = Some(slug);
    o.edit = true;
    o.comment = true;
    o.open_log = true;
    o.allow_upload = true;
    o.subfolders = true;
    o.not_before = Some(later(2));
    o.expires_at = Some(later(1));
    o.max_downloads = Some(0);
    assert_errors(
        &o,
        &plain_facts(),
        &[
            OptionError::SlugNeedsPasswordOrNamedAudience,
            OptionError::EditNeedsNamedAudience,
            OptionError::CommentNeedsSignedInAudience,
            OptionError::CommentNeedsLog,
            OptionError::OpenLogNeedsSignedInAudience,
            OptionError::UploadNeedsPairedRequest,
            OptionError::SubfoldersOnlyForFolders {
                kind: LinkKind::File,
            },
            OptionError::EmptyWindow,
            OptionError::NotPositive {
                option: OptionName::MaxDownloads,
            },
        ],
        "everything wrong",
    );
}

#[test]
fn every_error_names_its_option() {
    let cases: [(OptionError, OptionName); 24] = [
        (OptionError::PeopleEmpty, OptionName::Audience),
        (
            OptionError::TooManyPeople { people: 1 },
            OptionName::Audience,
        ),
        (
            OptionError::TooManyGroups { groups: 1 },
            OptionName::Audience,
        ),
        (
            OptionError::PeopleNeedOneWrapEach {
                people: 1,
                wraps: 2,
            },
            OptionName::Audience,
        ),
        (
            OptionError::WrapsNotAllowed { wraps: 1 },
            OptionName::Audience,
        ),
        (OptionError::TooManyWraps { wraps: 1 }, OptionName::Audience),
        (
            OptionError::ApprovalNeedsPairedRequest,
            OptionName::Audience,
        ),
        (OptionError::PasswordNeedsRecord, OptionName::Password),
        (OptionError::RecordWithoutPassword, OptionName::Password),
        (
            OptionError::SlugNeedsPasswordOrNamedAudience,
            OptionName::Slug,
        ),
        (OptionError::EditNeedsNamedAudience, OptionName::Edit),
        (
            OptionError::CommentNeedsSignedInAudience,
            OptionName::Comment,
        ),
        (OptionError::CommentNeedsLog, OptionName::Comment),
        (
            OptionError::OpenLogNeedsSignedInAudience,
            OptionName::OpenLog,
        ),
        (
            OptionError::UploadNeedsPairedRequest,
            OptionName::AllowUpload,
        ),
        (
            OptionError::SubfoldersOnlyForFolders {
                kind: LinkKind::File,
            },
            OptionName::Subfolders,
        ),
        (
            OptionError::NeedsObjects {
                kind: LinkKind::File,
            },
            OptionName::Kind,
        ),
        (OptionError::TooManyGrants { grants: 1 }, OptionName::Kind),
        (OptionError::EmptyWindow, OptionName::NotBefore),
        (OptionError::ExpiresAtNotInTheFuture, OptionName::ExpiresAt),
        (
            OptionError::NotPositive {
                option: OptionName::BytesCap,
            },
            OptionName::BytesCap,
        ),
        (
            OptionError::TooLong {
                option: OptionName::IdleExpiry,
            },
            OptionName::IdleExpiry,
        ),
        (OptionError::TitleTooLong, OptionName::Title),
        (OptionError::MessageTooLong, OptionName::Message),
    ];
    for (error, option) in cases {
        assert_eq!(error.option(), option, "{error}");
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn a_valid_set_gives_a_witness_that_remembers_what_it_checked() {
    let mut o = opts(people(&[10, 11]));
    o.expires_at = Some(later(7));
    let f = facts_for(&o);
    let validated = validate(&o, &f).expect("valid options");
    assert_eq!(validated.options(), &o);
    assert_eq!(validated.facts(), &f);
    // Every audience of the table can be valid.
    for audience in every_audience() {
        let o = opts(audience.clone());
        assert_valid(&o, &facts_for(&o), &format!("{audience:?}"));
    }
}
