//! The customer's receipt (ENT27-B06): made from the record of what the token server booked,
//! saying only what the record holds, in the window's language, with the file's name safe.

use azcloud_kit::{azlin_contract::ReceiptWire, pending::PaidRecord};
use azul::prelude::*;
use azul_appkit::l10n::set_locale;

use crate::receipt::{file_name, lines, receipt_dom};

/// The booked sale of `ck_1`: EUR 9.90 at 19 % VAT, a direct debit, a day in November.
fn receipt() -> ReceiptWire {
    serde_json::from_str(
        r#"{"issue_day": "2026-11-05", "service": "Azlin Storage 100GB, 12 months",
            "gross_cents": 990, "currency": "EUR", "rate_permille": 190, "country": "DE",
            "scheme": "sepa", "checkout_id": "ck_1", "kind": "payment_receipt"}"#,
    )
    .expect("a receipt")
}

fn record() -> PaidRecord {
    PaidRecord {
        checkout: String::from("ck_1"),
        drive: String::from("d_1"),
        tier: String::from("100GB"),
        months: 12,
        at: 1_793_000_000,
        receipt: Some(receipt()),
    }
}

/// Every text `dom` shows, in document order.
fn texts_of(dom: &Dom) -> Vec<String> {
    let mut out: Vec<String> = dom
        .root
        .node_type
        .get_text()
        .into_option()
        .map(|t| t.as_str().to_string())
        .filter(|t| !t.is_empty())
        .into_iter()
        .collect();
    for child in dom.children.as_slice() {
        out.extend(texts_of(child));
    }
    out
}

fn value<'a>(rows: &'a [(&'static str, String)], key: &str) -> Option<&'a str> {
    rows.iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.as_str())
}

/// ENT27-B06: the receipt says what the token server booked - the order, the drive, the day, the
/// amount, the VAT rate, the country, how it was paid and which document it is - in the window's
/// language. Before this the `receipt` field was read nowhere.
#[test]
fn ent27_b06_the_receipt_lists_what_the_token_server_booked() {
    crate::l10n::in_english();
    let rows = lines(&record(), "Photos");
    assert_eq!(value(&rows, "azdrive-receipt-checkout"), Some("ck_1"));
    assert_eq!(value(&rows, "azdrive-receipt-drive"), Some("Photos (d_1)"));
    assert_eq!(value(&rows, "azdrive-receipt-tier"), Some("100GB"));
    assert_eq!(value(&rows, "azdrive-receipt-months"), Some("12"));
    assert_eq!(value(&rows, "azdrive-receipt-date"), Some("2026-11-05"));
    assert_eq!(
        value(&rows, "azdrive-receipt-service"),
        Some("Azlin Storage 100GB, 12 months")
    );
    assert_eq!(value(&rows, "azdrive-receipt-amount"), Some("EUR 9.90"));
    assert_eq!(value(&rows, "azdrive-receipt-vat"), Some("19 %"));
    assert_eq!(value(&rows, "azdrive-receipt-country"), Some("DE"));
    assert_eq!(value(&rows, "azdrive-receipt-method"), Some("sepa"));
    assert_eq!(
        value(&rows, "azdrive-receipt-document"),
        Some("Payment receipt")
    );
    assert_eq!(
        value(&rows, "azdrive-receipt-eur"),
        None,
        "a sale in euros has no euro line"
    );

    let texts = texts_of(&receipt_dom(&record(), "Photos"));
    for wanted in [
        "Payment receipt",
        "Order:",
        "ck_1",
        "EUR 9.90",
        "19 %",
        "2026-11-05",
    ] {
        assert!(texts.iter().any(|t| t == wanted), "{wanted}: {texts:?}");
    }
    assert!(
        texts.iter().any(|t| t.contains("gives no tax advice")),
        "the neutral note: {texts:?}"
    );
    assert!(!texts.iter().any(|t| t.contains("no sale")), "{texts:?}");

    set_locale("de-DE");
    let rows = lines(&record(), "Photos");
    assert_eq!(value(&rows, "azdrive-receipt-amount"), Some("9,90 EUR"));
    assert_eq!(
        value(&rows, "azdrive-receipt-document"),
        Some("Zahlungsbeleg")
    );
    let texts = texts_of(&receipt_dom(&record(), "Photos"));
    assert!(texts.iter().any(|t| t == "Zahlungsbeleg"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Bestellung:"), "{texts:?}");
    set_locale("en-US");
}

/// A sale paid in another currency also says the euro amount and the rate it was booked at.
#[test]
fn ent27_b06_a_sale_in_another_currency_says_its_euro_amount_and_rate() {
    crate::l10n::in_english();
    let mut zloty = record();
    zloty.receipt = Some(
        serde_json::from_str(
            r#"{"issue_day": "2026-11-05", "service": "Azlin Storage 100GB, 1 month",
                "gross_cents": 449, "currency": "PLN", "rate_permille": 230, "country": "PL",
                "scheme": "blik", "checkout_id": "ck_1", "kind": "small_amount_invoice",
                "eur": {"cents": 103, "rate_source": "ecb", "rate_day": "2026-11-04"}}"#,
        )
        .expect("a receipt"),
    );
    let rows = lines(&zloty, "");
    assert_eq!(value(&rows, "azdrive-receipt-amount"), Some("PLN 4.49"));
    assert_eq!(value(&rows, "azdrive-receipt-vat"), Some("23 %"));
    assert_eq!(
        value(&rows, "azdrive-receipt-document"),
        Some("Small-amount invoice")
    );
    assert_eq!(
        value(&rows, "azdrive-receipt-eur"),
        Some("EUR 1.03 at the rate of ecb of 2026-11-04")
    );
    assert_eq!(
        value(&rows, "azdrive-receipt-drive"),
        Some("d_1"),
        "no name: the id"
    );
}

/// A token server that books no sale leaves a record without an amount: the page says so and
/// states no amount, no VAT and no country of its own making.
#[test]
fn ent27_b06_a_record_without_a_booked_sale_states_no_amount() {
    crate::l10n::in_english();
    let mut bare = record();
    bare.receipt = None;
    let rows = lines(&bare, "Photos");
    for key in [
        "azdrive-receipt-amount",
        "azdrive-receipt-vat",
        "azdrive-receipt-country",
        "azdrive-receipt-method",
        "azdrive-receipt-service",
    ] {
        assert_eq!(value(&rows, key), None, "{key}");
    }
    assert_eq!(value(&rows, "azdrive-receipt-checkout"), Some("ck_1"));
    assert_eq!(value(&rows, "azdrive-receipt-months"), Some("12"));
    assert_eq!(
        value(&rows, "azdrive-receipt-date"),
        Some("2026-10-26"),
        "the day this computer saw it paid"
    );
    let texts = texts_of(&receipt_dom(&bare, "Photos"));
    assert!(
        texts.iter().any(|t| t.contains("booked no sale")),
        "{texts:?}"
    );
}

/// The file is named after the order; nothing in a checkout id can climb out of the folder.
#[test]
fn ent27_b06_the_files_name_holds_only_the_orders_letters_and_digits() {
    assert_eq!(file_name(&record()), "Azlin receipt ck_1.pdf");
    let mut odd = record();
    odd.checkout = String::from("../../etc/passwd ck_9");
    assert_eq!(file_name(&odd), "Azlin receipt etcpasswdck_9.pdf");
}

/// X-DELTA-AZDRIVE-MONEY-2: when the sale could not be recorded the user is told - the reason in
/// one sentence, as a phrase of the window's language - and when it was recorded, or was bought on
/// another computer, nothing is said. (The paid drive is delivered either way: the poll's tests
/// in azcloud-kit hold that.)
#[test]
fn x_delta_azdrive_money_2_the_user_is_told_when_the_receipt_was_not_kept_and_only_then() {
    use azcloud_kit::pending::Recorded;

    use crate::add_flow::receipt_notice;

    assert!(receipt_notice("ck_1", &Recorded::Kept).is_none());
    assert!(receipt_notice("ck_1", &Recorded::NotBoughtHere).is_none());
    let told = receipt_notice(
        "ck_1",
        &Recorded::NotKept(String::from("the folder is read-only")),
    )
    .expect("the user is told");
    assert_eq!(told.keys(), vec!["azdrive-receipt-not-kept"]);
    let why = told
        .phrase("azdrive-receipt-not-kept")
        .and_then(|phrase| phrase.get("why"))
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(why.contains("read-only"), "{why}");
}
