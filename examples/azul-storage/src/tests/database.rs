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

/// sqlx always trusts its compiled-in Mozilla roots: under an app that pinned or distrusted
/// roots a PostgreSQL or MySQL database over TLS is refused when it opens (it would trust more
/// than the app does); without TLS, or under the public roots, it opens.
#[test]
fn a_server_database_over_tls_is_refused_when_the_app_narrowed_its_trusted_roots() {
    let mut options = BTreeMap::new();
    options.insert("host".to_string(), "db.example".to_string());
    options.insert("database".to_string(), "shop".to_string());
    options.insert("user".to_string(), "ann".to_string());
    let secrets = SecretOptions::new();
    for (engine, tls, plain) in [
        (DatabaseEngine::Postgres, "verify-full", "disable"),
        (DatabaseEngine::Mysql, "verify-identity", "disabled"),
    ] {
        options.insert("sslmode".to_string(), tls.to_string());
        match DatabaseDrive::open_trusting(engine, &options, &secrets, false) {
            Err(DriveError::InvalidConfig(why)) => assert!(why.contains("trusted roots"), "{why}"),
            other => panic!("{tls}: {:?}", other.map(|_| ())),
        }
        assert!(DatabaseDrive::open_trusting(engine, &options, &secrets, true).is_ok());
        options.insert("sslmode".to_string(), plain.to_string());
        assert!(DatabaseDrive::open_trusting(engine, &options, &secrets, false).is_ok());
    }
}

/// AZ-CN2: an entry without a TLS word (a hand-made drives file, an older build) opens the way the
/// form's default does - checking the server - and not as sqlx's own default (`prefer`: plain text
/// when the server says no TLS, any certificate, the password in clear text on request). A word the
/// entry has is kept, the opportunistic ones included: a stored drive keeps its value.
#[test]
fn az_cn2_a_server_database_without_a_tls_word_checks_the_server() {
    let mut options = BTreeMap::new();
    options.insert("host".to_string(), "db.example".to_string());
    options.insert("database".to_string(), "shop".to_string());
    options.insert("user".to_string(), "ann".to_string());
    let secrets = SecretOptions::new();
    for (engine, none, kept) in [
        (
            DatabaseEngine::Postgres,
            "VerifyFull",
            [
                ("prefer", "Prefer"),
                ("require", "Require"),
                ("disable", "Disable"),
            ],
        ),
        (
            DatabaseEngine::Mysql,
            "VerifyIdentity",
            [
                ("preferred", "Preferred"),
                ("required", "Required"),
                ("disabled", "Disabled"),
            ],
        ),
    ] {
        options.remove("sslmode");
        let drive = DatabaseDrive::open(engine, &options, &secrets).unwrap();
        assert_eq!(
            drive.tls_mode().as_deref(),
            Some(none),
            "{engine:?}: no word"
        );
        for (word, mode) in kept {
            options.insert("sslmode".to_string(), word.to_string());
            let drive = DatabaseDrive::open(engine, &options, &secrets).unwrap();
            assert_eq!(drive.tls_mode().as_deref(), Some(mode), "{engine:?} {word}");
        }
    }
    // A SQLite file has no connection.
    let dir = TempDir::new("az-cn2");
    let file = database(&dir);
    assert_eq!(open(&file).tls_mode(), None);
}

/// A TLS error of a connection that CHECKS the server says how to reach a server with a certificate
/// of its own; one that only encrypts, or none, has nothing to add.
#[test]
fn az_cn2_a_failed_check_of_the_servers_certificate_says_what_to_choose() {
    let mut options = BTreeMap::new();
    options.insert("host".to_string(), "db.example".to_string());
    options.insert("database".to_string(), "shop".to_string());
    options.insert("user".to_string(), "ann".to_string());
    let secrets = SecretOptions::new();
    let mut hint = |engine, word: Option<&str>| {
        match word {
            Some(word) => options.insert("sslmode".to_string(), word.to_string()),
            None => options.remove("sslmode"),
        };
        DatabaseDrive::open(engine, &options, &secrets)
            .unwrap()
            .tls_hint()
    };
    assert!(hint(DatabaseEngine::Postgres, None).contains("only encrypts"));
    assert!(hint(DatabaseEngine::Postgres, Some("verify-full")).contains("only encrypts"));
    assert!(hint(DatabaseEngine::Mysql, None).contains("only encrypts"));
    assert!(hint(DatabaseEngine::Mysql, Some("verify-ca")).contains("only encrypts"));
    assert_eq!(hint(DatabaseEngine::Postgres, Some("require")), "");
    assert_eq!(hint(DatabaseEngine::Mysql, Some("disabled")), "");
}

// ==== AZ-CN6: a table with more rows than a listing shows says so ====

/// A database file with a table of `big` rows (a number each), made with a writable connection of
/// the test's own.
fn database_with_rows(dir: &TempDir, tables: &[(&str, u32)]) -> PathBuf {
    let path = dir.path().join("rows.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    runtime::block_on(async {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("a new database file");
        for (name, rows) in tables {
            sqlx::query(&format!(
                "CREATE TABLE {name} (id INTEGER PRIMARY KEY, label TEXT)"
            ))
            .execute(&pool)
            .await
            .expect("a table");
            sqlx::query(&format!(
                "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {rows}) \
                 INSERT INTO {name} (id, label) SELECT x, 'row ' || x FROM c"
            ))
            .execute(&pool)
            .await
            .expect("its rows");
        }
        pool.close().await;
    })
    .expect("the storage runtime");
    path
}

/// Every object under `prefix` of the rows folder, page after page.
fn all_pages(drive: &DatabaseDrive, prefix: &str) -> Vec<ObjectInfo> {
    let mut found = Vec::new();
    let mut request = ListRequest::folder(prefix);
    loop {
        let page = drive.list(&request).unwrap();
        found.extend(page.objects);
        match page.next {
            Some(next) => request = ListRequest::folder(prefix).with_continuation(next),
            None => return found,
        }
    }
}

/// The review's case: a copy of `orders/rows/` (50,000 rows) or the CSV in a spreadsheet stopped at
/// 10,000 without a word. The table folder, the `rows/` folder and a recursive listing each carry
/// a note file when the table has more rows than they show, `schema.json` says it, and the note
/// reads. A table with exactly the cap has none of it.
#[test]
fn az_cn6_a_table_with_more_rows_than_the_cap_says_so_where_a_copy_looks() {
    let dir = TempDir::new("db-cap");
    let file = database_with_rows(&dir, &[("big", 10_001), ("exact", 10_000), ("small", 3)]);
    let drive = open(&file);
    let note = "_ONLY-THE-FIRST-10000-ROWS.txt";

    let table = drive.list(&ListRequest::folder("big/")).unwrap();
    assert_eq!(
        keys(&table.objects),
        vec!["big/big.csv", "big/schema.json", &format!("big/{note}")]
    );
    let rows = all_pages(&drive, "big/rows/");
    assert_eq!(rows.len(), 10_000 + 1, "the first 10000 rows and the note");
    assert_eq!(
        rows.last().map(|o| o.key.as_str()),
        Some(format!("big/rows/{note}").as_str()),
        "the note comes with the last page"
    );
    let text = String::from_utf8(drive.get(&format!("big/{note}")).unwrap()).unwrap();
    assert!(text.contains("has more than 10000 rows"), "{text}");
    assert_eq!(
        drive.get(&format!("big/rows/{note}")).unwrap(),
        text.as_bytes()
    );
    assert_eq!(
        drive.head(&format!("big/{note}")).unwrap().size,
        text.len() as u64
    );

    let everything = drive.list(&ListRequest::recursive("big/")).unwrap();
    let names = keys(&everything.objects);
    assert!(names.contains(&format!("big/{note}").as_str()), "{names:?}");
    assert!(
        names.contains(&format!("big/rows/{note}").as_str()),
        "{names:?}"
    );

    let schema: serde_json::Value =
        serde_json::from_slice(&drive.get("big/schema.json").unwrap()).unwrap();
    assert_eq!(schema["rows_cap"], 10_000);
    assert_eq!(schema["rows_truncated"], true);
    let csv = String::from_utf8(drive.get("big/big.csv").unwrap()).unwrap();
    assert_eq!(csv.lines().count(), 10_000 + 1, "a header and the cap");

    // The cap itself is no truncation, and a small table has no note anywhere.
    for table in ["exact", "small"] {
        let listed = drive
            .list(&ListRequest::folder(&format!("{table}/")))
            .unwrap();
        assert_eq!(
            listed.objects.len(),
            2,
            "{table}: its CSV and its schema: {:?}",
            keys(&listed.objects)
        );
        let rows = all_pages(&drive, &format!("{table}/rows/"));
        assert!(
            rows.iter().all(|o| !o.key.contains("_ONLY-THE-FIRST")),
            "{table}"
        );
        let schema: serde_json::Value =
            serde_json::from_slice(&drive.get(&format!("{table}/schema.json")).unwrap()).unwrap();
        assert_eq!(schema["rows_truncated"], false, "{table}");
        assert!(matches!(
            drive.get(&format!("{table}/{note}")),
            Err(DriveError::NotFound { .. })
        ));
    }
}

/// The drive's metadata for anything in a table names the limit (Properties shows it).
#[test]
fn az_cn6_the_metadata_of_a_table_item_names_the_row_limit() {
    let dir = TempDir::new("db-cap-meta");
    let drive = open(&database(&dir));
    let rows = drive.metadata("customers/schema.json").unwrap();
    assert!(
        rows.contains(&("Row limit".to_string(), "10000".to_string())),
        "{rows:?}"
    );
}
