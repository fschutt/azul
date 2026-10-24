//! Unit tests of the kit: the token server and the bucket are a fake [`Transport`] that answers
//! as the real ones do (the token server's JSON, S3's XML) and records every call.

mod account;
mod ban;
mod bucket;
/// Many files at once: folders deleted and moved, batch deletes and moves.
mod bulk;
mod cash;
mod claim;
mod claim_ticket;
mod cloud_drive;
mod devices;
mod drive;
/// An Azlin drive under answers that say too little (a HEAD's refusal has no body).
mod drive_chaos;
#[cfg(feature = "encryption")]
mod encryption;
mod endpoints;
mod failover;
mod fake_s3;
/// Grants whose answer was lost: the lockdown and a pick-up's claim asked again.
mod grants;
mod hello;
mod lane;
mod lock;
mod lockdown;
mod outbox;
/// Small files packed by an encrypted drive, through the sync (feature `encryption`).
#[cfg(feature = "encryption")]
mod packing;
mod pending;
mod period;
mod recovery;
mod recovery_keys;
mod renewal;
mod restore;
mod secrets;
mod session;
mod settings;
mod share;
mod state;
mod sync;
/// The sync under hostile conditions: failed passes, files it cannot take, twin names, clocks.
mod sync_chaos;
mod sync_format;
mod sync_guard;
mod sync_local;
mod sync_merge;
mod sync_named;
mod sync_ondemand;
/// The per-file problem list: codes, what stays listed, escaped names.
mod sync_problems;
/// The base as a SQLite state database.
#[cfg(feature = "state-db")]
mod sync_base;
/// The engine around real passes: loops, the operations log, problems, recovery, marks.
mod sync_engine;
/// The sync test harness's drive folder as a drive side.
mod sync_folder_remote;
/// The journal: steps a crash cut off are finished or rolled back.
mod sync_journal;
/// The loop detector.
mod sync_loops;
/// The operations log.
mod sync_oplog;
/// Quarantine marks on files from share participants.
mod sync_quarantine;
/// Bounded retries and Retry-After.
mod sync_retry;
/// The watcher and the full passes' clock.
mod sync_watcher;
mod sync_remote;
mod sync_rules;
mod sync_session;
mod spend;
mod sync_stream;
mod token;
mod usage;
mod transport;
mod user_errors;
/// A drive's period tokens kept with the drive itself (feature `encryption`).
#[cfg(feature = "encryption")]
mod vault;
mod voucher;

use std::sync::{Arc, Mutex};

use azul_storage::{HttpCall, HttpReply, Transport};

/// The token server of the tests (an unencrypted one must be on this computer).
pub(crate) const TOKEN: &str = "http://127.0.0.1:18081";
/// The bucket's endpoint the bundles name.
pub(crate) const S3: &str = "http://127.0.0.1:19000";

/// A server in a closure: every call is recorded, the closure answers it.
pub(crate) struct Fake {
    pub calls: Mutex<Vec<HttpCall>>,
    answer: Box<dyn Fn(&HttpCall, usize) -> Result<HttpReply, String> + Send + Sync>,
}

impl Fake {
    /// A server answering with `answer(call, how many calls came before)`.
    pub fn new(
        answer: impl Fn(&HttpCall, usize) -> Result<HttpReply, String> + Send + Sync + 'static,
    ) -> Arc<Fake> {
        Arc::new(Fake {
            calls: Mutex::new(Vec::new()),
            answer: Box::new(answer),
        })
    }

    pub fn calls(&self) -> Vec<HttpCall> {
        self.calls.lock().unwrap().clone()
    }
}

impl Transport for Fake {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        let before = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(call.clone());
            calls.len() - 1
        };
        (self.answer)(call, before)
    }
}

/// The fake behind an `Arc`, as a transport of its own (one per S3 drive the kit makes).
pub(crate) struct Shared(pub Arc<Fake>);

impl Transport for Shared {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.send(call)
    }
}

/// A JSON answer.
pub(crate) fn json(status: u16, body: &str) -> HttpReply {
    HttpReply {
        status,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: body.as_bytes().to_vec(),
    }
}

/// The token server's bundle for drive `d_1` (azlin-token's `signup_response`): temporary
/// credentials `key` expiring at `expires` (RFC 3339), the drive token `token`.
pub(crate) fn bundle(key: &str, expires: &str, token: &str) -> String {
    format!(
        r#"{{"drive": {{"id": "d_1", "name": "Azlin Storage",
              "location": {{"kind": "s3", "endpoint": "{S3}", "region": "us-east-1",
                            "bucket": "d-1", "path_style": true,
                            "auth": {{"type": "azlin", "drive_id": "d_1",
                                      "account_url": "{TOKEN}"}}}}}},
            "credentials": {{"access_key_id": "{key}", "secret_access_key": "secret-of-{key}",
                             "session_token": "session-of-{key}", "expires_at": "{expires}"}},
            "failover": [], "nodes": [], "quota_bytes": 100000000000, "read_only": false,
            "period_until": "2026-11-07T09:15:00Z", "drive_token": "{token}", "tier": "100GB"}}"#
    )
}

/// An empty ListObjectsV2 answer.
pub(crate) fn empty_listing() -> HttpReply {
    HttpReply {
        status: 200,
        headers: vec![("content-type".to_string(), "application/xml".to_string())],
        body: b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult \
                xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>d-1</Name><Prefix>\
                </Prefix><KeyCount>0</KeyCount><MaxKeys>1000</MaxKeys><IsTruncated>false\
                </IsTruncated></ListBucketResult>"
            .to_vec(),
    }
}

/// The idempotency key the tests' grant calls carry.
pub(crate) fn grant_key() -> azlin_contract::IdempotencyKey {
    azlin_contract::IdempotencyKey::fresh([7; 16])
}

/// The header `name` of `call`.
pub(crate) fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
    call.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// A keyring (a map) holding drive `d_1`'s session with the drive token `token`, its locks in
/// `dir`.
pub(crate) fn keyring_holding(dir: &azul_storage::testing::TempDir, token: &str) -> crate::SharedKeyring {
    use azul_storage::keyring::{KeyringNamespace, KeyringStore, MemoryKeyring};
    let keyring = Arc::new(MemoryKeyring::new());
    let session = crate::DriveBundle::parse(&bundle("AKID1", "2026-10-09T21:15:00Z", token))
        .unwrap()
        .session();
    keyring
        .set(&azul_storage::config::keyring_key("d_1"), &session.to_keyring_secret())
        .unwrap();
    crate::SharedKeyring::with_lock_root(keyring, KeyringNamespace::azlin_shared(), dir.path())
}
