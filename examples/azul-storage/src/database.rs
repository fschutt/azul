//! A database browsed as files: a PostgreSQL or MySQL / MariaDB database or a SQLite file,
//! its tables as folders, every row a JSON file named by its key, a table's rows as a CSV file -
//! read-only. The layout and the SQL are [`crate::tables`]'s; this is the driver: sqlx 0.8, the
//! one OpenDAL's postgresql / mysql / sqlite services use.
//!
//! Every call blocks: it runs on [`crate::runtime`]'s runtime from the calling thread (an azul
//! `Thread`, never a UI callback). Opening checks the settings and connects to nothing; the
//! first call opens a small pool of connections (two) that later calls share. A table's
//! columns and key are read once (again when its folder or the root is listed), its CSV made
//! when its folder is listed and kept until the next listing of it.
//!
//! No password in `Debug` output or in an error.

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use serde_json::Value;
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlSslMode},
    postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgSslMode},
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
};

use crate::{
    config::{DatabaseEngine, SecretOptions},
    key::folder_of,
    runtime,
    tables::{self, Column, Dialect, RowKey, TablePath, TableRef},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo,
};

/// Connections a drive keeps open at most.
const MAX_CONNECTIONS: u32 = 2;
/// How long a call waits for a connection (a server that does not answer).
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);

/// How to connect, per engine.
enum Connect {
    Sqlite(SqliteConnectOptions),
    Postgres(PgConnectOptions),
    Mysql(MySqlConnectOptions),
}

/// The connections, per engine (a clone shares them).
#[derive(Clone)]
enum Pool {
    Sqlite(SqlitePool),
    Postgres(PgPool),
    Mysql(MySqlPool),
}

/// Runs `$body` with `$p` the pool of whichever engine it is and `$db` naming its sqlx
/// database type: one query written once for the three engines.
macro_rules! on_pool {
    ($pool:expr, $p:ident, $db:ident => $body:expr) => {
        match $pool {
            Pool::Sqlite($p) => {
                type $db = sqlx::Sqlite;
                $body
            }
            Pool::Postgres($p) => {
                type $db = sqlx::Postgres;
                $body
            }
            Pool::Mysql($p) => {
                type $db = sqlx::MySql;
                $body
            }
        }
    };
}

/// Rows of three texts.
async fn fetch_triples(
    pool: &Pool,
    sql: &str,
    binds: &[String],
) -> Result<Vec<(String, String, String)>, sqlx::Error> {
    on_pool!(pool, p, Db => {
        let mut query = sqlx::query_as::<Db, (String, String, String)>(sql);
        for value in binds {
            query = query.bind(value.clone());
        }
        query.fetch_all(p).await
    })
}

/// Rows of two texts.
async fn fetch_pairs(
    pool: &Pool,
    sql: &str,
    binds: &[String],
) -> Result<Vec<(String, String)>, sqlx::Error> {
    on_pool!(pool, p, Db => {
        let mut query = sqlx::query_as::<Db, (String, String)>(sql);
        for value in binds {
            query = query.bind(value.clone());
        }
        query.fetch_all(p).await
    })
}

/// Rows of one text.
async fn fetch_texts(pool: &Pool, sql: &str, binds: &[String]) -> Result<Vec<String>, sqlx::Error> {
    on_pool!(pool, p, Db => {
        let mut query = sqlx::query_scalar::<Db, String>(sql);
        for value in binds {
            query = query.bind(value.clone());
        }
        query.fetch_all(p).await
    })
}

/// A table as the drive knows it: where it is, its columns, what names its rows.
#[derive(Clone)]
struct Table {
    reference: TableRef,
    columns: Vec<Column>,
    key: RowKey,
}

/// A database as a [`Drive`]: its tables are folders, every row a JSON file, a table's rows a
/// CSV file (module docs). Read-only.
pub struct DatabaseDrive {
    engine: DatabaseEngine,
    dialect: Dialect,
    /// What the messages call the database: the file's name, `shop on db.example`.
    label: String,
    connect: Connect,
    /// The connections, opened on the first call (on the storage runtime).
    pool: Mutex<Option<Pool>>,
    /// The tables of the last listing of the root, by folder.
    folders: Mutex<HashMap<String, TableRef>>,
    /// The columns and key of each table asked for, by folder.
    tables: Mutex<HashMap<String, Table>>,
    /// Each table's CSV as its folder's last listing made it, by folder.
    csv: Mutex<HashMap<String, Arc<Vec<u8>>>>,
}

impl fmt::Debug for DatabaseDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseDrive")
            .field("engine", &self.engine)
            .field("database", &self.label)
            .finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A setting of the connection, trimmed; `None` when it is empty.
fn setting<'a>(options: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    options
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn missing(what: &str) -> DriveError {
    DriveError::InvalidConfig(format!("the database's settings need {what}"))
}

/// A server's host, port, database and user.
fn server(
    options: &BTreeMap<String, String>,
    default_port: u16,
) -> Result<(&str, u16, &str, &str), DriveError> {
    let host = setting(options, "host").ok_or_else(|| missing("its host"))?;
    let database = setting(options, "database").ok_or_else(|| missing("the database's name"))?;
    let user = setting(options, "user").ok_or_else(|| missing("a user"))?;
    let port = match setting(options, "port") {
        None => default_port,
        Some(text) => text.parse::<u16>().map_err(|_| {
            DriveError::InvalidConfig(format!(
                "the port \"{text}\" is not a number from 1 to 65535"
            ))
        })?,
    };
    Ok((host, port, database, user))
}

/// The password, when there is one.
fn password(secrets: &SecretOptions) -> Option<&str> {
    secrets.get("password").filter(|p| !p.is_empty())
}

fn sqlite_connect(options: &BTreeMap<String, String>) -> Result<(Connect, String), DriveError> {
    let path = setting(options, "path").ok_or_else(|| missing("the path of the SQLite file"))?;
    let path = PathBuf::from(path);
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => {
            return Err(DriveError::InvalidConfig(format!(
                "{} is a folder, not a SQLite database file",
                path.display()
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DriveError::NotFound {
                key: path.display().to_string(),
            })
        }
        Err(e) => return Err(DriveError::Io(format!("{}: {e}", path.display()))),
    }
    let label = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let connect = SqliteConnectOptions::new()
        .filename(&path)
        .read_only(true)
        .create_if_missing(false);
    Ok((Connect::Sqlite(connect), label))
}

fn postgres_connect(
    options: &BTreeMap<String, String>,
    secrets: &SecretOptions,
) -> Result<(Connect, String), DriveError> {
    let (host, port, database, user) = server(options, 5432)?;
    let ssl = match setting(options, "sslmode").unwrap_or("prefer") {
        "prefer" => PgSslMode::Prefer,
        "disable" => PgSslMode::Disable,
        "allow" => PgSslMode::Allow,
        "require" => PgSslMode::Require,
        "verify-ca" => PgSslMode::VerifyCa,
        "verify-full" => PgSslMode::VerifyFull,
        other => {
            return Err(DriveError::InvalidConfig(format!(
                "the TLS mode \"{other}\" is none of prefer, disable, require, verify-full"
            )))
        }
    };
    let mut connect = PgConnectOptions::new()
        .host(host)
        .port(port)
        .database(database)
        .username(user)
        .ssl_mode(ssl)
        .application_name("azul-storage");
    if let Some(password) = password(secrets) {
        connect = connect.password(password);
    }
    Ok((Connect::Postgres(connect), format!("{database} on {host}")))
}

fn mysql_connect(
    options: &BTreeMap<String, String>,
    secrets: &SecretOptions,
) -> Result<(Connect, String), DriveError> {
    let (host, port, database, user) = server(options, 3306)?;
    let ssl = match setting(options, "sslmode").unwrap_or("preferred") {
        "preferred" => MySqlSslMode::Preferred,
        "disabled" => MySqlSslMode::Disabled,
        "required" => MySqlSslMode::Required,
        "verify-ca" => MySqlSslMode::VerifyCa,
        "verify-identity" => MySqlSslMode::VerifyIdentity,
        other => {
            return Err(DriveError::InvalidConfig(format!(
                "the TLS mode \"{other}\" is none of preferred, disabled, required, \
                 verify-identity"
            )))
        }
    };
    let mut connect = MySqlConnectOptions::new()
        .host(host)
        .port(port)
        .database(database)
        .username(user)
        .ssl_mode(ssl);
    if let Some(password) = password(secrets) {
        connect = connect.password(password);
    }
    Ok((Connect::Mysql(connect), format!("{database} on {host}")))
}

/// An object of a listing: a file of the drive and its size.
fn object(key: String, size: usize) -> ObjectInfo {
    ObjectInfo {
        key,
        size: size as u64,
        modified: None,
        etag: None,
    }
}

impl DatabaseDrive {
    /// The database `options` describe (the catalog's form: `path` for SQLite; `host`,
    /// `port`, `database`, `user`, `sslmode` for PostgreSQL and MySQL), with its `password`
    /// from `secrets`. Checks the settings - a SQLite file must be there - and connects to
    /// nothing yet.
    pub fn open(
        engine: DatabaseEngine,
        options: &BTreeMap<String, String>,
        secrets: &SecretOptions,
    ) -> Result<DatabaseDrive, DriveError> {
        let (connect, label) = match engine {
            DatabaseEngine::Sqlite => sqlite_connect(options)?,
            DatabaseEngine::Postgres => postgres_connect(options, secrets)?,
            DatabaseEngine::Mysql => mysql_connect(options, secrets)?,
        };
        Ok(DatabaseDrive {
            engine,
            dialect: Dialect::of(engine),
            label,
            connect,
            pool: Mutex::new(None),
            folders: Mutex::new(HashMap::new()),
            tables: Mutex::new(HashMap::new()),
            csv: Mutex::new(HashMap::new()),
        })
    }

    /// The connections, opened on first use. Call it on the storage runtime (inside
    /// [`runtime::block_on`]): a pool starts its housekeeping task there.
    fn pool(&self) -> Pool {
        let mut slot = lock(&self.pool);
        if let Some(pool) = slot.as_ref() {
            return pool.clone();
        }
        let pool = match &self.connect {
            Connect::Sqlite(options) => Pool::Sqlite(
                SqlitePoolOptions::new()
                    .max_connections(MAX_CONNECTIONS)
                    .acquire_timeout(ACQUIRE_TIMEOUT)
                    .connect_lazy_with(options.clone()),
            ),
            Connect::Postgres(options) => Pool::Postgres(
                PgPoolOptions::new()
                    .max_connections(MAX_CONNECTIONS)
                    .acquire_timeout(ACQUIRE_TIMEOUT)
                    .connect_lazy_with(options.clone()),
            ),
            Connect::Mysql(options) => Pool::Mysql(
                MySqlPoolOptions::new()
                    .max_connections(MAX_CONNECTIONS)
                    .acquire_timeout(ACQUIRE_TIMEOUT)
                    .connect_lazy_with(options.clone()),
            ),
        };
        *slot = Some(pool.clone());
        pool
    }

    /// Why a call failed, as a sentence (never with the password): a refused login is
    /// [`DriveError::Denied`], a table or row that is not there [`DriveError::NotFound`]
    /// (`what` names it), no answer [`DriveError::Transport`].
    fn failure(&self, error: sqlx::Error, what: &str) -> DriveError {
        let label = &self.label;
        match error {
            sqlx::Error::RowNotFound => DriveError::NotFound {
                key: what.to_string(),
            },
            sqlx::Error::Database(db) => {
                let code = sqlx::error::DatabaseError::code(&*db)
                    .map(|c| c.into_owned())
                    .unwrap_or_default();
                let message = sqlx::error::DatabaseError::message(&*db).to_string();
                let lower = message.to_ascii_lowercase();
                if code.starts_with("28")
                    || code == "42501"
                    || lower.contains("access denied")
                    || lower.contains("authentication failed")
                {
                    DriveError::Denied {
                        message: format!("{label} refused: {message}"),
                    }
                } else if code == "42P01"
                    || code == "42S02"
                    || lower.contains("no such table")
                    || lower.contains("doesn't exist") && lower.contains("table")
                {
                    DriveError::NotFound {
                        key: what.to_string(),
                    }
                } else if code == "3D000" || lower.contains("unknown database") {
                    DriveError::InvalidConfig(format!("{label}: {message}"))
                } else {
                    DriveError::Protocol(format!("{label}: {message}"))
                }
            }
            sqlx::Error::Io(e) => DriveError::Transport(format!("{label}: {e}")),
            sqlx::Error::Tls(e) => DriveError::Transport(format!("{label}: TLS: {e}")),
            sqlx::Error::PoolTimedOut => {
                DriveError::Transport(format!("{label} did not answer in time"))
            }
            sqlx::Error::Configuration(e) => DriveError::InvalidConfig(format!("{label}: {e}")),
            other => DriveError::Protocol(format!("{label}: {other}")),
        }
    }

    /// What a write is answered with.
    fn read_only(&self, key: &str) -> DriveError {
        DriveError::Denied {
            message: format!(
                "\"{key}\" is in the database {}, which is browsed read-only",
                self.label
            ),
        }
    }

    /// Every table and view, by folder, in the folders' order (and they are what the folders
    /// name from now on; the columns are read again).
    async fn list_tables(&self, pool: &Pool) -> Result<Vec<(String, TableRef)>, DriveError> {
        let rows = fetch_triples(pool, self.dialect.list_tables_sql(), &[])
            .await
            .map_err(|e| self.failure(e, ""))?;
        let mut found: Vec<(String, TableRef)> = rows
            .into_iter()
            .map(|(schema, name, kind)| {
                let table = TableRef {
                    view: tables::is_view(&kind),
                    schema,
                    name,
                };
                (tables::folder_name(self.dialect, &table), table)
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.dedup_by(|a, b| a.0 == b.0);
        *lock(&self.folders) = found.iter().cloned().collect();
        lock(&self.tables).clear();
        Ok(found)
    }

    /// The table a folder names.
    async fn resolve(&self, pool: &Pool, folder: &str) -> Result<TableRef, DriveError> {
        let known = lock(&self.folders).get(folder).cloned();
        if let Some(table) = known {
            return Ok(table);
        }
        let not_found = || DriveError::NotFound {
            key: format!("{folder}/"),
        };
        let binds = self.dialect.find_table_binds(folder).ok_or_else(not_found)?;
        let rows = fetch_triples(pool, self.dialect.find_table_sql(), &binds)
            .await
            .map_err(|e| self.failure(e, folder))?;
        let (schema, name, kind) = rows.into_iter().next().ok_or_else(not_found)?;
        let table = TableRef {
            view: tables::is_view(&kind),
            schema,
            name,
        };
        // Only the table this folder shows (PostgreSQL: `a.b` of `public` is not `b` of `a`).
        if tables::folder_name(self.dialect, &table) != folder {
            return Err(not_found());
        }
        lock(&self.folders).insert(folder.to_string(), table.clone());
        Ok(table)
    }

    /// A table's columns, in their order.
    async fn columns(&self, pool: &Pool, table: &TableRef) -> Result<Vec<Column>, sqlx::Error> {
        let sql = self.dialect.columns_sql();
        let binds = self.dialect.columns_binds(table);
        match pool {
            Pool::Sqlite(p) => {
                let mut query = sqlx::query_as::<sqlx::Sqlite, (String, String, i64, i64)>(sql);
                for value in &binds {
                    query = query.bind(value.clone());
                }
                Ok(query
                    .fetch_all(p)
                    .await?
                    .into_iter()
                    .map(|(name, data_type, not_null, key)| Column {
                        name,
                        data_type,
                        nullable: not_null == 0,
                        primary_key: u32::try_from(key).unwrap_or(0),
                    })
                    .collect())
            }
            Pool::Postgres(p) => {
                let mut query = sqlx::query_as::<sqlx::Postgres, (String, String, bool, i32)>(sql);
                for value in &binds {
                    query = query.bind(value.clone());
                }
                Ok(query
                    .fetch_all(p)
                    .await?
                    .into_iter()
                    .map(|(name, data_type, nullable, key)| Column {
                        name,
                        data_type,
                        nullable,
                        primary_key: u32::try_from(key).unwrap_or(0),
                    })
                    .collect())
            }
            Pool::Mysql(p) => {
                let mut query = sqlx::query_as::<sqlx::MySql, (String, String, i64, i64)>(sql);
                for value in &binds {
                    query = query.bind(value.clone());
                }
                Ok(query
                    .fetch_all(p)
                    .await?
                    .into_iter()
                    .map(|(name, data_type, nullable, key)| Column {
                        name,
                        data_type,
                        nullable: nullable != 0,
                        primary_key: u32::try_from(key).unwrap_or(0),
                    })
                    .collect())
            }
        }
    }

    /// The table a folder names, with its columns and key (read again when `fresh`).
    async fn table(&self, pool: &Pool, folder: &str, fresh: bool) -> Result<Table, DriveError> {
        let known = if fresh {
            lock(&self.tables).remove(folder);
            None
        } else {
            lock(&self.tables).get(folder).cloned()
        };
        if let Some(table) = known {
            return Ok(table);
        }
        let reference = self.resolve(pool, folder).await?;
        let columns = self
            .columns(pool, &reference)
            .await
            .map_err(|e| self.failure(e, folder))?;
        if columns.is_empty() {
            return Err(DriveError::NotFound {
                key: format!("{folder}/"),
            });
        }
        let key = RowKey::for_table(self.dialect, &columns, reference.view);
        let table = Table {
            reference,
            columns,
            key,
        };
        lock(&self.tables).insert(folder.to_string(), table.clone());
        Ok(table)
    }

    /// `limit` rows from `offset` in key order, as (file name without `.json`, JSON text).
    async fn rows(
        &self,
        pool: &Pool,
        folder: &str,
        table: &Table,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<(String, String)>, DriveError> {
        let sql = self
            .dialect
            .rows_sql(&table.reference, &table.columns, &table.key, limit, offset);
        let found = fetch_pairs(pool, &sql, &[])
            .await
            .map_err(|e| self.failure(e, folder))?;
        Ok(found
            .into_iter()
            .zip(offset + 1..)
            .map(|((key_json, json), place)| (tables::row_name(&table.key, &key_json, place), json))
            .collect())
    }

    /// One row's JSON text, by its file's name (without `.json`).
    async fn row(
        &self,
        pool: &Pool,
        folder: &str,
        table: &Table,
        name: &str,
    ) -> Result<String, DriveError> {
        let key = TablePath::Row {
            folder: folder.to_string(),
            name: name.to_string(),
        }
        .key();
        let Some(lookup) = tables::parse_row_name(name, &table.key) else {
            return Err(DriveError::NotFound { key });
        };
        let (sql, binds) =
            self.dialect
                .row_sql(&table.reference, &table.columns, &table.key, &lookup);
        let found = fetch_texts(pool, &sql, &binds)
            .await
            .map_err(|e| self.failure(e, &key))?;
        found
            .into_iter()
            .next()
            .ok_or(DriveError::NotFound { key })
    }

    /// A table's CSV: the one its folder's last listing made, or (`fresh`, or none yet) a new
    /// one of its first rows.
    async fn csv(
        &self,
        pool: &Pool,
        folder: &str,
        table: &Table,
        fresh: bool,
    ) -> Result<Arc<Vec<u8>>, DriveError> {
        if !fresh {
            let known = lock(&self.csv).get(folder).cloned();
            if let Some(bytes) = known {
                return Ok(bytes);
            }
        }
        let rows = self
            .rows(pool, folder, table, tables::CSV_MAX_ROWS, 0)
            .await?;
        let values: Vec<Value> = rows
            .iter()
            .map(|(_, json)| serde_json::from_str(json).unwrap_or(Value::Null))
            .collect();
        let bytes = Arc::new(tables::csv_text(&table.columns, &values).into_bytes());
        lock(&self.csv).insert(folder.to_string(), bytes.clone());
        Ok(bytes)
    }

    /// A table's `schema.json`.
    fn schema(&self, table: &Table) -> String {
        tables::schema_json(
            self.engine.key(),
            &table.reference,
            &table.columns,
            &table.key,
        )
    }

    /// One folder level (`request.prefix` a folder, or a folder and the start of a name).
    async fn list_folder(&self, pool: &Pool, request: &ListRequest) -> Result<ListPage, DriveError> {
        let prefix = request.prefix.as_str();
        let page_size = request.page_size();
        let continuation = request.continuation.as_deref();
        let Some(path) = TablePath::parse(folder_of(prefix)) else {
            return Ok(ListPage::default());
        };
        match path {
            TablePath::Root => {
                let folders: Vec<String> = self
                    .list_tables(pool)
                    .await?
                    .into_iter()
                    .map(|(folder, _)| format!("{folder}/"))
                    .filter(|folder| folder.starts_with(prefix))
                    .collect();
                let (start, end, next) = tables::page_bounds(folders.len(), page_size, continuation)?;
                Ok(ListPage {
                    folders: folders[start..end].to_vec(),
                    objects: Vec::new(),
                    next,
                })
            }
            TablePath::Table { folder } => {
                if continuation.is_some() {
                    // The folder fits on one page.
                    return Ok(ListPage::default());
                }
                let table = self.table(pool, &folder, true).await?;
                let csv = self.csv(pool, &folder, &table, true).await?;
                let schema = self.schema(&table);
                let mut objects = vec![
                    object(TablePath::Csv { folder: folder.clone() }.key(), csv.len()),
                    object(
                        TablePath::Schema {
                            folder: folder.clone(),
                        }
                        .key(),
                        schema.len(),
                    ),
                ];
                objects.retain(|o| o.key.starts_with(prefix));
                let mut folders = vec![TablePath::Rows { folder }.key()];
                folders.retain(|f| f.starts_with(prefix));
                Ok(ListPage {
                    folders,
                    objects,
                    next: None,
                })
            }
            TablePath::Rows { folder } => {
                let offset = match continuation {
                    None => 0,
                    Some(token) => token.trim().parse::<u64>().map_err(|_| {
                        DriveError::Protocol(String::from(
                            "the listing's continuation token is not one of this drive's",
                        ))
                    })?,
                };
                if offset >= tables::MAX_ROW_FILES {
                    return Ok(ListPage::default());
                }
                let table = self.table(pool, &folder, false).await?;
                let limit = u64::from(page_size).min(tables::MAX_ROW_FILES - offset);
                // One row more than the page: is there another page?
                let mut rows = self.rows(pool, &folder, &table, limit + 1, offset).await?;
                let more = rows.len() as u64 > limit && offset + limit < tables::MAX_ROW_FILES;
                rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
                let objects: Vec<ObjectInfo> = rows
                    .into_iter()
                    .map(|(name, json)| {
                        object(
                            TablePath::Row {
                                folder: folder.clone(),
                                name,
                            }
                            .key(),
                            json.len(),
                        )
                    })
                    .filter(|o| o.key.starts_with(prefix))
                    .collect();
                Ok(ListPage {
                    folders: Vec::new(),
                    objects,
                    next: more.then(|| (offset + limit).to_string()),
                })
            }
            // A file's name is no folder.
            TablePath::Csv { .. } | TablePath::Schema { .. } | TablePath::Row { .. } => {
                Ok(ListPage::default())
            }
        }
    }

    /// Every file under `request.prefix` at any depth (S3 semantics: no folders): the tables'
    /// CSV and schema files and their rows' files.
    async fn list_everything(
        &self,
        pool: &Pool,
        request: &ListRequest,
    ) -> Result<ListPage, DriveError> {
        let prefix = request.prefix.as_str();
        let folders: Vec<String> = match TablePath::parse(folder_of(prefix)) {
            Some(TablePath::Root) => self
                .list_tables(pool)
                .await?
                .into_iter()
                .map(|(folder, _)| folder)
                .collect(),
            Some(TablePath::Table { folder } | TablePath::Rows { folder }) => vec![folder],
            _ => Vec::new(),
        };
        let mut objects = Vec::new();
        for folder in folders {
            let table = self.table(pool, &folder, false).await?;
            let csv = self.csv(pool, &folder, &table, false).await?;
            objects.push(object(
                TablePath::Csv {
                    folder: folder.clone(),
                }
                .key(),
                csv.len(),
            ));
            objects.push(object(
                TablePath::Schema {
                    folder: folder.clone(),
                }
                .key(),
                self.schema(&table).len(),
            ));
            let mut offset = 0;
            while offset < tables::MAX_ROW_FILES {
                let limit = tables::ROWS_PAGE.min(tables::MAX_ROW_FILES - offset);
                let rows = self.rows(pool, &folder, &table, limit, offset).await?;
                let count = rows.len() as u64;
                objects.extend(rows.into_iter().map(|(name, json)| {
                    object(
                        TablePath::Row {
                            folder: folder.clone(),
                            name,
                        }
                        .key(),
                        json.len(),
                    )
                }));
                if count < limit {
                    break;
                }
                offset += count;
            }
        }
        objects.retain(|o| o.key.starts_with(prefix));
        objects.sort_by(|a, b| a.key.cmp(&b.key));
        let (start, end, next) = tables::page_bounds(
            objects.len(),
            request.page_size(),
            request.continuation.as_deref(),
        )?;
        Ok(ListPage {
            folders: Vec::new(),
            objects: objects[start..end].to_vec(),
            next,
        })
    }

    async fn list_now(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let pool = self.pool();
        if request.delimiter.is_none() {
            self.list_everything(&pool, request).await
        } else {
            self.list_folder(&pool, request).await
        }
    }

    /// A file's bytes.
    async fn get_now(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let pool = self.pool();
        let Some(path) = TablePath::parse(key) else {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        };
        match path {
            TablePath::Csv { folder } => {
                let table = self.table(&pool, &folder, false).await?;
                Ok(self.csv(&pool, &folder, &table, false).await?.to_vec())
            }
            TablePath::Schema { folder } => {
                let table = self.table(&pool, &folder, false).await?;
                Ok(self.schema(&table).into_bytes())
            }
            TablePath::Row { folder, name } => {
                let table = self.table(&pool, &folder, false).await?;
                Ok(self.row(&pool, &folder, &table, &name).await?.into_bytes())
            }
            TablePath::Root | TablePath::Table { .. } | TablePath::Rows { .. } => {
                Err(DriveError::InvalidKey {
                    key: key.to_string(),
                    reason: "it names a folder, not a file",
                })
            }
        }
    }

    /// A file's size.
    async fn head_now(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let pool = self.pool();
        let Some(path) = TablePath::parse(key) else {
            return Err(DriveError::NotFound {
                key: key.to_string(),
            });
        };
        let size = match path {
            TablePath::Csv { folder } => {
                let table = self.table(&pool, &folder, false).await?;
                self.csv(&pool, &folder, &table, false).await?.len()
            }
            TablePath::Schema { folder } => {
                let table = self.table(&pool, &folder, false).await?;
                self.schema(&table).len()
            }
            TablePath::Row { folder, name } => {
                let table = self.table(&pool, &folder, false).await?;
                self.row(&pool, &folder, &table, &name).await?.len()
            }
            TablePath::Root | TablePath::Table { .. } | TablePath::Rows { .. } => {
                return Err(DriveError::InvalidKey {
                    key: key.to_string(),
                    reason: "it names a folder, not a file",
                })
            }
        };
        Ok(object(key.to_string(), size))
    }
}

impl Drive for DatabaseDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        runtime::block_on(self.list_now(request))?
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        runtime::block_on(self.get_now(key))?
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.get(key)?;
        tables::byte_range(&bytes, range, key)
    }

    fn put(&self, key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
        Err(self.read_only(key))
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        Err(self.read_only(key))
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        runtime::block_on(self.head_now(key))?
    }

    fn copy(&self, _from: &str, to: &str) -> Result<(), DriveError> {
        Err(self.read_only(to))
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        Err(self.read_only(prefix))
    }

    fn rename(&self, from: &str, _to: &str) -> Result<(), DriveError> {
        Err(self.read_only(from))
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        Err(self.read_only(prefix))
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.head(key)?;
        let mut out = vec![
            (String::from("Database"), self.label.clone()),
            (String::from("Engine"), self.engine.name().to_string()),
        ];
        if let Some(folder) = TablePath::parse(key).as_ref().and_then(TablePath::folder) {
            let table = tables::unescape_segment(folder).unwrap_or_else(|| folder.to_string());
            out.push((String::from("Table"), table));
        }
        Ok(out)
    }
}

impl Drop for DatabaseDrive {
    fn drop(&mut self) {
        // A pool's connections go back to the runtime they were opened on.
        let pool = self
            .pool
            .get_mut()
            .map(Option::take)
            .unwrap_or_else(|poisoned| poisoned.into_inner().take());
        if let Some(pool) = pool {
            if let Ok(runtime) = runtime::shared() {
                let _context = runtime.enter();
                drop(pool);
            }
        }
    }
}
