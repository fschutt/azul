//! Cash by post (cash contract v1) on the app's side: the activation code a slip prints, the
//! claim code (AZK1) on the buyer's copy, the poll of a cash checkout, its slip kept with it in
//! the keyring's list, and a checkout picked up by its code on another computer.

use std::sync::Arc;

use azul_storage::{keyring::MemoryKeyring, testing::TempDir};

use super::{bundle, json, Fake, Shared, TOKEN};
use crate::{
    cash::{ActivationCode, ClaimCode, ACTIVATION_PREFIX, CLAIM_CODE_PREFIX},
    claim::seal,
    lock::LockDir,
    pending::{self, CashKept, PendingCheckout, Polled},
    shared::SharedKeyring,
    token::{CheckoutStatus, TokenServer},
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

fn shared(dir: &TempDir) -> SharedKeyring {
    SharedKeyring::new(
        Arc::new(MemoryKeyring::new()),
        LockDir::new(dir.path().join("locks")),
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
        Polled::Claimed(claimed) => assert_eq!(claimed.bundle.tier.as_deref(), Some("100GB")),
        other => panic!("not claimed: {other:?}"),
    }
    let listed = pending::list(&shared).unwrap();
    assert_eq!(
        listed[0].tier, "100GB",
        "its period tokens are issued for the tier the drive has"
    );
}
