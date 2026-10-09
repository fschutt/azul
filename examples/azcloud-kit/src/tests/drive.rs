//! An Azlin drive refreshes its 12-hour credentials by itself, once at a time - in this process
//! and across every process of the user: the refresh holds the drive's lock, re-reads the
//! keyring first (another window may have spent the token already: its session is the newest)
//! and stores the rotated session before it lets go. Every session the drive switches to goes
//! to the app (the old token is dead: the next run must not spend it).

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Barrier, Mutex,
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, DriveError, HttpReply, ListRequest, Method, Transport,
};

use super::{bundle, empty_listing, header, json, Fake, Shared, TOKEN};
use crate::{
    drive::{AzlinDrive, TransportFactory},
    lock::LockDir,
    shared::SharedKeyring,
    AzlinSession, DriveBundle,
};

/// 2026-10-08T21:15:00Z: when the first credentials run out.
const EXPIRES: u64 = 1_791_494_100;

/// The cloud: the token server (a refresh with the right token answers the next bundle) and
/// the bucket (a listing; a refused key when `refuse_key` names the credentials used).
fn cloud(refuse_key: Option<&'static str>) -> Arc<Fake> {
    Fake::new(move |call, _| {
        if call.url.starts_with(TOKEN) {
            return Ok(match header(call, "authorization") {
                Some("Bearer dt_f.0.aaa") => {
                    json(200, &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb"))
                }
                Some("Bearer dt_f.1.bbb") => {
                    json(200, &bundle("AKID3", "2026-10-09T21:15:00Z", "dt_f.2.ccc"))
                }
                _ => json(
                    401,
                    r#"{"error": "token_reuse", "message": "an old token was reused"}"#,
                ),
            });
        }
        let signed_with = header(call, "authorization").unwrap_or_default().to_string();
        if refuse_key.is_some_and(|key| signed_with.contains(&format!("Credential={key}/"))) {
            return Ok(HttpReply {
                status: 400,
                headers: Vec::new(),
                body: b"<Error><Code>ExpiredToken</Code><Message>The provided token has \
                        expired.</Message></Error>"
                    .to_vec(),
            });
        }
        Ok(empty_listing())
    })
}

fn factory(fake: &Arc<Fake>) -> TransportFactory {
    let fake = fake.clone();
    Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>)
}

/// What the drive handed the app: each session and whether the keyring has it.
type Rotations = Arc<Mutex<Vec<(AzlinSession, Result<(), String>)>>>;

/// The first bundle's session in a keyring of its own (what the app keeps when it opens the
/// drive), with the locks in `dir`.
fn keyring_with(first: &DriveBundle, dir: &TempDir) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let shared = SharedKeyring::new(keyring.clone(), LockDir::new(dir.path()));
    (shared, keyring)
}

/// The drive of the test bundle at time `now`: the sessions it handed over, its keyring.
fn drive(
    fake: &Arc<Fake>,
    now: u64,
    dir: &TempDir,
) -> (AzlinDrive, Rotations, Arc<MemoryKeyring>) {
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let (shared, keyring) = keyring_with(&first, dir);
    let rotated: Rotations = Arc::new(Mutex::new(Vec::new()));
    let keep = rotated.clone();
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        shared,
        factory(fake),
        Box::new(move |session: &AzlinSession, saved: Result<(), String>| {
            keep.lock().unwrap().push((session.clone(), saved));
        }),
    )
    .unwrap()
    .with_clock(move || now);
    (drive, rotated, keyring)
}

fn refreshes(fake: &Fake) -> usize {
    fake.calls()
        .iter()
        .filter(|c| c.method == Method::Post && c.url.ends_with("/credentials"))
        .count()
}

fn token_of(text: &str) -> String {
    AzlinSession::from_keyring_secret(text).unwrap().drive_token
}

fn kept_token(keyring: &MemoryKeyring) -> String {
    token_of(&keyring.get(&keyring_key("d_1")).unwrap().unwrap())
}

#[test]
fn fresh_credentials_are_used_as_they_are() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(None);
    let (drive, rotated, _) = drive(&fake, EXPIRES - 6 * 3600, &dir);
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 0);
    assert!(rotated.lock().unwrap().is_empty());
}

#[test]
fn credentials_running_out_are_refreshed_once_and_the_new_token_is_in_the_keyring() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(None);
    let (drive, rotated, keyring) = drive(&fake, EXPIRES - 600, &dir);
    drive.list(&ListRequest::folder("")).unwrap();
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1, "{:?}", fake.calls());
    let handed = rotated.lock().unwrap().clone();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].0.drive_token, "dt_f.1.bbb");
    assert_eq!(handed[0].0.access_key_id, "AKID2");
    assert_eq!(handed[0].1, Ok(()), "the keyring has it");
    assert_eq!(drive.session().drive_token, "dt_f.1.bbb");
    assert_eq!(kept_token(&keyring), "dt_f.1.bbb");
    // The bucket was listed with the NEW key.
    let listing = fake
        .calls()
        .into_iter()
        .filter(|c| c.method == Method::Get)
        .last()
        .unwrap();
    assert!(
        header(&listing, "authorization")
            .unwrap()
            .contains("Credential=AKID2/"),
        "{listing:?}"
    );
}

#[test]
fn credentials_the_bucket_refuses_are_refreshed_and_the_call_tried_again() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(Some("AKID1"));
    let (drive, rotated, keyring) = drive(&fake, EXPIRES - 6 * 3600, &dir);
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1);
    assert_eq!(rotated.lock().unwrap().len(), 1);
    assert_eq!(kept_token(&keyring), "dt_f.1.bbb");
}

#[test]
fn a_session_another_process_refreshed_is_read_from_the_keyring_instead_of_spending_the_token() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(None);
    let (drive, rotated, keyring) = drive(&fake, EXPIRES - 600, &dir);
    // Another window spent dt_f.0.aaa meanwhile and kept what it got.
    let newer = DriveBundle::parse(&bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb")).unwrap();
    keyring
        .set(&keyring_key("d_1"), &newer.session().to_keyring_secret())
        .unwrap();
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 0, "the spent token is never sent again");
    assert_eq!(drive.session().drive_token, "dt_f.1.bbb");
    let handed = rotated.lock().unwrap().clone();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].0.access_key_id, "AKID2");
    assert_eq!(handed[0].1, Ok(()));
}

#[test]
fn a_refused_key_takes_the_newer_session_another_process_kept() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(Some("AKID1"));
    let (drive, _, keyring) = drive(&fake, EXPIRES - 6 * 3600, &dir);
    let newer = DriveBundle::parse(&bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb")).unwrap();
    keyring
        .set(&keyring_key("d_1"), &newer.session().to_keyring_secret())
        .unwrap();
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 0);
    assert_eq!(drive.session().access_key_id, "AKID2");
}

/// A keyring that takes nothing (none on this system).
struct NoKeyring;

impl KeyringStore for NoKeyring {
    fn get(&self, _: &str) -> Result<Option<String>, KeyringError> {
        Err(KeyringError::Unavailable)
    }
    fn set(&self, _: &str, _: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Unavailable)
    }
    fn delete(&self, _: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Unavailable)
    }
}

#[test]
fn a_rotated_session_the_keyring_does_not_take_is_handed_over_with_why() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(None);
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let rotated: Rotations = Arc::new(Mutex::new(Vec::new()));
    let keep = rotated.clone();
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        SharedKeyring::new(Arc::new(NoKeyring), LockDir::new(dir.path())),
        factory(&fake),
        Box::new(move |session: &AzlinSession, saved: Result<(), String>| {
            keep.lock().unwrap().push((session.clone(), saved));
        }),
    )
    .unwrap()
    .with_clock(move || EXPIRES - 600);
    drive.list(&ListRequest::folder("")).unwrap();
    let handed = rotated.lock().unwrap().clone();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].0.drive_token, "dt_f.1.bbb", "the drive goes on with it");
    let why = handed[0].1.clone().unwrap_err();
    assert!(why.contains("no keyring"), "{why}");
}

#[test]
fn a_drive_whose_token_is_refused_asks_to_sign_in_again() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(None);
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.9.zzz")).unwrap();
    let (shared, _) = keyring_with(&first, &dir);
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        shared,
        factory(&fake),
        Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
    )
    .unwrap()
    .with_clock(move || EXPIRES + 60);
    match drive.list(&ListRequest::folder("")) {
        Err(DriveError::Denied { message }) => assert!(message.contains("sign in"), "{message}"),
        other => panic!("not denied: {other:?}"),
    }
}

#[test]
fn a_drive_without_a_token_server_is_a_configuration_error() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(None);
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let (shared, _) = keyring_with(&first, &dir);
    let mut entry = first.entry_named("Cloud", TOKEN);
    if let azul_storage::config::DriveLocation::S3 { auth, .. } = &mut entry.location {
        *auth = azul_storage::config::DriveAuth::Azlin {
            drive_id: "d_1".to_string(),
            account_url: String::new(),
        };
    }
    assert!(matches!(
        AzlinDrive::new(
            &entry,
            first.session(),
            "",
            shared,
            factory(&fake),
            Box::new(|_: &AzlinSession, _: Result<(), String>| {})
        ),
        Err(DriveError::InvalidConfig(_))
    ));
}

// ==== Two processes, one drive ====

/// The token server's side of one token family, as the real one keeps it: the current token
/// refreshes (and is spent), a spent one revokes the whole family.
#[derive(Default)]
struct Family {
    generation: u64,
    revoked: bool,
    refreshes: usize,
}

fn token_of_generation(generation: u64) -> String {
    format!("dt_f.{generation}.t{generation}")
}

/// The cloud of one token family: a refresh with the current token answers the next one (and
/// credentials 12 hours past the last), a spent one revokes the family; the bucket lists.
fn rotating(family: Arc<Mutex<Family>>) -> Arc<Fake> {
    Fake::new(move |call, _| {
        if !call.url.starts_with(TOKEN) {
            return Ok(empty_listing());
        }
        let mut family = family.lock().unwrap();
        if family.revoked {
            return Ok(json(
                401,
                r#"{"error": "credentials_revoked", "message": "this device was removed"}"#,
            ));
        }
        let bearer = header(call, "authorization")
            .unwrap_or_default()
            .trim_start_matches("Bearer ")
            .to_string();
        if bearer != token_of_generation(family.generation) {
            family.revoked = true;
            return Ok(json(
                401,
                r#"{"error": "token_reuse", "message": "an old token was reused"}"#,
            ));
        }
        family.generation += 1;
        family.refreshes += 1;
        let generation = family.generation;
        let expires = crate::rfc3339(i64::try_from(EXPIRES + generation * 12 * 3600).unwrap());
        Ok(json(
            200,
            &bundle(
                &format!("AKID{generation}"),
                &expires,
                &token_of_generation(generation),
            ),
        ))
    })
}

#[test]
fn two_sessions_over_one_keyring_and_one_lock_race_a_refresh_and_neither_ends_with_a_spent_token(
) {
    let dir = TempDir::new("azcloud-race");
    let family = Arc::new(Mutex::new(Family::default()));
    let fake = rotating(family.clone());
    let first = DriveBundle::parse(&bundle(
        "AKID0",
        "2026-10-08T21:15:00Z",
        &token_of_generation(0),
    ))
    .unwrap();
    let keyring = Arc::new(MemoryKeyring::new());
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let now = Arc::new(AtomicU64::new(EXPIRES - 600));
    // Two windows (two processes): each read the same session when it started, each has its
    // own handle of the keyring and its own locks over the one folder.
    let windows: Arc<Vec<AzlinDrive>> = Arc::new(
        (0..2)
            .map(|_| {
                let clock = now.clone();
                AzlinDrive::new(
                    &first.entry_named("Cloud", TOKEN),
                    first.session(),
                    TOKEN,
                    SharedKeyring::new(keyring.clone(), LockDir::new(dir.path())),
                    factory(&fake),
                    Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
                )
                .unwrap()
                .with_clock(move || clock.load(Ordering::SeqCst))
            })
            .collect(),
    );
    for round in 1..=3u64 {
        // Both windows find the credentials running out at the same moment.
        let start = Arc::new(Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|window| {
                let windows = windows.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    windows[window].list(&ListRequest::folder("")).map(|_| ())
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        let newest = token_of_generation(round);
        {
            let family = family.lock().unwrap();
            assert!(!family.revoked, "round {round}: a spent token was sent again");
            assert_eq!(
                family.refreshes,
                usize::try_from(round).unwrap(),
                "round {round}: ONE refresh for both windows"
            );
        }
        for (window, drive) in windows.iter().enumerate() {
            assert_eq!(
                drive.session().drive_token,
                newest,
                "round {round}: window {window} holds the newest token"
            );
        }
        assert_eq!(kept_token(&keyring), newest, "round {round}: the keyring has it");
        // Twelve hours on, both run out again.
        now.store(EXPIRES + round * 12 * 3600 - 600, Ordering::SeqCst);
    }
}
