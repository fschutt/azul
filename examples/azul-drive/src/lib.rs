//! AzDrive: a file manager on the public azul API that looks and works like
//! Windows 8's File Explorer, with a Finder window for its body.
//!
//! The window is the S5 `BrowserShell` without a title row: Windows 8's
//! ribbon (`ui_ribbon`: File - a mini backstage in a popup under its tab -,
//! Home, Share, View; Computer at This PC) whose tab strip is the window's
//! title bar, over Explorer's address row (round Back / Forward, Recent
//! locations, Up, the breadcrumb box - a chevron per crumb dropping its
//! folders, the first crumbs folded into « when the path is long, a click on
//! its empty part for the typed path - with Refresh at its end, "Search
//! <folder>"). The window's title is the open place's path. Under it the body
//! is a Finder window in the app theme (`look`: flora's linen, leaves and
//! Garamond capitals; Office 2010's silver in flat): the navigation pane is
//! Finder's source list (`ui_sidebar`: FAVORITES - Quick access, the standard
//! folders, the pins -, LOCATIONS - This PC and the drives on this computer,
//! their folders listed when a row opens -, CLOUD - the S3 drives with their
//! state -, the transfers' activity, + and the actions); the content is a leaf
//! on the page (This PC's drive tiles, Quick access's pinned folders, or a
//! folder in one of Explorer's eight layouts - Details by default, the icon
//! layouts on azul's IconGrid - grouped or not, with check boxes or not) with
//! Finder's path bar and status line ("N items, N selected, X available") at
//! its foot; the preview pane OR the details pane at the right, a leaf too.
//! The Options and About are the backstage (View > Options, File > Help).
//!
//! A folder of any size opens at once (`listing`, `jobs`): its scan reads the
//! names and kinds with one `read_dir` - no stat per entry - and streams them
//! in batches into the rows (kept in the view's order), a navigation stops it;
//! the views are virtual (only the rows in view and a screen either side are
//! built); the rows in view get their sizes and dates (a stat each), the
//! folders among them their item counts (one `read_dir` each) and the pictures
//! their thumbnails, nothing else.
//!
//! The search box searches the open folder and every folder below it (`find`, on azul-search:
//! ripgrep's parallel walker and searcher, AzCode's find in files' engine too): the names first,
//! then - the Search tab's "File contents" - the files whose lines hold the text. The results
//! stream into the view as rows (Details, with their folder and the line they matched on) and
//! the status line counts them ("Searching... 1,234 found"); every key starts the search again
//! (the one running stops within milliseconds), Escape closes it. A cloud drive is searched by
//! name over a recursive listing - slower, and its files are not read.
//!
//! The drives: "Home" (the user's home folder, a `LocalDrive`), the local
//! folders and S3 drives the user added (AWS S3, Cloudflare R2, MinIO). Every
//! storage call goes through `azul_storage::Drive` on an azul `Thread`; no
//! callback waits on a drive. Copies, moves, uploads and downloads go
//! through ONE queue (one transfer at a time, progress in the source list):
//! a plan first (every file under a folder, every name taken at the target),
//! a dialog per conflict (Replace / Skip / Keep both, for all), then the run.
//! Delete moves an item into `.azdrive-trash/` on a local drive (Ctrl+Z
//! brings it back) and asks first on a cloud drive. Errors show in an
//! InfoBar over the content.
//!
//! The drives list is `<config dir>/azul-storage/drives.json` WITHOUT
//! secrets (shared with AzMail); the keys go to the OS keyring. The Azlin
//! data tree (azul-appkit's data root: `--data-dir`, `$AZLIN_DATA`, else
//! `<data dir>/Azlin`) is a drive of its own, "Azlin", and holds AzDrive's
//! files: `drive/settings.json` (the app theme and mode, azul-appkit's) and
//! `drive/view.json` (layout, sort, columns, panes, the Quick access pins),
//! written through the data tree's `LocalDrive` on a Thread.
//!
//! Command line ([`args`]; each switch wins over the variable an older build read, which is the
//! fallback when the switch is absent):
//! - `--home <dir>` (`$AZDRIVE_HOME`): the folder the Home drive shows (default: the user's home).
//! - `--drives <file>` (`$AZUL_DRIVES`): the drives file (default:
//!   `<config dir>/azul-storage/drives.json`).
//! - `--downloads <dir>` (`$AZDRIVE_DOWNLOADS`): where "Download" saves (default: the user's
//!   Downloads folder).
//! - `--data-dir <dir>` (`$AZLIN_DATA`): the data root (azul-appkit).
//! - `--dialogs inline` (`$AZDRIVE_DIALOGS=inline`): show the dialogs as a sheet inside the
//!   window instead of a modal dialog window (scripts: the debug server drives the main window).
//! - `--open <path>`: open at a place as the address bar names it (File > Open new window).
//!
//! On stdout, for scripts: `AZDRIVE_PLACE quick-access | this-pc | <drive id> <prefix or />`,
//! `AZDRIVE_LISTED <drive id> <prefix or /> <entries>`, `AZDRIVE_TREE <drive id>
//! <prefix or /> <folders>`, `AZDRIVE_SELECTED <n> <first key or ->`, `AZDRIVE_LAYOUT <name>`,
//! `AZDRIVE_SORT <column> <asc|desc>`, `AZDRIVE_GROUP <name>`, `AZDRIVE_PANES <nav> <preview> <details>`,
//! `AZDRIVE_TRANSFER <id> planned|conflict|done|failed|cancelled <n>`, `AZDRIVE_DONE <what> <key>`,
//! `AZDRIVE_DELETED <n>`, `AZDRIVE_RENAMING <key>`, `AZDRIVE_PREVIEW <kind> <key>`,
//! `AZDRIVE_CLIPBOARD copy|cut <n>`, `AZDRIVE_TESTED ok|error`, `AZDRIVE_ADDED <drive id>`,
//! `AZDRIVE_ADD_PAGE choose|buy|sources|form <source>`, `AZDRIVE_TIERS <n>`,
//! `AZDRIVE_CHECKOUT <checkout id>`, `AZDRIVE_CLAIMED <checkout id> <drive id>`,
//! `AZDRIVE_TITLE <window title>`, `AZDRIVE_RIBBON_TAB <tab>`, `AZDRIVE_FILE_MENU <action>`,
//! `AZDRIVE_NEW_WINDOW <path>`, `AZDRIVE_SEARCHING <text>`,
//! `AZDRIVE_SEARCHED <results> names|contents <text>`, `AZDRIVE_SEARCH_CLOSED`. Keys,
//! passwords, tokens and payment pages are never printed.
//!
//! Add drive (Home > Add drive, Computer > Add drive, the source list's "Add drive...", the
//! Options' Drives) is a modal dialog - a transient window over the window: Buy storage (Azlin's
//! tiers from the token server, a test drive on a development server, a checkout paid in the
//! browser; azcloud-kit) or Connect data source (azul-storage's catalog: S3-compatible storage,
//! a folder on this computer, OpenDAL's services with the feature `opendal`, PostgreSQL / MySQL /
//! SQLite browsed as tables with `sql`; a form per source, "Test connection", "Add drive").
//! `--token-url <url>` / `--profile <name>` name the token server (else `$AZLIN_TOKEN_URL`, the
//! shared Azlin config, the profile's).
//!
//! A checkout names a claim key of its own (azcloud-kit's `claim`): the token server seals the
//! paid drive's sign-up to it. The checkout and its key go on the keyring's list of unfinished
//! checkouts before the payment page opens, so a drive paid after "Stop waiting" joins the
//! source list in the background, and one paid while AzDrive was closed arrives at the next start
//! (azcloud-kit's `pending`). An Azlin drive's session is written by worker threads only, under
//! the drive's lock every AzDrive window shares: a second window never spends a drive token the
//! first one has spent (the token server would revoke the device).

mod actions;
/// The Add drive dialog as data: Buy storage, Connect data source, the source's form.
mod add_drive;
#[cfg(test)]
mod add_drive_tests;
/// What the Add drive dialog's buttons start, and the answers of its jobs.
mod add_flow;
pub mod args;
pub mod browse;
pub mod fileops;
/// The search box's search of the open folder and below it, as plain data.
pub mod find;
#[cfg(test)]
mod find_tests;
mod ids;
mod jobs;
pub mod keys;
/// The open folder's listing as it streams in, and the window of it the views build.
pub mod listing;
/// The body's looks in flat and flora, by day and at night.
mod look;
pub mod model;
pub mod preview;
/// The Add drive dialog's pages.
mod ui_add_drive;
mod ui_dialogs;
/// The search's results in the folder view.
mod ui_find;
mod ui_panes;
/// Windows 8's ribbon and its File menu.
mod ui_ribbon;
/// The navigation pane: Finder's source list.
mod ui_sidebar;
mod ui_view;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use azul::{
    callbacks::ShellOnPaneResizeCallbackType,
    css::DarkLightMode,
    error::KeyringResult,
    file::FilePath,
    prelude::*,
    shells::{BrowserShell, ShellPane, ShellPaneKind, ShellThemeScope},
    str::String as AzString,
    url::Url,
    widgets::{AlertKind, Dialog, IconGridView},
};
use azul_storage::{
    azul_transport::AzulTransport,
    config::{self, DriveEntry, DriveLocation, DrivesFile},
    key, Credentials, Drive, DriveError, LocalDrive,
};
use browse::{Entry, History, Place};
use fileops::{ConflictChoice, Plan, SourceItem, TransferKind, TransferQueue};
use jobs::{Done, FolderSize, Job, JobInit, Outcome, PreviewContent};
use model::{Selection, Settings, TypeAhead};

pub(crate) const USER_AGENT: &str = "AzDrive/0.2";
/// The view settings' key in the data tree (layout, sort, columns, panes, pins). The app theme
/// and mode are azul-appkit's `drive/settings.json` beside it.
pub(crate) const SETTINGS_KEY: &str = "drive/view.json";
/// The data tree's drive id (the data root, a `LocalDrive` now, the user's bucket later).
pub(crate) const DATA_ID: &str = "azlin";
/// What the About page and azul-appkit say about AzDrive.
pub(crate) const ABOUT: azul_appkit::AboutInfo = azul_appkit::AboutInfo {
    name: "AzDrive",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A file manager like Windows Explorer for the Azlin data tree, the folders of this \
              computer, S3 buckets, Azlin cloud storage and the data sources OpenDAL reaches \
              (WebDAV, FTP, Google Drive, Dropbox, OneDrive, GitHub, ...), with databases \
              browsed as tables.",
    license: "MIT",
    app_folder: "drive",
};
/// The Home drive's id (never in the drives file).
pub(crate) const HOME_ID: &str = "home";
/// The places "Recent locations" remembers.
const RECENT_PLACES: usize = 10;

/// A node of the navigation tree: a drive's folder.
pub(crate) type TreeKey = (String, String);

/// The content's leaf frame across: the page's padding either side and the leaf's rule.
const LEAF_FRAME_X: f32 = 2.0 * 8.0 + 2.0;
/// ... and down, with the leaf's foot: the page's padding, the rule, the path bar (24 px and its
/// rule) and the status line (22 px and its rule).
const LEAF_FRAME_Y: f32 = 2.0 * 8.0 + 2.0 + 25.0 + 23.0;

// ==== Drives ====

/// A session an Azlin drive switched to on a worker thread: refreshed there (and stored in the
/// keyring under the drive's lock before anything used it) or read from the keyring (another
/// window refreshed). The UI thread takes it as the drive's secret (`take_rotated`).
pub(crate) struct Rotated {
    pub drive_id: String,
    /// The keyring text of the session. Never printed.
    pub secret: String,
    /// Why the keyring did not take it: it lives in this process only.
    pub unsaved: Option<String>,
}

/// The sessions Azlin drives switched to on worker threads, for the UI thread.
pub(crate) type RotatedSessions = Arc<std::sync::Mutex<Vec<Rotated>>>;

/// A drive.
pub(crate) struct Slot {
    pub entry: DriveEntry,
    /// The text of the drive's keyring entry, once read or typed in: an S3 drive's keys, an
    /// Azlin drive's session, a data source's secret settings. Never printed.
    pub secret: Option<String>,
    /// The drive, once it could be opened; shared with the worker threads.
    pub drive: Option<Arc<dyn Drive>>,
}

impl Slot {
    pub fn new(entry: DriveEntry) -> Self {
        Slot {
            entry,
            secret: None,
            drive: None,
        }
    }

    /// Whether opening it waits for the keyring.
    pub fn locked(&self) -> bool {
        self.drive.is_none() && self.secret.is_none() && self.entry.needs_keyring()
    }

    /// The drive, opened on first use: an Azlin drive through azcloud-kit (it refreshes its
    /// credentials under the drive's lock in `keyring`, which it re-reads first - another window
    /// may have refreshed - and writes before it lets go; every session it switches to lands in
    /// `rotated`; its token server is the one the entry names, else `token_url`), every other
    /// one through azul-storage.
    pub fn open(
        &mut self,
        rotated: &RotatedSessions,
        token_url: Option<&str>,
        keyring: &azcloud_kit::SharedKeyring,
    ) -> Result<Arc<dyn Drive>, DriveError> {
        if let Some(drive) = &self.drive {
            return Ok(drive.clone());
        }
        let drive: Arc<dyn Drive> = if self.entry.azlin().is_some() {
            let secret = self.secret.as_deref().ok_or_else(|| DriveError::Denied {
                message: format!(
                    "\"{}\" has no session (the keyring has no entry for it)",
                    self.entry.name
                ),
            })?;
            let session = azcloud_kit::AzlinSession::from_keyring_secret(secret)
                .map_err(|e| DriveError::InvalidConfig(e.to_string()))?;
            let queue = rotated.clone();
            let id = self.entry.id.clone();
            let transports: azcloud_kit::drive::TransportFactory = Arc::new(|| {
                Box::new(AzulTransport::new(USER_AGENT)) as Box<dyn azul_storage::Transport>
            });
            Arc::new(azcloud_kit::AzlinDrive::new(
                &self.entry,
                session,
                token_url.unwrap_or_default(),
                keyring.clone(),
                transports,
                Box::new(
                    move |session: &azcloud_kit::AzlinSession, saved: Result<(), String>| {
                        if let Ok(mut queue) = queue.lock() {
                            queue.push(Rotated {
                                drive_id: id.clone(),
                                secret: session.to_keyring_secret(),
                                unsaved: saved.err(),
                            });
                        }
                    },
                ),
            )?)
        } else {
            Arc::from(self.entry.open_with_secret(
                self.secret.as_deref(),
                Box::new(AzulTransport::new(USER_AGENT)),
            )?)
        };
        self.drive = Some(drive.clone());
        Ok(drive)
    }

    /// An S3 drive's keys (an Azlin drive's current ones), for a presigned link.
    pub fn credentials(&self) -> Option<Credentials> {
        match &self.entry.location {
            DriveLocation::S3 { .. } => self
                .secret
                .as_deref()
                .and_then(|s| Credentials::from_keyring_secret(s).ok()),
            _ => None,
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self.entry.location, DriveLocation::Local { .. })
    }

    /// Home and the Azlin data tree: always there, never in the drives file, never removed.
    pub fn is_built_in(&self) -> bool {
        self.entry.id == HOME_ID || self.entry.id == DATA_ID
    }

    /// The drive is browsed, not written (a database, a web server, an IPFS gateway).
    pub fn read_only(&self) -> bool {
        azul_storage::catalog::service_of(&self.entry).is_some_and(|s| s.read_only)
    }

    /// The icon of the drive's tiles and tree rows.
    pub fn icon(&self) -> &'static str {
        if self.entry.id == HOME_ID {
            "home"
        } else if self.entry.id == DATA_ID {
            "folder_special"
        } else if self.is_local() {
            "storage"
        } else if self.entry.azlin().is_some() {
            "cloud_done"
        } else if let Some(spec) = azul_storage::catalog::service_of(&self.entry)
            .filter(|_| !matches!(self.entry.location, DriveLocation::S3 { .. }))
        {
            spec.icon
        } else {
            "cloud"
        }
    }

    /// What the drive is: "Local Disk", "S3 bucket", "Azlin cloud drive", "WebDAV", "SQLite
    /// database".
    pub fn kind(&self) -> String {
        azul_storage::catalog::kind_label(&self.entry)
    }
}

// ==== State ====

/// How serious a message of the InfoBar is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MessageKind {
    Info,
    Success,
    Warning,
    Error,
}

impl MessageKind {
    pub fn alert(self) -> AlertKind {
        match self {
            MessageKind::Info => AlertKind::Info,
            MessageKind::Success => AlertKind::Success,
            MessageKind::Warning => AlertKind::Warning,
            MessageKind::Error => AlertKind::Danger,
        }
    }
}

/// What the InfoBar over the content says.
#[derive(Clone, Debug)]
pub(crate) struct Message {
    pub kind: MessageKind,
    pub text: String,
}

/// Copy or Cut: the items to paste, their drive, whether a paste moves them.
#[derive(Clone, Debug)]
pub(crate) struct ClipboardItems {
    pub drive: String,
    pub items: Vec<SourceItem>,
    pub cut: bool,
}

/// F2: the item being renamed in place and the name typed so far.
#[derive(Clone, Debug)]
pub(crate) struct Renaming {
    pub key: String,
    pub text: String,
    pub is_folder: bool,
}

/// What Ctrl+Z takes back.
#[derive(Clone, Debug)]
pub(crate) enum UndoOp {
    Rename {
        drive: String,
        from: String,
        to: String,
    },
    /// A delete into the trash: (key, trash key) pairs.
    Trash {
        drive: String,
        gone: Vec<(String, String)>,
    },
    /// A move within one drive: (from, to) pairs.
    Move {
        drive: String,
        pairs: Vec<(String, String)>,
    },
    /// A new folder or file.
    Create { drive: String, key: String },
}

impl UndoOp {
    pub fn label(&self) -> String {
        match self {
            UndoOp::Rename { to, .. } => format!("Undo rename of \"{}\"", key::last_segment(to)),
            UndoOp::Trash { gone, .. } => format!("Undo delete of {} item(s)", gone.len()),
            UndoOp::Move { pairs, .. } => format!("Undo move of {} item(s)", pairs.len()),
            UndoOp::Create { key, .. } => format!("Undo new \"{}\"", key::last_segment(key)),
        }
    }
}

/// A transfer of the queue: what it copies from where to where.
pub(crate) struct TransferJob {
    pub kind: TransferKind,
    pub source_id: String,
    pub target_id: String,
    pub source: Arc<dyn Drive>,
    pub target: Arc<dyn Drive>,
    pub items: Vec<SourceItem>,
    pub target_prefix: String,
    pub same_drive: bool,
    /// Planned (`None` until the plan is back).
    pub plan: Option<Plan>,
    pub cancel: Arc<AtomicBool>,
    /// Decides every conflict without asking (downloads keep both).
    pub auto: Option<ConflictChoice>,
}

/// The preview pane's file and what it shows (`None` while it is fetched).
#[derive(Clone)]
pub(crate) struct PreviewState {
    pub key: String,
    pub content: Option<PreviewContent>,
}

/// What the Properties dialog shows.
#[derive(Clone)]
pub(crate) struct PropertiesState {
    /// The items (one or more); empty for the open folder or a drive.
    pub items: Vec<Entry>,
    /// A drive's properties (This PC, the DRIVE tab).
    pub drive: Option<usize>,
    /// General (0) or Details (1).
    pub tab: usize,
    /// The size count of the folders, once back.
    pub size: Option<Result<FolderSize, String>>,
    pub serial: u64,
    /// One file's metadata, once back.
    pub metadata: Option<Result<Vec<(String, String)>, String>>,
}

/// The dialog (or the inline sheet) open over the window.
pub(crate) enum Popup {
    /// "Add drive": Buy storage or Connect data source (or new secrets for a drive whose keyring
    /// entry is gone: the dialog's `editing`).
    AddDrive(add_drive::AddDialog),
    /// A delete that cannot be undone (a cloud drive, Shift+Delete, the trash).
    ConfirmDelete {
        drive_id: String,
        items: Vec<SourceItem>,
    },
    ConfirmForget {
        drive_id: String,
    },
    /// "Replace or Skip Files" for transfer `id`'s next conflict.
    Conflict {
        id: u64,
        apply_all: bool,
    },
    Properties(PropertiesState),
    /// "Move to / Copy to > Choose location": a typed path.
    ChooseLocation {
        kind: TransferKind,
        text: String,
        error: String,
    },
    /// The transfer queue, with Cancel: the running transfer as azul's ProgressDialog over the
    /// others. `auto`: it opened by itself (a long transfer) and closes when the queue is done.
    Transfers { auto: bool },
}

/// The source list: which sections are open, which drives and folders show their folders,
/// whose folders are listed (the folder prefixes, one listing per opening), which are being
/// listed.
#[derive(Default)]
pub(crate) struct TreeState {
    /// FAVORITES: Quick access, the standard folders, the pins.
    pub favorites_open: bool,
    /// LOCATIONS: This PC and the drives on this computer.
    pub locations_open: bool,
    /// CLOUD: the S3 drives and "Add drive".
    pub cloud_open: bool,
    pub expanded: HashSet<TreeKey>,
    pub loaded: HashMap<TreeKey, Vec<String>>,
    pub listing: HashSet<TreeKey>,
    /// Nodes to list once their drive's keys are read.
    pub pending: Vec<TreeKey>,
}

/// A column edge being dragged in the Details header.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ColumnDrag {
    pub column: browse::Column,
    pub start_x: f32,
    pub start_width: f32,
}

/// The whole app.
pub(crate) struct DriveState {
    pub slots: Vec<Slot>,
    /// Where the window is.
    pub place: Place,
    pub history: History,
    /// The places visited last (Recent locations), newest first.
    pub recent: Vec<Place>,
    /// The open folder's rows, in the view's sort order (a scan's batches merge into them).
    pub entries: Vec<Entry>,
    /// Nothing of the open folder has arrived yet ("Loading...").
    pub loading: bool,
    /// The scan of the open folder has handed over its last batch.
    pub listing_done: bool,
    /// The open folder could not be listed (its drive did not open, its keys could not be
    /// read, the scan failed): the content says so instead of calling the folder empty.
    pub listing_failed: bool,
    /// The listing the rows belong to; an answer for an older one is dropped.
    pub list_serial: u64,
    /// The running scan's stop: set when the window goes elsewhere, so a big folder left
    /// half-read is not read to its end.
    pub list_cancel: Arc<AtomicBool>,
    /// A refresh's rows, gathered while the rows it replaces still show; they take their place
    /// when the scan ends (no folder blinks empty on F5).
    pub refreshing: Option<Vec<Entry>>,
    /// The rows whose size and date were asked for (the ones in view; a sort's).
    pub stats_asked: HashSet<String>,
    /// The item counts of the open folder's subfolders (their keys), and the ones asked for
    /// during this listing (a listing asks again; the old count shows until the new one is in).
    pub counts: HashMap<String, usize>,
    pub counts_asked: HashSet<String>,
    /// Where the folder view's last scroll left it: its offset and its height (px; the
    /// height as the view last drew, 0 before it has).
    pub view_scroll: (f32, f32),
    /// The folder view's width as it last drew (its virtual view records it; 0 before it has):
    /// the rows in view are counted in the columns the view draws.
    pub view_width: f32,
    pub selection: Selection,
    /// The selected drive tile of This PC.
    pub selected_drive: Option<usize>,
    /// The selected pin of Quick access.
    pub selected_pin: Option<usize>,
    pub type_ahead: TypeAhead,
    pub settings: Settings,
    /// The search box's text.
    pub search: String,
    /// The search of the open folder and every folder below it, while the search box holds
    /// text: its results are the rows the view shows.
    pub find: Option<find::FindState>,
    /// The searches started so far (a batch of an older one is dropped).
    pub find_serial: u64,
    /// The Search tab's Refine (Date modified, Kind, Size); forgotten when the search closes.
    pub refines: find::Refines,
    /// The item to select once the open folder is listed (Open file location of a result).
    pub select_when_listed: Option<String>,
    pub editing_path: bool,
    pub renaming: Option<Renaming>,
    pub column_drag: Option<ColumnDrag>,
    /// The items being dragged in the window (their drive, their items).
    pub dragging: Option<(String, Vec<SourceItem>)>,
    pub tree: TreeState,
    /// The standard folders the Home drive holds (Desktop, Documents, ...): FAVORITES' rows.
    pub standard_folders: Vec<ui_sidebar::Favorite>,
    /// Closed group headers (by label) of the folder view and This PC.
    pub groups_closed: HashSet<String>,
    /// The icon layouts' grid as its last event left it: the first row in view, a rubber band
    /// in progress (its selection is the app's, handed in on every build).
    pub grid_view: IconGridView,
    /// The splitters' shares: the navigation pane's of the window, the content's beside the
    /// right pane (preview or details).
    pub pane_ratios: (f32, f32),
    /// The backstage, open on its page (0 the Options, 1 About).
    pub backstage: Option<usize>,
    /// The ribbon's tab chosen last (the place shows it where it has it).
    pub ribbon_tab: ui_ribbon::RibbonTabKind,
    /// AzDrive's own settings as the Options found them when they opened: what their Cancel
    /// puts back (`reload_settings`).
    pub settings_found: Option<Settings>,
    pub clipboard: Option<ClipboardItems>,
    pub queue: TransferQueue,
    pub transfers: HashMap<u64, TransferJob>,
    pub undo: Vec<UndoOp>,
    pub preview: Option<PreviewState>,
    /// One file's metadata for the details pane, by key.
    pub metadata: HashMap<String, Result<Vec<(String, String)>, String>>,
    /// Items listed at a drive's root (the first page), once listed.
    pub root_counts: HashMap<String, usize>,
    /// A local drive's volume: (total, free) bytes.
    pub disk: HashMap<String, (u64, u64)>,
    pub message: Option<Message>,
    pub popup: Option<Popup>,
    pub popups_opened: u64,
    pub keyring_waiting: Option<KeyringOp>,
    pub keyring_queue: VecDeque<(KeyringOp, KeyringCall)>,
    pub drives_file: Option<PathBuf>,
    /// The data tree (the data root) the view settings are written into.
    pub settings_drive: Option<LocalDrive>,
    pub downloads: PathBuf,
    pub open_dir: PathBuf,
    /// AzDrive's folder in the user's cache folder (a cloud drive's last listing); `None` in a
    /// `--shot` run (nothing kept) or on a system without one.
    pub cache_dir: Option<PathBuf>,
    pub inline_dialogs: bool,
    /// Worker threads running.
    pub running: u32,
    /// Deletes so far (names their folders in the trash).
    pub trash_serial: u32,
    /// The window's width, for the grid's rows (the arrow keys).
    pub window_width: f32,
    /// The window's height: the estimate of the rows in view until the folder view has drawn
    /// (and says how tall it is).
    pub window_height: f32,
    /// The open folder's pictures as thumbnails (`None`: none can be made).
    pub thumbnails: HashMap<String, Option<azul::image::ImageRef>>,
    /// The pictures whose thumbnails are being made.
    pub thumbnails_pending: HashSet<String>,
    /// The preview's sound while it plays (dropping it stops it).
    pub audio: Option<azul::audio::AudioSink>,
    /// azul-appkit's kit: the data root, the theme and mode saved in `drive/settings.json`, the
    /// Options' Appearance / Data / Shortcuts / About sections, the `--shot` timer.
    pub kit: RefAny,
    /// The Azlin token server of this run (Buy storage; the refreshes of Azlin drives that
    /// name none): `--token-url`, the environment, the shared Azlin config, the profile.
    pub token: azcloud_kit::TokenEndpoint,
    /// The sessions Azlin drives switched to on worker threads, for the slots' secrets.
    pub rotated: RotatedSessions,
    /// The keyring as the worker threads read and write it (azul's blocking keyring calls),
    /// with the locks every AzDrive of this user shares beside the drives file: an Azlin
    /// drive's session (its refreshes), the unfinished checkouts.
    pub keyring: azcloud_kit::SharedKeyring,
    /// The background claims of unfinished checkouts run (one job at a time).
    pub claiming: bool,
}

impl DriveState {
    /// The backstage page showing: the Options only while azul-appkit's settings page is open
    /// (its OK / Cancel close it), About while it is chosen.
    pub fn backstage_shown(&self) -> Option<usize> {
        self.backstage
            .filter(|page| *page != 0 || azul_appkit::ui::settings_open(&self.kit))
    }

    /// No preview any more - and no sound from it.
    pub fn clear_preview(&mut self) {
        self.preview = None;
        self.audio = None;
    }

    pub fn slot_index(&self, drive_id: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.entry.id == drive_id)
    }

    /// The drive of the open folder.
    pub fn current_drive(&self) -> Option<usize> {
        match &self.place {
            Place::QuickAccess | Place::ThisPc => None,
            Place::Folder { drive, .. } => self.slot_index(drive),
        }
    }

    /// The id of the open folder's drive.
    pub fn current_drive_id(&self) -> Option<String> {
        match &self.place {
            Place::Folder { drive, .. } => Some(drive.clone()),
            _ => None,
        }
    }

    /// The open folder's prefix (`""` on This PC and Quick access too).
    pub fn prefix(&self) -> &str {
        match &self.place {
            Place::QuickAccess | Place::ThisPc => "",
            Place::Folder { prefix, .. } => prefix,
        }
    }

    /// The rows shown: the search's results while a search is open (the walk left the hidden
    /// items out unless they show), else the open folder's rows - hidden items only when asked.
    pub fn visible_entries(&self) -> Vec<&Entry> {
        if let Some(find) = &self.find {
            return find.shown();
        }
        self.entries
            .iter()
            .filter(|e| self.settings.show_hidden || !e.is_hidden())
            .filter(|e| browse::matches_search(e, &self.search))
            .collect()
    }

    /// The layout the folder view draws: the search's results always in Details (a row each,
    /// with its folder and the line it matched on), else the user's.
    pub fn view_layout(&self) -> model::ViewLayout {
        if self.find.is_some() {
            model::ViewLayout::Details
        } else {
            self.settings.layout
        }
    }

    /// The grouping the folder view draws: none for the search's results (they come in the
    /// order they were found: names first), else the user's.
    pub fn view_grouping(&self) -> model::GroupBy {
        if self.find.is_some() {
            model::GroupBy::None
        } else {
            self.settings.group_by
        }
    }

    /// The keys of the rows shown, in order.
    pub fn visible_keys(&self) -> Vec<String> {
        self.visible_entries()
            .iter()
            .map(|e| e.key.clone())
            .collect()
    }

    /// The selected rows, in the shown order.
    pub fn selected_entries(&self) -> Vec<&Entry> {
        // Nothing selected (the usual case) asks no row: a folder of 100,000 items would hash
        // every key once per caller, several times per build of the window.
        if self.selection.is_empty() {
            return Vec::new();
        }
        self.visible_entries()
            .into_iter()
            .filter(|e| self.selection.contains(&e.key))
            .collect()
    }

    /// The selected items as transfer items.
    pub fn selected_items(&self) -> Vec<SourceItem> {
        self.selected_entries()
            .into_iter()
            .map(SourceItem::of)
            .collect()
    }

    /// The one selected row.
    pub fn single_selected(&self) -> Option<&Entry> {
        let key = self.selection.single()?;
        self.entry(key)
    }

    /// The row of `key`: a search's result while a search is open, else the open folder's.
    pub fn entry(&self, key: &str) -> Option<&Entry> {
        match &self.find {
            Some(find) => find.entry(key),
            None => self.entries.iter().find(|e| e.key == key),
        }
    }

    /// The drives as `(id, name)`, for the typed path.
    pub fn drive_names(&self) -> Vec<(String, String)> {
        self.slots
            .iter()
            .map(|s| (s.entry.id.clone(), s.entry.name.clone()))
            .collect()
    }

    /// The name of the place's drive.
    pub fn drive_name(&self, place: &Place) -> String {
        match place {
            Place::QuickAccess => String::from(browse::QUICK_ACCESS),
            Place::ThisPc => String::from(browse::THIS_PC),
            Place::Folder { drive, .. } => self
                .slot_index(drive)
                .map(|i| self.slots[i].entry.name.clone())
                .unwrap_or_else(|| drive.clone()),
        }
    }

    /// What a place is called: the drive, or its folder.
    pub fn place_title(&self, place: &Place) -> String {
        match place {
            Place::QuickAccess => String::from(browse::QUICK_ACCESS),
            Place::ThisPc => String::from(browse::THIS_PC),
            Place::Folder { prefix, .. } if prefix.is_empty() => self.drive_name(place),
            Place::Folder { prefix, .. } => key::last_segment(prefix).to_string(),
        }
    }

    pub fn place_name(&self) -> String {
        self.place_title(&self.place)
    }

    pub fn place_line(&self) -> String {
        match &self.place {
            Place::QuickAccess => String::from("quick-access"),
            Place::ThisPc => String::from("this-pc"),
            Place::Folder { drive, prefix } => format!(
                "{drive} {}",
                if prefix.is_empty() { "/" } else { prefix }
            ),
        }
    }

    /// Whether `drive_id` is a folder on this computer.
    pub fn is_local_drive(&self, drive_id: &str) -> bool {
        self.slot_index(drive_id)
            .is_some_and(|i| self.slots[i].is_local())
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            kind: MessageKind::Info,
            text: text.into(),
        });
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            kind: MessageKind::Success,
            text: text.into(),
        });
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            kind: MessageKind::Warning,
            text: text.into(),
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        eprintln!("[azdrive] {text}");
        self.message = Some(Message {
            kind: MessageKind::Error,
            text,
        });
    }

    /// Clears a message that was not an error.
    pub fn clear_notice(&mut self) {
        if self
            .message
            .as_ref()
            .is_some_and(|m| m.kind != MessageKind::Error)
        {
            self.message = None;
        }
    }

    /// The items per row of a grid layout, for the arrow keys (the content
    /// is the window minus the navigation pane and the right pane).
    pub fn grid_columns(&self) -> usize {
        if !self.view_layout().is_grid() {
            return 1;
        }
        let mut width = self.window_width;
        if self.settings.navigation_pane {
            width *= 1.0 - self.pane_ratios.0;
        }
        if self.settings.preview_pane || self.settings.details_pane {
            width *= self.pane_ratios.1;
        }
        self.view_layout().columns_in(width - 32.0 - LEAF_FRAME_X)
    }

    /// The px the content pane has, for the icon grid (which draws exactly its viewport): the
    /// window less the panes beside it (their shares as the splitters left them), the chrome
    /// over it and the leaf's frame and foot around it. An estimate that errs small - an empty
    /// strip, never a clipped row.
    pub fn content_size(&self, window: (f32, f32)) -> (f32, f32) {
        /// The ribbon (its tab strip in the title bar, its band) and the address row.
        const CHROME_PX: f32 = 172.0;
        /// A splitter and the pane's edges.
        const SPLITTER_PX: f32 = 8.0;
        let (mut width, mut height) = window;
        if width <= 0.0 {
            width = self.window_width;
        }
        if height <= 0.0 {
            height = if self.window_height > 0.0 {
                self.window_height
            } else {
                760.0
            };
        }
        if self.settings.navigation_pane {
            width = width * (1.0 - self.pane_ratios.0) - SPLITTER_PX;
        }
        if self.settings.preview_pane || self.settings.details_pane {
            width = width * self.pane_ratios.1 - SPLITTER_PX;
        }
        height -= CHROME_PX + LEAF_FRAME_Y;
        width -= LEAF_FRAME_X;
        if self.message.is_some() {
            height -= 52.0;
        }
        ((width - 8.0).max(120.0), height.max(120.0))
    }

    /// [`Self::content_size`] of the window as it is now (its last resize).
    pub fn content_estimate(&self) -> (f32, f32) {
        self.content_size((self.window_width, self.window_height))
    }

    /// The folder on this computer that holds `prefix` of drive `index`, for a local drive (the
    /// scan reads it with `read_dir`); `None` for a bucket.
    pub fn local_dir(&self, index: usize, prefix: &str) -> Option<PathBuf> {
        match &self.slots.get(index)?.entry.location {
            DriveLocation::Local { root } => Some(jobs::path_in(Path::new(root), prefix)),
            DriveLocation::S3 { .. }
            | DriveLocation::Opendal { .. }
            | DriveLocation::Database { .. } => None,
        }
    }

    /// The root folder of drive `index` on this computer (`None` for a bucket).
    pub fn local_root(&self, index: usize) -> Option<PathBuf> {
        self.local_dir(index, "")
    }

    /// Prints the selection for scripts.
    pub fn print_selection(&self) {
        let first = self.selection.keys().first().cloned();
        println!(
            "AZDRIVE_SELECTED {} {}",
            self.selection.len(),
            first.map_or_else(|| String::from("-"), |key| key.replace('\u{0}', ":"))
        );
    }
}

// ==== Worker threads ====

pub(crate) fn spawn(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, job: Job) {
    s.running += 1;
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit { job: Some(job) }),
            app.clone(),
            jobs::job_thread,
        ),
    );
}

/// The open drive, opened on first use; an error is shown.
pub(crate) fn open_current(s: &mut DriveState) -> Option<Arc<dyn Drive>> {
    let index = s.current_drive()?;
    open_slot(s, index)
}

/// Drive `index`, opened on first use; an error is shown.
pub(crate) fn open_slot(s: &mut DriveState, index: usize) -> Option<Arc<dyn Drive>> {
    let rotated = s.rotated.clone();
    let token_url = s.token.url.clone();
    let keyring = s.keyring.clone();
    let opened = s
        .slots
        .get_mut(index)?
        .open(&rotated, token_url.as_deref(), &keyring);
    match opened {
        Ok(drive) => Some(drive),
        Err(e) => {
            s.loading = false;
            s.error(e.to_string());
            None
        }
    }
}

/// The drive with id `drive_id`, opened on first use.
pub(crate) fn open_drive(s: &mut DriveState, drive_id: &str) -> Option<Arc<dyn Drive>> {
    let index = s.slot_index(drive_id)?;
    open_slot(s, index)
}

/// Stops the running scan of the open folder (its batches would be dropped anyway: they carry
/// an older serial) - a big folder the window left is not read to its end.
pub(crate) fn cancel_listing(s: &mut DriveState) {
    s.list_cancel.store(true, Ordering::SeqCst);
}

/// Scans the open folder: its rows stream in batches ([`Outcome::Scanned`]). A `refresh`
/// gathers the new rows while the old ones still show and swaps them in at the end; otherwise
/// the rows start empty and fill as the batches land.
pub(crate) fn start_listing(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    refresh: bool,
) {
    let Some(index) = s.current_drive() else {
        return;
    };
    let Some(drive) = open_slot(s, index) else {
        // The drive cannot be opened (the message says why): nothing is being listed, so
        // nothing is waited for - the content says so instead of "Loading...".
        s.listing_done = true;
        s.listing_failed = true;
        return;
    };
    cancel_listing(s);
    let cancel = Arc::new(AtomicBool::new(false));
    s.list_cancel = cancel.clone();
    s.list_serial += 1;
    s.listing_done = false;
    s.listing_failed = false;
    s.stats_asked.clear();
    s.counts_asked.clear();
    if refresh && !s.entries.is_empty() {
        s.refreshing = Some(Vec::new());
    } else {
        s.refreshing = None;
        s.entries.clear();
        s.loading = true;
    }
    let prefix = s.prefix().to_string();
    let dir = s.local_dir(index, &prefix);
    let serial = s.list_serial;
    spawn(
        info,
        app,
        s,
        Job::Scan {
            drive,
            dir,
            prefix,
            serial,
            cancel,
        },
    );
}

/// Stops the open search (its worker hears it within milliseconds; its batches would be dropped
/// anyway): the folder's own rows show again. Whether one was open.
pub(crate) fn stop_find(s: &mut DriveState) -> bool {
    let Some(find) = s.find.take() else {
        return false;
    };
    find.cancel.store(true, Ordering::SeqCst);
    s.selection = Selection::default();
    s.clear_preview();
    true
}

/// The search box's text changed, or a setting of the search did: the search running stops and
/// a new one starts for the open folder - with every folder below it, or ("Current folder") its
/// own items - as the Search tab says: the names, then the files' contents when "File
/// contents" is on, the Refine's kinds, sizes and dates; a cloud drive's names over its listing
/// (slower, no contents); on This PC every drive on this computer, a job each. Its results
/// stream into the view as rows. An empty box (Escape cleared it) shows the place again and
/// forgets the Refine. Quick access is not searched.
pub(crate) fn start_find(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let was_open = stop_find(s);
    let query = s.search.trim().to_string();
    if query.is_empty() {
        s.refines = find::Refines::default();
        if was_open {
            println!("AZDRIVE_SEARCH_CLOSED");
        }
        return;
    }
    let mut options = find::FindOptions {
        contents: s.settings.search_contents && find::searches_contents(&query),
        show_hidden: s.settings.show_hidden,
        ignore_files: s.settings.search_ignore_files,
        subfolders: s.settings.search_subfolders,
        refine: s.refines.to_refine(&chrono::Local::now()),
    };
    s.find_serial += 1;
    let serial = s.find_serial;
    let cancel = Arc::new(AtomicBool::new(false));
    let mut jobs = Vec::new();
    let mut remote = false;
    if let Some(index) = s.current_drive() {
        let Some(drive) = open_slot(s, index) else {
            return;
        };
        let prefix = s.prefix().to_string();
        match s.local_dir(index, &prefix) {
            Some(dir) => jobs.push(Job::Find {
                serial,
                request: find::local_request(dir, &query, prefix.is_empty(), &options),
                prefix,
                cancel: cancel.clone(),
            }),
            None => {
                remote = true;
                options.contents = false;
                // The drive's last complete listing is kept in the cache folder.
                let cache = s.cache_dir.as_ref().map(|dir| {
                    find::listing_file(&dir.join("listings"), &s.slots[index].entry.id)
                });
                jobs.push(Job::FindRemote {
                    find: jobs::RemoteFind {
                        serial,
                        prefix,
                        pattern: azul_search::Pattern::guess(query.clone()),
                        options: options.clone(),
                        cache,
                    },
                    drive,
                    cancel: cancel.clone(),
                });
            }
        }
    } else if s.place == Place::ThisPc {
        // This PC: every drive on this computer, from its root; a row's key names its drive.
        for index in 0..s.slots.len() {
            let Some(root) = s.local_root(index) else {
                continue;
            };
            let prefix = find::pc_key(&s.slots[index].entry.id, "");
            jobs.push(Job::Find {
                serial,
                request: find::local_request(root, &query, true, &options),
                prefix,
                cancel: cancel.clone(),
            });
        }
    }
    if jobs.is_empty() {
        if was_open {
            println!("AZDRIVE_SEARCH_CLOSED");
        }
        return;
    }
    let mut state = find::FindState::new(query.clone(), options.contents, remote, serial, cancel);
    state.pending = jobs.len();
    s.find = Some(state);
    // Windows 8: the Search tab (Search Tools) comes forward when a search opens - not again
    // with every key typed into it (the user may have chosen another tab meanwhile).
    if !was_open {
        s.ribbon_tab = ui_ribbon::RibbonTabKind::Search;
    }
    s.view_scroll.0 = 0.0;
    ui_view::scroll_view_to_top(info);
    println!("AZDRIVE_SEARCHING {query}");
    for job in jobs {
        spawn(info, app, s, job);
    }
}

/// Lists the folders of the tree node `node` (one read of the folder), unlocking its drive
/// first when it needs the keyring.
pub(crate) fn start_tree_listing(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    node: TreeKey,
) {
    if s.tree.listing.contains(&node) {
        return;
    }
    let Some(index) = s.slot_index(&node.0) else {
        return;
    };
    if s.slots[index].locked() {
        if !s.tree.pending.contains(&node) {
            s.tree.pending.push(node);
        }
        unlock(info, s, index);
        return;
    }
    let Some(drive) = open_slot(s, index) else {
        return;
    };
    let dir = s.local_dir(index, &node.1);
    s.tree.listing.insert(node.clone());
    spawn(info, app, s, Job::Folders { drive, dir, node });
}

/// Counts the items of the local folders `keys` of drive `index` (`""`: its root), the ones not
/// asked for during this listing - one `read_dir` each, on a worker thread. A listing asks
/// again (F5, a change on disk): the old count shows until the new one is in.
pub(crate) fn request_counts(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    index: usize,
    keys: Vec<String>,
) {
    let Some(root) = s.local_root(index) else {
        return;
    };
    let keys: Vec<String> = keys
        .into_iter()
        .filter(|k| !s.counts_asked.contains(k))
        .collect();
    if keys.is_empty() {
        return;
    }
    s.counts_asked.extend(keys.iter().cloned());
    let drive_id = s.slots[index].entry.id.clone();
    let show_hidden = s.settings.show_hidden;
    spawn(
        info,
        app,
        s,
        Job::Count {
            drive_id,
            root,
            keys,
            show_hidden,
        },
    );
}

/// This PC's local drive tiles say how many items each drive's root holds: one cheap count each
/// (no listing of the drive).
pub(crate) fn count_drive_roots(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let local: Vec<usize> = (0..s.slots.len())
        .filter(|&i| s.slots[i].is_local() && !s.root_counts.contains_key(&s.slots[i].entry.id))
        .collect();
    for index in local {
        let Some(root) = s.local_root(index) else {
            continue;
        };
        let drive_id = s.slots[index].entry.id.clone();
        let show_hidden = s.settings.show_hidden;
        spawn(
            info,
            app,
            s,
            Job::Count {
                drive_id,
                root,
                keys: vec![String::new()],
                show_hidden,
            },
        );
    }
}

/// Reads an S3 drive's keys from the keyring (once at a time).
pub(crate) fn unlock(info: &mut CallbackInfo, s: &mut DriveState, index: usize) {
    let drive_id = s.slots[index].entry.id.clone();
    if matches!(&s.keyring_waiting, Some(KeyringOp::Unlock { drive_id: id }) if *id == drive_id)
        || s
            .keyring_queue
            .iter()
            .any(|(op, _)| matches!(op, KeyringOp::Unlock { drive_id: id } if *id == drive_id))
    {
        return;
    }
    let name = s.slots[index].entry.name.clone();
    s.info(format!("Reading the keys of \"{name}\" from the keyring..."));
    keyring(
        info,
        s,
        KeyringOp::Unlock {
            drive_id: drive_id.clone(),
        },
        KeyringCall::Get(config::keyring_key(&drive_id)),
    );
}

/// Goes to `place`; `remember` puts the place being left on the Back list.
pub(crate) fn go(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    place: Place,
    remember: bool,
) {
    if remember && place != s.place {
        let leaving = s.place.clone();
        s.history.visit(leaving);
    }
    s.recent.retain(|p| *p != place);
    s.recent.insert(0, place.clone());
    s.recent.truncate(RECENT_PLACES);
    s.place = place;
    s.clear_notice();
    s.editing_path = false;
    s.renaming = None;
    // A search ends where its folder is left (a result's folder opens without it): the box
    // empties too - what was typed there outranks the rebuild until the app sets it.
    let searching = stop_find(s);
    if searching || !s.search.is_empty() {
        println!("AZDRIVE_SEARCH_CLOSED");
        actions::clear_search_box(info);
    }
    s.search.clear();
    s.refines = find::Refines::default();
    // The folder being left may still be read: that read stops here.
    cancel_listing(s);
    s.entries.clear();
    s.refreshing = None;
    s.listing_done = false;
    s.listing_failed = false;
    s.stats_asked.clear();
    s.counts.clear();
    s.counts_asked.clear();
    s.selection = Selection::default();
    s.selected_pin = None;
    s.grid_view = IconGridView::create();
    s.clear_preview();
    s.thumbnails.clear();
    s.thumbnails_pending.clear();
    s.loading = false;
    s.list_serial += 1;
    s.backstage = None;
    // The new folder opens at its top, wherever the last one was scrolled to.
    s.view_scroll.0 = 0.0;
    ui_view::scroll_view_to_top(info);
    set_window_title(info, s);
    println!("AZDRIVE_PLACE {}", s.place_line());
    if s.place == Place::ThisPc {
        count_drive_roots(info, app, s);
    }
    let Some(index) = s.current_drive() else {
        return;
    };
    if s.slots[index].locked() {
        s.loading = true;
        unlock(info, s, index);
        return;
    }
    start_listing(info, app, s, false);
}

/// Lists the open folder again (F5), keeping the selection.
pub(crate) fn refresh(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.current_drive().is_none() {
        // This PC: the drives' sizes and item counts again.
        refresh_disks(s);
        s.root_counts.clear();
        count_drive_roots(info, app, s);
        return;
    }
    if s.find.is_some() {
        // F5 on a search's results searches again.
        start_find(info, app, s);
        return;
    }
    start_listing(info, app, s, true);
}

/// The window's title is the open place's path (the title bar shows the ribbon's tabs, so the
/// path is what the system's window list, the Dock and Mission Control name the window by).
pub(crate) fn set_window_title(info: &mut CallbackInfo, s: &DriveState) {
    let mut state = info.get_current_window_state();
    let title = window_title(s);
    println!("AZDRIVE_TITLE {title}");
    if state.title.as_str() != title {
        state.title = AzString::from(title);
        info.modify_window_state(state);
    }
}

/// [`set_window_title`]'s text: `Home/Documents - AzDrive`.
pub(crate) fn window_title(s: &DriveState) -> String {
    let drive_name = s.drive_name(&s.place);
    let path_drive = match &s.place {
        Place::Folder { .. } => Some(drive_name.as_str()),
        _ => None,
    };
    format!("{} - AzDrive", browse::path_text(&s.place, path_drive))
}

/// The local volumes' size and free space, from the OS - and the standard folders the Home
/// drive holds (FAVORITES' rows: a Downloads folder made since shows after This PC's refresh).
pub(crate) fn refresh_disks(s: &mut DriveState) {
    let mut found = Vec::new();
    let mut home = None;
    for slot in &s.slots {
        if let DriveLocation::Local { root } = &slot.entry.location {
            if let Some(space) = FilePath::create(root.as_str()).disk_space().into_option() {
                found.push((slot.entry.id.clone(), (space.total, space.free)));
            }
            if slot.entry.id == HOME_ID {
                home = Some(PathBuf::from(root));
            }
        }
    }
    s.disk.extend(found);
    if let Some(home) = home {
        s.standard_folders = ui_sidebar::standard_folders(&home);
    }
}

/// The place above the open one: the parent folder, the drive's root, This PC.
pub(crate) fn place_up(place: &Place) -> Option<Place> {
    match place {
        Place::QuickAccess | Place::ThisPc => None,
        Place::Folder { prefix, .. } if prefix.is_empty() => Some(Place::ThisPc),
        Place::Folder { drive, prefix } => {
            Some(Place::folder(drive, &key::parent_prefix(prefix)))
        }
    }
}

/// A folder's content changed: the tree lists it again on its next expand
/// (now, when it is open).
pub(crate) fn tree_invalidate(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    node: TreeKey,
) {
    if s.tree.loaded.remove(&node).is_some() && s.tree.expanded.contains(&node) {
        start_tree_listing(info, app, s, node);
    }
}

/// The Options opened (`was_open`: they showed already): AzDrive's own settings as they are
/// now are what the Options' Cancel puts back.
pub(crate) fn options_opened(s: &mut DriveState, was_open: bool) {
    if !was_open {
        s.settings_found = Some(s.settings.clone());
    }
}

/// Cancel on the Options (azul-appkit's settings page): AzDrive's own settings - the view, the
/// navigation - come back as the Options found them, and the settings file is written again.
pub(crate) fn reload_settings(
    app: &mut RefAny,
    info: &mut CallbackInfo,
    _settings: &azul_appkit::AppSettings,
) {
    let handle = app.clone();
    if let Some(mut guard) = app.downcast_mut::<DriveState>() {
        let s = &mut *guard;
        if let Some(found) = s.settings_found.take() {
            if s.settings != found {
                s.settings = found;
                save_settings(info, &handle, s);
            }
        }
    };
}

/// Writes the settings file (on a Thread, through the settings folder's drive).
pub(crate) fn save_settings(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(drive) = s.settings_drive.clone() else {
        return;
    };
    let text = s.settings.to_json();
    spawn(info, app, s, Job::SaveSettings { drive, text });
}

// ==== Keyring ====

/// A keyring operation of the UI thread in flight or queued (the keyring answers one at a
/// time). An Azlin drive's session is never written here: the worker threads write it under
/// the drive's lock (`DriveState::keyring`), where a later write of the UI thread could put a
/// spent drive token back over a newer one.
pub(crate) enum KeyringOp {
    /// Reading a drive's keyring entry (keys, a session, secret settings) to open it.
    Unlock { drive_id: String },
    /// Saving a new drive's keys.
    Store { drive_id: String },
    /// Removing a forgotten drive's keys.
    Forget,
}

/// Whether `secret` is what the keyring entry of `entry` holds: an S3 drive's keys, an Azlin
/// drive's session, a data source's secret settings. `Err` says what it is not.
pub(crate) fn check_secret(entry: &DriveEntry, secret: &str) -> Result<(), String> {
    if entry.azlin().is_some() {
        return azcloud_kit::AzlinSession::from_keyring_secret(secret)
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    match &entry.location {
        DriveLocation::S3 { .. } => Credentials::from_keyring_secret(secret)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        DriveLocation::Opendal { .. } | DriveLocation::Database { .. } => {
            azul_storage::SecretOptions::from_keyring_secret(secret)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        DriveLocation::Local { .. } => Ok(()),
    }
}

/// Takes the sessions Azlin drives switched to on worker threads as their slots' secrets (a
/// presigned link signs with the current keys). The keyring has them already - the refresh
/// wrote them under the drive's lock -; one it did not take is said.
pub(crate) fn take_rotated(s: &mut DriveState) {
    let rotated: Vec<Rotated> = match s.rotated.lock() {
        Ok(mut queue) => queue.drain(..).collect(),
        Err(_) => return,
    };
    for session in rotated {
        let Some(index) = s.slot_index(&session.drive_id) else {
            continue;
        };
        s.slots[index].secret = Some(session.secret);
        if let Some(why) = session.unsaved {
            let name = s.slots[index].entry.name.clone();
            s.error(format!(
                "The new session of \"{name}\" could not be saved in the keyring: {why}. AzDrive \
                 keeps it until it closes; after that the drive must be added again."
            ));
        }
    }
}

/// The request behind a [`KeyringOp`]. Holds a secret while queued; never printed.
pub(crate) enum KeyringCall {
    Get(String),
    Store(String, String),
    Delete(String),
}

fn issue(info: &mut CallbackInfo, call: &KeyringCall) {
    match call {
        KeyringCall::Get(key) => info.keyring_get(key.as_str()),
        KeyringCall::Store(key, secret) => info.keyring_store(key.as_str(), secret.as_str(), false),
        KeyringCall::Delete(key) => info.keyring_delete(key.as_str()),
    }
}

/// Sends a keyring request now, or after the one in flight (the keyring's
/// answer does not say which request it answers, so there is one at a time).
pub(crate) fn keyring(
    info: &mut CallbackInfo,
    s: &mut DriveState,
    op: KeyringOp,
    call: KeyringCall,
) {
    if s.keyring_waiting.is_none() {
        issue(info, &call);
        s.keyring_waiting = Some(op);
    } else {
        s.keyring_queue.push_back((op, call));
    }
}

fn keyring_problem(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::NotFound => "the keyring has no entry for it",
        KeyringResult::Denied => "the keyring refused",
        KeyringResult::Unavailable => "no keyring is available on this system",
        _ => "the keyring reported an error",
    }
}

/// The keyring answered the request in flight.
extern "C" fn on_keyring_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(result) = info.get_keyring_result().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(op) = s.keyring_waiting.take() else {
        return Update::DoNothing;
    };
    match op {
        KeyringOp::Unlock { drive_id } => {
            let Some(index) = s.slot_index(&drive_id) else {
                return Update::RefreshDom;
            };
            let name = s.slots[index].entry.name.clone();
            let is_current = s.current_drive() == Some(index);
            match &result {
                KeyringResult::Retrieved(secret) => {
                    match check_secret(&s.slots[index].entry, secret.as_str()) {
                        Ok(()) => {
                            s.slots[index].secret = Some(secret.as_str().to_string());
                            s.clear_notice();
                            if is_current {
                                start_listing(&mut info, &app, s, false);
                            }
                            let pending: Vec<TreeKey> = s
                                .tree
                                .pending
                                .iter()
                                .filter(|node| node.0 == drive_id)
                                .cloned()
                                .collect();
                            s.tree.pending.retain(|node| node.0 != drive_id);
                            for node in pending {
                                start_tree_listing(&mut info, &app, s, node);
                            }
                        }
                        Err(e) => {
                            s.loading = false;
                            if is_current {
                                // No listing follows: the folder waits for nothing.
                                s.listing_done = true;
                                s.listing_failed = true;
                            }
                            s.error(format!("The keys of \"{name}\" cannot be read: {e}."));
                        }
                    }
                }
                other => {
                    s.loading = false;
                    if is_current {
                        // No listing follows: the folder waits for nothing.
                        s.listing_done = true;
                        s.listing_failed = true;
                    }
                    s.tree.pending.retain(|node| node.0 != drive_id);
                    if s.slots[index].entry.azlin().is_some() {
                        // A drive token cannot be typed in: the drive is bought (or joined) again.
                        s.error(format!(
                            "\"{name}\" cannot be opened: {}. Its drive token is gone; remove \
                             the drive and add it again.",
                            keyring_problem(other)
                        ));
                    } else {
                        s.error(format!(
                            "\"{name}\" cannot be opened: {}. Enter its keys again.",
                            keyring_problem(other)
                        ));
                        if is_current && matches!(other, KeyringResult::NotFound) {
                            actions::open_drive_form(s, Some(index));
                        }
                    }
                }
            }
        }
        KeyringOp::Store { drive_id } => {
            let name = s
                .slot_index(&drive_id)
                .map(|i| s.slots[i].entry.name.clone())
                .unwrap_or_default();
            if matches!(result, KeyringResult::Stored) {
                s.success(format!(
                    "\"{name}\" is saved; its keys are in the system keyring."
                ));
            } else {
                s.error(format!(
                    "The keys of \"{name}\" could not be saved: {}. They are kept until AzDrive \
                     closes.",
                    keyring_problem(&result)
                ));
            }
        }
        KeyringOp::Forget => {}
    }
    if let Some((next, call)) = s.keyring_queue.pop_front() {
        issue(&mut info, &call);
        s.keyring_waiting = Some(next);
    }
    Update::RefreshDom
}

// ==== Answers of the worker threads ====

/// Opens `path` with the OS's default app.
pub(crate) fn open_with_os(path: &Path) -> Result<(), String> {
    let url = browse::file_url(path);
    match Url::parse(url.as_str()).into_result() {
        Ok(url) if url.open() => Ok(()),
        Ok(_) => Err(format!("the system could not open {}", path.display())),
        Err(e) => Err(format!("{} is not a URL: {}", url, e.message.as_str())),
    }
}

/// Whether the open folder is `prefix` of `drive_id`.
pub(crate) fn showing(s: &DriveState, drive_id: &str, prefix: &str) -> bool {
    matches!(&s.place, Place::Folder { drive, prefix: open } if drive == drive_id && open == prefix)
}

/// The folder `prefix` of `drive_id` changed: list it again when it is
/// open; the tree too.
pub(crate) fn changed(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    prefix: &str,
) {
    if showing(s, drive_id, prefix) {
        // Read again behind the rows that show: they stay until the new ones are in.
        start_listing(info, app, s, true);
    }
    if s.find.is_some() && s.current_drive_id().as_deref() == Some(drive_id) {
        // The search's results may hold what changed (a result renamed, deleted): it runs again.
        start_find(info, app, s);
    }
    tree_invalidate(info, app, s, (drive_id.to_string(), prefix.to_string()));
}

/// The open folder of the open drive changed.
fn changed_here(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if let Some(drive_id) = s.current_drive_id() {
        let prefix = s.prefix().to_string();
        changed(info, app, s, &drive_id, &prefix);
    }
}

/// A batch of the open folder's scan landed (`done` with the last one): it merges into the rows
/// in the view's order - while a refresh runs it is gathered behind the rows that still show,
/// which it replaces at the end -, and the rows in view get their stats, counts and thumbnails.
/// The selection is checked against the rows once, at the end (a batch only adds rows).
fn scanned(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    batch: Vec<Entry>,
    done: bool,
    error: Option<String>,
) {
    let sort = s.settings.sort;
    match s.refreshing.as_mut() {
        Some(gathered) => gathered.extend(batch),
        None => {
            listing::merge_batch(&mut s.entries, batch, sort);
            s.loading = false;
        }
    }
    if let Some(e) = error {
        s.loading = false;
        s.listing_done = true;
        s.listing_failed = true;
        s.refreshing = None;
        s.error(format!("Could not list this folder: {e}"));
        return;
    }
    if !done {
        actions::request_view_work(info, app, s);
        return;
    }
    s.listing_done = true;
    s.loading = false;
    if let Some(mut fresh) = s.refreshing.take() {
        // The rows read again show the old sizes and dates until their own stats are in.
        listing::carry_stats(&s.entries, &mut fresh);
        browse::sort_entries(&mut fresh, sort);
        s.entries = fresh;
    }
    let keys = s.visible_keys();
    let order: Vec<&str> = keys.iter().map(String::as_str).collect();
    s.selection.retain(&order);
    // Open file location: the result the folder was opened for is selected, in view.
    if let Some(key) = s.select_when_listed.take() {
        if s.entries.iter().any(|e| e.key == key) {
            s.selection.set(vec![key.clone()]);
            s.print_selection();
            ui_view::reveal_item(info, s, &key);
        }
    }
    if let Place::Folder { drive, prefix } = &s.place {
        if prefix.is_empty() {
            s.root_counts.insert(drive.clone(), s.entries.len());
        }
        println!(
            "AZDRIVE_LISTED {drive} {} {}",
            if prefix.is_empty() { "/" } else { prefix },
            s.entries.len()
        );
    }
    // A new folder waiting for its name is scrolled into view (it may sort far down).
    if let Some(key) = s.renaming.as_ref().map(|r| r.key.clone()) {
        ui_view::reveal_item(info, s, &key);
    }
    actions::request_preview(info, app, s);
    if actions::needs_all_stats(s) {
        actions::request_sort_stats(info, app, s);
    }
    actions::request_view_work(info, app, s);
}

pub(crate) extern "C" fn on_job_done(
    mut app: RefAny,
    mut msg: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let handle = app.clone();
    let Some(outcome) = msg
        .downcast_mut::<Done>()
        .and_then(|mut done| done.outcome.take())
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    // A scan's batches before its last one, a transfer's progress and what the background
    // claims found are messages of a thread that still runs.
    let still_running = matches!(
        outcome,
        Outcome::Progress { .. }
            | Outcome::Thumbnail { .. }
            | Outcome::Scanned { done: false, .. }
            | Outcome::Claimed { serial: None, .. }
            | Outcome::CheckoutDropped { .. }
            | Outcome::Searched { end: None, .. }
    );
    if !still_running {
        s.running = s.running.saturating_sub(1);
    }
    // An Azlin drive may have switched sessions during the job (refreshed, or read another
    // window's from the keyring): the slot's secret follows.
    take_rotated(s);
    match outcome {
        Outcome::Scanned {
            serial,
            batch,
            done,
            error,
        } => {
            if serial != s.list_serial {
                return Update::DoNothing; // another folder or drive by now
            }
            scanned(&mut info, &handle, s, batch, done, error);
        }
        Outcome::Stats { serial, stats } => {
            if serial != s.list_serial {
                return Update::DoNothing;
            }
            let mut changed = listing::apply_stats(&mut s.entries, &stats);
            if let Some(find) = s.find.as_mut() {
                // The search's rows in view were asked for (a result's key is the drive's).
                changed += find.apply_stats(&stats);
            }
            if actions::needs_all_stats(s) {
                // A sort by Size or Date modified is a chain: every answer asks for the next
                // rows - even one that changed no row (its rows gone since) - and the answer
                // that leaves no row unknown sorts.
                if listing::all_known(&s.entries) {
                    // The last sizes and dates are in: the rows take the order they ask for.
                    browse::sort_entries(&mut s.entries, s.settings.sort);
                } else {
                    actions::request_sort_stats(&mut info, &handle, s);
                }
            } else if changed == 0 {
                return Update::DoNothing;
            }
            // A picture's size is known now: it may get its thumbnail.
            actions::request_view_work(&mut info, &handle, s);
        }
        Outcome::Counted { drive_id, counts } => {
            for (key, n) in counts {
                if key.is_empty() {
                    s.root_counts.insert(drive_id.clone(), n);
                } else if s.current_drive_id().as_deref() == Some(drive_id.as_str()) {
                    s.counts.insert(key, n);
                }
            }
        }
        Outcome::Folders { node, result } => {
            s.tree.listing.remove(&node);
            match result {
                Ok((folders, items)) => {
                    if node.1.is_empty() {
                        s.root_counts.insert(node.0.clone(), items);
                    }
                    println!(
                        "AZDRIVE_TREE {} {} {}",
                        node.0,
                        if node.1.is_empty() { "/" } else { &node.1 },
                        folders.len()
                    );
                    let show_hidden = s.settings.show_hidden;
                    let folders: Vec<String> = folders
                        .into_iter()
                        .filter(|f| show_hidden || !key::last_segment(f).starts_with('.'))
                        .collect();
                    s.tree.loaded.insert(node, folders);
                }
                Err(e) => {
                    s.tree.expanded.remove(&node);
                    s.error(format!("Could not list the folder: {e}"));
                }
            }
        }
        Outcome::Planned { id, result } => {
            actions::transfer_planned(&mut info, &handle, s, id, result);
        }
        Outcome::Progress { id, progress } => {
            s.queue.progress(id, &progress);
            actions::show_progress_when_long(s);
        }
        Outcome::Ran { id, report } => actions::transfer_ran(&mut info, &handle, s, id, report),
        Outcome::Deleted { drive_id, result } => {
            match result {
                Ok(gone) => {
                    println!("AZDRIVE_DELETED {}", gone.len());
                    let trashed: Vec<(String, String)> = gone
                        .iter()
                        .filter(|(_, to)| !to.is_empty())
                        .cloned()
                        .collect();
                    if trashed.is_empty() {
                        s.success(format!("Deleted {} item(s) for good.", gone.len()));
                    } else {
                        s.success(format!(
                            "Moved {} item(s) to the trash folder. Ctrl+Z brings them back.",
                            trashed.len()
                        ));
                        s.undo.push(UndoOp::Trash {
                            drive: drive_id.clone(),
                            gone: trashed,
                        });
                    }
                    s.selection.clear();
                }
                Err(e) => s.error(format!("Could not delete: {e}")),
            }
            let prefix = s.prefix().to_string();
            changed(&mut info, &handle, s, &drive_id, &prefix);
        }
        Outcome::Renamed {
            drive_id,
            from,
            to,
            result,
        } => match result {
            Ok(()) => {
                println!("AZDRIVE_DONE renamed {to}");
                s.undo.push(UndoOp::Rename {
                    drive: drive_id.clone(),
                    from,
                    to: to.clone(),
                });
                s.selection.set(vec![to]);
                let prefix = s.prefix().to_string();
                changed(&mut info, &handle, s, &drive_id, &prefix);
            }
            Err(e) => s.error(format!(
                "Could not rename \"{}\": {e}",
                key::last_segment(&from)
            )),
        },
        Outcome::Created {
            drive_id,
            key,
            result,
        } => match result {
            Ok(()) => {
                println!("AZDRIVE_DONE created {key}");
                s.undo.push(UndoOp::Create {
                    drive: drive_id.clone(),
                    key: key.clone(),
                });
                s.selection.set(vec![key.clone()]);
                // Explorer: the new item waits for its name.
                println!("AZDRIVE_RENAMING {key}");
                s.renaming = Some(Renaming {
                    text: key::last_segment(&key).to_string(),
                    is_folder: key.ends_with('/'),
                    key,
                });
                let prefix = s.prefix().to_string();
                changed(&mut info, &handle, s, &drive_id, &prefix);
            }
            Err(e) => s.error(format!(
                "Could not create \"{}\": {e}",
                key::last_segment(&key)
            )),
        },
        Outcome::Undone { result } => {
            match result {
                Ok(()) => s.success("Undone."),
                Err(e) => s.error(format!("Could not undo: {e}")),
            }
            changed_here(&mut info, &handle, s);
        }
        Outcome::Previewed { key, content } => {
            if let Some(preview) = s.preview.as_mut().filter(|p| p.key == key) {
                let kind = match &content {
                    PreviewContent::Image { .. } => "image",
                    PreviewContent::Text(_) => "text",
                    PreviewContent::Video(_) => "video",
                    PreviewContent::Audio(_) => "audio",
                    PreviewContent::Message(_) => "none",
                };
                println!("AZDRIVE_PREVIEW {kind} {key}");
                preview.content = Some(content);
            }
        }
        Outcome::Measured { serial, result } => {
            if let Some(Popup::Properties(props)) = s.popup.as_mut() {
                if props.serial == serial {
                    props.size = Some(result.map_err(|e| e.to_string()));
                }
            }
        }
        Outcome::Metadata { key, result } => {
            let result = result.map_err(|e| e.to_string());
            if let Some(Popup::Properties(props)) = s.popup.as_mut() {
                if props.items.len() == 1 && props.items[0].key == key {
                    props.metadata = Some(result.clone());
                }
            }
            s.metadata.insert(key, result);
        }
        Outcome::Zipped { zip_key, result } => match result {
            Ok(bytes) => {
                println!("AZDRIVE_DONE zipped {zip_key}");
                s.success(format!(
                    "Compressed into \"{}\" ({}).",
                    key::last_segment(&zip_key),
                    browse::format_size(Some(bytes))
                ));
                s.selection.set(vec![zip_key]);
                changed_here(&mut info, &handle, s);
            }
            Err(e) => s.error(format!("Could not compress: {e}")),
        },
        Outcome::Opened { key, result } => match result {
            Ok(path) => {
                println!("AZDRIVE_DONE opened {key}");
                if let Err(e) = open_with_os(&path) {
                    s.error(e);
                }
            }
            Err(e) => s.error(format!(
                "Could not open \"{}\": {e}",
                key::last_segment(&key)
            )),
        },
        Outcome::Tested { serial, result } => {
            add_flow::tested(s, serial, result.map_err(|e| e.to_string()));
        }
        Outcome::Tiers { serial, result } => add_flow::tiers_answered(s, serial, result),
        Outcome::Bought { serial, result } => {
            add_flow::bought(&mut info, &handle, s, serial, result);
        }
        Outcome::CheckoutStarted { serial, result } => {
            add_flow::checkout_started(&mut info, &handle, s, serial, result);
        }
        Outcome::PaymentEnded { serial, why } => {
            add_flow::payment_ended(&mut info, &handle, s, serial, why);
        }
        Outcome::Claimed {
            serial,
            checkout,
            claimed,
        } => add_flow::claimed(&mut info, &handle, s, serial, &checkout, &claimed),
        Outcome::CheckoutDropped { checkout_id, why } => {
            add_flow::checkout_dropped(s, &checkout_id, &why);
        }
        Outcome::ClaimsDone { problem } => add_flow::claims_done(s, problem),
        Outcome::CheckoutForgotten {
            checkout_id,
            result,
        } => add_flow::checkout_forgotten(s, &checkout_id, result),
        Outcome::SettingsSaved { result } => {
            if let Err(e) = result {
                s.error(format!("The settings could not be saved: {e}"));
            }
        }
        Outcome::Thumbnail { key, image } => {
            if s.thumbnails_pending.remove(&key) {
                if image.is_some() {
                    println!("AZDRIVE_THUMBNAIL {key}");
                }
                s.thumbnails.insert(key, image);
            }
        }
        Outcome::ThumbnailsDone => {}
        Outcome::Searched {
            serial,
            batch,
            phase,
            searched,
            end,
        } => {
            let Some(find) = s.find.as_mut().filter(|f| f.serial == serial) else {
                return Update::DoNothing; // an older search, or none open any more
            };
            find.merge(batch);
            find.phase = phase;
            find.searched = searched;
            let mut removed = false;
            if let Some(end) = end {
                // A cloud drive's results of its last listing that the fresh one has not got.
                removed = !end.stale.is_empty();
                find.remove(&end.stale);
                // The last job's end is the search's (This PC runs one per drive).
                if find.job_ended(end) {
                    println!(
                        "AZDRIVE_SEARCHED {} {} {}",
                        find.rows.len(),
                        if find.contents { "contents" } else { "names" },
                        find.query
                    );
                }
            }
            if removed {
                let keys = s.visible_keys();
                let order: Vec<&str> = keys.iter().map(String::as_str).collect();
                s.selection.retain(&order);
            }
            // The rows that came into view get their sizes and dates.
            actions::request_view_work(&mut info, &handle, s);
        }
    }
    Update::RefreshDom
}

// ==== Layout ====

/// Runs `f` on the state with the app handle, and asks for a new DOM.
pub(crate) fn with_state(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut CallbackInfo, &RefAny, &mut DriveState),
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    f(info, &app, &mut *guard);
    Update::RefreshDom
}

/// Explorer's address row under the ribbon (Back, Forward, Recent, Up, the breadcrumb box with
/// Refresh, "Search <folder>"): the shell's address bar slot (the `shell-address-bar` host).
fn chrome(s: &DriveState, app: &RefAny, width: f32) -> Dom {
    Dom::create_div()
        .with_id(ids::CHROME)
        .with_css("display: flex; flex-direction: column;")
        .with_child(ui_panes::address_bar(s, app, width))
}

/// The DOM id of the details pane, which shares the right side with the preview pane.
const DETAILS_PANE_ID: &str = "shell-details";

/// A splitter was dragged: the pane left of it keeps its new share (the navigation pane's of
/// the window, the content's beside the right pane), handed back on every rebuild - and the
/// icon grid takes the new width.
extern "C" fn on_pane_resize(
    mut data: RefAny,
    _info: CallbackInfo,
    pane: usize,
    ratio: f32,
) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    if !ratio.is_finite() || ratio <= 0.05 || ratio >= 0.95 {
        return Update::DoNothing;
    }
    // The index counts the panes shown: the navigation pane first, when it is.
    let nav = s.settings.navigation_pane;
    if nav && pane == 0 {
        s.pane_ratios.0 = ratio;
    } else if (nav && pane == 1) || (!nav && pane == 0) {
        s.pane_ratios.1 = ratio;
    } else {
        return Update::DoNothing;
    }
    if ui_view::uses_icon_grid(&s) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<DriveState>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let width = if window.0 > 0.0 {
        window.0
    } else {
        s.window_width
    };

    // Finder's body under Explorer's ribbon: the source list, the content as a leaf on the
    // page (its path bar and status line at its foot), the right pane a leaf too.
    let backstage = s.backstage_shown();
    let mut browser = BrowserShell::create(
        chrome(s, &app, width),
        ui_sidebar::sidebar(s, &app),
        ui_view::content(s, &app, s.content_size(window)),
    )
    .with_tree_visible(s.settings.navigation_pane)
    .with_tree_ratio(s.pane_ratios.0)
    .with_content_ratio(s.pane_ratios.1);
    if s.settings.preview_pane {
        browser = browser.with_preview(ui_view::on_page(ui_panes::preview_pane(s, &app, dark)));
    }
    // Windows 8's ribbon over the address row; its tab strip is the window's title bar.
    if backstage.is_none() {
        browser = browser.with_ribbon(ui_ribbon::ribbon(s, &app));
    }
    let mut shell = browser.office_shell();
    // Explorer 10's right side: the preview pane OR the details pane.
    if s.settings.details_pane && !s.settings.preview_pane {
        shell.add_pane(
            ShellPane::create(DETAILS_PANE_ID, ui_view::on_page(ui_panes::details_pane(s)))
                .with_kind(ShellPaneKind::Side)
                .with_label("Details"),
        );
    }
    // No title row while the ribbon shows - its tabs are the title bar; the backstage (the
    // Options, About), which has no ribbon, keeps azul's title row to move the window by. The
    // counts are the leaf's status line (no status bar).
    let mut shell =
        shell.with_on_pane_resize(app.clone(), on_pane_resize as ShellOnPaneResizeCallbackType);
    if let Some(page) = backstage {
        shell = shell
            .with_title_row(azul_appkit::ui::title_row(&window_title(s)))
            .with_backstage(ui_dialogs::backstage(s, &app, page));
    }
    let mut body = Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; height: 100%; margin: 0px; font-size: 13px; \
             font-family: system:ui; color: system:text;",
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
                .with_child(ShellThemeScope::create(shell.dom()).dom()),
        )
        // The keyring's answers arrive as a window event.
        .with_callback(
            EventFilter::Window(WindowEventFilter::KeyringResult),
            app.clone(),
            on_keyring_result,
        )
        // Explorer's keyboard, wherever the focus is (a text field keeps its keys).
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            actions::on_key_down,
        )
        // Files dropped from the OS upload into the open folder.
        .with_callback(
            EventFilter::Window(WindowEventFilter::DroppedFile),
            app.clone(),
            actions::on_dropped_file,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::Resized),
            app.clone(),
            actions::on_resized,
        );
    if s.column_drag.is_some() {
        body = body
            .with_callback(
                EventFilter::Window(WindowEventFilter::MouseOver),
                app.clone(),
                ui_view::on_column_drag_move,
            )
            .with_callback(
                EventFilter::Window(WindowEventFilter::MouseUp),
                app.clone(),
                ui_view::on_column_drag_end,
            );
    }
    if let Some(popup) = &s.popup {
        let (title, panel) = ui_dialogs::popup_parts(popup, s, &app);
        if s.inline_dialogs {
            body.add_child(ui_dialogs::inline_sheet(title, panel));
        } else {
            body.add_child(
                Dialog::create(panel)
                    .with_title(AzString::from(title))
                    .with_open(true)
                    .with_modal(true)
                    .with_close_button(true)
                    .with_on_close(app.clone(), ui_dialogs::on_dialog_closed)
                    .dom(),
            );
        }
    }
    body
}

// ==== Start ====

fn path_of(dir: Option<FilePath>) -> Option<PathBuf> {
    dir.map(|d| PathBuf::from(d.inner.as_str()))
        .filter(|p| !p.as_os_str().is_empty())
}

extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        azul_appkit::ui::on_window_created(&s.kit, info);
        // The size the window opened at (a window manager may have changed it).
        let size = info.get_current_window_state().size.dimensions;
        if size.width > 0.0 && size.height > 0.0 {
            s.window_width = size.width;
            s.window_height = size.height;
        }
        let place = s.place.clone();
        // `--screen settings` opens on the backstage, which a visit closes.
        let backstage = s.backstage;
        go(info, app, s, place, false);
        s.backstage = backstage;
        // A drive paid after "Stop waiting", or while AzDrive was closed, arrives now.
        add_flow::start_claims(info, app, s);
    })
}

/// The folder of the locks every AzDrive of this user (and every Azlin app) shares: beside the
/// drives file (`<config dir>/azul-storage/locks`), else in the temporary folder.
fn lock_dir(drives_file: Option<&Path>) -> PathBuf {
    drives_file
        .and_then(Path::parent)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map_or_else(
            || std::env::temp_dir().join("azul-storage-locks"),
            |dir| dir.join("locks"),
        )
}

/// The data tree as a drive: the data root, opened as the data tree's `LocalDrive` (the one that
/// keeps its `.azlin/` bookkeeping, which it never lists), named "Azlin".
fn data_slot(data_root: &Path) -> Slot {
    let mut slot = Slot::new(DriveEntry {
        id: DATA_ID.to_string(),
        name: String::from("Azlin"),
        location: DriveLocation::Local {
            root: data_root.to_string_lossy().into_owned(),
        },
    });
    slot.drive = Some(Arc::new(LocalDrive::new(data_root.to_path_buf())));
    slot
}

pub fn start() {
    // The switches; a variable an older build read fills in a switch that is absent.
    let args = match args::Args::parse(std::env::args().skip(1)) {
        Ok(args) => args.with_env_fallbacks(|var| std::env::var(var).ok()),
        Err(text) => {
            eprintln!("{text}");
            std::process::exit(2);
        }
    };
    let home = args
        .home
        .clone()
        .or_else(|| path_of(FilePath::get_home_dir().into_option()))
        .unwrap_or_else(|| PathBuf::from("."));
    let downloads = args
        .downloads
        .clone()
        .or_else(|| path_of(FilePath::get_download_dir().into_option()))
        .unwrap_or_else(|| home.join("Downloads"));
    let config_dir = path_of(FilePath::get_config_dir().into_option());
    let drives_file = match &args.drives {
        Some(file) => Some(file.clone()),
        None => config::drives_file(None, config_dir.clone()),
    };
    // The Azlin token server (Buy storage; the refreshes of Azlin drives that name none):
    // `--token-url` / `--profile`, the environment, the shared Azlin config (`AZLIN_CONFIG`,
    // else ~/.azlin/config.json - not in a `--shot` run), the profile's address.
    let user_home = path_of(FilePath::get_home_dir().into_option());
    let token = azcloud_kit::endpoints::token_endpoint(
        args.token_url.as_deref(),
        args.profile.as_deref(),
        &|var: &str| std::env::var(var).ok(),
        if args.kit.shot.is_some() {
            None
        } else {
            user_home.as_deref()
        },
    );
    for problem in &token.problems {
        eprintln!("[azdrive] {problem}");
    }
    // The kit resolves the data root (--data-dir, $AZLIN_DATA, <data dir>/Azlin) and reads the
    // theme and mode saved last time, before the window exists.
    let kit = azul_appkit::ui::create_kit(
        args::SPEC,
        ABOUT,
        &keys::SHORTCUTS,
        &ui_dialogs::CATEGORIES,
        args.kit.clone(),
    );
    let data_root = {
        let mut kit = kit.clone();
        let root = kit
            .downcast_ref::<azul_appkit::ui::Kit>()
            .map(|k| k.data_root.clone());
        root.unwrap_or_else(|| PathBuf::from(azul_appkit::data::ROOT_DIR))
    };
    let settings_drive = Some(LocalDrive::new(data_root.clone()));
    if args.screen == args::Screen::Settings {
        azul_appkit::ui::open_settings(&kit, None);
    }
    let inline_dialogs = args.dialogs == Some(args::Dialogs::Inline);

    if args.kit.sample {
        match args::write_sample(&home) {
            Ok(n) => eprintln!("[azdrive] {n} sample files in {}", home.display()),
            Err(e) => eprintln!("[azdrive] the sample files could not be written: {e}"),
        }
    }

    // The settings, read once before the window opens (no callback waits).
    let mut settings = settings_drive
        .as_ref()
        .and_then(|d| d.get(SETTINGS_KEY).ok())
        .map(|bytes| Settings::from_json(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default();
    if let Some(layout) = args.layout {
        settings.layout = layout;
    }
    // The preview pane and the details pane share the right side (an older build showed both).
    if settings.preview_pane && settings.details_pane {
        settings.details_pane = false;
    }

    let mut slots = vec![
        Slot::new(DriveEntry {
            id: HOME_ID.to_string(),
            name: String::from("Home"),
            location: DriveLocation::Local {
                root: home.to_string_lossy().into_owned(),
            },
        }),
        data_slot(&data_root),
    ];
    let mut message = None;
    match drives_file.as_deref().map(DrivesFile::load) {
        Some(Ok(file)) => slots.extend(
            file.drives
                .into_iter()
                .filter(|d| d.id != HOME_ID && d.id != DATA_ID)
                .map(Slot::new),
        ),
        Some(Err(e)) => {
            message = Some(Message {
                kind: MessageKind::Error,
                text: format!("The drives file could not be read: {e}"),
            });
        }
        None => {}
    }
    eprintln!(
        "[azdrive] {} drive(s); drives file {}; downloads to {}",
        slots.len(),
        drives_file
            .as_deref()
            .map_or_else(|| String::from("none"), |p| p.display().to_string()),
        downloads.display()
    );

    let place = match args.screen {
        args::Screen::ThisPc | args::Screen::Settings => Place::ThisPc,
        args::Screen::QuickAccess => Place::QuickAccess,
        args::Screen::Home => Place::folder(HOME_ID, ""),
        args::Screen::Default => match settings.start {
            model::StartPlace::QuickAccess => Place::QuickAccess,
            model::StartPlace::ThisPc => Place::ThisPc,
        },
    };
    // `--open` (File > Open new window): the place as the address bar names it.
    let drive_names: Vec<(String, String)> = slots
        .iter()
        .map(|slot| (slot.entry.id.clone(), slot.entry.name.clone()))
        .collect();
    let place = args
        .open
        .as_deref()
        .and_then(|text| browse::parse_path(text, &drive_names))
        .unwrap_or(place);
    // The keyring as the worker threads use it (azul's blocking calls), with the locks of its
    // entries beside the drives file.
    let keyring = azcloud_kit::SharedKeyring::new(
        Arc::new(azul_storage::azul_keyring::AzulKeyring::new()),
        azcloud_kit::LockDir::new(lock_dir(drives_file.as_deref())),
    );
    let mut state = DriveState {
        slots,
        place,
        history: History::default(),
        recent: Vec::new(),
        entries: Vec::new(),
        loading: false,
        listing_done: false,
        listing_failed: false,
        list_serial: 0,
        list_cancel: Arc::new(AtomicBool::new(false)),
        refreshing: None,
        stats_asked: HashSet::new(),
        counts: HashMap::new(),
        counts_asked: HashSet::new(),
        view_scroll: (0.0, 0.0),
        view_width: 0.0,
        selection: Selection::default(),
        selected_drive: None,
        selected_pin: None,
        type_ahead: TypeAhead::default(),
        settings,
        search: String::new(),
        find: None,
        find_serial: 0,
        refines: find::Refines::default(),
        select_when_listed: None,
        editing_path: false,
        renaming: None,
        column_drag: None,
        dragging: None,
        tree: TreeState {
            favorites_open: true,
            locations_open: true,
            cloud_open: true,
            ..TreeState::default()
        },
        // Read with the volumes' sizes, below (`refresh_disks`).
        standard_folders: Vec::new(),
        groups_closed: HashSet::new(),
        grid_view: IconGridView::create(),
        pane_ratios: (0.22, 0.7),
        backstage: (args.screen == args::Screen::Settings).then_some(0),
        ribbon_tab: ui_ribbon::RibbonTabKind::default(),
        settings_found: None,
        clipboard: None,
        queue: TransferQueue::default(),
        transfers: HashMap::new(),
        undo: Vec::new(),
        preview: None,
        metadata: HashMap::new(),
        root_counts: HashMap::new(),
        disk: HashMap::new(),
        message,
        popup: None,
        popups_opened: 0,
        keyring_waiting: None,
        keyring_queue: VecDeque::new(),
        drives_file,
        settings_drive,
        downloads,
        open_dir: std::env::temp_dir().join("AzDrive-open"),
        cache_dir: if args.kit.shot.is_some() {
            None
        } else {
            path_of(FilePath::get_cache_dir().into_option()).map(|dir| dir.join("AzDrive"))
        },
        inline_dialogs,
        running: 0,
        trash_serial: 0,
        window_width: 1200.0,
        window_height: 760.0,
        thumbnails: HashMap::new(),
        thumbnails_pending: HashSet::new(),
        audio: None,
        kit,
        token,
        rotated: RotatedSessions::default(),
        keyring,
        claiming: false,
    };
    if args.screen == args::Screen::Settings {
        state.settings_found = Some(state.settings.clone());
    }
    refresh_disks(&mut state);

    // The theme and mode: a switch for this run, else the ones saved on the Options' Appearance.
    let config = azul_appkit::ui::app_config(&state.kit);
    let mut window = azul_appkit::ui::window_options(
        &state.kit,
        layout,
        (1200.0, 760.0),
        (640.0, 420.0),
        startup,
    );
    // The rows in view are estimated from the window's size until the folder view has drawn:
    // the size this run opens at (`--size`, else the default).
    let opens_at = window.window_state.size.dimensions;
    state.window_width = opens_at.width;
    state.window_height = opens_at.height;
    // The title bar shows the ribbon's tabs: the title is what the system's window list names
    // the window by - the open place's path.
    window.window_state.title = AzString::from(window_title(&state));
    let app = App::create(RefAny::new(state), config);
    app.run(window);
}
