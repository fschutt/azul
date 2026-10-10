//! A recovery-key lockdown (AZDRIVE-INTEGRATION §4, AZLINSEC17 F14): a fresh nonce for every
//! request, signed with the recovery key; the token server answers a replay with 409.

use azul_storage::Method;

use super::{header, json, Fake, Shared, TOKEN};
use crate::token::{recovery_lockdown_message, TokenError, TokenServer};

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
