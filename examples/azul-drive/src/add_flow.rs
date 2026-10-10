//! What the Add drive dialog's buttons start, and what their jobs answer: a source's connection
//! test and its saving (the drives file without secrets, the keyring with them), Buy storage's
//! tier list, a test drive of a development token server, a checkout paid in the browser and
//! the wait for its drive - and the claims of the checkouts no dialog waits for any more: after
//! "Stop waiting" in the background, at every start (azcloud-kit's `pending`: the checkouts and
//! their claim keys outlive AzDrive in the keyring). The dialog's data is `add_drive`, its view
//! `ui_add_drive`.
//!
//! On stdout, for scripts: `AZDRIVE_ADD_PAGE <page>`, `AZDRIVE_TESTED ok|error`,
//! `AZDRIVE_TIERS <n>`, `AZDRIVE_CHECKOUT <checkout id>`, `AZDRIVE_CLAIMED <checkout id> <drive
//! id>`, `AZDRIVE_ADDED <drive id>`. No secret and no payment page address is printed.

use std::sync::{atomic::AtomicBool, Arc};

use azcloud_kit::{
    pending::{Claimed, Finished},
    Checkout, PendingCheckout, Tiers,
};
use azul::{prelude::*, url::Url};
use azul_storage::{
    config::{self, DriveEntry, DrivesFile},
    DriveError,
};

use crate::{
    add_drive::{AddDialog, BuyStep, TiersState},
    browse::Place,
    go,
    jobs::{BoughtDrive, Job},
    keyring, refresh_disks, spawn, DriveState, KeyringCall, KeyringOp, Popup, Slot,
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
    Save,
    Cancel,
    CreateTestDrive,
    Buy,
    StopWaiting,
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
            if let BuyStep::Paying { cancel, .. } = &d.step {
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            d.step = BuyStep::Idle;
            d.back();
        }
        AddEvent::Service(id) => {
            if !d.open_service(id) {
                d.error = String::from("This app cannot open that source.");
            }
        }
        AddEvent::Tier(index) => {
            d.tier = index;
            d.notice.clear();
        }
        AddEvent::Yearly => {
            d.yearly = !d.yearly;
            d.notice.clear();
        }
        AddEvent::RetryTiers => load_tiers(info, app, s, true),
        AddEvent::Test => test(info, app, s),
        AddEvent::Save => save(info, app, s),
        AddEvent::Cancel => cancel(s),
        AddEvent::CreateTestDrive => create_test_drive(info, app, s),
        AddEvent::Buy => buy(info, app, s),
        AddEvent::StopWaiting => stop_waiting(info, app, s),
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
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    s.popup = None;
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

/// The tier list's answer.
pub(crate) fn tiers_answered(s: &mut DriveState, serial: u64, result: Result<Tiers, String>) {
    let Some(d) = dialog_of(s, serial) else {
        return;
    };
    match result {
        Ok(tiers) => {
            println!("AZDRIVE_TIERS {}", tiers.tiers.len());
            if d.tier >= tiers.tiers.len() {
                d.tier = 0;
            }
            d.tiers = TiersState::Loaded(tiers);
        }
        Err(why) => d.tiers = TiersState::Failed(why),
    }
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

/// "Buy": a checkout, on the keyring's list of unfinished checkouts with its claim key; its
/// payment page opens in the browser when it is made.
fn buy(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
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
        },
    );
}

/// The checkout's answer: its payment page opens in the browser, and the dialog waits for the
/// drive (the checkout is on the keyring's list already: a payment the dialog does not see
/// still brings the drive).
pub(crate) fn checkout_started(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    serial: u64,
    result: Result<(Checkout, PendingCheckout), String>,
) {
    let keyring = s.keyring.clone();
    if dialog_of(s, serial).is_none() {
        // The dialog closed while the checkout was made: the background claims wait for it.
        if result.is_ok() {
            start_claims(info, app, s);
        }
        return;
    }
    let Some(d) = dialog_of(s, serial) else {
        return;
    };
    let (checkout, kept) = match result {
        Ok(started) => started,
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
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
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
    match dialog_of(s, serial) {
        Some(d) => {
            d.step = BuyStep::Idle;
            d.notice = why;
        }
        None => s.info(why),
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
    let keyring = s.keyring.clone();
    let token_url = s.token.url.clone();
    let store = s.period_tokens.clone();
    spawn(
        info,
        app,
        s,
        Job::Claims {
            keyring,
            token_url,
            store,
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

/// The background claims took a checkout off the keyring's list: said once.
pub(crate) fn checkout_dropped(s: &mut DriveState, checkout_id: &str, why: &str) {
    s.warn(format!(
        "The checkout {checkout_id} is not waited for any more: {why}."
    ));
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
    s: &mut DriveState,
    checkout_id: &str,
    result: Result<Option<Finished>, String>,
) {
    match result {
        Ok(None | Some(Finished::Settled)) => {}
        Ok(Some(Finished::Issued { drive_id, count })) => {
            println!("AZDRIVE_PERIOD_TOKENS {checkout_id} {drive_id} {count}");
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
