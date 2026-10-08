//! What an app keeps for an Azlin drive: the drives-file entry (no secret) and the keyring's
//! session (the drive token and the current credentials).

use azul_storage::{
    config::{DriveAuth, DriveLocation},
    Credentials,
};

use super::{bundle, S3, TOKEN};
use crate::{AzlinSession, DriveBundle};

/// 2026-10-08T21:15:00Z in seconds since 1970.
const EXPIRES: u64 = 1_791_494_100;

fn parsed() -> DriveBundle {
    DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap()
}

#[test]
fn a_bundle_is_a_drives_file_entry_under_the_users_name_without_a_secret() {
    let drive = parsed();
    let entry = drive.entry_named("My cloud", "http://ignored.example");
    assert_eq!(entry.id, "d_1");
    assert_eq!(entry.name, "My cloud");
    match &entry.location {
        DriveLocation::S3 {
            endpoint,
            bucket,
            path_style,
            auth,
            ..
        } => {
            assert_eq!(endpoint, S3);
            assert_eq!(bucket, "d-1");
            assert!(*path_style);
            assert_eq!(
                auth,
                &DriveAuth::Azlin {
                    drive_id: "d_1".to_string(),
                    account_url: TOKEN.to_string(),
                }
            );
        }
        other => panic!("not an S3 drive: {other:?}"),
    }
    let text = format!("{entry:?}");
    assert!(!text.contains("secret-of-AKID1") && !text.contains("dt_f.0.aaa"), "{text}");
}

#[test]
fn a_bundle_without_its_token_servers_address_gets_the_one_it_came_from() {
    let text = bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")
        .replace(&format!("\"account_url\": \"{TOKEN}\""), "\"account_url\": \"\"");
    let drive = DriveBundle::parse(&text).unwrap();
    let entry = drive.entry_named("Cloud", "http://127.0.0.1:8081");
    assert_eq!(entry.azlin(), Some(("d_1", "http://127.0.0.1:8081")));
}

#[test]
fn the_session_round_trips_through_the_keyring_and_reads_as_plain_credentials() {
    let session = parsed().session();
    assert_eq!(session.drive_id, "d_1");
    assert_eq!(session.drive_token, "dt_f.0.aaa");
    assert_eq!(session.expires_at, Some(EXPIRES));
    let secret = session.to_keyring_secret();
    assert_eq!(AzlinSession::from_keyring_secret(&secret).unwrap(), session);
    // An app without the kit (azul-storage alone) opens the bucket with what the session holds.
    assert_eq!(
        Credentials::from_keyring_secret(&secret).unwrap(),
        Credentials::new("AKID1", "secret-of-AKID1").with_session_token("session-of-AKID1")
    );
    assert_eq!(session.credentials(), Credentials::from_keyring_secret(&secret).unwrap());
    let text = format!("{session:?}");
    assert!(
        !text.contains("dt_f.0.aaa")
            && !text.contains("secret-of-AKID1")
            && !text.contains("session-of-AKID1"),
        "{text}"
    );
    assert!(AzlinSession::from_keyring_secret("not a session").is_err());
}

#[test]
fn credentials_are_refreshed_an_hour_before_they_run_out() {
    let session = parsed().session();
    assert!(!session.needs_refresh(EXPIRES - 2 * 3600));
    assert!(session.needs_refresh(EXPIRES - 1800));
    assert!(session.needs_refresh(EXPIRES + 1));
    assert!(session.is_valid_at(EXPIRES - 1800));
    assert!(!session.is_valid_at(EXPIRES));
}

#[test]
fn long_lived_keys_never_need_a_refresh() {
    let mut session = parsed().session();
    session.expires_at = None;
    assert!(!session.needs_refresh(u64::MAX / 2));
    assert!(session.is_valid_at(u64::MAX / 2));
}
