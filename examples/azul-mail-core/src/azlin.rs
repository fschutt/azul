//! The Azlin account kind: the mailbox is files in the user's Azlin drive (`AZLIN_MAIL.md`).
//!
//! - The bucket's names: [`object_name`] (`<stamp>-<hash>.eml`), [`message_id`],
//!   [`marker_key`], the well-known folders ([`WELL_KNOWN`]).
//! - [`AzlinSession`]: what the keyring keeps for an Azlin account - the drive token and the
//!   current S3 credentials with where the drive is - and the S3 drive they open.
//! - [`CloudAccount`]: what AzMail needs from the Azlin account service (a new drive, fresh
//!   credentials), as sessions: azcloud-kit's [`TokenServer`] - the token server's HTTP API
//!   over any azul-storage `Transport` (azul's HTTP client in the app, a fake in the tests) -
//!   answers drive bundles, [`session_of`] makes them sessions.
//! - [`refresh_shared`]: a refresh every AzMail of this user takes turns at - under the
//!   account's keyring lock, the keyring re-read first, the new session written before the lock
//!   is let go: two AzMail processes on one account never spend one drive token twice.
//! - [`Endpoints`]: the token server's URL and an S3 endpoint override, from the shared Azlin
//!   config, then the environment, then the command line. There is no built-in default.
//!
//! No azul types here: tested without a window. Nothing here prints or `Debug`s a secret.

use std::collections::{BTreeSet, HashMap};

use azcloud_kit::{bundle::DriveBundle, shared::SharedKeyring, token::DEFAULT_TIER};
use azul_storage::{
    ops,
    sigv4::{sha256_hex, uri_decode, uri_encode},
    time::{amz_date, parse_iso8601},
    Credentials, Drive, DriveError, ObjectInfo, S3Config, S3Drive, Transport,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::folders::{self, LocalMailbox, Role, ServerMailbox};

// The token server's API and its address checks are azcloud-kit's.
pub use azcloud_kit::token::{check_token_url, url_host, TokenError, TokenServer};
// The margins of a session's credentials are the kit's: refreshed before a Send/Receive when
// less than an hour is left, used by an action while more than a minute is.
pub use azcloud_kit::session::{REFRESH_MARGIN_SECS, VALID_MARGIN_SECS};

// ==== The bucket's names ====

/// The mailbox in the drive.
pub const MAIL_PREFIX: &str = "mail/";
/// The state markers: `mail/.state/<id>/<flag>`.
pub const STATE_PREFIX: &str = "mail/.state/";
/// A message object's extension.
pub const EML: &str = ".eml";
/// The marker of a read message.
pub const SEEN: &str = "seen";
/// The marker of a message flagged for follow-up.
pub const FLAGGED: &str = "flagged";
/// The marker of a message replied to.
pub const ANSWERED: &str = "answered";
/// The marker of a message another program marked deleted and has not expunged yet (IMAP's
/// `\Deleted`, written by the Azlin Bridge): AzMail hides the message while it is there.
pub const DELETED: &str = "deleted";
/// The folder of a message's label markers in its state.
pub const LABEL_DIR: &str = "label";

/// The well-known folders as AzMail writes them, by role (read in any case).
pub const WELL_KNOWN: [(Role, &str); 6] = [
    (Role::Inbox, "Inbox"),
    (Role::Sent, "Sent"),
    (Role::Drafts, "Drafts"),
    (Role::Archive, "Archive"),
    (Role::Spam, "Spam"),
    (Role::Trash, "Trash"),
];

/// The bucket's name of the well-known folder of `role` (`Spam`); `None` for any other role.
pub fn well_known_name(role: Role) -> Option<&'static str> {
    WELL_KNOWN
        .iter()
        .find(|(r, _)| *r == role)
        .map(|(_, name)| *name)
}

/// `20261008T091500Z-3f2a9c1e5b7d4a60.eml`: the arrival time `stamp_secs` (UTC, the SigV4 date
/// format) and the first 16 hex digits of the SHA-256 of `bytes`. The same bytes arriving in
/// the same second always get the same name; different bytes never share one.
pub fn object_name(bytes: &[u8], stamp_secs: u64) -> String {
    let hash = sha256_hex(bytes);
    format!("{}-{}{EML}", amz_date(stamp_secs), &hash[..16])
}

/// `name` without its `.eml` (any case); `None` when it has none.
fn strip_eml(name: &str) -> Option<&str> {
    let cut = name.len().checked_sub(EML.len())?;
    let stem = name.get(..cut)?;
    let extension = name.get(cut..)?;
    extension.eq_ignore_ascii_case(EML).then_some(stem)
}

/// The id of the message object `key` (`mail/Inbox/X.eml` is `X`): what its state markers are
/// filed under. `None` for a key that is no message of a folder (not under `mail/<folder>/`,
/// AzMail's own bookkeeping, not `.eml`).
pub fn message_id(key: &str) -> Option<&str> {
    folder_of_key(key)?;
    let name = key.rsplit('/').next()?;
    strip_eml(name).filter(|id| !id.is_empty())
}

/// The folder of the message object `key`, under `mail/` (`mail/Work/Projects/X.eml` is
/// `Work/Projects`); `None` outside a folder or in AzMail's bookkeeping (a name starting with
/// `.`).
pub fn folder_of_key(key: &str) -> Option<&str> {
    let rest = key.strip_prefix(MAIL_PREFIX)?;
    let (folder, _) = rest.rsplit_once('/')?;
    let hidden = folder
        .split('/')
        .any(|segment| segment.is_empty() || segment.starts_with('.'));
    (!hidden).then_some(folder)
}

/// `mail/<folder>/<name>`.
pub fn message_key(folder: &str, name: &str) -> String {
    format!("{MAIL_PREFIX}{folder}/{name}")
}

/// `mail/<folder>/`.
pub fn folder_prefix(folder: &str) -> String {
    format!("{MAIL_PREFIX}{folder}/")
}

/// The arrival time an id starts with (`20261008T091500Z-...`), in seconds since 1970; `None`
/// for a name that does not start with a stamp (a file put into a folder by hand).
pub fn stamp_of(id: &str) -> Option<u64> {
    let b = id.as_bytes();
    if b.len() < 16 || b[8] != b'T' || b[15] != b'Z' || (b.len() > 16 && b[16] != b'-') {
        return None;
    }
    let digits = |range: std::ops::Range<usize>| b[range].iter().all(u8::is_ascii_digit);
    if !digits(0..8) || !digits(9..15) {
        return None;
    }
    // The first 16 bytes are ASCII from here on.
    let s = &id[..16];
    let iso = format!(
        "{}-{}-{}T{}:{}:{}Z",
        &s[0..4],
        &s[4..6],
        &s[6..8],
        &s[9..11],
        &s[11..13],
        &s[13..15]
    );
    parse_iso8601(&iso)
}

/// `mail/.state/<id>/<flag>`: the marker that says `flag` (`seen`, `flagged`, `answered`).
pub fn marker_key(id: &str, flag: &str) -> String {
    format!("{STATE_PREFIX}{id}/{flag}")
}

/// `mail/.state/<id>/label/<label>`, the label percent-encoded.
pub fn label_marker_key(id: &str, label: &str) -> String {
    format!("{STATE_PREFIX}{id}/{LABEL_DIR}/{}", uri_encode(label, true))
}

/// `mail/.state/<id>/`: every marker of a message.
pub fn state_prefix(id: &str) -> String {
    format!("{STATE_PREFIX}{id}/")
}

/// What one marker says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    /// `seen`, `flagged`, `answered`, or a flag of a newer AzMail.
    Flag(String),
    Label(String),
}

/// The message id and what the marker `key` says; `None` for a key that is no marker.
pub fn parse_marker(key: &str) -> Option<(String, Marker)> {
    let rest = key.strip_prefix(STATE_PREFIX)?;
    let (id, what) = rest.split_once('/')?;
    if id.is_empty() || what.is_empty() {
        return None;
    }
    match what.split_once('/') {
        None => Some((id.to_string(), Marker::Flag(what.to_string()))),
        Some((LABEL_DIR, label)) if !label.is_empty() && !label.contains('/') => {
            uri_decode(label).map(|label| (id.to_string(), Marker::Label(label)))
        }
        Some(_) => None,
    }
}

/// A message's state, as its markers say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageState {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    /// Marked deleted by another program (not expunged yet): hidden in AzMail.
    pub deleted: bool,
    pub labels: BTreeSet<String>,
}

impl MessageState {
    /// Takes one marker into the state (a flag AzMail does not know is left out).
    pub fn apply(&mut self, marker: Marker) {
        match marker {
            Marker::Flag(flag) => match flag.as_str() {
                SEEN => self.seen = true,
                FLAGGED => self.flagged = true,
                ANSWERED => self.answered = true,
                DELETED => self.deleted = true,
                _ => {}
            },
            Marker::Label(label) => {
                self.labels.insert(label);
            }
        }
    }

    /// The state as an IMAP server's flags (`\Seen`, `\Flagged`, `\Answered`): what the index's
    /// `flags` hold for every account.
    pub fn imap_flags(&self) -> Vec<String> {
        let mut flags = Vec::new();
        if self.seen {
            flags.push(String::from("\\Seen"));
        }
        if self.flagged {
            flags.push(String::from("\\Flagged"));
        }
        if self.answered {
            flags.push(String::from("\\Answered"));
        }
        flags
    }
}

/// Every message's state from the keys of a listing of `mail/.state/`.
pub fn states_from_keys<'a>(
    keys: impl IntoIterator<Item = &'a str>,
) -> HashMap<String, MessageState> {
    let mut states: HashMap<String, MessageState> = HashMap::new();
    for key in keys {
        if let Some((id, marker)) = parse_marker(key) {
            states.entry(id).or_default().apply(marker);
        }
    }
    states
}

// ==== The session: the drive token and the S3 credentials ====

/// The region a bundle that names none is in (S3's own default, what the token server says).
pub const DEFAULT_REGION: &str = "us-east-1";

/// What the keyring keeps for an Azlin account (`AzMail/<account>/azlin`, as JSON): the drive
/// token - a refresh token, a new one with every refresh - and the S3 credentials of the last
/// refresh with where the drive is. `Debug` shows no secret.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AzlinSession {
    pub drive_id: String,
    pub drive_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bucket: String,
    #[serde(default = "path_style_default")]
    pub path_style: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_key_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secret_access_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    /// When the credentials stop working, in seconds since 1970; `None`: keys that do not expire
    /// (a token server handing out long-lived keys).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

fn path_style_default() -> bool {
    true
}

impl std::fmt::Debug for AzlinSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzlinSession")
            .field("drive_id", &self.drive_id)
            .field("drive_token", &"<hidden>")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("credentials", &self.has_credentials())
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl AzlinSession {
    /// A session that has only its drive token (typed into the wizard): the first Send/Receive
    /// refreshes it.
    pub fn with_token(drive_id: &str, drive_token: &str) -> AzlinSession {
        AzlinSession {
            drive_id: drive_id.trim().to_string(),
            drive_token: drive_token.trim().to_string(),
            endpoint: String::new(),
            region: String::new(),
            bucket: String::new(),
            path_style: true,
            access_key_id: String::new(),
            secret_access_key: String::new(),
            session_token: None,
            expires_at: None,
        }
    }

    /// The keyring entry's text (JSON).
    pub fn to_secret(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The keyring entry read back: a session, or (anything that is not one) a bare drive token
    /// for the drive `drive_id`.
    pub fn from_secret(secret: &str, drive_id: &str) -> AzlinSession {
        let text = secret.trim();
        if text.starts_with('{') {
            if let Ok(mut session) = serde_json::from_str::<AzlinSession>(text) {
                if session.drive_id.is_empty() {
                    session.drive_id = drive_id.trim().to_string();
                }
                return session;
            }
        }
        AzlinSession::with_token(drive_id, text)
    }

    /// The session holds S3 credentials and knows where the drive is.
    pub fn has_credentials(&self) -> bool {
        !self.access_key_id.is_empty()
            && !self.secret_access_key.is_empty()
            && !self.endpoint.is_empty()
            && !self.bucket.is_empty()
    }

    /// The credentials must be refreshed before they are used at `now` (seconds since 1970):
    /// there are none, or less than [`REFRESH_MARGIN_SECS`] are left.
    pub fn needs_refresh(&self, now: u64) -> bool {
        !self.has_credentials()
            || self
                .expires_at
                .is_some_and(|at| at <= now.saturating_add(REFRESH_MARGIN_SECS))
    }

    /// The credentials still work at `now`, with a minute to spare: an action (a move, a mark)
    /// uses them as they are; only Send/Receive refreshes.
    pub fn is_valid_at(&self, now: u64) -> bool {
        self.has_credentials()
            && self
                .expires_at
                .is_none_or(|at| at > now.saturating_add(VALID_MARGIN_SECS))
    }

    /// Where the bucket is: the endpoint the token server reported, or `endpoint_override`
    /// (`$AZLIN_S3_URL`, `--azlin-s3-url`).
    pub fn s3_config(&self, endpoint_override: Option<&str>) -> S3Config {
        let endpoint = endpoint_override
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .unwrap_or(self.endpoint.trim());
        let region = if self.region.trim().is_empty() {
            DEFAULT_REGION
        } else {
            self.region.trim()
        };
        S3Config {
            endpoint: endpoint.to_string(),
            region: region.to_string(),
            bucket: self.bucket.clone(),
            path_style: self.path_style,
        }
    }

    /// The S3 credentials (with the session token of temporary ones).
    pub fn credentials(&self) -> Credentials {
        let credentials = Credentials::new(&self.access_key_id, &self.secret_access_key);
        match self.session_token.as_deref().filter(|t| !t.is_empty()) {
            Some(token) => credentials.with_session_token(token),
            None => credentials,
        }
    }

    /// The drive these credentials open, its requests sent through `transport`. Sends nothing.
    pub fn open_drive(
        &self,
        endpoint_override: Option<&str>,
        transport: Box<dyn Transport>,
    ) -> Result<S3Drive, DriveError> {
        if !self.has_credentials() {
            return Err(DriveError::Denied {
                message: String::from(
                    "not signed in to the Azlin drive yet (Send/Receive signs in)",
                ),
            });
        }
        S3Drive::new(
            self.s3_config(endpoint_override),
            self.credentials(),
            transport,
        )
    }
}

// ==== The account service: the token server ====

/// What AzMail needs from the Azlin account service, as sessions. azcloud-kit's
/// [`TokenServer`] is it: blocking, call it from an azul `Thread`.
pub trait CloudAccount {
    /// A new drive named `name` and its first credentials (a development token server only:
    /// real sign-ups go through a checkout).
    fn create_drive(&self, name: &str) -> Result<AzlinSession, TokenError>;
    /// Fresh credentials for `drive_id` with this device's drive token. The answer carries the
    /// NEXT drive token: the one given is dead from then on, and reusing it makes the token
    /// server revoke this device.
    fn refresh_session(&self, drive_id: &str, drive_token: &str)
        -> Result<AzlinSession, TokenError>;
}

impl CloudAccount for TokenServer<'_> {
    fn create_drive(&self, name: &str) -> Result<AzlinSession, TokenError> {
        session_of(&self.create_dev_drive(name, DEFAULT_TIER)?)
    }

    fn refresh_session(
        &self,
        drive_id: &str,
        drive_token: &str,
    ) -> Result<AzlinSession, TokenError> {
        session_of(&TokenServer::refresh(self, drive_id, drive_token)?)
    }
}

/// A drive bundle of the token server (a new drive, a refresh) as the keyring's session: the
/// drive token, the credentials and where the drive is.
pub fn session_of(bundle: &DriveBundle) -> Result<AzlinSession, TokenError> {
    let Some(config) = bundle.entry.s3_config() else {
        return Err(TokenError::Protocol(String::from(
            "the answer's drive is not an S3 bucket",
        )));
    };
    let credentials = &bundle.credentials;
    let session = AzlinSession {
        drive_id: bundle.drive_id().trim().to_string(),
        drive_token: bundle.drive_token.trim().to_string(),
        endpoint: config.endpoint.trim().to_string(),
        region: config.region.trim().to_string(),
        bucket: config.bucket.trim().to_string(),
        path_style: config.path_style,
        access_key_id: credentials.access_key_id.clone(),
        secret_access_key: credentials.secret_access_key.clone(),
        session_token: credentials.session_token.clone(),
        expires_at: credentials.expires_at,
    };
    if !session.has_credentials() {
        return Err(TokenError::Protocol(String::from(
            "the answer has no credentials, endpoint or bucket",
        )));
    }
    Ok(session)
}

/// A token server's answer (the drive bundle: `drive.id`, `drive.location`, `credentials`,
/// `drive_token`) as a session.
pub fn session_from_bundle(json: &str) -> Result<AzlinSession, TokenError> {
    session_of(&DriveBundle::parse(json)?)
}

// ==== A refresh every AzMail of this user takes turns at ====

/// `session` - what the user gave the account (a drive token typed into its settings, a new
/// drive's session) - into the keyring entry `key`, replacing what was there, under the entry's
/// lock: from the sync thread, before anything spends its token, so no later write of another
/// thread or process can land between. `Err`: why the keyring did not take it. Blocking.
///
/// # Errors
///
/// The lock or the keyring's refusal, as a sentence.
pub fn store_shared(
    keyring: &SharedKeyring,
    key: &str,
    session: &AzlinSession,
) -> Result<(), String> {
    let _lock = keyring.lock(key).map_err(|e| e.to_string())?;
    keyring
        .set(key, &session.to_secret())
        .map_err(|e| e.to_string())
}

/// What [`refresh_shared`] switched to. `Debug` shows no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refreshed {
    pub session: AzlinSession,
    /// Another process had refreshed: its session was read from the keyring, no token spent.
    pub adopted: bool,
    /// Whether the keyring has the session: `Err` why it did not take a new one - it lives in
    /// this process only.
    pub saved: Result<(), String>,
}

/// Fresh credentials for the account whose session is kept in the keyring entry `key`, one
/// process at a time and spending only the NEWEST drive token: the refresh holds the entry's
/// lock in `keyring` (every AzMail of this user shares it) and reads the entry first. When
/// another process has rotated the token `held` carries, its session is taken as it is - unless
/// its credentials run out at `now` too, or are the ones the drive refused (`refused`, an access
/// key), and then ITS token is the one spent. A new session goes into the keyring before the
/// lock is let go: the token just spent is dead, and the next process must find this one. A
/// keyring that cannot be read leaves `held` the newest token this process knows. Blocking:
/// from the sync thread.
///
/// # Errors
///
/// The token server's refusal or no answer; [`TokenError::Connect`] when another AzMail holds
/// the lock longer than a refresh takes.
pub fn refresh_shared(
    account: &dyn CloudAccount,
    keyring: &SharedKeyring,
    key: &str,
    held: &AzlinSession,
    refused: Option<&str>,
    now: u64,
) -> Result<Refreshed, TokenError> {
    let _lock = keyring.lock(key).map_err(|e| {
        TokenError::Connect(format!(
            "another AzMail is renewing this account's session ({e})"
        ))
    })?;
    let mut spend = held.clone();
    if let Ok(Some(text)) = keyring.get(key) {
        let stored = AzlinSession::from_secret(&text, &held.drive_id);
        let newer = stored.drive_id == held.drive_id
            && !stored.drive_token.is_empty()
            && stored.drive_token != held.drive_token;
        if newer {
            let usable = stored.has_credentials()
                && match refused {
                    Some(refused) => stored.access_key_id != refused,
                    None => !stored.needs_refresh(now),
                };
            if usable {
                return Ok(Refreshed {
                    session: stored,
                    adopted: true,
                    saved: Ok(()),
                });
            }
            // The token `held` carries is spent: the keyring's is the newest.
            spend = stored;
        }
    }
    let fresh = account.refresh_session(&spend.drive_id, &spend.drive_token)?;
    let saved = keyring
        .set(key, &fresh.to_secret())
        .map_err(|e| e.to_string());
    Ok(Refreshed {
        session: fresh,
        adopted: false,
        saved,
    })
}

// ==== Where the token server is ====

/// The token server's URL in the environment.
pub const TOKEN_URL_VAR: &str = "AZLIN_TOKEN_URL";
/// The S3 endpoint override in the environment.
pub const S3_URL_VAR: &str = "AZLIN_S3_URL";
/// The token server's URL on the command line.
pub const TOKEN_URL_FLAG: &str = "--azlin-token-url";
/// The S3 endpoint override on the command line.
pub const S3_URL_FLAG: &str = "--azlin-s3-url";
/// The section of the shared Azlin config (`~/.azlin/config.json`) with the endpoints.
pub const CONFIG_SECTION: &str = "endpoints";
/// The spellings of the token server's key in that section this reader takes, until azul-appkit's
/// typed accessor of the section replaces [`endpoints_from_config`].
const CONFIG_TOKEN_KEYS: [&str; 5] = [
    "token",
    "token_url",
    "tokenUrl",
    "token_server",
    "tokenServer",
];
/// The same for the S3 endpoint.
const CONFIG_S3_KEYS: [&str; 4] = ["s3", "s3_url", "s3Url", "storage"];

/// Where the Azlin services are, as this run was told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Endpoints {
    /// The token server; `None`: nobody said (the wizard asks).
    pub token_url: Option<String>,
    /// The S3 endpoint to use instead of the one the token server reports.
    pub s3_url: Option<String>,
}

/// A value that says something: trimmed, not empty.
fn said(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Endpoints {
    /// The shared config's `endpoints` section (the file's text, if there is a file), then the
    /// environment (`env` reads a variable), then the switches: a later source wins where it
    /// says something.
    pub fn resolve(
        config_json: Option<&str>,
        env: &dyn Fn(&str) -> Option<String>,
        flags: &Endpoints,
    ) -> Endpoints {
        let mut out = config_json.map(endpoints_from_config).unwrap_or_default();
        if let Some(url) = said(env(TOKEN_URL_VAR)) {
            out.token_url = Some(url);
        }
        if let Some(url) = said(env(S3_URL_VAR)) {
            out.s3_url = Some(url);
        }
        if let Some(url) = said(flags.token_url.clone()) {
            out.token_url = Some(url);
        }
        if let Some(url) = said(flags.s3_url.clone()) {
            out.s3_url = Some(url);
        }
        out
    }

    /// The token server an account uses: its own (the one it was created with), else this
    /// run's.
    pub fn token_url_for(&self, account_url: &str) -> Option<String> {
        said(Some(account_url.to_string())).or_else(|| self.token_url.clone())
    }
}

/// The `endpoints` section of the shared Azlin config's text; nothing for a file that is not a
/// config or has no such section.
pub fn endpoints_from_config(json: &str) -> Endpoints {
    let Ok(Value::Object(config)) = serde_json::from_str::<Value>(json) else {
        return Endpoints::default();
    };
    let Some(Value::Object(section)) = config.get(CONFIG_SECTION) else {
        return Endpoints::default();
    };
    let pick = |keys: &[&str]| {
        said(
            keys.iter()
                .find_map(|key| section.get(*key).and_then(Value::as_str))
                .map(str::to_string),
        )
    };
    Endpoints {
        token_url: pick(&CONFIG_TOKEN_KEYS),
        s3_url: pick(&CONFIG_S3_KEYS),
    }
}

// ==== The drive's folders ====

/// One folder of the drive's mailbox: its path under `mail/` (`Inbox`, `Work/Projects`) and the
/// listing's entries of its messages, in key order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFolder {
    pub path: String,
    pub messages: Vec<ObjectInfo>,
}

/// Every folder of the drive's mailbox, at any depth, by path: one listing per folder (pages
/// of 1000). AzMail's bookkeeping (`.state`, `.index`) is no folder.
pub fn list_mailbox(drive: &dyn Drive) -> Result<Vec<RemoteFolder>, DriveError> {
    let mut found = Vec::new();
    let mut queue: Vec<String> = vec![String::new()];
    while let Some(path) = queue.pop() {
        let prefix = if path.is_empty() {
            String::from(MAIL_PREFIX)
        } else {
            folder_prefix(&path)
        };
        let level = ops::list_folder_all(drive, &prefix)?;
        for sub in &level.folders {
            let name = sub
                .strip_prefix(prefix.as_str())
                .unwrap_or_default()
                .trim_end_matches('/');
            if name.is_empty() || name.starts_with('.') {
                continue;
            }
            queue.push(if path.is_empty() {
                name.to_string()
            } else {
                format!("{path}/{name}")
            });
        }
        if !path.is_empty() {
            let messages = level
                .objects
                .into_iter()
                .filter(|object| message_id(&object.key).is_some())
                .collect();
            found.push(RemoteFolder { path, messages });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// Every message's state, from one listing of `mail/.state/`.
pub fn list_states(drive: &dyn Drive) -> Result<HashMap<String, MessageState>, DriveError> {
    let markers = ops::list_all(drive, STATE_PREFIX)?;
    Ok(states_from_keys(
        markers.iter().map(|m| m.key.as_str()),
    ))
}

/// The IMAP special-use attribute that gives a well-known folder its role.
fn special_use(role: Role) -> Option<&'static str> {
    match role {
        Role::Sent => Some("\\Sent"),
        Role::Drafts => Some("\\Drafts"),
        Role::Archive => Some("\\Archive"),
        Role::Spam => Some("\\Junk"),
        Role::Trash => Some("\\Trash"),
        _ => None,
    }
}

/// A folder of the drive as `folders::local_mailboxes` reads an IMAP server's: a well-known
/// name with its special-use attribute, `/` the hierarchy, and `&` written `&-` (the drive's
/// names are UTF-8, a server's modified UTF-7, which the folder rules decode).
fn as_server_mailbox(path: &str) -> ServerMailbox {
    let attributes = WELL_KNOWN
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(path))
        .and_then(|(role, _)| special_use(*role))
        .map(|attribute| vec![attribute.to_string()])
        .unwrap_or_default();
    ServerMailbox {
        name: path.replace('&', "&-"),
        delimiter: Some(String::from("/")),
        attributes,
    }
}

/// The local folders of the drive's folders `paths` and of the well-known ones it has none
/// for (by role): a folder's key, role and sidebar name as an IMAP server's folder gets them
/// (`folders.rs`: `Inbox` is `inbox`, `Junk` is the spam folder when there is no `Spam`),
/// its `server_name` its path in the drive.
pub fn local_folders(paths: &[String]) -> Vec<LocalMailbox> {
    let mut listed: Vec<ServerMailbox> = paths.iter().map(|path| as_server_mailbox(path)).collect();
    let roles: Vec<Role> = folders::local_mailboxes(&listed)
        .iter()
        .map(|mailbox| mailbox.role)
        .collect();
    for (role, name) in WELL_KNOWN {
        if !roles.contains(&role) && !paths.iter().any(|path| path.eq_ignore_ascii_case(name)) {
            listed.push(as_server_mailbox(name));
        }
    }
    let mut boxes = folders::local_mailboxes(&listed);
    for mailbox in &mut boxes {
        mailbox.server_name = mailbox.server_name.replace("&-", "&");
    }
    boxes
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use azul_storage::{HttpCall, HttpReply, Method};

    use super::*;

    #[test]
    fn an_object_name_is_the_arrival_stamp_and_the_start_of_the_hash() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad.
        assert_eq!(
            object_name(b"abc", 0),
            "19700101T000000Z-ba7816bf8f01cfea.eml"
        );
        // 2026-10-08T09:15:00Z
        let name = object_name(b"abc", 1_791_450_900);
        assert_eq!(name, "20261008T091500Z-ba7816bf8f01cfea.eml");
        assert_eq!(
            object_name(b"abc", 1_791_450_900),
            name,
            "the same bytes, the same name"
        );
        assert_ne!(
            object_name(b"abd", 1_791_450_900),
            name,
            "other bytes, another name"
        );
        assert_eq!(stamp_of(name.trim_end_matches(".eml")), Some(1_791_450_900));
    }

    #[test]
    fn a_messages_id_and_folder_come_from_its_key() {
        let key = "mail/Inbox/20261008T091500Z-ba7816bf8f01cfea.eml";
        assert_eq!(message_id(key), Some("20261008T091500Z-ba7816bf8f01cfea"));
        assert_eq!(folder_of_key(key), Some("Inbox"));
        assert_eq!(
            folder_of_key("mail/Work/Projects/x.EML"),
            Some("Work/Projects")
        );
        assert_eq!(
            message_id("mail/Work/Projects/x.EML"),
            Some("x"),
            "any case"
        );
        assert_eq!(message_id("mail/x.eml"), None, "not in a folder");
        assert_eq!(message_id("mail/Inbox/notes.txt"), None, "not a message");
        assert_eq!(
            message_id("mail/.state/x/seen"),
            None,
            "AzMail's bookkeeping"
        );
        assert_eq!(message_id("docs/Inbox/x.eml"), None, "not the mailbox");
        assert_eq!(message_id("mail/Inbox/.eml"), None, "no name");
        assert_eq!(message_key("Spam", "x.eml"), "mail/Spam/x.eml");
        assert_eq!(folder_prefix("Work/Projects"), "mail/Work/Projects/");
        assert_eq!(stamp_of("invoice"), None, "a name put there by hand");
        assert_eq!(stamp_of("20261008T091500Zx"), None);
        assert_eq!(
            stamp_of("2026€008T091500Z-x"),
            None,
            "never cuts a character"
        );
    }

    #[test]
    fn markers_say_a_flag_or_a_label_of_one_message() {
        assert_eq!(marker_key("X", SEEN), "mail/.state/X/seen");
        assert_eq!(state_prefix("X"), "mail/.state/X/");
        let label = label_marker_key("X", "Work/Urgent");
        assert_eq!(label, "mail/.state/X/label/Work%2FUrgent");
        assert_eq!(
            parse_marker(&label),
            Some((
                String::from("X"),
                Marker::Label(String::from("Work/Urgent"))
            ))
        );
        assert_eq!(
            parse_marker("mail/.state/X/seen"),
            Some((String::from("X"), Marker::Flag(String::from("seen"))))
        );
        assert_eq!(parse_marker("mail/.state/X"), None);
        assert_eq!(parse_marker("mail/.state/X/other/thing"), None);
        assert_eq!(parse_marker("mail/Inbox/X.eml"), None);
        let states = states_from_keys([
            "mail/.state/A/seen",
            "mail/.state/A/flagged",
            "mail/.state/B/answered",
            "mail/.state/B/label/Garden",
            "mail/.state/C/something-new",
        ]);
        assert_eq!(states["A"].imap_flags(), vec!["\\Seen", "\\Flagged"]);
        assert_eq!(states["B"].imap_flags(), vec!["\\Answered"]);
        assert!(states["B"].labels.contains("Garden"));
        assert_eq!(
            states["C"],
            MessageState::default(),
            "an unknown flag is left out"
        );
    }

    #[test]
    fn a_deleted_marker_is_read_and_is_no_imap_flag_of_the_index() {
        let states = states_from_keys(["mail/.state/D/deleted", "mail/.state/D/seen"]);
        assert!(states["D"].deleted);
        assert_eq!(states["D"].imap_flags(), vec!["\\Seen"]);
        assert_eq!(marker_key("D", DELETED), "mail/.state/D/deleted");
    }

    #[test]
    fn a_session_round_trips_through_its_keyring_entry_and_debug_shows_no_secret() {
        let mut session = AzlinSession::with_token("d_1", " dt_f.0.secret-token ");
        assert_eq!(session.drive_token, "dt_f.0.secret-token");
        assert!(!session.has_credentials());
        assert!(session.needs_refresh(0), "no credentials yet");
        session.endpoint = String::from("http://127.0.0.1:9000");
        session.bucket = String::from("d-1");
        session.access_key_id = String::from("AKID-SECRET-PART");
        session.secret_access_key = String::from("s3-secret-key");
        session.session_token = Some(String::from("session-token-secret"));
        session.expires_at = Some(10_000);
        let text = session.to_secret();
        assert_eq!(AzlinSession::from_secret(&text, "d_1"), session);
        let shown = format!("{session:?}");
        for secret in [
            "secret-token",
            "AKID-SECRET-PART",
            "s3-secret-key",
            "session-token-secret",
        ] {
            assert!(!shown.contains(secret), "{shown}");
        }
        assert!(!session.needs_refresh(10_000 - REFRESH_MARGIN_SECS - 1));
        assert!(
            session.needs_refresh(10_000 - REFRESH_MARGIN_SECS),
            "less than the margin left"
        );
        assert!(session.is_valid_at(10_000 - 61), "an action uses them to the last minute");
        assert!(!session.is_valid_at(10_000 - 60));
        session.expires_at = None;
        assert!(
            !session.needs_refresh(u64::MAX / 2),
            "long-lived keys never need one"
        );
        assert!(session.is_valid_at(u64::MAX / 2));
        // A drive token typed into the wizard is a session too.
        assert_eq!(
            AzlinSession::from_secret("dt_f.0.typed", "d_1"),
            AzlinSession::with_token("d_1", "dt_f.0.typed")
        );
        let config = session.s3_config(None);
        assert_eq!(config.endpoint, "http://127.0.0.1:9000");
        assert_eq!(config.region, DEFAULT_REGION);
        assert!(config.path_style);
        assert_eq!(
            session.s3_config(Some(" http://127.0.0.1:9100 ")).endpoint,
            "http://127.0.0.1:9100"
        );
        assert_eq!(
            session.credentials().session_token.as_deref(),
            Some("session-token-secret")
        );
    }

    /// The token server's answer as azlin-token writes it (`drives::signup_response`).
    fn bundle(token: &str, expires: &str) -> String {
        serde_json::json!({
            "drive": {
                "id": "d_42",
                "name": "AzMail",
                "location": {"kind": "s3", "endpoint": "http://127.0.0.1:9000", "region": "us-east-1",
                             "bucket": "d-42", "path_style": true,
                             "auth": {"type": "azlin", "drive_id": "d_42", "account_url": ""}}
            },
            "credentials": {"access_key_id": "AZT1", "secret_access_key": "sk", "session_token": "st",
                            "expires_at": expires},
            "failover": ["http://127.0.0.1:9001"],
            "nodes": [{"url": "http://127.0.0.1:9001"}],
            "quota_bytes": 107_374_182_400_u64,
            "read_only": false,
            "drive_token": token,
            "tier": "100GB"
        })
        .to_string()
    }

    #[test]
    fn a_drive_bundle_becomes_a_session() {
        let session = session_from_bundle(&bundle("dt_f.1.abc", "2026-10-08T21:15:00Z")).unwrap();
        assert_eq!(session.drive_id, "d_42");
        assert_eq!(session.drive_token, "dt_f.1.abc");
        assert_eq!(session.endpoint, "http://127.0.0.1:9000");
        assert_eq!(session.bucket, "d-42");
        assert_eq!(session.region, "us-east-1");
        assert_eq!(session.access_key_id, "AZT1");
        assert_eq!(session.session_token.as_deref(), Some("st"));
        assert_eq!(session.expires_at, Some(1_791_494_100));
        assert!(matches!(
            session_from_bundle("{}"),
            Err(TokenError::Protocol(_))
        ));
        assert!(matches!(
            session_from_bundle("not json"),
            Err(TokenError::Protocol(_))
        ));
    }

    /// A transport that answers from a list and records the calls.
    struct Fake {
        calls: Mutex<Vec<HttpCall>>,
        answers: Mutex<Vec<HttpReply>>,
    }

    impl Transport for Fake {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            self.calls.lock().unwrap().push(call.clone());
            let mut answers = self.answers.lock().unwrap();
            if answers.is_empty() {
                return Err(String::from("connection refused"));
            }
            Ok(answers.remove(0))
        }
    }

    fn reply(status: u16, body: &str) -> HttpReply {
        HttpReply {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn the_token_server_client_signs_up_and_refreshes_with_the_drive_token() {
        let fake = Fake {
            calls: Mutex::new(Vec::new()),
            answers: Mutex::new(vec![
                reply(201, &bundle("dt_f.0.first", "2026-10-08T21:15:00Z")),
                reply(200, &bundle("dt_f.1.second", "2026-10-09T09:15:00Z")),
                reply(
                    401,
                    r#"{"error": "token_reuse", "message": "an old token was reused"}"#,
                ),
            ]),
        };
        let server = TokenServer::new("http://127.0.0.1:8081/", &fake).unwrap();
        let created = server.create_drive("AzMail").unwrap();
        assert_eq!(created.drive_token, "dt_f.0.first");
        let refreshed = server
            .refresh_session("d_42", &created.drive_token)
            .unwrap();
        assert_eq!(
            refreshed.drive_token, "dt_f.1.second",
            "a new token with every refresh"
        );
        let refused = server
            .refresh_session("d_42", &created.drive_token)
            .unwrap_err();
        assert!(
            matches!(&refused, TokenError::SignIn(why) if why.contains("token_reuse")),
            "{refused:?}"
        );
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls[0].method, Method::Post);
        assert_eq!(calls[0].url, "http://127.0.0.1:8081/v1/drives");
        assert!(String::from_utf8_lossy(&calls[0].body).contains("AzMail"));
        assert_eq!(
            calls[1].url,
            "http://127.0.0.1:8081/v1/drives/d_42/credentials"
        );
        let auth = calls[1]
            .headers
            .iter()
            .find(|(n, _)| n == "authorization")
            .map(|(_, v)| v.as_str());
        assert_eq!(auth, Some("Bearer dt_f.0.first"));
        assert!(
            calls[0].headers.iter().all(|(n, _)| n != "authorization"),
            "a sign-up needs none"
        );
        drop(calls);
        assert!(matches!(
            server.refresh_session("d_42", ""),
            Err(TokenError::SignIn(_))
        ));
        assert!(
            matches!(
                server.refresh_session("d_42", "dt"),
                Err(TokenError::Connect(_))
            ),
            "no answers left"
        );
    }

    #[test]
    fn a_new_drive_is_asked_for_in_the_tier_every_azlin_app_signs_up_with() {
        let fake = Fake {
            calls: Mutex::new(Vec::new()),
            answers: Mutex::new(vec![reply(
                201,
                &bundle("dt_f.0.first", "2026-10-08T21:15:00Z"),
            )]),
        };
        let server = TokenServer::new("http://127.0.0.1:8081", &fake).unwrap();
        server.create_drive("AzMail").unwrap();
        let calls = fake.calls.lock().unwrap();
        let body: Value = serde_json::from_slice(&calls[0].body).unwrap();
        assert_eq!(body["name"], "AzMail");
        assert_eq!(body["tier"], "100GB", "{body}");
    }

    #[test]
    fn a_token_server_is_https_or_on_this_computer() {
        let fake = Fake {
            calls: Mutex::new(Vec::new()),
            answers: Mutex::new(Vec::new()),
        };
        assert!(TokenServer::new("https://token.example", &fake).is_ok());
        assert!(TokenServer::new("http://127.0.0.1:8081", &fake).is_ok());
        assert!(TokenServer::new("http://localhost:8081", &fake).is_ok());
        assert!(TokenServer::new("http://[::1]:8081", &fake).is_ok());
        assert!(matches!(
            TokenServer::new("http://token.example", &fake),
            Err(TokenError::Config(_))
        ));
        assert!(matches!(
            TokenServer::new("ftp://token.example", &fake),
            Err(TokenError::Config(_))
        ));
        assert!(matches!(
            TokenServer::new("", &fake),
            Err(TokenError::Config(_))
        ));
        assert_eq!(url_host("https://user@token.example"), None);
        assert_eq!(
            url_host("http://127.0.0.1:8081/v1"),
            Some(("127.0.0.1", false))
        );
    }

    #[test]
    fn the_endpoints_come_from_the_shared_config_then_the_environment_then_the_switches() {
        let config = r#"{"currentTheme": "flora", "endpoints": {"token": " http://127.0.0.1:8081 ", "s3": ""}}"#;
        let none = |_: &str| -> Option<String> { None };
        let from_config = Endpoints::resolve(Some(config), &none, &Endpoints::default());
        assert_eq!(
            from_config.token_url.as_deref(),
            Some("http://127.0.0.1:8081")
        );
        assert_eq!(from_config.s3_url, None, "an empty value says nothing");
        let env =
            |name: &str| (name == TOKEN_URL_VAR).then(|| String::from("http://127.0.0.1:18081"));
        let from_env = Endpoints::resolve(Some(config), &env, &Endpoints::default());
        assert_eq!(
            from_env.token_url.as_deref(),
            Some("http://127.0.0.1:18081")
        );
        let flags = Endpoints {
            token_url: Some(String::from("http://127.0.0.1:28081")),
            s3_url: Some(String::from("http://127.0.0.1:29000")),
        };
        let from_flags = Endpoints::resolve(Some(config), &env, &flags);
        assert_eq!(from_flags, flags);
        assert_eq!(
            Endpoints::resolve(None, &none, &Endpoints::default()),
            Endpoints::default()
        );
        assert_eq!(endpoints_from_config("not json"), Endpoints::default());
        assert_eq!(
            endpoints_from_config(r#"{"endpoints": {"tokenUrl": "https://t.example"}}"#)
                .token_url
                .as_deref(),
            Some("https://t.example")
        );
        // The account's own token server wins over this run's.
        assert_eq!(
            from_config.token_url_for("https://own.example").as_deref(),
            Some("https://own.example")
        );
        assert_eq!(
            from_config.token_url_for(" ").as_deref(),
            Some("http://127.0.0.1:8081")
        );
    }

    #[test]
    fn the_well_known_folders_have_their_names_by_role() {
        assert_eq!(well_known_name(Role::Spam), Some("Spam"));
        assert_eq!(well_known_name(Role::Inbox), Some("Inbox"));
        assert_eq!(well_known_name(Role::Other), None);
        assert_eq!(well_known_name(Role::All), None);
    }
}

/// Two AzMail processes on one Azlin account share its keyring entry: a refresh holds the
/// entry's lock, re-reads it first and spends only the newest drive token (the token server
/// revokes the device for a spent one).
#[cfg(test)]
mod shared_refresh_tests {
    use std::sync::{Arc, Barrier, Mutex};

    use azcloud_kit::{lock::LockDir, shared::SharedKeyring};
    use azul_storage::{
        keyring::{KeyringError, KeyringStore, MemoryKeyring},
        testing::TempDir,
    };

    use super::{refresh_shared, AzlinSession, CloudAccount, TokenError};

    /// The account's keyring entry.
    const KEY: &str = "AzMail/ada@example.org/azlin";
    /// 2026-10-08T21:15:00Z: when the first credentials run out.
    const FIRST_EXPIRES: u64 = 1_791_494_100;

    fn token_of(generation: u64) -> String {
        format!("dt_f.{generation}.t{generation}")
    }

    /// The session of the family's `generation`: its token, credentials 12 hours past the last.
    fn session_of(generation: u64) -> AzlinSession {
        AzlinSession {
            drive_id: String::from("d_42"),
            drive_token: token_of(generation),
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: String::from("d-42"),
            path_style: true,
            access_key_id: format!("AZT{generation}"),
            secret_access_key: String::from("sk"),
            session_token: Some(String::from("st")),
            expires_at: Some(FIRST_EXPIRES + generation * 12 * 3600),
        }
    }

    /// The token server's side of one token family, as the real one keeps it: the current token
    /// refreshes (and is spent), a spent one revokes the whole family.
    #[derive(Default)]
    struct Family {
        generation: u64,
        revoked: bool,
        refreshes: usize,
    }

    struct Rotating(Mutex<Family>);

    impl CloudAccount for Rotating {
        fn create_drive(&self, _name: &str) -> Result<AzlinSession, TokenError> {
            Err(TokenError::Config(String::from("no sign-ups here")))
        }

        fn refresh_session(
            &self,
            drive_id: &str,
            drive_token: &str,
        ) -> Result<AzlinSession, TokenError> {
            let mut family = self.0.lock().unwrap();
            if family.revoked || drive_id != "d_42" || drive_token != token_of(family.generation)
            {
                family.revoked = true;
                return Err(TokenError::SignIn(String::from(
                    "an old token was reused, token_reuse",
                )));
            }
            family.generation += 1;
            family.refreshes += 1;
            Ok(session_of(family.generation))
        }
    }

    fn token_kept(keyring: &MemoryKeyring) -> String {
        AzlinSession::from_secret(&keyring.get(KEY).unwrap().unwrap(), "d_42").drive_token
    }

    #[test]
    fn two_azmail_processes_on_one_account_race_a_refresh_and_neither_ends_with_a_spent_token() {
        let dir = TempDir::new("azmail-race");
        let account = Arc::new(Rotating(Mutex::new(Family::default())));
        let keyring = Arc::new(MemoryKeyring::new());
        keyring.set(KEY, &session_of(0).to_secret()).unwrap();
        // Two processes: each read the session when it started.
        let held: Vec<Arc<Mutex<AzlinSession>>> = (0..2)
            .map(|_| Arc::new(Mutex::new(session_of(0))))
            .collect();
        for round in 1..=3u64 {
            // Both Send / Receive find the credentials running out at the same moment.
            let now = FIRST_EXPIRES + (round - 1) * 12 * 3600 - 600;
            let start = Arc::new(Barrier::new(2));
            let processes: Vec<_> = held
                .iter()
                .map(|held| {
                    let held = held.clone();
                    let start = start.clone();
                    let account = account.clone();
                    // Each its own handle of the keyring and its own locks over one folder.
                    let shared = SharedKeyring::new(keyring.clone(), LockDir::new(dir.path()));
                    std::thread::spawn(move || {
                        let mine = held.lock().unwrap().clone();
                        assert!(mine.needs_refresh(now));
                        start.wait();
                        let refreshed =
                            refresh_shared(&*account, &shared, KEY, &mine, None, now).unwrap();
                        assert_eq!(refreshed.saved, Ok(()));
                        *held.lock().unwrap() = refreshed.session;
                    })
                })
                .collect();
            for process in processes {
                process.join().unwrap();
            }
            {
                let family = account.0.lock().unwrap();
                assert!(!family.revoked, "round {round}: a spent token was sent again");
                assert_eq!(
                    family.refreshes,
                    usize::try_from(round).unwrap(),
                    "round {round}: ONE refresh for both processes"
                );
            }
            for (process, session) in held.iter().enumerate() {
                assert_eq!(
                    session.lock().unwrap().drive_token,
                    token_of(round),
                    "round {round}: process {process} holds the newest token"
                );
            }
            assert_eq!(token_kept(&keyring), token_of(round), "round {round}: kept");
        }
    }

    #[test]
    fn a_session_another_azmail_refreshed_is_read_from_the_keyring_instead_of_spending_the_token(
    ) {
        let dir = TempDir::new("azmail-race");
        let account = Rotating(Mutex::new(Family {
            generation: 1,
            ..Family::default()
        }));
        let keyring = Arc::new(MemoryKeyring::new());
        keyring.set(KEY, &session_of(1).to_secret()).unwrap();
        let shared = SharedKeyring::new(keyring.clone(), LockDir::new(dir.path()));
        let now = FIRST_EXPIRES - 600;
        // Credentials running out: the other process's fresh session is taken as it is.
        let taken = refresh_shared(&account, &shared, KEY, &session_of(0), None, now).unwrap();
        assert!(taken.adopted);
        assert_eq!(taken.session.drive_token, token_of(1));
        // The drive refused this process's key: the other process's (another key) is taken.
        let taken =
            refresh_shared(&account, &shared, KEY, &session_of(0), Some("AZT0"), now).unwrap();
        assert!(taken.adopted);
        assert_eq!(account.0.lock().unwrap().refreshes, 0, "no token spent");
        // The drive refused the keyring's key too: ITS token (the newest) is the one spent.
        let fresh =
            refresh_shared(&account, &shared, KEY, &session_of(0), Some("AZT1"), now).unwrap();
        assert!(!fresh.adopted);
        assert_eq!(fresh.session.drive_token, token_of(2));
        assert!(!account.0.lock().unwrap().revoked);
        assert_eq!(token_kept(&keyring), token_of(2));
    }

    /// A keyring that takes nothing (none on this system).
    struct NoKeyring;

    impl KeyringStore for NoKeyring {
        fn get(&self, _: &str) -> Result<Option<String>, KeyringError> {
            Err(KeyringError::Unavailable)
        }
        fn set(&self, _: &str, _: &str) -> Result<(), KeyringError> {
            Err(KeyringError::Unavailable)
        }
        fn delete(&self, _: &str) -> Result<(), KeyringError> {
            Err(KeyringError::Unavailable)
        }
    }

    #[test]
    fn a_refreshed_session_the_keyring_does_not_take_comes_back_with_why() {
        let dir = TempDir::new("azmail-race");
        let account = Rotating(Mutex::new(Family::default()));
        let shared = SharedKeyring::new(Arc::new(NoKeyring), LockDir::new(dir.path()));
        let fresh = refresh_shared(
            &account,
            &shared,
            KEY,
            &session_of(0),
            None,
            FIRST_EXPIRES - 600,
        )
        .unwrap();
        assert_eq!(fresh.session.drive_token, token_of(1), "Send / Receive goes on with it");
        let why = fresh.saved.unwrap_err();
        assert!(why.contains("no keyring"), "{why}");
    }

    #[test]
    fn a_typed_drive_token_replaces_the_kept_session_under_the_lock_and_is_what_gets_spent() {
        let dir = TempDir::new("azmail-race");
        let keyring = Arc::new(MemoryKeyring::new());
        // An old, revoked session in the keyring; the user types the drive token of a new token
        // family into the account's settings.
        keyring.set(KEY, &session_of(7).to_secret()).unwrap();
        let shared = SharedKeyring::new(keyring.clone(), LockDir::new(dir.path()));
        let typed = AzlinSession::with_token("d_42", &token_of(0));
        super::store_shared(&shared, KEY, &typed).unwrap();
        assert_eq!(token_kept(&keyring), token_of(0), "the typed token replaced the old one");
        // Send / Receive spends the typed token, not the one it replaced.
        let account = Rotating(Mutex::new(Family::default()));
        let fresh =
            refresh_shared(&account, &shared, KEY, &typed, None, FIRST_EXPIRES - 600).unwrap();
        assert!(!fresh.adopted);
        assert_eq!(fresh.session.drive_token, token_of(1));
        assert!(!account.0.lock().unwrap().revoked);
        assert_eq!(token_kept(&keyring), token_of(1));
        let refused = SharedKeyring::new(Arc::new(NoKeyring), LockDir::new(dir.path()));
        let why = super::store_shared(&refused, KEY, &typed).unwrap_err();
        assert!(why.contains("no keyring"), "{why}");
    }
}
