//! AzKeys: the password manager of the Azlin apps (the plan:
//! azul-apps/planning/core/password-manager.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`crypto`]: the vault file - Argon2id derives the password key that seals a random vault
//!   key, XChaCha20-Poly1305 seals the vault's JSON with it;
//! - [`vault`]: the items (logins, cards, notes, identities, SSH keys), search, scopes, sections;
//! - [`totp`]: one-time codes (RFC 4226 / 6238), otpauth URLs, base32;
//! - [`import`]: CSV from browsers and password managers, Bitwarden JSON;
//! - [`generator`]: passwords, passphrases ([`words`]) and PINs, the strength estimate;
//! - [`clipboard`]: the clipboard guard (a copied secret is cleared after a while);
//! - [`lock`]: the idle lock and the wait after wrong master passwords;
//! - [`audit`]: weak, reused and old passwords, logins without a one-time code;
//! - [`store`]: the files in the data tree and the work on them (list, create, unlock, save);
//! - [`sample`]: the `--sample` vault;
//! - [`session`]: an unlocked vault in the window (selection, edit form, reading pane).
//!
//! The window ([`app`]: facts, settings, state; `jobs`: the vault thread, the timer, the
//! keyring; `ui`: the screens) is azul's S4 PimShell with azul-appkit's skeleton.

pub mod app;
pub mod audit;
pub mod clipboard;
pub mod crypto;
pub mod generator;
pub mod ids;
pub mod import;
pub mod jobs;
pub mod lock;
pub mod sample;
pub mod session;
pub mod store;
pub mod totp;
pub mod ui;
pub mod ui_item;
pub mod vault;
pub mod words;

use azul::{app::App, callbacks::RefAny};
use azul_appkit::{args::AppArgs, ui as kit};

use crate::app::{now, KeysApp, Settings, ABOUT, APP_CATEGORIES, SHORTCUTS, SPEC};

/// Starts AzKeys: the switches, the kit (settings file, data root), the window. Bare arguments
/// are exports to import once a vault is unlocked.
pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.clone());
    // The device unlock is an action (the vault's key goes into the keyring or leaves it), not
    // a choice: Cancel on the settings page leaves its `device_unlock.<vault>` values alone.
    kit::keep_on_cancel(&kit_ref, &crate::app::device_unlock_key(""));
    let (data_root, settings) = {
        let mut k = kit_ref.clone();
        let found = k.downcast_ref::<kit::Kit>().map(|k| {
            let settings = Settings::from_values(|key| k.settings.get(key).map(str::to_string));
            (k.data_root.clone(), settings)
        });
        found.unwrap_or_default()
    };
    println!("AZKEYS_DATA {}", data_root.display());
    let screen = args.screen.clone().unwrap_or_default();
    match screen.as_str() {
        "settings" => kit::open_settings(&kit_ref, None),
        "shortcuts" => kit::open_settings(&kit_ref, Some("Shortcuts")),
        _ => {}
    }
    let state = KeysApp {
        kit: kit_ref.clone(),
        data_root,
        args: args.clone(),
        auto_lock: crate::lock::AutoLock::new(settings.idle_minutes, now()),
        clipboard: crate::clipboard::ClipboardGuard::new(settings.clear_seconds),
        settings,
        screen: app::Screen::Unlock,
        start_screen: screen,
        vaults: Vec::new(),
        listed: false,
        unlock: Default::default(),
        create: Default::default(),
        change: Default::default(),
        attempts: Default::default(),
        busy: None,
        session: None,
        nav_open: [true, true, true],
        notice: String::new(),
        saving: false,
        save_again: false,
        close_after_save: false,
        lock_pending: false,
        asking_close: false,
        keyring_waiting: None,
        biometric_for: None,
        timer_started: false,
        shown_tick: 0,
        import_files: args.files.clone(),
        sample: args.sample,
    };
    let config = kit::app_config(&kit_ref).with_app_id("org.azul.AzKeys");
    let window = kit::window_options(
        &kit_ref,
        ui::layout,
        (1180.0, 760.0),
        (720.0, 480.0),
        jobs::on_window_created,
    );
    App::create(RefAny::new(state), config).run(window);
}
