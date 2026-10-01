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

// ==== Actions (the ribbon, the backstage's buttons, the window's keys) ====

/// What a ribbon button, a backstage button or a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    NewMail,
    Reply,
    ReplyAll,
    Forward,
    Delete,
    Move,
    /// Quick Steps > Done: mark the selection read.
    Done,
    SendReceive,
    CancelSendReceive,
    ToggleRead,
    ToggleFlag,
    MarkAllRead,
    UnreadOnly,
    ReverseSort,
    ToggleNavigation,
    ToggleReading,
    ToggleTodo,
    PlainText,
    ThemeFlat,
    ThemeFlora,
    ModeLight,
    ModeDark,
    OpenFile,
    AddAccount,
    AccountSettings,
    CloseBackstage,
}

struct ActionRef {
    app: RefAny,
    action: Action,
}

fn action_ref(app: &RefAny, action: Action) -> RefAny {
    RefAny::new(ActionRef {
        app: app.clone(),
        action,
    })
}

extern "C" fn on_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action))
    else {
        return Update::DoNothing;
    };
    run_action(&mut app, &mut info, action)
}

/// The UIDs of the selected messages.
fn selected_uids(s: &MailApp) -> Vec<u32> {
    s.selection
        .rows
        .as_ref()
        .iter()
        .filter_map(|&i| match s.rows.get(i as usize) {
            Some(ListRow::Message(uid)) => Some(*uid),
            _ => None,
        })
        .collect()
}

/// Sets AzMail's read mark of `uids` (all read, or all unread) and saves the folder's marks.
fn mark_read(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny, uids: &[u32], read: bool) {
    if uids.is_empty() {
        return;
    }
    for uid in uids {
        s.flags.read.insert(*uid, read);
    }
    s.refresh_unread_count();
    let flags = s.flags.clone();
    crate::save_flags(s, info, app, flags);
}

/// Runs `action` on the app.
pub(crate) fn run_action(data: &mut RefAny, info: &mut CallbackInfo, action: Action) -> Update {
    with_app(data, |s, app| {
        match action {
            Action::NewMail => ui_compose::open_compose(s, info, app, ComposeKind::New),
            Action::Reply | Action::ReplyAll | Action::Forward => {
                let kind = match action {
                    Action::Reply => ComposeKind::Reply,
                    Action::ReplyAll => ComposeKind::ReplyAll,
                    _ => ComposeKind::Forward,
                };
                if s.open.as_ref().is_some_and(|o| o.view.is_some()) {
                    ui_compose::open_compose(s, info, app, kind);
                } else {
                    s.notice = String::from("Select a message first.");
                }
            }
            Action::Delete | Action::Move => {
                s.notice = String::from(
                    "AzMail keeps the server's folders as they are (it receives read-only); \
                     deleting and moving come with two-way sync.",
                );
            }
            Action::Done => {
                let uids = selected_uids(s);
                mark_read(s, info, app, &uids, true);
            }
            Action::SendReceive => crate::start_sync(s, info, app),
            Action::CancelSendReceive => crate::stop_sync(s, info),
            Action::ToggleRead => {
                let uids = selected_uids(s);
                let all_read = uids.iter().all(|uid| {
                    s.entries
                        .iter()
                        .find(|e| e.uid == *uid)
                        .is_some_and(|e| s.flags.is_read(e))
                });
                mark_read(s, info, app, &uids, !all_read);
            }
            Action::ToggleFlag => {
                let uids = selected_uids(s);
                for uid in &uids {
                    let flagged = s
                        .entries
                        .iter()
                        .find(|e| e.uid == *uid)
                        .is_some_and(|e| s.flags.is_flagged(e));
                    s.flags.flagged.insert(*uid, !flagged);
                }
                if !uids.is_empty() {
                    let flags = s.flags.clone();
                    crate::save_flags(s, info, app, flags);
                }
            }
            Action::MarkAllRead => {
                let uids: Vec<u32> = s.entries.iter().map(|e| e.uid).collect();
                mark_read(s, info, app, &uids, true);
            }
            Action::UnreadOnly => {
                s.scope = if s.scope == 1 { 0 } else { 1 };
                s.first_row = 0;
                s.rebuild_view();
            }
            Action::ReverseSort => {
                s.newest_first = !s.newest_first;
                s.first_row = 0;
                s.rebuild_view();
            }
            Action::ToggleNavigation => s.nav_collapsed = !s.nav_collapsed,
            Action::ToggleReading => s.show_reading = !s.show_reading,
            Action::ToggleTodo => s.show_todo = !s.show_todo,
            Action::PlainText => s.plain_text = !s.plain_text,
            Action::ThemeFlat => info.set_theme("flat"),
            Action::ThemeFlora => info.set_theme("flora"),
            Action::ModeLight => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Light)),
            Action::ModeDark => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark)),
            Action::OpenFile => s.backstage = Some(PAGE_INFO),
            Action::AddAccount => ui_account::open_wizard(s, None),
            Action::AccountSettings => ui_account::open_settings(s),
            Action::CloseBackstage => {
                let saving = s.editor.as_ref().is_some_and(|e| e.saving);
                if s.backstage.is_some() && !s.accounts.is_empty() && !saving {
                    s.backstage = None;
                    s.editor = None;
                }
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== The ribbon ====

fn ribbon(s: &MailApp, app: &RefAny) -> Dom {
    let button = |icon: &str, label: &str, action: Action| {
        RibbonButton::create(icon, label)
            .with_on_click(action_ref(app, action), on_action as ButtonOnClickCallbackType)
    };
    let big = |icon: &str, label: &str, action: Action| {
        RibbonItem::LargeButton(button(icon, label, action))
    };
    let small = |icon: &str, label: &str, action: Action| {
        RibbonItem::SmallButton(button(icon, label, action))
    };
    let toggle = |icon: &str, label: &str, action: Action, on: bool| {
        RibbonItem::SmallButton(button(icon, label, action).with_toggled(on))
    };
    let syncing = matches!(s.sync, SyncState::Running { .. });

    let home = RibbonTab::create("Home")
        .with_group(RibbonGroup::create("New").with_item(big("mail", "New E-mail", Action::NewMail)))
        .with_group(RibbonGroup::create("Delete").with_item(big("delete", "Delete", Action::Delete)))
        .with_group(
            RibbonGroup::create("Respond")
                .with_item(big("reply", "Reply", Action::Reply))
                .with_item(big("reply_all", "Reply All", Action::ReplyAll))
                .with_item(big("forward", "Forward", Action::Forward)),
        )
        .with_group(
            RibbonGroup::create("Quick Steps")
                .with_item(small("done", "Done", Action::Done))
                .with_item(small("group", "Team E-mail", Action::NewMail))
                .with_item(small("reply", "Reply & Delete", Action::Reply)),
        )
        .with_group(
            RibbonGroup::create("Move")
                .with_item(small("drive_file_move", "Move", Action::Move))
                .with_item(small("rule", "Rules", Action::Move)),
        )
        .with_group(
            RibbonGroup::create("Tags")
                .with_item(small("mark_email_unread", "Unread/Read", Action::ToggleRead))
                .with_item(small("flag", "Follow Up", Action::ToggleFlag)),
        )
        .with_group(RibbonGroup::create("Find").with_item(toggle(
            "filter_list",
            "Unread Mail",
            Action::UnreadOnly,
            s.scope == 1,
        )))
        .with_group(
            RibbonGroup::create("Send/Receive")
                .with_item(big("sync", "Send/Receive All Folders", Action::SendReceive)),
        );
    let send_receive = RibbonTab::create("Send / Receive").with_group(
        RibbonGroup::create("Send & Receive")
            .with_item(big("sync", "Send/Receive All Folders", Action::SendReceive))
            .with_item(toggle("cancel", "Cancel All", Action::CancelSendReceive, syncing)),
    );
    let folder = RibbonTab::create("Folder")
        .with_group(
            RibbonGroup::create("Clean Up")
                .with_item(small("mark_email_read", "Mark All as Read", Action::MarkAllRead)),
        )
        .with_group(
            RibbonGroup::create("Actions")
                .with_item(small("refresh", "Update Folder", Action::SendReceive)),
        );
    let view = RibbonTab::create("View")
        .with_group(RibbonGroup::create("Arrangement").with_item(toggle(
            "swap_vert",
            "Reverse Sort",
            Action::ReverseSort,
            !s.newest_first,
        )))
        .with_group(
            RibbonGroup::create("Layout")
                .with_item(toggle(
                    "view_sidebar",
                    "Navigation Pane",
                    Action::ToggleNavigation,
                    !s.nav_collapsed,
                ))
                .with_item(toggle(
                    "chrome_reader_mode",
                    "Reading Pane",
                    Action::ToggleReading,
                    s.show_reading,
                ))
                .with_item(toggle("checklist", "To-Do Bar", Action::ToggleTodo, s.show_todo)),
        )
        .with_group(
            RibbonGroup::create("Message")
                .with_item(toggle("notes", "Plain Text", Action::PlainText, s.plain_text)),
        )
        .with_group(
            RibbonGroup::create("Look")
                .with_item(small("crop_square", "Flat", Action::ThemeFlat))
                .with_item(small("spa", "Flora", Action::ThemeFlora))
                .with_item(small("light_mode", "Light", Action::ModeLight))
                .with_item(small("dark_mode", "Dark", Action::ModeDark)),
        );
    Ribbon::create(vec![home, send_receive, folder, view])
        .with_app_button(RibbonAppButton::create("File").with_on_click(
            action_ref(app, Action::OpenFile),
            on_action as ButtonOnClickCallbackType,
        ))
        .with_active_tab(s.ribbon_tab)
        .with_on_tab_click(app.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType)
        .dom_desktop()
}

extern "C" fn on_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, |s, _| {
        s.ribbon_tab = index;
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== The status bar ====

fn status_bar(s: &MailApp, app: &RefAny) -> Dom {
    let unread = s.view.iter().filter(|e| !s.flags.is_read(e)).count();
    let mut segments = Vec::new();
    if !s.search.is_empty() || s.scope == 1 {
        segments.push(StatusBarSegment::create("Filter applied"));
    }
    segments.push(StatusBarSegment::create(format!("Items: {}", s.view.len())));
    segments.push(StatusBarSegment::create(format!("Unread: {unread}")));
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()));
    }
    let (label, kind) = match &s.sync {
        SyncState::Running {
            status, percent, ..
        } => (format!("{status} ({percent:.0}%)"), StatusBarSyncKind::Syncing),
        SyncState::Done(text) => (text.clone(), StatusBarSyncKind::Connected),
        SyncState::Failed(text) => (text.clone(), StatusBarSyncKind::Error),
        SyncState::Idle if s.accounts.is_empty() => {
            (String::from("Offline"), StatusBarSyncKind::Offline)
        }
        SyncState::Idle => (String::from("Connected"), StatusBarSyncKind::Connected),
    };
    StatusBar::create(segments)
        .with_sync(StatusBarSync::create(label, kind).with_on_click(
            action_ref(app, Action::SendReceive),
            on_action as ButtonOnClickCallbackType,
        ))
        .dom()
}

// ==== The To-Do bar ====

fn todo_bar(s: &MailApp, app: &RefAny) -> Dom {
    let tasks: Vec<ToDoTask> = s
        .tasks
        .iter()
        .map(|t| ToDoTask::create(t.id, t.title.as_str()).with_done(t.done))
        .collect();
    ToDoBar::create(s.calendar.0, s.calendar.1, s.calendar.2)
        .with_today(s.today.0, s.today.1, s.today.2)
        .with_appointments_empty("No upcoming appointments.")
        .with_task_line("Type a new task", s.task_text.as_str())
        .with_tasks(tasks)
        .with_on_pick(app.clone(), on_todo_event as ToDoBarOnEventCallbackType)
        .with_on_task(app.clone(), on_todo_event as ToDoBarOnEventCallbackType)
        .dom()
}

extern "C" fn on_todo_event(mut data: RefAny, _info: CallbackInfo, event: ToDoBarEvent) -> Update {
    with_app(&mut data, |s, _| {
        match event.kind {
            ToDoBarEventKind::DatePicked => {
                s.calendar = (event.date.year, event.date.month, event.date.day);
            }
            ToDoBarEventKind::TaskAdded => {
                let title = event.text.as_str().trim().to_string();
                if title.is_empty() {
                    return Update::DoNothing;
                }
                s.tasks.push(Task {
                    id: s.next_task,
                    title,
                    done: false,
                });
                s.next_task += 1;
                s.task_text.clear();
            }
            ToDoBarEventKind::TaskToggled => {
                if let Some(task) = s.tasks.iter_mut().find(|t| t.id == event.id) {
                    task.done = !task.done;
                }
            }
            ToDoBarEventKind::TaskOpened | ToDoBarEventKind::AppointmentOpened => {
                return Update::DoNothing;
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}
