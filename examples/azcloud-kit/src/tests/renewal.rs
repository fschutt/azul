//! A drive's next months and its tier: a renewal's sign-up is claimed as months for its drive,
//! every finished checkout is the drive's price lock for its next renewal, a monthly plan's new
//! months are issued with the key it keeps, a tier bought for a drive is taken when the token
//! server allows it, and the reminders come before each deadline once.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use azlin_contract::DeadlineKind;
use azul_storage::{testing::TempDir, time::iso8601 as rfc3339, HttpReply, Method};

use super::{
    header, json,
    period::{blind_sign, keyring_with_session, keys, pem, APPROVED, D1, DAY, ISSUE_KEY, MONTH, N1, NOW},
    tokens_in_drive, Fake, Shared, TOKEN,
};
use crate::{
    bundle::PeriodTokens,
    claim::seal,
    pending::{self, Final, Finished, PendingCheckout, Polled, Recorded},
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
        Polled::Final(Final::Renewed {
            drive_id,
            months,
            record,
        }) => {
            assert_eq!((drive_id.as_str(), months), ("d_1", 3));
            assert_eq!(record, Recorded::Kept, "and the sale is recorded");
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

/// X-DELTA-AZDRIVE-MONEY-2 (ENT27-B06): the months of a renewal are claimed whether or not the
/// record of the payment can be kept - the poll claims them first, then records the sale, and
/// says in the answer when it could not (`Recorded::NotKept`). Before, the record came first and
/// a list that could not be written answered `NotKept` at every poll: the renewal sat on the
/// list until the token server purged its sealed sign-up.
#[test]
fn x_delta_azdrive_money_2_the_months_of_a_renewal_are_claimed_whether_or_not_the_record_is_kept() {
    let dir = TempDir::new("azcloud-renewal");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let claim = ClaimKey::generate().unwrap();
    let mut ck = PendingCheckout::new("ck_2", &claim, "100GB", TOKEN, "");
    ck.for_drive = String::from("d_1");
    pending::add(&shared, &ck).unwrap();
    // A file where the folder of the paid list should be.
    let folder = shared
        .paid()
        .unwrap()
        .path()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
    std::fs::write(&folder, b"in the way").unwrap();
    let signup = serde_json::json!({
        "renewal_of": "ck_1", "grant": "period_tokens", "tier": "100GB",
        "period_tokens": {"checkout_id": "ck_2", "months": 3, "issue_key": ISSUE_KEY},
    });
    let sealed = seal(
        &claim.public_base64(),
        "ck_2",
        signup.to_string().as_bytes(),
    );
    let fake = Fake::new(move |_, _| Ok(approved("ck_2", sealed.clone())));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &ck) {
        Polled::Final(Final::Renewed {
            drive_id,
            months,
            record,
        }) => {
            assert_eq!((drive_id.as_str(), months), ("d_1", 3));
            assert!(record.not_kept().is_some(), "the app is told: {record:?}");
        }
        other => panic!("held back by its record: {other:?}"),
    }
    let owed = pending::list(&shared).unwrap()[0]
        .period
        .clone()
        .expect("its months wait for the drive");
    assert_eq!((owed.drive_id.as_str(), owed.months), ("d_1", 3));
}

/// AD-02: every finished checkout is its drive's price lock (its id, its issue key, its ladder
/// version, kept with the drive's period tokens); a renewal names it once and is the next lock.
#[test]
fn a_finished_checkout_is_the_drives_price_lock_and_its_renewal_takes_it_over() {
    let dir = TempDir::new("azcloud-renewal");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    // The drive has a vault of its own: a grant leaves the keyring only when it is in two places
    // (AZ-P5), and "finished" below means it did (X-DELTA-AZDRIVE-MONEY-13).
    let (tokens, _in_drive) = tokens_in_drive(&store);
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
        Ok(json(
            200,
            r#"{"checkout_id": "ck_2", "pay_url": "https://pay.example/ck_2",
                "amount_cents": 89, "currency": "EUR"}"#,
        ))
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
        Ok(json(
            200,
            r#"{"checkout_id": "ck_9", "pay_url": "https://pay.example/ck_9",
                "amount_cents": 499, "currency": "EUR"}"#,
        ))
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

/// Every file below `dir`, its folders walked.
fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_below(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// LANE-AZ-2: `files_of` names every file the store keeps for a drive. After a vault, its second
/// copy, the members seen and a reminder said for two drives, the folder holds exactly the files
/// the two drives' lists name: a removed drive leaves none of its own behind, and none of the
/// other drive's is named.
#[test]
fn lane_az_2_files_of_names_every_file_the_store_keeps_for_a_drive() {
    let dir = TempDir::new("azcloud-store-files");
    let store = store_in(&dir);
    let until = NOW + 20 * DAY;
    let status = crate::token::DriveStatus {
        tier: Some(String::from("100GB")),
        period_until: Some(until),
        lockdown_pending_until: None,
        read_only: false,
        members: vec![String::from("m_laptop")],
        you: None,
        quota_bytes: None,
        used_bytes: None,
        ban: None,
        read_only_why: None,
        archive: None,
    };
    for drive in ["d_1", "d_2"] {
        let tokens = store.on_this_device(drive).unwrap();
        // The reminder is due while no months are kept for the drive.
        let reminder = reminder_due(&status, &tokens.state().unwrap(), NOW).expect("a reminder");
        assert!(store.remind_once(drive, &reminder).unwrap());
        tokens.add_tokens(&[token_of_tier("100GB", "aa")]).unwrap();
        tokens.keep_second_copy().unwrap();
        store.see_members(drive, &status).unwrap();
    }

    // A lock file is the lock: never removed, so no list names one (X-DELTA-AZDRIVE-KEYS-32: the
    // vault's was named, and a removal unlinked it under a waiter that had it open).
    let (locks, mut on_disk): (Vec<PathBuf>, Vec<PathBuf>) =
        files_below(store.dir()).into_iter().partition(|path| {
            path.parent()
                .and_then(Path::file_name)
                .is_some_and(|folder| folder == "locks")
        });
    assert!(
        !locks.is_empty(),
        "the changes of the vaults took their locks"
    );
    on_disk.sort();
    let mut named: Vec<PathBuf> = ["d_1", "d_2"]
        .iter()
        .flat_map(|drive| store.files_of(drive))
        .collect();
    named.sort();
    assert_eq!(
        on_disk, named,
        "the files there are the files named, no more"
    );
    assert!(
        named.iter().all(|file| !locks.contains(file)),
        "a lock file is never unlinked, so none is named: {named:?}"
    );
    let first = store.files_of("d_1");
    assert!(first.iter().all(|file| file.exists()), "{first:?}");
    assert!(
        first
            .iter()
            .all(|file| !store.files_of("d_2").contains(file)),
        "no file is two drives'"
    );
    // An id that is no drive id has no files by name.
    assert!(store.files_of("../d_1").is_empty());
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

/// X-DELTA-AZDRIVE-MONEY-3 (AZ-P16): the move to a bigger tier redeems a token like a month does,
/// and a token server that lost its issuer key refuses it as `unknown_issuer` all the same: the
/// token is kept for the next look, not spent (it was, on that word alone). A word that is surely
/// the token's (`token_used`) still spends it.
#[test]
fn x_delta_azdrive_money_3_a_tier_change_keeps_a_token_refused_on_a_doubtful_word() {
    let dir = TempDir::new("azcloud-renewal");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    let mut grant = VaultGrant::new("ck_up", "1TB", 1, ISSUE_KEY);
    grant.change_tier = true;
    grant.issued = 1; // its one month is issued: the token below is the only one there is
    tokens.keep_grant(grant).unwrap();
    tokens.add_tokens(&[token_of_tier("1TB", "cc")]).unwrap();
    for word in ["unknown_issuer", "bad_token", "token_expired"] {
        let lost = changing_server(move |_| {
            json(
                400,
                &format!(r#"{{"error": "{word}", "message": "refused"}}"#),
            )
        });
        let transport = Shared(lost);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        match change_due(&server, &shared, &tokens, || Some(proof_of(5)), NOW) {
            TierChanged::Kept(why) => {
                assert!(why.contains("kept for the next look"), "{word}: {why}")
            }
            other => panic!("{word}: not kept: {other:?}"),
        }
        assert_eq!(
            tokens.unspent("1TB").unwrap().len(),
            1,
            "{word}: the token stays"
        );
    }
    // A token the server calls used is spent.
    let used = changing_server(|_| {
        json(
            409,
            r#"{"error": "token_used", "message": "redeemed before"}"#,
        )
    });
    let transport = Shared(used);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let _ = change_due(&server, &shared, &tokens, || Some(proof_of(5)), NOW);
    assert!(
        tokens.unspent("1TB").unwrap().is_empty(),
        "a used token is gone"
    );
}

/// X-DELTA-MONEY2-11: the move to a bigger tier redeems a token like a month does, and the rule
/// that a token the server calls expired is gone when this computer's own clock agrees holds
/// there too: a token of 2021 refused as `token_expired` is spent (it was kept as a doubt that
/// counted), one of this year refused the same way is the server's doubt and stays.
#[test]
fn x_delta_money2_11_a_tier_change_spends_a_token_both_clocks_call_expired() {
    let dir = TempDir::new("azcloud-renewal");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    let mut grant = VaultGrant::new("ck_up", "1TB", 1, ISSUE_KEY);
    grant.change_tier = true;
    grant.issued = 1; // its one month is issued: the token below is the only one there is
    tokens.keep_grant(grant).unwrap();
    let expired = changing_server(|_| {
        json(
            400,
            r#"{"error": "token_expired", "message": "the issuer year is not valid"}"#,
        )
    });
    let transport = Shared(expired);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // this year's token: the server's doubt, kept
    tokens.add_tokens(&[token_of_tier("1TB", "cc")]).unwrap();
    assert!(matches!(
        change_due(&server, &shared, &tokens, || Some(proof_of(5)), NOW),
        TierChanged::Kept(_)
    ));
    assert_eq!(
        tokens.unspent("1TB").unwrap().len(),
        1,
        "this year's token stays"
    );
    // a token of 2021: expired by both clocks, spent
    tokens.spend(&"cc".repeat(32)).unwrap();
    let mut old = token_of_tier("1TB", "dd");
    old.year = 2021;
    tokens.add_tokens(&[old]).unwrap();
    let _ = change_due(&server, &shared, &tokens, || Some(proof_of(5)), NOW);
    assert!(
        tokens.unspent("1TB").unwrap().is_empty(),
        "the expired token is gone"
    );
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
        archive: None,
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
