use std::path::PathBuf;

use super::TempDir;
use crate::{
    config::{
        drives_file, keyring_key, new_drive_id, DriveAuth, DriveEntry, DriveLocation, DrivesFile,
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
