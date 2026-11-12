//! A payment TAKEN BACK is not the end of an unclaimed checkout (X-DELTA-MONEY5-1). The token
//! server's restore - a refund that failed at the provider, a dispute that was won, the operator's
//! `payments restore` - approves a reversed checkout again, and the drive it hands over is sealed
//! to the claim key that only the app's list holds (the approval nulled the server's own copy).
//! The poll used to read `reversed` as `Declined`, take the entry off the list and erase the claim
//! secret; the restore then approved a drive no computer could open. Now the entry stays, asked
//! once a day, until the token server's hand-over is over.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringError, KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
};

use super::{bundle, json, period::ISSUE_KEY, Fake, Shared, TOKEN};
use crate::{
    claim::seal,
    pending::{
        self, Abandoned, AddError, Begun, Final, PendingCheckout, PendingTokens, Polled,
        ReceiptDue, TakenBack, Transient, CASH_HANDOVER_KEPT_SECS, GRANT_WAIT_SECS,
        HANDOVER_KEPT_SECS, TAKEN_BACK_ASK_SECS,
    },
    token::CheckoutStatus,
    ClaimKey, SharedKeyring, TokenServer,
};

const DAY: i64 = 86_400;

/// What the token server says of a reversed checkout.
const REVERSED: &str =
    r#"{"checkout_id": "ck_1", "status": "reversed", "reason": "the debit was returned"}"#;

fn shared(dir: &TempDir) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::with_lock_root(
        keyring.clone(),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    (shared, keyring)
}

/// `ck_1`, ordered with `claim`, not claimed.
fn unclaimed(claim: &ClaimKey) -> PendingCheckout {
    PendingCheckout::new("ck_1", claim, "100GB", TOKEN, "Photos")
}

/// What the token server says of `ck_1` once it approved it again: the new drive `d_1`, its
/// sign-up sealed to `claim`.
fn approved(claim: &ClaimKey) -> String {
    let sealed = seal(
        &claim.public_base64(),
        "ck_1",
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    format!(
        r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{sealed}",
            "tier": "100GB", "months": 12, "months_paid": 12}}"#
    )
}

/// A token server that answers each call with the next of `answers` (the last goes on).
fn saying(answers: Vec<String>) -> Arc<Fake> {
    Fake::new(move |_, n| Ok(json(200, &answers[n.min(answers.len() - 1)])))
}

/// The entry of `ck_1` as the list has it now.
fn listed(shared: &SharedKeyring) -> PendingCheckout {
    pending::list(shared).unwrap().remove(0)
}

/// Whether the claim secret of `claim` is in the keyring's list entry.
fn secret_is_kept(keyring: &MemoryKeyring, claim: &ClaimKey) -> bool {
    keyring
        .get(pending::PENDING_KEY)
        .unwrap()
        .is_some_and(|text| text.contains(claim.to_base64().as_str()))
}

/// X-DELTA-MONEY5-1, the whole story. The payment is taken back before the app ever claimed the
/// drive (a cash buyer's app that was not run for a week, a bank-transfer payer whose app was
/// closed when the money arrived); the poll finds `reversed`; the token server then restores the
/// checkout; the next poll claims the drive. The old code read `reversed` as `Declined`, took the
/// entry off the list and erased the claim secret on the first look: the first assertion here
/// fails (`Final::Declined`), and a restore would have approved a drive nobody can open.
#[test]
fn x_delta_money5_1_an_unclaimed_checkout_taken_back_keeps_its_secret_and_the_restore_lets_the_next_poll_claim_the_drive(
) {
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &unclaimed(&claim)).unwrap();
    let fake = saying(vec![REVERSED.into(), REVERSED.into(), approved(&claim)]);
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();

    // Two looks while the payment is taken back (the claims read the list anew every round).
    for look in 1..=2 {
        match pending::poll(&server, &shared, &listed(&shared)) {
            Polled::Transient(Transient::TakenBack(why)) => {
                assert!(why.contains("the debit was returned"), "look {look}: {why}");
                assert!(why.contains("taken back"), "look {look}: {why}");
            }
            other => panic!("look {look}: the checkout was not kept as taken back: {other:?}"),
        }
        let entry = listed(&shared);
        assert_eq!(entry.checkout_id, "ck_1", "look {look}: the entry stays");
        assert!(
            entry.taken_back.is_some(),
            "look {look}: counted in the entry"
        );
        assert!(
            secret_is_kept(&keyring, &claim),
            "look {look}: the claim secret stays in the keyring"
        );
        assert!(
            keyring.get(&keyring_key("d_1")).unwrap().is_none(),
            "look {look}: no drive yet"
        );
    }

    // The token server restores the payment: its hand-over is sealed to the same claim key.
    let claimed = match pending::poll(&server, &shared, &listed(&shared)) {
        Polled::Final(Final::Claimed(claimed)) => claimed,
        other => panic!("the restored drive was not claimed: {other:?}"),
    };
    assert_eq!(
        claimed.bundle.drive_id(),
        "d_1",
        "opened with the kept claim secret"
    );
    assert!(
        keyring.get(&keyring_key("d_1")).unwrap().is_some(),
        "the drive's session is in the keyring: the app can open the drive"
    );
    assert_eq!(
        listed(&shared).taken_back,
        None,
        "the payment is not taken back any more: the daily wait goes with it"
    );
    // The app saves the drive and tells the list: the checkout is done.
    pending::claimed(&shared, "ck_1", "d_1", None).unwrap();
    assert!(pending::list(&shared).unwrap().is_empty());
    assert_eq!(fake.calls().len(), 3, "one question per look");
}

/// The wire's `reversed` is its own status, not a decline: the reason as the token server said
/// it, else a default that says what it was.
#[test]
fn x_delta_money5_1_a_reversed_status_is_its_own_answer_and_a_declined_one_is_still_an_end() {
    let claim = ClaimKey::generate().unwrap();
    let say = |body: &'static str| {
        let fake = Fake::new(move |_, _| Ok(json(200, body)));
        let transport = Shared(fake);
        TokenServer::new(TOKEN, &transport)
            .unwrap()
            .checkout_status("ck_1", &claim)
            .unwrap()
    };
    assert_eq!(
        say(REVERSED),
        CheckoutStatus::Reversed(String::from("the debit was returned"))
    );
    assert_eq!(
        say(r#"{"checkout_id": "ck_1", "status": "reversed"}"#),
        CheckoutStatus::Reversed(String::from("a refund or a chargeback"))
    );
    // the negative controls: what is over is over
    assert!(matches!(
        say(r#"{"checkout_id": "ck_1", "status": "declined", "reason": "no funds"}"#),
        CheckoutStatus::Declined(_)
    ));
    assert!(matches!(
        say(r#"{"checkout_id": "ck_1", "status": "rejected", "reason": "wrong amount"}"#),
        CheckoutStatus::Declined(_)
    ));
    assert!(matches!(
        say(r#"{"checkout_id": "ck_1", "status": "expired"}"#),
        CheckoutStatus::Gone(_)
    ));
}

/// The one thing that ends a checkout taken back: the hand-over's life, counted from the first
/// look that found the payment taken back. Over: the entry ends `Gone`, off the list, and the
/// claim secret is erased from the keyring - nothing the token server could restore is a drive this
/// key opens any more. Not over yet: the entry stays.
#[test]
fn x_delta_money5_1_an_entry_ends_gone_only_when_the_hand_over_s_life_has_passed_since_the_first_look(
) {
    let now = crate::now();
    for (age, over) in [
        (HANDOVER_KEPT_SECS + 3_600, true),
        (HANDOVER_KEPT_SECS - 3_600, false),
        (DAY, false),
    ] {
        let dir = TempDir::new("azcloud-taken-back");
        let (shared, keyring) = shared(&dir);
        let claim = ClaimKey::generate().unwrap();
        let mut ck = unclaimed(&claim);
        ck.taken_back = Some(TakenBack {
            since: now - age,
            counted_at: now - 3_600,
        });
        pending::add(&shared, &ck).unwrap();
        let fake = saying(vec![REVERSED.into()]);
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        match (pending::poll(&server, &shared, &ck), over) {
            (Polled::Final(Final::Gone(why)), true) => {
                assert!(why.contains("30 days"), "{why}");
                assert!(why.contains("120 for cash"), "{why}");
                assert!(why.contains("ck_1"), "{why}");
                assert!(pending::list(&shared).unwrap().is_empty(), "off the list");
                assert!(!secret_is_kept(&keyring, &claim), "the secret is erased");
            }
            (Polled::Transient(Transient::TakenBack(_)), false) => {
                assert_eq!(pending::list(&shared).unwrap().len(), 1, "age {age}: kept");
                assert!(
                    secret_is_kept(&keyring, &claim),
                    "age {age}: the secret stays"
                );
            }
            (other, _) => panic!("age {age}: over {over}: {other:?}"),
        }
    }
}

/// A cash buyer's hand-over lives four times as long (the operator activates the order when the
/// post is opened, and the app asks only while it runs): 31 days taken back is nothing, 121 are the
/// end. A drive picked up with a claim code is a cash buyer's.
#[test]
fn x_delta_money5_1_a_cash_buyer_s_entry_is_kept_for_120_days() {
    assert_eq!(CASH_HANDOVER_KEPT_SECS, 120 * DAY);
    let now = crate::now();
    for picked_up in [false, true] {
        for (age, over) in [(31 * DAY, false), (121 * DAY, true)] {
            let dir = TempDir::new("azcloud-taken-back");
            let (shared, keyring) = shared(&dir);
            let claim = ClaimKey::generate().unwrap();
            let mut ck = unclaimed(&claim);
            ck.method = String::from("cash");
            ck.picked_up = picked_up;
            ck.taken_back = Some(TakenBack {
                since: now - age,
                counted_at: now - 3_600,
            });
            pending::add(&shared, &ck).unwrap();
            let transport = Shared(saying(vec![REVERSED.into()]));
            let server = TokenServer::new(TOKEN, &transport).unwrap();
            match (pending::poll(&server, &shared, &ck), over) {
                (Polled::Final(Final::Gone(_)), true) => {
                    assert!(!secret_is_kept(&keyring, &claim), "{age}");
                }
                (Polled::Transient(Transient::TakenBack(_)), false) => {
                    assert!(secret_is_kept(&keyring, &claim), "{age}");
                }
                (other, _) => panic!("picked up {picked_up}, age {age}: {other:?}"),
            }
        }
    }
}

/// The count is the one the receipt's and the vault's waits make (AZ-P5): a step of the clock
/// counts at most a week, so a clock set a year forward does not end an entry the token server
/// might still restore - on the strength of the wall clock alone it would have been erased (an
/// order's `started_at` a year ago). A step back counts nothing.
#[test]
fn x_delta_money5_1_a_step_of_the_clock_counts_at_most_a_week_towards_the_hand_over_s_life() {
    let now = crate::now();
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = unclaimed(&claim);
    // first seen taken back a year ago by the clock, last counted then: the clock stepped a year
    ck.started_at = now - 365 * DAY;
    ck.taken_back = Some(TakenBack {
        since: now - 365 * DAY,
        counted_at: now - 365 * DAY,
    });
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(saying(vec![REVERSED.into()]));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(
        matches!(
            pending::poll(&server, &shared, &ck),
            Polled::Transient(Transient::TakenBack(_))
        ),
        "a year of the clock is a week of the wait"
    );
    assert!(secret_is_kept(&keyring, &claim));
    let counted = listed(&shared).taken_back.expect("counted");
    assert_eq!(
        counted.counted_at - counted.since,
        7 * DAY,
        "the step of a year counted a week"
    );

    // a step BACK: ten days ahead on the last count; the time seen so far stays what it was
    let mut ahead = listed(&shared);
    let counted = TakenBack {
        since: now - 10 * DAY,
        counted_at: now + 10 * DAY,
    };
    ahead.taken_back = Some(counted);
    pending::add(&shared, &ahead).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &ahead),
        Polled::Transient(Transient::TakenBack(_))
    ));
    let after = listed(&shared).taken_back.expect("counted");
    let before_seen = counted.counted_at - counted.since;
    let after_seen = after.counted_at - after.since;
    assert_eq!(after_seen, before_seen, "a step back counts nothing");
}

/// The look is once a day: the entry's own count says when ([`TakenBack::look_is_due`]), so a
/// restart of the app does not ask again within the day. A stamp in the future - a clock set back -
/// is stale, never fresh.
#[test]
fn x_delta_money5_1_the_look_at_a_taken_back_checkout_is_due_once_a_day() {
    assert_eq!(TAKEN_BACK_ASK_SECS, DAY);
    let at = 1_791_450_000;
    let marked = TakenBack {
        since: at - 3 * DAY,
        counted_at: at,
    };
    assert!(!marked.look_is_due(at), "just asked");
    assert!(!marked.look_is_due(at + DAY - 1));
    assert!(marked.look_is_due(at + DAY));
    assert!(marked.look_is_due(at - 1), "a stamp in the future is stale");
    assert!(
        TakenBack {
            since: 0,
            counted_at: 0
        }
        .look_is_due(at),
        "never counted: due"
    );
}

/// A claimed entry (a period of months to issue) is [`pending::finish`]'s, never ended by this
/// count: its claim secret is dead and its issue key is a paid month's. The poll says it is
/// taken back and changes nothing.
#[test]
fn x_delta_money5_1_a_poll_of_a_claimed_entry_found_taken_back_changes_nothing() {
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = unclaimed(&claim);
    ck.period = Some(PendingTokens {
        drive_id: String::from("d_1"),
        months: 12,
        issue_key: String::from(ISSUE_KEY),
        recurring: false,
        claimed_at: crate::now() - 400 * DAY,
        counted_at: crate::now() - 400 * DAY,
    });
    // an old mark of a build that counted: never ends a claimed entry either
    ck.taken_back = Some(TakenBack {
        since: crate::now() - 400 * DAY,
        counted_at: crate::now() - 400 * DAY,
    });
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(saying(vec![REVERSED.into()]));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Transient(Transient::TakenBack(_))
    ));
    assert_eq!(listed(&shared), ck, "the entry is as it was");
    assert!(secret_is_kept(&keyring, &claim));
}

/// An entry whose payment was taken back stands in nobody's way: nobody can pay it, so a new order
/// of the same intent is not "the same order, open" (the buyer pays twice for one thing) - and an
/// order that IS open still blocks, which is what the guard is for.
#[test]
fn x_delta_money5_1_a_taken_back_checkout_does_not_block_a_new_order_of_its_intent() {
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &unclaimed(&claim)).unwrap();
    let order = || {
        let claim = ClaimKey::generate().unwrap();
        PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos")
    };
    // control: the unclaimed order is open until it is known to be taken back
    assert!(matches!(
        pending::begin(&shared, &order()),
        Err(AddError::SameIntentOpen { .. })
    ));
    let transport = Shared(saying(vec![REVERSED.into()]));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &listed(&shared)),
        Polled::Transient(Transient::TakenBack(_))
    ));
    assert!(!listed(&shared).is_open_order());
    assert!(matches!(
        pending::begin(&shared, &order()),
        Ok(Begun::Fresh)
    ));
    assert_eq!(
        pending::list(&shared).unwrap().len(),
        2,
        "the old entry stays"
    );
}

/// A checkout that waits for its receipt only gives its place to a new order (the oldest first);
/// an unclaimed one that is taken back never does: its claim secret is the only key to a drive the
/// token server may restore.
#[test]
fn x_delta_money5_1_the_list_never_gives_the_place_of_a_taken_back_unclaimed_entry_to_an_order() {
    let waiter = |n: usize| {
        let claim = ClaimKey::generate().unwrap();
        let mut ck = PendingCheckout::new(
            &format!("ck_{n:026}"),
            &claim,
            "100GB",
            TOKEN,
            &"A drive with a long name ".repeat(10),
        );
        ck.started_at = 1_791_450_000 + i64::try_from(n).unwrap();
        ck.receipt_due = Some(ReceiptDue::GrantKept);
        ck.period = Some(PendingTokens {
            drive_id: format!("d_{n:024}"),
            months: 12,
            issue_key: String::from(ISSUE_KEY),
            recurring: false,
            claimed_at: 1_791_450_000,
            counted_at: 1_791_450_000,
        });
        ck
    };
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, keyring) = shared(&dir);
    // the oldest entry of the list: unclaimed, taken back
    let claim = ClaimKey::generate().unwrap();
    let mut oldest = unclaimed(&claim);
    oldest.started_at = 1_000;
    oldest.taken_back = Some(TakenBack {
        since: crate::now(),
        counted_at: crate::now(),
    });
    pending::add(&shared, &oldest).unwrap();
    // the list fills with checkouts that wait for their receipts only: it is full when one more
    // gives a place of its own away (the oldest waiter's) instead of growing
    let mut waiting = 0;
    loop {
        let before = pending::list(&shared).unwrap().len();
        pending::add(&shared, &waiter(waiting)).unwrap();
        if pending::list(&shared).unwrap().len() <= before {
            break;
        }
        waiting += 1;
        assert!(waiting < 100, "the list never fills");
    }
    assert!(waiting >= 2, "{waiting}");
    assert!(
        pending::list(&shared)
            .unwrap()
            .iter()
            .any(|c| c.checkout_id == "ck_1"),
        "a waiter gave its place, the taken-back entry did not"
    );
    // orders come, each one with room made by the oldest waiter, never by the taken-back entry
    let mut placed = 0;
    loop {
        let order = PendingCheckout::posting(
            &ClaimKey::generate().unwrap(),
            &format!("T{placed}"),
            TOKEN,
            "New",
        );
        match pending::begin(&shared, &order) {
            Ok(_) => placed += 1,
            Err(AddError::NoRoom { .. }) => break,
            Err(other) => panic!("{other}"),
        }
        assert!(
            pending::list(&shared)
                .unwrap()
                .iter()
                .any(|c| c.checkout_id == "ck_1"),
            "after {placed} orders: the taken-back entry stays"
        );
        assert!(placed < 100, "the list never fills");
    }
    assert!(placed >= 1, "a courtesy gave its place to an order");
    assert!(
        pending::list(&shared)
            .unwrap()
            .iter()
            .any(|c| c.checkout_id == "ck_1"),
        "the list is full and the taken-back entry is still on it"
    );
    assert!(secret_is_kept(&keyring, &claim));
}

/// The entry's JSON keeps its shape (rule 15): an entry that was never taken back writes no mark,
/// an entry from before this field reads without one, and the mark round-trips.
#[test]
fn x_delta_money5_1_the_mark_is_additive_in_the_entry_s_json() {
    let claim = ClaimKey::generate().unwrap();
    let plain = unclaimed(&claim);
    assert!(
        !serde_json::to_string(&plain)
            .unwrap()
            .contains("taken_back"),
        "an entry never taken back writes no mark"
    );
    let old: PendingCheckout = serde_json::from_str(
        r#"{"checkout_id": "ck_old", "claim_secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            "tier": "100GB", "started_at": 1791450000}"#,
    )
    .unwrap();
    assert_eq!(old.taken_back, None);
    let mut marked = plain;
    marked.taken_back = Some(TakenBack {
        since: 1_791_450_000,
        counted_at: 1_791_536_400,
    });
    let text = serde_json::to_string(&marked).unwrap();
    assert!(
        text.contains(r#""taken_back":{"since":1791450000,"counted_at":1791536400}"#),
        "{text}"
    );
    assert_eq!(
        serde_json::from_str::<PendingCheckout>(&text).unwrap(),
        marked
    );
}

/// The room the list keeps for a claim holds what an unclaimed entry can carry before it: the three
/// marks (in flight, unopened, taken back) are cleared by the claim, and together they are no more
/// than the room the claim itself leaves ([`pending::PERIOD_ROOM`]), so a mark never takes the room
/// another entry's claim needs.
#[test]
fn x_delta_money5_1_the_marks_an_unclaimed_entry_can_carry_fit_in_the_room_a_claim_keeps() {
    let claim = ClaimKey::generate().unwrap();
    let bare = PendingCheckout::new("ck_00000000000000000000000000", &claim, "100GB", TOKEN, "A");
    let size = |ck: &PendingCheckout| serde_json::to_string(ck).unwrap().len();
    let mut marked = bare.clone();
    marked.in_flight_since = Some(1_791_450_000);
    marked.open_failures = 1;
    marked.taken_back = Some(TakenBack {
        since: 1_791_450_000,
        counted_at: 1_791_450_000,
    });
    let marks = size(&marked) - size(&bare);
    assert!(
        marks <= pending::PERIOD_ROOM,
        "the marks of an unclaimed entry grow it by {marks} bytes, the list keeps {} for a claim",
        pending::PERIOD_ROOM
    );
}

/// The hand-over's life is ONE number: the token server keeps the sealed sign-up of a paid checkout
/// 30 days (azlin-token's `HANDOFF_KEPT_DAYS`) and a cash buyer's four times as long
/// (`CASH_HANDOFF_FACTOR`, 120 days), and the wait of a paid grant for the drive's vault is the same
/// 30 days. The server's side of the pair lives in azul-apps (iso/crates/azlin-token/src/payments.rs
/// `HANDOFF_KEPT_DAYS`, `CASH_HANDOFF_FACTOR`); a change on either side is a change of both.
#[test]
fn x_delta_money5_1_the_hand_over_s_life_is_one_number_the_token_server_s() {
    assert_eq!(HANDOVER_KEPT_SECS, 30 * DAY);
    assert_eq!(CASH_HANDOVER_KEPT_SECS, 4 * HANDOVER_KEPT_SECS);
    assert_eq!(GRANT_WAIT_SECS, HANDOVER_KEPT_SECS);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = unclaimed(&claim);
    assert_eq!(ck.handover_life_secs(), 30 * DAY);
    ck.method = String::from("cash");
    assert_eq!(ck.handover_life_secs(), 120 * DAY);
}

/// The twin of the poll (X-DELTA-MONEY5-1): the abandon of an unclaimed checkout - the buyer
/// starts another order, or closes the popover - that the token server answers `reversed`. The
/// contract calls that answer "over" (`AbandonAnswer::is_over`), and the old kit let the claim
/// secret go on it: the token server's restore then approved a drive nobody could open, through
/// this door too. Now the entry and its secret stay, the buyer is told it is over for them
/// (`Abandoned::TakenBack`: a new order may follow), and the restore's drive is claimed by the next
/// poll. On the old code the first assertion fails (`Abandoned::Ended`, the entry gone).
#[test]
fn x_delta_money5_1_the_abandon_of_a_checkout_whose_payment_was_taken_back_keeps_its_entry_and_secret(
) {
    // a list of its own for each negative control (`shared` is this test's list below)
    let list_in = |dir: &TempDir| shared(dir).0;
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &unclaimed(&claim)).unwrap();
    let abandoning = Fake::new(|call, _| {
        assert!(
            call.url.ends_with("/v1/checkout/ck_1/abandon"),
            "{}",
            call.url
        );
        Ok(json(
            200,
            r#"{"checkout_id": "ck_1", "status": "reversed"}"#,
        ))
    });
    let transport = Shared(abandoning);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(
        pending::abandon(&server, &shared, "ck_1"),
        Abandoned::TakenBack,
        "over for the buyer, not for the claim key"
    );
    assert_eq!(pending::list(&shared).unwrap().len(), 1, "the entry stays");
    assert!(secret_is_kept(&keyring, &claim), "and its claim secret");
    // the other answers that are over still end it (the negative controls)
    for status in ["expired", "declined", "rejected"] {
        let dir = TempDir::new("azcloud-taken-back");
        let other = list_in(&dir);
        pending::add(&other, &unclaimed(&ClaimKey::generate().unwrap())).unwrap();
        let body = format!(r#"{{"checkout_id": "ck_1", "status": "{status}"}}"#);
        let transport = Shared(Fake::new(move |_, _| Ok(json(200, &body))));
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        assert_eq!(
            pending::abandon(&server, &other, "ck_1"),
            Abandoned::Ended,
            "{status}"
        );
        assert!(pending::list(&other).unwrap().is_empty(), "{status}");
    }
    // the token server restores the payment: the next poll claims the drive with the kept secret
    let restored = Shared(saying(vec![approved(&claim)]));
    let server = TokenServer::new(TOKEN, &restored).unwrap();
    match pending::poll(&server, &shared, &listed(&shared)) {
        Polled::Final(Final::Claimed(claimed)) => assert_eq!(claimed.bundle.drive_id(), "d_1"),
        other => panic!("the restored drive was not claimed: {other:?}"),
    }
}

// ==== X-DELTA-MONEY6-2: the abandon makes the count the poll makes ====

/// A keyring whose writes can be refused on demand: the user's "No", a locked keychain.
struct WritesOnDemand {
    inner: MemoryKeyring,
    refuse: AtomicBool,
}

impl KeyringStore for WritesOnDemand {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.inner.get(key)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        if self.refuse.load(Ordering::SeqCst) {
            return Err(KeyringError::Denied);
        }
        self.inner.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.inner.delete(key)
    }
}

/// What the token server answers an abandon of `ck_1` with.
fn abandon_says(status: &'static str) -> Arc<Fake> {
    Fake::new(move |call, _| {
        assert!(
            call.url.ends_with("/v1/checkout/ck_1/abandon"),
            "{}",
            call.url
        );
        Ok(json(
            200,
            &format!(r#"{{"checkout_id": "ck_1", "status": "{status}"}}"#),
        ))
    })
}

/// The order the dialog places next for the same thing (a new drive of the tier): what the machine's
/// `OrderAgain` makes of an abandon that answered `Ended`.
fn order_again() -> PendingCheckout {
    PendingCheckout::posting(&ClaimKey::generate().unwrap(), "100GB", TOKEN, "Photos")
}

/// X-DELTA-MONEY6-2, the finding's scenario: the buyer presses pay again while an earlier checkout is
/// in flight; the abandon answers `reversed` (the payment was taken back in the last seconds, before
/// the claims' poll marked it). `Abandoned::TakenBack` made the machine order again at once - and the
/// entry was unmarked, so `begin` found an open order of the same intent and refused: "an earlier
/// order is still open", the dialog's "you can order again" false for the moment it was said. Now the
/// abandon makes the same count the poll makes, before it answers: the entry is no open order, the
/// next order is `Fresh`, the claim secret is kept, and the hand-over's life counts from this look.
/// On the old code the first assertion after the abandon fails (no mark) and `begin` is refused.
#[test]
fn x_delta_money6_2_an_abandon_answered_reversed_marks_the_entry_so_the_next_order_is_not_refused()
{
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &unclaimed(&claim)).unwrap();
    // the control: until it is known to be taken back the unclaimed order is open
    assert!(matches!(
        pending::begin(&shared, &order_again()),
        Err(AddError::SameIntentOpen { .. })
    ));
    let before = crate::now();
    let transport = Shared(abandon_says("reversed"));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(
        pending::abandon(&server, &shared, "ck_1"),
        Abandoned::TakenBack
    );
    let entry = listed(&shared);
    let mark = entry.taken_back.expect("the abandon counted the look");
    assert!(
        (before..=crate::now()).contains(&mark.since) && mark.since == mark.counted_at,
        "the count starts at this look: {mark:?}"
    );
    assert!(!entry.is_open_order(), "no open order: nobody can pay it");
    assert!(secret_is_kept(&keyring, &claim), "the claim secret stays");
    // the order the machine places the moment the abandon answers is not refused
    assert!(matches!(
        pending::begin(&shared, &order_again()),
        Ok(Begun::Fresh)
    ));
    assert_eq!(pending::list(&shared).unwrap().len(), 2);
    // a poll that comes later adds to the SAME count: the hand-over's life runs from the abandon's look
    let polled = Shared(saying(vec![REVERSED.into()]));
    let server = TokenServer::new(TOKEN, &polled).unwrap();
    let entry = pending::list(&shared)
        .unwrap()
        .into_iter()
        .find(|c| c.checkout_id == "ck_1")
        .unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &entry),
        Polled::Transient(Transient::TakenBack(_))
    ));
    let later = pending::list(&shared)
        .unwrap()
        .into_iter()
        .find(|c| c.checkout_id == "ck_1")
        .unwrap()
        .taken_back
        .unwrap();
    assert_eq!(later.since, mark.since, "counted from the abandon's look");
}

/// X-DELTA-MONEY6-2, the other abandon answers are not marked as taken back (the negative controls:
/// an answer that ends the checkout takes it off the list, one that leaves it open keeps it unmarked
/// and in flight), and an abandon that finds an entry whose hand-over is over - first seen taken back
/// 31 days ago and not asked since - ends it as the poll does, claim secret and all.
#[test]
fn x_delta_money6_2_only_the_answer_reversed_marks_and_an_old_mark_ends_the_entry_as_the_poll_does()
{
    // still pending: kept, in flight, never marked as taken back
    let dir = TempDir::new("azcloud-taken-back");
    let (open, _) = shared(&dir);
    pending::add(&open, &unclaimed(&ClaimKey::generate().unwrap())).unwrap();
    let transport = Shared(abandon_says("pending"));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::abandon(&server, &open, "ck_1"),
        Abandoned::Kept { paid: false, .. }
    ));
    assert_eq!(listed(&open).taken_back, None);
    assert!(listed(&open).in_flight_since.is_some());

    // a hand-over that is over: the first look was 31 days ago
    let dir = TempDir::new("azcloud-taken-back");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut old = unclaimed(&claim);
    let now = crate::now();
    old.taken_back = Some(TakenBack {
        since: now - 31 * DAY,
        counted_at: now - 3_600,
    });
    pending::add(&shared, &old).unwrap();
    let transport = Shared(abandon_says("reversed"));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(pending::abandon(&server, &shared, "ck_1"), Abandoned::Ended);
    assert!(pending::list(&shared).unwrap().is_empty(), "off the list");
    assert!(!secret_is_kept(&keyring, &claim), "the secret is erased");
}

/// X-DELTA-MONEY6-2, a list that cannot take the count: the entry stays unmarked, so it is still an
/// open order as far as the list can say - the abandon says so (`Kept`, the payment possibly under
/// way) and does not tell the buyer to order again; the claims' poll marks it when the list works.
#[test]
fn x_delta_money6_2_an_abandon_whose_count_cannot_be_written_does_not_say_order_again() {
    let dir = TempDir::new("azcloud-taken-back");
    let gate = Arc::new(WritesOnDemand {
        inner: MemoryKeyring::new(),
        refuse: AtomicBool::new(false),
    });
    let shared = SharedKeyring::with_lock_root(
        gate.clone() as Arc<dyn KeyringStore>,
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    pending::add(&shared, &unclaimed(&ClaimKey::generate().unwrap())).unwrap();
    gate.refuse.store(true, Ordering::SeqCst);
    let transport = Shared(abandon_says("reversed"));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::abandon(&server, &shared, "ck_1") {
        Abandoned::Kept { paid: false, why } => {
            assert!(why.contains("could not count it"), "{why}");
        }
        other => panic!("not kept: {other:?}"),
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 1, "the entry stays");
    assert_eq!(listed(&shared).taken_back, None);
}

