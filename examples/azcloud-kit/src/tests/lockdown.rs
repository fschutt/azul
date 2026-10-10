//! A recovery-key lockdown (AZDRIVE-INTEGRATION §4, AZLINSEC17 F14): a fresh nonce for every
//! request, signed with the recovery key; the token server answers a replay with 409.

use azul_storage::Method;

use super::{header, json, Fake, Shared, TOKEN};
use crate::token::{
    lockdown_cancel_message, recovery_key_message, recovery_lockdown_message, TokenError,
    TokenServer,
};

/// F12 (the user: "the recovery code always wins"): cancelling a recovery-key lockdown and
/// changing the recovery key are signed with the CURRENT recovery code's key - a fresh nonce
/// each; the cancel sends no drive token (a device's token alone cancels nothing).
#[test]
fn a_cancel_and_a_key_change_are_signed_with_the_current_recovery_code() {
    assert_eq!(
        lockdown_cancel_message("d_1", "0123456789abcdef"),
        "lockdown-cancel:d_1:0123456789abcdef"
    );
    assert_eq!(
        recovery_key_message("d_1", "TkVX", "0123456789abcdef"),
        "recovery:d_1:TkVX:0123456789abcdef"
    );
    let fake = Fake::new(|_, n| {
        Ok(if n == 0 {
            json(200, r#"{"cancelled": true}"#)
        } else {
            json(200, r#"{"ok": true}"#)
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let sign = |message: &[u8]| Ok(format!("sig({})", String::from_utf8_lossy(message)));
    server.lockdown_cancel_signed("d_1", sign).unwrap();
    server
        .replace_recovery_key("d_1", "dt_f.3.newest", "TkVX", sign)
        .unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    let cancel = &calls[0];
    assert_eq!(cancel.method, Method::Post);
    assert_eq!(cancel.url, format!("{TOKEN}/v1/drives/d_1/lockdown/cancel"));
    assert_eq!(header(cancel, "authorization"), None, "the recovery key, no drive token");
    let body: serde_json::Value = serde_json::from_slice(&cancel.body).unwrap();
    let nonce = body["nonce"].as_str().unwrap();
    assert!((16..=128).contains(&nonce.len()) && nonce.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(body["signature"], format!("sig(lockdown-cancel:d_1:{nonce})").as_str());
    let change = &calls[1];
    assert_eq!(change.url, format!("{TOKEN}/v1/drives/d_1/recovery"));
    assert_eq!(header(change, "authorization"), Some("Bearer dt_f.3.newest"));
    let body: serde_json::Value = serde_json::from_slice(&change.body).unwrap();
    assert_eq!(body["recovery_pubkey"], "TkVX");
    let nonce = body["nonce"].as_str().unwrap();
    assert_eq!(body["signature"], format!("sig(recovery:d_1:TkVX:{nonce})").as_str());
    // Without the code at hand nothing is sent.
    assert!(matches!(
        server.lockdown_cancel_signed("d_1", |_| Err(String::from("no code"))),
        Err(TokenError::Config(_))
    ));
    assert_eq!(fake.calls().len(), 2);
}

#[test]
fn the_lockdown_message_names_the_drive_and_the_nonce() {
    assert_eq!(
        recovery_lockdown_message("d_1", "0123456789abcdef"),
        "lockdown:d_1:0123456789abcdef"
    );
}

#[test]
fn a_recovery_key_lockdown_signs_a_fresh_nonce_every_time_and_a_replay_is_refused() {
    let fake = Fake::new(|_, n| {
        Ok(if n < 2 {
            json(
                202,
                r#"{"pending_until": "2026-10-12T09:00:00Z", "drive_token": "dt_r.0.xyz",
                    "note": "existing devices can cancel within 48 h"}"#,
            )
        } else {
            json(
                409,
                r#"{"error": "nonce_used", "message": "this lockdown request was used before"}"#,
            )
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let sign = |message: &[u8]| Ok(format!("sig({})", String::from_utf8_lossy(message)));
    let first = server.recovery_lockdown("d_1", sign).unwrap();
    let second = server.recovery_lockdown("d_1", sign).unwrap();
    for lockdown in [&first, &second] {
        assert!((16..=128).contains(&lockdown.nonce.len()), "{}", lockdown.nonce);
        assert!(lockdown.nonce.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    assert_ne!(first.nonce, second.nonce, "a new nonce for every request");
    assert_eq!(
        first.pending_until,
        azul_storage::time::parse_iso8601("2026-10-12T09:00:00Z")
    );
    assert_eq!(first.drive_token, "dt_r.0.xyz");
    assert!(!format!("{first:?}").contains("dt_r.0.xyz"), "Debug shows no token");
    let calls = fake.calls();
    for (call, lockdown) in calls.iter().zip([&first, &second]) {
        assert_eq!(call.method, Method::Post);
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/lockdown"));
        assert_eq!(header(call, "authorization"), None, "the recovery key, no drive token");
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert_eq!(body["nonce"], lockdown.nonce.as_str());
        assert_eq!(
            body["signature"],
            format!("sig(lockdown:d_1:{})", lockdown.nonce).as_str()
        );
    }
    // A replayed request: refused with its code, not a sign-in.
    match server.recovery_lockdown("d_1", sign) {
        Err(TokenError::Refused { status: 409, code, .. }) => assert_eq!(code, "nonce_used"),
        other => panic!("not refused as a replay: {other:?}"),
    }
    // No signature (the recovery key is not at hand): nothing is sent.
    let sent = fake.calls().len();
    assert!(matches!(
        server.recovery_lockdown("d_1", |_| Err(String::from("no recovery key"))),
        Err(TokenError::Config(_))
    ));
    assert_eq!(fake.calls().len(), sent);
}
