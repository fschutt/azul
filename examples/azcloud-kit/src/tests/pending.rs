//! The unfinished checkouts: one keyring entry changed under its lock (two windows lose none of
//! each other's), polled into the drive (its session kept once, by drive id: a rotated one is
//! never replaced by the sign-up's spent token) or dropped (said once).

use std::sync::Arc;

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
};

use super::{bundle, json, Fake, Shared, TOKEN};
use crate::{
    claim::seal,
    lock::LockDir,
    pending::{self, PendingCheckout, Polled},
    shared::SharedKeyring,
    AzlinSession, ClaimKey, TokenServer,
};

/// The keyring and the locks of one user, and the keyring itself to look into.
fn shared(dir: &TempDir) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::new(keyring.clone(), LockDir::new(dir.path().join("locks")));
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
    AzlinSession::from_keyring_secret(text).unwrap().drive_token
}

#[test]
fn the_unfinished_checkouts_are_one_keyring_entry_two_windows_change_without_losing_any() {
    let dir = TempDir::new("azcloud-pending");
    let (shared, keyring) = shared(&dir);
    assert!(pending::list(&shared).unwrap().is_empty());
    let windows: Vec<_> = (0..2)
        .map(|window| {
            // Each window its own keyring handle and locks over the same entry and folder.
            let shared = SharedKeyring::new(
                keyring.clone() as Arc<dyn KeyringStore>,
                LockDir::new(dir.path().join("locks")),
            );
            std::thread::spawn(move || {
                for i in 0..10 {
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
    assert_eq!(pending::list(&shared).unwrap().len(), 20, "none lost");
    let text = keyring.get(pending::PENDING_KEY).unwrap().unwrap();
    assert!(text.contains("ck_0_9") && text.contains("ck_1_0"));
    assert!(pending::remove(&shared, "ck_0_3").unwrap());
    assert!(!pending::remove(&shared, "ck_0_3").unwrap(), "removed once");
    assert_eq!(pending::list(&shared).unwrap().len(), 19);
    // One entry per checkout: adding one again replaces it.
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &checkout("ck_1_1", &claim)).unwrap();
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed.len(), 19);
    let again = listed.iter().find(|c| c.checkout_id == "ck_1_1").unwrap();
    assert_eq!(again.claim_secret, claim.to_base64());
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
        Polled::Claimed(claimed) => claimed,
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
    rotated.drive_token = String::from("dt_f.1.bbb");
    keyring
        .set(&keyring_key("d_1"), &rotated.to_keyring_secret())
        .unwrap();
    let again = match pending::poll(&server, &shared, &ck) {
        Polled::Claimed(claimed) => claimed,
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
        Polled::Dropped(why) => assert!(why.contains("insufficient funds"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    match pending::poll(&server, &shared, &gone) {
        Polled::Dropped(why) => assert!(why.contains("unknown checkout"), "{why}"),
        other => panic!("not dropped: {other:?}"),
    }
    assert!(matches!(
        pending::poll(&server, &shared, &expired),
        Polled::Dropped(_)
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
    let other = ClaimKey::generate().unwrap();
    let ck = checkout("ck_1", &claim);
    pending::add(&shared, &ck).unwrap();
    let sealed_to_another = seal(
        &other.public_base64(),
        "ck_1",
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    let fake = Fake::new(move |_, n| match n {
        0 => Err(String::from("connection refused")),
        1 => Ok(json(503, r#"{"error": "busy", "message": "try later"}"#)),
        2 => Ok(json(200, r#"{"checkout_id": "ck_1", "status": "pending"}"#)),
        _ => Ok(json(
            200,
            &format!(
                r#"{{"checkout_id": "ck_1", "status": "approved",
                     "sealed_signup": "{sealed_to_another}"}}"#
            ),
        )),
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Kept(_)
    ));
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Kept(_)
    ));
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Pending
    ));
    // A sign-up that does not open with the checkout's claim key: kept - the checkout id and
    // its key are what support can help with.
    assert!(matches!(
        pending::poll(&server, &shared, &ck),
        Polled::Kept(_)
    ));
    assert_eq!(pending::list(&shared).unwrap().len(), 1);
}
