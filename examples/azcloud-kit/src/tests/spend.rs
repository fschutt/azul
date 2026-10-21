//! The drive token spent once: only from the keyring under the drive's lock, by value; an
//! outcome nobody knows is answered by sending the same token again, never another one; a 503
//! that does not say it spent nothing is not sent again; the lock is the keyring's, whichever
//! app asks.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringError, KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, HttpReply, Method, Transport,
};

use super::{bundle, header, json, keyring_holding, Fake, Shared, TOKEN};
use crate::{
    drive::{AzlinDrive, TransportFactory},
    shared::{lock_folder_for, RefreshOutcome, SharedKeyring},
    AzlinSession, DriveBundle, TokenError, TokenServer,
};

fn server(fake: &Arc<Fake>) -> (Shared, Duration) {
    (Shared(fake.clone()), Duration::ZERO)
}

fn refreshes(fake: &Fake) -> Vec<String> {
    fake.calls()
        .iter()
        .filter(|c| c.method == Method::Post && c.url.ends_with("/credentials"))
        .map(|c| header(c, "authorization").unwrap_or_default().to_string())
        .collect()
}

#[test]
fn a_refresh_without_an_answer_is_unknown_and_goes_on_only_with_the_same_token() {
    let dir = TempDir::new("azcloud-spend-unknown");
    let shared = keyring_holding(&dir, "dt_f.0.aaa");
    let fake = Fake::new(|_, n| {
        if n == 0 {
            return Err(String::from("timed out"));
        }
        Ok(json(200, &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb")))
    });
    let (transport, pause) = server(&fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap().with_retry_pause(pause);
    let pending = match server.refresh(shared.spend("d_1").unwrap(), None) {
        RefreshOutcome::Unknown(pending) => pending,
        other => panic!("not unknown: {other:?}"),
    };
    assert!(matches!(pending.error(), TokenError::Connect(_)));
    let rotation = server.retry(pending, None).rotated().expect("answered");
    assert_eq!(rotation.bundle().drive_token, "dt_f.1.bbb");
    assert_eq!(
        refreshes(&fake),
        vec!["Bearer dt_f.0.aaa", "Bearer dt_f.0.aaa"],
        "the same token both times"
    );
    let kept = rotation.keep();
    assert!(kept.saved.is_ok());
    assert_eq!(kept.session.drive_token, "dt_f.1.bbb");
    let next = shared.spend("d_1").unwrap();
    assert_eq!(next.session().unwrap().drive_token, "dt_f.1.bbb");
}

#[test]
fn a_503_that_does_not_say_it_spent_nothing_is_not_sent_again() {
    for body in [
        r#"{"error": "unavailable", "message": "database", "spent": true}"#,
        r#"{"error": "unavailable", "message": "database"}"#,
        "<html>bad gateway</html>",
    ] {
        let dir = TempDir::new("azcloud-spend-503");
        let shared = keyring_holding(&dir, "dt_f.0.aaa");
        let fake = Fake::new(move |_, _| Ok(json(503, body)));
        let (transport, pause) = server(&fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap().with_retry_pause(pause);
        let outcome = server.refresh(shared.spend("d_1").unwrap(), None);
        assert!(matches!(outcome, RefreshOutcome::Unknown(_)), "{body}: {outcome:?}");
        assert_eq!(refreshes(&fake).len(), 1, "{body}: sent once");
    }
    // one that says it spent nothing goes again, with the same token
    let dir = TempDir::new("azcloud-spend-503-again");
    let shared = keyring_holding(&dir, "dt_f.0.aaa");
    let fake = Fake::new(|_, n| {
        Ok(if n == 0 {
            json(503, r#"{"error": "unavailable", "spent": false}"#)
        } else {
            json(200, &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb"))
        })
    });
    let (transport, pause) = server(&fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap().with_retry_pause(pause);
    assert!(server.refresh(shared.spend("d_1").unwrap(), None).rotated().is_ok());
    assert_eq!(refreshes(&fake), vec!["Bearer dt_f.0.aaa", "Bearer dt_f.0.aaa"]);
}

/// A token server that rotates as azlin-token does: the current token, or the one rotated from
/// last while the current one was never presented (a lost answer); anything older is a reuse.
/// The first rotation's answer is lost when `lose_first`.
fn rotating_server(lose_first: bool) -> Arc<Fake> {
    struct Family {
        generation: u32,
        current: String,
        previous: Option<String>,
        used: bool,
        lost: bool,
    }
    let state = Mutex::new(Family {
        generation: 0,
        current: String::from("dt_f.0.aaa"),
        previous: None,
        used: false,
        lost: !lose_first,
    });
    Fake::new(move |call, _| {
        if !call.url.starts_with(TOKEN) {
            // the bucket: any credentials of the server's answers do
            return Ok(HttpReply {
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
            });
        }
        let mut f = state.lock().unwrap();
        let presented = header(call, "authorization")
            .unwrap_or_default()
            .trim_start_matches("Bearer ")
            .to_string();
        let lost_answer = !f.used && f.previous.as_deref() == Some(presented.as_str());
        if presented != f.current && !lost_answer {
            return Ok(json(401, r#"{"error": "token_reuse"}"#));
        }
        f.generation += 1;
        let next = format!("dt_f.{}.t{}", f.generation, f.generation);
        if !lost_answer {
            f.previous = Some(f.current.clone());
        }
        f.current = next.clone();
        f.used = false;
        let key = format!("AKID{}", f.generation + 1);
        if !f.lost {
            f.lost = true;
            return Err(String::from("timed out"));
        }
        Ok(json(200, &bundle(&key, "2026-10-09T21:15:00Z", &next)))
    })
}

#[test]
fn a_rotation_whose_answer_never_arrived_does_not_cost_the_device_its_drive() {
    let dir = TempDir::new("azcloud-spend-lost");
    let fake = rotating_server(true);
    let keyring = Arc::new(MemoryKeyring::new());
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T09:30:00Z", "dt_f.0.aaa")).unwrap();
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let shared =
        SharedKeyring::with_lock_root(keyring.clone(), KeyringNamespace::azlin_shared(), dir.path());
    let answering = fake.clone();
    let factory: TransportFactory =
        Arc::new(move || Box::new(Shared(answering.clone())) as Box<dyn Transport>);
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        shared,
        factory,
        Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
    )
    .unwrap()
    // the credentials run out within the hour: every call refreshes first
    .with_clock(|| 1_791_450_000);
    // the token server rotates, its answer is lost: this call fails
    assert!(drive.head("a.txt").is_err());
    // the next one sends the same token and goes on
    drive.head("a.txt").expect("the next request works");
    let stored = AzlinSession::from_keyring_secret(
        &keyring.get(&keyring_key("d_1")).unwrap().unwrap(),
    )
    .unwrap();
    assert_eq!(stored.drive_token, "dt_f.2.t2");
    assert_eq!(
        refreshes(&fake),
        vec!["Bearer dt_f.0.aaa", "Bearer dt_f.0.aaa"],
        "never a reuse"
    );
}

/// A keyring that reads but takes no write.
struct ReadOnlyKeyring(MemoryKeyring);

impl KeyringStore for ReadOnlyKeyring {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.0.get(key)
    }
    fn set(&self, _: &str, _: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Failed(String::from("locked")))
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.0.delete(key)
    }
}

#[test]
fn a_rotated_session_the_keyring_did_not_take_is_what_the_next_refresh_spends() {
    let dir = TempDir::new("azcloud-spend-unsaved");
    let inner = MemoryKeyring::new();
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-09T21:15:00Z", "dt_f.0.aaa")).unwrap();
    inner
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let shared = SharedKeyring::with_lock_root(
        Arc::new(ReadOnlyKeyring(inner)),
        KeyringNamespace::azlin_shared(),
        dir.path(),
    );
    let fake = rotating_server(false);
    let (transport, pause) = server(&fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap().with_retry_pause(pause);
    let kept = server
        .refresh(shared.spend("d_1").unwrap(), None)
        .rotated()
        .unwrap()
        .keep();
    assert!(kept.saved.is_err(), "the keyring refused it");
    // the keyring still has the spent token; the next refresh spends the rotated one
    let again = server.refresh(shared.spend("d_1").unwrap(), None).rotated();
    assert!(again.is_ok(), "{again:?}");
    assert_eq!(
        refreshes(&fake),
        vec!["Bearer dt_f.0.aaa", "Bearer dt_f.1.t1"],
        "never the spent one again"
    );
}

#[test]
fn two_keyrings_of_one_namespace_spend_a_token_one_at_a_time() {
    let dir = TempDir::new("azcloud-spend-two");
    let memory = Arc::new(MemoryKeyring::new());
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-09T21:15:00Z", "dt_f.0.aaa")).unwrap();
    memory
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let ns = KeyringNamespace::azlin_shared();
    // two apps, each with its own keyring value over the same entries
    let a = SharedKeyring::with_lock_root(memory.clone(), ns.clone(), dir.path());
    let b = SharedKeyring::with_lock_root(memory.clone(), ns, dir.path());
    let fake = rotating_server(false);
    let held = a.spend("d_1").unwrap();
    let spent = std::thread::scope(|scope| {
        let waiter = scope.spawn(|| {
            let spend = b.spend("d_1").unwrap();
            spend.session().unwrap().drive_token.as_str().to_string()
        });
        std::thread::sleep(Duration::from_millis(200));
        let transport = Shared(fake.clone());
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        let kept = server.refresh(held, None).rotated().unwrap().keep();
        assert!(kept.saved.is_ok());
        waiter.join().unwrap()
    });
    assert_eq!(spent, "dt_f.1.t1", "the second spends what the first answered");
    assert_eq!(refreshes(&fake), vec!["Bearer dt_f.0.aaa"]);
}

#[test]
fn the_locks_follow_the_keyring_and_its_namespace_not_the_app() {
    let shared = KeyringNamespace::azlin_shared();
    let config = Some(std::path::PathBuf::from("/home/ann/.config"));
    let folder = lock_folder_for(None, config.clone(), &shared);
    assert_eq!(
        folder,
        std::path::PathBuf::from("/home/ann/.config/azlin/locks/io.azlin.shared")
    );
    let other = lock_folder_for(None, config.clone(), &KeyringNamespace::named("org.example.Editor"));
    assert_ne!(other, folder, "another namespace, other entries, other locks");
    let own = lock_folder_for(None, config, &KeyringNamespace::app());
    assert!(own.ends_with("azlin/locks/app"), "{own:?}");
    // a test stack's keyring stand-in file: its entries, its locks
    let stand_in = lock_folder_for(
        Some(std::ffi::OsString::from("/tmp/stack/keyring.json")),
        Some(std::path::PathBuf::from("/home/ann/.config")),
        &shared,
    );
    assert_eq!(
        stand_in,
        std::path::PathBuf::from("/tmp/stack/keyring.json.locks/io.azlin.shared")
    );
    // every app's keyring of the namespace locks in one folder
    let memory: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
    let a = SharedKeyring::new(memory.clone(), shared.clone());
    let b = SharedKeyring::new(memory, shared);
    assert_eq!(a.locks(), b.locks());
}
