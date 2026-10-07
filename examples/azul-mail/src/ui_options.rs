//! File > Options: the kit's settings page - Outlook 2010's Options dialog (Mail, General, Data,
//! Shortcuts, About; `azul_appkit::ui`) - in a window of its own, as Outlook opens its Options
//! dialog over the main window. OK keeps the changes, Cancel (Escape, the window's close
//! button) puts back the settings the dialog found; either closes the window, and the main
//! window stays as it was - File stays open behind the dialog when it was opened from there.
//!
//! The window shares the app's state like a message window: the Mail page's check boxes are the
//! View tab's switches (`ui_main::mail_options`), and every change shows in the main window at
//! once. Cancel restores settings.json, then the window's reload ([`reload`]) reads the
//! switches back into the app (`ui_main::read_view_settings`), so the main window shows them as
//! they were.
//!
//! Opened by File > Options, File > Help > Options and Keyboard Shortcuts, Ctrl/Cmd+, and F1 in
//! the main window, and by `--screen options` (over the main window; a `--shot` is this
//! window's). The window id is `azmail-options` (the debug server addresses it by it); stdout
//! says `AZMAIL_SETTINGS_OPEN <window id>` when it opens and `AZMAIL_SETTINGS_WINDOW_CLOSED
//! <window id>` when it is gone (the kit says `AZMAIL_SETTINGS_CLOSED ok|cancel`).

use azul::{
    option::OptionLogicalSize,
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    widgets::Titlebar,
    window::WindowDecorations,
};
use azul_appkit::ui as kit;

use crate::{args::Screen, ui_main, with_app, MailApp};

/// The Options window's id.
pub(crate) const OPTIONS_WINDOW_ID: &str = "azmail-options";
/// Its title (Outlook's "Outlook Options").
const TITLE: &str = "AzMail Options";

/// Opens File > Options at `category` (a category's name: "Mail", "Shortcuts") in its own
/// window; while that is open, it only moves to `category` (the caller redraws every window).
pub(crate) fn open(s: &mut MailApp, info: &mut CallbackInfo, category: &str) {
    if kit::open_settings_window(&s.kit, Some(category), OPTIONS_WINDOW_ID) {
        info.create_window(window());
        println!("AZMAIL_SETTINGS_OPEN {OPTIONS_WINDOW_ID}");
    }
}

/// The window: drawn by the app like the others (`NoTitle`, azul's `Titlebar`), the size of
/// Outlook's Options dialog.
fn window() -> WindowCreateOptions {
    let mut window = WindowCreateOptions::create(layout_options);
    window.window_state.window_id = AzString::from(OPTIONS_WINDOW_ID);
    window.window_state.title = AzString::from(TITLE);
    window.window_state.size.dimensions = LogicalSize::create(780.0, 580.0);
    window.window_state.size.min_dimensions =
        OptionLogicalSize::Some(LogicalSize::create(560.0, 420.0));
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(on_options_created)).into();
    window
}

/// The window is up: `--screen options --shot` photographs this window, not the main one.
extern "C" fn on_options_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, |s, _| {
        if s.screen == Screen::Options {
            kit::on_window_created(&s.kit, &mut info);
        }
        Update::DoNothing
    })
    .unwrap_or(Update::DoNothing)
}

/// The Options window's layout: the title row over the kit's page, with AzMail's Mail page
/// and the reload that reads the View tab's switches back after Cancel.
extern "C" fn layout_options(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<MailApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let page = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(Titlebar::create(TITLE).without_border_bottom().dom())
        .with_child(kit::settings_page_with_reload(
            &s.kit,
            ui_main::mail_options(s, &app),
            &app,
            reload,
        ));
    Dom::create_body()
        .with_css(crate::WINDOW_BODY_CSS)
        .with_child(
            ShellThemeScope::create(page)
                .with_accent(ShellThemeAccent::Blue)
                .dom(),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_options_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app,
            on_options_close_requested,
        )
}

/// Cancel put settings.json back: the View tab's switches and the zoom are read again (the
/// kit borrows nothing while this runs; neither does the callback that cancelled).
fn reload(app: &mut RefAny, _info: &mut CallbackInfo, settings: &azul_appkit::AppSettings) {
    if let Some(mut s) = app.downcast_mut::<MailApp>() {
        ui_main::read_view_settings(&mut s, settings);
    };
}

/// The window's keys are the kit's: Escape is Cancel (and closes the window), Ctrl/Cmd+, keeps
/// it, F1 moves to the shortcuts. The app is not borrowed while the kit runs (its Cancel calls
/// [`reload`]).
extern "C" fn on_options_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kit_ref = data.downcast_ref::<MailApp>().map(|s| s.kit.clone());
    kit_ref
        .and_then(|k| kit::handle_key(&k, &mut info))
        .unwrap_or(Update::DoNothing)
}

/// The window is closing - after OK or Cancel, or by its close button (which is Cancel): the
/// kit's page has no window any more, and every window shows the settings as they are now.
extern "C" fn on_options_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<MailApp>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    if !kit::settings_window_closed(&kit_ref, &mut info) {
        return Update::DoNothing;
    }
    println!("AZMAIL_SETTINGS_WINDOW_CLOSED {OPTIONS_WINDOW_ID}");
    Update::RefreshDomAllWindows
}
