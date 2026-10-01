//! AzDrive: a file browser on the public azul API, laid out like Windows
//! Explorer's "Computer" view.
//!
//! A Ribbon (Home: Copy / Paste / Delete / Rename / New folder / Upload /
//! Download / Open; View: Tiles or Details, the panes, the sort; Drive: Add /
//! Remove / Properties), an address bar (Back / Forward / Up, the trail with
//! a menu per crumb, a click that turns it into an editable path, Refresh, a
//! search box that filters the listing), a navigation tree ("This PC", the
//! drives, their folders listed lazily - one listing per expand), the "This
//! PC" view (collapsible groups: "Local" with the Home drive, "Cloud / S3"
//! with the S3 drives; a tile per drive with a capacity bar for a local
//! volume - real free / total from the OS), the folder views (tiles, or a
//! sortable Name / Size / Modified list) and a details pane for the selected
//! drive, folder or file.
//!
//! The drives: "Home" (the user's home folder, a `LocalDrive`) and the S3
//! drives the user added (AWS S3, Cloudflare R2, MinIO). "Add drive" asks
//! for an S3-compatible bucket; "Test connection" makes ONE listing call.
//! The drives list is `<config dir>/azul-storage/drives.json` WITHOUT secrets
//! (shared with AzMail); the keys go to the OS keyring.
//!
//! Every storage call is blocking (`azul_storage::Drive`) and runs on an azul
//! `Thread`; the answer comes back through the thread's write-back. No
//! callback waits on the network. Browsing fetches listings only; "Download"
//! and "Open" fetch ONE object.
//!
//! Environment:
//! - `AZDRIVE_HOME`: the folder the Home drive shows (default: the user's home).
//! - `AZUL_DRIVES`: the drives file (default: `<config dir>/azul-storage/drives.json`).
//! - `AZDRIVE_DOWNLOADS`: where "Download" saves (default: the user's Downloads folder).
//! - `AZDRIVE_DIALOGS=inline`: show the forms as a sheet inside the window
//!   instead of a modal dialog window (scripts: the debug server drives the main window).
//!
//! On stdout, for scripts: `AZDRIVE_PLACE this-pc` | `<drive id> <prefix or />`,
//! `AZDRIVE_LISTED <drive id> <prefix or /> <entries>`, `AZDRIVE_TREE <drive id>
//! <prefix or /> <folders>`, `AZDRIVE_TESTED ok|error`, `AZDRIVE_ADDED <drive id>`,
//! `AZDRIVE_DOWNLOADED <path>`, `AZDRIVE_UPLOADED <key>`, `AZDRIVE_DELETED <key>`,
//! `AZDRIVE_DONE <what> <key>`. Keys and secrets are never printed.

pub mod browse;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use azul::{
    callbacks::{
        AccordionOnToggleCallbackType, AddressBarOnEventCallbackType, ButtonOnClickCallbackType,
        RibbonOnTabClickCallbackType, TileOnClickCallbackType, TreeViewOnNodeClickCallbackType,
        TreeViewOnNodeToggleCallbackType,
    },
    css::{DarkLightMode, WindowDecorations},
    dialog::{FileDialog, FileOpenResult},
    error::KeyringResult,
    file::FilePath,
    menu::{Menu, MenuItem, StringMenuItem},
    option::{OptionFileTypeList, OptionPixelValueNoPercent},
    prelude::*,
    str::String as AzString,
    url::Url,
    vec::{ListViewRowVec, StringVec},
    widgets::{
        Accordion, AccordionSection, AccordionVariant, AddressBar, AddressBarEvent,
        AddressBarEventKind, ButtonType, CheckBoxState, DetailsPane, Dialog, DialogState,
        ListView, ListViewRow, ListViewState, OnTextInputReturn, Ribbon, RibbonButton,
        RibbonColumn, RibbonGroup, RibbonItem, RibbonTab, TextInputState, TextInputValid, Tile,
        TileCapacity, Titlebar, TreeView, TreeViewNode,
    },
};
use azul_storage::{
    azul_transport::AzulTransport,
    config::{self, DriveEntry, DriveLocation, DrivesFile},
    key, transfer, Credentials, Drive, DriveError, ListPage, ListRequest, S3Config, S3Drive,
};
use browse::{Column, DriveForm, Entry, History, Place, Sort};

const HOME_VAR: &str = "AZDRIVE_HOME";
const DOWNLOADS_VAR: &str = "AZDRIVE_DOWNLOADS";
const DIALOGS_VAR: &str = "AZDRIVE_DIALOGS";
const USER_AGENT: &str = "AzDrive/0.1";
/// Entries per listing page.
const PAGE_SIZE: u32 = 200;
/// Folders per tree listing (one listing per expand).
const TREE_PAGE_SIZE: u32 = 500;
/// The Home drive's id (never in the drives file).
const HOME_ID: &str = "home";

// ==== Colours ====

/// The window's colours in one mode. The widgets follow the app theme
/// (flat / flora) and the mode themselves.
struct Palette {
    dark: bool,
    toolbar: &'static str,
    toolbar_rgb: (u8, u8, u8),
    content: &'static str,
    text: &'static str,
    text_rgb: (u8, u8, u8),
    secondary: &'static str,
    line: &'static str,
    notice_bg: &'static str,
    notice_text: &'static str,
    error: &'static str,
    ok: &'static str,
}

const LIGHT: Palette = Palette {
    dark: false,
    toolbar: "#f6f7f9",
    toolbar_rgb: (0xf6, 0xf7, 0xf9),
    content: "#ffffff",
    text: "#1d2330",
    text_rgb: (0x1d, 0x23, 0x30),
    secondary: "#5d6677",
    line: "#d9dce3",
    notice_bg: "#e6eefc",
    notice_text: "#2c4a7a",
    error: "#b3261e",
    ok: "#1f7a3a",
};

const DARK: Palette = Palette {
    dark: true,
    toolbar: "#2b2d31",
    toolbar_rgb: (0x2b, 0x2d, 0x31),
    content: "#1e1f22",
    text: "#e6e7ea",
    text_rgb: (0xe6, 0xe7, 0xea),
    secondary: "#a0a4ad",
    line: "#3a3c42",
    notice_bg: "#1f3350",
    notice_text: "#cfe0ff",
    error: "#ff8a80",
    ok: "#7ddc95",
};

// ==== State ====

/// A drive.
struct Slot {
    entry: DriveEntry,
    /// An S3 drive's keys, once read from the keyring or typed in.
    credentials: Option<Credentials>,
    /// The drive, once it could be opened; shared with the worker threads.
    drive: Option<Arc<dyn Drive>>,
}

impl Slot {
    fn new(entry: DriveEntry) -> Self {
        Slot {
            entry,
            credentials: None,
            drive: None,
        }
    }

    /// Whether opening it waits for the keyring.
    fn locked(&self) -> bool {
        self.drive.is_none() && self.credentials.is_none() && self.entry.needs_keyring()
    }

    /// The drive, opened on first use.
    fn open(&mut self) -> Result<Arc<dyn Drive>, DriveError> {
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

    fn is_local(&self) -> bool {
        matches!(self.entry.location, DriveLocation::Local { .. })
    }

    /// The icon of the drive's tiles and tree rows.
    fn icon(&self) -> &'static str {
        if self.entry.id == HOME_ID {
            "home"
        } else if self.is_local() {
            "storage"
        } else {
            "cloud"
        }
    }

    /// What the drive is.
    fn kind(&self) -> &'static str {
        if self.is_local() {
            "Local Disk"
        } else {
            "S3 bucket"
        }
    }
}

/// What the dialog (or the inline sheet) shows.
enum Popup {
    /// "Add drive" (or new keys for a drive whose keyring entry is gone: `editing`).
    AddDrive {
        form: DriveForm,
        editing: Option<String>,
        serial: u64,
        testing: bool,
        /// The last "Test connection": what it said.
        tested: Option<Result<String, String>>,
        error: String,
    },
    ConfirmDelete {
        key: String,
        name: String,
    },
    ConfirmForget {
        drive_id: String,
    },
    /// "Rename": the file `key`, the name typed so far.
    Rename {
        key: String,
        name: String,
    },
    /// "New folder": the name typed so far.
    NewFolder {
        name: String,
    },
}

/// A keyring operation in flight or queued (the keyring answers one at a time).
enum KeyringOp {
    /// Reading an S3 drive's keys to open it.
    Unlock { drive_id: String },
    /// Saving a new drive's keys.
    Store { drive_id: String },
    /// Removing a forgotten drive's keys.
    Forget,
}

/// The request behind a [`KeyringOp`]. Holds a secret while queued; never printed.
enum KeyringCall {
    Get(String),
    Store(String, String),
    Delete(String),
}

/// How the folder views draw a folder.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewLayout {
    Tiles,
    Details,
}

/// A node of the navigation tree: a drive's folder.
type TreeKey = (String, String);

/// The navigation tree: which nodes are open, whose children are listed
/// (the folder prefixes, one listing per expand), which are being listed.
#[derive(Default)]
struct TreeState {
    this_pc_open: bool,
    expanded: HashSet<TreeKey>,
    loaded: HashMap<TreeKey, Vec<String>>,
    listing: HashSet<TreeKey>,
    /// Nodes to list once their drive's keys are read.
    pending: Vec<TreeKey>,
}

struct DriveState {
    slots: Vec<Slot>,
    /// Where the window is.
    place: Place,
    history: History,
    /// The open folder's rows (a `Place::Folder`).
    entries: Vec<Entry>,
    /// Where the next page of the listing starts.
    next: Option<String>,
    loading: bool,
    /// The listing the rows belong to; an answer for an older one is dropped.
    list_serial: u64,
    /// The selected row of the open folder (its key).
    selected: Option<String>,
    /// The selected drive tile of "This PC".
    selected_drive: Option<usize>,
    sort: Sort,
    tree: TreeState,
    /// The "This PC" groups that are closed: Local, Cloud / S3.
    groups_closed: [bool; 2],
    ribbon_tab: usize,
    layout: ViewLayout,
    show_navigation: bool,
    show_details: bool,
    editing_path: bool,
    search: String,
    /// "Copy": a file to paste (its drive, its key).
    clipboard: Option<(String, String)>,
    /// Items listed at a drive's root (the first page), once listed.
    root_counts: HashMap<String, usize>,
    /// A local drive's volume: (total, free) bytes.
    disk: HashMap<String, (u64, u64)>,
    notice: String,
    error: String,
    popup: Option<Popup>,
    popups_opened: u64,
    keyring_waiting: Option<KeyringOp>,
    keyring_queue: VecDeque<(KeyringOp, KeyringCall)>,
    drives_file: Option<PathBuf>,
    downloads: PathBuf,
    open_dir: PathBuf,
    inline_dialogs: bool,
    /// Worker threads running (uploads, downloads, listings).
    running: u32,
}

impl DriveState {
    fn slot_index(&self, drive_id: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.entry.id == drive_id)
    }

    /// The drive of the open folder.
    fn current_drive(&self) -> Option<usize> {
        match &self.place {
            Place::ThisPc => None,
            Place::Folder { drive, .. } => self.slot_index(drive),
        }
    }

    /// The open folder's prefix (`""` on "This PC" too).
    fn prefix(&self) -> &str {
        match &self.place {
            Place::ThisPc => "",
            Place::Folder { prefix, .. } => prefix,
        }
    }

    fn selected_entry(&self) -> Option<&Entry> {
        let key = self.selected.as_deref()?;
        self.entries.iter().find(|e| e.key == key)
    }

    /// The rows the search keeps, in order.
    fn visible_entries(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| browse::matches_search(e, &self.search))
            .collect()
    }

    /// The drives as `(id, name)`, for the typed path.
    fn drive_names(&self) -> Vec<(String, String)> {
        self.slots
            .iter()
            .map(|s| (s.entry.id.clone(), s.entry.name.clone()))
            .collect()
    }

    /// The name of the place's drive.
    fn drive_name(&self, place: &Place) -> String {
        match place {
            Place::ThisPc => String::from(browse::THIS_PC),
            Place::Folder { drive, .. } => self
                .slot_index(drive)
                .map(|i| self.slots[i].entry.name.clone())
                .unwrap_or_else(|| drive.clone()),
        }
    }

    /// What the open place is called: the drive, or the open folder.
    fn place_name(&self) -> String {
        match &self.place {
            Place::ThisPc => String::from(browse::THIS_PC),
            Place::Folder { prefix, .. } if prefix.is_empty() => self.drive_name(&self.place),
            Place::Folder { prefix, .. } => key::last_segment(prefix).to_string(),
        }
    }

    fn place_line(&self) -> String {
        match &self.place {
            Place::ThisPc => String::from("this-pc"),
            Place::Folder { drive, prefix } => format!(
                "{drive} {}",
                if prefix.is_empty() { "/" } else { prefix }
            ),
        }
    }
}

// ==== Worker threads ====

/// Why a listing was asked for.
enum ListPurpose {
    /// The open folder's rows.
    Content { serial: u64, append: bool },
    /// A tree node's folders.
    Tree { node: TreeKey },
}

/// One blocking storage task.
enum Job {
    List {
        drive: Arc<dyn Drive>,
        request: ListRequest,
        purpose: ListPurpose,
    },
    Download {
        drive: Arc<dyn Drive>,
        key: String,
        size: Option<u64>,
        folder: PathBuf,
        open: bool,
    },
    Upload {
        drive: Arc<dyn Drive>,
        source: PathBuf,
        key: String,
    },
    Delete {
        drive: Arc<dyn Drive>,
        key: String,
    },
    Test {
        serial: u64,
        config: S3Config,
        credentials: Credentials,
    },
    /// A file copied within a drive or across two: ONE get, ONE put.
    Copy {
        source: Arc<dyn Drive>,
        key: String,
        target: Arc<dyn Drive>,
        target_key: String,
    },
    /// A file renamed: get, put, delete.
    Rename {
        drive: Arc<dyn Drive>,
        from: String,
        to: String,
    },
    /// A folder created: a local directory, or an S3 folder marker.
    NewFolder {
        drive: Arc<dyn Drive>,
        local_path: Option<PathBuf>,
        key: String,
    },
}

/// What a job answers, on the UI thread.
enum Outcome {
    Listed {
        purpose: ListPurpose,
        result: Result<ListPage, DriveError>,
    },
    Downloaded {
        key: String,
        open: bool,
        result: Result<(PathBuf, u64), DriveError>,
    },
    Uploaded {
        key: String,
        result: Result<u64, DriveError>,
    },
    Deleted {
        key: String,
        result: Result<(), DriveError>,
    },
    Tested {
        serial: u64,
        result: Result<String, DriveError>,
    },
    /// A copy, rename or new folder: `what` names it for the log.
    Done {
        what: &'static str,
        key: String,
        result: Result<(), DriveError>,
    },
}

/// A thread's start data: the job, taken out once.
struct JobInit {
    job: Option<Job>,
}

/// A thread's answer, taken out once by the write-back.
struct Done {
    outcome: Option<Outcome>,
}

fn run_job(job: Job) -> Outcome {
    match job {
        Job::List {
            drive,
            request,
            purpose,
        } => Outcome::Listed {
            purpose,
            result: drive.list(&request),
        },
        Job::Download {
            drive,
            key,
            size,
            folder,
            open,
        } => {
            let result = transfer::download_path(&folder, &key)
                .ok_or_else(|| DriveError::InvalidKey {
                    key: key.clone(),
                    reason: "it has no usable file name",
                })
                .and_then(|dest| {
                    let written =
                        transfer::download_to_file(&*drive, &key, size, &dest, transfer::CHUNK)?;
                    Ok((dest, written))
                });
            Outcome::Downloaded { key, open, result }
        }
        Job::Upload { drive, source, key } => Outcome::Uploaded {
            result: transfer::upload_file(&*drive, &source, &key),
            key,
        },
        Job::Delete { drive, key } => Outcome::Deleted {
            result: drive.delete(&key),
            key,
        },
        Job::Test {
            serial,
            config,
            credentials,
        } => {
            // ONE listing call, at most one entry: does the bucket answer to these keys?
            let result = S3Drive::new(
                config,
                credentials,
                Box::new(AzulTransport::new(USER_AGENT)),
            )
            .and_then(|drive| drive.list(&ListRequest::folder("").with_max_keys(1)))
            .map(|page| {
                if page.folders.is_empty() && page.objects.is_empty() {
                    String::from("Connection OK: the bucket answered; it is empty.")
                } else {
                    String::from("Connection OK: the bucket answered and lists its files.")
                }
            });
            Outcome::Tested { serial, result }
        }
        Job::Copy {
            source,
            key,
            target,
            target_key,
        } => Outcome::Done {
            what: "copied",
            result: source
                .get(&key)
                .and_then(|bytes| target.put(&target_key, &bytes)),
            key: target_key,
        },
        Job::Rename { drive, from, to } => Outcome::Done {
            what: "renamed",
            result: drive
                .get(&from)
                .and_then(|bytes| drive.put(&to, &bytes))
                .and_then(|()| drive.delete(&from)),
            key: to,
        },
        Job::NewFolder {
            drive,
            local_path,
            key,
        } => Outcome::Done {
            what: "folder",
            result: match local_path {
                Some(path) => std::fs::create_dir_all(&path)
                    .map_err(|e| DriveError::Io(format!("{}: {e}", path.display()))),
                None => drive.put(&key, &[]),
            },
            key,
        },
    }
}

/// Runs on a worker thread: the blocking storage call, then its answer to the UI thread.
extern "C" fn job_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut init| init.job.take())
    else {
        return;
    };
    let outcome = run_job(job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_job_done,
        RefAny::new(Done {
            outcome: Some(outcome),
        }),
    )));
}

fn spawn(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, job: Job) {
    s.running += 1;
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit { job: Some(job) }),
            app.clone(),
            job_thread,
        ),
    );
}

/// The open drive, opened on first use; an error is shown.
fn open_current(s: &mut DriveState) -> Option<Arc<dyn Drive>> {
    let index = s.current_drive()?;
    match s.slots[index].open() {
        Ok(drive) => Some(drive),
        Err(e) => {
            s.loading = false;
            s.error = e.to_string();
            None
        }
    }
}

/// Lists the open folder from its start (`append == false`) or its next page.
fn start_listing(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, append: bool) {
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
        s.entries.clear();
        s.next = None;
        s.selected = None;
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
fn start_tree_listing(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, node: TreeKey) {
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
    let drive = match s.slots[index].open() {
        Ok(drive) => drive,
        Err(e) => {
            s.error = e.to_string();
            return;
        }
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
fn unlock(info: &mut CallbackInfo, s: &mut DriveState, index: usize) {
    let drive_id = s.slots[index].entry.id.clone();
    if matches!(&s.keyring_waiting, Some(KeyringOp::Unlock { drive_id: id }) if *id == drive_id)
        || s
            .keyring_queue
            .iter()
            .any(|(op, _)| matches!(op, KeyringOp::Unlock { drive_id: id } if *id == drive_id))
    {
        return;
    }
    s.notice = format!(
        "Reading the keys of \"{}\" from the keyring...",
        s.slots[index].entry.name
    );
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
fn go(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, place: Place, remember: bool) {
    if remember && place != s.place {
        let leaving = s.place.clone();
        s.history.visit(leaving);
    }
    s.place = place;
    s.error.clear();
    s.editing_path = false;
    s.search.clear();
    s.entries.clear();
    s.next = None;
    s.selected = None;
    s.loading = false;
    s.list_serial += 1;
    println!("AZDRIVE_PLACE {}", s.place_line());
    let Some(index) = s.current_drive() else {
        return;
    };
    if s.slots[index].locked() {
        s.loading = true;
        unlock(info, s, index);
        return;
    }
    s.notice.clear();
    start_listing(info, app, s, false);
}

/// Lists the open folder again.
fn refresh(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let place = s.place.clone();
    go(info, app, s, place, false);
}

/// The place above the open one: the parent folder, the drive's root, "This PC".
fn place_up(place: &Place) -> Option<Place> {
    match place {
        Place::ThisPc => None,
        Place::Folder { prefix, .. } if prefix.is_empty() => Some(Place::ThisPc),
        Place::Folder { drive, prefix } => Some(Place::folder(drive, &key::parent_prefix(prefix))),
    }
}

// ==== Keyring ====

fn issue(info: &mut CallbackInfo, call: &KeyringCall) {
    match call {
        KeyringCall::Get(key) => info.keyring_get(key.as_str()),
        KeyringCall::Store(key, secret) => info.keyring_store(key.as_str(), secret.as_str(), false),
        KeyringCall::Delete(key) => info.keyring_delete(key.as_str()),
    }
}

/// Sends a keyring request now, or after the one in flight (the keyring's answer
/// does not say which request it answers, so there is one at a time).
fn keyring(info: &mut CallbackInfo, s: &mut DriveState, op: KeyringOp, call: KeyringCall) {
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
                            s.notice.clear();
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
                            s.error = format!("The keys of \"{name}\" cannot be read: {e}.");
                        }
                    }
                }
                other => {
                    s.loading = false;
                    s.notice.clear();
                    s.tree.pending.retain(|node| node.0 != drive_id);
                    s.error = format!(
                        "\"{name}\" cannot be opened: {}. Enter its keys again.",
                        keyring_problem(other)
                    );
                    if is_current && matches!(other, KeyringResult::NotFound) {
                        open_drive_form(s, Some(index));
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
                s.notice = format!("\"{name}\" is saved; its keys are in the system keyring.");
            } else {
                s.error = format!(
                    "The keys of \"{name}\" could not be saved: {}. They are kept until AzDrive \
                     closes.",
                    keyring_problem(&result)
                );
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
fn open_with_os(path: &Path) -> Result<(), String> {
    let url = browse::file_url(path);
    match Url::parse(url.as_str()).into_result() {
        Ok(url) if url.open() => Ok(()),
        Ok(_) => Err(format!("the system could not open {}", path.display())),
        Err(e) => Err(format!("{} is not a URL: {}", url, e.message.as_str())),
    }
}

/// A folder's content changed: the tree lists it again on its next expand
/// (now, when it is open).
fn tree_invalidate(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, node: TreeKey) {
    if s.tree.loaded.remove(&node).is_some() && s.tree.expanded.contains(&node) {
        start_tree_listing(info, app, s, node);
    }
}

extern "C" fn on_job_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
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
    s.running = s.running.saturating_sub(1);
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
                    browse::sort_entries(&mut s.entries, s.sort);
                    s.next = page.next;
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
                }
                Err(e) => {
                    s.error = format!("Could not list this folder: {e}");
                    eprintln!("[azdrive] {}", s.error);
                }
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
                    let mut folders = page.folders;
                    folders.sort_by_key(|f| f.to_lowercase());
                    s.tree.loaded.insert(node, folders);
                }
                Err(e) => {
                    s.tree.expanded.remove(&node);
                    s.error = format!("Could not list the folder: {e}");
                }
            }
        }
        Outcome::Downloaded { key, open, result } => match result {
            Ok((dest, written)) => {
                println!("AZDRIVE_DOWNLOADED {}", dest.display());
                s.notice = format!(
                    "Downloaded \"{}\" ({}) to {}",
                    key::last_segment(&key),
                    browse::format_size(Some(written)),
                    dest.display()
                );
                if open {
                    if let Err(e) = open_with_os(&dest) {
                        s.error = e;
                    }
                }
            }
            Err(e) => s.error = format!("Could not download \"{key}\": {e}"),
        },
        Outcome::Uploaded { key, result } => match result {
            Ok(written) => {
                println!("AZDRIVE_UPLOADED {key}");
                s.notice = format!(
                    "Uploaded \"{}\" ({}).",
                    key::last_segment(&key),
                    browse::format_size(Some(written))
                );
                start_listing(&mut info, &handle, s, false);
                s.selected = Some(key);
            }
            Err(e) => s.error = format!("Could not upload \"{key}\": {e}"),
        },
        Outcome::Deleted { key, result } => match result {
            Ok(()) => {
                println!("AZDRIVE_DELETED {key}");
                s.notice = format!("Deleted \"{}\".", key::last_segment(&key));
                start_listing(&mut info, &handle, s, false);
            }
            Err(e) => s.error = format!("Could not delete \"{key}\": {e}"),
        },
        Outcome::Done { what, key, result } => match result {
            Ok(()) => {
                println!("AZDRIVE_DONE {what} {key}");
                s.notice = match what {
                    "copied" => format!("Copied to \"{}\".", key::last_segment(&key)),
                    "renamed" => format!("Renamed to \"{}\".", key::last_segment(&key)),
                    _ => format!("Created the folder \"{}\".", key::last_segment(&key)),
                };
                start_listing(&mut info, &handle, s, false);
                s.selected = Some(key);
                if let Place::Folder { drive, prefix } = s.place.clone() {
                    tree_invalidate(&mut info, &handle, s, (drive, prefix));
                }
            }
            Err(e) => s.error = format!("Could not finish ({what}) \"{key}\": {e}"),
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
    }
    Update::RefreshDom
}

// ==== Layout ====

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let palette = match info.get_mode() {
        DarkLightMode::Dark => &DARK,
        DarkLightMode::Light => &LIGHT,
    };
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<DriveState>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let p = palette;

    let mut main = Dom::create_div()
        .with_id("main")
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;");
    if s.show_navigation {
        main.add_child(navigation_pane(s, &app, p));
    }
    main.add_child(content(s, &app, p));
    if s.inline_dialogs {
        if let Some(popup) = &s.popup {
            main.add_child(inline_sheet(popup, s, &app, p));
        }
    }

    let mut body = Dom::create_body()
        .with_css(format!(
            "display: flex; flex-direction: column; height: 100%; margin: 0px; font-family: \
             sans-serif; font-size: 14px; color: {}; background: {};",
            p.text, p.content
        ))
        .with_child(title_row(p))
        .with_child(ribbon(s, &app))
        .with_child(address_bar(s, &app));
    if !s.notice.is_empty() {
        body.add_child(
            Dom::create_span_with_text(s.notice.as_str())
                .with_id("drive-notice")
                .with_css(format!(
                    "padding: 6px 16px; font-size: 13px; color: {}; background: {};",
                    p.notice_text, p.notice_bg
                )),
        );
    }
    if !s.error.is_empty() {
        body.add_child(
            Dom::create_span_with_text(s.error.as_str())
                .with_id("drive-error")
                .with_css(format!(
                    "padding: 6px 16px; font-size: 13px; color: {};",
                    p.error
                )),
        );
    }
    body.add_child(main);
    if s.show_details {
        body.add_child(details_pane(s));
    }
    body = body
        .with_child(Dom::create_span_with_text(footer_text(s)).with_css(format!(
            "padding: 4px 16px; font-size: 12px; color: {}; background: {}; border-top: 1px \
             solid {};",
            p.secondary, p.toolbar, p.line
        )))
        // The keyring's answers arrive as a window event.
        .with_callback(
            EventFilter::Window(WindowEventFilter::KeyringResult),
            app.clone(),
            on_keyring_result,
        );
    if !s.inline_dialogs {
        if let Some(popup) = &s.popup {
            let (title, panel) = popup_parts(popup, s, &app, p);
            body.add_child(
                Dialog::create(panel)
                    .with_title(title.as_str())
                    .with_open(true)
                    .with_modal(true)
                    .with_close_button(true)
                    .with_on_close(app.clone(), on_dialog_closed)
                    .dom(),
            );
        }
    }
    body
}

/// The window's title row, drawn by azul (the window is `NoTitle`, so macOS draws
/// only the traffic lights): the toolbar's colour and no line of its own.
fn title_row(p: &Palette) -> Dom {
    let (r, g, b) = p.toolbar_rgb;
    let mut bar = Titlebar::create("AzDrive")
        .with_background(ColorU::rgb(r, g, b))
        .without_border_bottom();
    if p.dark {
        let (r, g, b) = p.text_rgb;
        bar.title_color = ColorU::rgb(r, g, b);
    }
    bar.dom()
}

// ==== The ribbon ====

/// A command of the ribbon.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Copy,
    Paste,
    Delete,
    Rename,
    NewFolder,
    Upload,
    Download,
    Open,
    LayoutTiles,
    LayoutDetails,
    ToggleNavigation,
    ToggleDetails,
    SortName,
    SortSize,
    SortModified,
    AddDrive,
    RemoveDrive,
    Properties,
}

/// A ribbon button's click data.
struct ActionRef {
    app: RefAny,
    action: Action,
}

fn ribbon_button(app: &RefAny, icon: &str, label: &str, action: Action, toggled: bool) -> RibbonButton {
    RibbonButton::create(AzString::from(icon), AzString::from(label))
        .with_toggled(toggled)
        .with_on_click(
            RefAny::new(ActionRef {
                app: app.clone(),
                action,
            }),
            on_action as ButtonOnClickCallbackType,
        )
}

fn large(app: &RefAny, icon: &str, label: &str, action: Action) -> RibbonItem {
    RibbonItem::LargeButton(ribbon_button(app, icon, label, action, false))
}

fn small(app: &RefAny, icon: &str, label: &str, action: Action, toggled: bool) -> RibbonItem {
    RibbonItem::SmallButton(ribbon_button(app, icon, label, action, toggled))
}

fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(
        items
            .into_iter()
            .fold(RibbonColumn::create(), |c, it| c.with_item(it)),
    )
}

fn ribbon(s: &DriveState, app: &RefAny) -> Dom {
    // Upper-case tabs (as Office's), so "HOME" is never the Home drive.
    let home = RibbonTab::create(AzString::from("HOME"))
        .with_group(
            RibbonGroup::create(AzString::from("Clipboard"))
                .with_item(large(app, "content_copy", "Copy", Action::Copy))
                .with_item(large(app, "content_paste", "Paste", Action::Paste)),
        )
        .with_group(
            RibbonGroup::create(AzString::from("Organize"))
                .with_item(large(app, "delete", "Delete", Action::Delete))
                .with_item(column(vec![
                    small(app, "drive_file_rename_outline", "Rename", Action::Rename, false),
                    small(app, "create_new_folder", "New folder", Action::NewFolder, false),
                ])),
        )
        .with_group(
            RibbonGroup::create(AzString::from("Open"))
                .with_item(large(app, "open_in_new", "Open", Action::Open))
                .with_item(column(vec![
                    small(app, "upload", "Upload", Action::Upload, false),
                    small(app, "download", "Download", Action::Download, false),
                ])),
        );
    let view = RibbonTab::create(AzString::from("VIEW"))
        .with_group(
            RibbonGroup::create(AzString::from("Layout"))
                .with_item(large_toggled(
                    app,
                    "grid_view",
                    "Tiles",
                    Action::LayoutTiles,
                    s.layout == ViewLayout::Tiles,
                ))
                .with_item(large_toggled(
                    app,
                    "view_list",
                    "Details",
                    Action::LayoutDetails,
                    s.layout == ViewLayout::Details,
                )),
        )
        .with_group(
            RibbonGroup::create(AzString::from("Panes")).with_item(column(vec![
                small(
                    app,
                    "account_tree",
                    "Navigation pane",
                    Action::ToggleNavigation,
                    s.show_navigation,
                ),
                small(
                    app,
                    "info",
                    "Details pane",
                    Action::ToggleDetails,
                    s.show_details,
                ),
            ])),
        )
        .with_group(
            RibbonGroup::create(AzString::from("Sort by")).with_item(column(vec![
                small(
                    app,
                    "sort_by_alpha",
                    "Name",
                    Action::SortName,
                    s.sort.column == Column::Name,
                ),
                small(
                    app,
                    "data_usage",
                    "Size",
                    Action::SortSize,
                    s.sort.column == Column::Size,
                ),
                small(
                    app,
                    "schedule",
                    "Modified",
                    Action::SortModified,
                    s.sort.column == Column::Modified,
                ),
            ])),
        );
    let drive = RibbonTab::create(AzString::from("DRIVE")).with_group(
        RibbonGroup::create(AzString::from("Drives"))
            .with_item(large(app, "add_circle", "Add drive", Action::AddDrive))
            .with_item(large(app, "remove_circle", "Remove drive", Action::RemoveDrive))
            .with_item(large(app, "info", "Properties", Action::Properties)),
    );
    let mut ribbon = Ribbon::create(vec![home, view, drive]).with_active_tab(s.ribbon_tab);
    ribbon.set_on_tab_click(app.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType);
    ribbon.dom_desktop()
}

fn large_toggled(app: &RefAny, icon: &str, label: &str, action: Action, toggled: bool) -> RibbonItem {
    RibbonItem::LargeButton(ribbon_button(app, icon, label, action, toggled))
}

extern "C" fn on_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    s.ribbon_tab = index;
    Update::RefreshDom
}

extern "C" fn on_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| match action {
        Action::Copy => copy_selected(s),
        Action::Paste => paste(info, app, s),
        Action::Delete => ask_delete(s),
        Action::Rename => ask_rename(s),
        Action::NewFolder => ask_new_folder(s),
        Action::Upload => {
            let _request = FileDialog::open_file(
                "Upload a file",
                OptionString::None,
                OptionFileTypeList::None,
                app.clone(),
                on_upload_picked,
            );
        }
        Action::Download => match s.selected_entry().cloned() {
            Some(entry) if !entry.is_folder => download(info, app, s, &entry, false),
            Some(_) => s.error = String::from("Download works on files; open the folder instead."),
            None => s.error = String::from("Select a file to download."),
        },
        Action::Open => open_selected(info, app, s),
        Action::LayoutTiles => s.layout = ViewLayout::Tiles,
        Action::LayoutDetails => s.layout = ViewLayout::Details,
        Action::ToggleNavigation => s.show_navigation = !s.show_navigation,
        Action::ToggleDetails => s.show_details = !s.show_details,
        Action::SortName => sort_by(s, Column::Name),
        Action::SortSize => sort_by(s, Column::Size),
        Action::SortModified => sort_by(s, Column::Modified),
        Action::AddDrive => {
            if s.popup.is_none() {
                open_drive_form(s, None);
            }
        }
        Action::RemoveDrive => ask_forget(s),
        Action::Properties => {
            if s.place == Place::ThisPc && s.selected_drive.is_none() {
                s.error = String::from("Select a drive to see its properties.");
            } else {
                s.show_details = true;
            }
        }
    })
}

fn sort_by(s: &mut DriveState, column: Column) {
    s.sort = s.sort.clicked(column);
    browse::sort_entries(&mut s.entries, s.sort);
}

/// "Open": the selected tile or row - a drive's root, a folder, or a file
/// fetched and handed to the OS.
fn open_selected(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.place == Place::ThisPc {
        match s.selected_drive {
            Some(index) => {
                let id = s.slots[index].entry.id.clone();
                go(info, app, s, Place::folder(&id, ""), true);
            }
            None => s.error = String::from("Select a drive to open."),
        }
        return;
    }
    match s.selected.clone() {
        Some(key) => activate(info, app, s, &key),
        None => s.error = String::from("Select a folder or a file to open."),
    }
}

fn copy_selected(s: &mut DriveState) {
    match (s.current_drive(), s.selected_entry().cloned()) {
        (Some(index), Some(entry)) if !entry.is_folder => {
            s.clipboard = Some((s.slots[index].entry.id.clone(), entry.key));
            s.notice = format!("\"{}\" is ready to paste.", entry.name);
        }
        (_, Some(_)) => s.error = String::from("Copy works on files."),
        _ => s.error = String::from("Select a file to copy."),
    }
}

/// "Paste": the copied file into the open folder, ONE get and ONE put.
fn paste(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some((drive_id, key)) = s.clipboard.clone() else {
        s.error = String::from("Nothing to paste: copy a file first.");
        return;
    };
    let Some(source_index) = s.slot_index(&drive_id) else {
        s.error = String::from("The copied file's drive is gone.");
        return;
    };
    if s.current_drive().is_none() {
        s.error = String::from("Open a folder to paste into.");
        return;
    }
    let source = match s.slots[source_index].open() {
        Ok(drive) => drive,
        Err(e) => {
            s.error = e.to_string();
            return;
        }
    };
    let Some(target) = open_current(s) else {
        return;
    };
    let name = key::last_segment(&key).to_string();
    let mut target_key = format!("{}{name}", s.prefix());
    if target_key == key && Arc::ptr_eq(&source, &target) {
        target_key = format!("{}copy of {name}", s.prefix());
    }
    s.error.clear();
    s.notice = format!("Pasting \"{name}\"...");
    spawn(
        info,
        app,
        s,
        Job::Copy {
            source,
            key,
            target,
            target_key,
        },
    );
}

// ==== The address bar ====

fn address_bar(s: &DriveState, app: &RefAny) -> Dom {
    let drive_name = s.drive_name(&s.place);
    let labels: Vec<AzString> = browse::crumbs_of(&s.place, &drive_name)
        .into_iter()
        .map(|(label, _)| AzString::from(label))
        .collect();
    let drive_name = match &s.place {
        Place::ThisPc => None,
        Place::Folder { .. } => Some(drive_name.as_str()),
    };
    AddressBar::create(StringVec::from(labels))
        .with_path(AzString::from(browse::path_text(&s.place, drive_name)))
        .with_search(AzString::from(s.search.as_str()))
        .with_search_placeholder(AzString::from(format!("Search {}", s.place_name())))
        .with_can_go(
            s.history.can_go_back(),
            s.history.can_go_forward(),
            place_up(&s.place).is_some(),
        )
        .with_editing(s.editing_path)
        .with_on_event(app.clone(), on_address as AddressBarOnEventCallbackType)
        .dom()
}

/// A crumb menu's item: the folder it goes to.
struct MenuRef {
    app: RefAny,
    place: Place,
}

extern "C" fn on_menu_go(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, place)) = data
        .downcast_ref::<MenuRef>()
        .map(|r| (r.app.clone(), r.place.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        go(info, app, s, place, true);
    })
}

/// The folders under `place` the app knows: the tree's listing, or the open
/// folder's rows. `None` when it was never listed.
fn known_folders(s: &DriveState, place: &Place) -> Option<Vec<(String, Place)>> {
    match place {
        Place::ThisPc => Some(
            s.slots
                .iter()
                .map(|slot| (slot.entry.name.clone(), Place::folder(&slot.entry.id, "")))
                .collect(),
        ),
        Place::Folder { drive, prefix } => {
            if let Some(folders) = s.tree.loaded.get(&(drive.clone(), prefix.clone())) {
                return Some(
                    folders
                        .iter()
                        .map(|f| (key::last_segment(f).to_string(), Place::folder(drive, f)))
                        .collect(),
                );
            }
            (*place == s.place && !s.loading).then(|| {
                s.entries
                    .iter()
                    .filter(|e| e.is_folder)
                    .map(|e| (e.name.clone(), Place::folder(drive, &e.key)))
                    .collect()
            })
        }
    }
}

extern "C" fn on_address(mut data: RefAny, mut info: CallbackInfo, event: AddressBarEvent) -> Update {
    let index = event.index;
    let text = event.text.as_str().to_string();
    with_state(&mut data, &mut info, |info, app, s| match event.kind {
        AddressBarEventKind::Back => {
            let current = s.place.clone();
            if let Some(place) = s.history.back(current) {
                go(info, app, s, place, false);
            }
        }
        AddressBarEventKind::Forward => {
            let current = s.place.clone();
            if let Some(place) = s.history.forward(current) {
                go(info, app, s, place, false);
            }
        }
        AddressBarEventKind::Up => {
            if let Some(place) = place_up(&s.place) {
                go(info, app, s, place, true);
            }
        }
        AddressBarEventKind::Refresh => refresh(info, app, s),
        AddressBarEventKind::Crumb => {
            let drive_name = s.drive_name(&s.place);
            if let Some((_, place)) = browse::crumbs_of(&s.place, &drive_name).get(index) {
                let place = place.clone();
                go(info, app, s, place, true);
            }
        }
        AddressBarEventKind::CrumbMenu => {
            let drive_name = s.drive_name(&s.place);
            let Some((_, place)) = browse::crumbs_of(&s.place, &drive_name).get(index).cloned()
            else {
                return;
            };
            match known_folders(s, &place) {
                Some(folders) if !folders.is_empty() => {
                    let items: Vec<MenuItem> = folders
                        .into_iter()
                        .map(|(label, place)| {
                            MenuItem::String(
                                StringMenuItem::create(AzString::from(label)).with_callback(
                                    RefAny::new(MenuRef {
                                        app: app.clone(),
                                        place,
                                    }),
                                    on_menu_go,
                                ),
                            )
                        })
                        .collect();
                    info.open_menu_for_hit_node(Menu::create(items));
                }
                Some(_) => s.notice = String::from("This folder has no subfolders."),
                None => {
                    if let Place::Folder { drive, prefix } = place {
                        s.notice = String::from("Listing the folder...");
                        start_tree_listing(info, app, s, (drive, prefix));
                    }
                }
            }
        }
        AddressBarEventKind::EditStarted => s.editing_path = true,
        AddressBarEventKind::PathEntered => {
            s.editing_path = false;
            match browse::parse_path(&text, &s.drive_names()) {
                Some(place) => go(info, app, s, place, true),
                None => s.error = format!("There is no drive for \"{}\".", text.trim()),
            }
        }
        AddressBarEventKind::EditCancelled => s.editing_path = false,
        AddressBarEventKind::Search => {
            s.search = text.clone();
            s.selected = None;
        }
    })
}

// ==== The navigation tree ====

/// The tree and, in the tree's depth-first order, the place of every node.
fn tree_model(s: &DriveState) -> (TreeViewNode, Vec<Place>) {
    let mut places = vec![Place::ThisPc];
    let mut root = TreeViewNode::create(AzString::from(browse::THIS_PC))
        .with_icon(AzString::from("computer"))
        .with_expanded(s.tree.this_pc_open)
        .with_selected(s.place == Place::ThisPc);
    for slot in &s.slots {
        root = root.with_child(folder_node(
            s,
            &slot.entry.id,
            "",
            &slot.entry.name,
            slot.icon(),
            &mut places,
        ));
    }
    (root, places)
}

fn folder_node(
    s: &DriveState,
    drive: &str,
    prefix: &str,
    label: &str,
    icon: &str,
    places: &mut Vec<Place>,
) -> TreeViewNode {
    let place = Place::folder(drive, prefix);
    let node_key = (drive.to_string(), prefix.to_string());
    places.push(place.clone());
    let loaded = s.tree.loaded.get(&node_key);
    let mut node = TreeViewNode::create(AzString::from(label))
        .with_icon(AzString::from(icon))
        .with_expanded(s.tree.expanded.contains(&node_key))
        .with_selected(s.place == place)
        .with_unloaded_children(loaded.is_none());
    if let Some(folders) = loaded {
        for folder in folders {
            node = node.with_child(folder_node(
                s,
                drive,
                folder,
                key::last_segment(folder),
                "folder",
                places,
            ));
        }
    }
    node
}

fn navigation_pane(s: &DriveState, app: &RefAny, p: &Palette) -> Dom {
    let (root, _) = tree_model(s);
    Dom::create_div()
        .with_id("navigation")
        .with_css(format!(
            "display: flex; flex-direction: column; width: 230px; flex-shrink: 0; \
             border-right: 1px solid {}; overflow-y: auto; min-height: 0px;",
            p.line
        ))
        .with_child(
            TreeView::create(root)
                .with_on_node_click(app.clone(), on_tree_click as TreeViewOnNodeClickCallbackType)
                .with_on_node_toggle(
                    app.clone(),
                    on_tree_toggle as TreeViewOnNodeToggleCallbackType,
                )
                .dom()
                .with_css("flex-grow: 1;"),
        )
}

extern "C" fn on_tree_click(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let (_, places) = tree_model(s);
        if let Some(place) = places.get(index).cloned() {
            go(info, app, s, place, true);
        }
    })
}

extern "C" fn on_tree_toggle(
    mut data: RefAny,
    mut info: CallbackInfo,
    index: usize,
    expand: bool,
) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let (_, places) = tree_model(s);
        match places.get(index).cloned() {
            Some(Place::ThisPc) => s.tree.this_pc_open = expand,
            Some(Place::Folder { drive, prefix }) => {
                let node = (drive, prefix);
                if expand {
                    s.tree.expanded.insert(node.clone());
                    if !s.tree.loaded.contains_key(&node) {
                        start_tree_listing(info, app, s, node);
                    }
                } else {
                    s.tree.expanded.remove(&node);
                }
            }
            None => {}
        }
    })
}

// ==== The content: "This PC" or a folder ====

/// A drive tile or a row tile, for its click and double-click.
struct TileRef {
    app: RefAny,
    target: TileTarget,
}

enum TileTarget {
    Drive(usize),
    Entry(String),
}

fn drive_tile(s: &DriveState, app: &RefAny, index: usize) -> Dom {
    let slot = &s.slots[index];
    let mut tile = Tile::create(AzString::from(slot.entry.name.as_str()))
        .with_icon(AzString::from(slot.icon()))
        .with_selected(s.selected_drive == Some(index));
    match (s.disk.get(&slot.entry.id), s.root_counts.get(&slot.entry.id)) {
        (Some((total, free)), _) => tile = tile.with_capacity(TileCapacity::create(*total, *free)),
        (None, Some(count)) => {
            tile = tile.with_detail(AzString::from(format!(
                "{}, {count} items at the root",
                slot.kind()
            )))
        }
        (None, None) => tile = tile.with_detail(AzString::from(slot.kind())),
    }
    let target = || {
        RefAny::new(TileRef {
            app: app.clone(),
            target: TileTarget::Drive(index),
        })
    };
    tile.with_on_click(target(), on_tile_click as TileOnClickCallbackType)
        .with_on_double_click(target(), on_tile_open as TileOnClickCallbackType)
        .dom()
        .with_css("width: 260px; margin: 0px 8px 8px 0px;")
}

fn entry_tile(s: &DriveState, app: &RefAny, entry: &Entry) -> Dom {
    let detail = if entry.is_folder {
        String::from("Folder")
    } else {
        browse::format_size(entry.size)
    };
    let target = || {
        RefAny::new(TileRef {
            app: app.clone(),
            target: TileTarget::Entry(entry.key.clone()),
        })
    };
    Tile::create(AzString::from(entry.label()))
        .with_icon(AzString::from(if entry.is_folder {
            "folder"
        } else {
            "description"
        }))
        .with_detail(AzString::from(detail))
        .with_selected(s.selected.as_deref() == Some(entry.key.as_str()))
        .with_on_click(target(), on_tile_click as TileOnClickCallbackType)
        .with_on_double_click(target(), on_tile_open as TileOnClickCallbackType)
        .dom()
        .with_css("width: 220px; margin: 0px 8px 8px 0px;")
}

/// A wrapping row of tiles, `padding` around it.
fn tile_row(tiles: Vec<Dom>, padding: &str) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; flex-wrap: wrap; padding: {padding};"
        ))
        .with_children(DomVec::from(tiles))
}

extern "C" fn on_tile_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, target)) = data.downcast_ref::<TileRef>().map(|r| {
        (
            r.app.clone(),
            match &r.target {
                TileTarget::Drive(i) => TileTarget::Drive(*i),
                TileTarget::Entry(k) => TileTarget::Entry(k.clone()),
            },
        )
    }) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |_info, _app, s| match target {
        TileTarget::Drive(index) => s.selected_drive = Some(index),
        TileTarget::Entry(key) => s.selected = Some(key),
    })
}

extern "C" fn on_tile_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, target)) = data.downcast_ref::<TileRef>().map(|r| {
        (
            r.app.clone(),
            match &r.target {
                TileTarget::Drive(i) => TileTarget::Drive(*i),
                TileTarget::Entry(k) => TileTarget::Entry(k.clone()),
            },
        )
    }) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| match target {
        TileTarget::Drive(index) => {
            s.selected_drive = Some(index);
            let id = s.slots[index].entry.id.clone();
            go(info, app, s, Place::folder(&id, ""), true);
        }
        TileTarget::Entry(key) => activate(info, app, s, &key),
    })
}

extern "C" fn on_group_toggle(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    if let Some(closed) = s.groups_closed.get_mut(index) {
        *closed = !*closed;
    }
    Update::RefreshDom
}

/// "This PC": the drives in collapsible groups.
fn this_pc(s: &DriveState, app: &RefAny) -> Dom {
    let local: Vec<usize> = (0..s.slots.len()).filter(|i| s.slots[*i].is_local()).collect();
    let cloud: Vec<usize> = (0..s.slots.len()).filter(|i| !s.slots[*i].is_local()).collect();
    let group = |title: &str, indices: &[usize], closed: bool| {
        AccordionSection::create(
            AzString::from(title),
            tile_row(
                indices.iter().map(|i| drive_tile(s, app, *i)).collect(),
                "4px 0px 4px 16px",
            ),
        )
        .with_count(indices.len())
        .with_open(!closed)
    };
    Accordion::create_with_sections(vec![
        group("Local", &local, s.groups_closed[0]),
        group("Cloud / S3", &cloud, s.groups_closed[1]),
    ])
    .with_variant(AccordionVariant::Groups)
    .with_on_toggle(app.clone(), on_group_toggle as AccordionOnToggleCallbackType)
    .dom()
    .with_css("padding: 8px 16px;")
}

/// A row of the list, for its double-click.
struct RowRef {
    app: RefAny,
    key: String,
}

fn details_list(s: &DriveState, app: &RefAny, visible: &[&Entry]) -> Dom {
    let columns: Vec<AzString> = Column::ALL
        .iter()
        .map(|column| {
            let arrow = if s.sort.column == *column && s.sort.descending {
                " (descending)"
            } else {
                ""
            };
            AzString::from(format!("{}{arrow}", column.label()))
        })
        .collect();
    let rows: Vec<ListViewRow> = visible
        .iter()
        .map(|entry| {
            let name = Dom::create_div()
                .with_css("width: 100%;")
                .with_child(Dom::create_span_with_text(entry.label()))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::DoubleClick),
                    RefAny::new(RowRef {
                        app: app.clone(),
                        key: entry.key.clone(),
                    }),
                    on_row_double_click,
                );
            ListViewRow {
                cells: DomVec::from(vec![
                    name,
                    Dom::create_span_with_text(browse::format_size(entry.size)),
                    Dom::create_span_with_text(browse::format_modified(
                        entry.modified,
                        &chrono::Local,
                    )),
                ]),
                height: OptionPixelValueNoPercent::None,
            }
        })
        .collect();
    let selected_row = s
        .selected
        .as_deref()
        .and_then(|key| visible.iter().position(|e| e.key == key));
    ListView::create(StringVec::from(columns))
        .with_rows(ListViewRowVec::from(rows))
        .with_sorted_by(Some(s.sort.column.index()))
        .with_selected_row(selected_row)
        .with_on_row_click(app.clone(), on_row_click)
        .with_on_column_click(app.clone(), on_column_click)
        .dom()
}

fn content(s: &DriveState, app: &RefAny, p: &Palette) -> Dom {
    let mut area = Dom::create_div()
        .with_id("content")
        .with_css("flex-grow: 1; overflow-y: auto; min-height: 0px;");
    let status = if s.place == Place::ThisPc {
        area.add_child(this_pc(s, app));
        format!("{} drives", s.slots.len())
    } else {
        let visible = s.visible_entries();
        match s.layout {
            ViewLayout::Tiles => area.add_child(tile_row(
                visible.iter().map(|e| entry_tile(s, app, e)).collect(),
                "8px 0px 8px 16px",
            )),
            ViewLayout::Details => area.add_child(details_list(s, app, &visible)),
        }
        if s.loading {
            String::from("Loading...")
        } else if s.entries.is_empty() && s.error.is_empty() {
            String::from("This folder is empty.")
        } else {
            let folders = visible.iter().filter(|e| e.is_folder).count();
            let files = visible.len() - folders;
            let hidden = s.entries.len() - visible.len();
            let more = if s.next.is_some() {
                " (more in the folder)"
            } else {
                ""
            };
            let filtered = if hidden > 0 {
                format!(", {hidden} hidden by the search")
            } else {
                String::new()
            };
            format!("{folders} folders, {files} files{more}{filtered}")
        }
    };
    let mut footer = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; \
             font-size: 12px; color: {};",
            p.secondary
        ))
        .with_child(Dom::create_span_with_text(status).with_id("content-status"));
    if s.next.is_some() && !s.loading {
        footer.add_child(
            Button::create("Load more")
                .with_on_click(app.clone(), on_load_more)
                .dom()
                .with_css("margin-left: 12px;"),
        );
    }
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; background: {};",
            p.content
        ))
        .with_child(area)
        .with_child(footer)
}

// ==== The details pane ====

fn details_pane(s: &DriveState) -> Dom {
    let pane = match (&s.place, s.selected_drive, s.selected_entry()) {
        (Place::ThisPc, Some(index), _) if index < s.slots.len() => {
            let slot = &s.slots[index];
            let mut pane = DetailsPane::create(AzString::from(slot.entry.name.as_str()))
                .with_icon(AzString::from(slot.icon()))
                .with_subtitle(AzString::from(slot.kind()));
            match &slot.entry.location {
                DriveLocation::Local { root } => {
                    if let Some((total, free)) = s.disk.get(&slot.entry.id) {
                        pane = pane
                            .with_property(
                                AzString::from("Space used"),
                                AzString::from(browse::format_size(Some(
                                    total.saturating_sub(*free),
                                ))),
                            )
                            .with_property(
                                AzString::from("Free space"),
                                AzString::from(browse::format_size(Some(*free))),
                            )
                            .with_property(
                                AzString::from("Total size"),
                                AzString::from(browse::format_size(Some(*total))),
                            );
                    }
                    pane = pane.with_property(AzString::from("Path"), AzString::from(root.as_str()));
                }
                DriveLocation::S3 {
                    endpoint,
                    region,
                    bucket,
                    ..
                } => {
                    pane = pane
                        .with_property(AzString::from("Bucket"), AzString::from(bucket.as_str()))
                        .with_property(
                            AzString::from("Endpoint"),
                            AzString::from(endpoint.as_str()),
                        )
                        .with_property(AzString::from("Region"), AzString::from(region.as_str()));
                    if let Some(count) = s.root_counts.get(&slot.entry.id) {
                        pane = pane.with_property(
                            AzString::from("Items at the root"),
                            AzString::from(count.to_string()),
                        );
                    }
                }
            }
            pane
        }
        (Place::ThisPc, _, _) => DetailsPane::create(AzString::from(browse::THIS_PC))
            .with_icon(AzString::from("computer"))
            .with_subtitle(AzString::from(format!("{} drives", s.slots.len()))),
        (Place::Folder { .. }, _, Some(entry)) => {
            let mut pane = DetailsPane::create(AzString::from(entry.name.as_str()))
                .with_icon(AzString::from(if entry.is_folder {
                    "folder"
                } else {
                    "description"
                }))
                .with_subtitle(AzString::from(if entry.is_folder {
                    "Folder"
                } else {
                    "File"
                }));
            if !entry.is_folder {
                pane = pane
                    .with_property(
                        AzString::from("Size"),
                        AzString::from(browse::format_size(entry.size)),
                    )
                    .with_property(
                        AzString::from("Modified"),
                        AzString::from(browse::format_modified(entry.modified, &chrono::Local)),
                    );
            }
            pane.with_property(AzString::from("Key"), AzString::from(entry.key.as_str()))
        }
        (Place::Folder { .. }, _, None) => DetailsPane::create(AzString::from(s.place_name()))
            .with_icon(AzString::from("folder"))
            .with_subtitle(AzString::from("Folder"))
            .with_property(
                AzString::from("Items"),
                AzString::from(format!("{}", s.entries.len())),
            ),
    };
    pane.dom().with_id("details").with_css("flex-shrink: 0;")
}

fn footer_text(s: &DriveState) -> String {
    let busy = if s.running > 0 {
        format!(" - {} running", s.running)
    } else {
        String::new()
    };
    match s.current_drive() {
        None => format!("{}: {} drives{busy}", browse::THIS_PC, s.slots.len()),
        Some(index) => {
            let slot = &s.slots[index];
            let place = match &slot.entry.location {
                DriveLocation::Local { root } => root.clone(),
                DriveLocation::S3 {
                    endpoint, bucket, ..
                } => format!("s3://{bucket} at {endpoint}"),
            };
            format!(
                "{}: {place}/{}{busy}",
                slot.entry.name,
                s.prefix().trim_end_matches('/')
            )
        }
    }
}

// ==== Popups ====

/// The forms' fields, for their text callbacks.
#[derive(Clone, Copy)]
enum Field {
    Name,
    Endpoint,
    Region,
    Bucket,
    AccessKey,
    SecretKey,
    RenameName,
    NewFolderName,
}

struct FieldRef {
    app: RefAny,
    field: Field,
}

fn button(
    label: &str,
    app: &RefAny,
    on_click: extern "C" fn(RefAny, CallbackInfo) -> Update,
) -> Dom {
    Button::create(label)
        .with_on_click(app.clone(), on_click)
        .dom()
        .with_css("margin-right: 6px;")
}

/// A dialog's title and content.
fn popup_parts(popup: &Popup, s: &DriveState, app: &RefAny, p: &Palette) -> (String, Dom) {
    let label = |text: &str| {
        Dom::create_span_with_text(text).with_css(format!(
            "font-size: 12px; color: {}; margin-top: 10px; margin-bottom: 4px;",
            p.secondary
        ))
    };
    let buttons = |children: Vec<Dom>| {
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 16px;",
            )
            .with_children(DomVec::from(children))
    };
    let input = |value: &str, placeholder: &str, field: Field, id: &str, secret: bool| {
        let base = if secret {
            TextInput::create_password()
        } else {
            TextInput::create()
        };
        base.with_text(value)
            .with_placeholder(placeholder)
            .with_on_text_input(
                RefAny::new(FieldRef {
                    app: app.clone(),
                    field,
                }),
                on_form_text,
            )
            .dom()
            .with_id(id)
    };
    match popup {
        Popup::AddDrive {
            form,
            editing,
            testing,
            tested,
            error,
            ..
        } => {
            let mut body = Dom::create_div()
                .with_css("display: flex; flex-direction: column; min-width: 320px;")
                .with_child(label("Name"))
                .with_child(input(
                    form.name.as_str(),
                    "S3 Drive",
                    Field::Name,
                    "add-name",
                    false,
                ))
                .with_child(label("Endpoint"))
                .with_child(input(
                    form.endpoint.as_str(),
                    "https://s3.eu-central-1.amazonaws.com",
                    Field::Endpoint,
                    "add-endpoint",
                    false,
                ))
                .with_child(label("Region"))
                .with_child(input(
                    form.region.as_str(),
                    "us-east-1 (R2: auto)",
                    Field::Region,
                    "add-region",
                    false,
                ))
                .with_child(label("Bucket"))
                .with_child(input(
                    form.bucket.as_str(),
                    "my-bucket",
                    Field::Bucket,
                    "add-bucket",
                    false,
                ))
                .with_child(label("Access key"))
                .with_child(input(
                    form.access_key.as_str(),
                    "",
                    Field::AccessKey,
                    "add-access-key",
                    false,
                ))
                .with_child(label("Secret key"))
                .with_child(input(
                    form.secret_key.as_str(),
                    "",
                    Field::SecretKey,
                    "add-secret-key",
                    true,
                ))
                .with_child(
                    Dom::create_div()
                        .with_css(
                            "display: flex; flex-direction: row; align-items: center; \
                             margin-top: 12px;",
                        )
                        .with_child(
                            CheckBox::create(form.path_style)
                                .with_on_toggle(app.clone(), on_path_style)
                                .dom(),
                        )
                        .with_child(
                            Dom::create_span_with_text("Path-style URLs (MinIO, local servers)")
                                .with_css("margin-left: 8px; cursor: pointer;")
                                .with_callback(
                                    EventFilter::Hover(HoverEventFilter::Click),
                                    app.clone(),
                                    on_path_style_label,
                                ),
                        ),
                );
            let status = if *testing {
                Some((String::from("Testing the connection..."), p.secondary))
            } else {
                match tested {
                    Some(Ok(text)) => Some((text.clone(), p.ok)),
                    Some(Err(text)) => Some((format!("The connection failed: {text}"), p.error)),
                    None => None,
                }
            };
            if let Some((text, colour)) = status {
                body.add_child(
                    Dom::create_span_with_text(text)
                        .with_id("add-status")
                        .with_css(format!(
                            "font-size: 13px; margin-top: 12px; color: {colour};"
                        )),
                );
            }
            if !error.is_empty() {
                body.add_child(Dom::create_span_with_text(error.as_str()).with_css(format!(
                    "font-size: 13px; margin-top: 12px; color: {};",
                    p.error
                )));
            }
            body.add_child(buttons(vec![
                button("Test connection", app, on_test_connection),
                button("Cancel", app, on_cancel_popup),
                Button::with_type("Save drive", ButtonType::Primary)
                    .with_on_click(app.clone(), on_save_drive)
                    .dom(),
            ]));
            let title = if editing.is_some() {
                "Enter the drive's keys again"
            } else {
                "Add an S3 drive"
            };
            (title.to_string(), body)
        }
        Popup::ConfirmDelete { name, .. } => (
            format!("Delete \"{name}\"?"),
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; min-width: 300px;")
                .with_child(Dom::create_span_with_text(format!(
                    "\"{name}\" is deleted from \"{}\". This cannot be undone.",
                    s.drive_name(&s.place)
                )))
                .with_child(buttons(vec![
                    button("Cancel", app, on_cancel_popup),
                    Button::with_type("Delete file", ButtonType::Danger)
                        .with_on_click(app.clone(), on_confirm_delete)
                        .dom(),
                ])),
        ),
        Popup::ConfirmForget { drive_id } => {
            let name = s
                .slot_index(drive_id)
                .map(|i| s.slots[i].entry.name.clone())
                .unwrap_or_default();
            (
                format!("Remove the drive \"{name}\"?"),
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; min-width: 300px;")
                    .with_child(Dom::create_span_with_text(
                        "AzDrive forgets the drive and removes its keys from the keyring. Its \
                         files stay where they are.",
                    ))
                    .with_child(buttons(vec![
                        button("Cancel", app, on_cancel_popup),
                        Button::with_type("Remove", ButtonType::Danger)
                            .with_on_click(app.clone(), on_confirm_forget)
                            .dom(),
                    ])),
            )
        }
        Popup::Rename { key, name } => (
            format!("Rename \"{}\"", key::last_segment(key)),
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; min-width: 300px;")
                .with_child(label("New name"))
                .with_child(input(
                    name.as_str(),
                    "",
                    Field::RenameName,
                    "rename-name",
                    false,
                ))
                .with_child(buttons(vec![
                    button("Cancel", app, on_cancel_popup),
                    Button::with_type("Rename", ButtonType::Primary)
                        .with_on_click(app.clone(), on_confirm_rename)
                        .dom(),
                ])),
        ),
        Popup::NewFolder { name } => (
            String::from("New folder"),
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; min-width: 300px;")
                .with_child(label("Name"))
                .with_child(input(
                    name.as_str(),
                    "New folder",
                    Field::NewFolderName,
                    "new-folder-name",
                    false,
                ))
                .with_child(buttons(vec![
                    button("Cancel", app, on_cancel_popup),
                    Button::with_type("Create", ButtonType::Primary)
                        .with_on_click(app.clone(), on_confirm_new_folder)
                        .dom(),
                ])),
        ),
    }
}

/// A dialog as a sheet right of the content (`AZDRIVE_DIALOGS=inline`).
fn inline_sheet(popup: &Popup, s: &DriveState, app: &RefAny, p: &Palette) -> Dom {
    let (title, panel) = popup_parts(popup, s, app, p);
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; width: 360px; flex-shrink: 0; padding: 16px; \
             background: {}; border-left: 1px solid {}; overflow-y: auto;",
            p.toolbar, p.line
        ))
        .with_child(
            Dom::create_span_with_text(title)
                .with_css("font-size: 17px; font-weight: bold; margin-bottom: 6px;"),
        )
        .with_child(panel)
}

// ==== Callbacks: navigation and rows ====

/// Runs `f` on the state with the app handle, and asks for a new DOM.
fn with_state(
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

extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        println!("AZDRIVE_PLACE {}", s.place_line());
    })
}

extern "C" fn on_load_more(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if !s.loading {
            start_listing(info, app, s, true);
        }
    })
}

extern "C" fn on_row_click(
    mut data: RefAny,
    _info: CallbackInfo,
    _state: ListViewState,
    row: usize,
) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let key = s.visible_entries().get(row).map(|e| e.key.clone());
    s.selected = key;
    Update::RefreshDom
}

extern "C" fn on_column_click(
    mut data: RefAny,
    _info: CallbackInfo,
    _state: ListViewState,
    column: usize,
) -> Update {
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let Some(column) = Column::from_index(column) else {
        return Update::DoNothing;
    };
    sort_by(&mut *guard, column);
    Update::RefreshDom
}

/// A folder opens; a file is fetched and opened with the OS's app.
fn activate(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, key: &str) {
    let Some(entry) = s.entries.iter().find(|e| e.key == key).cloned() else {
        return;
    };
    s.selected = Some(entry.key.clone());
    if entry.is_folder {
        if let Place::Folder { drive, .. } = s.place.clone() {
            go(info, app, s, Place::folder(&drive, &entry.key), true);
        }
    } else {
        download(info, app, s, &entry, true);
    }
}

extern "C" fn on_row_double_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.key.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        activate(info, app, s, &key)
    })
}

// ==== Callbacks: files ====

fn download(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, entry: &Entry, open: bool) {
    let Some(index) = s.current_drive() else {
        return;
    };
    let Some(drive) = open_current(s) else {
        return;
    };
    let folder = if open {
        s.open_dir.join(&s.slots[index].entry.id)
    } else {
        s.downloads.clone()
    };
    s.error.clear();
    s.notice = format!("Fetching \"{}\"...", entry.name);
    spawn(
        info,
        app,
        s,
        Job::Download {
            drive,
            key: entry.key.clone(),
            size: entry.size,
            folder,
            open,
        },
    );
}

fn ask_delete(s: &mut DriveState) {
    match s.selected_entry().cloned() {
        Some(entry) if !entry.is_folder => {
            s.popups_opened += 1;
            s.popup = Some(Popup::ConfirmDelete {
                key: entry.key,
                name: entry.name,
            });
        }
        Some(_) => {
            s.error = String::from("Delete works on files; a folder goes away with its last file.")
        }
        None => s.error = String::from("Select a file to delete."),
    }
}

extern "C" fn on_confirm_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::ConfirmDelete { key, .. }) = s.popup.take() else {
            return;
        };
        if let Some(drive) = open_current(s) {
            spawn(info, app, s, Job::Delete { drive, key });
        }
    })
}

fn ask_rename(s: &mut DriveState) {
    match s.selected_entry().cloned() {
        Some(entry) if !entry.is_folder => {
            s.popups_opened += 1;
            s.popup = Some(Popup::Rename {
                key: entry.key,
                name: entry.name,
            });
        }
        Some(_) => s.error = String::from("Rename works on files."),
        None => s.error = String::from("Select a file to rename."),
    }
}

extern "C" fn on_confirm_rename(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Rename { key, name }) = s.popup.take() else {
            return;
        };
        let name = name.trim().to_string();
        let Some(to) = browse::upload_key(s.prefix(), &name) else {
            s.error = format!("\"{name}\" cannot be a file name on a drive.");
            return;
        };
        if to == key {
            return;
        }
        if let Some(drive) = open_current(s) {
            s.notice = format!("Renaming to \"{name}\"...");
            spawn(info, app, s, Job::Rename { drive, from: key, to });
        }
    })
}

fn ask_new_folder(s: &mut DriveState) {
    if s.current_drive().is_none() {
        s.error = String::from("Open a drive to create a folder in.");
        return;
    }
    s.popups_opened += 1;
    s.popup = Some(Popup::NewFolder {
        name: String::new(),
    });
}

extern "C" fn on_confirm_new_folder(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::NewFolder { name }) = s.popup.take() else {
            return;
        };
        let name = name.trim().to_string();
        let Some(key) = browse::upload_key(s.prefix(), &name) else {
            s.error = format!("\"{name}\" cannot be a folder name on a drive.");
            return;
        };
        let Some(index) = s.current_drive() else {
            return;
        };
        let local_path = match &s.slots[index].entry.location {
            DriveLocation::Local { root } => Some(Path::new(root).join(&key)),
            DriveLocation::S3 { .. } => None,
        };
        if let Some(drive) = open_current(s) {
            s.notice = format!("Creating \"{name}\"...");
            spawn(
                info,
                app,
                s,
                Job::NewFolder {
                    drive,
                    local_path,
                    key: format!("{key}/"),
                },
            );
        }
    })
}

extern "C" fn on_upload_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let source = PathBuf::from(path.as_string().as_str());
    with_state(&mut data, &mut info, |info, app, s| {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(key) = browse::upload_key(s.prefix(), &name) else {
            s.error = format!("\"{name}\" cannot be a file name on a drive.");
            return;
        };
        if s.current_drive().is_none() {
            s.error = String::from("Open a folder to upload into.");
            return;
        }
        if let Some(drive) = open_current(s) {
            s.error.clear();
            s.notice = format!("Uploading \"{name}\"...");
            spawn(info, app, s, Job::Upload { drive, source, key });
        }
    })
}

// ==== Callbacks: drives ====

/// Opens the "Add drive" form; with `editing`, prefilled from that drive to enter
/// its keys again.
fn open_drive_form(s: &mut DriveState, editing: Option<usize>) {
    let mut form = DriveForm::default();
    let mut editing_id = None;
    if let Some(slot) = editing.and_then(|i| s.slots.get(i)) {
        if let Some(config) = slot.entry.s3_config() {
            form.name = slot.entry.name.clone();
            form.endpoint = config.endpoint;
            form.region = config.region;
            form.bucket = config.bucket;
            form.path_style = config.path_style;
            editing_id = Some(slot.entry.id.clone());
        }
    }
    s.popups_opened += 1;
    s.popup = Some(Popup::AddDrive {
        form,
        editing: editing_id,
        serial: s.popups_opened,
        testing: false,
        tested: None,
        error: String::new(),
    });
}

extern "C" fn on_cancel_popup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.popup = None)
}

/// The dialog window was closed (x, Escape).
extern "C" fn on_dialog_closed(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: DialogState,
) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.popup = None)
}

extern "C" fn on_form_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, field)) = data
        .downcast_ref::<FieldRef>()
        .map(|r| (r.app.clone(), r.field))
    else {
        return keep;
    };
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    match (s.popup.as_mut(), field) {
        (Some(Popup::AddDrive { form, error, .. }), field) => {
            match field {
                Field::Name => form.name = text,
                Field::Endpoint => form.endpoint = text,
                Field::Region => form.region = text,
                Field::Bucket => form.bucket = text,
                Field::AccessKey => form.access_key = text,
                Field::SecretKey => form.secret_key = text,
                Field::RenameName | Field::NewFolderName => {}
            }
            error.clear();
        }
        (Some(Popup::Rename { name, .. }), Field::RenameName) => *name = text,
        (Some(Popup::NewFolder { name }), Field::NewFolderName) => *name = text,
        _ => {}
    }
    keep
}

fn set_path_style(data: &mut RefAny, checked: Option<bool>) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    if let Some(Popup::AddDrive { form, .. }) = s.popup.as_mut() {
        form.path_style = checked.unwrap_or(!form.path_style);
    }
    Update::RefreshDom
}

extern "C" fn on_path_style(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    set_path_style(&mut data, Some(state.checked))
}

extern "C" fn on_path_style_label(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_path_style(&mut data, None)
}

extern "C" fn on_test_connection(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::AddDrive {
            form,
            serial,
            testing,
            tested,
            error,
            ..
        }) = s.popup.as_mut()
        else {
            return;
        };
        if *testing {
            return;
        }
        match form.check() {
            Ok((config, credentials)) => {
                *testing = true;
                *tested = None;
                error.clear();
                let serial = *serial;
                spawn(
                    info,
                    app,
                    s,
                    Job::Test {
                        serial,
                        config,
                        credentials,
                    },
                );
            }
            Err(problem) => *error = problem,
        }
    })
}

extern "C" fn on_save_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::AddDrive {
            form,
            editing,
            error,
            ..
        }) = s.popup.as_mut()
        else {
            return;
        };
        let id = editing
            .clone()
            .unwrap_or_else(|| config::new_drive_id(form.name.trim()));
        let (entry, credentials) = match form.entry(&id).and_then(|entry| {
            let (_, credentials) = form.check()?;
            Ok((entry, credentials))
        }) {
            Ok(parts) => parts,
            Err(problem) => {
                *error = problem;
                return;
            }
        };
        let Some(file_path) = s.drives_file.clone() else {
            *error = String::from("There is no configuration folder to save the drive in.");
            return;
        };
        let saved = DrivesFile::load(&file_path).and_then(|mut file| {
            file.add(entry.clone());
            file.save(&file_path)
        });
        if let Err(e) = saved {
            *error = format!("The drive could not be saved: {e}");
            return;
        }
        let secret = credentials.to_keyring_secret();
        let index = match s.slot_index(&id) {
            Some(index) => {
                s.slots[index] = Slot::new(entry);
                index
            }
            None => {
                s.slots.push(Slot::new(entry));
                s.slots.len() - 1
            }
        };
        s.slots[index].credentials = Some(credentials);
        s.popup = None;
        s.selected_drive = Some(index);
        println!("AZDRIVE_ADDED {id}");
        keyring(
            info,
            s,
            KeyringOp::Store {
                drive_id: id.clone(),
            },
            KeyringCall::Store(config::keyring_key(&id), secret),
        );
        go(info, app, s, Place::folder(&id, ""), true);
    })
}

fn ask_forget(s: &mut DriveState) {
    let index = match s.place {
        Place::ThisPc => s.selected_drive,
        _ => s.current_drive(),
    };
    match index {
        Some(index) if s.slots[index].entry.id != HOME_ID => {
            if s.popup.is_none() {
                let drive_id = s.slots[index].entry.id.clone();
                s.popups_opened += 1;
                s.popup = Some(Popup::ConfirmForget { drive_id });
            }
        }
        Some(_) => s.error = String::from("The Home drive stays."),
        None => s.error = String::from("Select a drive to remove."),
    }
}

extern "C" fn on_confirm_forget(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::ConfirmForget { drive_id }) = s.popup.take() else {
            return;
        };
        if let Some(file_path) = s.drives_file.clone() {
            let saved = DrivesFile::load(&file_path).and_then(|mut file| {
                file.remove(&drive_id);
                file.save(&file_path)
            });
            if let Err(e) = saved {
                s.error = format!("The drives file could not be updated: {e}");
                return;
            }
        }
        if let Some(index) = s.slot_index(&drive_id) {
            let name = s.slots[index].entry.name.clone();
            s.slots.remove(index);
            s.notice = format!("\"{name}\" was removed from AzDrive.");
        }
        s.selected_drive = None;
        s.tree.expanded.retain(|node| node.0 != drive_id);
        s.tree.loaded.retain(|node, _| node.0 != drive_id);
        s.root_counts.remove(&drive_id);
        keyring(
            info,
            s,
            KeyringOp::Forget,
            KeyringCall::Delete(config::keyring_key(&drive_id)),
        );
        s.history.clear();
        go(info, app, s, Place::ThisPc, false);
    })
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

pub fn start() {
    let home = env_path(HOME_VAR)
        .or_else(|| path_of(FilePath::get_home_dir().into_option()))
        .unwrap_or_else(|| PathBuf::from("."));
    let downloads = env_path(DOWNLOADS_VAR)
        .or_else(|| path_of(FilePath::get_download_dir().into_option()))
        .unwrap_or_else(|| home.join("Downloads"));
    let drives_file = config::drives_file(
        std::env::var(config::DRIVES_VAR).ok().as_deref(),
        path_of(FilePath::get_config_dir().into_option()),
    );
    let inline_dialogs = std::env::var(DIALOGS_VAR).is_ok_and(|v| v.trim() == "inline");

    let mut slots = vec![Slot::new(DriveEntry {
        id: HOME_ID.to_string(),
        name: String::from("Home"),
        location: DriveLocation::Local {
            root: home.to_string_lossy().into_owned(),
        },
    })];
    let mut error = String::new();
    match drives_file.as_deref().map(DrivesFile::load) {
        Some(Ok(file)) => slots.extend(
            file.drives
                .into_iter()
                .filter(|d| d.id != HOME_ID)
                .map(Slot::new),
        ),
        Some(Err(e)) => error = format!("The drives file could not be read: {e}"),
        None => {}
    }
    // The Home volume's size and free space: one statfs, from the OS.
    let mut disk = HashMap::new();
    if let Some(space) = FilePath::create(home.to_string_lossy().as_ref())
        .disk_space()
        .into_option()
    {
        disk.insert(HOME_ID.to_string(), (space.total, space.free));
    }
    eprintln!(
        "[azdrive] {} drive(s); drives file {}; downloads to {}",
        slots.len(),
        drives_file
            .as_deref()
            .map_or_else(|| String::from("none"), |p| p.display().to_string()),
        downloads.display()
    );

    let state = DriveState {
        slots,
        place: Place::ThisPc,
        history: History::default(),
        entries: Vec::new(),
        next: None,
        loading: false,
        list_serial: 0,
        selected: None,
        selected_drive: None,
        sort: Sort::default(),
        tree: TreeState {
            this_pc_open: true,
            ..TreeState::default()
        },
        groups_closed: [false, false],
        ribbon_tab: 0,
        layout: ViewLayout::Tiles,
        show_navigation: true,
        show_details: true,
        editing_path: false,
        search: String::new(),
        clipboard: None,
        root_counts: HashMap::new(),
        disk,
        notice: String::new(),
        error,
        popup: None,
        popups_opened: 0,
        keyring_waiting: None,
        keyring_queue: VecDeque::new(),
        drives_file,
        downloads,
        open_dir: std::env::temp_dir().join("AzDrive-open"),
        inline_dialogs,
        running: 0,
    };
    let app = App::create(RefAny::new(state), AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1100.0, 720.0);
    window.window_state.title = AzString::from("AzDrive");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}
