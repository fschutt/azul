//! System-tray demo.
//!
//! ```sh
//! cargo run --release -p azul-examples --example tray
//! ```
//!
//! **macOS only for now.** Windows and Linux report the tray as unavailable;
//! this demo says so and keeps running, which is the behaviour every app needs
//! anyway - on a vanilla GNOME there is genuinely no tray to talk to.
//!
//! Everything here goes through the ordinary public API (`azul::tray::*`,
//! `App::set_tray`), i.e. the same C ABI every language binding sees. Nothing
//! reaches into the crate internals.
//!
//! What it shows:
//!
//! * a tray icon whose image is an **icon-registry spec** - the same string an
//!   `<icon>` node takes, resolved through the same pass, so any registered
//!   icon works (Material Icons is the default pack) with no tray-specific
//!   icon path;
//! * a tray **menu**, built from the ordinary `Menu` type the menu bar uses.
//!
//! Note the menu is *state*: it is set once as part of `TrayIconData`, not
//! shown on demand. That shape is forced by Linux, where the panel draws the
//! menu itself and calls back asking for the layout.

use azul::menu::{Menu, MenuItem, StringMenuItem};
use azul::prelude::*;
use azul::tray::TrayIconData;

struct TrayDemo {
    available: bool,
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    padding: 24px; background: #fafafa; font-family: sans-serif;";
const TITLE: &str = "font-size: 20px; color: #111; margin-bottom: 8px;";
const OK: &str = "font-size: 13px; color: #228822; margin-bottom: 14px;";
const WARN: &str = "font-size: 13px; color: #bb0000; margin-bottom: 14px;";
const HINT: &str = "font-size: 13px; color: #555555;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let available = data
        .downcast_ref::<TrayDemo>()
        .map(|d| d.available)
        .unwrap_or(false);

    let (status_text, status_css) = if available {
        ("Tray available - look in the menu bar.", OK)
    } else {
        (
            "No system tray on this platform yet - the app runs regardless.",
            WARN,
        )
    };

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_div_with_text("System tray demo").with_css(TITLE))
            .with_child(Dom::create_div_with_text(status_text).with_css(status_css))
            .with_child(
                Dom::create_div_with_text(
                    "Click the icon to open its menu. Menu clicks are logged to stdout.",
                )
                .with_css(HINT),
            ),
    )
}

fn main() {
    println!("azul - system tray demo");
    println!("=======================");

    // The icon is an icon-registry SPEC, not a bitmap: "settings" resolves
    // through the same registry and resolver an `<icon>settings</icon>` node
    // uses, then renders to RGBA at whatever size the platform asks for.
    // Try any other Material Icons name - "home", "favorite", "cloud".
    let tray = TrayIconData::new("rs.azul.tray-demo", "Azul Tray Demo")
        .with_named_icon("settings")
        .with_tooltip("Azul tray demo")
        .with_menu(Menu::create(vec![
            MenuItem::String(StringMenuItem::create("Open")),
            MenuItem::Separator,
            MenuItem::String(StringMenuItem::create("Quit")),
        ]));

    let data = RefAny::new(TrayDemo {
        // set_tray is best-effort and never fails the app; this is read only so
        // the window can be honest about whether an icon actually appeared.
        available: cfg!(target_os = "macos"),
    });

    let mut app = App::create(data, AppConfig::create());
    app.set_tray(tray);

    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "Azul - tray demo".into();
    app.run(window);
}
