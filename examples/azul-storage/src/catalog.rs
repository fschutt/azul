//! The data sources an app offers in its "Add drive" dialog, each with the form it asks for:
//! S3-compatible storage and a folder on this computer in every build (azul-storage's own
//! `S3Drive` and `LocalDrive`), the services Apache OpenDAL reaches with the feature
//! `opendal`, the databases browsed as tables with the feature `sql`.
//!
//! A filled form becomes a drives-file entry WITHOUT secrets and the text of its keyring
//! entry ([`build_entry`]); an entry fills its form again for editing ([`form_values`]).
//!
//! The OpenDAL services listed are the ones that build on macOS, Windows and Linux from Rust
//! alone and reach their service over HTTP (through the app's transport, azul's TLS stack) -
//! plus FTP and Redis, which speak their own protocols over tokio. Not listed: SFTP (Unix only,
//! it drives the system's `ssh`), HDFS, RocksDB, FoundationDB, TiKV, etcd (C/C++/Java or
//! protoc at build time), Hugging Face (its xet client brings reqwest), MongoDB (its driver is
//! large; a feature of its own later). OpenDAL's own PostgreSQL / MySQL / SQLite services map
//! ONE key/value table; the database sources here show every table instead, over the same
//! driver (sqlx).
//!
//! No azul types here: tested without a window. Nothing here prints a secret.

use std::{collections::BTreeMap, fmt};

use crate::{
    config::{DatabaseEngine, DriveAuth, DriveEntry, DriveLocation, SecretOptions},
    Credentials, HttpCall, HttpReply, S3Config, S3Drive, Transport,
};

/// The groups of the dialog's list, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceGroup {
    CloudStorage,
    NetworkNas,
    ConsumerCloud,
    Developer,
    Databases,
}

impl ServiceGroup {
    /// Every group, in the dialog's order.
    pub const ALL: [ServiceGroup; 5] = [
        ServiceGroup::CloudStorage,
        ServiceGroup::NetworkNas,
        ServiceGroup::ConsumerCloud,
        ServiceGroup::Developer,
        ServiceGroup::Databases,
    ];

    /// The group's heading.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            ServiceGroup::CloudStorage => "Cloud object storage",
            ServiceGroup::NetworkNas => "Network & NAS",
            ServiceGroup::ConsumerCloud => "Consumer clouds",
            ServiceGroup::Developer => "Developer",
            ServiceGroup::Databases => "Databases & key-value",
        }
    }
}

/// What opens a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// azul-storage's own S3 client: `DriveLocation::S3`.
    S3,
    /// A folder on this computer: `DriveLocation::Local`.
    Local,
    /// An OpenDAL service by its scheme: `DriveLocation::Opendal`.
    Opendal(&'static str),
    /// A database browsed as tables: `DriveLocation::Database`.
    Database(DatabaseEngine),
}

/// What a field of a form holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    /// A password, a token, a key: the keyring's, never the drives file's.
    Secret,
    /// An address with its scheme (`https://`, `ftp://`, `tcp://`).
    Url,
    /// A file or folder on this computer.
    Path,
    /// Digits.
    Number,
    /// `true` or `false` (a check box).
    Bool,
    /// One of these words (a drop-down). The field's `default` is the word a new form holds - a
    /// Choice names one (a test over the catalog), and a TLS field's is the word that VERIFIES
    /// the server ([`PG_SSLMODE_DEFAULT`], [`MYSQL_SSLMODE_DEFAULT`]), never the one that sends a
    /// password to whoever answers (AZ-CN2).
    Choice(&'static [&'static str]),
}

/// One field of a source's form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    /// The setting's name: OpenDAL's option, the database's connection setting.
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    /// What the empty field shows.
    pub placeholder: &'static str,
    /// A line under the field; empty for none.
    pub help: &'static str,
    /// What a new form holds; empty for nothing.
    pub default: &'static str,
    /// The schemes an address field takes (`https`, `http`; FTP's `ftp`, `ftps`); empty for a
    /// field that is no address.
    pub schemes: &'static [&'static str],
}

/// What a source can do beyond browsing, as an app decides on it BEFORE it offers a command
/// (AZ-CN4: "Sync with a folder" was offered for every source, and an OpenDAL one failed only at
/// the commit, after the whole folder had gone up as plain files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// A write can say "only if the object is still the one I read" (S3's `If-Match` /
    /// `If-None-Match`, [`crate::Drive::put_if`]): a sync with a folder commits its index that
    /// way. A source without it is never offered the sync.
    pub conditional_writes: bool,
    /// The source speaks WebDAV, so its requests are PROPFIND, MKCOL, COPY, MOVE, ... and the
    /// transport must send those verbs.
    pub webdav_verbs: bool,
    /// A listing shows at most this many rows of a table (a database); `None`: no cap.
    pub rows_cap: Option<u64>,
}

impl Capabilities {
    /// A source that can do none of it.
    pub const NONE: Capabilities = Capabilities {
        conditional_writes: false,
        webdav_verbs: false,
        rows_cap: None,
    };

    /// A bucket or a folder: writes that say "only if unchanged".
    pub const CONDITIONAL: Capabilities = Capabilities {
        conditional_writes: true,
        ..Capabilities::NONE
    };
}

/// One source of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceSpec {
    /// Stable: it names the service in the app's DOM ids and its scripts.
    pub id: &'static str,
    pub name: &'static str,
    pub group: ServiceGroup,
    /// A Material icon name.
    pub icon: &'static str,
    /// One line under the name.
    pub summary: &'static str,
    pub backend: Backend,
    pub fields: &'static [FieldSpec],
    /// It can be browsed but not written (a web server, a database, an IPFS gateway).
    pub read_only: bool,
    /// What it can do ([`Capabilities`]).
    pub caps: Capabilities,
    /// A source whose credentials are optional - it would otherwise use whatever this computer
    /// has: the developer's gcloud login, an environment variable - needs one of these fields
    /// filled in (a switch counts when it is on); empty: the source has no such fallback
    /// (AZ-CN9).
    pub needs_one_of: &'static [&'static str],
}

impl ServiceSpec {
    /// Whether this build can open it (S3 and folders always; OpenDAL's with the feature
    /// `opendal`, databases with `sql`).
    #[must_use]
    pub const fn available(&self) -> bool {
        self.unavailable_reason().is_none()
    }

    /// Why this build cannot open it, as a sentence; `None` when it can.
    #[must_use]
    pub const fn unavailable_reason(&self) -> Option<&'static str> {
        match self.backend {
            Backend::S3 | Backend::Local => None,
            Backend::Opendal(_) => {
                if cfg!(feature = "opendal") {
                    None
                } else {
                    Some("This app was built without OpenDAL (azul-storage's feature \"opendal\").")
                }
            }
            Backend::Database(_) => {
                if cfg!(feature = "sql") {
                    None
                } else {
                    Some(
                        "This app was built without the database drivers (azul-storage's \
                         feature \"sql\").",
                    )
                }
            }
        }
    }

    /// The field `key`.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&'static FieldSpec> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// The keys of the secret fields.
    pub fn secret_keys(&self) -> impl Iterator<Item = &'static str> {
        self.fields
            .iter()
            .filter(|f| f.kind == FieldKind::Secret)
            .map(|f| f.key)
    }

    /// What a new form of this source holds: every field's default.
    #[must_use]
    pub fn defaults(&self) -> FormValues {
        self.fields
            .iter()
            .filter_map(|f| {
                let value = match f.kind {
                    FieldKind::Choice(words) if f.default.is_empty() => words.first().copied(),
                    _ if f.default.is_empty() => None,
                    _ => Some(f.default),
                };
                value.map(|v| (f.key.to_string(), v.to_string()))
            })
            .collect()
    }
}

/// What a form holds: a value per field key (the drive's name is the form's own).
pub type FormValues = BTreeMap<String, String>;

// ==== The fields ====

const fn field(
    key: &'static str,
    label: &'static str,
    kind: FieldKind,
    required: bool,
    placeholder: &'static str,
) -> FieldSpec {
    FieldSpec {
        key,
        label,
        kind,
        required,
        placeholder,
        help: "",
        default: "",
        schemes: match kind {
            FieldKind::Url => HTTP_SCHEMES,
            _ => &[],
        },
    }
}

/// The schemes of a web address: TLS first.
const HTTP_SCHEMES: &[&str] = &["https", "http"];
/// FTP's.
const FTP_SCHEMES: &[&str] = &["ftp", "ftps"];
/// Redis'.
const REDIS_SCHEMES: &[&str] = &["tcp", "redis", "rediss"];

const fn with_schemes(mut f: FieldSpec, schemes: &'static [&'static str]) -> FieldSpec {
    f.schemes = schemes;
    f
}

const fn with_default(mut f: FieldSpec, default: &'static str) -> FieldSpec {
    f.default = default;
    f
}

const fn with_help(mut f: FieldSpec, help: &'static str) -> FieldSpec {
    f.help = help;
    f
}

/// The folder inside the source the drive starts at.
const ROOT: FieldSpec = field(
    "root",
    "Folder",
    FieldKind::Text,
    false,
    "/ (the whole source)",
);

const S3_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://s3.example.com",
    ),
    field("region", "Region", FieldKind::Text, false, "us-east-1"),
    field("bucket", "Bucket", FieldKind::Text, true, "my-bucket"),
    field("access_key_id", "Access key", FieldKind::Text, true, ""),
    with_help(
        field("secret_access_key", "Secret key", FieldKind::Secret, true, ""),
        "Kept in the system keyring only.",
    ),
    with_default(
        field(
            "path_style",
            "Path-style URLs (self-hosted servers)",
            FieldKind::Bool,
            false,
            "",
        ),
        "true",
    ),
];

const LOCAL_FIELDS: &[FieldSpec] = &[field(
    "root",
    "Folder",
    FieldKind::Path,
    true,
    "/path/to/folder",
)];

const GCS_FIELDS: &[FieldSpec] = &[
    field("bucket", "Bucket", FieldKind::Text, true, "my-bucket"),
    with_help(
        field("credential", "Service account key", FieldKind::Secret, false, ""),
        "The service account's JSON key, base64-encoded.",
    ),
    with_default(
        field(
            "anonymous",
            "Public bucket, no credentials",
            FieldKind::Bool,
            false,
            "",
        ),
        "false",
    ),
    with_default(
        field(
            "machine_identity",
            "Use this computer's Google login",
            FieldKind::Bool,
            false,
            "",
        ),
        "false",
    ),
    ROOT,
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        false,
        "https://storage.googleapis.com",
    ),
];

const AZBLOB_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://<account>.blob.core.windows.net",
    ),
    field("container", "Container", FieldKind::Text, true, ""),
    field("account_name", "Account name", FieldKind::Text, false, ""),
    field("account_key", "Account key", FieldKind::Secret, false, ""),
    field("sas_token", "SAS token", FieldKind::Secret, false, ""),
    ROOT,
];

const AZDLS_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://<account>.dfs.core.windows.net",
    ),
    field("filesystem", "File system", FieldKind::Text, true, ""),
    field("account_name", "Account name", FieldKind::Text, false, ""),
    field("account_key", "Account key", FieldKind::Secret, false, ""),
    ROOT,
];

const AZFILE_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://<account>.file.core.windows.net",
    ),
    field("share_name", "Share", FieldKind::Text, true, ""),
    field("account_name", "Account name", FieldKind::Text, false, ""),
    field("account_key", "Account key", FieldKind::Secret, false, ""),
    field("sas_token", "SAS token", FieldKind::Secret, false, ""),
    ROOT,
];

const B2_FIELDS: &[FieldSpec] = &[
    field("bucket", "Bucket", FieldKind::Text, true, ""),
    field("bucket_id", "Bucket ID", FieldKind::Text, true, ""),
    field("application_key_id", "Key ID", FieldKind::Text, true, ""),
    field("application_key", "Application key", FieldKind::Secret, true, ""),
    ROOT,
];

const SWIFT_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://swift.example.com/v1/AUTH_account",
    ),
    field("container", "Container", FieldKind::Text, true, ""),
    field("token", "Token", FieldKind::Secret, true, ""),
    ROOT,
];

const OSS_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://oss-eu-central-1.aliyuncs.com",
    ),
    field("bucket", "Bucket", FieldKind::Text, true, ""),
    field("access_key_id", "Access key ID", FieldKind::Text, false, ""),
    field("access_key_secret", "Access key secret", FieldKind::Secret, false, ""),
    ROOT,
];

const COS_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://cos.ap-guangzhou.myqcloud.com",
    ),
    field("bucket", "Bucket", FieldKind::Text, true, ""),
    field("secret_id", "Secret ID", FieldKind::Text, false, ""),
    field("secret_key", "Secret key", FieldKind::Secret, false, ""),
    ROOT,
];

const OBS_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Endpoint",
        FieldKind::Url,
        true,
        "https://obs.cn-north-4.myhuaweicloud.com",
    ),
    field("bucket", "Bucket", FieldKind::Text, true, ""),
    field("access_key_id", "Access key ID", FieldKind::Text, false, ""),
    field("secret_access_key", "Secret access key", FieldKind::Secret, false, ""),
    ROOT,
];

const UPYUN_FIELDS: &[FieldSpec] = &[
    field("bucket", "Service", FieldKind::Text, true, ""),
    field("operator", "Operator", FieldKind::Text, true, ""),
    field("password", "Password", FieldKind::Secret, true, ""),
    ROOT,
];

const WEBDAV_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Server address",
        FieldKind::Url,
        true,
        "https://cloud.example.com/remote.php/dav/files/ann",
    ),
    field("username", "User name", FieldKind::Text, false, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    with_help(
        field("token", "Bearer token", FieldKind::Secret, false, ""),
        "Instead of a password.",
    ),
    ROOT,
];

const FTP_FIELDS: &[FieldSpec] = &[
    with_schemes(
        with_help(
            field(
                "endpoint",
                "Server address",
                FieldKind::Url,
                true,
                "ftp://nas.local:21",
            ),
            "ftps:// for FTP over TLS.",
        ),
        FTP_SCHEMES,
    ),
    field("user", "User name", FieldKind::Text, false, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    ROOT,
];

const HTTP_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Address",
        FieldKind::Url,
        true,
        "https://example.com/files/",
    ),
    field("username", "User name", FieldKind::Text, false, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    field("token", "Bearer token", FieldKind::Secret, false, ""),
    ROOT,
];

const WEBHDFS_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "NameNode address",
        FieldKind::Url,
        true,
        "http://namenode:9870",
    ),
    field("user_name", "User name", FieldKind::Text, false, ""),
    field("delegation", "Delegation token", FieldKind::Secret, false, ""),
    ROOT,
];

/// Google Drive, Dropbox, OneDrive: an OAuth refresh token with the app's client (or a
/// short-lived access token).
const OAUTH_FIELDS: &[FieldSpec] = &[
    with_help(
        field("refresh_token", "Refresh token", FieldKind::Secret, false, ""),
        "From the service's OAuth consent, with the client below.",
    ),
    field("client_id", "OAuth client ID", FieldKind::Text, false, ""),
    field("client_secret", "OAuth client secret", FieldKind::Secret, false, ""),
    with_help(
        field("access_token", "Access token", FieldKind::Secret, false, ""),
        "Instead of a refresh token (it expires).",
    ),
    ROOT,
];

const KOOFR_FIELDS: &[FieldSpec] = &[
    with_default(
        field("endpoint", "Server", FieldKind::Url, true, "https://app.koofr.net"),
        "https://app.koofr.net",
    ),
    field("email", "Email", FieldKind::Text, true, ""),
    with_help(
        field("password", "Application password", FieldKind::Secret, true, ""),
        "Made in Koofr's preferences, not your login password.",
    ),
    ROOT,
];

const PCLOUD_FIELDS: &[FieldSpec] = &[
    with_default(
        field("endpoint", "Server", FieldKind::Url, true, "https://api.pcloud.com"),
        "https://api.pcloud.com",
    ),
    field("username", "User name", FieldKind::Text, true, ""),
    field("password", "Password", FieldKind::Secret, true, ""),
    ROOT,
];

const SEAFILE_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Server",
        FieldKind::Url,
        true,
        "https://seafile.example.com",
    ),
    field("username", "User name", FieldKind::Text, true, ""),
    field("password", "Password", FieldKind::Secret, true, ""),
    field("repo_name", "Library", FieldKind::Text, true, ""),
    ROOT,
];

const YANDEX_FIELDS: &[FieldSpec] = &[
    field("access_token", "OAuth token", FieldKind::Secret, true, ""),
    ROOT,
];

const GITHUB_FIELDS: &[FieldSpec] = &[
    field("owner", "Owner", FieldKind::Text, true, "octocat"),
    field("repo", "Repository", FieldKind::Text, true, "hello-world"),
    with_help(
        field("token", "Access token", FieldKind::Secret, false, ""),
        "Needed for private repositories and to write.",
    ),
    ROOT,
];

const LAKEFS_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Server",
        FieldKind::Url,
        true,
        "https://lakefs.example.com",
    ),
    field("repository", "Repository", FieldKind::Text, true, ""),
    with_default(field("branch", "Branch", FieldKind::Text, false, "main"), "main"),
    field("username", "Access key ID", FieldKind::Text, false, ""),
    field("password", "Secret access key", FieldKind::Secret, false, ""),
    ROOT,
];

const VERCEL_BLOB_FIELDS: &[FieldSpec] = &[
    field("token", "Read-write token", FieldKind::Secret, true, ""),
    ROOT,
];

const DBFS_FIELDS: &[FieldSpec] = &[
    field(
        "endpoint",
        "Workspace",
        FieldKind::Url,
        true,
        "https://<workspace>.cloud.databricks.com",
    ),
    field("token", "Access token", FieldKind::Secret, true, ""),
    ROOT,
];

const IPFS_FIELDS: &[FieldSpec] = &[
    with_default(
        field("endpoint", "Gateway", FieldKind::Url, true, "https://ipfs.io"),
        "https://ipfs.io",
    ),
    field("root", "Path", FieldKind::Text, true, "/ipfs/<CID>/"),
];

const REDIS_FIELDS: &[FieldSpec] = &[
    with_schemes(
        with_help(
            field(
                "endpoint",
                "Address",
                FieldKind::Url,
                true,
                "tcp://127.0.0.1:6379",
            ),
            "rediss:// for TLS. Keys with / in them show as folders.",
        ),
        REDIS_SCHEMES,
    ),
    field("username", "User name", FieldKind::Text, false, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    with_default(field("db", "Database number", FieldKind::Number, false, "0"), "0"),
    field("root", "Key prefix", FieldKind::Text, false, ""),
];

/// The TLS words of a PostgreSQL form, the verifying one first. `prefer` (sqlx's own default)
/// answers a server's "no TLS" with plain text and accepts any certificate, and sqlx answers a
/// server's request for the password in clear text: whoever sits on the path reads it (AZ-CN2).
pub const PG_SSLMODES: &[&str] = &["verify-full", "require", "disable", "prefer"];
/// What a new PostgreSQL form holds, and what an entry without the word means: the connection
/// checks the server's certificate and its name.
pub const PG_SSLMODE_DEFAULT: &str = "verify-full";
/// The TLS words of a MySQL / MariaDB form, the verifying one first.
pub const MYSQL_SSLMODES: &[&str] = &["verify-identity", "required", "disabled", "preferred"];
/// What a new MySQL form holds, and what an entry without the word means.
pub const MYSQL_SSLMODE_DEFAULT: &str = "verify-identity";

/// Whether the two texts are the same (a const fn cannot compare `str`s with `==`).
const fn same_text(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Whether `word` is one of `words`.
const fn has_word(words: &[&str], word: &str) -> bool {
    let mut i = 0;
    while i < words.len() {
        if same_text(words[i], word) {
            return true;
        }
        i += 1;
    }
    false
}

// A TLS default that is not one of the form's words does not compile.
const _: () = assert!(has_word(PG_SSLMODES, PG_SSLMODE_DEFAULT));
const _: () = assert!(has_word(MYSQL_SSLMODES, MYSQL_SSLMODE_DEFAULT));

const POSTGRES_FIELDS: &[FieldSpec] = &[
    with_default(field("host", "Host", FieldKind::Text, true, "localhost"), "localhost"),
    with_default(field("port", "Port", FieldKind::Number, false, "5432"), "5432"),
    field("database", "Database", FieldKind::Text, true, ""),
    field("user", "User", FieldKind::Text, true, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    with_default(
        with_help(
            field("sslmode", "TLS", FieldKind::Choice(PG_SSLMODES), false, ""),
            "verify-full checks the server's certificate and name; require encrypts without checking it; prefer falls back to no encryption.",
        ),
        PG_SSLMODE_DEFAULT,
    ),
];

const MYSQL_FIELDS: &[FieldSpec] = &[
    with_default(field("host", "Host", FieldKind::Text, true, "localhost"), "localhost"),
    with_default(field("port", "Port", FieldKind::Number, false, "3306"), "3306"),
    field("database", "Database", FieldKind::Text, true, ""),
    field("user", "User", FieldKind::Text, true, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    with_default(
        with_help(
            field("sslmode", "TLS", FieldKind::Choice(MYSQL_SSLMODES), false, ""),
            "verify-identity checks the server's certificate and name; required encrypts without checking it; preferred falls back to no encryption.",
        ),
        MYSQL_SSLMODE_DEFAULT,
    ),
];

const SQLITE_FIELDS: &[FieldSpec] = &[field(
    "path",
    "Database file",
    FieldKind::Path,
    true,
    "/path/to/database.sqlite",
)];

// ==== The sources ====

const fn source(
    id: &'static str,
    name: &'static str,
    group: ServiceGroup,
    icon: &'static str,
    summary: &'static str,
    backend: Backend,
    fields: &'static [FieldSpec],
) -> ServiceSpec {
    ServiceSpec {
        id,
        name,
        group,
        icon,
        summary,
        backend,
        fields,
        read_only: false,
        caps: Capabilities::NONE,
        needs_one_of: &[],
    }
}

const fn read_only(mut s: ServiceSpec) -> ServiceSpec {
    s.read_only = true;
    s
}

const fn with_caps(mut s: ServiceSpec, caps: Capabilities) -> ServiceSpec {
    s.caps = caps;
    s
}

const fn needing_one_of(mut s: ServiceSpec, keys: &'static [&'static str]) -> ServiceSpec {
    s.needs_one_of = keys;
    s
}

/// A database lists at most [`crate::tables::MAX_ROW_FILES`] rows of a table.
const ROWS_CAPPED: Capabilities = Capabilities {
    rows_cap: Some(crate::tables::MAX_ROW_FILES),
    ..Capabilities::NONE
};

/// WebDAV's verbs.
const WEBDAV_VERBS: Capabilities = Capabilities {
    webdav_verbs: true,
    ..Capabilities::NONE
};

use Backend::{Database as Db, Opendal as Dal};
use ServiceGroup::{CloudStorage, ConsumerCloud, Databases, Developer, NetworkNas};

static SERVICES: &[ServiceSpec] = &[
    // ---- Cloud object storage ----
    with_caps(
        source(
            "s3",
            "S3-compatible storage",
            CloudStorage,
            "cloud",
            "Any S3-compatible storage",
            Backend::S3,
            S3_FIELDS,
        ),
        Capabilities::CONDITIONAL,
    ),
    needing_one_of(
        source(
            "gcs",
            "Google Cloud Storage",
            CloudStorage,
            "cloud",
            "A bucket with a service account's key",
            Dal("gcs"),
            GCS_FIELDS,
        ),
        &["credential", "anonymous", "machine_identity"],
    ),
    needing_one_of(
        source(
            "azblob",
            "Azure Blob Storage",
            CloudStorage,
            "cloud",
            "A container of a storage account",
            Dal("azblob"),
            AZBLOB_FIELDS,
        ),
        &["account_key", "sas_token"],
    ),
    needing_one_of(
        source(
            "azdls",
            "Azure Data Lake Storage Gen2",
            CloudStorage,
            "cloud",
            "A file system of a storage account",
            Dal("azdls"),
            AZDLS_FIELDS,
        ),
        &["account_key"],
    ),
    needing_one_of(
        source(
            "azfile",
            "Azure Files",
            CloudStorage,
            "folder_shared",
            "A file share of a storage account",
            Dal("azfile"),
            AZFILE_FIELDS,
        ),
        &["account_key", "sas_token"],
    ),
    source(
        "b2",
        "Backblaze B2",
        CloudStorage,
        "cloud",
        "B2's own API (its S3 API: S3-compatible storage)",
        Dal("b2"),
        B2_FIELDS,
    ),
    source(
        "swift",
        "OpenStack Swift",
        CloudStorage,
        "cloud",
        "A container, with a token",
        Dal("swift"),
        SWIFT_FIELDS,
    ),
    needing_one_of(
        source(
            "oss",
            "Alibaba Cloud OSS",
            CloudStorage,
            "cloud",
            "Object Storage Service",
            Dal("oss"),
            OSS_FIELDS,
        ),
        &["access_key_secret"],
    ),
    needing_one_of(
        source(
            "cos",
            "Tencent Cloud COS",
            CloudStorage,
            "cloud",
            "Cloud Object Storage",
            Dal("cos"),
            COS_FIELDS,
        ),
        &["secret_key"],
    ),
    needing_one_of(
        source(
            "obs",
            "Huawei Cloud OBS",
            CloudStorage,
            "cloud",
            "Object Storage Service",
            Dal("obs"),
            OBS_FIELDS,
        ),
        &["secret_access_key"],
    ),
    source(
        "upyun",
        "Upyun",
        CloudStorage,
        "cloud",
        "Upyun storage",
        Dal("upyun"),
        UPYUN_FIELDS,
    ),
    // ---- Network & NAS ----
    with_caps(
        source(
            "local",
            "Folder on this computer",
            NetworkNas,
            "folder",
            "A folder, a mounted NAS share, an external disk",
            Backend::Local,
            LOCAL_FIELDS,
        ),
        Capabilities::CONDITIONAL,
    ),
    with_caps(
        source(
            "webdav",
            "WebDAV",
            NetworkNas,
            "dns",
            "Nextcloud, ownCloud, Synology, QNAP, any WebDAV server",
            Dal("webdav"),
            WEBDAV_FIELDS,
        ),
        WEBDAV_VERBS,
    ),
    source(
        "ftp",
        "FTP / FTPS",
        NetworkNas,
        "dns",
        "A NAS or a server over FTP",
        Dal("ftp"),
        FTP_FIELDS,
    ),
    read_only(source(
        "http",
        "Web server",
        NetworkNas,
        "public",
        "Files of a website (read-only)",
        Dal("http"),
        HTTP_FIELDS,
    )),
    source(
        "webhdfs",
        "Hadoop WebHDFS",
        NetworkNas,
        "dns",
        "HDFS through its REST API",
        Dal("webhdfs"),
        WEBHDFS_FIELDS,
    ),
    // ---- Consumer clouds ----
    source(
        "gdrive",
        "Google Drive",
        ConsumerCloud,
        "add_to_drive",
        "My Drive, with an OAuth token",
        Dal("gdrive"),
        OAUTH_FIELDS,
    ),
    source(
        "dropbox",
        "Dropbox",
        ConsumerCloud,
        "cloud",
        "Your Dropbox, with an OAuth token",
        Dal("dropbox"),
        OAUTH_FIELDS,
    ),
    source(
        "onedrive",
        "OneDrive",
        ConsumerCloud,
        "cloud",
        "Your OneDrive, with an OAuth token",
        Dal("onedrive"),
        OAUTH_FIELDS,
    ),
    source(
        "koofr",
        "Koofr",
        ConsumerCloud,
        "cloud",
        "Koofr with an application password",
        Dal("koofr"),
        KOOFR_FIELDS,
    ),
    source(
        "pcloud",
        "pCloud",
        ConsumerCloud,
        "cloud",
        "pCloud (api.pcloud.com, eapi.pcloud.com in Europe)",
        Dal("pcloud"),
        PCLOUD_FIELDS,
    ),
    source(
        "seafile",
        "Seafile",
        ConsumerCloud,
        "cloud",
        "A library of a Seafile server",
        Dal("seafile"),
        SEAFILE_FIELDS,
    ),
    source(
        "yandex-disk",
        "Yandex Disk",
        ConsumerCloud,
        "cloud",
        "Yandex Disk with an OAuth token",
        Dal("yandex-disk"),
        YANDEX_FIELDS,
    ),
    // ---- Developer ----
    source(
        "github",
        "GitHub repository",
        Developer,
        "code",
        "A repository's files",
        Dal("github"),
        GITHUB_FIELDS,
    ),
    source(
        "lakefs",
        "lakeFS",
        Developer,
        "account_tree",
        "A branch of a lakeFS repository",
        Dal("lakefs"),
        LAKEFS_FIELDS,
    ),
    source(
        "vercel-blob",
        "Vercel Blob",
        Developer,
        "cloud",
        "A Vercel Blob store",
        Dal("vercel-blob"),
        VERCEL_BLOB_FIELDS,
    ),
    source(
        "dbfs",
        "Databricks DBFS",
        Developer,
        "dns",
        "The Databricks file system",
        Dal("dbfs"),
        DBFS_FIELDS,
    ),
    read_only(source(
        "ipfs",
        "IPFS",
        Developer,
        "hub",
        "Content on IPFS through a gateway (read-only)",
        Dal("ipfs"),
        IPFS_FIELDS,
    )),
    // ---- Databases & key-value ----
    with_caps(
        read_only(source(
            "postgres",
            "PostgreSQL",
            Databases,
            "table_chart",
            "Its tables as folders, every row a file",
            Db(DatabaseEngine::Postgres),
            POSTGRES_FIELDS,
        )),
        ROWS_CAPPED,
    ),
    with_caps(
        read_only(source(
            "mysql",
            "MySQL / MariaDB",
            Databases,
            "table_chart",
            "Its tables as folders, every row a file",
            Db(DatabaseEngine::Mysql),
            MYSQL_FIELDS,
        )),
        ROWS_CAPPED,
    ),
    with_caps(
        read_only(source(
            "sqlite",
            "SQLite file",
            Databases,
            "table_chart",
            "Its tables as folders, every row a file",
            Db(DatabaseEngine::Sqlite),
            SQLITE_FIELDS,
        )),
        ROWS_CAPPED,
    ),
    source(
        "redis",
        "Redis",
        Databases,
        "storage",
        "Its keys as files",
        Dal("redis"),
        REDIS_FIELDS,
    ),
];

/// Every source, grouped in the dialog's order.
#[must_use]
pub fn services() -> &'static [ServiceSpec] {
    SERVICES
}

/// The source `id`.
#[must_use]
pub fn service(id: &str) -> Option<&'static ServiceSpec> {
    SERVICES.iter().find(|s| s.id == id)
}

/// The sources of `group`, in the dialog's order.
pub fn services_in(group: ServiceGroup) -> impl Iterator<Item = &'static ServiceSpec> {
    SERVICES.iter().filter(move |s| s.group == group)
}

/// The source a drive is (`None` for an Azlin drive or an access link: neither has a form).
#[must_use]
pub fn service_of(entry: &DriveEntry) -> Option<&'static ServiceSpec> {
    match &entry.location {
        DriveLocation::Local { .. } => service("local"),
        DriveLocation::S3 {
            auth: DriveAuth::Keyring,
            ..
        } => service("s3"),
        DriveLocation::S3 { .. } => None,
        DriveLocation::Opendal { scheme, .. } => SERVICES
            .iter()
            .find(|s| matches!(s.backend, Backend::Opendal(x) if x == scheme)),
        DriveLocation::Database { engine, .. } => SERVICES
            .iter()
            .find(|s| matches!(s.backend, Backend::Database(e) if e == *engine)),
    }
}

/// What the drive `entry` can do ([`Capabilities`]): a bucket (S3-compatible or an Azlin drive)
/// and a folder on this computer write conditionally; an OpenDAL source or a database has what
/// its catalog entry says, and one the catalog does not list nothing. An app asks this BEFORE it
/// offers a command that needs it (AZ-CN4).
#[must_use]
pub fn capabilities_of(entry: &DriveEntry) -> Capabilities {
    match &entry.location {
        DriveLocation::Local { .. } => Capabilities::CONDITIONAL,
        DriveLocation::S3 {
            auth: DriveAuth::Keyring | DriveAuth::Azlin { .. },
            ..
        } => Capabilities::CONDITIONAL,
        // An access link opens as a scoped drive once the server exists: nothing is known of it.
        DriveLocation::S3 {
            auth: DriveAuth::AccessLink { .. },
            ..
        } => Capabilities::NONE,
        DriveLocation::Opendal { .. } | DriveLocation::Database { .. } => {
            service_of(entry).map_or(Capabilities::NONE, |spec| spec.caps)
        }
    }
}

/// What kind of drive a drives-file entry is ([`kind_of`]); its `Display` is the English
/// [`kind_label`] says, an app says it in its own language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveKind {
    /// A folder on this computer.
    LocalDisk,
    /// An Azlin drive.
    AzlinCloud,
    /// An S3 bucket of the user's own keys.
    S3Bucket,
    /// A source of the catalog.
    Source(&'static ServiceSpec),
    /// An OpenDAL scheme the catalog does not list.
    Scheme(String),
    /// A database browsed as tables.
    Database(DatabaseEngine),
}

impl fmt::Display for DriveKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveKind::LocalDisk => f.write_str("Local Disk"),
            DriveKind::AzlinCloud => f.write_str("Azlin cloud drive"),
            DriveKind::S3Bucket => f.write_str("S3 bucket"),
            DriveKind::Source(spec) => f.write_str(spec.name),
            DriveKind::Scheme(scheme) => f.write_str(scheme),
            DriveKind::Database(engine) => write!(f, "{} database", engine.name()),
        }
    }
}

/// What kind of drive `entry` is.
#[must_use]
pub fn kind_of(entry: &DriveEntry) -> DriveKind {
    match &entry.location {
        DriveLocation::Local { .. } => DriveKind::LocalDisk,
        DriveLocation::S3 {
            auth: DriveAuth::Azlin { .. },
            ..
        } => DriveKind::AzlinCloud,
        DriveLocation::S3 { .. } => DriveKind::S3Bucket,
        DriveLocation::Opendal { scheme, .. } => {
            service_of(entry).map_or_else(|| DriveKind::Scheme(scheme.clone()), DriveKind::Source)
        }
        DriveLocation::Database { engine, .. } => DriveKind::Database(*engine),
    }
}

/// What kind of drive it is, as Explorer's Type column says it: "Local Disk", "S3 bucket",
/// "Azlin cloud drive", the source's name ("WebDAV"), "SQLite database".
#[must_use]
pub fn kind_label(entry: &DriveEntry) -> String {
    kind_of(entry).to_string()
}

// ==== Checking a form ====

/// The value of `key`, trimmed (secrets as they are, but a blank one is none).
fn value<'a>(values: &'a FormValues, f: &FieldSpec) -> Option<&'a str> {
    let raw = values.get(f.key)?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(if f.kind == FieldKind::Secret {
        raw.as_str()
    } else {
        raw.trim()
    })
}

/// What a filled form lacks first ([`problem`]); its `Display` is the English sentence
/// [`check`] says, an app says it in its own language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormProblem {
    /// The drive has no name.
    NoName,
    /// A required field is empty.
    Required(&'static FieldSpec),
    /// An address without its scheme ([`FormProblem::scheme`]: the one its example has).
    NotAnAddress(&'static FieldSpec),
    NotANumber(&'static FieldSpec),
    /// A check box's value that is neither `true` nor `false`.
    NotOnOff(&'static FieldSpec),
    /// A drop-down's value that is none of its words.
    NotOneOf(&'static FieldSpec),
    /// An address whose scheme is none of the field's ([`FieldSpec::schemes`]): `ssh://` for a
    /// WebDAV server, `javascript:` for anything.
    WrongScheme(&'static FieldSpec),
    /// An address with a user name or a password in it (`https://ann:pw@host/`): drives.json would
    /// keep it in plain text and every window would show it (AZ-CN8). The form has fields for them.
    AddressHasLogin(&'static FieldSpec),
    /// An address that sends the form's password or token over the internet unencrypted
    /// (`http://`, `ftp://`, `tcp://` to a host that is not on this computer or its own network).
    SendsInClear(&'static FieldSpec),
    /// A service whose credentials are optional (it would use whatever this computer has: the
    /// developer's gcloud login, an environment variable) and none is filled in (AZ-CN9).
    NoCredential(&'static FieldSpec),
    /// A credential that is not what the service hands out (a Google key that is neither the
    /// service account's JSON nor its base64): ignored by the service, which then uses ambient ones.
    NotAKey(&'static FieldSpec),
}

impl FormProblem {
    /// The scheme the field's example address has (`https` without one).
    #[must_use]
    pub fn scheme(self) -> &'static str {
        match self {
            FormProblem::NoName => "https",
            FormProblem::Required(f)
            | FormProblem::NotAnAddress(f)
            | FormProblem::NotANumber(f)
            | FormProblem::NotOnOff(f)
            | FormProblem::NotOneOf(f)
            | FormProblem::WrongScheme(f)
            | FormProblem::AddressHasLogin(f)
            | FormProblem::SendsInClear(f)
            | FormProblem::NoCredential(f)
            | FormProblem::NotAKey(f) => f
                .placeholder
                .split_once("://")
                .map_or("https", |(scheme, _)| scheme),
        }
    }
}

impl fmt::Display for FormProblem {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormProblem::NoName => out.write_str("Give the drive a name."),
            FormProblem::Required(f) => write!(out, "\"{}\" is required.", f.label),
            FormProblem::NotAnAddress(f) => write!(
                out,
                "\"{}\" must be an address with its scheme, such as {}://...",
                f.label,
                self.scheme()
            ),
            FormProblem::NotANumber(f) => write!(out, "\"{}\" must be a number.", f.label),
            FormProblem::NotOnOff(f) => write!(out, "\"{}\" must be on or off.", f.label),
            FormProblem::NotOneOf(f) => {
                let words = match f.kind {
                    FieldKind::Choice(words) => words.join(", "),
                    _ => String::new(),
                };
                write!(out, "\"{}\" must be one of: {words}.", f.label)
            }
            FormProblem::WrongScheme(f) => write!(
                out,
                "\"{}\" must start with {}.",
                f.label,
                f.schemes
                    .iter()
                    .map(|scheme| format!("{scheme}://"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
            FormProblem::AddressHasLogin(f) => write!(
                out,
                "\"{}\" must not hold a user name or a password: type them in their own fields.",
                f.label
            ),
            FormProblem::SendsInClear(f) => write!(
                out,
                "\"{}\" would send the password over the internet unencrypted: use the secure \
                 address (https://, ftps://, rediss://). A server on your own network may use the \
                 plain one.",
                f.label
            ),
            FormProblem::NoCredential(f) => write!(
                out,
                "\"{}\" or another credential is needed: without one the service would use \
                 whatever credentials this computer has.",
                f.label
            ),
            FormProblem::NotAKey(f) => write!(
                out,
                "\"{}\" is not the key the service hands out (its JSON text, or that text in \
                 base64).",
                f.label
            ),
        }
    }
}

/// An address as the form reads it: its scheme, whether it holds a user name or password, and its
/// host (without a port and without brackets).
struct Address<'a> {
    scheme: &'a str,
    has_login: bool,
    host: &'a str,
}

/// `text` as an address: `scheme://authority...` with a scheme of letters, digits, `+`, `-`, `.`
/// and an authority that is not empty.
fn address(text: &str) -> Option<Address<'_>> {
    let (scheme, rest) = text.split_once("://")?;
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() {
        return None;
    }
    // The user info ends at the LAST '@' (a password may hold one).
    let (has_login, host_port) = match authority.rsplit_once('@') {
        Some((_, host_port)) => (true, host_port),
        None => (false, authority),
    };
    let host = match host_port.strip_prefix('[') {
        Some(inner) => inner.split(']').next().unwrap_or(inner),
        None => host_port.split(':').next().unwrap_or(host_port),
    };
    Some(Address {
        scheme,
        has_login,
        host,
    })
}

/// Whether the address `text` holds a user name or a password (`https://ann:pw@host/`).
#[must_use]
pub fn address_has_login(text: &str) -> bool {
    address(text.trim()).is_some_and(|address| address.has_login)
}

/// Whether `host` is this computer or lies on its own network: a loopback, private or link-local
/// address, `localhost`, a name with one label (`nas`) or one of the names a home or an office
/// network gives itself (`.local`, `.lan`, `.home.arpa`, `.internal`). A password sent in clear to
/// such a host does not cross the internet.
#[must_use]
pub fn is_own_network(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => {
                let first = v6.segments()[0];
                v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
            }
        };
    }
    host == "localhost"
        || !host.contains('.')
        || [
            ".localhost",
            ".local",
            ".lan",
            ".home.arpa",
            ".internal",
            ".localdomain",
        ]
        .iter()
        .any(|end| host.ends_with(end))
}

/// Whether a connection to `address` carries what is sent over it in clear text to a host that is
/// not on this computer or its own network.
fn sends_in_clear(address: &Address<'_>) -> bool {
    let secure = ["https", "ftps", "rediss", "wss"]
        .iter()
        .any(|scheme| scheme.eq_ignore_ascii_case(address.scheme));
    !secure && !is_own_network(address.host)
}

/// The text of a Google service account key as OpenDAL takes it - base64 of the key's JSON - from
/// what the user pasted: the JSON itself (as Google's console hands it out) or its base64. `None`
/// for anything else. A key pasted as JSON used to be ignored by the service, which then used the
/// machine's own login (AZ-CN9).
#[must_use]
pub fn service_account_key(text: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let text = text.trim();
    let (json, based) = if text.starts_with('{') {
        (text.as_bytes().to_vec(), STANDARD.encode(text))
    } else {
        (STANDARD.decode(text).ok()?, text.to_string())
    };
    let key: serde_json::Value = serde_json::from_slice(&json).ok()?;
    let holds = |name: &str| {
        key.get(name)
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty())
    };
    (holds("client_email") && holds("private_key")).then_some(based)
}

/// Whether the field `f` has an answer in `values`: a value, or a switch that is on.
fn is_set(values: &FormValues, f: &FieldSpec) -> bool {
    if f.kind == FieldKind::Bool {
        values.get(f.key).is_some_and(|v| v.trim() == "true")
    } else {
        value(values, f).is_some()
    }
}

/// Checks one filled field.
fn check_field(f: &'static FieldSpec, v: &str) -> Result<(), FormProblem> {
    match f.kind {
        FieldKind::Url => {
            let Some(address) = address(v).filter(|_| !v.chars().any(char::is_whitespace)) else {
                return Err(FormProblem::NotAnAddress(f));
            };
            if !f
                .schemes
                .iter()
                .any(|scheme| scheme.eq_ignore_ascii_case(address.scheme))
            {
                return Err(FormProblem::WrongScheme(f));
            }
            if address.has_login {
                return Err(FormProblem::AddressHasLogin(f));
            }
            Ok(())
        }
        FieldKind::Number => {
            if v.chars().all(|c| c.is_ascii_digit()) {
                Ok(())
            } else {
                Err(FormProblem::NotANumber(f))
            }
        }
        FieldKind::Bool => {
            if v == "true" || v == "false" {
                Ok(())
            } else {
                Err(FormProblem::NotOnOff(f))
            }
        }
        FieldKind::Choice(words) => {
            if words.contains(&v) {
                Ok(())
            } else {
                Err(FormProblem::NotOneOf(f))
            }
        }
        FieldKind::Text | FieldKind::Secret | FieldKind::Path => Ok(()),
    }
}

/// Checks a filled form: the drive's name first, then every field in the form's order (a
/// required one empty, an address without its scheme, a port that is no number). `Err` says
/// what to fix, as a sentence ([`problem`]'s English).
pub fn check(spec: &ServiceSpec, name: &str, values: &FormValues) -> Result<(), String> {
    problem(spec, name, values).map_or(Ok(()), |p| Err(p.to_string()))
}

/// What a filled form lacks first, as [`check`] reads it; `None`: nothing.
#[must_use]
pub fn problem(spec: &ServiceSpec, name: &str, values: &FormValues) -> Option<FormProblem> {
    if name.trim().is_empty() {
        return Some(FormProblem::NoName);
    }
    for f in spec.fields {
        match value(values, f) {
            None if f.required => return Some(FormProblem::Required(f)),
            None => {}
            Some(v) => {
                if let Err(problem) = check_field(f, v) {
                    return Some(problem);
                }
                // Google's key: the JSON text or its base64, nothing else (AZ-CN9).
                if spec.id == "gcs" && f.key == "credential" && service_account_key(v).is_none() {
                    return Some(FormProblem::NotAKey(f));
                }
            }
        }
    }
    // A service that would otherwise use whatever credentials this computer has needs one filled
    // in (AZ-CN9).
    if let Some(first) = spec.needs_one_of.first().and_then(|key| spec.field(key)) {
        let filled = spec
            .needs_one_of
            .iter()
            .filter_map(|key| spec.field(key))
            .any(|f| is_set(values, f));
        if !filled {
            return Some(FormProblem::NoCredential(first));
        }
    }
    // A password or a token never goes over the internet in clear text (AZ-CN8).
    let carries_secret = spec
        .fields
        .iter()
        .any(|f| f.kind == FieldKind::Secret && value(values, f).is_some());
    if carries_secret {
        for f in spec.fields.iter().filter(|f| f.kind == FieldKind::Url) {
            let clear = value(values, f)
                .and_then(address)
                .is_some_and(|address| sends_in_clear(&address));
            if clear {
                return Some(FormProblem::SendsInClear(f));
            }
        }
    }
    None
}

// ==== What a form becomes ====

/// A new drive: its drives-file entry and the text of its keyring entry (`None`: it keeps no
/// secret). `Debug` never shows the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct NewDrive {
    pub entry: DriveEntry,
    pub secret: Option<String>,
}

impl fmt::Debug for NewDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewDrive")
            .field("entry", &self.entry)
            .field("secret", &self.secret.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}

/// The region an empty S3 "Region" field means.
pub const DEFAULT_REGION: &str = "us-east-1";

/// A transport that sends nothing: `S3Drive::new` checks the endpoint and the bucket name
/// without one.
struct NoTransport;

impl Transport for NoTransport {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        Err(String::from("not sent"))
    }
}

/// The drive a checked form describes, named `name`, with the id `id`
/// ([`crate::config::new_drive_id`] for a new one).
pub fn build_entry(
    spec: &ServiceSpec,
    id: &str,
    name: &str,
    values: &FormValues,
) -> Result<NewDrive, String> {
    check(spec, name, values)?;
    let name = name.trim().to_string();
    let get = |key: &str| {
        spec.field(key)
            .and_then(|f| value(values, f))
            .unwrap_or_default()
            .to_string()
    };
    let (location, secret) = match spec.backend {
        Backend::S3 => {
            let region = match get("region") {
                r if r.is_empty() => DEFAULT_REGION.to_string(),
                r => r,
            };
            let config = S3Config {
                endpoint: get("endpoint"),
                region,
                bucket: get("bucket"),
                path_style: get("path_style") != "false",
            };
            let credentials = Credentials::new(
                get("access_key_id").as_str(),
                get("secret_access_key").as_str(),
            );
            // The same checks the drive makes when it opens: a readable endpoint and a bucket
            // name that fits in the URL.
            S3Drive::new(config.clone(), credentials.clone(), Box::new(NoTransport))
                .map_err(|e| format!("{e}."))?;
            (
                DriveLocation::S3 {
                    endpoint: config.endpoint,
                    region: config.region,
                    bucket: config.bucket,
                    path_style: config.path_style,
                    auth: DriveAuth::Keyring,
                },
                Some(credentials.to_keyring_secret()),
            )
        }
        Backend::Local => (DriveLocation::Local { root: get("root") }, None),
        Backend::Opendal(scheme) => {
            let (options, secrets) = split(spec, values);
            let keyring = !secrets.is_empty();
            (
                DriveLocation::Opendal {
                    scheme: scheme.to_string(),
                    options,
                    keyring,
                },
                keyring.then(|| secrets.to_keyring_secret()),
            )
        }
        Backend::Database(engine) => {
            let (options, secrets) = split(spec, values);
            let keyring = !secrets.is_empty();
            (
                DriveLocation::Database {
                    engine,
                    options,
                    keyring,
                },
                keyring.then(|| secrets.to_keyring_secret()),
            )
        }
    };
    Ok(NewDrive {
        entry: DriveEntry {
            id: id.to_string(),
            name,
            location,
        },
        secret,
    })
}

/// A form's filled fields: the plain ones (for the drives file) and the secret ones (for the
/// keyring). Keys the form does not have are dropped.
fn split(spec: &ServiceSpec, values: &FormValues) -> (BTreeMap<String, String>, SecretOptions) {
    let mut options = BTreeMap::new();
    let mut secrets = SecretOptions::new();
    for f in spec.fields {
        let Some(v) = value(values, f) else {
            continue;
        };
        if f.kind == FieldKind::Secret {
            // A Google key pasted as JSON is kept as the base64 OpenDAL takes: pasted raw it was
            // ignored and the service used the machine's own login (AZ-CN9).
            let key = (spec.id == "gcs" && f.key == "credential")
                .then(|| service_account_key(v))
                .flatten();
            secrets.insert(f.key, key.as_deref().unwrap_or(v));
        } else {
            options.insert(f.key.to_string(), v.to_string());
        }
    }
    (options, secrets)
}

/// The source of a drive and the form it fills again, WITHOUT its secrets (they stay in the
/// keyring; the form asks for them anew).
#[must_use]
pub fn form_values(entry: &DriveEntry) -> (Option<&'static ServiceSpec>, FormValues) {
    let spec = service_of(entry);
    let mut values = FormValues::new();
    match &entry.location {
        DriveLocation::Local { root } => {
            values.insert(String::from("root"), root.clone());
        }
        DriveLocation::S3 {
            endpoint,
            region,
            bucket,
            path_style,
            ..
        } => {
            values.insert(String::from("endpoint"), endpoint.clone());
            values.insert(String::from("region"), region.clone());
            values.insert(String::from("bucket"), bucket.clone());
            values.insert(String::from("path_style"), path_style.to_string());
        }
        DriveLocation::Opendal { options, .. } | DriveLocation::Database { options, .. } => {
            values.extend(options.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }
    (spec, values)
}
