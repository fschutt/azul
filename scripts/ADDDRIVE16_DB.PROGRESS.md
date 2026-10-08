# ADDDRIVE16 - databases as files (fork of ADDDRIVE16)

Task: azul-storage's tables-as-folders data source: `tables.rs` (the layout and the SQL as
plain functions, always compiled) and `database.rs` (`DatabaseDrive` over sqlx 0.8, feature
`sql`), with their tests. No builds here; the lead compiles
(`cargo test -p azul-storage --features sql`; without the feature only tests/tables.rs runs).

## DONE

- RED tests: src/tests/tables.rs (pure), src/tests/database.rs (a SQLite file, feature `sql`).
- tables.rs: TablePath (keys), folder names, row file names (key values, rowid, place),
  Dialect SQL (list / find tables, columns, rows page, one row), CSV and schema.json text,
  page bounds, byte ranges.
- database.rs: DatabaseDrive::open (settings checked, no connection), a lazy 2-connection
  pool on the storage runtime, read-only Drive (list / get / get_range / head / metadata).

## Layout

```text
/                    a folder per table and view (PostgreSQL outside public: schema.table)
customers/
  customers.csv      the first 10,000 rows as CSV, in key order
  schema.json        columns, types, nullable, primary key
  rows/              a JSON file per row (the first 10,000), named by the primary key
    1.json
```

## Notes for the lead

- Row file names keep letters and spaces; only / \ % , ( ) : * ? " < > | control characters
  and a leading dot are %XX (decoded with sigv4::uri_decode); (null) / (empty) for NULL / "".
- SQLite tables without a primary key name rows by rowid; PostgreSQL / MySQL tables without
  one and views by place (row-000001), which a write can shift.
