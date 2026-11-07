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
    period::{issuing_server, keys, pem, D1, ISSUE_KEY, N1, NOW},
    tokens_in_drive, DownVault, Fake, Shared, TOKEN,
};
use crate::{
    bundle::PeriodTokens,
    claim::seal,
    pending::{self, Abandoned, Final, Finished, PendingCheckout, Polled, Settled, Transient},
    period::{Issuer, PeriodTokenStore},
    shared::SharedKeyring,
    vault::{DriveTokens, NoVault, TokenVault, VaultState},
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
        for_drive: String::new(),
        renews: String::new(),
        change_tier: false,
        ladder_version: None,
        drive_key: None,
        key_id: None,
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
fn a_cash_checkout_keeps_its_new_drives_key_and_the_id_its_claim_code_names_and_debug_hides_the_key(
) {
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = checkout("ck_1", &claim);
    ck.method = String::from("cash");
    ck.drive_key = Some("4b".repeat(32));
    ck.key_id = Some("5a".repeat(16));
    pending::add(&shared, &ck).unwrap();
    assert_eq!(pending::list(&shared).unwrap(), vec![ck.clone()]);
    let shown = format!("{ck:?}");
    assert!(!shown.contains("4b4b") && shown.contains(&"5a".repeat(16)), "{shown}");
    // A checkout without them writes neither: an older reader takes the list.
    let plain = checkout("ck_2", &claim);
    pending::add(&shared, &plain).unwrap();
    let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
    assert_eq!(text.matches("drive_key").count(), 1, "{text}");
    assert_eq!(text.matches("key_id").count(), 1, "{text}");
    // And an entry from before reads without them.
    let old: PendingCheckout = serde_json::from_str(
        r#"{"checkout_id": "ck_old", "claim_secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            "tier": "100GB", "started_at": 1791450000}"#,
    )
    .unwrap();
    assert_eq!((old.drive_key, old.key_id), (None, None));
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
        Polled::Transient(Transient::Busy { .. })
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

/// X-DELTA-AZDRIVE-MONEY-5: an abandon decides from the token server's answer, not from whether an
/// entry is left on the list. The dialog was left in flight, the background claims claimed and
/// finished the paid checkout (its entry is gone), the user presses Buy again: the abandon meets
/// 409 `checkout_paid` - the checkout is NOT ended, it is paid - and an app that heard `Settled`
/// ordered again for what was paid. Only an answer that ends the checkout is `Settled` when the
/// entry is gone.
#[test]
fn x_delta_azdrive_money_5_a_paid_checkout_whose_entry_is_gone_is_not_ended_by_its_abandon() {
    let cases: [(u16, &'static str, Option<bool>); 4] = [
        (
            409,
            r#"{"error": "checkout_paid", "message": "the drive comes"}"#,
            Some(true),
        ),
        (
            200,
            r#"{"checkout_id": "ck_1", "status": "pending"}"#,
            Some(false),
        ),
        (
            502,
            r#"{"error": "provider_unavailable", "message": "no answer"}"#,
            Some(false),
        ),
        // an answer that ends it: settled, as before
        (200, r#"{"checkout_id": "ck_1", "status": "expired"}"#, None),
    ];
    for (status, body, paid_said) in cases {
        let dir = TempDir::new("azcloud-pending");
        let (shared, _) = shared(&dir);
        let claim = ClaimKey::generate().unwrap();
        pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
        // The claims finished it meanwhile: off the list.
        let finished = Settled::abandoned(azlin_contract::AbandonAnswer::Expired).unwrap();
        assert!(pending::settle(&shared, "ck_1", finished).unwrap());
        let transport = Shared(abandoning(&claim, (status, body)));
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        match (pending::abandon(&server, &shared, "ck_1"), paid_said) {
            (Abandoned::Kept { paid, .. }, Some(said)) => assert_eq!(paid, said, "{body}"),
            (Abandoned::Settled, None) => {}
            (other, _) => panic!("{body}: {other:?}"),
        }
    }
}

/// What `ck_1`'s sealed sign-up grants: `months` period tokens against the tests' issue key.
fn grant(months: u32) -> PeriodTokens {
    PeriodTokens {
        checkout_id: String::from("ck_1"),
        months,
        issue_key: String::from(ISSUE_KEY),
        recurring: false,
    }
}

#[test]
fn a_claimed_checkout_keeps_its_issue_key_with_its_claim_secret_until_its_grant_is_kept_with_the_drive(
) {
    // AZDRIVE-INTEGRATION §4: the claim secret stays until the drive is saved AND its period
    // grant is kept with the drive (where every device of it issues the months, one a month);
    // the issue key is kept with it until then; then both leave the keyring.
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &checkout("ck_1", &claim)).unwrap();
    pending::claimed(&shared, "ck_1", "d_1", Some(&grant(3))).unwrap();
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "a claimed checkout stays until its grant is kept");
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
    let (tokens, in_drive) = tokens_in_drive(&store);
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Issued { drive_id, count } => assert_eq!((drive_id.as_str(), count), ("d_1", 1)),
        other => panic!("not issued: {other:?}"),
    }
    assert!(
        in_drive.read().unwrap().grant("ck_1").is_some(),
        "the grant is in the drive's vault before the keyring lets go of it"
    );
    let state = tokens.state().unwrap();
    assert_eq!(state.tokens.len(), 1, "the first month");
    let issuer = Issuer::new("100GB", 2026, &pem(N1)).unwrap();
    assert!(state.tokens.iter().all(|token| issuer.verify(token).is_ok()));
    let kept = state.grant("ck_1").expect("the grant, with the drive");
    assert_eq!((kept.months, kept.issued, kept.issue_key.as_str()), (3, 1, ISSUE_KEY));
    // Off the list: no claim secret, no issue key is left in the keyring.
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(keyring.get(pending::PENDING_KEY).unwrap().is_none());
    // Another window finishing the same checkout afterwards issues nothing.
    let calls = fake.calls().len();
    assert!(matches!(
        pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
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

/// PT4: a first month without an answer waits with the drive (the grant is kept there, the
/// checkout leaves the list); a payment taken back after the approval, or a checkout from before
/// period tokens, is said once with its words and never asked again.
#[test]
fn a_first_month_without_an_answer_waits_with_the_drive_and_a_reversed_payment_is_said_once() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let keys = keys();
    let fake = Fake::new(move |call, n| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &keys));
        }
        match n {
            1 => Err(String::from("connection refused")),
            _ if call.body.windows(4).any(|w| w == b"ck_2") => Ok(json(
                409,
                r#"{"error": "not_paid", "message": "the checkout is not approved"}"#,
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
    let (tokens, _) = tokens_in_drive(&store);
    let claimed = |id: &str| {
        pending::add(&shared, &checkout(id, &claim)).unwrap();
        let mut grant = grant(3);
        grant.checkout_id = id.to_string();
        pending::claimed(&shared, id, "d_1", Some(&grant)).unwrap();
        let ck = pending::list(&shared).unwrap().into_iter().find(|c| c.checkout_id == id).unwrap();
        let owed = ck.period.clone().unwrap();
        (ck, owed)
    };
    // No answer: the grant waits with the drive, its request kept for the identical resend.
    let (ck, owed) = claimed("ck_1");
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Granted { drive_id, .. } => assert_eq!(drive_id, "d_1"),
        other => panic!("not granted: {other:?}"),
    }
    assert!(pending::list(&shared).unwrap().is_empty());
    let state = tokens.state().unwrap();
    assert!(state.grant("ck_1").is_some_and(|g| g.is_open()));
    assert_eq!(state.open_issues("ck_1").len(), 1);
    // A payment taken back: said once, in its words.
    let (ck, owed) = claimed("ck_2");
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Dropped(why) => assert!(why.contains("taken back"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(tokens.state().unwrap().grant("ck_2").is_some_and(|g| !g.is_open()));
    // A checkout from before period tokens has no issue key the token server takes.
    let (ck, owed) = claimed("ck_3");
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Dropped(why) => assert!(why.contains("support"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    assert!(tokens.state().unwrap().tokens.is_empty());
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

// ==== RF51: evidence, not a guess (AZ-P1, AZ-P5, AZ-P6, X-AR5b-7) ====

/// A claimed checkout `ck_1` on the list, claimed at `claimed_at` (the list stamps the claim with
/// this computer's clock; the test puts the moment it wants in its place), with what its issue
/// needs.
fn claimed_on_the_list(
    shared: &SharedKeyring,
    claimed_at: i64,
) -> (PendingCheckout, pending::PendingTokens) {
    let claim = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(shared, &ck).unwrap();
    pending::claimed(shared, "ck_1", "d_1", Some(&grant(3))).unwrap();
    restamp(shared, "ck_1", claimed_at, 0);
    let ck = pending::list(shared).unwrap().remove(0);
    let owed = ck.period.clone().expect("what the issue needs");
    (ck, owed)
}

/// The wait of the claimed checkout `id` as a test wants it: claimed at `claimed_at`, counted to
/// `counted_at` (0: not yet).
fn restamp(shared: &SharedKeyring, id: &str, claimed_at: i64, counted_at: i64) -> PendingCheckout {
    let mut ck = pending::list(shared)
        .unwrap()
        .into_iter()
        .find(|c| c.checkout_id == id)
        .expect("on the list");
    let period = ck.period.as_mut().expect("claimed");
    period.claimed_at = claimed_at;
    period.counted_at = counted_at;
    pending::add(shared, &ck).unwrap();
    ck
}

/// AZ-P1: the poll and the abandon end a checkout, and erase its claim secret, only on the
/// token server's own word `no_such_checkout`. A bare `not_found` (a route missing in a rollback)
/// and a page of a proxy leave it where it is: it may be paid, and the secret is the only key to
/// the drive.
#[test]
fn az_p1_a_404_without_the_contracts_word_neither_ends_a_poll_nor_an_abandon() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    for body in [
        r#"{"error": "not_found", "message": "no route"}"#,
        "<html><body>404 Not Found</body></html>",
    ] {
        let fake = Fake::new(move |_, _| Ok(json(404, body)));
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        assert!(
            matches!(
                pending::poll(&server, &shared, &ck),
                Polled::Transient(Transient::Unread(_))
            ),
            "{body}: asked again, nothing ended"
        );
        assert!(
            matches!(
                pending::abandon(&server, &shared, "ck_1"),
                Abandoned::Kept { paid: false, .. }
            ),
            "{body}: the abandon did not end it"
        );
        let listed = pending::list(&shared).unwrap();
        assert_eq!(listed.len(), 1, "{body}: the claim secret stays");
        assert_eq!(listed[0].claim_secret, ck.claim_secret);
    }
    // The contract's word ends both.
    let fake = Fake::new(|_, _| {
        Ok(json(
            404,
            r#"{"error": "no_such_checkout", "message": "unknown checkout"}"#,
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Final(Final::Gone(_))
    ));
    assert!(pending::list(&shared).unwrap().is_empty());
    pending::add(&shared, &checkout("ck_2", &claim)).unwrap();
    assert_eq!(pending::abandon(&server, &shared, "ck_2"), Abandoned::Ended);
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// AZ-P6: a sealed sign-up that OPENS and does not read (a newer contract, a field renamed) is
/// never a strike: the text is intact for 30 days, an update reads it. Only a sign-up that does
/// not open at all is counted (the test of two strikes above).
#[test]
fn az_p6_a_sign_up_that_opens_and_does_not_read_is_kept_however_often_it_is_asked() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    let sealed = seal(&claim.public_base64(), "ck_1", b"{}");
    let fake = Fake::new(move |_, _| {
        Ok(json(
            200,
            &format!(
                r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{sealed}"}}"#
            ),
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    for round in 0..5 {
        match pending::poll(&server, &shared, &ck) {
            Polled::Transient(Transient::Unread(why)) => {
                assert!(why.contains("update"), "round {round}: {why}")
            }
            other => panic!("round {round}: not kept: {other:?}"),
        }
    }
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "the claim secret stays");
    assert_eq!(listed[0].open_failures, 0, "nothing was counted");
    assert_eq!(listed[0].claim_secret, ck.claim_secret);
}

/// X-DELTA-AZDRIVE-MONEY-7 (AZ-P6 class): a sealed sign-up whose `period_tokens` is THERE and does
/// not read - a `months` that is no whole number, a renamed field, a part missing - is not a
/// sign-up without period tokens: it is kept like one that opens and does not read (asked again,
/// never counted, "update the app"), with the claim secret that opens it. Read as "no grant" the
/// poll claimed the drive, the app saved it, and `claimed(.., None)` settled the checkout: the
/// claim secret and - with it - every month still owed were gone.
#[test]
fn x_delta_azdrive_money_7_a_sign_up_whose_period_tokens_do_not_read_is_kept() {
    for (what, grant) in [
        (
            "months a float",
            r#"{"checkout_id": "ck_1", "months": 3.5, "issue_key": "k"}"#,
        ),
        (
            "months negative",
            r#"{"checkout_id": "ck_1", "months": -1, "issue_key": "k"}"#,
        ),
        (
            "a renamed key",
            r#"{"checkout_id": "ck_1", "months": 3, "key_of_issue": "k"}"#,
        ),
        (
            "an id that is a number",
            r#"{"checkout_id": 1, "months": 3, "issue_key": "k"}"#,
        ),
        ("no object at all", r#""three months""#),
    ] {
        let dir = TempDir::new("azcloud-pending");
        let (shared, _) = shared(&dir);
        let claim = ClaimKey::generate().unwrap();
        let ck = checkout("ck_1", &claim);
        pending::add(&shared, &ck).unwrap();
        let text = bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").replace(
            r#""tier": "100GB"}"#,
            &format!(r#""tier": "100GB", "period_tokens": {grant}}}"#),
        );
        let sealed = seal(&claim.public_base64(), "ck_1", text.as_bytes());
        let fake = Fake::new(move |_, _| {
            Ok(json(
                200,
                &format!(
                    r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{sealed}"}}"#
                ),
            ))
        });
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        for round in 0..3 {
            match pending::poll(&server, &shared, &ck) {
                Polled::Transient(Transient::Unread(why)) => assert!(
                    why.contains("period tokens") && why.contains("update"),
                    "{what}, round {round}: {why}"
                ),
                other => panic!("{what}, round {round}: not kept: {other:?}"),
            }
        }
        let listed = pending::list(&shared).unwrap();
        assert_eq!(listed.len(), 1, "{what}: the checkout stays");
        assert_eq!(
            listed[0].claim_secret, ck.claim_secret,
            "{what}: with its claim secret"
        );
        assert_eq!(
            listed[0].open_failures, 0,
            "{what}: nothing was counted against it"
        );
    }
}

/// AZ-P6: a kept claim secret that does not read is not erased on that: the entry stays and says
/// so.
#[test]
fn az_p6_a_kept_claim_secret_that_does_not_read_is_not_erased() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut ck = checkout("ck_1", &claim);
    ck.claim_secret = String::from("not a claim key");
    pending::add(&shared, &ck).unwrap();
    let fake = Fake::new(|call, _| {
        panic!(
            "nothing is asked about a secret that does not read: {}",
            call.url
        )
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Transient(Transient::Unread(why)) => assert!(why.contains("claim key"), "{why}"),
        other => panic!("not kept: {other:?}"),
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
}

/// AZ-P5: the claim secret and the issue key leave the keyring only when the period grant is in
/// two places. A drive that has no vault for it - no key of it here yet - leaves the keyring's
/// entry as the second copy; the first month is issued and kept all the same. When the key
/// arrives the next try puts the grant in the drive's vault and lets go.
#[test]
fn az_p5_a_grant_that_only_this_computer_holds_keeps_the_claim_secret_and_the_issue_key() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let (ck, owed) = claimed_on_the_list(&shared, i64::try_from(NOW).unwrap());
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let local_only = store.on_this_device("d_1").unwrap();
    match pending::finish(&server, &shared, &store, &local_only, &ck, &owed, NOW) {
        Finished::Waits {
            drive_id,
            count,
            why,
        } => {
            assert_eq!(
                (drive_id.as_str(), count),
                ("d_1", 1),
                "the first month is issued here"
            );
            assert!(why.contains("no key"), "{why}");
        }
        other => panic!("not waiting: {other:?}"),
    }
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "the checkout stays");
    assert_eq!(
        listed[0].claim_secret, ck.claim_secret,
        "the claim secret stays"
    );
    assert_eq!(
        listed[0].period.as_ref().map(|p| p.issue_key.as_str()),
        Some(ISSUE_KEY),
        "the issue key stays"
    );
    let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
    assert!(
        text.contains(ISSUE_KEY) && text.contains(&ck.claim_secret),
        "{text}"
    );
    assert_eq!(
        local_only.state().unwrap().tokens.len(),
        1,
        "the month is kept here"
    );
    // The key arrives, the drive has its vault: the grant goes in, and only then the keyring
    // lets go.
    let (with_key, in_drive) = tokens_in_drive(&store);
    match pending::finish(&server, &shared, &store, &with_key, &ck, &owed, NOW) {
        Finished::Issued { .. } | Finished::Granted { .. } => {}
        other => panic!("not finished: {other:?}"),
    }
    assert!(in_drive.read().unwrap().grant("ck_1").is_some());
    assert!(pending::list(&shared).unwrap().is_empty());
    assert!(keyring.get(pending::PENDING_KEY).unwrap().is_none());
}

/// X-DELTA-AZDRIVE-MONEY-4: a first month refused as `bad_request` (a proxy emptied the body of
/// the POST) does not drop the checkout: the grant is in the drive's vault with its issue key,
/// not ended, with the kept request, and the checkout leaves the list as a grant kept
/// (`Granted`, saying to update the app). Before, `finish` answered `Dropped`, marked the grant
/// ended in every vault and erased the claim secret and the issue key.
#[test]
fn x_delta_azdrive_money_4_a_first_month_refused_as_unreadable_does_not_drop_the_checkout() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let now = i64::try_from(NOW).unwrap();
    let (ck, owed) = claimed_on_the_list(&shared, now);
    let listed = keys();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        Ok(json(
            400,
            r#"{"error": "bad_request", "message": "the body did not read"}"#,
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let (tokens, in_drive) = tokens_in_drive(&store);
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Granted { drive_id, why } => {
            assert_eq!(drive_id, "d_1");
            assert!(why.contains("update the app"), "{why}");
        }
        other => panic!("dropped on a refusal about the request: {other:?}"),
    }
    let state = in_drive.read().unwrap();
    let grant = state.grant("ck_1").expect("the grant, with the drive");
    assert!(grant.ended.is_none(), "{:?}", grant.ended);
    assert_eq!(
        grant.issue_key, ISSUE_KEY,
        "the issue key is with the drive"
    );
    assert_eq!(
        state.open_issues("ck_1").len(),
        1,
        "the request is kept for the next look"
    );
}

/// X-DELTA-AZDRIVE-MONEY-6: `state_of_all` tells the vaults' state only when every vault the drive
/// has was read - a decision that rests on something being ABSENT (this drive has no price lock)
/// needs that. A vault that does not answer, and the drive's own vault that this device could not
/// open, make it an error that names them, while `state` still answers what could be read. A
/// drive that has no vault of its own (a plain drive) has this computer's, which is the whole.
#[test]
fn x_delta_azdrive_money_6_the_state_of_all_vaults_is_told_only_when_every_vault_was_read() {
    let dir = TempDir::new("azcloud-pending");
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let plain = store.on_this_device("d_1").unwrap();
    plain
        .keep_grant(crate::vault::VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY))
        .unwrap();
    assert!(plain.state_of_all().unwrap().grant("ck_1").is_some());
    // The drive's own vault could not be opened: what is not in this computer's may be in it.
    let unopened = store
        .on_this_device_because(
            "d_1",
            NoVault::NotOpened(String::from("the drive is not open")),
        )
        .unwrap();
    assert!(
        unopened.state().unwrap().grant("ck_1").is_some(),
        "what can be read is read"
    );
    let why = unopened.state_of_all().unwrap_err().to_string();
    assert!(why.contains("the drive is not open"), "{why}");
    // The drive's vault does not answer.
    let down = DriveTokens::new(
        "d_1",
        store.local("d_1").unwrap(),
        Some(Arc::new(DownVault) as Arc<dyn TokenVault>),
    );
    assert!(down.state().is_ok());
    let why = down.state_of_all().unwrap_err().to_string();
    assert!(why.contains("no answer from the drive"), "{why}");
    // Both vaults read.
    let (whole, _) = tokens_in_drive(&store);
    assert!(whole.state_of_all().unwrap().grant("ck_1").is_some());
}

/// AZ-P5: a vault of the drive that does not answer is no place either.
#[test]
fn az_p5_a_drive_vault_that_does_not_answer_keeps_the_claim_secret_too() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let (ck, owed) = claimed_on_the_list(&shared, i64::try_from(NOW).unwrap());
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let tokens = DriveTokens::new(
        "d_1",
        store.local("d_1").unwrap(),
        Some(Arc::new(DownVault) as Arc<dyn TokenVault>),
    );
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Waits { why, .. } => assert!(why.contains("did not take"), "{why}"),
        other => panic!("not waiting: {other:?}"),
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
}

/// X-AR5b-7: a drive that could not be opened (not open in the app, a refused unlock) is a
/// reason, not a silent choice of "this computer only": the grant waits, and says why.
#[test]
fn x_ar5b_7_a_drive_that_could_not_be_opened_says_why_and_its_grant_waits() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let (ck, owed) = claimed_on_the_list(&shared, i64::try_from(NOW).unwrap());
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let tokens = store
        .on_this_device_because(
            "d_1",
            NoVault::NotOpened(String::from("the drive is not open")),
        )
        .unwrap();
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Waits { why, .. } => assert!(why.contains("the drive is not open"), "{why}"),
        other => panic!("not waiting: {other:?}"),
    }
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
}

/// AZ-P5 / Q20: after thirty days - the sealed sign-up is purged at the token server by then -
/// a drive that still has no vault gets a SECOND COPY of the grant on this computer, and only
/// then the keyring lets go; the app is told that the months are on this computer only. One
/// second short of it, the checkout still waits. The thirty days count from the claim, in the
/// time this computer saw pass.
#[test]
fn az_p5_after_thirty_days_without_a_vault_of_the_drive_a_second_copy_is_kept_and_the_checkout_goes(
) {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let now = i64::try_from(NOW).unwrap();
    // Claimed a second short of thirty days ago; the app counted the wait up to a minute ago.
    let (ck, owed) = claimed_on_the_list(&shared, now - pending::GRANT_WAIT_SECS + 1);
    restamp(
        &shared,
        "ck_1",
        now - pending::GRANT_WAIT_SECS + 1,
        now - 60,
    );
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let tokens = store.on_this_device("d_1").unwrap();
    assert!(
        matches!(
            pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW),
            Finished::Waits { .. }
        ),
        "one second short of the thirtieth day"
    );
    assert!(
        !store.dir().join("d_1.copy.json").exists(),
        "no second copy yet"
    );
    // The thirtieth day: claimed a second earlier.
    let older = restamp(&shared, "ck_1", now - pending::GRANT_WAIT_SECS, now - 60);
    match pending::finish(&server, &shared, &store, &tokens, &older, &owed, NOW) {
        Finished::LocalOnly { drive_id, why, .. } => {
            assert_eq!(drive_id, "d_1");
            assert!(why.contains("no key"), "{why}");
        }
        other => panic!("not kept locally: {other:?}"),
    }
    assert!(
        pending::list(&shared).unwrap().is_empty(),
        "the keyring let go"
    );
    let copy = std::fs::read_to_string(store.dir().join("d_1.copy.json")).expect("a second copy");
    assert!(copy.contains("ck_1") && copy.contains(ISSUE_KEY), "{copy}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(store.dir().join("d_1.copy.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "readable by this user only: {mode:o}");
    }
}

/// X-DELTA-AZDRIVE-MONEY-1 (AZ-P5): the thirty days for the drive's vault count from the CLAIM -
/// the payment as this computer saw it - not from the order. A cash order is made on day 0 and
/// its letter approved on day 45: its claim, the first `finish` after it, must answer `Waits`,
/// keep the claim secret and the issue key in the keyring and write no second copy; the old
/// code counted from `started_at`, found the order 45 days old and erased both at the first call
/// while the sealed sign-up was zero days old. The claim stamps its moment; a round every five
/// days then ends the wait on day thirty AFTER the claim, not before.
#[test]
fn x_delta_azdrive_money_1_a_cash_order_claimed_on_day_forty_five_waits_thirty_days_from_its_claim()
{
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    let now = i64::try_from(NOW).unwrap();
    let claim = ClaimKey::generate().unwrap();
    let mut ck = checkout("ck_1", &claim);
    ck.method = String::from("cash");
    ck.started_at = now - 45 * 86_400;
    pending::add(&shared, &ck).unwrap();
    // The claim stamps this computer's clock - the moment of the payment's claim, not the order's.
    let before = crate::now();
    pending::claimed(&shared, "ck_1", "d_1", Some(&grant(3))).unwrap();
    let after = crate::now();
    let stamped = pending::list(&shared).unwrap().remove(0);
    let at = stamped.period.as_ref().expect("claimed").claimed_at;
    assert!((before..=after).contains(&at), "the claim moment: {at}");
    // A claim made again keeps its first moment.
    pending::claimed(&shared, "ck_1", "d_1", Some(&grant(3))).unwrap();
    let again = pending::list(&shared).unwrap().remove(0);
    assert_eq!(again.period.as_ref().map(|p| p.claimed_at), Some(at));
    // The rounds below run on the tests' clock: the claim was made at NOW on it.
    let ck = restamp(&shared, "ck_1", now, 0);
    let owed = ck.period.clone().unwrap();
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let tokens = store.on_this_device("d_1").unwrap();
    let day = 86_400_u64;
    match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
        Finished::Waits { drive_id, .. } => assert_eq!(drive_id, "d_1"),
        other => panic!("an order 45 days old is not a payment 30 days old: {other:?}"),
    }
    assert!(
        !store.dir().join("d_1.copy.json").exists(),
        "no second copy"
    );
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 1, "the checkout stays");
    assert_eq!(
        listed[0].claim_secret, ck.claim_secret,
        "the claim secret stays"
    );
    let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
    assert!(text.contains(ISSUE_KEY), "the issue key stays: {text}");
    // A round every five days from the claim: the wait is over on the thirtieth day, no sooner.
    for round in 1..=5_u64 {
        let outcome = pending::finish(
            &server,
            &shared,
            &store,
            &tokens,
            &ck,
            &owed,
            NOW + round * 5 * day,
        );
        assert!(
            matches!(outcome, Finished::Waits { .. }),
            "day {}: {outcome:?}",
            round * 5
        );
    }
    match pending::finish(
        &server,
        &shared,
        &store,
        &tokens,
        &ck,
        &owed,
        NOW + 30 * day,
    ) {
        Finished::LocalOnly { .. } => {}
        other => panic!("day 30 after the claim: {other:?}"),
    }
    assert!(pending::list(&shared).unwrap().is_empty());
}

/// X-DELTA-AZDRIVE-MONEY-8 (AZ-P5): the local clock alone does not decide to erase the keyring's
/// copy. A clock set forward by a year (a typed date, a reset battery, a resumed machine) counts
/// a week of the wait, not a year: the claim secret and the issue key stay, round after round,
/// until thirty days have been SEEN pass. The old code compared `started_at` with the clock and
/// let go at the first round.
#[test]
fn x_delta_azdrive_money_8_a_clock_set_forward_does_not_end_the_wait() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let now = i64::try_from(NOW).unwrap();
    let (ck, owed) = claimed_on_the_list(&shared, now);
    restamp(&shared, "ck_1", now, now);
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let tokens = store.on_this_device("d_1").unwrap();
    let day = 86_400_u64;
    let year_on = NOW + 400 * day;
    for (what, at) in [
        ("the clock reads a year ahead", year_on),
        ("a minute later", year_on + 60),
        (
            "a week of seeing it go by, a week of the jump",
            year_on + 7 * day,
        ),
        ("two weeks", year_on + 14 * day),
        ("three weeks", year_on + 21 * day),
    ] {
        match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, at) {
            Finished::Waits { .. } => {}
            other => panic!("{what}: the wait is not over: {other:?}"),
        }
    }
    assert_eq!(
        pending::list(&shared).unwrap().len(),
        1,
        "the keyring still holds it"
    );
    assert!(!store.dir().join("d_1.copy.json").exists());
    // Thirty days seen pass since the jump's week: over.
    match pending::finish(
        &server,
        &shared,
        &store,
        &tokens,
        &ck,
        &owed,
        year_on + 28 * day,
    ) {
        Finished::LocalOnly { .. } => {}
        other => panic!("five weeks seen: {other:?}"),
    }
}

/// X-DELTA-AZDRIVE-MONEY-8: a clock that goes BACK counts nothing and costs nothing: the days
/// seen pass before the step stay counted, the days after it count as they come.
#[test]
fn x_delta_azdrive_money_8_a_clock_set_back_neither_shortens_nor_lengthens_the_wait() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let now = i64::try_from(NOW).unwrap();
    let day = 86_400_u64;
    // Twenty days seen pass already.
    let (ck, owed) = claimed_on_the_list(&shared, now - 20 * 86_400);
    restamp(&shared, "ck_1", now - 20 * 86_400, now);
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let tokens = store.on_this_device("d_1").unwrap();
    // The clock steps back a hundred days: still twenty days counted, so ten more days wait.
    let back = NOW - 100 * day;
    for (at, over) in [
        (back, false),
        (back + 5 * day, false),
        (back + 9 * day, false),
        (back + 10 * day, true),
    ] {
        let outcome = pending::finish(&server, &shared, &store, &tokens, &ck, &owed, at);
        assert_eq!(
            matches!(outcome, Finished::LocalOnly { .. }),
            over,
            "{} days after the step back: {outcome:?}",
            (at - back) / day
        );
        if !over {
            assert!(matches!(outcome, Finished::Waits { .. }), "{outcome:?}");
        }
    }
}

/// X-DELTA-AZDRIVE-MONEY-8 (AZ-P5): the second copy of the grant is READ. After the keyring let
/// go the copy is one of two files, and the vault's file damaged (a cut write, a disk error) or
/// gone must not take the grant with it: the vault reads from the copy, and the next change
/// writes the file whole again. A file and a copy that both do not read are an error, never an
/// empty vault. The old code read the first file only, and `state()` failed with the copy lying
/// beside it.
#[test]
fn x_delta_azdrive_money_8_the_second_copy_is_read_when_the_first_file_does_not() {
    let dir = TempDir::new("azcloud-pending");
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    let tokens = store.on_this_device("d_1").unwrap();
    tokens
        .keep_grant(crate::vault::VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY))
        .unwrap();
    tokens.keep_second_copy().unwrap();
    let first = store.path_of("d_1").unwrap();
    // A cut file.
    std::fs::write(&first, br#"{"tokens": [{"tier": "100GB", "ye"#).unwrap();
    let state = tokens.state().expect("the copy reads");
    assert!(
        state.grant("ck_1").is_some(),
        "the grant is not lost with the file"
    );
    // The next change writes the file whole again, with the grant and the change.
    tokens.spend("00ff").unwrap();
    let healed: VaultState = serde_json::from_slice(&std::fs::read(&first).unwrap()).unwrap();
    assert!(healed.grant("ck_1").is_some() && healed.spent == ["00ff"]);
    // A file that is gone.
    std::fs::remove_file(&first).unwrap();
    assert!(tokens.state().unwrap().grant("ck_1").is_some());
    // Both damaged: an error, not an empty vault.
    std::fs::write(&first, b"{ cut").unwrap();
    std::fs::write(store.dir().join("d_1.copy.json"), b"{ cut").unwrap();
    assert!(tokens.state().is_err());
}

/// AZ-P10: a token server that asks to wait (429 with `Retry-After`) is heard: the poll says how
/// long, so the loops that poll do not ask again after their fixed three or ten seconds.
#[test]
fn az_p10_a_rate_limited_poll_carries_the_wait_the_token_server_asked_for() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, _) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    let fake = Fake::new(|_, _| {
        let mut reply = json(
            429,
            r#"{"error": "rate_limited", "message": "too many requests", "spent": false}"#,
        );
        reply
            .headers
            .push((String::from("retry-after"), String::from("60")));
        Ok(reply)
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(std::time::Duration::ZERO);
    match pending::poll(&server, &shared, &ck) {
        Polled::Transient(busy @ Transient::Busy { .. }) => {
            assert_eq!(busy.retry_after(), Some(60), "{busy:?}");
        }
        other => panic!("not busy: {other:?}"),
    }
    // Any other reason to stay says no wait of the server's.
    assert_eq!(Transient::NoAnswer(String::from("x")).retry_after(), None);
    assert_eq!(
        Transient::Busy {
            why: String::from("x"),
            retry_after: None
        }
        .retry_after(),
        None
    );
    assert_eq!(
        pending::list(&shared).unwrap().len(),
        1,
        "the checkout stays"
    );
}
