//! The customer's receipt (ENT27-B06): made from the record of what the token server booked,
//! saying only what the record holds, in the window's language, with the file's name safe.

use azcloud_kit::{
    azlin_contract::ReceiptWire,
    pending::{PaidList, PaidRecord},
    CloudError,
};
use azul::prelude::*;
use azul_appkit::l10n::set_locale;
use azul_storage::testing::TempDir;

use crate::receipt::{file_name, lines, receipt_dom, row_text, shown};

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

/// A record of the checkout `checkout` for the drive `drive`, seen paid at `at`.
fn paid(checkout: &str, drive: &str, at: i64) -> PaidRecord {
    PaidRecord {
        checkout: checkout.to_string(),
        drive: drive.to_string(),
        at,
        ..record()
    }
}

/// The paid list of the folder `dir` after `records` were kept, oldest first.
fn kept(dir: &TempDir, records: &[PaidRecord]) -> PaidList {
    let list = PaidList::in_dir(dir.path());
    for record in records {
        list.keep(record).expect("a record is kept");
    }
    list
}

/// X-DELTA-AZDRIVE-MONEY-17: the Receipts entry of the Options page shows EVERYTHING this
/// computer paid - the records of a drive that was removed since, or has lapsed, as well as the
/// ones of the drives in the list - newest first. Before this the only way to a receipt was the
/// button in the Options row of a drive still in the list, and it showed that drive's records
/// alone: a customer who removed a drive could not get its receipt from the app although the
/// record lay in `paid.json`.
#[test]
fn x_delta_azdrive_money_17_every_record_is_listed_newest_first_whatever_drive_it_was_paid_for() {
    let dir = TempDir::new("receipts-all");
    let list = kept(
        &dir,
        &[
            paid("ck_a", "d_listed", 1_793_000_000),
            paid("ck_b", "d_removed", 1_793_100_000),
            paid("ck_c", "d_listed", 1_793_200_000),
            paid("ck_d", "d_lapsed", 1_793_300_000),
        ],
    );
    let (records, error) = shown(Ok(list));
    assert!(error.is_empty(), "{:?}", error.keys());
    let order: Vec<&str> = records.iter().map(|r| r.checkout.as_str()).collect();
    assert_eq!(
        order,
        ["ck_d", "ck_c", "ck_b", "ck_a"],
        "newest first, the receipts of the drives that are gone among them"
    );
    let drives: Vec<&str> = records.iter().map(|r| r.drive.as_str()).collect();
    assert!(drives.contains(&"d_removed"), "{drives:?}");
    assert!(drives.contains(&"d_lapsed"), "{drives:?}");
}

/// Nothing paid on this computer is no error: the dialog is empty and says nothing is wrong.
#[test]
fn x_delta_azdrive_money_17_a_computer_that_paid_nothing_shows_an_empty_list_and_no_error() {
    let dir = TempDir::new("receipts-none");
    let (records, error) = shown(Ok(PaidList::in_dir(dir.path())));
    assert!(records.is_empty());
    assert!(error.is_empty(), "{:?}", error.keys());
}

/// A list that cannot be read is said - with the reason - and never shown as "nothing paid": the
/// customer would take the empty page for a lost receipt. Both ways it fails are held: the system
/// has no place for the list, and the file is cut off.
#[test]
fn x_delta_azdrive_money_17_a_list_that_cannot_be_read_is_said_never_shown_as_nothing_paid() {
    crate::l10n::in_english();
    let (records, error) = shown(Err(CloudError::failed("no place to keep it")));
    assert!(records.is_empty());
    assert_eq!(error.keys(), vec!["azdrive-receipts-unreadable"]);
    let why = error
        .phrase("azdrive-receipts-unreadable")
        .and_then(|phrase| phrase.get("why"))
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(why.contains("no place to keep it"), "{why}");

    let dir = TempDir::new("receipts-torn");
    std::fs::write(dir.path().join("paid.json"), b"{\"format\": \"azcloud.pa")
        .expect("a cut-off list");
    let (records, error) = shown(Ok(PaidList::in_dir(dir.path())));
    assert!(
        records.is_empty(),
        "no record is made up from a list that does not read"
    );
    assert_eq!(error.keys(), vec!["azdrive-receipts-unreadable"]);
}

/// A row says which drive the payment was for: its name while the drive is in the list, its id
/// once it is gone (the record holds the id only) - in the window's language, with the day, the
/// tier, the months and the amount the token server booked.
#[test]
fn x_delta_azdrive_money_17_a_row_names_its_drive_by_name_while_listed_and_by_id_once_gone() {
    crate::l10n::in_english();
    let listed = row_text(&record(), "Photos");
    assert!(
        listed.starts_with("2026-11-05: Photos, 100GB, "),
        "{listed}"
    );
    assert!(listed.contains("12 months"), "{listed}");
    assert!(listed.ends_with("EUR 9.90"), "{listed}");

    let gone = row_text(&record(), "d_1");
    assert!(gone.starts_with("2026-11-05: d_1, 100GB, "), "{gone}");

    set_locale("de-DE");
    let german = row_text(&record(), "Fotos");
    assert!(german.starts_with("2026-11-05: Fotos, 100GB, "), "{german}");
    assert!(german.contains("12 Monate"), "{german}");
    assert!(german.ends_with("9,90 EUR"), "{german}");
    set_locale("en-US");
}

/// The page of a receipt for a drive that is gone names the drive by its id once, not as
/// `d_1 (d_1)`; a drive that is still listed is its name and its id.
#[test]
fn x_delta_azdrive_money_17_the_page_of_a_removed_drive_shows_its_id_once() {
    crate::l10n::in_english();
    assert_eq!(
        value(&lines(&record(), ""), "azdrive-receipt-drive"),
        Some("d_1")
    );
    assert_eq!(
        value(&lines(&record(), "d_1"), "azdrive-receipt-drive"),
        Some("d_1"),
        "the id is not said twice"
    );
    assert_eq!(
        value(&lines(&record(), "Photos"), "azdrive-receipt-drive"),
        Some("Photos (d_1)")
    );
}
