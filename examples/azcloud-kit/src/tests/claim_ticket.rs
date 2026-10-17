//! A paid drive picked up on another computer (the claim code; SRV17's claim tickets): the
//! sealed sign-up carries `"claim": {"ticket", "max", "window_days"}`, and the computer that
//! picks the drive up claims a token family of its own (`POST /v1/drives/{id}/claim
//! {"ticket"}`) - so it and the buyer's computer are two devices, never one family whose second
//! refresh is a reuse. A drive from before the tickets (401) keeps the sealed drive token.

use std::sync::{Arc, Mutex};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
};
use serde_json::{json, Value};

use super::{bundle, header, json, Fake, Shared, TOKEN};
use crate::{
    claim::seal,
    lock::LockDir,
    pending::{self, PendingCheckout, Polled},
    shared::SharedKeyring,
    AzlinSession, ClaimCode, ClaimKey, DriveBundle, TokenServer,
};

const CHECKOUT: &str = "ck_aaaqeayeaudaocajbifqydiob4";

/// The test bundle (drive `d_1`, its first token `dt_f.0.aaa`) with the claim ticket `ticket`.
fn ticketed(ticket: Option<&str>) -> String {
    let mut value: Value =
        serde_json::from_str(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    if let Some(ticket) = ticket {
        value["claim"] = json!({"ticket": ticket, "max": 3, "window_days": 30});
    }
    value.to_string()
}

fn shared(dir: &TempDir) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::new(keyring.clone(), LockDir::new(dir.path().join("locks")));
    (shared, keyring)
}

/// The token server: the checkout approved, its sign-up (`signup`) sealed to `claim`; a claim
/// answered `claimed` (status, body); every claim's body recorded in `claims`.
fn server(
    claim: &ClaimKey,
    signup: String,
    claimed: (u16, &'static str),
    claims: Arc<Mutex<Vec<(Value, bool)>>>,
) -> Arc<Fake> {
    let sealed = seal(&claim.public_base64(), CHECKOUT, signup.as_bytes());
    Fake::new(move |call, _| {
        if call.url.ends_with("/claim") {
            let body: Value = serde_json::from_slice(&call.body).unwrap_or(Value::Null);
            claims
                .lock()
                .unwrap()
                .push((body, header(call, "authorization").is_some()));
            return Ok(json(claimed.0, claimed.1));
        }
        Ok(json(
            200,
            &format!(
                r#"{{"checkout_id": "{CHECKOUT}", "status": "approved",
                     "sealed_signup": "{sealed}"}}"#
            ),
        ))
    })
}

fn token_kept(keyring: &MemoryKeyring) -> String {
    let text = keyring.get(&keyring_key("d_1")).unwrap().unwrap();
    AzlinSession::from_keyring_secret(&text).unwrap().drive_token
}

#[test]
fn the_sealed_sign_up_carries_a_claim_ticket_that_debug_never_shows() {
    let bundle = DriveBundle::parse(&ticketed(Some("tk-secret-ticket"))).unwrap();
    let claim = bundle.claim.clone().expect("the ticket");
    assert_eq!(claim.ticket, "tk-secret-ticket");
    assert_eq!((claim.max, claim.window_days), (3, 30));
    let shown = format!("{bundle:?}");
    assert!(!shown.contains("tk-secret-ticket"), "{shown}");
    assert_eq!(DriveBundle::parse(&ticketed(None)).unwrap().claim, None);
}

#[test]
fn a_drive_picked_up_by_its_claim_code_claims_a_token_family_of_its_own_once() {
    let dir = TempDir::new("azcloud-claim-ticket");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let kept = ClaimCode::new(CHECKOUT, &claim).pending(TOKEN, "Photos");
    assert!(kept.picked_up, "a claim code's checkout is a pick-up");
    pending::add(&shared, &kept).unwrap();
    let claims = Arc::new(Mutex::new(Vec::new()));
    let fake = server(
        &claim,
        ticketed(Some("tk-1")),
        (
            201,
            r#"{"member": "owner", "drive_token": "dt_g.0.own", "claims_left": 2}"#,
        ),
        claims.clone(),
    );
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &kept) {
        Polled::Claimed(claimed) => {
            assert!(!claimed.already);
            assert_eq!(claimed.bundle.drive_token, "dt_g.0.own", "its own family's token");
        }
        other => panic!("not claimed: {other:?}"),
    }
    assert_eq!(token_kept(&keyring), "dt_g.0.own");
    {
        let claims = claims.lock().unwrap();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].0["ticket"], "tk-1");
        assert!(!claims[0].1, "the ticket alone: no drive token goes with it");
    }
    // The next poll (another window, the next start) claims nothing more: the keyring has it.
    match pending::poll(&server, &shared, &kept) {
        Polled::Claimed(claimed) => assert!(claimed.already),
        other => panic!("not claimed: {other:?}"),
    }
    assert_eq!(claims.lock().unwrap().len(), 1, "one family per computer");
    assert_eq!(token_kept(&keyring), "dt_g.0.own");
}

#[test]
fn a_drive_from_before_the_tickets_keeps_the_sealed_drive_token() {
    let dir = TempDir::new("azcloud-claim-ticket");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let kept = ClaimCode::new(CHECKOUT, &claim).pending(TOKEN, "Photos");
    pending::add(&shared, &kept).unwrap();
    let claims = Arc::new(Mutex::new(Vec::new()));
    let fake = server(
        &claim,
        ticketed(Some("tk-old")),
        (401, r#"{"error": "unauthorized", "message": "unknown claim ticket"}"#),
        claims.clone(),
    );
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &kept),
        Polled::Claimed(_)
    ));
    assert_eq!(token_kept(&keyring), "dt_f.0.aaa");
}

#[test]
fn a_drive_picked_up_as_often_as_it_may_be_is_not_picked_up_again() {
    for (status, body) in [
        (
            409,
            r#"{"error": "claims_used", "message": "this drive was picked up 3 times already"}"#,
        ),
        (
            410,
            r#"{"error": "claim_expired", "message": "the pick-ups of this drive ended"}"#,
        ),
    ] {
        let dir = TempDir::new("azcloud-claim-ticket");
        let (shared, keyring) = shared(&dir);
        let claim = ClaimKey::generate().unwrap();
        let kept = ClaimCode::new(CHECKOUT, &claim).pending(TOKEN, "Photos");
        pending::add(&shared, &kept).unwrap();
        let fake = server(
            &claim,
            ticketed(Some("tk-1")),
            (status, body),
            Arc::new(Mutex::new(Vec::new())),
        );
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        match pending::poll(&server, &shared, &kept) {
            Polled::Dropped(why) => assert!(why.contains("picked up"), "{why}"),
            other => panic!("HTTP {status}: {other:?}"),
        }
        assert!(pending::list(&shared).unwrap().is_empty(), "off the list");
        assert!(keyring.get(&keyring_key("d_1")).unwrap().is_none(), "no session kept");
    }
}

#[test]
fn the_buyers_own_checkout_claims_no_family_it_has_the_first() {
    let dir = TempDir::new("azcloud-claim-ticket");
    let (shared, keyring) = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let kept = PendingCheckout::new(CHECKOUT, &claim, "100GB", TOKEN, "Photos");
    assert!(!kept.picked_up);
    pending::add(&shared, &kept).unwrap();
    let claims = Arc::new(Mutex::new(Vec::new()));
    let fake = server(
        &claim,
        ticketed(Some("tk-1")),
        (201, r#"{"member": "owner", "drive_token": "dt_g.0.own"}"#),
        claims.clone(),
    );
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        pending::poll(&server, &shared, &kept),
        Polled::Claimed(_)
    ));
    assert!(claims.lock().unwrap().is_empty());
    assert_eq!(token_kept(&keyring), "dt_f.0.aaa", "the sealed sign-up's first token");
}
