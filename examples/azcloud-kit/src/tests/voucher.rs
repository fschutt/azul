//! A voucher (AZLINSEC17 F29): on a drive it answers the days it added (its months and its
//! value pro rata); without one, a new drive of its tier.

use azul_storage::Method;

use super::{bundle, grant_key, header, json, Fake, Shared, TOKEN};
use crate::token::{TokenError, TokenServer, VoucherRedeemed};

#[test]
fn a_voucher_on_a_drive_answers_the_days_it_added() {
    let fake = Fake::new(|call, n| {
        Ok(match n {
            0 => {
                assert_eq!(call.method, Method::Post);
                assert_eq!(call.url, format!("{TOKEN}/v1/vouchers/redeem"));
                assert_eq!(header(call, "authorization"), Some("Bearer dt_a.1.x"));
                // a lost answer is asked again with the same key: its first answer, not "invalid"
                assert_eq!(
                    header(call, azlin_contract::IDEMPOTENCY_HEADER),
                    Some(grant_key().header_value().as_str())
                );
                let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
                assert_eq!(body["code"], "AZ-TEST-1234");
                assert_eq!(body["drive_id"], "d_1");
                json(
                    200,
                    r#"{"months_added": 1, "days_added": 45,
                        "period_until": "2026-12-22T00:00:00Z"}"#,
                )
            }
            // A token server from before days_added: its months.
            1 => json(
                200,
                r#"{"months_added": 2, "period_until": "2027-01-06T00:00:00Z"}"#,
            ),
            _ => json(
                400,
                r#"{"error": "voucher_too_small",
                    "message": "this voucher is worth less than a day of this tier"}"#,
            ),
        })
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let key = grant_key();
    let drive = Some(("d_1", "dt_a.1.x"));
    match server.redeem_voucher("AZ-TEST-1234", drive, "", &key).unwrap() {
        VoucherRedeemed::Extended {
            days_added,
            period_until,
            grant,
        } => {
            // PT3: the drive's credentials must follow the days bought
            assert_eq!(grant.drive_id(), "d_1");
            assert_eq!(days_added, 45);
            assert_eq!(
                period_until,
                azul_storage::time::parse_iso8601("2026-12-22T00:00:00Z")
            );
        }
        other => panic!("not extended: {other:?}"),
    }
    match server.redeem_voucher("AZ-TEST-1234", drive, "", &key).unwrap() {
        VoucherRedeemed::Extended { days_added, .. } => assert_eq!(days_added, 60),
        other => panic!("not extended: {other:?}"),
    }
    match server.redeem_voucher("AZ-TEST-1234", drive, "", &key) {
        Err(TokenError::Refused { status: 400, code, .. }) => {
            assert_eq!(code, "voucher_too_small");
        }
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_voucher_without_a_drive_answers_a_new_drive_of_its_tier() {
    let fake = Fake::new(|call, _| {
        assert_eq!(header(call, "authorization"), None);
        // the new drive's grant is named too: a lost answer gets the same drive again
        assert_eq!(
            header(call, azlin_contract::IDEMPOTENCY_HEADER),
            Some(grant_key().header_value().as_str())
        );
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert_eq!(body["code"], "AZ-TEST-1234");
        assert_eq!(body["tier"], "1TB");
        assert!(body.get("drive_id").is_none(), "no drive: a new one");
        Ok(json(
            201,
            &bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa"),
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let key = grant_key();
    match server.redeem_voucher("AZ-TEST-1234", None, "1TB", &key).unwrap() {
        VoucherRedeemed::NewDrive(bundle) => {
            assert_eq!(bundle.drive_id(), "d_1");
            assert_eq!(bundle.drive_token, "dt_f.0.aaa");
        }
        other => panic!("not a new drive: {other:?}"),
    }
    assert!(matches!(
        server.redeem_voucher(" ", None, "1TB", &key),
        Err(TokenError::Config(_))
    ));
}

/// A voucher that makes a new drive, its answer lost: the next try carries the same key (kept in
/// the keyring's outbox, the code named only by its hash), so the token server answers the drive
/// it made instead of "not valid" - the voucher is spent by then.
#[test]
fn a_new_drive_s_voucher_whose_answer_was_lost_is_asked_again_with_the_same_key() {
    let keys = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = keys.clone();
    let fake = Fake::new(move |call, n| {
        let key = header(call, azlin_contract::IDEMPOTENCY_HEADER).unwrap_or_default();
        seen.lock().unwrap().push(key.to_string());
        if n == 0 {
            return Err(String::from("timed out"));
        }
        Ok(json(201, &bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.1.bbb")))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let dir = azul_storage::testing::TempDir::new("azcloud-voucher");
    let shared = super::keyring_holding(&dir, "dt_a.1.x");
    let lost = crate::period::redeem_voucher_for_new_drive(&server, &shared, "AZ-TEST-1234", "1TB");
    assert!(lost.is_err(), "no answer: {lost:?}");
    match crate::period::redeem_voucher_for_new_drive(&server, &shared, "AZ-TEST-1234", "1TB") {
        Ok(VoucherRedeemed::NewDrive(bundle)) => assert_eq!(bundle.drive_token, "dt_f.1.bbb"),
        other => panic!("not the drive: {other:?}"),
    }
    // answered: the same words again are another request, with a key of its own
    let fresh = crate::period::redeem_voucher_for_new_drive(&server, &shared, "AZ-TEST-1234", "1TB");
    assert!(fresh.is_ok(), "{fresh:?}");
    let keys = keys.lock().unwrap().clone();
    assert_eq!(keys.len(), 3, "{keys:?}");
    assert!(!keys[0].is_empty() && keys[0] == keys[1], "the same key: {keys:?}");
    assert_ne!(keys[2], keys[0], "a closed call's key is not used again: {keys:?}");
}
