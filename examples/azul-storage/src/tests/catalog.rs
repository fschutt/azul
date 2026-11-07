//! The data sources the Add drive dialog offers: their groups, their forms, and what a filled
//! form becomes (a drives-file entry without secrets, a keyring text with them).

use std::collections::{BTreeMap, HashSet};

use crate::{
    catalog::{self, Backend, DriveKind, FieldKind, FormProblem, FormValues, ServiceGroup},
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
    // AZ-CN2: the TLS word a new form holds VERIFIES the server (the old pin was "prefer": plain
    // text when the server says no TLS, any certificate, the password in clear text on request).
    assert_eq!(
        defaults.get("sslmode").map(String::as_str),
        Some("verify-full")
    );
    assert_eq!(
        spec("mysql").defaults().get("sslmode").map(String::as_str),
        Some("verify-identity")
    );
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
            encrypted: false,
            key_id: None,
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

/// What a form lacks, as data: an app says it in its own language (`check` says it in English).
#[test]
fn what_a_form_lacks_is_data_for_an_app_that_says_it_in_its_language() {
    let webdav = spec("webdav");
    let endpoint = webdav.field("endpoint").unwrap();
    assert_eq!(
        catalog::problem(webdav, "  ", &values(&[])),
        Some(FormProblem::NoName)
    );
    assert_eq!(
        catalog::problem(webdav, "NAS", &values(&[])),
        Some(FormProblem::Required(endpoint))
    );
    assert_eq!(
        catalog::problem(webdav, "NAS", &values(&[("endpoint", "nas.local")])),
        Some(FormProblem::NotAnAddress(endpoint))
    );
    assert_eq!(FormProblem::NotAnAddress(endpoint).scheme(), "https");
    assert_eq!(
        catalog::problem(webdav, "NAS", &values(&[("endpoint", "https://nas/dav")])),
        None
    );
    let postgres = spec("postgres");
    let bad_port = values(&[
        ("host", "db.example"),
        ("port", "54x"),
        ("database", "shop"),
        ("user", "ann"),
    ]);
    assert_eq!(
        catalog::problem(postgres, "Shop", &bad_port),
        Some(FormProblem::NotANumber(postgres.field("port").unwrap()))
    );
    // check's words are the problem's.
    assert_eq!(
        catalog::check(webdav, "NAS", &values(&[])).unwrap_err(),
        FormProblem::Required(endpoint).to_string()
    );
}

/// What kind of drive it is, as data; `kind_label` is its English.
#[test]
fn a_drives_kind_is_data_too() {
    let entry = |location| DriveEntry {
        id: "x".to_string(),
        name: "X".to_string(),
        location,
    };
    let local = entry(DriveLocation::Local {
        root: "/".to_string(),
    });
    assert_eq!(catalog::kind_of(&local), DriveKind::LocalDisk);
    assert_eq!(
        catalog::kind_of(&local).to_string(),
        catalog::kind_label(&local)
    );
    let opendal = |scheme: &str| {
        entry(DriveLocation::Opendal {
            scheme: scheme.to_string(),
            options: BTreeMap::new(),
            keyring: false,
        })
    };
    assert_eq!(
        catalog::kind_of(&opendal("webdav")),
        DriveKind::Source(spec("webdav"))
    );
    assert_eq!(
        catalog::kind_of(&opendal("frob")),
        DriveKind::Scheme("frob".to_string())
    );
    assert_eq!(
        catalog::kind_of(&entry(DriveLocation::Database {
            engine: DatabaseEngine::Sqlite,
            options: BTreeMap::new(),
            keyring: false,
        })),
        DriveKind::Database(DatabaseEngine::Sqlite)
    );
}

// ==== AZ-CN4: what a source can do, asked before a command is offered ====

fn drive_of(location: DriveLocation) -> DriveEntry {
    DriveEntry {
        id: "x".to_string(),
        name: "X".to_string(),
        location,
    }
}

fn opendal_drive(scheme: &str) -> DriveEntry {
    drive_of(DriveLocation::Opendal {
        scheme: scheme.to_string(),
        options: BTreeMap::new(),
        keyring: false,
    })
}

/// "Sync with a folder" commits its index with a conditional write. The review's four examples
/// (Dropbox, Google Drive, Google Cloud Storage, Redis) were all offered the sync, and the folder
/// went up as plain files before the commit was refused: none of them can do a conditional write,
/// and only a bucket and a folder can.
#[test]
fn az_cn4_only_a_bucket_and_a_folder_can_commit_a_folder_sync() {
    for service in catalog::services() {
        let expected = matches!(service.backend, Backend::S3 | Backend::Local);
        assert_eq!(
            service.caps.conditional_writes, expected,
            "{}: conditional writes are the bucket's and the folder's only",
            service.id
        );
    }
    for id in [
        "dropbox", "gdrive", "onedrive", "gcs", "redis", "webdav", "ftp", "postgres",
    ] {
        assert!(
            !spec(id).caps.conditional_writes,
            "{id} has no conditional write"
        );
    }
    assert!(spec("s3").caps.conditional_writes);
}

/// The same answer for a drive that is already in the drives file, by the kind of drive it is
/// (an Azlin drive has no catalog form, so the entry decides, not the catalog).
#[test]
fn az_cn4_an_entry_answers_by_its_location() {
    let s3 = |auth| {
        drive_of(DriveLocation::S3 {
            endpoint: "https://e".to_string(),
            region: "eu-central-1".to_string(),
            bucket: "b".to_string(),
            path_style: false,
            auth,
        })
    };
    assert!(catalog::capabilities_of(&s3(DriveAuth::Keyring)).conditional_writes);
    assert!(
        catalog::capabilities_of(&s3(DriveAuth::Azlin {
            drive_id: "d_k3f9".to_string(),
            account_url: String::new(),
            encrypted: true,
            key_id: None,
        }))
        .conditional_writes,
        "an Azlin drive is a bucket"
    );
    assert!(
        !catalog::capabilities_of(&s3(DriveAuth::AccessLink {
            link: "l".to_string(),
            prefix: String::new(),
            can_write: false,
        }))
        .conditional_writes,
        "an access link opens once the server exists: nothing is known of it"
    );
    assert!(
        catalog::capabilities_of(&drive_of(DriveLocation::Local {
            root: "/tmp".to_string()
        }))
        .conditional_writes
    );
    for scheme in ["dropbox", "gdrive", "gcs", "redis", "webdav", "frob"] {
        assert!(
            !catalog::capabilities_of(&opendal_drive(scheme)).conditional_writes,
            "{scheme}: an OpenDAL source cannot commit a sync, a scheme the catalog does not list neither"
        );
    }
    let database = drive_of(DriveLocation::Database {
        engine: DatabaseEngine::Postgres,
        options: BTreeMap::new(),
        keyring: false,
    });
    assert!(!catalog::capabilities_of(&database).conditional_writes);
}

/// WebDAV is the one source that sends WebDAV's verbs; a database is the one with a row cap, the
/// cap the listing applies.
#[test]
fn az_cn5_az_cn6_the_catalog_says_which_source_needs_the_webdav_verbs_and_which_caps_its_rows() {
    for service in catalog::services() {
        assert_eq!(
            service.caps.webdav_verbs,
            service.id == "webdav",
            "{}: WebDAV's verbs are WebDAV's",
            service.id
        );
        let capped = matches!(service.backend, Backend::Database(_));
        assert_eq!(
            service.caps.rows_cap,
            capped.then_some(crate::tables::MAX_ROW_FILES),
            "{}: a database lists at most MAX_ROW_FILES rows of a table",
            service.id
        );
    }
    assert_eq!(
        crate::tables::CSV_MAX_ROWS,
        crate::tables::MAX_ROW_FILES,
        "one cap for both the rows/ folder and the CSV"
    );
}

// ==== AZ-CN2: a TLS choice holds the word that verifies the server ====

/// Every drop-down names the word a new form holds, and it is one of its words: "the first word
/// is the default" is how `prefer` became the default of a form that asks for a password.
#[test]
fn az_cn2_every_choice_names_its_default_and_it_is_one_of_its_words() {
    let mut choices = 0;
    for service in catalog::services() {
        for field in service.fields {
            let FieldKind::Choice(words) = field.kind else {
                continue;
            };
            choices += 1;
            assert!(
                !field.default.is_empty(),
                "{}.{} leaves its default to the first word",
                service.id,
                field.key
            );
            assert!(
                words.contains(&field.default),
                "{}.{}: {:?} is none of {words:?}",
                service.id,
                field.key,
                field.default
            );
        }
    }
    assert!(choices >= 2, "PostgreSQL and MySQL have one each");
}

/// The review's case: a PostgreSQL or MySQL form left untouched must check the server's
/// certificate and name, and the opportunistic words stay chosen-only, last in the list.
#[test]
fn az_cn2_the_tls_default_of_a_database_form_verifies_the_server() {
    for (id, secure, opportunistic) in [
        ("postgres", "verify-full", "prefer"),
        ("mysql", "verify-identity", "preferred"),
    ] {
        let field = spec(id).field("sslmode").expect("a TLS field");
        assert_eq!(field.default, secure, "{id}");
        assert_eq!(
            spec(id).defaults().get("sslmode").map(String::as_str),
            Some(secure),
            "{id}: a new form holds it"
        );
        let FieldKind::Choice(words) = field.kind else {
            panic!("{id}: the TLS field is a drop-down");
        };
        assert_eq!(
            words.first().copied(),
            Some(secure),
            "{id}: first in the list"
        );
        assert_eq!(
            words.last().copied(),
            Some(opportunistic),
            "{id}: the opportunistic word is chosen on purpose, last"
        );
        // A form the user did not touch is a form that verifies.
        let mut form = FormValues::new();
        form.insert("host".to_string(), "db.example".to_string());
        form.insert("database".to_string(), "shop".to_string());
        form.insert("user".to_string(), "ann".to_string());
        form.extend(spec(id).defaults());
        let new = catalog::build_entry(spec(id), "shop-1", "Shop", &form).expect("a good form");
        match new.entry.location {
            DriveLocation::Database { options, .. } => {
                assert_eq!(
                    options.get("sslmode").map(String::as_str),
                    Some(secure),
                    "{id}"
                );
            }
            other => panic!("{id}: not a database: {other:?}"),
        }
    }
}
