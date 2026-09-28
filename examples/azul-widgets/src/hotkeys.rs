//! The "Global hotkey" section: a SYSTEM-WIDE key combination - Cmd+Shift+K
//! on a Mac, Ctrl+Alt+K elsewhere - that counts its presses and brings this
//! window to the front, even while another app has the keyboard.
//!
//! Declared FROM STATE, like the rest of the UI: `layout()` declares the
//! hotkey while `enabled` is set (`LayoutCallbackInfo::add_global_hotkey`,
//! the shape of `with_callback`), and the engine grabs, keeps or releases it
//! to match. The button only flips `enabled`; there is no id to keep and no
//! unregister call to forget. The status line reads the status IN
//! `layout()`, so it updates by itself when the answer arrives (a Wayland
//! desktop's approval dialog, another app taking the combination).
//!
//! Off at startup, so a scripted or headless run of the showcase never grabs
//! a real key by surprise. The capability line says whether this desktop can
//! do it at all (a Wayland desktop needs the `GlobalShortcuts` portal; iOS,
//! Android and the web cannot).
//!
//! Headless / AZ_E2E: click "Enable Ctrl+Alt+K", then
//! `{ "op": "global_hotkey", "accelerator": "Ctrl+Alt+K" }` presses it
//! (`Cmd+Shift+K` on a Mac host). `{ "op": "global_hotkey_answer", ...,
//! "answer": "taken" }` before enabling makes the status read "taken" and
//! shows the Retry button.

use azul::{
    app::{GlobalHotkey, GlobalHotkeyStatus, HotkeyModifiers},
    dom::VirtualKeyCode,
    prelude::*,
    widgets::Button,
    window::PlatformCapability,
};

use super::{labelled, section, Showcase};

/// What the section derives from; lives in `Showcase::hotkey`.
#[derive(Clone, Default)]
pub struct HotkeyDemo {
    /// The app WANTS the hotkey: `layout()` declares it while this is set.
    pub enabled: bool,
    /// How often the hotkey fired since it was enabled.
    pub fired: usize,
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

/// Enable / disable: flip the state; `layout()` declares (or stops
/// declaring) and the engine grabs (or releases) to match.
extern "C" fn on_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Showcase>() else {
        return Update::DoNothing;
    };
    s.interactions += 1;
    s.hotkey.enabled = !s.hotkey.enabled;
    if s.hotkey.enabled {
        s.hotkey.fired = 0;
    }
    Update::RefreshDom
}

/// A failure is sticky - an ordinary relayout never asks the OS (or shows a
/// Wayland dialog) again - until the app asks for a retry.
extern "C" fn on_retry(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.interactions += 1;
    }
    info.retry_global_hotkey(demo_hotkey());
    Update::RefreshDom
}

fn text(s: String) -> Dom {
    Dom::create_span_with_text(s).with_css("color: system:text;")
}

/// The section. Called from `layout()`, which is where the hotkey is
/// declared: `info` is that layout's info. Reads the capability on every
/// layout: the probe is cached after the first call.
pub fn hotkey_section(data: &RefAny, demo: &HotkeyDemo, info: &LayoutCallbackInfo) -> Dom {
    let label = demo_hotkey_label();

    // The hotkey is part of what this state renders - declared here, the
    // way a button's `with_on_click` is. Not declaring it releases it.
    if demo.enabled {
        info.add_global_hotkey_with_description(
            demo_hotkey(),
            "Bring AzWidgets to the front".into(),
            data.clone(),
            on_hotkey,
        );
    }
    // Read in layout(): when the status moves, this layout runs again by
    // itself, so the line below is never stale.
    let status = info.get_global_hotkey_status(demo_hotkey());

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

    let toggle = Button::create(if demo.enabled {
        format!("Disable {label}")
    } else {
        format!("Enable {label}")
    })
    .with_on_click(data.clone(), on_toggle)
    .dom();

    let (status_line, failed) = match status {
        GlobalHotkeyStatus::Active => (
            format!(
                "{label} is active - press it from any app to bump the counter and bring this \
                 window to the front"
            ),
            false,
        ),
        GlobalHotkeyStatus::Pending => (
            format!("{label}: waiting for the desktop to confirm the shortcut"),
            false,
        ),
        GlobalHotkeyStatus::Failed(e) => (
            format!("{label}: {}", e.to_display_string().as_str()),
            true,
        ),
        GlobalHotkeyStatus::NotRegistered => (format!("{label} is off"), false),
    };

    let mut rows = vec![
        labelled("Capability", text(capability_line)),
        labelled("Hotkey", toggle),
        labelled("Status", text(status_line)),
    ];
    if failed {
        rows.push(labelled(
            "Refused",
            Button::create(format!("Retry {label}"))
                .with_on_click(data.clone(), on_retry)
                .dom(),
        ));
    }
    rows.push(labelled(
        "Fired",
        text(format!("{} time(s) since it was enabled", demo.fired)),
    ));
    section("Global hotkey", rows)
}
