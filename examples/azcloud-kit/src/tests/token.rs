//! The token server's API: tiers, a development sign-up, a checkout and its sealed sign-up, a
//! refresh.

use azul_storage::{config::DriveAuth, config::DriveLocation, HttpReply, Method};

use super::{bundle, header, json, Fake, Shared, TOKEN};
use crate::{
    claim::seal,
    token::{
        check_id, check_token_url, is_loopback_host, CheckoutStatus, CheckoutVia, OptionsQuery,
        TokenError, TokenServer,
    },
    ClaimKey, CloudError, DriveBundle,
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
fn a_checkout_names_its_claim_key_and_its_status_opens_the_sealed_signup_every_time() {
    let claim = ClaimKey::generate().unwrap();
    let checkout = r#"{"checkout_id": "ck_1", "pay_url": "http://127.0.0.1:18081/v1/pay/ck_1",
        "tier": "100GB", "method": "sepa", "months": 12, "amount_cents": 990, "currency": "EUR",
        "first_month_free": true, "withdrawal_consent_required": true, "mock": true}"#;
    let sealed = seal(
        &claim.public_base64(),
        "ck_1",
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    let approved = format!(
        r#"{{"checkout_id": "ck_1", "status": "approved", "tier": "100GB", "months": 12,
             "amount_cents": 990, "sealed_signup": "{sealed}"}}"#
    );
    let answers = vec![
        json(201, checkout),
        json(
            200,
            r#"{"checkout_id": "ck_1", "status": "pending", "tier": "100GB", "months": 12,
                "amount_cents": 990}"#,
        ),
        json(200, &approved),
        // The token server keeps the sealed sign-up 30 days: a second poll opens it again.
        json(200, &approved),
        json(
            200,
            r#"{"checkout_id": "ck_2", "status": "declined", "tier": "100GB", "months": 1,
                "amount_cents": 99, "reason": "insufficient funds"}"#,
        ),
        json(
            200,
            r#"{"checkout_id": "ck_3", "status": "reversed", "tier": "100GB", "months": 1,
                "amount_cents": 99}"#,
        ),
    ];
    let fake = Fake::new(move |_, n| Ok(answers[n.min(answers.len() - 1)].clone()));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();

    let started = server.checkout("100GB", 12, "sepa", &claim).unwrap();
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
    // The public half of the claim key, never its secret.
    assert_eq!(body["claim_key"], claim.public_base64().as_str());
    let sent = String::from_utf8_lossy(&call.body).into_owned();
    assert!(!sent.contains(claim.to_base64().as_str()), "{sent}");

    assert_eq!(
        server.checkout_status("ck_1", &claim).unwrap(),
        CheckoutStatus::Pending
    );
    for _ in 0..2 {
        match server.checkout_status("ck_1", &claim).unwrap() {
            CheckoutStatus::Approved(drive) => {
                assert_eq!(drive.drive_id(), "d_1");
                assert_eq!(drive.drive_token, "dt_f.0.aaa");
                assert_eq!(drive.credentials.access_key_id, "AKID1");
            }
            other => panic!("not approved with its drive: {other:?}"),
        }
    }
    assert_eq!(
        server.checkout_status("ck_2", &claim).unwrap(),
        CheckoutStatus::Declined(String::from("insufficient funds"))
    );
    assert!(matches!(
        server.checkout_status("ck_3", &claim).unwrap(),
        CheckoutStatus::Declined(_)
    ));
    assert_eq!(fake.calls()[1].method, Method::Get);
    assert_eq!(fake.calls()[1].url, format!("{TOKEN}/v1/checkout/ck_1"));
}

#[test]
fn a_checkout_the_token_server_no_longer_has_is_gone() {
    let fake = Fake::new(|call, _| {
        Ok(if call.url.ends_with("/ck_old") {
            json(
                200,
                r#"{"checkout_id": "ck_old", "status": "expired", "tier": "100GB", "months": 1,
                    "amount_cents": 99}"#,
            )
        } else {
            json(
                404,
                r#"{"error": "no_such_checkout", "message": "unknown checkout"}"#,
            )
        })
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let claim = ClaimKey::generate().unwrap();
    assert!(matches!(
        server.checkout_status("ck_old", &claim).unwrap(),
        CheckoutStatus::Gone(_)
    ));
    match server.checkout_status("ck_nobody", &claim).unwrap() {
        CheckoutStatus::Gone(why) => assert!(why.contains("unknown checkout"), "{why}"),
        other => panic!("a 404 is a checkout that is gone, not {other:?}"),
    }
}

#[test]
fn an_approved_checkout_whose_signup_is_not_sealed_to_its_claim_key_is_refused() {
    let claim = ClaimKey::generate().unwrap();
    let other = ClaimKey::generate().unwrap();
    let signup = bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa");
    let answers = vec![
        // A token server without claims: the plaintext sign-up, readable by anyone who knows
        // the checkout id - not taken.
        format!(r#"{{"checkout_id": "ck_1", "status": "approved", "signup": {signup}}}"#),
        // Sealed to another checkout's key.
        format!(
            r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{}"}}"#,
            seal(&other.public_base64(), "ck_1", signup.as_bytes())
        ),
        // Sealed for another checkout.
        format!(
            r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{}"}}"#,
            seal(&claim.public_base64(), "ck_2", signup.as_bytes())
        ),
        // Opens, but holds no drive bundle.
        format!(
            r#"{{"checkout_id": "ck_1", "status": "approved", "sealed_signup": "{}"}}"#,
            seal(&claim.public_base64(), "ck_1", b"{}")
        ),
    ];
    let fake = Fake::new(move |_, n| Ok(json(200, &answers[n.min(answers.len() - 1)])));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    for case in 0..4 {
        assert!(
            matches!(
                server.checkout_status("ck_1", &claim),
                Err(TokenError::Protocol(_))
            ),
            "answer {case}"
        );
    }
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

#[test]
fn a_named_refresh_tells_the_token_server_what_this_device_calls_the_drive() {
    let fake = Fake::new(|_, _| {
        Ok(json(
            200,
            &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb"),
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let fresh = server
        .refresh_named("d_1", "dt_f.0.aaa", " Ann's drive ")
        .unwrap();
    assert_eq!(fresh.drive_token, "dt_f.1.bbb");
    let call = &fake.calls()[0];
    assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/credentials"));
    assert_eq!(header(call, "authorization"), Some("Bearer dt_f.0.aaa"));
    let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["name"], "Ann's drive");
}

#[test]
fn the_account_calls_go_to_the_drives_routes_with_this_devices_drive_token() {
    let fake = Fake::new(|call, _| {
        let url = call.url.as_str();
        Ok(match call.method {
            Method::Post if url.ends_with("/members") => json(
                201,
                r#"{"member": "m_laptop", "drive_token": "dt_m.0.joins"}"#,
            ),
            Method::Post if url.ends_with("/lockdown/cancel") => json(204, ""),
            Method::Post if url.ends_with("/restore") => json(
                202,
                r#"{"request_id": "rs_1", "status": "queued"}"#,
            ),
            Method::Get if url.ends_with("/restore/rs_1") => json(
                200,
                r#"{"request_id": "rs_1", "status": "done"}"#,
            ),
            Method::Get if url.ends_with("/v1/drives/d_1") => {
                json(200, r#"{"id": "d_1", "tier": "100GB"}"#)
            }
            _ => json(404, r#"{"error": "not_found"}"#),
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let member = server
        .add_member("d_1", "dt_f.0.aaa", Some("laptop"))
        .unwrap();
    assert_eq!(member["drive_token"], "dt_m.0.joins");
    assert_eq!(server.info("d_1", "dt_f.0.aaa").unwrap()["tier"], "100GB");
    assert_eq!(
        server.lockdown_cancel("d_1", "dt_f.0.aaa").unwrap(),
        serde_json::Value::Null,
        "an empty answer is no error"
    );
    let queued = server
        .restore("d_1", "dt_f.0.aaa", "docs/", "2026-10-08T09:00:00Z")
        .unwrap();
    assert_eq!(queued["request_id"], "rs_1");
    assert_eq!(
        server
            .restore_status("d_1", "dt_f.0.aaa", "rs_1")
            .unwrap()["status"],
        "done"
    );
    let calls = fake.calls();
    assert_eq!(calls.len(), 5);
    assert_eq!(calls[0].url, format!("{TOKEN}/v1/drives/d_1/members"));
    let body: serde_json::Value = serde_json::from_slice(&calls[0].body).unwrap();
    assert_eq!(body["member"], "laptop");
    assert_eq!(calls[1].method, Method::Get);
    assert_eq!(calls[1].url, format!("{TOKEN}/v1/drives/d_1"));
    assert_eq!(
        calls[2].url,
        format!("{TOKEN}/v1/drives/d_1/lockdown/cancel")
    );
    assert_eq!(calls[3].url, format!("{TOKEN}/v1/drives/d_1/restore"));
    let body: serde_json::Value = serde_json::from_slice(&calls[3].body).unwrap();
    assert_eq!(body["prefix"], "docs/");
    assert_eq!(body["as_of"], "2026-10-08T09:00:00Z");
    assert_eq!(calls[4].url, format!("{TOKEN}/v1/drives/d_1/restore/rs_1"));
    for call in &calls {
        assert_eq!(header(call, "authorization"), Some("Bearer dt_f.0.aaa"));
    }
    assert!(matches!(
        server.info("../d_1", "dt_f.0.aaa"),
        Err(TokenError::Config(_))
    ));
    assert!(matches!(
        server.restore_status("d_1", "dt_f.0.aaa", "rs_1/../x"),
        Err(TokenError::Config(_))
    ));
    assert!(matches!(
        server.lockdown("d_1", " "),
        Err(TokenError::SignIn(_))
    ));
    assert_eq!(fake.calls().len(), 5, "a bad id or no token sends nothing");
}

#[test]
fn an_id_that_could_change_the_url_is_refused() {
    assert!(check_id("d_k3f9-x").is_ok());
    for bad in ["", "../keys", "d_1/members", "d 1", "d_1?x", "%2e%2e"] {
        assert!(check_id(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_refused_drive_token_means_signing_in_again_under_any_context() {
    let refused = CloudError::from(TokenError::SignIn(String::from(
        "an old token was reused, token_reuse",
    )))
    .context("credential refresh");
    assert!(refused.is_sign_in());
    let text = refused.to_string();
    assert!(text.starts_with("credential refresh: "), "{text}");
    assert!(text.contains("token_reuse") && text.contains("sign in"), "{text}");
    let busy = CloudError::from(TokenError::Refused {
        status: 503,
        code: String::from("busy"),
        message: String::new(),
    });
    assert!(!busy.is_sign_in());
}

// ==== The checkout through a provider (CHECKOUT-PLAN §3.11: claim contract v1, extended) ====

#[test]
fn the_payment_options_are_asked_for_the_tier_period_country_and_surfaces() {
    let fake = Fake::new(|_, _| {
        Ok(json(
            200,
            r#"{"offers": [{"provider": "stripe", "methods": [{"method": "card"}]}]}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let query = OptionsQuery {
        tier: "1TB",
        months: 12,
        country: "DE",
        currency: "EUR",
        surfaces: &["fields", "page", "browser"],
    };
    let text = server.checkout_options(&query).unwrap().expect("options");
    assert!(text.contains("\"stripe\""), "{text}");
    let call = &fake.calls()[0];
    assert_eq!(call.method, Method::Get);
    assert_eq!(
        call.url,
        format!(
            "{TOKEN}/v1/checkout/options?tier=1TB&months=12&country=DE&currency=EUR&\
             surfaces=fields%2Cpage%2Cbrowser"
        )
    );
    assert_eq!(header(call, "authorization"), None, "the options are no account call");
}

#[test]
fn a_token_server_without_payment_options_answers_none() {
    let fake = Fake::new(|_, _| {
        Ok(json(404, r#"{"error": "not_found", "message": "no route"}"#))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let query = OptionsQuery {
        tier: "100GB",
        months: 1,
        country: "de",
        currency: "EUR",
        surfaces: &[],
    };
    assert_eq!(server.checkout_options(&query).unwrap(), None);
    let failing = Fake::new(|_, _| Ok(json(500, r#"{"error": "internal"}"#)));
    let transport = Shared(failing);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        server.checkout_options(&query),
        Err(TokenError::Refused { status: 500, .. })
    ));
}

#[test]
fn a_checkout_through_a_provider_names_it_and_answers_its_surface() {
    let claim = ClaimKey::generate().unwrap();
    let fake = Fake::new(|_, _| {
        Ok(json(
            201,
            r#"{"checkout_id": "ck_9", "pay_url": "https://pay.azlin.io/legacy/ck_9",
                "provider": "stripe", "method": "card", "tier": "1TB", "months": 12,
                "amount_cents": 4990, "currency": "EUR",
                "surface": {"kind": "fields", "page": "https://pay.azlin.io/fields/stripe/v1",
                            "publishable_key": "pk_test_1", "client_secret": "pi_1_secret_2"}}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let via = CheckoutVia {
        provider: "stripe",
        method: "card",
        surface: "fields",
        vat_country: "DE",
        withdrawal_consent: true,
    };
    let (started, answer) = server.checkout_via("1TB", 12, &via, &claim).unwrap();
    assert_eq!(started.checkout_id, "ck_9");
    assert_eq!(started.amount_cents, 4990);
    assert_eq!(started.months, 12);
    assert_eq!(answer["surface"]["kind"], "fields");
    assert_eq!(answer["surface"]["client_secret"], "pi_1_secret_2");
    let call = &fake.calls()[0];
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, format!("{TOKEN}/v1/checkout"));
    let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["tier"], "1TB");
    assert_eq!(body["months"], 12);
    assert_eq!(body["provider"], "stripe");
    assert_eq!(body["method"], "card");
    assert_eq!(body["surface"], "fields");
    assert_eq!(body["vat_country"], "DE");
    assert_eq!(body["withdrawal_consent"], true);
    assert_eq!(body["claim_key"], claim.public_base64().as_str());
    let sent = String::from_utf8_lossy(&call.body).into_owned();
    assert!(!sent.contains(claim.to_base64().as_str()), "{sent}");
}

#[test]
fn a_checkout_through_a_provider_needs_an_id_but_no_payment_page() {
    let claim = ClaimKey::generate().unwrap();
    let via = CheckoutVia {
        provider: "fake-paypal",
        method: "paypal",
        surface: "browser",
        vat_country: "DE",
        withdrawal_consent: true,
    };
    let without_page = Fake::new(|_, _| {
        Ok(json(
            201,
            r#"{"checkout_id": "ck_10", "surface": {"kind": "browser",
                "url": "http://localhost:18081/fake-paypal/checkoutnow?token=ck_10"}}"#,
        ))
    });
    let transport = Shared(without_page);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let (started, _) = server.checkout_via("100GB", 12, &via, &claim).unwrap();
    assert_eq!(started.checkout_id, "ck_10");
    assert_eq!(started.months, 12, "the months asked for when the answer names none");
    let without_id = Fake::new(|_, _| Ok(json(201, r#"{"surface": {"kind": "browser"}}"#)));
    let transport = Shared(without_id);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        server.checkout_via("100GB", 12, &via, &claim),
        Err(TokenError::Protocol(_))
    ));
}

#[test]
fn the_same_checkout_moves_to_another_surface_and_is_abandoned() {
    let fake = Fake::new(|call, _| {
        Ok(if call.url.ends_with("/surface") {
            json(
                200,
                r#"{"checkout_id": "ck_9", "surface": {"kind": "browser",
                    "url": "https://checkout.stripe.com/c/pay/cs_test_1"}}"#,
            )
        } else {
            HttpReply {
                status: 204,
                headers: Vec::new(),
                body: Vec::new(),
            }
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let answer = server.checkout_surface("ck_9", "browser").unwrap();
    assert_eq!(answer["surface"]["kind"], "browser");
    server.abandon_checkout("ck_9").unwrap();
    let calls = fake.calls();
    assert_eq!(calls[0].method, Method::Post);
    assert_eq!(calls[0].url, format!("{TOKEN}/v1/checkout/ck_9/surface"));
    let body: serde_json::Value = serde_json::from_slice(&calls[0].body).unwrap();
    assert_eq!(body["kind"], "browser");
    assert_eq!(calls[1].method, Method::Post);
    assert_eq!(calls[1].url, format!("{TOKEN}/v1/checkout/ck_9/abandon"));
    assert!(matches!(
        server.abandon_checkout("ck/../drives"),
        Err(TokenError::Config(_))
    ));
    assert!(matches!(
        server.checkout_surface("", "browser"),
        Err(TokenError::Config(_))
    ));
    assert_eq!(fake.calls().len(), 2, "a bad id never reaches the network");
}

#[test]
fn only_a_401_to_a_call_with_the_drive_token_means_signing_in_again() {
    let fake = Fake::new(|call, _| {
        Ok(if call.url.ends_with("/credentials") {
            json(
                403,
                r#"{"error": "forbidden", "message": "the pending device cannot do that"}"#,
            )
        } else {
            json(
                403,
                r#"{"error": "issue_key_wrong", "message": "not this checkout's issue key"}"#,
            )
        })
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // A 403 to a call with the drive token: the token server refused this one call, the token
    // is not gone.
    match server.refresh("d_1", "dt_f.0.aaa") {
        Err(TokenError::Refused { status, code, .. }) => {
            assert_eq!(status, 403);
            assert_eq!(code, "forbidden");
        }
        other => panic!("not a refusal: {other:?}"),
    }
    // A 403 to a call without one: its code is the answer.
    match server.tiers() {
        Err(TokenError::Refused { status, code, .. }) => {
            assert_eq!(status, 403);
            assert_eq!(code, "issue_key_wrong");
        }
        other => panic!("not a refusal: {other:?}"),
    }
}

#[test]
fn a_refresh_answered_503_is_tried_again_with_the_same_token_and_never_drops_it() {
    let not_verified =
        r#"{"error": "not_verified", "message": "not verified yet; try again later"}"#;
    let try_again = r#"{"error": "try_again", "message": "the token changed meanwhile"}"#;
    let fresh = bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb");
    let fake = Fake::new(move |_, n| {
        Ok(match n {
            0 => json(503, not_verified),
            1 => json(503, try_again),
            _ => json(200, &fresh),
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(std::time::Duration::ZERO);
    let bundle = server.refresh("d_1", "dt_f.0.aaa").unwrap();
    assert_eq!(bundle.drive_token, "dt_f.1.bbb");
    let calls = fake.calls();
    assert_eq!(calls.len(), 3, "two 503s, then the answer");
    for call in &calls {
        assert_eq!(
            header(call, "authorization"),
            Some("Bearer dt_f.0.aaa"),
            "the SAME token"
        );
    }
    // A token server that keeps answering 503: a refusal after three tries - the token stays
    // the device's (only a 401 says it is gone).
    let busy = Fake::new(move |_, _| Ok(json(503, not_verified)));
    let transport = Shared(busy.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(std::time::Duration::ZERO);
    let error = server.refresh("d_1", "dt_f.0.aaa").unwrap_err();
    assert!(
        matches!(&error, TokenError::Refused { status: 503, code, .. } if code == "not_verified"),
        "{error:?}"
    );
    assert!(!CloudError::from(error).is_sign_in());
    assert_eq!(busy.calls().len(), 3);
}

/// The issue key of a test checkout's period tokens (base64url of 32 bytes).
const ISSUE_KEY: &str = "Zm9yIHRoZSBwZXJpb2QgdG9rZW5zIG9mIGNrXzEgb25seQ";

#[test]
fn a_paid_checkouts_signup_carries_the_issue_key_of_its_period_tokens() {
    let signup = bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").replace(
        r#""tier": "100GB"}"#,
        &format!(
            r#""tier": "100GB", "period_tokens": {{"checkout_id": "ck_1", "months": 3,
                 "issue_key": "{ISSUE_KEY}"}}}}"#
        ),
    );
    let drive = DriveBundle::parse(&signup).unwrap();
    let period = drive.period_tokens.clone().expect("the period tokens' grant");
    assert_eq!(period.checkout_id, "ck_1");
    assert_eq!(period.months, 3);
    assert_eq!(period.issue_key, ISSUE_KEY);
    let shown = format!("{drive:?} {period:?}");
    assert!(!shown.contains(ISSUE_KEY), "Debug shows no issue key: {shown}");
    // A development sign-up (no payment) has none.
    let free = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    assert_eq!(free.period_tokens, None);
}

#[test]
fn period_tokens_are_issued_only_with_the_issue_key_the_sealed_signup_carries() {
    let fake = Fake::new(|call, n| {
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        Ok(match n {
            0 => {
                assert_eq!(body["checkout_id"], "ck_1");
                assert_eq!(body["issue_key"], ISSUE_KEY);
                assert_eq!(body["blinded"], serde_json::json!(["Ymxp", "bmQ="]));
                json(
                    200,
                    r#"{"tier": "100GB", "key_id": "100GB/2026",
                        "public_key_pem": "-----BEGIN PUBLIC KEY-----",
                        "blind_signatures": ["c2ln", "bmVk"]}"#,
                )
            }
            1 => json(
                400,
                r#"{"error": "issue_key_required", "message": "issue_key required"}"#,
            ),
            2 => json(
                403,
                r#"{"error": "issue_key_wrong", "message": "not this checkout's issue key"}"#,
            ),
            _ => json(
                200,
                r#"{"tier": "100GB", "key_id": "100GB/2026", "public_key_pem": "",
                    "blind_signatures": ["c2ln"]}"#,
            ),
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let blinded = vec![String::from("Ymxp"), String::from("bmQ=")];
    let issued = server
        .issue_period_tokens("ck_1", ISSUE_KEY, &blinded)
        .unwrap();
    assert_eq!(issued.tier, "100GB");
    assert_eq!(issued.key_id, "100GB/2026");
    assert_eq!(issued.signatures, vec!["c2ln", "bmVk"]);
    let call = &fake.calls()[0];
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, format!("{TOKEN}/v1/tokens/issue"));
    assert_eq!(header(call, "authorization"), None, "no drive token");
    // No issue key: nothing is sent (the checkout id alone issues nothing).
    assert!(matches!(
        server.issue_period_tokens("ck_1", " ", &blinded),
        Err(TokenError::Config(_))
    ));
    assert_eq!(fake.calls().len(), 1);
    // The token server's two refusals, as refusals with their codes.
    for code in ["issue_key_required", "issue_key_wrong"] {
        match server.issue_period_tokens("ck_1", ISSUE_KEY, &blinded) {
            Err(TokenError::Refused { code: got, .. }) => assert_eq!(got, code),
            other => panic!("not refused with {code}: {other:?}"),
        }
    }
    // One signature for two blinded messages: an answer that makes no sense.
    assert!(matches!(
        server.issue_period_tokens("ck_1", ISSUE_KEY, &blinded),
        Err(TokenError::Protocol(_))
    ));
}
