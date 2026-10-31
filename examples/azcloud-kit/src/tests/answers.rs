//! Every answer of the token server read once, by what it means: a refusal that spent nothing
//! (a rate limit, a provider that did not answer) is asked again with the same token, the same
//! idempotency key and the same order; a pause the server names is kept, a long one is the
//! caller's to wait out and said in the user's words; a refusal that does not pass says what it
//! is.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringNamespace, KeyringStore, MemoryKeyring},
    testing::TempDir,
    Credentials, Drive, HttpReply, Method, S3Config, Transport,
};

use super::{
    bundle, fake_s3::FakeS3, grant_key, header, json, keyring_holding, Fake, Shared, S3, TOKEN,
};
use crate::{
    bucket::Bucket,
    drive::TransportFactory,
    pending::{self, Abandoned, PendingCheckout},
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

/// The pauses a token server took.
type Pauses = Arc<Mutex<Vec<Duration>>>;

/// The token server on `transport`, its pauses recorded instead of slept.
fn recording(transport: &Shared) -> (TokenServer<'_>, Pauses) {
    let pauses: Pauses = Arc::new(Mutex::new(Vec::new()));
    let heard = pauses.clone();
    let server = TokenServer::new(TOKEN, transport)
        .unwrap()
        .with_sleep(Arc::new(move |pause| heard.lock().unwrap().push(pause)));
    (server, pauses)
}

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
    let (server, pauses) = recording(&transport);
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
    assert_eq!(*pauses.lock().unwrap(), vec![Duration::from_secs(2)], "its Retry-After");
}

#[test]
fn a_rate_limited_refresh_keeps_its_token_and_says_how_long_to_wait() {
    let dir = TempDir::new("azcloud-answers-refresh-long");
    let shared = keyring_holding(&dir, "dt_f.0.aaa");
    let fake = Fake::new(|_, _| Ok(rate_limited("60")));
    let transport = Shared(fake.clone());
    let (server, pauses) = recording(&transport);
    let error = server
        .refresh(shared.spend("d_1").unwrap(), None)
        .rotated()
        .expect_err("still limited");
    assert_eq!(fake.calls().len(), 1, "a minute is the caller's to wait out");
    assert!(pauses.lock().unwrap().is_empty());
    assert!(
        matches!(&error, crate::TokenError::Busy { retry_after: Some(60), .. }),
        "{error:?}"
    );
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
    let (server, pauses) = recording(&transport);
    let started = server
        .checkout("100GB", 12, "sepa", &claim, CheckoutFor::NewDrive)
        .expect("asked again after the pause");
    assert_eq!(started.checkout_id, "ck_1");
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].url, calls[1].url);
    assert_eq!(calls[0].body, calls[1].body, "the same order, the same claim key");
    assert_eq!(*pauses.lock().unwrap(), vec![Duration::from_secs(2)]);
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
    let (server, pauses) = recording(&transport);
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
    assert_eq!(*pauses.lock().unwrap(), vec![Duration::from_secs(2)]);
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
    let (server, pauses) = recording(&transport);
    let answer = server
        .checkout_surface("ck_9", "browser")
        .expect("nothing changed: asked again");
    assert_eq!(answer["surface"]["kind"], "browser");
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].body, calls[1].body);
    let pauses = pauses.lock().unwrap().clone();
    assert!(pauses.len() == 1 && !pauses[0].is_zero(), "after a backoff: {pauses:?}");
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

// ==== Requests named by their idempotency key: a checkout and its abandon ====

/// The idempotency key `call` carries.
fn key_of(call: &azul_storage::HttpCall) -> String {
    header(call, azlin_contract::IDEMPOTENCY_HEADER)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_checkout_whose_answer_was_lost_is_asked_again_under_the_same_key() {
    // The token server keeps a checkout's first answer under its key: the same order asked again
    // gets it, so a lost answer is sent again instead of being a second checkout.
    let claim = ClaimKey::generate().unwrap();
    let fake = Fake::new(|_, n| {
        if n == 0 {
            return Err(String::from("timed out"));
        }
        Ok(json(201, CHECKOUT))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let started = server
        .checkout("100GB", 12, "sepa", &claim, CheckoutFor::NewDrive)
        .expect("the lost answer, asked for again");
    assert_eq!(started.checkout_id, "ck_1");
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    let keys: Vec<String> = calls.iter().map(key_of).collect();
    assert!(!keys[0].is_empty(), "the checkout is named by a key");
    assert_eq!(keys[0], keys[1], "the same key: {keys:?}");
    assert_eq!(calls[0].body, calls[1].body, "the same order");
}

#[test]
fn an_abandon_whose_answer_was_lost_is_asked_again_under_the_same_key() {
    let dir = TempDir::new("azcloud-answers-abandon");
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::with_lock_root(
        keyring,
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    let claim = ClaimKey::generate().unwrap();
    pending::add(&shared, &PendingCheckout::new("ck_1", &claim, "100GB", TOKEN, "Photos")).unwrap();
    let fake = Fake::new(|call, n| {
        assert!(call.url.ends_with("/v1/checkout/ck_1/abandon"), "{}", call.url);
        if n == 0 {
            return Err(String::from("timed out"));
        }
        Ok(json(200, r#"{"checkout_id": "ck_1", "status": "expired"}"#))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    assert_eq!(
        pending::abandon(&server, &shared, "ck_1"),
        Abandoned::Ended,
        "the lost answer, asked for again, ends the checkout"
    );
    let keys: Vec<String> = fake.calls().iter().map(key_of).collect();
    assert_eq!(keys.len(), 2);
    assert!(!keys[0].is_empty() && keys[0] == keys[1], "one key: {keys:?}");
}

#[test]
fn a_key_reused_for_another_request_is_a_bug_and_never_asked_again() {
    let fake = Fake::new(|_, _| {
        Ok(json(
            422,
            r#"{"error": "idempotency_key_reused",
                "message": "this x-azlin-idempotency key named another request"}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO);
    let claim = ClaimKey::generate().unwrap();
    let error = server
        .checkout("100GB", 12, "sepa", &claim, CheckoutFor::NewDrive)
        .expect_err("refused");
    assert_eq!(fake.calls().len(), 1, "never asked again");
    assert!(crate::outbox::is_final_error(&error), "the call is closed: {error:?}");
    let user = UserError::from_token_error(&error).expect("an error the user sees");
    assert_eq!(user.class(), Class::Fatal);
    assert_ne!(user.code, Code::Other, "it names what happened");
    assert!(user.message(Lang::En).contains("bug"), "{}", user.message(Lang::En));
    assert!(user.message(Lang::De).contains("Fehler"), "{}", user.message(Lang::De));
    assert!(
        !user.message(Lang::En).contains("x-azlin-idempotency"),
        "never the server's sentence"
    );
}

// ==== An encrypted drive's index commits on a drive that just froze ====

/// 2026-10-10T00:00:00Z.
const NOW: u64 = 1_791_590_400;

/// A bucket of drive `d-1` over `fake`, its clock at `clock`, its pauses recorded.
fn bucket_on(fake: &Arc<Fake>, clock: &Arc<AtomicU64>) -> (Bucket, Pauses) {
    let pauses: Pauses = Arc::new(Mutex::new(Vec::new()));
    let heard = pauses.clone();
    let answering = fake.clone();
    let factory: TransportFactory =
        Arc::new(move || Box::new(Shared(answering.clone())) as Box<dyn Transport>);
    let now = clock.clone();
    let bucket = Bucket::new(
        S3Config {
            endpoint: S3.to_string(),
            region: String::from("us-east-1"),
            bucket: String::from("d-1"),
            path_style: true,
        },
        Credentials::new("AKID1", "secret-of-AKID1"),
        factory,
    )
    .unwrap()
    .with_clock(move || now.load(Ordering::SeqCst))
    .with_sleep(Arc::new(move |pause| heard.lock().unwrap().push(pause)));
    (bucket, pauses)
}

/// A node's refusal with the Azlin code `code`.
fn refused_by_node(code: &str) -> HttpReply {
    HttpReply {
        status: 403,
        headers: vec![(String::from("x-azlin-error"), code.to_string())],
        body: b"<Error><Code>AccessDenied</Code><Message>the drive takes no writes</Message></Error>"
            .to_vec(),
    }
}

/// An encrypted drive's index object (a log entry of its index).
const COMMIT: &str = ".azlin/meta/log/0000000001-0";

#[test]
fn a_frozen_drives_index_commit_is_asked_again_while_the_nodes_learn_its_frozen_bytes() {
    // A drive's block leader records the bytes the drive stored at its freeze on its heartbeat,
    // and the nodes read them at their next control poll: until then its owner's index commit is
    // refused with the drive's read-only code (rarely quota_exceeded, the commit's own line).
    // Within a minute of the freeze it is asked again after a backoff.
    for code in ["read_only_unpaid", "read_only_operator", "quota_exceeded"] {
        let s3 = FakeS3::new();
        let service = s3.clone();
        let fake = Fake::new(move |call, n| {
            if n == 0 {
                return Ok(refused_by_node(code));
            }
            Ok(service.answer(call))
        });
        let (bucket, pauses) = bucket_on(&fake, &Arc::new(AtomicU64::new(NOW)));
        // A read-only refusal tells the drive itself that it froze; a quota refusal does not,
        // so for that one the drive knows of the freeze from the token server's view, as it
        // does once its refresh says the drive is read-only.
        if code == "quota_exceeded" {
            bucket.failover().saw_frozen(true);
        }
        bucket
            .put(COMMIT, b"the index's commit")
            .unwrap_or_else(|e| panic!("{code}: not asked again: {e:?}"));
        let puts = fake.calls().iter().filter(|c| c.method == Method::Put).count();
        assert_eq!(puts, 2, "{code}");
        assert_eq!(s3.read(COMMIT).as_deref(), Some(&b"the index's commit"[..]), "{code}");
        let pauses = pauses.lock().unwrap().clone();
        assert!(
            pauses.len() == 1 && !pauses[0].is_zero(),
            "{code}: after a backoff: {pauses:?}"
        );
    }
}

#[test]
fn a_full_drives_index_commit_is_not_asked_again_when_no_freeze_is_known() {
    // A quota refusal of a drive that is not known to be frozen says the drive is full: it is
    // final, never a minute of retries.
    let fake = Fake::new(move |_, _| Ok(refused_by_node("quota_exceeded")));
    let (bucket, pauses) = bucket_on(&fake, &Arc::new(AtomicU64::new(NOW)));
    assert!(bucket.put(COMMIT, b"the index's commit").is_err());
    let puts = fake.calls().iter().filter(|c| c.method == Method::Put).count();
    assert_eq!(puts, 1, "a full drive's commit was asked again");
    assert!(pauses.lock().unwrap().is_empty());
}

#[test]
fn the_outbox_keeps_one_key_per_checkout_attempt_across_its_calls() {
    // Asked again after its answers were lost - a later call, the app started anew - the same
    // order carries the key of its first try; another order (another claim key) a key of its own.
    let dir = TempDir::new("azcloud-answers-checkout-outbox");
    let outbox = crate::Outbox::new(dir.path().join("outbox"));
    let lost = std::sync::atomic::AtomicBool::new(true);
    let fake = Fake::new(move |_, _| {
        if lost.swap(false, Ordering::SeqCst) {
            return Err(String::from("timed out"));
        }
        Ok(json(201, CHECKOUT))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport)
        .unwrap()
        .with_retry_pause(Duration::ZERO)
        .with_policy(crate::policy::Policy {
            attempts: 1,
            ..crate::policy::Policy::TOKEN
        })
        .with_outbox(outbox.clone());
    let claim = ClaimKey::generate().unwrap();
    assert!(
        server
            .checkout("100GB", 12, "sepa", &claim, CheckoutFor::NewDrive)
            .is_err(),
        "one try, no answer"
    );
    assert!(
        outbox
            .pending(azlin_contract::IdempotentScope::Checkout, "")
            .is_some(),
        "the attempt stays open"
    );
    server
        .checkout("100GB", 12, "sepa", &claim, CheckoutFor::NewDrive)
        .expect("asked again");
    let other = ClaimKey::generate().unwrap();
    server
        .checkout("100GB", 12, "sepa", &other, CheckoutFor::NewDrive)
        .expect("another order");
    let keys: Vec<String> = fake.calls().iter().map(key_of).collect();
    assert_eq!(keys.len(), 3);
    assert_eq!(keys[0], keys[1], "the same attempt, the same key: {keys:?}");
    assert_ne!(keys[2], keys[0], "another attempt, a key of its own: {keys:?}");
    assert!(
        outbox
            .pending(azlin_contract::IdempotentScope::Checkout, "")
            .is_none(),
        "answered: closed"
    );
}

#[test]
fn an_index_commit_refused_a_minute_after_the_freeze_says_the_drive_is_read_only() {
    let clock = Arc::new(AtomicU64::new(NOW));
    let fake = Fake::new(|call, _| {
        assert_eq!(call.method, Method::Put);
        Ok(refused_by_node("read_only_unpaid"))
    });
    let (bucket, pauses) = bucket_on(&fake, &clock);
    // A file's write on the frozen drive: refused at once, never asked again, and the drive's
    // freeze is known from then on.
    assert!(bucket.put("photos/a.jpg", b"x").is_err());
    assert_eq!(fake.calls().len(), 1, "a write is no index commit");
    // A minute and more later the nodes know the bytes at the freeze: a refused commit is over
    // the budget, and the user is told the drive takes no writes.
    clock.fetch_add(61, Ordering::SeqCst);
    let error = bucket.put(COMMIT, b"the index's commit").expect_err("over the budget");
    assert_eq!(fake.calls().len(), 2, "not asked again");
    assert!(pauses.lock().unwrap().is_empty());
    let user = UserError::from_cloud_error(&error).expect("an error the user sees");
    assert_eq!(user.code, Code::ReadOnlyUnpaid);
}
