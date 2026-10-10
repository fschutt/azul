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
    /// One of these words (a drop-down), the first the default unless the field says another.
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
    }
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
        "https://s3.eu-central-1.amazonaws.com",
    ),
    field("region", "Region", FieldKind::Text, false, "us-east-1 (R2: auto)"),
    field("bucket", "Bucket", FieldKind::Text, true, "my-bucket"),
    field("access_key_id", "Access key", FieldKind::Text, true, ""),
    with_help(
        field("secret_access_key", "Secret key", FieldKind::Secret, true, ""),
        "Kept in the system keyring only.",
    ),
    with_default(
        field(
            "path_style",
            "Path-style URLs (MinIO, local servers)",
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
    field("username", "User name", FieldKind::Text, false, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    with_default(field("db", "Database number", FieldKind::Number, false, "0"), "0"),
    field("root", "Key prefix", FieldKind::Text, false, ""),
];

const POSTGRES_FIELDS: &[FieldSpec] = &[
    with_default(field("host", "Host", FieldKind::Text, true, "localhost"), "localhost"),
    with_default(field("port", "Port", FieldKind::Number, false, "5432"), "5432"),
    field("database", "Database", FieldKind::Text, true, ""),
    field("user", "User", FieldKind::Text, true, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    field(
        "sslmode",
        "TLS",
        FieldKind::Choice(&["prefer", "disable", "require", "verify-full"]),
        false,
        "",
    ),
];

const MYSQL_FIELDS: &[FieldSpec] = &[
    with_default(field("host", "Host", FieldKind::Text, true, "localhost"), "localhost"),
    with_default(field("port", "Port", FieldKind::Number, false, "3306"), "3306"),
    field("database", "Database", FieldKind::Text, true, ""),
    field("user", "User", FieldKind::Text, true, ""),
    field("password", "Password", FieldKind::Secret, false, ""),
    field(
        "sslmode",
        "TLS",
        FieldKind::Choice(&["preferred", "disabled", "required", "verify-identity"]),
        false,
        "",
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
    }
}

const fn read_only(mut s: ServiceSpec) -> ServiceSpec {
    s.read_only = true;
    s
}

use Backend::{Database as Db, Opendal as Dal};
use ServiceGroup::{CloudStorage, ConsumerCloud, Databases, Developer, NetworkNas};

static SERVICES: &[ServiceSpec] = &[
    // ---- Cloud object storage ----
    source(
        "s3",
        "S3-compatible storage",
        CloudStorage,
        "cloud",
        "AWS S3, MinIO, Cloudflare R2, Backblaze B2, Wasabi, Garage",
        Backend::S3,
        S3_FIELDS,
    ),
    source(
        "gcs",
        "Google Cloud Storage",
        CloudStorage,
        "cloud",
        "A bucket with a service account's key",
        Dal("gcs"),
        GCS_FIELDS,
    ),
    source(
        "azblob",
        "Azure Blob Storage",
        CloudStorage,
        "cloud",
        "A container of a storage account",
        Dal("azblob"),
        AZBLOB_FIELDS,
    ),
    source(
        "azdls",
        "Azure Data Lake Storage Gen2",
        CloudStorage,
        "cloud",
        "A file system of a storage account",
        Dal("azdls"),
        AZDLS_FIELDS,
    ),
    source(
        "azfile",
        "Azure Files",
        CloudStorage,
        "folder_shared",
        "A file share of a storage account",
        Dal("azfile"),
        AZFILE_FIELDS,
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
    source(
        "oss",
        "Alibaba Cloud OSS",
        CloudStorage,
        "cloud",
        "Object Storage Service",
        Dal("oss"),
        OSS_FIELDS,
    ),
    source(
        "cos",
        "Tencent Cloud COS",
        CloudStorage,
        "cloud",
        "Cloud Object Storage",
        Dal("cos"),
        COS_FIELDS,
    ),
    source(
        "obs",
        "Huawei Cloud OBS",
        CloudStorage,
        "cloud",
        "Object Storage Service",
        Dal("obs"),
        OBS_FIELDS,
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
    source(
        "local",
        "Folder on this computer",
        NetworkNas,
        "folder",
        "A folder, a mounted NAS share, an external disk",
        Backend::Local,
        LOCAL_FIELDS,
    ),
    source(
        "webdav",
        "WebDAV",
        NetworkNas,
        "dns",
        "Nextcloud, ownCloud, Synology, QNAP, any WebDAV server",
        Dal("webdav"),
        WEBDAV_FIELDS,
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
    read_only(source(
        "postgres",
        "PostgreSQL",
        Databases,
        "table_chart",
        "Its tables as folders, every row a file",
        Db(DatabaseEngine::Postgres),
        POSTGRES_FIELDS,
    )),
    read_only(source(
        "mysql",
        "MySQL / MariaDB",
        Databases,
        "table_chart",
        "Its tables as folders, every row a file",
        Db(DatabaseEngine::Mysql),
        MYSQL_FIELDS,
    )),
    read_only(source(
        "sqlite",
        "SQLite file",
        Databases,
        "table_chart",
        "Its tables as folders, every row a file",
        Db(DatabaseEngine::Sqlite),
        SQLITE_FIELDS,
    )),
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
            | FormProblem::NotOneOf(f) => f
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
        }
    }
}

/// Checks one filled field.
fn check_field(f: &'static FieldSpec, v: &str) -> Result<(), FormProblem> {
    match f.kind {
        FieldKind::Url => {
            let ok = v
                .split_once("://")
                .is_some_and(|(scheme, rest)| {
                    !scheme.is_empty()
                        && scheme
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                        && !rest.is_empty()
                })
                && !v.chars().any(char::is_whitespace);
            if ok {
                Ok(())
            } else {
                Err(FormProblem::NotAnAddress(f))
            }
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
            secrets.insert(f.key, v);
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
