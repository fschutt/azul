//! A drive this device holds: signup, joining from another device, the
//! credentials refreshed before they expire, the node list re-read on every
//! refresh, and the drive's emergency calls (lockdown, restore).
//!
//! What a grant answer holds goes to three places of the state folder: the
//! drive entry to `drives.json` in azul-storage's format with `{"type":
//! "keyring"}` (the credentials this account keeps current under that entry's
//! keyring name), the Azlin side (node list, expiry, the token server it came
//! from) to `azlin.json`, the credentials and the drive token to the secrets
//! file.
//!
//! Every device has its OWN token family. A drive token rotates on every
//! refresh and the server revokes the whole family when a spent one comes
//! back, so two devices sharing one token would lock each other out. A second
//! device therefore joins with a code from `invite`: a new member family
//! (`POST /v1/drives/{id}/members`), exchanged once by the joining device.
//! Refreshes take a lock in the state folder, so two processes of one device
//! never spend the same token.
//!
//! Blocking, through azul-storage's `Transport` (a new one from the factory for
//! every call to the token server): call it from an azul `Thread`.

use std::{path::Path, time::Duration};

use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation, DrivesFile},
    Credentials,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    bundle::DriveBundle,
    drive::TransportFactory,
    error::{fail, CloudError, CloudResult, Context},
    now, parse_rfc3339, rfc3339,
    secrets::{credentials_entry, drive_token_entry},
    state::{read_json, write_json, StateDir},
    token::{check_id, TokenError, TokenServer},
};

/// The `format` of `azlin.json`.
pub const ACCOUNT_FORMAT: &str = "azcloud.account";
/// Credentials are renewed when less than this is left (12-hour credentials,
/// refreshed 6 hours early).
pub const REFRESH_BEFORE_SECS: i64 = 6 * 3600;
/// What a join code starts with.
pub const JOIN_PREFIX: &str = "azlin-join:";
/// How long a refresh waits for another process's refresh.
const REFRESH_LOCK_WAIT: Duration = Duration::from_secs(60);

/// The Azlin side of one drive (no secrets).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriveRecord {
    /// `d_...`, the token server's id.
    pub id: String,
    /// What the user called it.
    pub name: String,
    /// The bucket (`d-...`).
    pub bucket: String,
    /// The block endpoint the token server handed out.
    pub endpoint: String,
    pub region: String,
    pub path_style: bool,
    /// The token server it came from (a later run may be configured with
    /// another one: `azcloud status` says so).
    pub token_url: String,
    /// This device's member name (`owner` for the device that signed up).
    pub member: String,
    /// When the credentials expire (seconds since 1970; 0 = a long-lived key).
    pub expires_at: i64,
    /// When they were fetched.
    pub refreshed_at: i64,
    /// The node list of the last refresh (ordered by the server).
    #[serde(default)]
    pub nodes: Vec<Value>,
    /// The failover URLs of the last refresh.
    #[serde(default)]
    pub failover: Vec<String>,
    #[serde(default)]
    pub quota_bytes: Option<i64>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub tier: Option<String>,
}

impl DriveRecord {
    /// The direct node URLs and the failover list, without duplicates.
    #[must_use]
    pub fn node_urls(&self) -> Vec<String> {
        crate::bundle::node_urls(&self.nodes, &self.failover)
    }
}

/// `azlin.json`: the drives of this device and the current one.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountFile {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default)]
    pub drives: Vec<DriveRecord>,
}

impl AccountFile {
    /// Reads the file; a missing one has no drives.
    ///
    /// # Errors
    ///
    /// When it cannot be read or is no account file.
    pub fn load(path: &Path) -> CloudResult<AccountFile> {
        let file = read_json::<AccountFile>(path)?.unwrap_or_else(|| AccountFile {
            format: ACCOUNT_FORMAT.to_string(),
            version: 1,
            current: None,
            drives: Vec::new(),
        });
        if file.format != ACCOUNT_FORMAT {
            fail!(
                "{} is not an azcloud account file (format {:?})",
                path.display(),
                file.format
            );
        }
        Ok(file)
    }

    /// Writes the file.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(&self, path: &Path) -> CloudResult<()> {
        write_json(path, self, false)
    }

    /// The drive `id`, else the current one, else the only one.
    #[must_use]
    pub fn get(&self, id: Option<&str>) -> Option<&DriveRecord> {
        match id.or(self.current.as_deref()) {
            Some(id) => self.drives.iter().find(|d| d.id == id),
            None if self.drives.len() == 1 => self.drives.first(),
            None => None,
        }
    }

    /// Adds `record` (or replaces the drive with its id) and makes it the
    /// current one.
    pub fn put(&mut self, record: DriveRecord) {
        self.current = Some(record.id.clone());
        match self.drives.iter_mut().find(|d| d.id == record.id) {
            Some(existing) => *existing = record,
            None => self.drives.push(record),
        }
    }
}

/// What a grant answer (signup, credentials, lockdown) gives, checked.
pub struct Grant {
    pub record: DriveRecord,
    pub credentials: Credentials,
    pub drive_token: String,
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grant")
            .field("record", &self.record)
            .field("credentials", &self.credentials)
            .field("drive_token", &"<hidden>")
            .finish()
    }
}

/// Reads a grant answer of the token server at `token_url` for this device's
/// member `member`.
///
/// # Errors
///
/// When the answer is no drive bundle (a drive with an S3 location, credentials, a drive
/// token) or names a drive id that could change a URL. The parser's message is not passed on
/// (it could quote the credentials).
pub fn read_grant(v: &Value, token_url: &str, member: &str, now: i64) -> CloudResult<Grant> {
    grant_of(&DriveBundle::from_value(v)?, token_url, member, now)
}

/// The grant a drive bundle of the token server at `token_url` is for this device's member
/// `member`.
///
/// # Errors
///
/// When the bundle names a drive id that could change a URL, or no bucket or endpoint.
pub fn grant_of(
    bundle: &DriveBundle,
    token_url: &str,
    member: &str,
    now: i64,
) -> CloudResult<Grant> {
    let id = check_id(bundle.drive_id())?.to_string();
    let Some(config) = bundle.entry.s3_config() else {
        fail!("the grant for {id} names no bucket");
    };
    if config.bucket.trim().is_empty() {
        fail!("the grant for {id} names no bucket");
    }
    if config.endpoint.trim().is_empty() {
        fail!("the grant for {id} names no S3 endpoint");
    }
    let creds = &bundle.credentials;
    let record = DriveRecord {
        name: bundle.entry.name.clone(),
        bucket: config.bucket,
        endpoint: config.endpoint,
        region: config.region,
        path_style: config.path_style,
        token_url: token_url.to_string(),
        member: member.to_string(),
        expires_at: creds
            .expires_at
            .and_then(|at| i64::try_from(at).ok())
            .unwrap_or(0),
        refreshed_at: now,
        nodes: bundle.nodes.clone(),
        failover: bundle.failover.clone(),
        quota_bytes: bundle.quota_bytes.and_then(|q| i64::try_from(q).ok()),
        read_only: bundle.read_only,
        tier: bundle.tier.clone(),
        id,
    };
    let mut credentials = Credentials::new(&creds.access_key_id, &creds.secret_access_key);
    if let Some(token) = creds.session_token.as_deref().filter(|t| !t.is_empty()) {
        credentials = credentials.with_session_token(token);
    }
    Ok(Grant {
        record,
        credentials,
        drive_token: bundle.drive_token.clone(),
    })
}

/// Stores a grant in the state folder: the drive token first (the server
/// spent the old one), then the credentials, the drive entry and the record.
///
/// # Errors
///
/// When a file cannot be written.
pub fn store_grant(state: &StateDir, grant: &Grant) -> CloudResult<()> {
    let secrets = state.secrets();
    let id = &grant.record.id;
    secrets.set(&drive_token_entry(id), &grant.drive_token)?;
    secrets.set(
        &credentials_entry(id),
        &grant.credentials.to_keyring_secret(),
    )?;
    let drives_path = state.drives_file();
    let mut drives = DrivesFile::load(&drives_path)
        .map_err(|e| CloudError::Drive(e).context(drives_path.display()))?;
    drives.add(DriveEntry {
        id: id.clone(),
        name: grant.record.name.clone(),
        location: DriveLocation::S3 {
            endpoint: grant.record.endpoint.clone(),
            region: grant.record.region.clone(),
            bucket: grant.record.bucket.clone(),
            path_style: grant.record.path_style,
            auth: DriveAuth::Keyring,
        },
    });
    drives
        .save(&drives_path)
        .map_err(|e| CloudError::Drive(e).context(drives_path.display()))?;
    let account_path = state.account_file();
    let mut file = AccountFile::load(&account_path)?;
    file.put(grant.record.clone());
    file.save(&account_path)
}

/// URL-safe base64 without padding (RFC 4648 section 5).
fn b64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// [`b64url`] read back (padding accepted); `None` for anything that is not it.
fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    // The bits after the last byte are zero in a code this side wrote.
    (acc == 0).then_some(out)
}

/// What a second device joins a drive with: a member token family of its
/// own. It holds a secret (the family's first token): pass it by a file, not
/// on a command line others can read.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinCode {
    pub drive_id: String,
    pub drive_token: String,
    pub member: String,
    pub name: String,
    /// The token server the code came from (the joining device uses its own
    /// configured one, and says so when they differ).
    pub token_url: String,
    /// For an encrypted drive: the secret of the one-time key the inviting device
    /// sealed the drive key to (64 hex digits; azul-storage's `crypto::device`
    /// invite). With it the joining device gets the drive key once. A secret too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_seal: Option<String>,
}

impl std::fmt::Debug for JoinCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinCode")
            .field("drive_id", &self.drive_id)
            .field("drive_token", &"<hidden>")
            .field("member", &self.member)
            .field("name", &self.name)
            .field("token_url", &self.token_url)
            .field("key_seal", &self.key_seal.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}

impl JoinCode {
    /// `azlin-join:<base64url of the JSON>`.
    #[must_use]
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        format!("{JOIN_PREFIX}{}", b64url(&json))
    }

    /// Reads [`JoinCode::encode`] back.
    ///
    /// # Errors
    ///
    /// When the text is no join code.
    pub fn decode(text: &str) -> CloudResult<JoinCode> {
        let Some(body) = text.trim().strip_prefix(JOIN_PREFIX) else {
            fail!("not a join code (one starts with {JOIN_PREFIX})");
        };
        let Some(bytes) = b64url_decode(body) else {
            fail!("the join code is damaged (not base64url)");
        };
        let code: JoinCode = serde_json::from_slice(&bytes)
            .map_err(|_| CloudError::failed("the join code is damaged (not a code's fields)"))?;
        check_id(&code.drive_id)?;
        if code.drive_token.is_empty() {
            fail!("the join code has no drive token");
        }
        Ok(code)
    }
}

/// A token server's address as the account keeps it: trimmed, without a trailing slash.
fn base_of(token_url: &str) -> String {
    token_url.trim().trim_end_matches('/').to_string()
}

/// Runs `call` against the token server at `base`, over a new transport of `transports`.
fn with_server<T>(
    base: &str,
    transports: &TransportFactory,
    call: impl FnOnce(&TokenServer<'_>) -> Result<T, TokenError>,
) -> CloudResult<T> {
    let transport = transports();
    let server = TokenServer::new(base, transport.as_ref())?;
    Ok(call(&server)?)
}

/// A drive this device holds.
pub struct Account {
    state: StateDir,
    token_url: String,
    transports: TransportFactory,
    record: DriveRecord,
    /// The S3 endpoint this run asks instead of the drive's (`--s3-url`), for the calls that
    /// reach the bucket directly ([`Account::with_s3_endpoint`]).
    s3_endpoint: Option<String>,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("state", &self.state)
            .field("token_url", &self.token_url)
            .field("record", &self.record)
            .finish_non_exhaustive()
    }
}

impl Account {
    /// Signs up for a drive of `tier` at the token server `token_url`
    /// (`POST /v1/drives`; development servers) and keeps it in `state`.
    ///
    /// # Errors
    ///
    /// The server's refusal, no answer, or a file that cannot be written.
    pub fn signup(
        state: &StateDir,
        token_url: &str,
        transports: TransportFactory,
        tier: &str,
        name: &str,
    ) -> CloudResult<Account> {
        let base = base_of(token_url);
        let bundle = with_server(&base, &transports, |server| {
            server.create_dev_drive(name, tier)
        })?;
        // The server names the first family's member "owner".
        let grant = grant_of(&bundle, &base, "owner", now())?;
        store_grant(state, &grant)?;
        Ok(Account {
            state: state.clone(),
            token_url: base,
            transports,
            record: grant.record,
            s3_endpoint: None,
        })
    }

    /// Joins the drive of `code` at the token server `token_url`: the code's
    /// token is exchanged once for this device's credentials and its next
    /// token.
    ///
    /// # Errors
    ///
    /// The server's refusal (a spent code is a 401), or a file that cannot be
    /// written.
    pub fn join(
        state: &StateDir,
        token_url: &str,
        transports: TransportFactory,
        code: &JoinCode,
    ) -> CloudResult<Account> {
        let base = base_of(token_url);
        let bundle = with_server(&base, &transports, |server| {
            server.refresh_named(&code.drive_id, &code.drive_token, &code.name)
        })?;
        let grant = grant_of(&bundle, &base, &code.member, now())?;
        if grant.record.id != code.drive_id {
            fail!(
                "the token server answered for drive {} instead of {}",
                grant.record.id,
                code.drive_id
            );
        }
        store_grant(state, &grant)?;
        Ok(Account {
            state: state.clone(),
            token_url: base,
            transports,
            record: grant.record,
            s3_endpoint: None,
        })
    }

    /// The drive `drive` (else the current one) of `state`, talking to the
    /// token server `token_url` - the configured one, which may differ from
    /// the one the drive came from ([`Account::token_url_moved`]). Sends nothing.
    ///
    /// # Errors
    ///
    /// When the state folder holds no such drive.
    pub fn open(
        state: &StateDir,
        token_url: &str,
        transports: TransportFactory,
        drive: Option<&str>,
    ) -> CloudResult<Account> {
        let file = AccountFile::load(&state.account_file())?;
        let record = file.get(drive).cloned().ok_or_else(|| match drive {
            Some(id) => CloudError::failed(format!(
                "this device has no drive {id} (azcloud signup or join)"
            )),
            None => CloudError::failed(
                "this device has no drive yet: azcloud signup, or azcloud join <code> with a \
                 code from `azcloud invite` on a device that has one",
            ),
        })?;
        Ok(Account {
            state: state.clone(),
            token_url: base_of(token_url),
            transports,
            record,
            s3_endpoint: None,
        })
    }

    #[must_use]
    pub fn record(&self) -> &DriveRecord {
        &self.record
    }

    /// Reaches the bucket at `endpoint` instead of the drive's own (a configured `--s3-url`;
    /// `None` keeps the drive's).
    #[must_use]
    pub fn with_s3_endpoint(mut self, endpoint: Option<&str>) -> Account {
        self.s3_endpoint = endpoint
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty());
        self
    }

    /// The S3 endpoint the bucket is reached at: the configured one, else the drive's.
    #[must_use]
    pub fn s3_endpoint(&self) -> &str {
        self.s3_endpoint
            .as_deref()
            .unwrap_or(self.record.endpoint.as_str())
    }

    #[must_use]
    pub fn state(&self) -> &StateDir {
        &self.state
    }

    /// The token server this account talks to (the configured one).
    #[must_use]
    pub fn token_url(&self) -> &str {
        &self.token_url
    }

    /// The transports the account's calls go through (also the bucket's, for a drive of it).
    #[must_use]
    pub fn transports(&self) -> &TransportFactory {
        &self.transports
    }

    /// `(the drive's, the configured)` token server when they differ.
    #[must_use]
    pub fn token_url_moved(&self) -> Option<(&str, &str)> {
        (self.record.token_url != self.token_url)
            .then(|| (self.record.token_url.as_str(), self.token_url.as_str()))
    }

    /// Runs `call` against the configured token server.
    fn server<T>(
        &self,
        call: impl FnOnce(&TokenServer<'_>) -> Result<T, TokenError>,
    ) -> CloudResult<T> {
        with_server(&self.token_url, &self.transports, call)
    }

    /// This device's drive token.
    ///
    /// # Errors
    ///
    /// When the secrets file has none for the drive.
    pub fn drive_token(&self) -> CloudResult<String> {
        let secrets = self.state.secrets();
        secrets
            .get(&drive_token_entry(&self.record.id))?
            .ok_or_else(|| {
                CloudError::failed(format!(
                    "the drive token of {} is missing from {}",
                    self.record.id,
                    secrets.path().display()
                ))
            })
    }

    /// The current credentials.
    ///
    /// # Errors
    ///
    /// When the secrets file has none for the drive.
    pub fn credentials(&self) -> CloudResult<Credentials> {
        let secrets = self.state.secrets();
        let secret = secrets
            .get(&credentials_entry(&self.record.id))?
            .ok_or_else(|| {
                CloudError::failed(format!(
                    "the credentials of {} are missing from {}",
                    self.record.id,
                    secrets.path().display()
                ))
            })?;
        Ok(Credentials::from_keyring_secret(&secret)?)
    }

    /// Whether the credentials should be renewed now: less than six hours
    /// left (a long-lived key, `expires_at` 0, never is).
    #[must_use]
    pub fn needs_refresh(&self, now: i64) -> bool {
        self.record.expires_at > 0 && self.record.expires_at - now < REFRESH_BEFORE_SECS
    }

    /// Renews the credentials when [`Account::needs_refresh`]; whether it
    /// did.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub fn ensure_fresh(&mut self) -> CloudResult<bool> {
        if !self.needs_refresh(now()) {
            return Ok(false);
        }
        self.refresh_locked(false)
    }

    /// Renews the credentials (and the node list) now.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub fn refresh(&mut self) -> CloudResult<()> {
        self.refresh_locked(true).map(|_| ())
    }

    fn refresh_locked(&mut self, force: bool) -> CloudResult<bool> {
        let _lock = self.state.lock("refresh", REFRESH_LOCK_WAIT)?;
        // Another process may have refreshed while this one waited for the
        // lock: its token is the one to spend now.
        self.reload()?;
        if !force && !self.needs_refresh(now()) {
            return Ok(false);
        }
        let token = self.drive_token()?;
        let bundle = self.server(|server| {
            server.refresh_named(&self.record.id, &token, &self.record.name)
        })?;
        let mut grant = grant_of(&bundle, &self.token_url, &self.record.member, now())?;
        grant.record.name = self.record.name.clone();
        store_grant(&self.state, &grant)?;
        self.record = grant.record;
        Ok(true)
    }

    fn reload(&mut self) -> CloudResult<()> {
        let file = AccountFile::load(&self.state.account_file())?;
        if let Some(record) = file.get(Some(&self.record.id)) {
            self.record = record.clone();
        }
        Ok(())
    }

    /// A join code for another device: a new member family of this drive. Like every call
    /// that grants (the lockdown and its cancel, a restore), it is sent under the refresh lock
    /// with the newest drive token: the token server takes no other for it.
    ///
    /// # Errors
    ///
    /// The refresh lock held too long by another azcloud, the server's refusal or no answer.
    pub fn invite(&self, member: Option<&str>) -> CloudResult<JoinCode> {
        // A grant: the current token only - behind any refresh another azcloud runs.
        let _lock = self.state.lock("refresh", REFRESH_LOCK_WAIT)?;
        let token = self.drive_token()?;
        let answer = self.server(|server| server.add_member(&self.record.id, &token, member))?;
        let drive_token = answer["drive_token"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if drive_token.is_empty() {
            fail!("the token server's member answer has no drive token");
        }
        Ok(JoinCode {
            drive_id: self.record.id.clone(),
            drive_token,
            member: answer["member"].as_str().unwrap_or("member").to_string(),
            name: self.record.name.clone(),
            token_url: self.token_url.clone(),
            key_seal: None,
        })
    }

    /// The token server's view of the drive (tier, quota, members, status).
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub fn info(&self) -> CloudResult<Value> {
        let token = self.drive_token()?;
        self.server(|server| server.info(&self.record.id, &token))
    }

    /// Locks the drive down from this device: every other device, key and
    /// public link loses access at once; this device continues with the new
    /// grant the answer carries. What is returned holds no secret.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub fn lockdown(&mut self) -> CloudResult<Value> {
        let _lock = self.state.lock("refresh", REFRESH_LOCK_WAIT)?;
        let token = self.drive_token()?;
        let answer = self.server(|server| server.lockdown(&self.record.id, &token))?;
        let mut grant = read_grant(&answer, &self.token_url, &self.record.member, now())
            .context("the lockdown answer (this device's new grant)")?;
        grant.record.name = self.record.name.clone();
        store_grant(&self.state, &grant)?;
        self.record = grant.record;
        Ok(json!({
            "locked_down": true,
            "drive": self.record.id,
            "credentials_expire": rfc3339(self.record.expires_at),
        }))
    }

    /// Cancels a pending recovery-key lockdown.
    ///
    /// # Errors
    ///
    /// The server's refusal (none pending) or no answer.
    pub fn lockdown_cancel(&self) -> CloudResult<Value> {
        let _lock = self.state.lock("refresh", REFRESH_LOCK_WAIT)?;
        let token = self.drive_token()?;
        self.server(|server| server.lockdown_cancel(&self.record.id, &token))
    }

    /// Queues a restore of `prefix` as it was at `as_of` (RFC 3339).
    ///
    /// # Errors
    ///
    /// A time that is no RFC 3339, the server's refusal, or no answer.
    pub fn restore(&self, prefix: &str, as_of: &str) -> CloudResult<Value> {
        if parse_rfc3339(as_of).is_none() {
            fail!("{as_of:?} is no RFC 3339 time (2026-10-08T09:00:00Z)");
        }
        let _lock = self.state.lock("refresh", REFRESH_LOCK_WAIT)?;
        let token = self.drive_token()?;
        self.server(|server| server.restore(&self.record.id, &token, prefix, as_of))
    }

    /// A restore's progress.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub fn restore_status(&self, request: &str) -> CloudResult<Value> {
        let token = self.drive_token()?;
        self.server(|server| server.restore_status(&self.record.id, &token, request))
    }
}
