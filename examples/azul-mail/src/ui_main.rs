//! The main window: Outlook 2010's mail view on the PIM shell, and the backstage.
//!
//! ```text
//! title row (azul's Titlebar: the window is NoTitle)
//! ribbon: File | Home | Send / Receive | Folder | View      (File: its tab row over the backstage)
//! navigation pane  | message list           | reading pane        | To-Do bar
//! (Drag Your Favor-| (search, Arrange By:   | (subject, sender,   | (calendar,
//!  ite Folders Here|  Date, grouped two-line|  Sent / To / Cc,    |  appointments,
//!  the accounts'   |  rows)                 |  attachments, body, |  tasks)
//!  trees, modules) |                        |  People Pane)       |
//! status bar: Items, Unread, a notice | All folders are up to date. Connected to ... | zoom
//! ```
//!
//! The ribbon is Outlook 2010's, tab for tab and control kind for control kind (see
//! [`ribbon`]): Home's New | Delete | Respond | Quick Steps | Move | Tags | Find, Send /
//! Receive, Folder, View.
//!
//! With no account the window is the same, empty: the message list says "No account yet" and
//! offers Add Account (the wizard, `ui_account.rs`), and the navigation pane shows Local Folders
//! (Drafts, Sent Items, Outbox): where the mail written without an account goes (New E-mail
//! needs none, `ui_compose.rs`). File is `ui_backstage.rs`, File > Options a window of its own
//! (`ui_options.rs`).
//!
//! Everything is the toolkit's: `PimShell` (an `OfficeShell`), `Ribbon`, `Backstage`,
//! `ShellNavigationPane` (`TreeView` per account, unread counts as node badges),
//! `SummaryList`, `ReadingPane` (+ `InfoBar`), `ToDoBar`, `StatusBar` (+ `StatusBarSync`),
//! inside a `ShellThemeScope`; the app theme (flat / flora) and the mode (light / dark) are
//! azul's, switched from the View tab.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, ComboBoxOnSelectCallbackType,
        ModalOnCloseCallbackType, SummaryListOnEventCallbackType, ReadingPaneOnEventCallbackType,
        ResumeCallbackType, RibbonGalleryOnSelectCallbackType, RibbonOnTabClickCallbackType,
        ShellNavigationPaneOnEventCallbackType, SliderOnValueChangeCallbackType,
        StandardDialogOnEventCallbackType, ToDoBarOnEventCallbackType, WriteBackCallbackType,
    },
    dom::VirtualKeyCode,
    error::ResultRawImageDecodeImageError,
    http::{HttpBytesResult, HttpRequestConfig},
    image::{ImageRef, RawImage},
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, MenuPopupPosition, StringMenuItem},
    option::{OptionCssPropertyWithConditionsVec, OptionMenuItemIcon, OptionThreadSendMsg},
    prelude::*,
    shells::{
        PimShell, ShellEmptyState, ShellNavigationGroup, ShellNavigationModule,
        ShellNavigationPane, ShellNavigationPaneEvent, ShellNavigationPaneEventKind,
        ShellThemeAccent, ShellThemeScope,
    },
    str::String as AzString,
    vec::U8VecRef,
    widgets::{
        AboutDialog, CheckBoxState, ComboBox, ComboBoxState, InfoBar, SummaryList,
        SummaryListEvent, Modal, ModalState, StandardDialogEvent,
        SummaryListEventKind, SummaryRow, ReadingPane, ReadingPaneEvent, ReadingPaneEventKind,
        Ribbon, RibbonAppButton, RibbonArrow, RibbonBehavior, RibbonButton, RibbonGallery,
        RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonStyle, RibbonTab,
        StatusBar, SliderState, StatusBarSegment, StatusBarSync, StatusBarSyncKind, StatusBarZoom,
        Titlebar, ToDoBar, ToDoBarEvent, ToDoBarEventKind, ToDoTask, TreeViewNode,
    },
};

use crate::{
    compose::ComposeKind,
    folders::Role,
    html, ids,
    listing::{self, FolderNode, ListRow},
    message, pictures, ui_account, ui_backstage, ui_compose, ui_options, with_app, MailApp,
    SyncState,
};

/// The libraries AzMail is built on and their licences (the About box, File > Help).
pub(crate) const CREDITS: [(&str, &str); 6] = [
    ("azul", "MIT"),
    azul_icons_haiku::CREDIT,
    ("imap", "MIT / Apache-2.0"),
    ("mail-parser", "MIT / Apache-2.0"),
    ("micromail", "MIT"),
    ("rustls", "MIT / Apache-2.0 / ISC"),
];

/// The view settings remembered across restarts (the kit's settings.json `values`).
pub(crate) const SET_READING_PANE: &str = "reading_pane";
pub(crate) const SET_TODO_BAR: &str = "todo_bar";
pub(crate) const SET_NAVIGATION_COLLAPSED: &str = "navigation_collapsed";
pub(crate) const SET_PLAIN_TEXT: &str = "plain_text";
pub(crate) const SET_NEWEST_FIRST: &str = "newest_first";
pub(crate) const SET_ZOOM: &str = "zoom";

/// The reading pane's zoom: the status bar's range in percent, and one click of `-` / `+`.
pub(crate) const ZOOM_MIN: f32 = 50.0;
pub(crate) const ZOOM_MAX: f32 = 200.0;
const ZOOM_STEP: f32 = 10.0;

/// The zoom a settings.json value names: a number in percent, inside the range; 100 when there
/// is none or it is no number.
pub(crate) fn zoom_setting(value: Option<&str>) -> f32 {
    value
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|z| z.is_finite())
        .map_or(100.0, |z| z.clamp(ZOOM_MIN, ZOOM_MAX))
}

/// The View tab's switches and the reading pane's zoom as `settings` (the kit's settings.json
/// values) have them: read at the start, and again after File > Options' Cancel put the
/// settings back (the Options window's reload, `ui_options.rs`).
pub(crate) fn read_view_settings(s: &mut MailApp, settings: &azul_appkit::AppSettings) {
    let view = |key: &str, default: bool| settings.get_bool(key, default);
    s.nav_collapsed = view(SET_NAVIGATION_COLLAPSED, false);
    s.show_reading = view(SET_READING_PANE, true);
    s.show_todo = view(SET_TODO_BAR, true);
    s.plain_text = view(SET_PLAIN_TEXT, false);
    s.zoom = zoom_setting(settings.get(SET_ZOOM));
    let newest_first = view(SET_NEWEST_FIRST, true);
    if newest_first != s.newest_first {
        s.newest_first = newest_first;
        s.first_row = 0;
        s.rebuild_view();
    }
}

/// `zoom` moved by `steps` clicks of the status bar's `-` / `+` (negative: out), inside the
/// range.
fn zoom_by(zoom: f32, steps: f32) -> f32 {
    (zoom + steps * ZOOM_STEP).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// Sets the reading pane's zoom and remembers it across restarts (the kit's settings.json).
fn set_zoom(s: &mut MailApp, info: &mut CallbackInfo, zoom: f32) {
    s.zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    azul_appkit::ui::set_value(&s.kit, info, SET_ZOOM, &format!("{}", s.zoom));
}

/// The status bar's zoom slider: the zoom it points at, in whole percent.
extern "C" fn on_zoom_slider(mut data: RefAny, mut info: CallbackInfo, slider: SliderState) -> Update {
    with_app(&mut data, |s, _| {
        set_zoom(s, &mut info, slider.value.round());
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// Rows the list renders at once around what is in view (the list is virtualised).
const LIST_WINDOW: usize = 200;
/// Every row's height in the list.
const ROW_HEIGHT: usize = 48;

/// Plain-text lines shown at most.
const MAX_LINES: usize = 3000;
/// Quote bar colours by level (1, 2, 3, then again).
const QUOTE_COLOURS: [&str; 4] = ["#2f6db0", "#2e7d32", "#8e24aa", "#b36b00"];
/// A quoted line under flora, every level: flora.css's `blockquote` - set off by a thread of
/// brass in the margin (`system:link`, the brass ink), in the quiet ink, in italic. After the
/// flat colours, which it outranks under flora.
const FLORA_QUOTE: &str = "@theme(flora) { border-left-color: system:link; \
                           color: system:secondary-text; font-style: italic; }";
/// The paper a plain-text mail is read on: white with dark text in either mode, like a mail
/// without dark rules (`html.rs`). Under flora ([`FLORA_PAPER`]) flora's field paper and ink,
/// by day and at night.
const PAPER: &str = "display: flex; flex-direction: column; padding: 12px 16px; \
                     background: #ffffff; color: #1a1a1a;";
/// [`PAPER`] under flora: a plain-text mail has no colours of its own to keep light, so it
/// lies on flora's field paper in the ink of the mode (the `system:` keywords flora names);
/// its text inherits the reading pane's Garamond.
const FLORA_PAPER: &str =
    "@theme(flora) { background: system:background; color: system:text; }";
/// An error line in the reading pane: Material red under flat; flora's clay stone by day and
/// its glow at night under flora (the red read 2.4:1 on the night leaf).
const ERROR_LINE: &str = "padding: 16px; color: #b3261e; @theme(flora) { color: #7E4A42; \
                          @media (prefers-color-scheme: dark) { color: #B3837A; } }";
/// The plain-text paper's font size and a line's height at 100 %.
const PAPER_FONT_SIZE: f32 = 14.0;
const PAPER_LINE_HEIGHT: f32 = 18.0;

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
    // File > Options is a window of its own (`ui_options.rs`): this one stays as it is.
    let shell = match s.backstage {
        // File: the ribbon's tab row stays on top (File is its first tab), the backstage under
        // it fills the window.
        Some(page) => PimShell::create(Dom::create_div(), Dom::create_div(), Dom::create_div())
            .office_shell()
            .with_backstage(ui_backstage::file_tab(s, &app, page)),
        None => {
            let mut pim = PimShell::create(
                navigation_pane(s, &app),
                message_list(s, &app),
                reading_pane(s, &app),
            )
            .with_list_label("Message list")
            .with_navigation_ratio(0.2)
            .with_list_ratio(0.42);
            if s.show_todo {
                pim = pim.with_todo_bar(todo_bar(s, &app));
            }
            // The chrome is the OfficeShell's.
            pim.office_shell()
                .with_ribbon(ribbon(s, &app, false))
                .with_status_bar(status_bar(s, &app))
        }
    };
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(title_row(s));
    // An Azlin account on a banned drive (ban contract v1): the banner every Azlin app shows.
    if let Some(bar) = ban_bar(s) {
        column.add_child(bar);
    }
    let mut column = column.with_child(shell.dom());
    if s.about_open {
        column.add_child(about_dialog(&app));
    }
    Dom::create_body()
        .with_css(crate::WINDOW_BODY_CSS)
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

/// The banner over an Azlin account whose drive is banned: why, the hours left to migrate (the
/// kit's words, AzDrive's too), and that AzMail sends nothing from it.
fn ban_bar(s: &MailApp) -> Option<Dom> {
    let account = s.current_account()?;
    let text = s.sending_refused(&account.id)?;
    Some(
        Dom::create_div()
            .with_id(crate::ids::BAN_BAR)
            .with_css(
                "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
                 background: #FDE7E9; color: #5C0F14; font-size: 13px;",
            )
            .with_child(Dom::create_span_with_text(text.as_str())),
    )
}

/// The window's title row, drawn by azul (the window is `NoTitle`): Outlook's "Inbox -
/// ada@example.org - AzMail" ("Drafts - Local Folders - AzMail").
fn title_row(s: &MailApp) -> Dom {
    let folder = current_folder_label(s);
    let title = match (folder, s.current_account()) {
        (Some(folder), Some(account)) => format!("{folder} - {} - AzMail", account.email),
        (None, Some(account)) => format!("{} - AzMail", account.email),
        (Some(folder), None) if s.shows_local() => {
            format!("{folder} - {} - AzMail", listing::LOCAL_FOLDERS)
        }
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

/// The main window is up: what `--screen compose` / `reply` / `options` asked for opens over
/// it, and the kit's `--shot` timer starts - in the window the screen is (a message window and
/// File > Options start it themselves once they are up), else here.
pub(crate) extern "C" fn on_main_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, |s, app| {
        match s.screen {
            crate::args::Screen::Compose => {
                ui_compose::open_compose(s, &mut info, app, ComposeKind::New);
            }
            crate::args::Screen::Reply => {
                // The newest message of the shown folder.
                let first = s.rows.iter().find_map(|row| match row {
                    ListRow::Message(uid) => Some(*uid),
                    ListRow::Group(_) => None,
                });
                match first {
                    Some(uid) => {
                        let _ = s.open_message(uid);
                        ui_compose::open_compose(s, &mut info, app, ComposeKind::Reply);
                    }
                    // Nothing to answer: the screenshot is this window's.
                    None => azul_appkit::ui::on_window_created(&s.kit, &mut info),
                }
            }
            crate::args::Screen::Options => {
                ui_options::open(s, &mut info, crate::args::APP_CATEGORIES[0]);
            }
            _ => azul_appkit::ui::on_window_created(&s.kit, &mut info),
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// Window keys: Ctrl/Cmd+N new mail, Ctrl/Cmd+R reply, Ctrl/Cmd+Shift+R reply all, Ctrl/Cmd+F
/// forward, F9 Send / Receive, Escape leaves the backstage; the kit's keys open File > Options'
/// window - Ctrl/Cmd+, at the Mail page, F1 at the shortcuts (the window handles them itself
/// once it is open, `ui_options.rs`).
extern "C" fn on_main_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let Some(key) = keyboard.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    let primary = modifiers.primary_down();
    let action = match key {
        VirtualKeyCode::N if primary => Action::NewMail,
        VirtualKeyCode::R if primary && modifiers.shift => Action::ReplyAll,
        VirtualKeyCode::R if primary => Action::Reply,
        VirtualKeyCode::F if primary => Action::Forward,
        VirtualKeyCode::Comma if primary => Action::Options,
        VirtualKeyCode::F1 => Action::Shortcuts,
        VirtualKeyCode::F9 => Action::SendReceive,
        VirtualKeyCode::Escape => Action::CloseBackstage,
        _ => return Update::DoNothing,
    };
    if matches!(action, Action::Options | Action::Shortcuts) {
        info.prevent_default();
    }
    run_action(&mut data, &mut info, action)
}

// ==== The About box and File > Options' Mail page ====

/// File > Help > About AzMail: the standard About dialog (the kit's About facts, the libraries
/// AzMail is built on) in a modal over the window. The keyboard shortcuts are the kit's table
/// (F1).
fn about_dialog(app: &RefAny) -> Dom {
    let about = crate::args::ABOUT;
    let dialog = AboutDialog::create(about.name, format!("Version {}", about.version))
        .with_icon("mail")
        .with_description(about.summary)
        .with_on_event(app.clone(), on_about_event as StandardDialogOnEventCallbackType);
    let dialog = CREDITS
        .iter()
        .fold(dialog, |dialog, (name, license)| dialog.with_credit(*name, *license));
    Modal::create(dialog.dom())
        .with_title(format!("About {}", about.name))
        .with_open(true)
        .with_on_close(app.clone(), on_about_closed as ModalOnCloseCallbackType)
        .dom()
}

/// The About dialog's OK.
extern "C" fn on_about_event(mut data: RefAny, _info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    with_app(&mut data, |s, _| {
        s.about_open = false;
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// The About dialog's modal closed (its close button, Escape).
extern "C" fn on_about_closed(mut data: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    with_app(&mut data, |s, _| {
        s.about_open = false;
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// File > Options' own category ("Mail", before the kit's Appearance, Data, Shortcuts, About):
/// the View tab's switches as check boxes, as Outlook's Options dialog has its Mail page. The
/// page is in a window of its own (`ui_options.rs`).
pub(crate) fn mail_options(s: &MailApp, app: &RefAny) -> Vec<azul_appkit::ui::AppSection> {
    let check = |label: &str, on: bool, action: Action, id: AzString| {
        azul_appkit::ui::row(
            label,
            CheckBox::create(on)
                .with_on_toggle(
                    action_ref(app, action),
                    on_option_toggle as CheckBoxOnToggleCallbackType,
                )
                .dom()
                .with_id(id),
        )
    };
    vec![azul_appkit::ui::AppSection {
        category: 0,
        title: String::from("Mail"),
        content: azul_appkit::pieces::column(
            "",
            vec![
                check("Reading Pane", s.show_reading, Action::ToggleReading, ids::OPTION_READING_PANE),
                check("To-Do Bar", s.show_todo, Action::ToggleTodo, ids::OPTION_TODO_BAR),
                check(
                    "Navigation Pane",
                    !s.nav_collapsed,
                    Action::ToggleNavigation,
                    ids::OPTION_NAVIGATION_PANE,
                ),
                check("Newest on top", s.newest_first, Action::ReverseSort, ids::OPTION_NEWEST_FIRST),
                check("Read as plain text", s.plain_text, Action::PlainText, ids::OPTION_PLAIN_TEXT),
                azul_appkit::ui::note(
                    "The View tab's switches; AzMail remembers them. The accounts are under \
                     File > Info.",
                ),
            ],
        ),
    }]
}

/// A check box of File > Options' Mail page: the View tab's action of the same name - the main
/// window shows it at once.
extern "C" fn on_option_toggle(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action))
    else {
        return Update::DoNothing;
    };
    match run_action(&mut app, &mut info, action) {
        Update::DoNothing => Update::DoNothing,
        _ => Update::RefreshDomAllWindows,
    }
}

// ==== Actions (the ribbon, the backstage's buttons, the window's keys) ====

/// What a ribbon button, a backstage button or a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    NewMail,
    Reply,
    ReplyAll,
    Forward,
    /// Into Deleted Items; there, for good (an Azlin account's: an IMAP account keeps the
    /// server's folders as they are).
    Delete,
    /// Into the Archive folder (Azlin accounts).
    Archive,
    /// Into Junk E-mail, the spam folder (Azlin accounts).
    Junk,
    /// The menu of the folders to move into (Azlin accounts).
    Move,
    /// Quick Steps > Done: mark the selection read.
    Done,
    SendReceive,
    CancelSendReceive,
    ToggleRead,
    ToggleFlag,
    MarkAllRead,
    ReverseSort,
    ToggleNavigation,
    ToggleReading,
    ToggleTodo,
    PlainText,
    /// The status bar's `-` / `+`: the reading pane's zoom.
    ZoomOut,
    ZoomIn,
    /// The File tab: opens the backstage, or (open) leaves it.
    OpenFile,
    AddAccount,
    AccountSettings,
    CloseBackstage,
    /// File > Print > Print: the open message's PDF into the exports folder.
    Print,
    /// File > Help > Keyboard Shortcuts: the kit's settings page at its shortcut table.
    Shortcuts,
    /// File > Help > Options: the kit's settings page at AzMail's Mail page.
    Options,
    /// File > Help > About AzMail: the About box.
    About,
    /// Home > New Items: the menu of the new items AzMail makes.
    NewItemsMenu,
    /// A command AzMail does not have (yet): its line in the status bar says why.
    Notice(&'static str),
    /// Home > Respond > More: the menu of the other answers.
    MoreRespondMenu,
    /// Home > Tags > Follow Up's arrow: the flag's menu.
    FollowUpMenu,
    /// Home > Find > Address Book: the Contacts module.
    AddressBook,
    /// Home > Find > Filter E-mail: the filter menu.
    FilterMenu,
    /// Show every message (`false`) or only the unread ones (`true`).
    FilterUnread(bool),
    /// Send / Receive > Send/Receive Groups: the menu of what one Send / Receive covers.
    SendReceiveGroupsMenu,
    /// Send / Receive > Show Progress: how the Send / Receive goes, in the status bar.
    ShowProgress,
    /// Folder > Folder Properties: the shown folder's facts, in the status bar.
    FolderProperties,
    /// View > Layout's menus (Navigation Pane, Reading Pane, To-Do Bar).
    NavigationPaneMenu,
    ReadingPaneMenu,
    TodoBarMenu,
    /// Show (`true`) or hide the navigation pane, the reading pane, the To-Do bar.
    ShowNavigation(bool),
    ShowReading(bool),
    ShowTodo(bool),
}

/// Outlook 2010's Quick Steps (icon, name, what it does in AzMail): two columns of three.
const QUICK_STEPS: [(&str, &str, Action); 6] = [
    ("drive_file_move", "Move to: ?", Action::Move),
    ("group", "Team E-mail", Action::NewMail),
    ("reply", "Reply & Delete", Action::Reply),
    ("forward", "To Manager", Action::Forward),
    ("done", "Done", Action::Done),
    ("add", "Create New", Action::Notice(QUICK_STEPS_FIXED)),
];

/// Why the commands that change the server's folders are not there yet.
const READ_ONLY: &str = "AzMail keeps the server's folders as they are (it receives read-only); \
                         deleting, moving and filing come with two-way sync.";
/// Why Quick Steps cannot be made yet.
const QUICK_STEPS_FIXED: &str = "AzMail's Quick Steps are fixed: Move to, Team E-mail, Reply & \
                                 Delete, To Manager, Done.";
/// Why the server tools of Send / Receive are greyed.
const WHOLE_MESSAGES: &str = "AzMail downloads whole messages: there are no headers to mark.";

pub(crate) struct ActionRef {
    app: RefAny,
    action: Action,
}

/// The callback data of a button that runs `action` ([`on_action`]).
pub(crate) fn action_ref(app: &RefAny, action: Action) -> RefAny {
    RefAny::new(ActionRef {
        app: app.clone(),
        action,
    })
}

/// A button's click: its [`action_ref`]'s action.
pub(crate) extern "C" fn on_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
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
        .keys
        .as_ref()
        .iter()
        .filter_map(|&i| match s.rows.get(i as usize) {
            Some(ListRow::Message(uid)) => Some(*uid),
            _ => None,
        })
        .collect()
}

// ==== An Azlin account's folders: moving, deleting (`azlin_sync`, on a Thread) ====

/// The account shown keeps its mailbox in an Azlin drive: its folders can be changed (an IMAP
/// account's stay as the server has them).
fn shows_azlin(s: &MailApp) -> bool {
    s.current_account().is_some_and(crate::account::Account::is_azlin)
}

/// The messages an action is for: the selected ones, else the one open in the reading pane.
fn action_uids(s: &MailApp) -> Vec<u32> {
    let mut uids = selected_uids(s);
    if uids.is_empty() {
        if let Some(open) = s
            .open
            .as_ref()
            .filter(|open| s.folder.as_deref() == Some(open.folder.as_str()))
        {
            uids.push(open.entry.uid);
        }
    }
    uids
}

/// Moves the selected (or the open) messages into the local folder `to` of the Azlin account
/// shown: in its drive first, then here (`crate::AzlinAction::Move`).
fn move_to(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny, to: String) {
    let Some(from) = s.folder.clone() else {
        return;
    };
    if from == listing::OUTBOX_KEY {
        s.notice = String::from("The Outbox's mail is not in the drive yet: it is sent first.");
        return;
    }
    if from == to {
        s.notice = String::from("The messages are in that folder already.");
        return;
    }
    let uids = action_uids(s);
    if uids.is_empty() {
        s.notice = String::from("Select a message first.");
        return;
    }
    crate::spawn_azlin(s, info, app, crate::AzlinAction::Move { from, uids, to });
}

/// Archive, Junk: into the account's folder of `role` (Archive, Junk E-mail).
fn move_to_role(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny, role: Role) {
    if !shows_azlin(s) {
        s.notice = String::from(READ_ONLY);
        return;
    }
    if let Some(to) = role.key() {
        move_to(s, info, app, to.to_string());
    }
}

/// Delete: into Deleted Items; in Deleted Items, out of the drive for good.
fn delete_messages(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny) {
    if !shows_azlin(s) {
        s.notice = String::from(READ_ONLY);
        return;
    }
    if s.folder.as_deref() != Role::Trash.key() {
        move_to_role(s, info, app, Role::Trash);
        return;
    }
    let (Some(folder), uids) = (s.folder.clone(), action_uids(s)) else {
        return;
    };
    if uids.is_empty() {
        s.notice = String::from("Select a message first.");
        return;
    }
    crate::spawn_azlin(s, info, app, crate::AzlinAction::Delete { folder, uids });
}

/// A Move menu entry's data: the app and the local folder it moves into.
struct MoveRef {
    app: RefAny,
    to: String,
}

/// Home > Move: the account's folders (all but the shown one and the Outbox), each moving the
/// selected messages there.
fn move_menu(s: &MailApp, app: &RefAny) -> Vec<MenuItem> {
    let folders = s.current.and_then(|i| s.folders.get(i)).cloned().unwrap_or_default();
    folders
        .iter()
        .filter(|f| f.key != listing::OUTBOX_KEY && s.folder.as_deref() != Some(f.key.as_str()))
        .map(|f| {
            let data = RefAny::new(MoveRef {
                app: app.clone(),
                to: f.key.clone(),
            });
            let label = listing::folder_label(f.role, &f.display);
            MenuItem::String(StringMenuItem::create(label.as_str()).with_callback(data, on_move_to))
        })
        .collect()
}

/// A folder picked in Home > Move.
extern "C" fn on_move_to(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, to)) = data
        .downcast_ref::<MoveRef>()
        .map(|r| (r.app.clone(), r.to.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, |s, app| {
        move_to(s, &mut info, app, to);
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// An Azlin account's action is done (`crate::run_azlin_action`): the folders and the list as
/// the files are now, a downloaded message shown, and a line in the status bar.
pub(crate) fn azlin_action_done(
    s: &mut MailApp,
    info: &mut CallbackInfo,
    app: RefAny,
    action: crate::AzlinAction,
    result: Result<u64, String>,
) -> Update {
    match (action, result) {
        (crate::AzlinAction::Fetch { folder, uid }, result) => {
            let here = s.folder.as_deref() == Some(folder.as_str())
                && s.open.as_ref().is_some_and(|open| open.entry.uid == uid);
            if !here {
                return Update::DoNothing;
            }
            match result {
                Ok(_) => {
                    stop_pictures(s, info);
                    let _ = s.open_message(uid);
                    show_own_pictures(s, info, &app);
                }
                Err(e) => {
                    if let Some(open) = s.open.as_mut() {
                        open.fetching = false;
                        open.error = format!("Could not download this message: {e}");
                    }
                }
            }
        }
        (crate::AzlinAction::PushMarks { .. }, Ok(_)) => {
            // The marks are the drive's now: the index says so, flags.json keeps none.
            s.reload_folders();
            s.reload_messages();
        }
        (crate::AzlinAction::PushMarks { .. }, Err(e)) => {
            s.notice = format!("The read and flag marks wait for the next Send/Receive: {e}");
        }
        (crate::AzlinAction::Move { to, .. }, result) => {
            let label = s
                .current
                .and_then(|i| s.folders.get(i))
                .and_then(|list| list.iter().find(|f| f.key == to))
                .map_or(to.clone(), |f| listing::folder_label(f.role, &f.display));
            s.notice = match result {
                Ok(1) => format!("Moved 1 message to {label}."),
                Ok(n) => format!("Moved {n} messages to {label}."),
                Err(e) => format!("Could not move to {label}: {e}"),
            };
            s.selection = azul::widgets::ListSelection::create();
            s.reload_folders();
            s.reload_messages();
        }
        (crate::AzlinAction::Delete { .. }, result) => {
            s.notice = match result {
                Ok(1) => String::from("Deleted 1 message for good."),
                Ok(n) => format!("Deleted {n} messages for good."),
                Err(e) => format!("Could not delete: {e}"),
            };
            s.selection = azul::widgets::ListSelection::create();
            s.reload_folders();
            s.reload_messages();
        }
    }
    Update::RefreshDom
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

/// Remembers a view setting across restarts: the kit's settings.json, written on a Thread.
fn remember(s: &MailApp, info: &mut CallbackInfo, key: &str, on: bool) {
    azul_appkit::ui::set_value(&s.kit, info, key, if on { "true" } else { "false" });
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
            Action::Delete => delete_messages(s, info, app),
            Action::Archive => move_to_role(s, info, app, Role::Archive),
            Action::Junk => move_to_role(s, info, app, Role::Spam),
            Action::Move if shows_azlin(s) => {
                open_menu_below(info, move_menu(s, &app));
                return Update::DoNothing;
            }
            Action::Move => s.notice = String::from(READ_ONLY),
            Action::Done => {
                let uids = selected_uids(s);
                mark_read(s, info, app, &uids, true);
            }
            // No account shown (none at all, or Local Folders): their Outbox goes out.
            Action::SendReceive if s.current_account().is_none() => {
                crate::send_local_outbox(s, info, app);
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
            Action::ReverseSort => {
                s.newest_first = !s.newest_first;
                s.first_row = 0;
                s.rebuild_view();
                remember(s, info, SET_NEWEST_FIRST, s.newest_first);
            }
            Action::ToggleNavigation => {
                s.nav_collapsed = !s.nav_collapsed;
                remember(s, info, SET_NAVIGATION_COLLAPSED, s.nav_collapsed);
            }
            Action::ToggleReading => {
                s.show_reading = !s.show_reading;
                remember(s, info, SET_READING_PANE, s.show_reading);
            }
            Action::ToggleTodo => {
                s.show_todo = !s.show_todo;
                remember(s, info, SET_TODO_BAR, s.show_todo);
            }
            Action::PlainText => {
                s.plain_text = !s.plain_text;
                remember(s, info, SET_PLAIN_TEXT, s.plain_text);
            }
            Action::ZoomOut | Action::ZoomIn => {
                let steps = if action == Action::ZoomIn { 1.0 } else { -1.0 };
                let zoom = zoom_by(s.zoom, steps);
                set_zoom(s, info, zoom);
            }
            // File is the ribbon's first tab: clicked while open, it leaves (Outlook 2010).
            Action::OpenFile if s.backstage.is_some() => leave_backstage(s),
            Action::OpenFile => s.backstage = Some(ui_backstage::PAGE_INFO),
            Action::AddAccount => ui_account::open_wizard(s, None),
            Action::AccountSettings => ui_account::open_settings(s),
            Action::CloseBackstage => leave_backstage(s),
            Action::Print => ui_backstage::print_now(s, info, app),
            Action::Shortcuts | Action::Options => {
                // A window of its own, as Outlook 2010's Options dialog; File (or whatever
                // this window shows) stays behind it. Open already: it shows the category.
                let category = if action == Action::Shortcuts {
                    "Shortcuts"
                } else {
                    crate::args::APP_CATEGORIES[0]
                };
                ui_options::open(s, info, category);
                return Update::RefreshDomAllWindows;
            }
            Action::About => s.about_open = true,
            Action::Notice(text) => s.notice = String::from(text),
            Action::NewItemsMenu => {
                open_menu_below(
                    info,
                    vec![
                        menu_item(&app, "E-mail Message", Action::NewMail),
                        MenuItem::Separator,
                        greyed_item("Appointment"),
                        greyed_item("Meeting"),
                        greyed_item("Contact"),
                        greyed_item("Task"),
                    ],
                );
                return Update::DoNothing;
            }
            Action::MoreRespondMenu => {
                open_menu_below(
                    info,
                    vec![
                        menu_item(&app, "Forward", Action::Forward),
                        greyed_item("Forward as Attachment"),
                        greyed_item("Reply with Meeting"),
                    ],
                );
                return Update::DoNothing;
            }
            Action::FollowUpMenu => {
                open_menu_below(info, vec![menu_item(&app, "Flag / Clear Flag", Action::ToggleFlag)]);
                return Update::DoNothing;
            }
            Action::FilterMenu => {
                open_menu_below(
                    info,
                    vec![
                        check_item(&app, "All Mail", Action::FilterUnread(false), s.scope == 0),
                        check_item(&app, "Unread", Action::FilterUnread(true), s.scope == 1),
                    ],
                );
                return Update::DoNothing;
            }
            Action::SendReceiveGroupsMenu => {
                open_menu_below(
                    info,
                    vec![menu_item(&app, "All Accounts", Action::SendReceive)],
                );
                return Update::DoNothing;
            }
            Action::NavigationPaneMenu => {
                open_menu_below(
                    info,
                    vec![
                        check_item(&app, "Normal", Action::ShowNavigation(true), !s.nav_collapsed),
                        check_item(&app, "Minimized", Action::ShowNavigation(false), s.nav_collapsed),
                    ],
                );
                return Update::DoNothing;
            }
            Action::ReadingPaneMenu => {
                open_menu_below(
                    info,
                    vec![
                        check_item(&app, "Right", Action::ShowReading(true), s.show_reading),
                        check_item(&app, "Off", Action::ShowReading(false), !s.show_reading),
                    ],
                );
                return Update::DoNothing;
            }
            Action::TodoBarMenu => {
                open_menu_below(
                    info,
                    vec![
                        check_item(&app, "Normal", Action::ShowTodo(true), s.show_todo),
                        check_item(&app, "Off", Action::ShowTodo(false), !s.show_todo),
                    ],
                );
                return Update::DoNothing;
            }
            Action::FilterUnread(unread) => {
                s.scope = usize::from(unread);
                s.first_row = 0;
                s.rebuild_view();
            }
            Action::AddressBook => s.module = 2,
            Action::ShowProgress => {
                s.notice = match &s.sync {
                    SyncState::Running {
                        status, percent, ..
                    } => format!("Send/Receive: {status} ({percent:.0}%)."),
                    SyncState::Done(text) | SyncState::Failed(text) => format!("Last Send/Receive: {text}"),
                    SyncState::Idle => String::from("Nothing is being sent or received."),
                };
            }
            Action::FolderProperties => {
                let unread = s.view.iter().filter(|e| !s.flags.is_read(e)).count();
                let folder = current_folder_label(s).unwrap_or_else(|| String::from("No folder"));
                s.notice = format!("{folder}: {} items, {unread} unread.", s.entries.len());
            }
            Action::ShowNavigation(on) => {
                s.nav_collapsed = !on;
                remember(s, info, SET_NAVIGATION_COLLAPSED, s.nav_collapsed);
            }
            Action::ShowReading(on) => {
                s.show_reading = on;
                remember(s, info, SET_READING_PANE, s.show_reading);
            }
            Action::ShowTodo(on) => {
                s.show_todo = on;
                remember(s, info, SET_TODO_BAR, s.show_todo);
            }
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

// ==== Menus (a ribbon button's ▾) ====

/// A menu entry running `action`.
fn menu_item(app: &RefAny, label: &str, action: Action) -> MenuItem {
    MenuItem::String(StringMenuItem::create(label).with_callback(action_ref(app, action), on_action))
}

/// A menu entry with a check mark (the state it is in).
fn check_item(app: &RefAny, label: &str, action: Action, checked: bool) -> MenuItem {
    let mut item = StringMenuItem::create(label).with_callback(action_ref(app, action), on_action);
    item.icon = OptionMenuItemIcon::Some(MenuItemIcon::Checkbox(checked));
    MenuItem::String(item)
}

/// A menu entry AzMail has not got: greyed.
fn greyed_item(label: &str) -> MenuItem {
    let mut item = StringMenuItem::create(label);
    item.menu_item_state = MenuItemState::Greyed;
    MenuItem::String(item)
}

/// Opens `items` as a drop-down under the ribbon button that asked for it (where the pointer
/// is when there is none).
fn open_menu_below(info: &mut CallbackInfo, items: Vec<MenuItem>) {
    let menu = Menu::create(items).with_popup_position(MenuPopupPosition::BottomOfHitRect);
    if !info.open_menu_for_hit_node(menu.clone()) {
        info.open_menu(menu);
    }
}

/// Leaves File for the mail - not while an account is being saved (its typed secret goes to
/// the keyring when the files are written). With no account the mail window is there too.
fn leave_backstage(s: &mut MailApp) {
    let saving = s.editor.as_ref().is_some_and(|e| e.saving);
    if s.backstage.is_some() && !saving {
        s.backstage = None;
        s.editor = None;
    }
}

// ==== The ribbon ====

/// The ribbon; with `file_open` only its tab row (File lit, no tab active, the band hidden),
/// over the backstage: Outlook 2010's File is the ribbon's first tab.
///
/// Every tab is Outlook 2010's, control kind for control kind: large buttons (icon over a label
/// of one or two lines, a menu's ▾ under it), small ones stacked three to a column, Follow Up a
/// split button, Quick Steps a list gallery, Find a Contact a combo box. What AzMail cannot do
/// (it receives read-only) says why in the status bar, or is greyed with its reason.
pub(crate) fn ribbon(s: &MailApp, app: &RefAny, file_open: bool) -> Dom {
    let button = |icon: &str, label: &str, action: Action| {
        RibbonButton::create(icon, label)
            .with_on_click(action_ref(app, action), on_action as ButtonOnClickCallbackType)
    };
    let big = |icon: &str, label: &str, action: Action| {
        RibbonItem::LargeButton(button(icon, label, action))
    };
    let big_menu = |icon: &str, label: &str, action: Action| {
        RibbonItem::LargeButton(button(icon, label, action).with_arrow(RibbonArrow::Menu))
    };
    let small = |icon: &str, label: &str, action: Action| {
        RibbonItem::SmallButton(button(icon, label, action))
    };
    let small_menu = |icon: &str, label: &str, action: Action| {
        RibbonItem::SmallButton(button(icon, label, action).with_arrow(RibbonArrow::Menu))
    };
    let toggle = |icon: &str, label: &str, action: Action, on: bool| {
        RibbonItem::SmallButton(button(icon, label, action).with_toggled(on))
    };
    // A command AzMail has not got: greyed, its reason the tooltip.
    let off = |icon: &str, label: &str, why: &str| {
        RibbonButton::create(icon, label).with_disabled(why)
    };
    let syncing = matches!(s.sync, SyncState::Running { .. });

    // Home: New | Delete | Respond | Quick Steps | Move | Tags | Find.
    let quick_steps: Vec<RibbonGalleryCell> = QUICK_STEPS
        .iter()
        .map(|(icon, label, _)| RibbonGalleryCell::create(Dom::create_icon(*icon), *label))
        .collect();
    let quick_steps = RibbonGallery::create(quick_steps)
        .with_columns(2)
        .with_on_select(app.clone(), on_quick_step as RibbonGalleryOnSelectCallbackType);
    let follow_up = button("flag", "Follow Up", Action::ToggleFlag).with_on_arrow_click(
        action_ref(app, Action::FollowUpMenu),
        on_action as ButtonOnClickCallbackType,
    );
    let home = RibbonTab::create("Home")
        .with_group(
            RibbonGroup::create("New")
                .with_item(big("mail", "New E-mail", Action::NewMail))
                .with_item(big_menu("description", "New Items", Action::NewItemsMenu)),
        )
        .with_group(
            // An Azlin account's Junk, Delete and Archive change its drive's folders; an IMAP
            // account's say why they cannot (it receives read-only).
            RibbonGroup::create("Delete")
                .with_item(small("visibility_off", "Ignore", Action::Notice(READ_ONLY)))
                .with_item(small_menu("cleaning_services", "Clean Up", Action::Notice(READ_ONLY)))
                .with_item(small_menu("report", "Junk", Action::Junk))
                .with_item(big("delete", "Delete", Action::Delete))
                .with_item(big("archive", "Archive", Action::Archive)),
        )
        .with_group(
            RibbonGroup::create("Respond")
                .with_item(big("reply", "Reply", Action::Reply))
                .with_item(big("reply_all", "Reply All", Action::ReplyAll))
                .with_item(big("forward", "Forward", Action::Forward))
                .with_item(small(
                    "calendar_month",
                    "Meeting",
                    Action::Notice("Meetings are AzCalendar's: plan one there."),
                ))
                .with_item(small_menu("more_horiz", "More", Action::MoreRespondMenu)),
        )
        .with_group(
            RibbonGroup::create("Quick Steps")
                .with_item(RibbonItem::Gallery(quick_steps))
                .with_launcher(
                    action_ref(app, Action::Notice(QUICK_STEPS_FIXED)),
                    on_action as ButtonOnClickCallbackType,
                ),
        )
        .with_group(
            RibbonGroup::create("Move")
                .with_item(big_menu("drive_file_move", "Move", Action::Move))
                .with_item(big_menu("rule", "Rules", Action::Notice(READ_ONLY))),
        )
        .with_group(
            RibbonGroup::create("Tags")
                .with_item(big("mark_email_unread", "Unread/ Read", Action::ToggleRead))
                .with_item(big_menu(
                    "label",
                    "Categorize",
                    Action::Notice("Categories come with two-way sync."),
                ))
                .with_item(RibbonItem::LargeButton(follow_up)),
        )
        .with_group(
            RibbonGroup::create("Find")
                .with_item(RibbonItem::Combo(find_contact(s, app)))
                .with_item(small("contacts", "Address Book", Action::AddressBook))
                .with_item(small_menu("filter_list", "Filter E-mail", Action::FilterMenu)),
        );

    // Send / Receive: Send & Receive | Download | Server.
    let mut cancel_all = button("cancel", "Cancel All", Action::CancelSendReceive);
    if !syncing {
        cancel_all = cancel_all.with_disabled("Nothing is being sent or received.");
    }
    let send_receive = RibbonTab::create("Send / Receive")
        .with_group(
            RibbonGroup::create("Send & Receive")
                .with_item(big("sync", "Send/Receive All Folders", Action::SendReceive))
                .with_item(small("refresh", "Update Folder", Action::SendReceive))
                .with_item(small("send", "Send All", Action::SendReceive))
                .with_item(small_menu("folder", "Send/Receive Groups", Action::SendReceiveGroupsMenu)),
        )
        .with_group(
            RibbonGroup::create("Download")
                .with_item(big("hourglass_empty", "Show Progress", Action::ShowProgress))
                .with_item(RibbonItem::LargeButton(cancel_all)),
        )
        .with_group(
            RibbonGroup::create("Server")
                .with_item(RibbonItem::LargeButton(off("download", "Download Headers", WHOLE_MESSAGES)))
                .with_item(RibbonItem::SmallButton(
                    off("download_done", "Mark to Download", WHOLE_MESSAGES).with_arrow(RibbonArrow::Menu),
                ))
                .with_item(RibbonItem::SmallButton(
                    off("file_download_off", "Unmark to Download", WHOLE_MESSAGES)
                        .with_arrow(RibbonArrow::Menu),
                ))
                .with_item(RibbonItem::SmallButton(
                    off("task_alt", "Process Marked Headers", WHOLE_MESSAGES).with_arrow(RibbonArrow::Menu),
                )),
        );

    // Folder: New | Actions | Clean Up | Properties.
    let folder = RibbonTab::create("Folder")
        .with_group(
            RibbonGroup::create("New")
                .with_item(RibbonItem::LargeButton(off("create_new_folder", "New Folder", READ_ONLY)))
                .with_item(RibbonItem::LargeButton(off("saved_search", "New Search Folder", READ_ONLY))),
        )
        .with_group(
            RibbonGroup::create("Actions")
                .with_item(RibbonItem::LargeButton(off(
                    "drive_file_rename_outline",
                    "Rename Folder",
                    READ_ONLY,
                )))
                .with_item(RibbonItem::SmallButton(off("file_copy", "Copy Folder", READ_ONLY)))
                .with_item(RibbonItem::SmallButton(off("drive_file_move", "Move Folder", READ_ONLY)))
                .with_item(RibbonItem::SmallButton(off("folder_delete", "Delete Folder", READ_ONLY))),
        )
        .with_group(
            RibbonGroup::create("Clean Up")
                .with_item(big("mark_email_read", "Mark All as Read", Action::MarkAllRead))
                .with_item(RibbonItem::LargeButton(off("rule", "Run Rules Now", READ_ONLY)))
                .with_item(RibbonItem::LargeButton(
                    off("cleaning_services", "Clean Up Folder", READ_ONLY).with_arrow(RibbonArrow::Menu),
                ))
                .with_item(RibbonItem::LargeButton(off("delete_sweep", "Delete All", READ_ONLY))),
        )
        .with_group(
            RibbonGroup::create("Properties")
                .with_item(big("info", "Folder Properties", Action::FolderProperties)),
        );

    // View: Arrangement | Layout | Message.
    let view = RibbonTab::create("View")
        .with_group(
            RibbonGroup::create("Arrangement")
                .with_item(RibbonItem::LargeButton(
                    button(
                        "calendar_month",
                        "Date",
                        Action::Notice("Messages are arranged by date."),
                    )
                    .with_toggled(true),
                ))
                .with_item(toggle("swap_vert", "Reverse Sort", Action::ReverseSort, !s.newest_first))
                .with_item(toggle(
                    "filter_list",
                    "Unread Only",
                    Action::FilterUnread(s.scope != 1),
                    s.scope == 1,
                )),
        )
        .with_group(
            RibbonGroup::create("Layout")
                .with_item(big_menu("view_sidebar", "Navigation Pane", Action::NavigationPaneMenu))
                .with_item(big_menu("chrome_reader_mode", "Reading Pane", Action::ReadingPaneMenu))
                .with_item(big_menu("checklist", "To-Do Bar", Action::TodoBarMenu)),
        )
        .with_group(
            RibbonGroup::create("Message")
                .with_item(toggle("notes", "Plain Text", Action::PlainText, s.plain_text)),
        );
    let mut ribbon = Ribbon::create(vec![home, send_receive, folder, view])
        .with_app_button(RibbonAppButton::create("File").with_on_click(
            action_ref(app, Action::OpenFile),
            on_action as ButtonOnClickCallbackType,
        ))
        .with_active_tab(s.ribbon_tab)
        .with_on_tab_click(app.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType);
    if file_open {
        // An index past the tabs lights none and renders no group (the widget's documented
        // public-field case); the band itself is hidden, and no double click can bring it back.
        ribbon.active_tab = usize::MAX;
        ribbon.style.content_style =
            OptionCssPropertyWithConditionsVec::Some(ui_backstage::hidden());
        ribbon = ribbon.with_behavior(RibbonBehavior::inert());
    }
    ribbon.dom_desktop().with_id(ids::RIBBON)
}

/// Home > Find: Outlook's "Find a Contact" box - the people of the shown folder; picking one
/// searches the list for their mail.
fn find_contact(s: &MailApp, app: &RefAny) -> ComboBox {
    let mut names: Vec<String> = Vec::new();
    for entry in &s.view {
        let name = display_name(&entry.from);
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
            if names.len() == 30 {
                break;
            }
        }
    }
    let names: Vec<AzString> = names.into_iter().map(AzString::from).collect();
    RibbonStyle::create_default()
        .styled_combo_box(names, "", 150)
        .with_placeholder("Find a Contact")
        .with_accessibility_name("Find a Contact")
        .with_on_select(app.clone(), on_find_contact as ComboBoxOnSelectCallbackType)
}

/// A contact picked (or typed) in Find a Contact: the list shows their mail.
extern "C" fn on_find_contact(mut data: RefAny, _info: CallbackInfo, state: ComboBoxState) -> Update {
    with_app(&mut data, |s, _| {
        let name = state.text.as_str().trim().to_string();
        if name.is_empty() || name == s.search {
            return Update::DoNothing;
        }
        s.search = name;
        s.first_row = 0;
        s.selection = azul::widgets::ListSelection::create();
        s.rebuild_view();
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// A Quick Step clicked: what it does in AzMail ([`QUICK_STEPS`]).
extern "C" fn on_quick_step(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((_, _, action)) = QUICK_STEPS.get(index) else {
        return Update::DoNothing;
    };
    run_action(&mut data, &mut info, *action)
}

/// A ribbon tab: shown; with File open it also leaves File (Outlook 2010's tabs).
extern "C" fn on_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, |s, _| {
        if s.editor.as_ref().is_some_and(|e| e.saving) {
            return Update::DoNothing;
        }
        s.ribbon_tab = index;
        s.backstage = None;
        s.editor = None;
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
    if unread > 0 {
        segments.push(StatusBarSegment::create(format!("Unread: {unread}")));
    }
    if !s.notice.is_empty() {
        segments.push(StatusBarSegment::create(s.notice.as_str()));
    }
    let (label, kind) = match &s.sync {
        SyncState::Running {
            status, percent, ..
        } => (format!("{status} ({percent:.0}%)"), StatusBarSyncKind::Syncing),
        // Without an account nothing is connected (Local Folders' Outbox went out: the
        // notice says how).
        _ if s.accounts.is_empty() => (String::from("No account"), StatusBarSyncKind::Offline),
        // Outlook 2010: "All folders are up to date." beside "Connected to ...".
        SyncState::Done(_) => (up_to_date(s), StatusBarSyncKind::Connected),
        SyncState::Failed(text) => (text.clone(), StatusBarSyncKind::Error),
        SyncState::Idle => (up_to_date(s), StatusBarSyncKind::Connected),
    };
    // Outlook's zoom at the right end: the reading pane's, `-` / `+` by ten, the slider over the
    // buttons' whole range.
    let zoom = StatusBarZoom::create(s.zoom, ZOOM_MIN, ZOOM_MAX)
        .with_on_zoom_out(action_ref(app, Action::ZoomOut), on_action as ButtonOnClickCallbackType)
        .with_on_zoom_in(action_ref(app, Action::ZoomIn), on_action as ButtonOnClickCallbackType)
        .with_on_slider_change(app.clone(), on_zoom_slider as SliderOnValueChangeCallbackType);
    StatusBar::create(segments)
        .with_sync(StatusBarSync::create(label, kind).with_on_click(
            action_ref(app, Action::SendReceive),
            on_action as ButtonOnClickCallbackType,
        ))
        .with_zoom(zoom)
        .dom()
}

/// The status bar's sync line while nothing runs: Outlook 2010's "All folders are up to date.",
/// and the server the shown account is connected to.
fn up_to_date(s: &MailApp) -> String {
    match s.current_account() {
        Some(account) => match &account.azlin {
            Some(link) => format!(
                "All folders are up to date.   Connected to the Azlin drive {}",
                link.drive_id
            ),
            None => format!(
                "All folders are up to date.   Connected to {}",
                account.imap.host
            ),
        },
        None => String::from("All folders are up to date."),
    }
}

// ==== The To-Do bar ====

/// The To-Do bar: the month, the appointments, and the shared task store's tasks (`todo.rs`;
/// a task's id in the bar is its index in `MailApp::tasks`).
fn todo_bar(s: &MailApp, app: &RefAny) -> Dom {
    let tasks: Vec<ToDoTask> = s
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| ToDoTask::create(i as u64, t.title.as_str()).with_done(t.is_done()))
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

extern "C" fn on_todo_event(mut data: RefAny, mut info: CallbackInfo, event: ToDoBarEvent) -> Update {
    with_app(&mut data, |s, app| {
        let now = chrono::Local::now().naive_local();
        let changed: Vec<azul_pim::task::Task> = match event.kind {
            ToDoBarEventKind::DatePicked => {
                s.calendar = (event.date.year, event.date.month, event.date.day);
                Vec::new()
            }
            ToDoBarEventKind::TaskAdded => {
                let Some(task) = crate::todo::new_task(
                    event.text.as_str(),
                    &s.task_list,
                    &s.tasks,
                    crate::new_id(),
                    now,
                ) else {
                    return Update::DoNothing;
                };
                println!("AZMAIL_TASK_ADDED {}", task.key());
                s.task_text.clear();
                s.tasks.push(task.clone());
                vec![task]
            }
            ToDoBarEventKind::TaskToggled => {
                let Some(task) = s.tasks.get_mut(event.id as usize) else {
                    return Update::DoNothing;
                };
                // Ticking off a repeating task leaves its next occurrence, as in AzTasks.
                let next = crate::todo::toggle_done(task, crate::new_id(), now);
                let mut changed = vec![task.clone()];
                changed.extend(next.clone());
                s.tasks.extend(next);
                changed
            }
            ToDoBarEventKind::TaskOpened | ToDoBarEventKind::AppointmentOpened => {
                return Update::DoNothing;
            }
        };
        crate::todo::sort(&mut s.tasks);
        save_tasks(s, &mut info, app, &changed);
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// The write-back tag of the To-Do bar's task files.
const TAG_TASKS: u64 = 1;

/// Writes `tasks` to their files in the shared task store, on a Thread (azul-appkit's file
/// jobs on the data root's drive).
fn save_tasks(s: &MailApp, info: &mut CallbackInfo, app: RefAny, tasks: &[azul_pim::task::Task]) {
    let jobs: Vec<azul_appkit::FileJob> = tasks.iter().map(crate::todo::put_job).collect();
    azul_appkit::ui::spawn_file_jobs(
        info,
        &s.data_root,
        jobs,
        app,
        TAG_TASKS,
        on_tasks_saved as WriteBackCallbackType,
    );
}

/// The task files are written (or a sentence says why not).
extern "C" fn on_tasks_saved(mut app: RefAny, mut reply: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = azul_appkit::ui::take_reply(&mut reply) else {
        return Update::DoNothing;
    };
    let Some(error) = reply.outcomes.iter().find_map(azul_appkit::FileOutcome::error) else {
        for outcome in &reply.outcomes {
            if let azul_appkit::FileOutcome::Put { key, .. } = outcome {
                println!("AZMAIL_TASK_SAVED {key}");
            }
        }
        return Update::DoNothing;
    };
    with_app(&mut app, |s, _| {
        s.notice = format!("The task could not be saved: {error}");
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

/// A folder of the tree as a tree node: its unread count as the node's badge (the Outbox's:
/// how many wait in it), selected when `selected` says so.
fn tree_node(node: &FolderNode, role_of: &dyn Fn(&str) -> Role, selected: &dyn Fn(&str) -> bool) -> TreeViewNode {
    let icon = if node.key == listing::OUTBOX_KEY {
        "outbox"
    } else {
        folder_icon(role_of(&node.key))
    };
    let mut tree = TreeViewNode::create(node.label.as_str())
        .with_icon(icon)
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

/// Mailbox `i`'s group in the navigation pane: `name` (the account's address, "Local
/// Folders") as the root of its folder tree.
fn mailbox_group(s: &MailApp, i: usize, name: &str) -> ShellNavigationGroup {
    let folders = s.folders.get(i).map_or(&[][..], Vec::as_slice);
    let roles = |key: &str| -> Role {
        folders
            .iter()
            .find(|f| f.key == key)
            .map_or(Role::Other, |f| f.role)
    };
    let here = Some(i) == s.current;
    let picked = |key: &str| here && s.folder.as_deref() == Some(key);
    let open = s.groups_open.get(i + 1).copied().unwrap_or(true);
    let mut root = TreeViewNode::create(name).with_expanded(open);
    for node in listing::folder_tree(folders) {
        root = root.with_child(tree_node(&node, &roles, &picked));
    }
    ShellNavigationGroup::create(name, root).with_open(open)
}

/// The navigation pane as Outlook 2010 has it: "Drag Your Favorite Folders Here" over every
/// account's tree - its address the root, the folders under it in Outlook's order, the unread
/// counts as badges - then Local Folders (the mail written without an account: always while
/// there is no account), and the big module buttons at the bottom (Mail, Calendar, Contacts,
/// Tasks).
fn navigation_pane(s: &MailApp, app: &RefAny) -> Dom {
    let mut pane = ShellNavigationPane::create()
        .with_label("Mail")
        .with_header(favorites_hint())
        .with_trees_only(true)
        .with_active_module(s.module)
        .with_collapsed(s.nav_collapsed)
        .with_on_event(app.clone(), on_nav_event as ShellNavigationPaneOnEventCallbackType);

    // Every account: its address over its folders.
    for (i, account) in s.accounts.iter().enumerate() {
        pane = pane.with_group(mailbox_group(s, i, account.email.as_str()));
    }
    // Local Folders: the group after the accounts' (`MailApp::local_index`).
    if s.local_visible() {
        pane = pane.with_group(mailbox_group(s, s.local_index(), listing::LOCAL_FOLDERS));
    }

    let unread: usize = s.current.and_then(|i| s.folders.get(i)).map_or(0, |list| {
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
        .with_id(ids::FOLDER_PANE)
}

/// The Favorites strip at the top of the pane: Outlook 2010's hint while it holds no folder.
fn favorites_hint() -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; flex-grow: 1; padding: 5px 8px 6px 10px; \
             border-bottom: 1px dashed system:separator;",
        )
        .with_child(
            Dom::create_span_with_text("Drag Your Favorite Folders Here")
                .with_css("font-size: 12px; color: system:secondary-text;"),
        )
}

extern "C" fn on_nav_event(mut data: RefAny, _info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
    with_app(&mut data, |s, _| {
        match event.kind {
            // The pane is trees only: an account's root triangle is its tree's toggle.
            ShellNavigationPaneEventKind::GroupToggled => return Update::DoNothing,
            ShellNavigationPaneEventKind::NodeToggled => {
                if event.index != 0 {
                    return Update::DoNothing;
                }
                if let Some(open) = s.groups_open.get_mut(event.group + 1) {
                    *open = event.expand;
                }
            }
            ShellNavigationPaneEventKind::NodeClicked => {
                // Row 0 of a mailbox's tree is its root (an account's address, "Local
                // Folders"); the groups are the accounts', then Local Folders' when shown.
                let mailbox = event.group;
                let shown = mailbox < s.accounts.len()
                    || (mailbox == s.local_index() && s.local_visible());
                if !shown {
                    return Update::DoNothing;
                }
                if s.current != Some(mailbox) {
                    s.show_account(mailbox);
                }
                let folders = s.folders.get(mailbox).cloned().unwrap_or_default();
                let keys = listing::preorder_keys(&listing::folder_tree(&folders));
                if let Some(key) = event.index.checked_sub(1).and_then(|k| keys.get(k)) {
                    s.show_folder(key);
                }
                s.module = 0;
            }
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
    if s.accounts.is_empty() && !s.shows_local() {
        // The real window, empty: one calm line and the way in (as File > Info > Add Account).
        return ShellEmptyState::create("No account yet")
            .with_icon("inbox")
            .with_detail(
                "Add an e-mail account to receive mail. AzMail keeps a copy of every folder as \
                 files on this computer. Writing needs no account: a new message is sent from \
                 this computer, and Local Folders keep what you write.",
            )
            .with_action_label("Add Account\u{2026}")
            .with_on_action(
                action_ref(app, Action::AddAccount),
                on_action as ButtonOnClickCallbackType,
            )
            .dom()
            .with_id(ids::NO_ACCOUNT);
    }
    let total = s.rows.len();
    let first = s.first_row.min(total.saturating_sub(1));
    let end = (first + LIST_WINDOW).min(total);
    let role = s.folder.as_deref().map_or(Role::Other, Role::of_key);
    let outbox = s.folder.as_deref() == Some(listing::OUTBOX_KEY);
    // Sent Items, Drafts and the Outbox show whom the mail is to, as Outlook does.
    let outgoing = outbox || matches!(role, Role::Sent | Role::Drafts);
    let today = chrono::NaiveDate::from_ymd_opt(s.today.0 as i32, s.today.1, s.today.2)
        .unwrap_or_default();
    let rows: Vec<SummaryRow> = s.rows[first..end]
        .iter()
        .enumerate()
        .filter_map(|(offset, row)| match row {
            ListRow::Group(group) => Some(SummaryRow::create_group(group.label())),
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
                    _ if outbox => "outbox",
                    Role::Drafts => "drafts",
                    Role::Sent => "send",
                    _ if read => "drafts",
                    _ => "mail",
                };
                Some(
                    SummaryRow::create(u64::from(*uid), who, subject)
                        .with_date(listing::list_date(&entry.date, today, &chrono::Local))
                        .with_icon(icon)
                        .with_unread(!read)
                        .with_flagged(s.flags.is_flagged(entry))
                        .with_selected(s.selection.contains((first + offset) as u64)),
                )
            }
        })
        .collect();
    let folder = current_folder_label(s).unwrap_or_else(|| String::from("Mail"));
    SummaryList::create(rows)
        .with_window(first, total)
        .with_row_height(ROW_HEIGHT)
        .with_search(s.search.as_str())
        .with_search_placeholder(format!("Search {folder}"))
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
        .with_on_select(app.clone(), on_list_event as SummaryListOnEventCallbackType)
        .with_on_open(app.clone(), on_list_event as SummaryListOnEventCallbackType)
        .with_on_flag(app.clone(), on_list_event as SummaryListOnEventCallbackType)
        .with_on_delete(app.clone(), on_list_event as SummaryListOnEventCallbackType)
        .with_on_sort(app.clone(), on_list_event as SummaryListOnEventCallbackType)
        .with_on_search(app.clone(), on_list_event as SummaryListOnEventCallbackType)
        .with_on_scroll(app.clone(), on_list_event as SummaryListOnEventCallbackType)
        .dom()
        .with_id(ids::MESSAGE_LIST)
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

extern "C" fn on_list_event(mut data: RefAny, mut info: CallbackInfo, event: SummaryListEvent) -> Update {
    with_app(&mut data, |s, app| {
        match event.kind {
            SummaryListEventKind::Select | SummaryListEventKind::Open => {
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
                    .apply(index as u64, event.shift, event.ctrl);
                if !event.shift && !event.ctrl {
                    stop_pictures(s, &mut info);
                    let flags = s.open_message(uid);
                    show_own_pictures(s, &mut info, &app);
                    // An Azlin account's big message is downloaded now.
                    crate::fetch_if_needed(s, &mut info, app.clone());
                    if let Some(flags) = flags {
                        crate::save_flags(s, &mut info, app.clone(), flags);
                    }
                }
                // A draft opens in a compose window again.
                if event.kind == SummaryListEventKind::Open
                    && s.folder.as_deref() == Some(crate::compose::DRAFTS_FOLDER)
                {
                    ui_compose::open_compose(s, &mut info, app, ComposeKind::Draft);
                }
            }
            SummaryListEventKind::Flag => {
                let Some(entry) = s.entry_of_row(event.index).cloned() else {
                    return Update::DoNothing;
                };
                let flagged = s.flags.is_flagged(&entry);
                s.flags.flagged.insert(entry.uid, !flagged);
                let flags = s.flags.clone();
                crate::save_flags(s, &mut info, app, flags);
            }
            SummaryListEventKind::Delete => delete_messages(s, &mut info, app),
            SummaryListEventKind::Sort => {
                s.notice = String::from("Messages are arranged by date.");
            }
            SummaryListEventKind::SortDirection => {
                s.newest_first = !s.newest_first;
                s.first_row = 0;
                s.rebuild_view();
            }
            SummaryListEventKind::Search => {
                s.search = event.text.as_str().to_string();
                s.first_row = 0;
                s.selection = azul::widgets::ListSelection::create();
                s.rebuild_view();
            }
            SummaryListEventKind::Scope => {
                s.scope = event.index.min(1);
                s.first_row = 0;
                s.rebuild_view();
            }
            SummaryListEventKind::Scroll => {
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
    if s.module != 0 || (s.accounts.is_empty() && !s.shows_local()) {
        // Without an account the list says it all ("No account yet"): the pane stays blank
        // (Local Folders show their mail as an account's).
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
            .map(|a| {
                AzString::from(format!(
                    "{} ({})",
                    a.name,
                    azul::file::DiskSpace::format_bytes(a.size as u64)
                ))
            })
            .collect();
        pane = pane.with_attachments(names);
    }
    let name = display_name(&sender);
    // The PIM apps' avatar initials (DEDUP_EDITORS B24).
    let initials = azul_pim::initials::initials(&name);
    pane = pane.with_people(vec![AzString::from(initials)], format!("See more about: {name}."));
    let html = open.sanitized.as_ref().filter(|_| !s.plain_text);
    if let Some(sanitized) = html {
        if sanitized.blocked_images > 0 && !open.pictures {
            // What the pre-pass found on the web ("3 pictures and 1 font"), else "some
            // pictures" (pictures that are not on the web).
            let held_back = match sanitized.remote.summary() {
                summary if summary.is_empty() => String::from("some pictures"),
                summary => summary,
            };
            let text = format!(
                "Click here to download pictures. To help protect your privacy, AzMail \
                 prevented automatic download of {held_back} in this message."
            );
            pane = pane.with_info_bar(
                InfoBar::create(text.as_str())
                    .with_icon("info")
                    .with_action("Download pictures"),
            );
        }
    }
    let body = if open.fetching {
        // An Azlin account's big message on its way from the drive: no error.
        Dom::create_span_with_text(open.error.as_str()).with_css("padding: 16px;")
    } else if !open.error.is_empty() {
        Dom::create_span_with_text(open.error.as_str()).with_css(ERROR_LINE)
    } else {
        match html {
            // A mail wider than the pane (its paper grows with it, `html.rs`) scrolls sideways
            // here. The zoom scales what the mail leaves to the paper (an `em` of the pane's
            // font); its own px sizes are its author's - azul has no CSS `zoom` yet.
            // TODO(LAYOUT7, wave 7): once CSS `zoom` lands, this box says `zoom: <s.zoom>%`
            // instead of the `em` (the mail's px sizes scale too, as in Outlook).
            Some(sanitized) => {
                let mut css = String::from("overflow-x: auto;");
                if (s.zoom - 100.0).abs() > f32::EPSILON {
                    css.push_str(&format!(" font-size: {:.2}em;", s.zoom / 100.0));
                }
                Dom::create_div().with_css(css).with_child(html_body(sanitized))
            }
            None => plain_body(&view.text, s.zoom),
        }
    };
    pane.with_body(body)
        .with_on_load_images(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .with_on_link(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .with_on_attachment(app.clone(), on_reading_event as ReadingPaneOnEventCallbackType)
        .dom()
}

/// Plain text on paper at `zoom` percent: every line a row, quoted lines indented behind a bar
/// in their level's colour.
fn plain_body(text: &str, zoom: f32) -> Dom {
    let scale = zoom / 100.0;
    let mut body = Dom::create_div().with_css(format!(
        "{PAPER} font-size: {:.1}px; {FLORA_PAPER}",
        PAPER_FONT_SIZE * scale
    ));
    let line_height = PAPER_LINE_HEIGHT * scale;
    let lines = message::quote_lines(text);
    for line in lines.iter().take(MAX_LINES) {
        let css = if line.level == 0 {
            format!(
                "white-space: pre-wrap; min-height: {line_height:.1}px; overflow-wrap: anywhere;"
            )
        } else {
            let colour = QUOTE_COLOURS[(line.level - 1) % QUOTE_COLOURS.len()];
            format!(
                "white-space: pre-wrap; min-height: {line_height:.1}px; overflow-wrap: anywhere; \
                 margin-left: {}px; padding-left: 8px; border-left: 3px solid {colour}; color: \
                 {colour}; {FLORA_QUOTE}",
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
        .with_css(ERROR_LINE),
    }
}

extern "C" fn on_reading_event(mut data: RefAny, mut info: CallbackInfo, event: ReadingPaneEvent) -> Update {
    with_app(&mut data, |s, app| {
        match event.kind {
            ReadingPaneEventKind::LoadImages => load_pictures(s, &mut info, &app),
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

// ==== Pictures: the mail's own at once, its web pictures after "Download pictures" ====
//
// Nothing is fetched or decoded on the UI thread: a download Thread fetches the web pictures
// one after the other through azul's HTTP client (each `download_bytes` blocks that worker; its
// bytes resume on the UI thread, where the mail's Budget is checked), and a decode Thread turns
// bytes into images and writes each back; the write-back puts it into the image cache under the
// key its `<img src>` names (the web address, or the `cid:` key of a part of the mail).

/// "Download pictures": the mail is sanitized again with its web pictures kept, and the
/// download Thread fetches the ones it shows ([`pictures::fetch_list`]: http / https, no
/// tracking pixel, at most `MAX_PICTURES`).
fn load_pictures(s: &mut MailApp, info: &mut CallbackInfo, app: &RefAny) {
    stop_pictures(s, info);
    let Some(open) = s.open.as_mut() else {
        return;
    };
    let Some(part) = open.view.as_ref().and_then(|v| v.html.clone()) else {
        return;
    };
    open.pictures = true;
    let sanitized = html::sanitize_mail(
        &part,
        &html::PictureOptions {
            web: true,
            inline: pictures::content_ids(&open.inline),
        },
    );
    let urls = pictures::fetch_list(&sanitized);
    open.sanitized = Some(sanitized);
    if urls.is_empty() {
        return;
    }
    println!("AZMAIL_PICTURES_FETCH {}", urls.len());
    let thread = ThreadId::unique();
    open.pictures_thread = Some(thread);
    open.budget = pictures::Budget::default();
    let job = DownloadJob {
        app: app.clone(),
        folder: open.folder.clone(),
        uid: open.entry.uid,
        urls,
    };
    info.add_thread(
        thread,
        Thread::create(RefAny::new(job), app.clone(), download_thread),
    );
}

/// Stops the open mail's picture downloads (another mail is opened, or they start again).
pub(crate) fn stop_pictures(s: &mut MailApp, info: &mut CallbackInfo) {
    if let Some(thread) = s.open.as_mut().and_then(|open| open.pictures_thread.take()) {
        info.remove_thread(thread);
    }
}

/// The open mail's own pictures (`cid:` parts it shows): decoded on a Thread, no download.
pub(crate) fn show_own_pictures(s: &mut MailApp, info: &mut CallbackInfo, app: &RefAny) {
    let Some(open) = s.open.as_ref() else {
        return;
    };
    let Some(sanitized) = open.sanitized.as_ref() else {
        return;
    };
    let decodes = pictures::inline_decodes(sanitized, &open.inline);
    if decodes.is_empty() {
        return;
    }
    start_decode(info, app, &open.folder, open.entry.uid, decodes);
}

/// Decodes `pictures` (image-cache key, bytes) of message `uid` in `folder` on a Thread.
fn start_decode(
    info: &mut CallbackInfo,
    app: &RefAny,
    folder: &str,
    uid: u32,
    pictures: Vec<(String, Vec<u8>)>,
) {
    let job = DecodeJob {
        folder: folder.to_string(),
        uid,
        pictures,
    };
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(job), app.clone(), decode_thread),
    );
}

/// What the download Thread fetches, for which message.
#[derive(Clone)]
struct DownloadJob {
    app: RefAny,
    folder: String,
    uid: u32,
    urls: Vec<String>,
}

/// One web picture on its way: the message it belongs to and its address.
#[derive(Clone)]
struct PictureRef {
    app: RefAny,
    folder: String,
    uid: u32,
    url: String,
}

/// Fetches the job's pictures one after the other through azul's HTTP client (no cookies;
/// size cap and timeout per picture), until it is stopped.
extern "C" fn download_thread(mut init: RefAny, _sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_ref::<DownloadJob>()
        .map(|job| DownloadJob::clone(&job))
    else {
        return;
    };
    let config = HttpRequestConfig::create()
        .with_timeout(pictures::PICTURE_TIMEOUT_SECS)
        .with_max_size(pictures::MAX_PICTURE_BYTES as u64)
        .with_user_agent("AzMail");
    for url in &job.urls {
        // Stopped (another mail is open, or the mail's budget is spent): nothing more.
        while let OptionThreadSendMsg::Some(message) = receiver.recv() {
            if matches!(message, ThreadSendMsg::TerminateThread) {
                return;
            }
        }
        // Blocks this worker for the transfer; the bytes resume on the UI thread.
        let _request = config.download_bytes(
            url.as_str(),
            RefAny::new(PictureRef {
                app: job.app.clone(),
                folder: job.folder.clone(),
                uid: job.uid,
                url: url.clone(),
            }),
            on_picture_bytes as ResumeCallbackType,
        );
    }
}

/// A web picture's bytes arrived (on the UI thread): counted against the mail's budget, then
/// decoded on a Thread. Past the mail's total the downloads stop.
extern "C" fn on_picture_bytes(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picture) = data
        .downcast_ref::<PictureRef>()
        .map(|p| PictureRef::clone(&p))
    else {
        return Update::DoNothing;
    };
    let Some(answer) = HttpBytesResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let bytes = match answer.result {
        azul::error::ResultU8VecHttpError::Ok(bytes) => bytes.as_slice().to_vec(),
        azul::error::ResultU8VecHttpError::Err(e) => {
            eprintln!("[azmail] picture {} not downloaded: {e:?}", picture.url);
            return Update::DoNothing;
        }
    };
    let mut app = picture.app.clone();
    with_app(&mut app, |s, app| {
        let Some(open) = s
            .open
            .as_mut()
            .filter(|o| o.folder == picture.folder && o.entry.uid == picture.uid)
        else {
            // Another mail is open now.
            return Update::DoNothing;
        };
        let len = bytes.len();
        if !open.budget.take(len) {
            if open.budget.spent_for(len) {
                // The mail's total is spent: no further download.
                if let Some(thread) = open.pictures_thread.take() {
                    info.remove_thread(thread);
                }
            }
            eprintln!("[azmail] picture {} left out ({len} bytes)", picture.url);
            return Update::DoNothing;
        }
        let (folder, uid) = (open.folder.clone(), open.entry.uid);
        start_decode(&mut info, &app, &folder, uid, vec![(picture.url.clone(), bytes)]);
        Update::DoNothing
    })
    .unwrap_or(Update::DoNothing)
}

/// What the decode Thread turns into images, for which message.
#[derive(Clone)]
struct DecodeJob {
    folder: String,
    uid: u32,
    /// The image-cache key and the bytes of each picture.
    pictures: Vec<(String, Vec<u8>)>,
}

/// A decoded picture, written back to the UI thread.
struct DecodedPicture {
    folder: String,
    uid: u32,
    key: String,
    image: RawImage,
}

/// Decodes the job's pictures and writes each one back as soon as it is ready.
extern "C" fn decode_thread(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_ref::<DecodeJob>()
        .map(|job| DecodeJob::clone(&job))
    else {
        return;
    };
    for (key, bytes) in job.pictures {
        while let OptionThreadSendMsg::Some(message) = receiver.recv() {
            if matches!(message, ThreadSendMsg::TerminateThread) {
                return;
            }
        }
        let image = match RawImage::decode_image_bytes_any(U8VecRef::from(bytes.as_slice())) {
            ResultRawImageDecodeImageError::Ok(image) => image,
            ResultRawImageDecodeImageError::Err(e) => {
                eprintln!("[azmail] picture {key} not decoded: {e:?}");
                continue;
            }
        };
        sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
            refany: RefAny::new(DecodedPicture {
                folder: job.folder.clone(),
                uid: job.uid,
                key,
                image,
            }),
            callback: WriteBackCallback {
                cb: on_picture_decoded,
                ctx: OptionRefAny::None,
            },
        }));
    }
}

/// A picture is decoded: into the image cache under its key, and the window redraws (if its
/// mail is still the open one).
extern "C" fn on_picture_decoded(mut app: RefAny, mut payload: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut decoded) = payload.downcast_mut::<DecodedPicture>() else {
        return Update::DoNothing;
    };
    let still_open = with_app(&mut app, |s, _| {
        s.open
            .as_ref()
            .is_some_and(|o| o.folder == decoded.folder && o.entry.uid == decoded.uid)
    })
    .unwrap_or(false);
    if !still_open {
        return Update::DoNothing;
    }
    let image = std::mem::replace(&mut decoded.image, RawImage::empty());
    match ImageRef::create_rawimage(image).into_option() {
        Some(image) => {
            println!("AZMAIL_PICTURE_SHOWN {}", decoded.key);
            info.add_image_to_cache(decoded.key.as_str(), image);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The zoom is remembered as a number in settings.json: read back inside the status bar's
    /// range, 100 % when the value is missing or no number.
    #[test]
    fn the_zoom_setting_is_read_back_inside_the_range() {
        assert_eq!(zoom_setting(None), 100.0);
        assert_eq!(zoom_setting(Some("130")), 130.0);
        assert_eq!(zoom_setting(Some(" 80 ")), 80.0);
        assert_eq!(zoom_setting(Some("1000")), ZOOM_MAX);
        assert_eq!(zoom_setting(Some("5")), ZOOM_MIN);
        assert_eq!(zoom_setting(Some("big")), 100.0);
        assert_eq!(zoom_setting(Some("NaN")), 100.0);
    }

    /// `-` and `+` move the zoom by ten percent and stop at the range's ends.
    #[test]
    fn zoom_out_and_in_step_by_ten_and_stop_at_the_ends() {
        assert_eq!(zoom_by(100.0, 1.0), 110.0);
        assert_eq!(zoom_by(100.0, -1.0), 90.0);
        assert_eq!(zoom_by(ZOOM_MAX, 1.0), ZOOM_MAX);
        assert_eq!(zoom_by(ZOOM_MIN, -1.0), ZOOM_MIN);
        assert_eq!(zoom_by(57.0, -1.0), ZOOM_MIN);
    }
}
