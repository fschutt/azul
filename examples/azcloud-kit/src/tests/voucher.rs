//! A voucher (AZLINSEC17 F29): on a drive it answers the days it added (its months and its
//! value pro rata); without one, a new drive of its tier.

use azul_storage::Method;

use super::{bundle, header, json, Fake, Shared, TOKEN};
use crate::token::{TokenError, TokenServer, VoucherRedeemed};

#[test]
fn a_voucher_on_a_drive_answers_the_days_it_added() {
    let fake = Fake::new(|call, n| {
        Ok(match n {
            0 => {
                assert_eq!(call.method, Method::Post);
                assert_eq!(call.url, format!("{TOKEN}/v1/vouchers/redeem"));
                assert_eq!(header(call, "authorization"), Some("Bearer dt_a.1.x"));
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
    let drive = Some(("d_1", "dt_a.1.x"));
    match server.redeem_voucher("AZ-TEST-1234", drive, "").unwrap() {
        VoucherRedeemed::Extended {
            days_added,
            period_until,
        } => {
            assert_eq!(days_added, 45);
            assert_eq!(
                period_until,
                azul_storage::time::parse_iso8601("2026-12-22T00:00:00Z")
            );
        }
        other => panic!("not extended: {other:?}"),
    }
    match server.redeem_voucher("AZ-TEST-1234", drive, "").unwrap() {
        VoucherRedeemed::Extended { days_added, .. } => assert_eq!(days_added, 60),
        other => panic!("not extended: {other:?}"),
    }
    match server.redeem_voucher("AZ-TEST-1234", drive, "") {
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
    match server.redeem_voucher("AZ-TEST-1234", None, "1TB").unwrap() {
        VoucherRedeemed::NewDrive(bundle) => {
            assert_eq!(bundle.drive_id(), "d_1");
            assert_eq!(bundle.drive_token, "dt_f.0.aaa");
        }
        other => panic!("not a new drive: {other:?}"),
    }
    assert!(matches!(
        server.redeem_voucher(" ", None, "1TB"),
        Err(TokenError::Config(_))
    ));
}
