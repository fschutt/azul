//! A drive this device holds: signup, joining from another device, the
//! credentials refreshed before they expire, the node list re-read on every
//! refresh (AZDRIVE-INTEGRATION.md §3, PLAN §13.5).
//!
//! What a grant answer holds goes to three places of the state folder: the
//! drive entry to `drives.json` in azul-storage's format with `{"type":
//! "keyring"}` (the token server says `{"type": "azlin"}`, which azul-storage
//! cannot read yet - its `DriveAuth` knows `keyring` and `access_link`), the
//! Azlin side (node list, expiry, the token server it came from) to
//! `azlin.json`, the credentials and the drive token to the secrets file.
//!
//! Every device has its OWN token family. A drive token rotates on every
//! refresh and the server revokes the whole family when a spent one comes
//! back (§4.9), so two devices sharing one token would lock each other out.
//! A second device therefore joins with a code from `invite`: a new member
//! family (`POST /v1/drives/{id}/members`), exchanged once by the joining
//! device. Refreshes take a lock in the state folder, so two processes of one
//! device never spend the same token.

use std::{path::Path, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use azlin_client::DriveBundle;
use azul_storage::config::{DriveAuth, DriveEntry, DriveLocation, DrivesFile};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    now,
    secrets::{credentials_entry, drive_token_entry},
    state::{read_json, write_json, StateDir},
    token_api::{check_id, TokenApi},
};

/// The `format` of `azlin.json`.
pub const ACCOUNT_FORMAT: &str = "azcloud.account";
/// Credentials are renewed when less than this is left (PLAN: 12-hour
/// credentials, refreshed 6 hours early).
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
    /// The node list of the last refresh (ordered by the server, canary
    /// first for a canary drive).
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
    /// The direct node URLs and the failover list, without duplicates
    /// (azlin-client's `DriveBundle::node_urls`).
    #[must_use]
    pub fn node_urls(&self) -> Vec<String> {
        let mut urls: Vec<String> = Vec::new();
        let from_nodes = self
            .nodes
            .iter()
            .filter_map(|n| n["url"].as_str().or_else(|| n["public_url"].as_str()));
        for url in from_nodes.chain(self.failover.iter().map(String::as_str)) {
            if !url.is_empty() && !urls.iter().any(|u| u == url) {
                urls.push(url.to_string());
            }
        }
        urls
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
    pub fn load(path: &Path) -> Result<AccountFile> {
        let file = read_json::<AccountFile>(path)?.unwrap_or_else(|| AccountFile {
            format: ACCOUNT_FORMAT.to_string(),
            version: 1,
            current: None,
            drives: Vec::new(),
        });
        if file.format != ACCOUNT_FORMAT {
            bail!(
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
    pub fn save(&self, path: &Path) -> Result<()> {
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
    pub credentials: azul_storage::Credentials,
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
/// When the answer lacks the drive id, bucket, endpoint, credentials or the
/// drive token. The parser's message is not passed on (it could quote the
/// credentials).
pub fn read_grant(v: &Value, token_url: &str, member: &str, now: i64) -> Result<Grant> {
    let bundle: DriveBundle = serde_json::from_value(v.clone())
        .map_err(|_| anyhow!("the token server's answer is no drive grant (drive, credentials)"))?;
    let id = bundle.drive_id();
    check_id(&id)?;
    let bucket = bundle.bucket();
    if bucket.is_empty() {
        bail!("the grant for {id} names no bucket");
    }
    let endpoint = bundle.endpoint();
    if endpoint.is_empty() {
        bail!("the grant for {id} names no S3 endpoint");
    }
    let creds = bundle.creds();
    if creds.access_key_id.is_empty() || creds.secret_access_key.is_empty() {
        bail!("the grant for {id} has no credentials");
    }
    if bundle.drive_token.is_empty() {
        bail!("the grant for {id} has no drive token");
    }
    let location = &v["drive"]["location"];
    let failover = bundle
        .failover
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let record = DriveRecord {
        name: v["drive"]["name"]
            .as_str()
            .unwrap_or("Azlin Storage")
            .to_string(),
        bucket,
        endpoint,
        region: location["region"]
            .as_str()
            .unwrap_or("us-east-1")
            .to_string(),
        path_style: location["path_style"].as_bool().unwrap_or(true),
        token_url: token_url.to_string(),
        member: member.to_string(),
        expires_at: creds.expires_at,
        refreshed_at: now,
        nodes: bundle.nodes.clone(),
        failover,
        quota_bytes: v["quota_bytes"].as_i64(),
        read_only: v["read_only"].as_bool().unwrap_or(false),
        tier: v["tier"].as_str().map(String::from),
        id,
    };
    let mut credentials =
        azul_storage::Credentials::new(&creds.access_key_id, &creds.secret_access_key);
    if !creds.session_token.is_empty() {
        credentials = credentials.with_session_token(&creds.session_token);
    }
    Ok(Grant {
        record,
        credentials,
        drive_token: bundle.drive_token,
    })
}

/// Stores a grant in the state folder: the drive token first (the server
/// spent the old one), then the credentials, the drive entry and the record.
///
/// # Errors
///
/// When a file cannot be written.
pub fn store_grant(state: &StateDir, grant: &Grant) -> Result<()> {
    let secrets = state.secrets();
    let id = &grant.record.id;
    secrets.set(&drive_token_entry(id), &grant.drive_token)?;
    secrets.set(
        &credentials_entry(id),
        &grant.credentials.to_keyring_secret(),
    )?;
    let drives_path = state.drives_file();
    let mut drives =
        DrivesFile::load(&drives_path).map_err(|e| anyhow!("{}: {e}", drives_path.display()))?;
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
        .map_err(|e| anyhow!("{}: {e}", drives_path.display()))?;
    let account_path = state.account_file();
    let mut file = AccountFile::load(&account_path)?;
    file.put(grant.record.clone());
    file.save(&account_path)
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
}

impl std::fmt::Debug for JoinCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinCode")
            .field("drive_id", &self.drive_id)
            .field("drive_token", &"<hidden>")
            .field("member", &self.member)
            .field("name", &self.name)
            .field("token_url", &self.token_url)
            .finish()
    }
}

impl JoinCode {
    /// `azlin-join:<base64url of the JSON>`.
    #[must_use]
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        format!("{JOIN_PREFIX}{}", azlin_proto::b64url(&json))
    }

    /// Reads [`JoinCode::encode`] back.
    ///
    /// # Errors
    ///
    /// When the text is no join code.
    pub fn decode(text: &str) -> Result<JoinCode> {
        let body = text
            .trim()
            .strip_prefix(JOIN_PREFIX)
            .ok_or_else(|| anyhow!("not a join code (one starts with {JOIN_PREFIX})"))?;
        let bytes = azlin_proto::b64url_decode(body)
            .ok_or_else(|| anyhow!("the join code is damaged (not base64url)"))?;
        let code: JoinCode = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow!("the join code is damaged (not a code's fields)"))?;
        check_id(&code.drive_id)?;
        if code.drive_token.is_empty() {
            bail!("the join code has no drive token");
        }
        Ok(code)
    }
}

/// A drive this device holds.
#[derive(Debug)]
pub struct Account {
    state: StateDir,
    api: TokenApi,
    record: DriveRecord,
}

impl Account {
    /// Signs up for a drive of `tier` at the token server `token_url`
    /// (`POST /v1/drives`; dev servers) and keeps it in `state`.
    ///
    /// # Errors
    ///
    /// The server's refusal, no answer, or a file that cannot be written.
    pub async fn signup(
        state: &StateDir,
        token_url: &str,
        tier: &str,
        name: &str,
    ) -> Result<Account> {
        let api = TokenApi::new(token_url)?;
        let answer = api.signup(tier, name).await?;
        // The server names the first family's member "owner" (create_drive).
        let grant = read_grant(&answer, api.base(), "owner", now())?;
        store_grant(state, &grant)?;
        Ok(Account {
            state: state.clone(),
            api,
            record: grant.record,
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
    pub async fn join(state: &StateDir, token_url: &str, code: &JoinCode) -> Result<Account> {
        let api = TokenApi::new(token_url)?;
        let answer = api
            .credentials(&code.drive_id, &code.drive_token, &code.name)
            .await?;
        let grant = read_grant(&answer, api.base(), &code.member, now())?;
        if grant.record.id != code.drive_id {
            bail!(
                "the token server answered for drive {} instead of {}",
                grant.record.id,
                code.drive_id
            );
        }
        store_grant(state, &grant)?;
        Ok(Account {
            state: state.clone(),
            api,
            record: grant.record,
        })
    }

    /// The drive `drive` (else the current one) of `state`, talking to the
    /// token server `token_url` - the configured one, which may differ from
    /// the one the drive came from ([`Account::token_url_moved`]).
    ///
    /// # Errors
    ///
    /// When the state folder holds no such drive.
    pub fn open(state: &StateDir, token_url: &str, drive: Option<&str>) -> Result<Account> {
        let file = AccountFile::load(&state.account_file())?;
        let record = file.get(drive).cloned().ok_or_else(|| match drive {
            Some(id) => anyhow!("this device has no drive {id} (azcloud signup or join)"),
            None => anyhow!(
                "this device has no drive yet: azcloud signup, or azcloud join <code> with a \
                 code from `azcloud invite` on a device that has one"
            ),
        })?;
        Ok(Account {
            state: state.clone(),
            api: TokenApi::new(token_url)?,
            record,
        })
    }

    #[must_use]
    pub fn record(&self) -> &DriveRecord {
        &self.record
    }

    #[must_use]
    pub fn state(&self) -> &StateDir {
        &self.state
    }

    #[must_use]
    pub fn api(&self) -> &TokenApi {
        &self.api
    }

    /// `(the drive's, the configured)` token server when they differ.
    #[must_use]
    pub fn token_url_moved(&self) -> Option<(&str, &str)> {
        (self.record.token_url != self.api.base())
            .then(|| (self.record.token_url.as_str(), self.api.base()))
    }

    /// This device's drive token.
    ///
    /// # Errors
    ///
    /// When the secrets file has none for the drive.
    pub fn drive_token(&self) -> Result<String> {
        let secrets = self.state.secrets();
        secrets
            .get(&drive_token_entry(&self.record.id))?
            .ok_or_else(|| {
                anyhow!(
                    "the drive token of {} is missing from {}",
                    self.record.id,
                    secrets.path().display()
                )
            })
    }

    /// The current credentials, as azlin-client signs with them.
    ///
    /// # Errors
    ///
    /// When the secrets file has none for the drive.
    pub fn credentials(&self) -> Result<azlin_proto::creds::Credentials> {
        let secrets = self.state.secrets();
        let secret = secrets
            .get(&credentials_entry(&self.record.id))?
            .ok_or_else(|| {
                anyhow!(
                    "the credentials of {} are missing from {}",
                    self.record.id,
                    secrets.path().display()
                )
            })?;
        let stored =
            azul_storage::Credentials::from_keyring_secret(&secret).map_err(|e| anyhow!("{e}"))?;
        Ok(azlin_proto::creds::Credentials {
            access_key_id: stored.access_key_id,
            secret_access_key: stored.secret_access_key,
            session_token: stored.session_token.unwrap_or_default(),
            expires_at: self.record.expires_at,
        })
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
    pub async fn ensure_fresh(&mut self) -> Result<bool> {
        if !self.needs_refresh(now()) {
            return Ok(false);
        }
        self.refresh_locked(false).await
    }

    /// Renews the credentials (and the node list) now.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub async fn refresh(&mut self) -> Result<()> {
        self.refresh_locked(true).await.map(|_| ())
    }

    async fn refresh_locked(&mut self, force: bool) -> Result<bool> {
        let _lock = self.state.lock("refresh", REFRESH_LOCK_WAIT)?;
        // Another process may have refreshed while this one waited for the
        // lock: its token is the one to spend now.
        self.reload()?;
        if !force && !self.needs_refresh(now()) {
            return Ok(false);
        }
        let token = self.drive_token()?;
        let answer = self
            .api
            .credentials(&self.record.id, &token, &self.record.name)
            .await?;
        let mut grant = read_grant(&answer, self.api.base(), &self.record.member, now())?;
        grant.record.name = self.record.name.clone();
        store_grant(&self.state, &grant)?;
        self.record = grant.record;
        Ok(true)
    }

    fn reload(&mut self) -> Result<()> {
        let file = AccountFile::load(&self.state.account_file())?;
        if let Some(record) = file.get(Some(&self.record.id)) {
            self.record = record.clone();
        }
        Ok(())
    }

    /// A join code for another device: a new member family of this drive.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub async fn invite(&self, member: Option<&str>) -> Result<JoinCode> {
        let token = self.drive_token()?;
        let answer = self.api.add_member(&self.record.id, &token, member).await?;
        let drive_token = answer["drive_token"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if drive_token.is_empty() {
            bail!("the token server's member answer has no drive token");
        }
        Ok(JoinCode {
            drive_id: self.record.id.clone(),
            drive_token,
            member: answer["member"].as_str().unwrap_or("member").to_string(),
            name: self.record.name.clone(),
            token_url: self.api.base().to_string(),
        })
    }

    /// The token server's view of the drive (tier, quota, members, status).
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub async fn info(&self) -> Result<Value> {
        let token = self.drive_token()?;
        self.api.info(&self.record.id, &token).await
    }

    /// Locks the drive down from this device (§18.7): every other device,
    /// key and public link loses access at once; this device continues with
    /// the new grant the answer carries. What is returned holds no secret.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub async fn lockdown(&mut self) -> Result<Value> {
        let _lock = self.state.lock("refresh", REFRESH_LOCK_WAIT)?;
        let token = self.drive_token()?;
        let answer = self.api.lockdown(&self.record.id, &token).await?;
        let mut grant = read_grant(&answer, self.api.base(), &self.record.member, now())
            .context("the lockdown answer (this device's new grant)")?;
        grant.record.name = self.record.name.clone();
        store_grant(&self.state, &grant)?;
        self.record = grant.record;
        Ok(json!({
            "locked_down": true,
            "drive": self.record.id,
            "credentials_expire": azlin_proto::time::rfc3339(self.record.expires_at),
        }))
    }

    /// Cancels a pending recovery-key lockdown.
    ///
    /// # Errors
    ///
    /// The server's refusal (none pending) or no answer.
    pub async fn lockdown_cancel(&self) -> Result<Value> {
        let token = self.drive_token()?;
        self.api.lockdown_cancel(&self.record.id, &token).await
    }

    /// Queues a restore of `prefix` as it was at `as_of` (RFC 3339).
    ///
    /// # Errors
    ///
    /// A time that is no RFC 3339, the server's refusal, or no answer.
    pub async fn restore(&self, prefix: &str, as_of: &str) -> Result<Value> {
        if azlin_proto::time::parse_iso(as_of).is_none() {
            bail!("{as_of:?} is no RFC 3339 time (2026-10-08T09:00:00Z)");
        }
        let token = self.drive_token()?;
        self.api
            .restore(&self.record.id, &token, prefix, as_of)
            .await
    }

    /// A restore's progress.
    ///
    /// # Errors
    ///
    /// The server's refusal or no answer.
    pub async fn restore_status(&self, request: &str) -> Result<Value> {
        let token = self.drive_token()?;
        self.api
            .restore_status(&self.record.id, &token, request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    /// A signup answer in the token server's shape (drives::signup_response).
    fn answer(token: &str) -> Value {
        json!({
            "drive": {
                "id": "d_k3f9",
                "name": "Ann's drive",
                "location": {"kind": "s3", "endpoint": "http://127.0.0.1:9000", "region": "us-east-1",
                             "bucket": "d-k3f9", "path_style": true,
                             "auth": {"type": "azlin", "drive_id": "d_k3f9", "account_url": ""}}
            },
            "credentials": {"access_key_id": "AZTKEY", "secret_access_key": "sesame",
                            "session_token": "st", "expires_at": "2026-10-08T21:00:00Z"},
            "failover": ["http://127.0.0.1:9001", "http://127.0.0.1:9002"],
            "nodes": [{"name": "n1", "url": "http://127.0.0.1:9001", "ready": true}],
            "quota_bytes": 100_000_000_000_i64,
            "read_only": false,
            "drive_token": token,
            "tier": "100GB",
        })
    }

    #[test]
    fn a_grant_is_kept_as_azdrives_drive_entry_the_azlin_record_and_two_secrets() {
        let dir = TempDir::new("azcloud-grant");
        let state = StateDir::open(dir.path()).unwrap();
        let grant = read_grant(&answer("dt_f.0.a"), "http://127.0.0.1:8081", "owner", 100).unwrap();
        assert_eq!(grant.record.id, "d_k3f9");
        assert_eq!(grant.record.endpoint, "http://127.0.0.1:9000");
        assert_eq!(
            grant.record.expires_at,
            azlin_proto::time::parse_iso("2026-10-08T21:00:00Z").unwrap()
        );
        assert_eq!(
            grant.record.node_urls(),
            vec!["http://127.0.0.1:9001", "http://127.0.0.1:9002"]
        );
        assert!(!format!("{grant:?}").contains("sesame"));
        store_grant(&state, &grant).unwrap();

        // AzDrive can read the drives file: the auth is the keyring one it knows.
        let drives = DrivesFile::load(&state.drives_file()).unwrap();
        let entry = drives.get("d_k3f9").expect("the drive entry");
        assert!(entry.needs_keyring());
        assert_eq!(entry.s3_config().unwrap().bucket, "d-k3f9");
        let text = std::fs::read_to_string(state.drives_file()).unwrap();
        assert!(!text.contains("sesame"), "no secret in drives.json");

        let account = Account::open(&state, "http://127.0.0.1:8081", None).unwrap();
        assert_eq!(account.drive_token().unwrap(), "dt_f.0.a");
        let creds = account.credentials().unwrap();
        assert_eq!(creds.access_key_id, "AZTKEY");
        assert_eq!(creds.session_token, "st");
        assert_eq!(account.token_url_moved(), None);
        let moved = Account::open(&state, "http://127.0.0.1:18081", None).unwrap();
        assert_eq!(
            moved.token_url_moved(),
            Some(("http://127.0.0.1:8081", "http://127.0.0.1:18081"))
        );

        // The next grant replaces the token and keeps one entry.
        let next = read_grant(&answer("dt_f.1.b"), "http://127.0.0.1:8081", "owner", 200).unwrap();
        store_grant(&state, &next).unwrap();
        let account = Account::open(&state, "http://127.0.0.1:8081", Some("d_k3f9")).unwrap();
        assert_eq!(account.drive_token().unwrap(), "dt_f.1.b");
        assert_eq!(
            AccountFile::load(&state.account_file())
                .unwrap()
                .drives
                .len(),
            1
        );
    }

    #[test]
    fn an_answer_without_credentials_or_token_is_no_grant_and_the_error_quotes_nothing() {
        let mut v = answer("");
        assert!(read_grant(&v, "u", "owner", 0)
            .unwrap_err()
            .to_string()
            .contains("drive token"));
        v["drive_token"] = json!("dt_x");
        v["credentials"]["secret_access_key"] = json!("");
        assert!(read_grant(&v, "u", "owner", 0).is_err());
        v["drive"] = json!("sesame");
        let err = read_grant(&v, "u", "owner", 0).unwrap_err().to_string();
        assert!(!err.contains("sesame"), "{err}");
        v = answer("dt_x");
        v["drive"]["id"] = json!("../../keys");
        assert!(read_grant(&v, "u", "owner", 0).is_err());
    }

    #[test]
    fn credentials_are_renewed_six_hours_before_they_expire_and_a_long_lived_key_never() {
        let dir = TempDir::new("azcloud-refresh");
        let state = StateDir::open(dir.path()).unwrap();
        let grant = read_grant(&answer("dt_f.0.a"), "http://t", "owner", 0).unwrap();
        let expires = grant.record.expires_at;
        store_grant(&state, &grant).unwrap();
        let account = Account::open(&state, "http://t", None).unwrap();
        assert!(!account.needs_refresh(expires - 7 * 3600));
        assert!(account.needs_refresh(expires - 5 * 3600));
        assert!(account.needs_refresh(expires + 1));
        let mut long = answer("dt_f.0.a");
        long["credentials"] = json!({"access_key_id": "AZK1", "secret_access_key": "s"});
        let grant = read_grant(&long, "http://t", "owner", 0).unwrap();
        assert_eq!(grant.record.expires_at, 0);
    }

    #[test]
    fn a_join_code_round_trips_hides_its_token_and_refuses_what_is_not_one() {
        let code = JoinCode {
            drive_id: String::from("d_k3f9"),
            drive_token: String::from("dt_m.0.sesame"),
            member: String::from("m_laptop"),
            name: String::from("Ann's drive"),
            token_url: String::from("http://127.0.0.1:8081"),
        };
        let text = code.encode();
        assert!(text.starts_with(JOIN_PREFIX));
        assert_eq!(JoinCode::decode(&format!("  {text}\n")).unwrap(), code);
        assert!(!format!("{code:?}").contains("sesame"));
        assert!(JoinCode::decode("dt_m.0.sesame").is_err());
        assert!(JoinCode::decode("azlin-join:%%%").is_err());
        let mut bad = code.clone();
        bad.drive_id = String::from("../x");
        assert!(JoinCode::decode(&bad.encode()).is_err());
    }

    #[test]
    fn the_current_drive_is_the_one_named_else_the_last_added_else_the_only_one() {
        let rec = |id: &str| DriveRecord {
            id: id.to_string(),
            name: String::new(),
            bucket: String::new(),
            endpoint: String::new(),
            region: String::new(),
            path_style: true,
            token_url: String::new(),
            member: String::new(),
            expires_at: 0,
            refreshed_at: 0,
            nodes: vec![],
            failover: vec![],
            quota_bytes: None,
            read_only: false,
            tier: None,
        };
        let mut file = AccountFile::default();
        assert!(file.get(None).is_none());
        file.drives.push(rec("d_a"));
        assert_eq!(file.get(None).map(|d| d.id.as_str()), Some("d_a"));
        file.put(rec("d_b"));
        assert_eq!(file.get(None).map(|d| d.id.as_str()), Some("d_b"));
        assert_eq!(file.get(Some("d_a")).map(|d| d.id.as_str()), Some("d_a"));
        assert!(file.get(Some("d_c")).is_none());
    }
}
