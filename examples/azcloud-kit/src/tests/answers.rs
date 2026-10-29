//! Every answer of the token server read once, by what it means: a refusal that spent nothing
//! (a rate limit, a provider that did not answer) is asked again with the same token, the same
//! idempotency key and the same order; a pause the server names is kept, a long one is the
//! caller's to wait out and said in the user's words; a refusal that does not pass says what it
//! is.

use std::{sync::Arc, time::Duration};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, HttpReply, Transport,
};

use super::{bundle, grant_key, header, json, keyring_holding, Fake, Shared, TOKEN};
use crate::{
    drive::TransportFactory,
    sync::retry::{classify_drive, Verdict},
    token::{CheckoutFor, IssueAnswer, TokenServer, VoucherRedeemed},
    user_errors::{Class, Code, Lang, UserError},
    AzlinDrive, AzlinSession, ClaimKey, CloudError, DriveBundle, SharedKeyring,
};

/// The token server's rate limit, as azlin-token answers it: 429 `rate_limited`, nothing spent,
/// and how many seconds to wait.
fn rate_limited(retry_after: &str) -> HttpReply {
    let mut reply = json(
        429,
        r#"{"error": "rate_limited",
            "message": "too many requests from this address; try again in a minute",
            "spent": false}"#,
    );
    reply
        .headers
        .push((String::from("retry-after"), retry_after.to_string()));
    reply
}

/// A checkout's answer (claim contract v1).
const CHECKOUT: &str = r#"{"checkout_id": "ck_1", "pay_url": "http://127.0.0.1:18081/v1/pay/ck_1",
    "tier": "100GB", "method": "sepa", "months": 12, "amount_cents": 990, "currency": "EUR",
    "first_month_free": true, "withdrawal_consent_required": true, "mock": true}"#;

/// The issue key of a test checkout's period tokens (base64url of 32 bytes).
const ISSUE_KEY: &str = "Zm9yIHRoZSBwZXJpb2QgdG9rZW5zIG9mIGNrXzEgb25seQ";

fn authorizations(fake: &Fake) -> Vec<String> {
    fake.calls()
        .iter()
        .map(|c| header(c, "authorization").unwrap_or_default().to_string())
        .collect()
}

#[test]
fn a_rate_limited_refresh_waits_and_retries_with_the_same_token() {
    let dir = TempDir::new("azcloud-answers-refresh");
    let shared = keyring_holding(&dir, "dt_f.0.aaa");
    let fake = Fake::new(|_, n| {
        Ok(if n == 0 {
            rate_limited("2")
        } else {
            json(200, &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb"))
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let rotation = server
        .refresh(shared.spend("d_1").unwrap(), None)
        .rotated()
        .expect("asked again after the pause");
    assert_eq!(rotation.bundle().drive_token, "dt_f.1.bbb");
    assert_eq!(
        authorizations(&fake),
        vec!["Bearer dt_f.0.aaa", "Bearer dt_f.0.aaa"],
        "the same token both times: a rate limit spends nothing"
    );
}

#[test]
fn a_rate_limited_refresh_keeps_its_token_and_says_how_long_to_wait() {
    let dir = TempDir::new("azcloud-answers-refresh-long");
    let shared = keyring_holding(&dir, "dt_f.0.aaa");
    let fake = Fake::new(|_, _| Ok(rate_limited("60")));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let error = server
        .refresh(shared.spend("d_1").unwrap(), None)
        .rotated()
        .expect_err("still limited");
    assert_eq!(fake.calls().len(), 1, "a minute is the caller's to wait out");
    assert!(!CloudError::from(error.clone()).is_sign_in());
    let user = UserError::from_token_error(&error).expect("an error the user sees");
    assert_eq!(user.class(), Class::Retry, "it passes by itself");
    assert_eq!(user.retry_after, Some(60));
    let kept = shared.spend("d_1").unwrap();
    assert_eq!(kept.token().as_str(), "dt_f.0.aaa", "the keyring keeps the token");
}

#[test]
fn a_rate_limited_refresh_tells_the_sync_how_long_to_wait() {
    let dir = TempDir::new("azcloud-answers-drive");
    let keyring = Arc::new(MemoryKeyring::new());
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T09:30:00Z", "dt_f.0.aaa")).unwrap();
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let fake = Fake::new(|call, _| {
        assert!(call.url.ends_with("/credentials"), "the refresh comes first: {}", call.url);
        Ok(rate_limited("60"))
    });
    let shared = SharedKeyring::with_lock_root(
        keyring.clone(),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    let answering = fake.clone();
    let factory: TransportFactory =
        Arc::new(move || Box::new(Shared(answering.clone())) as Box<dyn Transport>);
    let stored = AzlinSession::from_keyring_secret(
        &keyring.get(&keyring_key("d_1")).unwrap().unwrap(),
    )
    .unwrap();
    // The credentials run out within the hour: the drive refreshes them first.
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        stored,
        TOKEN,
        shared,
        factory,
        Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
    )
    .unwrap()
    .with_clock(|| 1_791_450_000);
    let error = drive.head("a.txt").expect_err("the token server asked to wait");
    assert_eq!(
        classify_drive(&error),
        Verdict::Retry(Some(Duration::from_secs(60))),
        "the sync waits as long as the token server said: {error:?}"
    );
    let user = UserError::from_drive_error(&error).expect("an error the user sees");
    assert_ne!(user.code, Code::Other, "never a generic error");
    assert_eq!(user.retry_after, Some(60));
}

#[test]
fn a_rate_limited_checkout_is_asked_again_with_the_same_order() {
    let claim = ClaimKey::generate().unwrap();
    let fake = Fake::new(|_, n| Ok(if n == 0 { rate_limited("2") } else { json(201, CHECKOUT) }));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let started = server
        .checkout("100GB", 12, "sepa", &claim, CheckoutFor::NewDrive)
        .expect("asked again after the pause");
    assert_eq!(started.checkout_id, "ck_1");
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].url, calls[1].url);
    assert_eq!(calls[0].body, calls[1].body, "the same order, the same claim key");
}

#[test]
fn a_rate_limited_voucher_keeps_its_idempotency_key() {
    let key = grant_key();
    let fake = Fake::new(|_, n| {
        Ok(if n == 0 {
            rate_limited("2")
        } else {
            json(201, &bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.1.bbb"))
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    match server.redeem_voucher("AZ-TEST-1234", None, "1TB", &key) {
        Ok(VoucherRedeemed::NewDrive(drive)) => assert_eq!(drive.drive_token, "dt_f.1.bbb"),
        other => panic!("not the drive: {other:?}"),
    }
    let keys: Vec<String> = fake
        .calls()
        .iter()
        .map(|c| {
            header(c, azlin_contract::IDEMPOTENCY_HEADER)
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(keys, vec![key.header_value(), key.header_value()], "never a new key");
}

#[test]
fn a_rate_limited_issue_is_asked_again_not_read_as_an_unknown_refusal() {
    let fake = Fake::new(|_, n| {
        Ok(if n == 0 {
            rate_limited("2")
        } else {
            json(
                200,
                r#"{"tier": "100GB", "key_id": "100GB/2026", "public_key_pem": "",
                    "blind_signatures": ["c2ln", "bmVk"],
                    "schedule": {"months": 3, "issued": 2,
                                 "next_issue_at": "2026-11-21T08:00:00Z"}}"#,
            )
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let blinded = vec![String::from("Ymxp"), String::from("bmQ=")];
    match server.issue_period_tokens("ck_1", ISSUE_KEY, "100GB/2026", &blinded) {
        Ok(IssueAnswer::Signed(signed)) => assert_eq!(signed.signatures, vec!["c2ln", "bmVk"]),
        other => panic!("not signed: {other:?}"),
    }
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].body, calls[1].body, "the same blinded messages");
}

#[test]
fn checkout_surface_unavailable_is_not_a_conflict() {
    // A method without such a surface: asking again does not help, and it is said as what it
    // is - neither "something went wrong" nor a checkout that changed meanwhile.
    let fake = Fake::new(|_, _| {
        Ok(json(
            400,
            r#"{"error": "surface_unavailable", "message": "no surface for this method"}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let error = server
        .checkout_surface("ck_9", "fields")
        .expect_err("refused");
    assert_eq!(fake.calls().len(), 1, "not asked again");
    let user = UserError::from_token_error(&error).expect("a refusal the user sees");
    assert_eq!(user.class(), Class::Fatal);
    assert!(
        !matches!(user.code, Code::Other | Code::NameConflict | Code::CheckoutPaid),
        "{:?}",
        user.code
    );
    for lang in Lang::ALL {
        let text = user.message(lang);
        assert!(
            !text.contains("no surface for this method") && !text.contains("HTTP"),
            "{lang:?}: never the server's sentence: {text}"
        );
    }
}

#[test]
fn a_provider_that_did_not_answer_is_asked_again_for_the_surface() {
    let fake = Fake::new(|_, n| {
        Ok(if n == 0 {
            json(
                502,
                r#"{"error": "provider_unavailable", "message": "the provider did not answer"}"#,
            )
        } else {
            json(
                200,
                r#"{"checkout_id": "ck_9", "surface": {"kind": "browser",
                    "url": "https://checkout.stripe.com/c/pay/cs_test_1"}}"#,
            )
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let answer = server
        .checkout_surface("ck_9", "browser")
        .expect("nothing changed: asked again");
    assert_eq!(answer["surface"]["kind"], "browser");
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].body, calls[1].body);
}

#[test]
fn a_rate_limited_answer_says_when_it_is_asked_again() {
    let fake = Fake::new(|_, _| Ok(rate_limited("60")));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let claim = ClaimKey::generate().unwrap();
    let error = server
        .checkout("100GB", 12, "sepa", &claim, CheckoutFor::NewDrive)
        .expect_err("still limited");
    let user = UserError::from_token_error(&error).expect("an error the user sees");
    assert_eq!(user.class(), Class::Retry, "it passes by itself");
    assert_ne!(user.code, Code::Other, "never a generic error");
    assert_eq!(user.retry_after, Some(60));
    for lang in Lang::ALL {
        let text = user.message(lang);
        assert!(text.contains("60"), "{lang:?}: when it is asked again: {text}");
        assert!(
            !text.contains("too many requests") && !text.contains("HTTP"),
            "{lang:?}: never the server's sentence: {text}"
        );
    }
}
