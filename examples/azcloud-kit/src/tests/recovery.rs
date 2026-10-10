//! A drive's recovery key at the token server (AZDRIVE-INTEGRATION §4, §18.7): an Ed25519 key
//! derived from the recovery code, registered with the drive token, that signs a lockdown
//! request without one; and the drive's status as the token server keeps it - its period and a
//! pending recovery-key lockdown, which the recovery code may cancel.

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use super::{header, json, Fake, Shared, TOKEN};
use crate::{
    recovery::RecoveryKey,
    token::{recovery_lockdown_message, DriveStatus, TokenServer, RECOVERY_MEMBER},
};

const CODE: [u8; 16] = [0x5A; 16];

fn verifying(public_b64: &str) -> VerifyingKey {
    let bytes: [u8; 32] = STANDARD.decode(public_b64).unwrap().try_into().unwrap();
    VerifyingKey::from_bytes(&bytes).unwrap()
}

#[test]
fn a_recovery_key_is_the_same_for_the_same_code_and_drive_and_another_for_another_drive() {
    let one = RecoveryKey::derive(&CODE, "d_1");
    // HKDF-SHA256 then RFC 8032, as Python's standard library computes it
    // (scripts/azlin_ed25519.py: the mock's and the conformance checks' side).
    assert_eq!(
        one.public_base64(),
        "TZrvbj1nF30/6IIsz2wc36FCNEID9Pu4HUE9+wjQ7Wo="
    );
    assert_eq!(one.public_base64(), RecoveryKey::derive(&CODE, "d_1").public_base64());
    assert_ne!(one.public_base64(), RecoveryKey::derive(&CODE, "d_2").public_base64());
    assert_ne!(one.public_base64(), RecoveryKey::derive(&[0x5B; 16], "d_1").public_base64());
    assert_eq!(STANDARD.decode(one.public_base64()).unwrap().len(), 32);
    // What it signs, the public key verifies (Ed25519, standard base64 as the server reads it).
    let message = recovery_lockdown_message("d_1", "0123456789abcdef0123456789abcdef");
    let signature = STANDARD.decode(one.sign_base64(message.as_bytes())).unwrap();
    let signature = Signature::from_slice(&signature).unwrap();
    verifying(&one.public_base64())
        .verify(message.as_bytes(), &signature)
        .unwrap();
    assert!(format!("{one:?}").contains("hidden"), "Debug shows no key");
}

#[test]
fn the_recovery_key_is_registered_with_the_drive_token_and_signs_a_lockdown_without_one() {
    let fake = Fake::new(|call, n| {
        Ok(if n == 0 {
            assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/recovery"));
            assert_eq!(header(call, "authorization"), Some("Bearer dt_f.3.newest"));
            let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
            assert_eq!(
                body["recovery_pubkey"],
                RecoveryKey::derive(&CODE, "d_1").public_base64().as_str()
            );
            json(200, r#"{"ok": true}"#)
        } else {
            // The lockdown: the server checks the signature with the registered key.
            assert_eq!(header(call, "authorization"), None);
            let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
            let nonce = body["nonce"].as_str().unwrap();
            let signature = STANDARD.decode(body["signature"].as_str().unwrap()).unwrap();
            verifying(&RecoveryKey::derive(&CODE, "d_1").public_base64())
                .verify(
                    recovery_lockdown_message("d_1", nonce).as_bytes(),
                    &Signature::from_slice(&signature).unwrap(),
                )
                .unwrap();
            json(
                202,
                r#"{"pending_until": "2026-10-13T09:00:00Z", "drive_token": "dt_r.0.new"}"#,
            )
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let key = RecoveryKey::derive(&CODE, "d_1");
    server
        .set_recovery_key("d_1", "dt_f.3.newest", &key.public_base64())
        .unwrap();
    let pending = server
        .recovery_lockdown("d_1", |message| Ok(key.sign_base64(message)))
        .unwrap();
    assert_eq!(pending.drive_token, "dt_r.0.new");
    assert_eq!(fake.calls().len(), 2);
}

#[test]
fn the_drives_status_names_its_period_a_pending_lockdown_and_its_members() {
    let fake = Fake::new(|call, n| {
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1"));
        Ok(json(
            200,
            if n == 0 {
                r#"{"id": "d_1", "tier": "100GB", "read_only": false, "status": "active",
                    "quota_bytes": 100000000000, "used_bytes": 62000000000,
                    "period_until": "2026-11-07T00:00:00Z",
                    "lockdown_pending_until": "2026-10-13T09:00:00Z", "you": "owner",
                    "members": [
                        {"member": "owner", "role": "member", "added_at": "2026-10-01T00:00:00Z"},
                        {"member": "recovery-pending", "role": "member",
                         "added_at": "2026-10-11T00:00:00Z"},
                        {"member": "m_laptop", "role": "member", "added_at": "2026-10-05T00:00:00Z"}
                    ]}"#
            } else {
                r#"{"id": "d_1", "tier": "1TB", "read_only": true,
                    "period_until": null, "lockdown_pending_until": null}"#
            },
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let parse = azul_storage::time::parse_iso8601;
    assert_eq!(
        server.drive_status("d_1", "dt_f.0.a").unwrap(),
        DriveStatus {
            tier: Some(String::from("100GB")),
            period_until: parse("2026-11-07T00:00:00Z"),
            lockdown_pending_until: parse("2026-10-13T09:00:00Z"),
            read_only: false,
            members: vec![
                String::from("m_laptop"),
                String::from("owner"),
                String::from(RECOVERY_MEMBER),
            ],
            you: Some(String::from("owner")),
            quota_bytes: Some(100_000_000_000),
            used_bytes: Some(62_000_000_000),
        }
    );
    assert_eq!(
        server.drive_status("d_1", "dt_f.0.a").unwrap(),
        DriveStatus {
            tier: Some(String::from("1TB")),
            period_until: None,
            lockdown_pending_until: None,
            read_only: true,
            members: Vec::new(),
            you: None,
            quota_bytes: None,
            used_bytes: None,
        }
    );
}
