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
    docs::{Doc, DocError, Document},
    sigv4::sha256_hex,
    Credentials, Drive, DriveError, LocalDrive, S3Config, S3Drive, Transport,
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
        /// This device found the drive encrypted - it kept the drive's key, or set it up - and
        /// wrote that down: from then on the drive is never opened as a plain one, whatever its
        /// bucket's listing says (the listing is the storage's word, the key and this record
        /// are the device's own: [`crate::encrypted::AutoEncrypted`]). An entry from before
        /// this record has none until the drive opens encrypted once.
        #[serde(default, skip_serializing_if = "is_false")]
        encrypted: bool,
        /// The id (32 hex digits) of the drive key this device held when it last opened the
        /// drive: a fingerprint of the key, never the key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key_id: Option<String>,
    },
}

/// What the drives file says of an Azlin drive's encryption ([`DriveEntry::encryption_record`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncryptionRecord {
    /// This device found the drive encrypted.
    pub encrypted: bool,
    /// The id of the drive key it held then.
    pub key_id: Option<String>,
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

/// The reserved setting of a data source's keyring text that BINDS its secrets to where they go:
/// [`DriveEntry::secret_origin`] as it was when the secrets were saved. drives.json is a plain
/// file any process of the user can write; the address it holds decides where a password or a
/// refresh token is sent, so a secret is read back only for the address it was typed for
/// (AZ-CN7). Not an OpenDAL setting: it never reaches the service.
pub const ORIGIN_KEY: &str = "@origin";

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

/// The name a drive's entry has in the drives file ([`DriveEntry::id`]: "stable, unique in the list; names the keyring
/// entry"): what the window, the sync pairings and the records written into the drives file call the drive. Not an Azlin
/// drive's id at the token server ([`DriveId`]), which its keys, its vault and its grants are kept under in every app.
/// Where a drive's key and the record of it meet the two are written side by side, and as two `&str` a swap of them
/// looked the key up under the slot's name, which no other app uses (X-DELTA-STRUCT4-3). Two types, so it does not
/// compile:
///
/// ```compile_fail,E0308
/// use azul_storage::config::{DriveId, SlotId};
/// fn wrap(_drive: DriveId<'_>, _slot: SlotId<'_>) {}
/// fn swapped(drive: DriveId<'_>, slot: SlotId<'_>) {
///     wrap(slot, drive)
/// }
/// ```
///
/// Made by [`DriveEntry::slot`] and by nothing else (the entry owns its name):
///
/// ```compile_fail,E0451
/// use azul_storage::config::SlotId;
/// fn made_by_hand() -> SlotId<'static> {
///     SlotId { id: "slot-1" }
/// }
/// ```
///
/// ```no_run
/// // The control of the two above: the right order, with what an entry gives.
/// use azul_storage::config::{DriveEntry, DriveId, SlotId};
/// fn wrap(_drive: DriveId<'_>, _slot: SlotId<'_>) {}
/// fn right(entry: &DriveEntry) {
///     if let Some(drive) = entry.azlin_drive() {
///         wrap(drive, entry.slot());
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotId<'a> {
    id: &'a str,
}

impl<'a> SlotId<'a> {
    /// The name as the drives file spells it.
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        self.id
    }

    /// A slot a test names: no entry stands behind it.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn named(id: &'a str) -> SlotId<'a> {
        SlotId { id }
    }
}

/// An Azlin drive's id at the token server, as an entry of the drives file names it ([`DriveEntry::azlin_drive`]): what
/// the drive's key is kept and bound under, in every app of this computer. Not the slot's name ([`SlotId`]); made by
/// [`DriveEntry::azlin_drive`] and by nothing else (the entry owns what it names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveId<'a> {
    id: &'a str,
}

impl<'a> DriveId<'a> {
    /// The id as the entry spells it.
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        self.id
    }

    /// A drive id a test names: no entry stands behind it.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn named(id: &'a str) -> DriveId<'a> {
        DriveId { id }
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
        self.open_with_secret_rotating(secret, transport, None)
    }

    /// [`Self::open_with_secret`]. A consumer cloud signed in at its provider (Google Drive,
    /// Dropbox, OneDrive with a refresh token and a client id: [`crate::oauth::signed_in`])
    /// keeps its access token fresh itself; when the provider rotates the refresh token
    /// (OneDrive), the drive's new keyring text goes to `rotated`, to be stored under
    /// [`keyring_key`] (without a sink the new token lives as long as the drive).
    pub fn open_with_secret_rotating(
        &self,
        secret: Option<&str>,
        transport: Box<dyn Transport>,
        rotated: Option<crate::oauth::SecretSink>,
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
                    let (options, secrets, transport) =
                        crate::oauth::signed_in(scheme, options, &secrets, transport, rotated);
                    Ok(Box::new(crate::opendal_drive::OpendalDrive::open(
                        scheme, &options, &secrets, transport,
                    )?))
                }
                #[cfg(not(feature = "opendal"))]
                {
                    let _ = (options, secrets, transport, rotated);
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
                    Ok(Box::new(crate::database::DatabaseDrive::open_trusting(
                        *engine,
                        options,
                        &secrets,
                        transport.trusts_public_roots(),
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

    /// Where this drive's secrets are sent, as one text: the scheme, the address (scheme, host and
    /// port - not the path, not a login) and the token endpoint it was signed in at, for a data
    /// source; the engine, the host and the port for a database. `None` for a drive whose secrets
    /// are not bound this way (a folder, a bucket, an Azlin drive). The keyring text of a data
    /// source holds it ([`ORIGIN_KEY`]) from the day its secrets are saved.
    #[must_use]
    pub fn secret_origin(&self) -> Option<String> {
        let origin = |url: Option<&String>| url.map(|url| origin_of(url)).unwrap_or_default();
        match &self.location {
            DriveLocation::Opendal {
                scheme, options, ..
            } => Some(format!(
                "{scheme}|{}|{}",
                origin(options.get("endpoint")),
                origin(options.get(crate::oauth::TOKEN_URL))
            )),
            DriveLocation::Database {
                engine, options, ..
            } => {
                let text = |key: &str| {
                    options
                        .get(key)
                        .map(|value| value.trim().to_ascii_lowercase())
                        .unwrap_or_default()
                };
                Some(format!(
                    "{}|{}|{}",
                    engine.key(),
                    text("host"),
                    text("port")
                ))
            }
            DriveLocation::Local { .. } | DriveLocation::S3 { .. } => None,
        }
    }

    /// A data source's secret settings: its keyring text read back, or none when the entry
    /// keeps none (`keyring` off). Secrets saved for another address than the entry has now are
    /// not handed out (AZ-CN7): a process that edited drives.json to point the drive somewhere
    /// else would get the password sent there.
    fn secret_options(
        &self,
        secret: Option<&str>,
        keyring: bool,
    ) -> Result<SecretOptions, DriveError> {
        match secret {
            Some(text) if !text.trim().is_empty() => {
                let secrets = SecretOptions::from_keyring_secret(text)?;
                self.check_bound(&secrets)?;
                Ok(secrets)
            }
            _ if keyring => Err(DriveError::Denied {
                message: format!(
                    "\"{}\" has no password or token (the keyring has no entry for it)",
                    self.name
                ),
            }),
            _ => Ok(SecretOptions::new()),
        }
    }

    /// Whether `secrets` were saved for the address this entry has: those saved before they were
    /// bound (no [`ORIGIN_KEY`]) are taken as they were; bound ones must match.
    fn check_bound(&self, secrets: &SecretOptions) -> Result<(), DriveError> {
        let Some(bound) = secrets.get(ORIGIN_KEY) else {
            return Ok(());
        };
        if self.secret_origin().as_deref() == Some(bound) {
            return Ok(());
        }
        Err(DriveError::Denied {
            message: format!(
                "\"{}\": its address changed since its password was saved, so the password is not \
                 sent to the new one: open the drive's form and enter it again",
                self.name
            ),
        })
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

    /// The Azlin drive's id at the token server, for an Azlin drive. The address the entry carries
    /// (`account_url`) is not handed out here: the token server a drive's credentials are SENT to is chosen
    /// by azcloud-kit's `azlin_target` and by no other function (X-DELTA-STRUCT3-11; this accessor gave the
    /// address to eight places, and `drives.json` is a file any process of the user can write):
    ///
    /// ```compile_fail,E0308
    /// use azul_storage::config::DriveEntry;
    /// fn address(entry: &DriveEntry) {
    ///     let Some((_id, _url)) = entry.azlin() else { return };
    /// }
    /// ```
    ///
    /// ```no_run
    /// // The control of the one above: the id.
    /// use azul_storage::config::DriveEntry;
    /// fn id(entry: &DriveEntry) -> Option<&str> {
    ///     entry.azlin()
    /// }
    /// ```
    #[must_use]
    pub fn azlin(&self) -> Option<&str> {
        self.azlin_drive().map(|drive| drive.as_str())
    }

    /// An Azlin drive's id at the token server as this entry names it, as the type that is not the slot's name
    /// ([`DriveId`]); `None` for every other drive. The one derivation: [`Self::azlin`] is this one's `&str`.
    #[must_use]
    pub fn azlin_drive(&self) -> Option<DriveId<'_>> {
        match &self.location {
            DriveLocation::S3 {
                auth: DriveAuth::Azlin { drive_id, .. },
                ..
            } => Some(DriveId {
                id: drive_id.as_str(),
            }),
            _ => None,
        }
    }

    /// This entry's name in the drives file ([`SlotId`]).
    #[must_use]
    pub fn slot(&self) -> SlotId<'_> {
        SlotId { id: &self.id }
    }

    /// What this device wrote down of an Azlin drive's encryption; `None` for every other
    /// drive (only an Azlin drive is encrypted by the apps, always).
    #[must_use]
    pub fn encryption_record(&self) -> Option<EncryptionRecord> {
        match &self.location {
            DriveLocation::S3 {
                auth: DriveAuth::Azlin {
                    encrypted, key_id, ..
                },
                ..
            } => Some(EncryptionRecord {
                encrypted: *encrypted,
                key_id: key_id.clone(),
            }),
            _ => None,
        }
    }

    /// The entry without the record of the drive's encryption: that record is this device's own
    /// word (it found the drive encrypted, with this key), written by [`Self::record_encryption`]
    /// and by nothing else. An entry that comes from a server's bundle carries whatever the server
    /// put there, and a server that says "encrypted" with a key id of its own makes a drive that
    /// no key of this device opens.
    #[must_use]
    pub fn without_encryption_record(mut self) -> DriveEntry {
        if let DriveLocation::S3 {
            auth: DriveAuth::Azlin {
                encrypted, key_id, ..
            },
            ..
        } = &mut self.location
        {
            *encrypted = false;
            *key_id = None;
        }
        self
    }

    /// Keeps `kept` - what this device had written down of the drive in the entry this one
    /// replaces - over what this entry says: a drive found encrypted stays found encrypted, with
    /// the key id that was recorded (the entry's own id counts only where none was). Nothing for a
    /// drive that is no Azlin drive, or one the device had not found encrypted.
    pub fn keep_encryption_record(&mut self, kept: EncryptionRecord) {
        if let DriveLocation::S3 {
            auth: DriveAuth::Azlin {
                encrypted, key_id, ..
            },
            ..
        } = &mut self.location
        {
            if kept.encrypted {
                *encrypted = true;
                *key_id = kept.key_id.or_else(|| key_id.take());
            }
        }
    }

    /// Writes down that this device found the Azlin drive encrypted, with the id of its drive
    /// key. Whether the entry changed; `false` as well for a drive that is no Azlin drive.
    pub fn record_encryption(&mut self, key: &str) -> bool {
        let DriveLocation::S3 {
            auth: DriveAuth::Azlin {
                encrypted, key_id, ..
            },
            ..
        } = &mut self.location
        else {
            return false;
        };
        if *encrypted && key_id.as_deref() == Some(key) {
            return false;
        }
        *encrypted = true;
        *key_id = Some(key.to_string());
        true
    }
}

/// The drives file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrivesFile {
    pub format: String,
    pub version: u32,
    pub drives: Vec<DriveEntry>,
}

impl Default for DrivesFile {
    fn default() -> Self {
        DrivesFile::empty()
    }
}

impl Document for DrivesFile {
    fn check(&self) -> Result<(), String> {
        if self.format != DRIVES_FORMAT {
            return Err(format!(
                "the drives file is \"{}\", not \"{DRIVES_FORMAT}\"",
                self.format
            ));
        }
        if self.version == 0 || self.version > DRIVES_VERSION {
            return Err(format!(
                "the drives file is version {}; this app reads up to version {DRIVES_VERSION}",
                self.version
            ));
        }
        Ok(())
    }
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
        file.check().map_err(DriveError::InvalidConfig)?;
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

    /// Writes the file as this process has it (its folder is created), through a temporary file.
    /// A test's way to put a drives file down: a writer that reads the file, changes it and writes
    /// it back loses what another process changed in between - changes go through
    /// [`DrivesFile::update`], the only writer of shipped code.
    #[cfg(test)]
    pub(crate) fn save(&self, path: &Path) -> Result<(), DriveError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        crate::local::write_atomically(path, self.to_json().as_bytes())
            .map_err(|e| DriveError::Io(format!("{}: {e}", path.display())))
    }

    /// Adds `entry`, or replaces the drive with its id. What this device wrote down of the drive's
    /// encryption survives the replacement ([`DriveEntry::keep_encryption_record`]): a drive that
    /// is added again (a purchase picked up once more, a join, a hand-over) arrives as the server's
    /// bundle describes it, which knows nothing of what this device found, and the record is
    /// monotone - a device that found a drive encrypted never takes it for a plain one again
    /// because its entry was written anew (X-DELTA-AZDRIVE-KEYS-21).
    pub fn add(&mut self, entry: DriveEntry) {
        match self.drives.iter_mut().find(|d| d.id == entry.id) {
            Some(existing) => {
                let kept = existing.encryption_record();
                *existing = entry;
                if let Some(kept) = kept {
                    existing.keep_encryption_record(kept);
                }
            }
            None => self.drives.push(entry),
        }
    }

    /// Forgets the drive with this id.
    pub fn remove(&mut self, id: &str) -> Option<DriveEntry> {
        let index = self.drives.iter().position(|d| d.id == id)?;
        Some(self.drives.remove(index))
    }

    /// Changes the drives file at `path`: under the file's lock, on what is in the file NOW (the
    /// list another process changed since this one read it included), the rest of it - fields this
    /// build does not know among them - as it was; a change that leaves the list as it was writes
    /// nothing. The only way a drive is added, forgotten or changed: five writers loaded the file,
    /// changed it and saved it back, and two of them at a time dropped each other's change
    /// (AZ-U21, X-AR5a-18).
    ///
    /// # Errors
    ///
    /// [`DriveError::InvalidConfig`] for a file of another format or a newer version (never
    /// written over), [`DriveError::Io`] when the lock is held for long or the file cannot be
    /// read or written.
    pub fn update<R>(
        path: &Path,
        change: impl FnOnce(&mut DrivesFile) -> R,
    ) -> Result<R, DriveError> {
        Doc::<DrivesFile>::new(path)
            .update(change)
            .map(|(_, answer)| answer)
            .map_err(|e| match e {
                DocError::Foreign(why) => DriveError::InvalidConfig(why),
                other => DriveError::Io(format!("{}: {other}", path.display())),
            })
    }

    /// Writes down in the file at `path` that this device found the Azlin drive `id` encrypted,
    /// with the id of its drive key ([`DriveEntry::record_encryption`]). Whether the file
    /// changed: not when the drive is not in it, is no Azlin drive, or says so already.
    pub fn record_encryption(path: &Path, id: &str, key: &str) -> Result<bool, DriveError> {
        Self::update(path, |file| {
            file.drives
                .iter_mut()
                .find(|d| d.id == id)
                .is_some_and(|entry| entry.record_encryption(key))
        })
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&DriveEntry> {
        self.drives.iter().find(|d| d.id == id)
    }
}

/// `url` as `scheme://host[:port]`, lower case, without a login, a path, a query: where a request
/// to it goes. A text that is no address is itself, trimmed and lower case.
fn origin_of(url: &str) -> String {
    let url = url.trim().to_ascii_lowercase();
    let Some((scheme, rest)) = url.split_once("://") else {
        return url;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}")
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
    crate::footprint::KeyEntry::Session.name(drive_id)
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
