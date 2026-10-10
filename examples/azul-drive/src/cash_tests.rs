//! Cash by post in AzDrive (cash contract v1): the pill, the two pages (the buyer's copy with
//! the claim code, the slip posted with the cash), the waiting line, a claim code picked up on
//! another computer, the daily look. No window.

use azcloud_kit::{ClaimCode, ClaimKey, PendingCheckout};
use azul_pay::{
    cash::{CashSlip, MailTo},
    offer::{Offer, OfferContext},
    registry::{Method, SurfaceKind},
};

use crate::{
    add_drive::{AddDialog, AddPage, TiersState},
    cash::{copy_paper, ended_text, picked_up, slip_paper, Letter, WAITING_TEXT},
    periods::Schedule,
};

const CHECKOUT: &str = "ck_aaaaaaaaaaaaaaaaaaaaaaaaaa";
const CODE: &str = "AZC1-MNVV-6YLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYIA-AAB5-\
                    4RKV-KI74-IMPG-BG5O-LTW7-WE";
const SECRET: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
const TOKEN: &str = "http://127.0.0.1:18081";

fn slip() -> CashSlip {
    CashSlip {
        checkout_id: CHECKOUT.to_string(),
        amount_cents: 990,
        currency: String::from("EUR"),
        activation_code: CODE.to_string(),
        mail_to: MailTo {
            name: String::from("Azlin Test Operator"),
            lines: vec![
                String::from("Postfach 10 20 30"),
                String::from("12345 Teststadt"),
                String::from("Germany"),
            ],
        },
        expires_at: Some(String::from("2026-12-09T10:00:00Z")),
    }
}

fn letter() -> Letter {
    Letter::of_slip(&slip(), "100GB", 12)
}

fn claim_code() -> String {
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    ClaimCode::new(CHECKOUT, &claim).to_text().to_string()
}

fn all_text(paper: &crate::paper::Paper) -> String {
    let mut text = vec![paper.title.clone(), paper.subtitle.clone()];
    text.extend(paper.address.iter().cloned());
    text.extend(paper.text.iter().cloned());
    text.push(paper.label.to_string());
    text.push(paper.qr_label.to_string());
    text.join("\n")
}

// ==== The pill ====

#[test]
fn cash_by_post_is_a_pill_of_buy_storage_when_the_token_server_offers_it() {
    let offer = Offer::parse(
        &serde_json::json!({"offers": [
            {"provider": "stripe", "methods": [{"method": "card"}]},
            {"provider": "cash", "methods": [{"method": "cash", "surfaces": ["paper"]}]}]})
        .to_string(),
        &OfferContext::for_token_url("https://token.azlin.io"),
    )
    .unwrap();
    let mut dialog = AddDialog::new(1);
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(azcloud_kit::Tiers {
        tiers: vec![azcloud_kit::Tier {
            id: String::from("100GB"),
            quota_bytes: 100_000_000_000,
            price_cents_month: Some(99),
            price_cents_year: Some(990),
            currency: String::from("EUR"),
            first_month_free: true,
        }],
        methods: Vec::new(),
        withdrawal_consent: None,
    });
    dialog.offer_loaded(offer);
    let methods: Vec<Method> = dialog.pills().iter().map(|p| p.method).collect();
    assert_eq!(methods, vec![Method::Card, Method::Cash]);
    dialog.choose_pill(Method::Cash);
    assert_eq!(
        dialog.choice().unwrap().method.surfaces,
        vec![SurfaceKind::Paper],
        "AzDrive prints paper"
    );
    assert_eq!(dialog.pills_line(), "card:stripe cash:cash");
}

// ==== The two pages ====

#[test]
fn the_buyers_copy_says_what_was_bought_the_amount_and_the_checkout_and_holds_the_claim_code() {
    let paper = copy_paper(&letter(), &claim_code(), "2026-10-10");
    let text = all_text(&paper);
    for said in [
        "100 GB",
        "12 months",
        "EUR 9.90",
        "nine euros and ninety cents",
        CHECKOUT,
        "Keep this; AzDrive picks up your drive once the money arrived.",
        "once a day",
        "Pick up a paid drive with a claim code",
    ] {
        assert!(text.contains(said), "{said:?} is not on the copy:\n{text}");
    }
    assert_eq!(paper.secret.as_str(), claim_code(), "the claim code as text and QR code");
    assert!(paper.label.contains("claim code"));
    assert!(paper.file_name.ends_with(".pdf"));
    assert!(!text.contains(CODE), "the activation code is the slip's");
}

#[test]
fn the_slip_names_the_address_the_amount_in_words_and_digits_and_the_activation_code_only() {
    let paper = slip_paper(&letter(), "2026-10-10");
    assert_eq!(
        paper.address,
        vec![
            "Azlin Test Operator",
            "Postfach 10 20 30",
            "12345 Teststadt",
            "Germany"
        ]
    );
    let text = all_text(&paper);
    for said in [
        "EUR 9.90",
        "nine euros and ninety cents",
        "Put this slip and exactly EUR 9.90 in cash in the envelope.",
        CHECKOUT,
    ] {
        assert!(text.contains(said), "{said:?} is not on the slip:\n{text}");
    }
    assert_eq!(paper.secret.as_str(), CODE, "the activation code as text and QR code");
    // No secret of the drive goes into the post.
    assert!(!text.contains("AZK1") && !paper.secret.contains("AZK1"));
    assert!(!text.contains(SECRET));
    assert_ne!(
        paper.file_name,
        copy_paper(&letter(), &claim_code(), "2026-10-10").file_name
    );
}

#[test]
fn a_letter_is_made_from_the_answer_and_again_from_the_keyrings_list() {
    let letter = letter();
    assert_eq!(letter.amount_text(), "EUR 9.90");
    assert_eq!(letter.amount_words(), "nine euros and ninety cents");
    let claim = ClaimKey::from_base64(SECRET).unwrap();
    let mut kept = PendingCheckout::new(CHECKOUT, &claim, "100GB", TOKEN, "Photos");
    kept.method = String::from("cash");
    kept.cash = Some(letter.kept());
    assert_eq!(Letter::of(&kept), Some(letter), "Show the letter again prints the same");
    let picked = ClaimCode::new(CHECKOUT, &claim).pending(TOKEN, "Photos");
    assert_eq!(Letter::of(&picked), None, "a code picked up has no slip on this computer");
}

// ==== Waiting, ending, picking up ====

#[test]
fn the_waiting_line_says_postal_cash_takes_a_while_and_azdrive_looks_daily() {
    assert_eq!(
        WAITING_TEXT,
        "Waiting for your letter: postal cash takes a while, AzDrive checks once a day."
    );
    let text = ended_text("the payment did not go through: the envelope held less");
    assert!(text.contains("the envelope held less"), "{text}");
}

#[test]
fn a_claim_code_picks_up_the_cash_checkout_as_typed_and_a_typo_is_refused() {
    let code = claim_code();
    let kept = picked_up(&code.to_lowercase().replace('-', " "), TOKEN, "Photos").unwrap();
    assert_eq!(kept.checkout_id, CHECKOUT);
    assert_eq!(kept.claim_secret, SECRET);
    assert!(kept.is_cash());
    assert_eq!(kept.token_url, TOKEN);
    assert_eq!(kept.name, "Photos");
    let typo = code.replacen("DVRW", "DVRX", 1);
    let refused = picked_up(&typo, TOKEN, "Photos").unwrap_err();
    assert!(refused.contains("claim code"), "{refused}");
    assert!(picked_up("", TOKEN, "Photos").is_err());
    assert!(picked_up(&code, "", "Photos").is_err(), "no token server to ask");
}

#[test]
fn the_daily_look_asks_a_cash_checkout_once_a_day() {
    let mut looks = Schedule::new(86_400);
    let now = 1_791_450_000;
    assert_eq!(looks.due(["ck_1"], now), vec!["ck_1"], "a new one at once");
    looks.looked_at("ck_1", now);
    assert!(looks.due(["ck_1"], now + 3_600).is_empty(), "not in a loop");
    assert_eq!(looks.due(["ck_1"], now + 86_400), vec!["ck_1"], "a day later");
}

#[test]
fn the_choose_page_offers_to_pick_up_a_paid_drive_with_a_claim_code() {
    let mut dialog = AddDialog::new(1);
    dialog.choose_claim_code();
    assert_eq!(dialog.page, AddPage::ClaimCode);
    assert_eq!(dialog.page_line(), "claim-code");
    dialog.back();
    assert_eq!(dialog.page_line(), "choose");
}
