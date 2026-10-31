//! A database as files: the layout `DatabaseDrive` (feature `sql`) shows, and the SQL it asks
//! each engine, as plain functions - no driver here, so all of it is tested without a database.
//!
//! ```text
//! /                          a folder per table and view
//! customers/                 a table
//!   customers.csv            its rows as CSV (the first 10,000, in key order)
//!   schema.json              its columns, their types, its primary key
//!   rows/                    a JSON file per row (the first 10,000, in key order)
//!     1.json                 the row whose primary key is 1
//! sales.orders/              PostgreSQL: a table outside the `public` schema
//! ```
//!
//! A table's folder is its name with `%` and `/` written `%25` and `%2F`. A row's file is named
//! by its primary key: each value as the engine writes it as text, the characters a file name
//! cannot hold (`/ \ % , ( ) : * ? " < > |`, control characters, a leading `.`) as `%XX`,
//! several values joined by `,`; `(null)` and `(empty)` name a NULL and an empty text. A SQLite
//! table without a primary key names its rows by their `rowid`; other tables without one, and
//! views, by their place (`row-000001`), which a write elsewhere in the table can shift.
//!
//! The ENGINE renders the values: a row's file is the JSON text the engine makes of the row
//! (`json_object` in SQLite - a BLOB as hex -, `row_to_json` in PostgreSQL, `JSON_OBJECT` in
//! MySQL), so every column type reads as the engine writes it; a key's values come back as
//! text the same way.

use serde_json::{json, Value};

use crate::{config::DatabaseEngine, sigv4::uri_decode, ByteRange, DriveError};

/// Rows a table's `rows/` folder lists at most.
pub const MAX_ROW_FILES: u64 = 10_000;
/// Rows a listing asks the engine for at once.
pub const ROWS_PAGE: u64 = 1000;
/// Rows a table's CSV file holds at most.
pub const CSV_MAX_ROWS: u64 = 10_000;
/// The folder of a table's row files.
pub const ROWS_FOLDER: &str = "rows";
/// A table's schema file.
pub const SCHEMA_FILE: &str = "schema.json";

/// A row file's extension.
const ROW_EXTENSION: &str = ".json";
/// What a row's place starts with (a table without a key).
const POSITION_PREFIX: &str = "row-";
/// The name of a NULL key value, and of an empty text.
const NULL_PART: &str = "(null)";
const EMPTY_PART: &str = "(empty)";
/// The table's alias in every query (PostgreSQL's `row_to_json` takes the row by it).
const ALIAS: &str = "azul_row";
/// Name-value pairs per `json_object` / `json_insert` call: SQLite's functions take at most
/// 127 arguments in older builds.
const SQLITE_PAIRS_PER_CALL: usize = 60;

// ==== Tables, columns, keys ====

/// The SQL each engine reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
    Mysql,
}

/// A table or view: its schema (PostgreSQL's; empty for SQLite and MySQL) and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRef {
    pub schema: String,
    pub name: String,
    /// A view (its rows have no key of their own).
    pub view: bool,
}

/// One column of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    /// The type as the engine names it (`INTEGER`, `character varying`, `varchar(64)`).
    pub data_type: String,
    pub nullable: bool,
    /// Its place in the primary key, from 1; 0 when it is not part of it.
    pub primary_key: u32,
}

/// What names a table's rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKey {
    /// The primary key's columns, in the key's order.
    Columns(Vec<String>),
    /// SQLite's `rowid` (a table without a primary key).
    RowId,
    /// The row's place in the order of all its columns (no key at all; it can shift).
    Position,
}

impl RowKey {
    /// The key of a table with `columns`: its primary key, else SQLite's `rowid` (a table, not a
    /// view), else the rows' place.
    #[must_use]
    pub fn for_table(dialect: Dialect, columns: &[Column], view: bool) -> RowKey {
        let mut keyed: Vec<&Column> = columns.iter().filter(|c| c.primary_key > 0).collect();
        keyed.sort_by_key(|c| c.primary_key);
        if !keyed.is_empty() {
            return RowKey::Columns(keyed.iter().map(|c| c.name.clone()).collect());
        }
        if dialect == Dialect::Sqlite && !view {
            RowKey::RowId
        } else {
            RowKey::Position
        }
    }
}

/// Which row a row file's name asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowLookup {
    /// The key's values (`None`: NULL), in the key's order.
    Key(Vec<Option<String>>),
    /// The place, from 1.
    Position(u64),
}

/// Whether an engine's word for a table's kind (`table`, `BASE TABLE`, `VIEW`) is a view.
#[must_use]
pub fn is_view(kind: &str) -> bool {
    kind.trim().eq_ignore_ascii_case("view")
}

// ==== Keys of the drive ====

/// What a key of a database drive names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TablePath {
    /// The drive's root: the tables.
    Root,
    /// A table's folder (`customers/`).
    Table { folder: String },
    /// A table's CSV file (`customers/customers.csv`).
    Csv { folder: String },
    /// A table's schema file (`customers/schema.json`).
    Schema { folder: String },
    /// A table's row files (`customers/rows/`).
    Rows { folder: String },
    /// One row's file (`customers/rows/1.json`); `name` without `.json`.
    Row { folder: String, name: String },
}

impl TablePath {
    /// What `key` names; `None` for a key no table, row or file of a database has.
    #[must_use]
    pub fn parse(key: &str) -> Option<TablePath> {
        if key.is_empty() {
            return Some(TablePath::Root);
        }
        let (folder, rest) = key.split_once('/')?;
        if folder.is_empty() {
            return None;
        }
        let folder = folder.to_string();
        if rest.is_empty() {
            return Some(TablePath::Table { folder });
        }
        if rest == SCHEMA_FILE {
            return Some(TablePath::Schema { folder });
        }
        if rest == csv_name(&folder) {
            return Some(TablePath::Csv { folder });
        }
        let in_rows = rest.strip_prefix(ROWS_FOLDER)?.strip_prefix('/')?;
        if in_rows.is_empty() {
            return Some(TablePath::Rows { folder });
        }
        let name = in_rows.strip_suffix(ROW_EXTENSION)?;
        if name.is_empty() || name.contains('/') {
            return None;
        }
        Some(TablePath::Row {
            folder,
            name: name.to_string(),
        })
    }

    /// The key (or, for a folder, the prefix) this path names.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            TablePath::Root => String::new(),
            TablePath::Table { folder } => format!("{folder}/"),
            TablePath::Csv { folder } => format!("{folder}/{}", csv_name(folder)),
            TablePath::Schema { folder } => format!("{folder}/{SCHEMA_FILE}"),
            TablePath::Rows { folder } => format!("{folder}/{ROWS_FOLDER}/"),
            TablePath::Row { folder, name } => {
                format!("{folder}/{ROWS_FOLDER}/{name}{ROW_EXTENSION}")
            }
        }
    }

    /// Whether it names a folder (the root, a table, its rows).
    #[must_use]
    pub fn is_folder(&self) -> bool {
        matches!(
            self,
            TablePath::Root | TablePath::Table { .. } | TablePath::Rows { .. }
        )
    }

    /// The table's folder it is in (`None` for the root).
    #[must_use]
    pub fn folder(&self) -> Option<&str> {
        match self {
            TablePath::Root => None,
            TablePath::Table { folder }
            | TablePath::Csv { folder }
            | TablePath::Schema { folder }
            | TablePath::Rows { folder }
            | TablePath::Row { folder, .. } => Some(folder),
        }
    }
}

/// A table's CSV file name: its folder's name and `.csv`.
#[must_use]
pub fn csv_name(folder: &str) -> String {
    format!("{folder}.csv")
}

/// A name as one segment of a key: `%` and `/` written `%25` and `%2F`.
#[must_use]
pub fn escape_segment(name: &str) -> String {
    name.replace('%', "%25").replace('/', "%2F")
}

/// [`escape_segment`] read back; `None` for a broken `%` sequence.
#[must_use]
pub fn unescape_segment(segment: &str) -> Option<String> {
    uri_decode(segment)
}

/// A table's folder: its name, a PostgreSQL table outside `public` as `<schema>.<name>`.
#[must_use]
pub fn folder_name(dialect: Dialect, table: &TableRef) -> String {
    let name = escape_segment(&table.name);
    match dialect {
        Dialect::Postgres if !table.schema.is_empty() && table.schema != "public" => {
            format!("{}.{name}", escape_segment(&table.schema))
        }
        _ => name,
    }
}

/// The tables a folder can name, as `(schema, name)`, the likelier first: a PostgreSQL folder
/// `sales.orders` is the table `sales.orders` of `public` or the table `orders` of `sales`.
/// `None` for a folder name with a broken `%` sequence.
#[must_use]
pub fn table_candidates(dialect: Dialect, folder: &str) -> Option<Vec<(String, String)>> {
    let name = unescape_segment(folder)?;
    Some(match dialect {
        Dialect::Postgres => {
            let mut candidates = vec![(String::from("public"), name.clone())];
            if let Some((schema, table)) = name.split_once('.') {
                if !schema.is_empty() && !table.is_empty() {
                    candidates.push((schema.to_string(), table.to_string()));
                }
            }
            candidates
        }
        Dialect::Sqlite | Dialect::Mysql => vec![(String::new(), name)],
    })
}

// ==== Row files ====

/// One value of a key in a file name (module docs): `None` is NULL.
#[must_use]
pub fn encode_key_part(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::from(NULL_PART);
    };
    if value.is_empty() {
        return String::from(EMPTY_PART);
    }
    let mut out = String::with_capacity(value.len());
    for (i, c) in value.char_indices() {
        let escape = (c == '.' && i == 0)
            || c.is_control()
            || matches!(
                c,
                '/' | '\\' | '%' | ',' | '(' | ')' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            );
        if escape {
            let mut buffer = [0u8; 4];
            for byte in c.encode_utf8(&mut buffer).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// [`encode_key_part`] read back: `Some(None)` for NULL, `None` for a broken `%` sequence.
#[must_use]
pub fn decode_key_part(part: &str) -> Option<Option<String>> {
    match part {
        NULL_PART => Some(None),
        EMPTY_PART => Some(Some(String::new())),
        _ => uri_decode(part).map(Some),
    }
}

/// The values of a key as the engine sent them (a JSON array of texts).
fn key_parts(key_json: &str) -> Option<Vec<Option<String>>> {
    let value: Value = serde_json::from_str(key_json).ok()?;
    Some(
        value
            .as_array()?
            .iter()
            .map(|v| match v {
                Value::Null => None,
                Value::String(text) => Some(text.clone()),
                other => Some(other.to_string()),
            })
            .collect(),
    )
}

/// The name of a row at a place (from 1).
fn position_name(position: u64) -> String {
    format!("{POSITION_PREFIX}{position:06}")
}

/// A row file's name without `.json`: its key's values (`key_json`, the engine's JSON array of
/// texts), or for a table without a key its place (`position`, from 1).
#[must_use]
pub fn row_name(key: &RowKey, key_json: &str, position: u64) -> String {
    match key {
        RowKey::Position => position_name(position),
        RowKey::Columns(_) | RowKey::RowId => match key_parts(key_json) {
            Some(parts) if !parts.is_empty() => parts
                .iter()
                .map(|part| encode_key_part(part.as_deref()))
                .collect::<Vec<_>>()
                .join(","),
            _ => position_name(position),
        },
    }
}

/// The row a row file's name (without `.json`) asks for; `None` when it cannot name a row of a
/// table with this key.
#[must_use]
pub fn parse_row_name(name: &str, key: &RowKey) -> Option<RowLookup> {
    match key {
        RowKey::Position => {
            let digits = name.strip_prefix(POSITION_PREFIX)?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let place: u64 = digits.parse().ok()?;
            (place >= 1).then_some(RowLookup::Position(place))
        }
        RowKey::RowId => {
            let id = decode_key_part(name)??;
            Some(RowLookup::Key(vec![Some(id)]))
        }
        RowKey::Columns(columns) => {
            let parts = name
                .split(',')
                .map(decode_key_part)
                .collect::<Option<Vec<Option<String>>>>()?;
            (parts.len() == columns.len()).then_some(RowLookup::Key(parts))
        }
    }
}

// ==== SQL ====

impl Dialect {
    /// The dialect of an engine.
    #[must_use]
    pub const fn of(engine: DatabaseEngine) -> Dialect {
        match engine {
            DatabaseEngine::Sqlite => Dialect::Sqlite,
            DatabaseEngine::Postgres => Dialect::Postgres,
            DatabaseEngine::Mysql => Dialect::Mysql,
        }
    }

    /// A table's or column's name as the engine reads it: `"name"`, MySQL's `` `name` ``.
    #[must_use]
    pub fn quote_ident(self, name: &str) -> String {
        match self {
            Dialect::Sqlite | Dialect::Postgres => format!("\"{}\"", name.replace('"', "\"\"")),
            Dialect::Mysql => format!("`{}`", name.replace('`', "``")),
        }
    }

    /// A text as an SQL string: `'text'` (MySQL reads a backslash as an escape, so it is
    /// doubled there).
    #[must_use]
    pub fn quote_literal(self, text: &str) -> String {
        let text = match self {
            Dialect::Mysql => text.replace('\\', "\\\\"),
            Dialect::Sqlite | Dialect::Postgres => text.to_string(),
        };
        format!("'{}'", text.replace('\'', "''"))
    }

    /// The `n`th parameter (from 1): PostgreSQL's `$n`, else `?`.
    #[must_use]
    pub fn placeholder(self, n: usize) -> String {
        match self {
            Dialect::Postgres => format!("${n}"),
            Dialect::Sqlite | Dialect::Mysql => String::from("?"),
        }
    }

    /// The table in a `FROM`: PostgreSQL's with its schema.
    #[must_use]
    pub fn table_sql(self, table: &TableRef) -> String {
        match self {
            Dialect::Postgres => {
                let schema = if table.schema.is_empty() {
                    "public"
                } else {
                    table.schema.as_str()
                };
                format!(
                    "{}.{}",
                    self.quote_ident(schema),
                    self.quote_ident(&table.name)
                )
            }
            Dialect::Sqlite | Dialect::Mysql => self.quote_ident(&table.name),
        }
    }

    /// `expr` as text.
    fn text_of(self, expr: &str) -> String {
        match self {
            Dialect::Sqlite => format!("CAST({expr} AS TEXT)"),
            Dialect::Postgres => format!("{expr}::text"),
            Dialect::Mysql => format!("CAST({expr} AS CHAR)"),
        }
    }

    /// Every table and view: rows of `(schema, name, kind)` texts, ordered.
    #[must_use]
    pub const fn list_tables_sql(self) -> &'static str {
        match self {
            Dialect::Sqlite => {
                "SELECT '', name, type FROM sqlite_master WHERE type IN ('table', 'view') AND \
                 name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name"
            }
            Dialect::Postgres => {
                "SELECT table_schema::text, table_name::text, table_type::text FROM \
                 information_schema.tables WHERE table_schema::text NOT IN ('pg_catalog', \
                 'information_schema') AND table_schema::text NOT LIKE 'pg\\_%' ORDER BY \
                 table_schema, table_name"
            }
            Dialect::Mysql => {
                "SELECT CAST('' AS CHAR), CAST(table_name AS CHAR), CAST(table_type AS CHAR) \
                 FROM information_schema.tables WHERE table_schema = DATABASE() ORDER BY \
                 table_name"
            }
        }
    }

    /// The table a folder names: rows of `(schema, name, kind)` texts, the likelier first; its
    /// parameters are [`Self::find_table_binds`].
    #[must_use]
    pub const fn find_table_sql(self) -> &'static str {
        match self {
            Dialect::Sqlite => {
                "SELECT '', name, type FROM sqlite_master WHERE type IN ('table', 'view') AND \
                 name = ?"
            }
            Dialect::Postgres => {
                "SELECT table_schema::text, table_name::text, table_type::text FROM \
                 information_schema.tables WHERE (table_schema::text = 'public' AND \
                 table_name::text = $1) OR (table_schema::text = $2 AND table_name::text = $3) \
                 ORDER BY (table_schema::text = 'public') DESC"
            }
            Dialect::Mysql => {
                "SELECT CAST('' AS CHAR), CAST(table_name AS CHAR), CAST(table_type AS CHAR) \
                 FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name \
                 = ?"
            }
        }
    }

    /// The parameters of [`Self::find_table_sql`] for a folder; `None` for a folder name with a
    /// broken `%` sequence.
    #[must_use]
    pub fn find_table_binds(self, folder: &str) -> Option<Vec<String>> {
        let candidates = table_candidates(self, folder)?;
        let first = candidates.first()?.1.clone();
        Some(match self {
            Dialect::Postgres => {
                let (schema, name) = candidates.get(1).cloned().unwrap_or_default();
                vec![first, schema, name]
            }
            Dialect::Sqlite | Dialect::Mysql => vec![first],
        })
    }

    /// A table's columns in their order: rows of `(name, type, nullable, place in the primary
    /// key)` - SQLite `(text, text, notnull integer, pk integer)`, PostgreSQL `(text, text,
    /// boolean, int4)`, MySQL `(text, text, bigint, bigint)`; its parameters are
    /// [`Self::columns_binds`].
    #[must_use]
    pub const fn columns_sql(self) -> &'static str {
        match self {
            Dialect::Sqlite => "SELECT name, type, \"notnull\", pk FROM pragma_table_info(?)",
            Dialect::Postgres => {
                "SELECT c.column_name::text, CASE WHEN c.data_type = 'USER-DEFINED' THEN \
                 c.udt_name::text ELSE c.data_type::text END, c.is_nullable::text = 'YES', \
                 COALESCE((SELECT k.ordinal_position::int FROM \
                 information_schema.table_constraints t JOIN \
                 information_schema.key_column_usage k ON k.constraint_schema = \
                 t.constraint_schema AND k.constraint_name = t.constraint_name AND \
                 k.table_schema = t.table_schema AND k.table_name = t.table_name WHERE \
                 t.constraint_type = 'PRIMARY KEY' AND t.table_schema = c.table_schema AND \
                 t.table_name = c.table_name AND k.column_name = c.column_name LIMIT 1), 0) FROM \
                 information_schema.columns c WHERE c.table_schema::text = $1 AND \
                 c.table_name::text = $2 ORDER BY c.ordinal_position"
            }
            Dialect::Mysql => {
                "SELECT CAST(c.column_name AS CHAR), CAST(c.column_type AS CHAR), \
                 CAST(c.is_nullable = 'YES' AS SIGNED), CAST(COALESCE((SELECT \
                 k.ordinal_position FROM information_schema.key_column_usage k WHERE \
                 k.table_schema = c.table_schema AND k.table_name = c.table_name AND \
                 k.column_name = c.column_name AND k.constraint_name = 'PRIMARY' LIMIT 1), 0) AS \
                 SIGNED) FROM information_schema.columns c WHERE c.table_schema = DATABASE() AND \
                 c.table_name = ? ORDER BY c.ordinal_position"
            }
        }
    }

    /// The parameters of [`Self::columns_sql`].
    #[must_use]
    pub fn columns_binds(self, table: &TableRef) -> Vec<String> {
        match self {
            Dialect::Postgres => {
                let schema = if table.schema.is_empty() {
                    String::from("public")
                } else {
                    table.schema.clone()
                };
                vec![schema, table.name.clone()]
            }
            Dialect::Sqlite | Dialect::Mysql => vec![table.name.clone()],
        }
    }

    /// The JSON text of a row of a table with `columns`, rendered by the engine (a SQLite BLOB
    /// as hex).
    #[must_use]
    pub fn row_json_sql(self, columns: &[Column]) -> String {
        match self {
            Dialect::Postgres => format!("row_to_json({ALIAS})::text"),
            Dialect::Mysql => {
                let pairs: Vec<String> = columns
                    .iter()
                    .map(|c| {
                        format!(
                            "{}, {}",
                            self.quote_literal(&c.name),
                            self.quote_ident(&c.name)
                        )
                    })
                    .collect();
                format!("CAST(JSON_OBJECT({}) AS CHAR)", pairs.join(", "))
            }
            Dialect::Sqlite => {
                let pairs: Vec<(String, String)> = columns
                    .iter()
                    .map(|c| {
                        let q = self.quote_ident(&c.name);
                        (
                            c.name.clone(),
                            format!("CASE typeof({q}) WHEN 'blob' THEN hex({q}) ELSE {q} END"),
                        )
                    })
                    .collect();
                let mut chunks = pairs.chunks(SQLITE_PAIRS_PER_CALL);
                let first = chunks.next().unwrap_or(&[]);
                let first: Vec<String> = first
                    .iter()
                    .map(|(name, value)| format!("{}, {value}", self.quote_literal(name)))
                    .collect();
                let mut sql = format!("json_object({})", first.join(", "));
                for chunk in chunks {
                    let rest: Vec<String> = chunk
                        .iter()
                        .map(|(name, value)| {
                            let path = format!("$.\"{}\"", name.replace('"', "\\\""));
                            format!("{}, {value}", self.quote_literal(&path))
                        })
                        .collect();
                    sql = format!("json_insert({sql}, {})", rest.join(", "));
                }
                sql
            }
        }
    }

    /// The JSON array of a row's key values as texts (`'[]'` for a table without a key).
    fn key_json_sql(self, key: &RowKey) -> String {
        let empty = match self {
            Dialect::Postgres => String::from("'[]'::text"),
            Dialect::Sqlite | Dialect::Mysql => String::from("'[]'"),
        };
        let parts: Vec<String> = match key {
            RowKey::Columns(columns) => columns
                .iter()
                .map(|c| self.text_of(&self.quote_ident(c)))
                .collect(),
            RowKey::RowId if self == Dialect::Sqlite => vec![self.text_of("rowid")],
            RowKey::RowId | RowKey::Position => return empty,
        };
        let parts = parts.join(", ");
        match self {
            Dialect::Sqlite => format!("json_array({parts})"),
            Dialect::Postgres => format!("json_build_array({parts})::text"),
            Dialect::Mysql => format!("CAST(JSON_ARRAY({parts}) AS CHAR)"),
        }
    }

    /// The `ORDER BY` of the rows (empty when there is nothing to order by).
    fn order_sql(self, columns: &[Column], key: &RowKey) -> String {
        match key {
            RowKey::Columns(names) => {
                let names: Vec<String> = names.iter().map(|n| self.quote_ident(n)).collect();
                format!("ORDER BY {}", names.join(", "))
            }
            RowKey::RowId => String::from("ORDER BY rowid"),
            RowKey::Position => match self {
                Dialect::Postgres => format!("ORDER BY {ALIAS}::text"),
                Dialect::Sqlite | Dialect::Mysql if columns.is_empty() => String::new(),
                Dialect::Sqlite | Dialect::Mysql => {
                    let names: Vec<String> =
                        columns.iter().map(|c| self.quote_ident(&c.name)).collect();
                    format!("ORDER BY {}", names.join(", "))
                }
            },
        }
    }

    /// A page of rows in key order: rows of `(key JSON, row JSON)` texts, `limit` of them from
    /// `offset`. No parameters.
    #[must_use]
    pub fn rows_sql(
        self,
        table: &TableRef,
        columns: &[Column],
        key: &RowKey,
        limit: u64,
        offset: u64,
    ) -> String {
        let mut sql = format!(
            "SELECT {} AS azul_key, {} AS azul_json FROM {} AS {ALIAS}",
            self.key_json_sql(key),
            self.row_json_sql(columns),
            self.table_sql(table)
        );
        let order = self.order_sql(columns, key);
        if !order.is_empty() {
            sql.push(' ');
            sql.push_str(&order);
        }
        sql.push_str(&format!(" LIMIT {limit} OFFSET {offset}"));
        sql
    }

    /// One row's JSON text, and the query's parameters: by its key's values (a NULL with
    /// `IS NULL`; PostgreSQL compares the column's text, the others the column with a text
    /// parameter), or by its place.
    #[must_use]
    pub fn row_sql(
        self,
        table: &TableRef,
        columns: &[Column],
        key: &RowKey,
        lookup: &RowLookup,
    ) -> (String, Vec<String>) {
        let select = format!(
            "SELECT {} FROM {} AS {ALIAS}",
            self.row_json_sql(columns),
            self.table_sql(table)
        );
        match lookup {
            RowLookup::Position(place) => {
                let mut sql = select;
                let order = self.order_sql(columns, &RowKey::Position);
                if !order.is_empty() {
                    sql.push(' ');
                    sql.push_str(&order);
                }
                sql.push_str(&format!(" LIMIT 1 OFFSET {}", place.saturating_sub(1)));
                (sql, Vec::new())
            }
            RowLookup::Key(parts) => {
                let names: Vec<String> = match key {
                    RowKey::Columns(names) => names.iter().map(|n| self.quote_ident(n)).collect(),
                    RowKey::RowId | RowKey::Position => vec![String::from("rowid")],
                };
                let mut conditions = Vec::with_capacity(names.len());
                let mut binds = Vec::new();
                for (name, part) in names.iter().zip(parts) {
                    match part {
                        None => conditions.push(format!("{name} IS NULL")),
                        Some(value) => {
                            binds.push(value.clone());
                            let parameter = self.placeholder(binds.len());
                            conditions.push(match self {
                                Dialect::Postgres => format!("{name}::text = {parameter}"),
                                Dialect::Sqlite | Dialect::Mysql => {
                                    format!("{name} = {parameter}")
                                }
                            });
                        }
                    }
                }
                (
                    format!("{select} WHERE {} LIMIT 1", conditions.join(" AND ")),
                    binds,
                )
            }
        }
    }
}

// ==== The CSV and schema files ====

/// A CSV field: as it is, or quoted when it holds a comma, a quote or a line break.
fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// A value of a row as CSV text: a text as it is, NULL empty, a number or a truth as written,
/// anything else as its JSON.
fn csv_value(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// A table's rows (each the engine's JSON object of a row) as CSV: a header line with the
/// columns' names, then a line per row, the columns in the table's order. Lines end in `\n`.
#[must_use]
pub fn csv_text(columns: &[Column], rows: &[Value]) -> String {
    let mut out = String::new();
    let header: Vec<String> = columns.iter().map(|c| csv_field(&c.name)).collect();
    out.push_str(&header.join(","));
    out.push('\n');
    for row in rows {
        let fields: Vec<String> = columns
            .iter()
            .map(|c| csv_field(&csv_value(row.get(c.name.as_str()))))
            .collect();
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

/// A table's `schema.json`: its name, engine, columns (name, type, nullable, in the primary
/// key) and primary key, and how its row files are named.
#[must_use]
pub fn schema_json(engine: &str, table: &TableRef, columns: &[Column], key: &RowKey) -> String {
    let primary_key: Vec<&str> = match key {
        RowKey::Columns(names) => names.iter().map(String::as_str).collect(),
        RowKey::RowId | RowKey::Position => Vec::new(),
    };
    let rows = match key {
        RowKey::Columns(_) => "rows/<primary key>.json",
        RowKey::RowId => "rows/<rowid>.json",
        RowKey::Position => "rows/row-<place>.json (a place can shift when the table changes)",
    };
    let schema = if table.schema.is_empty() {
        Value::Null
    } else {
        Value::String(table.schema.clone())
    };
    let column_list: Vec<Value> = columns
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "type": c.data_type,
                "nullable": c.nullable,
                "primary_key": c.primary_key > 0,
            })
        })
        .collect();
    let value = json!({
        "table": table.name,
        "schema": schema,
        "engine": engine,
        "view": table.view,
        "primary_key": primary_key,
        "rows": rows,
        "columns": column_list,
    });
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

// ==== Pages and ranges ====

/// The entries `start..end` of a listing of `len` entries, a page of `page_size` (0: the
/// default page) after `continuation` (the offset where the last page ended), and where the
/// next page starts (`None`: this is the last).
pub fn page_bounds(
    len: usize,
    page_size: u32,
    continuation: Option<&str>,
) -> Result<(usize, usize, Option<String>), DriveError> {
    let start = match continuation {
        None => 0,
        Some(token) => token.trim().parse::<usize>().map_err(|_| {
            DriveError::Protocol(String::from(
                "the listing's continuation token is not one of this drive's",
            ))
        })?,
    };
    let start = start.min(len);
    let page = if page_size == 0 {
        crate::DEFAULT_PAGE_SIZE
    } else {
        page_size
    };
    let size = usize::try_from(page).unwrap_or(usize::MAX);
    let end = start.saturating_add(size).min(len);
    let next = (end < len).then(|| end.to_string());
    Ok((start, end, next))
}

/// The bytes `range` names of a file (`key` names it in the error): S3's semantics - the end
/// past the file is the file's end, a start past it is no range.
pub fn byte_range(bytes: &[u8], range: ByteRange, key: &str) -> Result<Vec<u8>, DriveError> {
    let len = bytes.len() as u64;
    let out_of_range = || DriveError::InvalidRange {
        key: key.to_string(),
    };
    if range.start >= len {
        return Err(out_of_range());
    }
    let end = range.end.map_or(len - 1, |end| end.min(len - 1));
    if end < range.start {
        return Err(out_of_range());
    }
    // Both are below the length of a slice in memory.
    #[allow(clippy::cast_possible_truncation)]
    let (start, end) = (range.start as usize, end as usize);
    Ok(bytes[start..=end].to_vec())
}
