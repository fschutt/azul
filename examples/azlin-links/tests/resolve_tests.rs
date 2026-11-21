//! Check 6: resolving, one test per rule (planning/architecture.md section 5.12 "Resolving": "checks
//! every server option (dates, counts, devices, `paused`, blocks, the audience)"), with the rules the
//! table of `resolve.rs` adds: the id the link answers to, the order of the rules, and what a
//! stranger is told. Every expectation is typed from the plan's sentence, not computed by the code
//! under test.
//!
//! Implements planning/architecture.md section 5.12 ("Resolving", "Links used as a CDN") and section
//! 3.9 (`links_disabled`).

#![allow(dead_code)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use azlin_links::counters::{admit_bytes, count_open, OpenRefusal};
use azlin_links::resolve::{
    password_login_needed, resolve, Asked, Caller, ExpiredBy, ResolveCtx, ResolveRefusal, Resolved,
};
use azlin_links::{
    pause, revoke, rotate, unpause, update, Actor, Audience, Change, DeviceToken, KeyBound,
    LinkEnded, LinkRecord, LinkRow, PausedBy, Pauses, PublicLinkId, RequestId, Slug,
};
use azlin_policy::LinkPolicy;
use azlin_seams::sim::Rng;
use azlin_seams::{AccountId, ByteCount, Span, Stamp};
use common::{
    build, creator, group, groups, link_id, member, object, opts, people, request, wrap,
    TestEntropy, NOW,
};

/// Seconds in a day.
const DAY_SECS: u64 = 86_400;

/// No link is blocked.
static NO_BLOCKS: BTreeSet<PublicLinkId> = BTreeSet::new();

/// The stamp `secs` seconds after `NOW`.
fn later(secs: u64) -> Stamp {
    NOW.checked_add(Span::secs(secs)).expect("no overflow")
}

/// The context of a plain request for `link` by `caller` at `NOW`: nothing blocked, no device,
/// nothing served today, a big daily cap, no login, not the human step.
fn ctx_for(link: &LinkRecord, caller: Caller) -> ResolveCtx<'static> {
    ResolveCtx {
        asked: Asked::Id(*link.id()),
        now: NOW,
        caller,
        device: None,
        blocks: &NO_BLOCKS,
        links_disabled: false,
        served_today: ByteCount::ZERO,
        daily_cap: ByteCount::gib(100),
        password_verified: false,
        human_step: false,
    }
}

/// The same request `secs` seconds after `NOW`.
fn at(link: &LinkRecord, caller: Caller, secs: u64) -> ResolveCtx<'static> {
    ResolveCtx {
        now: later(secs),
        ..ctx_for(link, caller)
    }
}

/// Nobody is signed in.
fn anon() -> Caller {
    Caller::Anonymous
}

/// A member of another account.
fn signed_in(n: u8) -> Caller {
    Caller::SignedIn {
        account: AccountId::from_bytes([200; 16]),
        member: member(n),
    }
}

/// A member of the link's own account, in the groups `group_ids`.
fn account_member(n: u8, group_ids: &[u8]) -> Caller {
    Caller::Member {
        account: AccountId::from_bytes([1; 16]),
        member: member(n),
        groups: group_ids.iter().map(|g| group(*g)).collect(),
    }
}

/// The row of `link` changed by `change`, read back as a record (every invariant is checked).
fn with_row(link: &LinkRecord, change: impl FnOnce(&mut LinkRow)) -> LinkRecord {
    let mut row = link.row().clone();
    change(&mut row);
    LinkRecord::from_row(row).expect("a valid row")
}

/// A link for anyone.
fn anyone_link() -> LinkRecord {
    build(opts(Audience::Anyone))
}

/// The policy that bounds nothing.
fn open_policy() -> LinkPolicy {
    LinkPolicy::allow_all()
}

/// The key-bound parts a link has now, with a NEW paired request where it has one.
fn same_key(r: &LinkRecord) -> KeyBound {
    KeyBound {
        audience: r.options().audience.clone(),
        kind: r.kind(),
        subfolders: r.options().subfolders,
        manifest_object: *r.manifest_object(),
        grants: r.grants().clone(),
        opaque_record: r.opaque_record().cloned(),
        wraps: r.wraps().cloned().collect(),
        request: r
            .request_id()
            .map(|old| RequestId::from_bytes([old.as_bytes()[0].wrapping_add(100); 16])),
    }
}

#[test]
fn not_before_in_the_future() {
    let mut options = opts(Audience::Anyone);
    options.not_before = Some(later(3_600));
    let link = build(options);
    let wait = ResolveRefusal::NotYetOpen {
        opens_at: later(3_600),
    };
    assert_eq!(resolve(&link, &at(&link, anon(), 0)).unwrap_err(), wait);
    assert_eq!(resolve(&link, &at(&link, anon(), 3_599)).unwrap_err(), wait);
    assert!(
        resolve(&link, &at(&link, anon(), 3_600)).is_ok(),
        "the link opens AT its date"
    );
    assert!(resolve(&link, &at(&link, anon(), 90_000)).is_ok());
}

#[test]
fn expires_at_passed() {
    let mut options = opts(Audience::Anyone);
    options.expires_at = Some(later(DAY_SECS));
    let link = build(options);
    assert!(resolve(&link, &at(&link, anon(), DAY_SECS - 1)).is_ok());
    for secs in [DAY_SECS, DAY_SECS + 1, 10 * DAY_SECS] {
        assert_eq!(
            resolve(&link, &at(&link, anon(), secs)).unwrap_err(),
            ResolveRefusal::Expired {
                by: ExpiredBy::Date
            },
            "at {secs} seconds the link has ended: it ends AT its date"
        );
    }
}

#[test]
fn expires_after_open_counts_from_the_first_open() {
    let mut options = opts(Audience::Anyone);
    options.expires_after_open = Some(Span::hours(1));
    let link = build(options);
    assert!(
        resolve(&link, &at(&link, anon(), 400 * DAY_SECS)).is_ok(),
        "the clock starts at the first open: a link nobody opened does not expire"
    );
    let opened = with_row(&link, |row| {
        row.counters.first_open_at = Some(later(1_000));
        row.counters.last_open_at = Some(later(1_000));
    });
    assert!(resolve(&opened, &at(&opened, anon(), 1_000 + 3_599)).is_ok());
    for secs in [1_000 + 3_600, 1_000 + 7_200] {
        assert_eq!(
            resolve(&opened, &at(&opened, anon(), secs)).unwrap_err(),
            ResolveRefusal::Expired {
                by: ExpiredBy::AfterOpen
            }
        );
    }
}

#[test]
fn idle_expiry_counts_days_without_an_open() {
    let mut options = opts(Audience::Anyone);
    options.idle_expiry = Some(Span::days(7));
    let link = build(options);
    assert!(resolve(&link, &at(&link, anon(), 7 * DAY_SECS - 1)).is_ok());
    assert_eq!(
        resolve(&link, &at(&link, anon(), 7 * DAY_SECS)).unwrap_err(),
        ResolveRefusal::Expired {
            by: ExpiredBy::Idle
        }
    );
    // An open on day 6 starts the idle time again.
    let opened = with_row(&link, |row| {
        row.counters.first_open_at = Some(later(6 * DAY_SECS));
        row.counters.last_open_at = Some(later(6 * DAY_SECS));
    });
    assert!(resolve(&opened, &at(&opened, anon(), 13 * DAY_SECS - 1)).is_ok());
    assert_eq!(
        resolve(&opened, &at(&opened, anon(), 13 * DAY_SECS)).unwrap_err(),
        ResolveRefusal::Expired {
            by: ExpiredBy::Idle
        }
    );
}

#[test]
fn idle_expiry_counts_from_the_last_rotation_for_a_link_never_opened() {
    let mut options = opts(Audience::Anyone);
    options.idle_expiry = Some(Span::days(7));
    let link = build(options);
    // Created at NOW, keyed again 30 days later (a rotation): the idle time counts from the key.
    let rotated = with_row(&link, |row| row.keyed_at = later(30 * DAY_SECS));
    assert!(resolve(&rotated, &at(&rotated, anon(), 31 * DAY_SECS)).is_ok());
    assert!(resolve(&rotated, &at(&rotated, anon(), 37 * DAY_SECS - 1)).is_ok());
    assert_eq!(
        resolve(&rotated, &at(&rotated, anon(), 37 * DAY_SECS)).unwrap_err(),
        ResolveRefusal::Expired {
            by: ExpiredBy::Idle
        }
    );
}

#[test]
fn one_time_used() {
    let mut options = opts(Audience::Anyone);
    options.one_time = true;
    let link = build(options);
    assert!(resolve(&link, &ctx_for(&link, anon())).is_ok());
    let used = with_row(&link, |row| {
        row.counters.downloads = 1;
        row.revoked_at = Some(later(5));
    });
    assert_eq!(
        resolve(&used, &ctx_for(&used, anon())).unwrap_err(),
        ResolveRefusal::OneTimeUsed
    );
    // A row that counted the download and was not revoked (it fell behind): still used up.
    let counted = with_row(&link, |row| row.counters.downloads = 1);
    assert_eq!(
        resolve(&counted, &ctx_for(&counted, anon())).unwrap_err(),
        ResolveRefusal::OneTimeUsed
    );
}

#[test]
fn max_downloads_exhausted() {
    let mut options = opts(Audience::Anyone);
    options.max_downloads = Some(3);
    let link = build(options);
    for (downloads, open) in [(0u64, true), (2, true), (3, false), (4, false)] {
        let state = with_row(&link, |row| row.counters.downloads = downloads);
        let answer = resolve(&state, &ctx_for(&state, anon()));
        if open {
            assert!(answer.is_ok(), "{downloads} of 3 downloads");
        } else {
            assert_eq!(
                answer.unwrap_err(),
                ResolveRefusal::DownloadsExhausted,
                "{downloads} of 3 downloads"
            );
        }
    }
}

#[test]
fn bytes_cap_exhausted_and_admit_bytes_truncates() {
    let mut options = opts(Audience::Anyone);
    options.bytes_cap = Some(ByteCount::mib(10));
    let link = build(options);
    let served = |mib: u32| with_row(&link, |row| row.counters.bytes_served = ByteCount::mib(mib));

    let four = served(4);
    assert!(resolve(&four, &ctx_for(&four, anon())).is_ok());
    let c = ctx_for(&four, anon());
    assert_eq!(
        admit_bytes(&four, ByteCount::mib(8), &c),
        ByteCount::mib(6),
        "10 MiB cap, 4 MiB served: 6 MiB are left"
    );
    assert_eq!(admit_bytes(&four, ByteCount::mib(5), &c), ByteCount::mib(5));
    assert_eq!(admit_bytes(&four, ByteCount::ZERO, &c), ByteCount::ZERO);

    for mib in [10, 11] {
        let full = served(mib);
        assert_eq!(
            resolve(&full, &ctx_for(&full, anon())).unwrap_err(),
            ResolveRefusal::BytesCapExhausted
        );
        assert_eq!(
            admit_bytes(&full, ByteCount::kib(1), &ctx_for(&full, anon())),
            ByteCount::ZERO,
            "a link whose cap is used serves nothing more"
        );
    }
}

#[test]
fn max_devices_refuses_the_n_plus_first_token_and_passes_known_ones() {
    let mut options = opts(Audience::Anyone);
    options.max_devices = Some(2);
    let mut link = build(options);
    let token = |n: u8| DeviceToken::from_bytes([n; 16]);
    let with_device = |link: &LinkRecord, n: u8| ResolveCtx {
        device: Some(token(n)),
        human_step: true,
        ..ctx_for(link, anon())
    };

    // A link that counts devices needs the client's token.
    assert_eq!(
        resolve(&link, &ctx_for(&link, anon())).unwrap_err(),
        ResolveRefusal::DeviceTokenNeeded
    );

    // Two devices open it.
    for n in [1u8, 2] {
        let c = with_device(&link, n);
        assert!(resolve(&link, &c).is_ok());
        count_open(&mut link, &c, false).expect("counted");
    }
    assert_eq!(link.counters().devices_seen.len(), 2);

    // The third is refused, at the resolve and at the human step, and nothing is added.
    let c3 = with_device(&link, 3);
    assert_eq!(
        resolve(&link, &c3).unwrap_err(),
        ResolveRefusal::DeviceLimit
    );
    assert_eq!(
        count_open(&mut link, &c3, false).unwrap_err(),
        OpenRefusal::Link(ResolveRefusal::DeviceLimit)
    );
    assert_eq!(link.counters().devices_seen.len(), 2);

    // The known ones still pass, again and again.
    for n in [1u8, 2, 1, 2] {
        let c = with_device(&link, n);
        assert!(resolve(&link, &c).is_ok(), "device {n} is known");
        count_open(&mut link, &c, false).expect("a known device opens again");
    }
    assert_eq!(link.counters().devices_seen.len(), 2);
}

#[test]
fn devices_are_kept_only_for_a_link_that_counts_them() {
    let mut link = anyone_link();
    let c = ResolveCtx {
        device: Some(DeviceToken::from_bytes([9; 16])),
        human_step: true,
        ..ctx_for(&link, anon())
    };
    count_open(&mut link, &c, false).expect("counted");
    assert!(
        link.counters().devices_seen.is_empty(),
        "a link without max_devices keeps no device hash"
    );
}

#[test]
fn paused() {
    let mut link = anyone_link();
    pause(&mut link, &creator()).expect("the owner pauses");
    assert_eq!(
        resolve(&link, &ctx_for(&link, anon())).unwrap_err(),
        ResolveRefusal::Paused
    );
    unpause(&mut link, &creator()).expect("the owner lifts its pause");
    assert!(resolve(&link, &ctx_for(&link, anon())).is_ok());

    // The reports and the operator pause as well, and the owner cannot lift theirs.
    for by in [Actor::Reports, Actor::Operator] {
        let mut link = anyone_link();
        pause(&mut link, &by).expect("paused");
        assert_eq!(
            resolve(&link, &ctx_for(&link, anon())).unwrap_err(),
            ResolveRefusal::Paused
        );
        let still = unpause(&mut link, &creator()).expect("the owner lifts only its own pause");
        assert!(still.is_paused(), "the pause of {by:?} stays");
        assert_eq!(
            resolve(&link, &ctx_for(&link, anon())).unwrap_err(),
            ResolveRefusal::Paused
        );
    }
}

#[test]
fn revoked() {
    let mut link = anyone_link();
    revoke(&mut link, &creator(), later(5)).expect("revoked");
    assert_eq!(
        resolve(&link, &ctx_for(&link, anon())).unwrap_err(),
        ResolveRefusal::Revoked
    );
}

#[test]
fn blocked_by_ctl() {
    let link = anyone_link();
    let blocks: BTreeSet<PublicLinkId> = [*link.id()].into_iter().collect();
    let c = ResolveCtx {
        blocks: &blocks,
        ..ctx_for(&link, anon())
    };
    assert_eq!(resolve(&link, &c).unwrap_err(), ResolveRefusal::Blocked);

    let other: BTreeSet<PublicLinkId> = [link_id(99)].into_iter().collect();
    let c = ResolveCtx {
        blocks: &other,
        ..ctx_for(&link, anon())
    };
    assert!(
        resolve(&link, &c).is_ok(),
        "the block of another link touches nothing"
    );
}

#[test]
fn links_disabled_pauses_anyone_links_only_named_people_keep_working() {
    // (3.9: "its public links pause ... sharing with named people keeps working".)
    let with_disabled = |link: &LinkRecord, caller: Caller| ResolveCtx {
        links_disabled: true,
        ..ctx_for(link, caller)
    };

    let anyone = anyone_link();
    assert_eq!(
        resolve(&anyone, &with_disabled(&anyone, anon())).unwrap_err(),
        ResolveRefusal::PublicLinksOff
    );
    assert_eq!(
        resolve(&anyone, &with_disabled(&anyone, signed_in(10))).unwrap_err(),
        ResolveRefusal::PublicLinksOff,
        "a public link is paused for everyone"
    );
    assert!(
        resolve(&anyone, &ctx_for(&anyone, anon())).is_ok(),
        "the switch is off for an account that has kept its public links"
    );

    let named = build(opts(people(&[10, 11])));
    assert!(resolve(&named, &with_disabled(&named, signed_in(10))).is_ok());

    let signed = build(opts(Audience::SignedIn));
    assert!(resolve(&signed, &with_disabled(&signed, signed_in(5))).is_ok());

    let members = build(opts(groups(&[1, 2])));
    assert!(resolve(&members, &with_disabled(&members, account_member(20, &[1]))).is_ok());

    let mut approval = build(opts(Audience::Approval));
    update(
        &mut approval,
        Change::AddWrap(wrap(77)),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("approved");
    assert!(resolve(&approval, &with_disabled(&approval, signed_in(77))).is_ok());
}

#[test]
fn daily_cap_refuses_with_retry_after_the_next_day_start() {
    let link = anyone_link();
    let cap = ByteCount::gib(5);
    let capped = ResolveCtx {
        served_today: cap,
        daily_cap: cap,
        ..ctx_for(&link, anon())
    };
    let next_day = (NOW.as_secs() / DAY_SECS + 1) * DAY_SECS;
    let refusal = resolve(&link, &capped).unwrap_err();
    match refusal {
        ResolveRefusal::DailyCapReached { retry_after } => {
            assert_eq!(
                NOW.checked_add(retry_after),
                Some(Stamp::from_secs(next_day)),
                "retry at the next day start (UTC midnight)"
            );
            assert!(retry_after >= Span::secs(1));
            assert!(retry_after <= Span::days(1));
        }
        other => panic!("expected the daily cap, got {other:?}"),
    }
    assert_eq!(
        refusal.retry_after(),
        Some(Span::secs(next_day - NOW.as_secs()))
    );

    // Under the cap the link opens; a cap of zero refuses every open.
    let under = ResolveCtx {
        served_today: ByteCount::gib(4),
        daily_cap: cap,
        ..ctx_for(&link, anon())
    };
    assert!(resolve(&link, &under).is_ok());
    let none = ResolveCtx {
        daily_cap: ByteCount::ZERO,
        ..ctx_for(&link, anon())
    };
    assert!(matches!(
        resolve(&link, &none).unwrap_err(),
        ResolveRefusal::DailyCapReached { .. }
    ));

    // At a day start the wait is a whole day.
    let midnight = ResolveCtx {
        now: Stamp::from_secs(next_day),
        ..capped.clone()
    };
    assert_eq!(
        resolve(&link, &midnight).unwrap_err(),
        ResolveRefusal::DailyCapReached {
            retry_after: Span::days(1)
        }
    );

    // The cap is a public link's (3.9, 5.12 "Links used as a CDN"): a link for signed-in accounts has
    // none.
    let signed = build(opts(Audience::SignedIn));
    let c = ResolveCtx {
        served_today: cap,
        daily_cap: cap,
        ..ctx_for(&signed, signed_in(5))
    };
    assert!(resolve(&signed, &c).is_ok());
}

#[test]
fn admit_bytes_stops_at_the_daily_cap_of_a_public_link() {
    let link = anyone_link();
    let c = ResolveCtx {
        served_today: ByteCount::gib(4),
        daily_cap: ByteCount::gib(5),
        ..ctx_for(&link, anon())
    };
    assert_eq!(
        admit_bytes(&link, ByteCount::gib(3), &c),
        ByteCount::gib(1),
        "one GiB of the day's cap is left"
    );
    let used = ResolveCtx {
        served_today: ByteCount::gib(5),
        ..c.clone()
    };
    assert_eq!(
        admit_bytes(&link, ByteCount::kib(1), &used),
        ByteCount::ZERO
    );
}

#[test]
fn password_audience_needs_login_first() {
    let mut options = opts(Audience::Anyone);
    options.password = true;
    let link = build(options);
    assert!(password_login_needed(&link));
    assert_eq!(
        resolve(&link, &ctx_for(&link, anon())).unwrap_err(),
        ResolveRefusal::LoginNeeded
    );
    let logged_in = ResolveCtx {
        password_verified: true,
        ..ctx_for(&link, anon())
    };
    assert!(resolve(&link, &logged_in).is_ok());
    assert!(!password_login_needed(&anyone_link()));

    // The audience comes first: a stranger of a `people` link with a password is not asked for it.
    let mut options = opts(people(&[10, 11]));
    options.password = true;
    let named = build(options);
    assert_eq!(
        resolve(&named, &ctx_for(&named, signed_in(99))).unwrap_err(),
        ResolveRefusal::NotAllowed
    );
    assert_eq!(
        resolve(&named, &ctx_for(&named, signed_in(10))).unwrap_err(),
        ResolveRefusal::LoginNeeded
    );
}

#[test]
fn approval_unapproved_needs_approval() {
    let mut link = build(opts(Audience::Approval));
    let ask = ResolveRefusal::NeedsApproval {
        request: Some(request(3)),
    };
    for caller in [anon(), signed_in(77), signed_in(88)] {
        assert_eq!(
            resolve(&link, &ctx_for(&link, caller)).unwrap_err(),
            ask,
            "the refusal names the paired request that carries the ask"
        );
    }
    update(
        &mut link,
        Change::AddWrap(wrap(77)),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("the sender approves the member 77");
    assert!(resolve(&link, &ctx_for(&link, signed_in(77))).is_ok());
    assert_eq!(
        resolve(&link, &ctx_for(&link, signed_in(88))).unwrap_err(),
        ask,
        "approving one member approves no one else"
    );
}

#[test]
fn the_old_id_and_a_dropped_slug_resolve_nothing() {
    let slug = Slug::parse("quarterly-report").expect("a slug");
    let mut options = opts(people(&[10, 11]));
    options.slug = Some(slug.clone());
    let mut link = build(options);
    let named = || signed_in(10);

    let by_slug = |link: &LinkRecord, text: &str| ResolveCtx {
        asked: Asked::Slug(Slug::parse(text).expect("a slug")),
        ..ctx_for(link, named())
    };
    assert!(resolve(&link, &ctx_for(&link, named())).is_ok());
    assert!(resolve(&link, &by_slug(&link, "quarterly-report")).is_ok());

    let wrong_id = ResolveCtx {
        asked: Asked::Id(link_id(2)),
        ..ctx_for(&link, named())
    };
    assert_eq!(
        resolve(&link, &wrong_id).unwrap_err(),
        ResolveRefusal::NotFound
    );
    assert_eq!(
        resolve(&link, &by_slug(&link, "another-name")).unwrap_err(),
        ResolveRefusal::NotFound
    );

    // A dropped slug stops at once.
    update(
        &mut link,
        Change::Slug(None),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("slug dropped");
    assert_eq!(
        resolve(&link, &by_slug(&link, "quarterly-report")).unwrap_err(),
        ResolveRefusal::NotFound
    );

    // A rotation: "the old URL stops at once".
    let old = *link.id();
    let bound = same_key(&link);
    let mut entropy = TestEntropy::new(5);
    rotate(
        &mut link,
        &mut entropy,
        bound,
        &creator(),
        &open_policy(),
        later(60),
    )
    .expect("rotated");
    let asked_old = ResolveCtx {
        asked: Asked::Id(old),
        ..ctx_for(&link, named())
    };
    assert_eq!(
        resolve(&link, &asked_old).unwrap_err(),
        ResolveRefusal::NotFound
    );
    assert!(
        resolve(&link, &ctx_for(&link, named())).is_ok(),
        "the new id opens it"
    );
}

#[test]
fn a_stranger_learns_nothing_about_the_links_state() {
    let mut link = build(opts(people(&[10, 11])));
    revoke(&mut link, &creator(), later(5)).expect("revoked");
    assert_eq!(
        resolve(&link, &ctx_for(&link, signed_in(99))).unwrap_err(),
        ResolveRefusal::NotAllowed,
        "an unnamed person is told 'not for you', not 'revoked'"
    );
    assert_eq!(
        resolve(&link, &ctx_for(&link, anon())).unwrap_err(),
        ResolveRefusal::SignInRequired
    );
    assert_eq!(
        resolve(&link, &ctx_for(&link, signed_in(10))).unwrap_err(),
        ResolveRefusal::Revoked,
        "a named person is told the truth"
    );

    // A link for anyone has no strangers.
    let mut public = anyone_link();
    revoke(&mut public, &creator(), later(5)).expect("revoked");
    assert_eq!(
        resolve(&public, &ctx_for(&public, anon())).unwrap_err(),
        ResolveRefusal::Revoked
    );
}

#[test]
fn the_rules_apply_in_a_fixed_order() {
    let mut options = opts(Audience::Anyone);
    options.expires_at = Some(later(100));
    options.max_downloads = Some(1);
    options.bytes_cap = Some(ByteCount::mib(1));
    options.max_devices = Some(1);
    options.password = true;
    let base = build(options);
    let token1 = DeviceToken::from_bytes([1; 16]);
    let token2 = DeviceToken::from_bytes([2; 16]);

    let mut row = base.row().clone();
    row.revoked_at = Some(later(10));
    row.pauses = Pauses::NONE.with(PausedBy::Owner);
    row.counters.downloads = 1;
    row.counters.bytes_served = ByteCount::mib(1);
    row.counters.devices_seen.insert(token1.seen_by(base.id()));

    let blocks: BTreeSet<PublicLinkId> = [*base.id()].into_iter().collect();
    let empty: BTreeSet<PublicLinkId> = BTreeSet::new();
    let mut c = ResolveCtx {
        blocks: &blocks,
        links_disabled: true,
        now: later(200),
        device: Some(token2),
        served_today: ByteCount::gib(1),
        daily_cap: ByteCount::gib(1),
        ..ctx_for(&base, anon())
    };
    let first_refusal = |row: &LinkRow, c: &ResolveCtx<'_>| {
        let record = LinkRecord::from_row(row.clone()).expect("a valid row");
        resolve(&record, c).unwrap_err()
    };

    assert_eq!(first_refusal(&row, &c), ResolveRefusal::Revoked);
    row.revoked_at = None;
    assert_eq!(first_refusal(&row, &c), ResolveRefusal::Blocked);
    c.blocks = &empty;
    assert_eq!(first_refusal(&row, &c), ResolveRefusal::Paused);
    row.pauses = Pauses::NONE;
    assert_eq!(first_refusal(&row, &c), ResolveRefusal::PublicLinksOff);
    c.links_disabled = false;
    assert_eq!(
        first_refusal(&row, &c),
        ResolveRefusal::Expired {
            by: ExpiredBy::Date
        }
    );
    c.now = later(50);
    assert_eq!(first_refusal(&row, &c), ResolveRefusal::DownloadsExhausted);
    row.counters.downloads = 0;
    assert_eq!(first_refusal(&row, &c), ResolveRefusal::BytesCapExhausted);
    row.counters.bytes_served = ByteCount::ZERO;
    assert_eq!(first_refusal(&row, &c), ResolveRefusal::DeviceLimit);
    c.device = Some(token1);
    assert!(matches!(
        first_refusal(&row, &c),
        ResolveRefusal::DailyCapReached { .. }
    ));
    c.served_today = ByteCount::ZERO;
    assert_eq!(first_refusal(&row, &c), ResolveRefusal::LoginNeeded);
    c.password_verified = true;
    let record = LinkRecord::from_row(row).expect("a valid row");
    assert!(resolve(&record, &c).is_ok());
}

#[test]
fn a_one_time_link_that_is_used_up_says_so_before_it_says_revoked() {
    let mut options = opts(Audience::Anyone);
    options.one_time = true;
    let link = build(options);
    let used = with_row(&link, |row| {
        row.counters.downloads = 1;
        row.revoked_at = Some(later(5));
    });
    assert_eq!(
        resolve(&used, &ctx_for(&used, anon())).unwrap_err(),
        ResolveRefusal::OneTimeUsed
    );
    // The owner revokes a one-time link before anyone downloaded it: that is a revocation.
    let mut fresh = build({
        let mut options = opts(Audience::Anyone);
        options.one_time = true;
        options
    });
    revoke(&mut fresh, &creator(), later(5)).expect("revoked");
    assert_eq!(
        resolve(&fresh, &ctx_for(&fresh, anon())).unwrap_err(),
        ResolveRefusal::Revoked
    );
}

#[test]
fn a_deadline_beyond_the_largest_stamp_is_never_reached() {
    let mut options = opts(Audience::Anyone);
    options.expires_after_open = Some(Span::days(3650));
    let link = build(options);
    let odd = with_row(&link, |row| {
        row.counters.first_open_at = Some(Stamp::from_secs(u64::MAX - 5));
        row.counters.last_open_at = Some(Stamp::from_secs(u64::MAX - 5));
    });
    let resolved = resolve(&odd, &ctx_for(&odd, anon())).expect("the sum overflows: no end");
    assert_eq!(
        resolved.credential.ttl,
        Span::minutes(15),
        "no date bounds the credential"
    );
}

#[test]
fn resolved_carries_the_manifest_and_the_open_log_notice() {
    let link = anyone_link();
    let resolved = resolve(&link, &ctx_for(&link, anon())).expect("opens");
    assert_eq!(resolved.manifest_object, object(9));
    assert!(!resolved.tell_recipient_open_log);

    let mut options = opts(Audience::SignedIn);
    options.open_log = true;
    let logged = build(options);
    let resolved = resolve(&logged, &ctx_for(&logged, signed_in(5))).expect("opens");
    assert!(resolved.tell_recipient_open_log);
}

/// Every variant of `ResolveRefusal`, once.
fn every_refusal() -> Vec<ResolveRefusal> {
    vec![
        ResolveRefusal::NotFound,
        ResolveRefusal::SignInRequired,
        ResolveRefusal::NotAllowed,
        ResolveRefusal::NeedsApproval { request: None },
        ResolveRefusal::LoginNeeded,
        ResolveRefusal::Revoked,
        ResolveRefusal::OneTimeUsed,
        ResolveRefusal::Blocked,
        ResolveRefusal::Paused,
        ResolveRefusal::PublicLinksOff,
        ResolveRefusal::NotYetOpen { opens_at: NOW },
        ResolveRefusal::Expired {
            by: ExpiredBy::Date,
        },
        ResolveRefusal::DownloadsExhausted,
        ResolveRefusal::BytesCapExhausted,
        ResolveRefusal::DeviceTokenNeeded,
        ResolveRefusal::DeviceLimit,
        ResolveRefusal::DailyCapReached {
            retry_after: Span::secs(1),
        },
        ResolveRefusal::StaleCredential,
    ]
}

/// The position of a variant: an exhaustive match, so that a variant added to the enum does not
/// compile until `every_refusal` is extended.
fn variant_index(refusal: &ResolveRefusal) -> usize {
    match refusal {
        ResolveRefusal::NotFound => 0,
        ResolveRefusal::SignInRequired => 1,
        ResolveRefusal::NotAllowed => 2,
        ResolveRefusal::NeedsApproval { .. } => 3,
        ResolveRefusal::LoginNeeded => 4,
        ResolveRefusal::Revoked => 5,
        ResolveRefusal::OneTimeUsed => 6,
        ResolveRefusal::Blocked => 7,
        ResolveRefusal::Paused => 8,
        ResolveRefusal::PublicLinksOff => 9,
        ResolveRefusal::NotYetOpen { .. } => 10,
        ResolveRefusal::Expired { .. } => 11,
        ResolveRefusal::DownloadsExhausted => 12,
        ResolveRefusal::BytesCapExhausted => 13,
        ResolveRefusal::DeviceTokenNeeded => 14,
        ResolveRefusal::DeviceLimit => 15,
        ResolveRefusal::DailyCapReached { .. } => 16,
        ResolveRefusal::StaleCredential => 17,
    }
}

#[test]
fn reason_words_are_one_for_each_refusal_and_snake_case() {
    let all = every_refusal();
    let indexes: BTreeSet<usize> = all.iter().map(variant_index).collect();
    assert_eq!(indexes, (0..18).collect::<BTreeSet<usize>>());
    let words: BTreeSet<&str> = all.iter().map(ResolveRefusal::reason).collect();
    assert_eq!(words.len(), all.len(), "one word for each refusal");
    for word in &words {
        assert!(
            !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{word}"
        );
    }
    for refusal in &all {
        assert!(!refusal.to_string().is_empty());
    }
}

#[test]
fn only_the_daily_cap_has_a_delay() {
    for refusal in every_refusal() {
        let expected = match refusal {
            ResolveRefusal::DailyCapReached { retry_after } => Some(retry_after),
            _ => None,
        };
        assert_eq!(refusal.retry_after(), expected, "{refusal:?}");
    }
}

#[test]
fn ends_link_names_how_a_link_ended() {
    for refusal in every_refusal() {
        let expected = match refusal {
            ResolveRefusal::Revoked | ResolveRefusal::OneTimeUsed | ResolveRefusal::Blocked => {
                Some(LinkEnded::Revoked)
            }
            ResolveRefusal::Expired { .. } => Some(LinkEnded::Expired),
            _ => None,
        };
        assert_eq!(refusal.ends_link(), expected, "{refusal:?}");
    }
}

#[test]
fn the_three_expiries_have_their_own_words() {
    let words: Vec<&str> = ExpiredBy::ALL.iter().map(|by| by.as_str()).collect();
    assert_eq!(words, ["expires_at", "expires_after_open", "idle_expiry"]);
    for by in ExpiredBy::ALL {
        assert_eq!(by.as_str().parse::<ExpiredBy>(), Ok(*by));
    }
    assert!("never".parse::<ExpiredBy>().is_err());
}

#[test]
fn caller_accessors_tell_signed_in_from_anonymous() {
    assert!(!anon().is_signed_in());
    assert_eq!(anon().member(), None);
    assert_eq!(anon().account(), None);
    let outsider = signed_in(5);
    assert!(outsider.is_signed_in());
    assert_eq!(outsider.member(), Some(&member(5)));
    assert_eq!(outsider.account(), Some(&AccountId::from_bytes([200; 16])));
    let insider = account_member(6, &[1]);
    assert!(insider.is_signed_in());
    assert_eq!(insider.member(), Some(&member(6)));
    assert_eq!(insider.account(), Some(&AccountId::from_bytes([1; 16])));
}

#[test]
fn a_refused_resolve_changes_nothing() {
    let link = build(opts(people(&[10, 11])));
    let before = link.state_hash();
    for caller in [anon(), signed_in(99), account_member(1, &[1])] {
        assert!(resolve(&link, &ctx_for(&link, caller)).is_err());
    }
    assert_eq!(link.state_hash(), before);
}

#[test]
fn approval_is_asked_before_the_password() {
    let mut options = opts(Audience::Approval);
    options.password = true;
    let mut link = build(options);
    // An unapproved member is asked for the approval, not for the password.
    assert!(matches!(
        resolve(&link, &ctx_for(&link, signed_in(88))).unwrap_err(),
        ResolveRefusal::NeedsApproval { .. }
    ));
    update(
        &mut link,
        Change::AddWrap(wrap(77)),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("approved");
    assert_eq!(
        resolve(&link, &ctx_for(&link, signed_in(77))).unwrap_err(),
        ResolveRefusal::LoginNeeded,
        "an approved member is asked for the password"
    );
    let logged_in = ResolveCtx {
        password_verified: true,
        ..ctx_for(&link, signed_in(77))
    };
    assert!(resolve(&link, &logged_in).is_ok());
}

/// A number below `n`.
fn below(rng: &mut Rng, n: u64) -> u64 {
    rng.below(n).expect("a range")
}

/// What the oracle expects: the first refusal of the table in the docs of `resolve.rs`, or that the
/// link opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Want {
    Opens,
    NotFound,
    SignIn,
    NotAllowed,
    OneTimeUsed,
    Revoked,
    Blocked,
    Paused,
    PublicLinksOff,
    NotYetOpen,
    ExpiredDate,
    ExpiredAfterOpen,
    ExpiredIdle,
    DownloadsExhausted,
    BytesCapExhausted,
    DeviceTokenNeeded,
    DeviceLimit,
    DailyCap,
    NeedsApproval,
    LoginNeeded,
}

/// Every answer, once.
const EVERY_WANT: [Want; 20] = [
    Want::Opens,
    Want::NotFound,
    Want::SignIn,
    Want::NotAllowed,
    Want::OneTimeUsed,
    Want::Revoked,
    Want::Blocked,
    Want::Paused,
    Want::PublicLinksOff,
    Want::NotYetOpen,
    Want::ExpiredDate,
    Want::ExpiredAfterOpen,
    Want::ExpiredIdle,
    Want::DownloadsExhausted,
    Want::BytesCapExhausted,
    Want::DeviceTokenNeeded,
    Want::DeviceLimit,
    Want::DailyCap,
    Want::NeedsApproval,
    Want::LoginNeeded,
];

/// The side of an answer of `resolve`.
fn want_of(answer: &Result<Resolved, ResolveRefusal>) -> Want {
    match answer {
        Ok(_) => Want::Opens,
        Err(ResolveRefusal::NotFound) => Want::NotFound,
        Err(ResolveRefusal::SignInRequired) => Want::SignIn,
        Err(ResolveRefusal::NotAllowed) => Want::NotAllowed,
        Err(ResolveRefusal::NeedsApproval { .. }) => Want::NeedsApproval,
        Err(ResolveRefusal::LoginNeeded) => Want::LoginNeeded,
        Err(ResolveRefusal::Revoked) => Want::Revoked,
        Err(ResolveRefusal::OneTimeUsed) => Want::OneTimeUsed,
        Err(ResolveRefusal::Blocked) => Want::Blocked,
        Err(ResolveRefusal::Paused) => Want::Paused,
        Err(ResolveRefusal::PublicLinksOff) => Want::PublicLinksOff,
        Err(ResolveRefusal::NotYetOpen { .. }) => Want::NotYetOpen,
        Err(ResolveRefusal::Expired {
            by: ExpiredBy::Date,
        }) => Want::ExpiredDate,
        Err(ResolveRefusal::Expired {
            by: ExpiredBy::AfterOpen,
        }) => Want::ExpiredAfterOpen,
        Err(ResolveRefusal::Expired {
            by: ExpiredBy::Idle,
        }) => Want::ExpiredIdle,
        Err(ResolveRefusal::DownloadsExhausted) => Want::DownloadsExhausted,
        Err(ResolveRefusal::BytesCapExhausted) => Want::BytesCapExhausted,
        Err(ResolveRefusal::DeviceTokenNeeded) => Want::DeviceTokenNeeded,
        Err(ResolveRefusal::DeviceLimit) => Want::DeviceLimit,
        Err(ResolveRefusal::DailyCapReached { .. }) => Want::DailyCap,
        Err(ResolveRefusal::StaleCredential) => {
            panic!("a resolve never answers StaleCredential")
        }
    }
}

/// The rules of the table of `resolve.rs`, typed a second time as a list of (violated, answer) in
/// the order of the table: the first violated rule answers.
fn oracle(link: &LinkRecord, c: &ResolveCtx<'_>, blocked: bool) -> Want {
    let o = link.options();
    let k = link.counters();
    let now = c.now.as_secs();
    let member = c.caller.member();
    let public = matches!(o.audience, Audience::Anyone);
    let audience = match &o.audience {
        Audience::Anyone | Audience::Approval => Want::Opens,
        Audience::SignedIn => {
            if member.is_some() {
                Want::Opens
            } else {
                Want::SignIn
            }
        }
        Audience::Members { groups } => match &c.caller {
            Caller::Anonymous => Want::SignIn,
            Caller::SignedIn { .. } => Want::NotAllowed,
            Caller::Member { groups: mine, .. } => {
                if groups.is_empty() || groups.iter().any(|g| mine.contains(g)) {
                    Want::Opens
                } else {
                    Want::NotAllowed
                }
            }
        },
        Audience::People { people } => match member {
            None => Want::SignIn,
            Some(m) => {
                if people.contains(m) {
                    Want::Opens
                } else {
                    Want::NotAllowed
                }
            }
        },
    };
    let idle_since = k
        .last_open_at
        .or(k.first_open_at)
        .unwrap_or(link.keyed_at())
        .as_secs();
    let known = c
        .device
        .map(|t| k.devices_seen.contains(&t.seen_by(link.id())));
    let rules = [
        (c.asked != Asked::Id(*link.id()), Want::NotFound),
        (audience != Want::Opens, audience),
        (o.one_time && k.downloads >= 1, Want::OneTimeUsed),
        (link.revoked_at().is_some(), Want::Revoked),
        (blocked, Want::Blocked),
        (link.is_paused(), Want::Paused),
        (c.links_disabled && public, Want::PublicLinksOff),
        (
            o.not_before.is_some_and(|t| now < t.as_secs()),
            Want::NotYetOpen,
        ),
        (
            o.expires_at.is_some_and(|t| now >= t.as_secs()),
            Want::ExpiredDate,
        ),
        (
            o.expires_after_open.is_some_and(|d| {
                k.first_open_at
                    .is_some_and(|f| now >= f.as_secs() + d.as_secs())
            }),
            Want::ExpiredAfterOpen,
        ),
        (
            o.idle_expiry
                .is_some_and(|d| now >= idle_since + d.as_secs()),
            Want::ExpiredIdle,
        ),
        (
            o.max_downloads.is_some_and(|m| k.downloads >= u64::from(m)),
            Want::DownloadsExhausted,
        ),
        (
            o.bytes_cap.is_some_and(|cap| k.bytes_served >= cap),
            Want::BytesCapExhausted,
        ),
        (
            o.max_devices.is_some() && c.device.is_none(),
            Want::DeviceTokenNeeded,
        ),
        (
            o.max_devices
                .is_some_and(|m| known == Some(false) && k.devices_seen.len() >= usize::from(m)),
            Want::DeviceLimit,
        ),
        (public && c.served_today >= c.daily_cap, Want::DailyCap),
        (
            matches!(o.audience, Audience::Approval)
                && member.is_none_or(|m| link.wrap_for(m).is_none()),
            Want::NeedsApproval,
        ),
        (o.password && !c.password_verified, Want::LoginNeeded),
    ];
    rules
        .iter()
        .find(|(violated, _)| *violated)
        .map_or(Want::Opens, |(_, want)| *want)
}

#[test]
fn resolve_agrees_with_an_oracle_over_20000_seeded_cases() {
    let tokens: Vec<DeviceToken> = (1..=5u8)
        .map(|n| DeviceToken::from_bytes([n; 16]))
        .collect();
    let mut answers: BTreeMap<Want, usize> = BTreeMap::new();
    for seed in 0..20_000u64 {
        let mut rng = Rng::new(seed);

        // The link: every option that a rule reads, at random.
        let audience = match below(&mut rng, 5) {
            0 => Audience::Anyone,
            1 => Audience::SignedIn,
            2 => groups(&[1, 2]),
            3 => people(&[10, 11]),
            _ => Audience::Approval,
        };
        let mut options = opts(audience);
        options.password = rng.chance(1, 4);
        let mut not_before_secs = 0;
        if rng.chance(1, 5) {
            not_before_secs = below(&mut rng, 2_000);
            options.not_before = Some(later(not_before_secs));
        }
        if rng.chance(1, 3) {
            options.expires_at = Some(later(not_before_secs + 1 + below(&mut rng, 3_000)));
        }
        if rng.chance(1, 5) {
            options.expires_after_open = Some(Span::secs(1 + below(&mut rng, 2_000)));
        }
        if rng.chance(1, 5) {
            options.idle_expiry = Some(Span::secs(1 + below(&mut rng, 2_000)));
        }
        options.one_time = rng.chance(1, 5);
        if rng.chance(1, 4) {
            options.max_downloads = Some(1 + below(&mut rng, 3) as u32);
        }
        if rng.chance(1, 4) {
            options.bytes_cap = Some(ByteCount::kib(1 + below(&mut rng, 4) as u32));
        }
        if rng.chance(1, 4) {
            options.max_devices = Some(1 + below(&mut rng, 3) as u16);
        }
        let mut base = build(options);
        if matches!(base.options().audience, Audience::Approval) && rng.chance(1, 2) {
            update(
                &mut base,
                Change::AddWrap(wrap(77)),
                &creator(),
                &open_policy(),
                NOW,
            )
            .expect("approved");
        }

        // What the link has seen so far.
        let link = with_row(&base, |row| {
            row.counters.downloads = below(&mut rng, 4);
            row.counters.bytes_served = ByteCount::kib(below(&mut rng, 6) as u32);
            for token in &tokens {
                if rng.chance(1, 3) {
                    row.counters.devices_seen.insert(token.seen_by(&row.id));
                }
            }
            if rng.chance(1, 2) {
                let first = below(&mut rng, 1_500);
                row.counters.first_open_at = Some(later(first));
                row.counters.last_open_at = Some(later(first + below(&mut rng, 500)));
            }
            if rng.chance(1, 8) {
                row.revoked_at = Some(later(below(&mut rng, 100)));
            }
            if rng.chance(1, 8) {
                row.pauses = Pauses::NONE.with(PausedBy::Owner);
            }
            row.keyed_at = later(below(&mut rng, 500));
        });

        // The request.
        let blocked = rng.chance(1, 10);
        let blocks: BTreeSet<PublicLinkId> = if blocked {
            [*link.id()].into_iter().collect()
        } else {
            BTreeSet::new()
        };
        let caller = match below(&mut rng, 6) {
            0 => anon(),
            1 => signed_in(5),
            2 => signed_in(10),
            3 => signed_in(77),
            4 => account_member(20, &[1]),
            _ => account_member(21, &[7]),
        };
        let c = ResolveCtx {
            asked: if rng.chance(1, 20) {
                Asked::Id(link_id(2))
            } else {
                Asked::Id(*link.id())
            },
            now: later(below(&mut rng, 4_000)),
            caller,
            device: if rng.chance(2, 3) {
                Some(DeviceToken::from_bytes([1 + below(&mut rng, 5) as u8; 16]))
            } else {
                None
            },
            blocks: &blocks,
            links_disabled: rng.chance(1, 8),
            served_today: ByteCount::kib(below(&mut rng, 6) as u32),
            daily_cap: ByteCount::kib(4),
            password_verified: rng.chance(1, 2),
            human_step: true,
        };

        // The answer is the oracle's.
        let answer = resolve(&link, &c);
        let want = oracle(&link, &c, blocked);
        assert_eq!(
            want_of(&answer),
            want,
            "seed {seed}: resolve and the oracle differ"
        );
        *answers.entry(want).or_insert(0) += 1;

        // `admit_bytes` serves nothing when the link refuses, and what the caps leave when it opens.
        let ask = ByteCount::kib(8);
        let admitted = admit_bytes(&link, ask, &c);
        if want == Want::Opens {
            let mut left = ask;
            if let Some(cap) = link.options().bytes_cap {
                left = left.min(cap.saturating_sub(link.counters().bytes_served));
            }
            if matches!(link.options().audience, Audience::Anyone) {
                left = left.min(c.daily_cap.saturating_sub(c.served_today));
            }
            assert_eq!(admitted, left, "seed {seed}: admit_bytes of an open link");
        } else {
            assert_eq!(
                admitted,
                ByteCount::ZERO,
                "seed {seed}: admit_bytes of a link that refuses"
            );
        }

        // The human step asks the same question and changes only the counters it names.
        let mut counted = link.clone();
        let outcome = count_open(&mut counted, &c, true);
        match answer {
            Err(refusal) => {
                assert_eq!(
                    outcome.unwrap_err(),
                    OpenRefusal::Link(refusal),
                    "seed {seed}"
                );
                assert_eq!(
                    counted, link,
                    "seed {seed}: a refused open changed the record"
                );
            }
            Ok(_) => {
                outcome.expect("the open is counted");
                let mut expected = link.row().clone();
                if expected.counters.first_open_at.is_none() {
                    expected.counters.first_open_at = Some(c.now);
                }
                expected.counters.last_open_at = Some(match expected.counters.last_open_at {
                    Some(at) if at > c.now => at,
                    _ => c.now,
                });
                if link.options().max_devices.is_some() {
                    if let Some(token) = c.device {
                        expected
                            .counters
                            .devices_seen
                            .insert(token.seen_by(link.id()));
                    }
                }
                assert_eq!(
                    counted.row(),
                    &expected,
                    "seed {seed}: the human step changed more than the counters it names"
                );
            }
        }
    }

    // The generator reached every answer, and the link opened often enough to be tested.
    let missing: Vec<Want> = EVERY_WANT
        .iter()
        .copied()
        .filter(|want| !answers.contains_key(want))
        .collect();
    assert!(
        missing.is_empty(),
        "the generator never produced {missing:?}"
    );
    assert!(
        answers[&Want::Opens] > 500,
        "only {} of 20,000 cases opened: {answers:?}",
        answers[&Want::Opens]
    );
}
