//! The bridge's drive: the user's Azlin drive through a device account of the bridge's own.
//!
//! The bridge is a device like any other (azcloud-kit's [`Account`] in its state folder): it
//! signs up (a development server) or joins a drive with a code from `azcloud invite` / AzDrive,
//! and so holds its OWN token family - sharing AzMail's or AzDrive's drive token would make each
//! refresh spend the other's token, and the token server revokes a family when a spent token
//! comes back. The credentials are renewed under the state folder's lock six hours before they
//! run out ([`Account::ensure_fresh`]), and once more when the bucket refuses them.
//!
//! Every request goes through azcloud-kit's failover, as an app's Azlin drive does: iroh to the
//! nodes first when the bridge is built with libazul (`os-keyring`, `tray`: azul's iroh
//! endpoint, [`AccountDrive::with_iroh`]), then the block endpoint, the nodes the last refresh
//! listed and its failover URLs, each reached at its addresses when its name does not resolve.

use std::sync::{Arc, Mutex};

use azcloud_kit::{
    drive::TransportFactory,
    error::{CloudError, CloudResult},
    failover::{Failover, Node},
    Account, IrohDialer, IrohLane, StateDir,
};
use azul_storage::{
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition, S3Config,
    S3Drive,
};

/// The S3 error codes that mean "these credentials no longer work".
const REFUSED_CREDENTIALS: &[&str] = &[
    "ExpiredToken",
    "InvalidToken",
    "TokenRefreshRequired",
    "InvalidAccessKeyId",
];

fn drive_error(e: &CloudError) -> DriveError {
    if e.is_sign_in() {
        return DriveError::Denied {
            message: e.to_string(),
        };
    }
    match e.root() {
        CloudError::Drive(inner) => inner.clone(),
        _ => DriveError::Transport(e.to_string()),
    }
}

/// The drive of the bridge's device account, refreshing its credentials by itself.
pub struct AccountDrive {
    account: Mutex<Account>,
    transports: TransportFactory,
    /// An S3 endpoint to use instead of the token server's (`--s3-url`).
    s3_override: Option<String>,
    /// The bucket of the current credentials, by their access key.
    bucket: Mutex<Option<(String, Arc<S3Drive>)>>,
    /// Where every request goes (the failover of the drive's block endpoint).
    failover: Mutex<Option<Arc<Failover>>>,
    /// iroh to the nodes first, when the bridge dials it.
    lane: Option<Arc<IrohLane>>,
}

impl std::fmt::Debug for AccountDrive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountDrive").finish_non_exhaustive()
    }
}

impl AccountDrive {
    /// The drive this device holds in `state` (its current one), refreshed at `token_url`.
    /// Sends nothing.
    ///
    /// # Errors
    ///
    /// When the state folder holds no drive (sign up or join first).
    pub fn open(
        state: &StateDir,
        token_url: &str,
        transports: TransportFactory,
        s3_override: Option<String>,
    ) -> CloudResult<AccountDrive> {
        let account = Account::open(state, token_url, transports.clone(), None)?;
        Ok(AccountDrive {
            account: Mutex::new(account),
            transports,
            s3_override: s3_override.filter(|s| !s.trim().is_empty()),
            bucket: Mutex::new(None),
            failover: Mutex::new(None),
            lane: None,
        })
    }

    /// Sends every request over iroh first: to each ready node the node list names with an iroh
    /// id, dialed through `dialer` and relayed through `relay`; HTTPS with the failover is the
    /// fallback.
    #[must_use]
    pub fn with_iroh(mut self, dialer: Arc<dyn IrohDialer>, relay: Option<&str>) -> Self {
        self.lane = Some(Arc::new(IrohLane::new(dialer, relay)));
        self
    }

    /// The failover of `endpoint` (kept while the endpoint stays), with the node list and the
    /// failover URLs of the refresh `record` holds.
    fn failover_for(&self, endpoint: &str, record: &azcloud_kit::account::DriveRecord) -> Arc<Failover> {
        let mut kept = self
            .failover
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let wanted = endpoint.trim().trim_end_matches('/');
        let failover = match kept.as_ref() {
            Some(failover) if failover.block() == wanted => failover.clone(),
            _ => {
                let failover = Arc::new(Failover::new(endpoint));
                failover.set_lane(self.lane.clone());
                *kept = Some(failover.clone());
                failover
            }
        };
        failover.set_nodes(Node::list(&record.nodes));
        failover.set_alternatives(record.failover.clone());
        failover
    }

    /// The drive's id (for people).
    #[must_use]
    pub fn drive_id(&self) -> String {
        self.account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record()
            .id
            .clone()
    }

    /// The bucket with credentials that work now; `force` renews them first.
    fn current(&self, force: bool) -> Result<(Arc<S3Drive>, String), DriveError> {
        let mut account = self
            .account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if force {
            account.refresh().map_err(|e| drive_error(&e))?;
        } else {
            account.ensure_fresh().map_err(|e| drive_error(&e))?;
        }
        let credentials = account.credentials().map_err(|e| drive_error(&e))?;
        let key = credentials.access_key_id.clone();
        let mut bucket = self
            .bucket
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((cached_key, drive)) = bucket.as_ref() {
            if *cached_key == key {
                return Ok((drive.clone(), key));
            }
        }
        let record = account.record();
        let config = S3Config {
            endpoint: self
                .s3_override
                .clone()
                .unwrap_or_else(|| record.endpoint.clone()),
            region: if record.region.trim().is_empty() {
                String::from("us-east-1")
            } else {
                record.region.clone()
            },
            bucket: record.bucket.clone(),
            path_style: record.path_style,
        };
        let failover = self.failover_for(&config.endpoint, &record);
        let drive =
            Arc::new(S3Drive::new(config, credentials, (self.transports)())?.with_router(failover));
        *bucket = Some((key.clone(), drive.clone()));
        Ok((drive, key))
    }

    /// Runs `call` on the bucket; when the bucket refuses the credentials, renews them and
    /// runs it once more.
    fn with_bucket<T>(&self, call: impl Fn(&S3Drive) -> Result<T, DriveError>) -> Result<T, DriveError> {
        let (drive, _) = self.current(false)?;
        match call(&drive) {
            Err(DriveError::Service(service))
                if REFUSED_CREDENTIALS.contains(&service.code.as_str()) =>
            {
                let (drive, _) = self.current(true)?;
                call(&drive)
            }
            other => other,
        }
    }
}

impl Drive for AccountDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.with_bucket(|b| b.list(request))
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.with_bucket(|b| b.get(key))
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.with_bucket(|b| b.get_range(key, range))
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.with_bucket(|b| b.put(key, bytes))
    }
    // The bucket's conditional write (S3 If-None-Match / If-Match): what an encrypted drive's
    // index commits with; the default of the trait would refuse it.
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.with_bucket(|b| b.put_if(key, bytes, condition))
    }
    // The bucket's streamed write (parts for a big file). Once: the body cannot be read again for
    // a retry, so the credentials are made fresh before it starts.
    fn put_from(&self, key: &str, body: &mut dyn std::io::Read) -> Result<u64, DriveError> {
        let (drive, _) = self.current(false)?;
        drive.put_from(key, body)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.delete(key))
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.with_bucket(|b| b.head(key))
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.copy(from, to))
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.create_folder(prefix))
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.rename(from, to))
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.delete_folder(prefix))
    }
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.with_bucket(|b| b.metadata(key))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use azcloud_kit::{
        account::{read_grant, store_grant},
        drive::TransportFactory,
        StateDir,
    };
    use azul_storage::{testing::TempDir, Drive, HttpCall, HttpReply, ListRequest, Transport};

    use super::AccountDrive;

    const TOKEN: &str = "http://127.0.0.1:18081";
    const BLOCK: &str = "http://127.0.0.1:19000";
    const NODE: &str = "http://127.0.0.1:19002";

    /// The block endpoint refuses every connection; the node answers an empty listing. Every
    /// call's URL is kept.
    struct Cloud(Arc<Mutex<Vec<String>>>);

    impl Transport for Cloud {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            self.0.lock().unwrap().push(call.url.clone());
            if call.url.starts_with(BLOCK) {
                return Err(String::from("connection refused"));
            }
            Ok(HttpReply {
                status: 200,
                headers: vec![(String::from("content-type"), String::from("application/xml"))],
                body: b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult><Name>d-1</Name>\
                        <Prefix></Prefix><KeyCount>0</KeyCount><MaxKeys>1000</MaxKeys>\
                        <IsTruncated>false</IsTruncated></ListBucketResult>"
                    .to_vec(),
            })
        }
    }

    /// The bridge's device holding drive d_1 at the block endpoint, its refresh's node list
    /// naming one node (credentials good until 2099: no refresh).
    fn device(dir: &TempDir) -> StateDir {
        let state = StateDir::open(dir.path()).unwrap();
        let answer: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"drive": {{"id": "d_1", "name": "Azlin Storage",
                  "location": {{"kind": "s3", "endpoint": "{BLOCK}", "region": "us-east-1",
                                "bucket": "d-1", "path_style": true,
                                "auth": {{"type": "azlin", "drive_id": "d_1",
                                          "account_url": "{TOKEN}"}}}}}},
                "credentials": {{"access_key_id": "AKID1", "secret_access_key": "secret",
                                 "session_token": "session", "expires_at": "2099-01-01T00:00:00Z"}},
                "failover": [], "nodes": [{{"name": "n2", "url": "{NODE}", "ready": true}}],
                "quota_bytes": 100000000000, "read_only": false,
                "period_until": "2099-01-01T00:00:00Z", "drive_token": "dt_f.0.aaa",
                "tier": "100GB"}}"#
        ))
        .unwrap();
        store_grant(&state, &read_grant(&answer, TOKEN, "owner", 0).unwrap()).unwrap();
        state
    }

    #[test]
    fn the_bridges_drive_fails_over_to_the_nodes_its_refresh_listed() {
        let dir = TempDir::new("bridge-failover");
        let state = device(&dir);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let transports: TransportFactory =
            Arc::new(move || Box::new(Cloud(seen.clone())) as Box<dyn Transport>);
        let drive = AccountDrive::open(&state, TOKEN, transports, None).unwrap();
        drive.list(&ListRequest::folder("")).unwrap();
        let calls = calls.lock().unwrap().clone();
        assert!(calls.first().is_some_and(|url| url.starts_with(BLOCK)), "{calls:?}");
        assert!(
            calls.last().is_some_and(|url| url.starts_with(NODE)),
            "the block endpoint gave no answer: the node did - {calls:?}"
        );
    }
}
