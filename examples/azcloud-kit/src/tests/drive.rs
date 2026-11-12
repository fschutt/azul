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
    keyring::KeyringNamespace,
    config::keyring_key,
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, DriveError, HttpReply, ListRequest, Method, Precondition, Transport,
};

use super::{bundle, empty_listing, header, json, Fake, Shared, TOKEN};
use crate::{
    drive::{AzlinDrive, TransportFactory},
    shared::SharedKeyring,
    sync::session::SyncProblem,
    user_errors::{Behaviour, Code, UserError},
    AzlinSession, CloudError, DriveBundle,
};

/// 2026-10-08T21:15:00Z: when the first credentials run out.
const EXPIRES: u64 = 1_791_494_100;

/// The cloud: the token server (a refresh with the right token answers the next bundle) and
/// the bucket (a listing; a refused key when `refuse_key` names the credentials used).
fn cloud(refuse_key: Option<&'static str>) -> Arc<Fake> {
    cloud_expiring(refuse_key, "2026-10-09T09:15:00Z", "2026-10-09T21:15:00Z")
}

/// [`cloud`] whose credentials, once refreshed, last: a drive that makes several requests (the encryption's
/// gate decides the drive from the bucket before its first answer) refreshes once, as the system's clock
/// stands, and not at every request.
fn cloud_lasting() -> Arc<Fake> {
    cloud_expiring(None, "2099-01-01T00:00:00Z", "2099-01-01T12:00:00Z")
}

fn cloud_expiring(
    refuse_key: Option<&'static str>,
    first_refresh_expires: &'static str,
    second_refresh_expires: &'static str,
) -> Arc<Fake> {
    Fake::new(move |call, _| {
        if call.url.starts_with(TOKEN) {
            return Ok(match header(call, "authorization") {
                Some("Bearer dt_f.0.aaa") => {
                    json(200, &bundle("AKID2", first_refresh_expires, "dt_f.1.bbb"))
                }
                Some("Bearer dt_f.1.bbb") => {
                    json(200, &bundle("AKID3", second_refresh_expires, "dt_f.2.ccc"))
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

/// A copy of `session` for the test to look at (a session has no `Clone`).
fn copy(session: &AzlinSession) -> AzlinSession {
    AzlinSession::from_keyring_secret_without_credentials(&session.to_keyring_secret()).unwrap()
}

/// What the drive handed over so far.
fn handed(rotated: &Rotations) -> Vec<(AzlinSession, Result<(), String>)> {
    let handed = rotated.lock().unwrap();
    handed.iter().map(|(s, saved)| (copy(s), saved.clone())).collect()
}

/// The drive token the drive holds now.
fn held_token(drive: &AzlinDrive) -> String {
    drive.with_session(|s| s.drive_token.as_str().to_string())
}

/// The first bundle's session in a keyring of its own (what the app keeps when it opens the
/// drive), with the locks in `dir`.
fn keyring_with(first: &DriveBundle, dir: &TempDir) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let shared =
        SharedKeyring::with_lock_root(keyring.clone(), KeyringNamespace::azlin_shared(), dir.path());
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
            keep.lock().unwrap().push((copy(session), saved));
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
    AzlinSession::from_keyring_secret(text)
        .unwrap()
        .drive_token
        .as_str()
        .to_string()
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
    let handed = handed(&rotated);
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].0.drive_token.as_str(), "dt_f.1.bbb");
    assert_eq!(handed[0].0.access_key_id, "AKID2");
    assert_eq!(handed[0].1, Ok(()), "the keyring has it");
    assert_eq!(held_token(&drive), "dt_f.1.bbb");
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
    assert_eq!(held_token(&drive), "dt_f.1.bbb");
    let handed = handed(&rotated);
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
    assert_eq!(drive.with_session(|s| s.access_key_id.clone()), "AKID2");
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
        SharedKeyring::with_lock_root(
            Arc::new(NoKeyring),
            KeyringNamespace::azlin_shared(),
            dir.path(),
        ),
        factory(&fake),
        Box::new(move |session: &AzlinSession, saved: Result<(), String>| {
            keep.lock().unwrap().push((copy(session), saved));
        }),
    )
    .unwrap()
    .with_clock(move || EXPIRES - 600);
    drive.list(&ListRequest::folder("")).unwrap();
    let handed = handed(&rotated);
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].0.drive_token.as_str(), "dt_f.1.bbb", "the drive goes on with it");
    let why = handed[0].1.clone().unwrap_err();
    assert!(why.contains("no keyring"), "{why}");
}

#[test]
fn a_drive_whose_token_is_refused_says_this_device_was_removed_in_the_tables_words() {
    // The token server refused the drive token (401: this device was removed, or its token was
    // spent elsewhere): only signing in again helps. It is the table's credentials_revoked, as a
    // node's refusal of revoked credentials is, so a sync's status says "This device was removed
    // from the drive. Sign in again." and the app stops asking the drive.
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
    let error = drive.list(&ListRequest::folder("")).unwrap_err();
    let user = UserError::from_drive_error(&error).expect("the errors table words it");
    assert_eq!(user.code, Code::CredentialsRevoked, "{error}");
    assert_eq!(user.behaviour(), Behaviour::SignInAgain);
    let error = CloudError::from(error);
    assert!(error.is_sign_in(), "{error}");
    let problem = SyncProblem::of(&error).expect("a failed pass keeps the table's code");
    assert_eq!(problem.code, "credentials_revoked");
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
            encrypted: false,
            key_id: None,
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
                    SharedKeyring::with_lock_root(
                        keyring.clone(),
                        KeyringNamespace::azlin_shared(),
                        dir.path(),
                    ),
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
                held_token(&drive),
                newest,
                "round {round}: window {window} holds the newest token"
            );
        }
        assert_eq!(kept_token(&keyring), newest, "round {round}: the keyring has it");
        // Twelve hours on, both run out again.
        now.store(EXPIRES + round * 12 * 3600 - 600, Ordering::SeqCst);
    }
}

#[test]
fn an_azlin_drive_writes_conditionally_and_streams_through_its_bucket() {
    let dir = TempDir::new("azcloud-drive");
    let fake = cloud(None);
    let (drive, _, _) = drive(&fake, EXPIRES - 6 * 3600, &dir);
    drive
        .put_if("data/ab/object", b"sealed", &Precondition::Absent)
        .unwrap();
    let put = fake
        .calls()
        .into_iter()
        .filter(|c| c.method == Method::Put)
        .last()
        .expect("a conditional PUT");
    assert_eq!(header(&put, "if-none-match"), Some("*"));
    assert_eq!(put.body, b"sealed");

    assert_eq!(
        drive
            .put_from("data/cd/object", &mut &b"streamed"[..])
            .unwrap(),
        8
    );
    let put = fake
        .calls()
        .into_iter()
        .filter(|c| c.method == Method::Put)
        .last()
        .unwrap();
    assert!(put.url.ends_with("/data/cd/object"), "{}", put.url);
    assert_eq!(put.body, b"streamed");
    assert_eq!(refreshes(&fake), 0);
}

#[test]
fn a_token_server_that_cannot_refresh_now_is_a_busy_service_to_the_user() {
    // The refresh's refusal keeps its status and code (C11): the user sees "we'll keep trying",
    // not "something went wrong".
    let dir = TempDir::new("azcloud-drive");
    let fake = Fake::new(|call, _| {
        Ok(if call.url.ends_with("/credentials") {
            json(
                429,
                r#"{"error": "too_many_requests", "message": "slow down"}"#,
            )
        } else {
            empty_listing()
        })
    });
    let (drive, _, _) = drive(&fake, EXPIRES + 60, &dir);
    let error = drive.list(&ListRequest::folder("")).unwrap_err();
    match &error {
        DriveError::Service(service) => {
            assert_eq!((service.status, service.code.as_str()), (429, "too_many_requests"));
        }
        other => panic!("not a service error: {other:?}"),
    }
    assert_eq!(
        UserError::from_drive_error(&error).map(|user| user.code),
        Some(Code::Unavailable)
    );
}

#[test]
fn a_banned_drive_past_its_end_refuses_the_refresh_in_the_tables_words() {
    // Ban contract v1: past the ban's end the token server refuses the credentials (403
    // drive_banned) - the listing says it is the banned drive, not "something went wrong".
    let dir = TempDir::new("azcloud-drive");
    let fake = Fake::new(|call, _| {
        Ok(if call.url.ends_with("/credentials") {
            json(
                403,
                r#"{"error": "drive_banned", "message": "this drive was closed",
                    "ban_reason": "spam distribution", "ban_until": "2026-10-08T21:00:00Z"}"#,
            )
        } else {
            empty_listing()
        })
    });
    let (drive, _, _) = drive(&fake, EXPIRES + 60, &dir);
    let error = drive.list(&ListRequest::folder("")).unwrap_err();
    assert_eq!(
        UserError::from_drive_error(&error).map(|user| user.code),
        Some(Code::DriveBanned)
    );
}

/// PT3: credentials made while the drive took no writes stay read-only for 12 hours. Once the
/// drive's grant changed (a month bought, a voucher) they are refreshed before the next request;
/// a node that calls them stale (`credentials_stale`) has them refreshed and the request sent
/// again.
#[test]
fn read_only_credentials_are_refreshed_once_the_grant_changed_or_the_node_calls_them_stale() {
    let fake = cloud(None);
    let dir = TempDir::new("azcloud-drive");
    let text = bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")
        .replace(r#""read_only": false"#, r#""read_only": true"#);
    let first = DriveBundle::parse(&text).unwrap();
    assert_eq!(first.session().grant, crate::Grant::ReadOnly);
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
    .with_clock(|| EXPIRES - 6 * 3600);
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 0, "valid credentials are used as they are");
    drive.apply(crate::period::GrantChanged::of("d_1"));
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1, "refreshed before the request");
    assert_eq!(held_token(&drive), "dt_f.1.bbb");
    // A grant of another drive changes nothing here.
    drive.apply(crate::period::GrantChanged::of("d_2"));
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1);

    // The node calls the credentials stale: refreshed, the request sent again.
    let stale = Fake::new(|call, _| {
        if call.url.starts_with(TOKEN) {
            return Ok(json(200, &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb")));
        }
        let signed_with = header(call, "authorization").unwrap_or_default().to_string();
        if signed_with.contains("Credential=AKID1/") {
            return Ok(HttpReply {
                status: 403,
                headers: vec![(String::from("x-azlin-error"), String::from("credentials_stale"))],
                body: b"<Error><Code>AccessDenied</Code><Message>refresh them</Message></Error>"
                    .to_vec(),
            });
        }
        Ok(empty_listing())
    });
    let dir = TempDir::new("azcloud-drive");
    let (drive, _, _) = super::drive::drive(&stale, EXPIRES - 6 * 3600, &dir);
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&stale), 1);
}

#[test]
fn a_newer_map_a_node_names_makes_the_drive_take_the_token_servers_view_before_its_next_request() {
    let dir = TempDir::new("azcloud-drive");
    let fake = Fake::new(|call, _| {
        if call.url.starts_with(TOKEN) {
            return Ok(match header(call, "authorization") {
                Some("Bearer dt_f.0.aaa") => json(
                    200,
                    &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb").replace(
                        r#""nodes": []"#,
                        r#""nodes": [], "map": {"cluster": "c1", "epoch": 2, "hash": "9f86"}"#,
                    ),
                ),
                _ => json(401, r#"{"error": "token_reuse", "message": "reused"}"#),
            });
        }
        match header(call, "x-azlin-map") {
            Some("1") => Ok(HttpReply {
                status: 409,
                headers: vec![
                    (String::from("x-azlin-error"), String::from("map_stale")),
                    (String::from("x-azlin-map"), String::from("2 9f86")),
                ],
                body: b"<Error><Code>AzlinMapStale</Code><Message>epoch 2</Message></Error>"
                    .to_vec(),
            }),
            _ => Ok(empty_listing()),
        }
    });
    let (drive, _, _) = drive(&fake, EXPIRES - 6 * 3600, &dir);
    drive.failover().set_map_epoch(Some(1));
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 0, "the request itself went on with the node's epoch");
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1, "the node list and the map came from the token server");
    assert_eq!(drive.failover().map_epoch(), Some(2));
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1, "once");
}

// ==== The account and the bytes of an Azlin drive (ARCH29 C6d) ====

/// The live token server and a hostile one that `drives.json` (any process of the user can write
/// it) or a lying token server's bundle names.
const LIVE: &str = "https://token.azlin.io";
const HOSTILE: &str = "https://evil.example";

fn azlin_entry(account_url: &str) -> azul_storage::config::DriveEntry {
    use azul_storage::config::{DriveAuth, DriveEntry, DriveLocation};
    DriveEntry {
        id: String::from("d_1"),
        name: String::from("Photos"),
        location: DriveLocation::S3 {
            endpoint: String::from("https://s3.example.test"),
            region: String::from("us-east-1"),
            bucket: String::from("d-1"),
            path_style: true,
            auth: DriveAuth::Azlin {
                drive_id: String::from("d_1"),
                account_url: account_url.to_string(),
                encrypted: false,
                key_id: None,
            },
        },
    }
}

fn target(drive_id: &str, url: &str) -> Option<crate::drive::AzlinTarget> {
    Some(crate::drive::AzlinTarget {
        drive_id: drive_id.to_string(),
        token_url: url.to_string(),
    })
}

/// A SHIPPED build sends the drive token, the period tokens, a recovery key, a voucher and a lockdown
/// to the server the run resolved (the live one), whatever the entry says: the old code took
/// `account_url` of `drives.json` and the next start's daily look sent the drive token there
/// (`Authorization: Bearer ...`), which the attacker spends at the real server. The one resolver is
/// `azlin_target`, in the kit (X-DELTA-AZDRIVE-KEYS-9; it was AzDrive's `periods::azlin_drive`).
#[test]
fn x_delta_azdrive_keys_9_a_shipped_build_sends_the_drive_token_to_the_live_server_whatever_the_entry_names(
) {
    use crate::drive::azlin_target_choice;
    assert_eq!(
        azlin_target_choice(false, &azlin_entry(HOSTILE), Some(LIVE)),
        target("d_1", LIVE)
    );
    // The same for an entry that names the live server, and one that names nothing.
    assert_eq!(
        azlin_target_choice(false, &azlin_entry(LIVE), Some(LIVE)),
        target("d_1", LIVE)
    );
    assert_eq!(
        azlin_target_choice(false, &azlin_entry(""), Some(LIVE)),
        target("d_1", LIVE)
    );
    // A run with no server to name sends nothing - not to the entry's.
    assert_eq!(
        azlin_target_choice(false, &azlin_entry(HOSTILE), None),
        None
    );
    assert_eq!(
        azlin_target_choice(false, &azlin_entry(HOSTILE), Some("  ")),
        None
    );
}

/// A TEST build keeps what it had: the entry's server (a local stack's drive), else the run's; only an
/// Azlin drive has a token server; and the choice `azlin_target` makes is the anchors' for the build under
/// test (the shipped branch is the one above).
#[test]
fn a_test_build_takes_the_entrys_server_else_the_runs_and_only_an_azlin_drive_has_one() {
    use crate::drive::{azlin_target, azlin_target_choice};
    assert_eq!(
        azlin_target_choice(true, &azlin_entry("http://127.0.0.1:18081"), Some(LIVE)),
        target("d_1", "http://127.0.0.1:18081")
    );
    assert_eq!(
        azlin_target_choice(true, &azlin_entry("  "), Some("http://127.0.0.1:8081")),
        target("d_1", "http://127.0.0.1:8081")
    );
    assert_eq!(azlin_target_choice(true, &azlin_entry(""), None), None);
    let local = azul_storage::config::DriveEntry {
        id: String::from("home"),
        name: String::from("Home"),
        location: azul_storage::config::DriveLocation::Local {
            root: String::from("/tmp/home"),
        },
    };
    assert_eq!(azlin_target_choice(true, &local, Some(LIVE)), None);
    assert_eq!(azlin_target_choice(false, &local, Some(LIVE)), None);
    let honoured = azul_appkit::anchors::layers_honoured();
    assert_eq!(
        azlin_target(&azlin_entry(HOSTILE), Some(LIVE)),
        azlin_target_choice(honoured, &azlin_entry(HOSTILE), Some(LIVE))
    );
}

/// ARCH29 C6d, X-DELTA-AZDRIVE-KEYS-4 and -9, X-DELTA-STRUCT3-10. `open_azlin` hands out the account and the
/// drive the gate made, nothing else: the gate is called once and is given the bytes of the bucket; it can
/// only return the encryption's own drive (`AutoEncrypted`: the compile-fail doctest of `open_azlin`), and a
/// request of the drive that comes out reaches the bucket with the drive's credentials; the account (a
/// lockdown, the session, the token server it resolved) is the same drive's and has no put and no get.
#[cfg(feature = "encryption")]
#[test]
fn c6d_open_azlin_hands_out_the_account_and_only_the_drive_the_gate_made() {
    use crate::drive::{open_azlin, AzlinOpen};
    use azul_storage::AutoEncrypted;
    let dir = TempDir::new("c6d-open-azlin");
    let fake = cloud_lasting();
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let (shared, _) = keyring_with(&first, &dir);
    let gates = Arc::new(AtomicU64::new(0));
    let counted = gates.clone();
    let (account, drive) = open_azlin(
        &first.entry_named("Cloud", TOKEN),
        AzlinOpen {
            session: first.session(),
            token_url: TOKEN,
            keyring: shared,
            transports: factory(&fake),
            on_rotated: Box::new(|_, _| {}),
            nodes_file: None,
            iroh: None,
        },
        move |account, bytes| {
            counted.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                account.token_url(),
                TOKEN,
                "the resolver's choice is the account's"
            );
            Arc::new(AutoEncrypted::new(
                bytes,
                "d_1",
                Arc::new(MemoryKeyring::new()),
                None,
            ))
        },
    )
    .unwrap();
    assert_eq!(
        gates.load(Ordering::SeqCst),
        1,
        "the gate is made once, over the bytes"
    );
    // Nothing is sent by the opening itself.
    assert!(fake.calls().is_empty(), "{:?}", fake.calls());
    // A request of the drive goes through the gate (which decides the drive from the bucket's listing)
    // and to the bucket.
    drive.list(&ListRequest::folder("")).unwrap();
    assert!(
        fake.calls()
            .iter()
            .any(|c| c.method == Method::Get && c.url.contains("127.0.0.1:19000/d-1")),
        "and reached the bucket: {:?}",
        fake.calls()
    );
    // The account is the same drive's: the credentials of the bundle were out of date (the clock is the
    // system's), so the request refreshed them - and the account holds the session that refresh made.
    assert_eq!(
        account.with_session(|s| s.drive_token.as_str().to_string()),
        "dt_f.1.bbb"
    );
    assert_eq!(refreshes(&fake), 1, "{:?}", fake.calls());
    // An entry that is no bucket is no Azlin drive: nothing is opened and the gate is not asked.
    let local = azul_storage::config::DriveEntry {
        id: String::from("home"),
        name: String::from("Home"),
        location: azul_storage::config::DriveLocation::Local {
            root: String::from("/tmp/home"),
        },
    };
    let (shared, _) = keyring_with(&first, &dir);
    let asked = Arc::new(AtomicU64::new(0));
    let counted = asked.clone();
    let refused = open_azlin(
        &local,
        AzlinOpen {
            session: first.session(),
            token_url: TOKEN,
            keyring: shared,
            transports: factory(&fake),
            on_rotated: Box::new(|_, _| {}),
            nodes_file: None,
            iroh: None,
        },
        move |_, bytes| {
            counted.fetch_add(1, Ordering::SeqCst);
            Arc::new(AutoEncrypted::new(
                bytes,
                "home",
                Arc::new(MemoryKeyring::new()),
                None,
            ))
        },
    );
    assert!(
        refused.is_err(),
        "a folder on disk has no account at a token server"
    );
    assert_eq!(asked.load(Ordering::SeqCst), 0, "the gate was not asked");
}

/// X-DELTA-STRUCT3-16. The vault of an account is opened under the account's own drive id: a device that
/// holds the key of ANOTHER drive and none of this one finds no vault for this drive (it was a parameter,
/// and the key of the id a caller named was opened over this account's bucket, the sealed vault of that
/// other drive written into it). The id is the entry's (`AzlinAccount::drive_id`), the compile-fail doctest
/// of `unlocked` is the other half.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_struct3_16_the_vault_of_an_account_is_opened_under_its_own_drive_id_and_no_other() {
    use azul_storage::crypto::{device, DriveKey};

    use crate::drive::AzlinAccount;

    let dir = TempDir::new("struct3-16");
    let fake = cloud(None);
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let (shared, _) = keyring_with(&first, &dir);
    let entry = first.entry_named("Cloud", TOKEN);
    let target = crate::drive::target_of(&entry, TOKEN).unwrap();
    let account = Arc::new(
        AzlinAccount::new(
            &entry,
            first.session(),
            &target,
            shared,
            factory(&fake),
            Box::new(|_, _| {}),
        )
        .unwrap()
        .with_clock(|| EXPIRES - 6 * 3600),
    );
    assert_eq!(
        account.drive_id(),
        "d_1",
        "the entry's drive id, derived once"
    );
    let keys = MemoryKeyring::new();
    // This device holds the key of another drive, and none of this one.
    device::store_drive_key(&keys, "d_2", &DriveKey::generate().unwrap()).unwrap();
    assert!(
        account.unlocked(&keys).unwrap().is_none(),
        "the other drive's key is not this drive's: no vault is opened over this bucket with it"
    );
    // Its own key: the vault of THIS drive, under its own id.
    device::store_drive_key(&keys, "d_1", &DriveKey::generate().unwrap()).unwrap();
    let unlocked = account
        .unlocked(&keys)
        .unwrap()
        .expect("this device holds the key of this drive");
    assert_eq!(unlocked.drive_id(), "d_1");
    // Nothing was written into the bucket by either look.
    assert!(
        fake.calls()
            .iter()
            .all(|c| !matches!(c.method, Method::Put | Method::Delete)),
        "{:?}",
        fake.calls()
    );
}

/// ARCH29 C6d. The bytes of an Azlin drive have no drive below: what falls to `below()` is refused
/// (a method added to the trait and not written for the bytes is a refusal, not the trait's default for a
/// drive nobody looked at), and the places stay what they were - a drive is the very object it is, two
/// drives are two places, which the engine's overlap guards rest on.
#[test]
fn c6d_the_bytes_have_no_drive_below_and_each_drive_is_its_own_place() {
    use azul_storage::ListRequest;
    let (dir_a, dir_b) = (TempDir::new("c6d-below-a"), TempDir::new("c6d-below-b"));
    let fake = cloud(None);
    let (a, _, _) = drive(&fake, EXPIRES - 6 * 3600, &dir_a);
    let (b, _, _) = drive(&fake, EXPIRES - 6 * 3600, &dir_b);
    let below = azul_storage::DriveDelegate::below(&a);
    assert!(matches!(below.get("k"), Err(DriveError::Unsupported(_))));
    assert!(matches!(
        below.put("k", b"v"),
        Err(DriveError::Unsupported(_))
    ));
    assert!(matches!(below.delete("k"), Err(DriveError::Unsupported(_))));
    assert!(matches!(
        below.list(&ListRequest::folder("")),
        Err(DriveError::Unsupported(_))
    ));
    assert_eq!(a.site(), a.site(), "the very object it is");
    assert_ne!(a.site(), b.site(), "two drives are two places");
    assert_eq!(a.local_path("k"), None);
    assert!(a.conditional_writes());
    // Nothing of the above reached the bucket.
    assert!(fake
        .calls()
        .iter()
        .all(|c| !c.url.contains("127.0.0.1:19000")));
}

/// X-DELTA-STRUCT3-8. The drive below the bytes of an Azlin drive refuses EVERY method of the trait: the call of
/// every method of THE list (`testing::every_method`, so a method the trait gets next is in it) answers each request
/// `Unsupported` - a method the bytes did not write is a refusal and never the trait's default of a drive nobody
/// looked at - and each question about the drive with its "no". It wrote six methods and left the rest to the
/// defaults, which refuse only because they are composed over those six; a new question of the trait falls to its
/// default answer, and nothing required the bytes to write it. A question this test does not know fails it.
#[test]
fn x_delta_struct3_8_the_drive_below_the_bytes_refuses_every_request_and_answers_every_question_with_no(
) {
    use azul_storage::testing::{every_method, METHOD_NAMES};

    let dir = TempDir::new("struct3-8-below");
    let fake = cloud(None);
    let (a, _, _) = drive(&fake, EXPIRES - 6 * 3600, &dir);
    let below = azul_storage::DriveDelegate::below(&a);
    let called = every_method(below, dir.path());
    assert_eq!(
        called.len(),
        METHOD_NAMES.len(),
        "every method of the list was called"
    );
    let mut answered = Vec::new();
    for call in called.iter().filter(|call| call.request) {
        if !matches!(call.outcome, Err(DriveError::Unsupported(_))) {
            answered.push(format!("{}: {:?}", call.name, call.outcome));
        }
    }
    assert!(
        answered.is_empty(),
        "requests the drive below did not refuse: {answered:?}"
    );
    let questions: Vec<&str> = called
        .iter()
        .filter(|call| !call.request)
        .map(|call| call.name)
        .collect();
    assert_eq!(
        questions,
        ["local_path", "site", "resume_folder", "conditional_writes"],
        "a question the trait got is not answered here: say what the drive below answers it"
    );
    let answer = |name: &str| {
        called
            .iter()
            .find(|call| call.name == name)
            .and_then(|call| call.answer.as_deref())
            .map(str::to_string)
    };
    assert_eq!(answer("local_path").as_deref(), Some("None"));
    assert_eq!(answer("resume_folder").as_deref(), Some("None"));
    assert_eq!(answer("conditional_writes").as_deref(), Some("false"));
    // Nothing of the above reached the bucket.
    assert!(fake
        .calls()
        .iter()
        .all(|c| !c.url.contains("127.0.0.1:19000")));
}

/// X-DELTA-STRUCT3-11. The token server of an account is the resolver's answer and no other function's choice:
/// `open_azlin` asks `azlin_target` (it asked nothing, and `AzlinAccount::new` chose the server again itself, so a
/// later change of the one choice - a pinned host, a normalised address - did not reach the account's refresh), and the
/// account is built over what it answered: its server and its drive id. An entry that is no Azlin drive opens nothing:
/// a folder, and an S3 bucket of ANOTHER provider (the user's own access key), which the old opener took for an Azlin
/// drive and opened an account for against the run's server. Nothing is sent for any of it.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_struct3_11_open_azlin_builds_the_account_over_the_resolvers_answer_and_refuses_what_is_no_azlin_drive() {
    use crate::drive::{azlin_target, open_azlin, AzlinAccount, AzlinOpen};
    use azul_storage::{
        config::{DriveAuth, DriveEntry, DriveLocation},
        AutoEncrypted,
    };

    fn gate(_: &Arc<AzlinAccount>, bytes: Arc<dyn Drive>) -> Arc<AutoEncrypted> {
        Arc::new(AutoEncrypted::new(bytes, "d_1", Arc::new(MemoryKeyring::new()), None))
    }
    let dir = TempDir::new("struct3-11");
    let fake = cloud_lasting();
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let open = |entry: &DriveEntry, run: &str| {
        let (shared, _) = keyring_with(&first, &dir);
        open_azlin(
            entry,
            AzlinOpen {
                session: first.session(),
                token_url: run,
                keyring: shared,
                transports: factory(&fake),
                on_rotated: Box::new(|_, _| {}),
                nodes_file: None,
                iroh: None,
            },
            gate,
        )
    };

    // An Azlin drive: the account's server and its drive id are the resolver's answer, whatever the entry names.
    for named in [HOSTILE, LIVE, ""] {
        let entry = azlin_entry(named);
        let target = azlin_target(&entry, Some(LIVE)).expect("a drive that names a server, or the run's");
        let (account, _) = open(&entry, LIVE).expect("an Azlin drive opens");
        assert_eq!(account.token_url(), target.token_url, "the account chose its server itself ({named:?})");
        assert_eq!(account.drive_id(), target.drive_id);
    }
    // No server named by the entry or the run: no account.
    let refused = open(&azlin_entry(""), "").err().expect("no token server to name");
    assert!(
        matches!(&refused, DriveError::InvalidConfig(why) if why.contains("names no token server")),
        "{refused:?}"
    );
    // A folder, and a bucket of another provider, are no Azlin drive.
    let local = DriveEntry {
        id: String::from("home"),
        name: String::from("Home"),
        location: DriveLocation::Local {
            root: String::from("/tmp/home"),
        },
    };
    assert!(open(&local, LIVE).is_err(), "a folder is no Azlin drive");
    let other = DriveEntry {
        id: String::from("b_1"),
        name: String::from("Elsewhere"),
        location: DriveLocation::S3 {
            endpoint: String::from("https://s3.other.example"),
            region: String::from("us-east-1"),
            bucket: String::from("mine"),
            path_style: true,
            auth: DriveAuth::Keyring,
        },
    };
    let refused = open(&other, LIVE).err().expect("a bucket of another provider is no Azlin drive");
    assert!(
        matches!(&refused, DriveError::InvalidConfig(why) if why.contains("not an Azlin drive")),
        "{refused:?}"
    );
    assert!(fake.calls().is_empty(), "nothing was sent: {:?}", fake.calls());
}
