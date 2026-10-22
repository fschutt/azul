//! A drive's next months and its tier: a renewal's sign-up is claimed as months for its drive,
//! every finished checkout is the drive's price lock for its next renewal, a monthly plan's new
//! months are issued with the key it keeps, a tier bought for a drive is taken when the token
//! server allows it, and the reminders come before each deadline once.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use azlin_contract::DeadlineKind;
use azul_storage::{testing::TempDir, time::iso8601 as rfc3339, HttpReply, Method};

use super::{
    header, json,
    period::{blind_sign, keyring_with_session, keys, pem, APPROVED, D1, DAY, ISSUE_KEY, MONTH, N1, NOW},
    Fake, Shared, TOKEN,
};
use crate::{
    bundle::PeriodTokens,
    claim::seal,
    pending::{self, Final, Finished, PendingCheckout, Polled},
    period::{change_due, issue_next, reminder_due, Issued, PeriodToken, PeriodTokenStore, TierChanged},
    token::{CheckoutFor, TokenServer},
    vault::VaultGrant,
    ClaimKey,
};

fn store_in(dir: &TempDir) -> PeriodTokenStore {
    PeriodTokenStore::new(dir.path().join("period-tokens"))
}

fn approved(checkout_id: &'static str, sealed: String) -> HttpReply {
    json(
        200,
        &format!(
            r#"{{"checkout_id": "{checkout_id}", "status": "approved", "sealed_signup": "{sealed}"}}"#
        ),
    )
}

/// R1: a renewal's sign-up holds no drive: it is claimed as months for the drive its checkout
/// was made for, and kept until its grant is with that drive.
#[test]
fn a_renewal_signup_is_claimed_as_months_for_its_drive() {
    let dir = TempDir::new("azcloud-renewal");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let claim = ClaimKey::generate().unwrap();
    let mut ck = PendingCheckout::new("ck_2", &claim, "100GB", TOKEN, "");
    ck.for_drive = String::from("d_1");
    ck.renews = String::from("ck_1");
    pending::add(&shared, &ck).unwrap();
    let signup = serde_json::json!({
        "renewal_of": "ck_1", "grant": "period_tokens", "tier": "100GB",
        "period_tokens": {"checkout_id": "ck_2", "months": 3, "issue_key": ISSUE_KEY},
    });
    let sealed = seal(&claim.public_base64(), "ck_2", signup.to_string().as_bytes());
    let fake = Fake::new(move |_, _| Ok(approved("ck_2", sealed.clone())));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Renewed { drive_id, months }) => {
            assert_eq!((drive_id.as_str(), months), ("d_1", 3));
        }
        other => panic!("the renewal was not claimed: {other:?}"),
    }
    let listed = pending::list(&shared).unwrap();
    let owed = listed[0].period.clone().expect("its months wait for the drive");
    assert_eq!(
        (owed.drive_id.as_str(), owed.months, owed.issue_key.as_str()),
        ("d_1", 3, ISSUE_KEY)
    );
    // A renewal whose drive this computer did not name stays unclaimed: no drive is guessed.
    let mut other = PendingCheckout::new("ck_3", &claim, "100GB", TOKEN, "");
    other.renews = String::from("ck_1");
    pending::add(&shared, &other).unwrap();
    let signup3 = signup.to_string().replace("ck_2", "ck_3");
    let sealed3 = seal(&claim.public_base64(), "ck_3", signup3.as_bytes());
    let fake = Fake::new(move |_, _| Ok(approved("ck_3", sealed3.clone())));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(pending::poll(&server, &shared, &other), Polled::Transient(_)));
}

/// AD-02: every finished checkout is its drive's price lock (its id, its issue key, its ladder
/// version, kept with the drive's period tokens); a renewal names it once and is the next lock.
#[test]
fn a_finished_checkout_is_the_drives_price_lock_and_its_renewal_takes_it_over() {
    let dir = TempDir::new("azcloud-renewal");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    let claim = ClaimKey::generate().unwrap();
    // No answer to the issue: the grants wait with the drive, their locks are kept all the same.
    let quiet = Fake::new(|call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &keys()));
        }
        Err(String::from("connection refused"))
    });
    let transport = Shared(quiet);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let finished = |id: &str, renews: &str| {
        let mut ck = PendingCheckout::new(id, &claim, "100GB", TOKEN, "Photos");
        ck.ladder_version = Some(7);
        ck.renews = renews.to_string();
        pending::add(&shared, &ck).unwrap();
        let grant = PeriodTokens {
            checkout_id: id.to_string(),
            months: 1,
            issue_key: format!("{ISSUE_KEY}{id}"),
            recurring: false,
        };
        pending::claimed(&shared, id, "d_1", Some(&grant)).unwrap();
        let ck = pending::list(&shared).unwrap().into_iter().find(|c| c.checkout_id == id).unwrap();
        let owed = ck.period.clone().unwrap();
        match pending::finish(&server, &shared, &store, &tokens, &ck, &owed, NOW) {
            Finished::Granted { .. } => {}
            other => panic!("not granted: {other:?}"),
        }
    };
    finished("ck_1", "");
    let lock = tokens.state().unwrap().renewal_lock("100GB").cloned().expect("the lock");
    assert_eq!(lock.checkout_id, "ck_1");
    assert_eq!(lock.issue_key, format!("{ISSUE_KEY}ck_1"));
    assert_eq!(lock.ladder_version, Some(7));
    assert!(!format!("{lock:?}").contains(ISSUE_KEY), "Debug shows no issue key");

    // The renewal's order names the lock.
    let ordering = Fake::new(|call, _| {
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert_eq!(body["renews"]["checkout_id"], "ck_1");
        assert_eq!(body["renews"]["issue_key"], format!("{ISSUE_KEY}ck_1"));
        assert!(body.get("grant").is_none(), "a renewal under a lock is no tokens-only order");
        Ok(json(200, r#"{"checkout_id": "ck_2", "pay_url": "https://pay.example/ck_2"}"#))
    });
    let ordering_transport = Shared(ordering);
    let ordering_server = TokenServer::new(TOKEN, &ordering_transport).unwrap();
    ordering_server
        .checkout("100GB", 1, "sepa", &claim, CheckoutFor::Renewal(&lock))
        .unwrap();

    // Paid and finished: the old lock is used, the renewal is the drive's lock now.
    finished("ck_2", "ck_1");
    let state = tokens.state().unwrap();
    assert_eq!(state.renewal_lock("100GB").map(|l| l.checkout_id.as_str()), Some("ck_2"));
    let old = state.locks.iter().find(|l| l.checkout_id == "ck_1").unwrap();
    assert_eq!(old.used_by.as_deref(), Some("ck_2"));
}

/// Without a lock the drive is renewed by a checkout of period tokens only, at today's price.
#[test]
fn a_drive_without_a_lock_is_renewed_by_a_tokens_only_order() {
    let claim = ClaimKey::generate().unwrap();
    let fake = Fake::new(|call, _| {
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert_eq!(body["grant"], "period_tokens");
        assert_eq!(body["upgrade"], true);
        assert!(body.get("renews").is_none());
        Ok(json(200, r#"{"checkout_id": "ck_9", "pay_url": "https://pay.example/ck_9"}"#))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    server
        .checkout("1TB", 1, "sepa", &claim, CheckoutFor::PeriodTokens { upgrade: true })
        .unwrap();
}

/// R1: a monthly plan adds a month at each payment; the grant keeps its issue key and issues
/// each new month, instead of ending after the months its sign-up named.
#[test]
fn a_monthly_plan_whose_months_grow_issues_the_new_month_and_keeps_its_key() {
    let dir = TempDir::new("azcloud-renewal");
    let tokens = store_in(&dir).on_this_device("d_1").unwrap();
    let mut grant = VaultGrant::new("ck_1", "100GB", 1, ISSUE_KEY);
    grant.recurring = true;
    tokens.keep_grant(grant).unwrap();
    let paid = Arc::new(AtomicU64::new(1));
    let issued = Arc::new(AtomicU64::new(0));
    let (paid_in, issued_in) = (paid.clone(), issued.clone());
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &keys()));
        }
        assert_eq!(call.method, Method::Post);
        assert_eq!(header(call, "authorization"), None);
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert_eq!(body["issue_key"], ISSUE_KEY, "the same key for every month");
        let months = paid_in.load(Ordering::SeqCst);
        let done = issued_in.load(Ordering::SeqCst);
        if done >= months {
            let schedule = serde_json::json!({"months": months, "issued": done,
                "next_issue_at": rfc3339(APPROVED + done * MONTH)});
            let refusal = serde_json::json!({"error": "not_due", "schedule": schedule});
            return Ok(json(409, &refusal.to_string()));
        }
        let signatures: Vec<String> = body["blinded"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| blind_sign(N1, D1, b.as_str().unwrap()))
            .collect();
        let after = issued_in.fetch_add(1, Ordering::SeqCst) + 1;
        let schedule = serde_json::json!({"months": months, "issued": after,
            "next_issue_at": rfc3339(APPROVED + after * MONTH)});
        let answer = serde_json::json!({"tier": "100GB", "key_id": "100GB/2026",
            "public_key_pem": pem(N1), "blind_signatures": signatures, "schedule": schedule});
        Ok(json(200, &answer.to_string()))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(issue_next(&server, &tokens, "ck_1", NOW), Ok(Issued::Tokens(t)) if t.len() == 1));
    let kept = tokens.state().unwrap().grant("ck_1").cloned().unwrap();
    assert!(kept.is_open(), "paid up, the plan waits for its next payment");
    assert_eq!(kept.next_issue_at, Some(APPROVED + MONTH));
    // The second payment came: the checkout counts two months now.
    paid.store(2, Ordering::SeqCst);
    let later = APPROVED + MONTH;
    assert!(matches!(issue_next(&server, &tokens, "ck_1", later), Ok(Issued::Tokens(t)) if t.len() == 1));
    let kept = tokens.state().unwrap().grant("ck_1").cloned().unwrap();
    assert_eq!((kept.months, kept.issued), (2, 2), "the new month is the grant's");
    assert_eq!(kept.issue_key, ISSUE_KEY);
    assert!(kept.is_open());
    assert_eq!(tokens.state().unwrap().tokens.len(), 2);
}

fn token_of_tier(tier: &str, nonce: &str) -> PeriodToken {
    PeriodToken {
        tier: tier.to_string(),
        year: 2026,
        nonce: nonce.repeat(32),
        signature: String::from("c2lnbmF0dXJl"),
        randomizer: String::from("cmFuZG9taXplcg=="),
    }
}

/// A node's signed statement that `d_1`'s bucket holds `held` bytes (HeadBucket's
/// `x-azlin-usage-proof`; the kit passes it on unread).
fn proof_of(held: u64) -> String {
    format!("3.{NOW}.{held}.c2lnbmF0dXJl")
}

/// The token server of drive `d_1` (100 GB, paid until `NOW + 20 days`) answering each tier
/// change with `answer(body)`.
fn changing_server(answer: impl Fn(&serde_json::Value) -> HttpReply + Send + Sync + 'static) -> Arc<Fake> {
    Fake::new(move |call, _| {
        if call.method == Method::Get {
            let status = serde_json::json!({"id": "d_1", "tier": "100GB",
                "period_until": rfc3339(NOW + 20 * DAY)});
            return Ok(json(200, &status.to_string()));
        }
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/redeem"));
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert_eq!(body["change_tier"], true);
        assert_eq!(body["tier"], "1TB");
        Ok(answer(&body))
    })
}

/// Decision 5: a tier bought for a drive is redeemed as a change of its tier; the token server
/// takes a bigger one at once (the remaining days credited), a smaller one only at the period's
/// end and only when the stored bytes fit - the token waits unspent until then.
#[test]
fn a_tier_bought_for_a_drive_is_taken_when_the_token_server_allows_it() {
    let dir = TempDir::new("azcloud-renewal");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    let mut grant = VaultGrant::new("ck_up", "1TB", 1, ISSUE_KEY);
    grant.change_tier = true;
    tokens.keep_grant(grant).unwrap();
    tokens.add_tokens(&[token_of_tier("1TB", "cc")]).unwrap();

    // A smaller tier's end not reached: it waits, the token kept.
    let waits = changing_server(|_| {
        json(409, r#"{"error": "change_not_due", "from": "2026-10-28T00:00:00Z"}"#)
    });
    let transport = Shared(waits);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match change_due(&server, &shared, &tokens, || Some(proof_of(5)), NOW) {
        TierChanged::Waits { from } => assert_eq!(rfc3339(from), "2026-10-28T00:00:00Z"),
        other => panic!("not waiting: {other:?}"),
    }
    assert_eq!(tokens.unspent("1TB").unwrap().len(), 1, "the token waits");

    // The data does not fit: said with how much is stored, the token kept.
    let full = changing_server(|body| {
        // the node's signed count goes as it came, the app names no figure of its own
        assert_eq!(body["usage_proof"], proof_of(150));
        assert!(body.get("stored_bytes").is_none(), "{body}");
        json(409, r#"{"error": "does_not_fit", "stored": 150, "quota": 100}"#)
    });
    let transport = Shared(full);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match change_due(&server, &shared, &tokens, || Some(proof_of(150)), NOW) {
        TierChanged::DoesNotFit { stored, quota } => assert_eq!((stored, quota), (150, 100)),
        other => panic!("not refused: {other:?}"),
    }
    assert_eq!(tokens.unspent("1TB").unwrap().len(), 1);

    // Taken: the token spent, the change done, the credentials refreshed.
    let taken = changing_server(|_| {
        let answer = serde_json::json!({"period_until": rfc3339(NOW + 44 * DAY),
            "tier": "1TB", "credited_days": 14});
        json(200, &answer.to_string())
    });
    let transport = Shared(taken.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match change_due(&server, &shared, &tokens, || Some(proof_of(5)), NOW) {
        TierChanged::Changed {
            tier,
            period_until,
            credited_days,
            grant,
        } => {
            assert_eq!(tier, "1TB");
            assert_eq!(period_until, Some(NOW + 44 * DAY));
            assert_eq!(credited_days, Some(14));
            assert_eq!(grant.drive_id(), "d_1");
        }
        other => panic!("not changed: {other:?}"),
    }
    assert!(tokens.unspent("1TB").unwrap().is_empty());
    assert!(tokens.state().unwrap().grant("ck_up").is_some_and(|g| g.changed));
    // Done once: the next look changes nothing.
    let calls = taken.calls().len();
    let asked = std::cell::Cell::new(false);
    let ask = || {
        asked.set(true);
        Some(proof_of(5))
    };
    assert!(matches!(change_due(&server, &shared, &tokens, ask, NOW), TierChanged::Nothing));
    assert!(!asked.get(), "no node asked for a change already made");
    assert_eq!(taken.calls().len(), calls);
}

/// AD-02: the reminders come 30, 7 and 1 days before the period ends, before the drive turns
/// read-only and before it is closed - each once, and none while months are kept for it.
#[test]
fn the_reminders_come_before_each_deadline_once_and_not_while_months_are_kept() {
    let dir = TempDir::new("azcloud-renewal");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    let until = NOW + 20 * DAY;
    let status = crate::token::DriveStatus {
        tier: Some(String::from("100GB")),
        period_until: Some(until),
        lockdown_pending_until: None,
        read_only: false,
        members: Vec::new(),
        you: None,
        quota_bytes: None,
        used_bytes: None,
        ban: None,
        read_only_why: None,
    };
    let kept = tokens.state().unwrap();
    let first = reminder_due(&status, &kept, NOW).expect("within 30 days of the end");
    assert_eq!((first.kind, first.days_before, first.at), (DeadlineKind::PeriodEnd, 30, until));
    assert!(store.remind_once("d_1", &first).unwrap());
    assert!(!store.remind_once("d_1", &first).unwrap(), "said once");
    let week = reminder_due(&status, &kept, until - 6 * DAY).unwrap();
    assert_eq!(week.days_before, 7);
    let last = reminder_due(&status, &kept, until - DAY / 2).unwrap();
    assert_eq!(last.days_before, 1);
    let after = reminder_due(&status, &kept, until + DAY).unwrap();
    assert_eq!(after.kind, DeadlineKind::ReadOnlyStart);
    let closing = reminder_due(&status, &kept, until + 14 * DAY + 89 * DAY).unwrap();
    assert_eq!((closing.kind, closing.days_before), (DeadlineKind::ReadOnlyEnd, 1));
    assert!(reminder_due(&status, &kept, NOW - 30 * DAY).is_none(), "too early");
    tokens.add_tokens(&[token_of_tier("100GB", "dd")]).unwrap();
    let kept = tokens.state().unwrap();
    assert!(reminder_due(&status, &kept, NOW).is_none(), "the next month is kept");
}
