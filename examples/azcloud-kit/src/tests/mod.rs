//! Unit tests of the kit: the token server and the bucket are a fake [`Transport`] that answers
//! as the real ones do (the token server's JSON, S3's XML) and records every call.

mod drive;
mod endpoints;
mod session;
mod token;

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

/// The header `name` of `call`.
pub(crate) fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
    call.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}
