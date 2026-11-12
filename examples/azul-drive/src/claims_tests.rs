//! The background claims over time (X-DELTA-MONEY5-4, X-DELTA-MONEY5-1, X-DELTA-MONEY5-5): runs of
//! [`claim_run`] on a clock that only a pause moves, against a token server that remembers when it
//! was asked - days of the claims in a moment. No window: what a run says is collected, and the
//! window's timer ([`claims_due`]) is played by the test.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use azcloud_kit::{
    pending::{self, PendingTokens, ReceiptDue, TakenBack},
    ClaimKey, PendingCheckout, PeriodTokenStore, SharedKeyring,
};
use azul_storage::{
    keyring::{KeyringNamespace, MemoryKeyring},
    testing::TempDir,
    HttpCall, HttpReply, Transport,
};

use crate::{
    add_flow::claims_due,
    jobs::{claim_run, look_in, ClaimsWorld, Outcome, Pace, RECEIPT_LOOK_SECS},
};

const TOKEN: &str = "http://127.0.0.1:18081";
const DAY: u64 = 86_400;
/// An issue key (a sealed sign-up's; only that it is not empty matters here).
const ISSUE_KEY: &str = "Zm9yIHRoZSBwZXJpb2QgdG9rZW5zIG9mIGNrXzEgb25seQ";

/// A clock that only a pause moves.
struct Clock(AtomicU64);

impl Clock {
    fn at(secs: u64) -> Arc<Clock> {
        Arc::new(Clock(AtomicU64::new(secs)))
    }

    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    fn set(&self, secs: u64) {
        self.0.store(secs, Ordering::SeqCst);
    }
}

/// A token server that answers question number n about any checkout with `answer(n)` (a JSON
/// body, status 200) and remembers when each question came, on the test's clock.
struct Server {
    clock: Arc<Clock>,
    asked: Mutex<Vec<u64>>,
    answer: Box<dyn Fn(usize) -> String + Send + Sync>,
}

impl Server {
    fn new(clock: &Arc<Clock>, answer: impl Fn(usize) -> String + Send + Sync + 'static) -> Server {
        Server {
            clock: clock.clone(),
            asked: Mutex::new(Vec::new()),
            answer: Box::new(answer),
        }
    }

    /// When each question came.
    fn asked(&self) -> Vec<u64> {
        self.asked.lock().unwrap().clone()
    }
}

impl Transport for Server {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        let n = {
            let mut asked = self.asked.lock().unwrap();
            asked.push(self.clock.now());
            asked.len() - 1
        };
        Ok(HttpReply {
            status: 200,
            headers: vec![(
                String::from("content-type"),
                String::from("application/json"),
            )],
            body: (self.answer)(n).into_bytes(),
        })
    }
}

fn keyring(dir: &TempDir) -> SharedKeyring {
    SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    )
}

/// The seconds since 1970 the test starts at: the real now, since the kit stamps what it counts
/// itself (a poll's count of a payment taken back) with the real clock.
fn start() -> u64 {
    u64::try_from(azcloud_kit::now()).expect("a clock after 1970")
}

/// One run of the claims at the clock's now: what it said, and how it ended.
fn run(
    clock: &Arc<Clock>,
    server: &Server,
    shared: &SharedKeyring,
    store: &PeriodTokenStore,
    said: &mut Vec<Outcome>,
) -> Outcome {
    let origin = clock.now();
    let mut say = |outcome: Outcome| said.push(outcome);
    let sleeper = clock.clone();
    let mut world = ClaimsWorld {
        transport: server,
        mono: &|| clock.now().saturating_sub(origin),
        wall: &|| clock.now(),
        sleep: &|secs| sleeper.set(sleeper.now() + secs),
        say: &mut say,
    };
    claim_run(&mut world, shared, Some(TOKEN), store, &HashMap::new(), DAY)
}

/// The window's side, played: a run at the clock's now, and the next run when the last one says
/// it is due - [`claims_due`] is what the period timer asks - until a run says nothing is left (or
/// `runs` runs are gone). How many runs there were.
fn until_nothing_is_left(
    clock: &Arc<Clock>,
    server: &Server,
    shared: &SharedKeyring,
    store: &PeriodTokenStore,
    said: &mut Vec<Outcome>,
    runs: usize,
) -> usize {
    for made in 1..=runs {
        match run(clock, server, shared, store, said) {
            Outcome::ClaimsDone {
                again_in: Some(secs),
                ..
            } => {
                let at = clock.now() + secs;
                assert!(!claims_due(Some(at), false, at - 1), "not a second early");
                assert!(!claims_due(Some(at), true, at), "never while a run goes");
                assert!(claims_due(Some(at), false, at), "due at its time");
                clock.set(at);
            }
            Outcome::ClaimsDone { again_in: None, .. } => return made,
            _ => panic!("a run ends with ClaimsDone"),
        }
    }
    runs
}

/// A kept checkout: `ck_1` at the token server, ordered with a fresh claim key.
fn entry() -> PendingCheckout {
    PendingCheckout::new(
        "ck_1",
        &ClaimKey::generate().expect("a claim key"),
        "100GB",
        TOKEN,
        "Photos",
    )
}

/// `ck_1` after its claim: the grant kept elsewhere, only the receipt awaited, claimed at `at`.
fn waiting_for_its_receipt(at: u64) -> PendingCheckout {
    let at = i64::try_from(at).expect("a time");
    let mut ck = entry();
    ck.period = Some(PendingTokens {
        drive_id: String::from("d_1"),
        months: 12,
        issue_key: String::from(ISSUE_KEY),
        recurring: false,
        claimed_at: at,
        counted_at: at,
    });
    ck.receipt_due = Some(ReceiptDue::GrantKept);
    ck
}

/// X-DELTA-MONEY5-4. A card sale the token server's tick has not booked within the hour (a tick
/// that fails, a Turso outage, any foreign-currency sale) was asked for at 0, 10, ..., 60 minutes
/// of ONE claims run, and the run ended: nothing started another until the next start or purchase
/// of an app that stays open for days, though the entry sits on the list for three days. Now a run
/// that leaves a checkout with a pace of its own says when the next is due, the window's timer
/// starts it, and the checkout is asked every ten minutes for its whole wait - the old code's
/// hour-long run would have stopped at the seventh question - and, with no sale ever booked, the
/// last look after the three days ends it with the record as it is.
#[test]
fn x_delta_money5_4_a_checkout_that_waits_for_its_receipt_is_asked_for_the_whole_wait_and_not_only_for_the_first_hour(
) {
    let dir = TempDir::new("azdrive-claims");
    let shared = keyring(&dir);
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let origin = start();
    let clock = Clock::at(origin);
    pending::add(&shared, &waiting_for_its_receipt(origin)).unwrap();
    // the token server books no sale
    let server = Server::new(&clock, |_| {
        String::from(
            r#"{"checkout_id": "ck_1", "status": "approved", "tier": "100GB", "months": 12}"#,
        )
    });
    let mut said = Vec::new();
    let runs = until_nothing_is_left(&clock, &server, &shared, &store, &mut said, 1_000);
    let asked = server.asked();
    assert!(runs < 1_000, "the claims end when the wait does");
    assert!(
        asked.iter().any(|at| *at - origin > 2 * 3_600),
        "asked after the first hours of an uninterrupted session: {asked:?}"
    );
    assert!(
        asked.last().is_some_and(|last| *last - origin >= 3 * DAY),
        "asked until the three days were over: the last question came {} seconds on",
        asked.last().map_or(0, |last| *last - origin)
    );
    assert!(
        asked.windows(2).all(|w| w[1] - w[0] >= RECEIPT_LOOK_SECS),
        "never faster than the receipt's pace"
    );
    assert!(
        asked.len() > 400,
        "every ten minutes for three days: {} questions",
        asked.len()
    );
    assert!(
        pending::list(&shared).unwrap().is_empty(),
        "the wait is over: off the list"
    );
    let gave_up = said
        .iter()
        .filter(|outcome| {
            matches!(
                outcome,
                Outcome::CheckoutFinished {
                    result: Ok(Some(azcloud_kit::Finished::Receipt(
                        azcloud_kit::pending::Receipted::GaveUp
                    ))),
                    ..
                }
            )
        })
        .count();
    assert_eq!(gave_up, 1, "said once, when the wait was over");
}

/// X-DELTA-MONEY5-4, the receipt arriving: the checkout that is asked at its pace leaves the list
/// the moment the token server names the sale, and nothing asks again after it.
#[test]
fn x_delta_money5_4_the_receipt_that_is_booked_on_the_second_day_ends_the_asking() {
    let dir = TempDir::new("azdrive-claims");
    let shared = keyring(&dir);
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let origin = start();
    let clock = Clock::at(origin);
    pending::add(&shared, &waiting_for_its_receipt(origin)).unwrap();
    let booked_at = origin + 30 * 3_600;
    let books = clock.clone();
    let server = Server::new(&clock, move |_| {
        let receipt = if books.now() >= booked_at {
            r#", "receipt": {"issue_day": "2026-11-05", "service": "Azlin Storage 100GB, 12 months",
                "gross_cents": 990, "currency": "EUR", "rate_permille": 190, "country": "DE",
                "scheme": "sepa", "checkout_id": "ck_1", "kind": "payment_receipt"}"#
        } else {
            ""
        };
        format!(
            r#"{{"checkout_id": "ck_1", "status": "approved", "tier": "100GB", "months": 12{receipt}}}"#
        )
    });
    let mut said = Vec::new();
    until_nothing_is_left(&clock, &server, &shared, &store, &mut said, 1_000);
    let asked = server.asked();
    assert!(
        asked.last().is_some_and(|last| *last >= booked_at),
        "asked until the sale was booked"
    );
    assert!(
        asked.iter().filter(|at| **at >= booked_at).count() == 1,
        "and not after it: {asked:?}"
    );
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(shared
        .paid()
        .unwrap()
        .of_checkout("ck_1")
        .unwrap()
        .is_some_and(|record| record.receipt.is_some()));
}

/// The timer's decision is the window's: while a run goes, or before the time the run named, it
/// starts nothing; a run that named nothing leaves nothing to start.
#[test]
fn x_delta_money5_4_the_timer_starts_a_run_only_when_one_is_due_and_none_goes() {
    assert!(!claims_due(None, false, u64::MAX), "no run named a time");
    assert!(!claims_due(Some(1_000), false, 999));
    assert!(claims_due(Some(1_000), false, 1_000));
    assert!(claims_due(Some(1_000), false, 5_000));
    assert!(!claims_due(Some(1_000), true, 5_000), "one runs");
}

/// X-DELTA-MONEY5-1, the claims' pace: a checkout whose payment was taken back is asked once a day
/// by its own count. A run that finds it asked an hour ago asks nothing and says when the day is
/// up; a run that finds it due asks, says once that the payment was taken back and may come back,
/// keeps the entry and says the next day. The old code erased the entry at the first look - and
/// asked every round of the run before that.
#[test]
fn x_delta_money5_1_the_claims_ask_a_payment_taken_back_once_a_day_and_keep_its_entry() {
    let reversed = |clock: &Arc<Clock>| {
        Server::new(clock, |_| {
            String::from(
                r#"{"checkout_id": "ck_1", "status": "reversed", "reason": "the debit was returned"}"#,
            )
        })
    };
    let marked = |counted_ago: u64| {
        let counted = i64::try_from(start() - counted_ago).unwrap();
        let mut ck = entry();
        ck.taken_back = Some(TakenBack {
            since: counted - 3 * 86_400,
            counted_at: counted,
        });
        ck
    };

    // asked an hour ago: nothing is due, and the run says when the day is up
    let dir = TempDir::new("azdrive-claims");
    let shared = keyring(&dir);
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let clock = Clock::at(start());
    pending::add(&shared, &marked(3_600)).unwrap();
    let server = reversed(&clock);
    let mut said = Vec::new();
    let Outcome::ClaimsDone {
        again_in: Some(wait),
        ..
    } = run(&clock, &server, &shared, &store, &mut said)
    else {
        panic!("the run names the time of the next look");
    };
    assert!(server.asked().is_empty(), "not asked within the day");
    assert!(said.is_empty(), "nothing to say");
    assert!(
        (DAY - 3_700..=DAY - 3_500).contains(&wait),
        "the rest of the day: {wait}"
    );
    assert_eq!(pending::list(&shared).unwrap().len(), 1);

    // asked a day and a few seconds ago: asked now - once in the run, whatever its rounds -, said
    // once, kept, and the next look is a day on
    let dir = TempDir::new("azdrive-claims");
    let shared = keyring(&dir);
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let clock = Clock::at(start());
    pending::add(&shared, &marked(DAY + 5)).unwrap();
    let server = reversed(&clock);
    let mut said = Vec::new();
    let Outcome::ClaimsDone {
        again_in: Some(again),
        ..
    } = run(&clock, &server, &shared, &store, &mut said)
    else {
        panic!("a day on");
    };
    assert_eq!(server.asked().len(), 1, "asked once in the run");
    let taken_back: Vec<_> = said
        .iter()
        .filter_map(|outcome| match outcome {
            Outcome::TakenBack {
                checkout_id,
                why,
                days,
            } => Some((checkout_id.as_str(), why.as_str(), *days)),
            _ => None,
        })
        .collect();
    assert_eq!(taken_back.len(), 1, "said once");
    assert_eq!(taken_back[0].0, "ck_1");
    assert!(
        taken_back[0].1.contains("the debit was returned"),
        "{taken_back:?}"
    );
    assert_eq!(taken_back[0].2, 30, "kept 30 days");
    assert_eq!(pending::list(&shared).unwrap().len(), 1, "the entry stays");
    assert!(
        (DAY - 120..=DAY + 120).contains(&again),
        "a day on: {again}"
    );
}

/// The pace of each kind of entry, as the claims decide it: a cash order once per run and per
/// interval, a receipt every ten minutes and a day once the three days are over and a look could
/// not be made, a payment taken back by its own count, anything else every round.
#[test]
fn the_pace_of_a_kept_checkout_is_decided_by_what_it_waits_for() {
    let wall = 1_791_450_000_i64;
    let mut cash = entry();
    cash.method = String::from("cash");
    let mut receipt = entry();
    receipt.period = Some(PendingTokens {
        drive_id: String::from("d_1"),
        months: 12,
        issue_key: String::from(ISSUE_KEY),
        recurring: false,
        claimed_at: wall,
        counted_at: wall,
    });
    receipt.receipt_due = Some(ReceiptDue::GrantKept);
    let mut back = entry();
    back.taken_back = Some(TakenBack {
        since: wall - 3 * 86_400,
        counted_at: wall - 3_600,
    });
    let plain = entry();

    // never asked in this run: due at once
    for (ck, pace) in [(&cash, Pace::Cash), (&receipt, Pace::Receipt)] {
        assert_eq!(look_in(ck, None, 5_000, wall), (pace, 0));
    }
    // asked ten seconds ago at the pace of the entry
    assert_eq!(
        look_in(&cash, Some((4_990, DAY)), 5_000, wall),
        (Pace::Cash, DAY - 10)
    );
    assert_eq!(
        look_in(&receipt, Some((4_990, RECEIPT_LOOK_SECS)), 5_000, wall),
        (Pace::Receipt, RECEIPT_LOOK_SECS - 10)
    );
    assert_eq!(
        look_in(
            &receipt,
            Some((4_990, RECEIPT_LOOK_SECS)),
            5_000 + RECEIPT_LOOK_SECS,
            wall
        )
        .1,
        0,
        "due after its interval"
    );
    // a payment taken back: the entry's own count, an hour ago
    assert_eq!(
        look_in(&back, None, 5_000, wall),
        (Pace::TakenBack, DAY - 3_600)
    );
    assert_eq!(look_in(&back, None, 5_000, wall + DAY as i64).1, 0);
    // asked in this run and found taken back, though the list could not take its count: the run
    // does not ask it again in every round
    assert_eq!(
        look_in(&plain, Some((4_999, DAY)), 5_000, wall),
        (Pace::TakenBack, DAY - 1)
    );
    // every round: an entry the run has no pace of its own for
    assert_eq!(look_in(&plain, None, 5_000, wall), (Pace::Rounds, 0));
}
