//! The background claims over time (X-DELTA-MONEY5-4, X-DELTA-MONEY5-1, X-DELTA-MONEY5-5): runs of
//! [`claim_run`] on a clock that only a pause moves, against a token server that remembers when it
//! was asked - days of the claims in a moment. No window: what a run says is collected, and the
//! window's side - what it does with `Outcome::ClaimsDone`, the period timer's tick - is played
//! through the [`ClaimsClock`] the window holds (X-DELTA-MONEY6-3).

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use azcloud_kit::{
    pending::{self, PendingTokens, ReceiptDue, TakenBack},
    ClaimKey, PendingCheckout, PeriodTokenStore, SharedKeyring,
};
use azul_storage::{
    keyring::{KeyringError, KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
    HttpCall, HttpReply, Transport,
};

use crate::{
    claims_clock::ClaimsClock,
    jobs::{
        claim_run, finish_is_quiet, look_in, next_look_now, ClaimsWorld, Outcome, Pace,
        RECEIPT_LOOK_SECS,
    },
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

/// The window's side, played through the clock the window holds ([`ClaimsClock`]): the app's start
/// begins a run at the clock's now, what the run says (`Outcome::ClaimsDone`) is handed to the
/// clock the way the window's dispatch does, and the next run starts when the period timer's tick
/// ([`ClaimsClock::due`]) says it is due - until a run says nothing is left (or `runs` runs are
/// gone). How many runs there were.
fn until_nothing_is_left(
    clock: &Arc<Clock>,
    server: &Server,
    shared: &SharedKeyring,
    store: &PeriodTokenStore,
    said: &mut Vec<Outcome>,
    runs: usize,
) -> usize {
    let mut window = ClaimsClock::default();
    for made in 1..=runs {
        assert!(window.start(), "the app's start or the timer begins a run");
        match run(clock, server, shared, store, said) {
            Outcome::ClaimsDone { problem, again_in } => {
                let now = clock.now();
                let _ = window.done(problem, again_in, now, now);
                let Some(secs) = again_in else {
                    return made;
                };
                assert!(
                    !window.due(now + secs - 1, now + secs - 1),
                    "not a second early"
                );
                assert!(window.due(now + secs, now + secs), "due at its time");
                clock.set(now + secs);
            }
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
    let mut window = ClaimsClock::default();
    assert!(!window.due(u64::MAX, u64::MAX), "no run named a time");
    assert!(window.start());
    assert!(window.done(None, Some(1_000), 0, 0).is_none());
    assert!(!window.due(999, 999));
    assert!(window.due(1_000, 1_000));
    assert!(window.due(5_000, 5_000));
    assert!(window.start());
    assert!(!window.due(5_000, 5_000), "one runs");
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

/// X-DELTA-MONEY6-1: what the claims say to the window of a paid grant whose month the token server
/// holds back for a stop a restore lifts. The call that lets the entry go says it once
/// (`Finished::Stopped`); the rounds in which the entry waits for the drive's vault with nothing new
/// to say (`Waits` with no month) stay quiet, so the warning is not repeated every round.
#[test]
fn x_delta_money6_1_the_claims_say_a_stop_once_and_stay_quiet_while_the_entry_waits() {
    use azcloud_kit::Finished;
    assert!(!finish_is_quiet(&Finished::Stopped {
        drive_id: String::from("d_1"),
        why: String::from("its payment mandate was stopped after payments went back"),
        days: 90,
    }));
    assert!(finish_is_quiet(&Finished::Waits {
        drive_id: String::from("d_1"),
        count: 0,
        why: String::from("the drive keeps no vault for it yet"),
    }));
    // a final end is said, as before
    assert!(!finish_is_quiet(&Finished::Dropped(String::from(
        "the token server takes no issue key of this checkout"
    ))));
}

// ==== X-DELTA-MONEY6-3, -4, -5: the timer's clock ====

/// A keyring whose reads and writes can be refused on demand: a locked macOS keychain or Secret
/// Service answers an error without a prompt (the claims read through the quiet keyring), and a
/// keychain the user said "No" to takes no write.
struct LockedOnDemand {
    inner: MemoryKeyring,
    locked: AtomicBool,
    writes_refused: AtomicBool,
}

impl LockedOnDemand {
    fn open() -> Arc<LockedOnDemand> {
        Arc::new(LockedOnDemand {
            inner: MemoryKeyring::new(),
            locked: AtomicBool::new(false),
            writes_refused: AtomicBool::new(false),
        })
    }
}

impl KeyringStore for LockedOnDemand {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        if self.locked.load(Ordering::SeqCst) {
            return Err(KeyringError::Failed(String::from("the keychain is locked")));
        }
        self.inner.get(key)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        if self.writes_refused.load(Ordering::SeqCst) {
            return Err(KeyringError::Denied);
        }
        self.inner.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.inner.delete(key)
    }
}

fn keyring_over(dir: &TempDir, store: Arc<dyn KeyringStore>) -> SharedKeyring {
    SharedKeyring::with_lock_root(
        store,
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    )
}

/// X-DELTA-MONEY6-3: the window's clock, driven the way the window drives it - the app's start, a
/// run's end handed over as `Outcome::ClaimsDone` carries it, the period timer's tick. A run's word
/// (`again_in`) arms the next run on both clocks - dropping it in `done` is the defect, a receipt
/// asked again only in the first hour of each start -, and nothing is due while a run goes.
#[test]
fn x_delta_money6_3_a_runs_word_arms_the_timer_on_both_clocks_and_nothing_is_due_while_one_goes() {
    let mut window = ClaimsClock::default();
    assert!(!window.due(u64::MAX, u64::MAX), "no run named a time");
    // the app's start: a run goes, a second start finds it going
    assert!(window.start());
    assert!(window.running());
    assert!(!window.start(), "one run at a time");
    // the run ends with the word `590`: armed on both clocks
    assert!(window.done(None, Some(590), 1_000, 50).is_none());
    assert!(!window.running());
    assert_eq!(window.named(), Some(590), "the word is kept");
    assert!(!window.due(1_589, 639), "not before it on either clock");
    assert!(window.due(1_590, 50), "the wall clock reached it");
    assert!(window.due(1_000, 640), "the other clock reached it");
    // the timer starts the run: nothing is due while it goes, whatever the clocks say
    assert!(window.start());
    assert!(!window.due(u64::MAX, u64::MAX), "never while a run goes");
}

/// X-DELTA-MONEY6-3: a start spends the word of the last run (this run decides when the next is
/// due), and a run that names nothing leaves nothing to start - a clock that keeps the old word
/// would start a run nobody asked for at every tick.
#[test]
fn x_delta_money6_3_a_start_spends_the_word_of_the_last_run_and_a_run_that_names_nothing_leaves_nothing(
) {
    let mut window = ClaimsClock::default();
    assert!(window.start());
    assert!(window.done(None, Some(590), 1_000, 50).is_none());
    assert_eq!(window.named(), Some(590));
    assert!(window.start());
    assert_eq!(
        window.named(),
        None,
        "a start spends the word of the last run"
    );
    assert!(window.done(None, None, 2_000, 100).is_none());
    assert_eq!(window.named(), None);
    assert!(
        !window.due(u64::MAX, u64::MAX),
        "nothing named, nothing to start"
    );
}

/// X-DELTA-MONEY6-4: a run's problem is said once per streak of runs that had one - the timer
/// tries a locked keychain again every ten minutes, and a warning every ten minutes is no help; a
/// run that ended without a problem ends the streak, so the next failure says it again.
#[test]
fn x_delta_money6_4_a_streak_of_failing_runs_says_its_problem_once() {
    let mut window = ClaimsClock::default();
    let problem = || Some(azul_appkit::l10n::Text::key("azdrive-pay-checkouts-unread"));
    assert!(window.start());
    assert!(
        window.done(problem(), Some(600), 3_000, 200).is_some(),
        "said the first time"
    );
    assert!(window.start());
    assert!(
        window.done(problem(), Some(600), 3_600, 800).is_none(),
        "not again while it goes on"
    );
    assert!(window.start());
    assert!(window.done(None, None, 4_200, 1_400).is_none());
    assert!(window.start());
    assert!(
        window.done(problem(), None, 4_800, 2_000).is_some(),
        "a new streak says it again"
    );
}

/// X-DELTA-MONEY6-4, through the claims run: AzDrive stays open for days with a card sale whose
/// receipt is awaited; the run that would arm the timer reads the list through the quiet keyring,
/// and a locked keychain answers an error without a prompt. The old run said `again_in: None`: the
/// timer was disarmed until the next start and a receipt awaited "for the whole wait" was asked no
/// more. Now the run says one warning and names the next try, the timer starts it, and once the
/// keychain opens the receipt is asked; a second failing run in a row says nothing new.
#[test]
fn x_delta_money6_4_a_run_whose_list_cannot_be_read_names_the_next_run_and_the_timer_starts_it() {
    let dir = TempDir::new("azdrive-claims");
    let locked = LockedOnDemand::open();
    let shared = keyring_over(&dir, locked.clone());
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let origin = start();
    let clock = Clock::at(origin);
    pending::add(&shared, &waiting_for_its_receipt(origin)).unwrap();
    let server = Server::new(&clock, |_| {
        String::from(
            r#"{"checkout_id": "ck_1", "status": "approved", "tier": "100GB", "months": 12}"#,
        )
    });
    let mut window = ClaimsClock::default();
    let mut said = Vec::new();

    // the app's start finds the keychain locked
    locked.locked.store(true, Ordering::SeqCst);
    assert!(window.start());
    let Outcome::ClaimsDone { problem, again_in } =
        run(&clock, &server, &shared, &store, &mut said)
    else {
        panic!("a run ends with ClaimsDone");
    };
    assert!(problem.is_some(), "the user is told once");
    assert_eq!(
        again_in,
        Some(RECEIPT_LOOK_SECS),
        "and the next try is named"
    );
    assert!(window
        .done(problem, again_in, clock.now(), clock.now())
        .is_some());
    assert!(server.asked().is_empty());

    // the timer ticks before the time: nothing; at the time: the next run is due
    clock.set(origin + RECEIPT_LOOK_SECS - 1);
    assert!(!window.due(clock.now(), clock.now()));
    clock.set(origin + RECEIPT_LOOK_SECS);
    assert!(
        window.due(clock.now(), clock.now()),
        "the timer starts the next run"
    );

    // that run finds the keychain locked still: no second warning, and the next try is named again
    assert!(window.start());
    let Outcome::ClaimsDone { problem, again_in } =
        run(&clock, &server, &shared, &store, &mut said)
    else {
        panic!("a run ends with ClaimsDone");
    };
    assert_eq!(again_in, Some(RECEIPT_LOOK_SECS));
    assert!(
        window
            .done(problem, again_in, clock.now(), clock.now())
            .is_none(),
        "a streak of failing runs says it once"
    );

    // the keychain opens: the next run asks for the receipt
    locked.locked.store(false, Ordering::SeqCst);
    clock.set(clock.now() + RECEIPT_LOOK_SECS);
    assert!(window.due(clock.now(), clock.now()));
    assert!(window.start());
    let Outcome::ClaimsDone { problem, .. } = run(&clock, &server, &shared, &store, &mut said)
    else {
        panic!("a run ends with ClaimsDone");
    };
    assert!(problem.is_none());
    assert_eq!(server.asked().len(), 1, "the receipt was asked for");
}

/// X-DELTA-MONEY6-4, the run that ends at its one-hour limit: the list read that says what is left
/// to ask later fails there too - and there is no next round to fail it loudly. It leaves the next
/// tick to try again, where the old code returned `None` and disarmed the timer.
#[test]
fn x_delta_money6_4_a_list_that_cannot_be_read_at_the_end_of_a_run_leaves_the_next_tick_to_try_again(
) {
    let dir = TempDir::new("azdrive-claims");
    let locked = LockedOnDemand::open();
    let shared = keyring_over(&dir, locked.clone());
    let origin = start();
    let clock = Clock::at(origin);
    pending::add(&shared, &waiting_for_its_receipt(origin)).unwrap();
    let transport = Server::new(&clock, |_| String::new());
    let mut say = |_outcome: Outcome| {};
    let world = ClaimsWorld {
        transport: &transport,
        mono: &|| clock.now(),
        wall: &|| clock.now(),
        sleep: &|_| {},
        say: &mut say,
    };
    let asked = HashMap::new();
    // readable: the receipt's own pace
    assert_eq!(next_look_now(&world, &shared, &asked), Some(0));
    // locked: try again at the next tick
    locked.locked.store(true, Ordering::SeqCst);
    assert_eq!(
        next_look_now(&world, &shared, &asked),
        Some(RECEIPT_LOOK_SECS)
    );
}

/// X-DELTA-MONEY6-5: a laptop boots with a wrong clock (a dead CMOS battery: a year ahead), AzDrive
/// starts, a run ends and arms the timer; the network time then steps the clock back a year. An
/// absolute wall time (`now + again_in`, a year ahead) postponed the next ask by the size of the
/// step - no receipt, no daily taken-back ask, until the next start. The run's word is also counted
/// on a clock that never steps, so the next run is due after `again_in` seconds all the same - and a
/// machine that slept for days (its monotonic clock stopped) is due by the wall clock at once.
#[test]
fn x_delta_money6_5_a_wall_clock_stepped_back_does_not_postpone_the_next_ask() {
    let real = start();
    let a_year = 365 * DAY;
    let mut window = ClaimsClock::default();
    assert!(window.start());
    // the run ends while the wall clock says a year ahead; 590 seconds on both clocks
    let wrong_now = real + a_year;
    assert!(window.done(None, Some(590), wrong_now, 1_000).is_none());
    // the network time steps the wall clock back to the real time: 589 seconds on, not due
    assert!(!window.due(real, 1_589));
    // 590 seconds on the clock that never steps: due, though the wall clock is a year below the time
    // the run armed (the old `claims_due(Some(at), false, now)` was false for a year)
    assert!(window.due(real, 1_590));
    // the other way: the machine slept for three days, the monotonic clock stood still
    assert!(window.due(wrong_now + 3 * DAY, 1_000));
}

/// X-DELTA-MONEY6-6 (the fallback X-MONEY5A-3 names), through the claims run: the entry of a payment
/// taken back that the list cannot mark (the keyring refuses the write) was the kit's `NotKept`, and
/// `look_in`'s "asked in this run and found taken back, with no count in the entry" branch was dead
/// for it: the entry was polled in EVERY round of the run (an HTTP call each, 361 in an hour), the
/// first answer was said as "in flight" - a payment that was taken back - and the dialog's order
/// stayed refused. Now it is a typed answer of its own: asked once per run, said once as taken back,
/// never as in flight, the next ask a day on, and the entry stays on the list with its claim secret.
#[test]
fn x_delta_money6_6_an_entry_the_list_cannot_mark_is_asked_once_per_run_and_said_as_taken_back() {
    let dir = TempDir::new("azdrive-claims");
    let gate = LockedOnDemand::open();
    let shared = keyring_over(&dir, gate.clone());
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let clock = Clock::at(start());
    pending::add(&shared, &entry()).unwrap();
    gate.writes_refused.store(true, Ordering::SeqCst);
    let server = Server::new(&clock, |_| {
        String::from(
            r#"{"checkout_id": "ck_1", "status": "reversed", "reason": "the debit was returned"}"#,
        )
    });
    let mut said = Vec::new();
    let Outcome::ClaimsDone {
        again_in: Some(wait),
        ..
    } = run(&clock, &server, &shared, &store, &mut said)
    else {
        panic!("the run names the time of the next ask");
    };
    assert_eq!(
        server.asked().len(),
        1,
        "asked once in the run, not in every round: {:?}",
        server.asked().len()
    );
    let taken_back = said
        .iter()
        .filter(|outcome| matches!(outcome, Outcome::TakenBack { .. }))
        .count();
    assert_eq!(taken_back, 1, "said once, as taken back");
    assert!(
        !said
            .iter()
            .any(|outcome| matches!(outcome, Outcome::InFlight { .. })),
        "a payment taken back is never said to be in flight"
    );
    assert!(
        (DAY - 120..=DAY + 120).contains(&wait),
        "the next ask a day on: {wait}"
    );
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "the entry stays");
    assert_eq!(
        listed[0].taken_back, None,
        "unmarked: the list took no write"
    );
    assert!(!listed[0].claim_secret.is_empty(), "with its claim secret");
}
