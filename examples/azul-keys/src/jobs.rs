//! What does not run in a callback, and what answers later:
//!
//! - the vault thread: every [`Work`] (list, create, unlock - Argon2id takes a quarter second -,
//!   save, change the master password, write an export) runs on an azul `Thread` against the data
//!   root's drive (`store::run`), its [`Done`] comes back to [`on_done`] on the UI thread. One save
//!   at a time: a change during a save saves again after it;
//! - the one-second timer: the clipboard countdown (clears it), the idle lock, the one-time
//!   codes' seconds;
//! - the OS keyring (one request at a time) and azul's biometric prompt: device unlock;
//! - the window's close (held while a save is on the way) and the input that restarts the idle
//!   lock.
//!
//! On stdout, for `scripts/azkeys_e2e.py` (never a secret): `AZKEYS_DATA <dir>`,
//! `AZKEYS_LISTED <n>`, `AZKEYS_CREATED <vault-id>`, `AZKEYS_UNLOCKED <vault-id> <items>`,
//! `AZKEYS_WRONG_PASSWORD <wait-seconds>`, `AZKEYS_SAVED <key>`, `AZKEYS_LOCKED`,
//! `AZKEYS_COPIED <label>`, `AZKEYS_CLIPBOARD_CLEARED`, `AZKEYS_PASSWORD_CHANGED`,
//! `AZKEYS_WRITTEN <key>`, `AZKEYS_DEVICE_UNLOCK <vault-id> <mode|off>`, `AZKEYS_FAILED <what>`.

use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use azul::{
    biometric::{BiometricKind, BiometricPrompt, BiometricResult},
    callbacks::{CallbackInfo, RefAny, TimerCallbackInfo, TimerCallbackReturn, Update},
    dom::ClipboardContent,
    error::KeyringResult,
    option::OptionString,
    str::String as AzString,
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    vec::StyledTextRunVec,
};
use azul_appkit::ui as kit;
use azul_storage::LocalDrive;

use crate::app::{device_unlock_key, now, DeviceUnlock, KeyringOp, KeysApp, Screen};
use crate::crypto::SecretKey;
use crate::lock::AutoLock;
use crate::sample::{sample_vault, SAMPLE_PASSWORD};
use crate::session::{OpenVault, Reading, Session};
use crate::store::{self, Done, Opened, VaultFile, Work};

// ==== The vault thread ====

/// What the vault thread hands back.
struct Reply {
    done: Option<Done>,
}

struct ThreadInit {
    root: PathBuf,
    work: Option<Work>,
}

/// Runs on the worker thread: the work on the data root's drive, then the answer to the UI
/// thread.
extern "C" fn vault_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((root, work)) = init.downcast_mut::<ThreadInit>().and_then(|mut i| {
        let work = i.work.take()?;
        Some((i.root.clone(), work))
    }) else {
        return;
    };
    let done = store::run(&LocalDrive::new(root), work);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Reply { done: Some(done) }),
    )));
}

/// Runs `work` on a new azul `Thread`; [`on_done`] gets its answer.
pub fn spawn(info: &mut CallbackInfo, app: &RefAny, root: &Path, work: Work) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(ThreadInit {
                root: root.to_path_buf(),
                work: Some(work),
            }),
            app.clone(),
            vault_thread,
        ),
    );
}

/// The vault thread answered.
extern "C" fn on_done(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(done) = msg.downcast_mut::<Reply>().and_then(|mut r| r.done.take()) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<KeysApp>() else {
        return Update::DoNothing;
    };
    handle(&mut guard, &mut info, &app, done);
    Update::RefreshDom
}

fn handle(s: &mut KeysApp, info: &mut CallbackInfo, app: &RefAny, done: Done) {
    match done {
        Done::Listed { vaults, problems } => {
            s.listed = true;
            s.busy = None;
            for problem in &problems {
                eprintln!("[azkeys] {problem}");
            }
            if !problems.is_empty() {
                s.notice = format!(
                    "{} file(s) in the vault folder are not vaults",
                    problems.len()
                );
            }
            s.vaults = vaults;
            println!("AZKEYS_LISTED {}", s.vaults.len());
            let last = setting(s, "last_vault");
            s.unlock.chosen = s
                .vaults
                .iter()
                .position(|v| Some(v.id()) == last.as_deref())
                .unwrap_or(0);
            if s.vaults.is_empty() && s.sample {
                s.busy = Some("Making the sample vault\u{2026}".to_string());
                let work = Work::Create {
                    vault: sample_vault(now()),
                    password: Zeroizing::new(SAMPLE_PASSWORD.to_string()),
                    kdf: None,
                };
                spawn(info, app, &s.data_root, work);
            } else if s.vaults.is_empty() || s.start_screen == "create" {
                s.screen = Screen::Create;
            }
        }
        Done::Created(opened) => {
            s.busy = None;
            let id = opened.vault.id.clone();
            println!("AZKEYS_CREATED {id}");
            if opened.vault.name == crate::sample::SAMPLE_NAME && s.sample {
                kit::set_value(&s.kit, info, "sample_vault", &id);
            }
            s.vaults.push(VaultFile {
                key: opened.key.clone(),
                envelope: opened.envelope.clone(),
            });
            s.vaults
                .sort_by_cached_key(|v| azul_pim::search::fold(v.name()));
            s.unlock.chosen = s.vaults.iter().position(|v| v.id() == id).unwrap_or(0);
            s.create = Default::default();
            open_session(s, info, app, opened);
        }
        Done::Unlocked(opened) => {
            s.busy = None;
            s.attempts.succeeded();
            println!(
                "AZKEYS_UNLOCKED {} {}",
                opened.vault.id,
                opened.vault.items.len()
            );
            open_session(s, info, app, opened);
        }
        Done::Saved { key, envelope } => {
            s.saving = false;
            println!("AZKEYS_SAVED {key}");
            replace_envelope(s, &key, envelope);
            if s.save_again {
                s.save_again = false;
                if let Some(session) = s.session.as_mut() {
                    session.dirty = true;
                }
                save(s, info, app);
            }
            after_save(s, info);
        }
        Done::PasswordChanged { key, envelope } => {
            s.busy = None;
            s.saving = false;
            replace_envelope(s, &key, envelope);
            s.change = Default::default();
            s.change.message = "The master password is changed.".to_string();
            println!("AZKEYS_PASSWORD_CHANGED");
            // A change made while the file was rewritten is saved now.
            if s.save_again {
                s.save_again = false;
                if let Some(session) = s.session.as_mut() {
                    session.dirty = true;
                }
                save(s, info, app);
            }
            after_save(s, info);
        }
        Done::Put { key } => {
            s.notice = format!(
                "Written to {}",
                azul_appkit::data::local_path(&s.data_root, &key).display()
            );
            println!("AZKEYS_WRITTEN {key}");
        }
        Done::Failed(f) => {
            s.busy = None;
            println!("AZKEYS_FAILED {}", f.what);
            let t = now();
            if f.wrong_password {
                // The field starts empty for the next try.
                zeroize::Zeroize::zeroize(&mut *s.unlock.password);
                s.attempts.failed(t);
                s.unlock.message = s.attempts.message(t);
                println!("AZKEYS_WRONG_PASSWORD {}", s.attempts.wait(t));
            } else if f.wrong_key {
                if let Some(v) = s.chosen_vault() {
                    let id = v.id().to_string();
                    kit::set_value(&s.kit, info, &device_unlock_key(&id), "");
                }
                s.unlock.message = "The key kept on this device no longer opens this vault. \
                                    Unlock it with the master password."
                    .to_string();
            } else if f.what == "save" || f.what == "change the password" {
                s.saving = false;
                s.notice = format!("Not saved: {}", f.message);
                if f.what == "change the password" {
                    s.change.message = format!("The password is not changed: {}", f.message);
                }
                after_save(s, info);
            } else if s.screen == Screen::Create {
                s.create.message = format!("The vault could not be made: {}", f.message);
            } else if s.screen == Screen::Unlock {
                s.unlock.message = format!("The vault cannot be opened: {}", f.message);
            } else {
                s.notice = format!("Could not {}: {}", f.what, f.message);
            }
        }
    }
}

/// The value of one of AzKeys' settings (the kit's settings file).
pub fn setting(s: &KeysApp, key: &str) -> Option<String> {
    let mut kit_ref = s.kit.clone();
    let value = kit_ref
        .downcast_ref::<kit::Kit>()
        .and_then(|k| k.settings.get(key).map(str::to_string));
    value.filter(|v| !v.is_empty())
}

/// The vault file's header after a save or a new master password.
fn replace_envelope(s: &mut KeysApp, key: &str, envelope: crate::crypto::Envelope) {
    if let Some(v) = s.vaults.iter_mut().find(|v| v.key == key) {
        v.envelope = envelope.clone();
    }
    if let Some(session) = s.session.as_mut().filter(|session| session.open.key == key) {
        session.open.envelope = envelope;
    }
}

/// The vault is open: the session, the vault screen, the idle lock; the `--screen` and the
/// files to import from the command line.
fn open_session(s: &mut KeysApp, info: &mut CallbackInfo, app: &RefAny, opened: Opened) {
    let id = opened.vault.id.clone();
    let mut session = Session::new(OpenVault {
        key: opened.key,
        envelope: opened.envelope,
        vault_key: opened.vault_key,
        vault: opened.vault,
    });
    zeroize::Zeroize::zeroize(&mut *s.unlock.password);
    s.unlock.message.clear();
    s.screen = Screen::Vault;
    s.auto_lock = AutoLock::new(s.settings.idle_minutes, now());
    kit::set_value(&s.kit, info, "last_vault", &id);
    match std::mem::take(&mut s.start_screen).as_str() {
        "edit" => {
            session.reading = Reading::Edit(crate::session::Form::new_item(
                crate::vault::Kind::Login,
                now(),
            ))
        }
        "generator" => session.reading = Reading::Generator,
        "audit" => session.reading = Reading::Audit(crate::audit::Filter::All),
        "import" => session.reading = Reading::Import(Default::default()),
        _ => {}
    }
    s.session = Some(session);
    if let Some(path) = s.import_files.first().cloned() {
        s.import_files.clear();
        crate::ui::read_import_file(s, info, app, &path);
    }
}

/// Saves the vault when it changed (one save at a time: a change during a save saves again
/// after it).
pub fn save(s: &mut KeysApp, info: &mut CallbackInfo, app: &RefAny) {
    let Some(session) = s.session.as_mut() else {
        return;
    };
    if !session.dirty {
        return;
    }
    if s.saving {
        s.save_again = true;
        return;
    }
    session.dirty = false;
    s.saving = true;
    let work = Work::Save {
        key: session.open.key.clone(),
        envelope: session.open.envelope.clone(),
        vault_key: session.open.vault_key.clone(),
        name: session.open.vault.name.clone(),
        json: session.open.vault.to_json(),
    };
    spawn(info, app, &s.data_root, work);
}

/// After a save landed (or failed): a lock or a close that waited for it.
fn after_save(s: &mut KeysApp, info: &mut CallbackInfo) {
    if s.saving {
        return;
    }
    if s.lock_pending {
        finish_lock(s, info);
    }
    if s.close_after_save {
        s.close_after_save = false;
        info.close_window();
    }
}

/// Locks the vault: a change is saved first (the lock waits for the save), then every string
/// of the vault is overwritten and the unlock screen shows.
pub fn lock(s: &mut KeysApp, info: &mut CallbackInfo, app: &RefAny) {
    if s.session.is_none() {
        return;
    }
    save(s, info, app);
    if s.saving {
        s.lock_pending = true;
        s.screen = Screen::Unlock;
        s.busy = Some("Saving, then locking\u{2026}".to_string());
        return;
    }
    finish_lock(s, info);
}

fn finish_lock(s: &mut KeysApp, info: &mut CallbackInfo) {
    if let Some(mut session) = s.session.take() {
        session.wipe();
    }
    if s.clipboard.remaining(now()).is_some() {
        set_clipboard(info, "");
        s.clipboard.cleared();
    }
    s.lock_pending = false;
    s.busy = None;
    s.screen = Screen::Unlock;
    println!("AZKEYS_LOCKED");
}

// ==== The clipboard ====

/// Puts `text` on the clipboard ("" clears it).
pub fn set_clipboard(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
}

/// Copies a secret of the selected item (`field` as [`Session::copy_text`] names it) and starts
/// the clipboard countdown.
pub fn copy_secret(s: &mut KeysApp, info: &mut CallbackInfo, field: &str) {
    let t = now();
    let Some((text, label)) = s
        .session
        .as_ref()
        .and_then(|session| session.copy_text(field, t))
    else {
        return;
    };
    let text = Zeroizing::new(text);
    set_clipboard(info, &text);
    s.clipboard.clear_after = s.settings.clear_seconds;
    s.clipboard.copied(&text, &label, t);
    s.notice = match s.clipboard.remaining(t) {
        Some(secs) => format!("Copied the {label}; the clipboard clears in {secs} s"),
        None => format!("Copied the {label}"),
    };
    println!("AZKEYS_COPIED {label}");
}

// ==== The timer ====

/// Starts the one-second timer (once).
pub fn start_timer(s: &mut KeysApp, info: &mut CallbackInfo, app: &RefAny) {
    if s.timer_started {
        return;
    }
    s.timer_started = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(1000))),
    );
}

/// Whether the window shows something that changes every second right now: a one-time code's
/// seconds, the clipboard countdown, the last minute before the idle lock, the wait after wrong
/// passwords. An item being edited is not rebuilt under the user's typing.
fn ticking(s: &KeysApp, t: u64) -> bool {
    if s.screen == Screen::Unlock || s.screen == Screen::Create {
        return s.attempts.wait(t) > 0 || s.attempts.wait(t.saturating_sub(1)) > 0;
    }
    let Some(session) = s.session.as_ref() else {
        return false;
    };
    if matches!(session.reading, Reading::Edit(_) | Reading::Generator) {
        return false;
    }
    let code_shown = session.selected_item().is_some_and(|i| !i.totp.is_empty())
        || session.scope == crate::vault::Scope::OneTimeCodes;
    code_shown
        || s.clipboard.remaining(t).is_some()
        || s.auto_lock.remaining(t).is_some_and(|r| r <= 60)
}

extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<KeysApp>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let s = &mut *guard;
    let t = now();
    let mut refresh = false;
    if s.clipboard.due(t) {
        set_clipboard(&mut info.callback_info, "");
        s.clipboard.cleared();
        s.notice = "The clipboard is cleared.".to_string();
        println!("AZKEYS_CLIPBOARD_CLEARED");
        refresh = true;
    }
    if s.session.is_some() && !s.lock_pending && s.auto_lock.due(t) {
        println!("AZKEYS_AUTO_LOCK");
        lock(s, &mut info.callback_info, &app);
        refresh = true;
    }
    if refresh || ticking(s, t) {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

// ==== The keyring and the biometric prompt ====

/// Sends a keyring request when none is on the way; `false` (and a notice) when one is.
fn keyring(s: &mut KeysApp, info: &mut CallbackInfo, op: KeyringOp, call: KeyringCall) -> bool {
    if s.keyring_waiting.is_some() {
        s.notice = "The system keyring is still answering; try again in a moment.".to_string();
        return false;
    }
    match &call {
        KeyringCall::Get(name) => info.keyring_get(name.as_str()),
        KeyringCall::Store(name, secret, biometry) => {
            info.keyring_store(name.as_str(), secret.as_str(), *biometry);
        }
        KeyringCall::Delete(name) => info.keyring_delete(name.as_str()),
    }
    s.keyring_waiting = Some(op);
    true
}

/// A keyring request. Holds a secret while it is made; never printed.
enum KeyringCall {
    Get(String),
    Store(String, Zeroizing<String>, bool),
    Delete(String),
}

/// Lets this device unlock the open vault without its password: its vault key goes to the OS
/// keyring, behind the OS biometric prompt where there is one.
pub fn enable_device_unlock(s: &mut KeysApp, info: &mut CallbackInfo) {
    let biometric = !matches!(info.get_biometric_kind(), BiometricKind::NotAvailable);
    let mode = if biometric {
        DeviceUnlock::Biometric
    } else {
        DeviceUnlock::Keyring
    };
    store_device_key(s, info, mode);
}

fn store_device_key(s: &mut KeysApp, info: &mut CallbackInfo, mode: DeviceUnlock) {
    let Some(session) = s.session.as_ref() else {
        return;
    };
    let vault_id = session.open.vault.id.clone();
    let secret = session.open.vault_key.to_base64();
    let call = KeyringCall::Store(
        store::keyring_name(&vault_id),
        secret,
        mode == DeviceUnlock::Biometric,
    );
    keyring(s, info, KeyringOp::Store { vault_id, mode }, call);
}

/// This device stops unlocking the open vault: its key leaves the keyring.
pub fn disable_device_unlock(s: &mut KeysApp, info: &mut CallbackInfo) {
    let Some(vault_id) = s
        .session
        .as_ref()
        .map(|session| session.open.vault.id.clone())
    else {
        return;
    };
    let call = KeyringCall::Delete(store::keyring_name(&vault_id));
    keyring(s, info, KeyringOp::Delete { vault_id }, call);
}

/// "Unlock with Touch ID" (or the keyring): reads the picked vault's key from the keyring - the
/// OS shows its biometric prompt for a key bound to biometry - or asks azul's prompt first.
pub fn unlock_with_device(s: &mut KeysApp, info: &mut CallbackInfo) {
    let Some(v) = s.chosen_vault() else {
        return;
    };
    let (vault_id, key, name) = (v.id().to_string(), v.key.clone(), v.name().to_string());
    match s.device_unlock(&vault_id) {
        Some(DeviceUnlock::Prompt) => {
            s.biometric_for = Some((vault_id, key));
            info.request_biometric_auth(BiometricPrompt {
                reason: AzString::from(format!("Unlock the vault \u{201c}{name}\u{201d}")),
                cancel_label: AzString::from("Use the password"),
                allow_device_credential: true,
            });
        }
        Some(_) => {
            let call = KeyringCall::Get(store::keyring_name(&vault_id));
            if keyring(s, info, KeyringOp::Get { vault_id, key }, call) {
                s.busy = Some("Waiting for the system keyring\u{2026}".to_string());
            }
        }
        None => {
            s.unlock.message = "This device keeps no key of this vault yet: unlock it with the \
                                master password, then turn on device unlock in the settings."
                .to_string();
        }
    }
}

fn keyring_problem(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::NotFound => "the keyring has no key of this vault",
        KeyringResult::Denied => "the prompt was cancelled or refused",
        KeyringResult::Unavailable => "no system keyring is available",
        _ => "the keyring reported an error",
    }
}

/// The keyring answered the request on the way.
pub extern "C" fn on_keyring_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(result) = info.get_keyring_result().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<KeysApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(op) = s.keyring_waiting.take() else {
        return Update::DoNothing;
    };
    match op {
        KeyringOp::Store { vault_id, mode } => match &result {
            KeyringResult::Stored => {
                kit::set_value(
                    &s.kit,
                    &mut info,
                    &device_unlock_key(&vault_id),
                    mode.name(),
                );
                s.notice = match mode {
                    DeviceUnlock::Keyring => {
                        "This device now unlocks the vault from the system keyring."
                    }
                    _ => "This device now unlocks the vault with biometrics.",
                }
                .to_string();
                println!("AZKEYS_DEVICE_UNLOCK {vault_id} {}", mode.name());
            }
            // A keychain that cannot bind the item to biometry (an unsigned build): keep the key
            // without the binding and ask azul's biometric prompt before reading it.
            KeyringResult::Error | KeyringResult::Unavailable
                if mode == DeviceUnlock::Biometric =>
            {
                store_device_key(s, &mut info, DeviceUnlock::Prompt);
            }
            other => {
                s.notice = format!("Device unlock is not on: {}.", keyring_problem(other));
            }
        },
        KeyringOp::Get { vault_id, key } => {
            s.busy = None;
            match &result {
                KeyringResult::Retrieved(secret) => match SecretKey::from_base64(secret.as_str()) {
                    Some(vault_key) => {
                        s.busy = Some("Unlocking\u{2026}".to_string());
                        spawn(
                            &mut info,
                            &app,
                            &s.data_root,
                            Work::UnlockWithKey { key, vault_key },
                        );
                    }
                    None => {
                        s.unlock.message =
                            "The keyring's entry is not a vault key. Unlock with the master password."
                                .to_string();
                    }
                },
                KeyringResult::NotFound => {
                    kit::set_value(&s.kit, &mut info, &device_unlock_key(&vault_id), "");
                    s.unlock.message = "This device no longer keeps the key of this vault. Unlock \
                                        it with the master password."
                        .to_string();
                }
                other => {
                    s.unlock.message =
                        format!("Device unlock did not work: {}.", keyring_problem(other));
                }
            }
        }
        KeyringOp::Delete { vault_id } => {
            kit::set_value(&s.kit, &mut info, &device_unlock_key(&vault_id), "");
            s.notice = "This device no longer unlocks the vault without its password.".to_string();
            println!("AZKEYS_DEVICE_UNLOCK {vault_id} off");
        }
    }
    Update::RefreshDom
}

/// azul's biometric prompt answered (device unlock in `prompt` mode).
pub extern "C" fn on_biometric_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(result) = info.get_biometric_result().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<KeysApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some((vault_id, key)) = s.biometric_for.take() else {
        return Update::DoNothing;
    };
    match result {
        BiometricResult::Authenticated | BiometricResult::FellBackToPasscode => {
            let call = KeyringCall::Get(store::keyring_name(&vault_id));
            if keyring(s, &mut info, KeyringOp::Get { vault_id, key }, call) {
                s.busy = Some("Waiting for the system keyring\u{2026}".to_string());
            }
        }
        BiometricResult::Cancelled => {
            s.unlock.message = "Cancelled: unlock with the master password.".to_string();
        }
        _ => {
            s.unlock.message =
                "The biometric check did not pass: unlock with the master password.".to_string();
        }
    }
    Update::RefreshDom
}

// ==== The window ====

/// The window exists: the kit's `--shot`, the vault listing, the timer.
pub extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<KeysApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    kit::on_window_created(&s.kit, &mut info);
    spawn(&mut info, &app, &s.data_root, Work::List);
    start_timer(s, &mut info, &app);
    Update::DoNothing
}

/// A close: held while a save is on the way or due (the window closes when it lands). An
/// edited item is the close guard's question (`ui.rs`).
pub extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<KeysApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.has_unsaved_form() {
        return Update::DoNothing;
    }
    save(s, &mut info, &app);
    if s.saving {
        info.prevent_window_close();
        s.close_after_save = true;
        s.busy = Some("Saving before closing\u{2026}".to_string());
        return Update::RefreshDom;
    }
    // Closing: the vault's strings are overwritten before the process ends.
    if let Some(mut session) = s.session.take() {
        session.wipe();
    }
    Update::DoNothing
}

/// Input in the window: the idle lock starts over.
pub extern "C" fn on_activity(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<KeysApp>() {
        s.touch(now());
    }
    Update::DoNothing
}
