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

// ==== The navigation pane ====

/// A folder's icon in the tree.
fn folder_icon(role: Role) -> &'static str {
    match role {
        Role::Inbox => "inbox",
        Role::Drafts => "drafts",
        Role::Sent => "send",
        Role::Trash => "delete",
        Role::Spam => "report",
        Role::Archive => "archive",
        Role::All => "all_inbox",
        Role::Flagged => "flag",
        Role::Other => "folder",
    }
}

/// A folder of the tree as a tree node: its unread count as the node's badge, selected when
/// `selected` says so.
fn tree_node(node: &FolderNode, role_of: &dyn Fn(&str) -> Role, selected: &dyn Fn(&str) -> bool) -> TreeViewNode {
    let mut tree = TreeViewNode::create(node.label.as_str())
        .with_icon(folder_icon(role_of(&node.key)))
        .with_expanded(true)
        .with_selected(selected(&node.key));
    if node.unread > 0 {
        tree = tree.with_badge(node.unread.to_string());
    }
    for child in &node.children {
        tree = tree.with_child(tree_node(child, role_of, selected));
    }
    tree
}

fn navigation_pane(s: &MailApp, app: &RefAny) -> Dom {
    let mut pane = ShellNavigationPane::create()
        .with_label("Mail")
        .with_active_module(s.module)
        .with_collapsed(s.nav_collapsed)
        .with_on_event(app.clone(), on_nav_event as ShellNavigationPaneOnEventCallbackType);

    // Favorites: the shown account's Inbox and Sent Items.
    let current_folders = s.current.and_then(|i| s.folders.get(i));
    let role_of = |key: &str| -> Role {
        current_folders
            .and_then(|list| list.iter().find(|f| f.key == key))
            .map_or(Role::Other, |f| f.role)
    };
    let mut favorites = TreeViewNode::create("Favorites").with_expanded(true);
    for node in listing::favorites(current_folders.map_or(&[][..], Vec::as_slice)) {
        let picked = |key: &str| s.favorite_picked && s.folder.as_deref() == Some(key);
        favorites = favorites.with_child(tree_node(&node, &role_of, &picked));
    }
    pane = pane.with_group(
        ShellNavigationGroup::create("Favorites", favorites)
            .with_open(s.groups_open.first().copied().unwrap_or(true)),
    );

    // Every account: its address over its folders.
    for (i, account) in s.accounts.iter().enumerate() {
        let folders = s.folders.get(i).map_or(&[][..], Vec::as_slice);
        let roles = |key: &str| -> Role {
            folders
                .iter()
                .find(|f| f.key == key)
                .map_or(Role::Other, |f| f.role)
        };
        let here = Some(i) == s.current;
        let picked = |key: &str| here && !s.favorite_picked && s.folder.as_deref() == Some(key);
        let mut root = TreeViewNode::create(account.email.as_str())
            .with_icon("account_circle")
            .with_expanded(true);
        for node in listing::folder_tree(folders) {
            root = root.with_child(tree_node(&node, &roles, &picked));
        }
        let inbox_unread: usize = folders
            .iter()
            .filter(|f| f.role == Role::Inbox)
            .map(|f| f.unread)
            .sum();
        let mut group = ShellNavigationGroup::create(account.email.as_str(), root)
            .with_open(s.groups_open.get(i + 1).copied().unwrap_or(true));
        if inbox_unread > 0 {
            group = group.with_count(inbox_unread);
        }
        pane = pane.with_group(group);
    }

    let unread: usize = current_folders.map_or(0, |list| {
        list.iter()
            .filter(|f| f.role == Role::Inbox)
            .map(|f| f.unread)
            .sum()
    });
    let mut mail = ShellNavigationModule::create("Mail", "mail");
    if unread > 0 {
        mail = mail.with_badge(unread.to_string());
    }
    pane.with_module(mail)
        .with_module(ShellNavigationModule::create("Calendar", "calendar_month"))
        .with_module(ShellNavigationModule::create("Contacts", "contacts"))
        .with_module(ShellNavigationModule::create("Tasks", "task_alt"))
        .dom()
}

extern "C" fn on_nav_event(mut data: RefAny, _info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
    with_app(&mut data, |s, _| {
        match event.kind {
            ShellNavigationPaneEventKind::GroupToggled => {
                if let Some(open) = s.groups_open.get_mut(event.group) {
                    *open = event.expand;
                }
            }
            ShellNavigationPaneEventKind::NodeClicked => {
                // Row 0 of a group's tree is its root (Favorites, the account's address).
                if event.group == 0 {
                    let folders = s.current.and_then(|i| s.folders.get(i)).cloned().unwrap_or_default();
                    let keys = listing::preorder_keys(&listing::favorites(&folders));
                    if let Some(key) = event.index.checked_sub(1).and_then(|k| keys.get(k)) {
                        s.show_folder(key);
                        s.favorite_picked = true;
                    }
                } else {
                    let account = event.group - 1;
                    if account >= s.accounts.len() {
                        return Update::DoNothing;
                    }
                    if s.current != Some(account) {
                        s.show_account(account);
                    }
                    let folders = s.folders.get(account).cloned().unwrap_or_default();
                    let keys = listing::preorder_keys(&listing::folder_tree(&folders));
                    if let Some(key) = event.index.checked_sub(1).and_then(|k| keys.get(k)) {
                        s.show_folder(key);
                    }
                    s.favorite_picked = false;
                }
                s.module = 0;
            }
            ShellNavigationPaneEventKind::NodeToggled => return Update::DoNothing,
            // Drag and drop onto folders is not wired yet (messages stay put).
            ShellNavigationPaneEventKind::NodeDropped => return Update::DoNothing,
            ShellNavigationPaneEventKind::ModuleSelected => s.module = event.index,
            ShellNavigationPaneEventKind::CollapseToggled => s.nav_collapsed = !event.expand,
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== The message list ====

/// The name of an address entry ("Ben Okafor" from `Ben Okafor <ben@example.org>`); the
/// address when it has none.
fn display_name(entries: &str) -> String {
    let first = crate::compose::split_addresses(entries)
        .into_iter()
        .next()
        .unwrap_or_default();
    match first.find('<') {
        Some(at) if at > 0 => first[..at].trim().trim_matches('"').trim().to_string(),
        _ => crate::compose::bare_address(&first).unwrap_or(first),
    }
}

fn message_list(s: &MailApp, app: &RefAny) -> Dom {
    if s.module != 0 {
        return module_placeholder(s.module);
    }
    let total = s.rows.len();
    let first = s.first_row.min(total.saturating_sub(1));
    let end = (first + LIST_WINDOW).min(total);
    let role = s.folder.as_deref().map_or(Role::Other, Role::of_key);
    // Sent Items and Drafts show whom the mail is to, as Outlook does.
    let outgoing = matches!(role, Role::Sent | Role::Drafts);
    let today = chrono::NaiveDate::from_ymd_opt(s.today.0 as i32, s.today.1, s.today.2)
        .unwrap_or_default();
    let rows: Vec<MessageRow> = s.rows[first..end]
        .iter()
        .enumerate()
        .filter_map(|(offset, row)| match row {
            ListRow::Group(group) => Some(MessageRow::create_group(group.label())),
            ListRow::Message(uid) => {
                let entry = s.view.iter().find(|e| e.uid == *uid)?;
                let who = if outgoing {
                    format!("To: {}", display_name(&entry.to))
                } else if entry.from.is_empty() {
                    String::from("(no sender)")
                } else {
                    display_name(&entry.from)
                };
                let subject = if entry.subject.is_empty() {
                    "(no subject)"
                } else {
                    entry.subject.as_str()
                };
                let read = s.flags.is_read(entry);
                let icon = match role {
                    Role::Drafts => "drafts",
                    Role::Sent => "send",
                    _ if read => "drafts",
                    _ => "mail",
                };
                Some(
                    MessageRow::create(u64::from(*uid), who, subject)
                        .with_date(listing::list_date(&entry.date, today, &chrono::Local))
                        .with_icon(icon)
                        .with_unread(!read)
                        .with_flagged(s.flags.is_flagged(entry))
                        .with_selected(s.selection.contains((first + offset) as u32)),
                )
            }
        })
        .collect();
    let folder = current_folder_label(s).unwrap_or_else(|| String::from("Mail"));
    MessageList::create(rows)
        .with_window(first, total)
        .with_row_height(ROW_HEIGHT)
        .with_search(s.search.as_str())
        .with_search_placeholder(format!("Search {folder} (Ctrl+E)"))
        .with_scopes(
            vec![AzString::from("All"), AzString::from("Unread")],
            s.scope,
        )
        .with_sort(
            "Arrange By:",
            "Date",
            s.newest_first,
        )
        .with_sort_direction_label(if s.newest_first {
            "Newest on top"
        } else {
            "Oldest on top"
        })
        .with_on_select(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .with_on_open(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .with_on_flag(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .with_on_delete(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .with_on_sort(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .with_on_search(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .with_on_scope(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .with_on_scroll(app.clone(), on_list_event as MessageListOnEventCallbackType)
        .dom()
}

/// Calendar, Contacts and Tasks are other apps.
fn module_placeholder(module: usize) -> Dom {
    let (title, detail, icon) = match module {
        1 => ("Calendar", "Appointments live in AzCalendar.", "calendar_month"),
        2 => ("Contacts", "The address book is not part of AzMail yet.", "contacts"),
        _ => ("Tasks", "Tasks of this run are in the To-Do bar.", "task_alt"),
    };
    ShellEmptyState::create(title)
        .with_icon(icon)
        .with_detail(detail)
        .dom()
}

extern "C" fn on_list_event(mut data: RefAny, mut info: CallbackInfo, event: MessageListEvent) -> Update {
    with_app(&mut data, |s, app| {
        match event.kind {
            MessageListEventKind::Select | MessageListEventKind::Open => {
                let index = event.index;
                // A row beyond the rendered window (Home, End, a page): move the window there.
                if index < s.first_row || index >= s.first_row + LIST_WINDOW {
                    s.first_row = index.saturating_sub(LIST_WINDOW / 4);
                }
                let Some(uid) = s.entry_of_row(index).map(|e| e.uid) else {
                    return Update::RefreshDom;
                };
                s.selection = s
                    .selection
                    .clone()
                    .apply(index as u32, event.shift, event.ctrl);
                if !event.shift && !event.ctrl {
                    if let Some(flags) = s.open_message(uid) {
                        crate::save_flags(s, &mut info, app.clone(), flags);
                    }
                }
                // A draft opens in a compose window again.
                if event.kind == MessageListEventKind::Open
                    && s.folder.as_deref() == Some(crate::compose::DRAFTS_FOLDER)
                {
                    ui_compose::open_compose(s, &mut info, app, ComposeKind::Draft);
                }
            }
            MessageListEventKind::Flag => {
                let Some(entry) = s.entry_of_row(event.index).cloned() else {
                    return Update::DoNothing;
                };
                let flagged = s.flags.is_flagged(&entry);
                s.flags.flagged.insert(entry.uid, !flagged);
                let flags = s.flags.clone();
                crate::save_flags(s, &mut info, app, flags);
            }
            MessageListEventKind::Delete => {
                s.notice = String::from(
                    "AzMail keeps the server's folders as they are (it receives read-only); \
                     deleting comes with two-way sync.",
                );
            }
            MessageListEventKind::Sort => {
                s.notice = String::from("Messages are arranged by date.");
            }
            MessageListEventKind::SortDirection => {
                s.newest_first = !s.newest_first;
                s.first_row = 0;
                s.rebuild_view();
            }
            MessageListEventKind::Search => {
                s.search = event.text.as_str().to_string();
                s.first_row = 0;
                s.selection = azul::widgets::MessageListSelection::create();
                s.rebuild_view();
            }
            MessageListEventKind::Scope => {
                s.scope = event.index.min(1);
                s.first_row = 0;
                s.rebuild_view();
            }
            MessageListEventKind::Scroll => {
                let (start, end) = (event.index, event.end);
                let window_end = s.first_row + LIST_WINDOW;
                // Rebuild only when the view comes near the window's edges.
                let near_top = s.first_row > 0 && start < s.first_row + LIST_WINDOW / 8;
                let near_bottom = window_end < s.rows.len() && end + LIST_WINDOW / 8 > window_end;
                if !near_top && !near_bottom {
                    return Update::DoNothing;
                }
                s.first_row = start.saturating_sub(LIST_WINDOW / 4);
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== The reading pane ====

fn reading_pane(s: &MailApp, app: &RefAny) -> Dom {
    if !s.show_reading {
        return Dom::create_div();
    }
    if s.module != 0 {
        return Dom::create_div();
    }
    let Some(open) = &s.open else {
        return ShellEmptyState::create("Select an item to read")
            .with_icon("mail")
            .with_detail("Click a message in the list to see it here.")
            .dom();
    };
    let view = open.view.clone().unwrap_or_default();
    let subject = if view.subject.is_empty() {
        String::from("(no subject)")
    } else {
        view.subject.clone()
    };
    let sender = if view.from.is_empty() {
        open.entry.from.clone()
    } else {
        view.from.clone()
    };
    let date_source = if view.date.is_empty() {
        open.entry.date.as_str()
    } else {
        view.date.as_str()
    };
    let sent = message::short_date_in(date_source, &chrono::Local);
    let mut pane = ReadingPane::create(subject.as_str(), sender.as_str())
        .with_date(sent.as_str())
        .with_field("Sent", sent.as_str());
    if !view.to.is_empty() {
        pane = pane.with_field("To", view.to.as_str());
    }
    if !view.cc.is_empty() {
        pane = pane.with_field("Cc", view.cc.as_str());
    }
    if !view.attachments.is_empty() {
        let names: Vec<AzString> = view
            .attachments
            .iter()
            .map(|a| AzString::from(format!("{} ({})", a.name, human_size(a.size))))
            .collect();
        pane = pane.with_attachments(names);
    }
    let name = display_name(&sender);
    // The PIM apps' avatar initials (DEDUP_EDITORS B24).
    let initials = azul_pim::initials::initials(&name);
    pane = pane.with_people(vec![AzString::from(initials)], format!("More about: {name}"));
    let html = open.sanitized.as_ref().filter(|_| !s.plain_text);
    if let Some(sanitized) = html {
        if sanitized.blocked_images > 0 && !open.pictures {
            pane = pane.with_info_bar(
                InfoBar::create(
                    "Click here to download pictures. To help protect your privacy, AzMail \
                     prevented automatic download of some pictures in this message.",
                )
                .with_icon("info")
                .with_action("Download pictures"),
            );
        }
    }
    let body = if !open.error.is_empty() {
        Dom::create_span_with_text(open.error.as_str()).with_css("padding: 16px; color: #b3261e;")
    } else {
        match html {
            Some(sanitized) => html_body(sanitized),
            None => plain_body(&view.text),
        }
    };
    pane.with_body(body)
        .with_on_load_images(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .with_on_link(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .with_on_attachment(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .dom()
}

/// "12 KB" for a byte count.
fn human_size(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{} KB", (b + 1023) / 1024),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// Plain text on paper: every line a row, quoted lines indented behind a bar in their level's
/// colour.
fn plain_body(text: &str) -> Dom {
    let mut body = Dom::create_div().with_css(PAPER);
    let lines = message::quote_lines(text);
    for line in lines.iter().take(MAX_LINES) {
        let css = if line.level == 0 {
            String::from("white-space: pre-wrap; min-height: 18px; overflow-wrap: anywhere;")
        } else {
            let colour = QUOTE_COLOURS[(line.level - 1) % QUOTE_COLOURS.len()];
            format!(
                "white-space: pre-wrap; min-height: 18px; overflow-wrap: anywhere; margin-left: \
                 {}px; padding-left: 8px; border-left: 3px solid {colour}; color: {colour};",
                (line.level - 1) * 12
            )
        };
        body.add_child(
            Dom::create_div()
                .with_css(css)
                .with_child(Dom::create_span_with_text(line.text.as_str())),
        );
    }
    if lines.len() > MAX_LINES {
        body.add_child(
            Dom::create_span_with_text(format!("({} more lines)", lines.len() - MAX_LINES))
                .with_css("font-size: 12px; margin-top: 8px;"),
        );
    }
    body
}

/// The sanitized HTML part (already on its paper) through azul's own XML parser.
fn html_body(sanitized: &html::Sanitized) -> Dom {
    match Xml::from_str(sanitized.xhtml.as_str()) {
        ResultXmlXmlError::Ok(xml) => Dom::create_from_parsed_xml(xml),
        ResultXmlXmlError::Err(e) => Dom::create_span_with_text(format!(
            "The HTML part could not be shown: {e:?}"
        ))
        .with_css("padding: 16px; color: #b3261e;"),
    }
}

extern "C" fn on_reading_event(mut data: RefAny, _info: CallbackInfo, event: ReadingPaneEvent) -> Update {
    with_app(&mut data, |s, app| {
        match event.kind {
            ReadingPaneEventKind::LoadImages => load_pictures(s, &app),
            ReadingPaneEventKind::Attachment => {
                s.notice = format!("{} is in the message file; saving attachments comes next.", event.text.as_str());
            }
            ReadingPaneEventKind::Sender | ReadingPaneEventKind::People => {
                return Update::DoNothing;
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== Download pictures ====

/// "Download pictures": the mail is sanitized again with its web pictures kept, and each one is
/// fetched; when it arrives it is decoded and put into the image cache under its address, where
/// the `<img src>` finds it.
fn load_pictures(s: &mut MailApp, app: &RefAny) {
    let Some(open) = s.open.as_mut() else {
        return;
    };
    let Some(part) = open.view.as_ref().and_then(|v| v.html.clone()) else {
        return;
    };
    open.pictures = true;
    let sanitized = html::sanitize_with(&part, true);
    let urls = sanitized.remote_images.clone();
    open.sanitized = Some(sanitized);
    let config = HttpRequestConfig::create()
        .with_timeout(20)
        .with_max_size(10 * 1024 * 1024)
        .with_user_agent("AzMail");
    for url in urls {
        let _request = config.download_bytes(
            url.as_str(),
            RefAny::new(PictureRef {
                app: app.clone(),
                url: url.clone(),
            }),
            on_picture_bytes as ResumeCallbackType,
        );
    }
}

struct PictureRef {
    app: RefAny,
    url: String,
}

/// A picture's bytes arrived: decode them (off the UI thread too).
extern "C" fn on_picture_bytes(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some(picture) = data.downcast_ref::<PictureRef>().map(|p| PictureRef {
        app: p.app.clone(),
        url: p.url.clone(),
    }) else {
        return Update::DoNothing;
    };
    let Some(answer) = HttpBytesResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    match answer.result {
        azul::error::ResultU8VecHttpError::Ok(bytes) => {
            let _request = RawImage::decode_image_bytes(
                bytes,
                RefAny::new(picture),
                on_picture_decoded as ResumeCallbackType,
            );
            Update::DoNothing
        }
        azul::error::ResultU8VecHttpError::Err(e) => {
            eprintln!("[azmail] picture {} not downloaded: {e:?}", picture.url);
            Update::DoNothing
        }
    }
}

/// A picture is decoded: into the image cache under its address, and the window redraws.
extern "C" fn on_picture_decoded(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(url) = data.downcast_ref::<PictureRef>().map(|p| p.url.clone()) else {
        return Update::DoNothing;
    };
    let Some(decoded) = ImageDecodeResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    match decoded.result {
        azul::error::ResultRawImageDecodeImageError::Ok(raw) => {
            match ImageRef::create_rawimage(raw).into_option() {
                Some(image) => {
                    info.add_image_to_cache(url.as_str(), image);
                    Update::RefreshDom
                }
                None => Update::DoNothing,
            }
        }
        azul::error::ResultRawImageDecodeImageError::Err(e) => {
            eprintln!("[azmail] picture {url} not decoded: {e:?}");
            Update::DoNothing
        }
    }
}
