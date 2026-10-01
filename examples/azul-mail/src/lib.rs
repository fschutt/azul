//! AzMail: a mail client in the shape of Outlook 2010, on the public azul API.
//!
//! - **Accounts**: File > Add Account is a wizard (address and password, the IMAP server, how
//!   mail is sent); the password or OAuth token goes to the OS keyring, never to a file. The
//!   account is `<AzMail folder>/<account id>/account.json`, its sending settings `sending.json`
//!   (SEND's `SendSettings`) next to it.
//! - **Send / Receive** syncs every folder on an azul `Thread` (`sync.rs`, `imap_client.rs`) to
//!   files (`mail/<folder>/<yyyy>/<mm>/<uid>.eml`, `index.jsonl`, `state.json`); the status bar
//!   shows the progress.
//! - **The window** is the PIM shell (S4): the ribbon (File / Home / Send / Receive / Folder /
//!   View), the navigation pane (Favorites and every account's folder tree with unread counts;
//!   Mail / Calendar / Contacts / Tasks), the message list arranged by date, the reading pane
//!   (the mail on paper, pictures only after "download pictures"), the To-Do bar and the status
//!   bar (`ui_main.rs`).
//! - **New / Reply / Reply All / Forward** open a second window (`ui_compose.rs`): From, To / Cc
//!   / Bcc, Subject, a formatting ribbon, the rich editor (`editor.rs`), attachments; Save writes
//!   a draft into the Drafts folder, Send hands the mail to `send::send_mail` on a `Thread` and
//!   closes the window once it is sent (it is then in Sent Items).
//!
//! Environment:
//! - `AZMAIL_DATA`: the AzMail folder (default: `AzMail` in the user's data folder).
//! - `AZMAIL_TEST_PASSWORD`, `AZMAIL_TEST_CA`: with `AZ_BACKEND=headless` only, the password to
//!   sign in with and a PEM certificate to trust (a test server's). A headless run never
//!   touches the real keyring either: azul serves it from memory.
//!
//! On stdout, for scripts: `AZMAIL_ACCOUNT_SAVED <file>`, `AZMAIL_SYNC_START <mail folder>`,
//! `AZMAIL_SYNC_DONE fetched=<n> reused=<n> folders=<n>`, `AZMAIL_SYNC_FAILED <why>`,
//! `AZMAIL_KEYRING <outcome>`, `AZMAIL_OPEN <folder> <uid>`, `AZMAIL_COMPOSE_OPEN <window id>
//! <kind>`, `AZMAIL_DRAFT_SAVED <window id> <uid>`, `AZMAIL_SEND_START <window id>`,
//! `AZMAIL_SEND_DONE <window id> sent|queued|failed <message id or reason>`,
//! `AZMAIL_COMPOSE_CLOSED <window id>`. The secret is never printed.

pub mod account;
pub mod args;
pub mod auth;
pub mod compose;
pub mod editor;
pub mod folders;
pub mod html;
pub mod imap_client;
pub mod listing;
pub mod message;
pub mod mutf7;
pub mod sample;
pub mod send;
pub mod sending;
pub mod store;
pub mod sync;
mod ui_account;
mod ui_compose;
mod ui_main;

#[cfg(test)]
mod testutil;

use std::{collections::HashMap, path::PathBuf};

use account::{Account, Secret};
use args::{Args, Screen};
use azul::{
    css::DarkLightMode,
    error::KeyringResult,
    file::FilePath,
    option::{OptionDarkLightMode, OptionKeyringResult, OptionThreadSendMsg},
    prelude::*,
    str::String as AzString,
    widgets::MessageListSelection,
    window::WindowDecorations,
};
use listing::{FolderInfo, ListRow, LocalFlags};
use message::MessageView;
use store::{FolderState, IndexEntry, LocalFolder};
use sync::{Progress, SyncError, SyncOptions, SyncReport};

/// The main window's id (the debug server addresses a window by it).
pub(crate) const MAIN_WINDOW_ID: &str = "azmail-main";

// ==== State ====

/// The app: one per process, shared by the main window and every compose window.
pub(crate) struct MailApp {
    /// The AzMail folder (`AZMAIL_DATA`).
    pub(crate) root: PathBuf,
    pub(crate) args: Args,
    pub(crate) accounts: Vec<Account>,
    /// The account shown, an index into `accounts`.
    pub(crate) current: Option<usize>,
    /// Secrets in memory for this run, by account id: typed in the wizard, read from the
    /// keyring, or the headless test password.
    pub(crate) secrets: HashMap<String, Secret>,
    /// The keyring operation whose answer is awaited (one at a time: azul keeps only the last
    /// answer).
    pub(crate) keyring: Option<KeyringOp>,

    // -- the navigation pane --
    /// Every account's synced folders with their unread counts, by account index.
    pub(crate) folders: Vec<Vec<FolderInfo>>,
    /// The folder shown (its key, in the current account).
    pub(crate) folder: Option<String>,
    /// The folder was picked in Favorites (that tree shows the selection).
    pub(crate) favorite_picked: bool,
    /// Which groups are open: Favorites, then one per account.
    pub(crate) groups_open: Vec<bool>,
    pub(crate) nav_collapsed: bool,
    /// Mail, Calendar, Contacts, Tasks.
    pub(crate) module: usize,

    // -- the message list --
    /// The shown folder's index, as it is on disk.
    pub(crate) entries: Vec<IndexEntry>,
    /// AzMail's own read / flag marks for the shown folder.
    pub(crate) flags: LocalFlags,
    /// `entries` searched, scoped and sorted: what the list shows.
    pub(crate) view: Vec<IndexEntry>,
    /// The list's rows (group headers and messages) over `view`.
    pub(crate) rows: Vec<ListRow>,
    /// The first row rendered (the list is virtualised).
    pub(crate) first_row: usize,
    pub(crate) selection: MessageListSelection,
    pub(crate) search: String,
    /// 0: all mail, 1: unread.
    pub(crate) scope: usize,
    pub(crate) newest_first: bool,

    // -- the reading pane --
    pub(crate) open: Option<OpenMessage>,
    pub(crate) show_reading: bool,
    pub(crate) show_todo: bool,
    pub(crate) plain_text: bool,

    // -- the ribbon and the backstage --
    pub(crate) ribbon_tab: usize,
    /// The backstage page shown (`ui_main::PAGE_*`); `None` is the mail view.
    pub(crate) backstage: Option<usize>,
    /// The account being added (the wizard) or edited (Account Settings).
    pub(crate) editor: Option<ui_account::AccountEditor>,

    // -- Send / Receive --
    pub(crate) sync: SyncState,
    /// A line for the user in the status bar (a saved setting, a refused action).
    pub(crate) notice: String,

    // -- compose windows --
    pub(crate) composes: Vec<ui_compose::Compose>,
    pub(crate) next_compose: u64,

    // -- the To-Do bar --
    pub(crate) tasks: Vec<Task>,
    pub(crate) task_text: String,
    pub(crate) next_task: u64,
    /// The month the calendar shows (year, month, day picked).
    pub(crate) calendar: (u32, u32, u32),
    /// Today (year, month, day), local.
    pub(crate) today: (u32, u32, u32),
}

/// The message shown in the reading pane.
pub(crate) struct OpenMessage {
    pub(crate) folder: String,
    pub(crate) entry: IndexEntry,
    pub(crate) view: Option<MessageView>,
    pub(crate) error: String,
    /// The sanitized HTML part (made when it is first shown).
    pub(crate) sanitized: Option<html::Sanitized>,
    /// "Download pictures" was clicked for this message.
    pub(crate) pictures: bool,
}

/// A task of the To-Do bar (this run only).
#[derive(Debug, Clone)]
pub(crate) struct Task {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) done: bool,
}

pub(crate) enum SyncState {
    Idle,
    Running {
        thread: ThreadId,
        account: String,
        status: String,
        percent: f32,
    },
    Done(String),
    Failed(String),
}

pub(crate) enum KeyringOp {
    Store,
    /// A secret read to sync this account.
    Get { account: String },
}

impl MailApp {
    fn create(root: PathBuf, args: Args, accounts: Vec<Account>) -> MailApp {
        let today = local_today();
        let n = accounts.len();
        MailApp {
            root,
            args,
            accounts,
            current: None,
            secrets: HashMap::new(),
            keyring: None,
            folders: vec![Vec::new(); n],
            folder: None,
            favorite_picked: false,
            groups_open: vec![true; n + 1],
            nav_collapsed: false,
            module: 0,
            entries: Vec::new(),
            flags: LocalFlags::create(),
            view: Vec::new(),
            rows: Vec::new(),
            first_row: 0,
            selection: MessageListSelection::create(),
            search: String::new(),
            scope: 0,
            newest_first: true,
            open: None,
            show_reading: true,
            show_todo: true,
            plain_text: false,
            ribbon_tab: 0,
            backstage: None,
            editor: None,
            sync: SyncState::Idle,
            notice: String::new(),
            composes: Vec::new(),
            next_compose: 1,
            tasks: Vec::new(),
            task_text: String::new(),
            next_task: 1,
            calendar: today,
            today,
        }
    }

    pub(crate) fn current_account(&self) -> Option<&Account> {
        self.current.and_then(|i| self.accounts.get(i))
    }

    /// The synced files of account `index`.
    pub(crate) fn store_of(&self, index: usize) -> Option<LocalFolder> {
        self.accounts
            .get(index)
            .map(|a| LocalFolder::new(account::mail_root(&self.root, a)))
    }

    /// The current account's synced files.
    pub(crate) fn store(&self) -> Option<LocalFolder> {
        self.current.and_then(|i| self.store_of(i))
    }

    /// Reads every account's folders and their unread counts (from the index and flag files).
    pub(crate) fn reload_folders(&mut self) {
        self.folders = (0..self.accounts.len())
            .map(|i| self.store_of(i).map(|s| folder_infos(&s)).unwrap_or_default())
            .collect();
        if self.groups_open.len() != self.accounts.len() + 1 {
            self.groups_open.resize(self.accounts.len() + 1, true);
        }
        let current = self.current.and_then(|i| self.folders.get(i));
        let still_there = self
            .folder
            .as_ref()
            .is_some_and(|f| current.is_some_and(|list| list.iter().any(|row| &row.key == f)));
        if !still_there {
            // The Inbox first, as Outlook opens.
            self.folder = current.and_then(|list| {
                list.iter()
                    .find(|f| f.role == folders::Role::Inbox)
                    .or_else(|| list.first())
                    .map(|f| f.key.clone())
            });
            self.first_row = 0;
            self.selection = MessageListSelection::create();
        }
    }

    /// Reads the shown folder's index and flags, and rebuilds the list.
    pub(crate) fn reload_messages(&mut self) {
        let (entries, flags) = match (self.store(), self.folder.as_ref()) {
            (Some(store), Some(folder)) => (read_index(&store, folder), read_flags(&store, folder)),
            _ => (Vec::new(), LocalFlags::create()),
        };
        self.entries = entries;
        self.flags = flags;
        self.rebuild_view();
        let open_uid = self.open.as_ref().map(|o| (o.folder.clone(), o.entry.uid));
        if let Some((folder, uid)) = open_uid {
            let here = self.folder.as_deref() == Some(folder.as_str())
                && self.entries.iter().any(|e| e.uid == uid);
            if !here {
                self.open = None;
            }
        }
    }

    /// Searches, scopes, sorts and groups the shown folder's messages into the list's rows.
    pub(crate) fn rebuild_view(&mut self) {
        let unread_only = self.scope == 1;
        let mut view: Vec<IndexEntry> = self
            .entries
            .iter()
            .filter(|e| listing::matches_search(e, &self.search))
            .filter(|e| !unread_only || !self.flags.is_read(e))
            .cloned()
            .collect();
        listing::sort_by_date(&mut view, self.newest_first);
        let today = chrono::NaiveDate::from_ymd_opt(
            self.today.0 as i32,
            self.today.1,
            self.today.2,
        )
        .unwrap_or_default();
        self.rows = listing::grouped_rows(&view, today, &chrono::Local);
        self.view = view;
        if self.first_row >= self.rows.len() {
            self.first_row = 0;
        }
    }

    /// The message of list row `index`, if it is a message.
    pub(crate) fn entry_of_row(&self, index: usize) -> Option<&IndexEntry> {
        match self.rows.get(index) {
            Some(ListRow::Message(uid)) => self.view.iter().find(|e| e.uid == *uid),
            _ => None,
        }
    }

    /// Shows account `index` (its Inbox).
    pub(crate) fn show_account(&mut self, index: usize) {
        self.current = Some(index);
        self.folder = None;
        self.open = None;
        self.favorite_picked = false;
        self.reload_folders();
        self.reload_messages();
    }

    /// Shows folder `key` of the current account.
    pub(crate) fn show_folder(&mut self, key: &str) {
        if self.folder.as_deref() == Some(key) {
            return;
        }
        self.folder = Some(key.to_string());
        self.first_row = 0;
        self.selection = MessageListSelection::create();
        self.open = None;
        self.reload_messages();
    }

    /// Opens message `uid` of the shown folder in the reading pane, and marks it read here.
    /// Returns the flags to save when the mark changed them.
    pub(crate) fn open_message(&mut self, uid: u32) -> Option<LocalFlags> {
        let folder = self.folder.clone()?;
        let entry = self.entries.iter().find(|e| e.uid == uid).cloned()?;
        let bytes = match self.store() {
            Some(store) => store.get(&entry.path).map_err(|e| e.to_string()),
            None => Err(String::from("no account")),
        };
        let (view, error) = match bytes {
            Ok(bytes) => match message::parse_view(&bytes) {
                Some(view) => (Some(view), String::new()),
                None => (None, String::from("This file is not a mail message.")),
            },
            Err(e) => (None, format!("Could not read {}: {e}", entry.path)),
        };
        println!("AZMAIL_OPEN {folder} {uid}");
        let was_read = self.flags.is_read(&entry);
        self.open = Some(OpenMessage {
            folder,
            entry,
            view,
            error,
            sanitized: None,
            pictures: false,
        });
        if was_read {
            return None;
        }
        self.flags.read.insert(uid, true);
        self.refresh_unread_count();
        Some(self.flags.clone())
    }

    /// The shown folder's unread count in the navigation pane, after a mark changed it.
    pub(crate) fn refresh_unread_count(&mut self) {
        let unread = listing::unread_count(&self.entries, &self.flags);
        let (Some(index), Some(folder)) = (self.current, self.folder.clone()) else {
            return;
        };
        if let Some(info) = self
            .folders
            .get_mut(index)
            .and_then(|list| list.iter_mut().find(|f| f.key == folder))
        {
            info.unread = unread;
        }
        if self.scope == 1 {
            self.rebuild_view();
        }
    }
}

/// A folder's index, by UID; empty when there is none.
pub(crate) fn read_index(store: &LocalFolder, folder: &str) -> Vec<IndexEntry> {
    store
        .get(&store::index_key(folder))
        .map(|bytes| store::index_from_jsonl(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

/// A folder's local flags; none when there is no file.
pub(crate) fn read_flags(store: &LocalFolder, folder: &str) -> LocalFlags {
    store
        .get(&listing::flags_key(folder))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| LocalFlags::from_json(&text))
        .unwrap_or_else(LocalFlags::create)
}

/// The synced folders of `store` with their unread counts.
fn folder_infos(store: &LocalFolder) -> Vec<FolderInfo> {
    store
        .folders()
        .into_iter()
        .map(|key| {
            let state = store
                .get(&store::state_key(&key))
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .and_then(|text| FolderState::from_json(&text));
            let entries = read_index(store, &key);
            let flags = read_flags(store, &key);
            FolderInfo {
                display: state
                    .map(|s| s.display)
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| key.clone()),
                role: folders::Role::of_key(&key),
                unread: listing::unread_count(&entries, &flags),
                key,
            }
        })
        .collect()
}

/// Today in the local zone, as (year, month, day).
fn local_today() -> (u32, u32, u32) {
    use chrono::Datelike;
    let now = chrono::Local::now().date_naive();
    (now.year().max(0) as u32, now.month(), now.day())
}

/// The headless test password, if this is a headless run that has one.
pub(crate) fn test_secret() -> Option<Secret> {
    account::test_secret(
        std::env::var("AZ_BACKEND").ok().as_deref(),
        std::env::var(account::TEST_PASSWORD_VAR).ok().as_deref(),
    )
}

/// The headless test server's certificate, if this is a headless run that has one.
fn test_ca() -> Option<PathBuf> {
    (std::env::var("AZ_BACKEND").as_deref() == Ok("headless"))
        .then(|| std::env::var(account::TEST_CA_VAR).ok())
        .flatten()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// ==== Start ====

fn user_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
}

pub fn start() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(text) => {
            let help = text.contains("USAGE");
            if help {
                println!("{text}");
            } else {
                eprintln!("{text}");
            }
            std::process::exit(if help { 0 } else { 2 });
        }
    };
    let root = account::data_root(
        std::env::var(account::DATA_VAR).ok().as_deref(),
        user_data_dir(),
    );
    if args.sample {
        match sample::install(&root) {
            Ok(path) => eprintln!("[azmail] sample account in {}", path.display()),
            Err(e) => eprintln!("[azmail] could not add the sample account: {e}"),
        }
    }
    let (accounts, skipped) = account::load_all(&root);
    for (path, reason) in &skipped {
        eprintln!("[azmail] left out {}: {reason}", path.display());
    }
    eprintln!(
        "[azmail] {} account(s) in {}",
        accounts.len(),
        root.display()
    );
    let first_run = accounts.is_empty();
    let mut state = MailApp::create(root, args.clone(), accounts);
    if !first_run {
        state.show_account(0);
    }
    match args.screen {
        _ if first_run => ui_account::open_wizard(&mut state, None),
        Screen::AddAccount => ui_account::open_wizard(&mut state, None),
        Screen::Settings => ui_account::open_settings(&mut state),
        Screen::Backstage => state.backstage = Some(ui_main::PAGE_INFO),
        Screen::Mail | Screen::Compose | Screen::Reply => {}
    }

    let mut config = AppConfig::create();
    if let Some(theme) = args.theme {
        config = config.with_theme(theme.name());
    }
    if let Some(mode) = args.mode {
        config = config.with_mode(OptionDarkLightMode::Some(match mode {
            args::Mode::Light => DarkLightMode::Light,
            args::Mode::Dark => DarkLightMode::Dark,
        }));
    }
    let app = App::create(RefAny::new(state), config);
    let mut window = WindowCreateOptions::create(ui_main::layout_main);
    let (width, height) = args.size.unwrap_or((1280.0, 860.0));
    window.window_state.size.dimensions = LogicalSize::create(width, height);
    window.window_state.title = AzString::from("AzMail");
    window.window_state.window_id = AzString::from(MAIN_WINDOW_ID);
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(ui_main::on_main_window_created)).into();
    app.run(window);
}
