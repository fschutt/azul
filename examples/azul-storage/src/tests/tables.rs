//! A database as files, as plain functions: the keys of its tables, rows, CSV and schema
//! files, the names of row files, the SQL each engine is asked, the CSV and schema text. No
//! database here (tests/database.rs opens a real SQLite file with the `sql` feature).

use serde_json::json;

use crate::{
    tables::{
        self, csv_text, decode_key_part, encode_key_part, folder_name, page_bounds,
        parse_row_name, row_name, schema_json, table_candidates, unescape_segment, Column,
        Dialect, RowKey, RowLookup, TablePath, TableRef,
    },
    ByteRange, DriveError,
};

fn table(schema: &str, name: &str) -> TableRef {
    TableRef {
        schema: schema.to_string(),
        name: name.to_string(),
        view: false,
    }
}

fn column(name: &str, data_type: &str, primary_key: u32) -> Column {
    Column {
        name: name.to_string(),
        data_type: data_type.to_string(),
        nullable: primary_key == 0,
        primary_key,
    }
}

fn customers() -> Vec<Column> {
    vec![
        column("id", "INTEGER", 1),
        column("name", "TEXT", 0),
        column("city", "TEXT", 0),
    ]
}

// ==== Keys ====

#[test]
fn every_kind_of_key_reads_back_as_the_path_it_names() {
    let folder = || "customers".to_string();
    let cases = [
        ("", TablePath::Root),
        ("customers/", TablePath::Table { folder: folder() }),
        ("customers/customers.csv", TablePath::Csv { folder: folder() }),
        ("customers/schema.json", TablePath::Schema { folder: folder() }),
        ("customers/rows/", TablePath::Rows { folder: folder() }),
        (
            "customers/rows/42.json",
            TablePath::Row {
                folder: folder(),
                name: "42".to_string(),
            },
        ),
    ];
    for (key, path) in cases {
        assert_eq!(TablePath::parse(key), Some(path.clone()), "{key}");
        assert_eq!(path.key(), key, "{path:?}");
    }
}

#[test]
fn a_key_that_names_nothing_of_a_database_is_none() {
    for key in [
        "customers",
        "/customers/",
        "customers/other.csv",
        "customers/rows",
        "customers/rows/42",
        "customers/rows/.json",
        "customers/rows/a/b.json",
        "customers/notes.txt",
    ] {
        assert_eq!(TablePath::parse(key), None, "{key}");
    }
    assert!(TablePath::parse("customers/rows/").unwrap().is_folder());
    assert!(!TablePath::parse("customers/schema.json").unwrap().is_folder());
}

#[test]
fn a_table_is_a_folder_named_after_it_and_a_slash_in_its_name_is_escaped() {
    assert_eq!(folder_name(Dialect::Sqlite, &table("", "customers")), "customers");
    assert_eq!(folder_name(Dialect::Mysql, &table("", "a/b%c")), "a%2Fb%25c");
    assert_eq!(unescape_segment("a%2Fb%25c").as_deref(), Some("a/b%c"));
    assert_eq!(unescape_segment("bad%zz"), None);
}

#[test]
fn a_postgres_table_outside_public_is_prefixed_with_its_schema() {
    assert_eq!(folder_name(Dialect::Postgres, &table("public", "orders")), "orders");
    assert_eq!(
        folder_name(Dialect::Postgres, &table("sales", "orders")),
        "sales.orders"
    );
    assert_eq!(
        table_candidates(Dialect::Postgres, "sales.orders"),
        Some(vec![
            ("public".to_string(), "sales.orders".to_string()),
            ("sales".to_string(), "orders".to_string()),
        ])
    );
    assert_eq!(
        table_candidates(Dialect::Postgres, "orders"),
        Some(vec![("public".to_string(), "orders".to_string())])
    );
    assert_eq!(
        table_candidates(Dialect::Sqlite, "sales.orders"),
        Some(vec![(String::new(), "sales.orders".to_string())])
    );
}

// ==== Row files ====

#[test]
fn a_rows_file_is_named_by_its_primary_key_and_read_back_from_its_name() {
    let key = RowKey::Columns(vec!["id".to_string()]);
    let name = row_name(&key, r#"["7"]"#, 1);
    assert_eq!(name, "7");
    assert_eq!(
        parse_row_name(&name, &key),
        Some(RowLookup::Key(vec![Some("7".to_string())]))
    );
}

#[test]
fn a_composite_key_joins_its_parts_and_escapes_what_a_file_name_cannot_hold() {
    let key = RowKey::Columns(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    let name = row_name(&key, r#"["x/y,z", null, ""]"#, 1);
    assert_eq!(name, "x%2Fy%2Cz,(null),(empty)");
    assert_eq!(
        parse_row_name(&name, &key),
        Some(RowLookup::Key(vec![
            Some("x/y,z".to_string()),
            None,
            Some(String::new()),
        ]))
    );
    assert_eq!(parse_row_name("x,y", &key), None, "three parts, not two");
}

#[test]
fn a_key_part_keeps_letters_and_spaces_and_hides_no_file() {
    assert_eq!(encode_key_part(Some("Müller & Söhne")), "Müller & Söhne");
    assert_eq!(encode_key_part(Some(".hidden")), "%2Ehidden");
    assert_eq!(encode_key_part(Some("(null)")), "%28null%29");
    assert_eq!(encode_key_part(None), "(null)");
    assert_eq!(encode_key_part(Some("")), "(empty)");
    assert_eq!(encode_key_part(Some("a:b*c?d")), "a%3Ab%2Ac%3Fd");
    for text in ["Müller & Söhne", ".hidden", "(null)", "a/b", "100%", "tab\there"] {
        assert_eq!(
            decode_key_part(&encode_key_part(Some(text))),
            Some(Some(text.to_string())),
            "{text}"
        );
    }
    assert_eq!(decode_key_part("(null)"), Some(None));
}

#[test]
fn rows_without_a_key_are_named_by_their_place() {
    assert_eq!(row_name(&RowKey::Position, "[]", 12), "row-000012");
    assert_eq!(
        parse_row_name("row-000012", &RowKey::Position),
        Some(RowLookup::Position(12))
    );
    assert_eq!(parse_row_name("row-0", &RowKey::Position), None);
    assert_eq!(parse_row_name("12", &RowKey::Position), None);
    assert_eq!(row_name(&RowKey::RowId, r#"["3"]"#, 1), "3");
    assert_eq!(
        parse_row_name("3", &RowKey::RowId),
        Some(RowLookup::Key(vec![Some("3".to_string())]))
    );
}

#[test]
fn a_tables_key_is_its_primary_key_else_sqlites_rowid_else_the_rows_place() {
    let mut columns = vec![
        column("b", "TEXT", 2),
        column("x", "TEXT", 0),
        column("a", "TEXT", 1),
    ];
    assert_eq!(
        RowKey::for_table(Dialect::Postgres, &columns, false),
        RowKey::Columns(vec!["a".to_string(), "b".to_string()])
    );
    for c in &mut columns {
        c.primary_key = 0;
    }
    assert_eq!(RowKey::for_table(Dialect::Sqlite, &columns, false), RowKey::RowId);
    assert_eq!(RowKey::for_table(Dialect::Sqlite, &columns, true), RowKey::Position);
    assert_eq!(RowKey::for_table(Dialect::Postgres, &columns, false), RowKey::Position);
    assert_eq!(RowKey::for_table(Dialect::Mysql, &columns, false), RowKey::Position);
}

// ==== SQL ====

#[test]
fn names_are_quoted_the_way_each_engine_reads_them() {
    assert_eq!(Dialect::Sqlite.quote_ident("a\"b"), "\"a\"\"b\"");
    assert_eq!(Dialect::Postgres.quote_ident("a\"b"), "\"a\"\"b\"");
    assert_eq!(Dialect::Mysql.quote_ident("a`b"), "`a``b`");
    assert_eq!(Dialect::Sqlite.quote_literal("it's"), "'it''s'");
    assert_eq!(Dialect::Mysql.quote_literal("a\\b'c"), "'a\\\\b''c'");
    assert_eq!(Dialect::Postgres.placeholder(2), "$2");
    assert_eq!(Dialect::Mysql.placeholder(2), "?");
    assert_eq!(
        Dialect::Postgres.table_sql(&table("sales", "orders")),
        "\"sales\".\"orders\""
    );
    assert_eq!(Dialect::Sqlite.table_sql(&table("", "orders")), "\"orders\"");
}

#[test]
fn sqlite_lists_a_page_of_rows_as_their_key_and_their_json() {
    let key = RowKey::for_table(Dialect::Sqlite, &customers(), false);
    let sql = Dialect::Sqlite.rows_sql(&table("", "customers"), &customers(), &key, 1001, 0);
    assert!(sql.starts_with("SELECT json_array(CAST(\"id\" AS TEXT))"), "{sql}");
    assert!(
        sql.contains("CASE typeof(\"name\") WHEN 'blob' THEN hex(\"name\") ELSE \"name\" END"),
        "{sql}"
    );
    assert!(sql.contains("json_object('id', "), "{sql}");
    assert!(
        sql.ends_with("FROM \"customers\" AS azul_row ORDER BY \"id\" LIMIT 1001 OFFSET 0"),
        "{sql}"
    );
}

#[test]
fn sqlite_builds_the_json_of_a_wide_table_in_pieces_its_functions_can_take() {
    let columns: Vec<Column> = (0..150)
        .map(|i| column(&format!("c{i}"), "TEXT", u32::from(i == 0)))
        .collect();
    let json = Dialect::Sqlite.row_json_sql(&columns);
    assert!(json.starts_with("json_insert(json_insert(json_object("), "{json}");
    assert!(json.contains("'$.\"c149\"'"), "{json}");
}

#[test]
fn postgres_finds_one_row_by_the_text_of_its_key() {
    let columns = vec![
        column("region", "text", 1),
        column("no", "integer", 2),
        column("total", "numeric", 0),
    ];
    let key = RowKey::for_table(Dialect::Postgres, &columns, false);
    let (sql, binds) = Dialect::Postgres.row_sql(
        &table("sales", "orders"),
        &columns,
        &key,
        &RowLookup::Key(vec![Some("eu".to_string()), Some("7".to_string())]),
    );
    assert_eq!(
        sql,
        "SELECT row_to_json(azul_row)::text FROM \"sales\".\"orders\" AS azul_row WHERE \
         \"region\"::text = $1 AND \"no\"::text = $2 LIMIT 1"
    );
    assert_eq!(binds, vec!["eu".to_string(), "7".to_string()]);
    let (sql, binds) = Dialect::Postgres.row_sql(
        &table("sales", "orders"),
        &columns,
        &key,
        &RowLookup::Key(vec![None, Some("7".to_string())]),
    );
    assert!(sql.contains("\"region\" IS NULL AND \"no\"::text = $1"), "{sql}");
    assert_eq!(binds, vec!["7".to_string()]);
}

#[test]
fn mysql_and_sqlite_compare_a_key_with_a_text_parameter() {
    let key = RowKey::for_table(Dialect::Mysql, &customers(), false);
    let (sql, binds) = Dialect::Mysql.row_sql(
        &table("", "customers"),
        &customers(),
        &key,
        &RowLookup::Key(vec![Some("2".to_string())]),
    );
    assert!(sql.starts_with("SELECT CAST(JSON_OBJECT('id', `id`, "), "{sql}");
    assert!(sql.ends_with("FROM `customers` AS azul_row WHERE `id` = ? LIMIT 1"), "{sql}");
    assert_eq!(binds, vec!["2".to_string()]);
    let (sql, _) = Dialect::Sqlite.row_sql(
        &table("", "log"),
        &[column("at", "TEXT", 0)],
        &RowKey::RowId,
        &RowLookup::Key(vec![Some("5".to_string())]),
    );
    assert!(sql.ends_with("WHERE rowid = ? LIMIT 1"), "{sql}");
}

#[test]
fn a_row_by_its_place_is_found_in_the_order_of_all_its_columns() {
    let columns = vec![column("at", "text", 0), column("message", "text", 0)];
    let (sql, binds) = Dialect::Postgres.row_sql(
        &table("public", "log"),
        &columns,
        &RowKey::Position,
        &RowLookup::Position(3),
    );
    assert!(sql.ends_with("ORDER BY azul_row::text LIMIT 1 OFFSET 2"), "{sql}");
    assert!(binds.is_empty());
    let (sql, _) = Dialect::Mysql.row_sql(
        &table("", "log"),
        &columns,
        &RowKey::Position,
        &RowLookup::Position(1),
    );
    assert!(sql.ends_with("ORDER BY `at`, `message` LIMIT 1 OFFSET 0"), "{sql}");
}

// ==== CSV and schema ====

#[test]
fn the_csv_has_a_header_and_a_line_per_row_quoting_what_needs_it() {
    let rows = vec![
        json!({"id": 1, "name": "Ada", "city": "London"}),
        json!({"id": 3, "name": "Linus, Jr.", "city": null}),
        json!({"id": 4, "name": "say \"hi\"", "city": "two\nlines"}),
        json!({"id": 5, "name": true, "city": {"zip": "1"}}),
    ];
    let text = csv_text(&customers(), &rows);
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!(lines[0], "id,name,city");
    assert_eq!(lines[1], "1,Ada,London");
    assert_eq!(lines[2], "3,\"Linus, Jr.\",");
    assert_eq!(lines[3], "4,\"say \"\"hi\"\"\",\"two");
    assert_eq!(lines[4], "lines\"");
    assert_eq!(lines[5], "5,true,\"{\"\"zip\"\":\"\"1\"\"}\"");
    assert!(text.ends_with('\n'));
}

#[test]
fn the_schema_file_names_the_columns_and_the_primary_key() {
    let text = schema_json(
        "sqlite",
        &table("", "customers"),
        &customers(),
        &RowKey::Columns(vec!["id".to_string()]),
    );
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["table"], "customers");
    assert_eq!(value["engine"], "sqlite");
    assert_eq!(value["primary_key"], json!(["id"]));
    assert_eq!(value["columns"][1]["name"], "name");
    assert_eq!(value["columns"][1]["type"], "TEXT");
    assert_eq!(value["columns"][0]["primary_key"], true);
    assert_eq!(value["columns"][2]["nullable"], true);
}

// ==== Pages and ranges ====

#[test]
fn a_listing_continues_where_its_last_page_ended() {
    assert_eq!(page_bounds(5, 2, None).unwrap(), (0, 2, Some("2".to_string())));
    assert_eq!(page_bounds(5, 2, Some("2")).unwrap(), (2, 4, Some("4".to_string())));
    assert_eq!(page_bounds(5, 2, Some("4")).unwrap(), (4, 5, None));
    assert_eq!(page_bounds(0, 2, None).unwrap(), (0, 0, None));
    assert!(matches!(page_bounds(5, 2, Some("x")), Err(DriveError::Protocol(_))));
}

#[test]
fn a_byte_range_of_a_file_is_a_slice_of_it() {
    let bytes = b"0123456789";
    assert_eq!(
        tables::byte_range(bytes, ByteRange::new(2, Some(4)), "k").unwrap(),
        b"234".to_vec()
    );
    assert_eq!(
        tables::byte_range(bytes, ByteRange::new(8, None), "k").unwrap(),
        b"89".to_vec()
    );
    assert_eq!(
        tables::byte_range(bytes, ByteRange::new(8, Some(99)), "k").unwrap(),
        b"89".to_vec()
    );
    assert!(matches!(
        tables::byte_range(bytes, ByteRange::new(10, None), "k"),
        Err(DriveError::InvalidRange { .. })
    ));
}
