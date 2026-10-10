//! `azul-bridge-tray`: `azul-bridge serve` with an item in the menu bar (macOS), the notification
//! area (Windows) or the status notifier (Linux): how the bridge is, its settings to copy into
//! other programs, its password, the login item, Quit. Built with the `tray` feature, which links
//! libazul (`AZ_LINK_PATH`):
//!
//! ```text
//! cargo build --release -p azul-bridge --features tray --bin azul-bridge-tray
//! azul-bridge-tray [--state-dir DIR] [--keyring os|file] [serve's options]
//! ```
//!
//! It takes `serve`'s command line, so "Start at login" (the login item of `autostart`, made with
//! this program) starts the tray at every login. The menu is made once, when the bridge has
//! started (or failed to: it then says why); what it copies is `azul_bridge::tray`'s.

use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

use azcloud_kit::bridge::BridgeSettings;
use azul::{
    dom::ClipboardContent,
    menu::{Menu, MenuItem, MenuItemState, StringMenuItem},
    option::OptionString,
    prelude::*,
    str::String as AzString,
    tray::TrayIconData,
    vec::StyledTextRunVec,
};
use azul_bridge::{
    autostart, cli,
    secrets::PASSWORD_ENTRY,
    tray::{self, Entry},
};

/// What the menu's entries reach.
struct Tray {
    state: PathBuf,
    settings: Option<BridgeSettings>,
    /// Read once at the start, as `serve` reads it; only ever put on the clipboard.
    password: Option<String>,
}

/// One entry's data.
struct EntryRef {
    tray: RefAny,
    entry: Entry,
}

fn clipboard(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
}

/// Makes the login item (`on`) or removes it; what happened, as a line.
fn login_item(on: bool, state: &Path) -> String {
    let Some(system) = autostart::System::current() else {
        return String::from("this system has no login items the bridge knows");
    };
    let (Some(home), Some(config)) = (dirs::home_dir(), dirs::config_dir()) else {
        return String::from("no home or config folder");
    };
    if on {
        let made = std::env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|binary| autostart::enable(system, &home, &config, &binary, state).map_err(|e| e.to_string()));
        match made {
            Ok(path) => format!("AZUL_BRIDGE_AUTOSTART on {}", path.display()),
            Err(e) => format!("the login item could not be made: {e}"),
        }
    } else {
        match autostart::disable(system, &home, &config) {
            Ok(_) => String::from("AZUL_BRIDGE_AUTOSTART off"),
            Err(e) => format!("the login item could not be removed: {e}"),
        }
    }
}

extern "C" fn on_entry(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut tray_ref, entry)) = data
        .downcast_ref::<EntryRef>()
        .map(|e| (e.tray.clone(), e.entry.clone()))
    else {
        return Update::DoNothing;
    };
    if entry == Entry::Quit {
        std::process::exit(0);
    }
    let Some(t) = tray_ref.downcast_ref::<Tray>() else {
        return Update::DoNothing;
    };
    match &entry {
        Entry::CopyPassword => {
            if let Some(password) = &t.password {
                clipboard(&mut info, password);
            }
        }
        Entry::StartAtLogin(on) => println!("{}", login_item(*on, &t.state)),
        other => {
            if let Some(text) = t.settings.as_ref().and_then(|s| tray::text_of(other, s)) {
                clipboard(&mut info, &text);
            }
        }
    }
    Update::DoNothing
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if !args.iter().any(|arg| arg == "serve") {
        args.push(String::from("serve"));
    }
    let options = match cli::parse_args(&args) {
        Ok(options) if options.command == "serve" => options,
        Ok(_) => {
            eprintln!("azul-bridge-tray serves; the other commands are azul-bridge's");
            std::process::exit(2);
        }
        Err(text) => {
            eprintln!("{text}\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    let opened = match cli::open_state(&options) {
        Ok(opened) => opened,
        Err(text) => {
            eprintln!("azul-bridge-tray: {text}");
            std::process::exit(1);
        }
    };
    let password = opened.secrets.get(PASSWORD_ENTRY).ok().flatten();
    let settings = BridgeSettings::load(&opened.state);
    let state = opened.state.clone();
    drop(opened);

    // `serve` beside the tray: a start that fails (a port taken, no drive) says so in the menu.
    let (sender, receiver) = mpsc::channel();
    let serve_options = options.clone();
    std::thread::spawn(move || {
        let outcome = cli::run(&serve_options);
        if let Err(why) = &outcome {
            eprintln!("azul-bridge-tray: {why}");
        }
        let _ = sender.send(outcome);
    });
    let serving: Result<(), String> = match receiver.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => Err(String::from("it stopped")),
        Ok(Err(why)) => Err(why),
        Err(_) => Ok(()),
    };
    let at_login = match (autostart::System::current(), dirs::home_dir(), dirs::config_dir()) {
        (Some(system), Some(home), Some(config)) => autostart::is_enabled(system, &home, &config),
        _ => false,
    };

    let data = RefAny::new(Tray {
        state,
        settings: settings.clone(),
        password: password.clone(),
    });
    let entries = tray::menu(
        settings.as_ref(),
        serving.as_ref().map(|_| ()).map_err(String::as_str),
        at_login,
        password.is_some(),
    );
    let items: Vec<MenuItem> = entries
        .into_iter()
        .map(|(label, entry)| match entry {
            Entry::Separator => MenuItem::Separator,
            Entry::Status => {
                let mut item = StringMenuItem::create(AzString::from(label));
                item.menu_item_state = MenuItemState::Greyed;
                MenuItem::String(item)
            }
            entry => MenuItem::String(StringMenuItem::create(AzString::from(label)).with_callback(
                RefAny::new(EntryRef {
                    tray: data.clone(),
                    entry,
                }),
                on_entry,
            )),
        })
        .collect();
    let tooltip = match &serving {
        Ok(()) => String::from("Azlin Bridge"),
        Err(why) => format!("Azlin Bridge: not serving ({why})"),
    };
    let tray = TrayIconData::create(AzString::from("io.azlin.bridge"), AzString::from("Azlin Bridge"))
        .with_named_icon(AzString::from("cloud"))
        .with_tooltip(AzString::from(tooltip))
        .with_menu(Menu::create(items));
    let mut app = App::create(data, AppConfig::create());
    if !app.is_tray_available() {
        eprintln!("azul-bridge-tray: this desktop shows no tray items; the bridge serves on without one");
    }
    app.set_tray(tray);
    app.run_tray_only();
}
