//! AzDrive: a file manager on the public azul API that works like Windows
//! 10's File Explorer with the Ribbon.
//!
//! The window is the S5 `BrowserShell`: the app-drawn title row, the Ribbon
//! (FILE opens the backstage with the Options; HOME: Clipboard, Organize,
//! New, Open, Select; SHARE: Send; VIEW: Panes, Layout, Current view,
//! Show/hide; DRIVE: the drives) over the address bar (Back / Forward /
//! Recent locations / Up, the breadcrumb with a menu per crumb, the typed
//! path, Refresh, Search); the navigation pane (Quick access and This PC with
//! the drives and their folders, listed lazily), the content (This PC's
//! drive tiles, Quick access's pinned folders, or a folder in one of
//! Explorer's eight layouts, grouped or not, with check boxes or not), the
//! preview pane, the details pane and the status bar ("N items", "N items
//! selected", the running transfer, the Details / Large icons switch).
//!
//! The drives: "Home" (the user's home folder, a `LocalDrive`), the local
//! folders and S3 drives the user added (AWS S3, Cloudflare R2, MinIO). Every
//! storage call goes through `azul_storage::Drive` on an azul `Thread`; no
//! callback waits on a drive. Copies, moves, uploads and downloads go
//! through ONE queue (one transfer at a time, progress in the status bar):
//! a plan first (every file under a folder, every name taken at the target),
//! a dialog per conflict (Replace / Skip / Keep both, for all), then the run.
//! Delete moves an item into `.azdrive-trash/` on a local drive (Ctrl+Z
//! brings it back) and asks first on a cloud drive. Errors show in an
//! InfoBar over the content.
//!
//! The drives list is `<config dir>/azul-storage/drives.json` WITHOUT
//! secrets (shared with AzMail); the keys go to the OS keyring. The view
//! settings and the Quick access pins are `<config dir>/azul-drive/settings.json`,
//! written through a `LocalDrive` on a Thread.
//!
//! Environment:
//! - `AZDRIVE_HOME`: the folder the Home drive shows (default: the user's home).
//! - `AZUL_DRIVES`: the drives file (default: `<config dir>/azul-storage/drives.json`).
//! - `AZDRIVE_DOWNLOADS`: where "Download" saves (default: the user's Downloads folder).
//! - `AZDRIVE_SETTINGS`: the folder of the settings file (default `<config dir>/azul-drive`).
//! - `AZDRIVE_DIALOGS=inline`: show the dialogs as a sheet inside the window
//!   instead of a modal dialog window (scripts: the debug server drives the main window).
//!
//! On stdout, for scripts: `AZDRIVE_PLACE quick-access | this-pc | <drive id> <prefix or />`,
//! `AZDRIVE_LISTED <drive id> <prefix or /> <entries>`, `AZDRIVE_TREE <drive id>
//! <prefix or /> <folders>`, `AZDRIVE_SELECTED <n> <first key or ->`, `AZDRIVE_LAYOUT <name>`,
//! `AZDRIVE_SORT <column> <asc|desc>`, `AZDRIVE_GROUP <name>`, `AZDRIVE_PANES <nav> <preview> <details>`,
//! `AZDRIVE_TRANSFER <id> planned|conflict|done|failed|cancelled <n>`, `AZDRIVE_DONE <what> <key>`,
//! `AZDRIVE_DELETED <n>`, `AZDRIVE_RENAMING <key>`, `AZDRIVE_PREVIEW <kind> <key>`,
//! `AZDRIVE_CLIPBOARD copy|cut <n>`, `AZDRIVE_TESTED ok|error`, `AZDRIVE_ADDED <drive id>`.
//! Keys and secrets are never printed.

mod actions;
pub mod args;
pub mod browse;
pub mod fileops;
mod jobs;
pub mod keys;
pub mod model;
pub mod preview;
mod ui_dialogs;
mod ui_panes;
mod ui_ribbon;
mod ui_view;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

use azul::{
    css::DarkLightMode,
    error::KeyringResult,
    file::FilePath,
    option::OptionDarkLightMode,
    prelude::*,
    shells::{BrowserShell, ShellThemeScope},
    str::String as AzString,
    url::Url,
    widgets::{AlertKind, Dialog, Titlebar},
    window::WindowDecorations,
};
use azul_storage::{
    azul_transport::AzulTransport,
    config::{self, DriveEntry, DriveLocation, DrivesFile},
    key, Credentials, Drive, DriveError, ListRequest, LocalDrive,
};
use browse::{DriveForm, Entry, History, Place};
use fileops::{ConflictChoice, Plan, SourceItem, TransferKind, TransferQueue};
use jobs::{Done, FolderSize, Job, JobInit, ListPurpose, Outcome, PreviewContent};
use model::{Selection, Settings, TypeAhead};

const HOME_VAR: &str = "AZDRIVE_HOME";
const DOWNLOADS_VAR: &str = "AZDRIVE_DOWNLOADS";
const DIALOGS_VAR: &str = "AZDRIVE_DIALOGS";
const SETTINGS_VAR: &str = "AZDRIVE_SETTINGS";
pub(crate) const USER_AGENT: &str = "AzDrive/0.2";
/// The settings file's key in the settings folder's `LocalDrive`.
pub(crate) const SETTINGS_KEY: &str = "settings.json";
/// Entries per listing page.
const PAGE_SIZE: u32 = 500;
/// Folders per tree listing (one listing per expand).
const TREE_PAGE_SIZE: u32 = 500;
/// The Home drive's id (never in the drives file).
pub(crate) const HOME_ID: &str = "home";
/// The places "Recent locations" remembers.
const RECENT_PLACES: usize = 10;

/// A node of the navigation tree: a drive's folder.
pub(crate) type TreeKey = (String, String);

// ==== Drives ====

/// A drive.
pub(crate) struct Slot {
    pub entry: DriveEntry,
    /// An S3 drive's keys, once read from the keyring or typed in.
    pub credentials: Option<Credentials>,
    /// The drive, once it could be opened; shared with the worker threads.
    pub drive: Option<Arc<dyn Drive>>,
}

impl Slot {
    pub fn new(entry: DriveEntry) -> Self {
        Slot {
            entry,
            credentials: None,
            drive: None,
        }
    }

    /// Whether opening it waits for the keyring.
    pub fn locked(&self) -> bool {
        self.drive.is_none() && self.credentials.is_none() && self.entry.needs_keyring()
    }

    /// The drive, opened on first use.
    pub fn open(&mut self) -> Result<Arc<dyn Drive>, DriveError> {
        if let Some(drive) = &self.drive {
            return Ok(drive.clone());
        }
        let drive: Arc<dyn Drive> = Arc::from(self.entry.open(
            self.credentials.clone(),
            Box::new(AzulTransport::new(USER_AGENT)),
        )?);
        self.drive = Some(drive.clone());
        Ok(drive)
    }

    pub fn is_local(&self) -> bool {
        matches!(self.entry.location, DriveLocation::Local { .. })
    }

    /// The icon of the drive's tiles and tree rows.
    pub fn icon(&self) -> &'static str {
        if self.entry.id == HOME_ID {
            "home"
        } else if self.is_local() {
            "storage"
        } else {
            "cloud"
        }
    }

    /// What the drive is.
    pub fn kind(&self) -> &'static str {
        if self.is_local() {
            "Local Disk"
        } else {
            "S3 bucket"
        }
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
    /// "Add drive" (or new keys for a drive whose keyring entry is gone: `editing`).
    AddDrive {
        form: DriveForm,
        editing: Option<String>,
        serial: u64,
        testing: bool,
        tested: Option<Result<String, String>>,
        error: String,
    },
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
    /// The transfer queue, with Cancel.
    Transfers,
}

/// The navigation tree: which nodes are open, whose children are listed
/// (the folder prefixes, one listing per expand), which are being listed.
#[derive(Default)]
pub(crate) struct TreeState {
    pub quick_open: bool,
    pub this_pc_open: bool,
    pub expanded: HashSet<TreeKey>,
    pub loaded: HashMap<TreeKey, Vec<String>>,
    pub listing: HashSet<TreeKey>,
    /// Nodes to list once their drive's keys are read.
    pub pending: Vec<TreeKey>,
    /// The navigation pane's groups that are closed: Quick access, This PC.
    pub groups_closed: [bool; 2],
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
    /// The open folder's rows.
    pub entries: Vec<Entry>,
    /// Where the next page of the listing starts.
    pub next: Option<String>,
    pub loading: bool,
    /// The listing the rows belong to; an answer for an older one is dropped.
    pub list_serial: u64,
    pub selection: Selection,
    /// The selected drive tile of This PC.
    pub selected_drive: Option<usize>,
    /// The selected pin of Quick access.
    pub selected_pin: Option<usize>,
    pub type_ahead: TypeAhead,
    pub settings: Settings,
    pub search: String,
    pub editing_path: bool,
    pub renaming: Option<Renaming>,
    pub column_drag: Option<ColumnDrag>,
    /// The items being dragged in the window (their drive, their items).
    pub dragging: Option<(String, Vec<SourceItem>)>,
    pub tree: TreeState,
    /// Closed group headers (by label) of the folder view and This PC.
    pub groups_closed: HashSet<String>,
    pub ribbon_tab: usize,
    /// The FILE backstage, open on its page.
    pub backstage: Option<usize>,
    pub settings_category: usize,
    pub settings_search: String,
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
    /// The folder of the settings file, as a drive.
    pub settings_drive: Option<LocalDrive>,
    pub downloads: PathBuf,
    pub open_dir: PathBuf,
    pub inline_dialogs: bool,
    /// Worker threads running.
    pub running: u32,
    /// Deletes so far (names their folders in the trash).
    pub trash_serial: u32,
    /// The window's width, for the grid's rows (the arrow keys).
    pub window_width: f32,
    /// The open folder's pictures as thumbnails (`None`: none can be made).
    pub thumbnails: HashMap<String, Option<azul::image::ImageRef>>,
    /// The pictures whose thumbnails are being made.
    pub thumbnails_pending: HashSet<String>,
    /// The preview's sound while it plays (dropping it stops it).
    pub audio: Option<azul::audio::AudioSink>,
}

impl DriveState {
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

    /// The rows shown: hidden items only when asked, the search's matches.
    pub fn visible_entries(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| self.settings.show_hidden || !e.is_hidden())
            .filter(|e| browse::matches_search(e, &self.search))
            .collect()
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
        self.entries.iter().find(|e| e.key == key)
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key == key)
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
    /// is the window minus the navigation and preview panes).
    pub fn grid_columns(&self) -> usize {
        if !self.settings.layout.is_grid() {
            return 1;
        }
        let mut width = self.window_width;
        if self.settings.navigation_pane {
            width *= 0.78;
        }
        if self.settings.preview_pane {
            width *= 0.7;
        }
        self.settings.layout.columns_in(width - 32.0)
    }

    /// Prints the selection for scripts.
    pub fn print_selection(&self) {
        println!(
            "AZDRIVE_SELECTED {} {}",
            self.selection.len(),
            self.selection.keys().first().map_or("-", String::as_str)
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
    let opened = s.slots.get_mut(index)?.open();
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

/// Lists the open folder from its start (`append == false`) or its next page.
pub(crate) fn start_listing(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    append: bool,
) {
    let Some(drive) = open_current(s) else {
        return;
    };
    let mut request = ListRequest::folder(s.prefix()).with_max_keys(PAGE_SIZE);
    if append {
        match s.next.clone() {
            Some(token) => request = request.with_continuation(token),
            None => return,
        }
    } else {
        s.list_serial += 1;
        s.next = None;
    }
    s.loading = true;
    let serial = s.list_serial;
    spawn(
        info,
        app,
        s,
        Job::List {
            drive,
            request,
            purpose: ListPurpose::Content { serial, append },
        },
    );
}

/// Lists the folders of the tree node `node` (one listing), unlocking its
/// drive first when it needs the keyring.
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
    let request = ListRequest::folder(&node.1).with_max_keys(TREE_PAGE_SIZE);
    s.tree.listing.insert(node.clone());
    spawn(
        info,
        app,
        s,
        Job::List {
            drive,
            request,
            purpose: ListPurpose::Tree { node },
        },
    );
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
    s.search.clear();
    s.entries.clear();
    s.next = None;
    s.selection = Selection::default();
    s.selected_pin = None;
    s.preview = None;
    s.audio = None;
    s.thumbnails.clear();
    s.thumbnails_pending.clear();
    s.loading = false;
    s.list_serial += 1;
    s.backstage = None;
    println!("AZDRIVE_PLACE {}", s.place_line());
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
        // This PC: the drives' sizes again.
        refresh_disks(s);
        return;
    }
    start_listing(info, app, s, false);
}

/// The local volumes' size and free space, from the OS.
pub(crate) fn refresh_disks(s: &mut DriveState) {
    let mut found = Vec::new();
    for slot in &s.slots {
        if let DriveLocation::Local { root } = &slot.entry.location {
            if let Some(space) = FilePath::create(root.as_str()).disk_space().into_option() {
                found.push((slot.entry.id.clone(), (space.total, space.free)));
            }
        }
    }
    s.disk.extend(found);
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

/// Writes the settings file (on a Thread, through the settings folder's drive).
pub(crate) fn save_settings(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(drive) = s.settings_drive.clone() else {
        return;
    };
    let text = s.settings.to_json();
    spawn(info, app, s, Job::SaveSettings { drive, text });
}

// ==== Keyring ====

/// A keyring operation in flight or queued (the keyring answers one at a time).
pub(crate) enum KeyringOp {
    /// Reading an S3 drive's keys to open it.
    Unlock { drive_id: String },
    /// Saving a new drive's keys.
    Store { drive_id: String },
    /// Removing a forgotten drive's keys.
    Forget,
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
                    match Credentials::from_keyring_secret(secret.as_str()) {
                        Ok(credentials) => {
                            s.slots[index].credentials = Some(credentials);
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
                            s.error(format!("The keys of \"{name}\" cannot be read: {e}."));
                        }
                    }
                }
                other => {
                    s.loading = false;
                    s.tree.pending.retain(|node| node.0 != drive_id);
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
        start_listing(info, app, s, false);
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
    if !matches!(outcome, Outcome::Progress { .. } | Outcome::Thumbnail { .. }) {
        s.running = s.running.saturating_sub(1);
    }
    match outcome {
        Outcome::Listed {
            purpose: ListPurpose::Content { serial, append },
            result,
        } => {
            if serial != s.list_serial {
                return Update::DoNothing; // another folder or drive by now
            }
            s.loading = false;
            match result {
                Ok(page) => {
                    let prefix = s.prefix().to_string();
                    let mut more = browse::entries_of(&page, &prefix);
                    if append {
                        more.retain(|e| !s.entries.iter().any(|old| old.key == e.key));
                        s.entries.extend(more);
                    } else {
                        s.entries = more;
                    }
                    browse::sort_entries(&mut s.entries, s.settings.sort);
                    s.next = page.next;
                    let keys = s.visible_keys();
                    let order: Vec<&str> = keys.iter().map(String::as_str).collect();
                    s.selection.retain(&order);
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
                    actions::request_preview(&mut info, &handle, s);
                    actions::request_thumbnails(&mut info, &handle, s);
                }
                Err(e) => s.error(format!("Could not list this folder: {e}")),
            }
        }
        Outcome::Listed {
            purpose: ListPurpose::Tree { node },
            result,
        } => {
            s.tree.listing.remove(&node);
            match result {
                Ok(page) => {
                    if node.1.is_empty() {
                        s.root_counts
                            .insert(node.0.clone(), page.folders.len() + page.objects.len());
                    }
                    println!(
                        "AZDRIVE_TREE {} {} {}",
                        node.0,
                        if node.1.is_empty() { "/" } else { &node.1 },
                        page.folders.len()
                    );
                    let show_hidden = s.settings.show_hidden;
                    let mut folders: Vec<String> = page
                        .folders
                        .into_iter()
                        .filter(|f| show_hidden || !key::last_segment(f).starts_with('.'))
                        .collect();
                    folders.sort_by_key(|f| f.to_lowercase());
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
            println!(
                "AZDRIVE_TESTED {}",
                if result.is_ok() { "ok" } else { "error" }
            );
            if let Some(Popup::AddDrive {
                serial: open_serial,
                testing,
                tested,
                ..
            }) = s.popup.as_mut()
            {
                if *open_serial == serial {
                    *testing = false;
                    *tested = Some(result.map_err(|e| e.to_string()));
                }
            }
        }
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

/// The window's title row, drawn by azul (the window is `NoTitle`, so macOS
/// draws only the traffic lights).
fn title_row(s: &DriveState) -> Dom {
    let title = format!("{} - AzDrive", s.place_name());
    Titlebar::create(AzString::from(title))
        .without_border_bottom()
        .dom()
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<DriveState>() else {
        return Dom::create_body();
    };
    let s = &*guard;

    let mut shell = BrowserShell::create(
        ui_panes::address_bar(s, &app),
        ui_panes::navigation_pane(s, &app),
        ui_view::content(s, &app),
    )
    .with_title_row(title_row(s))
    .with_ribbon(ui_ribbon::ribbon(s, &app))
    .with_status_bar(ui_panes::status_bar(s, &app))
    .with_tree_visible(s.settings.navigation_pane);
    if s.settings.preview_pane {
        shell = shell.with_preview(ui_panes::preview_pane(s, &app, dark));
    }
    if s.settings.details_pane {
        shell = shell.with_details(ui_panes::details_pane(s));
    }
    if let Some(page) = s.backstage {
        shell = shell.with_backstage(ui_dialogs::backstage(s, &app, page));
    }
    let mut body = Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; height: 100%; margin: 0px; font-size: 13px;",
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

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let place = s.place.clone();
        // `--screen settings` opens on the backstage, which a visit closes.
        let backstage = s.backstage;
        go(info, app, s, place, false);
        s.backstage = backstage;
    })
}

pub fn start() {
    let args = match args::Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(text) => {
            eprintln!("{text}");
            std::process::exit(2);
        }
    };
    let home = env_path(HOME_VAR)
        .or_else(|| path_of(FilePath::get_home_dir().into_option()))
        .unwrap_or_else(|| PathBuf::from("."));
    let downloads = env_path(DOWNLOADS_VAR)
        .or_else(|| path_of(FilePath::get_download_dir().into_option()))
        .unwrap_or_else(|| home.join("Downloads"));
    let config_dir = path_of(FilePath::get_config_dir().into_option());
    let drives_file = config::drives_file(
        std::env::var(config::DRIVES_VAR).ok().as_deref(),
        config_dir.clone(),
    );
    let settings_dir = env_path(SETTINGS_VAR).or_else(|| config_dir.map(|d| d.join("azul-drive")));
    let settings_drive = settings_dir.map(LocalDrive::new);
    let inline_dialogs = std::env::var(DIALOGS_VAR).is_ok_and(|v| v.trim() == "inline");

    if args.sample {
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

    let mut slots = vec![Slot::new(DriveEntry {
        id: HOME_ID.to_string(),
        name: String::from("Home"),
        location: DriveLocation::Local {
            root: home.to_string_lossy().into_owned(),
        },
    })];
    let mut message = None;
    match drives_file.as_deref().map(DrivesFile::load) {
        Some(Ok(file)) => slots.extend(
            file.drives
                .into_iter()
                .filter(|d| d.id != HOME_ID)
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
    let mut state = DriveState {
        slots,
        place,
        history: History::default(),
        recent: Vec::new(),
        entries: Vec::new(),
        next: None,
        loading: false,
        list_serial: 0,
        selection: Selection::default(),
        selected_drive: None,
        selected_pin: None,
        type_ahead: TypeAhead::default(),
        settings,
        search: String::new(),
        editing_path: false,
        renaming: None,
        column_drag: None,
        dragging: None,
        tree: TreeState {
            quick_open: true,
            this_pc_open: true,
            ..TreeState::default()
        },
        groups_closed: HashSet::new(),
        ribbon_tab: 0,
        backstage: (args.screen == args::Screen::Settings).then_some(0),
        settings_category: 0,
        settings_search: String::new(),
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
        inline_dialogs,
        running: 0,
        trash_serial: 0,
        window_width: 1200.0,
        thumbnails: HashMap::new(),
        thumbnails_pending: HashSet::new(),
        audio: None,
    };
    refresh_disks(&mut state);

    let mut config = AppConfig::create();
    if let Some(theme) = args.theme.as_deref() {
        config.set_theme(AzString::from(theme));
    }
    if let Some(dark) = args.dark {
        config.set_mode(OptionDarkLightMode::Some(if dark {
            DarkLightMode::Dark
        } else {
            DarkLightMode::Light
        }));
    }
    let app = App::create(RefAny::new(state), config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1200.0, 760.0);
    window.window_state.title = AzString::from("AzDrive");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}
