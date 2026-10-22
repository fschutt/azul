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
