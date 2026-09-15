use azul::{
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
    tray::TrayIconData,
};

struct TrayDemo {
    available: bool,
    clicks: usize,
}

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

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; padding: 24px; \
                    background: #fafafa; font-family: sans-serif;";
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
            .with_child(
                Dom::create_div_with_text(format!("\"Open\" clicked {clicks} time(s)"))
                    .with_css(HINT),
            ),
    )
}

fn main() {
    println!("azul - system tray demo");
    println!("=======================");

    let data = RefAny::new(TrayDemo {
        available: false,
        clicks: 0,
    });

    let tray = TrayIconData::create("rs.azul.tray-demo", "Azul Tray Demo")
        .with_named_icon("red-heart")
        .with_tooltip("Azul tray demo")
        .with_menu(Menu::create(vec![
            MenuItem::String(StringMenuItem::create("Open").with_callback(data.clone(), on_open)),
            MenuItem::Separator,
            MenuItem::String(StringMenuItem::create("Quit").with_callback(data.clone(), on_quit)),
        ]));

    let mut config = AppConfig::create();
    config.icon_provider.register_dom_icon(
        String::from("demo"),
        String::from("red-heart"),
        Dom::create_icon(String::from("favorite")).with_css("color: #d7263d;"),
    );

    let mut app = App::create(data.clone(), config);
    let available = app.is_tray_available();
    if let Some(mut d) = data.clone().downcast_mut::<TrayDemo>() {
        d.available = available;
    }
    println!("[tray] tray available on this desktop: {available}");
    app.set_tray(tray);
    app.set_app_icon(String::from("red-heart"));

    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "Azul - tray demo".into();
    app.run(window);
}
