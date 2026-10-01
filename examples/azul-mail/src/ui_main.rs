//! The main window: Outlook 2010's mail view on the PIM shell, and the backstage.
//!
//! ```text
//! title row (azul's Titlebar: the window is NoTitle)
//! ribbon: File | Home | Send / Receive | Folder | View           (or the backstage, full width)
//! navigation pane | message list           | reading pane        | To-Do bar
//! (Favorites, the  | (search, Arrange By:   | (subject, sender,   | (calendar,
//!  accounts' trees,|  Date, grouped rows)   |  pictures bar, body)|  tasks)
//!  modules)        |                        |                     |
//! status bar: items, unread, filter, Send / Receive state
//! ```
//!
//! Everything is the toolkit's: `PimShell` (an `OfficeShell`), `Ribbon`, `Backstage`,
//! `ShellNavigationPane` (`TreeView` per account, unread counts as node badges),
//! `MessageList`, `ReadingPane` (+ `InfoBar`), `ToDoBar`, `StatusBar` (+ `StatusBarSync`),
//! inside a `ShellThemeScope`; the app theme (flat / flora) and the mode (light / dark) are
//! azul's, switched from the View tab.

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType,
        MessageListOnEventCallbackType, ReadingPaneOnEventCallbackType, ResumeCallbackType,
        RibbonOnTabClickCallbackType, ShellNavigationPaneOnEventCallbackType,
        ToDoBarOnEventCallbackType,
    },
    css::DarkLightMode,
    dialog::FileOpenResult,
    dom::VirtualKeyCode,
    http::{HttpBytesResult, HttpRequestConfig},
    image::{ImageDecodeResult, ImageRef, RawImage},
    option::OptionDarkLightMode,
    prelude::*,
    shells::{
        PimShell, ShellEmptyState, ShellNavigationGroup, ShellNavigationModule,
        ShellNavigationPane, ShellNavigationPaneEvent, ShellNavigationPaneEventKind,
        ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    widgets::{
        Backstage, BackstageNavItem, InfoBar, MessageList, MessageListEvent,
        MessageListEventKind, MessageRow, ReadingPane, ReadingPaneEvent, ReadingPaneEventKind,
        Ribbon, RibbonAppButton, RibbonButton, RibbonGroup, RibbonItem, RibbonTab, StatusBar,
        StatusBarSegment, StatusBarSync, StatusBarSyncKind, Titlebar, ToDoBar, ToDoBarEvent,
        ToDoBarEventKind, ToDoTask, TreeViewNode,
    },
};

use crate::{
    compose::ComposeKind,
    folders::Role,
    html,
    listing::{self, FolderNode, ListRow},
    message, ui_account, ui_compose, with_app, MailApp, SyncState, Task,
};

/// The backstage's pages (File).
pub(crate) const PAGE_INFO: usize = 0;
pub(crate) const PAGE_ADD_ACCOUNT: usize = 1;
pub(crate) const PAGE_SETTINGS: usize = 2;
pub(crate) const PAGE_ABOUT: usize = 3;
pub(crate) const PAGE_EXIT: usize = 4;
const BACKSTAGE_PAGES: [&str; 5] = ["Info", "Add Account", "Account Settings", "About", "Exit"];

/// Rows the list renders at once around what is in view (the list is virtualised).
const LIST_WINDOW: usize = 200;
/// Every row's height in the list.
const ROW_HEIGHT: usize = 48;

/// Plain-text lines shown at most.
const MAX_LINES: usize = 3000;
/// Quote bar colours by level (1, 2, 3, then again).
const QUOTE_COLOURS: [&str; 4] = ["#2f6db0", "#2e7d32", "#8e24aa", "#b36b00"];
/// The paper a plain-text mail is read on: white with dark text in either mode, like a mail
/// without dark rules (`html.rs`).
const PAPER: &str = "display: flex; flex-direction: column; padding: 12px 16px; \
                     background: #ffffff; color: #1a1a1a; font-size: 14px;";

// ==== The window ====

/// The main window's layout: the title row over the PIM shell (or the backstage).
pub(crate) extern "C" fn layout_main(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<MailApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let shell = match s.backstage {
        Some(page) => PimShell::create(Dom::create_div(), Dom::create_div(), Dom::create_div())
            .with_backstage(backstage(s, &app, page)),
        None => {
            let mut shell = PimShell::create(
                navigation_pane(s, &app),
                message_list(s, &app),
                reading_pane(s, &app),
            )
            .with_ribbon(ribbon(s, &app))
            .with_status_bar(status_bar(s, &app))
            .with_list_label("Message list")
            .with_navigation_ratio(0.2)
            .with_list_ratio(0.42);
            if s.show_todo {
                shell = shell.with_todo_bar(todo_bar(s, &app));
            }
            shell
        }
    };
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(title_row(s))
        .with_child(shell.dom());
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px;")
        .with_child(
            ShellThemeScope::create(column)
                .with_accent(ShellThemeAccent::Blue)
                .dom(),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::KeyringResult),
            app.clone(),
            crate::on_keyring_result,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app,
            on_main_key,
        )
}

/// The window's title row, drawn by azul (the window is `NoTitle`): Outlook's "Inbox -
/// ada@example.org - AzMail".
fn title_row(s: &MailApp) -> Dom {
    let folder = current_folder_label(s);
    let title = match (folder, s.current_account()) {
        (Some(folder), Some(account)) => format!("{folder} - {} - AzMail", account.email),
        (None, Some(account)) => format!("{} - AzMail", account.email),
        _ => String::from("AzMail"),
    };
    Titlebar::create(title).without_border_bottom().dom()
}

/// The shown folder's label ("Inbox", "Sent Items").
fn current_folder_label(s: &MailApp) -> Option<String> {
    let index = s.current?;
    let key = s.folder.as_ref()?;
    s.folders
        .get(index)?
        .iter()
        .find(|f| &f.key == key)
        .map(|f| listing::folder_label(f.role, &f.display))
}

/// The main window is up: open what `--screen compose` / `reply` asked for.
pub(crate) extern "C" fn on_main_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, |s, app| {
        match s.args.screen {
            crate::args::Screen::Compose => {
                ui_compose::open_compose(s, &mut info, app, ComposeKind::New);
            }
            crate::args::Screen::Reply => {
                // The newest message of the shown folder.
                let first = s.rows.iter().find_map(|row| match row {
                    ListRow::Message(uid) => Some(*uid),
                    ListRow::Group(_) => None,
                });
                if let Some(uid) = first {
                    let _ = s.open_message(uid);
                    ui_compose::open_compose(s, &mut info, app, ComposeKind::Reply);
                }
            }
            _ => {}
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// Window keys: Ctrl/Cmd+N new mail, Ctrl/Cmd+R reply, Ctrl/Cmd+Shift+R reply all, Ctrl/Cmd+F
/// forward, F9 Send / Receive, Escape leaves the backstage.
extern "C" fn on_main_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let Some(key) = keyboard.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    let primary = modifiers.ctrl || modifiers.meta;
    let action = match key {
        VirtualKeyCode::N if primary => Action::NewMail,
        VirtualKeyCode::R if primary && modifiers.shift => Action::ReplyAll,
        VirtualKeyCode::R if primary => Action::Reply,
        VirtualKeyCode::F if primary => Action::Forward,
        VirtualKeyCode::F9 => Action::SendReceive,
        VirtualKeyCode::Escape => Action::CloseBackstage,
        _ => return Update::DoNothing,
    };
    run_action(&mut data, &mut info, action)
}

// ==== The backstage (File) ====

fn backstage(s: &MailApp, app: &RefAny, page: usize) -> Dom {
    let content = match page {
        PAGE_ADD_ACCOUNT => ui_account::wizard_page(s, app),
        PAGE_SETTINGS => ui_account::settings_page(s, app),
        PAGE_ABOUT => about_page(s),
        _ => info_page(s, app),
    };
    let items: Vec<BackstageNavItem> = BACKSTAGE_PAGES
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let item = BackstageNavItem::create(*label);
            if i == PAGE_EXIT {
                item.with_gap_before()
            } else {
                item
            }
        })
        .collect();
    Backstage::create(items)
        .with_active_item(page)
        .with_content(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                     padding: 20px 32px; overflow-y: auto;",
                )
                .with_child(content),
        )
        .with_on_nav_select(app.clone(), on_backstage_nav as BackstageOnNavSelectCallbackType)
        .with_on_back(app.clone(), on_backstage_back as ButtonOnClickCallbackType)
        .dom()
}

fn heading(text: &str) -> Dom {
    Dom::create_span_with_text(text).with_css("font-size: 26px; margin-bottom: 16px;")
}

fn line(text: impl Into<AzString>) -> Dom {
    Dom::create_span_with_text(text).with_css("font-size: 13px; margin-top: 6px;")
}

/// File > Info: the accounts, and what can be done with them.
fn info_page(s: &MailApp, app: &RefAny) -> Dom {
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(heading("Account Information"));
    if s.accounts.is_empty() {
        page.add_child(line("No account yet."));
    }
    for (i, account) in s.accounts.iter().enumerate() {
        let current = Some(i) == s.current;
        let folders = s.folders.get(i).map_or(0, Vec::len);
        let unread: usize = s.folders.get(i).map_or(0, |list| list.iter().map(|f| f.unread).sum());
        page.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; margin-top: 10px;")
                .with_child(
                    Dom::create_span_with_text(if current {
                        format!("{} (shown)", account.sender())
                    } else {
                        account.sender()
                    })
                    .with_css("font-size: 15px; font-weight: bold;"),
                )
                .with_child(line(format!(
                    "IMAP {}:{} - {folders} folders, {unread} unread - {}",
                    account.imap.host,
                    account.imap.port,
                    crate::sending::describe(&crate::send::SendSettings::load(&s.root, &account.id))
                ))),
        );
    }
    let button = |label: &str, action: Action| {
        Button::create(label)
            .with_on_click(action_ref(app, action), on_action as ButtonOnClickCallbackType)
            .dom()
            .with_css("margin-right: 8px;")
    };
    page.with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin-top: 20px;")
            .with_child(button("Add Account", Action::AddAccount))
            .with_child(button("Account Settings", Action::AccountSettings))
            .with_child(button("Send/Receive All Folders", Action::SendReceive)),
    )
    .with_child(line(format!("Mail is kept in {}", s.root.display())))
}

/// File > About.
fn about_page(s: &MailApp) -> Dom {
    let keys = [
        "Ctrl+N  New E-mail",
        "Ctrl+R  Reply",
        "Ctrl+Shift+R  Reply All",
        "Ctrl+F  Forward",
        "F9  Send/Receive All Folders",
        "In a message: Ctrl+B / I / U  Bold, Italic, Underline; Ctrl+Enter  Send; Ctrl+S  Save",
    ];
    let mut page = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(heading("About AzMail"))
        .with_child(line(
            "A mail client on the azul toolkit: IMAP to files on this computer, sending \
             directly or through an SMTP server, the rich editor of azul.",
        ))
        .with_child(line(format!("Version {}", env!("CARGO_PKG_VERSION"))))
        .with_child(line(format!("AzMail folder: {}", s.root.display())))
        .with_child(
            Dom::create_span_with_text("Keyboard shortcuts")
                .with_css("font-size: 15px; font-weight: bold; margin-top: 18px;"),
        );
    for key in keys {
        page.add_child(line(key));
    }
    page
}

extern "C" fn on_backstage_nav(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, |s, _| {
        match index {
            PAGE_ADD_ACCOUNT => ui_account::open_wizard(s, None),
            PAGE_SETTINGS => ui_account::open_settings(s),
            PAGE_EXIT => {
                info.close_window();
            }
            page => {
                if s.editor.as_ref().is_some_and(|e| e.saving) {
                    return Update::DoNothing;
                }
                s.backstage = Some(page);
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

extern "C" fn on_backstage_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    run_action(&mut data, &mut info, Action::CloseBackstage)
}
