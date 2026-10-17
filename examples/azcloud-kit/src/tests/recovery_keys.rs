//! A drive's several recovery keys and the lookup by one (SRV17: D51, F12, §18.8): the kit's
//! code gives a findable key - no drive in it - that names its drive to a computer that never
//! had it; the drive's keys are listed, and added or removed with a current key's signature.

use azul_storage::Method;

use super::{header, json, Fake, Shared, TOKEN};
use crate::{
    recovery::{RecoveryKey, FINDABLE_INFO},
    token::{
        recovery_add_message, recovery_lookup_message, recovery_remove_message, FoundDrive,
        RecoveryKeyInfo, TokenError, TokenServer,
    },
};

const CODE: [u8; 16] = [0x5A; 16];

fn body(call: &azul_storage::HttpCall) -> serde_json::Value {
    serde_json::from_slice(&call.body).unwrap()
}

#[test]
fn the_findable_key_is_the_codes_alone_and_never_a_drives() {
    let findable = RecoveryKey::derive_findable(&CODE);
    // HKDF-SHA256 (the lockdown salt, the info "azlin-recovery-findable-v1") then RFC 8032, as
    // Python's standard library and scripts/azlin_ed25519.py compute it.
    assert_eq!(
        findable.public_base64(),
        "4fYnw9vu6c3jx5BLFidmGJhAk02LgGLcX1Wft8xsFE0="
    );
    assert_eq!(
        findable.public_base64(),
        RecoveryKey::derive(&CODE, FINDABLE_INFO).public_base64()
    );
    assert_ne!(
        findable.public_base64(),
        RecoveryKey::derive(&CODE, "d_1").public_base64(),
        "not the drive's key"
    );
    assert_ne!(
        findable.public_base64(),
        RecoveryKey::derive_findable(&[0x5B; 16]).public_base64()
    );
    assert!(
        !FINDABLE_INFO.starts_with("d_"),
        "no drive id looks like it"
    );
}

#[test]
fn the_messages_are_the_servers() {
    assert_eq!(
        recovery_lookup_message("rc1.9.ab.cd"),
        "recovery-lookup:rc1.9.ab.cd"
    );
    assert_eq!(
        recovery_add_message("d_1", "KEY=", "0123456789abcdef"),
        "recovery-add:d_1:KEY=:0123456789abcdef"
    );
    assert_eq!(
        recovery_remove_message("d_1", "rk_2", "0123456789abcdef"),
        "recovery-remove:d_1:rk_2:0123456789abcdef"
    );
}

#[test]
fn a_lookup_asks_a_challenge_signs_it_and_names_the_drives() {
    let fake = Fake::new(|call, n| {
        Ok(match n {
            0 => {
                assert_eq!(call.method, Method::Post);
                assert_eq!(call.url, format!("{TOKEN}/v1/recovery/challenge"));
                json(
                    200,
                    r#"{"challenge": "rc1.1790000300.abc.mac", "expires_at": "2026-09-21T14:05:00Z"}"#,
                )
            }
            _ => {
                assert_eq!(call.url, format!("{TOKEN}/v1/recovery/lookup"));
                assert_eq!(header(call, "authorization"), None, "no drive token");
                let sent = body(call);
                assert_eq!(sent["recovery_pubkey"], "PUB=");
                assert_eq!(sent["challenge"], "rc1.1790000300.abc.mac");
                assert_eq!(
                    sent["signature"],
                    "sig(recovery-lookup:rc1.1790000300.abc.mac)"
                );
                json(
                    200,
                    r#"{"drives": [{"drive_id": "d_1", "key_id": "rk_a"},
                                   {"drive_id": "d_2", "key_id": "rk_legacy"}]}"#,
                )
            }
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let found = server
        .recovery_lookup("PUB=", |message| {
            Ok(format!("sig({})", String::from_utf8_lossy(message)))
        })
        .unwrap();
    assert_eq!(
        found,
        vec![
            FoundDrive {
                drive_id: String::from("d_1"),
                key_id: String::from("rk_a"),
            },
            FoundDrive {
                drive_id: String::from("d_2"),
                key_id: String::from("rk_legacy"),
            },
        ]
    );
    assert_eq!(fake.calls().len(), 2);
}

#[test]
fn a_lookups_refusals_are_the_servers_and_nothing_signed_is_nothing_sent() {
    let fake = Fake::new(|_, n| {
        Ok(if n % 2 == 0 {
            json(
                200,
                r#"{"challenge": "rc1.1.a.b", "expires_at": "2026-09-21T14:05:00Z"}"#,
            )
        } else {
            json(
                401,
                r#"{"error": "bad_challenge", "message": "at most 5 minutes old"}"#,
            )
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match server.recovery_lookup("PUB=", |_| Ok(String::from("sig"))) {
        Err(TokenError::Refused {
            status: 401, code, ..
        }) => assert_eq!(code, "bad_challenge"),
        other => panic!("not the server's refusal: {other:?}"),
    }
    let sent = fake.calls().len();
    assert!(matches!(
        server.recovery_lookup("PUB=", |_| Err(String::from("no code"))),
        Err(TokenError::Config(_))
    ));
    assert_eq!(
        fake.calls().len(),
        sent + 1,
        "the challenge only, no lookup"
    );
    // An empty list is an answer: no drive has the key.
    let none = Fake::new(|_, n| {
        Ok(if n == 0 {
            json(
                200,
                r#"{"challenge": "rc1.1.a.b", "expires_at": "2026-09-21T14:05:00Z"}"#,
            )
        } else {
            json(200, r#"{"drives": []}"#)
        })
    });
    let transport = Shared(none);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(
        server
            .recovery_lookup("PUB=", |_| Ok(String::from("sig")))
            .unwrap(),
        Vec::new()
    );
}

#[test]
fn the_drives_recovery_keys_are_listed_added_and_removed_with_a_keys_signature() {
    let fake = Fake::new(|call, n| {
        Ok(match n {
            0 => {
                assert_eq!(call.method, Method::Get);
                assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/recovery_keys"));
                assert_eq!(header(call, "authorization"), Some("Bearer dt_f.3.x"));
                json(
                    200,
                    r#"{"keys": [{"key_id": "rk_legacy", "label": "recovery code",
                                  "recovery_pubkey": "A=", "created_at": null, "verified": true},
                                 {"key_id": "rk_b", "label": "second kit",
                                  "recovery_pubkey": "B=", "created_at": "2026-10-10T00:00:00Z",
                                  "verified": false}]}"#,
                )
            }
            1 => {
                assert_eq!(call.method, Method::Post);
                assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/recovery_keys"));
                assert_eq!(header(call, "authorization"), Some("Bearer dt_f.3.x"));
                let sent = body(call);
                let nonce = sent["nonce"].as_str().unwrap().to_string();
                assert!((16..=128).contains(&nonce.len()));
                assert_eq!(sent["recovery_pubkey"], "C=");
                assert_eq!(sent["label"], "third kit");
                assert_eq!(
                    sent["signature"],
                    format!("sig({})", recovery_add_message("d_1", "C=", &nonce)).as_str()
                );
                json(
                    201,
                    r#"{"key_id": "rk_c", "label": "third kit", "recovery_pubkey": "C=",
                        "created_at": "2026-10-10T00:00:00Z", "verified": true}"#,
                )
            }
            2 => {
                assert_eq!(call.method, Method::Delete);
                assert_eq!(
                    call.url,
                    format!("{TOKEN}/v1/drives/d_1/recovery_keys/rk_b")
                );
                let sent = body(call);
                let nonce = sent["nonce"].as_str().unwrap().to_string();
                assert_eq!(
                    sent["signature"],
                    format!("sig({})", recovery_remove_message("d_1", "rk_b", &nonce)).as_str()
                );
                json(200, r#"{"removed": "rk_b"}"#)
            }
            _ => json(
                409,
                r#"{"error": "last_recovery_key", "message": "add another first"}"#,
            ),
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let sign = |message: &[u8]| Ok(format!("sig({})", String::from_utf8_lossy(message)));
    let keys = server.recovery_keys("d_1", "dt_f.3.x").unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(
        keys[0],
        RecoveryKeyInfo {
            key_id: String::from("rk_legacy"),
            label: String::from("recovery code"),
            recovery_pubkey: String::from("A="),
            created_at: None,
            verified: true,
        }
    );
    assert_eq!(
        keys[1].created_at,
        azul_storage::time::parse_iso8601("2026-10-10T00:00:00Z")
    );
    assert!(!keys[1].verified);
    let added = server
        .add_recovery_key("d_1", "dt_f.3.x", "C=", "third kit", sign)
        .unwrap();
    assert_eq!(added.key_id, "rk_c");
    server
        .remove_recovery_key("d_1", "dt_f.3.x", "rk_b", sign)
        .unwrap();
    match server.remove_recovery_key("d_1", "dt_f.3.x", "rk_legacy", sign) {
        Err(TokenError::Refused {
            status: 409, code, ..
        }) => assert_eq!(code, "last_recovery_key"),
        other => panic!("not the server's refusal: {other:?}"),
    }
    // A key id that is no path segment is refused before anything is sent.
    let sent = fake.calls().len();
    assert!(matches!(
        server.remove_recovery_key("d_1", "dt_f.3.x", "../x", sign),
        Err(TokenError::Config(_))
    ));
    assert_eq!(fake.calls().len(), sent);
}
