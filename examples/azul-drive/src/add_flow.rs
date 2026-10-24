//! What the Add drive dialog's buttons start, and what their jobs answer: a source's connection
//! test and its saving (the drives file without secrets, the keyring with them), Buy storage's
//! tier list, a test drive of a development token server, a checkout paid in the browser and
//! the wait for its drive - and the claims of the checkouts no dialog waits for any more: after
//! "Stop waiting" in the background, at every start (azcloud-kit's `pending`: the checkouts and
//! their claim keys outlive AzDrive in the keyring). The dialog's data is `add_drive`, its view
//! `ui_add_drive`.
//!
//! Buy storage pays through the token server's payment options when it has them: a pill per
//! method, the consent, and azul-pay's checkout machine ([`pay`]) - its effects run here: the
//! checkout (with the claim key, onto the keyring's list), the popover with the provider's page
//! in its web view, `prevent_default` for every navigation the policy cancels, the fields
//! page's commands, the system browser, the next surface of the same checkout, the abandon of
//! a checkout nobody pays, the wait for the drive. Then the claim flow takes over unchanged.
//!
//! A consumer cloud's form (Google Drive, Dropbox, OneDrive) has "Sign in": azul's
//! `AuthSession` opens the provider's page in the system browser (or the platform's sign-in
//! sheet) with a PKCE pair and a state of its own (`AuthPkce`), the redirect comes back to
//! [`on_sign_in_redirect`], its code is exchanged for tokens on a worker thread
//! (`Job::OAuthExchange`), and the refresh token lands in the form's secrets (`sign_in`).
//!
//! On stdout, for scripts: `AZDRIVE_ADD_PAGE <page>`, `AZDRIVE_TESTED ok|error`,
//! `AZDRIVE_SIGN_IN <scheme> <step>` (`waiting`, `exchanging`, `cancelled`, `timed-out`,
//! `unsupported`, `failed`, `no-client`), `AZDRIVE_SIGNED_IN <scheme> ok|error`,
//! `AZDRIVE_COUNTRY <code>` (the buyer's country declared), `AZDRIVE_TIERS <n>`,
//! `AZDRIVE_PILLS <method>:<provider> ...` (`-` for none: the v1 checkout; in the server's
//! order), `AZDRIVE_PAY_REFUSED` (the token server refused this buyer),
//! `AZDRIVE_CHECKOUT <checkout id>`, `AZDRIVE_PAY <state>`, `AZDRIVE_PAY_SURFACE <kind> <host>`,
//! `AZDRIVE_PAY_BLOCKED <host>`, `AZDRIVE_OPEN_BROWSER <host>`, `AZDRIVE_ABANDONED <checkout id>
//! ok|error`, `AZDRIVE_CLAIMED <checkout id> <drive id>`, `AZDRIVE_ADDED <drive id>`,
//! `AZDRIVE_PERIOD_TOKENS <checkout id> <drive id> <count>`, `AZDRIVE_PERIOD_REDEEMED <drive id>
//! <count> <until>`. No secret,
//! no cardholder name and no payment page address is printed - hosts only.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use azcloud_kit::{
    pending::{Abandoned, Claimed, Finished},
    PendingCheckout, Tiers,
};
use azul::{
    css::DarkLightMode,
    prelude::*,
    str::String as AzString,
    url::{
        AuthCodeStatus, AuthPkce, AuthRequest, AuthSession, AuthSessionResult,
        AuthSessionStatus, Url,
    },
};
use azul_appkit::l10n::{self, t, t_args, Arg, Phrase, Text};
use azul_pay::{
    machine::Notice,
    offer::{Offer, OfferContext},
    AbandonOutcome, Effect, Event, Method, SecretUrl, State as PayState,
};
use azul_storage::{
    config::{self, DriveAuth, DriveEntry, DriveLocation, DrivesFile},
    oauth::Tokens,
    time::iso8601,
    DriveError,
};

use crate::{
    add_drive::{AddDialog, AddPage, BuyStep, OfferState, TiersState},
    countries,
    browse::Place,
    go, ids,
    jobs::{BoughtDrive, Job, OptionsFailed, PayVia, Started, TiersFailed},
    keyring, refresh_disks,
    sign_in::{self, PendingSignIn},
    spawn, with_state, DriveState, KeyringCall, KeyringOp, Popup, Slot,
};

/// What a button or a tile of the dialog asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AddEvent {
    ChooseBuy,
    ChooseConnect,
    Back,
    /// A source of the Sources page.
    Service(&'static str),
    /// A tier of Buy storage.
    Tier(usize),
    /// Buy storage's "Pay yearly".
    Yearly,
    /// "Try again" after the tier list failed.
    RetryTiers,
    /// "Tell me when it is back" for the closed tier of the list at this index.
    WaitForTier(usize),
    Test,
    /// A consumer cloud's "Sign in".
    SignIn,
    Save,
    Cancel,
    CreateTestDrive,
    Buy,
    StopWaiting,
    /// A payment pill (its method).
    Pill(Method),
    /// The pill's other provider (an index of the offer's providers).
    PillProvider(usize),
    /// "Change" beside Buy storage's country: the country picker again.
    ChangeCountry,
    /// A country of the picker's list.
    PickCountry(&'static str),
    /// The picker's Continue: the picked country is declared (and remembered).
    ConfirmCountry,
    /// The order's consent.
    Consent,
    /// The popover's Pay.
    PayConfirm,
    /// "Open in browser instead" (the popover), "Open the page again" (waiting for the browser).
    OpenInBrowser,
    /// The popover's close button.
    ClosePopover,
    /// "Check again" after the dialog stopped waiting.
    CheckAgain,
    /// Buy storage's "I have a voucher".
    VoucherPage,
    /// The voucher page's Redeem.
    RedeemVoucher,
    /// "Pick up a paid drive with a claim code" (the first page).
    ClaimCodePage,
    /// The claim code page's Pick up.
    PickUp,
    /// "Pay with cash by post" (the token server's tier list names cash, no pill offers it).
    PayCash,
}

/// The open dialog, if the popup is it.
fn dialog(s: &mut DriveState) -> Option<&mut AddDialog> {
    match s.popup.as_mut() {
        Some(Popup::AddDrive(dialog)) => Some(dialog),
        _ => None,
    }
}

/// The open dialog, to read.
fn dialog_ref(s: &DriveState) -> Option<&AddDialog> {
    match s.popup.as_ref() {
        Some(Popup::AddDrive(dialog)) => Some(dialog),
        _ => None,
    }
}

/// The open dialog with the serial `serial` (a job's answer for an older one finds none).
fn dialog_of(s: &mut DriveState, serial: u64) -> Option<&mut AddDialog> {
    dialog(s).filter(|d| d.serial == serial)
}

fn print_page(d: &AddDialog) {
    println!("AZDRIVE_ADD_PAGE {}", d.page_line());
}

/// Opens the dialog on its two choices, with the buyer's declared country (`AZLIN_COUNTRY`, else
/// the one remembered on this computer) or - for Buy storage's first question - the guess from
/// the window's locale, then the system's.
pub(crate) fn open(s: &mut DriveState) {
    s.popups_opened += 1;
    let mut d = AddDialog::new(s.popups_opened);
    let env = |name: &str| std::env::var(name).ok();
    d.country = countries::declared(s.settings.residence_country.as_deref(), env)
        .unwrap_or_default();
    let first = if d.country.is_empty() {
        countries::guess(&l10n::locale(), env)
    } else {
        Some(d.country.clone())
    };
    d.country_pick = first.as_deref().and_then(countries::listed);
    // A checkout whose payment may be under way: a new order asks about it first.
    if let Some(checkout) = s.in_flight.first() {
        d.pay = PayState::InFlight {
            checkout_id: checkout.checkout_id.clone(),
        };
        d.kept = Some(checkout.clone());
    }
    print_page(&d);
    s.popup = Some(Popup::AddDrive(d));
}

/// Opens Buy storage for the next months of the drive `renew` (Renew): on its tier, any other
/// tier a change of its tier; the payer's country as a new order has it.
pub(crate) fn open_renewing(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    renew: crate::renewals::RenewFor,
) {
    open(s);
    let Some(Popup::AddDrive(d)) = s.popup.take() else {
        return;
    };
    let mut renewing = AddDialog::renewing(d.serial, renew);
    renewing.country = d.country;
    renewing.country_pick = d.country_pick;
    renewing.pay = d.pay;
    renewing.kept = d.kept;
    renewing.choose_buy();
    print_page(&renewing);
    let on_buy = renewing.page == AddPage::Buy;
    s.popup = Some(Popup::AddDrive(renewing));
    if on_buy {
        load_tiers(info, app, s, false);
    }
}

/// What a Renew dialog's checkout carries to the job: its order and the drive's vaults, where
/// the job reads the drive's price lock.
fn renew_job(s: &DriveState, d: &AddDialog) -> Option<Box<crate::jobs::RenewJob>> {
    let order = d.renew_order()?;
    let azlin = crate::periods::azlin_drives(s).get(&order.drive_id).cloned();
    Some(Box::new(crate::jobs::RenewJob {
        order,
        store: s.period_tokens.clone(),
        azlin,
    }))
}

/// Opens the form of drive `index` again (its keyring entry is gone): its settings filled in,
/// its secrets to type anew.
pub(crate) fn open_editing(s: &mut DriveState, index: usize) {
    let Some(entry) = s.slots.get(index).map(|slot| slot.entry.clone()) else {
        return;
    };
    s.popups_opened += 1;
    let d = AddDialog::editing(&entry, s.popups_opened);
    print_page(&d);
    s.popup = Some(Popup::AddDrive(d));
}

/// A button or a tile of the dialog.
pub(crate) fn event(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, event: AddEvent) {
    let Some(d) = dialog(s) else {
        return;
    };
    let page_before = d.page_line();
    match event {
        AddEvent::ChooseBuy => {
            d.choose_buy();
            if d.page == AddPage::Buy {
                load_tiers(info, app, s, false);
            }
        }
        AddEvent::ChooseConnect => d.choose_connect(),
        AddEvent::Back => {
            // A payment shown is abandoned, one on its way waited for in the background.
            let _ = pay(info, app, s, Event::Close);
            if let Some(d) = dialog(s) {
                if let BuyStep::Paying { cancel, .. } = &d.step {
                    cancel.store(true, Ordering::SeqCst);
                }
                d.step = BuyStep::Idle;
                // A checkout in flight stays so: the next order asks about it first.
                if !matches!(d.pay, PayState::InFlight { .. } | PayState::Releasing { .. }) {
                    d.pay = PayState::Choosing;
                }
                d.back();
            }
        }
        AddEvent::Service(id) => {
            if !d.open_service(id) {
                d.error = t("azdrive-add-cannot-open-source");
            }
        }
        AddEvent::Tier(index) => {
            // The order is fixed while a payment runs; another tier has its own price and
            // methods, so the options are asked for again.
            if !d.busy() && d.tier != index {
                d.tier = index;
                d.notice.clear();
                load_options(info, app, s);
            }
        }
        AddEvent::Yearly => {
            if !d.busy() {
                d.yearly = !d.yearly;
                d.notice.clear();
                load_options(info, app, s);
            }
        }
        AddEvent::RetryTiers => load_tiers(info, app, s, true),
        AddEvent::WaitForTier(index) => {
            if let Some(tier) = d.closed.get(index).cloned() {
                crate::renewals::wait_for_tier(s, &tier);
            }
        }
        AddEvent::Test => test(info, app, s),
        AddEvent::SignIn => start_sign_in(info, app, s),
        AddEvent::Save => save(info, app, s),
        AddEvent::Cancel => close(info, app, s),
        AddEvent::CreateTestDrive => create_test_drive(info, app, s),
        AddEvent::Buy => buy(info, app, s),
        AddEvent::StopWaiting => {
            if d.pay.busy() {
                let _ = pay(info, app, s, Event::StopWaiting);
            } else {
                stop_waiting(info, app, s);
            }
        }
        AddEvent::Pill(method) => {
            if !d.busy() {
                d.choose_pill(method);
            }
        }
        AddEvent::PillProvider(index) => {
            if !d.busy() {
                d.choose_provider(index);
            }
        }
        AddEvent::ChangeCountry => {
            if !d.busy() {
                d.change_country();
            }
        }
        AddEvent::PickCountry(code) => {
            d.pick_country(code);
        }
        AddEvent::ConfirmCountry => confirm_country(info, app, s),
        AddEvent::Consent => {
            d.consent = !d.consent;
            if d.consent && matches!(d.pay, PayState::Choosing) {
                d.notice.clear();
            }
        }
        AddEvent::PayConfirm => {
            let name = d.card_name.clone();
            let _ = pay(info, app, s, Event::Confirm { name });
        }
        AddEvent::OpenInBrowser => {
            let _ = pay(info, app, s, Event::OpenInBrowser);
        }
        AddEvent::ClosePopover => {
            let _ = pay(info, app, s, Event::Close);
        }
        AddEvent::CheckAgain => {
            let _ = pay(info, app, s, Event::CheckAgain);
        }
        AddEvent::VoucherPage => {
            if !d.busy() {
                d.choose_voucher();
            }
        }
        AddEvent::RedeemVoucher => redeem_voucher(info, app, s),
        AddEvent::ClaimCodePage => {
            if !d.busy() {
                d.choose_claim_code();
            }
        }
        AddEvent::PickUp => pick_up(info, app, s),
        AddEvent::PayCash => pay_cash(info, app, s),
    }
    if let Some(d) = dialog(s) {
        if d.page_line() != page_before {
            print_page(d);
        }
    }
}

/// The country picker's Continue: the picked country is the buyer's declared one - remembered on
/// this computer (the settings file) - and Buy storage shows its tiers, priced in its currency.
fn confirm_country(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(d) = dialog(s) else {
        return;
    };
    if !d.confirm_country() {
        return;
    }
    let code = d.country.clone();
    println!("AZDRIVE_COUNTRY {code}");
    if s.settings.residence_country.as_deref() != Some(code.as_str()) {
        s.settings.residence_country = Some(code);
        crate::save_settings(info, app, s);
    }
    load_tiers(info, app, s, false);
}

/// Closes the dialog; a payment waited for is waited for no more.
pub(crate) fn cancel(s: &mut DriveState) {
    if let Some(d) = dialog(s) {
        if let BuyStep::Paying { cancel, .. } = &d.step {
            cancel.store(true, Ordering::SeqCst);
        }
    }
    s.popup = None;
}

/// The user closes the dialog (Cancel, its close button, Escape): a payment shown and not
/// confirmed is abandoned (nobody may pay it later), one on its way is waited for in the
/// background (the claim stays) - then it closes.
pub(crate) fn close(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let _ = pay(info, app, s, Event::Close);
    cancel(s);
}

// ==== Connect data source ====

/// "Test connection": the form's drive opened with its secrets, ONE listing of its root.
fn test(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(d) = dialog(s) else {
        return;
    };
    if d.testing {
        return;
    }
    match d.build("test") {
        Ok(new) => {
            d.testing = true;
            d.tested = None;
            d.error.clear();
            let serial = d.serial;
            spawn(
                info,
                app,
                s,
                Job::Test {
                    serial,
                    entry: new.entry,
                    secret: new.secret,
                },
            );
        }
        Err(problem) => d.error = problem,
    }
}

/// What a sign-in's answer finds: the app and the dialog that asked.
struct SignInRef {
    app: RefAny,
    serial: u64,
}

fn print_sign_in(scheme: &str, step: &str) {
    println!("AZDRIVE_SIGN_IN {scheme} {step}");
}

/// "Sign in" of a consumer cloud's form: the provider's OAuth client from the settings (a
/// missing one is said), a fresh PKCE pair and state, the authorization request, azul's
/// sign-in session. Its answer comes back to [`on_sign_in_redirect`]; nothing is sent from
/// here but the browser's visit to the provider's page.
fn start_sign_in(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let settings = s.sign_in_settings.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    let Some(provider) = d.sign_in_provider() else {
        return;
    };
    if d.signing_in() {
        return;
    }
    let plan = match sign_in::plan(provider.scheme, &settings) {
        Ok(plan) => plan,
        Err(why) => {
            d.sign_in_failed(&why);
            print_sign_in(provider.scheme, "no-client");
            return;
        }
    };
    let pkce = AuthPkce::create();
    let authorize_url = format!(
        "{}{}",
        pkce.authorize_url(
            plan.authorize_endpoint.as_str(),
            plan.client.client_id.as_str(),
            plan.scope.as_str(),
        )
        .as_str(),
        plan.extras_query()
    );
    let request = AuthRequest::create(authorize_url.as_str(), plan.redirect_uri.as_str());
    d.sign_in_waiting(PendingSignIn {
        plan,
        code_verifier: pkce.code_verifier.as_str().to_string(),
        code_challenge: pkce.code_challenge.as_str().to_string(),
        state: pkce.state.as_str().to_string(),
    });
    let serial = d.serial;
    print_sign_in(provider.scheme, "waiting");
    let _request = AuthSession::start(
        *info,
        request,
        RefAny::new(SignInRef {
            app: app.clone(),
            serial,
        }),
        on_sign_in_redirect,
    );
}

/// The sign-in session's answer (azul's `ResumeCallback`): the redirect read with the sign-in's
/// state, its code exchanged on a worker thread - or why there is none.
extern "C" fn on_sign_in_redirect(
    mut data: RefAny,
    mut info: CallbackInfo,
    result: RefAny,
) -> Update {
    let Some((mut app, serial)) = data
        .downcast_ref::<SignInRef>()
        .map(|r| (r.app.clone(), r.serial))
    else {
        return Update::DoNothing;
    };
    let Some(result) = AuthSessionResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    crate::ui_add_drive::everywhere(with_state(&mut app, &mut info, |info, app, s| {
        sign_in_returned(info, app, s, serial, &result);
    }))
}

/// What the sign-in session answered, for the dialog `serial`.
fn sign_in_returned(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    result: &AuthSessionResult,
) {
    let Some(d) = dialog_of(s, serial) else {
        return;
    };
    let Some(pending) = d.pending_sign_in.clone() else {
        return;
    };
    let scheme = pending.plan.provider.scheme;
    let ended = match result.status {
        AuthSessionStatus::Redirected => None,
        AuthSessionStatus::Cancelled => Some(("cancelled", t("azdrive-add-sign-in-cancelled"))),
        AuthSessionStatus::TimedOut => Some(("timed-out", t("azdrive-add-sign-in-timed-out"))),
        AuthSessionStatus::Unsupported => Some((
            "unsupported",
            t_args(
                "azdrive-add-sign-in-unsupported",
                &[("why", Arg::from(result.message.as_str()))],
            ),
        )),
        AuthSessionStatus::Failed => Some((
            "failed",
            t_args(
                "azdrive-add-sign-in-unfinished",
                &[("why", Arg::from(result.message.as_str()))],
            ),
        )),
    };
    if let Some((word, why)) = ended {
        d.sign_in_failed(&why);
        print_sign_in(scheme, word);
        return;
    }
    let pkce = AuthPkce {
        code_verifier: AzString::from(pending.code_verifier.as_str()),
        code_challenge: AzString::from(pending.code_challenge.as_str()),
        state: AzString::from(pending.state.as_str()),
    };
    let code = pkce.read_redirect(result.redirect_url.clone());
    if !matches!(code.status, AuthCodeStatus::Code) {
        d.sign_in_failed(&t_args(
            "azdrive-add-sign-in-unfinished",
            &[("why", Arg::from(code.message.as_str()))],
        ));
        print_sign_in(scheme, "failed");
        return;
    }
    d.sign_in_exchanging();
    print_sign_in(scheme, "exchanging");
    spawn(
        info,
        app,
        s,
        Job::OAuthExchange {
            serial,
            plan: Box::new(pending.plan),
            code: code.code.as_str().to_string(),
            code_verifier: pending.code_verifier,
            redirect_uri: result.redirect_uri.as_str().to_string(),
        },
    );
}

/// The token endpoint's answer for the dialog `serial`: the refresh token into the form - or
/// why not.
pub(crate) fn signed_in(s: &mut DriveState, serial: u64, result: Result<Tokens, String>) {
    let Some(d) = dialog_of(s, serial) else {
        return;
    };
    let scheme = d
        .pending_sign_in
        .as_ref()
        .map_or("-", |p| p.plan.provider.scheme);
    let done = match result {
        Ok(tokens) => d.signed_in(&tokens),
        Err(why) => {
            let why = t_args(
                "azdrive-add-sign-in-token-failed",
                &[("why", Arg::from(l10n::t_label(&why)))],
            );
            d.sign_in_failed(&why);
            Err(why)
        }
    };
    println!(
        "AZDRIVE_SIGNED_IN {scheme} {}",
        if done.is_ok() { "ok" } else { "error" }
    );
}

/// A connection test's answer.
pub(crate) fn tested(s: &mut DriveState, serial: u64, result: Result<String, String>) {
    println!(
        "AZDRIVE_TESTED {}",
        if result.is_ok() { "ok" } else { "error" }
    );
    if let Some(d) = dialog_of(s, serial) {
        d.testing = false;
        d.tested = Some(result);
    }
}

/// "Add drive": the entry into the drives file (no secrets), the secrets into the keyring, the
/// drive in the source list - and the window goes there.
fn save(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let file_path = s.drives_file.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    // What is missing first, in the form's order (building checks again).
    if let Err(problem) = d.check() {
        d.error = problem;
        return;
    }
    let id = d
        .editing
        .clone()
        .unwrap_or_else(|| config::new_drive_id(d.name.trim()));
    let new = match d.build(&id) {
        Ok(new) => new,
        Err(problem) => {
            d.error = problem;
            return;
        }
    };
    if let Err(problem) = save_entry(file_path.as_deref(), &new.entry) {
        d.error = problem;
        return;
    }
    add_slot(info, app, s, new.entry, new.secret, true, false);
}

/// Writes `entry` into the drives file at `path` (replacing the drive with its id).
fn save_entry(path: Option<&std::path::Path>, entry: &DriveEntry) -> Result<(), String> {
    let Some(path) = path else {
        return Err(t("azdrive-add-no-config-folder"));
    };
    DrivesFile::load(path)
        .and_then(|mut file| {
            file.add(entry.clone());
            file.save(path)
        })
        .map_err(|e: DriveError| {
            t_args(
                "azdrive-drive-not-saved",
                &[("detail", Arg::from(e.to_string()))],
            )
        })
}

/// A new (or re-keyed) drive joins the source list with its secret (into the keyring too,
/// unless `in_keyring`: an Azlin drive's session a worker thread stored under the drive's
/// lock); with `from_dialog` (the open dialog made it) the dialog closes and the window opens
/// the drive - a drive whose payment arrived after its dialog closed just joins the list.
pub(crate) fn add_slot(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    entry: DriveEntry,
    secret: Option<String>,
    from_dialog: bool,
    in_keyring: bool,
) {
    let id = entry.id.clone();
    let local = matches!(entry.location, azul_storage::config::DriveLocation::Local { .. });
    let index = match s.slot_index(&id) {
        Some(index) => {
            s.slots[index] = Slot::new(entry);
            index
        }
        None => {
            s.slots.push(Slot::new(entry));
            s.slots.len() - 1
        }
    };
    s.slots[index].secret = secret.clone().map(azcloud_kit::Zeroizing::new);
    if from_dialog {
        cancel(s);
        s.selected_drive = Some(index);
    }
    if local {
        s.tree.locations_open = true;
        refresh_disks(s);
    } else {
        s.tree.cloud_open = true;
    }
    println!("AZDRIVE_ADDED {id}");
    let name = s.slots[index].entry.name.clone();
    let stored_already = in_keyring && secret.is_some();
    match secret {
        Some(secret) if !in_keyring => keyring(
            info,
            s,
            KeyringOp::Store {
                drive_id: id.clone(),
            },
            KeyringCall::Store(config::keyring_key(&id), secret),
        ),
        Some(_) => {}
        None => s.success(Phrase::new("azdrive-is-a-drive").arg("name", name.as_str())),
    }
    if from_dialog {
        go(info, app, s, Place::folder(&id, ""), true);
        if stored_already {
            s.success(Phrase::new("azdrive-add-drive-with-session").arg("name", name.as_str()));
        }
    } else {
        s.info(Phrase::new("azdrive-add-drive-ready").arg("name", name.as_str()));
    }
}

// ==== Buy storage ====

/// Asks the token server for its tiers (once; `again` after a failure).
fn load_tiers(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, again: bool) {
    let token_url = s.token.url.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    let wanted = match &d.tiers {
        TiersState::NotLoaded => true,
        TiersState::Failed(_) => again,
        // Refused for this buyer's country: only another country asks again.
        TiersState::Loading | TiersState::Loaded(_) | TiersState::Refused(_) => false,
    };
    if !wanted {
        return;
    }
    let Some(token_url) = token_url else {
        d.tiers = TiersState::Failed(no_token_server());
        return;
    };
    d.tiers = TiersState::Loading;
    let serial = d.serial;
    let country = d.country.clone();
    spawn(
        info,
        app,
        s,
        Job::Tiers {
            serial,
            token_url,
            country,
        },
    );
}

/// Why Buy storage cannot ask anyone.
fn no_token_server() -> String {
    t("azdrive-add-no-token-server")
}

/// The tier list's answer; then the payment options are asked for (the page is ready -
/// `AZDRIVE_TIERS` - once they have answered too).
pub(crate) fn tiers_answered(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    country: &str,
    result: Result<Tiers, TiersFailed>,
    closed: Vec<String>,
) {
    // A list for the country declared before is not this one's (its prices are in another
    // currency): the list asked for since answers.
    let Some(d) = dialog_of(s, serial).filter(|d| d.tiers_current(country)) else {
        return;
    };
    match result {
        Ok(tiers) => {
            d.tiers_loaded(tiers);
            d.closed = closed;
            load_options(info, app, s);
        }
        Err(TiersFailed::Refused(said)) => {
            println!("AZDRIVE_TIERS_REFUSED");
            d.tiers = TiersState::Refused(l10n::t_text(&said));
        }
        Err(TiersFailed::Failed(why)) => d.tiers = TiersState::Failed(why),
    }
}

/// Asks the token server for its payment options for the chosen tier and period and the
/// payer's country (`GET /v1/checkout/options`).
fn load_options(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let token_url = s.token.url.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    let Some(token_url) = token_url else {
        d.offer = OfferState::Failed(no_token_server());
        tiers_ready(d);
        return;
    };
    let (tier, currency) = d
        .chosen_tier()
        .map(|t| (t.id.clone(), t.currency.clone()))
        .unwrap_or_else(|| (String::new(), String::from("EUR")));
    d.offer = OfferState::Loading;
    let job = Job::Options {
        serial: d.serial,
        asked: d.options_key(),
        token_url,
        tier,
        months: d.months(),
        country: d.country.clone(),
        currency,
    };
    spawn(info, app, s, job);
}

/// The payment options' answer: the offer, narrowed by azul-pay's registry (what it dropped
/// goes to the log), the token server's refusal of this buyer (its own sentence, shown as it
/// is: a country it does not serve), or the v1 checkout (an older token server, a failure).
pub(crate) fn options_answered(
    s: &mut DriveState,
    serial: u64,
    asked: &str,
    result: Result<Option<String>, OptionsFailed>,
) {
    let token_url = s.token.url.clone().unwrap_or_default();
    // An answer for the order as it was before (another tier, period or country) is dropped:
    // the one asked for since answers.
    let Some(d) = dialog_of(s, serial).filter(|d| d.options_current(asked)) else {
        return;
    };
    match result {
        Ok(Some(text)) => match Offer::parse(&text, &OfferContext::for_token_url(&token_url)) {
            Ok(offer) => {
                for why in &offer.dropped {
                    eprintln!("[azdrive] a payment option was not taken: {why}");
                }
                d.offer_loaded(offer);
            }
            Err(why) => d.offer = OfferState::Failed(why.to_string()),
        },
        Ok(None) => d.offer = OfferState::Legacy,
        Err(OptionsFailed::Refused(said)) => {
            println!("AZDRIVE_PAY_REFUSED");
            d.offer = OfferState::Refused(l10n::t_text(&said));
        }
        Err(OptionsFailed::Failed(why)) => d.offer = OfferState::Failed(why),
    }
    tiers_ready(d);
}

/// The page is ready: the tiers and the payment options are in.
fn tiers_ready(d: &AddDialog) {
    if let TiersState::Loaded(tiers) = &d.tiers {
        println!("AZDRIVE_TIERS {}", tiers.tiers.len());
    }
    print_pills(d);
}

/// For scripts: the pills shown now (`-`: none, the v1 checkout).
fn print_pills(d: &AddDialog) {
    let line = d.pills_line();
    println!(
        "AZDRIVE_PILLS {}",
        if line.is_empty() { "-" } else { line.as_str() }
    );
}

/// The chosen tier's id, the bought drive's name and the token server - or why Buy storage
/// cannot go on (said in the dialog).
fn buy_parts(s: &mut DriveState) -> Option<(String, String, String, u64)> {
    let token_url = s.token.url.clone();
    let d = dialog(s)?;
    if d.busy() {
        return None;
    }
    // The token server refused this buyer (a country it does not serve): its refusal stands.
    if let Some(sentence) = d.refused().map(String::from) {
        d.notice = sentence;
        return None;
    }
    let Some(token_url) = token_url else {
        d.notice = no_token_server();
        return None;
    };
    let Some(tier) = d.chosen_tier().map(|t| t.id.clone()) else {
        d.notice = t("azdrive-add-choose-tier");
        return None;
    };
    if d.buy_name.trim().is_empty() {
        d.notice = t("azdrive-add-give-name");
        return None;
    }
    Some((tier, d.buy_name.trim().to_string(), token_url, d.serial))
}

/// "Create test drive": a drive without payment (a development token server).
/// "I have a voucher"'s Redeem: the voucher's new drive (its tier, else the one chosen), its
/// session into the keyring like a test drive's; the answer is a bought drive's.
fn redeem_voucher(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let token_url = s.token.url.clone();
    let keyring = s.keyring.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    if d.busy() {
        return;
    }
    let Some(token_url) = token_url else {
        d.notice = no_token_server();
        return;
    };
    let code = d.voucher_code.trim().to_string();
    if code.is_empty() {
        d.notice = t("azdrive-voucher-type-code");
        return;
    }
    let tier = d.chosen_tier().map(|t| t.id.clone()).unwrap_or_default();
    let serial = d.serial;
    d.step = BuyStep::Creating;
    d.notice = t("azdrive-voucher-redeeming");
    spawn(
        info,
        app,
        s,
        Job::RedeemVoucher {
            serial,
            token_url,
            code,
            tier,
            drive: None,
            keyring,
        },
    );
}

fn create_test_drive(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some((tier, name, token_url, serial)) = buy_parts(s) else {
        return;
    };
    if let Some(d) = dialog(s) {
        d.step = BuyStep::Creating;
        d.notice = t("azdrive-add-creating-test-drive");
    }
    let keyring = s.keyring.clone();
    spawn(
        info,
        app,
        s,
        Job::CreateTestDrive {
            serial,
            token_url,
            name,
            tier,
            keyring,
        },
    );
}

/// "Buy": with the token server's payment options, the chosen pill's checkout through azul-pay's
/// machine (the consent first); without them the v1 checkout, its payment page in the browser.
fn buy(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if dialog(s).is_some_and(|d| d.refused().is_some()) {
        let _ = buy_parts(s);
        return;
    }
    if !dialog(s).is_some_and(|d| d.pays_with_pills()) {
        buy_on_the_payment_page(info, app, s);
        return;
    }
    if buy_parts(s).is_none() {
        return;
    }
    let look = look_name(info);
    let Some(d) = dialog(s) else {
        return;
    };
    d.look_name = look;
    let Some(choice) = d.choice() else {
        d.notice = t("azdrive-add-choose-payment");
        return;
    };
    let consent = d.consent;
    d.cash_order = false;
    // What the payer typed for a method that asks the app; the machine checks it and the
    // checkout takes it (the dialog drops it then).
    let payer = d.payer.clone();
    let _ = pay(
        info,
        app,
        s,
        Event::Pay {
            choice,
            consent,
            payer,
        },
    );
}

/// "Pay with cash by post": the cash checkout through azul-pay's machine, made of the registry's
/// cash provider (the token server says it takes cash in its tier list). Without payment options
/// the order is the button itself, as Buy on the v1 page is; with them the box is ticked first.
fn pay_cash(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if buy_parts(s).is_none() {
        return;
    }
    let Some(d) = dialog(s) else {
        return;
    };
    let Some(choice) = d.cash_choice() else {
        d.notice = t("azdrive-add-no-cash");
        return;
    };
    if d.offer().is_none() {
        // No payment options: the consent is the page's note, the button the order (the
        // checkout says so to the token server).
        d.consent = true;
    }
    let consent = d.consent;
    d.cash_order = true;
    // Cash by post asks nothing about the payer.
    let payer = azul_pay::Payer::default();
    let _ = pay(
        info,
        app,
        s,
        Event::Pay {
            choice,
            consent,
            payer,
        },
    );
}

/// The look the provider's fields should take: the app theme and the mode (`flora-dark`).
fn look_name(info: &CallbackInfo) -> String {
    let theme: String = info
        .get_theme()
        .as_str()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let mode = match info.get_mode().into_option() {
        Some(DarkLightMode::Dark) => "dark",
        _ => "light",
    };
    let theme = if theme.is_empty() { "flat" } else { theme.as_str() };
    format!("{theme}-{mode}")
}

/// The v1 checkout: on the keyring's list of unfinished checkouts with its claim key; its
/// payment page opens in the browser when it is made.
fn buy_on_the_payment_page(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some((tier, name, token_url, serial)) = buy_parts(s) else {
        return;
    };
    let (months, method) = match dialog(s) {
        Some(d) => {
            d.step = BuyStep::StartingCheckout;
            d.notice = t("azdrive-add-preparing-payment");
            let method = d.chosen_tier().map_or(azcloud_kit::token::DEFAULT_METHOD, |tier| {
                tier.v1_method()
            });
            (d.months(), method.to_string())
        }
        None => return,
    };
    let keyring = s.keyring.clone();
    let renew = dialog_ref(s).and_then(|d| renew_job(s, d));
    spawn(
        info,
        app,
        s,
        Job::Checkout {
            serial,
            token_url,
            tier,
            months,
            method,
            name,
            keyring,
            via: None,
            renew,
        },
    );
}

// ==== The checkout machine (azul-pay) ====

/// Feeds `event` to the dialog's checkout machine and runs its effects; whether the web view's
/// navigation `event` asked about is to be cancelled (`prevent_default` - also when no dialog
/// is there to ask).
pub(crate) fn pay(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    event: Event,
) -> bool {
    let navigation = matches!(event, Event::Navigation { .. });
    let Some(d) = dialog(s) else {
        return navigation;
    };
    let before = d.pay.name();
    let state = std::mem::replace(&mut d.pay, PayState::Choosing);
    let (next, effects) = azul_pay::step(state, event);
    if next.name() != before {
        println!("AZDRIVE_PAY {}", next.name());
    }
    // The dialog's checkout went in flight: the next opening of the dialog starts there too.
    let went_in_flight = match (&next, &d.kept) {
        (PayState::InFlight { checkout_id }, Some(kept)) if kept.checkout_id == *checkout_id => {
            Some(kept.clone())
        }
        _ => None,
    };
    d.pay = next;
    if let Some(kept) = went_in_flight {
        if !s.in_flight.iter().any(|c| c.checkout_id == kept.checkout_id) {
            s.in_flight.push(kept);
        }
    }
    let mut cancel = false;
    for effect in effects {
        cancel |= run_effect(info, app, s, effect);
    }
    cancel
}

/// One effect of the machine; whether it cancels the navigation asked about.
fn run_effect(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, effect: Effect) -> bool {
    match effect {
        Effect::CreateCheckout { surface, payer, .. } => {
            start_pay_checkout(info, app, s, surface, payer);
        }
        Effect::ShowSurface(surface) => {
            println!(
                "AZDRIVE_PAY_SURFACE {} {}",
                surface.kind.as_str(),
                surface.url.host()
            );
            if let Some(d) = dialog(s) {
                d.notice.clear();
            }
        }
        Effect::WebviewNavigate(url) => {
            let node = info
                .get_node_id_by_marker(AzString::from(ids::PAY_WEBVIEW_MARKER))
                .into_option();
            match node {
                Some(node) => info.webview_navigate(node, AzString::from(url.reveal())),
                None => {
                    if let Some(d) = dialog(s) {
                        d.notice = t("azdrive-add-pay-page-not-shown");
                    }
                }
            }
        }
        Effect::AllowNavigation => {}
        Effect::CancelNavigation => return true,
        Effect::OpenBrowser(url) => open_browser(s, &url),
        Effect::SwitchSurface { checkout_id, kind } => {
            let token_url = s.token.url.clone().unwrap_or_default();
            let Some(d) = dialog(s) else {
                return false;
            };
            let Some((checkout, _)) = d.pay.presenting() else {
                return false;
            };
            let job = Job::Surface {
                serial: d.serial,
                token_url: checkout_token_url(d, &token_url),
                checkout_id,
                kind,
                choice: Box::new(checkout.choice.clone()),
                look: d.look(),
            };
            spawn(info, app, s, job);
        }
        Effect::Abandon { checkout_id } => {
            let token_url = s.token.url.clone().unwrap_or_default();
            let keyring = s.keyring.clone();
            let token_url = match dialog(s) {
                Some(d) => checkout_token_url(d, &token_url),
                None => token_url,
            };
            spawn(
                info,
                app,
                s,
                Job::Abandon {
                    token_url,
                    checkout_id,
                    keyring,
                },
            );
        }
        // The checkout in flight ended: the order asked for meanwhile goes on, from the button
        // it was asked with (the dialog still holds what it needs).
        Effect::OrderAgain => {
            if dialog(s).is_some_and(|d| d.cash_order) {
                pay_cash(info, app, s);
            } else {
                buy(info, app, s);
            }
        }
        Effect::StartPoll { checkout_id } => start_poll(info, app, s, &checkout_id),
        Effect::StopPoll => {
            if let Some(d) = dialog(s) {
                if let BuyStep::Paying { cancel, .. } = &d.step {
                    cancel.store(true, Ordering::SeqCst);
                }
                d.step = BuyStep::Idle;
            }
            start_claims(info, app, s);
        }
        Effect::Notice(notice) => {
            if let Notice::Blocked { host } = &notice {
                println!("AZDRIVE_PAY_BLOCKED {host}");
            }
            if let Some(d) = dialog(s) {
                d.notice = crate::pay_words::notice(&notice);
            }
        }
        // Cash by post: the two pages are offered (the dialog shows them), the order joins the
        // drive list.
        Effect::ShowPaper(slip) => crate::cash::posted(s, &slip),
    }
    false
}

/// The token server the dialog's checkout was made at (the keyring's entry knows), else this
/// run's.
fn checkout_token_url(d: &AddDialog, fallback: &str) -> String {
    match &d.kept {
        Some(kept) if !kept.token_url.is_empty() => kept.token_url.clone(),
        _ => fallback.to_string(),
    }
}

/// The machine's checkout through the chosen provider: on the keyring's list with its claim key
/// before anything shows it.
fn start_pay_checkout(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    surface: azul_pay::SurfaceKind,
    payer: Option<azul_pay::Payer>,
) {
    let token_url = s.token.url.clone();
    let keyring = s.keyring.clone();
    let renew = dialog_ref(s).and_then(|d| renew_job(s, d));
    let Some(d) = dialog(s) else {
        return;
    };
    // The payer's details leave with this checkout alone: the dialog keeps none of them.
    let _ = d.take_payer();
    let PayState::Preparing { choice, .. } = &d.pay else {
        return;
    };
    let via = PayVia {
        choice: choice.clone(),
        surface,
        look: d.look(),
        country: d.country.clone(),
        consent: d.consent,
        payer,
    };
    let (Some(token_url), Some(tier)) = (token_url, d.chosen_tier().map(|t| t.id.clone())) else {
        let _ = pay(
            info,
            app,
            s,
            Event::CreateFailed(t("azdrive-add-no-server-or-tier")),
        );
        return;
    };
    d.step = BuyStep::StartingCheckout;
    d.notice = t("azdrive-add-preparing-payment");
    d.kept = None;
    let job = Job::Checkout {
        serial: d.serial,
        token_url,
        tier,
        months: d.months(),
        // Through a provider the checkout names its method itself.
        method: String::new(),
        name: d.buy_name.trim().to_string(),
        keyring,
        via: Some(Box::new(via)),
        renew,
    };
    spawn(info, app, s, job);
}

/// The system browser opens `url` (a headless run opens nothing); the dialog says where, the
/// script line names the host only.
fn open_browser(s: &mut DriveState, url: &SecretUrl) {
    println!("AZDRIVE_OPEN_BROWSER {}", url.host());
    let page = url.reveal();
    let opened = Url::parse(page.as_str())
        .into_result()
        .map(|parsed| parsed.open())
        .unwrap_or(false);
    if !opened {
        if let Some(d) = dialog(s) {
            d.notice = t_args(
                "azdrive-add-open-pay-page",
                &[("page", Arg::from(page.to_string()))],
            );
        }
    }
}

/// The dialog waits for the drive of the machine's checkout (the claim flow's wait).
fn start_poll(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, checkout_id: &str) {
    let keyring = s.keyring.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    let Some(kept) = d.kept.clone().filter(|k| k.checkout_id == checkout_id) else {
        return;
    };
    if let BuyStep::Paying { cancel, .. } = &d.step {
        cancel.store(true, Ordering::SeqCst);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    d.step = BuyStep::Paying {
        checkout_id: checkout_id.to_string(),
        cancel: cancel.clone(),
    };
    let serial = d.serial;
    let token_url = kept.token_url.clone();
    spawn(
        info,
        app,
        s,
        Job::AwaitPayment {
            serial,
            checkout: kept,
            token_url,
            keyring,
            cancel,
        },
    );
}

/// The next surface of the machine's checkout (or why there is none).
pub(crate) fn surface_answered(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    result: Result<azul_pay::Surface, String>,
) {
    if dialog_of(s, serial).is_none() {
        return;
    }
    let event = match result {
        Ok(surface) => Event::Switched(surface),
        Err(why) => Event::SwitchFailed(l10n::t_label(&why)),
    };
    let _ = pay(info, app, s, event);
}

/// What an abandon found: an ended checkout is off the keyring's list; one paid meanwhile, or
/// one the token server could not end, stays on it in flight - the background claims bring its
/// drive, and a new order asks about it first. The dialog's machine hears the answer.
pub(crate) fn abandoned(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    checkout_id: &str,
    result: Abandoned,
) {
    let outcome = match &result {
        Abandoned::Ended | Abandoned::Settled => {
            println!("AZDRIVE_ABANDONED {checkout_id} ok");
            s.in_flight.retain(|c| c.checkout_id != checkout_id);
            AbandonOutcome::Ended
        }
        Abandoned::Kept { paid, why } => {
            println!(
                "AZDRIVE_ABANDONED {checkout_id} {}",
                if *paid { "paid" } else { "kept" }
            );
            eprintln!("[azdrive] the checkout {checkout_id} stays on the list: {why}");
            // The dialog's own checkout is known here; any other the claims report.
            let kept = dialog(s)
                .and_then(|d| d.kept.clone())
                .filter(|k| k.checkout_id == checkout_id);
            if let Some(kept) = kept {
                if !s.in_flight.iter().any(|c| c.checkout_id == checkout_id) {
                    s.in_flight.push(kept);
                }
            }
            start_claims(info, app, s);
            if *paid {
                AbandonOutcome::Paid
            } else {
                AbandonOutcome::Open
            }
        }
    };
    let _ = pay(
        info,
        app,
        s,
        Event::Abandoned {
            checkout_id: checkout_id.to_string(),
            outcome,
        },
    );
}

/// The background claims found `checkout`, whose payment may be under way (bought here, not
/// claimed yet): a new order asks about it first.
pub(crate) fn in_flight(s: &mut DriveState, checkout: PendingCheckout) {
    if !s.in_flight.iter().any(|c| c.checkout_id == checkout.checkout_id) {
        s.in_flight.push(checkout.clone());
    }
    if let Some(d) = dialog(s) {
        if matches!(d.pay, PayState::Choosing) {
            d.pay = PayState::InFlight {
                checkout_id: checkout.checkout_id.clone(),
            };
            d.kept = Some(checkout);
        }
    }
}

/// The checkout's answer. Through a provider: the machine shows its surface (the checkout is on
/// the keyring's list already). The v1 checkout: its payment page opens in the browser, and the
/// dialog waits for the drive (a payment the dialog does not see still brings the drive).
pub(crate) fn checkout_started(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    result: Result<Started, String>,
) {
    let keyring = s.keyring.clone();
    if dialog_of(s, serial).is_none() {
        match result {
            // A checkout through a provider whose popover nobody sees: nobody pays it.
            Ok(started) if started.created.is_some() => {
                let job = Job::Abandon {
                    token_url: started.kept.token_url.clone(),
                    checkout_id: started.checkout.checkout_id.clone(),
                    keyring,
                };
                spawn(info, app, s, job);
            }
            // The dialog closed while the v1 checkout was made: the background claims wait
            // for it.
            Ok(_) => start_claims(info, app, s),
            Err(_) => {}
        }
        return;
    }
    let Some(d) = dialog_of(s, serial) else {
        return;
    };
    // Through a provider: the machine decides (a checkout for a popover that closed meanwhile
    // is abandoned by it).
    let through_provider = matches!(&result, Ok(started) if started.created.is_some())
        || matches!(d.pay, PayState::Preparing { .. });
    if through_provider {
        d.step = BuyStep::Idle;
        let event = match result {
            Ok(started) => {
                println!("AZDRIVE_CHECKOUT {}", started.checkout.checkout_id);
                d.kept = Some(started.kept);
                match (started.created, started.cash) {
                    (Some(created), _) => Event::Created(Box::new(created)),
                    (None, Some(slip)) => Event::Posted(Box::new(slip)),
                    (None, None) => Event::CreateFailed(t("azdrive-add-no-surface")),
                }
            }
            Err(why) => Event::CreateFailed(l10n::t_label(&why)),
        };
        let _ = pay(info, app, s, event);
        return;
    }
    let (checkout, kept) = match result {
        Ok(started) => (started.checkout, started.kept),
        Err(why) => {
            d.step = BuyStep::Idle;
            d.notice = t_args(
                "azdrive-add-pay-not-prepared",
                &[("why", Arg::from(l10n::t_label(&why)))],
            );
            return;
        }
    };
    let token_url = kept.token_url.clone();
    println!("AZDRIVE_CHECKOUT {}", checkout.checkout_id);
    let opened = Url::parse(checkout.pay_url.as_str())
        .into_result()
        .map(|url| url.open())
        .unwrap_or(false);
    let cancel = Arc::new(AtomicBool::new(false));
    d.step = BuyStep::Paying {
        checkout_id: checkout.checkout_id.clone(),
        cancel: cancel.clone(),
    };
    d.notice = if opened {
        t("azdrive-add-pay-page-open")
    } else {
        t_args(
            "azdrive-add-pay-page-open-yourself",
            &[("page", Arg::from(checkout.pay_url.to_string()))],
        )
    };
    spawn(
        info,
        app,
        s,
        Job::AwaitPayment {
            serial,
            checkout: kept,
            token_url,
            keyring,
            cancel,
        },
    );
}

/// "Stop waiting": the payment page stays where it is; the dialog asks no more, the background
/// claims do (and every start of AzDrive): a payment made now still brings the drive.
fn stop_waiting(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(d) = dialog(s) else {
        return;
    };
    let stopped = match &d.step {
        BuyStep::Paying {
            cancel,
            checkout_id,
        } => {
            cancel.store(true, Ordering::SeqCst);
            Some(t_args(
                "azdrive-add-stopped-waiting-for",
                &[("checkout", Arg::from(checkout_id.as_str()))],
            ))
        }
        _ => None,
    };
    d.step = BuyStep::Idle;
    d.notice = stopped
        .clone()
        .unwrap_or_else(|| t("azdrive-add-stopped-waiting"));
    if stopped.is_some() {
        start_claims(info, app, s);
    }
}

/// The wait for a payment ended without a drive: why - said in the dialog, else in the window
/// (the checkout's end is said once). Empty: the dialog stopped waiting ("Stop waiting", Back,
/// Cancel, the dialog closed), and the background claims wait for the payment instead.
pub(crate) fn payment_ended(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    why: Text,
    ended: bool,
) {
    if why.is_empty() {
        start_claims(info, app, s);
        return;
    }
    let said = l10n::t_text(&why);
    let waiting = match dialog_of(s, serial) {
        Some(d) => {
            d.step = BuyStep::Idle;
            matches!(d.pay, PayState::Waiting { .. })
        }
        None => {
            s.info(why);
            return;
        }
    };
    if waiting {
        // The machine's checkout ended: a new order may follow. One that did not end (the wait
        // timed out) stays in flight: a new order asks about it first.
        let event = if ended {
            Event::Declined(said.clone())
        } else {
            Event::StopWaiting
        };
        let _ = pay(info, app, s, event);
    }
    if let Some(d) = dialog_of(s, serial) {
        d.notice = said;
    }
}

/// A development server's test drive: into the drives file under the name typed, into the
/// source list with its session (the keyring has it: the job wrote it under the drive's lock -
/// unless the keyring did not take it, then it is tried once more from here).
pub(crate) fn bought(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    result: Result<BoughtDrive, String>,
) {
    let token_url = s.token.url.clone().unwrap_or_default();
    let file_path = s.drives_file.clone();
    let (name, open) = match dialog_of(s, serial) {
        Some(d) => (d.buy_name.trim().to_string(), true),
        None => (String::new(), false),
    };
    let bought = match result {
        Ok(bought) => bought,
        Err(why) => {
            if let Some(d) = dialog_of(s, serial) {
                d.step = BuyStep::Idle;
                d.notice = l10n::t_label(&why);
            } else {
                s.error(Phrase::new("azdrive-add-not-made").arg("why", l10n::t_label(&why)));
            }
            return;
        }
    };
    let entry = bought.bundle.entry_named(&name, &token_url);
    let saved = save_entry(file_path.as_deref(), &entry);
    let in_keyring = bought.unsaved.is_none();
    // The session is kept even when the drives file cannot take the entry: the drive token is
    // the only way back into the drive.
    let drive_id = entry.id.clone();
    add_slot(info, app, s, entry, Some(bought.session), open, in_keyring);
    if let Err(problem) = saved {
        // Said in the window (the dialog closed with the drive).
        s.error(Phrase::new("azdrive-add-unsaved").arg("problem", problem));
    }
    // "We always encrypt": the new drive's keys and recovery sheet are part of its making.
    #[cfg(feature = "encryption")]
    crate::encryption::encrypt_new_drive(info, app, s, &drive_id);
    #[cfg(not(feature = "encryption"))]
    let _ = drive_id;
}

// ==== The claims: a paid drive reaches AzDrive however late ====

/// Starts the background claims of the keyring's unfinished checkouts, unless they run: at
/// every start, after "Stop waiting".
pub(crate) fn start_claims(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.claiming {
        return;
    }
    s.claiming = true;
    // The cash orders are asked in this run: their next daily look is a day from now.
    crate::cash::looked(s);
    // Background work: an entry only a prompt would hand over waits for the next start.
    let keyring = s.quiet_keyring.clone();
    let token_url = s.token.url.clone();
    let store = s.period_tokens.clone();
    let cash_every = s.cash_looks.every();
    spawn(
        info,
        app,
        s,
        Job::Claims {
            keyring,
            token_url,
            store,
            drives: crate::periods::azlin_drives(s),
            cash_every,
        },
    );
}

/// A paid checkout's drive, its session in the keyring (written there under the drive's lock):
/// from the dialog's wait (`serial`: the dialog closes and the window opens the drive) or from
/// the background claims (it joins the source list, the window stays where it is). Idempotent
/// by drive id: a drive the window has keeps its slot and its session. Once the drive is in the
/// drives file its checkout is finished (AZDRIVE-INTEGRATION §4): it leaves the keyring's list,
/// a paid one only after its period tokens are issued and kept ([`Job::FinishCheckout`]); until
/// then the next start claims it again.
pub(crate) fn claimed(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: Option<u64>,
    checkout: &PendingCheckout,
    claimed: &Claimed,
) {
    let drive_id = claimed.bundle.drive_id().to_string();
    println!("AZDRIVE_CLAIMED {} {drive_id}", checkout.checkout_id);
    s.in_flight.retain(|c| c.checkout_id != checkout.checkout_id);
    crate::cash::claimed(s, &checkout.checkout_id);
    // A checkout picked up by its claim code learns its tier from the sealed sign-up: its
    // period tokens are issued for it.
    let mut checkout = checkout.clone();
    if checkout.tier.is_empty() {
        checkout.tier = claimed.bundle.tier.clone().unwrap_or_default();
    }
    let checkout = &checkout;
    let token_url = if checkout.token_url.is_empty() {
        s.token.url.clone().unwrap_or_default()
    } else {
        checkout.token_url.clone()
    };
    let from_dialog = serial.is_some_and(|serial| dialog_of(s, serial).is_some());
    let in_drives_file = match s.slot_index(&drive_id) {
        Some(index) => {
            if from_dialog {
                cancel(s);
                s.selected_drive = Some(index);
                go(info, app, s, Place::folder(&drive_id, ""), true);
            }
            true
        }
        None => {
            let entry = claimed.bundle.entry_named(&checkout.name, &token_url);
            let saved = save_entry(s.drives_file.as_deref(), &entry);
            add_slot(
                info,
                app,
                s,
                entry,
                Some(claimed.session.clone()),
                from_dialog,
                true,
            );
            // "We always encrypt": a paid drive's keys and recovery sheet, however late it
            // arrived (a drive claimed at a start shows the sheet then).
            #[cfg(feature = "encryption")]
            crate::encryption::encrypt_new_drive(info, app, s, &drive_id);
            match saved {
                Ok(()) => true,
                Err(problem) => {
                    s.error(Phrase::new("azdrive-add-unsaved-again").arg("problem", problem));
                    false
                }
            }
        }
    };
    if in_drives_file {
        let azlin = crate::periods::azlin_drives(s).remove(&drive_id);
        let job = Job::FinishCheckout {
            keyring: s.keyring.clone(),
            store: s.period_tokens.clone(),
            azlin,
            checkout: checkout.clone(),
            drive_id,
            grant: claimed.bundle.period_tokens.clone(),
            token_url,
        };
        spawn(info, app, s, job);
    }
}

/// A renewal's (or a change of tier's) months were claimed for the drive `drive_id`: said, and
/// a dialog that waited for them closes; the background claims keep their grant with the
/// drive and issue the first month ([`azcloud_kit::pending::finish`]).
pub(crate) fn renewed(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: Option<u64>,
    checkout_id: &str,
    drive_id: &str,
    months: u32,
) {
    println!("AZDRIVE_RENEWED {checkout_id} {drive_id} {months}");
    s.in_flight.retain(|c| c.checkout_id != checkout_id);
    // a locked drive: the payment is named in a proof, so it counts at once
    crate::archive::renewed(info, app, s, drive_id, checkout_id);
    crate::cash::claimed(s, checkout_id);
    let name = s.drive_name(&Place::folder(drive_id, ""));
    s.success(
        Phrase::new("azdrive-months-renewed")
            .arg("months", months)
            .arg("drive", name),
    );
    if serial.is_some_and(|serial| dialog_of(s, serial).is_some()) {
        cancel(s);
        start_claims(info, app, s);
    }
}

/// The background claims took a checkout off the keyring's list: said once - a cash order's end
/// on the drive list (until it is dismissed) and in the dialog that shows it.
pub(crate) fn checkout_dropped(s: &mut DriveState, checkout_id: &str, why: &str) {
    s.in_flight.retain(|c| c.checkout_id != checkout_id);
    // A checkout in flight that ended: the dialog's machine may order again.
    if let Some(d) = dialog(s) {
        if matches!(&d.pay, PayState::InFlight { checkout_id: id } if id == checkout_id) {
            d.pay = PayState::Choosing;
        }
    }
    if crate::cash::ended(s, checkout_id, why) {
        let text = crate::cash::ended_text(why);
        if let Some(d) = dialog(s) {
            if d.pay.checkout_id() == Some(checkout_id) {
                d.pay = PayState::Choosing;
                d.notice = text.clone();
            }
        }
        s.warn(text);
        return;
    }
    s.warn(
        Phrase::new("azdrive-add-checkout-dropped")
            .arg("checkout", checkout_id)
            .arg("why", l10n::t_label(why)),
    );
}

// ==== Cash by post: a claim code picked up ====

/// "Pick up a paid drive with a claim code"'s Pick up: the code read (a typo is said), its cash
/// checkout onto the keyring's list on a worker thread; then the background claims ask for it.
fn pick_up(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let token_url = s.token.url.clone().unwrap_or_default();
    let keyring = s.keyring.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    if d.busy() {
        return;
    }
    // a join code from another device of the drive goes in the same field
    if let Ok(code) = azcloud_kit::JoinCode::decode(&d.claim_code) {
        d.step = BuyStep::Creating;
        d.notice = t("azdrive-add-joining");
        let serial = d.serial;
        spawn(
            info,
            app,
            s,
            Job::Join {
                serial,
                code,
                token_url,
                keyring,
            },
        );
        return;
    }
    match crate::cash::picked_up(&d.claim_code, &token_url, &d.buy_name) {
        Err(why) => d.notice = why,
        Ok(checkout) => {
            d.step = BuyStep::Creating;
            d.notice = t("azdrive-add-picking-up");
            let serial = d.serial;
            spawn(
                info,
                app,
                s,
                Job::PickUp {
                    serial,
                    checkout,
                    keyring,
                },
            );
        }
    }
}

/// A claim code's checkout is on the keyring's list (or why not): the background claims ask
/// for its drive now, then once a day.
pub(crate) fn picked_up_answered(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    checkout: PendingCheckout,
    result: Result<(), String>,
) {
    if let Some(d) = dialog_of(s, serial) {
        d.step = BuyStep::Idle;
        d.notice = match &result {
            Ok(()) => t("azdrive-add-picked-up"),
            Err(why) => t_args(
                "azdrive-add-claim-not-kept",
                &[("why", Arg::from(l10n::t_label(why)))],
            ),
        };
        if result.is_ok() {
            d.claim_code.clear();
        }
    }
    if result.is_err() {
        return;
    }
    println!("AZDRIVE_PICKED_UP {}", checkout.checkout_id);
    crate::cash::upsert(s, checkout);
    start_claims(info, app, s);
}

/// A join code's drive is on this device (or why not): it joins the source list and opens; an
/// encrypted drive takes its key from the code's seal.
pub(crate) fn joined(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    code: &azcloud_kit::JoinCode,
    result: Result<azcloud_kit::Joined, String>,
) {
    let joined = match result {
        Ok(joined) => joined,
        Err(why) => {
            if let Some(d) = dialog_of(s, serial) {
                d.step = BuyStep::Idle;
                d.notice = t_args(
                    "azdrive-add-not-joined",
                    &[("why", Arg::from(l10n::t_label(&why)))],
                );
            }
            return;
        }
    };
    let drive_id = joined.drive_id().to_string();
    println!("AZDRIVE_JOINED {drive_id}");
    let token_url = s.token.url.clone().unwrap_or_else(|| code.token_url.clone());
    let name = dialog_of(s, serial)
        .map(|d| d.buy_name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| code.name.clone());
    let entry = joined.bundle.entry_named(&name, &token_url);
    let saved = save_entry(s.drives_file.as_deref(), &entry);
    let from_dialog = dialog_of(s, serial).is_some();
    let in_keyring = joined.saved.is_ok();
    let secret = joined.session.to_keyring_secret();
    add_slot(info, app, s, entry, Some(secret), from_dialog, in_keyring);
    if let Err(problem) = saved {
        s.error(Phrase::new("azdrive-add-unsaved").arg("problem", problem));
    }
    // the drive key rides in the code, sealed: this device enrols itself with it
    #[cfg(feature = "encryption")]
    {
        if let Some(seal) = code.key_seal.clone() {
            crate::encryption::adopt_join_key(info, app, s, &drive_id, seal);
        }
    }
}

/// The background claims ended.
pub(crate) fn claims_done(s: &mut DriveState, problem: Option<Text>) {
    s.claiming = false;
    if let Some(problem) = problem {
        s.warn(problem);
    }
}

/// A claimed checkout finished: off the keyring's list, its period tokens kept - or what kept it
/// there (a failed issue is tried again by the claims and at the next start, quietly).
pub(crate) fn checkout_finished(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    checkout_id: &str,
    result: Result<Option<Finished>, String>,
) {
    match result {
        Ok(None | Some(Finished::Settled)) => {}
        Ok(Some(Finished::Issued { drive_id, count })) => {
            println!("AZDRIVE_PERIOD_TOKENS {checkout_id} {drive_id} {count}");
            // A drive whose free month is nearly gone gets its first paid one at once.
            crate::periods::start_redemptions(info, app, s, Some(&drive_id));
        }
        Ok(Some(Finished::Granted { drive_id, why })) => {
            println!("AZDRIVE_PERIOD_TOKENS {checkout_id} {drive_id} 0");
            eprintln!(
                "[azdrive] the first month of the checkout {checkout_id} comes with the drive's \
                 next look: {why}"
            );
            crate::periods::start_redemptions(info, app, s, Some(&drive_id));
        }
        Ok(Some(Finished::Dropped(why))) => s.warn(
            Phrase::new("azdrive-add-months-not-fetched")
                .arg("checkout", checkout_id)
                .arg("why", why.as_str()),
        ),
        Ok(Some(Finished::Kept(why))) => {
            eprintln!(
                "[azdrive] the period tokens of the checkout {checkout_id} wait for the next \
                 try: {why}"
            );
        }
        Err(why) => s.error(
            Phrase::new("azdrive-add-checkout-unfinished")
                .arg("checkout", checkout_id)
                .arg("why", l10n::t_label(&why)),
        ),
    }
}
