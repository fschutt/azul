//! Check 4: changing a link. Server options change at any time; who can decrypt changes only by
//! rotating (a new id, the old URL stops at once); `paused` stops and resumes a link without changing
//! it and `link_revoke` ends it; writers re-point a live folder; only the creator or a manager changes
//! a link. Every change is judged whole, and a refused change leaves the record as it was (a property
//! test checks it over thousands of seeded changes).
//!
//! Implements planning/architecture.md section 5.12 ("Changing a link", "Live folders").

mod common;

use std::collections::BTreeMap;

use azlin_links::{
    pause, repoint, revoke, rotate, unpause, update, Actor, Audience, Change, Counters,
    DeviceToken, KeyBound, LinkKind, LinkOptions, LinkRecord, Notify, OptionError, OptionName,
    PausedBy, Pauses, PolicyViolation, PublicLinkId, RecordError, Rights, RotationReason, Slug,
    UpdateRefusal,
};
use azlin_policy::{AudienceClass, AudienceSet, LinkPolicy};
use azlin_seams::sim::Rng;
use azlin_seams::{ByteCount, EntropyError, Span, Stamp};
use common::{
    build, build_under, creator, creator_without_write, grants, groups, link_id, log, manager,
    member, object, opaque, opts, people, request, sealed, stranger, wrap, writer, DeadEntropy,
    FixedEntropy, TestEntropy, NOW,
};

/// A policy that bounds nothing.
fn open() -> LinkPolicy {
    LinkPolicy::allow_all()
}

/// The stamp `secs` seconds after `NOW`.
fn later(secs: u64) -> Stamp {
    NOW.checked_add(Span::secs(secs)).expect("no overflow")
}

/// The stamp `n` days after `NOW`.
fn days(n: u32) -> Stamp {
    NOW.checked_add(Span::days(n)).expect("no overflow")
}

/// A link for the people 10 and 11, made by the member 1.
fn people_link() -> LinkRecord {
    build(opts(people(&[10, 11])))
}

/// A link for every signed-in account.
fn signed_in_link() -> LinkRecord {
    build(opts(Audience::SignedIn))
}

/// A live folder link.
fn live_link() -> LinkRecord {
    build(LinkOptions::new(Audience::SignedIn, LinkKind::FolderLive))
}

/// The creator changes `r`.
fn apply(r: &mut LinkRecord, change: Change) -> Result<(), UpdateRefusal> {
    update(r, change, &creator(), &open(), NOW)
}

/// The key-bound parts a link has now (a rotation that changes nothing but the id).
fn same_key(r: &LinkRecord) -> KeyBound {
    KeyBound {
        audience: r.options().audience.clone(),
        kind: r.kind(),
        subfolders: r.options().subfolders,
        manifest_object: *r.manifest_object(),
        grants: r.grants().clone(),
        opaque_record: r.opaque_record().cloned(),
        wraps: r.wraps().cloned().collect(),
        request: r.request_id().copied(),
    }
}

/// The same link after some use: counters that are not zero.
fn used(link: &LinkRecord) -> LinkRecord {
    let mut row = link.row().clone();
    row.counters.downloads = 5;
    row.counters.bytes_served = ByteCount::mib(3);
    row.counters
        .devices_seen
        .insert(DeviceToken::from_bytes([1; 16]).seen_by(link.id()));
    row.counters.first_open_at = Some(NOW);
    row.counters.last_open_at = Some(later(60));
    LinkRecord::from_row(row).expect("a used link")
}

/// A check of the options after a change.
type Check = Box<dyn Fn(&LinkOptions) -> bool>;

fn check(f: impl Fn(&LinkOptions) -> bool + 'static) -> Check {
    Box::new(f)
}

/// Every server option, a change of it, and what the options must be after it.
fn server_option_cases() -> Vec<(&'static str, Change, Check)> {
    let slug = Slug::parse("quarterly-report").expect("a slug");
    let slug_after = slug.clone();
    let title = sealed(1, 40);
    let title_after = title.clone();
    let message = sealed(2, 80);
    let message_after = message.clone();
    vec![
        ("app_only", Change::AppOnly(true), check(|o| o.app_only)),
        (
            "not_before",
            Change::NotBefore(Some(days(1))),
            check(|o| o.not_before == Some(days(1))),
        ),
        (
            "expires_at",
            Change::ExpiresAt(Some(days(30))),
            check(|o| o.expires_at == Some(days(30))),
        ),
        (
            "expires_after_open",
            Change::ExpiresAfterOpen(Some(Span::days(2))),
            check(|o| o.expires_after_open == Some(Span::days(2))),
        ),
        (
            "idle_expiry",
            Change::IdleExpiry(Some(Span::days(7))),
            check(|o| o.idle_expiry == Some(Span::days(7))),
        ),
        ("one_time", Change::OneTime(true), check(|o| o.one_time)),
        (
            "max_downloads",
            Change::MaxDownloads(Some(5)),
            check(|o| o.max_downloads == Some(5)),
        ),
        (
            "bytes_cap",
            Change::BytesCap(Some(ByteCount::gib(1))),
            check(|o| o.bytes_cap == Some(ByteCount::gib(1))),
        ),
        (
            "max_devices",
            Change::MaxDevices(Some(3)),
            check(|o| o.max_devices == Some(3)),
        ),
        (
            "rights",
            Change::Rights(Rights::PreviewOnly),
            check(|o| o.rights == Rights::PreviewOnly),
        ),
        (
            "allow_upload",
            Change::AllowUpload(true),
            check(|o| o.allow_upload),
        ),
        ("edit", Change::Edit(true), check(|o| o.edit)),
        ("comment", Change::Comment(true), check(|o| o.comment)),
        (
            "allow_reshare",
            Change::AllowReshare(true),
            check(|o| o.allow_reshare),
        ),
        ("watermark", Change::Watermark(true), check(|o| o.watermark)),
        (
            "notify",
            Change::Notify(Notify::EveryOpen),
            check(|o| o.notify == Notify::EveryOpen),
        ),
        ("open_log", Change::OpenLog(true), check(|o| o.open_log)),
        (
            "title",
            Change::Title(Some(title)),
            check(move |o| o.title.as_ref() == Some(&title_after)),
        ),
        (
            "message",
            Change::Message(Some(message)),
            check(move |o| o.message.as_ref() == Some(&message_after)),
        ),
        (
            "slug",
            Change::Slug(Some(slug)),
            check(move |o| o.slug.as_ref() == Some(&slug_after)),
        ),
    ]
}

/// A people link with the paired request and the comment log its upload and comment options need.
fn paired_people_link() -> LinkRecord {
    let mut r = people_link();
    apply(&mut r, Change::PairRequest(request(7))).expect("pair the request");
    apply(&mut r, Change::PairCommentLog(log(8))).expect("pair the log");
    r
}

#[test]
fn server_options_change_at_any_time() {
    let base = paired_people_link();
    assert_eq!(base.request_id(), Some(&request(7)));
    assert_eq!(base.comment_log(), Some(&log(8)));
    assert_eq!(server_option_cases().len(), 20);

    // Each one alone, at three different times, with the link paused by its owner or not.
    for (what, change, holds) in server_option_cases() {
        for now in [NOW, later(10 * 86_400), later(29 * 86_400)] {
            for owner_paused in [false, true] {
                let mut r = base.clone();
                if owner_paused {
                    pause(&mut r, &creator()).expect("the owner pauses");
                }
                update(&mut r, change.clone(), &creator(), &open(), now)
                    .unwrap_or_else(|e| panic!("{what} at {now:?}: {e}"));
                assert!(holds(r.options()), "{what}");
                // A server option changes that option and nothing else: not the id, the key's
                // parts, the counters, the times.
                assert_eq!(r.id(), base.id(), "{what}");
                assert_eq!(r.keyed_at(), base.keyed_at(), "{what}");
                assert_eq!(r.created_at(), base.created_at(), "{what}");
                assert_eq!(r.manifest_object(), base.manifest_object(), "{what}");
                assert_eq!(r.grants(), base.grants(), "{what}");
                assert_eq!(r.options().audience, base.options().audience, "{what}");
                assert_eq!(r.counters(), base.counters(), "{what}");
                assert_eq!(r.check(), Ok(()), "{what}");
                assert_eq!(r.is_paused(), owner_paused, "{what}");
            }
        }
    }

    // All of them in a row on one link, and the record is the same link with the options changed.
    let mut r = base.clone();
    for (what, change, _) in server_option_cases() {
        apply(&mut r, change).unwrap_or_else(|e| panic!("{what}: {e}"));
    }
    for (what, _, holds) in server_option_cases() {
        assert!(holds(r.options()), "{what}");
    }
    assert_eq!(r.id(), base.id());
    assert_eq!(r.wraps().count(), 2);
    assert_eq!(r.check(), Ok(()));

    // And back: every option can be turned off again.
    for change in [
        Change::AppOnly(false),
        Change::NotBefore(None),
        Change::ExpiresAt(None),
        Change::ExpiresAfterOpen(None),
        Change::IdleExpiry(None),
        Change::OneTime(false),
        Change::MaxDownloads(None),
        Change::BytesCap(None),
        Change::MaxDevices(None),
        Change::Rights(Rights::Download),
        Change::AllowUpload(false),
        Change::Edit(false),
        Change::Comment(false),
        Change::AllowReshare(false),
        Change::Watermark(false),
        Change::Notify(Notify::Never),
        Change::OpenLog(false),
        Change::Title(None),
        Change::Message(None),
        Change::Slug(None),
    ] {
        apply(&mut r, change.clone()).unwrap_or_else(|e| panic!("{change:?}: {e}"));
    }
    assert_eq!(r.options(), base.options());
}

#[test]
fn a_change_that_changes_nothing_is_ok_and_changes_nothing() {
    let base = people_link();
    for change in [
        Change::AppOnly(false),
        Change::Rights(Rights::Download),
        Change::MaxDownloads(None),
        Change::Audience(base.options().audience.clone()),
        Change::Password(None),
        Change::Kind(LinkKind::File),
        Change::Subfolders(false),
        Change::Grants(base.grants().clone()),
    ] {
        let mut r = base.clone();
        assert_eq!(apply(&mut r, change.clone()), Ok(()), "{change:?}");
        assert_eq!(r, base, "{change:?}");
    }
}

#[test]
fn a_server_option_that_breaks_a_rule_is_refused_with_the_rule() {
    // The options a change would leave are checked whole: a rule between options is not a loophole.
    let mut r = signed_in_link();
    let before = r.clone();
    for (change, error) in [
        (Change::Edit(true), OptionError::EditNeedsNamedAudience),
        (
            Change::AllowUpload(true),
            OptionError::UploadNeedsPairedRequest,
        ),
        (Change::Comment(true), OptionError::CommentNeedsLog),
        (
            Change::MaxDownloads(Some(0)),
            OptionError::NotPositive {
                option: OptionName::MaxDownloads,
            },
        ),
    ] {
        assert_eq!(
            apply(&mut r, change.clone()),
            Err(UpdateRefusal::Invalid(RecordError::Options(vec![error]))),
            "{change:?}"
        );
        assert_eq!(r, before, "{change:?}");
    }
    // A slug needs a password or a named audience.
    let slug = Slug::parse("a-name").expect("a slug");
    assert_eq!(
        apply(&mut r, Change::Slug(Some(slug))),
        Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::SlugNeedsPasswordOrNamedAudience
        ])))
    );
    // A window of dates that is empty.
    let mut dated = signed_in_link();
    apply(&mut dated, Change::ExpiresAt(Some(days(2)))).expect("an expiry");
    assert_eq!(
        apply(&mut dated, Change::NotBefore(Some(days(3)))),
        Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::EmptyWindow
        ])))
    );
    // An expiry that is not in the future.
    assert_eq!(
        update(
            &mut dated,
            Change::ExpiresAt(Some(later(5))),
            &creator(),
            &open(),
            later(10)
        ),
        Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::ExpiresAtNotInTheFuture
        ])))
    );
    assert_eq!(dated.options().expires_at, Some(days(2)));
}

#[test]
fn pairings_are_added_once() {
    let mut r = people_link();
    assert_eq!(r.request_id(), None);
    apply(&mut r, Change::PairRequest(request(7))).expect("pair");
    assert_eq!(
        apply(&mut r, Change::PairRequest(request(8))),
        Err(UpdateRefusal::AlreadyPaired)
    );
    assert_eq!(r.request_id(), Some(&request(7)));
    apply(&mut r, Change::PairCommentLog(log(8))).expect("pair");
    assert_eq!(
        apply(&mut r, Change::PairCommentLog(log(9))),
        Err(UpdateRefusal::AlreadyPaired)
    );
    assert_eq!(r.comment_log(), Some(&log(8)));
    // Uploads and comments then work, and only then.
    apply(&mut r, Change::AllowUpload(true)).expect("uploads");
    apply(&mut r, Change::Comment(true)).expect("comments");
}

#[test]
fn changing_audience_across_key_strength_requires_rotation() {
    let audiences = [
        Audience::Anyone,
        Audience::SignedIn,
        groups(&[1, 2]),
        groups(&[3]),
        people(&[10, 11]),
        people(&[12]),
        Audience::Approval,
    ];
    for (i, from) in audiences.iter().enumerate() {
        for (j, to) in audiences.iter().enumerate() {
            if i == j {
                continue;
            }
            let base = build(opts(from.clone()));
            let mut r = base.clone();
            let result = apply(&mut r, Change::Audience(to.clone()));
            if from.changes_without_rotation(to) {
                // Only a server filter changes: signed in <-> members, one list of groups for another.
                assert_eq!(result, Ok(()), "{from:?} -> {to:?}");
                assert_eq!(r.options().audience, *to);
                assert_eq!(r.id(), base.id());
            } else {
                // Across key strength (key <-> server), or between the fragment and the wraps.
                assert_eq!(
                    result,
                    Err(UpdateRefusal::RotationRequired(RotationReason::Audience)),
                    "{from:?} -> {to:?}"
                );
                assert_eq!(r, base, "{from:?} -> {to:?}");
            }
        }
    }
    // The examples of the plan, by name: key -> server and server -> key.
    let mut r = build(opts(Audience::Anyone));
    assert_eq!(
        apply(&mut r, Change::Audience(Audience::SignedIn)),
        Err(UpdateRefusal::RotationRequired(RotationReason::Audience))
    );
    let mut r = build(opts(Audience::SignedIn));
    assert_eq!(
        apply(&mut r, Change::Audience(Audience::Anyone)),
        Err(UpdateRefusal::RotationRequired(RotationReason::Audience))
    );
    let mut r = build(opts(Audience::SignedIn));
    assert_eq!(
        apply(&mut r, Change::Audience(people(&[10]))),
        Err(UpdateRefusal::RotationRequired(RotationReason::Audience))
    );
    // The people of a `people` link change by wraps, not by a new audience.
    let mut r = people_link();
    assert_eq!(
        apply(&mut r, Change::Audience(people(&[10, 11, 12]))),
        Err(UpdateRefusal::RotationRequired(RotationReason::Audience))
    );
}

#[test]
fn adding_a_person_wrap_needs_no_rotation() {
    // A person added to a `people` link: the wrap and the name, the id and the key stay.
    let base = people_link();
    let mut r = base.clone();
    apply(&mut r, Change::AddWrap(wrap(12))).expect("a person is added");
    assert_eq!(r.options().audience, people(&[10, 11, 12]));
    assert_eq!(r.wraps().count(), 3);
    assert!(r.wrap_for(&member(12)).is_some());
    assert_eq!(r.id(), base.id());
    assert_eq!(r.keyed_at(), base.keyed_at());
    assert_eq!(r.manifest_object(), base.manifest_object());
    assert_eq!(r.check(), Ok(()));
    // The same person twice is refused, and the record stays.
    let before = r.clone();
    assert_eq!(
        apply(&mut r, Change::AddWrap(wrap(12))),
        Err(UpdateRefusal::AlreadyWrapped)
    );
    assert_eq!(r, before);

    // An approval: the wrap of the requester's key, the audience stays.
    let approval = build(opts(Audience::Approval));
    let mut r = approval.clone();
    apply(&mut r, Change::AddWrap(wrap(20))).expect("an approval");
    apply(&mut r, Change::AddWrap(wrap(21))).expect("an approval");
    assert_eq!(r.options().audience, Audience::Approval);
    assert_eq!(r.wraps().count(), 2);
    assert!(r.wrap_for(&member(20)).is_some() && r.wrap_for(&member(21)).is_some());
    assert_eq!(r.id(), approval.id());

    // A link whose key is in the URL holds no wraps.
    for audience in [Audience::Anyone, Audience::SignedIn, groups(&[1])] {
        let base = build(opts(audience.clone()));
        let mut r = base.clone();
        assert_eq!(
            apply(&mut r, Change::AddWrap(wrap(20))),
            Err(UpdateRefusal::Invalid(RecordError::Options(vec![
                OptionError::WrapsNotAllowed { wraps: 1 }
            ]))),
            "{audience:?}"
        );
        assert_eq!(r, base);
    }
}

#[test]
fn removing_a_person_requires_rotation() {
    let base = people_link();
    let mut r = base.clone();
    assert_eq!(
        apply(&mut r, Change::RemoveWrap(member(10))),
        Err(UpdateRefusal::RotationRequired(RotationReason::WrapRemoved))
    );
    assert_eq!(r, base);
    // Someone who holds no wrap cannot be removed.
    assert_eq!(
        apply(&mut r, Change::RemoveWrap(member(99))),
        Err(UpdateRefusal::NoSuchWrap)
    );
    assert_eq!(r, base);
    // An approval taken back is the same: its holder knows the key.
    let mut approved = build(opts(Audience::Approval));
    apply(&mut approved, Change::AddWrap(wrap(20))).expect("an approval");
    let before = approved.clone();
    assert_eq!(
        apply(&mut approved, Change::RemoveWrap(member(20))),
        Err(UpdateRefusal::RotationRequired(RotationReason::WrapRemoved))
    );
    assert_eq!(approved, before);

    // The rotation that does remove the person: a new key wrapped to the others only.
    let mut r = base.clone();
    let to = KeyBound {
        audience: people(&[11]),
        wraps: vec![wrap(11)],
        ..same_key(&r)
    };
    let rotated = rotate(
        &mut r,
        &mut TestEntropy::new(9),
        to,
        &creator(),
        &open(),
        later(60),
    )
    .expect("rotated");
    assert!(r.wrap_for(&member(10)).is_none());
    assert!(r.wrap_for(&member(11)).is_some());
    assert_eq!(r.options().audience, people(&[11]));
    assert_ne!(rotated.old, rotated.new);
}

#[test]
fn password_change_requires_rotation() {
    let mut o = opts(Audience::SignedIn);
    o.password = true;
    let base = build(o);
    assert_eq!(base.opaque_record(), Some(&opaque(5)));
    for change in [Change::Password(Some(opaque(6))), Change::Password(None)] {
        let mut r = base.clone();
        assert_eq!(
            apply(&mut r, change.clone()),
            Err(UpdateRefusal::RotationRequired(RotationReason::Password)),
            "{change:?}"
        );
        assert_eq!(r, base);
    }
    // The same record is no change.
    let mut r = base.clone();
    assert_eq!(apply(&mut r, Change::Password(Some(opaque(5)))), Ok(()));
    assert_eq!(r, base);
    // A password cannot be added to a link without one either: the key would derive from it.
    let plain = signed_in_link();
    let mut r = plain.clone();
    assert_eq!(
        apply(&mut r, Change::Password(Some(opaque(1)))),
        Err(UpdateRefusal::RotationRequired(RotationReason::Password))
    );
    assert_eq!(r, plain);

    // The rotation that carries the new password: the link stays a password link.
    let mut r = base.clone();
    let to = KeyBound {
        opaque_record: Some(opaque(6)),
        ..same_key(&r)
    };
    rotate(
        &mut r,
        &mut TestEntropy::new(2),
        to,
        &creator(),
        &open(),
        later(1),
    )
    .expect("rotated");
    assert_eq!(r.opaque_record(), Some(&opaque(6)));
    assert!(r.options().password);
    // And the rotation that drops it.
    let to = KeyBound {
        opaque_record: None,
        ..same_key(&r)
    };
    rotate(
        &mut r,
        &mut TestEntropy::new(3),
        to,
        &creator(),
        &open(),
        later(2),
    )
    .expect("rotated");
    assert_eq!(r.opaque_record(), None);
    assert!(!r.options().password);
}

#[test]
fn kind_subfolders_and_grants_changes_require_rotation() {
    let base = signed_in_link();
    let mut r = base.clone();
    assert_eq!(
        apply(&mut r, Change::Kind(LinkKind::Version)),
        Err(UpdateRefusal::RotationRequired(RotationReason::Kind))
    );
    assert_eq!(
        apply(&mut r, Change::Subfolders(true)),
        Err(UpdateRefusal::RotationRequired(RotationReason::Subfolders))
    );
    assert_eq!(
        apply(&mut r, Change::Grants(grants(3))),
        Err(UpdateRefusal::RotationRequired(RotationReason::Grants))
    );
    assert_eq!(r, base);
    // The rotation carries them: a folder with its subfolders and more grants.
    let to = KeyBound {
        kind: LinkKind::FolderSnapshot,
        subfolders: true,
        grants: grants(3),
        manifest_object: object(70),
        ..same_key(&r)
    };
    rotate(
        &mut r,
        &mut TestEntropy::new(4),
        to,
        &creator(),
        &open(),
        later(1),
    )
    .expect("rotated");
    assert_eq!(r.kind(), LinkKind::FolderSnapshot);
    assert!(r.options().subfolders);
    assert_eq!(r.grants().len(), 3);
    assert_eq!(r.manifest_object(), &object(70));
}

#[test]
fn rotation_gives_a_new_id_and_the_old_id_stops_at_once() {
    let mut o = opts(people(&[10, 11]));
    o.allow_upload = true;
    let r = used(&build(o));
    assert_eq!(r.request_id(), Some(&request(3)));
    let old = *r.id();

    // The node's index of ids, as the glue keeps it.
    let mut index: BTreeMap<PublicLinkId, LinkRecord> = BTreeMap::new();
    index.insert(old, r.clone());

    let when = later(5 * 86_400);
    let mut record = index.remove(&old).expect("the link");
    let to = KeyBound {
        audience: people(&[10, 12]),
        manifest_object: object(77),
        grants: grants(2),
        wraps: vec![wrap(10), wrap(12)],
        request: Some(request(30)),
        ..same_key(&record)
    };
    let rotated = rotate(
        &mut record,
        &mut TestEntropy::new(5),
        to,
        &creator(),
        &open(),
        when,
    )
    .expect("rotated");
    // The glue moves the link from the old id to the new one.
    index.insert(*record.id(), record.clone());

    // A new id; the old id answers nothing from this moment, in the record and in the index.
    assert_eq!(rotated.old, old);
    assert_ne!(rotated.new, old);
    assert_eq!(&rotated.new, record.id());
    assert!(!record.answers_to(&old));
    assert!(record.answers_to(&rotated.new));
    assert!(!index.contains_key(&old));
    assert!(index.contains_key(&rotated.new));
    // The new id is a link id like any other.
    assert!(azlin_links::has_link_id_shape(&rotated.new.to_string()));

    // The paired request of the old id ends with it, and the new one is the record's.
    assert_eq!(rotated.ended_request, Some(request(3)));
    assert_eq!(record.request_id(), Some(&request(30)));
    // The parts that come with the new key.
    assert_eq!(record.manifest_object(), &object(77));
    assert_eq!(record.grants(), &grants(2));
    assert_eq!(record.options().audience, people(&[10, 12]));
    assert!(record.wrap_for(&member(11)).is_none());
    assert!(record.wrap_for(&member(12)).is_some());
    // What the old URL's holders did is history: the counters start again.
    assert_eq!(record.counters(), &Counters::zero());
    assert_eq!(r.counters().downloads, 5);
    // The link began again at the rotation, and was created when it was.
    assert_eq!(record.keyed_at(), when);
    assert_eq!(record.created_at(), NOW);
    // What the rotation did not touch.
    assert_eq!(record.created_by(), r.created_by());
    assert_eq!(record.space(), r.space());
    let mut expected = r.options().clone();
    expected.audience = people(&[10, 12]);
    assert_eq!(record.options(), &expected);
    assert_eq!(record.check(), Ok(()));
    // Rotated again: another id, never the first.
    let first = rotated.new;
    let key = same_key(&record);
    let again = rotate(
        &mut record,
        &mut TestEntropy::new(6),
        key,
        &creator(),
        &open(),
        later(6 * 86_400),
    )
    .expect("rotated again");
    assert_eq!(again.old, first);
    assert_ne!(again.new, first);
    assert_ne!(again.new, old);
}

#[test]
fn rotation_keeps_the_pauses_and_the_comment_log() {
    let mut r = paired_people_link();
    pause(&mut r, &creator()).expect("the owner pauses");
    let key = same_key(&r);
    let rotated = rotate(
        &mut r,
        &mut TestEntropy::new(1),
        key,
        &creator(),
        &open(),
        later(1),
    )
    .expect("rotated");
    assert_ne!(rotated.old, rotated.new);
    assert_eq!(r.pauses(), Pauses::NONE.with(PausedBy::Owner));
    assert_eq!(r.comment_log(), Some(&log(8)));
    // With the same request given again, nothing about the request changes.
    assert_eq!(r.request_id(), Some(&request(7)));
    assert_eq!(rotated.ended_request, Some(request(7)));
}

#[test]
fn a_rotation_that_cannot_make_an_id_changes_nothing() {
    let base = people_link();
    // The entropy fails: no id, and the record is as it was.
    let mut r = base.clone();
    let key = same_key(&r);
    assert_eq!(
        rotate(&mut r, &mut DeadEntropy, key, &creator(), &open(), later(1)),
        Err(UpdateRefusal::Entropy(EntropyError::Exhausted))
    );
    assert_eq!(r, base);
    // A source that repeats the old id would leave the old URL working: refused.
    let old_bytes = *base.id().as_bytes();
    let key = same_key(&r);
    assert_eq!(
        rotate(
            &mut r,
            &mut FixedEntropy(old_bytes),
            key,
            &creator(),
            &open(),
            later(1)
        ),
        Err(UpdateRefusal::SameId)
    );
    assert_eq!(r, base);
}

#[test]
fn a_rotation_is_judged_whole() {
    let base = people_link();
    let rotate_to = |to: KeyBound| {
        let mut r = base.clone();
        let result = rotate(
            &mut r,
            &mut TestEntropy::new(1),
            to,
            &creator(),
            &open(),
            later(1),
        );
        assert_eq!(r, base, "a refused rotation changes nothing");
        result
    };
    // Two people, one wrap.
    let result = rotate_to(KeyBound {
        wraps: vec![wrap(10)],
        ..same_key(&base)
    });
    assert_eq!(
        result,
        Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::PeopleNeedOneWrapEach {
                people: 2,
                wraps: 1
            }
        ])))
    );
    // Two wraps for one member.
    let result = rotate_to(KeyBound {
        wraps: vec![wrap(10), wrap(10)],
        ..same_key(&base)
    });
    assert_eq!(
        result,
        Err(UpdateRefusal::Invalid(RecordError::DuplicateWrap(member(
            10
        ))))
    );
    // Wraps that are not the named people.
    let result = rotate_to(KeyBound {
        wraps: vec![wrap(10), wrap(12)],
        ..same_key(&base)
    });
    assert_eq!(
        result,
        Err(UpdateRefusal::Invalid(RecordError::WrapsAreNotThePeople))
    );
    // A selection that names no object.
    let result = rotate_to(KeyBound {
        kind: LinkKind::Selection,
        grants: grants(0),
        ..same_key(&base)
    });
    assert_eq!(
        result,
        Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::NeedsObjects {
                kind: LinkKind::Selection
            }
        ])))
    );
    // Wraps for an audience whose key is in the URL.
    let result = rotate_to(KeyBound {
        audience: Audience::SignedIn,
        ..same_key(&base)
    });
    assert_eq!(
        result,
        Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::WrapsNotAllowed { wraps: 2 }
        ])))
    );
    // An approval link needs its paired request.
    let result = rotate_to(KeyBound {
        audience: Audience::Approval,
        wraps: Vec::new(),
        request: None,
        ..same_key(&base)
    });
    assert_eq!(
        result,
        Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::ApprovalNeedsPairedRequest
        ])))
    );
    // The same rotation with the request is fine.
    let mut r = base.clone();
    let to = KeyBound {
        audience: Audience::Approval,
        wraps: Vec::new(),
        request: Some(request(31)),
        ..same_key(&base)
    };
    rotate(
        &mut r,
        &mut TestEntropy::new(1),
        to,
        &creator(),
        &open(),
        later(1),
    )
    .expect("rotated");
    assert_eq!(r.options().audience, Audience::Approval);
    assert_eq!(r.wraps().count(), 0);
    // A time before the creation is no time to key a link.
    let mut r = base.clone();
    let key = same_key(&r);
    let early = NOW.checked_sub(Span::secs(1)).expect("after the epoch");
    assert_eq!(
        rotate(
            &mut r,
            &mut TestEntropy::new(1),
            key,
            &creator(),
            &open(),
            early
        ),
        Err(UpdateRefusal::Invalid(RecordError::KeyedBeforeCreated))
    );
    assert_eq!(r, base);
}

#[test]
fn pause_and_unpause_change_nothing_else() {
    let base = used(&paired_people_link());
    for actor in [creator(), manager(), creator_without_write()] {
        let mut r = base.clone();
        let paused = pause(&mut r, &actor).expect("the owner pauses");
        assert_eq!(paused, Pauses::NONE.with(PausedBy::Owner));
        assert!(r.is_paused());
        assert_eq!(r.pauses(), paused);
        // Nothing but the pause differs.
        let mut row = r.row().clone();
        row.pauses = Pauses::NONE;
        assert_eq!(&row, base.row(), "{actor:?}");
        // Pausing again is the same pause.
        assert_eq!(pause(&mut r, &actor), Ok(paused));
        // And the owner lifts it: the link is the link it was.
        assert_eq!(unpause(&mut r, &actor), Ok(Pauses::NONE));
        assert!(!r.is_paused());
        assert_eq!(r, base, "{actor:?}");
        // Unpausing a link that is not paused is fine and changes nothing.
        assert_eq!(unpause(&mut r, &actor), Ok(Pauses::NONE));
        assert_eq!(r, base);
    }
    // The reports and the operator pause the same way (their pause is theirs).
    for (actor, by) in [
        (Actor::Reports, PausedBy::Reports),
        (Actor::Operator, PausedBy::Operator),
    ] {
        let mut r = base.clone();
        assert_eq!(pause(&mut r, &actor), Ok(Pauses::NONE.with(by)));
        let mut row = r.row().clone();
        row.pauses = Pauses::NONE;
        assert_eq!(&row, base.row());
    }
    // A pause never loses another source's pause.
    let mut r = base.clone();
    pause(&mut r, &creator()).expect("owner");
    pause(&mut r, &Actor::Reports).expect("reports");
    pause(&mut r, &Actor::Operator).expect("operator");
    assert_eq!(
        r.pauses().sources(),
        vec![PausedBy::Owner, PausedBy::Reports, PausedBy::Operator]
    );
}

#[test]
fn operator_pause_is_lifted_only_by_the_operator() {
    let base = people_link();
    let operator_pause = Pauses::NONE.with(PausedBy::Operator);

    let mut r = base.clone();
    assert_eq!(pause(&mut r, &Actor::Operator), Ok(operator_pause));
    // The owner, a manager, a stranger and the reports cannot lift it.
    for actor in [creator(), manager(), creator_without_write()] {
        assert_eq!(unpause(&mut r, &actor), Ok(operator_pause), "{actor:?}");
        assert!(r.is_paused(), "{actor:?}");
    }
    for actor in [writer(), stranger(), Actor::Reports] {
        assert_eq!(
            unpause(&mut r, &actor),
            Err(UpdateRefusal::NotPermitted),
            "{actor:?}"
        );
        assert!(r.is_paused(), "{actor:?}");
    }
    assert_eq!(r.pauses(), operator_pause);
    // The operator lifts it.
    assert_eq!(unpause(&mut r, &Actor::Operator), Ok(Pauses::NONE));
    assert!(!r.is_paused());
    assert_eq!(r, base);

    // The pause of the reports ("pauses until the operator decides") is the operator's to lift too.
    let mut r = base.clone();
    assert_eq!(
        pause(&mut r, &Actor::Reports),
        Ok(Pauses::NONE.with(PausedBy::Reports))
    );
    for actor in [creator(), manager()] {
        assert_eq!(
            unpause(&mut r, &actor),
            Ok(Pauses::NONE.with(PausedBy::Reports))
        );
        assert!(r.is_paused());
    }
    assert_eq!(unpause(&mut r, &Actor::Operator), Ok(Pauses::NONE));
    assert!(!r.is_paused());

    // Each source lifts its own: the owner's pause survives the operator's lifting, and the
    // operator's survives the owner's.
    let mut r = base.clone();
    pause(&mut r, &creator()).expect("owner");
    pause(&mut r, &Actor::Operator).expect("operator");
    assert_eq!(
        unpause(&mut r, &Actor::Operator),
        Ok(Pauses::NONE.with(PausedBy::Owner))
    );
    assert!(r.is_paused());
    assert_eq!(unpause(&mut r, &creator()), Ok(Pauses::NONE));
    assert!(!r.is_paused());
}

#[test]
fn a_link_the_operator_or_the_reports_paused_is_frozen_for_its_owner() {
    let base = paired_people_link();
    for (actor, by) in [
        (Actor::Operator, PausedBy::Operator),
        (Actor::Reports, PausedBy::Reports),
    ] {
        let mut r = base.clone();
        pause(&mut r, &actor).expect("paused");
        let frozen = r.clone();
        assert_eq!(
            apply(&mut r, Change::AppOnly(true)),
            Err(UpdateRefusal::Frozen(by))
        );
        let key = same_key(&r);
        assert_eq!(
            rotate(
                &mut r,
                &mut TestEntropy::new(1),
                key,
                &creator(),
                &open(),
                later(1)
            ),
            Err(UpdateRefusal::Frozen(by))
        );
        assert_eq!(r, frozen);
        // The owner can still end it.
        assert_eq!(revoke(&mut r, &creator(), later(2)), Ok(()));
        assert!(r.is_revoked());
    }
    // A live folder is frozen for its writers as well.
    let mut live = live_link();
    pause(&mut live, &Actor::Operator).expect("paused");
    assert_eq!(
        repoint(&mut live, object(70), grants(2), &writer()),
        Err(UpdateRefusal::Frozen(PausedBy::Operator))
    );
    // The owner's own pause freezes nothing: the owner can still change the link, and a writer can
    // still re-point a live folder (it follows the edits while it waits).
    let mut r = base.clone();
    pause(&mut r, &creator()).expect("paused");
    assert_eq!(apply(&mut r, Change::AppOnly(true)), Ok(()));
    let mut live = live_link();
    pause(&mut live, &creator()).expect("paused");
    assert_eq!(repoint(&mut live, object(70), grants(2), &writer()), Ok(()));
}

#[test]
fn revoke_is_final() {
    let base = live_link();
    let mut r = base.clone();
    assert_eq!(revoke(&mut r, &creator(), later(10)), Ok(()));
    assert!(r.is_revoked());
    assert_eq!(r.revoked_at(), Some(later(10)));
    let revoked = r.clone();

    // Nothing changes a revoked link: every function says Revoked and the record stays.
    assert_eq!(
        apply(&mut r, Change::AppOnly(true)),
        Err(UpdateRefusal::Revoked)
    );
    let key = same_key(&r);
    assert_eq!(
        rotate(
            &mut r,
            &mut TestEntropy::new(1),
            key,
            &creator(),
            &open(),
            later(11)
        ),
        Err(UpdateRefusal::Revoked)
    );
    for actor in [creator(), manager(), Actor::Reports, Actor::Operator] {
        assert_eq!(
            pause(&mut r, &actor),
            Err(UpdateRefusal::Revoked),
            "pause by {actor:?}"
        );
    }
    for actor in [creator(), manager(), Actor::Operator] {
        assert_eq!(
            unpause(&mut r, &actor),
            Err(UpdateRefusal::Revoked),
            "unpause by {actor:?}"
        );
    }
    assert_eq!(
        repoint(&mut r, object(70), grants(2), &writer()),
        Err(UpdateRefusal::Revoked)
    );
    assert_eq!(r, revoked);

    // Revoking again is fine and keeps the first time.
    assert_eq!(revoke(&mut r, &manager(), later(99)), Ok(()));
    assert_eq!(revoke(&mut r, &Actor::Operator, later(100)), Ok(()));
    assert_eq!(r.revoked_at(), Some(later(10)));
    assert_eq!(r, revoked);
    // It stays a record: it can be read back.
    assert_eq!(LinkRecord::from_row(r.row().clone()), Ok(r.clone()));

    // A link that was paused is revoked as it is (and stays paused: revoked links are not served).
    let mut p = base.clone();
    pause(&mut p, &Actor::Operator).expect("paused");
    revoke(&mut p, &creator(), later(1)).expect("revoked");
    assert!(p.is_revoked() && p.is_paused());
}

#[test]
fn revoking_is_for_the_owner_and_the_operator() {
    let base = signed_in_link();
    for actor in [creator(), manager(), Actor::Operator] {
        let mut r = base.clone();
        assert_eq!(revoke(&mut r, &actor, NOW), Ok(()), "{actor:?}");
        assert!(r.is_revoked());
    }
    for actor in [writer(), stranger(), Actor::Reports] {
        let mut r = base.clone();
        assert_eq!(
            revoke(&mut r, &actor, NOW),
            Err(UpdateRefusal::NotPermitted),
            "{actor:?}"
        );
        assert!(!r.is_revoked());
    }
}

#[test]
fn repoint_only_for_folder_live_by_a_writer() {
    let live = live_link();
    assert!(live.follow_latest());

    // A writer re-points the live folder: the manifest and the grants change, nothing else.
    for actor in [creator(), manager(), writer()] {
        let mut r = live.clone();
        assert_eq!(
            repoint(&mut r, object(70), grants(3), &actor),
            Ok(()),
            "{actor:?}"
        );
        assert_eq!(r.manifest_object(), &object(70));
        assert_eq!(r.grants(), &grants(3));
        let mut row = r.row().clone();
        row.manifest_object = *live.manifest_object();
        row.grants = live.grants().clone();
        assert_eq!(&row, live.row(), "{actor:?}");
        assert_eq!(r.check(), Ok(()));
    }
    // Again at each new version, and a live folder may point at nothing for now.
    let mut r = live.clone();
    repoint(&mut r, object(71), grants(1), &writer()).expect("version 2");
    repoint(&mut r, object(72), grants(0), &writer()).expect("version 3");
    assert_eq!(r.manifest_object(), &object(72));
    assert!(r.grants().is_empty());
    // Pointing at what it points at is no change.
    let before = r.clone();
    assert_eq!(repoint(&mut r, object(72), grants(0), &writer()), Ok(()));
    assert_eq!(r, before);

    // Not a writer: the member without the right to write, the reports, the operator.
    for actor in [
        stranger(),
        creator_without_write(),
        Actor::Reports,
        Actor::Operator,
    ] {
        let mut r = live.clone();
        assert_eq!(
            repoint(&mut r, object(70), grants(3), &actor),
            Err(UpdateRefusal::NotPermitted),
            "{actor:?}"
        );
        assert_eq!(r, live, "{actor:?}");
    }

    // Only a live folder: every other kind is changed by a rotation.
    for kind in [
        LinkKind::File,
        LinkKind::Selection,
        LinkKind::FolderSnapshot,
        LinkKind::Version,
    ] {
        let base = build(LinkOptions::new(Audience::SignedIn, kind));
        let mut r = base.clone();
        assert_eq!(
            repoint(&mut r, object(70), grants(3), &writer()),
            Err(UpdateRefusal::NotLive),
            "{kind}"
        );
        assert_eq!(r, base, "{kind}");
    }
}

#[test]
fn only_the_creator_or_a_manager_changes_a_link() {
    let base = people_link();
    let policy = open();
    // The creator, a manager who is not the creator, and the creator who lost the right to write.
    for actor in [creator(), manager(), creator_without_write()] {
        let mut r = base.clone();
        assert_eq!(
            update(&mut r, Change::AppOnly(true), &actor, &policy, NOW),
            Ok(()),
            "{actor:?}"
        );
        assert!(r.options().app_only);

        let mut r = base.clone();
        let key = same_key(&r);
        assert!(
            rotate(
                &mut r,
                &mut TestEntropy::new(1),
                key,
                &actor,
                &policy,
                later(1)
            )
            .is_ok(),
            "{actor:?}"
        );
        let mut r = base.clone();
        assert!(pause(&mut r, &actor).is_ok());
        assert!(unpause(&mut r, &actor).is_ok());
        assert!(revoke(&mut r, &actor, NOW).is_ok());
    }
    // Nobody else: a writer who is not the creator, a stranger, the reports, the operator.
    for actor in [writer(), stranger(), Actor::Reports, Actor::Operator] {
        let mut r = base.clone();
        assert_eq!(
            update(&mut r, Change::AppOnly(true), &actor, &policy, NOW),
            Err(UpdateRefusal::NotPermitted),
            "{actor:?}"
        );
        assert_eq!(r, base, "{actor:?}");
        let key = same_key(&r);
        assert_eq!(
            rotate(
                &mut r,
                &mut TestEntropy::new(1),
                key,
                &actor,
                &policy,
                later(1)
            ),
            Err(UpdateRefusal::NotPermitted),
            "{actor:?}"
        );
        assert_eq!(r, base, "{actor:?}");
    }
    for actor in [writer(), stranger()] {
        let mut r = base.clone();
        assert_eq!(pause(&mut r, &actor), Err(UpdateRefusal::NotPermitted));
        assert_eq!(unpause(&mut r, &actor), Err(UpdateRefusal::NotPermitted));
        assert_eq!(
            revoke(&mut r, &actor, NOW),
            Err(UpdateRefusal::NotPermitted)
        );
        assert_eq!(r, base);
    }
    // Every change a member without the right could ask is refused, whatever it is.
    for (_, change, _) in server_option_cases() {
        let mut r = base.clone();
        assert_eq!(
            update(&mut r, change, &stranger(), &policy, NOW),
            Err(UpdateRefusal::NotPermitted)
        );
        assert_eq!(r, base);
    }
}

#[test]
fn update_cannot_undo_what_the_policy_forces() {
    let policy = LinkPolicy {
        force_watermark: true,
        force_preview_only: true,
        force_app_only: true,
        ..open()
    };
    let mut r = build_under(opts(Audience::SignedIn), &policy);
    assert!(r.options().watermark && r.options().app_only);
    assert_eq!(r.options().rights, Rights::PreviewOnly);

    let before = r.clone();
    for (change, option) in [
        (Change::Watermark(false), OptionName::Watermark),
        (Change::Rights(Rights::Download), OptionName::Rights),
        (Change::AppOnly(false), OptionName::AppOnly),
    ] {
        assert_eq!(
            update(&mut r, change.clone(), &creator(), &policy, NOW),
            Err(UpdateRefusal::Policy(vec![PolicyViolation::Forced(option)])),
            "{change:?}"
        );
        assert_eq!(r, before);
    }
    // What the policy forces may be set again, and the other options still change.
    assert_eq!(
        update(&mut r, Change::Watermark(true), &creator(), &policy, NOW),
        Ok(())
    );
    assert_eq!(
        update(
            &mut r,
            Change::Notify(Notify::FirstOpen),
            &creator(),
            &policy,
            NOW
        ),
        Ok(())
    );
}

#[test]
fn update_keeps_to_the_policys_bounds() {
    let policy = LinkPolicy {
        max_expiry: Some(Span::days(30)),
        uploads: false,
        audiences: AudienceSet::NONE
            .with(AudienceClass::SignedIn)
            .with(AudienceClass::Members)
            .with(AudienceClass::People),
        ..open()
    };
    let mut o = opts(Audience::SignedIn);
    o.expires_at = Some(days(10));
    let mut r = build_under(o, &policy);
    let before = r.clone();
    let set = |r: &mut LinkRecord, change: Change| update(r, change, &creator(), &policy, NOW);

    // The longest life counts from the creation.
    assert_eq!(set(&mut r, Change::ExpiresAt(Some(days(30)))), Ok(()));
    assert_eq!(
        set(&mut r, Change::ExpiresAt(Some(later(30 * 86_400 + 1)))),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::ExpiryTooLate { latest: days(30) }
        ]))
    );
    assert_eq!(
        set(&mut r, Change::ExpiresAt(None)),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::ExpiryRequired {
                longest: Span::days(30)
            }
        ]))
    );
    assert_eq!(r.options().expires_at, Some(days(30)));
    // No uploads under this policy.
    let mut with_request = r.clone();
    apply(&mut with_request, Change::PairRequest(request(7))).expect("pair");
    assert_eq!(
        set(&mut with_request, Change::AllowUpload(true)),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::UploadsNotAllowed
        ]))
    );
    // An audience the policy does not allow, for a person added too.
    let mut o2 = opts(people(&[10]));
    o2.expires_at = Some(days(10));
    let mut r2 = build_under(o2, &policy);
    assert_eq!(set(&mut r2, Change::AddWrap(wrap(11))), Ok(()));
    let strict = LinkPolicy {
        audiences: AudienceSet::NONE.with(AudienceClass::SignedIn),
        ..open()
    };
    assert_eq!(
        update(&mut r2, Change::AddWrap(wrap(12)), &creator(), &strict, NOW),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::AudienceNotAllowed(AudienceClass::People)
        ]))
    );
    assert_ne!(r, before);
}

#[test]
fn a_change_is_judged_by_the_options_it_touches() {
    // A link made while uploads were allowed keeps changing after the policy forbids them: only a
    // change of the upload option meets the policy.
    let mut o = opts(Audience::SignedIn);
    o.allow_upload = true;
    let mut r = build(o);
    let forbids = LinkPolicy {
        uploads: false,
        ..open()
    };
    assert_eq!(
        update(
            &mut r,
            Change::Notify(Notify::EveryOpen),
            &creator(),
            &forbids,
            NOW
        ),
        Ok(())
    );
    // Turning uploads off is fine; turning them back on is not.
    assert_eq!(
        update(
            &mut r,
            Change::AllowUpload(false),
            &creator(),
            &forbids,
            NOW
        ),
        Ok(())
    );
    assert_eq!(
        update(&mut r, Change::AllowUpload(true), &creator(), &forbids, NOW),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::UploadsNotAllowed
        ]))
    );
    // A link that is already past the policy's longest life can still change something else.
    let mut o = opts(Audience::SignedIn);
    o.expires_at = Some(days(90));
    let mut r = build(o);
    let short = LinkPolicy {
        max_expiry: Some(Span::days(30)),
        ..open()
    };
    assert_eq!(
        update(&mut r, Change::AppOnly(true), &creator(), &short, NOW),
        Ok(())
    );
    assert_eq!(
        update(
            &mut r,
            Change::ExpiresAt(Some(days(89))),
            &creator(),
            &short,
            NOW
        ),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::ExpiryTooLate { latest: days(30) }
        ]))
    );
}

#[test]
fn rotation_keeps_to_the_policy_of_the_key_bound_options() {
    let base = build(opts(Audience::SignedIn));
    let no_people = LinkPolicy {
        audiences: AudienceSet::ALL.without(AudienceClass::People),
        ..open()
    };
    let to = KeyBound {
        audience: people(&[10]),
        wraps: vec![wrap(10)],
        ..same_key(&base)
    };
    let mut r = base.clone();
    assert_eq!(
        rotate(
            &mut r,
            &mut TestEntropy::new(1),
            to,
            &creator(),
            &no_people,
            later(1)
        ),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::AudienceNotAllowed(AudienceClass::People)
        ]))
    );
    assert_eq!(r, base);
    // A public link without a password, under a policy that asks one.
    let needs_password = LinkPolicy {
        password_for_anyone: true,
        ..open()
    };
    let to = KeyBound {
        audience: Audience::Anyone,
        ..same_key(&base)
    };
    assert_eq!(
        rotate(
            &mut r,
            &mut TestEntropy::new(1),
            to,
            &creator(),
            &needs_password,
            later(1)
        ),
        Err(UpdateRefusal::Policy(vec![
            PolicyViolation::PasswordRequiredForAnyone
        ]))
    );
    assert_eq!(r, base);
}

#[test]
fn every_refusal_has_a_message() {
    for refusal in [
        UpdateRefusal::NotPermitted,
        UpdateRefusal::Revoked,
        UpdateRefusal::Frozen(PausedBy::Operator),
        UpdateRefusal::RotationRequired(RotationReason::Audience),
        UpdateRefusal::Invalid(RecordError::FactsChanged),
        UpdateRefusal::Policy(Vec::new()),
        UpdateRefusal::NotLive,
        UpdateRefusal::NoSuchWrap,
        UpdateRefusal::AlreadyPaired,
        UpdateRefusal::AlreadyWrapped,
        UpdateRefusal::Entropy(EntropyError::Os),
        UpdateRefusal::SameId,
    ] {
        assert!(!refusal.to_string().is_empty(), "{refusal:?}");
    }
    for error in [
        RecordError::FactsChanged,
        RecordError::DuplicateWrap(member(1)),
        RecordError::WrapKeyMismatch(member(1)),
        RecordError::WrapsAreNotThePeople,
        RecordError::KeyedBeforeCreated,
        RecordError::Options(vec![OptionError::PeopleEmpty]),
    ] {
        assert!(!error.to_string().is_empty(), "{error:?}");
    }
    assert_eq!(link_id(1), link_id(1));
}

/// A random value from a small pool.
fn below(rng: &mut Rng, n: u64) -> u64 {
    rng.below(n).expect("a pool is never empty")
}

/// A random stamp around `NOW`: mostly ahead, sometimes behind.
fn random_stamp(rng: &mut Rng) -> Stamp {
    let secs = below(rng, 90 * 86_400);
    if rng.chance(1, 5) {
        NOW.checked_sub(Span::secs(secs)).expect("after the epoch")
    } else {
        later(secs)
    }
}

/// A random span, zero included.
fn random_span(rng: &mut Rng) -> Span {
    Span::secs(below(rng, 400 * 86_400))
}

/// With probability 1/3 `None`.
fn maybe<T>(rng: &mut Rng, value: impl FnOnce(&mut Rng) -> T) -> Option<T> {
    if rng.chance(1, 3) {
        None
    } else {
        Some(value(rng))
    }
}

/// A random change of every kind, over small pools so that equal values and refusals are common.
fn random_change(rng: &mut Rng) -> Change {
    match below(rng, 29) {
        0 => Change::AppOnly(rng.chance(1, 2)),
        1 => Change::NotBefore(maybe(rng, random_stamp)),
        2 => Change::ExpiresAt(maybe(rng, random_stamp)),
        3 => Change::ExpiresAfterOpen(maybe(rng, random_span)),
        4 => Change::IdleExpiry(maybe(rng, random_span)),
        5 => Change::OneTime(rng.chance(1, 2)),
        6 => Change::MaxDownloads(maybe(rng, |r| [0, 1, 5, u32::MAX][below(r, 4) as usize])),
        7 => Change::BytesCap(maybe(rng, |r| ByteCount::new(below(r, 3)))),
        8 => Change::MaxDevices(maybe(rng, |r| [0, 1, 9][below(r, 3) as usize])),
        9 => Change::Rights(if rng.chance(1, 2) {
            Rights::Download
        } else {
            Rights::PreviewOnly
        }),
        10 => Change::AllowUpload(rng.chance(1, 2)),
        11 => Change::Edit(rng.chance(1, 2)),
        12 => Change::Comment(rng.chance(1, 2)),
        13 => Change::AllowReshare(rng.chance(1, 2)),
        14 => Change::Watermark(rng.chance(1, 2)),
        15 => Change::Notify(Notify::ALL[below(rng, 3) as usize]),
        16 => Change::OpenLog(rng.chance(1, 2)),
        17 => Change::Title(maybe(rng, |r| {
            sealed(1, [28, 100, 512, 513][below(r, 4) as usize])
        })),
        18 => Change::Message(maybe(rng, |r| {
            sealed(2, [28, 4096, 4097][below(r, 3) as usize])
        })),
        19 => Change::Slug(maybe(rng, |r| {
            Slug::parse(["a-name", "other-name", "third"][below(r, 3) as usize]).expect("a slug")
        })),
        20 => Change::PairRequest(request(below(rng, 4) as u8)),
        21 => Change::PairCommentLog(log(below(rng, 4) as u8)),
        22 => Change::AddWrap(wrap(10 + below(rng, 6) as u8)),
        23 => Change::Audience(match below(rng, 6) {
            0 => Audience::Anyone,
            1 => Audience::SignedIn,
            2 => groups(&[1, 2][..below(rng, 3) as usize]),
            3 => people(&[10, 11, 12][..1 + below(rng, 3) as usize]),
            4 => people(&[10]),
            _ => Audience::Approval,
        }),
        24 => Change::RemoveWrap(member(10 + below(rng, 6) as u8)),
        25 => Change::Password(maybe(rng, |r| opaque(below(r, 3) as u8))),
        26 => Change::Kind(LinkKind::ALL[below(rng, 5) as usize]),
        27 => Change::Subfolders(rng.chance(1, 2)),
        _ => Change::Grants(grants(below(rng, 3) as usize)),
    }
}

/// A random actor: mostly the creator, sometimes someone who may not.
fn random_actor(rng: &mut Rng) -> Actor {
    match below(rng, 10) {
        0 => stranger(),
        1 => writer(),
        2 => manager(),
        _ => creator(),
    }
}

/// A random policy: mostly none, sometimes one that bounds the options the changes touch.
fn random_policy(rng: &mut Rng) -> LinkPolicy {
    match below(rng, 6) {
        0 => LinkPolicy {
            uploads: false,
            edit: false,
            ..open()
        },
        1 => LinkPolicy {
            force_watermark: true,
            force_preview_only: true,
            ..open()
        },
        2 => LinkPolicy {
            max_expiry: Some(Span::days(30)),
            slugs: false,
            ..open()
        },
        3 => LinkPolicy {
            audiences: AudienceSet::NONE
                .with(AudienceClass::SignedIn)
                .with(AudienceClass::People),
            ..open()
        },
        _ => open(),
    }
}

#[test]
fn random_changes_keep_every_invariant_and_refusals_change_nothing() {
    let mut accepted = 0usize;
    let mut refused = 0usize;
    let mut kinds_accepted = std::collections::BTreeSet::new();
    for seed in 0..25u64 {
        let mut rng = Rng::new(seed);
        let mut r = paired_people_link();
        for step in 0..300u64 {
            let before = r.clone();
            let change = random_change(&mut rng);
            let change_name = format!("{change:?}");
            let change_kind = change_name
                .split(['(', ' '])
                .next()
                .unwrap_or_default()
                .to_string();
            let actor = random_actor(&mut rng);
            let policy = random_policy(&mut rng);
            let now = later(step);
            let (name, result) = match below(&mut rng, 12) {
                // Now and then the node pauses, lifts or rotates instead.
                0 => {
                    let by = [Actor::Operator, Actor::Reports, creator()]
                        [below(&mut rng, 3) as usize]
                        .clone();
                    ("pause".to_string(), pause(&mut r, &by).map(|_| ()))
                }
                1 => {
                    let by = [Actor::Operator, creator()][below(&mut rng, 2) as usize].clone();
                    ("unpause".to_string(), unpause(&mut r, &by).map(|_| ()))
                }
                2 => {
                    let key = same_key(&r);
                    let mut entropy = TestEntropy::new(seed * 1_000 + step);
                    (
                        "rotate".to_string(),
                        rotate(&mut r, &mut entropy, key, &actor, &policy, now).map(|_| ()),
                    )
                }
                _ => (change_kind, update(&mut r, change, &actor, &policy, now)),
            };
            // Whatever happened, the record keeps every invariant, and a row read back is the record.
            assert_eq!(r.check(), Ok(()), "seed {seed} step {step}: {name}");
            assert_eq!(
                LinkRecord::from_row(r.row().clone()).as_ref(),
                Ok(&r),
                "seed {seed} step {step}"
            );
            match result {
                Ok(()) => {
                    if r != before {
                        accepted += 1;
                        kinds_accepted.insert(name);
                    }
                }
                Err(refusal) => {
                    refused += 1;
                    // A refused change leaves the record exactly as it was.
                    assert_eq!(r, before, "seed {seed} step {step}: {name}: {refusal}");
                }
            }
            // The created time never moves; the keyed time never goes back.
            assert_eq!(r.created_at(), before.created_at());
            assert!(r.keyed_at() >= before.keyed_at());
        }
    }
    // The generator exercises both: many changes go through, many are refused.
    assert!(accepted > 800, "only {accepted} changes went through");
    assert!(refused > 800, "only {refused} changes were refused");
    assert!(
        kinds_accepted.len() >= 15,
        "only these kinds of change went through: {kinds_accepted:?}"
    );
}
