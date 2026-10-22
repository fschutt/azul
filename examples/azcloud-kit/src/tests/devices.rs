//! A second device reaches a drive however long after its purchase: the device that has the
//! drive makes a join code from the keyring's session (one idempotency key across the retries),
//! the joining device spends the code's token once and keeps the session in its keyring; one
//! device removes another. A recovery key's registration is signed once, while the code is
//! shown, and sent again as it is.

use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
    Method,
};

use super::{bundle, header, json, keyring_holding, Fake, Shared, TOKEN};
use crate::{
    shared::SharedKeyring,
    token::{recovery_add_message, recovery_key_message, Presigned},
    AzlinSession, JoinCode, TokenServer,
};

const IDEMPOTENCY: &str = "x-azlin-idempotency";

#[test]
fn a_device_invites_another_with_its_newest_token_and_one_key_across_the_retries() {
    let dir = TempDir::new("kit-invite");
    let keyring = keyring_holding(&dir, "dt_f.0.aaa");
    let lost = Arc::new(AtomicU32::new(1));
    let fake = Fake::new({
        let lost = lost.clone();
        move |_, _| {
            if lost.fetch_sub(1, Ordering::SeqCst) > 0 {
                return Err(String::from("connection reset"));
            }
            Ok(json(201, r#"{"member": "m_1", "drive_token": "dt_m.0.joins"}"#))
        }
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let code = keyring.invite(&server, "d_1", "Photos", None).unwrap();
    assert_eq!(code.drive_id, "d_1");
    assert_eq!(code.drive_token, "dt_m.0.joins");
    assert_eq!(code.member, "m_1");
    assert_eq!(code.name, "Photos");
    assert_eq!(code.token_url, TOKEN);
    let calls = fake.calls();
    assert_eq!(calls.len(), 2, "the lost answer is asked for again");
    for call in &calls {
        assert_eq!(call.method, Method::Post);
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/members"));
        assert_eq!(header(call, "authorization"), Some("Bearer dt_f.0.aaa"));
    }
    let keys: Vec<Option<&str>> = calls.iter().map(|c| header(c, IDEMPOTENCY)).collect();
    assert!(keys[0].is_some() && keys[0] == keys[1], "one key: {keys:?}");
    assert!(keyring.outbox().pending(azlin_contract::IdempotentScope::AddMember, "d_1").is_none());
}

#[test]
fn a_join_code_s_token_is_spent_once_and_its_session_kept_in_the_keyring() {
    let dir = TempDir::new("kit-join");
    let store = Arc::new(MemoryKeyring::new());
    let keyring = SharedKeyring::with_lock_root(
        store.clone(),
        KeyringNamespace::azlin_shared(),
        dir.path(),
    );
    let fake = Fake::new(|_, _| {
        Ok(json(200, &bundle("AKID2", "2026-10-09T21:15:00Z", "dt_m.1.next")))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let code = JoinCode {
        drive_id: String::from("d_1"),
        drive_token: String::from("dt_m.0.joins"),
        member: String::from("m_1"),
        name: String::from("Photos"),
        token_url: String::from(TOKEN),
        key_seal: None,
    };
    let joined = keyring.join(&server, &code).unwrap();
    assert_eq!(joined.drive_id(), "d_1");
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(header(&calls[0], "authorization"), Some("Bearer dt_m.0.joins"));
    let kept = store.get(&keyring_key("d_1")).unwrap().expect("the session is kept");
    let session = AzlinSession::from_keyring_secret(&kept).unwrap();
    assert_eq!(session.drive_token.as_str(), "dt_m.1.next");
    // a device that holds the drive already joins nothing: its session stays
    let again = keyring.join(&server, &code);
    assert!(again.is_err(), "the drive is on this device already");
    assert_eq!(fake.calls().len(), 1, "nothing spent");
}

#[test]
fn one_device_removes_another_with_a_delete_that_carries_its_key() {
    let dir = TempDir::new("kit-remove");
    let keyring = keyring_holding(&dir, "dt_f.0.aaa");
    let fake = Fake::new(|_, _| Ok(json(200, r#"{"removed": "m_1"}"#)));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    keyring.remove_member(&server, "d_1", "m_1").unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, Method::Delete);
    assert_eq!(calls[0].url, format!("{TOKEN}/v1/drives/d_1/members/m_1"));
    assert_eq!(header(&calls[0], "authorization"), Some("Bearer dt_f.0.aaa"));
    assert!(header(&calls[0], IDEMPOTENCY).is_some());
    // a name that is no path segment never reaches a URL
    assert!(keyring.remove_member(&server, "d_1", "../x").is_err());
    assert_eq!(fake.calls().len(), 1);
}

#[test]
fn a_recovery_key_change_signed_once_is_sent_again_with_the_same_nonce_and_signature() {
    let mut signed = Vec::new();
    let replace = Presigned::replace("d_1", "NEWKEY", |m| {
        signed.push(String::from_utf8_lossy(m).to_string());
        Ok(String::from("sig-replace"))
    })
    .unwrap();
    assert_eq!(signed, [recovery_key_message("d_1", "NEWKEY", &replace.nonce)]);
    let mut added = Vec::new();
    let add = Presigned::add("d_1", "FINDABLE", |m| {
        added.push(String::from_utf8_lossy(m).to_string());
        Ok(String::from("sig-add"))
    })
    .unwrap();
    assert_eq!(added, [recovery_add_message("d_1", "FINDABLE", &add.nonce)]);
    let fake = Fake::new(|_, _| Ok(json(200, "{}")));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let key = super::grant_key();
    for _ in 0..2 {
        server
            .replace_recovery_key_presigned("d_1", "dt_f.0.aaa", "NEWKEY", &key, &replace)
            .unwrap();
        server
            .add_recovery_key_presigned("d_1", "dt_f.0.aaa", "FINDABLE", "label", &key, &add)
            .unwrap();
    }
    let bodies: Vec<serde_json::Value> = fake
        .calls()
        .iter()
        .map(|c| serde_json::from_slice(&c.body).unwrap())
        .collect();
    assert_eq!(bodies.len(), 4);
    for pair in bodies.chunks(2) {
        assert_eq!(pair[0]["nonce"], replace.nonce.as_str());
        assert_eq!(pair[0]["signature"], "sig-replace");
        assert_eq!(pair[0]["recovery_pubkey"], "NEWKEY");
        assert_eq!(pair[1]["nonce"], add.nonce.as_str());
        assert_eq!(pair[1]["signature"], "sig-add");
        assert_eq!(pair[1]["recovery_pubkey"], "FINDABLE");
    }
}
