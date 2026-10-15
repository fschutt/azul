//! An Azlin drive under answers that say too little: the answer to a HEAD carries no body, so
//! expired credentials look like any 400 or 403 there (a clock behind the node's never asks
//! for fresh ones in time). The drive asks for fresh credentials once and asks again, and it
//! does not spend a drive token on every refused HEAD.

use std::sync::Arc;

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, HttpReply, Method, Transport,
};

use super::{bundle, header, json, Fake, Shared, TOKEN};
use crate::{
    drive::{AzlinDrive, TransportFactory},
    lock::LockDir,
    shared::SharedKeyring,
    AzlinSession, DriveBundle,
};

/// The cloud: the token server's refreshes (AKID2, then AKID3), and a bucket whose HEAD answers
/// 400 without a body for the keys `refused` and the object for the rest.
fn cloud(refused: &'static [&'static str]) -> Arc<Fake> {
    Fake::new(move |call, _| {
        if call.url.starts_with(TOKEN) {
            return Ok(match header(call, "authorization") {
                Some("Bearer dt_f.0.aaa") => {
                    json(200, &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb"))
                }
                Some("Bearer dt_f.1.bbb") => {
                    json(200, &bundle("AKID3", "2026-10-09T21:15:00Z", "dt_f.2.ccc"))
                }
                _ => json(
                    401,
                    r#"{"error": "token_reuse", "message": "an old token was reused"}"#,
                ),
            });
        }
        let signed_with = header(call, "authorization").unwrap_or_default().to_string();
        if refused
            .iter()
            .any(|key| signed_with.contains(&format!("Credential={key}/")))
        {
            return Ok(HttpReply {
                status: 400,
                headers: Vec::new(),
                body: Vec::new(),
            });
        }
        Ok(HttpReply {
            status: 200,
            headers: vec![
                (String::from("content-length"), String::from("5")),
                (String::from("etag"), String::from("\"e1\"")),
                (
                    String::from("last-modified"),
                    String::from("Thu, 08 Oct 2026 21:00:00 GMT"),
                ),
            ],
            body: Vec::new(),
        })
    })
}

/// The drive of the first bundle (AKID1, good for hours by its clock).
fn drive(fake: &Arc<Fake>, dir: &TempDir) -> AzlinDrive {
    let first =
        DriveBundle::parse(&bundle("AKID1", "2026-10-09T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let keyring = Arc::new(MemoryKeyring::new());
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let shared = SharedKeyring::new(keyring, LockDir::new(dir.path()));
    let answering = fake.clone();
    let factory: TransportFactory =
        Arc::new(move || Box::new(Shared(answering.clone())) as Box<dyn Transport>);
    AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        shared,
        factory,
        Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
    )
    .unwrap()
    // 2026-10-08T09:00:00Z: the session looks good for hours.
    .with_clock(|| 1_791_450_000)
}

fn refreshes(fake: &Fake) -> usize {
    fake.calls()
        .iter()
        .filter(|c| c.method == Method::Post && c.url.ends_with("/credentials"))
        .count()
}

#[test]
fn a_head_refused_without_a_body_refreshes_the_credentials_once_and_asks_again() {
    let dir = TempDir::new("azcloud-drive-chaos-head");
    let fake = cloud(&["AKID1"]);
    let drive = drive(&fake, &dir);
    let info = drive
        .head("a.txt")
        .expect("with fresh credentials the HEAD is answered");
    assert_eq!(info.size, 5);
    assert_eq!(refreshes(&fake), 1);
}

#[test]
fn a_head_the_bucket_keeps_refusing_without_a_body_does_not_spend_a_token_each_time() {
    let dir = TempDir::new("azcloud-drive-chaos-head-again");
    let fake = cloud(&["AKID1", "AKID2", "AKID3"]);
    let drive = drive(&fake, &dir);
    assert!(drive.head("a.txt").is_err());
    assert!(drive.head("a.txt").is_err());
    assert!(drive.head("a.txt").is_err());
    assert_eq!(
        refreshes(&fake),
        1,
        "one refresh to rule out expired credentials, not one per HEAD"
    );
}
