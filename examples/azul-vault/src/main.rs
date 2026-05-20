//! AzulVault — P4 goal app (SUPER_PLAN_2 §4 P4.4).
//!
//! A biometric-gated key/value store, persisted to a local SQLite file via
//! the public `Db` API. On launch the vault is locked; the Unlock button
//! drives the OS biometric prompt (`CallbackInfo::request_biometric_auth`)
//! and `get_biometric_result()` unlocks it once the user authenticates.
//! Ties P4.1 (biometric) + P4.3 (db-sqlite) together using only the public
//! `azul::` api.json surface.
//!
//! Async caveat (same as AzulMaps' locate): the OS prompt resolves on
//! another thread, so the result is polled on the next Unlock tap — a
//! Timer-driven auto-unlock is a follow-up. This first cut adds + counts
//! sample entries; custom key/value text input + a listing view (which
//! needs the `DbRows`/`DbValue` accessor methods exposed) are P4.4b.

use azul::prelude::*;
use azul::error::BiometricResult;
use azul::misc::{BiometricPrompt, Db, DbValue};

struct VaultState {
    /// SQLite file path — the vault persists here across runs. Stored as a
    /// plain `String`; converted to `AzString` at the `Db::open` call via
    /// `.into()` (so no engine handle lives in `RefAny`).
    db_path: String,
    /// Set once the user authenticates; gates the entry UI.
    unlocked: bool,
    /// Transient message shown on the locked screen.
    status: String,
    /// Entries inserted this session (the file itself persists more).
    added_count: usize,
}

impl VaultState {
    fn new() -> Self {
        let path = std::env::temp_dir().join("azul-vault.db");
        Self {
            db_path: path.to_string_lossy().into_owned(),
            unlocked: false,
            status: String::new(),
            added_count: 0,
        }
    }
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    font-family: sans-serif; background: #14141c;";
const LOCKED: &str = "flex-grow: 1; display: flex; flex-direction: column; \
    align-items: center; justify-content: center; color: white;";
const TITLE: &str = "font-size: 28px; margin-bottom: 8px;";
const STATUS: &str = "color: #9aa0b4; font-size: 14px; margin-bottom: 16px; \
    min-height: 18px;";
const HEADER: &str = "background: #2b2b3c; color: white; padding: 14px 18px; \
    font-size: 18px;";
const BODY: &str = "flex-grow: 1; padding: 18px; color: #e6e6f0;";
const BTN: &str = "background: #4a90e2; color: white; padding: 12px 22px; \
    margin: 6px; border-radius: 8px; font-size: 15px; cursor: pointer; \
    text-align: center;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (unlocked, status, count) = match data.downcast_ref::<VaultState>() {
        Some(s) => (s.unlocked, s.status.clone(), s.added_count),
        None => (false, String::new(), 0),
    };

    if !unlocked {
        return Dom::create_body().with_child(
            Dom::create_div()
                .with_css(LOCKED)
                .with_child(Dom::create_text("🔒 AzulVault").with_css(TITLE))
                .with_child(Dom::create_text(status.as_str()).with_css(STATUS))
                .with_child(
                    Dom::create_div()
                        .with_css(BTN)
                        .with_child(Dom::create_text("Unlock with biometrics"))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseUp),
                            data.clone(),
                            on_unlock,
                        ),
                ),
        );
    }

    let summary = format!("{} entr{} added this session", count, if count == 1 { "y" } else { "ies" });
    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_text("🔓 AzulVault — unlocked").with_css(HEADER))
            .with_child(
                Dom::create_div()
                    .with_css(BODY)
                    .with_child(Dom::create_text(summary.as_str()))
                    .with_child(
                        Dom::create_div()
                            .with_css(BTN)
                            .with_child(Dom::create_text("Add sample entry"))
                            .with_callback(
                                EventFilter::Hover(HoverEventFilter::MouseUp),
                                data.clone(),
                                on_add,
                            ),
                    ),
            ),
    )
}

/// Unlock button. Polls for a completed biometric auth (the OS prompt
/// resolves asynchronously); if none yet, fires a fresh request.
extern "C" fn on_unlock(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(result) = info.get_biometric_result().into_option() {
        match result {
            BiometricResult::Authenticated | BiometricResult::FellBackToPasscode => {
                if let Some(mut s) = data.downcast_mut::<VaultState>() {
                    s.unlocked = true;
                    s.status = String::new();
                    // Ensure the table exists (idempotent).
                    let db = Db::open(s.db_path.clone());
                    let _ = db.execute(
                        "CREATE TABLE IF NOT EXISTS entries \
                         (id INTEGER PRIMARY KEY, k TEXT, v TEXT)",
                        Vec::<DbValue>::new(),
                    );
                }
                return Update::RefreshDom;
            }
            other => {
                if let Some(mut s) = data.downcast_mut::<VaultState>() {
                    s.status = match other {
                        BiometricResult::Unavailable => {
                            "Biometrics unavailable on this device.".to_string()
                        }
                        BiometricResult::Cancelled => {
                            "Cancelled — tap to try again.".to_string()
                        }
                        _ => "Authentication failed — tap to try again.".to_string(),
                    };
                }
                return Update::RefreshDom;
            }
        }
    }

    // No result yet → request the OS prompt. The user approves it, then
    // taps Unlock again to complete (polled above).
    let prompt = BiometricPrompt {
        reason: "Unlock your vault".into(),
        cancel_label: "".into(),
        allow_device_credential: true,
    };
    info.request_biometric_auth(prompt);
    if let Some(mut s) = data.downcast_mut::<VaultState>() {
        s.status = "Authenticating… approve the prompt, then tap Unlock again.".to_string();
    }
    Update::RefreshDom
}

/// Insert a sample secret via the public `Db` API.
extern "C" fn on_add(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<VaultState>() {
        let n = s.added_count + 1;
        let db = Db::open(s.db_path.clone());
        let affected = db.execute(
            "INSERT INTO entries (k, v) VALUES (?, ?)",
            vec![
                DbValue::Text(format!("entry-{}", n).into()),
                DbValue::Text(format!("secret-value-{}", n).into()),
            ],
        );
        if affected > 0 {
            s.added_count = n;
        }
    }
    Update::RefreshDom
}

fn main() {
    let data = RefAny::new(VaultState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
