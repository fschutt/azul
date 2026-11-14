//! Check 8: the human step, and the counters (planning/architecture.md section 5.12: "the counters
//! (`downloads`, `bytes_served`, `devices_seen` as hashed tokens, `first_open_at`)"; "Scanners and
//! previews": "an open counts only after the human step ... and `one_time` is used up when the first
//! complete download ends, not at `link_resolve`. So mail gateways that pre-open links and chat
//! previews consume and count nothing").
//!
//! Implements planning/architecture.md section 5.12 (the counters, "Scanners and previews"), section
//! 5.15 (M7) and section 12 (E5).

#![allow(dead_code)]

mod common;

use std::collections::BTreeSet;
use std::fs;

use azlin_links::counters::{
    admit_bytes, count_bytes_served, count_download_complete, count_open, Completed, OpenRefusal,
};
use azlin_links::credential::{may_read, CredentialClaims};
use azlin_links::notify::{link_used, tells_sharer, LinkUsed};
use azlin_links::resolve::{resolve, Asked, Caller, ResolveCtx, ResolveRefusal};
use azlin_links::{
    pause, revoke, Audience, Counters, DeviceToken, LinkRecord, LinkRow, Notify, PublicLinkId,
};
use azlin_seams::sim::Rng;
use azlin_seams::{AccountId, ByteCount, Span, Stamp};
use common::{build, creator, link_id, member, opts, root, TestEntropy, NOW};

/// Seconds in a day.
const DAY_SECS: u64 = 86_400;

/// No link is blocked.
static NO_BLOCKS: BTreeSet<PublicLinkId> = BTreeSet::new();

/// The stamp `secs` seconds after `NOW`.
fn later(secs: u64) -> Stamp {
    NOW.checked_add(Span::secs(secs)).expect("no overflow")
}

/// The context of a request for `link` by `caller` at `NOW`; `human_step` says whether it is the
/// human step.
fn ctx_for(link: &LinkRecord, caller: Caller, human_step: bool) -> ResolveCtx<'static> {
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
        human_step,
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

#[test]
fn resolve_counts_nothing_even_called_100_times() {
    let mut options = opts(Audience::SignedIn);
    options.notify = Notify::EveryOpen;
    options.open_log = true;
    options.max_devices = Some(3);
    options.expires_after_open = Some(Span::days(30));
    options.idle_expiry = Some(Span::days(30));
    options.max_downloads = Some(10);
    options.bytes_cap = Some(ByteCount::gib(1));
    let link = build(options);
    let before = link.clone();
    let hash = link.state_hash();

    let mut opened = 0;
    for i in 0..100u64 {
        let caller = if i % 3 == 2 {
            anon()
        } else {
            signed_in((i % 200) as u8)
        };
        let c = ResolveCtx {
            now: later(i * 60),
            device: Some(DeviceToken::from_bytes([(i % 5) as u8; 16])),
            human_step: i % 2 == 1,
            ..ctx_for(&link, caller, false)
        };
        if resolve(&link, &c).is_ok() {
            opened += 1;
        }
    }
    assert!(opened > 20, "the signed-in callers do resolve it");
    assert_eq!(link, before, "the record is as it was");
    assert_eq!(link.state_hash(), hash);
    assert_eq!(link.counters(), &Counters::zero());
}

#[test]
fn scanner_that_resolves_and_reads_the_manifest_changes_no_counter() {
    let mut options = opts(Audience::Anyone);
    options.notify = Notify::EveryOpen;
    options.max_devices = Some(1);
    options.expires_after_open = Some(Span::hours(1));
    options.idle_expiry = Some(Span::days(1));
    let link = build(options);
    let hash = link.state_hash();

    // A mail gateway pre-opens the link: it resolves it and reads the manifest object, and it
    // never presses a button.
    let scanner = ResolveCtx {
        device: Some(DeviceToken::from_bytes([0xAA; 16])),
        ..ctx_for(&link, anon(), false)
    };
    for secs in [0u64, 10, 600] {
        let c = ResolveCtx {
            now: later(secs),
            ..scanner.clone()
        };
        let resolved = resolve(&link, &c).expect("a scanner resolves the link");
        let claims = CredentialClaims::from_spec(&resolved.credential, later(secs))
            .expect("claims of the credential");
        assert!(
            may_read(&link, &claims, later(secs), &resolved.manifest_object, None),
            "the scanner reads the manifest object"
        );
        let served = admit_bytes(&link, ByteCount::mib(1), &c);
        assert_eq!(served, ByteCount::mib(1));
    }
    assert_eq!(link.state_hash(), hash);
    assert_eq!(link.counters(), &Counters::zero());
    assert!(link.counters().first_open_at.is_none());
    assert!(link.counters().devices_seen.is_empty());
}

#[test]
fn open_counts_only_after_the_human_step() {
    let mut options = opts(Audience::Anyone);
    options.notify = Notify::EveryOpen;
    let mut link = build(options);
    let hash = link.state_hash();

    // A page load, a preview, a scanner: not the human step.
    let page_load = ctx_for(&link, anon(), false);
    assert_eq!(
        count_open(&mut link, &page_load, false).unwrap_err(),
        OpenRefusal::NotTheHumanStep
    );
    assert_eq!(link.state_hash(), hash, "nothing was counted");

    // The "Open" button.
    let click = ctx_for(&link, anon(), true);
    let opened = count_open(&mut link, &click, false).expect("the human step counts");
    assert!(opened.first_open);
    assert!(opened.notify.is_some(), "the sharer is told of the open");
    assert_eq!(link.counters().first_open_at, Some(NOW));
    assert_ne!(link.state_hash(), hash);
}

#[test]
fn first_open_sets_first_open_at_once() {
    let mut link = anyone_link();
    for (secs, first) in [(10u64, true), (20, false), (30, false)] {
        let c = ResolveCtx {
            now: later(secs),
            ..ctx_for(&link, anon(), true)
        };
        let opened = count_open(&mut link, &c, false).expect("counted");
        assert_eq!(opened.first_open, first, "the open at {secs} seconds");
        assert_eq!(
            link.counters().first_open_at,
            Some(later(10)),
            "the first open is set once"
        );
        assert_eq!(link.counters().last_open_at, Some(later(secs)));
    }
    // A stamp that went back (a clock glitch) moves neither time back.
    let c = ResolveCtx {
        now: later(5),
        ..ctx_for(&link, anon(), true)
    };
    count_open(&mut link, &c, false).expect("counted");
    assert_eq!(link.counters().first_open_at, Some(later(10)));
    assert_eq!(link.counters().last_open_at, Some(later(30)));
}

#[test]
fn an_open_restarts_the_idle_time_and_starts_the_time_after_open() {
    let mut options = opts(Audience::Anyone);
    options.idle_expiry = Some(Span::days(7));
    options.expires_after_open = Some(Span::days(10));
    let mut link = build(options);
    let c = ResolveCtx {
        now: later(6 * DAY_SECS),
        ..ctx_for(&link, anon(), true)
    };
    count_open(&mut link, &c, false).expect("counted on day 6");
    let on = |link: &LinkRecord, day: u64| {
        let c = ResolveCtx {
            now: later(day * DAY_SECS),
            ..ctx_for(link, anon(), false)
        };
        resolve(link, &c).err()
    };
    assert_eq!(
        on(&link, 12),
        None,
        "idle time counts from the open on day 6"
    );
    assert!(matches!(
        on(&link, 13),
        Some(ResolveRefusal::Expired { .. })
    ));
    // The ten days after the first open end the link on day 16, however often it is opened.
    for day in [12u64, 18] {
        let c = ResolveCtx {
            now: later(day * DAY_SECS),
            ..ctx_for(&link, anon(), true)
        };
        let answer = count_open(&mut link, &c, false);
        if day < 16 {
            answer.expect("still open");
        } else {
            assert!(answer.is_err(), "ended on day 16");
        }
    }
}

#[test]
fn devices_seen_are_random_tokens_and_the_crate_has_no_address_type() {
    let mut entropy = TestEntropy::new(11);
    let a = DeviceToken::generate(&mut entropy).expect("a token");
    let b = DeviceToken::generate(&mut entropy).expect("a token");
    assert_ne!(a, b, "tokens are random");

    let mut options = opts(Audience::Anyone);
    options.max_devices = Some(5);
    let mut link = build(options);
    for token in [a, b] {
        let c = ResolveCtx {
            device: Some(token),
            ..ctx_for(&link, anon(), true)
        };
        count_open(&mut link, &c, false).expect("counted");
    }
    let seen = &link.counters().devices_seen;
    assert_eq!(seen.len(), 2);
    for token in [a, b] {
        assert!(seen.contains(&token.seen_by(link.id())));
        assert!(
            seen.iter().all(|kept| kept.as_bytes() != token.as_bytes()),
            "the row keeps a hash of the token with the link's id, never the token"
        );
    }
    assert_ne!(
        a.seen_by(&link_id(2)),
        a.seen_by(link.id()),
        "the same device is two unrelated values on two links"
    );

    // The crate has no address type: its source names none.
    for entry in fs::read_dir(root().join("src")).expect("src") {
        let path = entry.expect("an entry").path();
        let text = fs::read_to_string(&path).expect("a source file");
        for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
            for banned in [
                "IpAddr",
                "SocketAddr",
                "Ipv4Addr",
                "Ipv6Addr",
                "std::net",
                "ToSocketAddrs",
            ] {
                assert!(
                    !line.contains(banned),
                    "{}: `{banned}` in {line}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn one_time_is_used_up_when_the_first_complete_download_ends() {
    let mut options = opts(Audience::Anyone);
    options.one_time = true;
    let mut link = build(options);
    let click = ctx_for(&link, anon(), true);

    // Two people open it before anyone has finished: an open does not use the link up.
    count_open(&mut link, &click, false).expect("the first opens");
    assert!(
        !link.is_revoked(),
        "an open does not use up a one-time link"
    );
    count_open(&mut link, &click, false).expect("the second opens");
    assert!(resolve(&link, &ctx_for(&link, anon(), false)).is_ok());

    count_bytes_served(&mut link, ByteCount::kib(10));
    assert!(!link.is_revoked(), "a download in flight uses nothing up");

    // The first complete download uses it up.
    let done = count_download_complete(&mut link, ByteCount::kib(5), later(100));
    assert_eq!(
        done,
        Completed::Counted {
            downloads: 1,
            used_up: true
        }
    );
    assert!(link.is_revoked());
    assert_eq!(link.revoked_at(), Some(later(100)));
    assert_eq!(
        resolve(&link, &ctx_for(&link, anon(), false)).unwrap_err(),
        ResolveRefusal::OneTimeUsed
    );
    assert_eq!(
        admit_bytes(&link, ByteCount::kib(1), &click),
        ByteCount::ZERO,
        "the second person's download stops"
    );
    let second = count_download_complete(&mut link, ByteCount::kib(5), later(101));
    assert_eq!(second, Completed::Refused(ResolveRefusal::OneTimeUsed));
    assert_eq!(link.counters().downloads, 1);
    assert_eq!(
        link.revoked_at(),
        Some(later(100)),
        "the first stamp stands"
    );
}

#[test]
fn count_download_complete_adds_bytes_and_downloads() {
    let mut link = anyone_link();
    count_bytes_served(&mut link, ByteCount::kib(100));
    assert_eq!(link.counters().bytes_served, ByteCount::kib(100));
    assert_eq!(link.counters().downloads, 0);

    let done = count_download_complete(&mut link, ByteCount::kib(28), later(1));
    assert_eq!(
        done,
        Completed::Counted {
            downloads: 1,
            used_up: false
        }
    );
    assert_eq!(link.counters().downloads, 1);
    assert_eq!(link.counters().bytes_served, ByteCount::kib(128));

    let done = count_download_complete(&mut link, ByteCount::kib(2), later(2));
    assert_eq!(
        done,
        Completed::Counted {
            downloads: 2,
            used_up: false
        }
    );
    assert_eq!(link.counters().bytes_served, ByteCount::kib(130));
    assert!(!link.is_revoked(), "an ordinary link is not used up");
}

#[test]
fn a_completion_after_the_link_ended_counts_no_download() {
    let mut link = anyone_link();
    count_bytes_served(&mut link, ByteCount::kib(10));
    revoke(&mut link, &creator(), later(5)).expect("revoked");
    let done = count_download_complete(&mut link, ByteCount::kib(2), later(6));
    assert_eq!(done, Completed::Refused(ResolveRefusal::Revoked));
    assert_eq!(link.counters().downloads, 0);
    assert_eq!(
        link.counters().bytes_served,
        ByteCount::kib(12),
        "the bytes were served and count"
    );
    assert_eq!(
        link.revoked_at(),
        Some(later(5)),
        "the first revocation stands"
    );
}

#[test]
fn bytes_served_counts_partial_downloads_and_the_cap_holds() {
    let mut options = opts(Audience::Anyone);
    options.bytes_cap = Some(ByteCount::kib(100));
    let mut link = build(options);
    // A client that never finishes: it asks 40 KiB at a time, again and again.
    let mut served = ByteCount::ZERO;
    for _ in 0..10 {
        let c = ctx_for(&link, anon(), false);
        let got = admit_bytes(&link, ByteCount::kib(40), &c);
        count_bytes_served(&mut link, got);
        served = served.saturating_add(got);
    }
    assert_eq!(
        served,
        ByteCount::kib(100),
        "the cap holds for downloads that never complete"
    );
    assert_eq!(link.counters().downloads, 0);
    assert_eq!(
        resolve(&link, &ctx_for(&link, anon(), false)).unwrap_err(),
        ResolveRefusal::BytesCapExhausted
    );
}

#[test]
fn admit_bytes_stops_a_download_in_flight_when_the_link_ends_or_stops() {
    let ask = ByteCount::kib(8);
    let link = anyone_link();
    let c = ctx_for(&link, anon(), false);
    assert_eq!(admit_bytes(&link, ask, &c), ask);

    let mut revoked = anyone_link();
    revoke(&mut revoked, &creator(), later(5)).expect("revoked");
    assert_eq!(admit_bytes(&revoked, ask, &c), ByteCount::ZERO);

    let mut paused = anyone_link();
    pause(&mut paused, &creator()).expect("paused");
    assert_eq!(admit_bytes(&paused, ask, &c), ByteCount::ZERO);

    let blocks: BTreeSet<PublicLinkId> = [*link.id()].into_iter().collect();
    let blocked = ResolveCtx {
        blocks: &blocks,
        ..c.clone()
    };
    assert_eq!(admit_bytes(&link, ask, &blocked), ByteCount::ZERO);

    let mut options = opts(Audience::Anyone);
    options.expires_at = Some(later(100));
    let dated = build(options);
    let before = ResolveCtx {
        now: later(99),
        ..ctx_for(&dated, anon(), false)
    };
    let after = ResolveCtx {
        now: later(100),
        ..ctx_for(&dated, anon(), false)
    };
    assert_eq!(admit_bytes(&dated, ask, &before), ask);
    assert_eq!(admit_bytes(&dated, ask, &after), ByteCount::ZERO);
}

#[test]
fn a_refused_open_changes_nothing() {
    // Each case is a link, the request and the acknowledgement; every one is refused.
    let mut revoked = anyone_link();
    revoke(&mut revoked, &creator(), later(5)).expect("revoked");

    let mut logged = opts(Audience::SignedIn);
    logged.open_log = true;
    let logged = build(logged);

    let mut limited = opts(Audience::Anyone);
    limited.max_devices = Some(1);
    let limited = build(limited);

    let anyone = anyone_link();
    let cases = [
        (revoked.clone(), ctx_for(&revoked, anon(), true), false),
        (
            anyone.clone(),
            ResolveCtx {
                asked: Asked::Id(link_id(2)),
                ..ctx_for(&anyone, anon(), true)
            },
            false,
        ),
        (logged.clone(), ctx_for(&logged, signed_in(5), true), false),
        (logged.clone(), ctx_for(&logged, anon(), true), true),
        (limited.clone(), ctx_for(&limited, anon(), true), false),
        (anyone.clone(), ctx_for(&anyone, anon(), false), false),
    ];
    for (n, (mut link, c, ack)) in cases.into_iter().enumerate() {
        let before = link.state_hash();
        assert!(
            count_open(&mut link, &c, ack).is_err(),
            "case {n} is refused"
        );
        assert_eq!(link.state_hash(), before, "case {n} changed the record");
    }
}

#[test]
fn notify_never_first_every() {
    for (notify, told_first, told_second) in [
        (Notify::Never, false, false),
        (Notify::FirstOpen, true, false),
        (Notify::EveryOpen, true, true),
    ] {
        let mut options = opts(Audience::Anyone);
        options.notify = notify;
        let mut link = build(options);
        let c = ctx_for(&link, anon(), true);
        let first = count_open(&mut link, &c, false).expect("counted");
        let second = count_open(&mut link, &c, false).expect("counted");
        assert_eq!(first.notify.is_some(), told_first, "{notify:?}: first open");
        assert_eq!(
            second.notify.is_some(),
            told_second,
            "{notify:?}: second open"
        );
        if let Some(event) = first.notify {
            assert_eq!(
                event,
                LinkUsed {
                    link: *link.id(),
                    downloads: 0
                }
            );
        }
    }

    // The event says how many complete downloads the link has counted.
    let mut options = opts(Audience::Anyone);
    options.notify = Notify::EveryOpen;
    let link = build(options);
    let mut used = with_row(&link, |row| row.counters.downloads = 4);
    let c = ctx_for(&used, anon(), true);
    let opened = count_open(&mut used, &c, false).expect("counted");
    assert_eq!(
        opened.notify,
        Some(LinkUsed {
            link: *used.id(),
            downloads: 4
        })
    );

    // The decision itself, as a table.
    for first_open in [true, false] {
        assert!(!tells_sharer(Notify::Never, first_open));
        assert_eq!(tells_sharer(Notify::FirstOpen, first_open), first_open);
        assert!(tells_sharer(Notify::EveryOpen, first_open));
        assert_eq!(
            link_used(Notify::EveryOpen, link_id(3), first_open, 9),
            Some(LinkUsed {
                link: link_id(3),
                downloads: 9
            })
        );
        assert_eq!(link_used(Notify::Never, link_id(3), first_open, 9), None);
    }
}

#[test]
fn max_downloads_n_admits_exactly_n_completions_over_5000_seeded_schedules() {
    for seed in 0..5_000u64 {
        let mut rng = Rng::new(seed);
        let n = 1 + rng.below(6).expect("a range") as u32;
        let clients = 1 + rng.below(10).expect("a range") as usize;
        let mut options = opts(Audience::Anyone);
        options.max_downloads = Some(n);
        let mut link = build(options);

        // The phase of each client: 0 new, 1 opened, 2 serving, 3 finished.
        let mut phase = vec![0u8; clients];
        let mut counted = 0u32;
        let mut attempts = 0u32;
        let mut step = 0u64;
        while phase.iter().any(|p| *p < 3) {
            let waiting: Vec<usize> = (0..clients).filter(|i| phase[*i] < 3).collect();
            let who = *rng.pick(&waiting).expect("a client waits");
            let c = ResolveCtx {
                now: later(step),
                ..ctx_for(&link, anon(), true)
            };
            match phase[who] {
                0 => match count_open(&mut link, &c, false) {
                    Ok(_) => phase[who] = 1,
                    Err(refusal) => {
                        assert_eq!(
                            refusal,
                            OpenRefusal::Link(ResolveRefusal::DownloadsExhausted),
                            "seed {seed}"
                        );
                        phase[who] = 3;
                    }
                },
                1 => {
                    let got = admit_bytes(&link, ByteCount::kib(64), &c);
                    if got.is_zero() {
                        assert_eq!(
                            link.counters().downloads,
                            u64::from(n),
                            "seed {seed}: a download stops only when the limit is reached"
                        );
                        phase[who] = 3;
                    } else {
                        count_bytes_served(&mut link, got);
                        phase[who] = 2;
                    }
                }
                _ => {
                    attempts += 1;
                    match count_download_complete(&mut link, ByteCount::kib(1), later(step)) {
                        Completed::Counted { downloads, .. } => {
                            counted += 1;
                            assert_eq!(downloads, u64::from(counted), "seed {seed}");
                        }
                        Completed::Refused(refusal) => {
                            assert_eq!(refusal, ResolveRefusal::DownloadsExhausted, "seed {seed}");
                        }
                    }
                    phase[who] = 3;
                }
            }
            assert!(
                link.counters().downloads <= u64::from(n),
                "seed {seed}: more downloads than max_downloads"
            );
            step += 1;
        }
        assert_eq!(
            counted,
            n.min(attempts),
            "seed {seed}: N completions are admitted, no more and no fewer"
        );
        assert_eq!(link.counters().downloads, u64::from(counted), "seed {seed}");
    }
}

#[test]
fn extreme_values_never_panic_and_never_wrap() {
    let link = anyone_link();
    let largest = Stamp::from_secs(u64::MAX);

    // The largest stamp: the daily cap answers a delay between a second and a day; without a cap the
    // credential lives its 15 minutes and its end lies beyond the largest stamp.
    let capped = ResolveCtx {
        now: largest,
        served_today: ByteCount::MAX,
        daily_cap: ByteCount::MAX,
        ..ctx_for(&link, anon(), true)
    };
    match resolve(&link, &capped).unwrap_err() {
        ResolveRefusal::DailyCapReached { retry_after } => {
            assert!(retry_after >= Span::secs(1));
            assert!(retry_after <= Span::days(1));
        }
        other => panic!("expected the daily cap, got {other:?}"),
    }
    let open = ResolveCtx {
        now: largest,
        ..ctx_for(&link, anon(), true)
    };
    let resolved = resolve(&link, &open).expect("opens");
    assert_eq!(resolved.credential.ttl, Span::minutes(15));
    assert_eq!(resolved.credential.expires_at(largest), None);
    let mut opened = link.clone();
    count_open(&mut opened, &open, false).expect("counted at the largest stamp");
    assert_eq!(opened.counters().first_open_at, Some(largest));

    // Counters saturate, they never wrap.
    let mut full = with_row(&link, |row| {
        row.counters.downloads = u64::MAX;
        row.counters.bytes_served = ByteCount::MAX;
    });
    count_bytes_served(&mut full, ByteCount::MAX);
    assert_eq!(full.counters().bytes_served, ByteCount::MAX);
    let done = count_download_complete(&mut full, ByteCount::MAX, later(1));
    assert_eq!(
        done,
        Completed::Counted {
            downloads: u64::MAX,
            used_up: false
        }
    );
    assert_eq!(full.counters().bytes_served, ByteCount::MAX);

    // A cap and a request of the largest size.
    let mut options = opts(Audience::Anyone);
    options.bytes_cap = Some(ByteCount::MAX);
    let big = build(options);
    let c = ResolveCtx {
        daily_cap: ByteCount::MAX,
        ..ctx_for(&big, anon(), false)
    };
    assert_eq!(admit_bytes(&big, ByteCount::MAX, &c), ByteCount::MAX);
}
