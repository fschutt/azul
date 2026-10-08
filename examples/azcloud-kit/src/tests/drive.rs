//! An Azlin drive refreshes its 12-hour credentials by itself, once at a time, and hands every
//! rotated drive token to the app (the old one is dead: the next run must not spend it).

use std::sync::{Arc, Mutex};

use azul_storage::{Drive, DriveError, HttpReply, ListRequest, Method, Transport};

use super::{bundle, empty_listing, header, json, Fake, Shared, TOKEN};
use crate::{
    drive::{AzlinDrive, TransportFactory},
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

/// The drive of the test bundle, the rotated sessions it handed over, at time `now`.
fn drive(fake: &Arc<Fake>, now: u64) -> (AzlinDrive, Arc<Mutex<Vec<AzlinSession>>>) {
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let rotated = Arc::new(Mutex::new(Vec::new()));
    let keep = rotated.clone();
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        factory(fake),
        Box::new(move |session: &AzlinSession| keep.lock().unwrap().push(session.clone())),
    )
    .unwrap()
    .with_clock(move || now);
    (drive, rotated)
}

fn refreshes(fake: &Fake) -> usize {
    fake.calls()
        .iter()
        .filter(|c| c.method == Method::Post && c.url.ends_with("/credentials"))
        .count()
}

#[test]
fn fresh_credentials_are_used_as_they_are() {
    let fake = cloud(None);
    let (drive, rotated) = drive(&fake, EXPIRES - 6 * 3600);
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 0);
    assert!(rotated.lock().unwrap().is_empty());
}

#[test]
fn credentials_running_out_are_refreshed_once_and_the_new_token_is_handed_over() {
    let fake = cloud(None);
    let (drive, rotated) = drive(&fake, EXPIRES - 600);
    drive.list(&ListRequest::folder("")).unwrap();
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1, "{:?}", fake.calls());
    let handed = rotated.lock().unwrap().clone();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].drive_token, "dt_f.1.bbb");
    assert_eq!(handed[0].access_key_id, "AKID2");
    assert_eq!(drive.session().drive_token, "dt_f.1.bbb");
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
    let fake = cloud(Some("AKID1"));
    let (drive, rotated) = drive(&fake, EXPIRES - 6 * 3600);
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(refreshes(&fake), 1);
    assert_eq!(rotated.lock().unwrap().len(), 1);
}

#[test]
fn a_drive_whose_token_is_refused_asks_to_sign_in_again() {
    let fake = cloud(None);
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.9.zzz")).unwrap();
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        factory(&fake),
        Box::new(|_: &AzlinSession| {}),
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
    let fake = cloud(None);
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let mut entry = first.entry_named("Cloud", TOKEN);
    if let azul_storage::config::DriveLocation::S3 { auth, .. } = &mut entry.location {
        *auth = azul_storage::config::DriveAuth::Azlin {
            drive_id: "d_1".to_string(),
            account_url: String::new(),
        };
    }
    assert!(matches!(
        AzlinDrive::new(&entry, first.session(), "", factory(&fake), Box::new(|_: &AzlinSession| {})),
        Err(DriveError::InvalidConfig(_))
    ));
}
