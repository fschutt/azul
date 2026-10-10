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
//! `AZDRIVE_TIERS <n>`, `AZDRIVE_PILLS <method>:<provider> ...` (`-` for none: the v1 checkout),
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
    pending::{Claimed, Finished},
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
use azul_pay::{
    machine::Notice,
    offer::{Offer, OfferContext},
    Effect, Event, Method, SecretUrl, State as PayState,
};
use azul_storage::{
    config::{self, DriveAuth, DriveEntry, DriveLocation, DrivesFile},
    oauth::Tokens,
    time::iso8601,
    DriveError,
};

use crate::{
    add_drive::{AddDialog, BuyStep, OfferState, TiersState, COUNTRIES},
    browse::Place,
    go, ids,
    jobs::{BoughtDrive, Job, PayVia, Started},
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
    /// Buy storage's country (an index of `COUNTRIES`).
    Country(usize),
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
}

/// The open dialog, if the popup is it.
fn dialog(s: &mut DriveState) -> Option<&mut AddDialog> {
    match s.popup.as_mut() {
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

/// Opens the dialog on its two choices.
pub(crate) fn open(s: &mut DriveState) {
    s.popups_opened += 1;
    let d = AddDialog::new(s.popups_opened);
    print_page(&d);
    s.popup = Some(Popup::AddDrive(d));
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
            load_tiers(info, app, s, false);
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
                d.pay = PayState::Choosing;
                d.back();
            }
        }
        AddEvent::Service(id) => {
            if !d.open_service(id) {
                d.error = String::from("This app cannot open that source.");
            }
        }
        AddEvent::Tier(index) => {
            // The order is fixed while a payment runs.
            if !d.busy() {
                d.tier = index;
                d.notice.clear();
            }
        }
        AddEvent::Yearly => {
            if !d.busy() {
                d.yearly = !d.yearly;
                d.notice.clear();
                print_pills(d);
            }
        }
        AddEvent::RetryTiers => load_tiers(info, app, s, true),
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
        AddEvent::Country(index) => {
            let changed = !d.busy()
                && COUNTRIES
                    .get(index)
                    .is_some_and(|(code, _)| d.set_country(code));
            if changed {
                load_options(info, app, s);
            }
        }
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
    }
    if let Some(d) = dialog(s) {
        if d.page_line() != page_before {
            print_page(d);
        }
    }
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
        AuthSessionStatus::Cancelled => Some(("cancelled", String::from("The sign-in was cancelled."))),
        AuthSessionStatus::TimedOut => Some((
            "timed-out",
            String::from("The sign-in did not come back in time. Try again."),
        )),
        AuthSessionStatus::Unsupported => Some((
            "unsupported",
            format!("This computer cannot run the sign-in: {}.", result.message.as_str()),
        )),
        AuthSessionStatus::Failed => Some((
            "failed",
            format!("The sign-in did not finish: {}.", result.message.as_str()),
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
        d.sign_in_failed(&format!(
            "The sign-in did not finish: {}.",
            code.message.as_str()
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
            let why = format!("The sign-in's token request failed: {why}");
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
        return Err(String::from(
            "There is no configuration folder to save the drive in.",
        ));
    };
    DrivesFile::load(path)
        .and_then(|mut file| {
            file.add(entry.clone());
            file.save(path)
        })
        .map_err(|e: DriveError| format!("The drive could not be saved: {e}"))
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
    s.slots[index].secret = secret.clone();
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
        None => s.success(format!("\"{name}\" is a drive now.")),
    }
    if from_dialog {
        go(info, app, s, Place::folder(&id, ""), true);
        if stored_already {
            s.success(format!(
                "\"{name}\" is a drive now; its session is in the system keyring."
            ));
        }
    } else {
        s.info(format!("\"{name}\" is ready: it is in the source list."));
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
        TiersState::Loading | TiersState::Loaded(_) => false,
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
    spawn(info, app, s, Job::Tiers { serial, token_url });
}

/// Why Buy storage cannot ask anyone.
fn no_token_server() -> String {
    String::from(
        "No Azlin token server is set: start AzDrive with --token-url, set AZLIN_TOKEN_URL, or \
         name one in the endpoints of the shared Azlin config.",
    )
}

/// The tier list's answer; then the payment options are asked for (the page is ready -
/// `AZDRIVE_TIERS` - once they have answered too).
pub(crate) fn tiers_answered(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    result: Result<Tiers, String>,
) {
    let Some(d) = dialog_of(s, serial) else {
        return;
    };
    match result {
        Ok(tiers) => {
            if d.tier >= tiers.tiers.len() {
                d.tier = 0;
            }
            d.tiers = TiersState::Loaded(tiers);
            load_options(info, app, s);
        }
        Err(why) => d.tiers = TiersState::Failed(why),
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
        token_url,
        tier,
        months: d.months(),
        country: d.country.clone(),
        currency,
    };
    spawn(info, app, s, job);
}

/// The payment options' answer: the offer, narrowed by azul-pay's registry (what it dropped
/// goes to the log), or the v1 checkout (an older token server, a failure).
pub(crate) fn options_answered(
    s: &mut DriveState,
    serial: u64,
    result: Result<Option<String>, String>,
) {
    let token_url = s.token.url.clone().unwrap_or_default();
    let Some(d) = dialog_of(s, serial) else {
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
        Err(why) => d.offer = OfferState::Failed(why),
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
    let Some(token_url) = token_url else {
        d.notice = no_token_server();
        return None;
    };
    let Some(tier) = d.chosen_tier().map(|t| t.id.clone()) else {
        d.notice = String::from("Choose a tier first.");
        return None;
    };
    if d.buy_name.trim().is_empty() {
        d.notice = String::from("Give the drive a name.");
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
        d.notice = String::from("Type the voucher's code.");
        return;
    }
    let tier = d.chosen_tier().map(|t| t.id.clone()).unwrap_or_default();
    let serial = d.serial;
    d.step = BuyStep::Creating;
    d.notice = String::from("Redeeming the voucher...");
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
        d.notice = String::from("Creating the test drive...");
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
        d.notice = String::from("Choose how to pay first.");
        return;
    };
    let consent = d.consent;
    let _ = pay(info, app, s, Event::Pay { choice, consent });
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
    let months = match dialog(s) {
        Some(d) => {
            d.step = BuyStep::StartingCheckout;
            d.notice = String::from("Preparing the payment...");
            d.months()
        }
        None => return,
    };
    let keyring = s.keyring.clone();
    spawn(
        info,
        app,
        s,
        Job::Checkout {
            serial,
            token_url,
            tier,
            months,
            name,
            keyring,
            via: None,
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
    d.pay = next;
    let mut cancel = false;
    for effect in effects {
        cancel |= run_effect(info, app, s, effect);
    }
    cancel
}

/// One effect of the machine; whether it cancels the navigation asked about.
fn run_effect(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, effect: Effect) -> bool {
    match effect {
        Effect::CreateCheckout { surface, .. } => start_pay_checkout(info, app, s, surface),
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
                        d.notice = String::from(
                            "The payment page is not shown here, so it could not be told to pay.",
                        );
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
                d.notice = notice.text();
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
) {
    let token_url = s.token.url.clone();
    let keyring = s.keyring.clone();
    let Some(d) = dialog(s) else {
        return;
    };
    let PayState::Preparing { choice, .. } = &d.pay else {
        return;
    };
    let via = PayVia {
        choice: choice.clone(),
        surface,
        look: d.look(),
        country: d.country.clone(),
        consent: d.consent,
    };
    let (Some(token_url), Some(tier)) = (token_url, d.chosen_tier().map(|t| t.id.clone())) else {
        let _ = pay(
            info,
            app,
            s,
            Event::CreateFailed(String::from("there is no token server or no tier")),
        );
        return;
    };
    d.step = BuyStep::StartingCheckout;
    d.notice = String::from("Preparing the payment...");
    d.kept = None;
    let job = Job::Checkout {
        serial: d.serial,
        token_url,
        tier,
        months: d.months(),
        name: d.buy_name.trim().to_string(),
        keyring,
        via: Some(Box::new(via)),
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
            d.notice = format!("Open this payment page in your browser: {page}");
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
        Err(why) => Event::SwitchFailed(why),
    };
    let _ = pay(info, app, s, event);
}

/// A checkout abandoned at the token server (and off the keyring's list).
pub(crate) fn abandoned(checkout_id: &str, result: Result<(), String>) {
    match result {
        Ok(()) => println!("AZDRIVE_ABANDONED {checkout_id} ok"),
        Err(why) => {
            println!("AZDRIVE_ABANDONED {checkout_id} error");
            eprintln!("[azdrive] the checkout {checkout_id} was not abandoned: {why}");
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
                    (None, None) => Event::CreateFailed(String::from(
                        "the token server answered without a payment surface",
                    )),
                }
            }
            Err(why) => Event::CreateFailed(why),
        };
        let _ = pay(info, app, s, event);
        return;
    }
    let (checkout, kept) = match result {
        Ok(started) => (started.checkout, started.kept),
        Err(why) => {
            d.step = BuyStep::Idle;
            d.notice = format!("The payment could not be prepared: {why}");
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
        String::from(
            "The payment page is open in your browser. The drive appears here once the payment \
             went through.",
        )
    } else {
        format!(
            "Open this payment page in your browser: {}. The drive appears here once the \
             payment went through.",
            checkout.pay_url
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
            Some(format!(
                "Stopped waiting for the payment of checkout {checkout_id}. A payment made now \
                 still brings the drive: AzDrive asks in the background, and again at its next \
                 start."
            ))
        }
        _ => None,
    };
    d.step = BuyStep::Idle;
    d.notice = stopped
        .clone()
        .unwrap_or_else(|| String::from("Stopped waiting."));
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
    why: String,
) {
    if why.is_empty() {
        start_claims(info, app, s);
        return;
    }
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
        // The machine's checkout ended: a new order may follow.
        let _ = pay(info, app, s, Event::Declined(why.clone()));
    }
    if let Some(d) = dialog_of(s, serial) {
        d.notice = why;
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
                d.notice = why;
            } else {
                s.error(format!("The new drive could not be made: {why}"));
            }
            return;
        }
    };
    let entry = bought.bundle.entry_named(&name, &token_url);
    let saved = save_entry(file_path.as_deref(), &entry);
    let in_keyring = bought.unsaved.is_none();
    // The session is kept even when the drives file cannot take the entry: the drive token is
    // the only way back into the drive.
    add_slot(info, app, s, entry, Some(bought.session), open, in_keyring);
    if let Err(problem) = saved {
        // Said in the window (the dialog closed with the drive).
        s.error(format!(
            "{problem} The drive works until AzDrive closes; its session is in the keyring."
        ));
    }
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
    let keyring = s.keyring.clone();
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
            match saved {
                Ok(()) => true,
                Err(problem) => {
                    s.error(format!(
                        "{problem} The drive works until AzDrive closes; its session is in the \
                         keyring, and AzDrive adds it again at its next start."
                    ));
                    false
                }
            }
        }
    };
    if in_drives_file {
        let job = Job::FinishCheckout {
            keyring: s.keyring.clone(),
            store: s.period_tokens.clone(),
            checkout: checkout.clone(),
            drive_id,
            grant: claimed.bundle.period_tokens.clone(),
            token_url,
        };
        spawn(info, app, s, job);
    }
}

/// The background claims took a checkout off the keyring's list: said once - a cash order's end
/// on the drive list (until it is dismissed) and in the dialog that shows it.
pub(crate) fn checkout_dropped(s: &mut DriveState, checkout_id: &str, why: &str) {
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
    s.warn(format!(
        "The checkout {checkout_id} is not waited for any more: {why}."
    ));
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
    match crate::cash::picked_up(&d.claim_code, &token_url, &d.buy_name) {
        Err(why) => d.notice = why,
        Ok(checkout) => {
            d.step = BuyStep::Creating;
            d.notice = String::from("Picking the drive up\u{2026}");
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
            Ok(()) => String::from(
                "Picked up. AzDrive asks for the drive now, then once a day until the money \
                 arrived: postal cash takes a while.",
            ),
            Err(why) => format!("The claim code was not kept: {why}"),
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

/// The background claims ended.
pub(crate) fn claims_done(s: &mut DriveState, problem: Option<String>) {
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
        Ok(Some(Finished::Dropped(why))) => s.warn(format!(
            "The paid months of the checkout {checkout_id} could not be fetched: {why}."
        )),
        Ok(Some(Finished::Kept(why))) => {
            eprintln!(
                "[azdrive] the period tokens of the checkout {checkout_id} wait for the next \
                 try: {why}"
            );
        }
        Err(why) => s.error(format!(
            "The checkout {checkout_id} could not be finished in the keyring's list ({why}); \
             AzDrive asks about it again at its next start."
        )),
    }
}
