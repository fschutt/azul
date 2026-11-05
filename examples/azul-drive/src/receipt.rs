//! The customer's receipt (ENT27-B06), on a durable medium: Options > Drives > "Receipts" of an
//! Azlin drive lists what this computer paid for it and saves a page as a PDF.
//!
//! The token server books each sale and answers it with the checkout's approval; azcloud-kit
//! keeps the booked sale as a [`PaidRecord`] before the checkout leaves its list. The page is
//! made from that record - the amount, the VAT rate, the day, the country, the payment scheme
//! the token server booked, the order and the drive - never from today's tier list. A token
//! server that books no sale leaves a record without an amount, and the page says so instead of
//! making one up. The wording is neutral and factual: it says what was booked and that it gives
//! no tax advice (the legal text of a receipt is not decided here).
//!
//! On stdout, for scripts: `AZDRIVE_RECEIPT_SAVED <checkout id> <bytes>`.

use azcloud_kit::{
    azlin_contract::{Cents, ReceiptWire},
    pending::PaidRecord,
};
use azul::{
    callbacks::ButtonOnClickCallbackType, dialog::FileDialog, pdf::Pdf, prelude::*,
    str::String as AzString,
};
use azul_appkit::l10n::{label, t, t_args, Arg, Text};

use crate::{
    ids,
    paper::{block, A4},
    pay_words::show,
    ui_dialogs::{button, buttons, line, on_cancel_popup, text_line},
    with_state, DriveState, Popup,
};

/// The receipt's lines: the label (a key of the resources) and what it says, in the window's
/// language. Nothing is said that the record does not hold.
#[must_use]
pub(crate) fn lines(record: &PaidRecord, drive: &str) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("azdrive-receipt-checkout", record.checkout.clone()),
        (
            "azdrive-receipt-drive",
            if drive.is_empty() {
                record.drive.clone()
            } else {
                format!("{drive} ({})", record.drive)
            },
        ),
    ];
    if !record.tier.is_empty() {
        rows.push(("azdrive-receipt-tier", record.tier.clone()));
    }
    if record.months > 0 {
        rows.push(("azdrive-receipt-months", record.months.to_string()));
    }
    let Some(receipt) = &record.receipt else {
        let day = azul_storage::time::iso8601(u64::try_from(record.at).unwrap_or(0));
        rows.push(("azdrive-receipt-date", day.chars().take(10).collect()));
        return rows;
    };
    rows.push(("azdrive-receipt-date", receipt.issue_day.clone()));
    if !receipt.service.trim().is_empty() {
        rows.push(("azdrive-receipt-service", receipt.service.trim().to_string()));
    }
    if let Some(gross) = record.charge() {
        rows.push(("azdrive-receipt-amount", show(&gross)));
    }
    rows.push(("azdrive-receipt-vat", vat_rate(receipt)));
    rows.push(("azdrive-receipt-country", receipt.country.clone()));
    rows.push(("azdrive-receipt-method", receipt.scheme.clone()));
    rows.push(("azdrive-receipt-document", kind_text(&receipt.kind)));
    if let Some(eur) = &receipt.eur {
        if let (Ok(cents), Some(euro)) = (u64::try_from(eur.cents), euro()) {
            rows.push((
                "azdrive-receipt-eur",
                t_args(
                    "azdrive-receipt-eur-rate",
                    &[
                        (
                            "amount",
                            Arg::from(show(&Cents {
                                amount: cents,
                                currency: euro,
                            })),
                        ),
                        ("source", Arg::from(eur.rate_source.as_str())),
                        ("day", Arg::from(eur.rate_day.as_str())),
                    ],
                ),
            ));
        }
    }
    rows
}

/// The euro as a currency of the contract.
fn euro() -> Option<azcloud_kit::azlin_contract::Currency> {
    azcloud_kit::azlin_contract::Currency::parse("EUR").ok()
}

/// The VAT rate of the booked sale: `19 %`, `7.5 %` in the language's marks.
fn vat_rate(receipt: &ReceiptWire) -> String {
    let permille = receipt.rate_permille;
    let rate = if permille % 10 == 0 {
        (permille / 10).to_string()
    } else {
        azul_appkit::l10n::decimal(&format!("{}.{}", permille / 10, permille % 10))
    };
    t_args("azdrive-receipt-vat-rate", &[("rate", Arg::from(rate))])
}

/// What kind of document the token server booked, in words (the wire's word for one this build
/// does not know).
fn kind_text(kind: &str) -> String {
    match kind {
        "payment_receipt" => t("azdrive-receipt-kind-receipt"),
        "small_amount_invoice" => t("azdrive-receipt-kind-small"),
        other => other.to_string(),
    }
}

/// The page: black on white whatever mode the window is in, the lines under a title, and the note.
#[must_use]
pub(crate) fn receipt_dom(record: &PaidRecord, drive: &str) -> Dom {
    let mut body = Dom::create_body().with_css(
        "margin: 0px; padding: 56px; background: #ffffff; color: #000000; font-family: \
         sans-serif; font-size: 13px; display: flex; flex-direction: column;",
    );
    body.add_child(block(
        &t("azdrive-receipt-title"),
        "font-size: 26px; font-weight: bold;",
    ));
    body.add_child(block(
        &t("azdrive-receipt-subtitle"),
        "margin-top: 4px; padding-bottom: 10px; border-bottom: 2px solid #000000;",
    ));
    for (key, value) in lines(record, drive) {
        body.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; margin-top: 8px;")
                .with_child(block(
                    &format!("{}:", t(key)),
                    "width: 190px; flex-shrink: 0; font-weight: bold;",
                ))
                .with_child(block(&value, "flex-grow: 1;")),
        );
    }
    if record.receipt.is_none() {
        body.add_child(block(&t("azdrive-receipt-no-sale"), "margin-top: 18px;"));
    }
    body.add_child(block(
        &t("azdrive-receipt-note"),
        "margin-top: 28px; padding-top: 10px; border-top: 1px solid #000000; font-size: 12px;",
    ));
    body
}

/// The page as a PDF (azul's writer lays the DOM out in this callback, with the window's fonts).
pub(crate) fn receipt_pdf(
    info: &mut CallbackInfo,
    record: &PaidRecord,
    drive: &str,
) -> Result<Vec<u8>, String> {
    let bytes = Pdf::create()
        .from_dom_in_callback(*info, receipt_dom(record, drive), A4.0, A4.1)
        .as_ref()
        .to_vec();
    if bytes.is_empty() {
        Err(t("azdrive-kit-no-pdf"))
    } else {
        Ok(bytes)
    }
}

/// The file the page is saved as: `Azlin receipt <checkout id>.pdf` (the id's letters, digits,
/// `-` and `_` only).
#[must_use]
pub(crate) fn file_name(record: &PaidRecord) -> String {
    let id: String = record
        .checkout
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("Azlin receipt {id}.pdf")
}

// ==== The dialog ====

/// Opens the receipts of `drive_id`: what this computer paid for it, newest first.
pub(crate) fn open(s: &mut DriveState, drive_id: &str) {
    let (records, error) = match s.keyring.paid().records() {
        Ok(all) => {
            let mut mine: Vec<PaidRecord> =
                all.into_iter().filter(|r| r.drive == drive_id).collect();
            mine.reverse();
            (mine, Text::default())
        }
        // A list that cannot be read is said; it is never shown as "nothing paid".
        Err(why) => (
            Vec::new(),
            azul_appkit::l10n::Phrase::new("azdrive-receipts-unreadable")
                .arg("why", why.to_string())
                .into(),
        ),
    };
    s.popups_opened += 1;
    s.popup.open(Popup::Receipts {
        drive_id: drive_id.to_string(),
        records,
        error,
    });
}

/// What a Save button carries.
struct SaveRef {
    app: RefAny,
    checkout: String,
}

/// The dialog: title and body.
pub(crate) fn dialog_parts(
    s: &DriveState,
    app: &RefAny,
    drive_id: &str,
    records: &[PaidRecord],
    error: &Text,
) -> (String, Dom) {
    let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
    let mut body = Dom::create_div()
        .with_id(ids::RECEIPTS)
        .with_css("display: flex; flex-direction: column; min-width: 420px;");
    if !error.is_empty() {
        body.add_child(text_line(error).with_css("color: system:danger;"));
    } else if records.is_empty() {
        body.add_child(line("azdrive-receipts-none"));
    }
    for record in records {
        let what = t_args(
            "azdrive-receipts-row",
            &[
                ("date", Arg::from(day_of(record))),
                ("tier", Arg::from(record.tier.as_str())),
                ("months", Arg::from(record.months)),
                (
                    "amount",
                    Arg::from(record.charge().map_or_else(String::new, |c| show(&c))),
                ),
            ],
        );
        body.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 0px;")
                .with_child(
                    Dom::create_span_with_text(AzString::from(what)).with_css("flex-grow: 1;"),
                )
                .with_child(
                    Button::create(label("azdrive-receipts-save"))
                        .with_on_click(
                            RefAny::new(SaveRef {
                                app: app.clone(),
                                checkout: record.checkout.clone(),
                            }),
                            on_save as ButtonOnClickCallbackType,
                        )
                        .dom()
                        .with_id(ids::receipt_save(&record.checkout)),
                ),
        );
    }
    body.add_child(buttons(vec![button("kit-button-ok", app, on_cancel_popup)]));
    (t_args("azdrive-receipts-title", &[("name", Arg::from(name))]), body)
}

/// The day of `record`: the booked sale's, else the one this computer saw it paid.
fn day_of(record: &PaidRecord) -> String {
    match &record.receipt {
        Some(receipt) => receipt.issue_day.clone(),
        None => azul_storage::time::iso8601(u64::try_from(record.at).unwrap_or(0))
            .chars()
            .take(10)
            .collect(),
    }
}

/// Save as PDF: the system's save dialog, the app's state let go before it.
extern "C" fn on_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, checkout)) = data
        .downcast_ref::<SaveRef>()
        .map(|r| (r.app.clone(), r.checkout.clone()))
    else {
        return Update::DoNothing;
    };
    let made = {
        let Some(s) = app.downcast_ref::<DriveState>() else {
            return Update::DoNothing;
        };
        let Some(Popup::Receipts {
            drive_id, records, ..
        }) = s.popup.as_ref()
        else {
            return Update::DoNothing;
        };
        records.iter().find(|r| r.checkout == checkout).map(|record| {
            let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
            (
                file_name(record),
                receipt_pdf(&mut info, record, &name),
            )
        })
    };
    let Some((name, bytes)) = made else {
        return Update::DoNothing;
    };
    let note = match bytes {
        Err(why) => Text::plain(why),
        Ok(bytes) => {
            let len = bytes.len();
            if FileDialog::save_bytes(name.as_str(), "application/pdf", bytes) {
                println!("AZDRIVE_RECEIPT_SAVED {checkout} {len}");
                azul_appkit::l10n::Phrase::new("azdrive-kit-saved")
                    .arg("name", name.as_str())
                    .into()
            } else {
                Text::key("azdrive-kit-not-saved")
            }
        }
    };
    with_state(&mut app, &mut info, |_info, _app, s| s.info(note))
}
