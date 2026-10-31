//! Cash by post (cash contract v1) on the app's side: the activation code a slip prints, the
//! claim code (AZK1, or AZK2 naming the new drive's key) on the buyer's copy, the poll of a cash
//! checkout, its slip kept with it in the keyring's list, and a checkout picked up by its code on
//! another computer - which takes no other drive key than the one its code names.

use std::sync::Arc;

use azul_storage::{
    base32,
    keyring::{KeyringNamespace, MemoryKeyring},
    testing::TempDir,
};
use sha2::{Digest, Sha256};

use super::{bundle, json, Fake, Shared, TOKEN};
use crate::{
    cash::{
        ActivationCode, ClaimCode, ACTIVATION_PREFIX, CLAIM_CODE_PREFIX, KEYED_CLAIM_CODE_PREFIX,
        KEY_ID_LEN,
    },
    claim::seal,
    pending::{self, Final, CashKept, PendingCheckout, Polled},
    shared::SharedKeyring,
    token::{CheckoutStatus, Tiers, TokenServer},
    ClaimKey,
};

/// The token server's test vector (azlin-token cash.rs, SRV17): checkout `ck_aaaqeayeaudaocajbifqydiob4`
/// (the 16 bytes 00 01 .. 0f behind `ck_`), EUR 11.88, the key `cash-key-for-tests`.
const ACTIVATION: &str = "AZC1-AAAQ-EAYE-AUDA-OCAJ-BIFQ-YDIO-B4AA-ABFE-IVKV-F3QG-5NDF-5KZK-SMTY-I";
const CODE_CHECKOUT: &str = "ck_aaaqeayeaudaocajbifqydiob4";
/// The code an app wrote before the id's bytes were the token server's: the id as ASCII.
const ASCII_ID_CODE: &str = "AZC1-MNVV-6YLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYIA-AAB5-\
                             4RKV-KI74-IMPG-BG5O-LTW7-WE";
const CHECKOUT: &str = "ck_aaaaaaaaaaaaaaaaaaaaaaaaaa";
/// The claim code of that checkout with the claim secret 0, 1, ..., 31 (scripts/azlin_claim.py
/// writes the same).
const CLAIM_CODE: &str = "AZK1-DVRW-WX3B-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-AAAQ-\
                          EAYE-AUDA-OCAJ-BIFQ-YDIO-B4IB-CEQT-CQKR-MFYY-DENB-WHA5-DYP6-P3CB-LY";
const SECRET: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
/// The same code naming the id of its new drive's key, the bytes 32, 33, ..., 47
/// (scripts/azlin_claim.py writes the same).
const KEYED_CLAIM_CODE: &str = "AZK2-DVRW-WX3B-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-\
                                AAAQ-EAYE-AUDA-OCAJ-BIFQ-YDIO-B4IB-CEQT-CQKR-MFYY-DENB-WHA5-\
                                DYPS-AIJC-EMSC-KJRH-FAUS-UKZM-FUXC-6WYA-OWAA";
/// That key id in hex.
const KEY_ID_HEX: &str = "202122232425262728292a2b2c2d2e2f";

/// The bytes 32, 33, ..., 47: the drive key id of [`KEYED_CLAIM_CODE`].
fn key_id() -> [u8; KEY_ID_LEN] {
    let mut id = [0u8; KEY_ID_LEN];
    for (byte, value) in id.iter_mut().zip(32u8..) {
        *byte = value;
    }
    id
}

fn shared(dir: &TempDir) -> SharedKeyring {
    SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    )
}

// ==== The activation code ====

#[test]
fn the_activation_code_names_its_checkout_its_amount_and_its_currency() {
    let code = ActivationCode::parse(ACTIVATION).unwrap();
    assert_eq!(code.checkout_id, CODE_CHECKOUT, "the 16 bytes behind ck_, as the id writes them");
    assert_eq!(code.amount_cents, 1188);
    assert_eq!(code.currency, "EUR");
    assert_eq!(code.mac.len(), 10);
    assert_eq!(code.to_text(), ACTIVATION, "written as the token server wrote it");
    assert!(code.check(CODE_CHECKOUT, 1188, "EUR").is_ok());
    assert!(code.check(CODE_CHECKOUT, 11880, "EUR").is_err(), "a slip for another amount");
    assert!(code.check(CODE_CHECKOUT, 1188, "CHF").is_err(), "a slip in another currency");
    assert!(
        code.check("ck_bbbbbbbbbbbbbbbbbbbbbbbbbb", 1188, "EUR").is_err(),
        "a slip of another checkout"
    );
    let typed = ACTIVATION.to_lowercase().replace('-', " ");
    assert_eq!(ActivationCode::parse(&typed).unwrap(), code, "any case, blanks");
}

#[test]
fn an_activation_code_of_another_version_or_a_broken_one_is_refused() {
    assert!(ActivationCode::parse("").is_err());
    assert!(ActivationCode::parse(&ACTIVATION.replacen("AZC1", "AZC2", 1)).is_err());
    assert!(ActivationCode::parse(&ACTIVATION[..40]).is_err(), "too short for a MAC");
    assert!(ActivationCode::parse(&ACTIVATION.replacen("AAAQ", "AAA1", 1)).is_err());
    assert!(ACTIVATION.starts_with(ACTIVATION_PREFIX));
    assert!(
        ActivationCode::parse(ASCII_ID_CODE).is_err(),
        "a code whose id is not the 16 bytes behind ck_ is no token server's"
    );
}

// ==== The claim code ====

#[test]
fn a_claim_code_holds_the_checkout_id_and_the_claim_secret_and_reads_back() {
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let code = ClaimCode::new(CHECKOUT, &claim);
    let text = code.to_text();
    assert_eq!(text.as_str(), CLAIM_CODE, "the same as scripts/azlin_claim.py writes");
    assert!(text.starts_with(CLAIM_CODE_PREFIX));
    let back = ClaimCode::parse(&text).unwrap();
    assert_eq!(back.checkout_id, CHECKOUT);
    assert_eq!(back.claim.to_base64(), SECRET);
    assert_eq!(back.key_id, None);
    let fresh = ClaimKey::generate().unwrap();
    let other = ClaimCode::parse(&ClaimCode::new("ck_2", &fresh).to_text()).unwrap();
    assert_eq!(other.checkout_id, "ck_2");
    assert_eq!(other.claim.public_base64(), fresh.public_base64());
}

#[test]
fn a_claim_code_reads_back_in_any_case_with_blanks_and_without_dashes() {
    for typed in [
        CLAIM_CODE.to_lowercase(),
        CLAIM_CODE.replace('-', ""),
        CLAIM_CODE.replace('-', " "),
        format!("  {CLAIM_CODE}\n"),
        CLAIM_CODE.replacen("AZK1-", "azk1 ", 1),
    ] {
        let code = ClaimCode::parse(&typed).unwrap_or_else(|e| panic!("{typed:?}: {e}"));
        assert_eq!(code.checkout_id, CHECKOUT);
    }
}

#[test]
fn a_mistyped_claim_code_is_refused_never_read_as_another_checkout() {
    // One character changed anywhere: the check value refuses it.
    let chars: Vec<char> = CLAIM_CODE.chars().collect();
    for at in (5..chars.len()).filter(|i| chars[*i] != '-') {
        let mut typo = chars.clone();
        typo[at] = if typo[at] == 'A' { 'B' } else { 'A' };
        let typo: String = typo.into_iter().collect();
        assert!(ClaimCode::parse(&typo).is_err(), "{typo}");
    }
    assert!(ClaimCode::parse("").is_err());
    assert!(ClaimCode::parse(&CLAIM_CODE.replacen("AZK1", "AZK2", 1)).is_err());
    assert!(ClaimCode::parse(&CLAIM_CODE[..60]).is_err());
    assert!(ClaimCode::parse(ACTIVATION).is_err(), "an activation code is no claim code");
}

#[test]
fn a_claim_code_that_names_its_drive_key_is_an_azk2_code_and_reads_back() {
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let code = ClaimCode {
        key_id: Some(key_id()),
        ..ClaimCode::new(CHECKOUT, &claim)
    };
    let text = code.to_text();
    assert_eq!(text.as_str(), KEYED_CLAIM_CODE, "the same as scripts/azlin_claim.py writes");
    assert!(text.starts_with(KEYED_CLAIM_CODE_PREFIX));
    for typed in [
        KEYED_CLAIM_CODE.to_string(),
        KEYED_CLAIM_CODE.to_lowercase().replace('-', " "),
    ] {
        let back = ClaimCode::parse(&typed).unwrap_or_else(|e| panic!("{typed:?}: {e}"));
        assert_eq!(back.checkout_id, CHECKOUT);
        assert_eq!(back.claim.to_base64(), SECRET);
        assert_eq!(back.key_id, Some(key_id()));
    }
    // A code without a key id is written and read as before.
    let plain = ClaimCode::parse(CLAIM_CODE).unwrap();
    assert_eq!(plain.key_id, None, "an AZK1 code names no key");
    assert_eq!(plain.to_text().as_str(), CLAIM_CODE);
}

#[test]
fn a_mistyped_azk2_code_or_one_checked_as_another_version_is_refused() {
    let chars: Vec<char> = KEYED_CLAIM_CODE.chars().collect();
    for at in (5..chars.len()).filter(|i| chars[*i] != '-') {
        let mut typo = chars.clone();
        typo[at] = if typo[at] == 'A' { 'B' } else { 'A' };
        let typo: String = typo.into_iter().collect();
        assert!(ClaimCode::parse(&typo).is_err(), "{typo}");
    }
    assert!(ClaimCode::parse(&KEYED_CLAIM_CODE.replacen("AZK2", "AZK1", 1)).is_err());
    assert!(ClaimCode::parse(&KEYED_CLAIM_CODE.replacen("AZK2", "AZK3", 1)).is_err());
    // The bytes of an AZK2 code with the check value of an AZK1 one: the version is part of
    // what the check covers, so a code cut or relabelled never reads.
    let mut payload = vec![u8::try_from(CHECKOUT.len()).unwrap()];
    payload.extend_from_slice(CHECKOUT.as_bytes());
    payload.extend(0u8..32);
    payload.extend_from_slice(&key_id());
    let check = Sha256::digest([b"AZK1".as_slice(), payload.as_slice()].concat());
    payload.extend_from_slice(&check[..4]);
    let relabelled = format!("AZK2-{}", base32::grouped(&base32::encode(&payload)));
    assert!(ClaimCode::parse(&relabelled).is_err(), "{relabelled}");
}

#[test]
fn a_picked_up_azk2_code_keeps_the_key_id_it_names_and_its_checkout_prints_it_again() {
    let picked = ClaimCode::parse(KEYED_CLAIM_CODE)
        .unwrap()
        .pending(TOKEN, "Photos");
    assert_eq!(picked.key_id.as_deref(), Some(KEY_ID_HEX));
    assert_eq!(picked.drive_key, None, "the drive's key is the buyer's computer's");
    assert_eq!(
        ClaimCode::of(&picked).unwrap().to_text().as_str(),
        KEYED_CLAIM_CODE
    );
    let older = ClaimCode::parse(CLAIM_CODE).unwrap().pending(TOKEN, "Photos");
    assert_eq!(older.key_id, None, "an AZK1 code commits to no key");
    // The buyer's kept checkout prints the code its copy printed.
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let mut bought = PendingCheckout::new(CHECKOUT, &claim, "100GB", TOKEN, "Photos");
    bought.method = String::from("cash");
    bought.key_id = Some(KEY_ID_HEX.to_string());
    assert_eq!(
        ClaimCode::of(&bought).unwrap().to_text().as_str(),
        KEYED_CLAIM_CODE
    );
    // A damaged id is refused, never printed as a code that names no key.
    bought.key_id = Some(String::from("not hex"));
    assert!(ClaimCode::of(&bought).is_err());
}

#[test]
fn a_claim_code_shows_no_secret_in_debug() {
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let code = ClaimCode::new(CHECKOUT, &claim);
    let shown = format!("{code:?}");
    assert!(shown.contains(CHECKOUT), "{shown}");
    assert!(!shown.contains(SECRET) && !shown.contains("DVRW-WX3B"), "{shown}");
}

#[test]
fn a_picked_up_code_becomes_an_unfinished_cash_checkout_of_this_token_server() {
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let kept = ClaimCode::new(CHECKOUT, &claim).pending(TOKEN, "Photos");
    assert_eq!(kept.checkout_id, CHECKOUT);
    assert_eq!(kept.claim_secret, SECRET);
    assert_eq!(kept.token_url, TOKEN);
    assert_eq!(kept.name, "Photos");
    assert!(kept.is_cash(), "a claim code is a cash checkout's: asked once a day");
    assert!(kept.tier.is_empty(), "the sealed sign-up names the tier");
    assert!(kept.cash.is_none(), "the slip is on paper, not on this computer");
    // And the code of a kept checkout is the code the buyer's copy printed.
    let mut bought = PendingCheckout::new(CHECKOUT, &claim, "100GB", TOKEN, "Photos");
    bought.method = String::from("cash");
    assert_eq!(
        ClaimCode::of(&bought).unwrap().to_text().as_str(),
        CLAIM_CODE
    );
}

// ==== The poll and the keyring's list ====

#[test]
fn a_cash_checkout_awaits_its_letter_and_a_rejected_one_says_why() {
    let fake = Fake::new(|call, _| {
        Ok(json(
            200,
            if call.url.ends_with("/ck_waiting") {
                r#"{"checkout_id": "ck_waiting", "status": "awaiting_cash", "amount_cents": 990}"#
            } else {
                r#"{"checkout_id": "ck_rejected", "status": "rejected",
                    "reason": "the envelope held EUR 5.00, not EUR 9.90"}"#
            },
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let claim = ClaimKey::generate().unwrap();
    assert_eq!(
        server.checkout_status("ck_waiting", &claim).unwrap(),
        CheckoutStatus::Pending
    );
    assert_eq!(
        server.checkout_status("ck_rejected", &claim).unwrap(),
        CheckoutStatus::Declined(String::from("the envelope held EUR 5.00, not EUR 9.90"))
    );
}

fn slip() -> CashKept {
    CashKept {
        months: 12,
        amount_cents: 990,
        currency: String::from("EUR"),
        activation_code: ACTIVATION.to_string(),
        mail_to_name: String::from("Azlin Test Operator"),
        mail_to_lines: vec![String::from("Postfach 10 20 30"), String::from("12345 Teststadt")],
        expires_at: String::from("2026-12-09T10:00:00Z"),
    }
}

#[test]
fn a_cash_checkout_keeps_its_slip_in_the_keyring_and_an_older_entry_reads_without_one() {
    let dir = TempDir::new("azcloud-cash");
    let shared = shared(&dir);
    let claim = ClaimKey::generate().unwrap();
    let mut kept = PendingCheckout::new(CHECKOUT, &claim, "100GB", TOKEN, "Photos");
    kept.method = String::from("cash");
    kept.cash = Some(slip());
    pending::add(&shared, &kept).unwrap();
    let listed = pending::list(&shared).unwrap();
    assert_eq!(listed, vec![kept.clone()]);
    assert!(listed[0].is_cash());
    // A checkout written before cash existed reads as one of another method.
    let old: PendingCheckout = serde_json::from_str(
        r#"{"checkout_id": "ck_old", "claim_secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
            "tier": "100GB", "started_at": 1791450000}"#,
    )
    .unwrap();
    assert!(!old.is_cash() && old.method.is_empty() && old.cash.is_none());
    // Its Debug shows the slip (no secret of the drive) but never the claim secret.
    let shown = format!("{kept:?}");
    assert!(shown.contains("AZC1-") && !shown.contains(&claim.to_base64()), "{shown}");
}

#[test]
fn a_picked_up_checkout_learns_its_tier_from_the_sealed_sign_up() {
    let dir = TempDir::new("azcloud-cash");
    let shared = shared(&dir);
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let kept = ClaimCode::new(CHECKOUT, &claim).pending(TOKEN, "Photos");
    pending::add(&shared, &kept).unwrap();
    let sealed = seal(
        &claim.public_base64(),
        CHECKOUT,
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    let transport = Shared(Fake::new(move |_, _| {
        Ok(json(
            200,
            &format!(
                r#"{{"checkout_id": "{CHECKOUT}", "status": "approved",
                     "sealed_signup": "{sealed}"}}"#
            ),
        ))
    }));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &shared, &kept) {
        Polled::Final(Final::Claimed(claimed)) => {
            assert_eq!(claimed.bundle.tier.as_deref(), Some("100GB"));
        }
        other => panic!("not claimed: {other:?}"),
    }
    let listed = pending::list(&shared).unwrap();
    assert_eq!(
        listed[0].tier, "100GB",
        "its period tokens are issued for the tier the drive has"
    );
}

#[test]
fn the_tier_list_says_whether_the_token_server_takes_cash_by_post() {
    let ladder = |methods: &str| {
        Tiers::parse(&format!(
            r#"{{"tiers": [{{"id": "100GB", "quota_bytes": 100000000000,
                 "price_cents_month": 99, "price_cents_year": 990, "currency": "EUR",
                 "prepay_months": [1, 12]}}],
                "methods": {methods}}}"#
        ))
        .unwrap()
    };
    assert!(ladder(r#"["sepa", "card", "cash"]"#).takes_cash());
    assert!(!ladder(r#"["sepa", "card"]"#).takes_cash(), "a server without its cash key");
}

#[test]
fn a_cash_checkout_names_its_method_and_claim_key_and_no_provider() {
    // The token server takes cash as a method of POST /v1/checkout (cash contract v1), with or
    // without payment options: the app names no provider or surface for it.
    let fake = Fake::new(|_, _| {
        Ok(json(
            201,
            r#"{"checkout_id": "ck_aaaqeayeaudaocajbifqydiob4", "status": "awaiting_cash",
                "tier": "100GB", "months": 12, "amount_cents": 1188, "currency": "EUR",
                "activation_code": "AZC1-AAAQ-EAYE-AUDA-OCAJ-BIFQ-YDIO-B4AA-ABFE-IVKV-F3QG-5NDF-5KZK-SMTY-I",
                "mail_to": {"name": "Azlin", "lines": ["Postfach 1"]},
                "expires_at": "2026-12-09T00:00:00Z"}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let claim = ClaimKey::generate().unwrap();
    let (checkout, answer) = server.checkout_cash("100GB", 12, "de", true, &claim, crate::token::CheckoutFor::NewDrive).unwrap();
    assert_eq!(checkout.checkout_id, CODE_CHECKOUT);
    assert_eq!(checkout.amount_cents, 1188);
    assert_eq!(answer["status"], "awaiting_cash");
    let sent: serde_json::Value = serde_json::from_slice(&fake.calls()[0].body).unwrap();
    assert_eq!(sent["method"], "cash");
    assert_eq!(sent["tier"], "100GB");
    assert_eq!(sent["months"], 12);
    assert_eq!(sent["vat_country"], "DE");
    assert_eq!(sent["withdrawal_consent"], true);
    assert_eq!(sent["claim_key"], claim.public_base64());
    assert!(sent.get("provider").is_none() && sent.get("surface").is_none(), "{sent}");
    assert!(fake.calls()[0].url.ends_with("/v1/checkout"));
}

// ==== The new drive's key and its claim code (feature `encryption`) ====

/// The token server of [`CHECKOUT`], approved: its sign-up (the test bundle, drive `d_1`)
/// sealed to `claim`.
#[cfg(feature = "encryption")]
fn approved(claim: &ClaimKey) -> Arc<Fake> {
    let sealed = seal(
        &claim.public_base64(),
        CHECKOUT,
        bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa").as_bytes(),
    );
    Fake::new(move |_, _| {
        Ok(json(
            200,
            &format!(
                r#"{{"checkout_id": "{CHECKOUT}", "status": "approved",
                     "sealed_signup": "{sealed}"}}"#
            ),
        ))
    })
}

/// One computer's keyring with its locks under `dir/<name>`, and the keyring to look into.
#[cfg(feature = "encryption")]
fn computer(dir: &TempDir, name: &str) -> (SharedKeyring, Arc<MemoryKeyring>) {
    let keyring = Arc::new(MemoryKeyring::new());
    let shared = SharedKeyring::with_lock_root(
        keyring.clone(),
        KeyringNamespace::azlin_shared(),
        &dir.path().join(name),
    );
    (shared, keyring)
}

/// A tiny Argon2id cost: the tests run in debug builds too.
#[cfg(feature = "encryption")]
fn cheap() -> azul_storage::crypto::keys::RecoveryKdf {
    azul_storage::crypto::keys::RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

#[cfg(feature = "encryption")]
#[test]
fn a_new_drives_cash_checkout_sets_its_drive_up_with_the_key_its_claim_code_names() {
    use azul_storage::{
        crypto::{device, KeyId},
        keyring::KeyringStore,
        meta::MemoryBucket,
    };

    let dir = TempDir::new("azcloud-cash");
    let (buyer, buyer_keys) = computer(&dir, "buyer");
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let mut kept = PendingCheckout::new(CHECKOUT, &claim, "100GB", TOKEN, "Photos");
    kept.method = String::from("cash");
    kept.mint_drive_key().unwrap();
    let secret = kept.drive_key.clone().expect("the drive's key, made with its order");
    let named = kept.key_id.clone().expect("the claim code names its id");
    assert_eq!((secret.len(), named.len()), (64, 32));
    assert!(!format!("{kept:?}").contains(&secret), "Debug shows no drive key");
    let code = ClaimCode::of(&kept).unwrap();
    assert!(code.to_text().starts_with(KEYED_CLAIM_CODE_PREFIX));
    assert_eq!(code.key_id.map(|id| KeyId(id).to_hex()), Some(named.clone()));
    pending::add(&buyer, &kept).unwrap();
    assert_eq!(pending::list(&buyer).unwrap(), vec![kept.clone()], "kept with its key");

    // Paid: the claim keeps the key as the one the drive is set up with.
    let transport = Shared(approved(&claim));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &buyer, &kept) {
        Polled::Final(Final::Claimed(claimed)) => assert_eq!(claimed.bundle.drive_id(), "d_1"),
        other => panic!("not claimed: {other:?}"),
    }
    assert!(buyer_keys
        .get(&device::planned_key_entry("d_1"))
        .unwrap()
        .is_some());
    let bucket = MemoryBucket::new();
    let (drive_key, recovery) =
        device::setup_new_drive(&bucket, &*buyer_keys, "d_1", cheap()).unwrap();
    assert_eq!(drive_key.id().to_hex(), named, "the drive's key is the one the code names");

    // Another computer picks the drive up with the code: it takes that key and no other.
    let (picker, picker_keys) = computer(&dir, "picker");
    let picked = ClaimCode::parse(&code.to_text())
        .unwrap()
        .pending(TOKEN, "Photos");
    pending::add(&picker, &picked).unwrap();
    match pending::poll(&server, &picker, &picked) {
        Polled::Final(Final::Claimed(_)) => {}
        other => panic!("not picked up: {other:?}"),
    }
    assert_eq!(
        device::committed_drive_key(&*picker_keys, "d_1").unwrap(),
        Some(drive_key.id())
    );
    assert!(
        picker_keys
            .get(&device::planned_key_entry("d_1"))
            .unwrap()
            .is_none(),
        "the picker makes no key of its own for the drive"
    );
    assert_eq!(
        device::recover(&bucket, &*picker_keys, "d_1", &recovery).unwrap(),
        drive_key
    );
    assert_eq!(
        device::pinned_drive_key(&*picker_keys, "d_1").unwrap(),
        Some(drive_key.id())
    );

    // Its period tokens wait with the drive; its key leaves the list (the keyring's drive
    // entries have it), the id its code names stays.
    let grant = crate::PeriodTokens {
        checkout_id: CHECKOUT.to_string(),
        months: 12,
        issue_key: String::from("issue-key"),
        recurring: false,
    };
    pending::claimed(&buyer, CHECKOUT, "d_1", Some(&grant)).unwrap();
    let listed = pending::list(&buyer).unwrap();
    assert_eq!(listed[0].drive_key, None);
    assert_eq!(listed[0].key_id.as_deref(), Some(named.as_str()));
}

#[cfg(feature = "encryption")]
#[test]
fn a_picked_up_drive_whose_storage_holds_another_key_than_its_claim_code_names_is_refused() {
    use azul_storage::{crypto::device, meta::MemoryBucket, DriveError};

    let dir = TempDir::new("azcloud-cash");
    let (picker, picker_keys) = computer(&dir, "picker");
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let picked = ClaimCode::parse(KEYED_CLAIM_CODE)
        .unwrap()
        .pending(TOKEN, "Photos");
    pending::add(&picker, &picked).unwrap();
    let transport = Shared(approved(&claim));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &picker, &picked) {
        Polled::Final(Final::Claimed(_)) => {}
        other => panic!("not picked up: {other:?}"),
    }
    // Whoever controls the storage set the drive up with a key of their own: its recovery
    // code opens a key the claim code does not name, and this computer takes none of it.
    let bucket = MemoryBucket::new();
    let (_, recovery) =
        device::setup_new_drive(&bucket, &MemoryKeyring::new(), "d_1", cheap()).unwrap();
    assert!(matches!(
        device::recover(&bucket, &*picker_keys, "d_1", &recovery),
        Err(DriveError::KeyRefused { .. })
    ));
    assert_eq!(device::load_drive_key(&*picker_keys, "d_1").unwrap(), None);
}

#[cfg(feature = "encryption")]
#[test]
fn a_kept_drive_key_that_is_not_the_one_its_claim_code_names_is_never_planned() {
    use azul_storage::{crypto::device, keyring::KeyringStore};

    let dir = TempDir::new("azcloud-cash");
    let (buyer, buyer_keys) = computer(&dir, "buyer");
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let mut kept = PendingCheckout::new(CHECKOUT, &claim, "100GB", TOKEN, "Photos");
    kept.method = String::from("cash");
    kept.mint_drive_key().unwrap();
    // The list's entry was changed: its id is another key's.
    kept.key_id = Some(KEY_ID_HEX.to_string());
    pending::add(&buyer, &kept).unwrap();
    let transport = Shared(approved(&claim));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match pending::poll(&server, &buyer, &kept) {
        Polled::Transient(pending::Transient::NotKept(why)) => {
            assert!(why.contains("key"), "{why}");
        }
        other => panic!("claimed with a key its code does not name: {other:?}"),
    }
    assert!(buyer_keys
        .get(&device::planned_key_entry("d_1"))
        .unwrap()
        .is_none());
    assert_eq!(pending::list(&buyer).unwrap().len(), 1, "it stays on the list");
}
