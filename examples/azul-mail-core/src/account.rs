//! A mail account: what AzMail needs to reach the servers, never the secret.
//!
//! The account's settings are one JSON file, `<AzMail folder>/<account id>/account.json`, where
//! the AzMail folder is `AZMAIL_DATA`, else `mail` in the Azlin data root. The file holds
//! the address, the servers, the kind of sign-in and the local mail folder; the password or token
//! lives only in the OS keyring (under [`keyring_key`]), and nothing in this module can print it:
//! [`Secret`] has no `Display`, and its `Debug` shows no characters of it.
//!
//! The format, version 1:
//!
//! ```json
//! {
//!   "format": "azmail.account",
//!   "version": 1,
//!   "email": "ada@example.org",
//!   "username": "ada@example.org",
//!   "imap": { "host": "imap.example.org", "port": 993 },
//!   "smtp": { "host": "smtp.example.org", "port": 465 },
//!   "security": "tls",
//!   "auth": "password"
//! }
//! ```
//!
//! plus `"folder": "<path>"` when the mail is synced somewhere else than the account's folder.
//!
//! An Azlin account - the mailbox is files in the user's Azlin drive (`AZLIN_MAIL.md`) - is
//! version 2 with its kind and drive instead of the IMAP server (an older AzMail refuses it
//! rather than syncing it as an IMAP account without a server):
//!
//! ```json
//! {
//!   "format": "azmail.account",
//!   "version": 2,
//!   "kind": "azlin",
//!   "email": "ada@example.org",
//!   "smtp": { "host": "smtp.example.org", "port": 465 },
//!   "azlin": { "token_url": "https://token.example", "drive_id": "d_..." }
//! }
//! ```
//!
//! Its secret is the drive token with the current S3 credentials (`azlin::AzlinSession`), in the
//! keyring under [`azlin_keyring_key`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::{DriveFolder, MailStore};

/// The `format` of an account file.
pub const FORMAT: &str = "azmail.account";
/// The newest version this AzMail reads: an Azlin account's.
pub const VERSION: u64 = 2;
/// The version an IMAP account's file is written as (every AzMail reads it).
pub const IMAP_VERSION: u64 = 1;
/// The `kind` of an Azlin account's file (an IMAP account's file names none).
pub const KIND_AZLIN: &str = "azlin";
/// The account file in an account's folder.
pub const ACCOUNT_FILE: &str = "account.json";
/// The folder in the user's data folder when `AZMAIL_DATA` is not set.
pub const APP_DIR: &str = "AzMail";
/// The AzMail folder's variable.
pub const DATA_VAR: &str = "AZMAIL_DATA";
/// The mailbox of the mail written without an account - Local Folders: its drafts, the mail it
/// sent from this computer (`send::SendRoute::Direct`, straight to the recipients' mail
/// servers) and its Outbox, in `<AzMail folder>/local/`, laid out as an account's folder
/// (`mail/drafts`, `mail/sent`, `outbox/`, an optional `sending.json`). No account has this id:
/// an account's id is its address, which has an `@` ([`account_id`]).
pub const LOCAL_ID: &str = "local";
/// Headless test runs only (`AZ_BACKEND=headless`): the password to sign in with, so a test
/// never types one and never touches a keyring.
pub const TEST_PASSWORD_VAR: &str = "AZMAIL_TEST_PASSWORD";
/// Headless test runs only: a PEM certificate to trust besides the Mozilla roots (the test
/// server's self-signed one).
pub const TEST_CA_VAR: &str = "AZMAIL_TEST_CA";

/// How the connection to the IMAP server is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// TLS from the first byte ("implicit TLS", IMAPS, port 993).
    Tls,
    /// No encryption at all. Only for a test server on this computer: see
    /// [`AccountForm::to_account`].
    Plain,
}

/// What the secret is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    /// A password or an app password: `AUTHENTICATE PLAIN`, else `LOGIN`.
    Password,
    /// An OAuth 2 access token: `AUTHENTICATE XOAUTH2`.
    Xoauth2,
}

/// A server's host and port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

/// One account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The account's folder name and keyring name: the address in lower case with every
    /// character a file name or an object key could trip on replaced ([`account_id`]).
    pub id: String,
    pub email: String,
    /// The sender's name for the From line ("Ada Lovelace"); empty for the address alone.
    pub name: String,
    /// The IMAP login name; most providers want the address.
    pub username: String,
    pub imap: Server,
    /// For sending, later; kept so the account is complete.
    pub smtp: Server,
    pub security: Security,
    pub auth: AuthKind,
    /// Where the mail is synced to; `None` is the account's own folder.
    pub folder: Option<PathBuf>,
    /// An Azlin account's drive (then `imap`, `username`, `security` and `auth` mean nothing);
    /// `None` for an IMAP account.
    pub azlin: Option<AzlinLink>,
}

/// Where an Azlin account's mailbox is: the token server that hands out the drive's
/// credentials, and the drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AzlinLink {
    /// The token server the account was created with (`https://token.example`; empty: the one
    /// this run was told, `azlin::Endpoints`).
    #[serde(default)]
    pub token_url: String,
    pub drive_id: String,
}

/// A mail provider AzMail knows the servers of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    pub name: &'static str,
    /// The address domains the provider serves (lower case).
    pub domains: &'static [&'static str],
    pub imap: (&'static str, u16),
    pub smtp: (&'static str, u16),
    /// What the setup form says about signing in.
    pub note: &'static str,
}

/// The providers whose servers the setup form fills in from the address.
pub const PROVIDERS: &[Provider] = &[
    Provider {
        name: "Gmail",
        domains: &["gmail.com", "googlemail.com"],
        imap: ("imap.gmail.com", 993),
        smtp: ("smtp.gmail.com", 465),
        note: "Gmail needs an app password, not your Google password: turn on 2-Step \
               Verification, then create one at myaccount.google.com/apppasswords.",
    },
    Provider {
        name: "Outlook",
        domains: &[
            "outlook.com",
            "hotmail.com",
            "live.com",
            "msn.com",
            "outlook.de",
            "hotmail.de",
            "live.de",
            "hotmail.co.uk",
            "live.co.uk",
            "outlook.fr",
            "hotmail.fr",
        ],
        imap: ("outlook.office365.com", 993),
        smtp: ("smtp.office365.com", 587),
        note: "Outlook.com and Microsoft 365 no longer accept passwords over IMAP for most \
               accounts: tick \"OAuth access token\" and paste an access token.",
    },
    Provider {
        name: "iCloud",
        domains: &["icloud.com", "me.com", "mac.com"],
        imap: ("imap.mail.me.com", 993),
        smtp: ("smtp.mail.me.com", 587),
        note: "iCloud needs an app-specific password: create one at account.apple.com under \
               Sign-In and Security, App-Specific Passwords.",
    },
    Provider {
        name: "Fastmail",
        domains: &["fastmail.com", "fastmail.fm"],
        imap: ("imap.fastmail.com", 993),
        smtp: ("smtp.fastmail.com", 465),
        note: "Fastmail needs an app password: Settings, Privacy & Security, Manage app \
               passwords.",
    },
];

/// What the setup form says for every address.
pub const APP_PASSWORD_NOTE: &str =
    "Gmail and iCloud need an app password, not your normal password.";

/// The IMAP port with TLS.
pub const IMAPS_PORT: u16 = 993;
/// The SMTP submission port with TLS.
pub const SMTPS_PORT: u16 = 465;

/// A password or token. It is never written to a file or a log: there is no `Display`, `Debug`
/// shows no character of it, and its bytes are overwritten when it is dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(secret: String) -> Secret {
        Secret(secret)
    }

    /// The secret itself, for the sign-in and the keyring only.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // Best effort: overwrite the bytes before the allocation is freed.
        let mut bytes = std::mem::take(&mut self.0).into_bytes();
        bytes.fill(0);
        std::hint::black_box(&bytes);
    }
}

/// The domain of an address, in lower case, if the address has the shape `local@domain`.
pub use azul_pim::mail_address::email_domain;

/// Whether `email` looks like an account's address: one `@`, something on both sides, a
/// dot-free (`x@localhost`, a test server) or dotted domain without blanks - the PIM apps'
/// shared rule, `azul_pim::mail_address::is_email_any_host`.
pub fn is_email(email: &str) -> bool {
    azul_pim::mail_address::is_email_any_host(email)
}

/// The provider serving the address's domain, if AzMail knows it.
pub fn provider_for(email: &str) -> Option<&'static Provider> {
    let domain = email_domain(email)?;
    PROVIDERS
        .iter()
        .find(|p| p.domains.contains(&domain.as_str()))
}

/// The IMAP and SMTP servers to prefill for an address: the provider's, else `imap.<domain>`
/// on 993 and `smtp.<domain>` on 465; `None` while the address has no domain yet.
pub fn guess_servers(email: &str) -> Option<(Server, Server)> {
    let server = |host: &str, port: u16| Server {
        host: host.to_string(),
        port,
    };
    if let Some(p) = provider_for(email) {
        return Some((server(p.imap.0, p.imap.1), server(p.smtp.0, p.smtp.1)));
    }
    let domain = email_domain(email)?;
    Some((
        server(&format!("imap.{domain}"), IMAPS_PORT),
        server(&format!("smtp.{domain}"), SMTPS_PORT),
    ))
}

/// The account id for an address: the address in lower case, trimmed, where every character
/// other than `a-z 0-9 . _ + - @` becomes `_` and a leading dot becomes `_`; `None` when the
/// address is not one.
pub fn account_id(email: &str) -> Option<String> {
    if !is_email(email) {
        return None;
    }
    let mut id: String = email
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '.' | '_' | '+' | '-' | '@' => c,
            _ => '_',
        })
        .collect();
    if id.starts_with('.') {
        id.replace_range(0..1, "_");
    }
    Some(id)
}

/// The name the secret is stored under in the OS keyring (every azul app shares one keyring
/// service, so the name says which app and which account).
pub fn keyring_key(id: &str) -> String {
    format!("{APP_DIR}/{id}/imap")
}

/// The name an Azlin account's secret (the drive token and the S3 credentials) is stored under.
pub fn azlin_keyring_key(id: &str) -> String {
    format!("{APP_DIR}/{id}/azlin")
}

/// The keyring name of `account`'s secret: an IMAP account's password or token, an Azlin
/// account's session.
pub fn secret_keyring_key(account: &Account) -> String {
    if account.is_azlin() {
        azlin_keyring_key(&account.id)
    } else {
        keyring_key(&account.id)
    }
}

/// Whether `host` is this computer: `localhost`, `127.x.x.x` or `::1`.
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// The secret for a headless test run: `value` (`AZMAIL_TEST_PASSWORD`) when the backend
/// (`AZ_BACKEND`) is `headless`, never otherwise.
pub fn test_secret(backend: Option<&str>, value: Option<&str>) -> Option<Secret> {
    match (backend, value) {
        (Some("headless"), Some(v)) if !v.is_empty() => Some(Secret::new(v.to_string())),
        _ => None,
    }
}

/// The AzMail folder: `setting` (`AZMAIL_DATA`, for scripts and tests), else AzMail's folder
/// in the Azlin data root (`<data root>/mail`, the user's bucket later; the root is
/// azul-appkit's: `--data-dir`, `AZLIN_DATA`, else `Azlin` in the user's data folder).
pub fn data_root(setting: Option<&str>, azlin_root: &Path) -> PathBuf {
    match setting.map(str::trim).filter(|s| !s.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => azlin_root.join(crate::args::APP_FOLDER),
    }
}

/// Where AzMail kept its folder before the Azlin data root: `AzMail` in the user's data folder.
pub fn legacy_root(user_data: Option<&Path>) -> Option<PathBuf> {
    user_data
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.join(APP_DIR))
}

/// Moves the folder of an older AzMail (`legacy`) to `root` once: only when `legacy` is a
/// folder and nothing is at `root` yet (a second run, or mail already in the new place, moves
/// nothing). Runs at start, before the window. `Ok(true)` when it moved.
pub fn migrate_legacy_root(legacy: &Path, root: &Path) -> std::io::Result<bool> {
    if !legacy.is_dir() || root.exists() {
        return Ok(false);
    }
    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(legacy, root)?;
    Ok(true)
}

/// The account's own folder: `<AzMail folder>/<account id>`.
pub fn account_dir(root: &DriveFolder, id: &str) -> DriveFolder {
    root.child(id)
}

/// Where the account's mail is synced to: its `folder` (placed as the AzMail folder's drive
/// places it: in the data tree a folder of the tree's drive, elsewhere a drive of its own),
/// else its own folder.
pub fn mail_root(root: &DriveFolder, account: &Account) -> DriveFolder {
    match &account.folder {
        Some(folder) => root.locate(folder),
        None => account_dir(root, &account.id),
    }
}

/// The key of an account's file in the AzMail folder: `<account id>/account.json`.
pub fn account_key(id: &str) -> String {
    format!("{id}/{ACCOUNT_FILE}")
}

/// Why an account file cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    NotJson(String),
    /// JSON, but `format` is missing or different.
    NotAnAccount,
    NewerVersion(u64),
    Malformed(String),
    /// The address in the file is not one.
    BadEmail(String),
    /// An account of a kind this AzMail does not know (a newer AzMail's).
    UnknownKind(String),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountError::NotJson(e) => write!(f, "not JSON ({e})"),
            AccountError::NotAnAccount => {
                write!(f, "not an AzMail account (no \"format\": \"{FORMAT}\")")
            }
            AccountError::NewerVersion(v) => write!(
                f,
                "written by a newer AzMail (version {v}; this one reads up to {VERSION})"
            ),
            AccountError::Malformed(e) => write!(f, "malformed ({e})"),
            AccountError::BadEmail(e) => write!(f, "the address {e:?} is not an address"),
            AccountError::UnknownKind(kind) => {
                write!(
                    f,
                    "an account of a kind this AzMail does not know ({kind:?})"
                )
            }
        }
    }
}

/// The account file on disk. The IMAP fields are an IMAP account's only.
#[derive(Serialize, Deserialize)]
struct AccountFile {
    format: String,
    version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    imap: Option<Server>,
    smtp: Server,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    security: Option<Security>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<AuthKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    azlin: Option<AzlinLink>,
}

/// The account file's contents (pretty JSON, ending in a newline): an IMAP account's exactly as
/// every AzMail wrote it (version 1), an Azlin account's as version 2.
pub fn to_json(account: &Account) -> String {
    let folder = account
        .folder
        .as_ref()
        .map(|f| f.to_string_lossy().into_owned());
    let file = match &account.azlin {
        None => AccountFile {
            format: FORMAT.to_string(),
            version: IMAP_VERSION,
            kind: None,
            email: account.email.clone(),
            name: account.name.trim().to_string(),
            username: Some(account.username.clone()),
            imap: Some(account.imap.clone()),
            smtp: account.smtp.clone(),
            security: Some(account.security),
            auth: Some(account.auth),
            folder,
            azlin: None,
        },
        Some(link) => AccountFile {
            format: FORMAT.to_string(),
            version: VERSION,
            kind: Some(KIND_AZLIN.to_string()),
            email: account.email.clone(),
            name: account.name.trim().to_string(),
            username: None,
            imap: None,
            smtp: account.smtp.clone(),
            security: None,
            auth: None,
            folder,
            azlin: Some(AzlinLink {
                token_url: link.token_url.trim().to_string(),
                drive_id: link.drive_id.trim().to_string(),
            }),
        },
    };
    // Strings and numbers only: serializing cannot fail.
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads an account file.
pub fn from_json(text: &str) -> Result<Account, AccountError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| AccountError::NotJson(e.to_string()))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(AccountError::NotAnAccount);
    }
    match value.get("version").and_then(|v| v.as_u64()) {
        Some(v) if v > VERSION => return Err(AccountError::NewerVersion(v)),
        Some(v) if v >= 1 => {}
        _ => return Err(AccountError::Malformed(String::from("no version"))),
    }
    let file: AccountFile =
        serde_json::from_value(value).map_err(|e| AccountError::Malformed(e.to_string()))?;
    let id = account_id(&file.email).ok_or_else(|| AccountError::BadEmail(file.email.clone()))?;
    let folder = file
        .folder
        .filter(|f| !f.trim().is_empty())
        .map(PathBuf::from);
    let missing = |what: &str| AccountError::Malformed(format!("no {what}"));
    match file.kind.as_deref().map(str::trim) {
        None | Some("imap") => Ok(Account {
            id,
            username: file.username.ok_or_else(|| missing("username"))?,
            imap: file.imap.ok_or_else(|| missing("IMAP server"))?,
            security: file.security.ok_or_else(|| missing("security"))?,
            auth: file.auth.ok_or_else(|| missing("auth"))?,
            email: file.email,
            name: file.name.trim().to_string(),
            smtp: file.smtp,
            folder,
            azlin: None,
        }),
        Some(KIND_AZLIN) => {
            let link = file
                .azlin
                .filter(|link| !link.drive_id.trim().is_empty())
                .ok_or_else(|| missing("Azlin drive"))?;
            Ok(Account {
                id,
                username: file.email.clone(),
                imap: Server {
                    host: String::new(),
                    port: 0,
                },
                security: Security::Tls,
                auth: AuthKind::Password,
                email: file.email,
                name: file.name.trim().to_string(),
                smtp: file.smtp,
                folder,
                azlin: Some(AzlinLink {
                    token_url: link.token_url.trim().to_string(),
                    drive_id: link.drive_id.trim().to_string(),
                }),
            })
        }
        Some(other) => Err(AccountError::UnknownKind(other.to_string())),
    }
}

/// Writes the account file through the AzMail folder's drive (whole or not at all) and returns
/// where it is on this computer.
pub fn save(root: &DriveFolder, account: &Account) -> std::io::Result<PathBuf> {
    MailStore::new(root.clone()).put(&account_key(&account.id), to_json(account).as_bytes())?;
    Ok(account_dir(root, &account.id).path().join(ACCOUNT_FILE))
}

/// The account in its file in the AzMail folder: `None` when there is no file, else the account
/// or why it cannot be read.
pub fn load(root: &DriveFolder, id: &str) -> Option<Result<Account, String>> {
    match MailStore::new(root.clone()).get(&account_key(id)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => Some(Err(e.to_string())),
        Ok(bytes) => Some(from_json(&String::from_utf8_lossy(&bytes)).map_err(|e| e.to_string())),
    }
}

/// Every account in the AzMail folder, in order of address, and the account files that could
/// not be read, with why.
pub fn load_all(root: &DriveFolder) -> (Vec<Account>, Vec<(PathBuf, String)>) {
    let mut accounts = Vec::new();
    let mut skipped = Vec::new();
    for id in MailStore::new(root.clone()).subfolders("") {
        match load(root, &id) {
            None => {}
            Some(Ok(account)) => accounts.push(account),
            Some(Err(reason)) => {
                skipped.push((account_dir(root, &id).path().join(ACCOUNT_FILE), reason));
            }
        }
    }
    accounts.sort_by(|a, b| a.id.cmp(&b.id));
    (accounts, skipped)
}

/// The setup form's fields as typed. The secret is not one of them: the form is printed in
/// logs and tests, the secret never is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountForm {
    pub email: String,
    /// "Your name", for the From line.
    pub name: String,
    pub username: String,
    pub imap_host: String,
    pub imap_port: String,
    pub smtp_host: String,
    pub smtp_port: String,
    /// The local mail folder; empty is the account's own folder.
    pub folder: String,
    /// "Unencrypted connection (a test server on this computer)".
    pub plain: bool,
    /// "Sign in with an OAuth access token (XOAUTH2)".
    pub xoauth2: bool,
    /// The account kind: an Azlin drive (`true`) or an IMAP server.
    pub azlin: bool,
    /// An Azlin account's token server as typed; empty: [`AccountForm::token_default`].
    pub token_url: String,
    /// The token server this run was told about (the shared Azlin config, `$AZLIN_TOKEN_URL`,
    /// `--azlin-token-url`): what the empty field stands for. Not typed.
    pub token_default: String,
    /// An Azlin account's drive id.
    pub drive_id: String,
}

impl Account {
    /// The mailbox is files in an Azlin drive (`AZLIN_MAIL.md`), not on an IMAP server.
    pub fn is_azlin(&self) -> bool {
        self.azlin.is_some()
    }

    /// The From line: `Name <address>` (the name quoted when it holds a special character), or
    /// the address alone without a name.
    pub fn sender(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            return self.email.clone();
        }
        // RFC 5322: a display name with a special character is a quoted string.
        if name.chars().any(|c| "()<>[]:;@\\,.\"".contains(c)) {
            let quoted = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{quoted}\" <{}>", self.email)
        } else {
            format!("{name} <{}>", self.email)
        }
    }
}

/// What the form shows in its empty fields: the values an empty field stands for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormDefaults {
    pub username: String,
    pub imap_host: String,
    pub imap_port: String,
    pub smtp_host: String,
    pub smtp_port: String,
    /// The provider's sign-in note, if AzMail knows the provider.
    pub note: String,
}

/// Why the form cannot make an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormError {
    BadEmail,
    NoImapHost,
    BadPort {
        field: &'static str,
        value: String,
    },
    /// An unencrypted connection to anything but this computer.
    PlainNotLocal(String),
    /// An Azlin account without a token server (none typed, none configured).
    NoTokenServer,
    /// A token server that cannot be one (the sentence says why).
    BadTokenServer(String),
    /// An Azlin account without a drive id.
    NoDrive,
}

impl std::fmt::Display for FormError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormError::BadEmail => write!(f, "Enter your mail address (name@example.org)."),
            FormError::NoImapHost => write!(f, "Enter the IMAP server."),
            FormError::BadPort { field, value } => {
                write!(f, "The {field} port {value:?} is not a port (1 - 65535).")
            }
            FormError::PlainNotLocal(host) => write!(
                f,
                "An unencrypted connection is only allowed to a test server on this computer, \
                 not to {host}: your password would cross the network in the clear."
            ),
            FormError::NoTokenServer => write!(
                f,
                "Enter the Azlin token server (https://...), or name it in the endpoints of \
                 ~/.azlin/config.json, in AZLIN_TOKEN_URL or with --azlin-token-url."
            ),
            FormError::BadTokenServer(why) => write!(f, "{why}"),
            FormError::NoDrive => write!(f, "Enter the drive id (d_...), or create a new drive."),
        }
    }
}

impl AccountForm {
    /// The form for an existing account.
    pub fn from_account(account: &Account) -> AccountForm {
        let folder = account
            .folder
            .as_ref()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(link) = &account.azlin {
            return AccountForm {
                email: account.email.clone(),
                name: account.name.clone(),
                smtp_host: account.smtp.host.clone(),
                smtp_port: account.smtp.port.to_string(),
                folder,
                azlin: true,
                token_url: link.token_url.clone(),
                drive_id: link.drive_id.clone(),
                ..AccountForm::default()
            };
        }
        AccountForm {
            email: account.email.clone(),
            name: account.name.clone(),
            username: account.username.clone(),
            imap_host: account.imap.host.clone(),
            imap_port: account.imap.port.to_string(),
            smtp_host: account.smtp.host.clone(),
            smtp_port: account.smtp.port.to_string(),
            folder,
            plain: account.security == Security::Plain,
            xoauth2: account.auth == AuthKind::Xoauth2,
            ..AccountForm::default()
        }
    }

    /// What the empty fields stand for, from the address.
    pub fn defaults(&self) -> FormDefaults {
        let Some((imap, smtp)) = guess_servers(&self.email) else {
            return FormDefaults {
                imap_port: IMAPS_PORT.to_string(),
                smtp_port: SMTPS_PORT.to_string(),
                ..FormDefaults::default()
            };
        };
        FormDefaults {
            username: self.email.trim().to_string(),
            imap_host: imap.host,
            imap_port: imap.port.to_string(),
            smtp_host: smtp.host,
            smtp_port: smtp.port.to_string(),
            note: provider_for(&self.email)
                .map(|p| p.note.to_string())
                .unwrap_or_default(),
        }
    }

    /// The account the form describes: typed values, else the defaults.
    pub fn to_account(&self) -> Result<Account, FormError> {
        let id = account_id(&self.email).ok_or(FormError::BadEmail)?;
        let defaults = self.defaults();
        let pick = |typed: &str, default: &str| {
            let typed = typed.trim();
            if typed.is_empty() {
                default.to_string()
            } else {
                typed.to_string()
            }
        };
        let port = |field: &'static str, typed: &str, default: &str| {
            let value = pick(typed, default);
            match value.parse::<u16>() {
                Ok(port) if port > 0 => Ok(port),
                _ => Err(FormError::BadPort { field, value }),
            }
        };
        let folder = self.folder.trim();
        if self.azlin {
            // An Azlin drive: the token server and the drive instead of the IMAP server; the
            // outgoing server stays the address's (sending is this computer's).
            let token_url = pick(&self.token_url, &self.token_default);
            if token_url.is_empty() {
                return Err(FormError::NoTokenServer);
            }
            crate::azlin::check_token_url(&token_url)
                .map_err(|e| FormError::BadTokenServer(e.to_string()))?;
            let drive_id = self.drive_id.trim();
            if drive_id.is_empty() {
                return Err(FormError::NoDrive);
            }
            let smtp_port = port("SMTP", &self.smtp_port, &defaults.smtp_port)?;
            return Ok(Account {
                id,
                email: self.email.trim().to_string(),
                name: self.name.trim().to_string(),
                username: self.email.trim().to_string(),
                imap: Server {
                    host: String::new(),
                    port: 0,
                },
                smtp: Server {
                    host: pick(&self.smtp_host, &defaults.smtp_host),
                    port: smtp_port,
                },
                security: Security::Tls,
                auth: AuthKind::Password,
                folder: (!folder.is_empty()).then(|| PathBuf::from(folder)),
                // The drive belongs to the token server it was made at: the account keeps
                // that one, whatever this run is told later.
                azlin: Some(AzlinLink {
                    token_url,
                    drive_id: drive_id.to_string(),
                }),
            });
        }
        let imap_host = pick(&self.imap_host, &defaults.imap_host);
        if imap_host.is_empty() {
            return Err(FormError::NoImapHost);
        }
        let imap_port = port("IMAP", &self.imap_port, &defaults.imap_port)?;
        let smtp_port = port("SMTP", &self.smtp_port, &defaults.smtp_port)?;
        if self.plain && !is_loopback_host(&imap_host) {
            return Err(FormError::PlainNotLocal(imap_host));
        }
        Ok(Account {
            id,
            email: self.email.trim().to_string(),
            name: self.name.trim().to_string(),
            username: pick(&self.username, &defaults.username),
            imap: Server {
                host: imap_host,
                port: imap_port,
            },
            smtp: Server {
                host: pick(&self.smtp_host, &defaults.smtp_host),
                port: smtp_port,
            },
            security: if self.plain {
                Security::Plain
            } else {
                Security::Tls
            },
            auth: if self.xoauth2 {
                AuthKind::Xoauth2
            } else {
                AuthKind::Password
            },
            folder: (!folder.is_empty()).then(|| PathBuf::from(folder)),
            azlin: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    const ADA: &str = "ada@example.org";

    fn account() -> Account {
        Account {
            id: String::from(ADA),
            email: String::from(ADA),
            name: String::new(),
            username: String::from(ADA),
            imap: Server {
                host: String::from("imap.example.org"),
                port: 993,
            },
            smtp: Server {
                host: String::from("smtp.example.org"),
                port: 465,
            },
            security: Security::Tls,
            auth: AuthKind::Password,
            folder: None,
            azlin: None,
        }
    }

    /// An Azlin account of the same address.
    fn azlin_account() -> Account {
        AccountForm {
            email: String::from(ADA),
            name: String::from("Ada Lovelace"),
            azlin: true,
            token_url: String::from("https://token.example"),
            drive_id: String::from("d_42"),
            ..AccountForm::default()
        }
        .to_account()
        .unwrap()
    }

    #[test]
    fn a_secret_never_shows_in_debug_output() {
        let secret = Secret::new(String::from("hunter2-app-password"));
        let shown = format!("{secret:?} {:?}", Some(secret.clone()));
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(!shown.contains("app-password"), "{shown}");
        assert_eq!(secret.expose(), "hunter2-app-password");
        assert!(!secret.is_empty());
        assert!(Secret::new(String::new()).is_empty());
    }

    #[test]
    fn known_providers_are_filled_in_from_the_address() {
        let gmail = provider_for("Ada.Lovelace@GMail.com").unwrap();
        assert_eq!(gmail.imap, ("imap.gmail.com", 993));
        assert_eq!(gmail.smtp, ("smtp.gmail.com", 465));
        assert!(gmail.note.contains("app password"), "{}", gmail.note);
        assert_eq!(provider_for("a@googlemail.com").unwrap().name, "Gmail");
        assert_eq!(
            provider_for("a@hotmail.com").unwrap().imap,
            ("outlook.office365.com", 993)
        );
        assert_eq!(provider_for("a@outlook.com").unwrap().name, "Outlook");
        let icloud = provider_for("a@me.com").unwrap();
        assert_eq!(icloud.imap, ("imap.mail.me.com", 993));
        assert!(
            icloud.note.contains("app-specific password"),
            "{}",
            icloud.note
        );
        assert_eq!(
            provider_for("a@fastmail.com").unwrap().imap,
            ("imap.fastmail.com", 993)
        );
        assert_eq!(provider_for("a@example.org"), None);
        assert_eq!(provider_for("not an address"), None);
    }

    #[test]
    fn an_unknown_domain_gets_imap_and_smtp_of_that_domain() {
        let (imap, smtp) = guess_servers("ada@Example.ORG").unwrap();
        assert_eq!(
            imap,
            Server {
                host: String::from("imap.example.org"),
                port: 993
            }
        );
        assert_eq!(
            smtp,
            Server {
                host: String::from("smtp.example.org"),
                port: 465
            }
        );
        let (imap, _) = guess_servers("a@gmail.com").unwrap();
        assert_eq!(imap.host, "imap.gmail.com");
        assert_eq!(guess_servers("ada@"), None);
        assert_eq!(guess_servers("ada"), None);
    }

    #[test]
    fn an_address_has_one_at_and_something_on_both_sides() {
        for good in [
            ADA,
            " ada@example.org ",
            "a.b+c@mail.example.co.uk",
            "x@localhost",
        ] {
            assert!(is_email(good), "{good:?}");
        }
        for bad in [
            "",
            "ada",
            "@example.org",
            "ada@",
            "a@b@c",
            "a d@example.org",
            "ada@ex ample.org",
        ] {
            assert!(!is_email(bad), "{bad:?}");
        }
        assert_eq!(
            email_domain("Ada@Example.Org").as_deref(),
            Some("example.org")
        );
        assert_eq!(email_domain("ada"), None);
    }

    #[test]
    fn the_account_id_is_the_address_made_safe_for_file_names_and_keys() {
        assert_eq!(account_id(" Ada@Example.org ").as_deref(), Some(ADA));
        assert_eq!(
            account_id("a.b+c_d-e@example.org").as_deref(),
            Some("a.b+c_d-e@example.org")
        );
        assert_eq!(
            account_id("we/ird\\na:me@example.org").as_deref(),
            Some("we_ird_na_me@example.org")
        );
        assert_eq!(
            account_id(".hidden@example.org").as_deref(),
            Some("_hidden@example.org")
        );
        assert_eq!(account_id("not an address"), None);
        let id = account_id(ADA).unwrap();
        assert!(!id.contains('/') && !id.contains(".."));
    }

    #[test]
    fn the_keyring_name_says_app_and_account() {
        assert_eq!(keyring_key(ADA), "AzMail/ada@example.org/imap");
        assert_eq!(
            secret_keyring_key(&account()),
            "AzMail/ada@example.org/imap"
        );
        assert_eq!(
            secret_keyring_key(&azlin_account()),
            "AzMail/ada@example.org/azlin"
        );
    }

    #[test]
    fn an_azlin_account_file_names_its_kind_and_drive_and_no_secret() {
        let a = azlin_account();
        assert!(a.is_azlin() && !account().is_azlin());
        let text = to_json(&a);
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            json["version"], 2,
            "an older AzMail refuses it instead of syncing it as IMAP"
        );
        assert_eq!(json["kind"], "azlin");
        assert_eq!(json["azlin"]["token_url"], "https://token.example");
        assert_eq!(json["azlin"]["drive_id"], "d_42");
        let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["azlin", "email", "format", "kind", "name", "smtp", "version"],
            "no IMAP server, no password, token or secret field"
        );
        assert_eq!(from_json(&text), Ok(a));
        // An IMAP account's file is the one every AzMail wrote.
        let imap: serde_json::Value = serde_json::from_str(&to_json(&account())).unwrap();
        assert_eq!(imap["version"], 1);
        assert!(imap.get("kind").is_none() && imap.get("azlin").is_none());
        let unknown = text.replace("\"kind\": \"azlin\"", "\"kind\": \"jmap\"");
        assert_eq!(
            from_json(&unknown),
            Err(AccountError::UnknownKind(String::from("jmap")))
        );
        let no_drive = text.replace("\"drive_id\": \"d_42\"", "\"drive_id\": \"\"");
        assert!(matches!(
            from_json(&no_drive),
            Err(AccountError::Malformed(_))
        ));
    }

    #[test]
    fn the_form_makes_an_azlin_account_from_the_token_server_and_the_drive() {
        let form = AccountForm::from_account(&azlin_account());
        assert!(form.azlin);
        assert_eq!(
            (form.token_url.as_str(), form.drive_id.as_str()),
            ("https://token.example", "d_42")
        );
        assert_eq!(form.to_account(), Ok(azlin_account()));
        // The configured token server stands for an empty field, and the account keeps it.
        let configured = AccountForm {
            token_url: String::new(),
            token_default: String::from("http://127.0.0.1:8081"),
            ..form.clone()
        };
        let made = configured.to_account().unwrap();
        assert_eq!(made.azlin.unwrap().token_url, "http://127.0.0.1:8081");
        assert_eq!(
            AccountForm {
                token_url: String::new(),
                ..form.clone()
            }
            .to_account(),
            Err(FormError::NoTokenServer)
        );
        assert_eq!(
            AccountForm {
                drive_id: String::from(" "),
                ..form.clone()
            }
            .to_account(),
            Err(FormError::NoDrive)
        );
        assert!(matches!(
            AccountForm {
                token_url: String::from("http://token.example"),
                ..form.clone()
            }
            .to_account(),
            Err(FormError::BadTokenServer(_))
        ));
        assert_eq!(
            AccountForm {
                email: String::from("ada"),
                ..form
            }
            .to_account(),
            Err(FormError::BadEmail)
        );
    }

    #[test]
    fn only_this_computer_is_a_loopback_host() {
        for local in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "127.1.2.3",
            "::1",
            "[::1]",
        ] {
            assert!(is_loopback_host(local), "{local}");
        }
        for remote in [
            "imap.gmail.com",
            "10.0.0.1",
            "128.0.0.1",
            "localhost.example.org",
            "",
        ] {
            assert!(!is_loopback_host(remote), "{remote}");
        }
    }

    #[test]
    fn the_test_password_is_used_only_by_a_headless_run() {
        assert_eq!(
            test_secret(Some("headless"), Some("pw")).map(|s| s.expose().to_string()),
            Some(String::from("pw"))
        );
        assert_eq!(test_secret(None, Some("pw")), None);
        assert_eq!(test_secret(Some("cpu"), Some("pw")), None);
        assert_eq!(test_secret(Some("headless"), None), None);
        assert_eq!(test_secret(Some("headless"), Some("")), None);
    }

    #[test]
    fn the_data_folder_is_azmail_data_else_mail_in_the_azlin_data_root() {
        let azlin = Path::new("/home/ada/.local/share/Azlin");
        assert_eq!(data_root(Some("/tmp/m"), azlin), PathBuf::from("/tmp/m"));
        assert_eq!(
            data_root(Some(" "), azlin),
            PathBuf::from("/home/ada/.local/share/Azlin/mail"),
            "a blank variable counts as unset"
        );
        assert_eq!(
            data_root(None, azlin),
            PathBuf::from("/home/ada/.local/share/Azlin/mail")
        );
        assert_eq!(
            legacy_root(Some(Path::new("/home/ada/.local/share"))),
            Some(PathBuf::from("/home/ada/.local/share/AzMail"))
        );
        assert_eq!(legacy_root(None), None);
        let data = Path::new("/data/Azlin");
        let root = DriveFolder::of(&data.join("mail"), data);
        assert_eq!(account_dir(&root, ADA).path(), data.join("mail").join(ADA));
        assert_eq!(mail_root(&root, &account()), account_dir(&root, ADA));
        assert!(mail_root(&root, &account()).is_data_tree());
        let elsewhere = Account {
            folder: Some(PathBuf::from("/Volumes/backup/mail")),
            ..account()
        };
        assert_eq!(
            mail_root(&root, &elsewhere).path(),
            PathBuf::from("/Volumes/backup/mail")
        );
        assert!(!mail_root(&root, &elsewhere).is_data_tree(), "outside the data tree");
        let inside = Account {
            folder: Some(data.join("archive").join("ada")),
            ..account()
        };
        assert!(mail_root(&root, &inside).is_data_tree(), "a folder of the data tree");
    }

    #[test]
    fn an_older_azmail_folder_moves_into_the_data_root_once() {
        let dir = crate::testutil::TempDir::new("legacy-root");
        let legacy = dir.0.join("AzMail");
        let root = dir.0.join("Azlin").join("mail");
        std::fs::create_dir_all(legacy.join(ADA)).unwrap();
        std::fs::write(legacy.join(ADA).join(ACCOUNT_FILE), to_json(&account())).unwrap();
        assert!(migrate_legacy_root(&legacy, &root).unwrap());
        assert!(root.join(ADA).join(ACCOUNT_FILE).is_file(), "the account came along");
        assert!(!legacy.exists(), "moved, not copied");
        assert!(!migrate_legacy_root(&legacy, &root).unwrap(), "nothing left to move");
        std::fs::create_dir_all(&legacy).unwrap();
        assert!(
            !migrate_legacy_root(&legacy, &root).unwrap(),
            "mail already in the new place wins: the old folder stays where it is"
        );
        assert!(legacy.exists());
    }

    #[test]
    fn an_account_file_round_trips_and_holds_no_secret() {
        let a = account();
        let text = to_json(&a);
        assert!(text.ends_with('\n'));
        assert_eq!(from_json(&text), Ok(a.clone()));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["format"], "azmail.account");
        assert_eq!(json["version"], 1);
        assert_eq!(json["email"], ADA);
        assert_eq!(json["imap"]["host"], "imap.example.org");
        assert_eq!(json["imap"]["port"], 993);
        assert_eq!(json["security"], "tls");
        assert_eq!(json["auth"], "password");
        let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            ["auth", "email", "format", "imap", "security", "smtp", "username", "version"],
            "no password, token or secret field"
        );
        let elsewhere = Account {
            folder: Some(PathBuf::from("/Volumes/backup/mail")),
            auth: AuthKind::Xoauth2,
            ..a
        };
        assert_eq!(from_json(&to_json(&elsewhere)), Ok(elsewhere));
    }

    #[test]
    fn an_account_with_a_name_sends_as_name_and_address() {
        assert_eq!(account().sender(), ADA, "no name: the address alone");
        let named = Account {
            name: String::from("Ada Lovelace"),
            ..account()
        };
        assert_eq!(named.sender(), "Ada Lovelace <ada@example.org>");
        let comma = Account {
            name: String::from("Lovelace, Ada"),
            ..account()
        };
        assert_eq!(comma.sender(), "\"Lovelace, Ada\" <ada@example.org>");
        let text = to_json(&named);
        assert!(text.contains("\"name\": \"Ada Lovelace\""), "{text}");
        assert_eq!(from_json(&text), Ok(named.clone()));
        let form = AccountForm::from_account(&named);
        assert_eq!(form.name, "Ada Lovelace");
        let mut typed = AccountForm {
            email: String::from(ADA),
            name: String::from("  Ada Lovelace "),
            ..AccountForm::default()
        };
        assert_eq!(typed.to_account().unwrap().name, "Ada Lovelace");
        typed.name.clear();
        assert_eq!(typed.to_account().unwrap().name, "");
    }

    #[test]
    fn a_file_that_is_not_an_account_is_refused() {
        assert!(matches!(from_json("{"), Err(AccountError::NotJson(_))));
        assert_eq!(from_json("{}"), Err(AccountError::NotAnAccount));
        let newer = to_json(&account()).replace("\"version\": 1", "\"version\": 3");
        assert_eq!(from_json(&newer), Err(AccountError::NewerVersion(3)));
        let broken = to_json(&account()).replace("\"imap\"", "\"imapx\"");
        assert!(matches!(
            from_json(&broken),
            Err(AccountError::Malformed(_))
        ));
        let bad =
            to_json(&account()).replace("\"email\": \"ada@example.org\"", "\"email\": \"ada\"");
        assert!(matches!(from_json(&bad), Err(AccountError::BadEmail(_))));
    }

    #[test]
    fn accounts_are_saved_in_their_folder_and_read_back() {
        let dir = TempDir::new("account");
        let root = DriveFolder::outside(dir.0.clone());
        let a = account();
        let path = save(&root, &a).unwrap();
        assert_eq!(path, dir.0.join(ADA).join("account.json"));
        let b = Account {
            id: String::from("ben@example.org"),
            email: String::from("ben@example.org"),
            username: String::from("ben"),
            ..account()
        };
        save(&root, &b).unwrap();
        std::fs::create_dir_all(dir.0.join("broken@example.org")).unwrap();
        std::fs::write(dir.0.join("broken@example.org").join("account.json"), "{").unwrap();
        std::fs::create_dir_all(dir.0.join("no-account-here")).unwrap();
        let (accounts, skipped) = load_all(&root);
        assert_eq!(accounts, vec![a, b]);
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        assert!(skipped[0].0.ends_with("broken@example.org/account.json"));
        assert!(load_all(&DriveFolder::outside(dir.0.join("missing"))).0.is_empty());
        assert_eq!(load(&root, "nobody@example.org"), None);
    }

    /// Local Folders (mail written without an account) live beside the accounts in a folder no
    /// account can have, and are never read as one.
    #[test]
    fn local_folders_are_no_account() {
        assert_eq!(account_id(LOCAL_ID), None, "an account's id is an address");
        assert!(!is_email(LOCAL_ID));
        let dir = TempDir::new("local-folders");
        let root = DriveFolder::outside(dir.0.clone());
        let drafts = account_dir(&root, LOCAL_ID).path().join("mail").join("drafts");
        std::fs::create_dir_all(&drafts).unwrap();
        std::fs::write(drafts.join("index.jsonl"), "").unwrap();
        save(&root, &account()).unwrap();
        let (accounts, skipped) = load_all(&root);
        assert_eq!(accounts, vec![account()]);
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(load(&root, LOCAL_ID), None);
    }

    #[test]
    fn empty_form_fields_stand_for_the_providers_servers() {
        let form = AccountForm {
            email: String::from("ada@gmail.com"),
            ..AccountForm::default()
        };
        let d = form.defaults();
        assert_eq!(d.username, "ada@gmail.com");
        assert_eq!(d.imap_host, "imap.gmail.com");
        assert_eq!(d.imap_port, "993");
        assert_eq!(d.smtp_host, "smtp.gmail.com");
        assert_eq!(d.smtp_port, "465");
        assert!(d.note.contains("app password"));
        let a = form.to_account().unwrap();
        assert_eq!(a.id, "ada@gmail.com");
        assert_eq!(a.imap.host, "imap.gmail.com");
        assert_eq!(a.imap.port, 993);
        assert_eq!(a.security, Security::Tls);
        assert_eq!(a.auth, AuthKind::Password);
        assert_eq!(a.folder, None);
        assert_eq!(AccountForm::default().defaults().note, "");
    }

    #[test]
    fn typed_form_fields_override_the_defaults() {
        let form = AccountForm {
            email: String::from(" Ada@Example.org "),
            name: String::from("Ada Lovelace"),
            username: String::from("ada"),
            imap_host: String::from(" 127.0.0.1 "),
            imap_port: String::from("1143"),
            smtp_host: String::from("mail.example.org"),
            smtp_port: String::from("587"),
            folder: String::from("/tmp/ada-mail"),
            plain: true,
            xoauth2: true,
            ..AccountForm::default()
        };
        let a = form.to_account().unwrap();
        assert_eq!(a.email, "Ada@Example.org");
        assert_eq!(a.id, ADA);
        assert_eq!(a.username, "ada");
        assert_eq!(
            a.imap,
            Server {
                host: String::from("127.0.0.1"),
                port: 1143
            }
        );
        assert_eq!(
            a.smtp,
            Server {
                host: String::from("mail.example.org"),
                port: 587
            }
        );
        assert_eq!(a.security, Security::Plain);
        assert_eq!(a.auth, AuthKind::Xoauth2);
        assert_eq!(a.folder, Some(PathBuf::from("/tmp/ada-mail")));
        assert_eq!(AccountForm::from_account(&a).to_account(), Ok(a));
    }

    #[test]
    fn the_form_refuses_what_cannot_be_an_account() {
        let good = AccountForm {
            email: String::from(ADA),
            ..AccountForm::default()
        };
        assert_eq!(
            AccountForm {
                email: String::from("ada"),
                ..good.clone()
            }
            .to_account(),
            Err(FormError::BadEmail)
        );
        assert_eq!(
            AccountForm {
                imap_port: String::from("0"),
                ..good.clone()
            }
            .to_account(),
            Err(FormError::BadPort {
                field: "IMAP",
                value: String::from("0")
            })
        );
        assert_eq!(
            AccountForm {
                smtp_port: String::from("smtp"),
                ..good.clone()
            }
            .to_account(),
            Err(FormError::BadPort {
                field: "SMTP",
                value: String::from("smtp")
            })
        );
        assert_eq!(
            AccountForm {
                plain: true,
                ..good.clone()
            }
            .to_account(),
            Err(FormError::PlainNotLocal(String::from("imap.example.org")))
        );
        let local = AccountForm {
            plain: true,
            imap_host: String::from("localhost"),
            ..good
        };
        assert_eq!(local.to_account().unwrap().security, Security::Plain);
    }
}
