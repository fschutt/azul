//! Cash by post in AzDrive (cash contract v1): Buy storage's "Cash by post" pill makes a cash
//! checkout (azul-pay's machine: Posted), and AzDrive offers two pages to save or print
//! (`paper`):
//!
//! - **the buyer's copy**: what was bought, the amount in digits and in words, the checkout id,
//!   "Keep this; AzDrive picks up your drive once the money arrived.", and the CLAIM CODE
//!   (azcloud-kit's AZK1: the checkout id and the claim key's secret) as text and QR code - what
//!   another computer picks the drive up with when this one is lost (Add drive > "Pick up a paid
//!   drive with a claim code");
//! - **the slip** posted with the cash: the operator's address, the amount in words and digits,
//!   the activation code as text and QR code and "Put this slip and exactly <amount> in cash in
//!   the envelope." - no secret of the drive.
//!
//! The checkout waits in the keyring's list like every other (claim contract v1), its slip with
//! it, so its pages can be printed again from the drive list. AzDrive asks about it once a day
//! (the period timer's daily look, `AZDRIVE_PERIOD_CHECK_SECS` for a test), never in a loop,
//! and the drive list says "Waiting for your letter ..." until the drive is there - or why the
//! order ended (the cash rejected, the order expired). Nothing notifies: the buyer looks.
//!
//! On stdout, for scripts (never a code): `AZDRIVE_CASH_SLIP <checkout id>` (the pages offered),
//! `AZDRIVE_CASH_SAVED copy|slip <bytes>`, `AZDRIVE_CASH_PRINT copy|slip <bytes>`,
//! `AZDRIVE_CASH_WAITING <checkout id>` (a look found it awaiting its letter),
//! `AZDRIVE_CASH_ENDED <checkout id>`, `AZDRIVE_PICKED_UP <checkout id>`.

use azcloud_kit::{CashKept, ClaimCode, PendingCheckout, Zeroizing};
use azul::{
    callbacks::ButtonOnClickCallbackType, dialog::FileDialog, prelude::*,
    str::String as AzString,
};
use azul_appkit::{
    l10n::{label, t, t_args, Arg},
    pieces::{block, text},
};
use azul_pay::{cash::amount_in_words, offer::amount_text, CashSlip};

use crate::{
    actions::now_secs, add_drive::DEFAULT_CLOUD_NAME, ids, look, paper::Paper, with_state,
    DriveState, Popup,
};

/// What the dialog and the drive list say while a cash order waits for its letter (a key).
pub(crate) const WAITING_TEXT: &str = "azdrive-cash-waiting";

/// A cash checkout this AzDrive waits for, as the drive list shows it. `Debug` shows no claim
/// secret (the checkout's).
#[derive(Clone, Debug)]
pub(crate) struct Wait {
    pub checkout: PendingCheckout,
    /// Why it ended (the cash rejected, the order expired), once the token server said so.
    pub ended: Option<String>,
}

/// What a cash order's two pages print.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Letter {
    pub checkout_id: String,
    /// `100GB`.
    pub tier: String,
    pub months: u32,
    pub amount_cents: u64,
    pub currency: String,
    pub activation_code: String,
    pub mail_to_name: String,
    pub mail_to_lines: Vec<String>,
    /// RFC 3339 (empty: no end said).
    pub expires_at: String,
}

impl Letter {
    /// The letter of the token server's checked answer, for `tier` paid `months` at once.
    #[must_use]
    pub(crate) fn of_slip(slip: &CashSlip, tier: &str, months: u32) -> Letter {
        Letter {
            checkout_id: slip.checkout_id.clone(),
            tier: tier.trim().to_string(),
            months,
            amount_cents: slip.amount_cents,
            currency: slip.currency.clone(),
            activation_code: slip.activation_code.clone(),
            mail_to_name: slip.mail_to.name.clone(),
            mail_to_lines: slip.mail_to.lines.clone(),
            expires_at: slip.expires_at.clone().unwrap_or_default(),
        }
    }

    /// The letter of a kept cash checkout; `None` for one without its slip (picked up here by
    /// its claim code).
    #[must_use]
    pub(crate) fn of(kept: &PendingCheckout) -> Option<Letter> {
        let cash = kept.cash.as_ref()?;
        Some(Letter {
            checkout_id: kept.checkout_id.clone(),
            tier: kept.tier.clone(),
            months: cash.months,
            amount_cents: cash.amount_cents,
            currency: cash.currency.clone(),
            activation_code: cash.activation_code.clone(),
            mail_to_name: cash.mail_to_name.clone(),
            mail_to_lines: cash.mail_to_lines.clone(),
            expires_at: cash.expires_at.clone(),
        })
    }

    /// What the keyring's list keeps of it.
    #[must_use]
    pub(crate) fn kept(&self) -> CashKept {
        CashKept {
            months: self.months,
            amount_cents: self.amount_cents,
            currency: self.currency.clone(),
            activation_code: self.activation_code.clone(),
            mail_to_name: self.mail_to_name.clone(),
            mail_to_lines: self.mail_to_lines.clone(),
            expires_at: self.expires_at.clone(),
        }
    }

    /// `EUR 9.90`.
    #[must_use]
    pub(crate) fn amount_text(&self) -> String {
        format!("{} {}", self.currency, amount_text(self.amount_cents))
    }

    /// `nine euros and ninety cents`.
    #[must_use]
    pub(crate) fn amount_words(&self) -> String {
        amount_in_words(self.amount_cents, &self.currency)
    }

    /// What was bought: `Azlin storage, 100 GB for 12 months` (in the window's language).
    fn bought(&self) -> String {
        t_args(
            if self.tier.is_empty() {
                "azdrive-cash-bought"
            } else {
                "azdrive-cash-bought-tier"
            },
            &[
                ("tier", Arg::from(tier_text(&self.tier))),
                ("months", Arg::from(self.months)),
            ],
        )
    }
}

/// A tier id as people read it: `100GB` -> `100 GB`.
fn tier_text(tier: &str) -> String {
    let digits = tier.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits == tier.len() {
        return tier.to_string();
    }
    format!("{} {}", &tier[..digits], &tier[digits..])
}

/// The day of an RFC 3339 time (`2026-12-09`), or the text as it is.
fn day_of(time: &str) -> &str {
    match time.get(..10) {
        Some(day) if day.as_bytes().get(4) == Some(&b'-') => day,
        _ => time,
    }
}

/// The buyer's copy: what was bought, the amount, the checkout, how to pick the drive up, the
/// claim code `claim_code` as text and QR code; `made`: the day of the order.
#[must_use]
pub(crate) fn copy_paper(letter: &Letter, claim_code: &str, made: &str) -> Paper {
    let title = t("azdrive-cash-copy-title");
    Paper {
        subtitle: t_args(
            "azdrive-cash-copy-subtitle",
            &[
                ("checkout", Arg::from(letter.checkout_id.as_str())),
                ("made", Arg::from(made)),
            ],
        ),
        address: Vec::new(),
        text: vec![
            t_args(
                "azdrive-cash-copy-bought",
                &[
                    ("bought", Arg::from(letter.bought())),
                    ("amount", Arg::from(letter.amount_text())),
                    ("words", Arg::from(letter.amount_words())),
                ],
            ),
            t("azdrive-cash-copy-keep"),
            t("azdrive-cash-copy-daily"),
            t("azdrive-cash-copy-lost"),
            t("azdrive-cash-copy-key"),
        ],
        label: "azdrive-cash-copy-label",
        secret: Zeroizing::new(claim_code.to_string()),
        qr_label: "azdrive-cash-copy-qr",
        file_name: format!("{title}.pdf"),
        title,
    }
}

/// The slip posted with the cash: the operator's address, the amount in words and digits, the
/// activation code as text and QR code - no secret of the drive; `made`: the day of the order.
#[must_use]
pub(crate) fn slip_paper(letter: &Letter, made: &str) -> Paper {
    let mut address = vec![letter.mail_to_name.clone()];
    address.extend(letter.mail_to_lines.iter().cloned());
    let amount = letter.amount_text();
    let mut text = vec![
        t_args(
            "azdrive-cash-slip-amount",
            &[
                ("amount", Arg::from(amount.as_str())),
                ("words", Arg::from(letter.amount_words())),
            ],
        ),
        t_args("azdrive-cash-slip-envelope", &[("amount", Arg::from(amount.as_str()))]),
        t("azdrive-cash-slip-send"),
    ];
    if !letter.expires_at.is_empty() {
        text.push(t_args(
            "azdrive-cash-slip-expires",
            &[("day", Arg::from(day_of(&letter.expires_at)))],
        ));
    }
    let title = t("azdrive-cash-slip-title");
    Paper {
        subtitle: t_args(
            "azdrive-cash-slip-subtitle",
            &[
                ("checkout", Arg::from(letter.checkout_id.as_str())),
                ("amount", Arg::from(amount.as_str())),
                ("made", Arg::from(made)),
            ],
        ),
        address,
        text,
        label: "azdrive-cash-slip-label",
        secret: Zeroizing::new(letter.activation_code.clone()),
        qr_label: "azdrive-cash-slip-qr",
        file_name: format!("{title}.pdf"),
        title,
    }
}

/// Why a cash order ended, as the dialog and the drive list say it.
#[must_use]
pub(crate) fn ended_text(why: &str) -> String {
    t_args(
        "azdrive-cash-ended",
        &[("why", Arg::from(why.trim().trim_end_matches('.')))],
    )
}

/// The unfinished cash checkout the claim code `code` (as typed or scanned) picks up at
/// `token_url`, its drive to be called `name`.
///
/// # Errors
///
/// A sentence: no token server, or no claim code (a typo).
pub(crate) fn picked_up(code: &str, token_url: &str, name: &str) -> Result<PendingCheckout, String> {
    let token_url = token_url.trim();
    if token_url.is_empty() {
        return Err(t("azdrive-cash-no-token-server"));
    }
    let code = ClaimCode::parse(code).map_err(|e| {
        t_args("azdrive-cash-unreadable-code", &[("detail", Arg::from(e.to_string()))])
    })?;
    let name = if name.trim().is_empty() {
        DEFAULT_CLOUD_NAME
    } else {
        name
    };
    Ok(code.pending(token_url, name))
}

// ==== The waits, as the jobs report them ====

/// The cash checkout `checkout` waits: onto the drive list (or its entry there updated - a slip
/// once known is kept).
pub(crate) fn upsert(s: &mut DriveState, checkout: PendingCheckout) {
    match s
        .cash_waits
        .iter_mut()
        .find(|w| w.checkout.checkout_id == checkout.checkout_id)
    {
        Some(wait) => {
            let slip = wait.checkout.cash.take();
            wait.checkout = checkout;
            if wait.checkout.cash.is_none() {
                wait.checkout.cash = slip;
            }
        }
        None => s.cash_waits.push(Wait {
            checkout,
            ended: None,
        }),
    }
}

/// The dialog's cash checkout is posted: its pages are offered, the order joins the drive list.
pub(crate) fn posted(s: &mut DriveState, slip: &CashSlip) {
    println!("AZDRIVE_CASH_SLIP {}", slip.checkout_id);
    let kept = match s.popup.as_ref() {
        Some(Popup::AddDrive(d)) => d
            .kept
            .clone()
            .filter(|k| k.checkout_id == slip.checkout_id),
        _ => None,
    };
    if let Some(kept) = kept {
        upsert(s, kept);
    }
}

/// A look found the cash checkout `checkout` still awaiting its letter.
pub(crate) fn waiting(s: &mut DriveState, checkout: PendingCheckout) {
    println!("AZDRIVE_CASH_WAITING {}", checkout.checkout_id);
    upsert(s, checkout);
}

/// The cash checkout `checkout_id`'s drive arrived: off the drive list.
pub(crate) fn claimed(s: &mut DriveState, checkout_id: &str) {
    s.cash_waits.retain(|w| w.checkout.checkout_id != checkout_id);
}

/// The token server ended the checkout `checkout_id` (`why`); whether it was a cash order
/// (the drive list says why until it is dismissed).
pub(crate) fn ended(s: &mut DriveState, checkout_id: &str, why: &str) -> bool {
    let Some(wait) = s
        .cash_waits
        .iter_mut()
        .find(|w| w.checkout.checkout_id == checkout_id)
    else {
        return false;
    };
    println!("AZDRIVE_CASH_ENDED {checkout_id}");
    wait.ended = Some(ended_text(why));
    true
}

/// The background claims start now: every cash order is asked in them, so the next daily look
/// is a day from now.
pub(crate) fn looked(s: &mut DriveState) {
    let now = now_secs();
    for wait in &s.cash_waits {
        s.cash_looks.looked_at(&wait.checkout.checkout_id, now);
    }
}

/// The period timer's tick: the background claims start when a cash order's daily look is due
/// (and none run).
pub(crate) fn look_if_due(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.claiming {
        return;
    }
    let open: Vec<String> = s
        .cash_waits
        .iter()
        .filter(|w| w.ended.is_none())
        .map(|w| w.checkout.checkout_id.clone())
        .collect();
    if s
        .cash_looks
        .due(open.iter().map(String::as_str), now_secs())
        .is_empty()
    {
        return;
    }
    crate::add_flow::start_claims(info, app, s);
}

// ==== The pages: save, print ====

/// Which of a cash order's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// The buyer's copy (the claim code).
    Copy,
    /// The slip to post (the activation code).
    Slip,
}

impl Page {
    fn word(self) -> &'static str {
        match self {
            Page::Copy => "copy",
            Page::Slip => "slip",
        }
    }
}

/// The kept cash checkout `checkout_id`: the dialog's, else the drive list's.
fn kept_of(s: &DriveState, checkout_id: &str) -> Option<PendingCheckout> {
    if let Some(Popup::AddDrive(d)) = s.popup.as_ref() {
        if let Some(kept) = d.kept.as_ref().filter(|k| k.checkout_id == checkout_id) {
            return Some(kept.clone());
        }
    }
    s.cash_waits
        .iter()
        .find(|w| w.checkout.checkout_id == checkout_id)
        .map(|w| w.checkout.clone())
}

/// The page `page` of the cash order `checkout_id`, if this computer has its slip.
fn paper_of(s: &DriveState, checkout_id: &str, page: Page) -> Option<Paper> {
    let kept = kept_of(s, checkout_id)?;
    let letter = Letter::of(&kept)?;
    let made = azul_storage::time::iso8601(u64::try_from(kept.started_at).unwrap_or(0));
    let made = day_of(&made).to_string();
    match page {
        Page::Copy => {
            let code = ClaimCode::of(&kept).ok()?;
            Some(copy_paper(&letter, &code.to_text(), &made))
        }
        Page::Slip => Some(slip_paper(&letter, &made)),
    }
}

/// What a page button carries.
struct PaperRef {
    app: RefAny,
    checkout_id: String,
    page: Page,
    print: bool,
}

/// A button saving (or printing) page `page` of the cash order `checkout_id`.
pub(crate) fn paper_button(
    app: &RefAny,
    words: &str,
    checkout_id: &str,
    page: Page,
    print: bool,
    id: AzString,
) -> Dom {
    Button::create(label(words))
        .with_on_click(
            RefAny::new(PaperRef {
                app: app.clone(),
                checkout_id: checkout_id.to_string(),
                page,
                print,
            }),
            on_paper as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(id)
        .with_css("margin-right: 6px; margin-top: 4px;")
}

/// What a page button made: said in the dialog when it shows the order, else in the window.
fn say(s: &mut DriveState, checkout_id: &str, note: String) {
    if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
        if d.kept.as_ref().is_some_and(|k| k.checkout_id == checkout_id) {
            d.notice = note;
            return;
        }
    }
    s.info(note);
}

/// Save as PDF (the system's save dialog, the app's state let go before it) or Print (the PDF
/// in the system's viewer, from a private copy).
extern "C" fn on_paper(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, checkout_id, page, print)) = data.downcast_ref::<PaperRef>().map(|r| {
        (r.app.clone(), r.checkout_id.clone(), r.page, r.print)
    }) else {
        return Update::DoNothing;
    };
    let made = {
        let Some(s) = app.downcast_ref::<DriveState>() else {
            return Update::DoNothing;
        };
        paper_of(&s, &checkout_id, page).map(|paper| {
            (
                paper.file_name.clone(),
                crate::paper::paper_pdf(&mut info, &paper),
            )
        })
    };
    let Some((name, bytes)) = made else {
        return Update::DoNothing;
    };
    let note = match bytes {
        Err(why) => why,
        Ok(bytes) if print => {
            let opened = crate::paper::print_copy(&name, &bytes).and_then(|path| {
                println!("AZDRIVE_CASH_PRINT {} {}", page.word(), bytes.len());
                azul_appkit::files::open_external(&path.to_string_lossy())
            });
            match opened {
                Ok(()) => t("azdrive-cash-print-open"),
                Err(why) => t_args("azdrive-kit-print-failed", &[("why", Arg::from(why))]),
            }
        }
        Ok(bytes) => {
            let len = bytes.len();
            if FileDialog::save_bytes(name.as_str(), "application/pdf", bytes.to_vec()) {
                println!("AZDRIVE_CASH_SAVED {} {len}", page.word());
                t_args("azdrive-kit-saved", &[("name", Arg::from(name.as_str()))])
            } else {
                t("azdrive-kit-not-saved")
            }
        }
    };
    crate::ui_add_drive::everywhere(with_state(&mut app, &mut info, |_info, _app, s| {
        say(s, &checkout_id, note);
    }))
}

// ==== The dialog and the drive list ====

/// The Buy page's part for its posted cash order: the waiting line, the claim code, the two
/// pages to save or print.
#[must_use]
pub(crate) fn posted_pieces(kept: Option<&PendingCheckout>, app: &RefAny) -> Vec<Dom> {
    let mut pieces = vec![Dom::create_span_with_text(label(WAITING_TEXT))
        .with_id(ids::ADD_CASH_WAITING)
        .with_css("margin-top: 8px; font-weight: 600;")];
    let Some(kept) = kept else {
        return pieces;
    };
    if let Some(letter) = Letter::of(kept) {
        pieces.push(
            Dom::create_span_with_text(AzString::from(t_args(
                "azdrive-cash-both-pages",
                &[("amount", Arg::from(letter.amount_text()))],
            )))
            .with_css("margin-top: 6px; font-size: 12px;"),
        );
    }
    if let Ok(code) = ClaimCode::of(kept) {
        pieces.push(
            Dom::create_span_with_text(label("azdrive-cash-claim-code-is"))
            .with_css("margin-top: 8px; font-size: 12px; opacity: 0.75;"),
        );
        pieces.push(
            Dom::create_div()
                .with_id(ids::ADD_CASH_CLAIM_CODE)
                .with_css(
                    "margin-top: 4px; padding: 6px; border: 1px solid #8A8A8A; border-radius: \
                     4px; font-family: monospace; font-size: 12px;",
                )
                .with_child(Dom::create_span_with_text(AzString::from(
                    code.to_text().as_str(),
                ))),
        );
    }
    let id = kept.checkout_id.as_str();
    pieces.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-wrap: wrap; margin-top: 8px;")
            .with_child(paper_button(
                app,
                "azdrive-cash-save-copy",
                id,
                Page::Copy,
                false,
                ids::CASH_COPY_SAVE,
            ))
            .with_child(paper_button(
                app,
                "azdrive-cash-print-copy",
                id,
                Page::Copy,
                true,
                ids::CASH_COPY_PRINT,
            ))
            .with_child(paper_button(
                app,
                "azdrive-cash-save-slip",
                id,
                Page::Slip,
                false,
                ids::CASH_SLIP_SAVE,
            ))
            .with_child(paper_button(
                app,
                "azdrive-cash-print-slip",
                id,
                Page::Slip,
                true,
                ids::CASH_SLIP_PRINT,
            )),
    );
    pieces
}

/// What Dismiss carries.
struct DismissRef {
    app: RefAny,
    checkout_id: String,
}

extern "C" fn on_dismiss(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, checkout_id)) = data
        .downcast_ref::<DismissRef>()
        .map(|r| (r.app.clone(), r.checkout_id.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |_info, _app, s| {
        s.cash_waits.retain(|w| w.checkout.checkout_id != checkout_id);
    })
}

/// A wrapping line of the drive list's cash area.
const LINE: &str = "padding: 1px 10px; white-space: normal;";

/// The drive list's cash orders: each one waiting for its letter (its pages to save again) or
/// why it ended (Dismiss).
#[must_use]
pub(crate) fn waiting_area(s: &DriveState, app: &RefAny) -> Option<Dom> {
    if s.cash_waits.is_empty() {
        return None;
    }
    let mut area = Dom::create_div()
        .with_id(ids::SIDE_CASH)
        .with_css(look::ACTIVITY)
        .with_accessibility_name(label("azdrive-cash-orders"))
        .with_child(block(look::ACTIVITY_HEAD, text(label("azdrive-cash-orders"))));
    for (index, wait) in s.cash_waits.iter().enumerate() {
        let kept = &wait.checkout;
        let letter = Letter::of(kept);
        let name = if kept.name.is_empty() {
            DEFAULT_CLOUD_NAME
        } else {
            kept.name.as_str()
        };
        let head = match &letter {
            Some(letter) => format!("{name} - {}", letter.amount_text()),
            None => name.to_string(),
        };
        area.add_child(block(look::ACTIVITY_LINE, text(head)));
        match &wait.ended {
            Some(why) => {
                area.add_child(
                    block(look::ACTIVITY_ERROR, text(why.clone()))
                        .with_id(ids::side_cash(index, "line"))
                        .with_css("white-space: normal;"),
                );
                area.add_child(
                    Dom::create_div().with_css("padding: 2px 10px;").with_child(
                        Button::create(label("azdrive-message-dismiss"))
                            .with_on_click(
                                RefAny::new(DismissRef {
                                    app: app.clone(),
                                    checkout_id: kept.checkout_id.clone(),
                                }),
                                on_dismiss as ButtonOnClickCallbackType,
                            )
                            .dom()
                            .with_id(ids::side_cash(index, "dismiss")),
                    ),
                );
            }
            None => {
                area.add_child(
                    block(LINE, text(label(WAITING_TEXT))).with_id(ids::side_cash(index, "line")),
                );
                if letter.is_some() {
                    area.add_child(
                        Dom::create_div()
                            .with_css("display: flex; flex-direction: row; padding: 2px 10px;")
                            .with_child(paper_button(
                                app,
                                "azdrive-cash-your-copy",
                                &kept.checkout_id,
                                Page::Copy,
                                false,
                                ids::side_cash(index, "copy"),
                            ))
                            .with_child(paper_button(
                                app,
                                "azdrive-cash-the-slip",
                                &kept.checkout_id,
                                Page::Slip,
                                false,
                                ids::side_cash(index, "slip"),
                            )),
                    );
                }
            }
        }
    }
    Some(area)
}
