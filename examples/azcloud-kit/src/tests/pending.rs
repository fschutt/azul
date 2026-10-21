//! The unfinished checkouts: one keyring entry changed under its lock (two windows lose none of
//! each other's), polled into the drive (its session kept once, by drive id: a rotated one is
//! never replaced by the sign-up's spent token) or dropped (said once).

use std::sync::Arc;

use azul_storage::{
    keyring::KeyringNamespace,
    config::keyring_key,
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
};

use super::{
    bundle, json,
    period::{issuing_server, keys, pem, D1, ISSUE_KEY, N1},
    Fake, Shared, TOKEN,
};
use crate::{
    bundle::PeriodTokens,
    claim::seal,
    pending::{self, Finished, PendingCheckout, Polled},
    period::{Issuer, PeriodTokenStore},
    shared::SharedKeyring,
    AzlinSession, ClaimKey, TokenServer,
};

/// The keyring and the locks of one user, and the keyring itself to look into.
fn shared(dir: &TempDir) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::with_lock_root(
        keyring.clone(),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    (shared, keyring)
}

fn checkout(id: &str, claim: &ClaimKey) -> PendingCheckout {
    PendingCheckout {
        checkout_id: id.to_string(),
        claim_secret: claim.to_base64(),
        tier: String::from("100GB"),
        started_at: 1_791_450_000,
        token_url: TOKEN.to_string(),
        name: String::from("Photos"),
        period: None,
        method: String::new(),
        cash: None,
        picked_up: false,
        in_flight_since: None,
        open_failures: 0,
    }
}

/// The token server of `checkout_id`, approved: its sign-up (the test bundle, drive `d_1`)
/// sealed to `claim`.
fn approving(checkout_id: &'static str, claim: &ClaimKey) -> Arc<Fake> {
    let sealed = seal(
        &claim.public_base64(),
        checkout_id,
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    Fake::new(move |_, _| {
        Ok(json(
            200,
            &format!(
                r#"{{"checkout_id": "{checkout_id}", "status": "approved",
                     "sealed_signup": "{sealed}"}}"#
            ),
        ))
    })
}

fn token_of(text: &str) -> String {
    AzlinSession::from_keyring_secret(text)
        .unwrap()
        .drive_token
        .as_str()
        .to_string()
}

#[test]
fn the_unfinished_checkouts_are_one_keyring_entry_two_windows_change_without_losing_any() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    assert!(pending::list(&shared).unwrap().is_empty());
    let windows: Vec<_> = (0..2)
        .map(|window| {
            // Each window its own keyring handle and locks over the same entry and folder.
            let shared = SharedKeyring::with_lock_root(
                keyring.clone() as Arc<dyn KeyringStore>,
                KeyringNamespace::azlin_shared(),
                &dir.path().join("locks"),
            );
            // Four each: eight checkouts fit in one entry (MAX_PENDING_BYTES) with room left
            // for each one's issue key after its claim.
            std::thread::spawn(move || {
                for i in 0..4 {
                    let claim = ClaimKey::generate().unwrap();
                    pending::add(&shared, &checkout(&format!("ck_{window}_{i}"), &claim))
                        .unwrap();
                }
            })
        })
        .collect();
    for window in windows {
        window.join().unwrap();
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 8, "none lost");
    let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
    assert!(text.contains("ck_0_3") && text.contains("ck_1_0"));
    let ended = || Settled::abandoned(azlin_contract::AbandonAnswer::Expired).unwrap();
    assert!(pending::settle(&shared, "ck_0_3", ended()).unwrap());
    assert!(!pending::settle(&shared, "ck_0_3", ended()).unwrap(), "removed once");
    assert_eq!(pending::list(&shared).unwrap().len(), 7);
    // One entry per checkout: adding one again replaces it.
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &checkout("ck_1_1", &claim)).unwrap();
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 7);
    let again = listed.iter().find(|c| c.checkout_id == "ck_1_1").unwrap();
    assert_eq!(again.claim_secret, claim.to_base64());
}

#[test]
fn the_unfinished_checkouts_stay_within_what_every_keyring_keeps_in_one_entry() {
    // Windows' Credential Manager keeps at most 2560 bytes per entry, the strictest keyring:
    // the list never grows past it - a checkout that would not fit is refused (before its
    // payment page opens), and a name is cut to what a drive name needs.
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let mut added = 0;
    loop {
        let claim = ClaimKey::generate().unwrap();
        let mut ck = checkout(&format!("ck_{added:026}"), &claim);
        ck.name = "A drive with a long name ".repeat(10);
        match pending::add(&shared, &ck) {
            Ok(()) => added += 1,
            Err(e) => {
                assert!(e.to_string().contains("unfinished checkouts"), "{e}");
                break;
            }
        }
        let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
        assert!(text.len() <= pending::MAX_PENDING_BYTES, "{} bytes", text.len());
        assert!(added < 100, "the list never stops growing");
    }
    assert!(added >= 5, "a few checkouts always fit, not {added}");
    let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
    assert!(text.len() <= pending::MAX_PENDING_BYTES, "{} bytes", text.len());
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), added, "every kept checkout is whole");
    assert!(listed.iter().all(|c| c.name.chars().count() <= 64));
    assert!(listed.iter().all(|c| c.claim_key().is_ok()));
    // Taking one off makes room again.
    let ended = Settled::abandoned(azlin_contract::AbandonAnswer::Expired).unwrap();
    assert!(pending::settle(&shared, &listed[0].checkout_id, ended).unwrap());
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &checkout("ck_room", &claim)).unwrap();
}

#[test]
fn a_pending_checkout_keeps_its_claim_secret_and_debug_shows_none() {
    let claim = ClaimKey::generate().unwrap();
    let pending = checkout("ck_1", &claim);
    assert_eq!(
        pending.claim_key().unwrap().public_base64(),
        claim.public_base64()
    );
    let shown = format!("{pending:?}");
    assert!(
        shown.contains("ck_1") && !shown.contains(claim.to_base64().as_str()),
        "{shown}"
    );
}

#[test]
fn an_approved_checkout_becomes_the_drive_and_its_session_is_kept_once_by_drive_id() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    let transport = Shared(approving("ck_1", &claim));
    let server = TokenServer::new(TOKEN, &transport).unwrap();

    let first = match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => claimed,
        other => panic!("not claimed: {other:?}"),
    };
    assert!(!first.already);
    assert_eq!(first.bundle.drive_id(), "d_1");
    let kept = keyring.get(&keyring_key("d_1")).unwrap().unwrap();
    assert_eq!(token_of(&kept), "dt_f.0.aaa");
    assert_eq!(first.session, kept);
    assert!(!format!("{first:?}").contains("dt_f.0.aaa"), "Debug shows no token");
    // The app adds the drive, then takes the checkout off the list: until then it stays.
    assert_eq!(pending::list(&shared).unwrap().len(), 1);

    // Meanwhile the drive refreshed and its token rotated: the next claim (another window, the
    // next start) keeps the rotated session - the sign-up's token is spent.
    let mut rotated = AzlinSession::from_keyring_secret(&kept).unwrap();
    rotated.drive_token = crate::DriveToken::new("dt_f.1.bbb");
    keyring
        .set(&keyring_key("d_1"), &rotated.to_keyring_secret())
        .unwrap();
    let again = match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Claimed(claimed)) => claimed,
        other => panic!("not claimed: {other:?}"),
    };
    assert!(again.already);
    assert_eq!(token_of(&again.session), "dt_f.1.bbb");
    assert_eq!(
        token_of(&keyring.get(&keyring_key("d_1")).unwrap().unwrap()),
        "dt_f.1.bbb",
        "the spent token never comes back"
    );
}

#[test]
fn a_declined_or_gone_checkout_is_dropped_and_said_once() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let declined = checkout("ck_declined", &claim);
    let gone = checkout("ck_gone", &claim);
    let expired = checkout("ck_expired", &claim);
    for ck in [&declined, &gone, &expired] {
        pending::add(&shared, ck).unwrap();
    }
    let fake = Fake::new(|call, _| {
        Ok(if call.url.ends_with("/ck_declined") {
            json(
                200,
                r#"{"checkout_id": "ck_declined", "status": "declined",
                    "reason": "insufficient funds"}"#,
            )
        } else if call.url.ends_with("/ck_expired") {
            json(200, r#"{"checkout_id": "ck_expired", "status": "expired"}"#)
        } else {
            json(
                404,
                r#"{"error": "no_such_checkout", "message": "unknown checkout"}"#,
            )
        })
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &declined) {
        Polled::Final(Final::Declined(why)) => assert!(why.contains("insufficient funds"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    match pending::poll(&server, &shared, &gone) {
        Polled::Final(Final::Gone(why)) => assert!(why.contains("unknown checkout"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    assert!(matches!(
        pending::poll(&server, &shared, &expired),
        Polled::Final(Final::Gone(_))
    ));
    assert!(pending::list(&shared).unwrap().is_empty());
    // Another window polling the same checkout afterwards has nothing more to say.
    assert!(matches!(
        pending::poll(&server, &shared, &declined),
        Polled::Settled
    ));
}

#[test]
fn a_checkout_the_token_server_cannot_answer_about_is_kept_for_the_next_try() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    let fake = Fake::new(move |_, n| match n {
        0 => Err(String::from("connection refused")),
        1 => Ok(json(503, r#"{"error": "busy", "message": "try later"}"#)),
        2 => Ok(json(200, r#"{"checkout_id": "ck_1", "status": "pending"}"#)),
        3 => Ok(json(200, r#"{"checkout_id": "ck_1", "status": "on_hold"}"#)),
        _ => Ok(json(200, "not json")),
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Transient(Transient::NoAnswer(_))
    ));
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Transient(Transient::Busy(_))
    ));
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Pending
    ));
    // A status of a newer token server, an answer that is none: kept, never taken as an end.
    for _ in 0..2 {
        assert!(matches!(
            pending::poll(&server, &shared, &ck),
            Polled::Transient(Transient::Unread(_))
        ));
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
}

#[test]
fn a_sign_up_that_does_not_open_twice_ends_its_checkout_with_an_error_id() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let other = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    let sealed_to_another = seal(
        &other.public_base64(),
        "ck_1",
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    let fake = Fake::new(move |_, _| {
        Ok(json(
            200,
            &format!(
                r#"{{"checkout_id": "ck_1", "status": "approved",
                     "sealed_signup": "{sealed_to_another}"}}"#
            ),
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // The first time may have been a damaged answer: asked once more.
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Transient(Transient::Unopened(_))
    ));
    assert_eq!(pending::list(&shared).unwrap()[0].open_failures, 1);
    // A list read again by another window counts on from the list, not from its own copy.
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Unopenable { error_id, .. }) => assert_eq!(error_id, "ck_1"),
        other => panic!("not given up: {other:?}"),
    }
    assert!(pending::list(&shared).unwrap().is_empty(), "its room in the list is free again");
}

#[test]
fn an_approved_checkout_whose_hand_over_is_gone_ends_and_says_so() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    let fake = Fake::new(|_, _| {
        Ok(json(
            200,
            r#"{"checkout_id": "ck_1", "status": "approved", "tier": "100GB", "months": 12,
                "amount_cents": 990}"#,
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Gone(why)) => {
            assert!(why.contains("30 days") && why.contains("ck_1"), "{why}");
        }
        other => panic!("not ended: {other:?}"),
    }
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// A token server whose abandon of `ck_1` answers `abandon` (its HTTP status and body), whose
/// poll answers pending until the abandon was asked, then approves it (sealed to `claim`).
fn abandoning(claim: &ClaimKey, abandon: (u16, &'static str)) -> Arc<Fake> {
    let sealed = seal(
        &claim.public_base64(),
        "ck_1",
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    let asked = std::sync::atomic::AtomicBool::new(false);
    Fake::new(move |call, _| {
        if call.url.ends_with("/abandon") {
            asked.store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(json(abandon.0, abandon.1));
        }
        Ok(if asked.load(std::sync::atomic::Ordering::SeqCst) {
            json(
                200,
                &format!(
                    r#"{{"checkout_id": "ck_1", "status": "approved",
                         "sealed_signup": "{sealed}"}}"#
                ),
            )
        } else {
            json(200, r#"{"checkout_id": "ck_1", "status": "pending"}"#)
        })
    })
}

#[test]
fn an_abandon_the_token_server_does_not_end_keeps_the_checkout_and_its_drive_is_claimed() {
    // Paid meanwhile (409 checkout_paid), still waiting for its money, the provider not
    // answering, no answer at all: the claim key stays, so the payment still brings the drive.
    let cases: [(u16, &'static str, bool); 4] = [
        (409, r#"{"error": "checkout_paid", "message": "the drive comes"}"#, true),
        (200, r#"{"checkout_id": "ck_1", "status": "pending"}"#, false),
        (502, r#"{"error": "provider_unavailable", "message": "no answer"}"#, false),
        (200, r#"{"checkout_id": "ck_1", "status": "frozen"}"#, false),
    ];
    for (status, body, paid_said) in cases {
        let dir = TempDir::new("azcloud-pending");
        let (shared, _) = shared(&dir);
        let claim = ClaimKey::generate().unwrap();
        let ck = checkout("ck_1", &claim);
        pending::add(&shared, &ck).unwrap();
        let transport = Shared(abandoning(&claim, (status, body)));
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        match pending::abandon(&server, &shared, "ck_1") {
            Abandoned::Kept { paid, .. } => assert_eq!(paid, paid_said, "{body}"),
            other => panic!("{body}: not kept: {other:?}"),
        }
        let listed = pending::list(&shared).unwrap();
        assert_eq!(listed.len(), 1, "{body}: the claim key stays");
        assert!(listed[0].in_flight_since.is_some(), "{body}: marked in flight");
        match pending::poll(&server, &shared, &listed[0]) {
            Polled::Final(Final::Claimed(claimed)) => assert_eq!(claimed.bundle.drive_id(), "d_1"),
            other => panic!("{body}: the paid drive did not arrive: {other:?}"),
        }
    }
    // No answer at all.
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
    let silent = Fake::new(|_, _| Err(String::from("timed out")));
    let transport = Shared(silent);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::abandon(&server, &shared, "ck_1"),
        Abandoned::Kept { paid: false, .. }
    ));
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
}

#[test]
fn an_abandon_that_ended_the_checkout_takes_it_off_the_list() {
    for body in [
        r#"{"checkout_id": "ck_1", "status": "expired"}"#,
        r#"{"checkout_id": "ck_1", "status": "declined"}"#,
    ] {
        let dir = TempDir::new("azcloud-pending");
        let (shared, _) = shared(&dir);
        let claim = ClaimKey::generate().unwrap();
        pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
        let transport = Shared(abandoning(&claim, (200, body)));
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        assert_eq!(pending::abandon(&server, &shared, "ck_1"), Abandoned::Ended, "{body}");
        assert!(pending::list(&shared).unwrap().is_empty(), "{body}");
        assert_eq!(
            pending::abandon(&server, &shared, "ck_1"),
            Abandoned::Settled,
            "a second window finds it gone"
        );
    }
    // A pending answer never ends it, whatever it is called from.
    assert!(Settled::abandoned(azlin_contract::AbandonAnswer::StillPending).is_none());
}

/// What `ck_1`'s sealed sign-up grants: `months` period tokens against the tests' issue key.
fn grant(months: u32) -> PeriodTokens {
    PeriodTokens {
        checkout_id: String::from("ck_1"),
        months,
        issue_key: String::from(ISSUE_KEY),
    }
}

#[test]
fn a_claimed_checkout_keeps_its_issue_key_with_its_claim_secret_until_its_period_tokens_are_kept(
) {
    // AZDRIVE-INTEGRATION §4: the claim secret stays until the drive is saved AND the period
    // tokens are issued; the issue key is kept with it (the sealed sign-up is purged after 30
    // days) and goes with every issue; then both are gone.
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
    pending::claimed(&shared, "ck_1", "d_1", Some(&grant(3))).unwrap();
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "a claimed checkout stays until its tokens are kept");
    let ck = listed[0].clone();
    assert_eq!(ck.claim_secret, claim.to_base64(), "the claim secret stays");
    let owed = ck.period.clone().expect("what the issue needs");
    assert_eq!(
        (owed.drive_id.as_str(), owed.months, owed.issue_key.as_str()),
        ("d_1", 3, ISSUE_KEY)
    );
    assert!(!format!("{ck:?}").contains(ISSUE_KEY), "Debug shows no issue key");

    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    match pending::finish(&server, &shared, &store, &ck, &owed) {
        Finished::Issued { drive_id, count } => assert_eq!((drive_id.as_str(), count), ("d_1", 3)),
        other => panic!("not issued: {other:?}"),
    }
    let kept = store.tokens("d_1").unwrap();
    assert_eq!(kept.len(), 3);
    let issuer = Issuer::new("100GB", 2026, &pem(N1)).unwrap();
    assert!(kept.iter().all(|token| issuer.verify(token).is_ok()));
    // Off the list: no claim secret, no issue key is left in the keyring, and the issue request
    // (kept for an identical resend) is gone too.
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(keyring.get(pending::PENDING_KEY).unwrap().is_none());
    assert!(store.issue_request("ck_1").unwrap().is_none());
    // Another window finishing the same checkout afterwards issues nothing.
    let calls = fake.calls().len();
    assert!(matches!(
        pending::finish(&server, &shared, &store, &ck, &owed),
        Finished::Settled
    ));
    assert_eq!(fake.calls().len(), calls);
}

#[test]
fn a_drive_without_period_tokens_takes_its_checkout_off_the_list_once_it_is_saved() {
    // A development sign-up, a checkout approved before period tokens: nothing more to issue.
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
    pending::claimed(&shared, "ck_1", "d_1", None).unwrap();
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(keyring.get(pending::PENDING_KEY).unwrap().is_none());
}

#[test]
fn period_tokens_issued_before_or_refused_for_their_issue_key_drop_the_checkout_and_a_failed_issue_keeps_it(
) {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let keys = keys();
    let fake = Fake::new(move |call, n| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &keys));
        }
        // The keys once (call 0); the request kept after the first try is sent again as it is.
        match n {
            1 => Err(String::from("connection refused")),
            2 => Ok(json(503, r#"{"error": "busy", "message": "try later"}"#)),
            3 => Ok(json(
                409,
                r#"{"error": "already_issued", "message": "3 of 3 tokens already issued"}"#,
            )),
            _ => Ok(json(
                403,
                r#"{"error": "issue_key_wrong", "message": "not this checkout's issue key"}"#,
            )),
        }
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
    pending::claimed(&shared, "ck_1", "d_1", Some(&grant(3))).unwrap();
    let ck = pending::list(&shared).unwrap().remove(0);
    let owed = ck.period.clone().unwrap();
    // No answer, a busy server: kept, issue key and all, for the next try.
    for _ in 0..2 {
        assert!(matches!(
            pending::finish(&server, &shared, &store, &ck, &owed),
            Finished::Kept(_)
        ));
        let listed = pending::list(&shared).unwrap();
        assert_eq!(listed[0].period.as_ref(), Some(&owed));
        assert!(store.issue_request("ck_1").unwrap().is_some(), "for the identical resend");
    }
    // Issued before (and its answer purged): nothing to wait for any more - said once.
    match pending::finish(&server, &shared, &store, &ck, &owed) {
        Finished::Dropped(why) => assert!(why.contains("issued"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(store.issue_request("ck_1").unwrap().is_none());
    // A checkout from before period tokens has no issue key the token server takes.
    pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
    pending::claimed(&shared, "ck_1", "d_1", Some(&grant(3))).unwrap();
    match pending::finish(&server, &shared, &store, &ck, &owed) {
        Finished::Dropped(why) => assert!(why.contains("support"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(store.tokens("d_1").unwrap().is_empty());
}

#[test]
fn a_full_list_of_checkouts_still_has_room_for_each_ones_issue_key_after_its_claim() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let mut ids = Vec::new();
    loop {
        let claim = ClaimKey::generate().unwrap();
        let mut ck = checkout(&format!("ck_{:026}", ids.len()), &claim);
        ck.name = "A drive with a long name ".repeat(10);
        if pending::add(&shared, &ck).is_err() {
            break;
        }
        ids.push(ck.checkout_id);
        assert!(ids.len() < 100, "the list never stops growing");
    }
    assert!(ids.len() >= 5, "a few checkouts always fit, not {}", ids.len());
    for (i, id) in ids.iter().enumerate() {
        let drive_id = format!("d_{i:024}");
        let mut grant = grant(24);
        grant.checkout_id.clone_from(id);
        pending::claimed(&shared, id, &drive_id, Some(&grant)).unwrap();
        let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
        assert!(text.len() <= pending::MAX_PENDING_BYTES, "{} bytes", text.len());
    }
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), ids.len());
    assert!(listed.iter().all(|c| c.period.is_some()));
}
