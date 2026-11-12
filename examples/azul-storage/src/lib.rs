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
//! `head`; `put_from` streams a reader in, `put_if` writes only when the
//! object is absent or unchanged (a bucket's conditional PUT), `put_from_if` both;
//! `put_file` sends a local file (a bucket: in parts several at once, resumable after
//! the app was killed, see [`multipart`]). The apps call it from an azul `Thread`,
//! never from a callback.
//!
//! Backends:
//! - [`LocalDrive`]: a folder on disk. Keys are `/`-separated paths under its root; `..`, `.`,
//!   absolute and backslash keys are refused ([`key::check_path_key`]). The user's data tree
//!   (`LocalDrive::new`) keeps `<root>/.azlin/cache`, the [`manifest`] of what is there, on
//!   every write, so the later S3 / database sync only diffs ([`manifest::diff`]).
//! - [`S3Drive`]: an S3-compatible bucket (AWS S3, Cloudflare R2, MinIO), path-style or
//!   virtual-host style, every request signed with AWS SigV4 ([`sigv4`]). It builds its requests
//!   itself and sends them through a [`Transport`]; the apps use `AzulTransport` (feature `azul`),
//!   which goes through azul's `HttpRequestConfig`, the tests a fake.
//! - [`ScopedDrive`]: any drive seen through a grant (a key prefix, read-only or writable). This is
//!   the seam for RBAC and the access links the database hands out.
//! - `OpendalDrive` (feature `opendal`): a data source Apache OpenDAL reaches - WebDAV, FTP,
//!   Google Drive, Dropbox, OneDrive, GitHub, GCS, Azure, ... ([`catalog`] lists them). OpenDAL's
//!   HTTP goes through the same [`Transport`] as the S3 client's.
//! - `DatabaseDrive` (feature `sql`): a PostgreSQL, MySQL or SQLite database browsed as files:
//!   its tables are folders, every row a JSON file, a table's rows a CSV file.
//! - `EncryptedDrive` (feature `encryption`): any drive (a bucket) that holds only ciphertext
//!   under random names - every file version an AZL1 object (`crypto::azl1`: 1 MiB segments,
//!   zstd where it pays, XChaCha20-Poly1305 in the STREAM construction, a key commitment), the
//!   names in a `NameIndex`, the drive key sealed to the members and the recovery code
//!   (`crypto::keys`). Encryption and compression happen on this device only. Small files
//!   can share AZL1 objects (`packing`), each read back with one ranged GET.
//!
//! Configuration ([`config`]): the list of drives the user added lives in
//! `<config dir>/azul-storage/drives.json` WITHOUT secrets; the secrets (an S3
//! drive's credentials, an Azlin drive's session, a data source's passwords and
//! tokens) live in the OS keyring under [`config::keyring_key`]; [`keyring`] is
//! the seam a worker thread reads and writes them through (azul's keyring in the
//! apps, feature `azul`). The Add drive dialog's sources and their forms are
//! [`catalog`]. Nothing here logs a secret or puts one in `Debug` output.

pub mod base32;
/// Files under a root reached without a link: descriptor-relative opens, renames and deletes.
pub mod beneath;
/// What a probe of this computer could not answer, and the posture of the check that asked.
pub mod cap;
pub mod catalog;
pub mod checked;
pub mod config;
/// What a running transfer is told (the bytes done) and asks (whether to stop).
pub mod ctl;
/// One folder on disk read in batches: names, kinds and (where the platform has them with the
/// names) sizes, dates and inodes, without a stat per entry.
pub mod dir_read;
/// Documents with an owner: a JSON file several writers change, each applying its change to what
/// is there under a lock.
pub mod docs;
/// The one way a file is put in place: staged beside the name, synced, checked, renamed.
pub mod durable;
/// What one drive leaves on a device: the keyring entries' names, the evidence a secret needs
/// before it is erased, and the removal that takes it all off.
pub mod footprint;
pub mod ids;
pub mod key;
pub mod keyring;
/// Named maximums for numbers somebody else wrote: what an object may weigh before it is read.
pub mod limits;
pub mod local;
/// Locks every process of this user shares: the OS's own file lock, dropped with its process.
pub mod lock;
pub mod manifest;
pub mod meta;
/// Big uploads to a bucket: parts of 16 MiB four at once, resumable after the app was killed.
pub mod multipart;
/// What a name from a drive, a device or a user may be before it becomes a path: the policy, the
/// protected names, the key that passed.
pub mod names;
/// OAuth 2.0 for the consumer clouds (Google Drive, Dropbox, OneDrive): the token endpoint and
/// the transport that keeps a signed-in drive's access token fresh.
pub mod oauth;
pub mod ops;
/// The system's "downloaded from elsewhere" mark on a file (Gatekeeper's attribute, Windows'
/// Zone.Identifier stream): the sync marks other people's files, AzDrive's Open and Download the
/// files they bring from a drive.
pub mod quarantine;
/// The restore protocol: is what a drive serves a restore, a rollback or the ordinary course, and
/// what a device does with its memory of the drive after a restore. Pure.
pub mod restore;
pub mod s3;
pub mod scoped;
/// The one place for what a program keeps for a while and nobody asked for: private, named by
/// its owner process, swept.
pub mod scratch;
/// A text that names a URL without the secret in it (the user info, the query, the fragment).
pub mod scrub;
/// Where an object's bytes lie in an Azlin server's 4 + 2 code: chunks and data shards.
pub mod shards;
pub mod sigv4;
pub mod tables;
pub mod time;
pub mod transfer;
pub mod transport;
/// The certificate authorities the Azlin apps pin (the iroh relays').
pub mod trust;
/// The app's UI thread never waits for a network answer (debug builds assert it).
pub mod ui_thread;
/// What std does not tell on Windows - a file's identity and change time, the free space and the file
/// system of a disk, a rename written through -; its model is built and tested on every host.
pub(crate) mod win;
/// Extended attributes carried with a copy on this computer (Finder tags and labels, the resource
/// fork, `user.*` attributes): a Move whose copy would lose them keeps its source.
pub mod xattrs;
/// The transfer engine: where drives are and whether two places overlap, the plan of a copy or
/// a move, the run that lands every file before it retires a source.
pub mod xfer;
/// A request and its answer as HTTP/1.1 bytes: S3 over iroh.
pub mod http1;
pub(crate) mod xml;

#[cfg(feature = "azul")]
pub mod azul_keyring;
#[cfg(feature = "azul")]
pub mod azul_transport;
/// S3 over iroh through azul's iroh endpoint.
#[cfg(feature = "azul")]
pub mod azul_iroh;

/// The one tokio runtime the async back-ends (OpenDAL, the database drivers) run on.
#[cfg(any(feature = "opendal", feature = "sql"))]
pub mod runtime;

/// Data sources through Apache OpenDAL (the module is not called `opendal`: that is the crate's
/// name).
#[cfg(feature = "opendal")]
pub mod opendal_drive;

/// Databases browsed as files.
#[cfg(feature = "sql")]
pub mod database;

/// A temporary folder for tests: this crate's, and the apps' through the `testing` feature.
#[cfg(any(test, feature = "testing"))]
pub mod testing;

#[cfg(test)]
mod tests;

/// Client-side encryption: the keys of an encrypted drive and its AZL1 objects.
#[cfg(feature = "encryption")]
pub mod crypto;
/// A drive whose bucket holds only ciphertext under random names.
#[cfg(feature = "encryption")]
pub mod encrypted;
#[cfg(feature = "encryption")]
pub use encrypted::{
    open_encrypted, AutoEncrypted, EncryptedDrive, IndexProvider, MemoryIndex, NameIndex,
};
/// An encrypted file's upload that a cut resumes at its next part.
#[cfg(feature = "encryption")]
pub(crate) mod encrypted_upload;
/// A plaintext drive's files moved into its encrypted namespace, resumably.
#[cfg(feature = "encryption")]
pub mod migrate;
/// An encrypted drive's files written again, smaller, while the computer is idle.
#[cfg(feature = "encryption")]
pub mod recompress;
/// Links to an encrypted drive's files: share manifests, presigned links.
#[cfg(feature = "encryption")]
pub mod sharing;
/// A new drive key after a compromise ("I was hacked"), and re-encrypting everything.
#[cfg(feature = "encryption")]
pub mod rotation;
/// Small files of an encrypted drive packed together into shared AZL1 objects.
#[cfg(feature = "encryption")]
pub mod packing;

use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
};

pub use azlin_contract::AzlinError;
pub use config::SecretOptions;
pub use ctl::Ctl;
#[cfg(feature = "sql")]
pub use database::DatabaseDrive;
pub use limits::Limit;
pub use local::LocalDrive;
#[cfg(feature = "opendal")]
pub use opendal_drive::OpendalDrive;
pub use s3::{BucketSpace, Credentials, S3Config, S3Drive};
pub use scoped::ScopedDrive;
pub use transport::{
    BytesPerSec, CallLimits, ExpectedBody, HttpCall, HttpReply, Method, RequestTimeout,
    TransferBudget, Transport,
};

/// Objects per page when a listing does not say (S3's own default and maximum).
pub const DEFAULT_PAGE_SIZE: u32 = 1000;

/// The folder separator of every key.
pub const DELIMITER: &str = "/";

/// The name of a [`Drive::metadata`] row that holds the BLAKE3 of the file's content (64
/// lowercase hex digits), where the drive knows it without reading the file: an encrypted
/// drive's index keeps it. A sync compares it with a file on this device without a download.
pub const CONTENT_HASH_METADATA: &str = "BLAKE3";

/// Files below this many bytes are small: an encrypted drive packs them together
/// (`packing`), and a sync sends them in batches ([`Drive::put_many`]). From here on a file's
/// own object costs under 1 % in header and trailer, and its reads span shards of its own
/// anyway; below it the cost per object wins: six shards, four seeks and a 4 KiB header for
/// every file.
pub const SMALL_FILE_BYTES: u64 = 512 * 1024;

/// The small files' bytes one [`Drive::put_many`] batch holds before it is sent: a few packs'
/// worth on an encrypted drive, few enough to keep in memory.
pub const SMALL_BATCH_BYTES: u64 = 16 << 20;

/// The keys one batch delete request takes at most: S3's DeleteObjects limit.
pub const DELETE_BATCH: usize = 1000;

/// Files a batch moves at once: large uploads ([`Drive::put_from_many`]) and copies on the
/// service ([`Drive::move_many`]). Enough to fill a link, few enough that a weak computer and
/// the service's per-client limits keep up (a large upload has its own parts in flight too).
pub const PARALLEL_FILES: usize = 4;

/// The name of a [`Drive::metadata`] row that holds the key, in the bucket below the drive, of
/// the object the file's bytes are in: an encrypted drive's AZL1 object, or the pack a small
/// file shares with others. A device that keeps a file's ciphertext keeps that object.
pub const OBJECT_KEY_METADATA: &str = "Object key";

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
    /// Ask the bucket again (a user's refresh, F5): a drive that answers listings from a copy
    /// - an encrypted drive's index - reads the bucket first, its poll time aside. A plain
    /// bucket lists from the bucket anyway.
    pub refresh: bool,
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
            refresh: false,
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
            refresh: false,
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

    /// The listing asks the bucket again ([`ListRequest::refresh`]).
    #[must_use]
    pub fn refreshed(mut self) -> Self {
        self.refresh = true;
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

/// One file of a batch write ([`Drive::put_many`]).
#[derive(Debug, Clone, Copy)]
pub struct BatchPut<'a> {
    /// The file's key.
    pub key: &'a str,
    /// Its bytes (a batch is for small files: they are in memory anyway).
    pub bytes: &'a [u8],
    /// Written only when this holds ([`Drive::put_if`]); `None`: whatever is there.
    pub condition: Option<&'a Precondition>,
}

impl<'a> BatchPut<'a> {
    /// `bytes` for `key`, whatever is there.
    #[must_use]
    pub fn new(key: &'a str, bytes: &'a [u8]) -> BatchPut<'a> {
        BatchPut {
            key,
            bytes,
            condition: None,
        }
    }

    /// Only when `condition` holds.
    #[must_use]
    pub fn when(mut self, condition: &'a Precondition) -> BatchPut<'a> {
        self.condition = Some(condition);
        self
    }
}

/// One delete of a batch ([`Drive::delete_many`]).
#[derive(Debug, Clone, Copy)]
pub struct BatchDelete<'a> {
    /// The file's key.
    pub key: &'a str,
    /// Deleted only while the file is still the version with this entity tag (an edit made
    /// elsewhere beats a delete); `None`: whatever is there.
    pub version: Option<&'a str>,
}

impl<'a> BatchDelete<'a> {
    /// `key`, whatever version is there.
    #[must_use]
    pub fn new(key: &'a str) -> BatchDelete<'a> {
        BatchDelete { key, version: None }
    }

    /// Only while the file is the version `etag` (as [`ObjectInfo::etag`] shows it).
    #[must_use]
    pub fn of_version(mut self, etag: &'a str) -> BatchDelete<'a> {
        self.version = Some(etag);
        self
    }
}

/// One move of a batch ([`Drive::move_many`]).
#[derive(Debug, Clone, Copy)]
pub struct BatchMove<'a> {
    /// The file's key now.
    pub from: &'a str,
    /// Its new key: nothing may be there.
    pub to: &'a str,
    /// Moved only while the file is still the version with this entity tag; `None`: whatever is
    /// there.
    pub version: Option<&'a str>,
}

impl<'a> BatchMove<'a> {
    /// `from` to `to`, whatever version is at `from`.
    #[must_use]
    pub fn new(from: &'a str, to: &'a str) -> BatchMove<'a> {
        BatchMove {
            from,
            to,
            version: None,
        }
    }

    /// Only while `from` is the version `etag`.
    #[must_use]
    pub fn of_version(mut self, etag: &'a str) -> BatchMove<'a> {
        self.version = Some(etag);
        self
    }
}

/// One streamed write of a batch ([`Drive::put_from_many`]).
pub struct BatchStream<'a> {
    /// The file's key.
    pub key: &'a str,
    /// The file's bytes, read once to their end (a file on disk, hashed as it is read).
    pub body: &'a mut (dyn Read + Send),
    /// Written only when this holds ([`Drive::put_if`]); `None`: whatever is there.
    pub condition: Option<&'a Precondition>,
}

impl<'a> BatchStream<'a> {
    /// `body` for `key`, whatever is there.
    #[must_use]
    pub fn new(key: &'a str, body: &'a mut (dyn Read + Send)) -> BatchStream<'a> {
        BatchStream {
            key,
            body,
            condition: None,
        }
    }

    /// Only when `condition` holds.
    #[must_use]
    pub fn when(mut self, condition: &'a Precondition) -> BatchStream<'a> {
        self.condition = Some(condition);
        self
    }
}

/// Whether the entity tag a drive shows (`have`) is `want` (either may carry its quotes).
pub(crate) fn same_etag(have: Option<&str>, want: &str) -> bool {
    have.is_some_and(|have| have.trim().trim_matches('"') == want.trim().trim_matches('"'))
}

/// What must hold for a conditional write ([`Drive::put_if`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// No object has the key yet (S3 `If-None-Match: *`).
    Absent,
    /// The object is still the version with this entity tag, as [`ObjectInfo::etag`] shows it
    /// (S3 `If-Match`).
    Matches(String),
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
    /// An Azlin node's own code (`x-azlin-error`: `unavailable`, `quota_exceeded`, ...;
    /// [`AzlinError::Unknown`] for one this build does not know), which azcloud-kit's
    /// `user_errors` turns into what the user sees.
    pub azlin_error: Option<AzlinError>,
    /// `Retry-After` in seconds, when the service sent a number.
    pub retry_after: Option<u64>,
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
        match &self.azlin_error {
            Some(azlin) => write!(f, "{code} (HTTP {}, x-azlin-error {azlin})", self.status)?,
            None => write!(f, "{code} (HTTP {})", self.status)?,
        }
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
    /// An encrypted drive this device holds no key for (yet): it is unlocked with the drive's
    /// recovery code, or joined with a code from a device that has the key.
    Locked { drive: String },
    /// A drive key this device does not take: the bucket handed it another key than the one
    /// this device holds for the drive, or than its join or claim code names (anyone in
    /// control of the storage can seal a key of their own to a member). Nothing was read or
    /// written with it; the recovery code, or a join code from a device that has the drive,
    /// gives the drive's key back.
    KeyRefused { drive: String, reason: String },
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
    /// A conditional write lost ([`Drive::put_if`]): the object was there already, or it
    /// changed since it was read (S3: 412 Precondition Failed). Nothing was written.
    Conflict { key: String },
    /// The object is damaged, or was changed by someone without its key: an encrypted
    /// drive's object or key file that does not authenticate.
    Corrupt { key: String, reason: String },
    /// The caller stopped the call ([`Ctl::cancelled`]). Nothing was made visible: a bucket
    /// upload was aborted and its state forgotten, a file copy removed its temporary file. Not
    /// a failure of the drive: it is never retried and never counted against the drive.
    Cancelled,
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
            DriveError::Locked { drive } => write!(
                f,
                "this device has no key for \"{drive}\" yet: join it with a code from a device \
                 that has one, or unlock it with the recovery code"
            ),
            DriveError::KeyRefused { drive, reason } => write!(
                f,
                "this device does not take the drive key it was given for \"{drive}\" \
                 ({reason}): unlock it with the recovery code, or join it with a code from a \
                 device that has it"
            ),
            DriveError::Service(e) => write!(f, "the storage service answered {e}"),
            DriveError::Transport(message) => {
                write!(f, "no answer from the storage service: {message}")
            }
            DriveError::Io(message) => write!(f, "file error: {message}"),
            DriveError::Protocol(message) => write!(f, "unexpected answer: {message}"),
            DriveError::Unsupported(message) => write!(f, "not supported yet: {message}"),
            DriveError::InvalidConfig(message) => write!(f, "{message}"),
            DriveError::Conflict { key } => {
                write!(f, "\"{key}\" was written by someone else in the meantime")
            }
            DriveError::Corrupt { key, reason } => {
                write!(f, "\"{key}\" is damaged or was changed ({reason})")
            }
            DriveError::Cancelled => write!(f, "stopped before it was done; nothing was changed"),
        }
    }
}

impl std::error::Error for DriveError {}

impl From<std::io::Error> for DriveError {
    fn from(e: std::io::Error) -> Self {
        if ctl::Stopped::is(&e) {
            DriveError::Cancelled
        } else {
            DriveError::Io(e.to_string())
        }
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

    /// [`Drive::get_range`] of the object only while it is still the version `condition` names
    /// ([`Precondition::Matches`], the entity tag as [`ObjectInfo::etag`] shows it: S3 `If-Match` on
    /// the ranged GET, the object an encrypted drive's index names): [`DriveError::Conflict`], no
    /// byte read, when the object is another version now. What a copy that reads one object in many
    /// requests asks of every one of them, so that the bytes it puts together are of ONE version -
    /// not a version that was replaced and put back between two of them (X-DELTA-SYNC2-3).
    /// [`DriveError::Unsupported`] from a drive that cannot ask (the default); the caller then reads
    /// with [`Drive::get_range`] and looks at the object when it is done.
    fn get_range_if(
        &self,
        key: &str,
        range: ByteRange,
        condition: &Precondition,
    ) -> Result<Vec<u8>, DriveError> {
        let _ = (range, condition);
        Err(DriveError::Unsupported(format!(
            "reading a range of \"{key}\" only if it is unchanged"
        )))
    }

    /// The whole object when it is at most `limit.max` bytes; a bigger one is refused with an
    /// error that names the limit, and a drive that can says so before it reads it: a bucket
    /// gives the limit to the transport, which refuses an answer past it; a folder on disk looks
    /// at the size first. By default [`Drive::get`] and a check of the length after it (the
    /// memory is spent by then): a drive that wraps another forwards this call so the inner
    /// drive's guard stands. Use this, not [`Drive::get`], for an object whose size somebody
    /// else decides (a key file, an index, a manifest); [`Drive::get`] stays for the callers
    /// that bound the object some other way.
    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        let bytes = self.get(key)?;
        limit.check_object(key, bytes.len() as u64)?;
        Ok(bytes)
    }
    /// Creates or replaces the object.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError>;
    /// Removes the object; removing a missing object is not an error (S3 semantics).
    fn delete(&self, key: &str) -> Result<(), DriveError>;
    /// The object's size, date and tag, without its bytes.
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError>;

    /// Creates or replaces the object with what `body` reads, to its end; returns the
    /// bytes written. By default they are read into memory and put at once (a bucket
    /// takes an object in one request); a folder on disk copies them into its temporary
    /// file piece by piece, so a big file never sits in memory whole.
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)?;
        self.put(key, &bytes)?;
        Ok(bytes.len() as u64)
    }

    /// Writes the object only when `condition` holds, in one step no other writer can
    /// come between (S3 `If-None-Match: *` / `If-Match`). `Ok` with the new version's
    /// entity tag when the drive tells it; [`DriveError::Conflict`] when the condition
    /// did not hold (nothing written); [`DriveError::Unsupported`] from a drive that
    /// cannot ask (the default: a folder on disk keeps no versions).
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let _ = (bytes, condition);
        Err(DriveError::Unsupported(format!(
            "writing \"{key}\" only if it is unchanged"
        )))
    }

    /// Copies the object `from` to `to` within this drive (replacing `to`
    /// when it is there; conflicts are the caller's). A folder is not one
    /// object: refused. By default one get and one put; a folder on disk
    /// copies the file, a bucket asks the service (CopyObject), so nothing
    /// passes through this process.
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        if let Some(folder) = [from, to].into_iter().find(|k| k.ends_with('/')) {
            return Err(DriveError::InvalidKey {
                key: folder.to_string(),
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

    /// Removes the folder `prefix` (not the root) only when nothing is in it but empty folders:
    /// `true` when it is gone (removed now, or never there), `false` when something is in it and
    /// it stays. What a move does with the folder it emptied - never [`Drive::delete_folder`],
    /// which removes what is in the folder NOW, the files nobody copied included. By default the
    /// folder's markers are deleted when a listing shows nothing else; a folder on disk is
    /// removed with `rmdir`.
    fn remove_empty_folder(&self, prefix: &str) -> Result<bool, DriveError> {
        ops::remove_empty_folder(self, prefix)
    }

    /// The file (or, for a folder prefix, the directory) on this computer that
    /// holds `key`, when the drive is a folder on disk; `None` otherwise.
    fn local_path(&self, key: &str) -> Option<PathBuf> {
        let _ = key;
        None
    }

    /// Where this drive's files are ([`xfer::Site`]): a folder on this computer, a bucket (or a
    /// part of one) or - by default - a place only this very drive object is provably the same
    /// as. What "the same drive", "this file is its own destination" and "this folder is moved
    /// into itself" are decided from ([`xfer::overlap`]) - never from drive ids or key text.
    fn site(&self) -> xfer::Site {
        xfer::Site::other_of(self)
    }

    /// The folder on this computer where the drive keeps what its cut uploads resume from (a
    /// bucket's multipart state); an encrypted drive keeps a file's encrypted copy there too,
    /// until its upload completes. `None` (the default): its uploads do not resume.
    fn resume_folder(&self) -> Option<PathBuf> {
        None
    }

    /// What the backend knows about the object beyond [`Drive::head`], as
    /// `(name, value)` pairs to show: an S3 object's Content-Type, storage class,
    /// encryption and user metadata; a local file's location. Empty by default.
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.head(key).map(|_| Vec::new())
    }

    /// Creates or replaces the object with the local file `path`; returns the bytes written.
    /// `ctl` hears the bytes written so far (from the threads the parts travel on) and is asked,
    /// once per chunk or part, whether to stop: a stopped upload answers
    /// [`DriveError::Cancelled`] and leaves nothing behind (a bucket aborts it and forgets its
    /// state; a transport error, unlike a stop, keeps the state for the next try). A closure
    /// `Fn(u64) + Sync` is a `Ctl` that only hears the progress; [`ctl::Quiet`] hears nothing.
    /// By default the file is streamed in ([`Drive::put_from`]); a bucket sends a big file's
    /// parts several at once and resumes an upload a killed app left unfinished
    /// ([`multipart`]).
    fn put_file(&self, key: &str, path: &Path, ctl: &dyn Ctl) -> Result<u64, DriveError> {
        let mut file = std::fs::File::open(path)?;
        let mut reader = transfer::ProgressReader::new(&mut file, ctl);
        self.put_from(key, &mut reader)
    }

    /// [`Drive::put_file`] of a file that must be the one `check` describes from the upload's
    /// start to its completion (a sync names the object by the file's BLAKE3):
    /// [`checked::CheckedUpload::Changed`], nothing completed, when it is not. By default the
    /// file is streamed in and hashed as it is read (a read that fails at its end stops the
    /// upload before it completes); a bucket sends a big file's parts several at once, resumes
    /// an upload a killed app or a dropped connection left unfinished, and hashes the whole file
    /// once more before it completes.
    fn put_file_checked(
        &self,
        key: &str,
        path: &Path,
        check: &checked::FileCheck,
        ctl: &dyn Ctl,
    ) -> Result<checked::CheckedUpload, DriveError> {
        // Opened once, without a link or a FIFO, and only the file the scan saw.
        let Some(file) = checked::open_checked(path, check)? else {
            return Ok(checked::CheckedUpload::Changed);
        };
        let mut hashed = checked::HashCheck::new(file, &check.blake3);
        let sent = {
            let mut reader = transfer::ProgressReader::new(&mut hashed, ctl);
            self.put_from(key, &mut reader)
        };
        match sent {
            Ok(written) => Ok(checked::CheckedUpload::Sent(written)),
            Err(_) if hashed.changed() => Ok(checked::CheckedUpload::Changed),
            Err(e) => Err(e),
        }
    }

    /// [`Drive::put_from`], only when `condition` holds ([`Drive::put_if`]'s rules): the new
    /// version's entity tag when the drive tells it; [`DriveError::Conflict`] when the
    /// condition did not hold. By default the body is read into memory and written with
    /// [`Drive::put_if`]; a bucket streams it as a multipart upload whose completion asks the
    /// condition.
    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)?;
        self.put_if(key, &bytes, condition)
    }

    /// Whether this drive writes only when a [`Precondition`] holds, in the step that makes the
    /// object visible: [`Drive::put_if`], [`Drive::put_from_if`] and [`Drive::put_file_if`] ask
    /// the condition THEN (a bucket: S3's `If-None-Match` / `If-Match` on the request that
    /// completes the object; an encrypted drive: the index change that names it), not at the
    /// start of a long upload. `false` by default: a folder on disk keeps no versions (a copy
    /// into it is guarded by the rename that puts the file in place, `durable::Expect`), and a
    /// data source answers [`DriveError::Unsupported`]. A caller that has a long write ahead asks
    /// first, so that it neither reads a big body only to hear "unsupported" nor writes the file
    /// unconditionally after one look at the name when the upload began.
    fn conditional_writes(&self) -> bool {
        false
    }

    /// [`Drive::put_file`], only when `condition` holds ([`Drive::put_if`]'s rules, asked when the
    /// upload completes): the new version's entity tag when the drive tells it;
    /// [`DriveError::Conflict`] when the condition did not hold, with nothing written (a stopped
    /// upload leaves nothing behind, as [`Drive::put_file`]'s). By default the file is streamed
    /// into [`Drive::put_from_if`] for a drive that [writes conditionally](Drive::conditional_writes)
    /// and refused at once, unread, with [`DriveError::Unsupported`] for any other - the caller
    /// looks at the name first and uses [`Drive::put_file`].
    fn put_file_if(
        &self,
        key: &str,
        path: &Path,
        condition: &Precondition,
        ctl: &dyn Ctl,
    ) -> Result<Option<String>, DriveError> {
        if !self.conditional_writes() {
            return Err(DriveError::Unsupported(format!(
                "writing \"{key}\" from a file only if it is unchanged"
            )));
        }
        let mut file = std::fs::File::open(path)?;
        let mut reader = transfer::ProgressReader::new(&mut file, ctl);
        self.put_from_if(key, &mut reader, condition)
    }

    /// Writes several files, each as [`Drive::put`] or (with a condition) [`Drive::put_if`]
    /// would: one result per item, in order, the entity tag when the drive tells it. A failed
    /// condition refuses that file only. By default one write after the other (a plain bucket
    /// keeps one object per file); an encrypted drive that packs puts small files into shared
    /// objects, one index change per pack ([`crate::packing`]).
    fn put_many(&self, items: &[BatchPut<'_>]) -> Vec<Result<Option<String>, DriveError>> {
        items
            .iter()
            .map(|item| match item.condition {
                Some(condition) => self.put_if(item.key, item.bytes, condition),
                None => self.put(item.key, item.bytes).map(|()| None),
            })
            .collect()
    }

    /// Deletes several files: one result per item, in order. A file that is gone is deleted;
    /// one that is no longer the version an item names is [`DriveError::Conflict`] and stays.
    /// By default one delete after the other (a version checked with a HEAD first); a bucket
    /// sends [`DELETE_BATCH`] keys a request (DeleteObjects), an encrypted drive changes its
    /// index once for all of them.
    fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
        items
            .iter()
            .map(|item| {
                if let Some(version) = item.version {
                    match self.head(item.key) {
                        Ok(info) if same_etag(info.etag.as_deref(), version) => {}
                        Ok(_) => {
                            return Err(DriveError::Conflict {
                                key: item.key.to_string(),
                            })
                        }
                        Err(DriveError::NotFound { .. }) => return Ok(()),
                        Err(e) => return Err(e),
                    }
                }
                self.delete(item.key)
            })
            .collect()
    }

    /// Moves several files: one result per item, in order. A move never lands on something
    /// that is there (an error naming the target), and one whose file is gone is
    /// [`DriveError::NotFound`]. By default each file is copied ([`Drive::copy`]: a bucket
    /// copies on the service, [`PARALLEL_FILES`] at once) and the copied ones are deleted in one
    /// [`Drive::delete_many`]; an encrypted drive changes its index once and moves no object.
    fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
        ops::move_by_copy(self, items, true)
    }

    /// Writes several files from their readers, each as [`Drive::put_from`] or (with a
    /// condition) [`Drive::put_from_if`] would: one result per item, in order. By default one
    /// after the other; a bucket sends [`PARALLEL_FILES`] at once, an encrypted drive packs the
    /// small ones and names all the large ones in one index change.
    fn put_from_many(
        &self,
        items: &mut [BatchStream<'_>],
    ) -> Vec<Result<Option<String>, DriveError>> {
        items
            .iter_mut()
            .map(|item| match item.condition {
                Some(condition) => self.put_from_if(item.key, &mut *item.body, condition),
                None => self.put_from(item.key, &mut *item.body).map(|_| None),
            })
            .collect()
    }
}

/// A drive behind a pointer is that drive: a `Box`, an `Arc` or a reference forwards
/// every call to it.
macro_rules! forward_drive {
    ($($pointer:ty),*) => {$(
        impl<D: Drive + ?Sized> Drive for $pointer {
            fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
                (**self).list(request)
            }
            fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
                (**self).get(key)
            }
            fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
                (**self).get_range(key, range)
            }
            fn get_range_if(
                &self,
                key: &str,
                range: ByteRange,
                condition: &Precondition,
            ) -> Result<Vec<u8>, DriveError> {
                (**self).get_range_if(key, range, condition)
            }
            fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
                (**self).get_limited(key, limit)
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
            fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
                (**self).put_from(key, body)
            }
            fn put_if(
                &self,
                key: &str,
                bytes: &[u8],
                condition: &Precondition,
            ) -> Result<Option<String>, DriveError> {
                (**self).put_if(key, bytes, condition)
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
            fn remove_empty_folder(&self, prefix: &str) -> Result<bool, DriveError> {
                (**self).remove_empty_folder(prefix)
            }
            fn local_path(&self, key: &str) -> Option<PathBuf> {
                (**self).local_path(key)
            }
            fn site(&self) -> xfer::Site {
                (**self).site()
            }
            fn resume_folder(&self) -> Option<PathBuf> {
                (**self).resume_folder()
            }
            fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
                (**self).metadata(key)
            }
            fn put_file(&self, key: &str, path: &Path, ctl: &dyn Ctl) -> Result<u64, DriveError> {
                (**self).put_file(key, path, ctl)
            }
            fn put_file_checked(
                &self,
                key: &str,
                path: &Path,
                check: &checked::FileCheck,
                ctl: &dyn Ctl,
            ) -> Result<checked::CheckedUpload, DriveError> {
                (**self).put_file_checked(key, path, check, ctl)
            }
            fn put_from_if(
                &self,
                key: &str,
                body: &mut dyn Read,
                condition: &Precondition,
            ) -> Result<Option<String>, DriveError> {
                (**self).put_from_if(key, body, condition)
            }
            fn conditional_writes(&self) -> bool {
                (**self).conditional_writes()
            }
            fn put_file_if(
                &self,
                key: &str,
                path: &Path,
                condition: &Precondition,
                ctl: &dyn Ctl,
            ) -> Result<Option<String>, DriveError> {
                (**self).put_file_if(key, path, condition, ctl)
            }
            fn put_many(
                &self,
                items: &[BatchPut<'_>],
            ) -> Vec<Result<Option<String>, DriveError>> {
                (**self).put_many(items)
            }
            fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
                (**self).delete_many(items)
            }
            fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
                (**self).move_many(items)
            }
            fn put_from_many(
                &self,
                items: &mut [BatchStream<'_>],
            ) -> Vec<Result<Option<String>, DriveError>> {
                (**self).put_from_many(items)
            }
        }
    )*};
}

forward_drive!(Box<D>, std::sync::Arc<D>, &D);

// ==== One list of the methods, and the wrapper that forwards what it does not name ====

/// THE list of the methods of [`Drive`], for every piece of code that has to cover each one of them
/// (ARCH29 C6e). `drive_methods!(callback; context)` calls `callback! { context ; entries }`, one
/// entry per method:
///
/// ```text
/// (send|pass, required|provided) fn name(argument: Type, ..) -> Return;
/// ```
///
/// - `required`: the trait has no default for it, an implementation writes it;
/// - `send`: a request that may be sent again as it is, so a wrapper that retries it (or renews what
///   it needs first) wraps it: [`DriveDelegate::send`];
/// - `pass`: forwarded as it is - a question about the drive (`site`, `local_path`) or a call a
///   wrapper has to decide about for itself: a body that is read once, a file's upload that hears a
///   stop, a batch. Never retried by default: what cannot be sent twice is not sent twice.
///
/// A new method of [`Drive`] is written here AND in the trait; `tests/drive_coverage.rs` fails
/// until both say the same and every `send` entry is a request that can be replayed. Every wrapper
/// made with [`delegate_drive!`] then forwards it without being touched (X-DELTA-AZDRIVE-FILES-2:
/// `RetryingDrive` forwarded neither `site()` nor `remove_empty_folder()`, and the trait's defaults
/// answered for the wrapper).
#[macro_export]
macro_rules! drive_methods {
    ($callback:path ; $($context:tt)*) => {
        $callback! {
            $($context)* ;
            (send, required) fn list(request: &$crate::ListRequest) -> ::core::result::Result<$crate::ListPage, $crate::DriveError>;
            (send, required) fn get(key: &str) -> ::core::result::Result<::std::vec::Vec<u8>, $crate::DriveError>;
            (send, required) fn get_range(key: &str, range: $crate::ByteRange) -> ::core::result::Result<::std::vec::Vec<u8>, $crate::DriveError>;
            (send, provided) fn get_range_if(key: &str, range: $crate::ByteRange, condition: &$crate::Precondition) -> ::core::result::Result<::std::vec::Vec<u8>, $crate::DriveError>;
            (send, provided) fn get_limited(key: &str, limit: $crate::Limit) -> ::core::result::Result<::std::vec::Vec<u8>, $crate::DriveError>;
            (send, required) fn put(key: &str, bytes: &[u8]) -> ::core::result::Result<(), $crate::DriveError>;
            (send, required) fn delete(key: &str) -> ::core::result::Result<(), $crate::DriveError>;
            (send, required) fn head(key: &str) -> ::core::result::Result<$crate::ObjectInfo, $crate::DriveError>;
            (pass, provided) fn put_from(key: &str, body: &mut dyn ::std::io::Read) -> ::core::result::Result<u64, $crate::DriveError>;
            (send, provided) fn put_if(key: &str, bytes: &[u8], condition: &$crate::Precondition) -> ::core::result::Result<::core::option::Option<::std::string::String>, $crate::DriveError>;
            (send, provided) fn copy(from: &str, to: &str) -> ::core::result::Result<(), $crate::DriveError>;
            (send, provided) fn create_folder(prefix: &str) -> ::core::result::Result<(), $crate::DriveError>;
            (send, provided) fn rename(from: &str, to: &str) -> ::core::result::Result<(), $crate::DriveError>;
            (send, provided) fn delete_folder(prefix: &str) -> ::core::result::Result<(), $crate::DriveError>;
            (send, provided) fn remove_empty_folder(prefix: &str) -> ::core::result::Result<bool, $crate::DriveError>;
            (pass, provided) fn local_path(key: &str) -> ::core::option::Option<::std::path::PathBuf>;
            (pass, provided) fn site() -> $crate::xfer::Site;
            (pass, provided) fn resume_folder() -> ::core::option::Option<::std::path::PathBuf>;
            (send, provided) fn metadata(key: &str) -> ::core::result::Result<::std::vec::Vec<(::std::string::String, ::std::string::String)>, $crate::DriveError>;
            (pass, provided) fn put_file(key: &str, path: &::std::path::Path, ctl: &dyn $crate::Ctl) -> ::core::result::Result<u64, $crate::DriveError>;
            (pass, provided) fn put_file_checked(key: &str, path: &::std::path::Path, check: &$crate::checked::FileCheck, ctl: &dyn $crate::Ctl) -> ::core::result::Result<$crate::checked::CheckedUpload, $crate::DriveError>;
            (pass, provided) fn put_from_if(key: &str, body: &mut dyn ::std::io::Read, condition: &$crate::Precondition) -> ::core::result::Result<::core::option::Option<::std::string::String>, $crate::DriveError>;
            (pass, provided) fn conditional_writes() -> bool;
            (pass, provided) fn put_file_if(key: &str, path: &::std::path::Path, condition: &$crate::Precondition, ctl: &dyn $crate::Ctl) -> ::core::result::Result<::core::option::Option<::std::string::String>, $crate::DriveError>;
            (pass, provided) fn put_many(items: &[$crate::BatchPut<'_>]) -> ::std::vec::Vec<::core::result::Result<::core::option::Option<::std::string::String>, $crate::DriveError>>;
            (pass, provided) fn delete_many(items: &[$crate::BatchDelete<'_>]) -> ::std::vec::Vec<::core::result::Result<(), $crate::DriveError>>;
            (pass, provided) fn move_many(items: &[$crate::BatchMove<'_>]) -> ::std::vec::Vec<::core::result::Result<(), $crate::DriveError>>;
            (pass, provided) fn put_from_many(items: &mut [$crate::BatchStream<'_>]) -> ::std::vec::Vec<::core::result::Result<::core::option::Option<::std::string::String>, $crate::DriveError>>;
        }
    };
}

/// One method of [`DriveDelegate`], by its kind in [`drive_methods!`]: a `send` goes through
/// [`DriveDelegate::send`], a `pass` straight to the drive below.
#[doc(hidden)]
#[macro_export]
macro_rules! __drive_delegate_method {
    (send fn $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty) => {
        fn $name(&self $(, $arg: $ty)*) -> $ret {
            self.send(|drive| drive.$name($($arg),*))
        }
    };
    (pass fn $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty) => {
        fn $name(&self $(, $arg: $ty)*) -> $ret {
            self.below().$name($($arg),*)
        }
    };
}

/// The trait [`DriveDelegate`], its methods written from [`drive_methods!`].
#[doc(hidden)]
#[macro_export]
macro_rules! __drive_delegate_trait {
    ( ; $( ($kind:ident, $required:ident) fn $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty ; )* ) => {
        /// What a wrapper of a drive does with a call, with a default for every method of [`Drive`]
        /// (ARCH29 C6e): it goes to the drive below ([`DriveDelegate::below`]). A request that may
        /// be sent again as it is goes through [`DriveDelegate::send`], which sends it as it is
        /// unless the wrapper says how else; any other call - a body that is read once, a file's
        /// upload that hears a stop, a batch, a question about the drive - goes straight, unless
        /// the wrapper writes the method itself. [`drive_methods!`] says which is which.
        ///
        /// A wrapper implements this trait BY PATH (`impl azul_storage::DriveDelegate for X`, never
        /// a `use` of it: it names every method of [`Drive`], and a type that has both traits in
        /// scope makes `x.get(..)` ambiguous), writes `below`, `send` where it wraps requests and the
        /// methods it changes, and calls [`delegate_drive!`] for the `impl Drive`. A method it does
        /// not write is forwarded, and so is a method the trait gets next year: the wrapper that
        /// forgot one cannot answer it for itself any more (X-DELTA-AZDRIVE-FILES-2).
        ///
        /// Not for a wrapper whose forwarding as it is would be WRONG for a method nobody has
        /// looked at: the encryption wrappers (`AutoEncrypted`, `EncryptedDrive`: each method has
        /// its own `Op` and a plaintext call below them is the bug), `ScopedDrive` (every key is
        /// rewritten and every write checked against the grant), `AzlinDrive` and the bridge's
        /// account drive (no one drive below; the trait's defaults are deliberate). They write
        /// every method, and `tests/drive_coverage.rs` counts them.
        pub trait DriveDelegate {
            /// The drive below.
            fn below(&self) -> &dyn $crate::Drive;

            /// Sends a request that may be sent again as it is (every `send` method of
            /// [`drive_methods!`]): `request` is given the drive below. The default sends it once,
            /// as it is. A wrapper that retries it, waits for a gate or renews credentials first,
            /// says so here.
            ///
            /// # Errors
            ///
            /// What `request` answers, or the wrapper's own refusal.
            fn send<T>(
                &self,
                request: impl Fn(&dyn $crate::Drive) -> ::core::result::Result<T, $crate::DriveError>,
            ) -> ::core::result::Result<T, $crate::DriveError> {
                request(self.below())
            }

            $( $crate::__drive_delegate_method! { $kind fn $name ( $($arg : $ty),* ) -> $ret } )*
        }
    };
}

/// `impl Drive for $wrapper`: each method calls the method of the same name of the wrapper's
/// [`DriveDelegate`].
#[doc(hidden)]
#[macro_export]
macro_rules! __drive_delegate_impl {
    ( $wrapper:ty ; $( ($kind:ident, $required:ident) fn $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty ; )* ) => {
        impl $crate::Drive for $wrapper {
            $(
                fn $name(&self $(, $arg: $ty)*) -> $ret {
                    <$wrapper as $crate::DriveDelegate>::$name(self $(, $arg)*)
                }
            )*
        }
    };
}

/// A drive that wraps another: `impl Drive for $wrapper` made from its [`DriveDelegate`], so that
/// every method of [`Drive`] is forwarded - the ones the wrapper wrote through its own method, the
/// rest through the default (ARCH29 C6e). The wrapper type has no generics.
///
/// ```text
/// impl azul_storage::DriveDelegate for RetryingDrive {
///     fn below(&self) -> &dyn Drive { &*self.inner }
///     fn send<T>(&self, request: impl Fn(&dyn Drive) -> Result<T, DriveError>) -> Result<T, DriveError> {
///         self.call(|| request(&*self.inner))
///     }
///     fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> { .. }
/// }
/// azul_storage::delegate_drive!(RetryingDrive);
/// ```
#[macro_export]
macro_rules! delegate_drive {
    ($wrapper:ty) => {
        $crate::drive_methods!($crate::__drive_delegate_impl; $wrapper);
    };
}

drive_methods!(__drive_delegate_trait;);
