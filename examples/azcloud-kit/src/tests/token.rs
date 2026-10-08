//! The token server's API: tiers, a development sign-up, a checkout and its status, a refresh.

use azul_storage::{config::DriveAuth, config::DriveLocation, Method};

use super::{bundle, header, json, Fake, Shared, TOKEN};
use crate::{
    token::{check_token_url, is_loopback_host, CheckoutStatus, TokenError, TokenServer},
    DriveBundle,
};

/// azlin-token's price ladder (tiers.rs), as GET /v1/tiers answers.
const LADDER: &str = r#"{"tiers": [
    {"id": "100GB", "quota_bytes": 100000000000, "price_cents_month": 99,
     "price_cents_year": 990, "currency": "EUR", "first_month_free": true,
     "prepay_months": [1, 3, 6, 12, 24]},
    {"id": "1TB", "quota_bytes": 1000000000000, "price_cents_month": 499,
     "price_cents_year": 4990, "currency": "EUR", "first_month_free": true,
     "prepay_months": [1, 3, 6, 12, 24]},
    {"id": "12TB", "quota_bytes": 12000000000000, "price_cents_month": 3499,
     "price_cents_year": 34990, "currency": "EUR", "first_month_free": true,
     "prepay_months": [1, 3, 6, 12, 24]}],
  "methods": ["sepa", "bank_transfer", "prepaid", "voucher", "app_store", "card"],
  "legal": {"withdrawal_consent": "I agree that the service starts immediately."}}"#;

#[test]
fn the_tier_ladder_reads_with_its_prices() {
    let fake = Fake::new(|_, _| Ok(json(200, LADDER)));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let tiers = server.tiers().unwrap();
    assert_eq!(tiers.tiers.len(), 3);
    let first = &tiers.tiers[0];
    assert_eq!(first.id, "100GB");
    assert_eq!(first.quota_text(), "100 GB");
    assert_eq!(first.price_text(false).as_deref(), Some("EUR 0.99 a month"));
    assert_eq!(first.price_text(true).as_deref(), Some("EUR 9.90 a year"));
    assert!(first.first_month_free);
    assert_eq!(tiers.tiers[1].quota_text(), "1 TB");
    assert_eq!(tiers.tiers[2].quota_text(), "12 TB");
    assert_eq!(tiers.tiers[2].price_text(false).as_deref(), Some("EUR 34.99 a month"));
    assert!(tiers.methods.contains(&"sepa".to_string()));
    assert!(tiers.withdrawal_consent.unwrap().contains("immediately"));
    let call = &fake.calls()[0];
    assert_eq!(call.method, Method::Get);
    assert_eq!(call.url, format!("{TOKEN}/v1/tiers"));
    assert_eq!(header(call, "authorization"), None);
}

#[test]
fn a_ladder_without_prices_still_reads() {
    let fake = Fake::new(|_, _| {
        Ok(json(
            200,
            r#"{"tiers": [{"id": "100GB", "quota_bytes": 100000000000}]}"#,
        ))
    });
    let transport = Shared(fake);
    let tiers = TokenServer::new(TOKEN, &transport).unwrap().tiers().unwrap();
    assert_eq!(tiers.tiers[0].price_text(false), None);
    assert_eq!(tiers.tiers[0].quota_text(), "100 GB");
}

#[test]
fn a_development_token_server_signs_up_a_test_drive() {
    let fake = Fake::new(|_, _| {
        Ok(json(
            201,
            &bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa"),
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let drive: DriveBundle = server.create_dev_drive("Test drive", "100GB").unwrap();
    assert_eq!(drive.drive_id(), "d_1");
    assert_eq!(drive.drive_token, "dt_f.0.aaa");
    assert_eq!(drive.credentials.access_key_id, "AKID1");
    assert_eq!(drive.quota_bytes, Some(100_000_000_000));
    match &drive.entry.location {
        DriveLocation::S3 { auth, bucket, .. } => {
            assert_eq!(bucket, "d-1");
            assert!(matches!(auth, DriveAuth::Azlin { drive_id, .. } if drive_id == "d_1"));
        }
        other => panic!("not an S3 drive: {other:?}"),
    }
    let call = &fake.calls()[0];
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, format!("{TOKEN}/v1/drives"));
    assert_eq!(call.content_type, "application/json");
    let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["name"], "Test drive");
    assert_eq!(body["tier"], "100GB");
    assert_eq!(header(call, "authorization"), None, "a sign-up has no drive token");
}

#[test]
fn a_production_token_server_refuses_test_drives_with_its_own_sentence() {
    let fake = Fake::new(|_, _| {
        Ok(json(
            404,
            r#"{"error": "not_found", "message": "signups go through /v1/checkout"}"#,
        ))
    });
    let transport = Shared(fake);
    let error = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .create_dev_drive("Test drive", "100GB")
        .unwrap_err();
    assert!(error.is_checkout_only(), "{error:?}");
    match &error {
        TokenError::Refused {
            status,
            code,
            message,
        } => {
            assert_eq!(*status, 404);
            assert_eq!(code, "not_found");
            assert!(message.contains("checkout"));
        }
        other => panic!("not a refusal: {other:?}"),
    }
    assert!(error.to_string().contains("checkout"), "{error}");
}

#[test]
fn a_checkout_says_where_to_pay_and_its_status_hands_the_drive_over_once() {
    let checkout = r#"{"checkout_id": "ck_1", "pay_url": "http://127.0.0.1:18081/v1/pay/ck_1",
        "tier": "100GB", "method": "sepa", "months": 12, "amount_cents": 990, "currency": "EUR",
        "first_month_free": true, "withdrawal_consent_required": true, "mock": true}"#;
    let approved = format!(
        r#"{{"checkout_id": "ck_1", "status": "approved", "tier": "100GB", "months": 12,
             "amount_cents": 990, "signup": {}}}"#,
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")
    );
    let answers = vec![
        json(201, checkout),
        json(
            200,
            r#"{"checkout_id": "ck_1", "status": "pending", "tier": "100GB", "months": 12,
                "amount_cents": 990}"#,
        ),
        json(200, &approved),
        json(
            200,
            r#"{"checkout_id": "ck_1", "status": "approved", "tier": "100GB", "months": 12,
                "amount_cents": 990}"#,
        ),
        json(
            200,
            r#"{"checkout_id": "ck_2", "status": "declined", "tier": "100GB", "months": 1,
                "amount_cents": 99}"#,
        ),
    ];
    let fake = Fake::new(move |_, n| Ok(answers[n.min(answers.len() - 1)].clone()));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();

    let started = server.checkout("100GB", 12, "sepa").unwrap();
    assert_eq!(started.checkout_id, "ck_1");
    assert_eq!(started.pay_url, "http://127.0.0.1:18081/v1/pay/ck_1");
    assert_eq!(started.amount_cents, 990);
    assert_eq!(started.months, 12);
    assert!(started.mock);
    let call = &fake.calls()[0];
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, format!("{TOKEN}/v1/checkout"));
    let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["tier"], "100GB");
    assert_eq!(body["months"], 12);
    assert_eq!(body["method"], "sepa");

    assert_eq!(server.checkout_status("ck_1").unwrap(), CheckoutStatus::Pending);
    match server.checkout_status("ck_1").unwrap() {
        CheckoutStatus::Approved(drive) => assert_eq!(drive.drive_id(), "d_1"),
        other => panic!("not approved with its drive: {other:?}"),
    }
    assert_eq!(
        server.checkout_status("ck_1").unwrap(),
        CheckoutStatus::ApprovedElsewhere
    );
    assert!(matches!(
        server.checkout_status("ck_2").unwrap(),
        CheckoutStatus::Declined(_)
    ));
    assert_eq!(fake.calls()[1].method, Method::Get);
    assert_eq!(fake.calls()[1].url, format!("{TOKEN}/v1/checkout/ck_1"));
}

#[test]
fn a_refresh_spends_the_drive_token_and_answers_the_next_one() {
    let fake = Fake::new(|call, _| {
        if header(call, "authorization") == Some("Bearer dt_f.0.aaa") {
            Ok(json(
                200,
                &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb"),
            ))
        } else {
            Ok(json(
                401,
                r#"{"error": "token_reuse", "message": "an old token was reused"}"#,
            ))
        }
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let fresh = server.refresh("d_1", "dt_f.0.aaa").unwrap();
    assert_eq!(fresh.drive_token, "dt_f.1.bbb");
    assert_eq!(fresh.credentials.access_key_id, "AKID2");
    let call = &fake.calls()[0];
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/credentials"));
    assert!(matches!(
        server.refresh("d_1", "dt_f.0.old"),
        Err(TokenError::SignIn(_))
    ));
    assert!(matches!(server.refresh("d_1", " "), Err(TokenError::SignIn(_))));
    assert!(matches!(server.refresh(" ", "dt_x"), Err(TokenError::Config(_))));
}

#[test]
fn a_request_without_an_answer_is_a_connection_error() {
    let fake = Fake::new(|_, _| Err("connection refused".to_string()));
    let transport = Shared(fake);
    match TokenServer::new(TOKEN, &transport).unwrap().tiers() {
        Err(TokenError::Connect(why)) => assert!(why.contains("refused")),
        other => panic!("not a connection error: {other:?}"),
    }
}

#[test]
fn an_unencrypted_token_server_must_be_on_this_computer() {
    assert!(check_token_url("http://127.0.0.1:8081").is_ok());
    assert!(check_token_url("http://localhost:8081/").is_ok());
    assert!(check_token_url("http://[::1]:8081").is_ok());
    assert!(check_token_url("https://token.example").is_ok());
    assert!(matches!(
        check_token_url("http://token.example"),
        Err(TokenError::Config(_))
    ));
    assert!(check_token_url("ftp://127.0.0.1").is_err());
    assert!(check_token_url("").is_err());
    assert!(is_loopback_host("127.0.0.1"));
    assert!(is_loopback_host("[::1]"));
    assert!(!is_loopback_host("example.com"));
    let fake = Fake::new(|_, _| Ok(json(200, "{}")));
    let transport = Shared(fake);
    assert!(TokenServer::new("http://token.example", &transport).is_err());
}

#[test]
fn an_answer_that_is_no_bundle_is_a_protocol_error() {
    let fake = Fake::new(|_, _| Ok(json(201, r#"{"drive": {"id": "d_1"}}"#)));
    let transport = Shared(fake);
    assert!(matches!(
        TokenServer::new(TOKEN, &transport)
            .unwrap()
            .create_dev_drive("x", "100GB"),
        Err(TokenError::Protocol(_))
    ));
}
