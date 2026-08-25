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
    /// Bumped by the tray menu, and rendered by the window - the point being
    /// that a tray callback mutates the SAME state the window draws from.
    clicks: usize,
}

/// Tray menu callbacks are ordinary azul callbacks: your `RefAny` comes back
/// as the first argument, and returning `Update::RefreshDom` re-runs layout.
extern "C" fn on_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<TrayDemo>() {
        Some(mut d) => {
            d.clicks += 1;
            println!("[tray] \"Open\" clicked ({} total)", d.clicks);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_quit(_data: RefAny, _info: CallbackInfo) -> Update {
    println!("[tray] \"Quit\" clicked - exiting");
    std::process::exit(0);
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    padding: 24px; background: #fafafa; font-family: sans-serif;";
const TITLE: &str = "font-size: 20px; color: #111; margin-bottom: 8px;";
const OK: &str = "font-size: 13px; color: #228822; margin-bottom: 14px;";
const WARN: &str = "font-size: 13px; color: #bb0000; margin-bottom: 14px;";
const HINT: &str = "font-size: 13px; color: #555555;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (available, clicks) = data
        .downcast_ref::<TrayDemo>()
        .map(|d| (d.available, d.clicks))
        .unwrap_or((false, 0));

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
            )
            // The count is what makes the wiring visible: it is bumped by a
            // callback fired from the TRAY, mutating the same RefAny this
            // window lays out from.
            .with_child(
                Dom::create_div_with_text(format!("\"Open\" clicked {clicks} time(s)"))
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
    let data = RefAny::new(TrayDemo {
        // set_tray is best-effort and never fails the app; this is read only so
        // the window can be honest about whether an icon actually appeared.
        available: cfg!(target_os = "macos"),
        clicks: 0,
    });

    let tray = TrayIconData::new("rs.azul.tray-demo", "Azul Tray Demo")
        .with_named_icon("settings")
        .with_tooltip("Azul tray demo")
        .with_menu(Menu::create(vec![
            // A tray menu item takes the SAME callback a menu-bar item takes:
            // your own RefAny plus a `CallbackInfo`. Without one the item is
            // still clickable, but the click only shows up as a `TrayEvent` for
            // you to poll - attaching a callback is what makes it act.
            MenuItem::String(
                StringMenuItem::create("Open").with_callback(data.clone(), on_open),
            ),
            MenuItem::Separator,
            MenuItem::String(
                StringMenuItem::create("Quit").with_callback(data.clone(), on_quit),
            ),
        ]));

    // A DOM registered as an icon. The colour lives HERE, with the icon, not as
    // a tint parameter threaded through every call site - which is the whole
    // reason icons resolve on the Dom before the cascade.
    let mut config = AppConfig::create();
    config.icon_provider.register_dom_icon(
        String::from("demo"),
        String::from("red-heart"),
        Dom::create_icon(String::from("favorite"))
            .with_css("color: #d7263d;"),
    );

    let mut app = App::create(data, config);
    app.set_tray(tray);
    // The Dock tile, from the same registry + pipeline as the tray icon. macOS
    // documents this as temporary: it is process-local and resets next launch.
    app.set_app_icon(String::from("red-heart"));

    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "Azul - tray demo".into();
    app.run(window);
}
