//! azul-storage: the storage layer the azul apps share.
//!
//! AzDrive browses with it and AzMail exports with it. Durable data is FILES,
//! one bucket (or folder) per user: mail under `mail/`, later `docs/`,
//! `events/`, one folder per meeting UUID (see the azlin cloud storage split).
//! The database only mints ids, holds invites, transient state and the access
//! links that say who may read or write which path; it never holds content.
//!
//! One trait, [`Drive`], with blocking calls: `list` (paged, S3 semantics:
//! "folders" are common prefixes), `get`, `get_range`, `put`, `delete`,
//! `head`. The apps call it from an azul `Thread`, never from a callback.
//!
//! Backends:
//! - [`LocalDrive`]: a folder on disk. Keys are `/`-separated paths under its root; `..`, `.`,
//!   absolute and backslash keys are refused ([`key::check_path_key`]).
//! - [`S3Drive`]: an S3-compatible bucket (AWS S3, Cloudflare R2, MinIO), path-style or
//!   virtual-host style, every request signed with AWS SigV4 ([`sigv4`]). It builds its requests
//!   itself and sends them through a [`Transport`]; the apps use `AzulTransport` (feature `azul`),
//!   which goes through azul's `HttpRequestConfig`, the tests a fake.
//! - [`ScopedDrive`]: any drive seen through a grant (a key prefix, read-only or writable). This is
//!   the seam for RBAC and the access links the database hands out.
//!
//! Configuration ([`config`]): the list of drives the user added lives in
//! `<config dir>/azul-storage/drives.json` WITHOUT secrets; the credentials
//! live in the OS keyring under [`config::keyring_key`], as
//! [`Credentials::to_keyring_secret`]. Nothing here logs a secret or puts one
//! in `Debug` output.

pub mod config;
pub mod key;
pub mod local;
pub mod ops;
pub mod s3;
pub mod scoped;
pub mod sigv4;
pub mod time;
pub mod transfer;
pub mod transport;
pub(crate) mod xml;

#[cfg(feature = "azul")]
pub mod azul_transport;

#[cfg(test)]
mod tests;

use std::{fmt, path::PathBuf};

pub use local::LocalDrive;
pub use s3::{Credentials, S3Config, S3Drive};
pub use scoped::ScopedDrive;
pub use transport::{HttpCall, HttpReply, Method, Transport};

/// Objects per page when a listing does not say (S3's own default and maximum).
pub const DEFAULT_PAGE_SIZE: u32 = 1000;

/// The folder separator of every key.
pub const DELIMITER: &str = "/";

/// One object of a drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    /// The full key, `mail/inbox/0001.eml`.
    pub key: String,
    /// Bytes.
    pub size: u64,
    /// Last modified, in seconds since 1970-01-01 UTC, if the backend knows.
    pub modified: Option<u64>,
    /// The entity tag without its quotes (S3: the MD5 of a single-part upload).
    pub etag: Option<String>,
}

impl ObjectInfo {
    /// The last segment of the key: the file name.
    #[must_use]
    pub fn name(&self) -> &str {
        key::last_segment(&self.key)
    }
}

/// What to list: S3 ListObjectsV2 semantics for every backend.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListRequest {
    /// Only keys that start with this. A folder is `mail/inbox/`, the root is empty.
    pub prefix: String,
    /// With `Some("/")`, keys below the next `/` after the prefix collapse into one "folder"
    /// (a common prefix); with `None` every key under the prefix is listed.
    pub delimiter: Option<String>,
    /// The `next` token of the previous page, to continue after it.
    pub continuation: Option<String>,
    /// Entries (objects and folders) per page; 0 means [`DEFAULT_PAGE_SIZE`].
    pub max_keys: u32,
}

impl ListRequest {
    /// One folder level: the folders and objects directly in `prefix`.
    #[must_use]
    pub fn folder(prefix: &str) -> Self {
        ListRequest {
            prefix: prefix.to_string(),
            delimiter: Some(DELIMITER.to_string()),
            continuation: None,
            max_keys: DEFAULT_PAGE_SIZE,
        }
    }

    /// Every object under `prefix`, at any depth.
    #[must_use]
    pub fn recursive(prefix: &str) -> Self {
        ListRequest {
            prefix: prefix.to_string(),
            delimiter: None,
            continuation: None,
            max_keys: DEFAULT_PAGE_SIZE,
        }
    }

    #[must_use]
    pub fn with_max_keys(mut self, max_keys: u32) -> Self {
        self.max_keys = max_keys;
        self
    }

    #[must_use]
    pub fn with_continuation(mut self, token: impl Into<String>) -> Self {
        self.continuation = Some(token.into());
        self
    }

    /// The page size this request asks for.
    #[must_use]
    pub fn page_size(&self) -> u32 {
        if self.max_keys == 0 {
            DEFAULT_PAGE_SIZE
        } else {
            self.max_keys.min(DEFAULT_PAGE_SIZE)
        }
    }
}

/// One page of a listing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListPage {
    /// Common prefixes ("folders"), each ending in the delimiter: `mail/inbox/`.
    pub folders: Vec<String>,
    /// The objects of this page.
    pub objects: Vec<ObjectInfo>,
    /// Where the next page starts; `None` on the last page.
    pub next: Option<String>,
}

/// An inclusive byte range, as in HTTP `Range: bytes=start-end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    /// The last byte, inclusive; `None` = to the end.
    pub end: Option<u64>,
}

impl ByteRange {
    #[must_use]
    pub const fn new(start: u64, end: Option<u64>) -> Self {
        ByteRange { start, end }
    }

    /// `bytes=0-9`, `bytes=10-`.
    #[must_use]
    pub fn header_value(&self) -> String {
        match self.end {
            Some(end) => format!("bytes={}-{end}", self.start),
            None => format!("bytes={}-", self.start),
        }
    }
}

/// An error answer of an S3-compatible service, from its XML error body.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServiceError {
    /// The HTTP status.
    pub status: u16,
    /// `SignatureDoesNotMatch`, `NoSuchBucket`, ... (or a name for the status when there was no
    /// body, as for HEAD).
    pub code: String,
    /// The service's own sentence.
    pub message: String,
    pub resource: Option<String>,
    pub request_id: Option<String>,
    /// `AuthorizationHeaderMalformed`: the region the bucket is in.
    pub region: Option<String>,
    /// `PermanentRedirect`: the endpoint to use instead.
    pub endpoint: Option<String>,
}

impl ServiceError {
    /// What to check, for the errors a user can fix in the drive's settings.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        let hint = match self.code.as_str() {
            "SignatureDoesNotMatch" => "check the secret key".to_string(),
            "InvalidAccessKeyId" => "check the access key".to_string(),
            "NoSuchBucket" => "check the bucket name".to_string(),
            "AccessDenied" | "Forbidden" => {
                "these keys may not use this bucket or path".to_string()
            }
            "RequestTimeTooSkewed" => "this computer's clock is off".to_string(),
            "AuthorizationHeaderMalformed" | "IllegalLocationConstraintException" => {
                match &self.region {
                    Some(region) => format!("the bucket is in region {region}"),
                    None => "check the region".to_string(),
                }
            }
            "PermanentRedirect" | "TemporaryRedirect" => match &self.endpoint {
                Some(endpoint) => format!("use the endpoint {endpoint}"),
                None => "check the endpoint and region".to_string(),
            },
            _ => return None,
        };
        Some(hint)
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = if self.code.is_empty() {
            "Error"
        } else {
            self.code.as_str()
        };
        write!(f, "{code} (HTTP {})", self.status)?;
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        if let Some(hint) = self.hint() {
            write!(f, " ({hint})")?;
        }
        Ok(())
    }
}

/// Why a drive call failed. Every variant reads as a sentence for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveError {
    /// The key cannot name an object of this drive (`..`, absolute, empty, ...).
    InvalidKey { key: String, reason: &'static str },
    /// No object has this key.
    NotFound { key: String },
    /// The range starts past the end of the object.
    InvalidRange { key: String },
    /// Not allowed: a read-only grant, missing credentials.
    Denied { message: String },
    /// The service answered with an error.
    Service(ServiceError),
    /// The request did not get an answer (DNS, connection, TLS, timeout).
    Transport(String),
    /// A local file operation failed.
    Io(String),
    /// The answer made no sense (not a listing, a truncated listing without a token, ...).
    Protocol(String),
    /// Not built yet (access links).
    Unsupported(String),
    /// The drive's settings cannot work (an endpoint that is not a URL, a bucket name
    /// that cannot be in one, a keyring entry that is not credentials).
    InvalidConfig(String),
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveError::InvalidKey { key, reason } => {
                write!(f, "\"{key}\" is not a valid name: {reason}")
            }
            DriveError::NotFound { key } => write!(f, "\"{key}\" does not exist"),
            DriveError::InvalidRange { key } => {
                write!(f, "the requested range is outside \"{key}\"")
            }
            DriveError::Denied { message } => write!(f, "not allowed: {message}"),
            DriveError::Service(e) => write!(f, "the storage service answered {e}"),
            DriveError::Transport(message) => {
                write!(f, "no answer from the storage service: {message}")
            }
            DriveError::Io(message) => write!(f, "file error: {message}"),
            DriveError::Protocol(message) => write!(f, "unexpected answer: {message}"),
            DriveError::Unsupported(message) => write!(f, "not supported yet: {message}"),
            DriveError::InvalidConfig(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for DriveError {}

impl From<std::io::Error> for DriveError {
    fn from(e: std::io::Error) -> Self {
        DriveError::Io(e.to_string())
    }
}

/// A place that holds files: a folder, a bucket. Every call blocks; call it from a
/// worker thread (an azul `Thread`), never from a UI callback.
pub trait Drive: Send + Sync {
    /// One page of keys under `request.prefix`.
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError>;
    /// The whole object.
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError>;
    /// Part of the object.
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError>;
    /// Creates or replaces the object.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError>;
    /// Removes the object; removing a missing object is not an error (S3 semantics).
    fn delete(&self, key: &str) -> Result<(), DriveError>;
    /// The object's size, date and tag, without its bytes.
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError>;

    /// Copies the object `from` to `to` within this drive (replacing `to`
    /// when it is there; conflicts are the caller's). A folder is not one
    /// object: refused. By default one get and one put; a folder on disk
    /// copies the file, a bucket asks the service (CopyObject), so nothing
    /// passes through this process.
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        if from.ends_with('/') || to.ends_with('/') {
            return Err(DriveError::InvalidKey {
                key: from.to_string(),
                reason: "a folder is copied object by object",
            });
        }
        let bytes = self.get(from)?;
        self.put(to, &bytes)
    }

    /// Creates the folder `prefix` (not empty, ending in `/`); an existing one is
    /// fine. A bucket has no folders, so by default this puts an empty "folder
    /// marker" object named `prefix`, as the S3 consoles do.
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        ops::check_folder(prefix)?;
        self.put(prefix, &[])
    }

    /// Moves the object `from` to `to`, or (both ending in `/`) the folder `from`
    /// with everything under it. Never over something that is there, never a
    /// folder into itself. By default every object is copied, then deleted.
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        ops::rename_by_copy(self, from, to)
    }

    /// Removes the folder `prefix` (not the root) with everything under it.
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        ops::delete_by_listing(self, prefix)
    }

    /// The file (or, for a folder prefix, the directory) on this computer that
    /// holds `key`, when the drive is a folder on disk; `None` otherwise.
    fn local_path(&self, key: &str) -> Option<PathBuf> {
        let _ = key;
        None
    }

    /// What the backend knows about the object beyond [`Drive::head`], as
    /// `(name, value)` pairs to show: an S3 object's Content-Type, storage class,
    /// encryption and user metadata; a local file's location. Empty by default.
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.head(key).map(|_| Vec::new())
    }
}

impl<D: Drive + ?Sized> Drive for Box<D> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        (**self).list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        (**self).get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        (**self).get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        (**self).put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        (**self).delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        (**self).head(key)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        (**self).copy(from, to)
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        (**self).create_folder(prefix)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        (**self).rename(from, to)
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        (**self).delete_folder(prefix)
    }
    fn local_path(&self, key: &str) -> Option<PathBuf> {
        (**self).local_path(key)
    }
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        (**self).metadata(key)
    }
}

impl<D: Drive + ?Sized> Drive for std::sync::Arc<D> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        (**self).list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        (**self).get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        (**self).get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        (**self).put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        (**self).delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        (**self).head(key)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        (**self).copy(from, to)
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        (**self).create_folder(prefix)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        (**self).rename(from, to)
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        (**self).delete_folder(prefix)
    }
    fn local_path(&self, key: &str) -> Option<PathBuf> {
        (**self).local_path(key)
    }
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        (**self).metadata(key)
    }
}
