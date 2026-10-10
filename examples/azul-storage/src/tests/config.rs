use std::{collections::BTreeMap, path::PathBuf};

use super::TempDir;
use crate::{
    config::{
        drives_file, keyring_key, new_drive_id, DatabaseEngine, DriveAuth, DriveEntry,
        DriveLocation, DrivesFile, SecretOptions,
    },
    Credentials, Drive, DriveError, HttpCall, HttpReply, ListRequest, Transport,
};

struct NoNetwork;

impl Transport for NoNetwork {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        Err("no network in this test".to_string())
    }
}

fn s3_entry(id: &str) -> DriveEntry {
    DriveEntry {
        id: id.to_string(),
        name: "S3 Drive".to_string(),
        location: DriveLocation::S3 {
            endpoint: "https://s3.eu-central-1.amazonaws.com".to_string(),
            region: "eu-central-1".to_string(),
            bucket: "felix-azlin".to_string(),
            path_style: false,
            auth: DriveAuth::Keyring,
        },
    }
}

#[test]
fn the_drives_file_round_trips_without_any_secret() {
    let tmp = TempDir::new("config-roundtrip");
    let path = tmp.path().join("nested").join("drives.json");
    let mut file = DrivesFile::empty();
    file.add(s3_entry("d1"));
    file.save(&path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("\"format\": \"azul-storage.drives\""),
        "{text}"
    );
    assert!(!text.to_lowercase().contains("secret"), "{text}");
    assert!(!text.contains("access_key"), "{text}");

    let back = DrivesFile::load(&path).unwrap();
    assert_eq!(back, file);
    assert_eq!(back.get("d1"), Some(&s3_entry("d1")));
}

#[test]
fn adding_a_drive_with_a_known_id_replaces_it_and_remove_forgets_it() {
    let mut file = DrivesFile::empty();
    file.add(s3_entry("d1"));
    let mut renamed = s3_entry("d1");
    renamed.name = "Archive".to_string();
    file.add(renamed.clone());
    assert_eq!(file.drives, vec![renamed]);
    assert!(file.remove("d1").is_some());
    assert!(file.drives.is_empty());
}

#[test]
fn a_missing_drives_file_is_an_empty_list() {
    let tmp = TempDir::new("config-missing");
    let file = DrivesFile::load(&tmp.path().join("drives.json")).unwrap();
    assert!(file.drives.is_empty());
}

#[test]
fn a_drives_file_of_another_format_or_a_newer_version_is_refused() {
    assert!(
        DrivesFile::parse("{\"format\":\"something.else\",\"version\":1,\"drives\":[]}").is_err()
    );
    assert!(
        DrivesFile::parse("{\"format\":\"azul-storage.drives\",\"version\":99,\"drives\":[]}")
            .is_err()
    );
    assert!(DrivesFile::parse("not json").is_err());
    assert!(
        DrivesFile::parse("{\"format\":\"azul-storage.drives\",\"version\":1,\"drives\":[]}")
            .is_ok()
    );
}

#[test]
fn an_access_link_drive_reads_back_with_its_grant() {
    let entry = DriveEntry {
        id: "shared".to_string(),
        name: "Meeting notes".to_string(),
        location: DriveLocation::S3 {
            endpoint: "https://example.r2.cloudflarestorage.com".to_string(),
            region: "auto".to_string(),
            bucket: "azlin".to_string(),
            path_style: true,
            auth: DriveAuth::AccessLink {
                link: "azlin://drive/grant/abc".to_string(),
                prefix: "meetings/0192/".to_string(),
                can_write: false,
            },
        },
    };
    let mut file = DrivesFile::empty();
    file.add(entry.clone());
    let back = DrivesFile::parse(&file.to_json()).unwrap();
    assert_eq!(back.drives, vec![entry]);
}

#[test]
fn the_drives_file_goes_to_the_variable_or_the_config_folder() {
    assert_eq!(
        drives_file(Some("/tmp/x/drives.json"), Some(PathBuf::from("/cfg"))),
        Some(PathBuf::from("/tmp/x/drives.json"))
    );
    assert_eq!(
        drives_file(None, Some(PathBuf::from("/cfg"))),
        Some(
            PathBuf::from("/cfg")
                .join("azul-storage")
                .join("drives.json")
        )
    );
    assert_eq!(
        drives_file(Some(""), Some(PathBuf::from("/cfg"))),
        Some(
            PathBuf::from("/cfg")
                .join("azul-storage")
                .join("drives.json")
        )
    );
    assert_eq!(drives_file(None, None), None);
}

#[test]
fn every_drive_has_its_own_keyring_key_and_id() {
    assert_eq!(keyring_key("d1"), "azul-storage/s3/d1");
    assert_ne!(keyring_key("d1"), keyring_key("d2"));
    let a = new_drive_id("S3 Drive");
    let b = new_drive_id("S3 Drive");
    assert_ne!(a, b);
    assert!(
        a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "{a}"
    );
}

#[test]
fn a_local_drive_entry_opens_a_local_drive() {
    let tmp = TempDir::new("config-open-local");
    std::fs::write(tmp.path().join("a.txt"), b"x").unwrap();
    let entry = DriveEntry {
        id: "home".to_string(),
        name: "Home".to_string(),
        location: DriveLocation::Local {
            root: tmp.path().to_string_lossy().into_owned(),
        },
    };
    let drive = entry.open(None, Box::new(NoNetwork)).unwrap();
    let page = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(page.objects[0].key, "a.txt");
}

#[test]
fn an_s3_drive_entry_needs_its_credentials() {
    assert!(matches!(
        s3_entry("d1").open(None, Box::new(NoNetwork)),
        Err(DriveError::Denied { .. })
    ));
    assert!(s3_entry("d1")
        .open(
            Some(Credentials::new("AKID", "secret")),
            Box::new(NoNetwork)
        )
        .is_ok());
}

#[test]
fn an_access_link_drive_is_not_opened_yet() {
    let mut entry = s3_entry("shared");
    if let DriveLocation::S3 { auth, .. } = &mut entry.location {
        *auth = DriveAuth::AccessLink {
            link: "azlin://drive/grant/abc".to_string(),
            prefix: "meetings/0192/".to_string(),
            can_write: false,
        };
    }
    assert!(matches!(
        entry.open(None, Box::new(NoNetwork)),
        Err(DriveError::Unsupported(_))
    ));
}

// ==== The data sources of the Add drive dialog: Azlin, OpenDAL, databases ====

/// A drives file as a newer AzDrive writes it: an Azlin drive (the token server's bundle), a
/// WebDAV source through OpenDAL and a PostgreSQL database.
const NEWER_FILE: &str = r#"{
  "format": "azul-storage.drives",
  "version": 1,
  "drives": [
    { "id": "d_k3f9", "name": "Azlin Storage",
      "location": { "kind": "s3", "endpoint": "http://127.0.0.1:9000", "region": "us-east-1",
                    "bucket": "d-k3f9", "path_style": true,
                    "auth": { "type": "azlin", "drive_id": "d_k3f9",
                              "account_url": "http://127.0.0.1:8081" } } },
    { "id": "nas-1", "name": "NAS",
      "location": { "kind": "opendal", "scheme": "webdav",
                    "options": { "endpoint": "https://nas.example/dav", "username": "ann" },
                    "keyring": true } },
    { "id": "shop-1", "name": "Shop",
      "location": { "kind": "database", "engine": "postgres",
                    "options": { "host": "db.example", "port": "5432", "database": "shop",
                                 "user": "ann" },
                    "keyring": true } }
  ]
}"#;

fn options(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn an_azlin_drive_of_the_token_servers_bundle_reads_and_round_trips() {
    let file = DrivesFile::parse(NEWER_FILE).unwrap();
    let azlin = file.get("d_k3f9").unwrap();
    match &azlin.location {
        DriveLocation::S3 { auth, bucket, .. } => {
            assert_eq!(bucket, "d-k3f9");
            assert_eq!(
                auth,
                &DriveAuth::Azlin {
                    drive_id: "d_k3f9".to_string(),
                    account_url: "http://127.0.0.1:8081".to_string(),
                }
            );
        }
        other => panic!("not an S3 drive: {other:?}"),
    }
    assert!(azlin.needs_keyring(), "the session is in the keyring");
    let back = DrivesFile::parse(&file.to_json()).unwrap();
    assert_eq!(back, file);
}

#[test]
fn an_azlin_drive_without_its_token_servers_address_still_reads() {
    let text = r#"{ "format": "azul-storage.drives", "version": 1, "drives": [
      { "id": "d_k3f9", "name": "Azlin Storage",
        "location": { "kind": "s3", "endpoint": "http://127.0.0.1:9000", "region": "us-east-1",
                      "bucket": "d-k3f9", "path_style": true,
                      "auth": { "type": "azlin", "drive_id": "d_k3f9" } } } ] }"#;
    let file = DrivesFile::parse(text).unwrap();
    match &file.get("d_k3f9").unwrap().location {
        DriveLocation::S3 {
            auth: DriveAuth::Azlin { account_url, .. },
            ..
        } => assert_eq!(account_url, ""),
        other => panic!("not an Azlin drive: {other:?}"),
    }
}

#[test]
fn an_opendal_source_keeps_only_its_plain_settings_in_the_drives_file() {
    let file = DrivesFile::parse(NEWER_FILE).unwrap();
    let nas = file.get("nas-1").unwrap();
    assert_eq!(
        nas.location,
        DriveLocation::Opendal {
            scheme: "webdav".to_string(),
            options: options(&[("endpoint", "https://nas.example/dav"), ("username", "ann")]),
            keyring: true,
        }
    );
    assert!(nas.needs_keyring());
    let text = file.to_json();
    assert!(text.contains("\"kind\": \"opendal\""), "{text}");
    assert!(text.contains("\"scheme\": \"webdav\""), "{text}");
}

#[test]
fn a_database_source_names_its_engine() {
    let file = DrivesFile::parse(NEWER_FILE).unwrap();
    let shop = file.get("shop-1").unwrap();
    assert_eq!(
        shop.location,
        DriveLocation::Database {
            engine: DatabaseEngine::Postgres,
            options: options(&[
                ("database", "shop"),
                ("host", "db.example"),
                ("port", "5432"),
                ("user", "ann"),
            ]),
            keyring: true,
        }
    );
    let text = file.to_json();
    assert!(text.contains("\"kind\": \"database\""), "{text}");
    assert!(text.contains("\"engine\": \"postgres\""), "{text}");
}

#[test]
fn a_source_without_secrets_needs_no_keyring_and_says_none_in_its_file() {
    let entry = DriveEntry {
        id: "db".to_string(),
        name: "Local database".to_string(),
        location: DriveLocation::Database {
            engine: DatabaseEngine::Sqlite,
            options: options(&[("path", "/tmp/shop.sqlite")]),
            keyring: false,
        },
    };
    assert!(!entry.needs_keyring());
    let mut file = DrivesFile::empty();
    file.add(entry.clone());
    let text = file.to_json();
    assert!(!text.contains("keyring"), "no keyring flag when there is none: {text}");
    assert_eq!(DrivesFile::parse(&text).unwrap().drives, vec![entry]);
}

#[test]
fn secret_options_round_trip_through_the_keyring_text_and_never_show_in_debug() {
    let mut secrets = SecretOptions::new();
    secrets.insert("password", "sesame-42");
    secrets.insert("token", "tok-9");
    assert_eq!(secrets.get("password"), Some("sesame-42"));
    let text = secrets.to_keyring_secret();
    let back = SecretOptions::from_keyring_secret(&text).unwrap();
    assert_eq!(back, secrets);
    let debug = format!("{secrets:?}");
    assert!(!debug.contains("sesame-42") && !debug.contains("tok-9"), "{debug}");
    assert!(debug.contains("password"), "the names show, not the values: {debug}");
    assert!(SecretOptions::from_keyring_secret("not json").is_err());
    assert!(SecretOptions::new().is_empty());
}

#[test]
fn an_azlin_session_in_the_keyring_still_reads_as_plain_credentials() {
    // What azcloud-kit stores for an Azlin drive: the credentials and more.
    let session = r#"{"drive_id":"d_k3f9","drive_token":"dt_f.1.x","access_key_id":"AKID",
        "secret_access_key":"secret","session_token":"st","expires_at":1791450900}"#;
    let credentials = Credentials::from_keyring_secret(session).unwrap();
    assert_eq!(
        credentials,
        Credentials::new("AKID", "secret").with_session_token("st")
    );
}

#[test]
fn open_with_secret_reads_an_s3_drives_credentials_from_the_keyring_text() {
    let secret = Credentials::new("AKID", "secret").to_keyring_secret();
    assert!(s3_entry("d1")
        .open_with_secret(Some(&secret), Box::new(NoNetwork))
        .is_ok());
    assert!(matches!(
        s3_entry("d1").open_with_secret(None, Box::new(NoNetwork)),
        Err(DriveError::Denied { .. })
    ));
}

#[test]
fn open_with_secret_opens_a_local_drive_without_one() {
    let tmp = TempDir::new("config-open-secret-local");
    std::fs::write(tmp.path().join("b.txt"), b"y").unwrap();
    let entry = DriveEntry {
        id: "folder".to_string(),
        name: "Folder".to_string(),
        location: DriveLocation::Local {
            root: tmp.path().to_string_lossy().into_owned(),
        },
    };
    let drive = entry.open_with_secret(None, Box::new(NoNetwork)).unwrap();
    assert_eq!(
        drive.list(&ListRequest::folder("")).unwrap().objects[0].key,
        "b.txt"
    );
}

#[cfg(not(feature = "opendal"))]
#[test]
fn an_opendal_source_in_a_build_without_opendal_says_which_feature_it_needs() {
    let file = DrivesFile::parse(NEWER_FILE).unwrap();
    let secret = SecretOptions::new().to_keyring_secret();
    match file
        .get("nas-1")
        .unwrap()
        .open_with_secret(Some(&secret), Box::new(NoNetwork))
    {
        Err(DriveError::Unsupported(why)) => assert!(why.contains("opendal"), "{why}"),
        Err(other) => panic!("another error: {other}"),
        Ok(_) => panic!("opened without OpenDAL"),
    }
}

#[cfg(not(feature = "sql"))]
#[test]
fn a_database_in_a_build_without_sql_says_which_feature_it_needs() {
    let file = DrivesFile::parse(NEWER_FILE).unwrap();
    let secret = SecretOptions::new().to_keyring_secret();
    match file
        .get("shop-1")
        .unwrap()
        .open_with_secret(Some(&secret), Box::new(NoNetwork))
    {
        Err(DriveError::Unsupported(why)) => assert!(why.contains("sql"), "{why}"),
        Err(other) => panic!("another error: {other}"),
        Ok(_) => panic!("opened without the database drivers"),
    }
}

#[test]
fn every_database_engine_has_a_name_and_a_key() {
    for (engine, key, name) in [
        (DatabaseEngine::Sqlite, "sqlite", "SQLite"),
        (DatabaseEngine::Postgres, "postgres", "PostgreSQL"),
        (DatabaseEngine::Mysql, "mysql", "MySQL"),
    ] {
        assert_eq!(engine.key(), key);
        assert_eq!(engine.name(), name);
    }
}
