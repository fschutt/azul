//! AzMail: a mail client in the shape of Outlook 2010, on the public azul API.
//!
//! - **The window is always the real one**: with no account it is the same ribbon, folder pane,
//!   message list and reading pane, empty - the list says "No account yet" and offers Add
//!   Account, as File > Info does. No wizard stands in front of the window.
//! - **Writing needs no account**: New E-mail always opens a message window. Without an account
//!   its From line is typed, and the mail is sent from this computer straight to the
//!   recipients' mail servers (SEND's direct route); its draft, the sent mail and what waits in
//!   its Outbox are Local Folders (`<AzMail folder>/local/`, `account::LOCAL_ID`), a mailbox of
//!   the navigation pane after the accounts. Send / Receive sends what waits there.
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
//!   Options, About), Options, Exit.
//! - **File > Options** is the kit's settings page in a window of its own, as Outlook 2010's
//!   Options dialog (`ui_options.rs`); OK and Cancel close it, the main window stays as it was.
//! - **New / Reply / Reply All / Forward** open a second window (`ui_compose.rs`): From, To / Cc
//!   / Bcc, Subject, a formatting ribbon, azul's shared rich-text editor, attachments; Save writes
//!   a draft into the Drafts folder, Send hands the mail to `send::send_mail` on a `Thread` and
//!   closes the window once it is sent (it is then in Sent Items).
//! - **Azlin accounts** keep the mailbox as files in the user's Azlin drive (`AZLIN_MAIL.md`,
//!   `azlin.rs`, `azlin_sync.rs`): Send / Receive signs in at the token server when the drive's
//!   credentials run out - under the account's keyring lock every AzMail of this user takes
//!   turns at, the keyring re-read first and the rotated drive token written there before the
//!   lock is let go, so a second AzMail on the account never spends a spent token -, sends the
//!   Outbox, then syncs the drive's folders; Archive, Junk, Delete, Move and the read / flag
//!   marks are written into the drive on a `Thread`, a saved draft too.
//!
//! Environment:
//! - `AZMAIL_DATA`: the AzMail folder (default: `AzMail` in the user's data folder).
//! - `AZMAIL_TEST_PASSWORD`, `AZMAIL_TEST_CA`: with `AZ_BACKEND=headless` only, the password to
//!   sign in with and a PEM certificate to trust (a test server's). A headless run never
//!   touches the real keyring either: azul serves it from memory.
//! - `AZLIN_TOKEN_URL`, `AZLIN_S3_URL` (over the shared Azlin config's `endpoints`; the switches
//!   `--azlin-token-url`, `--azlin-s3-url` win): where the Azlin services are.
//!
//! On stdout, for scripts: `AZMAIL_ACCOUNT_SAVED <file>`, `AZMAIL_SYNC_START <mail folder>`,
//! `AZMAIL_SYNC_DONE fetched=<n> reused=<n> folders=<n> pushed=<n> removed=<n>`,
//! `AZMAIL_SYNC_FAILED <why>`,
//! `AZMAIL_KEYRING <outcome>`, `AZMAIL_OPEN <folder> <uid>`, `AZMAIL_COMPOSE_OPEN <window id>
//! <kind>`, `AZMAIL_DRAFT_SAVED <window id> <uid>`, `AZMAIL_SEND_START <window id>`,
//! `AZMAIL_SEND_DONE <window id> sent|queued|failed <message id or reason>`,
//! `AZMAIL_COMPOSE_CLOSED <window id>`, `AZMAIL_PRINT_PDF <folder> <uid> <bytes>`,
//! `AZMAIL_PRINT_PREVIEW pages=<n> shown=<bool>`, `AZMAIL_PRINTED <file>`,
//! `AZMAIL_OUTBOX_START local`, `AZMAIL_OUTBOX_DONE sent=<n> queued=<n> failed=<n>` (Send /
//! Receive of Local Folders' Outbox), `AZMAIL_SETTINGS_OPEN <window id>`,
//! `AZMAIL_SETTINGS_WINDOW_CLOSED <window id>` (File > Options' window; the kit prints
//! `AZMAIL_SETTINGS_CLOSED ok|cancel`); for Azlin accounts `AZMAIL_AZLIN_SIGNED_IN <drive id>`
//! (the token server gave new credentials), `AZMAIL_AZLIN_DRIVE_CREATED <drive id>`,
//! `AZMAIL_AZLIN_DONE <action> <n>` / `AZMAIL_AZLIN_FAILED <action> <why>` (move, delete, marks,
//! fetch), `AZMAIL_DRAFT_UPLOADED <window id> <key>` / `AZMAIL_DRAFT_UPLOAD_FAILED <window id>
//! <why>`. The secret is never printed.

// The mail logic without azul types lives in azul-mail-core, so a headless process (the Azlin
// Bridge) runs the same code; it keeps its module names here (`azmail::send`, `crate::folders`).
pub use azmail_core::{
    account, args, auth, azlin, dkim, folders, mail_drive, message, mutf7, send, store, submit,
};

pub mod azlin_sync;
pub mod compose;
pub mod html;
pub mod ids;
pub mod imap_client;
pub mod listing;
pub mod pictures;
pub mod sample;
pub mod sending;
pub mod sync;
pub mod todo;
#[cfg(test)]
mod l10n_tests;
mod ui_account;
mod ui_backstage;
mod ui_bridge;
mod ui_compose;
mod ui_main;
mod ui_options;

// The tests' helpers (a temporary folder, the SMTP sink) are azul-mail-core's `testing` ones.
#[cfg(test)]
use azmail_core::testutil;

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
use azul_appkit::ui as kit;
use azul_storage::{azul_keyring::AzulKeyring, azul_transport::AzulTransport, Drive};
use listing::{FolderInfo, ListRow, LocalFlags};
use message::MessageView;
use store::{DriveFolder, FolderState, IndexEntry, MailStore};
use sync::{Progress, SyncError, SyncOptions, SyncReport};

/// The main window's id (the debug server addresses a window by it).
pub(crate) const MAIN_WINDOW_ID: &str = "azmail-main";

/// What AzMail's requests to the Azlin services say they are.
pub(crate) const USER_AGENT: &str = "AzMail";

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
    /// The mailbox shown: an index into `accounts`, or [`MailApp::local_index`] for Local
    /// Folders (the mail written without an account).
    pub(crate) current: Option<usize>,
    /// Local Folders hold mail (a draft, a sent mail, mail in their Outbox): the navigation pane
    /// shows them beside the accounts too.
    pub(crate) local_used: bool,
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
    /// Where the Azlin services are for this run (the shared Azlin config, the environment,
    /// the switches): the token server a new Azlin account signs in at, an S3 override.
    pub(crate) endpoints: azlin::Endpoints,
    /// The keyring as the sync thread reads and writes it (azul's blocking keyring calls),
    /// with the locks every AzMail of this user takes turns at (`<data root>/.azlin/locks`,
    /// which never leaves the computer): an Azlin account's session is refreshed and written
    /// there only ([`azlin::refresh_shared`]).
    pub(crate) shared_keyring: azcloud_kit::SharedKeyring,
    /// Azlin accounts whose session the user just gave (a typed drive token, a new drive's
    /// session): the next Send / Receive's thread writes it into the keyring under the
    /// account's lock before anything spends its token (never the UI thread: its write could
    /// land after the first refresh and put the spent token back).
    pub(crate) sessions_to_store: std::collections::HashSet<String>,

    // -- the navigation pane --
    /// Every mailbox's folders with their unread counts, by mailbox index (the accounts, then
    /// Local Folders); a mailbox's Outbox is one of them while mail waits in it.
    pub(crate) folders: Vec<Vec<FolderInfo>>,
    /// The folder shown (its key, in the current mailbox).
    pub(crate) folder: Option<String>,
    /// Which mailbox trees are open in the navigation pane: slot 0 (Favorites' once) unused,
    /// then one per account, then Local Folders'.
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
    /// An Azlin account's message that is in the drive but not here yet (Send/Receive fetched
    /// its header block only): it is being downloaded, and shown once it is.
    pub(crate) fetching: bool,
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

/// A keyring operation of the UI thread. An Azlin account's refreshed session is never stored
/// from here: the sync thread writes it under the account's lock ([`azlin::refresh_shared`]),
/// where a later write of the UI thread could put a spent drive token back over another AzMail's
/// newer one.
pub(crate) enum KeyringOp {
    Store,
    /// A secret read to sync this account.
    Get { account: String },
    /// A DKIM private key stored.
    StoreDkim,
    /// The DKIM private key read to sign this account's mail.
    GetDkim { account: String },
    /// The Azlin Bridge's password, read into the clipboard (Account Settings, Other programs).
    GetBridge,
}

/// A keyring call: store `secret` under `key`, or (`secret` is `None`) read `key`.
pub(crate) struct KeyringCall {
    pub(crate) op: KeyringOp,
    pub(crate) key: String,
    pub(crate) secret: Option<Secret>,
}

impl MailApp {
    fn create(
        root: DriveFolder,
        kit: RefAny,
        screen: Screen,
        accounts: Vec<Account>,
        endpoints: azlin::Endpoints,
    ) -> MailApp {
        let today = local_today();
        let n = accounts.len();
        let data_root = kit_data_root(&kit);
        // The drive index keeps this computer's copies of encrypted drives in the user's
        // cache folder between runs, as AzDrive does.
        mail_drive::set_index_cache_root(
            user_cache_dir().map(|dir| dir.join("AzMail").join("drive-index")),
        );
        // The locks of the account sessions beside the data root's own bookkeeping (`.azlin`
        // is never synced): every AzMail on this data root takes turns there.
        let shared_keyring = azcloud_kit::SharedKeyring::new(
            std::sync::Arc::new(azul_storage::azul_keyring::AzulKeyring::new()),
            azcloud_kit::LockDir::new(data_root.join(".azlin").join("locks")),
        );
        // Read once, before the window (as the kit reads settings.json).
        let todo = todo::load(&data_root);
        let settings = {
            let mut kit = kit.clone();
            let settings = kit
                .downcast_ref::<kit::Kit>()
                .map(|k| k.settings.clone())
                .unwrap_or_default();
            settings
        };
        let mut app = MailApp {
            root,
            kit,
            screen,
            accounts,
            current: None,
            local_used: false,
            secrets: HashMap::new(),
            keyring: None,
            keyring_queue: std::collections::VecDeque::new(),
            dkim_keys: HashMap::new(),
            endpoints,
            shared_keyring,
            sessions_to_store: std::collections::HashSet::new(),
            // The accounts, then Local Folders.
            folders: vec![Vec::new(); n + 1],
            folder: None,
            groups_open: vec![true; n + 2],
            nav_collapsed: false,
            module: 0,
            entries: Vec::new(),
            flags: LocalFlags::create(),
            view: Vec::new(),
            rows: Vec::new(),
            first_row: 0,
            selection: ListSelection::create(),
            search: String::new(),
            scope: 0,
            newest_first: true,
            open: None,
            show_reading: true,
            show_todo: true,
            about_open: false,
            plain_text: false,
            zoom: 100.0,
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
        };
        // The view as it was left: the View tab's switches and the zoom, in the kit's
        // settings.json (File > Options' Cancel reads them again the same way).
        ui_main::read_view_settings(&mut app, &settings);
        app
    }

    /// The account shown; `None` with Local Folders shown, or nothing.
    pub(crate) fn current_account(&self) -> Option<&Account> {
        self.current.and_then(|i| self.accounts.get(i))
    }

    /// Local Folders' place among the mailboxes: after the accounts.
    pub(crate) fn local_index(&self) -> usize {
        self.accounts.len()
    }

    /// Local Folders are the mailbox shown.
    pub(crate) fn shows_local(&self) -> bool {
        self.current == Some(self.local_index())
    }

    /// The navigation pane shows Local Folders: always without an account (where mail written
    /// without one goes), else while they hold mail.
    pub(crate) fn local_visible(&self) -> bool {
        self.accounts.is_empty() || self.local_used
    }

    /// The id of mailbox `index`: an account's, or Local Folders' (`account::LOCAL_ID`).
    pub(crate) fn mailbox_id(&self, index: usize) -> Option<String> {
        if index == self.local_index() {
            return Some(String::from(account::LOCAL_ID));
        }
        self.accounts.get(index).map(|a| a.id.clone())
    }

    /// The mail files of mailbox `index`: an account's synced files, or Local Folders'.
    pub(crate) fn store_of(&self, index: usize) -> Option<MailStore> {
        if index == self.local_index() {
            return Some(MailStore::new(account::account_dir(&self.root, account::LOCAL_ID)));
        }
        self.accounts
            .get(index)
            .map(|a| MailStore::new(account::mail_root(&self.root, a)))
    }

    /// The current mailbox's mail files.
    pub(crate) fn store(&self) -> Option<MailStore> {
        self.current.and_then(|i| self.store_of(i))
    }

    /// Where the messages of the current mailbox's folder `folder` are read from: the Outbox's
    /// are in the mailbox's own folder (`<id>/outbox/<entry>.eml`), every other folder's in its
    /// mail folder.
    pub(crate) fn message_store(&self, folder: &str) -> Option<MailStore> {
        if folder == listing::OUTBOX_KEY {
            let id = self.mailbox_id(self.current?)?;
            return Some(MailStore::new(account::account_dir(&self.root, &id)));
        }
        self.store()
    }

    /// The current mailbox's Outbox as list entries.
    fn outbox_list(&self) -> Vec<IndexEntry> {
        self.current
            .and_then(|i| self.mailbox_id(i))
            .map(|id| send::outbox_index(&send::outbox_entries(&self.root, &id)))
            .unwrap_or_default()
    }

    /// Reads every mailbox's folders and their unread counts (from the index and flag files):
    /// each account's synced folders, Local Folders' Drafts and Sent Items, and every Outbox
    /// with mail in it (Local Folders' always).
    pub(crate) fn reload_folders(&mut self) {
        let local = self.local_index();
        let mut folders = Vec::with_capacity(local + 1);
        let mut local_used = false;
        for i in 0..=local {
            let mut list = self.store_of(i).map(|s| folder_infos(&s)).unwrap_or_default();
            let waiting = self
                .mailbox_id(i)
                .map_or(0, |id| send::outbox_entries(&self.root, &id).len());
            if i == local {
                local_used = !list.is_empty() || waiting > 0;
                listing::with_local_folders(&mut list);
            }
            if waiting > 0 || i == local {
                list.push(listing::outbox_folder(waiting));
            }
            folders.push(list);
        }
        self.folders = folders;
        self.local_used = local_used;
        if self.groups_open.len() != local + 2 {
            self.groups_open.resize(local + 2, true);
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

    /// Reads the shown folder's index and flags (the Outbox: what waits in it), and rebuilds
    /// the list.
    pub(crate) fn reload_messages(&mut self) {
        let outbox = self.folder.as_deref() == Some(listing::OUTBOX_KEY);
        let (entries, flags) = match (self.store(), self.folder.as_ref()) {
            (Some(_), Some(_)) if outbox => (self.outbox_list(), LocalFlags::create()),
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

    /// Shows mailbox `index` (an account's Inbox, Local Folders' Drafts).
    pub(crate) fn show_account(&mut self, index: usize) {
        self.current = Some(index);
        self.folder = None;
        self.open = None;
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
        let store = self.message_store(&folder);
        // An Azlin account's big message Send/Receive left in the drive: downloaded first
        // (`fetch_if_needed`), shown once it is here.
        let fetching = store
            .as_ref()
            .is_some_and(|store| azlin_sync::needs_fetch(store, &entry));
        let bytes = match &store {
            _ if fetching => Err(String::new()),
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
            Err(_) if fetching => (
                None,
                format!(
                    "Downloading this message ({}) from the Azlin drive...",
                    azul::file::DiskSpace::format_bytes(entry.size)
                ),
                Vec::new(),
            ),
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
            fetching,
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

    /// The shown folder's unread count in the navigation pane, after a mark changed it (the
    /// Outbox's count is how many wait in it, not marks).
    pub(crate) fn refresh_unread_count(&mut self) {
        let unread = listing::unread_count(&self.entries, &self.flags);
        let (Some(index), Some(folder)) = (self.current, self.folder.clone()) else {
            return;
        };
        if folder == listing::OUTBOX_KEY {
            return;
        }
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
                let what = if s.accounts.iter().any(|a| a.id == account && a.is_azlin()) {
                    "the drive token"
                } else {
                    "the password"
                };
                ui_account::open_settings_with_error(
                    s,
                    &account,
                    format!(
                        "Enter {what} again: the system keyring has none for this account \
                         ({outcome})."
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
            (Some(KeyringOp::GetBridge), KeyringResult::Retrieved(secret)) => {
                ui_bridge::copy_password(s, &mut info, secret.as_str());
            }
            (Some(KeyringOp::GetBridge), _) => {
                ui_bridge::password_missing(s, outcome);
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

/// Puts `secret` for `account` into the OS keyring (an IMAP account's password or token) and
/// keeps it in memory for this run. An Azlin account's session goes into the keyring from the
/// next Send / Receive's thread, under the account's lock (`sessions_to_store`).
pub(crate) fn remember_secret(s: &mut MailApp, info: &mut CallbackInfo, account: &Account, secret: Secret) {
    if account.is_azlin() {
        s.sessions_to_store.insert(account.id.clone());
        s.secrets.insert(account.id.clone(), secret);
        return;
    }
    keyring_call(
        s,
        info,
        KeyringCall {
            op: KeyringOp::Store,
            key: account::secret_keyring_key(account),
            secret: Some(secret.clone()),
        },
    );
    s.secrets.insert(account.id.clone(), secret);
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
    // The headless test password is an IMAP account's: an Azlin account's secret is its
    // session (the rotating drive token), never a fixed one.
    let secret = s
        .secrets
        .get(&account.id)
        .cloned()
        .or_else(|| (!account.is_azlin()).then(test_secret).flatten());
    let Some(secret) = secret else {
        if !keyring_reading(s, &account.id, false) {
            keyring_call(
                s,
                info,
                KeyringCall {
                    op: KeyringOp::Get {
                        account: account.id.clone(),
                    },
                    key: account::secret_keyring_key(&account),
                    secret: None,
                },
            );
        }
        s.sync = SyncState::Done(String::from(if account.is_azlin() {
            "Reading the drive token from the system keyring..."
        } else {
            "Reading the password from the system keyring..."
        }));
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
    // A session the user just gave: this thread keeps it before it spends its token.
    let store_first = s.sessions_to_store.remove(&account.id);
    let mail_root = account::mail_root(&s.root, &account);
    println!("AZMAIL_SYNC_START {}", mail_root.path().display());
    let thread = ThreadId::unique();
    s.sync = SyncState::Running {
        thread,
        account: account.id.clone(),
        status: if account.is_azlin() {
            String::from("Connecting to the Azlin drive...")
        } else {
            format!("Connecting to {}...", account.imap.host)
        },
        percent: 0.0,
    };
    let init = SyncInit {
        account,
        secret,
        mail_root,
        azmail_root: s.root.clone(),
        extra_ca: test_ca(),
        dkim_key,
        endpoints: s.endpoints.clone(),
        keyring: s.shared_keyring.clone(),
        store_first,
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
    /// Where the Azlin services are (an Azlin account's token server and S3 override).
    endpoints: azlin::Endpoints,
    /// The keyring and the locks an Azlin account's session is refreshed under.
    keyring: azcloud_kit::SharedKeyring,
    /// An Azlin account's `secret` is new (typed, a new drive's): kept in the keyring first.
    store_first: bool,
}

/// What the thread reports.
#[derive(Clone)]
enum SyncEvent {
    Progress(Progress),
    /// An Azlin account switched sessions: signed in at its token server (the rotated drive
    /// token, the new credentials - in the keyring already, written under the account's lock,
    /// unless `unsaved` says why not) or read another AzMail's newer one from the keyring. For
    /// memory, before anything else happens.
    Session {
        secret: Secret,
        unsaved: Option<String>,
    },
    /// The sync's result, and what the outbox retry did (sent, still queued, failed).
    Finished(Result<SyncReport, SyncError>, (usize, usize, usize)),
}

struct SyncMessage {
    account: String,
    event: SyncEvent,
}

/// Runs Send / Receive: the IMAP sync, then the outbox's queued mail - or, for an Azlin
/// account, the outbox first (the copies it files in Sent go into the drive with this
/// Send/Receive), then the drive's folders. The connections block here, never in a callback.
extern "C" fn sync_thread(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_ref::<SyncInit>()
        .map(|job| SyncInit::clone(&job))
    else {
        return;
    };
    let (outcome, results) = if job.account.is_azlin() {
        // The local copy is this Thread's until the folders are synced.
        let _cache = azlin_sync::lock_cache();
        let results = retry_outboxes(&job);
        (run_azlin_sync(&job, &mut sender, &mut receiver), results)
    } else {
        let outcome = run_sync(&job, &mut sender, &mut receiver);
        (outcome, retry_outboxes(&job))
    };
    let outbox = outbox_counts(&results);
    post(
        &mut sender,
        &job.account.id,
        SyncEvent::Finished(outcome, outbox),
    );
}

/// "Send" of Send / Receive: whatever waits in the account's Outbox gets another try, and the
/// mail written before there was an account (Local Folders' Outbox) goes out with every
/// Send / Receive too.
fn retry_outboxes(job: &SyncInit) -> Vec<(String, send::SendStatus)> {
    let mut settings = send::SendSettings::load(&job.azmail_root, &job.account.id);
    settings.dkim_key = job.dkim_key.clone();
    // Submission signs in with the secret the sync signed in with - an IMAP account's: an
    // Azlin account's secret opens its drive, it is no password of a mail server.
    settings.sign_in = (!job.account.is_azlin()).then(|| job.secret.clone());
    let mut results = send::retry_outbox(&job.azmail_root, &job.account.id, &settings, false);
    results.extend(retry_local_outbox(&job.azmail_root));
    results
}

/// Tells the window how the sync goes; `false` when the window asked the Thread to stop.
fn report_progress(
    sender: &mut ThreadSender,
    receiver: &mut ThreadReceiver,
    account: &str,
    progress: Progress,
) -> bool {
    let delivered = post(sender, account, SyncEvent::Progress(progress));
    let mut stop = false;
    while let OptionThreadSendMsg::Some(message) = receiver.recv() {
        if matches!(message, ThreadSendMsg::TerminateThread) {
            stop = true;
        }
    }
    delivered && !stop
}

/// The token server's refusal as the sync's: a refused drive token (or a token server nobody
/// named) opens Account Settings with why.
fn azlin_error(e: azlin::TokenError) -> SyncError {
    match e {
        azlin::TokenError::Connect(why) => SyncError::Connect(why),
        azlin::TokenError::Protocol(why) => SyncError::Protocol(why),
        e @ azlin::TokenError::Refused { .. } => SyncError::Protocol(e.to_string()),
        e @ (azlin::TokenError::SignIn(_) | azlin::TokenError::Config(_)) => {
            SyncError::Auth(e.to_string())
        }
    }
}

/// New credentials at the account's token server - under the account's lock, spending only the
/// newest drive token ([`azlin::refresh_shared`]: another AzMail on this account may have
/// refreshed, and its session is taken from the keyring then; `refused` is the access key the
/// drive refused). The new session is in the keyring already (written before the lock was let
/// go); it goes to the window at once, for memory.
fn refresh_session(
    job: &SyncInit,
    link: &account::AzlinLink,
    session: &azlin::AzlinSession,
    refused: Option<&str>,
    transport: &AzulTransport,
    sender: &mut ThreadSender,
) -> Result<azlin::AzlinSession, SyncError> {
    let url = job.endpoints.token_url_for(&link.token_url).ok_or_else(|| {
        SyncError::Auth(account::FormError::NoTokenServer.to_string())
    })?;
    let server = azlin::TokenServer::new(&url, transport).map_err(azlin_error)?;
    let now = u64::try_from(now_unix()).unwrap_or(0);
    let refreshed = azlin::refresh_shared(
        &server,
        &job.keyring,
        &account::azlin_keyring_key(&job.account.id),
        session,
        refused,
        now,
    )
    .map_err(azlin_error)?;
    post(
        sender,
        &job.account.id,
        SyncEvent::Session {
            secret: Secret::new(refreshed.session.to_secret()),
            unsaved: refreshed.saved.err(),
        },
    );
    if !refreshed.adopted {
        println!("AZMAIL_AZLIN_SIGNED_IN {}", refreshed.session.drive_id);
    }
    Ok(refreshed.session)
}

/// One sync of the drive with `session`'s credentials.
fn sync_drive(
    job: &SyncInit,
    session: &azlin::AzlinSession,
    transport: &AzulTransport,
    options: &azlin_sync::AzlinOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<SyncReport, SyncError> {
    // An encrypted drive's incoming mail first: its drops into the folders the sync reads.
    mail_drive::receive_drops(
        session,
        job.endpoints.s3_url.as_deref(),
        Box::new(transport.clone()),
        &AzulKeyring::new(),
    )
    .map_err(azlin_sync::drive_error)?;
    let drive = mail_drive::open(
        session,
        job.endpoints.s3_url.as_deref(),
        Box::new(transport.clone()),
        std::sync::Arc::new(AzulKeyring::new()),
    )
    .map_err(azlin_sync::drive_error)?;
    azlin_sync::sync_account(&drive, &MailStore::new(job.mail_root.clone()), options, progress)
}

/// An Azlin account's Receive: signs in at the token server when the drive's credentials run
/// out, then syncs the drive's folders (`azlin_sync`); a drive that refuses credentials that
/// looked good (a clock off, keys revoked) gets one fresh sign-in.
fn run_azlin_sync(
    job: &SyncInit,
    sender: &mut ThreadSender,
    receiver: &mut ThreadReceiver,
) -> Result<SyncReport, SyncError> {
    let Some(link) = job.account.azlin.as_ref() else {
        return Err(SyncError::Protocol(String::from("not an Azlin account")));
    };
    let transport = AzulTransport::new(USER_AGENT);
    let now = now_unix();
    let mut session = azlin::AzlinSession::from_secret(job.secret.expose(), &link.drive_id);
    if job.store_first {
        // What the user just gave replaces what the keyring had, before its token is spent.
        let key = account::azlin_keyring_key(&job.account.id);
        if let Err(why) = azlin::store_shared(&job.keyring, &key, &session) {
            post(
                sender,
                &job.account.id,
                SyncEvent::Session {
                    secret: job.secret.clone(),
                    unsaved: Some(why),
                },
            );
        }
    }
    let mut refreshed = false;
    if session.needs_refresh(u64::try_from(now).unwrap_or(0)) {
        session = refresh_session(job, link, &session, None, &transport, sender)?;
        refreshed = true;
    }
    let options = azlin_sync::AzlinOptions {
        now,
        ..azlin_sync::AzlinOptions::default()
    };
    let account = job.account.id.clone();
    let first = sync_drive(job, &session, &transport, &options, &mut |p| {
        report_progress(sender, receiver, &account, p)
    });
    match first {
        Err(SyncError::Auth(_)) if !refreshed => {
            let refused = session.access_key_id.clone();
            session = refresh_session(job, link, &session, Some(&refused), &transport, sender)?;
            sync_drive(job, &session, &transport, &options, &mut |p| {
                report_progress(sender, receiver, &account, p)
            })
        }
        other => other,
    }
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
        SyncEvent::Session { secret, unsaved } => {
            // The old drive token is dead: the new session replaces it in memory. The keyring
            // has it already - the sync thread wrote it under the account's lock; a write from
            // here could put it back over another AzMail's newer one.
            s.secrets.insert(account, secret);
            let Some(why) = unsaved else {
                return Update::DoNothing;
            };
            s.notice = format!(
                "The Azlin drive's new token could not be saved in the system keyring ({why}): \
                 AzMail keeps it only until it is closed, then asks for a drive token again."
            );
        }
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
                "AZMAIL_SYNC_DONE fetched={} reused={} folders={} pushed={} removed={}",
                report.fetched(),
                report.reused(),
                report.folders.len(),
                report.pushed(),
                report.removed()
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

// ==== Send / Receive of Local Folders: the mail written without an account ====

/// How many mails of an Outbox retry went out, still wait, failed.
fn outbox_counts(results: &[(String, send::SendStatus)]) -> (usize, usize, usize) {
    let mut counts = (0, 0, 0);
    for (_, status) in results {
        match status {
            send::SendStatus::Sent { .. } => counts.0 += 1,
            send::SendStatus::Queued { .. } => counts.1 += 1,
            send::SendStatus::Failed { .. } => counts.2 += 1,
        }
    }
    counts
}

/// Every mail waiting in Local Folders' Outbox tried again, from this computer (their own
/// `sending.json`, else straight to the recipients' mail servers). Blocking: on a Thread.
fn retry_local_outbox(root: &DriveFolder) -> Vec<(String, send::SendStatus)> {
    let settings = send::SendSettings::load(root, account::LOCAL_ID);
    send::retry_outbox(root, account::LOCAL_ID, &settings, true)
}

/// Send / Receive with Local Folders shown (or no account at all): what waits in their Outbox
/// goes out, on a Thread; with nothing waiting the status bar says so.
pub(crate) fn send_local_outbox(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny) {
    if matches!(s.sync, SyncState::Running { .. }) {
        return;
    }
    if send::outbox_entries(&s.root, account::LOCAL_ID).is_empty() {
        s.notice = if s.accounts.is_empty() {
            String::from(
                "Nothing waits in the Outbox. To receive mail, add an account: File > Info > Add \
                 Account.",
            )
        } else {
            String::from("Nothing waits in the Outbox of Local Folders.")
        };
        return;
    }
    println!("AZMAIL_OUTBOX_START {}", account::LOCAL_ID);
    let thread = ThreadId::unique();
    s.sync = SyncState::Running {
        thread,
        account: String::from(account::LOCAL_ID),
        status: String::from("Sending the Outbox..."),
        percent: 0.0,
    };
    let job = LocalOutboxJob {
        root: s.root.clone(),
    };
    info.add_thread(thread, Thread::create(RefAny::new(job), app, local_outbox_thread));
}

/// What the Local Folders' Outbox thread is given.
#[derive(Clone)]
struct LocalOutboxJob {
    root: DriveFolder,
}

/// What it did: sent, still waiting, failed.
#[derive(Clone, Copy)]
struct LocalOutboxDone {
    counts: (usize, usize, usize),
}

extern "C" fn local_outbox_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_ref::<LocalOutboxJob>()
        .map(|job| LocalOutboxJob::clone(&job))
    else {
        return;
    };
    let counts = outbox_counts(&retry_local_outbox(&job.root));
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
        refany: RefAny::new(LocalOutboxDone { counts }),
        callback: WriteBackCallback {
            cb: on_local_outbox_done,
            ctx: OptionRefAny::None,
        },
    }));
}

/// Local Folders' Outbox was tried: the status bar says how it went, the folders show where
/// the mail is now (Sent Items, or still the Outbox).
extern "C" fn on_local_outbox_done(mut app: RefAny, mut payload: RefAny, _info: CallbackInfo) -> Update {
    let Some((sent, queued, failed)) = payload
        .downcast_ref::<LocalOutboxDone>()
        .map(|done| done.counts)
    else {
        return Update::DoNothing;
    };
    println!("AZMAIL_OUTBOX_DONE sent={sent} queued={queued} failed={failed}");
    with_app(&mut app, |s, _| {
        let text = format!("Outbox: {sent} sent, {queued} waiting, {failed} failed.");
        s.sync = SyncState::Done(text.clone());
        s.notice = text;
        s.reload_folders();
        s.reload_messages();
        Update::RefreshDomAllWindows
    })
    .unwrap_or(Update::DoNothing)
}

// ==== An Azlin account's drive: actions on an azul Thread ====

/// What a Thread needs to reach an Azlin account's drive: the account, its session (the
/// keyring's secret: the drive token and the S3 credentials), this run's endpoints and the
/// account's local copy.
#[derive(Clone)]
pub(crate) struct AzlinContext {
    pub(crate) account: Account,
    pub(crate) session: Secret,
    pub(crate) endpoints: azlin::Endpoints,
    pub(crate) store_root: DriveFolder,
}

impl AzlinContext {
    /// The Azlin account `account_id`'s, once it has a session (the wizard's drive token, a
    /// Send/Receive's sign-in); `None` for an IMAP account.
    pub(crate) fn of(s: &MailApp, account_id: &str) -> Option<AzlinContext> {
        let account = s
            .accounts
            .iter()
            .find(|a| a.id == account_id && a.is_azlin())?
            .clone();
        let session = s.secrets.get(account_id)?.clone();
        Some(AzlinContext {
            store_root: account::mail_root(&s.root, &account),
            account,
            session,
            endpoints: s.endpoints.clone(),
        })
    }

    /// The drive with the session's credentials. Never signs in - two Threads refreshing one
    /// drive token at once would make the token server revoke the device - so credentials that
    /// are missing or out of date are an error saying that Send/Receive signs in again.
    pub(crate) fn open_drive(&self) -> Result<std::sync::Arc<dyn Drive>, String> {
        let drive_id = self
            .account
            .azlin
            .as_ref()
            .map(|link| link.drive_id.as_str())
            .unwrap_or_default();
        let session = azlin::AzlinSession::from_secret(self.session.expose(), drive_id);
        if !session.is_valid_at(azul_storage::time::now_unix()) {
            return Err(String::from(
                "the Azlin drive's sign-in is out of date: Send/Receive (F9) signs in again",
            ));
        }
        mail_drive::open(
            &session,
            self.endpoints.s3_url.as_deref(),
            Box::new(AzulTransport::new(USER_AGENT)),
            std::sync::Arc::new(AzulKeyring::new()),
        )
        .map_err(|e| e.to_string())
    }
}

/// What an Azlin account's action does in the drive and in the local copy (`azlin_sync`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AzlinAction {
    /// The messages `uids` of the local folder `from` into the local folder `to`.
    Move {
        from: String,
        uids: Vec<u32>,
        to: String,
    },
    /// The messages `uids` of the local folder `folder`, for good (Delete in Deleted Items).
    Delete { folder: String, uids: Vec<u32> },
    /// The marks made here in the local folder `folder`, written into the drive.
    PushMarks { folder: String },
    /// The message `uid` of the local folder `folder`, downloaded whole (it is being opened).
    Fetch { folder: String, uid: u32 },
}

impl AzlinAction {
    /// The action as scripts read it: `move inbox archive`, `delete trash`, `marks inbox`,
    /// `fetch inbox 3`.
    pub(crate) fn describe(&self) -> String {
        match self {
            AzlinAction::Move { from, to, .. } => format!("move {from} {to}"),
            AzlinAction::Delete { folder, .. } => format!("delete {folder}"),
            AzlinAction::PushMarks { folder } => format!("marks {folder}"),
            AzlinAction::Fetch { folder, uid } => format!("fetch {folder} {uid}"),
        }
    }
}

/// Runs `action` in the drive, then in the local copy. Blocking: on an azul Thread, with the
/// local copy locked by the caller (`azlin_sync::lock_cache`). `Ok`: how many messages moved,
/// went or were marked, the bytes of a download.
fn run_azlin_action(context: &AzlinContext, action: &AzlinAction) -> Result<u64, String> {
    let drive = context.open_drive()?;
    let store = MailStore::new(context.store_root.clone());
    let done = match action {
        AzlinAction::Move { from, uids, to } => {
            azlin_sync::move_messages(&drive, &store, from, uids, to).map(|n| n as u64)
        }
        AzlinAction::Delete { folder, uids } => {
            azlin_sync::delete_messages(&drive, &store, folder, uids).map(|n| n as u64)
        }
        AzlinAction::PushMarks { folder } => {
            azlin_sync::push_marks(&drive, &store, folder).map(|n| n as u64)
        }
        AzlinAction::Fetch { folder, uid } => azlin_sync::fetch_message(
            &drive,
            &store,
            folder,
            *uid,
            azlin_sync::AzlinOptions::default().chunk,
        ),
    };
    done.map_err(|e| e.to_string())
}

/// Runs `action` of the account shown on a Thread; `false` (and a line in the status bar) when
/// it is no Azlin account, or one that has not signed in yet.
pub(crate) fn spawn_azlin(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny, action: AzlinAction) -> bool {
    let Some(id) = s
        .current_account()
        .filter(|a| a.is_azlin())
        .map(|a| a.id.clone())
    else {
        return false;
    };
    let Some(context) = AzlinContext::of(s, &id) else {
        s.notice = String::from("Send/Receive (F9) first: AzMail signs in to the Azlin drive then.");
        return false;
    };
    spawn_io(info, app, IoJob::Azlin { context, action });
    true
}

/// The message just opened is an Azlin account's that is still in the drive only (bigger than
/// what Send/Receive fetches): its download starts, the reading pane says so meanwhile.
pub(crate) fn fetch_if_needed(s: &mut MailApp, info: &mut CallbackInfo, app: RefAny) {
    let Some((folder, uid)) = s
        .open
        .as_ref()
        .filter(|open| open.fetching)
        .map(|open| (open.folder.clone(), open.entry.uid))
    else {
        return;
    };
    if !spawn_azlin(s, info, app, AzlinAction::Fetch { folder, uid }) {
        if let Some(open) = s.open.as_mut() {
            open.fetching = false;
            open.error = String::from(
                "This message is in the Azlin drive only: Send/Receive (F9) signs in, then it \
                 opens.",
            );
        }
    }
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
    /// A folder's read / flag marks; an Azlin account's are written into its drive right after.
    SaveFlags {
        store_root: DriveFolder,
        folder: String,
        flags: LocalFlags,
        azlin: Option<AzlinContext>,
    },
    /// An Azlin account's action, in the drive and here.
    Azlin {
        context: AzlinContext,
        action: AzlinAction,
    },
    /// The wizard's "Create a new drive": a drive at the token server `token_url`.
    CreateDrive { token_url: String, name: String },
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
    /// An Azlin account's action, and how it went.
    Azlin {
        action: AzlinAction,
        result: Result<u64, String>,
    },
    /// The wizard's new drive (its session), or why there is none.
    DriveCreated(Result<azlin::AzlinSession, String>),
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
            azlin,
        } => {
            // An Azlin account's local copy is one Thread's at a time.
            let _cache = azlin.as_ref().map(|_| azlin_sync::lock_cache());
            match MailStore::new(store_root).put(
                &listing::flags_key(&folder),
                flags.to_json().as_bytes(),
            ) {
                Err(e) => IoDone::Failed(format!("Could not save the read marks: {e}")),
                Ok(()) => match azlin {
                    None => IoDone::FlagsSaved,
                    Some(context) => {
                        let action = AzlinAction::PushMarks { folder };
                        let result = run_azlin_action(&context, &action);
                        IoDone::Azlin { action, result }
                    }
                },
            }
        }
        IoJob::Azlin { context, action } => {
            let _cache = azlin_sync::lock_cache();
            let result = run_azlin_action(&context, &action);
            IoDone::Azlin { action, result }
        }
        IoJob::CreateDrive { token_url, name } => {
            use azlin::CloudAccount;
            let transport = AzulTransport::new(USER_AGENT);
            let created = azlin::TokenServer::new(&token_url, &transport)
                .and_then(|server| server.create_drive(&name));
            IoDone::DriveCreated(created.map_err(|e| e.to_string()))
        }
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

/// Saves the shown folder's marks (after a message was opened or flagged). The Outbox keeps
/// none: it is no folder of files.
pub(crate) fn save_flags(s: &MailApp, info: &mut CallbackInfo, app: RefAny, flags: LocalFlags) {
    let (Some(store), Some(folder)) = (s.store(), s.folder.clone()) else {
        return;
    };
    if folder == listing::OUTBOX_KEY {
        return;
    }
    // An Azlin account's marks go into its drive right after (they wait in flags.json until
    // they are there: Send/Receive writes what is left).
    let azlin = s
        .current_account()
        .filter(|a| a.is_azlin())
        .and_then(|a| AzlinContext::of(s, &a.id));
    spawn_io(
        info,
        app,
        IoJob::SaveFlags {
            store_root: store.folder().clone(),
            folder,
            flags,
            azlin,
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
        IoDone::Azlin { action, result } => {
            match &result {
                Ok(n) => println!("AZMAIL_AZLIN_DONE {} {n}", action.describe()),
                Err(e) => println!("AZMAIL_AZLIN_FAILED {} {e}", action.describe()),
            }
            ui_main::azlin_action_done(s, &mut info, app, action, result)
        }
        IoDone::DriveCreated(result) => {
            ui_account::drive_created(s, result);
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

fn user_cache_dir() -> Option<PathBuf> {
    FilePath::get_cache_dir()
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

/// Where the Azlin services are for this run: the shared Azlin config's `endpoints` (the file
/// the kit found: `$AZLIN_CONFIG`, else `~/.azlin/config.json`; none for a `--shot`), then
/// `$AZLIN_TOKEN_URL` / `$AZLIN_S3_URL`, then the switches.
fn resolve_endpoints(kit_ref: &RefAny, flags: &azlin::Endpoints) -> azlin::Endpoints {
    let mut kit_ref = kit_ref.clone();
    let config_path = kit_ref
        .downcast_ref::<kit::Kit>()
        .and_then(|k| k.config_path.clone());
    // Read once at the start, before the window, as the kit reads the same file.
    let config = config_path.and_then(|path| std::fs::read_to_string(path).ok());
    let env = |name: &str| std::env::var(name).ok();
    azlin::Endpoints::resolve(config.as_deref(), &env, flags)
}

pub fn start() {
    let args = match args::MailArgs::from_env() {
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
        args.kit.clone(),
    );
    let endpoints = resolve_endpoints(&kit_ref, &args.endpoints);
    if let Some(url) = &endpoints.token_url {
        eprintln!("[azmail] the Azlin token server of this run: {url}");
    }
    // The resolvers of the DKIM / DMARC / SPF check: the switch, else the environment, else
    // microdns' public ones.
    let dns = args
        .dns_servers
        .clone()
        .or_else(|| std::env::var(dkim::DNS_SERVERS_VAR).ok())
        .filter(|v| !v.trim().is_empty());
    if let Some(text) = dns {
        match dkim::parse_dns_servers(&text) {
            Some(servers) => dkim::use_dns_servers(servers),
            None => eprintln!("[azmail] no IP address in the DNS servers {text:?}: left out"),
        }
    }
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
    if args.kit.sample {
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
    let screen = Screen::of(&args.kit);
    // The From line of mail written without an account records what was typed, not a choice
    // of File > Options: its Cancel leaves it.
    kit::keep_on_cancel(&kit_ref, ui_compose::SET_LOCAL_FROM);
    let mut state = MailApp::create(root, kit_ref.clone(), screen, accounts, endpoints);
    if state.accounts.is_empty() {
        // Local Folders' tree (Drafts, Sent Items, Outbox), with nothing shown yet.
        state.reload_folders();
    } else {
        state.show_account(0);
    }
    // The window is the mail window from the first start on - with no account it is empty and
    // its message list offers Add Account (as File > Info does); no wizard stands in front.
    // The message window (`--screen compose` / `reply`) and File > Options (`options`) open
    // over it once it is up (`ui_main::on_main_window_created`).
    match screen {
        Screen::AddAccount => ui_account::open_wizard(&mut state, None),
        Screen::Settings => ui_account::open_settings(&mut state),
        Screen::Backstage => state.backstage = Some(ui_backstage::PAGE_INFO),
        Screen::Mail | Screen::Compose | Screen::Reply | Screen::Options => {}
    }

    // The app theme and the mode from settings.json (a --theme / --mode switch wins for this
    // run), and the kit's icons (Haiku's under flora, azul-icons-haiku); the window: NoTitle
    // (the app draws the title row), --size, a minimum size.
    let config = kit::app_config(&kit_ref);
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
