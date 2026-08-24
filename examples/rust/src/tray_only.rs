//! azul - a system-tray app with NO window at all.
//!
//! The ordinary `App::run()` creates a root window unconditionally, so a
//! menu-bar utility used to have to open (and then hide) a window it never
//! wanted. `run_tray_only()` skips window creation entirely.
//!
//! What that costs, and why it is deliberate:
//!
//! * On macOS the process switches to **Accessory** activation - the runtime
//!   equivalent of `LSUIElement` in an Info.plist. No Dock tile, no application
//!   menu bar. A `Regular` app with no windows is worse than having one: it owns
//!   the menu bar and shows a Dock icon that does nothing.
//! * Because there is no Dock tile, `set_app_icon` has nowhere to draw, so this
//!   example does not call it.
//!
//! Tray menu callbacks still work exactly as they do in a windowed app - the
//! same `RefAny`, the same `CallbackInfo` - because a headless stub window
//! supplies the callback context without any OS window existing.
//!
//! Run it and look in the menu bar; there is no window and no Dock icon.

use azul::menu::{Menu, MenuItem, StringMenuItem};
use azul::prelude::*;
use azul::tray::TrayIconData;

struct TrayOnly {
    clicks: usize,
}

extern "C" fn on_ping(mut data: RefAny, _info: CallbackInfo) -> Update {
    match data.downcast_mut::<TrayOnly>() {
        Some(mut d) => {
            d.clicks += 1;
            println!("[tray-only] ping ({} total) - no window involved", d.clicks);
            Update::DoNothing
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_quit(_data: RefAny, _info: CallbackInfo) -> Update {
    println!("[tray-only] quit");
    std::process::exit(0);
}

fn main() {
    println!("azul - tray-only demo (no window)");
    println!("=================================");
    println!("Look in the menu bar. There is no window and no Dock icon.");

    let data = RefAny::new(TrayOnly { clicks: 0 });

    let tray = TrayIconData::new("rs.azul.tray-only", "Azul Tray Only")
        .with_named_icon("bolt")
        .with_tooltip("Azul tray-only demo")
        .with_menu(Menu::create(vec![
            MenuItem::String(StringMenuItem::create("Ping").with_callback(data.clone(), on_ping)),
            MenuItem::Separator,
            MenuItem::String(StringMenuItem::create("Quit").with_callback(data.clone(), on_quit)),
        ]));

    let mut app = App::create(data, AppConfig::create());
    app.set_tray(tray);

    // NOT `app.run(window)` - there is no window.
    app.run_tray_only();
}
