//! The drives a user added, shared by the apps (AzDrive shows them, AzMail
//! exports to one): `<config dir>/azul-storage/drives.json`, WITHOUT secrets.
//! Each drive's credentials are one OS keyring entry, [`keyring_key`], holding
//! [`Credentials::to_keyring_secret`].
//!
//! ```json
//! { "format": "azul-storage.drives", "version": 1, "drives": [
//!   { "id": "k3f9...", "name": "S3 Drive",
//!     "location": { "kind": "s3", "endpoint": "https://s3.eu-central-1.amazonaws.com",
//!                   "region": "eu-central-1", "bucket": "felix-azlin", "path_style": false,
//!                   "auth": { "type": "keyring" } } } ] }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Credentials, Drive, DriveError, Transport};

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
    /// keyring); a local one ignores them and the transport.
    pub fn open(
        &self,
        credentials: Option<Credentials>,
        transport: Box<dyn Transport>,
    ) -> Result<Box<dyn Drive>, DriveError> {
        let _ = (credentials, transport);
        todo!("RED")
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
        let _ = json;
        todo!("RED")
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        todo!("RED")
    }

    /// Reads the file at `path`; a missing file is an empty list.
    pub fn load(path: &Path) -> Result<Self, DriveError> {
        let _ = path;
        todo!("RED")
    }

    /// Writes the file (its folder is created), through a temporary file.
    pub fn save(&self, path: &Path) -> Result<(), DriveError> {
        let _ = path;
        todo!("RED")
    }

    /// Adds `entry`, or replaces the drive with its id.
    pub fn add(&mut self, entry: DriveEntry) {
        let _ = entry;
        todo!("RED")
    }

    /// Forgets the drive with this id.
    pub fn remove(&mut self, id: &str) -> Option<DriveEntry> {
        let _ = id;
        todo!("RED")
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
    let _ = (var, config_dir);
    todo!("RED")
}

/// The OS keyring entry holding a drive's credentials.
#[must_use]
pub fn keyring_key(drive_id: &str) -> String {
    let _ = drive_id;
    todo!("RED")
}

/// A new drive id: unique, `[0-9a-z-]`, starting with a readable slug of `name`.
#[must_use]
pub fn new_drive_id(name: &str) -> String {
    let _ = name;
    todo!("RED")
}
