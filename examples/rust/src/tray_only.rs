use azul::{
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
    tray::TrayIconData,
};

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

    let tray = TrayIconData::create("rs.azul.tray-only", "Azul Tray Only")
        .with_named_icon("bolt")
        .with_tooltip("Azul tray-only demo")
        .with_menu(Menu::create(vec![
            MenuItem::String(StringMenuItem::create("Ping").with_callback(data.clone(), on_ping)),
            MenuItem::Separator,
            MenuItem::String(StringMenuItem::create("Quit").with_callback(data.clone(), on_quit)),
        ]));

    let mut app = App::create(data, AppConfig::create());
    app.set_tray(tray);

    app.run_tray_only();
}
