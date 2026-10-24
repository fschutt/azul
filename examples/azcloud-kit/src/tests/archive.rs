//! A locked drive's way back, as the kit speaks it: the drive's status names its archive (and a
//! locked drive is no payment due, nor reminded of one); a proof signs the token server's
//! statement - purpose, drive, nonce, expiry, the checkout it unlocks - with one of the drive's
//! recovery keys, and its refusals stay refusals (a wrong key is no reason to sign in again); a
//! device's own key becomes one of the drive's recovery keys once, signed by the code's key.

use azul_storage::testing::TempDir;
use serde_json::Value;

use super::{json, Fake, Shared, TOKEN};
use crate::{
    azlin_contract::{archive_statement, ArchiveState},
    device_key::{register_as_recovery_key, DeviceKey, DEVICE_LABEL},
    token::{ProofAnswer, TokenServer},
    RecoveryKey, TokenError,
};

/// The token server's challenge: its nonce and its expiry (2026-10-21T08:05:00Z).
const CHALLENGE: &str = r#"{"challenge": "ac1.d_1.1792570000.n0nce0123456789abcdef.g0:00",
    "nonce": "n0nce0123456789abcdef", "expires_at": "2026-10-21T08:05:00Z"}"#;
const EXPIRES: i64 = 1_792_570_000;

const LOCKED: &str = r#"{"state": "locked", "locked_at": "2026-10-13T00:00:00Z",
    "pay_through": "2026-10-13T00:00:00Z"}"#;

fn verifies(public_b64: &str, message: &[u8], signature_b64: &str) -> bool {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let (Ok(key), Ok(sig)) = (STANDARD.decode(public_b64), STANDARD.decode(signature_b64)) else {
        return false;
    };
    let (Ok(key), Ok(sig)) = (
        <[u8; 32]>::try_from(key.as_slice()),
        <[u8; 64]>::try_from(sig.as_slice()),
    ) else {
        return false;
    };
    VerifyingKey::from_bytes(&key)
        .is_ok_and(|k| k.verify(message, &Signature::from_bytes(&sig)).is_ok())
}

#[test]
fn a_locked_drive_s_status_names_its_archive_and_owes_no_ordinary_payment() {
    let body = format!(
        r#"{{"id": "d_1", "tier": "100GB", "status": "archived", "read_only": true,
            "read_only_reason": "archived", "period_until": "2026-07-01T00:00:00Z",
            "archive": {LOCKED}}}"#
    );
    let fake = Fake::new(move |_, _| Ok(json(200, &body)));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let status = server.drive_status("d_1", "dt_f.0.a").unwrap();
    let archive = status.archive.clone().expect("the archive");
    assert_eq!(archive.state, ArchiveState::Locked);
    assert!(status.locked());
    assert!(!status.payment_due(), "a locked drive's way back is its own page");
    assert_eq!(
        status.read_only_why,
        Some(crate::azlin_contract::ReadOnlyWhy::Archived)
    );
}

#[test]
fn a_proof_signs_the_token_server_s_challenge_for_this_drive() {
    let key = DeviceKey::from_seed(&[4; 32]);
    let public = key.public_base64();
    let fake = Fake::new(|call, n| {
        if n == 0 {
            assert!(call.url.ends_with("/v1/drives/d_1/archive/challenge"), "{}", call.url);
            Ok(json(
                200,
                CHALLENGE,
            ))
        } else {
            assert!(call.url.ends_with("/v1/drives/d_1/archive/proof"), "{}", call.url);
            Ok(json(
                200,
                r#"{"state": "locked", "locked_at": "2026-10-13T00:00:00Z",
                    "pay_through": "2026-10-13T00:00:00Z", "proven": true}"#,
            ))
        }
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let answer = server
        .archive_prove("d_1", "dt_f.0.a", Some("ck_paid"), Some("rk_device"), |m| {
            Ok(key.sign_base64(m))
        })
        .unwrap();
    let wire = match answer {
        ProofAnswer::Locked(wire) => wire,
        other => panic!("{other:?}"),
    };
    assert!(wire.proven);
    let calls = fake.calls();
    let sent: Value = serde_json::from_slice(&calls[1].body).unwrap();
    assert_eq!(sent["challenge"], "ac1.d_1.1792570000.n0nce0123456789abcdef.g0:00");
    assert_eq!(sent["checkout_id"], "ck_paid");
    assert_eq!(sent["key_id"], "rk_device");
    let statement = archive_statement("d_1", "n0nce0123456789abcdef", EXPIRES, Some("ck_paid"));
    assert!(verifies(&public, statement.as_bytes(), sent["signature"].as_str().unwrap()));
    // the same signature says nothing for another payment
    let other = archive_statement("d_1", "n0nce0123456789abcdef", EXPIRES, Some("ck_other"));
    assert!(!verifies(&public, other.as_bytes(), sent["signature"].as_str().unwrap()));
    // open again at once (its files never left the servers)
    let fake = Fake::new(|_, n| {
        Ok(json(
            200,
            if n == 0 {
                CHALLENGE
            } else {
                r#"{"state": "restored"}"#
            },
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let answer = server
        .archive_prove("d_1", "dt_f.0.a", None, None, |m| Ok(key.sign_base64(m)))
        .unwrap();
    assert_eq!(answer, ProofAnswer::Restored);
}

#[test]
fn a_wrong_key_or_a_used_challenge_is_a_refusal_not_a_sign_in() {
    for (status, body, code) in [
        (401, r#"{"error": "bad_signature", "message": "no key of the drive signed it"}"#, "bad_signature"),
        (409, r#"{"error": "challenge_used"}"#, "challenge_used"),
        (403, r#"{"error": "no_key"}"#, "no_key"),
        (409, r#"{"error": "checkout_not_paid"}"#, "checkout_not_paid"),
    ] {
        let fake = Fake::new(move |_, n| {
            if n == 0 {
                Ok(json(200, CHALLENGE))
            } else {
                Ok(json(status, body))
            }
        });
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        let refused = server
            .archive_prove("d_1", "dt_f.0.a", None, None, |_| Ok(String::from("c2ln")))
            .unwrap_err();
        assert!(
            matches!(&refused, TokenError::Refused { code: c, .. } if c == code),
            "{refused:?}"
        );
    }
    // a drive token the token server does not know is a sign-in, as everywhere
    let fake = Fake::new(|_, _| Ok(json(401, r#"{"error": "unauthorized"}"#)));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let refused = server
        .archive_prove("d_1", "dt_f.0.a", None, None, |_| Ok(String::new()))
        .unwrap_err();
    assert!(matches!(refused, TokenError::SignIn(_)), "{refused:?}");
}

#[test]
fn a_device_s_key_becomes_one_of_the_drive_s_recovery_keys_once_signed_by_the_code_s_key() {
    let dir = TempDir::new("azcloud-device-key");
    let shared = super::period::keyring_with_session(&dir, "dt_f.3.newest");
    let code_key = RecoveryKey::derive(b"ABCDE-FGHIJ-KLMNO-PQRST-UVWXYZ", "d_1");
    let fake = Fake::new(|call, _| {
        assert!(call.url.ends_with("/v1/drives/d_1/recovery_keys"), "{}", call.url);
        let sent: Value = serde_json::from_slice(&call.body).unwrap();
        Ok(json(
            201,
            &serde_json::json!({
                "key_id": "rk_device", "label": sent["label"], "recovery_pubkey": sent["recovery_pubkey"],
                "verified": true,
            })
            .to_string(),
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let id = register_as_recovery_key(&server, &shared, "d_1", |m| Ok(code_key.sign_base64(m)))
        .unwrap();
    assert_eq!(id, "rk_device");
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    let sent: Value = serde_json::from_slice(&calls[0].body).unwrap();
    assert_eq!(sent["label"], DEVICE_LABEL);
    // signed by the code's key over the addition, as every new recovery key is
    let public = sent["recovery_pubkey"].as_str().unwrap();
    let message = crate::token::recovery_add_message("d_1", public, sent["nonce"].as_str().unwrap());
    assert!(verifies(&code_key.public_base64(), message.as_bytes(), sent["signature"].as_str().unwrap()));
    // kept with its id: nothing is sent again
    let again = register_as_recovery_key(&server, &shared, "d_1", |_| Err(String::from("no code")))
        .unwrap();
    assert_eq!(again, "rk_device");
    assert_eq!(fake.calls().len(), 1);
    let kept = DeviceKey::load(&shared, "d_1").unwrap().expect("in the keyring");
    assert_eq!(kept.key_id(), Some("rk_device"));
    assert_eq!(kept.public_base64(), public);
}
