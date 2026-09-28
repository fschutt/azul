//! The "Global hotkey" section: a SYSTEM-WIDE key combination - Cmd+Shift+K
//! on a Mac, Ctrl+Alt+K elsewhere - that counts its presses and brings this
//! window to the front, even while another app has the keyboard.
//!
//! Registered on a button press rather than at startup, so a scripted or
//! headless run of the showcase never grabs a real key by surprise. The
//! capability line says whether this desktop can do it at all (a Wayland
//! desktop needs the `GlobalShortcuts` portal; iOS, Android and the web
//! cannot).
//!
//! Headless / AZ_E2E: `{ "op": "global_hotkey", "accelerator": "Ctrl+Alt+K" }`
//! presses it once it is registered (`Cmd+Shift+K` on a Mac host).

use azul::{
    app::{GlobalHotkey, GlobalHotkeyId, GlobalHotkeyStatus, HotkeyModifiers},
    dom::VirtualKeyCode,
    error::ResultGlobalHotkeyIdGlobalHotkeyError,
    prelude::*,
    widgets::Button,
    window::PlatformCapability,
};

use super::{labelled, section, Showcase};

/// What the section shows; lives in `Showcase::hotkey`.
#[derive(Clone, Default)]
pub struct HotkeyDemo {
    /// The live registration's id, `0` while none.
    pub id: u32,
    /// How often the hotkey fired.
    pub fired: usize,
    /// Where the registration stands, as the platform last answered.
    pub status: String,
    /// Why the last registration failed; empty when it did not.
    pub error: String,
}

/// The platform's summon convention: Cmd leads on a Mac, where Cmd+Shift+K
/// is free system-wide; elsewhere Ctrl+Alt+K (Ctrl+Shift+K belongs to
/// browsers' dev tools, and a Ctrl+Alt chord is what desktops leave to apps).
fn demo_hotkey() -> GlobalHotkey {
    if cfg!(target_os = "macos") {
        GlobalHotkey {
            modifiers: HotkeyModifiers {
                ctrl: false,
                alt: false,
                shift: true,
                meta: true,
            },
            key: VirtualKeyCode::K,
        }
    } else {
        GlobalHotkey {
            modifiers: HotkeyModifiers {
                ctrl: true,
                alt: true,
                shift: false,
                meta: false,
            },
            key: VirtualKeyCode::K,
        }
    }
}

fn demo_hotkey_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+K"
    } else {
        "Ctrl+Alt+K"
    }
}

fn status_text(status: GlobalHotkeyStatus) -> (String, String) {
    match status {
        GlobalHotkeyStatus::Active => ("active".to_string(), String::new()),
        GlobalHotkeyStatus::Pending => (
            "waiting for the desktop to confirm the shortcut".to_string(),
            String::new(),
        ),
        GlobalHotkeyStatus::Failed(e) => (
            "refused".to_string(),
            e.to_display_string().as_str().to_string(),
        ),
        GlobalHotkeyStatus::NotRegistered => ("not registered".to_string(), String::new()),
    }
}

/// The hotkey fired - from any app. Count it and summon the window.
extern "C" fn on_hotkey(mut data: RefAny, mut info: CallbackInfo) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.hotkey.fired += 1;
            s.interactions += 1;
        }
        None => return Update::DoNothing,
    }
    // Every platform decides whether an app may take the focus; right after
    // its own hotkey fired, Windows and macOS let it.
    info.raise_window();
    Update::RefreshDom
}

/// Register / unregister the hotkey.
extern "C" fn on_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // The registration keeps its own handle on the app state, taken before
    // the state is borrowed below.
    let hotkey_data = data.clone();
    let Some(mut s) = data.downcast_mut::<Showcase>() else {
        return Update::DoNothing;
    };
    s.interactions += 1;
    if s.hotkey.id != 0 {
        let _ = info.unregister_global_hotkey(GlobalHotkeyId { id: s.hotkey.id });
        s.hotkey.id = 0;
        s.hotkey.status = "not registered".to_string();
        s.hotkey.error.clear();
        return Update::RefreshDom;
    }
    match info.register_global_hotkey(demo_hotkey(), hotkey_data, on_hotkey) {
        ResultGlobalHotkeyIdGlobalHotkeyError::Ok(id) => {
            s.hotkey.id = id.id;
            s.hotkey.fired = 0;
            let (status, error) = status_text(info.get_global_hotkey_status(id));
            s.hotkey.status = status;
            s.hotkey.error = error;
        }
        ResultGlobalHotkeyIdGlobalHotkeyError::Err(e) => {
            s.hotkey.status = "not registered".to_string();
            s.hotkey.error = e.to_display_string().as_str().to_string();
        }
    }
    Update::RefreshDom
}

fn text(s: String) -> Dom {
    Dom::create_span_with_text(s).with_css("color: system:text;")
}

/// The section. Reads the capability on every layout: the probe is cached
/// after the first call.
pub fn hotkey_section(data: &RefAny, demo: &HotkeyDemo) -> Dom {
    let label = demo_hotkey_label();
    let capability = PlatformCapability::global_hotkeys();
    let capability_line = if capability.available {
        format!("available - {}", capability.backend.as_str())
    } else {
        format!(
            "unavailable - {}: {}",
            capability.backend.as_str(),
            capability.reason.as_str()
        )
    };

    let registered = demo.id != 0;
    let button = Button::create(if registered {
        format!("Unregister {label}")
    } else {
        format!("Register {label}")
    })
    .with_on_click(data.clone(), on_toggle)
    .dom();

    let status_line = if !demo.error.is_empty() {
        format!("{label}: {}", demo.error)
    } else if registered {
        format!(
            "{label} is {} - press it from any app to bump the counter and bring this window \
             to the front",
            demo.status
        )
    } else {
        format!("{label} is not registered")
    };

    section(
        "Global hotkey",
        vec![
            labelled("Capability", text(capability_line)),
            labelled("Registration", button),
            labelled("Status", text(status_line)),
            labelled(
                "Fired",
                text(format!("{} time(s) since it was registered", demo.fired)),
            ),
        ],
    )
}
