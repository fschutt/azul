//! A SQLite file browsed as files (feature `sql`): its tables are folders, every row a JSON
//! file named by its key, a table's rows a CSV file - read-only. The test makes the file with
//! its own writable connection, then opens it as a drive.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use super::TempDir;
use crate::{
    config::{DatabaseEngine, SecretOptions},
    runtime, ByteRange, DatabaseDrive, Drive, DriveError, ListRequest, ObjectInfo,
};

/// Two tables with a primary key (a number, a text holding a slash) and one without.
const SCRIPT: &[&str] = &[
    "CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT NOT NULL, city TEXT)",
    "INSERT INTO customers (id, name, city) VALUES (1, 'Ada', 'London'), (2, 'Grace', \
     'New York'), (3, 'Linus, Jr.', NULL)",
    "CREATE TABLE orders (order_no TEXT PRIMARY KEY, customer INTEGER, total REAL, note BLOB)",
    "INSERT INTO orders (order_no, customer, total, note) VALUES ('A-1', 1, 9.5, X'CAFE'), \
     ('B/2', 2, 20.0, NULL)",
    "CREATE TABLE log (at TEXT, message TEXT)",
    "INSERT INTO log (at, message) VALUES ('2026-10-08', 'started'), ('2026-10-09', 'stopped')",
];

/// The database file, made and filled with a writable connection of the test's own.
fn database(dir: &TempDir) -> PathBuf {
    let path = dir.path().join("shop.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    runtime::block_on(async {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("a new database file");
        for statement in SCRIPT {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect(statement);
        }
        pool.close().await;
    })
    .expect("the storage runtime");
    path
}

fn open(path: &Path) -> DatabaseDrive {
    let mut options = BTreeMap::new();
    options.insert("path".to_string(), path.to_string_lossy().into_owned());
    DatabaseDrive::open(DatabaseEngine::Sqlite, &options, &SecretOptions::new())
        .expect("the database opens")
}

fn keys(objects: &[ObjectInfo]) -> Vec<&str> {
    objects.iter().map(|o| o.key.as_str()).collect()
}

fn json(drive: &DatabaseDrive, key: &str) -> serde_json::Value {
    let bytes = drive.get(key).unwrap_or_else(|e| panic!("{key}: {e}"));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{key} is not JSON: {e}"))
}

#[test]
fn the_tables_of_a_sqlite_file_are_its_folders() {
    let dir = TempDir::new("db-tables");
    let drive = open(&database(&dir));
    let page = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(page.folders, vec!["customers/", "log/", "orders/"]);
    assert!(page.objects.is_empty());
    assert_eq!(page.next, None);
}

#[test]
fn a_table_folder_holds_its_csv_its_schema_and_its_rows_folder() {
    let dir = TempDir::new("db-table-folder");
    let drive = open(&database(&dir));
    let page = drive.list(&ListRequest::folder("customers/")).unwrap();
    assert_eq!(page.folders, vec!["customers/rows/"]);
    assert_eq!(
        keys(&page.objects),
        vec!["customers/customers.csv", "customers/schema.json"]
    );
    for object in &page.objects {
        assert_eq!(
            object.size,
            drive.get(&object.key).unwrap().len() as u64,
            "{}",
            object.key
        );
    }
}

#[test]
fn every_row_is_a_json_file_named_by_its_primary_key() {
    let dir = TempDir::new("db-rows");
    let drive = open(&database(&dir));
    let page = drive.list(&ListRequest::folder("customers/rows/")).unwrap();
    assert_eq!(
        keys(&page.objects),
        vec![
            "customers/rows/1.json",
            "customers/rows/2.json",
            "customers/rows/3.json",
        ]
    );
    assert!(page.folders.is_empty());
    let grace = json(&drive, "customers/rows/2.json");
    assert_eq!(grace["id"], 2);
    assert_eq!(grace["name"], "Grace");
    assert_eq!(grace["city"], "New York");
    assert!(json(&drive, "customers/rows/3.json")["city"].is_null());
    for object in &page.objects {
        assert_eq!(object.size, drive.get(&object.key).unwrap().len() as u64);
        assert_eq!(object.modified, None);
    }
}

#[test]
fn a_text_key_with_a_slash_is_escaped_in_its_file_name_and_a_blob_reads_as_hex() {
    let dir = TempDir::new("db-text-key");
    let drive = open(&database(&dir));
    let page = drive.list(&ListRequest::folder("orders/rows/")).unwrap();
    assert_eq!(
        keys(&page.objects),
        vec!["orders/rows/A-1.json", "orders/rows/B%2F2.json"]
    );
    let first = json(&drive, "orders/rows/A-1.json");
    assert_eq!(first["order_no"], "A-1");
    assert_eq!(first["note"], "CAFE");
    assert_eq!(first["total"], 9.5);
    let second = json(&drive, "orders/rows/B%2F2.json");
    assert_eq!(second["order_no"], "B/2");
    assert!(second["note"].is_null());
}

#[test]
fn a_table_without_a_primary_key_names_its_rows_by_rowid() {
    let dir = TempDir::new("db-rowid");
    let drive = open(&database(&dir));
    let page = drive.list(&ListRequest::folder("log/rows/")).unwrap();
    assert_eq!(keys(&page.objects), vec!["log/rows/1.json", "log/rows/2.json"]);
    assert_eq!(json(&drive, "log/rows/2.json")["message"], "stopped");
}

#[test]
fn the_csv_has_a_header_and_a_line_per_row() {
    let dir = TempDir::new("db-csv");
    let drive = open(&database(&dir));
    let text = String::from_utf8(drive.get("customers/customers.csv").unwrap()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            "id,name,city",
            "1,Ada,London",
            "2,Grace,New York",
            "3,\"Linus, Jr.\",",
        ]
    );
}

#[test]
fn the_schema_names_the_columns_and_the_primary_key() {
    let dir = TempDir::new("db-schema");
    let drive = open(&database(&dir));
    let schema = json(&drive, "orders/schema.json");
    assert_eq!(schema["table"], "orders");
    assert_eq!(schema["primary_key"], serde_json::json!(["order_no"]));
    let names: Vec<&str> = schema["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["order_no", "customer", "total", "note"]);
}

#[test]
fn head_says_the_size_that_get_returns() {
    let dir = TempDir::new("db-head");
    let drive = open(&database(&dir));
    for key in [
        "customers/customers.csv",
        "customers/schema.json",
        "customers/rows/1.json",
        "orders/rows/B%2F2.json",
    ] {
        let info = drive.head(key).unwrap();
        assert_eq!(info.key, key);
        assert_eq!(info.size, drive.get(key).unwrap().len() as u64, "{key}");
    }
}

#[test]
fn a_range_of_a_row_file_is_a_slice_of_it() {
    let dir = TempDir::new("db-range");
    let drive = open(&database(&dir));
    let whole = drive.get("customers/rows/1.json").unwrap();
    let part = drive
        .get_range("customers/rows/1.json", ByteRange::new(0, Some(3)))
        .unwrap();
    assert_eq!(part, whole[..4].to_vec());
}

#[test]
fn a_database_drive_is_read_only() {
    let dir = TempDir::new("db-read-only");
    let drive = open(&database(&dir));
    let denied = |r: Result<(), DriveError>| matches!(r, Err(DriveError::Denied { .. }));
    assert!(denied(drive.put("customers/rows/9.json", b"{}")));
    assert!(denied(drive.delete("customers/rows/1.json")));
    assert!(denied(drive.copy("customers/rows/1.json", "customers/rows/9.json")));
    assert!(denied(drive.create_folder("new/")));
    assert!(denied(drive.rename("customers/", "clients/")));
    assert!(denied(drive.delete_folder("customers/")));
    assert_eq!(drive.list(&ListRequest::folder("customers/rows/")).unwrap().objects.len(), 3);
}

#[test]
fn a_missing_table_or_row_is_not_found() {
    let dir = TempDir::new("db-missing");
    let drive = open(&database(&dir));
    assert!(matches!(
        drive.list(&ListRequest::folder("nope/")),
        Err(DriveError::NotFound { .. })
    ));
    assert!(matches!(
        drive.get("nope/schema.json"),
        Err(DriveError::NotFound { .. })
    ));
    assert!(matches!(
        drive.get("customers/rows/99.json"),
        Err(DriveError::NotFound { .. })
    ));
    assert!(matches!(
        drive.get("customers/notes.txt"),
        Err(DriveError::NotFound { .. })
    ));
}

#[test]
fn a_recursive_listing_of_a_table_holds_every_file_and_no_folder() {
    let dir = TempDir::new("db-recursive");
    let drive = open(&database(&dir));
    let page = drive.list(&ListRequest::recursive("customers/")).unwrap();
    assert!(page.folders.is_empty());
    assert_eq!(
        keys(&page.objects),
        vec![
            "customers/customers.csv",
            "customers/rows/1.json",
            "customers/rows/2.json",
            "customers/rows/3.json",
            "customers/schema.json",
        ]
    );
}

#[test]
fn pages_of_rows_continue_where_the_last_one_ended() {
    let dir = TempDir::new("db-pages");
    let drive = open(&database(&dir));
    let first = drive
        .list(&ListRequest::folder("customers/rows/").with_max_keys(2))
        .unwrap();
    assert_eq!(
        keys(&first.objects),
        vec!["customers/rows/1.json", "customers/rows/2.json"]
    );
    let token = first.next.expect("a second page");
    let second = drive
        .list(
            &ListRequest::folder("customers/rows/")
                .with_max_keys(2)
                .with_continuation(token),
        )
        .unwrap();
    assert_eq!(keys(&second.objects), vec!["customers/rows/3.json"]);
    assert_eq!(second.next, None);
}

#[test]
fn a_missing_database_file_is_refused_when_it_opens() {
    let dir = TempDir::new("db-no-file");
    let mut options = BTreeMap::new();
    options.insert(
        "path".to_string(),
        dir.path().join("none.sqlite").to_string_lossy().into_owned(),
    );
    assert!(matches!(
        DatabaseDrive::open(DatabaseEngine::Sqlite, &options, &SecretOptions::new()),
        Err(DriveError::NotFound { .. })
    ));
    assert!(matches!(
        DatabaseDrive::open(DatabaseEngine::Sqlite, &BTreeMap::new(), &SecretOptions::new()),
        Err(DriveError::InvalidConfig(_))
    ));
}

#[test]
fn a_server_database_needs_its_host_database_and_user_and_never_shows_its_password() {
    let mut options = BTreeMap::new();
    options.insert("host".to_string(), "db.example".to_string());
    options.insert("database".to_string(), "shop".to_string());
    let mut secrets = SecretOptions::new();
    secrets.insert("password", "pg-sesame");
    assert!(matches!(
        DatabaseDrive::open(DatabaseEngine::Postgres, &options, &secrets),
        Err(DriveError::InvalidConfig(_))
    ));
    options.insert("user".to_string(), "ann".to_string());
    options.insert("port".to_string(), "54x".to_string());
    assert!(matches!(
        DatabaseDrive::open(DatabaseEngine::Postgres, &options, &secrets),
        Err(DriveError::InvalidConfig(_))
    ));
    options.insert("port".to_string(), "5432".to_string());
    options.insert("sslmode".to_string(), "require".to_string());
    // Opening connects to nothing yet.
    let drive = DatabaseDrive::open(DatabaseEngine::Postgres, &options, &secrets).unwrap();
    let debug = format!("{drive:?}");
    assert!(!debug.contains("pg-sesame"), "{debug}");
    options.insert("sslmode".to_string(), "sometimes".to_string());
    assert!(matches!(
        DatabaseDrive::open(DatabaseEngine::Mysql, &options, &secrets),
        Err(DriveError::InvalidConfig(_))
    ));
}
