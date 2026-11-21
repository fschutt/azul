//! Checks R1 to R10 of lane W1-S46a: the counting engine (planning/architecture.md sections 3.9, 5.3, 5.12).
//! Every test enters through the public API of the crate.

mod common;

use std::collections::BTreeSet;

use azlin_reports::{
    AutoStep, Bound, Config, Day, Days, DeviceRef, Ledger, LedgerError, LinkRef, ReportEvent, Stats,
};

use common::*;

/// One report: `report` is only the caller's own key.
fn ev(report: u8, link_id: Option<u8>, account: u8, device: u8, on: u32) -> ReportEvent {
    ReportEvent {
        report: rid(report),
        link: link_id.map(link),
        account: acct(account),
        reporter: dev(device),
        day: day(on),
    }
}

/// A ledger whose account step cannot fire, to look at the link count alone.
fn links_only() -> Ledger {
    Ledger::new(Config {
        account_devices: 1_000,
        ..Config::default()
    })
}

/// A ledger whose link step cannot fire, to look at the account count alone.
fn accounts_only() -> Ledger {
    Ledger::new(Config {
        link_devices: 1_000,
        ..Config::default()
    })
}

// ---------------------------------------------------------------- R1

#[test]
fn r1_three_different_devices_pause_a_link_two_do_not() {
    let mut ledger = Ledger::new(Config::default());
    assert_eq!(
        ledger.record(ev(1, Some(7), 1, 1, 100)),
        Ok(vec![]),
        "device A"
    );
    assert_eq!(
        ledger.record(ev(2, Some(7), 1, 2, 100)),
        Ok(vec![]),
        "device B: two do not"
    );
    assert_eq!(
        ledger.record(ev(3, Some(7), 1, 3, 100)),
        Ok(vec![AutoStep::PauseLink {
            link: link(7),
            devices: 3
        }]),
        "device C: three do"
    );
    // A fourth device adds nothing: once per breach.
    assert_eq!(ledger.record(ev(4, Some(7), 1, 4, 100)), Ok(vec![]));
    // The pause is about the link: another link (of another account, whose count is its own) is untouched.
    assert_eq!(ledger.record(ev(5, Some(8), 2, 5, 100)), Ok(vec![]));
}

#[test]
fn r1_the_default_config_is_the_plans_numbers() {
    // Planning/architecture.md 3.9: three devices pause a link, five devices within 30 days disable an account's
    // public links (both est.); the 30-day link window is the answer X-D-27; the limits are the brief's.
    assert_eq!(
        Config::default(),
        Config {
            link_devices: 3,
            link_window: Some(Days::new(30)),
            account_devices: 5,
            account_window: Days::new(30),
            max_links: 100_000,
            max_accounts: 100_000,
            max_reporters_per_key: 1_024,
        }
    );
}

#[test]
fn r1_the_threshold_comes_from_the_config() {
    for threshold in 1..=6u32 {
        let mut ledger = Ledger::new(Config {
            link_devices: threshold,
            account_devices: 1_000,
            ..Config::default()
        });
        for n in 1..threshold {
            let steps = ledger.record(ev(n as u8, Some(1), 1, n as u8, 5)).unwrap();
            assert!(
                steps.is_empty(),
                "threshold {threshold}: device {n} paused the link"
            );
        }
        let steps = ledger
            .record(ev(99, Some(1), 1, threshold as u8, 5))
            .unwrap();
        assert_eq!(
            steps,
            vec![AutoStep::PauseLink {
                link: link(1),
                devices: threshold
            }],
            "threshold {threshold}"
        );
    }
}

// ---------------------------------------------------------------- R2

#[test]
fn r2_one_device_many_times_counts_once() {
    let mut ledger = links_only();
    for n in 0..50u8 {
        let steps = ledger
            .record(ev(n, Some(3), 1, 1, 100 + u32::from(n % 5)))
            .unwrap();
        assert!(steps.is_empty(), "report {n} of one device paused the link");
    }
    // The 50 reports did not inflate the count: it still takes two more devices.
    assert_eq!(ledger.record(ev(60, Some(3), 1, 2, 105)), Ok(vec![]));
    assert_eq!(
        ledger.record(ev(61, Some(3), 1, 3, 105)),
        Ok(vec![AutoStep::PauseLink {
            link: link(3),
            devices: 3
        }])
    );
}

// ---------------------------------------------------------------- R3

#[test]
fn r3_five_devices_in_thirty_days_disable_links_four_do_not() {
    // Five devices over five different links of one account: four do not, the fifth does.
    let mut ledger = accounts_only();
    for n in 1..=4u8 {
        let steps = ledger
            .record(ev(n, Some(10 + n), 9, n, 200 + u32::from(n)))
            .unwrap();
        assert!(
            steps.is_empty(),
            "device {n}: four devices disabled the links"
        );
    }
    assert_eq!(
        ledger.record(ev(5, Some(15), 9, 5, 205)),
        Ok(vec![AutoStep::DisableLinks {
            account: acct(9),
            devices: 5
        }])
    );
    // The same device on two links counts once for the account: A on links 1 and 2, then B, C, D: four.
    let mut ledger = accounts_only();
    let reports = [(1u8, 1u8), (1, 2), (2, 3), (3, 4), (4, 5)];
    for (i, (device, on_link)) in reports.into_iter().enumerate() {
        let steps = ledger
            .record(ev(i as u8, Some(on_link), 9, device, 300))
            .unwrap();
        assert!(
            steps.is_empty(),
            "report {i}: a device counted twice for the account"
        );
    }
    assert_eq!(
        ledger.record(ev(9, Some(6), 9, 5, 300)),
        Ok(vec![AutoStep::DisableLinks {
            account: acct(9),
            devices: 5
        }])
    );
    // Another account's reports are counted on their own.
    let mut ledger = accounts_only();
    for n in 1..=4u8 {
        assert!(ledger.record(ev(n, Some(n), 1, n, 10)).unwrap().is_empty());
        assert!(ledger
            .record(ev(n, Some(n), 2, n + 10, 10))
            .unwrap()
            .is_empty());
    }
    // A report without a link (a member report) counts for the account too.
    assert_eq!(
        ledger.record(ev(77, None, 1, 5, 10)),
        Ok(vec![AutoStep::DisableLinks {
            account: acct(1),
            devices: 5
        }])
    );
}

#[test]
fn r3_the_link_and_the_account_step_can_fire_on_one_report_the_link_first() {
    let mut ledger = Ledger::new(Config {
        link_devices: 1,
        account_devices: 1,
        ..Config::default()
    });
    assert_eq!(
        ledger.record(ev(1, Some(4), 2, 1, 50)),
        Ok(vec![
            AutoStep::PauseLink {
                link: link(4),
                devices: 1
            },
            AutoStep::DisableLinks {
                account: acct(2),
                devices: 1
            },
        ])
    );
}

// ---------------------------------------------------------------- R4

/// Five devices on five links of one account (so no link pauses): the first on `first`, the other four on the
/// next days, the fifth on `fifth`. The steps of the fifth report.
fn account_edge(first: u32, fifth: u32) -> Vec<AutoStep> {
    let mut ledger = accounts_only();
    assert!(ledger
        .record(ev(1, Some(1), 1, 1, first))
        .unwrap()
        .is_empty());
    for n in 2..=4u8 {
        let steps = ledger
            .record(ev(n, Some(n), 1, n, first + u32::from(n) - 1))
            .unwrap();
        assert!(steps.is_empty());
    }
    ledger.record(ev(5, Some(5), 1, 5, fifth)).unwrap()
}

#[test]
fn r4_the_window_edges_are_exact() {
    let d = 1_000;
    let disables = [AutoStep::DisableLinks {
        account: acct(1),
        devices: 5,
    }];
    // The fifth device arrives on d + 29: the first (on d) still counts: five devices, it disables.
    assert_eq!(account_edge(d, d + 29), disables);
    // On d + 30 the first has left: four devices, nothing.
    assert_eq!(account_edge(d, d + 30), Vec::<AutoStep>::new());
    // On the days in between too.
    for fifth in d + 3..=d + 29 {
        assert_eq!(
            account_edge(d, fifth),
            disables,
            "fifth device on day {fifth}"
        );
    }
    for fifth in d + 30..=d + 40 {
        assert_eq!(
            account_edge(d, fifth),
            Vec::<AutoStep>::new(),
            "fifth device on day {fifth}"
        );
    }
    // The same with the link window (30 days by default): three devices, the first on d.
    let pause = |first: u32, third: u32| -> Vec<AutoStep> {
        let mut ledger = links_only();
        assert!(ledger
            .record(ev(1, Some(1), 1, 1, first))
            .unwrap()
            .is_empty());
        assert!(ledger
            .record(ev(2, Some(1), 1, 2, first + 1))
            .unwrap()
            .is_empty());
        ledger.record(ev(3, Some(1), 1, 3, third)).unwrap()
    };
    let paused = [AutoStep::PauseLink {
        link: link(1),
        devices: 3,
    }];
    assert_eq!(pause(d, d + 29), paused);
    assert_eq!(pause(d, d + 30), Vec::<AutoStep>::new());
    // The window length is the config's: a window of 1 day counts the day itself only.
    let mut ledger = Ledger::new(Config {
        link_devices: 2,
        link_window: Some(Days::new(1)),
        account_devices: 1_000,
        ..Config::default()
    });
    assert!(ledger.record(ev(1, Some(1), 1, 1, 10)).unwrap().is_empty());
    assert!(
        ledger.record(ev(2, Some(1), 1, 2, 11)).unwrap().is_empty(),
        "day 10 left a 1-day window"
    );
    assert_eq!(
        ledger.record(ev(3, Some(1), 1, 3, 11)),
        Ok(vec![AutoStep::PauseLink {
            link: link(1),
            devices: 2
        }])
    );
}

#[test]
fn r4_day_arithmetic_is_exact_and_never_wraps() {
    let d = Day::new(1_000);
    let w = Days::new(30);
    assert_eq!(d.get(), 1_000);
    assert_eq!(w.get(), 30);
    assert_eq!(d.plus(Days::new(29)), Day::new(1_029));
    assert_eq!(
        Day::new(u32::MAX - 1).plus(Days::new(30)),
        Day::new(u32::MAX),
        "a sum stops at the last day instead of wrapping"
    );
    assert_eq!(d.since(Day::new(990)), Some(Days::new(10)));
    assert_eq!(d.since(d), Some(Days::new(0)));
    assert_eq!(
        d.since(Day::new(1_001)),
        None,
        "an earlier day is not after a later one"
    );
    // A report on day d counts through d + 29 inclusive and not on d + 30.
    assert!(d.within(d, w));
    assert!(Day::new(1_029).within(d, w));
    assert!(!Day::new(1_030).within(d, w));
    assert!(!Day::new(5_000).within(d, w));
    // A day later than the one asked from has not left any window.
    assert!(d.within(Day::new(1_001), w));
    // A window of no days holds nothing, not even the day itself.
    assert!(!d.within(d, Days::new(0)));
    assert!(d < Day::new(1_001) && Days::new(2) < Days::new(3));
}

#[test]
fn r4_without_a_link_window_a_link_waits_until_decided() {
    // The plan's words: "pauses until decided" (drift X-DELTA-W1S46A-2): with `None` no report leaves.
    let mut ledger = Ledger::new(Config {
        link_window: None,
        account_devices: 1_000,
        ..Config::default()
    });
    assert!(ledger.record(ev(1, Some(1), 1, 1, 1)).unwrap().is_empty());
    assert!(ledger.record(ev(2, Some(1), 1, 2, 400)).unwrap().is_empty());
    ledger.expire(day(5_000));
    assert_eq!(
        ledger.record(ev(3, Some(1), 1, 3, 5_000)),
        Ok(vec![AutoStep::PauseLink {
            link: link(1),
            devices: 3
        }])
    );
}

// ---------------------------------------------------------------- R5

#[test]
fn r5_steps_fire_once_not_on_every_report() {
    let mut ledger = links_only();
    for n in 1..=2u8 {
        assert!(ledger.record(ev(n, Some(1), 1, n, 10)).unwrap().is_empty());
    }
    assert_eq!(
        ledger.record(ev(3, Some(1), 1, 3, 10)),
        Ok(vec![AutoStep::PauseLink {
            link: link(1),
            devices: 3
        }])
    );
    // After PauseLink further reports of the same link return no step until `decided`: new devices, the
    // same devices, other days.
    for n in 4..=40u8 {
        let steps = ledger
            .record(ev(n, Some(1), 1, n, 10 + u32::from(n)))
            .unwrap();
        assert!(steps.is_empty(), "report {n} of a paused link fired again");
    }
    for n in 1..=3u8 {
        assert!(ledger.record(ev(n, Some(1), 1, n, 80)).unwrap().is_empty());
    }
    // The account step fires once as well (default config: five devices).
    let mut ledger = Ledger::new(Config::default());
    let mut fired = 0;
    for n in 1..=30u8 {
        let steps = ledger.record(ev(n, Some(n), 1, n, 10)).unwrap();
        fired += steps
            .iter()
            .filter(|s| matches!(s, AutoStep::DisableLinks { .. }))
            .count();
    }
    assert_eq!(fired, 1, "the account's step fired {fired} times");
}

// ---------------------------------------------------------------- R6

#[test]
fn r6_a_decision_restarts_the_link_count() {
    let mut ledger = links_only();
    for n in 1..=3u8 {
        ledger.record(ev(n, Some(1), 1, n, 10)).unwrap();
    }
    ledger.decided(&link(1), day(11));
    // Three NEW reports are needed: the same devices reporting again count, but one or two do not.
    assert!(ledger.record(ev(11, Some(1), 1, 1, 11)).unwrap().is_empty());
    assert!(ledger.record(ev(12, Some(1), 1, 2, 11)).unwrap().is_empty());
    assert_eq!(
        ledger.record(ev(13, Some(1), 1, 3, 11)),
        Ok(vec![AutoStep::PauseLink {
            link: link(1),
            devices: 3
        }]),
        "a report on the decision day itself counts for the next round"
    );

    // Reports made before the decision no longer count: two devices, a decision, then one more is not three.
    let mut ledger = links_only();
    assert!(ledger.record(ev(1, Some(2), 1, 1, 5)).unwrap().is_empty());
    assert!(ledger.record(ev(2, Some(2), 1, 2, 5)).unwrap().is_empty());
    ledger.decided(&link(2), day(6));
    assert!(
        ledger.record(ev(3, Some(2), 1, 3, 6)).unwrap().is_empty(),
        "earlier reports still counted"
    );
    assert!(ledger.record(ev(4, Some(2), 1, 4, 6)).unwrap().is_empty());
    assert_eq!(
        ledger.record(ev(5, Some(2), 1, 5, 6)),
        Ok(vec![AutoStep::PauseLink {
            link: link(2),
            devices: 3
        }])
    );
    // A decision on one link does not touch another link's count.
    let mut ledger = links_only();
    assert!(ledger.record(ev(1, Some(1), 1, 1, 5)).unwrap().is_empty());
    assert!(ledger.record(ev(2, Some(1), 1, 2, 5)).unwrap().is_empty());
    ledger.decided(&link(9), day(5));
    assert_eq!(
        ledger.record(ev(3, Some(1), 1, 3, 5)),
        Ok(vec![AutoStep::PauseLink {
            link: link(1),
            devices: 3
        }])
    );
}

#[test]
fn r6_a_decision_does_not_restart_the_accounts_count() {
    let mut ledger = Ledger::new(Config::default());
    for n in 1..=4u8 {
        ledger.record(ev(n, Some(n), 1, n, 10)).unwrap();
    }
    ledger.decided(&link(1), day(11));
    assert_eq!(
        ledger.record(ev(5, Some(5), 1, 5, 11)),
        Ok(vec![AutoStep::DisableLinks {
            account: acct(1),
            devices: 5
        }]),
        "the four reports before the decision still count for the account"
    );
}

// ---------------------------------------------------------------- R7

#[test]
fn r7_links_disabled_stays_until_the_operator_enables() {
    let mut ledger = accounts_only();
    for n in 1..=4u8 {
        assert!(ledger.record(ev(n, Some(n), 1, n, 10)).unwrap().is_empty());
    }
    assert_eq!(
        ledger.record(ev(5, Some(5), 1, 5, 10)),
        Ok(vec![AutoStep::DisableLinks {
            account: acct(1),
            devices: 5
        }])
    );
    // After DisableLinks more reports return no step: other devices, other links, much later.
    for n in 6..=30u8 {
        assert!(ledger
            .record(ev(n, Some(n), 1, n, 10 + u32::from(n)))
            .unwrap()
            .is_empty());
    }
    ledger.expire(day(5_000));
    assert!(
        ledger
            .record(ev(40, Some(40), 1, 40, 5_000))
            .unwrap()
            .is_empty(),
        "the switch stays until the operator acts; the window does not lift it"
    );
    assert_eq!(ledger.stats().disabled_accounts, 1);
    // `links_enabled` restarts the account's count: five new devices are needed.
    ledger.links_enabled(&acct(1), day(5_001));
    assert_eq!(ledger.stats().disabled_accounts, 0);
    for n in 1..=4u8 {
        assert!(ledger
            .record(ev(n, Some(n), 1, n, 5_001))
            .unwrap()
            .is_empty());
    }
    assert_eq!(
        ledger.record(ev(5, Some(5), 1, 5, 5_001)),
        Ok(vec![AutoStep::DisableLinks {
            account: acct(1),
            devices: 5
        }])
    );
    // Enabling one account does not touch another.
    ledger.links_enabled(&acct(9), day(5_001));
    assert_eq!(ledger.stats().disabled_accounts, 1);
}

// ---------------------------------------------------------------- R8

fn small(max_links: usize, max_accounts: usize, max_reporters: usize) -> Config {
    Config {
        link_devices: 1_000,
        account_devices: 1_000,
        max_links,
        max_accounts,
        max_reporters_per_key: max_reporters,
        ..Config::default()
    }
}

#[test]
fn r8_the_ledger_is_bounded_and_refuses_rather_than_evicts() {
    // max_links + 1 links: a typed Full, nothing changed, the counts intact.
    let mut ledger = Ledger::new(Config {
        link_devices: 2,
        account_devices: 1_000,
        max_links: 2,
        ..Config::default()
    });
    assert!(ledger.record(ev(1, Some(1), 1, 1, 10)).unwrap().is_empty());
    assert!(ledger.record(ev(2, Some(2), 1, 1, 10)).unwrap().is_empty());
    let before = ledger.stats();
    assert_eq!(
        ledger.record(ev(3, Some(3), 1, 1, 10)),
        Err(LedgerError::Full(Bound::Links))
    );
    assert_eq!(
        ledger.stats(),
        before,
        "a refused report changed the ledger"
    );
    // Link 1 still has the device that reported it: one more device reaches two.
    assert_eq!(
        ledger.record(ev(4, Some(1), 1, 2, 10)),
        Ok(vec![AutoStep::PauseLink {
            link: link(1),
            devices: 2
        }]),
        "the count of an existing link was evicted"
    );

    // max_reporters_per_key + 1 reporters of one link.
    let mut ledger = Ledger::new(small(100, 100, 3));
    for n in 1..=3u8 {
        assert!(ledger.record(ev(n, Some(1), 1, n, 10)).unwrap().is_empty());
    }
    assert_eq!(
        ledger.record(ev(4, Some(1), 1, 4, 10)),
        Err(LedgerError::Full(Bound::ReportersOfLink))
    );
    // The reporters it holds are not evicted: they may report again, and the stats are unchanged.
    let before = ledger.stats();
    for n in 1..=3u8 {
        assert!(ledger
            .record(ev(10 + n, Some(1), 1, n, 11))
            .unwrap()
            .is_empty());
    }
    assert_eq!(ledger.stats(), before);

    // max_reporters_per_key + 1 reporters of one account: the link written by the refused report is NOT
    // created (nothing changes), and the account's reporters are intact.
    let mut ledger = Ledger::new(small(100, 100, 2));
    assert!(ledger.record(ev(1, Some(1), 5, 1, 10)).unwrap().is_empty());
    assert!(ledger.record(ev(2, Some(2), 5, 2, 10)).unwrap().is_empty());
    let before = ledger.stats();
    assert_eq!(before.links, 2);
    assert_eq!(
        ledger.record(ev(3, Some(3), 5, 3, 10)),
        Err(LedgerError::Full(Bound::ReportersOfAccount))
    );
    assert_eq!(
        ledger.stats(),
        before,
        "the refused report left a link behind"
    );
    // max_accounts + 1 accounts.
    let mut ledger = Ledger::new(small(100, 2, 10));
    assert!(ledger.record(ev(1, None, 1, 1, 10)).unwrap().is_empty());
    assert!(ledger.record(ev(2, None, 2, 1, 10)).unwrap().is_empty());
    assert_eq!(
        ledger.record(ev(3, None, 3, 1, 10)),
        Err(LedgerError::Full(Bound::Accounts))
    );
    assert!(
        ledger.record(ev(4, None, 2, 2, 10)).unwrap().is_empty(),
        "an account it holds takes reports"
    );
}

#[test]
fn r8_expire_frees_only_entries_that_left_every_window() {
    // Full of links: only the entries that left the window free their slot.
    let mut ledger = Ledger::new(small(2, 100, 10));
    assert!(ledger.record(ev(1, Some(1), 1, 1, 100)).unwrap().is_empty());
    assert!(ledger.record(ev(2, Some(2), 1, 1, 110)).unwrap().is_empty());
    assert_eq!(
        ledger.record(ev(3, Some(3), 1, 1, 111)),
        Err(LedgerError::Full(Bound::Links))
    );
    ledger.expire(day(129));
    assert_eq!(
        ledger.stats().links,
        2,
        "an entry of day 100 still counts on day 129"
    );
    assert_eq!(
        ledger.record(ev(3, Some(3), 1, 1, 129)),
        Err(LedgerError::Full(Bound::Links))
    );
    ledger.expire(day(130));
    assert_eq!(
        ledger.stats().links,
        1,
        "the entry of day 100 left on day 130, the one of day 110 stays"
    );
    assert!(ledger.record(ev(3, Some(3), 1, 1, 130)).unwrap().is_empty());
    assert_eq!(ledger.stats().links, 2);

    // The two windows differ: a link entry leaves after 10 days, the account's after 30.
    let mut ledger = Ledger::new(Config {
        link_devices: 1_000,
        link_window: Some(Days::new(10)),
        account_devices: 1_000,
        account_window: Days::new(30),
        ..Config::default()
    });
    assert!(ledger.record(ev(1, Some(1), 1, 1, 100)).unwrap().is_empty());
    assert_eq!(
        ledger.stats(),
        Stats {
            links: 1,
            accounts: 1,
            paused_links: 0,
            disabled_accounts: 0
        }
    );
    ledger.expire(day(110));
    assert_eq!(
        ledger.stats(),
        Stats {
            links: 0,
            accounts: 1,
            paused_links: 0,
            disabled_accounts: 0
        }
    );
    ledger.expire(day(129));
    assert_eq!(ledger.stats().accounts, 1);
    ledger.expire(day(130));
    assert_eq!(
        ledger.stats(),
        Stats {
            links: 0,
            accounts: 0,
            paused_links: 0,
            disabled_accounts: 0
        }
    );

    // A paused link and a disabled account are never freed by `expire`.
    let mut ledger = Ledger::new(Config {
        link_devices: 1,
        account_devices: 1,
        ..Config::default()
    });
    ledger.record(ev(1, Some(1), 1, 1, 10)).unwrap();
    assert_eq!(
        ledger.stats(),
        Stats {
            links: 1,
            accounts: 1,
            paused_links: 1,
            disabled_accounts: 1
        }
    );
    ledger.expire(day(10_000));
    assert_eq!(
        ledger.stats(),
        Stats {
            links: 1,
            accounts: 1,
            paused_links: 1,
            disabled_accounts: 1
        }
    );
}

#[test]
fn r8_a_report_that_left_its_window_counts_for_nothing_and_takes_no_room() {
    let mut ledger = Ledger::new(small(1, 1, 1));
    assert!(ledger.record(ev(1, Some(1), 1, 1, 100)).unwrap().is_empty());
    // The ledger's present is day 100; a report dated day 60 is 40 days old: it is stale, not refused.
    assert_eq!(ledger.record(ev(2, Some(2), 2, 2, 60)), Ok(vec![]));
    assert_eq!(ledger.stats().links, 1);
    assert_eq!(ledger.stats().accounts, 1);
}

// ---------------------------------------------------------------- R9

#[test]
fn r9_counting_sees_no_content() {
    // An event is exactly these five fields: the literal breaks if one is added (a text, a url, a key).
    let ReportEvent {
        report,
        link: l,
        account,
        reporter,
        day: d,
    } = ev(1, Some(2), 3, 4, 5);
    assert_eq!(
        (report, l, account, reporter, d),
        (rid(1), Some(link(2)), acct(3), dev(4), day(5))
    );
    // The module has no text, bytes or string anywhere in its code.
    let source = include_str!("../src/ledger.rs");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<&str>>()
        .join("\n");
    for needle in [
        "String",
        "&str",
        "Vec<u8>",
        "[u8",
        "Text",
        "Subject",
        "ReportBody",
        "Value",
    ] {
        assert!(
            !code.contains(needle),
            "src/ledger.rs holds `{needle}`: the counting must not see content"
        );
    }
    // A Debug print of a ledger with 1,000 events shows only counts.
    let mut ledger = Ledger::new(Config::default());
    let mut rng = Rng::new(9);
    for i in 0..1_000u32 {
        let has_link = rng.one_in(2);
        let link_id = 0xD0 + rng.below(40) as u8;
        let event = ReportEvent {
            report: azlin_reports::ReportId::from_bytes([0xE7; 16]),
            link: if has_link { Some(link(link_id)) } else { None },
            account: acct(0xA0 + rng.below(10) as u8),
            // Different devices, so that some steps fire.
            reporter: dev(0xC3 + rng.below(60) as u8),
            day: day(1_000 + i / 50),
        };
        ledger.record(event).unwrap();
    }
    let printed = format!("{ledger:?}");
    let s = ledger.stats();
    assert_eq!(
        printed,
        format!(
            "Ledger {{ links: {}, accounts: {}, paused_links: {}, disabled_accounts: {} }}",
            s.links, s.accounts, s.paused_links, s.disabled_accounts
        )
    );
    assert!(
        !printed.contains("e7e7") && !printed.contains("c3c3") && !printed.contains("DeviceRef")
    );
    // Even an event prints no device.
    assert_eq!(format!("{:?}", dev(1)), "DeviceRef(..)");
    assert!(!format!("{:?}", ev(1, Some(2), 3, 0x5C, 5)).contains("5c5c"));
}

// ---------------------------------------------------------------- R10

#[derive(Clone, Copy, Debug)]
enum Op {
    Record(ReportEvent),
    Decided(LinkRef, Day),
    Enabled(azlin_reports::AccountRef, Day),
    Expire(Day),
}

fn op_day(op: &Op) -> Day {
    match op {
        Op::Record(e) => e.day,
        Op::Decided(_, d) | Op::Enabled(_, d) | Op::Expire(d) => *d,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Key {
    Link(LinkRef),
    Account(azlin_reports::AccountRef),
}

/// The naive model: it keeps the whole history and, for every operation, recomputes the answer from it.
fn naive(history: &[Op], cfg: &Config) -> Vec<Vec<AutoStep>> {
    let counts = |now: Day, d: Day, window: Option<Days>| window.is_none_or(|w| now.within(d, w));
    let mut fired: Vec<(usize, Key)> = Vec::new();
    let mut out = Vec::new();
    for (i, op) in history.iter().enumerate() {
        let now = history[..=i].iter().map(op_day).max().unwrap();
        let mut steps = Vec::new();
        if let Op::Record(e) = op {
            let mut keys = Vec::new();
            if let Some(l) = e.link {
                keys.push((Key::Link(l), cfg.link_window, cfg.link_devices));
            }
            keys.push((
                Key::Account(e.account),
                Some(cfg.account_window),
                cfg.account_devices,
            ));
            for (key, window, threshold) in keys {
                if !counts(now, e.day, window) {
                    continue;
                }
                // The key's current round starts after its last restart.
                let start = history[..i]
                    .iter()
                    .rposition(|o| match (o, key) {
                        (Op::Decided(l, _), Key::Link(k)) => *l == k,
                        (Op::Enabled(a, _), Key::Account(k)) => *a == k,
                        _ => false,
                    })
                    .map_or(0, |p| p + 1);
                if fired
                    .iter()
                    .any(|(at, k)| *at >= start && *at < i && *k == key)
                {
                    continue;
                }
                let mut devices: BTreeSet<DeviceRef> = BTreeSet::new();
                devices.insert(e.reporter);
                for o in &history[start..i] {
                    if let Op::Record(p) = o {
                        let same = match key {
                            Key::Link(k) => p.link == Some(k),
                            Key::Account(k) => p.account == k,
                        };
                        if same && counts(now, p.day, window) {
                            devices.insert(p.reporter);
                        }
                    }
                }
                let n = devices.len() as u32;
                if n >= threshold {
                    fired.push((i, key));
                    steps.push(match key {
                        Key::Link(link) => AutoStep::PauseLink { link, devices: n },
                        Key::Account(account) => AutoStep::DisableLinks {
                            account,
                            devices: n,
                        },
                    });
                }
            }
        }
        out.push(steps);
    }
    out
}

fn random_config(rng: &mut Rng) -> Config {
    Config {
        link_devices: rng.range(1, 4) as u32,
        link_window: if rng.one_in(4) {
            None
        } else {
            Some(Days::new(rng.range(1, 40) as u32))
        },
        account_devices: rng.range(2, 6) as u32,
        account_window: Days::new(rng.range(1, 40) as u32),
        max_links: 1_000_000,
        max_accounts: 1_000_000,
        max_reporters_per_key: 1_000_000,
    }
}

fn random_stream(rng: &mut Rng) -> Vec<Op> {
    let mut base = 1_000u32;
    let mut ops = Vec::new();
    for i in 0..rng.range(20, 90) {
        base += match rng.below(10) {
            0 => rng.range(5, 25) as u32,
            1..=3 => rng.range(0, 2) as u32,
            _ => 0,
        };
        // Some days go back: a late report, or one that already left every window.
        let on = if rng.one_in(12) {
            base.saturating_sub(rng.range(1, 45) as u32)
        } else {
            base
        };
        let op = match rng.below(100) {
            0..=7 => Op::Decided(link(rng.below(5) as u8), day(on)),
            8..=14 => Op::Enabled(acct(rng.below(3) as u8), day(on)),
            15..=22 => Op::Expire(day(on)),
            _ => Op::Record(ReportEvent {
                report: rid(i as u8),
                link: if rng.one_in(6) {
                    None
                } else {
                    Some(link(rng.below(5) as u8))
                },
                account: acct(rng.below(3) as u8),
                reporter: dev(rng.below(9) as u8),
                day: day(on),
            }),
        };
        ops.push(op);
    }
    ops
}

#[test]
fn r10_property_against_a_naive_model() {
    let mut fired_total = 0usize;
    for seed in 0..2_000u64 {
        let mut rng = Rng::new(seed);
        let cfg = random_config(&mut rng);
        let ops = random_stream(&mut rng);
        let expected = naive(&ops, &cfg);
        let mut ledger = Ledger::new(cfg);
        for (i, op) in ops.iter().enumerate() {
            let steps = match op {
                Op::Record(e) => ledger
                    .record(*e)
                    .unwrap_or_else(|err| panic!("seed {seed} op {i}: {err}")),
                Op::Decided(l, d) => {
                    ledger.decided(l, *d);
                    Vec::new()
                }
                Op::Enabled(a, d) => {
                    ledger.links_enabled(a, *d);
                    Vec::new()
                }
                Op::Expire(d) => {
                    ledger.expire(*d);
                    Vec::new()
                }
            };
            assert_eq!(
                steps,
                expected[i],
                "seed {seed}, op {i} of {}: {op:?}\nconfig {cfg:?}\nhistory {:#?}",
                ops.len(),
                &ops[..=i]
            );
            fired_total += steps.len();
        }
    }
    assert!(
        fired_total > 2_000,
        "the streams fired only {fired_total} steps: the property is too weak"
    );
}
