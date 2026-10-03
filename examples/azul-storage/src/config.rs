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

use std::{
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
        match &self.location {
            // A folder the user added is not the data tree: no `.azlin/` there.
            DriveLocation::Local { root } => Ok(Box::new(LocalDrive::without_manifest(
                PathBuf::from(root),
            ))),
            DriveLocation::S3 { auth, .. } => match auth {
                DriveAuth::Keyring => {
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
            DriveLocation::Local { .. } => None,
        }
    }

    /// Whether opening it needs credentials from the keyring.
    #[must_use]
    pub fn needs_keyring(&self) -> bool {
        matches!(
            self.location,
            DriveLocation::S3 {
                auth: DriveAuth::Keyring,
                ..
            }
        )
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
