//! A grant whose answer was lost does not cost the device its drive: the lockdown and a
//! pick-up's claim carry one idempotency key across every retry (in this process and after a
//! restart), so the token server answers the repeat with the grant it made the first time
//! instead of refusing the token the lockdown revoked or counting another pick-up.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, HttpReply, Transport,
};
use serde_json::{json, Value};

use super::{bundle, header, json, Fake, Shared, TOKEN};
use crate::{
    claim::seal,
    drive::{AzlinDrive, TransportFactory},
    pending::{self, Final, Polled},
    shared::SharedKeyring,
    AzlinSession, ClaimCode, ClaimKey, DriveBundle, TokenServer,
};

const IDEMPOTENCY: &str = "x-azlin-idempotency";
/// The drive token this device holds before the lockdown.
const HELD: &str = "dt_f.0.aaa";

/// An S3 answer any credentials of the server's bundles get.
fn bucket_answer() -> HttpReply {
    HttpReply {
        status: 200,
        headers: vec![
            (String::from("content-length"), String::from("5")),
            (String::from("etag"), String::from("\"e1\"")),
            (
                String::from("last-modified"),
                String::from("Thu, 08 Oct 2026 21:00:00 GMT"),
            ),
        ],
        body: Vec::new(),
    }
}

/// What the lockdown server saw: the idempotency key of every lockdown, in order.
#[derive(Default)]
struct Seen {
    lockdown_keys: Vec<Option<String>>,
    grants: u32,
}

/// A token server whose lockdown works as azlin-token's: the first lockdown revokes the asking
/// family `f` and grants family `g`; asked again with the first one's key by the revoked token,
/// it answers that grant rotated once more; without that key the revoked token is refused.
/// The next `lose` lockdown answers are lost after the lockdown was done. `g`'s tokens refresh.
fn lockdown_server(lose: Arc<AtomicU32>, seen: Arc<Mutex<Seen>>) -> Arc<Fake> {
    struct State {
        first_key: Option<Option<String>>,
        generation: u32,
        current: String,
    }
    let state = Mutex::new(State {
        first_key: None,
        generation: 0,
        current: String::new(),
    });
    Fake::new(move |call, _| {
        if !call.url.starts_with(TOKEN) {
            return Ok(bucket_answer());
        }
        let presented = header(call, "authorization")
            .unwrap_or_default()
            .trim_start_matches("Bearer ")
            .to_string();
        let mut s = state.lock().unwrap();
        if call.url.ends_with("/lockdown") {
            let key = header(call, IDEMPOTENCY).map(String::from);
            seen.lock().unwrap().lockdown_keys.push(key.clone());
            if presented != HELD {
                return Ok(json(401, r#"{"error": "unauthorized"}"#));
            }
            match &s.first_key {
                None => s.first_key = Some(key),
                Some(first) if first.is_some() && *first == key => s.generation += 1,
                Some(_) => return Ok(json(401, r#"{"error": "credentials_revoked"}"#)),
            }
            seen.lock().unwrap().grants += 1;
            s.current = format!("dt_g.{}.g{}", s.generation, s.generation);
            let lost = lose.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
            if lost.is_ok() {
                return Err(String::from("timed out"));
            }
            return Ok(json(200, &bundle("AKIDG", "2026-10-20T09:00:00Z", &s.current)));
        }
        if call.url.ends_with("/credentials") {
            if s.first_key.is_some() && presented == HELD {
                return Ok(json(401, r#"{"error": "credentials_revoked"}"#));
            }
            if presented != s.current && s.first_key.is_some() {
                return Ok(json(401, r#"{"error": "token_reuse"}"#));
            }
            if s.first_key.is_none() {
                return Ok(json(200, &bundle("AKIDF", "2026-10-20T09:00:00Z", "dt_f.1.bbb")));
            }
            s.generation += 1;
            s.current = format!("dt_g.{}.g{}", s.generation, s.generation);
            return Ok(json(200, &bundle("AKIDG2", "2026-10-20T09:00:00Z", &s.current)));
        }
        Ok(json(404, r#"{"error": "not_found"}"#))
    })
}

/// The keyring of this device: drive `d_1` at [`HELD`], its credentials running out within the
/// hour.
fn device() -> Arc<MemoryKeyring> {
    let keyring = Arc::new(MemoryKeyring::new());
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T09:30:00Z", HELD)).unwrap();
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    keyring
}

/// AzDrive's drive `d_1` over `keyring` (an app started anew makes another one).
fn open(dir: &TempDir, keyring: &Arc<MemoryKeyring>, fake: &Arc<Fake>) -> AzlinDrive {
    let stored = AzlinSession::from_keyring_secret(
        &keyring.get(&keyring_key("d_1")).unwrap().unwrap(),
    )
    .unwrap();
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T09:30:00Z", HELD)).unwrap();
    let shared = SharedKeyring::with_lock_root(
        keyring.clone(),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    let answering = fake.clone();
    let factory: TransportFactory =
        Arc::new(move || Box::new(Shared(answering.clone())) as Box<dyn Transport>);
    AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        stored,
        TOKEN,
        shared,
        factory,
        Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
    )
    .unwrap()
    .with_clock(|| 1_791_450_000)
}

fn kept_token(keyring: &MemoryKeyring) -> String {
    let text = keyring.get(&keyring_key("d_1")).unwrap().unwrap();
    AzlinSession::from_keyring_secret(&text)
        .unwrap()
        .drive_token
        .as_str()
        .to_string()
}

/// Every lockdown carried a key, and the same one.
fn one_key(seen: &Mutex<Seen>) {
    let keys = seen.lock().unwrap().lockdown_keys.clone();
    assert!(!keys.is_empty());
    assert!(keys.iter().all(Option::is_some), "a key on every try: {keys:?}");
    assert!(keys.windows(2).all(|w| w[0] == w[1]), "the same key: {keys:?}");
}

#[test]
fn a_lockdown_whose_answer_was_lost_ends_with_this_devices_grant() {
    let dir = TempDir::new("azcloud-grant-lockdown");
    let keyring = device();
    // the first answer is lost, the next ones come
    let lose = Arc::new(AtomicU32::new(1));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let fake = lockdown_server(lose, seen.clone());
    let drive = open(&dir, &keyring, &fake);
    if drive.lockdown().is_err() {
        drive.lockdown().expect("the lockdown asked again ends with this device's grant");
    }
    assert!(kept_token(&keyring).starts_with("dt_g."), "{}", kept_token(&keyring));
    one_key(&seen);
}

#[test]
fn a_lockdown_whose_answers_were_lost_until_the_app_quit_is_finished_after_its_restart() {
    let dir = TempDir::new("azcloud-grant-restart");
    let keyring = device();
    let lose = Arc::new(AtomicU32::new(u32::MAX));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let fake = lockdown_server(lose.clone(), seen.clone());
    {
        let drive = open(&dir, &keyring, &fake);
        assert!(drive.lockdown().is_err(), "no answer came");
    }
    lose.store(0, Ordering::SeqCst);
    let drive = open(&dir, &keyring, &fake);
    drive.lockdown().expect("the started app asks again with the same key");
    assert!(kept_token(&keyring).starts_with("dt_g."), "{}", kept_token(&keyring));
    one_key(&seen);
}

#[test]
fn the_next_refresh_after_a_lost_lockdown_finishes_the_lockdown_instead_of_signing_out() {
    let dir = TempDir::new("azcloud-grant-refresh");
    let keyring = device();
    let lose = Arc::new(AtomicU32::new(u32::MAX));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let fake = lockdown_server(lose.clone(), seen.clone());
    let drive = open(&dir, &keyring, &fake);
    assert!(drive.lockdown().is_err(), "no answer came");
    lose.store(0, Ordering::SeqCst);
    // the credentials run out within the hour: the next request refreshes first
    drive.head("a.txt").expect("the drive goes on with the lockdown's grant");
    assert!(kept_token(&keyring).starts_with("dt_g."), "{}", kept_token(&keyring));
    one_key(&seen);
}

// ---- a pick-up's claim ----

const CHECKOUT: &str = "ck_aaaqeayeaudaocajbifqydiob4";

#[test]
fn a_pick_up_whose_claim_answer_was_lost_twice_is_one_device() {
    let dir = TempDir::new("azcloud-grant-claim");
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::with_lock_root(
        keyring.clone(),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    let claim = ClaimKey::generate().unwrap();
    let kept = ClaimCode::new(CHECKOUT, &claim).pending(TOKEN, "Photos");
    pending::add(&shared, &kept).unwrap();
    let mut signup: Value =
        serde_json::from_str(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    signup["claim"] = json!({"ticket": "tk-1", "max": 3, "window_days": 30});
    let sealed = seal(&claim.public_base64(), CHECKOUT, signup.to_string().as_bytes());
    // the server: a pick-up per new key (a family each), the same family rotated for a known
    // one; the first two answers lost after the pick-up was made
    struct Picks {
        by_key: BTreeMap<String, (String, u32)>,
        families: u32,
        lost: u32,
        keys: Vec<Option<String>>,
    }
    let picks = Arc::new(Mutex::new(Picks {
        by_key: BTreeMap::new(),
        families: 0,
        lost: 2,
        keys: Vec::new(),
    }));
    let state = picks.clone();
    let fake = Fake::new(move |call, _| {
        if !call.url.ends_with("/claim") {
            return Ok(json(
                200,
                &format!(
                    r#"{{"checkout_id": "{CHECKOUT}", "status": "approved",
                         "sealed_signup": "{sealed}"}}"#
                ),
            ));
        }
        let mut p = state.lock().unwrap();
        let key = header(call, IDEMPOTENCY).map(String::from);
        p.keys.push(key.clone());
        let known = key.as_ref().and_then(|k| p.by_key.get(k).cloned());
        let (family, generation) = match known {
            Some((family, generation)) => (family, generation + 1),
            None => {
                p.families += 1;
                (format!("p{}", p.families), 0)
            }
        };
        if let Some(k) = key {
            p.by_key.insert(k, (family.clone(), generation));
        }
        if p.lost > 0 {
            p.lost -= 1;
            return Err(String::from("timed out"));
        }
        let left = 3 - p.families;
        Ok(json(
            201,
            &format!(
                r#"{{"member": "owner", "drive_token": "dt_{family}.{generation}.x",
                     "claims_left": {left}}}"#
            ),
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let mut claimed = None;
    for _ in 0..3 {
        if let Polled::Final(Final::Claimed(done)) = pending::poll(&server, &shared, &kept) {
            claimed = Some(done);
            break;
        }
    }
    let claimed = claimed.expect("picked up within three polls");
    let p = picks.lock().unwrap();
    assert_eq!(p.families, 1, "one pick-up however often it was asked: {:?}", p.keys);
    assert!(p.keys.iter().all(Option::is_some), "a key on every try: {:?}", p.keys);
    assert!(p.keys.windows(2).all(|w| w[0] == w[1]), "the same key: {:?}", p.keys);
    assert!(claimed.bundle.drive_token.starts_with("dt_p1."), "{}", claimed.bundle.drive_token);
}
