//! The open log (planning/architecture.md section 5.12: "`open_log` | signed-in audiences only, off by
//! default: which account or member opened it and when; the recipient is told before opening (M7)";
//! section 5.15, M7: "link counters and caps only. The one exception is the open log (5.12):
//! signed-in audiences only, off by default, and shown to the recipient before opening").
//!
//! Implements planning/architecture.md section 5.12 (`open_log`) and section 5.15 (M7).

#![allow(dead_code)]

mod common;

use std::collections::BTreeSet;

use azlin_links::counters::{count_open, OpenRefusal};
use azlin_links::open_log::{entry_for, is_logged, OpenLogEntry};
use azlin_links::resolve::{resolve, Asked, Caller, ExpiredBy, ResolveCtx, ResolveRefusal};
use azlin_links::{update, validate, Audience, Change, LinkRecord, OptionError, PublicLinkId};
use azlin_policy::LinkPolicy;
use azlin_seams::{AccountId, ByteCount, Span, Stamp};
use common::{build, creator, group, groups, member, opts, people, plain_facts, wrap, NOW};

/// No link is blocked.
static NO_BLOCKS: BTreeSet<PublicLinkId> = BTreeSet::new();

/// The stamp `secs` seconds after `NOW`.
fn later(secs: u64) -> Stamp {
    NOW.checked_add(Span::secs(secs)).expect("no overflow")
}

/// The account of the callers of this file.
fn account() -> AccountId {
    AccountId::from_bytes([200; 16])
}

/// The context of a request for `link` by `caller` at `NOW`.
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

/// A member of another account.
fn signed_in(n: u8) -> Caller {
    Caller::SignedIn {
        account: account(),
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

/// The signed-in audiences, each as a link that keeps an open log, with a caller that is in it.
fn logged_links() -> Vec<(&'static str, LinkRecord, Caller)> {
    let logged = |audience: Audience| {
        let mut options = opts(audience);
        options.open_log = true;
        options
    };
    let mut approval = build(logged(Audience::Approval));
    update(
        &mut approval,
        Change::AddWrap(wrap(77)),
        &creator(),
        &LinkPolicy::allow_all(),
        NOW,
    )
    .expect("approved");
    vec![
        ("signed_in", build(logged(Audience::SignedIn)), signed_in(5)),
        (
            "members",
            build(logged(groups(&[1, 2]))),
            account_member(20, &[1]),
        ),
        ("people", build(logged(people(&[10, 11]))), signed_in(10)),
        ("approval", approval, signed_in(77)),
    ]
}

#[test]
fn open_log_entry_only_for_signed_in_audiences() {
    for (name, link, caller) in logged_links() {
        assert!(is_logged(&link), "{name}");
        let entry = entry_for(&link, &caller, later(5)).expect("a signed-in caller is logged");
        assert_eq!(
            entry,
            OpenLogEntry {
                link: *link.id(),
                account: *caller.account().expect("an account"),
                member: *caller.member().expect("a member"),
                at: later(5),
            },
            "{name}: which account or member opened it, and when"
        );
        assert_eq!(
            entry_for(&link, &Caller::Anonymous, later(5)),
            None,
            "{name}: an anonymous caller has no account to log"
        );
    }

    // `anyone` can keep no open log: the rule between options refuses it.
    let mut options = opts(Audience::Anyone);
    options.open_log = true;
    let errors = validate(&options, &plain_facts()).expect_err("an open log for anyone");
    assert!(errors.contains(&OptionError::OpenLogNeedsSignedInAudience));

    // A link that keeps no log logs nobody, whoever opens it.
    for audience in [
        Audience::Anyone,
        Audience::SignedIn,
        groups(&[1, 2]),
        people(&[10, 11]),
    ] {
        let link = build(opts(audience));
        assert!(!is_logged(&link), "an open log is off by default");
        for caller in [Caller::Anonymous, signed_in(10), account_member(20, &[1])] {
            assert_eq!(entry_for(&link, &caller, NOW), None);
        }
    }
}

#[test]
fn open_on_a_logged_link_needs_the_acknowledgement() {
    for (name, mut link, caller) in logged_links() {
        let hash = link.state_hash();
        let click = ctx_for(&link, caller.clone(), true);

        // "The recipient is told before opening": without the acknowledgement nothing is counted.
        assert_eq!(
            count_open(&mut link, &click, false).unwrap_err(),
            OpenRefusal::OpenLogNotAcknowledged,
            "{name}"
        );
        assert_eq!(link.state_hash(), hash, "{name}: nothing was counted");

        // With it, the open is counted and logged.
        let opened = count_open(&mut link, &click, true).expect("acknowledged");
        assert_eq!(
            opened.log_entry,
            Some(OpenLogEntry {
                link: *link.id(),
                account: *caller.account().expect("an account"),
                member: *caller.member().expect("a member"),
                at: NOW,
            }),
            "{name}"
        );
        assert_eq!(link.counters().first_open_at, Some(NOW));
    }

    // Acknowledging a link that keeps no log is harmless and logs nothing.
    let mut plain = build(opts(Audience::SignedIn));
    let click = ctx_for(&plain, signed_in(5), true);
    let opened = count_open(&mut plain, &click, true).expect("counted");
    assert_eq!(opened.log_entry, None);
}

#[test]
fn a_refused_rule_is_told_before_the_acknowledgement_is_asked() {
    let mut options = opts(Audience::SignedIn);
    options.open_log = true;
    options.expires_at = Some(later(100));
    let mut link = build(options);
    let late = ResolveCtx {
        now: later(200),
        ..ctx_for(&link, signed_in(5), true)
    };
    assert_eq!(
        count_open(&mut link, &late, false).unwrap_err(),
        OpenRefusal::Link(ResolveRefusal::Expired {
            by: ExpiredBy::Date
        }),
        "the link has ended: the recipient is not asked about a log of a link that does not open"
    );
}

#[test]
fn the_recipient_is_told_before_opening() {
    for (name, link, caller) in logged_links() {
        let resolved = resolve(&link, &ctx_for(&link, caller, false)).expect("opens");
        assert!(
            resolved.tell_recipient_open_log,
            "{name}: the app shows the notice before the human step"
        );
    }
    let plain = build(opts(Audience::SignedIn));
    let resolved = resolve(&plain, &ctx_for(&plain, signed_in(5), false)).expect("opens");
    assert!(!resolved.tell_recipient_open_log);
}

#[test]
fn each_open_has_its_own_entry() {
    let mut options = opts(Audience::SignedIn);
    options.open_log = true;
    let mut link = build(options);
    let mut entries = Vec::new();
    for (n, secs) in [(5u8, 10u64), (6, 20), (5, 30)] {
        let c = ResolveCtx {
            now: later(secs),
            ..ctx_for(&link, signed_in(n), true)
        };
        let opened = count_open(&mut link, &c, true).expect("counted");
        entries.push(opened.log_entry.expect("a logged open"));
    }
    let seen: Vec<(u8, Stamp)> = entries
        .iter()
        .map(|entry| (entry.member.as_bytes()[0], entry.at))
        .collect();
    assert_eq!(seen, vec![(5, later(10)), (6, later(20)), (5, later(30))]);
}

#[test]
fn an_entry_names_the_link_the_account_the_member_and_the_time_and_nothing_else() {
    // An exhaustive destructuring: a field added to the entry (a device, an address) does not
    // compile until this test, and the review it calls for, has seen it.
    let entry = OpenLogEntry {
        link: *build(opts(Audience::SignedIn)).id(),
        account: account(),
        member: member(5),
        at: NOW,
    };
    let OpenLogEntry {
        link: _,
        account: _,
        member: _,
        at: _,
    } = entry;
    let copy = entry;
    assert_eq!(entry, copy);
}
