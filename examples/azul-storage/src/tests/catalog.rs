//! The data sources the Add drive dialog offers: their groups, their forms, and what a filled
//! form becomes (a drives-file entry without secrets, a keyring text with them).

use std::collections::{BTreeMap, HashSet};

use crate::{
    catalog::{self, Backend, FieldKind, FormValues, ServiceGroup},
    config::{DatabaseEngine, DriveAuth, DriveEntry, DriveLocation, SecretOptions},
    Credentials,
};

fn values(pairs: &[(&str, &str)]) -> FormValues {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn spec(id: &str) -> &'static catalog::ServiceSpec {
    catalog::service(id).unwrap_or_else(|| panic!("no service {id}"))
}

#[test]
fn every_service_has_a_unique_id_and_its_fields_unique_keys() {
    let mut ids = HashSet::new();
    for service in catalog::services() {
        assert!(ids.insert(service.id), "two services are {}", service.id);
        assert!(!service.name.is_empty() && !service.summary.is_empty(), "{}", service.id);
        assert!(
            service
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "the id names a DOM id: {}",
            service.id
        );
        let mut keys = HashSet::new();
        for field in service.fields {
            assert!(keys.insert(field.key), "{} has two fields {}", service.id, field.key);
            assert!(!field.label.is_empty(), "{}.{}", service.id, field.key);
            assert_ne!(field.key, "name", "the drive's name is the form's own field");
        }
    }
}

#[test]
fn the_groups_are_the_five_of_the_dialog_and_each_has_services() {
    let titles: Vec<&str> = ServiceGroup::ALL.iter().map(|g| g.title()).collect();
    assert_eq!(
        titles,
        vec![
            "Cloud object storage",
            "Network & NAS",
            "Consumer clouds",
            "Developer",
            "Databases & key-value",
        ]
    );
    for group in ServiceGroup::ALL {
        assert!(
            catalog::services().iter().any(|s| s.group == group),
            "{} is empty",
            group.title()
        );
    }
}

#[test]
fn s3_storage_and_a_folder_on_this_computer_are_in_every_build() {
    assert_eq!(spec("s3").backend, Backend::S3);
    assert_eq!(spec("local").backend, Backend::Local);
    assert!(spec("s3").available() && spec("local").available());
    assert_eq!(spec("s3").unavailable_reason(), None);
}

#[test]
fn opendal_sources_are_offered_as_their_feature_was_built() {
    for id in ["webdav", "gdrive", "dropbox", "onedrive", "github", "gcs", "azblob", "ftp"] {
        let service = spec(id);
        assert!(matches!(service.backend, Backend::Opendal(_)), "{id}");
        assert_eq!(service.available(), cfg!(feature = "opendal"), "{id}");
        assert_eq!(service.unavailable_reason().is_some(), !cfg!(feature = "opendal"), "{id}");
    }
    assert_eq!(spec("webdav").backend, Backend::Opendal("webdav"));
    assert_eq!(spec("gdrive").backend, Backend::Opendal("gdrive"));
}

#[test]
fn databases_are_browsed_as_tables_with_the_sql_feature() {
    assert_eq!(spec("postgres").backend, Backend::Database(DatabaseEngine::Postgres));
    assert_eq!(spec("mysql").backend, Backend::Database(DatabaseEngine::Mysql));
    assert_eq!(spec("sqlite").backend, Backend::Database(DatabaseEngine::Sqlite));
    for id in ["postgres", "mysql", "sqlite"] {
        assert_eq!(spec(id).group, ServiceGroup::Databases);
        assert_eq!(spec(id).available(), cfg!(feature = "sql"), "{id}");
        assert!(spec(id).read_only, "a database is browsed, not written: {id}");
    }
}

#[test]
fn a_form_names_what_is_missing_first_the_drives_name() {
    let webdav = spec("webdav");
    assert_eq!(
        catalog::check(webdav, "  ", &values(&[("endpoint", "https://nas/dav")])).unwrap_err(),
        "Give the drive a name."
    );
    let missing = catalog::check(webdav, "NAS", &values(&[])).unwrap_err();
    assert!(missing.contains("Server address"), "{missing}");
    assert!(catalog::check(webdav, "NAS", &values(&[("endpoint", "https://nas/dav")])).is_ok());
}

#[test]
fn addresses_numbers_and_choices_are_checked() {
    let postgres = spec("postgres");
    let good = values(&[
        ("host", "db.example"),
        ("port", "5432"),
        ("database", "shop"),
        ("user", "ann"),
        ("sslmode", "prefer"),
    ]);
    assert!(catalog::check(postgres, "Shop", &good).is_ok());
    let mut bad_port = good.clone();
    bad_port.insert("port".to_string(), "54x".to_string());
    assert!(catalog::check(postgres, "Shop", &bad_port).unwrap_err().contains("Port"));
    let mut bad_mode = good.clone();
    bad_mode.insert("sslmode".to_string(), "sometimes".to_string());
    assert!(catalog::check(postgres, "Shop", &bad_mode).is_err());
    let webdav = spec("webdav");
    let not_an_address = catalog::check(webdav, "NAS", &values(&[("endpoint", "nas.local")]));
    assert!(not_an_address.unwrap_err().contains("://"));
    assert!(matches!(
        webdav.field("endpoint").unwrap().kind,
        FieldKind::Url
    ));
    assert!(matches!(
        webdav.field("password").unwrap().kind,
        FieldKind::Secret
    ));
}

#[test]
fn an_s3_form_is_a_keyring_drive_and_its_credentials() {
    let form = values(&[
        ("endpoint", "http://127.0.0.1:9000"),
        ("region", ""),
        ("bucket", "photos"),
        ("access_key_id", "AKIDTEST"),
        ("secret_access_key", "test-secret"),
        ("path_style", "true"),
    ]);
    let new = catalog::build_entry(spec("s3"), "photos-1", "Photos", &form).unwrap();
    assert_eq!(new.entry.id, "photos-1");
    assert_eq!(new.entry.name, "Photos");
    assert_eq!(
        new.entry.location,
        DriveLocation::S3 {
            endpoint: "http://127.0.0.1:9000".to_string(),
            region: "us-east-1".to_string(),
            bucket: "photos".to_string(),
            path_style: true,
            auth: DriveAuth::Keyring,
        }
    );
    let secret = new.secret.as_deref().expect("the keys go to the keyring");
    assert_eq!(
        Credentials::from_keyring_secret(secret).unwrap(),
        Credentials::new("AKIDTEST", "test-secret")
    );
    let debug = format!("{new:?}");
    assert!(!debug.contains("test-secret") && !debug.contains("AKIDTEST"), "{debug}");
}

#[test]
fn an_s3_form_with_a_bucket_name_no_url_can_hold_is_refused() {
    let form = values(&[
        ("endpoint", "https://s3.amazonaws.com"),
        ("bucket", "My Bucket"),
        ("access_key_id", "AKID"),
        ("secret_access_key", "secret"),
        ("path_style", "false"),
    ]);
    assert!(catalog::build_entry(spec("s3"), "x", "X", &form).is_err());
}

#[test]
fn a_webdav_form_keeps_its_password_out_of_the_drives_file() {
    let form = values(&[
        ("endpoint", "https://nas.example/dav"),
        ("root", "/photos"),
        ("username", "ann"),
        ("password", "sesame-42"),
        ("token", ""),
    ]);
    let new = catalog::build_entry(spec("webdav"), "nas-1", "NAS", &form).unwrap();
    let expected: BTreeMap<String, String> = values(&[
        ("endpoint", "https://nas.example/dav"),
        ("root", "/photos"),
        ("username", "ann"),
    ]);
    assert_eq!(
        new.entry.location,
        DriveLocation::Opendal {
            scheme: "webdav".to_string(),
            options: expected,
            keyring: true,
        }
    );
    let secrets = SecretOptions::from_keyring_secret(new.secret.as_deref().unwrap()).unwrap();
    assert_eq!(secrets.get("password"), Some("sesame-42"));
    assert_eq!(secrets.get("token"), None, "an empty secret is no secret");
}

#[test]
fn a_source_without_secrets_needs_no_keyring_entry() {
    let new = catalog::build_entry(
        spec("webdav"),
        "pub-1",
        "Public",
        &values(&[("endpoint", "https://dav.example")]),
    )
    .unwrap();
    assert!(new.secret.is_none());
    assert!(!new.entry.needs_keyring());
}

#[test]
fn a_sqlite_form_is_a_database_drive_on_a_file() {
    let new = catalog::build_entry(
        spec("sqlite"),
        "db-1",
        "Shop",
        &values(&[("path", "/tmp/shop.sqlite")]),
    )
    .unwrap();
    assert_eq!(
        new.entry.location,
        DriveLocation::Database {
            engine: DatabaseEngine::Sqlite,
            options: values(&[("path", "/tmp/shop.sqlite")]),
            keyring: false,
        }
    );
    assert!(new.secret.is_none());
}

#[test]
fn a_postgres_form_keeps_its_password_in_the_keyring() {
    let new = catalog::build_entry(
        spec("postgres"),
        "shop-1",
        "Shop",
        &values(&[
            ("host", "db.example"),
            ("port", "5432"),
            ("database", "shop"),
            ("user", "ann"),
            ("password", "pg-secret"),
            ("sslmode", "require"),
        ]),
    )
    .unwrap();
    match &new.entry.location {
        DriveLocation::Database {
            engine,
            options,
            keyring,
        } => {
            assert_eq!(*engine, DatabaseEngine::Postgres);
            assert!(*keyring);
            assert_eq!(options.get("sslmode").map(String::as_str), Some("require"));
            assert!(!options.contains_key("password"));
        }
        other => panic!("not a database: {other:?}"),
    }
    assert!(new.secret.unwrap().contains("pg-secret"));
}

#[test]
fn the_defaults_fill_a_new_form() {
    let defaults = spec("postgres").defaults();
    assert_eq!(defaults.get("port").map(String::as_str), Some("5432"));
    assert_eq!(defaults.get("sslmode").map(String::as_str), Some("prefer"));
    assert_eq!(
        spec("s3").defaults().get("path_style").map(String::as_str),
        Some("true")
    );
}

#[test]
fn an_entry_fills_its_form_again_without_its_secrets() {
    let new = catalog::build_entry(
        spec("webdav"),
        "nas-1",
        "NAS",
        &values(&[
            ("endpoint", "https://nas.example/dav"),
            ("username", "ann"),
            ("password", "sesame-42"),
        ]),
    )
    .unwrap();
    let (service, form) = catalog::form_values(&new.entry);
    assert_eq!(service.map(|s| s.id), Some("webdav"));
    assert_eq!(form.get("username").map(String::as_str), Some("ann"));
    assert!(!form.contains_key("password"));
    let s3 = DriveEntry {
        id: "s".to_string(),
        name: "S".to_string(),
        location: DriveLocation::S3 {
            endpoint: "https://e".to_string(),
            region: "eu-central-1".to_string(),
            bucket: "b".to_string(),
            path_style: false,
            auth: DriveAuth::Keyring,
        },
    };
    let (service, form) = catalog::form_values(&s3);
    assert_eq!(service.map(|s| s.id), Some("s3"));
    assert_eq!(form.get("bucket").map(String::as_str), Some("b"));
    assert_eq!(form.get("path_style").map(String::as_str), Some("false"));
}

#[test]
fn a_drive_reads_as_the_kind_of_source_it_is() {
    let entry = |location| DriveEntry {
        id: "x".to_string(),
        name: "X".to_string(),
        location,
    };
    assert_eq!(
        catalog::kind_label(&entry(DriveLocation::Local {
            root: "/".to_string()
        })),
        "Local Disk"
    );
    let s3 = |auth| DriveLocation::S3 {
        endpoint: "https://e".to_string(),
        region: "auto".to_string(),
        bucket: "b".to_string(),
        path_style: true,
        auth,
    };
    assert_eq!(catalog::kind_label(&entry(s3(DriveAuth::Keyring))), "S3 bucket");
    assert_eq!(
        catalog::kind_label(&entry(s3(DriveAuth::Azlin {
            drive_id: "d_1".to_string(),
            account_url: String::new(),
        }))),
        "Azlin cloud drive"
    );
    assert_eq!(
        catalog::kind_label(&entry(DriveLocation::Opendal {
            scheme: "webdav".to_string(),
            options: BTreeMap::new(),
            keyring: false,
        })),
        "WebDAV"
    );
    assert_eq!(
        catalog::kind_label(&entry(DriveLocation::Database {
            engine: DatabaseEngine::Sqlite,
            options: BTreeMap::new(),
            keyring: false,
        })),
        "SQLite database"
    );
}
