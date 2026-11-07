//! A new order is on the keyring's list BEFORE it is sent (ARCH28 RF51 step 4): an answer that is
//! lost leaves a record, the same order asked again goes out under the same claim key and gets the
//! first answer, and no second order of one intent is open at the same time - from a second
//! window or after a restart - so the buyer never pays twice for one thing.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use azul_storage::{
    keyring::{KeyringNamespace, MemoryKeyring},
    testing::TempDir,
};

use super::{header, json, Fake, Shared, TOKEN};
use crate::{
    pending::{
        self, AddError, Begun, Intent, OrderError, PendingCheckout, Polled, POSTING_LIFE_SECS,
    },
    policy::Policy,
    token::CheckoutFor,
    ClaimKey, SharedKeyring, TokenError, TokenServer,
};

/// A checkout's answer (claim contract v1).
const CHECKOUT: &str = r#"{"checkout_id": "ck_1", "pay_url": "http://127.0.0.1:18081/v1/pay/ck_1",
    "tier": "100GB", "method": "sepa", "months": 12, "amount_cents": 990, "currency": "EUR",
    "first_month_free": true, "withdrawal_consent_required": true, "mock": true}"#;

/// Two windows of one user: two lists over the same keyring and the same lock folder.
fn two_windows(dir: &TempDir) -> (SharedKeyring, SharedKeyring) {
    let keyring = Arc::new(MemoryKeyring::new());
    let window = || {
        SharedKeyring::with_lock_root(
            keyring.clone(),
            KeyringNamespace::azlin_shared(),
            &dir.path().join("locks"),
        )
    };
    (window(), window())
}

/// The idempotency key `call` carries.
fn key_of(call: &azul_storage::HttpCall) -> String {
    header(call, azlin_contract::IDEMPOTENCY_HEADER)
        .unwrap_or_default()
        .to_string()
}

/// A token server whose first answer is lost, and the others the checkout `ck_1`.
fn losing_the_first_answer() -> (Arc<Fake>, Shared) {
    let lost = AtomicBool::new(true);
    let fake = Fake::new(move |_, _| {
        if lost.swap(false, Ordering::SeqCst) {
            return Err(String::from("timed out"));
        }
        Ok(json(201, CHECKOUT))
    });
    (fake.clone(), Shared(fake))
}

/// The real entry of the checkout `ck_1` the answer named, for `claim`.
fn real(claim: &ClaimKey) -> PendingCheckout {
    PendingCheckout::new("ck_1", claim, "100GB", TOKEN, "Photos")
}

/// AZ-P4: a second order of one intent is refused while the first is open - by the list, which a
/// second window and the run after a restart both read - and is let through once the first ended.
#[test]
fn az_p4_a_second_order_of_one_intent_is_refused_in_a_second_window_and_after_a_restart() {
    let dir = TempDir::new("azcloud-orders");
    let (window_one, window_two) = two_windows(&dir);
    let claim = ClaimKey::generate().unwrap();
    let first = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
    assert!(matches!(
        pending::begin(&window_one, &first).unwrap(),
        Begun::Fresh
    ));
    // Its answer came: the checkout takes the order's place on the list.
    pending::posted(&window_one, &first.checkout_id, &real(&claim)).unwrap();

    // The second window (and a restarted app reads the same list): a new drive of the same tier.
    let other = ClaimKey::generate().unwrap();
    let second = PendingCheckout::posting(&other, "100GB", TOKEN, "Videos");
    match pending::begin(&window_two, &second) {
        Err(AddError::SameIntentOpen {
            checkout_id,
            may_be_paid,
        }) => {
            assert_eq!(checkout_id, "ck_1");
            assert!(!may_be_paid, "nothing says it was paid");
        }
        other => panic!("not refused: {other:?}"),
    }
    assert_eq!(
        pending::list(&window_two).unwrap().len(),
        1,
        "nothing was added"
    );

    // Its abandon found it paid: the refusal says so.
    let paid = Fake::new(|_, _| {
        Ok(json(
            409,
            r#"{"error": "checkout_paid", "message": "this checkout was paid"}"#,
        ))
    });
    let transport = Shared(paid);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::abandon(&server, &window_one, "ck_1"),
        pending::Abandoned::Kept { paid: true, .. }
    ));
    match pending::begin(&window_two, &second) {
        Err(AddError::SameIntentOpen { may_be_paid, .. }) => {
            assert!(may_be_paid, "its payment may be under way");
        }
        other => panic!("not refused: {other:?}"),
    }

    // Ended at the token server: the next order may begin.
    let ended = pending::Settled::abandoned(azlin_contract::AbandonAnswer::Expired).unwrap();
    assert!(pending::settle(&window_one, "ck_1", ended).unwrap());
    assert!(matches!(
        pending::begin(&window_two, &second).unwrap(),
        Begun::Fresh
    ));
}

/// AZ-P4: what a second order of another intent, a claimed checkout, a cash order and a drive
/// picked up with a claim code are: no obstacle. Two renewals of one drive are one intent; of
/// two drives, two.
#[test]
fn az_p4_only_an_open_order_of_the_same_intent_is_in_the_way() {
    let dir = TempDir::new("azcloud-orders");
    let (shared, _) = two_windows(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut renew = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
    renew.for_drive = String::from("d_1");
    assert_eq!(
        renew.intent(),
        Intent::Renew {
            drive: String::from("d_1")
        }
    );
    assert!(matches!(
        pending::begin(&shared, &renew).unwrap(),
        Begun::Fresh
    ));

    // Another drive's renewal, a change of tier of the same drive, a new drive: other intents.
    for (drive, change) in [("d_2", false), ("d_1", true), ("", false)] {
        let claim = ClaimKey::generate().unwrap();
        let mut order = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
        order.for_drive = drive.to_string();
        order.change_tier = change;
        assert!(
            matches!(pending::begin(&shared, &order).unwrap(), Begun::Fresh),
            "{drive} change={change}"
        );
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 4);

    // Not in the way of a new drive of another tier, and not in the way when they are not open
    // orders: a cash order waiting for its letter, a drive picked up with a claim code, a
    // claimed checkout whose months wait for the vault.
    let dir = TempDir::new("azcloud-orders");
    let (shared, _) = two_windows(&dir);
    let waiting = |id: &str, change: &dyn Fn(&mut PendingCheckout)| {
        let claim = ClaimKey::generate().unwrap();
        let mut ck = PendingCheckout::new(id, &claim, "100GB", TOKEN, "Photos");
        change(&mut ck);
        pending::add(&shared, &ck).unwrap();
    };
    waiting("ck_cash", &|c| c.method = String::from("cash"));
    waiting("ck_picked", &|c| c.picked_up = true);
    waiting("ck_claimed", &|c| {
        c.period = Some(pending::PendingTokens {
            drive_id: String::from("d_9"),
            months: 3,
            issue_key: String::from("k"),
            recurring: false,
            claimed_at: 0,
            counted_at: 0,
        })
    });
    let claim = ClaimKey::generate().unwrap();
    let new_drive = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
    assert!(matches!(
        pending::begin(&shared, &new_drive).unwrap(),
        Begun::Fresh
    ));
    let claim = ClaimKey::generate().unwrap();
    let bigger = PendingCheckout::posting(&claim, "1TB", TOKEN, "Photos");
    assert!(matches!(
        pending::begin(&shared, &bigger).unwrap(),
        Begun::Fresh
    ));
}

/// AZ-P8: the answer of the order is lost; the entry is on the list with the claim secret, and the
/// order asked again - by a new click, which makes a new claim key - goes out under the FIRST
/// claim key, with the first idempotency key, so the token server answers with its first checkout
/// instead of making a second one nobody can pay.
#[test]
fn az_p8_a_lost_answer_leaves_a_record_and_the_same_order_asked_again_gets_the_first_answer() {
    let dir = TempDir::new("azcloud-orders");
    let (shared, _) = two_windows(&dir);
    let (fake, transport) = losing_the_first_answer();
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO)
        .with_policy(Policy {
            attempts: 1,
            ..Policy::TOKEN
        })
        .with_outbox(shared.outbox());

    let first = ClaimKey::generate().unwrap();
    let draft = PendingCheckout::posting(&first, "100GB", TOKEN, "Photos");
    let lost = pending::place_order(&shared, &draft, |claim| {
        server.checkout("100GB", 12, "sepa", claim, CheckoutFor::NewDrive)
    })
    .expect_err("no answer");
    assert!(matches!(lost, OrderError::Token(_)), "{lost:?}");
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "the record of the order");
    assert!(listed[0].is_posting());
    assert_eq!(listed[0].claim_secret, first.to_base64());

    // A new click: a new claim key in a new draft - the list's entry is the order's.
    let second = ClaimKey::generate().unwrap();
    let draft = PendingCheckout::posting(&second, "100GB", TOKEN, "Photos");
    let placed = pending::place_order(&shared, &draft, |claim| {
        assert_eq!(
            claim.to_base64(),
            first.to_base64(),
            "the claim key of the first attempt"
        );
        server.checkout("100GB", 12, "sepa", claim, CheckoutFor::NewDrive)
    })
    .expect("the first answer");
    assert_eq!(placed.answer.checkout_id, "ck_1");
    let keys: Vec<String> = fake.calls().iter().map(key_of).collect();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(!keys[0].is_empty());
    assert_eq!(
        keys[0], keys[1],
        "the same key: the token server's first answer"
    );

    // The checkout takes the record's place; nothing else is on the list.
    pending::posted(&shared, &placed.posting_id, &real(&placed.claim)).unwrap();
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].checkout_id, "ck_1");
    assert!(!listed[0].is_posting());
    assert_eq!(listed[0].claim_secret, first.to_base64());
}

/// AZ-P8: an order the token server REFUSES made no checkout: its record goes. One that got no
/// answer, or a busy one, may have made one: the record stays.
#[test]
fn az_p8_a_refused_order_leaves_no_record_and_a_busy_server_leaves_one() {
    let dir = TempDir::new("azcloud-orders");
    let (shared, _) = two_windows(&dir);
    let refused = Fake::new(|_, _| {
        Ok(json(
            400,
            r#"{"error": "bad_tier", "message": "no such tier"}"#,
        ))
    });
    let transport = Shared(refused);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let claim = ClaimKey::generate().unwrap();
    let draft = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
    let error = pending::place_order(&shared, &draft, |claim| {
        server.checkout("100GB", 12, "sepa", claim, CheckoutFor::NewDrive)
    })
    .expect_err("refused");
    assert!(
        matches!(error, OrderError::Token(TokenError::Refused { .. })),
        "{error:?}"
    );
    assert!(
        pending::list(&shared).unwrap().is_empty(),
        "nothing was made"
    );

    let busy = Fake::new(|_, _| Ok(json(503, r#"{"error": "unavailable"}"#)));
    let transport = Shared(busy);
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO)
        .with_policy(Policy {
            attempts: 1,
            ..Policy::TOKEN
        });
    let draft = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
    let error = pending::place_order(&shared, &draft, |claim| {
        server.checkout("100GB", 12, "sepa", claim, CheckoutFor::NewDrive)
    })
    .expect_err("busy");
    assert!(
        matches!(error, OrderError::Token(TokenError::Busy { .. })),
        "{error:?}"
    );
    assert_eq!(pending::list(&shared).unwrap().len(), 1, "it may exist");
}

/// AZ-P8: the record of an order without an answer is kept a day - its payment page was never
/// shown - and a real checkout is never swept, however old.
#[test]
fn az_p8_the_record_of_an_order_without_an_answer_goes_after_a_day() {
    let dir = TempDir::new("azcloud-orders");
    let (shared, _) = two_windows(&dir);
    let claim = ClaimKey::generate().unwrap();
    let draft = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
    pending::begin(&shared, &draft).unwrap();
    let other = ClaimKey::generate().unwrap();
    let mut old = real(&other);
    old.started_at = 1;
    pending::add(&shared, &old).unwrap();

    let at = draft.started_at;
    assert_eq!(
        pending::forget_stale_postings(&shared, at + POSTING_LIFE_SECS - 1).unwrap(),
        0,
        "one second short of the day"
    );
    assert_eq!(pending::list(&shared).unwrap().len(), 2);
    assert_eq!(
        pending::forget_stale_postings(&shared, at + POSTING_LIFE_SECS).unwrap(),
        1
    );
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].checkout_id, "ck_1", "a real checkout stays");
}

/// AZ-P8: no poll asks the token server about an order that has no checkout yet.
#[test]
fn az_p8_a_poll_does_not_ask_about_an_order_without_a_checkout() {
    let dir = TempDir::new("azcloud-orders");
    let (shared, _) = two_windows(&dir);
    let claim = ClaimKey::generate().unwrap();
    let draft = PendingCheckout::posting(&claim, "100GB", TOKEN, "Photos");
    pending::begin(&shared, &draft).unwrap();
    let fake = Fake::new(|call, _| panic!("nothing is asked: {}", call.url));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &draft),
        Polled::Pending
    ));
}

/// AZ-P4: the list stays within what every keyring keeps in one entry: an order that does not fit
/// is refused before it is sent, with the room said.
#[test]
fn az_p4_an_order_that_does_not_fit_is_refused_before_it_is_sent() {
    let dir = TempDir::new("azcloud-orders");
    let (shared, _) = two_windows(&dir);
    let mut added = 0;
    loop {
        let claim = ClaimKey::generate().unwrap();
        let mut order = PendingCheckout::posting(&claim, &format!("T{added}"), TOKEN, "A drive");
        order.name = "A drive with a long name ".repeat(10);
        match pending::begin(&shared, &order) {
            Ok(_) => added += 1,
            Err(AddError::NoRoom { waiting }) => {
                assert_eq!(waiting, added);
                break;
            }
            Err(other) => panic!("not the room: {other:?}"),
        }
        assert!(added < 100, "the list never stops growing");
    }
    assert!(added >= 5, "a few orders always fit, not {added}");
}
