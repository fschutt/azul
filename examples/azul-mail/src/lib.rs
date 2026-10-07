//! AzMail: a mail client in the shape of Outlook 2010, on the public azul API.
//!
//! - **The window is always the real one**: with no account it is the same ribbon, folder pane,
//!   message list and reading pane, empty - the list says "No account yet" and offers Add
//!   Account, as File > Info does. No wizard stands in front of the window.
//! - **Accounts**: File > Info > Add Account is a wizard (address and password, the IMAP
//!   server, how mail is sent); the password or OAuth token goes to the OS keyring, never to a
//!   file. The account is `<AzMail folder>/<account id>/account.json`, its sending settings
//!   `sending.json` (SEND's `SendSettings`) next to it.
//! - **Send / Receive** syncs every folder on an azul `Thread` (`sync.rs`, `imap_client.rs`) to
//!   files (`mail/<folder>/<yyyy>/<mm>/<uid>.eml`, `index.jsonl`, `state.json`); the status bar
//!   shows the progress.
//! - **The window** is the PIM shell (S4): the ribbon (File / Home / Send / Receive / Folder /
//!   View), the navigation pane (Favorites and every account's folder tree with unread counts;
//!   Mail / Calendar / Contacts / Tasks), the message list arranged by date, the reading pane
//!   (the mail on paper, pictures only after "download pictures"), the To-Do bar and the status
//!   bar (`ui_main.rs`).
//! - **File** is Outlook 2010's backstage (`ui_backstage.rs`): the ribbon's tab row stays on top,
//!   no back button; Info (the accounts, Add Account, Account Settings, Send/Receive), Print
//!   (the open message to a PDF file, with a picture of its first page), Help (shortcuts,
//!   Options, About), Options (the kit's settings page), Exit.
//! - **New / Reply / Reply All / Forward** open a second window (`ui_compose.rs`): From, To / Cc
//!   / Bcc, Subject, a formatting ribbon, azul's shared rich-text editor, attachments; Save writes
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
//! `AZMAIL_COMPOSE_CLOSED <window id>`, `AZMAIL_PRINT_PDF <folder> <uid> <bytes>`,
//! `AZMAIL_PRINT_PREVIEW pages=<n> shown=<bool>`, `AZMAIL_PRINTED <file>`. The secret is never
//! printed.

pub mod account;
pub mod args;
pub mod auth;
pub mod compose;
pub mod dkim;
pub mod folders;
pub mod html;
pub mod icons;
pub mod ids;
pub mod imap_client;
pub mod listing;
pub mod message;
pub mod mutf7;
pub mod pictures;
pub mod sample;
pub mod send;
pub mod sending;
pub mod store;
pub mod submit;
pub mod sync;
pub mod todo;
mod ui_account;
mod ui_backstage;
mod ui_compose;
mod ui_main;

#[cfg(test)]
mod testutil;

use std::{collections::HashMap, path::PathBuf};

use account::{Account, Secret};
use args::Screen;
use azul::{
    error::KeyringResult,
    file::FilePath,
    option::{OptionKeyringResult, OptionThreadSendMsg},
    prelude::*,
    str::String as AzString,
    widgets::ListSelection,
};
use azul_appkit::{ui as kit, AppArgs};
use listing::{FolderInfo, ListRow, LocalFlags};
use message::MessageView;
use store::{DriveFolder, FolderState, IndexEntry, MailStore};
use sync::{Progress, SyncError, SyncOptions, SyncReport};

/// The main window's id (the debug server addresses a window by it).
pub(crate) const MAIN_WINDOW_ID: &str = "azmail-main";

/// Every window's body: a column as tall as the window (`height: 100%` of the viewport - a
/// flex body without it is only as tall as its content, and the shell's status bar floated
/// in the middle of the window).
pub(crate) const WINDOW_BODY_CSS: &str =
    "display: flex; flex-direction: column; height: 100%; margin: 0px;";

// ==== State ====

/// The app: one per process, shared by the main window and every compose window.
pub(crate) struct MailApp {
    /// The AzMail folder (`AZMAIL_DATA`, else `mail` in the data root): a folder of the data
    /// tree's one drive, which every file of AzMail is written through.
    pub(crate) root: DriveFolder,
    /// The app kit (azul-appkit): settings.json, the data root, the settings page, the shortcut
    /// table.
    pub(crate) kit: RefAny,
    /// The screen `--screen` asked for.
    pub(crate) screen: Screen,
    pub(crate) accounts: Vec<Account>,
    /// The account shown, an index into `accounts`.
    pub(crate) current: Option<usize>,
    /// Secrets in memory for this run, by account id: typed in the wizard, read from the
    /// keyring, or the headless test password.
    pub(crate) secrets: HashMap<String, Secret>,
    /// The keyring operation whose answer is awaited (one at a time: azul keeps only the last
    /// answer).
    pub(crate) keyring: Option<KeyringOp>,
    /// Keyring calls waiting for the one in flight ([`keyring_call`]).
    pub(crate) keyring_queue: std::collections::VecDeque<KeyringCall>,
    /// DKIM private keys in memory for this run, by account id: made under Account Settings,
    /// Sending, or read from the keyring at Send / Receive; `None`: the keyring has none.
    pub(crate) dkim_keys: HashMap<String, Option<Secret>>,

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
    pub(crate) selection: ListSelection,
    pub(crate) search: String,
    /// 0: all mail, 1: unread.
    pub(crate) scope: usize,
    pub(crate) newest_first: bool,

    // -- the reading pane --
    pub(crate) open: Option<OpenMessage>,
    pub(crate) show_reading: bool,
    pub(crate) show_todo: bool,
    pub(crate) plain_text: bool,
    /// The reading pane's zoom in percent (the status bar's zoom, remembered across restarts).
    pub(crate) zoom: f32,
    /// File > About is open (the standard About dialog).
    pub(crate) about_open: bool,

    // -- the ribbon and the backstage --
    pub(crate) ribbon_tab: usize,
    /// The backstage page shown (`ui_backstage::PAGE_*`); `None` is the mail view.
    pub(crate) backstage: Option<usize>,
    /// File > Print: the open message as a PDF and its preview.
    pub(crate) print: Option<ui_backstage::PrintJob>,
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
    /// The Azlin data root (the kit's): the shared task store is under it.
    pub(crate) data_root: PathBuf,
    /// The shared task store's tasks (`todo.rs`), open ones first.
    pub(crate) tasks: Vec<azul_pim::task::Task>,
    /// The list a task typed into the To-Do bar goes to.
    pub(crate) task_list: String,
    pub(crate) task_text: String,
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
    /// The pictures the mail carries itself (`cid:` parts): shown without asking.
    pub(crate) inline: Vec<message::InlinePicture>,
    /// The Thread downloading this mail's web pictures ("Download pictures"), while it runs.
    pub(crate) pictures_thread: Option<ThreadId>,
    /// What this mail's web pictures have downloaded so far.
    pub(crate) budget: pictures::Budget,
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
    /// A DKIM private key stored.
    StoreDkim,
    /// The DKIM private key read to sign this account's mail.
    GetDkim { account: String },
}

/// A keyring call: store `secret` under `key`, or (`secret` is `None`) read `key`.
pub(crate) struct KeyringCall {
    pub(crate) op: KeyringOp,
    pub(crate) key: String,
    pub(crate) secret: Option<Secret>,
}

impl MailApp {
    fn create(root: DriveFolder, kit: RefAny, screen: Screen, accounts: Vec<Account>) -> MailApp {
        let today = local_today();
        let n = accounts.len();
        let data_root = kit_data_root(&kit);
        // Read once, before the window (as the kit reads settings.json).
        let todo = todo::load(&data_root);
        // The view as it was left (File > Options shows theme and mode; these are the View
        // tab's toggles, in the same settings.json).
        let settings = {
            let mut kit = kit.clone();
            let settings = kit
                .downcast_ref::<kit::Kit>()
                .map(|k| k.settings.clone())
                .unwrap_or_default();
            settings
        };
        let view = |key: &str, default: bool| settings.get_bool(key, default);
        MailApp {
            root,
            kit,
            screen,
            accounts,
            current: None,
            secrets: HashMap::new(),
            keyring: None,
            keyring_queue: std::collections::VecDeque::new(),
            dkim_keys: HashMap::new(),
            folders: vec![Vec::new(); n],
            folder: None,
            favorite_picked: false,
            groups_open: vec![true; n + 1],
            nav_collapsed: view(ui_main::SET_NAVIGATION_COLLAPSED, false),
            module: 0,
            entries: Vec::new(),
            flags: LocalFlags::create(),
            view: Vec::new(),
            rows: Vec::new(),
            first_row: 0,
            selection: ListSelection::create(),
            search: String::new(),
            scope: 0,
            newest_first: view(ui_main::SET_NEWEST_FIRST, true),
            open: None,
            show_reading: view(ui_main::SET_READING_PANE, true),
            show_todo: view(ui_main::SET_TODO_BAR, true),
            about_open: false,
            plain_text: view(ui_main::SET_PLAIN_TEXT, false),
            zoom: ui_main::zoom_setting(settings.get(ui_main::SET_ZOOM)),
            ribbon_tab: 0,
            backstage: None,
            print: None,
            editor: None,
            sync: SyncState::Idle,
            notice: String::new(),
            composes: Vec::new(),
            next_compose: 1,
            data_root,
            tasks: todo.tasks,
            task_list: todo.new_task_list,
            task_text: String::new(),
            calendar: today,
            today,
        }
    }

    pub(crate) fn current_account(&self) -> Option<&Account> {
        self.current.and_then(|i| self.accounts.get(i))
    }

    /// The synced files of account `index`.
    pub(crate) fn store_of(&self, index: usize) -> Option<MailStore> {
        self.accounts
            .get(index)
            .map(|a| MailStore::new(account::mail_root(&self.root, a)))
    }

    /// The current account's synced files.
    pub(crate) fn store(&self) -> Option<MailStore> {
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
            self.selection = ListSelection::create();
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
        self.selection = ListSelection::create();
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
        let (view, error, inline) = match bytes {
            Ok(bytes) => match message::parse_view(&bytes) {
                Some(view) => (Some(view), String::new(), message::inline_pictures(&bytes)),
                None => (
                    None,
                    String::from("This file is not a mail message."),
                    Vec::new(),
                ),
            },
            Err(e) => (None, format!("Could not read {}: {e}", entry.path), Vec::new()),
        };
        println!("AZMAIL_OPEN {folder} {uid}");
        let was_read = self.flags.is_read(&entry);
        // The HTML part on its paper: the mail's own pictures shown, its web pictures off
        // until the reader asks for them.
        let options = html::PictureOptions {
            web: false,
            inline: pictures::content_ids(&inline),
        };
        let sanitized = view
            .as_ref()
            .and_then(|v| v.html.as_deref())
            .map(|part| html::sanitize_mail(part, &options));
        self.open = Some(OpenMessage {
            folder,
            entry,
            view,
            error,
            sanitized,
            pictures: false,
            inline,
            pictures_thread: None,
            budget: pictures::Budget::default(),
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
pub(crate) fn read_index(store: &MailStore, folder: &str) -> Vec<IndexEntry> {
    store
        .get(&store::index_key(folder))
        .map(|bytes| store::index_from_jsonl(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

/// A folder's local flags; none when there is no file.
pub(crate) fn read_flags(store: &MailStore, folder: &str) -> LocalFlags {
    store
        .get(&listing::flags_key(folder))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| LocalFlags::from_json(&text))
        .unwrap_or_else(LocalFlags::create)
}

/// The synced folders of `store` with their unread counts.
fn folder_infos(store: &MailStore) -> Vec<FolderInfo> {
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

/// A new id for what leaves the process (a file name in the data tree, the S3 bucket later):
/// azul's mint seeded from the OS (`Uuid::v4` is the same sequence in every run).
pub(crate) fn new_id() -> String {
    azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed())
        .as_str()
        .to_string()
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Runs `f` on the app's state with a clone of the app's `RefAny` (for threads and callback
/// data); `None` when `data` is not the app.
pub(crate) fn with_app<R>(data: &mut RefAny, f: impl FnOnce(&mut MailApp, RefAny) -> R) -> Option<R> {
    let app = data.clone();
    let mut guard = data.downcast_mut::<MailApp>()?;
    Some(f(&mut guard, app))
}

// ==== The keyring ====

/// A keyring answer, by name (never with the secret).
fn keyring_outcome(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::Stored => "stored",
        KeyringResult::Retrieved(_) => "retrieved",
        KeyringResult::Deleted => "deleted",
        KeyringResult::NotFound => "not found",
        KeyringResult::Denied => "denied",
        KeyringResult::Unavailable => "unavailable",
        KeyringResult::Error => "error",
    }
}

/// The answer to the awaited keyring operation (a window event of the main window).
pub(crate) extern "C" fn on_keyring_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let OptionKeyringResult::Some(result) = info.get_keyring_result() else {
        return Update::DoNothing;
    };
    let outcome = keyring_outcome(&result);
    println!("AZMAIL_KEYRING {outcome}");
    with_app(&mut data, |s, app| {
        match (s.keyring.take(), result) {
            (None, _) => return Update::DoNothing,
            (Some(KeyringOp::Store), KeyringResult::Stored) => {
                s.notice = String::from("The password is saved in the system keyring.");
            }
            (Some(KeyringOp::Store), _) => {
                s.notice = format!(
                    "The password could not be saved in the system keyring ({outcome}): AzMail \
                     keeps it only until it is closed."
                );
            }
            (Some(KeyringOp::Get { account }), KeyringResult::Retrieved(secret)) => {
                s.secrets
                    .insert(account.clone(), Secret::new(secret.as_str().to_string()));
                s.notice.clear();
                if s.current_account().map(|a| a.id.as_str()) == Some(account.as_str()) {
                    start_sync(s, &mut info, app);
                }
            }
            (Some(KeyringOp::Get { account }), _) => {
                s.sync = SyncState::Idle;
                ui_account::open_settings_with_error(
                    s,
                    &account,
                    format!(
                        "Enter the password again: the system keyring has none for this \
                         account ({outcome})."
                    ),
                );
            }
            (Some(KeyringOp::StoreDkim), KeyringResult::Stored) => {
                s.notice = String::from("The DKIM key is saved in the system keyring.");
            }
            (Some(KeyringOp::StoreDkim), _) => {
                s.notice = format!(
                    "The DKIM key could not be saved in the system keyring ({outcome}): AzMail \
                     keeps it only until it is closed - create a new key then."
                );
            }
            (Some(KeyringOp::GetDkim { account }), KeyringResult::Retrieved(secret)) => {
                s.dkim_keys
                    .insert(account.clone(), Some(Secret::new(secret.as_str().to_string())));
                if s.current_account().map(|a| a.id.as_str()) == Some(account.as_str()) {
                    start_sync(s, &mut info, app);
                }
            }
            (Some(KeyringOp::GetDkim { account }), _) => {
                // Asked once per run: signed mail waits in the Outbox until a new key is made.
                s.dkim_keys.insert(account.clone(), None);
                s.notice = format!(
                    "The system keyring has no DKIM key for this account ({outcome}): signed \
                     mail waits in the Outbox until you create a new key under Account \
                     Settings, Sending."
                );
                if s.current_account().map(|a| a.id.as_str()) == Some(account.as_str()) {
                    start_sync(s, &mut info, app);
                }
            }
        }
        keyring_next(s, &mut info);
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// Asks the keyring now, or after the call in flight (azul keeps only the last answer, so
/// the calls go one at a time; [`on_keyring_result`] starts the next).
pub(crate) fn keyring_call(s: &mut MailApp, info: &mut CallbackInfo, call: KeyringCall) {
    s.keyring_queue.push_back(call);
    keyring_next(s, info);
}

/// Starts the next queued keyring call when none is in flight.
fn keyring_next(s: &mut MailApp, info: &mut CallbackInfo) {
    if s.keyring.is_some() {
        return;
    }
    let Some(call) = s.keyring_queue.pop_front() else {
        return;
    };
    match &call.secret {
        Some(secret) => info.keyring_store(call.key.clone(), secret.expose(), false),
        None => info.keyring_get(call.key.clone()),
    }
    s.keyring = Some(call.op);
}

/// Whether a keyring read for `account` (the sign-in secret, or with `dkim` the DKIM key) is
/// in flight or queued.
fn keyring_reading(s: &MailApp, account: &str, dkim: bool) -> bool {
    let reads = |op: &KeyringOp| match op {
        KeyringOp::Get { account: a } => !dkim && a == account,
        KeyringOp::GetDkim { account: a } => dkim && a == account,
        _ => false,
    };
    s.keyring.as_ref().is_some_and(|op| reads(op)) || s.keyring_queue.iter().any(|c| reads(&c.op))
}

/// Puts `secret` for `account_id` into the OS keyring and keeps it in memory for this run.
pub(crate) fn remember_secret(s: &mut MailApp, info: &mut CallbackInfo, account_id: &str, secret: Secret) {
    keyring_call(
        s,
        info,
        KeyringCall {
            op: KeyringOp::Store,
            key: account::keyring_key(account_id),
            secret: Some(secret.clone()),
        },
    );
    s.secrets.insert(account_id.to_string(), secret);
}

/// Puts a new DKIM private key for `account_id` into the OS keyring and keeps it in memory for
/// this run.
pub(crate) fn remember_dkim_key(s: &mut MailApp, info: &mut CallbackInfo, account_id: &str, key: Secret) {
    keyring_call(
        s,
        info,
        KeyringCall {
            op: KeyringOp::StoreDkim,
            key: send::dkim_keyring_key(account_id),
            secret: Some(key.clone()),
        },
    );
    s.dkim_keys.insert(account_id.to_string(), Some(key));
}

// ==== Send / Receive: on an azul Thread, progress back through write-backs ====

/// Starts Send / Receive for the current account: with its secret the sync thread; without one
/// a keyring read whose answer starts it (`on_keyring_result`).
pub(crate) fn start_sync(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny) {
    if matches!(s.sync, SyncState::Running { .. }) {
        return;
    }
    let Some(account) = s.current_account().cloned() else {
        return;
    };
    let secret = s.secrets.get(&account.id).cloned().or_else(test_secret);
    let Some(secret) = secret else {
        if !keyring_reading(s, &account.id, false) {
            keyring_call(
                s,
                info,
                KeyringCall {
                    op: KeyringOp::Get {
                        account: account.id.clone(),
                    },
                    key: account::keyring_key(&account.id),
                    secret: None,
                },
            );
        }
        s.sync = SyncState::Done(String::from(
            "Reading the password from the system keyring...",
        ));
        return;
    };
    // An account that signs its mail (client-side DKIM) needs its key for the Outbox: read
    // once per run, after the password.
    let signs_from_keyring = send::SendSettings::load(&s.root, &account.id)
        .dkim
        .is_some_and(|dkim| dkim.key_file.is_none());
    if signs_from_keyring && !s.dkim_keys.contains_key(&account.id) {
        if !keyring_reading(s, &account.id, true) {
            keyring_call(
                s,
                info,
                KeyringCall {
                    op: KeyringOp::GetDkim {
                        account: account.id.clone(),
                    },
                    key: send::dkim_keyring_key(&account.id),
                    secret: None,
                },
            );
        }
        s.sync = SyncState::Done(String::from(
            "Reading the DKIM key from the system keyring...",
        ));
        return;
    }
    let dkim_key = s.dkim_keys.get(&account.id).cloned().flatten();
    let mail_root = account::mail_root(&s.root, &account);
    println!("AZMAIL_SYNC_START {}", mail_root.path().display());
    let thread = ThreadId::unique();
    s.sync = SyncState::Running {
        thread,
        account: account.id.clone(),
        status: format!("Connecting to {}...", account.imap.host),
        percent: 0.0,
    };
    let init = SyncInit {
        account,
        secret,
        mail_root,
        azmail_root: s.root.clone(),
        extra_ca: test_ca(),
        dkim_key,
    };
    info.add_thread(thread, Thread::create(RefAny::new(init), app, sync_thread));
}

/// Stops a running Send / Receive: the thread sees TerminateThread between batches, writes what
/// it has and stops.
pub(crate) fn stop_sync(s: &mut MailApp, info: &mut CallbackInfo) {
    if let SyncState::Running { thread, status, .. } = &mut s.sync {
        info.remove_thread(*thread);
        *status = String::from("Stopping...");
    }
}

/// What the sync thread starts with.
#[derive(Clone)]
struct SyncInit {
    account: Account,
    secret: Secret,
    mail_root: DriveFolder,
    /// The AzMail folder (for the outbox).
    azmail_root: DriveFolder,
    extra_ca: Option<PathBuf>,
    /// The DKIM private key the Outbox's mail is signed with, when the account signs.
    dkim_key: Option<Secret>,
}

/// What the thread reports.
#[derive(Clone)]
enum SyncEvent {
    Progress(Progress),
    /// The sync's result, and what the outbox retry did (sent, still queued, failed).
    Finished(Result<SyncReport, SyncError>, (usize, usize, usize)),
}

struct SyncMessage {
    account: String,
    event: SyncEvent,
}

/// Runs Send / Receive: the IMAP sync, then the outbox's queued mail. The connections block
/// here, never in a callback.
extern "C" fn sync_thread(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_ref::<SyncInit>()
        .map(|job| SyncInit::clone(&job))
    else {
        return;
    };
    let outcome = run_sync(&job, &mut sender, &mut receiver);
    // "Send" of Send / Receive: whatever waits in the outbox gets another try.
    let mut settings = send::SendSettings::load(&job.azmail_root, &job.account.id);
    settings.dkim_key = job.dkim_key.clone();
    // Submission signs in with the secret the sync signed in with.
    settings.sign_in = Some(job.secret.clone());
    let mut outbox = (0, 0, 0);
    for (_, status) in send::retry_outbox(&job.azmail_root, &job.account.id, &settings, false) {
        match status {
            send::SendStatus::Sent { .. } => outbox.0 += 1,
            send::SendStatus::Queued { .. } => outbox.1 += 1,
            send::SendStatus::Failed { .. } => outbox.2 += 1,
        }
    }
    post(
        &mut sender,
        &job.account.id,
        SyncEvent::Finished(outcome, outbox),
    );
}

fn run_sync(
    job: &SyncInit,
    sender: &mut ThreadSender,
    receiver: &mut ThreadReceiver,
) -> Result<SyncReport, SyncError> {
    let mut source =
        imap_client::ImapSource::connect(&job.account, &job.secret, job.extra_ca.as_deref())?;
    let store = MailStore::new(job.mail_root.clone());
    let options = SyncOptions {
        now: now_unix(),
        ..SyncOptions::default()
    };
    let account = job.account.id.clone();
    let mut on_progress = |progress: Progress| -> bool {
        let delivered = post(sender, &account, SyncEvent::Progress(progress));
        let mut stop = false;
        while let OptionThreadSendMsg::Some(message) = receiver.recv() {
            if matches!(message, ThreadSendMsg::TerminateThread) {
                stop = true;
            }
        }
        delivered && !stop
    };
    let result = sync::sync_account(&mut source, &store, &options, &mut on_progress);
    source.logout();
    result
}

fn post(sender: &mut ThreadSender, account: &str, event: SyncEvent) -> bool {
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany: RefAny::new(SyncMessage {
            account: account.to_string(),
            event,
        }),
        callback: WriteBackCallback {
            cb: on_sync_event,
            ctx: OptionRefAny::None,
        },
    }))
}

/// A report from the sync thread, on the UI thread.
extern "C" fn on_sync_event(mut app: RefAny, mut payload: RefAny, _info: CallbackInfo) -> Update {
    let Some((account, event)) = payload
        .downcast_ref::<SyncMessage>()
        .map(|m| (m.account.clone(), m.event.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MailApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let running_this = matches!(&s.sync, SyncState::Running { account: a, .. } if *a == account);
    match event {
        SyncEvent::Progress(progress) => {
            if !running_this {
                return Update::DoNothing;
            }
            if let SyncState::Running {
                status, percent, ..
            } = &mut s.sync
            {
                match progress {
                    Progress::Folder {
                        index,
                        count,
                        display,
                    } => {
                        *status = format!("Receiving {display} (folder {} of {count})", index + 1);
                        *percent = 0.0;
                    }
                    Progress::Messages {
                        display,
                        done,
                        total,
                    } => {
                        *status = format!("Receiving {display}: {done} of {total} messages");
                        *percent = if total == 0 {
                            100.0
                        } else {
                            done as f32 * 100.0 / total as f32
                        };
                    }
                }
            }
        }
        SyncEvent::Finished(Ok(report), (sent, queued, failed)) => {
            println!(
                "AZMAIL_SYNC_DONE fetched={} reused={} folders={}",
                report.fetched(),
                report.reused(),
                report.folders.len()
            );
            eprintln!(
                "[azmail] synced {} folder(s): {} new message(s)",
                report.folders.len(),
                report.fetched()
            );
            let mut text = if report.fetched() == 0 {
                String::from("All folders are up to date.")
            } else {
                format!("{} new messages.", report.fetched())
            };
            if sent + queued + failed > 0 {
                text.push_str(&format!(
                    " Outbox: {sent} sent, {queued} waiting, {failed} failed."
                ));
            }
            s.sync = SyncState::Done(text);
            s.reload_folders();
            s.reload_messages();
        }
        SyncEvent::Finished(Err(e), _) => {
            println!("AZMAIL_SYNC_FAILED {e}");
            eprintln!("[azmail] sync failed: {e}");
            if matches!(e, SyncError::Auth(_)) {
                // A wrong password in memory (or the keyring) would fail again: ask for it.
                s.secrets.remove(&account);
                ui_account::open_settings_with_error(s, &account, format!("Sign-in failed: {e}"));
            }
            s.sync = SyncState::Failed(format!("Send/Receive error: {e}"));
            s.reload_folders();
            s.reload_messages();
        }
    }
    Update::RefreshDom
}

// ==== Writing files: on an azul Thread, never in a callback ====

/// A write the UI asks for.
#[derive(Clone)]
pub(crate) enum IoJob {
    /// A new or edited account: `account.json` and SEND's `sending.json`.
    SaveAccount {
        root: DriveFolder,
        account: Account,
        settings: send::SendSettings,
        editing: bool,
    },
    /// A folder's read / flag marks.
    SaveFlags {
        store_root: DriveFolder,
        folder: String,
        flags: LocalFlags,
    },
    /// A new DKIM key for the account editor (making an RSA key takes a moment).
    DkimKey,
    /// The DKIM / DMARC / SPF records in DNS, for the account editor.
    DkimCheck {
        selector: String,
        domain: String,
        public_key: String,
    },
    /// File > Print: the message's PDF, as `key` in the AzMail folder (`exports/<name>.pdf`).
    SavePdf {
        root: DriveFolder,
        key: String,
        bytes: Vec<u8>,
    },
}

/// What a write did.
#[derive(Clone)]
pub(crate) enum IoDone {
    AccountSaved {
        account: Account,
        path: PathBuf,
        editing: bool,
    },
    AccountFailed(String),
    FlagsSaved,
    Failed(String),
    DkimKey(Result<dkim::KeyPair, String>),
    DkimChecked(dkim::DnsReport),
    /// The PDF of File > Print is written: its key and its file.
    PdfSaved {
        key: String,
        path: PathBuf,
    },
}

/// Runs `job` on a thread of the window whose callback asks.
pub(crate) fn spawn_io(info: &mut CallbackInfo, app: RefAny, job: IoJob) {
    info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(job), app, io_thread));
}

extern "C" fn io_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init.downcast_ref::<IoJob>().map(|job| IoJob::clone(&job)) else {
        return;
    };
    let done = match job {
        IoJob::SaveAccount {
            root,
            account,
            settings,
            editing,
        } => match account::save(&root, &account) {
            Ok(path) => match settings.save(&root, &account.id) {
                Ok(_) => IoDone::AccountSaved {
                    account,
                    path,
                    editing,
                },
                Err(e) => IoDone::AccountFailed(format!("Could not write the sending settings: {e}")),
            },
            Err(e) => IoDone::AccountFailed(format!("Could not write the account file: {e}")),
        },
        IoJob::SaveFlags {
            store_root,
            folder,
            flags,
        } => match MailStore::new(store_root).put(
            &listing::flags_key(&folder),
            flags.to_json().as_bytes(),
        ) {
            Ok(()) => IoDone::FlagsSaved,
            Err(e) => IoDone::Failed(format!("Could not save the read marks: {e}")),
        },
        IoJob::DkimKey => IoDone::DkimKey(dkim::generate_key()),
        IoJob::DkimCheck {
            selector,
            domain,
            public_key,
        } => IoDone::DkimChecked(dkim::dns_report(&selector, &domain, &public_key)),
        IoJob::SavePdf { root, key, bytes } => {
            match MailStore::new(root.clone()).put(&key, &bytes) {
                Ok(()) => IoDone::PdfSaved {
                    path: key.split('/').fold(root.path(), |path, name| path.join(name)),
                    key,
                },
                Err(e) => IoDone::Failed(format!("Could not write {key}: {e}")),
            }
        }
    };
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany: RefAny::new(done),
        callback: WriteBackCallback {
            cb: on_io_done,
            ctx: OptionRefAny::None,
        },
    }));
}

/// Saves the shown folder's marks (after a message was opened or flagged).
pub(crate) fn save_flags(s: &MailApp, info: &mut CallbackInfo, app: RefAny, flags: LocalFlags) {
    let (Some(store), Some(folder)) = (s.store(), s.folder.clone()) else {
        return;
    };
    spawn_io(
        info,
        app,
        IoJob::SaveFlags {
            store_root: store.folder().clone(),
            folder,
            flags,
        },
    );
}

/// A write is done.
extern "C" fn on_io_done(mut app: RefAny, mut payload: RefAny, mut info: CallbackInfo) -> Update {
    let Some(done) = payload.downcast_ref::<IoDone>().map(|d| IoDone::clone(&d)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, |s, app| match done {
        IoDone::AccountSaved {
            account,
            path,
            editing,
        } => {
            println!("AZMAIL_ACCOUNT_SAVED {}", path.display());
            ui_account::account_saved(s, &mut info, app, account, editing);
            Update::RefreshDom
        }
        IoDone::AccountFailed(error) => {
            if let Some(editor) = s.editor.as_mut() {
                editor.error = error;
            } else {
                s.notice = error;
            }
            Update::RefreshDom
        }
        IoDone::FlagsSaved => Update::DoNothing,
        IoDone::Failed(error) => {
            s.notice = error;
            Update::RefreshDom
        }
        IoDone::DkimKey(result) => {
            ui_account::dkim_key_made(s, result);
            Update::RefreshDom
        }
        IoDone::DkimChecked(report) => {
            ui_account::dkim_checked(s, &report);
            Update::RefreshDom
        }
        IoDone::PdfSaved { key, path } => {
            ui_backstage::printed(s, &key, path);
            Update::RefreshDom
        }
    })
    .unwrap_or(Update::DoNothing)
}

// ==== Start ====

fn user_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
        .filter(|p| !p.as_os_str().is_empty())
}

/// The Azlin data root the kit resolved (`--data-dir`, `AZLIN_DATA`, else `Azlin` in the
/// user's data folder).
pub(crate) fn kit_data_root(kit_ref: &RefAny) -> PathBuf {
    let mut kit_ref = kit_ref.clone();
    let root = kit_ref
        .downcast_ref::<kit::Kit>()
        .map(|k| k.data_root.clone());
    root.unwrap_or_default()
}

pub fn start() {
    let args = match AppArgs::from_env(&args::SPEC) {
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
    let kit_ref = kit::create_kit(
        args::SPEC,
        args::ABOUT,
        &args::SHORTCUTS,
        &args::APP_CATEGORIES,
        args.clone(),
    );
    let azmail_var = std::env::var(account::DATA_VAR).ok();
    let data_root = kit_data_root(&kit_ref);
    let root_path = account::data_root(azmail_var.as_deref(), &data_root);
    if azmail_var.as_deref().map_or(true, |v| v.trim().is_empty()) {
        // Once: the folder an older AzMail kept in the user's data folder.
        if let Some(legacy) = account::legacy_root(user_data_dir().as_deref()) {
            match account::migrate_legacy_root(&legacy, &root_path) {
                Ok(true) => eprintln!(
                    "[azmail] moved {} to {}",
                    legacy.display(),
                    root_path.display()
                ),
                Ok(false) => {}
                Err(e) => eprintln!("[azmail] {} could not be moved: {e}", legacy.display()),
            }
        }
    }
    // Every file of AzMail goes through the data tree's one drive (the AzMail folder is a
    // folder of it); an AZMAIL_DATA outside the tree is a drive of its own.
    let root = DriveFolder::of(&root_path, &data_root);
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
        root_path.display()
    );
    let screen = Screen::of(&args);
    let mut state = MailApp::create(root, kit_ref.clone(), screen, accounts);
    if !state.accounts.is_empty() {
        state.show_account(0);
    }
    // The window is the mail window from the first start on - with no account it is empty and
    // its message list offers Add Account (as File > Info does); no wizard stands in front.
    match screen {
        Screen::AddAccount => ui_account::open_wizard(&mut state, None),
        Screen::Settings => ui_account::open_settings(&mut state),
        Screen::Backstage => state.backstage = Some(ui_backstage::PAGE_INFO),
        Screen::Mail | Screen::Compose | Screen::Reply => {}
    }

    // The app theme and the mode from settings.json (a --theme / --mode switch wins for this
    // run); the window: NoTitle (the app draws the title row), --size, a minimum size.
    let mut config = kit::app_config(&kit_ref);
    // Haiku's icons under the Material names, searched first.
    icons::register(&mut config.icon_provider);
    let mut window = kit::window_options(
        &kit_ref,
        ui_main::layout_main,
        (1280.0, 860.0),
        (800.0, 520.0),
        ui_main::on_main_window_created,
    );
    window.window_state.window_id = AzString::from(MAIN_WINDOW_ID);
    App::create(RefAny::new(state), config).run(window);
}
