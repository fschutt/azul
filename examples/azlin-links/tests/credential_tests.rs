//! Check 9: the link credential (planning/architecture.md section 5.12 "Resolving": "a **link
//! credential**: short-lived (15 minutes, renewed while the link allows it) and limited to the
//! link's grants; plus `request_upload` to the paired request and `log_append` to the comment log,
//! where those are on") and section 5.11 ("`scopes` only narrows").
//!
//! Implements planning/architecture.md section 5.12 ("Resolving") and section 5.11 (credential v2).

#![allow(dead_code)]

mod common;

use std::collections::BTreeSet;

use azlin_links::counters::{count_download_complete, count_open};
use azlin_links::credential::{
    may_read, renew, CredentialClaims, CredentialSpec, LinkScope, CREDENTIAL_TTL,
};
use azlin_links::resolve::{resolve, Asked, Caller, ExpiredBy, ResolveCtx, ResolveRefusal};
use azlin_links::{
    pause, revoke, rotate, update, Actor, Audience, ByteRange, Change, DeviceToken, Grant, Grants,
    KeyBound, LinkRecord, LinkRow, ObjectRef, PublicLinkId, RequestId,
};
use azlin_policy::LinkPolicy;
use azlin_seams::sim::Rng;
use azlin_seams::{AccountId, ByteCount, Span, Stamp};
use common::{
    build, creator, group, groups, link_id, log, member, object, opts, people, request, wrap,
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

/// The context of a plain request for `link` by `caller` at `NOW`.
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

/// The row of `link` changed by `change`, read back as a record.
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

/// The claims of a credential for `link` issued `issued` seconds after `NOW` that lives `lives`
/// seconds, with the scope `Read` only.
fn claims_at(link: &LinkRecord, issued: u64, lives: u64) -> CredentialClaims {
    CredentialClaims::new(
        later(issued),
        later(issued + lives),
        vec![LinkScope::Read(*link.id())],
    )
}

/// The claims of the credential `link` gives `caller` at `NOW`.
fn issued_to(link: &LinkRecord, caller: Caller) -> CredentialClaims {
    let resolved = resolve(link, &ctx_for(link, caller)).expect("the link opens");
    CredentialClaims::from_spec(&resolved.credential, NOW).expect("claims")
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

/// The range from `start` to `end` (excluded).
fn range(start: u64, end: u64) -> ByteRange {
    ByteRange::new(ByteCount::new(start), ByteCount::new(end - start)).expect("a range")
}

#[test]
fn credential_is_15_minutes_and_limited_to_the_links_grants() {
    assert_eq!(CREDENTIAL_TTL, Span::minutes(15));
    let link = anyone_link();
    let resolved = resolve(&link, &ctx_for(&link, anon())).expect("opens");
    assert_eq!(resolved.credential.ttl, Span::minutes(15));
    assert_eq!(
        resolved.credential.scopes,
        vec![LinkScope::Read(*link.id())]
    );
    assert!(resolved.credential.reads(link.id()));
    assert_eq!(
        resolved.credential.expires_at(NOW),
        NOW.checked_add(Span::minutes(15))
    );

    // Never longer than 15 minutes, and never longer than the link itself.
    for (expires_in, ttl) in [
        (300u64, 300u64),
        (899, 899),
        (900, 900),
        (901, 900),
        (3_600, 900),
        (10 * DAY_SECS, 900),
    ] {
        let mut options = opts(Audience::Anyone);
        options.expires_at = Some(later(expires_in));
        let dated = build(options);
        let resolved = resolve(&dated, &ctx_for(&dated, anon())).expect("opens");
        assert_eq!(
            resolved.credential.ttl,
            Span::secs(ttl),
            "a link that ends in {expires_in} seconds"
        );
    }
    let mut options = opts(Audience::Anyone);
    options.idle_expiry = Some(Span::minutes(10));
    let idle = build(options);
    assert_eq!(
        resolve(&idle, &ctx_for(&idle, anon()))
            .expect("opens")
            .credential
            .ttl,
        Span::minutes(10),
        "ten idle minutes end the link: the credential ends with them"
    );
    let mut options = opts(Audience::Anyone);
    options.expires_after_open = Some(Span::minutes(5));
    let after_open = with_row(&build(options), |row| {
        row.counters.first_open_at = Some(NOW);
        row.counters.last_open_at = Some(NOW);
    });
    let c = at(&after_open, anon(), 60);
    assert_eq!(
        resolve(&after_open, &c).expect("opens").credential.ttl,
        Span::secs(240),
        "the five minutes after the first open end the link"
    );

    // Limited to the link's grants: the grant, and the manifest object the link hands out.
    let claims = issued_to(&link, anon());
    assert!(may_read(&link, &claims, NOW, &object(0), None), "granted");
    assert!(
        may_read(&link, &claims, NOW, &object(9), None),
        "the manifest object"
    );
    assert!(
        !may_read(&link, &claims, NOW, &object(5), None),
        "an object no grant names"
    );
}

#[test]
fn request_upload_and_log_append_scopes_only_where_on() {
    let scopes_of = |link: &LinkRecord, caller: Caller| {
        resolve(link, &ctx_for(link, caller))
            .expect("opens")
            .credential
            .scopes
    };

    // Uploads on: the paired request.
    let mut options = opts(Audience::Anyone);
    options.allow_upload = true;
    let uploads = build(options);
    assert_eq!(
        scopes_of(&uploads, anon()),
        vec![
            LinkScope::Read(*uploads.id()),
            LinkScope::RequestUpload(request(3))
        ]
    );

    // Comments on: the comment log.
    let mut options = opts(Audience::SignedIn);
    options.comment = true;
    let comments = build(options);
    assert_eq!(
        scopes_of(&comments, signed_in(5)),
        vec![
            LinkScope::Read(*comments.id()),
            LinkScope::LogAppend(log(4))
        ]
    );

    // Both.
    let mut options = opts(Audience::SignedIn);
    options.comment = true;
    options.allow_upload = true;
    let both = build(options);
    assert_eq!(
        scopes_of(&both, signed_in(5)),
        vec![
            LinkScope::Read(*both.id()),
            LinkScope::RequestUpload(request(3)),
            LinkScope::LogAppend(log(4))
        ]
    );

    // Neither: only `Read`, even when the link has a paired request (an `approval` link) or a
    // comment log that comments do not use.
    let mut approval = build(opts(Audience::Approval));
    update(
        &mut approval,
        Change::AddWrap(wrap(77)),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("approved");
    assert!(approval.request_id().is_some());
    assert_eq!(
        scopes_of(&approval, signed_in(77)),
        vec![LinkScope::Read(*approval.id())]
    );
    let quiet = with_row(&build(opts(Audience::SignedIn)), |row| {
        row.comment_log = Some(log(4));
    });
    assert_eq!(
        scopes_of(&quiet, signed_in(5)),
        vec![LinkScope::Read(*quiet.id())]
    );
    assert_eq!(
        scopes_of(&anyone_link(), anon()),
        vec![LinkScope::Read(*anyone_link().id())]
    );
}

#[test]
fn renewal_rechecks_every_option_and_counts_nothing() {
    // A renewal that passes: the same scopes, a fresh 15 minutes, no change of state.
    let link = anyone_link();
    let hash = link.state_hash();
    let claims = issued_to(&link, anon());
    let renewed = renew(&link, &claims, &at(&link, anon(), 600)).expect("renewed");
    assert_eq!(renewed.ttl, CREDENTIAL_TTL);
    assert_eq!(renewed.scopes, claims.scopes().to_vec());
    assert_eq!(link.state_hash(), hash, "a renewal counts nothing");

    // Each option, checked again at the renewal.
    let long_claims = |link: &LinkRecord| claims_at(link, 0, 900);

    let mut paused = anyone_link();
    pause(&mut paused, &creator()).expect("paused");
    assert_eq!(
        renew(&paused, &long_claims(&paused), &ctx_for(&paused, anon())).unwrap_err(),
        ResolveRefusal::Paused
    );

    let blocks: BTreeSet<PublicLinkId> = [*link.id()].into_iter().collect();
    let blocked = ResolveCtx {
        blocks: &blocks,
        ..ctx_for(&link, anon())
    };
    assert_eq!(
        renew(&link, &long_claims(&link), &blocked).unwrap_err(),
        ResolveRefusal::Blocked
    );

    let disabled = ResolveCtx {
        links_disabled: true,
        ..ctx_for(&link, anon())
    };
    assert_eq!(
        renew(&link, &long_claims(&link), &disabled).unwrap_err(),
        ResolveRefusal::PublicLinksOff
    );

    let capped = ResolveCtx {
        served_today: ByteCount::gib(1),
        daily_cap: ByteCount::gib(1),
        ..ctx_for(&link, anon())
    };
    assert!(matches!(
        renew(&link, &long_claims(&link), &capped).unwrap_err(),
        ResolveRefusal::DailyCapReached { .. }
    ));

    let mut options = opts(Audience::Anyone);
    options.expires_at = Some(later(500));
    let dated = build(options);
    assert_eq!(
        renew(&dated, &long_claims(&dated), &at(&dated, anon(), 600)).unwrap_err(),
        ResolveRefusal::Expired {
            by: ExpiredBy::Date
        }
    );

    let mut options = opts(Audience::Anyone);
    options.max_downloads = Some(1);
    options.bytes_cap = Some(ByteCount::mib(1));
    let counted = build(options);
    let downloaded = with_row(&counted, |row| row.counters.downloads = 1);
    assert_eq!(
        renew(
            &downloaded,
            &long_claims(&downloaded),
            &ctx_for(&downloaded, anon())
        )
        .unwrap_err(),
        ResolveRefusal::DownloadsExhausted
    );
    let served = with_row(&counted, |row| {
        row.counters.bytes_served = ByteCount::mib(1)
    });
    assert_eq!(
        renew(&served, &long_claims(&served), &ctx_for(&served, anon())).unwrap_err(),
        ResolveRefusal::BytesCapExhausted
    );

    let mut options = opts(Audience::Anyone);
    options.max_devices = Some(1);
    let limited = build(options);
    let token = |n: u8| DeviceToken::from_bytes([n; 16]);
    let seen = with_row(&limited, |row| {
        row.counters
            .devices_seen
            .insert(token(1).seen_by(limited.id()));
    });
    let stranger_device = ResolveCtx {
        device: Some(token(2)),
        ..ctx_for(&seen, anon())
    };
    assert_eq!(
        renew(&seen, &long_claims(&seen), &stranger_device).unwrap_err(),
        ResolveRefusal::DeviceLimit
    );
    let known_device = ResolveCtx {
        device: Some(token(1)),
        ..ctx_for(&seen, anon())
    };
    assert!(renew(&seen, &long_claims(&seen), &known_device).is_ok());

    // The audience is checked again: a member who left the chosen group no longer renews.
    let members = build(opts(groups(&[1, 2])));
    assert!(renew(
        &members,
        &long_claims(&members),
        &ctx_for(&members, account_member(20, &[1]))
    )
    .is_ok());
    assert_eq!(
        renew(
            &members,
            &long_claims(&members),
            &ctx_for(&members, account_member(20, &[7]))
        )
        .unwrap_err(),
        ResolveRefusal::NotAllowed
    );

    // What the old URL names is checked again.
    let wrong_id = ResolveCtx {
        asked: Asked::Id(link_id(2)),
        ..ctx_for(&link, anon())
    };
    assert_eq!(
        renew(&link, &long_claims(&link), &wrong_id).unwrap_err(),
        ResolveRefusal::NotFound
    );
}

#[test]
fn renewal_refused_after_revoke_pause_expiry_or_one_time_used() {
    let long_claims = |link: &LinkRecord| claims_at(link, 0, 900);

    let mut revoked = anyone_link();
    revoke(&mut revoked, &creator(), later(5)).expect("revoked");
    assert_eq!(
        renew(&revoked, &long_claims(&revoked), &ctx_for(&revoked, anon())).unwrap_err(),
        ResolveRefusal::Revoked
    );

    let mut paused = anyone_link();
    pause(&mut paused, &Actor::Operator).expect("paused");
    assert_eq!(
        renew(&paused, &long_claims(&paused), &ctx_for(&paused, anon())).unwrap_err(),
        ResolveRefusal::Paused
    );

    let mut options = opts(Audience::Anyone);
    options.expires_at = Some(later(600));
    let dated = build(options);
    assert!(renew(&dated, &long_claims(&dated), &at(&dated, anon(), 599)).is_ok());
    assert_eq!(
        renew(&dated, &long_claims(&dated), &at(&dated, anon(), 600)).unwrap_err(),
        ResolveRefusal::Expired {
            by: ExpiredBy::Date
        }
    );

    let mut options = opts(Audience::Anyone);
    options.one_time = true;
    let mut one_time = build(options);
    let claims = issued_to(&one_time, anon());
    let human = ResolveCtx {
        human_step: true,
        ..ctx_for(&one_time, anon())
    };
    count_open(&mut one_time, &human, false).expect("counted");
    assert!(
        renew(&one_time, &claims, &ctx_for(&one_time, anon())).is_ok(),
        "an open does not use the link up"
    );
    count_download_complete(&mut one_time, ByteCount::kib(1), later(100));
    assert_eq!(
        renew(&one_time, &claims, &ctx_for(&one_time, anon())).unwrap_err(),
        ResolveRefusal::OneTimeUsed
    );
}

#[test]
fn scopes_only_narrow() {
    // A link with uploads and comments: the credential has all three scopes.
    let mut options = opts(Audience::SignedIn);
    options.allow_upload = true;
    options.comment = true;
    let mut link = build(options);
    let claims = issued_to(&link, signed_in(5));
    assert_eq!(claims.scopes().len(), 3);

    // The owner turns comments off: the renewal drops `log_append`.
    update(
        &mut link,
        Change::Comment(false),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("comments off");
    let renewed = renew(&link, &claims, &ctx_for(&link, signed_in(5))).expect("renewed");
    assert_eq!(
        renewed.scopes,
        vec![
            LinkScope::Read(*link.id()),
            LinkScope::RequestUpload(request(3))
        ]
    );

    // A credential issued without comments does not gain them when the owner turns them on: it
    // renews as it was, and a fresh resolve gives the wider one.
    let mut plain = build(opts(Audience::SignedIn));
    let old = issued_to(&plain, signed_in(5));
    update(
        &mut plain,
        Change::PairCommentLog(log(4)),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("a log");
    update(
        &mut plain,
        Change::Comment(true),
        &creator(),
        &open_policy(),
        NOW,
    )
    .expect("comments on");
    let renewed = renew(&plain, &old, &ctx_for(&plain, signed_in(5))).expect("renewed");
    assert_eq!(renewed.scopes, vec![LinkScope::Read(*plain.id())]);
    let fresh = resolve(&plain, &ctx_for(&plain, signed_in(5))).expect("opens");
    assert_eq!(
        fresh.credential.scopes,
        vec![LinkScope::Read(*plain.id()), LinkScope::LogAppend(log(4))]
    );

    // Over many random links and credentials: a renewal never has a scope the old one lacked.
    for seed in 0..500u64 {
        let mut rng = Rng::new(seed);
        let mut options = opts(Audience::SignedIn);
        options.allow_upload = rng.chance(1, 2);
        options.comment = rng.chance(1, 2);
        let link = build(options);
        let full = resolve(&link, &ctx_for(&link, signed_in(5))).expect("opens");
        let kept: Vec<LinkScope> = full
            .credential
            .scopes
            .iter()
            .copied()
            .filter(|scope| matches!(scope, LinkScope::Read(_)) || rng.chance(1, 2))
            .collect();
        let old = CredentialClaims::new(NOW, later(900), kept);
        let renewed = renew(&link, &old, &at(&link, signed_in(5), 10)).expect("renewed");
        for scope in &renewed.scopes {
            assert!(
                old.scopes().contains(scope),
                "seed {seed}: {scope:?} was not in the old credential"
            );
        }
        assert!(renewed.ttl <= CREDENTIAL_TTL);
    }
}

#[test]
fn a_credential_of_another_key_or_an_old_one_is_not_renewed() {
    let mut link = build(opts(people(&[10, 11])));
    let old_claims = issued_to(&link, signed_in(10));
    assert!(
        renew(&link, &old_claims, &at(&link, signed_in(10), 60)).is_ok(),
        "the credential renews while it fits"
    );

    // Expired claims.
    assert_eq!(
        renew(&link, &old_claims, &at(&link, signed_in(10), 900)).unwrap_err(),
        ResolveRefusal::StaleCredential,
        "a credential that has ended is not renewed"
    );
    // Claims of another link.
    let other = claims_at(&anyone_link(), 0, 900);
    let other = CredentialClaims::new(
        other.issued_at(),
        other.expires_at(),
        vec![LinkScope::Read(link_id(77))],
    );
    assert_eq!(
        renew(&link, &other, &ctx_for(&link, signed_in(10))).unwrap_err(),
        ResolveRefusal::StaleCredential
    );
    // Claims without `Read`.
    let none = CredentialClaims::new(NOW, later(900), vec![LinkScope::LogAppend(log(4))]);
    assert_eq!(
        renew(&link, &none, &ctx_for(&link, signed_in(10))).unwrap_err(),
        ResolveRefusal::StaleCredential
    );
    // Issued before the link's key began (the same id, a later `keyed_at`).
    let rekeyed = with_row(&link, |row| row.keyed_at = later(100));
    assert_eq!(
        renew(
            &rekeyed,
            &claims_at(&rekeyed, 50, 900),
            &at(&rekeyed, signed_in(10), 120)
        )
        .unwrap_err(),
        ResolveRefusal::StaleCredential
    );
    assert!(renew(
        &rekeyed,
        &claims_at(&rekeyed, 100, 900),
        &at(&rekeyed, signed_in(10), 120)
    )
    .is_ok());

    // A rotation: the old credential names the old id.
    let bound = same_key(&link);
    let mut entropy = TestEntropy::new(3);
    rotate(
        &mut link,
        &mut entropy,
        bound,
        &creator(),
        &open_policy(),
        later(30),
    )
    .expect("rotated");
    let after = ResolveCtx {
        now: later(60),
        ..ctx_for(&link, signed_in(10))
    };
    assert_eq!(
        renew(&link, &old_claims, &after).unwrap_err(),
        ResolveRefusal::StaleCredential,
        "the old URL's credential stops at once"
    );
    // A credential of the new key renews.
    let new_claims =
        CredentialClaims::new(later(40), later(900), vec![LinkScope::Read(*link.id())]);
    assert!(renew(&link, &new_claims, &after).is_ok());
}

#[test]
fn renewal_does_not_ask_for_the_password_again() {
    let mut options = opts(Audience::Anyone);
    options.password = true;
    let link = build(options);
    // The credential was issued after the login.
    let logged_in = ResolveCtx {
        password_verified: true,
        ..ctx_for(&link, anon())
    };
    let resolved = resolve(&link, &logged_in).expect("opens after the login");
    let claims = CredentialClaims::from_spec(&resolved.credential, NOW).expect("claims");
    // The renewing request carries no login state: the old credential proves it.
    let renewing = at(&link, anon(), 300);
    assert!(!renewing.password_verified);
    assert!(renew(&link, &claims, &renewing).is_ok());
}

#[test]
fn a_credential_reads_only_what_the_grants_name() {
    let base = anyone_link();
    let linked = with_row(&base, |row| {
        row.grants = Grants::new(vec![
            // object 1: the whole object
            Grant {
                object: object(1),
                range: None,
            },
            // object 2: one range
            Grant {
                object: object(2),
                range: Some(range(100, 200)),
            },
            // object 3: two adjacent ranges
            Grant {
                object: object(3),
                range: Some(range(0, 50)),
            },
            Grant {
                object: object(3),
                range: Some(range(50, 100)),
            },
            // object 4: two ranges with a gap
            Grant {
                object: object(4),
                range: Some(range(0, 50)),
            },
            Grant {
                object: object(4),
                range: Some(range(60, 100)),
            },
        ])
        .expect("grants");
    });
    let claims = CredentialClaims::new(NOW, later(900), vec![LinkScope::Read(*linked.id())]);
    let reads = |target: ObjectRef, wanted: Option<ByteRange>| {
        may_read(&linked, &claims, later(1), &target, wanted)
    };

    // A whole-object grant covers the whole object and every range of it.
    assert!(reads(object(1), None));
    assert!(reads(object(1), Some(range(10, 20))));
    // A range grant covers its ranges and not the whole object, and not beyond.
    assert!(reads(object(2), Some(range(100, 200))));
    assert!(reads(object(2), Some(range(120, 130))));
    assert!(!reads(object(2), Some(range(90, 110))));
    assert!(!reads(object(2), Some(range(150, 250))));
    assert!(!reads(object(2), None));
    // Adjacent ranges cover a range that spans them.
    assert!(reads(object(3), Some(range(25, 75))));
    assert!(reads(object(3), Some(range(0, 100))));
    assert!(!reads(object(3), Some(range(0, 101))));
    assert!(!reads(object(3), None));
    // A gap is not covered.
    assert!(reads(object(4), Some(range(0, 50))));
    assert!(reads(object(4), Some(range(60, 100))));
    assert!(reads(object(4), Some(range(10, 40))));
    assert!(!reads(object(4), Some(range(40, 70))));
    assert!(!reads(object(4), Some(range(49, 61))));
    // An object no grant names; the manifest object is the link's own.
    assert!(!reads(object(5), None));
    assert!(!reads(object(5), Some(range(0, 10))));
    assert!(reads(*linked.manifest_object(), None));
    assert!(reads(*linked.manifest_object(), Some(range(0, 10))));

    // The credential itself: not after it ended, not another link's, not one older than the key.
    let may =
        |claims: &CredentialClaims, now: Stamp| may_read(&linked, claims, now, &object(1), None);
    assert!(may(&claims, later(899)));
    assert!(
        !may(&claims, later(900)),
        "an ended credential reads nothing"
    );
    let other_link = CredentialClaims::new(NOW, later(900), vec![LinkScope::Read(link_id(77))]);
    assert!(!may(&other_link, later(1)));
    let no_read = CredentialClaims::new(NOW, later(900), vec![LinkScope::LogAppend(log(4))]);
    assert!(!may(&no_read, later(1)));
    let rekeyed = with_row(&linked, |row| row.keyed_at = later(10));
    assert!(!may_read(&rekeyed, &claims, later(20), &object(1), None));
}

#[test]
fn grants_coverage_agrees_with_a_bitmap_oracle_over_seeded_cases() {
    let base = anyone_link();
    let target = object(1);
    for seed in 0..3_000u64 {
        let mut rng = Rng::new(seed);
        // 0 to 5 grants on the target (an object of 64 bytes), and one on another object.
        let mut grants = Vec::new();
        let mut whole = false;
        let mut mask: u64 = 0;
        for _ in 0..rng.below(6).expect("a range") {
            if rng.chance(1, 6) {
                whole = true;
                grants.push(Grant {
                    object: target,
                    range: None,
                });
            } else {
                let start = rng.below(63).expect("a range");
                let end = start + 1 + rng.below(64 - start).expect("a range");
                mask |= span_mask(start, end);
                grants.push(Grant {
                    object: target,
                    range: Some(range(start, end)),
                });
            }
        }
        grants.push(Grant {
            object: object(2),
            range: None,
        });
        let link = with_row(&base, |row| {
            row.grants = Grants::new(grants).expect("grants");
        });
        let claims = CredentialClaims::new(NOW, later(900), vec![LinkScope::Read(*link.id())]);

        // The request: the whole object, or a range of it.
        let (asked, wanted_mask) = if rng.chance(1, 8) {
            (None, u64::MAX)
        } else {
            let start = rng.below(63).expect("a range");
            let end = start + 1 + rng.below(64 - start).expect("a range");
            (Some(range(start, end)), span_mask(start, end))
        };
        let expected = whole || (asked.is_some() && (mask & wanted_mask) == wanted_mask);
        assert_eq!(
            may_read(&link, &claims, later(1), &target, asked),
            expected,
            "seed {seed}: whole={whole} granted={mask:#x} wanted={wanted_mask:#x}"
        );
        // Another object's grant never opens the target, and the target's never opens another.
        assert!(!may_read(&link, &claims, later(1), &object(3), asked));
    }
}

/// The bits `start` to `end` (excluded) of a 64-bit mask.
fn span_mask(start: u64, end: u64) -> u64 {
    let below_end = (1u128 << end) - 1;
    let below_start = (1u128 << start) - 1;
    (below_end ^ below_start) as u64
}

#[test]
fn claims_from_a_spec_end_when_the_spec_says() {
    let spec = CredentialSpec {
        ttl: Span::minutes(15),
        scopes: vec![LinkScope::Read(link_id(1))],
    };
    let claims = CredentialClaims::from_spec(&spec, later(100)).expect("claims");
    assert_eq!(claims.issued_at(), later(100));
    assert_eq!(claims.expires_at(), later(1_000));
    assert_eq!(claims.scopes(), spec.scopes.as_slice());
    assert!(claims.reads(&link_id(1)));
    assert!(!claims.reads(&link_id(2)));
    // An end beyond the largest stamp is no credential.
    assert_eq!(
        CredentialClaims::from_spec(&spec, Stamp::from_secs(u64::MAX - 10)),
        None
    );
    assert_eq!(spec.expires_at(Stamp::from_secs(u64::MAX - 10)), None);
}

#[test]
fn a_renewal_is_cut_at_the_links_end_too() {
    let mut options = opts(Audience::Anyone);
    options.expires_at = Some(later(500));
    let link = build(options);
    let claims = claims_at(&link, 0, 900);
    for (renewed_at, ttl) in [(100u64, 400u64), (300, 200), (499, 1)] {
        assert_eq!(
            renew(&link, &claims, &at(&link, anon(), renewed_at))
                .expect("renewed")
                .ttl,
            Span::secs(ttl),
            "renewed {renewed_at} seconds in"
        );
    }
}
