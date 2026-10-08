//! The drives a user added, shared by the apps (AzDrive shows them, AzMail
//! exports to one): `<config dir>/azul-storage/drives.json`, WITHOUT secrets.
//! Each drive's secrets are one OS keyring entry, [`keyring_key`]: an S3
//! drive's [`Credentials::to_keyring_secret`], an Azlin drive's session
//! (azcloud-kit's; it reads as credentials too), a data source's
//! [`SecretOptions`].
//!
//! ```json
//! { "format": "azul-storage.drives", "version": 1, "drives": [
//!   { "id": "k3f9...", "name": "S3 Drive",
//!     "location": { "kind": "s3", "endpoint": "https://s3.eu-central-1.amazonaws.com",
//!                   "region": "eu-central-1", "bucket": "felix-azlin", "path_style": false,
//!                   "auth": { "type": "keyring" } } },
//!   { "id": "d_k3f9", "name": "Azlin Storage",
//!     "location": { "kind": "s3", "endpoint": "...", "region": "us-east-1", "bucket": "d-k3f9",
//!                   "path_style": true,
//!                   "auth": { "type": "azlin", "drive_id": "d_k3f9", "account_url": "..." } } },
//!   { "id": "nas-1a2b", "name": "NAS",
//!     "location": { "kind": "opendal", "scheme": "webdav",
//!                   "options": { "endpoint": "https://nas.example/dav", "username": "ann" },
//!                   "keyring": true } },
//!   { "id": "shop-3c4d", "name": "Shop",
//!     "location": { "kind": "database", "engine": "sqlite",
//!                   "options": { "path": "/data/shop.sqlite" } } } ] }
//! ```

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    local::write_atomically, sigv4::sha256_hex, Credentials, Drive, DriveError, LocalDrive,
    S3Config, S3Drive, Transport,
};

/// The `format` of a drives file.
pub const DRIVES_FORMAT: &str = "azul-storage.drives";
/// The newest `version` of a drives file this crate reads and writes.
pub const DRIVES_VERSION: u32 = 1;
/// The variable naming the drives file (a test, a second profile).
pub const DRIVES_VAR: &str = "AZUL_DRIVES";

/// How a drive gets its right to read and write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DriveAuth {
    /// The user's own access key, in the OS keyring under [`keyring_key`].
    #[default]
    Keyring,
    /// A grant from the database's access links (RBAC): the server resolves `link`
    /// to short-lived credentials for the keys under `prefix`, read-only unless
    /// `can_write`. Opened as a [`crate::ScopedDrive`] once the server side exists.
    AccessLink {
        link: String,
        prefix: String,
        can_write: bool,
    },
    /// An Azlin cloud drive, as the token server's bundle describes it: temporary S3
    /// credentials (12 h) and a drive token that rotates on every refresh, both in the keyring
    /// entry [`keyring_key`] as azcloud-kit's session text. azcloud-kit opens it and refreshes
    /// the credentials at `account_url` (the token server the drive came from; empty: the one
    /// the app is configured with). [`DriveEntry::open_with_secret`] opens it with the
    /// credentials the session holds, without refreshing them.
    Azlin {
        drive_id: String,
        #[serde(default)]
        account_url: String,
    },
}

/// The database engines a [`DriveLocation::Database`] can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseEngine {
    Sqlite,
    Postgres,
    Mysql,
}

impl DatabaseEngine {
    /// Every engine, in the order the Add drive dialog lists them.
    pub const ALL: [DatabaseEngine; 3] = [
        DatabaseEngine::Postgres,
        DatabaseEngine::Mysql,
        DatabaseEngine::Sqlite,
    ];

    /// The name in the drives file: `sqlite`, `postgres`, `mysql`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            DatabaseEngine::Sqlite => "sqlite",
            DatabaseEngine::Postgres => "postgres",
            DatabaseEngine::Mysql => "mysql",
        }
    }

    /// What it is called: `SQLite`, `PostgreSQL`, `MySQL`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            DatabaseEngine::Sqlite => "SQLite",
            DatabaseEngine::Postgres => "PostgreSQL",
            DatabaseEngine::Mysql => "MySQL",
        }
    }
}

/// Whether a flag of the drives file is off (it is not written then).
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// Where a drive's files are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriveLocation {
    /// A folder on this computer.
    Local { root: String },
    /// An S3-compatible bucket.
    S3 {
        endpoint: String,
        region: String,
        bucket: String,
        path_style: bool,
        #[serde(default)]
        auth: DriveAuth,
    },
    /// A data source Apache OpenDAL reaches (azul-storage's feature `opendal`): `scheme` names
    /// the service (`webdav`, `gdrive`, `ftp`, ... - [`crate::catalog`] lists them), `options`
    /// holds its PLAIN settings (an endpoint, a user name, a folder); the secret ones
    /// (passwords, tokens, keys) are the keyring entry [`keyring_key`] as [`SecretOptions`]
    /// when `keyring` says there are any.
    Opendal {
        scheme: String,
        #[serde(default)]
        options: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "is_false")]
        keyring: bool,
    },
    /// A database browsed as files (feature `sql`): its tables are folders, every row a JSON
    /// file, a table's rows together a CSV file. `options` holds the connection's plain
    /// settings (`host`, `port`, `database`, `user`, `sslmode`; SQLite's `path`); a password is
    /// the keyring entry as [`SecretOptions`] when `keyring` says so.
    Database {
        engine: DatabaseEngine,
        #[serde(default)]
        options: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "is_false")]
        keyring: bool,
    },
}

/// The secret settings of a data source (a password, a token, an account key), by the name of
/// the setting: the ONE text its keyring entry holds (JSON). `Debug` shows the names, never a
/// value.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretOptions(BTreeMap<String, String>);

impl SecretOptions {
    #[must_use]
    pub fn new() -> Self {
        SecretOptions(BTreeMap::new())
    }

    /// Sets the secret setting `key`.
    pub fn insert(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }

    /// The secret setting `key`, if there is one.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every secret setting, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// The text of the keyring entry (JSON).
    #[must_use]
    pub fn to_keyring_secret(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_default()
    }

    /// Reads [`Self::to_keyring_secret`] back.
    pub fn from_keyring_secret(secret: &str) -> Result<Self, DriveError> {
        // The parser's message is not passed on: it could quote the secret.
        serde_json::from_str::<BTreeMap<String, String>>(secret)
            .map(SecretOptions)
            .map_err(|_| {
                DriveError::InvalidConfig(String::from(
                    "the keyring entry does not hold a data source's settings",
                ))
            })
    }
}

impl fmt::Debug for SecretOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.0.keys().map(|k| (k, "<hidden>")))
            .finish()
    }
}

impl FromIterator<(String, String)> for SecretOptions {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        SecretOptions(iter.into_iter().collect())
    }
}

/// One drive of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveEntry {
    /// Stable, unique in the list; names the keyring entry.
    pub id: String,
    /// What the user called it: "S3 Drive".
    pub name: String,
    pub location: DriveLocation,
}

impl DriveEntry {
    /// The drive this entry describes. An S3 drive needs `credentials` (from the
    /// keyring; an Azlin drive's are the ones its session holds, used as they are); a
    /// local one ignores them and the transport. A data source (OpenDAL, a database)
    /// opens with [`Self::open_with_secret`].
    pub fn open(
        &self,
        credentials: Option<Credentials>,
        transport: Box<dyn Transport>,
    ) -> Result<Box<dyn Drive>, DriveError> {
        match &self.location {
            // A folder the user added is not the data tree: no `.azlin/` there.
            DriveLocation::Local { root } => Ok(Box::new(LocalDrive::without_manifest(
                PathBuf::from(root),
            ))),
            DriveLocation::Opendal { .. } | DriveLocation::Database { .. } => {
                self.open_with_secret(None, transport)
            }
            DriveLocation::S3 { auth, .. } => match auth {
                DriveAuth::Keyring | DriveAuth::Azlin { .. } => {
                    let credentials = credentials.ok_or_else(|| DriveError::Denied {
                        message: format!(
                            "\"{}\" has no credentials (the keyring has no entry for it)",
                            self.name
                        ),
                    })?;
                    let config = self.s3_config().ok_or_else(|| {
                        DriveError::InvalidConfig(String::from("not an S3 drive"))
                    })?;
                    Ok(Box::new(S3Drive::new(config, credentials, transport)?))
                }
                DriveAuth::AccessLink { .. } => Err(DriveError::Unsupported(String::from(
                    "drives from access links open once the access-link server exists; it \
                     resolves the link to short-lived credentials and a ScopedDrive",
                ))),
            },
        }
    }

    /// The drive this entry describes, with the text of its keyring entry (`None`: there is
    /// none): an S3 drive's credentials, an Azlin drive's session (its credentials are used as
    /// they are - azcloud-kit's drive refreshes them), a data source's [`SecretOptions`]. A
    /// local folder needs nothing. Sends nothing: an OpenDAL source or a database checks its
    /// settings here and connects on its first call.
    pub fn open_with_secret(
        &self,
        secret: Option<&str>,
        transport: Box<dyn Transport>,
    ) -> Result<Box<dyn Drive>, DriveError> {
        match &self.location {
            DriveLocation::Local { .. } => self.open(None, transport),
            DriveLocation::S3 { .. } => {
                let credentials = secret.map(Credentials::from_keyring_secret).transpose()?;
                self.open(credentials, transport)
            }
            DriveLocation::Opendal {
                scheme,
                options,
                keyring,
            } => {
                let secrets = self.secret_options(secret, *keyring)?;
                #[cfg(feature = "opendal")]
                {
                    Ok(Box::new(crate::opendal_drive::OpendalDrive::open(
                        scheme, options, &secrets, transport,
                    )?))
                }
                #[cfg(not(feature = "opendal"))]
                {
                    let _ = (options, secrets, transport);
                    Err(DriveError::Unsupported(format!(
                        "\"{}\" is a {scheme} source, and this app was built without OpenDAL \
                         (azul-storage's feature \"opendal\")",
                        self.name
                    )))
                }
            }
            DriveLocation::Database {
                engine,
                options,
                keyring,
            } => {
                let secrets = self.secret_options(secret, *keyring)?;
                #[cfg(feature = "sql")]
                {
                    let _ = transport;
                    Ok(Box::new(crate::database::DatabaseDrive::open(
                        *engine, options, &secrets,
                    )?))
                }
                #[cfg(not(feature = "sql"))]
                {
                    let _ = (options, secrets, transport);
                    Err(DriveError::Unsupported(format!(
                        "\"{}\" is a {} database, and this app was built without the database \
                         drivers (azul-storage's feature \"sql\")",
                        self.name,
                        engine.name()
                    )))
                }
            }
        }
    }

    /// A data source's secret settings: its keyring text read back, or none when the entry
    /// keeps none (`keyring` off).
    fn secret_options(
        &self,
        secret: Option<&str>,
        keyring: bool,
    ) -> Result<SecretOptions, DriveError> {
        match secret {
            Some(text) if !text.trim().is_empty() => SecretOptions::from_keyring_secret(text),
            _ if keyring => Err(DriveError::Denied {
                message: format!(
                    "\"{}\" has no password or token (the keyring has no entry for it)",
                    self.name
                ),
            }),
            _ => Ok(SecretOptions::new()),
        }
    }

    /// The bucket settings of an S3 drive.
    #[must_use]
    pub fn s3_config(&self) -> Option<S3Config> {
        match &self.location {
            DriveLocation::S3 {
                endpoint,
                region,
                bucket,
                path_style,
                ..
            } => Some(S3Config {
                endpoint: endpoint.clone(),
                region: region.clone(),
                bucket: bucket.clone(),
                path_style: *path_style,
            }),
            DriveLocation::Local { .. }
            | DriveLocation::Opendal { .. }
            | DriveLocation::Database { .. } => None,
        }
    }

    /// Whether opening it needs its keyring entry: an S3 drive's keys, an Azlin drive's
    /// session, a data source's secret settings (when it has any).
    #[must_use]
    pub fn needs_keyring(&self) -> bool {
        match &self.location {
            DriveLocation::S3 { auth, .. } => {
                matches!(auth, DriveAuth::Keyring | DriveAuth::Azlin { .. })
            }
            DriveLocation::Opendal { keyring, .. } | DriveLocation::Database { keyring, .. } => {
                *keyring
            }
            DriveLocation::Local { .. } => false,
        }
    }

    /// The Azlin drive's id and the token server it came from (empty: the app's), for an
    /// Azlin drive.
    #[must_use]
    pub fn azlin(&self) -> Option<(&str, &str)> {
        match &self.location {
            DriveLocation::S3 {
                auth:
                    DriveAuth::Azlin {
                        drive_id,
                        account_url,
                    },
                ..
            } => Some((drive_id.as_str(), account_url.as_str())),
            _ => None,
        }
    }
}

/// The drives file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrivesFile {
    pub format: String,
    pub version: u32,
    pub drives: Vec<DriveEntry>,
}

impl DrivesFile {
    #[must_use]
    pub fn empty() -> Self {
        DrivesFile {
            format: DRIVES_FORMAT.to_string(),
            version: DRIVES_VERSION,
            drives: Vec::new(),
        }
    }

    /// Reads a drives file's text; another format or a newer version is an error.
    pub fn parse(json: &str) -> Result<Self, DriveError> {
        let file: DrivesFile = serde_json::from_str(json).map_err(|e| {
            DriveError::InvalidConfig(format!("the drives file cannot be read: {e}"))
        })?;
        if file.format != DRIVES_FORMAT {
            return Err(DriveError::InvalidConfig(format!(
                "the drives file is \"{}\", not \"{DRIVES_FORMAT}\"",
                file.format
            )));
        }
        if file.version == 0 || file.version > DRIVES_VERSION {
            return Err(DriveError::InvalidConfig(format!(
                "the drives file is version {}; this app reads up to version {DRIVES_VERSION}",
                file.version
            )));
        }
        Ok(file)
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Reads the file at `path`; a missing file is an empty list.
    pub fn load(path: &Path) -> Result<Self, DriveError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::empty()),
            Err(e) => Err(DriveError::Io(format!("{}: {e}", path.display()))),
        }
    }

    /// Writes the file (its folder is created), through a temporary file.
    pub fn save(&self, path: &Path) -> Result<(), DriveError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        write_atomically(path, self.to_json().as_bytes())
            .map_err(|e| DriveError::Io(format!("{}: {e}", path.display())))
    }

    /// Adds `entry`, or replaces the drive with its id.
    pub fn add(&mut self, entry: DriveEntry) {
        match self.drives.iter_mut().find(|d| d.id == entry.id) {
            Some(existing) => *existing = entry,
            None => self.drives.push(entry),
        }
    }

    /// Forgets the drive with this id.
    pub fn remove(&mut self, id: &str) -> Option<DriveEntry> {
        let index = self.drives.iter().position(|d| d.id == id)?;
        Some(self.drives.remove(index))
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&DriveEntry> {
        self.drives.iter().find(|d| d.id == id)
    }
}

/// The drives file: `var` (the [`DRIVES_VAR`] variable) when set, else
/// `<config_dir>/azul-storage/drives.json`.
#[must_use]
pub fn drives_file(var: Option<&str>, config_dir: Option<PathBuf>) -> Option<PathBuf> {
    match var.map(str::trim).filter(|v| !v.is_empty()) {
        Some(path) => Some(PathBuf::from(path)),
        None => config_dir.map(|dir| dir.join("azul-storage").join("drives.json")),
    }
}

/// The OS keyring entry holding a drive's credentials.
#[must_use]
pub fn keyring_key(drive_id: &str) -> String {
    format!("azul-storage/s3/{drive_id}")
}

/// A new drive id: unique, `[0-9a-z-]`, starting with a readable slug of `name`.
#[must_use]
pub fn new_drive_id(name: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if matches!(c, ' ' | '-' | '_' | '.') && !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.len() >= 24 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seed = format!(
        "{name}|{nanos}|{}|{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    let hash = sha256_hex(seed.as_bytes());
    if slug.is_empty() {
        hash[..12].to_string()
    } else {
        format!("{slug}-{}", &hash[..12])
    }
}
