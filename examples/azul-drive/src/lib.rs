//! AzDrive: an Explorer-like file browser on the public azul API.
//!
//! A sidebar of drives: "Home" (the user's home folder, a `LocalDrive`) and the
//! drives the user added ("S3 Drive": AWS S3, Cloudflare R2, MinIO). "Add
//! drive..." asks for an S3-compatible bucket (name, endpoint, region, bucket,
//! access key, secret key, path style); "Test connection" makes ONE listing call
//! and shows the service's error text when it fails. The drives list is
//! `<config dir>/azul-storage/drives.json` WITHOUT secrets (shared with AzMail);
//! the keys go to the OS keyring (`CallbackInfo::keyring_store`), read back with
//! `keyring_get` when the drive is opened.
//!
//! The main view is a breadcrumb and a list (name, size, modified) sorted by a
//! click on a column header; a double-click opens a folder, Back and Up walk
//! back. Listings are paged (200 entries, "Load more"): browsing a bucket
//! fetches listings, never objects. "Download" fetches ONE object into the
//! Downloads folder, "Open" into a temporary folder and hands it to the OS's
//! default app (`Url::open` on a `file://` URL). "Upload..." puts a picked file
//! into the open folder, "Delete" removes a file after a confirmation.
//!
//! Every storage call is blocking (`azul_storage::Drive`) and runs on an azul
//! `Thread`; the answer comes back through the thread's write-back. No callback
//! waits on the network.
//!
//! Environment:
//! - `AZDRIVE_HOME`: the folder the Home drive shows (default: the user's home).
//! - `AZUL_DRIVES`: the drives file (default: `<config dir>/azul-storage/drives.json`).
//! - `AZDRIVE_DOWNLOADS`: where "Download" saves (default: the user's Downloads folder).
//! - `AZDRIVE_DIALOGS=inline`: show "Add drive" and the confirmations as a sheet inside the
//!   window instead of a modal dialog window (scripts: the debug server drives the main window).
//!
//! On stdout, for scripts: `AZDRIVE_LISTED <drive id> <prefix or /> <entries>`, `AZDRIVE_TESTED
//! ok|error`, `AZDRIVE_ADDED <drive id>`, `AZDRIVE_DOWNLOADED <path>`, `AZDRIVE_UPLOADED <key>`,
//! `AZDRIVE_DELETED <key>`. Keys and secrets are never printed.

pub mod browse;

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
};

use azul::{
    css::{DarkLightMode, WindowDecorations},
    dialog::{FileDialog, FileOpenResult},
    error::KeyringResult,
    file::FilePath,
    option::{OptionFileTypeList, OptionPixelValueNoPercent},
    prelude::*,
    str::String as AzString,
    url::Url,
    vec::{ListViewRowVec, StringVec},
    widgets::{
        Breadcrumb, BreadcrumbState, ButtonType, CheckBoxState, Dialog, DialogState, ListView,
        ListViewRow, ListViewState, OnTextInputReturn, TextInputState, TextInputValid, Titlebar,
    },
};
use azul_storage::{
    azul_transport::AzulTransport,
    config::{self, DriveEntry, DriveLocation, DrivesFile},
    transfer, Credentials, Drive, DriveError, ListPage, ListRequest, S3Config, S3Drive,
};
use browse::{Column, DriveForm, Entry, History, Sort};

const HOME_VAR: &str = "AZDRIVE_HOME";
const DOWNLOADS_VAR: &str = "AZDRIVE_DOWNLOADS";
const DIALOGS_VAR: &str = "AZDRIVE_DIALOGS";
const USER_AGENT: &str = "AzDrive/0.1";
/// Entries per listing page.
const PAGE_SIZE: u32 = 200;
/// The Home drive's id (never in the drives file).
const HOME_ID: &str = "home";

// ==== Colours ====

/// The window's colours in one mode. The widgets (buttons, list, inputs) follow
/// the app theme (flat / flora) and the mode themselves.
struct Palette {
    dark: bool,
    toolbar: &'static str,
    toolbar_rgb: (u8, u8, u8),
    sidebar: &'static str,
    content: &'static str,
    text: &'static str,
    text_rgb: (u8, u8, u8),
    secondary: &'static str,
    line: &'static str,
    selected: &'static str,
    notice_bg: &'static str,
    notice_text: &'static str,
    error: &'static str,
    ok: &'static str,
}

const LIGHT: Palette = Palette {
    dark: false,
    toolbar: "#f6f7f9",
    toolbar_rgb: (0xf6, 0xf7, 0xf9),
    sidebar: "#eef0f3",
    content: "#ffffff",
    text: "#1d2330",
    text_rgb: (0x1d, 0x23, 0x30),
    secondary: "#5d6677",
    line: "#d9dce3",
    selected: "#dbe7ff",
    notice_bg: "#e6eefc",
    notice_text: "#2c4a7a",
    error: "#b3261e",
    ok: "#1f7a3a",
};

const DARK: Palette = Palette {
    dark: true,
    toolbar: "#2b2d31",
    toolbar_rgb: (0x2b, 0x2d, 0x31),
    sidebar: "#232428",
    content: "#1e1f22",
    text: "#e6e7ea",
    text_rgb: (0xe6, 0xe7, 0xea),
    secondary: "#a0a4ad",
    line: "#3a3c42",
    selected: "#2f4a73",
    notice_bg: "#1f3350",
    notice_text: "#cfe0ff",
    error: "#ff8a80",
    ok: "#7ddc95",
};

// ==== State ====

/// A drive of the sidebar.
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

struct DriveState {
    slots: Vec<Slot>,
    current: usize,
    /// The open folder of the current drive (`""` = its root).
    prefix: String,
    history: History,
    entries: Vec<Entry>,
    /// Where the next page of the listing starts.
    next: Option<String>,
    loading: bool,
    /// The listing the rows belong to; an answer for an older one is dropped.
    list_serial: u64,
    selected: Option<String>,
    sort: Sort,
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
    fn slot(&self) -> &Slot {
        &self.slots[self.current.min(self.slots.len() - 1)]
    }

    fn slot_index(&self, drive_id: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.entry.id == drive_id)
    }

    fn selected_entry(&self) -> Option<&Entry> {
        let key = self.selected.as_deref()?;
        self.entries.iter().find(|e| e.key == key)
    }
}

// ==== Worker threads ====

/// One blocking storage task.
enum Job {
    List {
        serial: u64,
        drive: Arc<dyn Drive>,
        request: ListRequest,
        append: bool,
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
}

/// What a job answers, on the UI thread.
enum Outcome {
    Listed {
        serial: u64,
        append: bool,
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
            serial,
            drive,
            request,
            append,
        } => Outcome::Listed {
            serial,
            append,
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

/// Lists the open folder from its start (`append == false`) or its next page.
fn start_listing(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, append: bool) {
    let drive = match s.slots[s.current].open() {
        Ok(drive) => drive,
        Err(e) => {
            s.loading = false;
            s.error = e.to_string();
            return;
        }
    };
    let mut request = ListRequest::folder(&s.prefix).with_max_keys(PAGE_SIZE);
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
            serial,
            drive,
            request,
            append,
        },
    );
}

/// Opens the folder `prefix` of the current drive; `remember` puts the folder
/// being left on the Back list.
fn show_folder(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    prefix: String,
    remember: bool,
) {
    if remember && prefix != s.prefix {
        let leaving = s.prefix.clone();
        s.history.visit(&leaving);
    }
    s.prefix = prefix;
    s.error.clear();
    start_listing(info, app, s, false);
}

/// Shows the drive at `index`: its root, after reading its keys from the keyring
/// when it needs them.
fn select_drive(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, index: usize) {
    if index >= s.slots.len() {
        return;
    }
    s.current = index;
    s.history.clear();
    s.prefix.clear();
    s.entries.clear();
    s.next = None;
    s.selected = None;
    s.error.clear();
    s.list_serial += 1;
    if s.slots[index].locked() {
        let drive_id = s.slots[index].entry.id.clone();
        s.notice = format!(
            "Reading the keys of \"{}\" from the keyring...",
            s.slots[index].entry.name
        );
        s.loading = true;
        keyring(
            info,
            s,
            KeyringOp::Unlock {
                drive_id: drive_id.clone(),
            },
            KeyringCall::Get(config::keyring_key(&drive_id)),
        );
        return;
    }
    s.notice.clear();
    start_listing(info, app, s, false);
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
            let is_current = index == s.current;
            match &result {
                KeyringResult::Retrieved(secret) => {
                    match Credentials::from_keyring_secret(secret.as_str()) {
                        Ok(credentials) => {
                            s.slots[index].credentials = Some(credentials);
                            if is_current {
                                s.notice.clear();
                                start_listing(&mut info, &app, s, false);
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
            serial,
            append,
            result,
        } => {
            if serial != s.list_serial {
                return Update::DoNothing; // another folder or drive by now
            }
            s.loading = false;
            match result {
                Ok(page) => {
                    let mut more = browse::entries_of(&page, &s.prefix);
                    if append {
                        more.retain(|e| !s.entries.iter().any(|old| old.key == e.key));
                        s.entries.extend(more);
                    } else {
                        s.entries = more;
                    }
                    browse::sort_entries(&mut s.entries, s.sort);
                    s.next = page.next;
                    println!(
                        "AZDRIVE_LISTED {} {} {}",
                        s.slot().entry.id,
                        if s.prefix.is_empty() {
                            "/"
                        } else {
                            s.prefix.as_str()
                        },
                        s.entries.len()
                    );
                }
                Err(e) => {
                    s.error = format!("Could not list this folder: {e}");
                    eprintln!("[azdrive] {}", s.error);
                }
            }
        }
        Outcome::Downloaded { key, open, result } => match result {
            Ok((dest, written)) => {
                println!("AZDRIVE_DOWNLOADED {}", dest.display());
                s.notice = format!(
                    "Downloaded \"{}\" ({}) to {}",
                    azul_storage::key::last_segment(&key),
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
                    azul_storage::key::last_segment(&key),
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
                s.notice = format!("Deleted \"{}\".", azul_storage::key::last_segment(&key));
                start_listing(&mut info, &handle, s, false);
            }
            Err(e) => s.error = format!("Could not delete \"{key}\": {e}"),
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
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
        .with_child(sidebar(s, &app, p))
        .with_child(content(s, &app, p));
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
        .with_child(toolbar(s, &app, p));
    if !s.notice.is_empty() {
        body.add_child(
            Dom::create_span_with_text(s.notice.as_str()).with_css(format!(
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
    body = body
        .with_child(main)
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
            let (title, content) = popup_parts(popup, s, &app, p);
            body.add_child(
                Dialog::create(content)
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
/// only the traffic lights): the toolbar's colour and no line of its own, so the
/// two read as one bar.
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

fn toolbar(s: &DriveState, app: &RefAny, p: &Palette) -> Dom {
    let slot = s.slot();
    let labels: Vec<AzString> = browse::crumbs(&slot.entry.name, &s.prefix)
        .into_iter()
        .map(|(label, _)| AzString::from(label))
        .collect();
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; \
             background: {}; border-bottom: 1px solid {};",
            p.toolbar, p.line
        ))
        .with_child(button("Back", app, on_back))
        .with_child(button("Up", app, on_up))
        .with_child(button("Refresh", app, on_refresh))
        .with_child(
            Breadcrumb::create(StringVec::from(labels))
                .with_on_navigate(app.clone(), on_breadcrumb)
                .dom()
                .with_css("margin-left: 8px;"),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(button("Upload...", app, on_upload))
        .with_child(button("Download", app, on_download))
        .with_child(button("Open", app, on_open))
        .with_child(
            Button::with_type("Delete", ButtonType::Danger)
                .with_on_click(app.clone(), on_delete)
                .dom(),
        )
}

/// A drive of the sidebar, for its click.
struct SlotRef {
    app: RefAny,
    index: usize,
}

fn sidebar(s: &DriveState, app: &RefAny, p: &Palette) -> Dom {
    let mut bar = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; width: 210px; flex-shrink: 0; padding: 10px \
             8px; background: {}; border-right: 1px solid {};",
            p.sidebar, p.line
        ))
        .with_child(Dom::create_span_with_text("Drives").with_css(format!(
            "font-size: 12px; font-weight: bold; color: {}; margin: 0px 6px 6px 6px;",
            p.secondary
        )));
    for (index, slot) in s.slots.iter().enumerate() {
        let current = index == s.current;
        let kind = match &slot.entry.location {
            DriveLocation::Local { .. } => "this computer",
            DriveLocation::S3 { bucket, .. } => bucket.as_str(),
        };
        bar.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; padding: 6px 8px; margin-bottom: 2px; \
                     border-radius: 4px; cursor: pointer; background: {};",
                    if current { p.selected } else { "transparent" }
                ))
                .with_child(
                    Dom::create_span_with_text(slot.entry.name.as_str()).with_css(if current {
                        "font-weight: bold;"
                    } else {
                        ""
                    }),
                )
                .with_child(
                    Dom::create_span_with_text(kind)
                        .with_css(format!("font-size: 11px; color: {};", p.secondary)),
                )
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(SlotRef {
                        app: app.clone(),
                        index,
                    }),
                    on_drive_clicked,
                ),
        );
    }
    bar = bar
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(
            Button::create("Add drive...")
                .with_on_click(app.clone(), on_add_drive)
                .dom()
                .with_css("margin-bottom: 6px;"),
        );
    if s.slot().entry.id != HOME_ID {
        bar.add_child(
            Button::create("Remove drive")
                .with_on_click(app.clone(), on_forget_drive)
                .dom(),
        );
    }
    bar
}

/// A row of the list, for its double-click.
struct RowRef {
    app: RefAny,
    key: String,
}

fn content(s: &DriveState, app: &RefAny, p: &Palette) -> Dom {
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
    let rows: Vec<ListViewRow> = s
        .entries
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
        .and_then(|key| s.entries.iter().position(|e| e.key == key));
    let list = ListView::create(StringVec::from(columns))
        .with_rows(ListViewRowVec::from(rows))
        .with_sorted_by(Some(s.sort.column.index()))
        .with_selected_row(selected_row)
        .with_on_row_click(app.clone(), on_row_click)
        .with_on_column_click(app.clone(), on_column_click)
        .dom();
    let status = if s.loading {
        String::from("Loading...")
    } else if s.entries.is_empty() && s.error.is_empty() {
        String::from("This folder is empty.")
    } else {
        let folders = s.entries.iter().filter(|e| e.is_folder).count();
        let files = s.entries.len() - folders;
        let more = if s.next.is_some() {
            " (more in the folder)"
        } else {
            ""
        };
        format!("{folders} folders, {files} files{more}")
    };
    let mut footer = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; \
             font-size: 12px; color: {};",
            p.secondary
        ))
        .with_child(Dom::create_span_with_text(status));
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
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 1; overflow-y: auto; min-height: 0px;")
                .with_child(list),
        )
        .with_child(footer)
}

fn footer_text(s: &DriveState) -> String {
    let slot = s.slot();
    let place = match &slot.entry.location {
        DriveLocation::Local { root } => root.clone(),
        DriveLocation::S3 {
            endpoint, bucket, ..
        } => format!("s3://{bucket} at {endpoint}"),
    };
    let busy = if s.running > 0 {
        format!(" - {} running", s.running)
    } else {
        String::new()
    };
    format!(
        "{}: {place}/{}{busy}",
        slot.entry.name,
        s.prefix.trim_end_matches('/')
    )
}

/// The "Add drive" form fields, for their text callbacks.
#[derive(Clone, Copy)]
enum Field {
    Name,
    Endpoint,
    Region,
    Bucket,
    AccessKey,
    SecretKey,
}

struct FieldRef {
    app: RefAny,
    field: Field,
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
    match popup {
        Popup::AddDrive {
            form,
            editing,
            testing,
            tested,
            error,
            ..
        } => {
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
                    s.slot().entry.name
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
    }
}

/// A dialog as a sheet right of the list (`AZDRIVE_DIALOGS=inline`).
fn inline_sheet(popup: &Popup, s: &DriveState, app: &RefAny, p: &Palette) -> Dom {
    let (title, content) = popup_parts(popup, s, app, p);
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; width: 360px; flex-shrink: 0; padding: 16px; \
             background: {}; border-left: 1px solid {};",
            p.toolbar, p.line
        ))
        .with_child(
            Dom::create_span_with_text(title)
                .with_css("font-size: 17px; font-weight: bold; margin-bottom: 6px;"),
        )
        .with_child(content)
}

// ==== Callbacks: navigation ====

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
    with_state(&mut data, &mut info, |info, app, s| {
        let index = s.current;
        select_drive(info, app, s, index);
    })
}

extern "C" fn on_drive_clicked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<SlotRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        select_drive(info, app, s, index);
    })
}

extern "C" fn on_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if let Some(prefix) = s.history.back() {
            show_folder(info, app, s, prefix, false);
        }
    })
}

extern "C" fn on_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if let Some(parent) = browse::up(&s.prefix) {
            show_folder(info, app, s, parent, true);
        }
    })
}

extern "C" fn on_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if s.slot().locked() {
            let index = s.current;
            select_drive(info, app, s, index);
        } else {
            let prefix = s.prefix.clone();
            show_folder(info, app, s, prefix, false);
        }
    })
}

extern "C" fn on_load_more(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if !s.loading {
            start_listing(info, app, s, true);
        }
    })
}

extern "C" fn on_breadcrumb(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: BreadcrumbState,
) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let crumbs = browse::crumbs(&s.slot().entry.name, &s.prefix);
        if let Some((_, prefix)) = crumbs.get(state.selected_index) {
            show_folder(info, app, s, prefix.clone(), true);
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
    s.selected = s.entries.get(row).map(|e| e.key.clone());
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
    let s = &mut *guard;
    let Some(column) = Column::from_index(column) else {
        return Update::DoNothing;
    };
    s.sort = s.sort.clicked(column);
    browse::sort_entries(&mut s.entries, s.sort);
    Update::RefreshDom
}

/// A folder opens; a file is fetched and opened with the OS's app.
fn activate(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, key: &str) {
    let Some(entry) = s.entries.iter().find(|e| e.key == key).cloned() else {
        return;
    };
    s.selected = Some(entry.key.clone());
    if entry.is_folder {
        show_folder(info, app, s, entry.key, true);
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
    let drive = match s.slots[s.current].open() {
        Ok(drive) => drive,
        Err(e) => {
            s.error = e.to_string();
            return;
        }
    };
    let folder = if open {
        s.open_dir.join(&s.slot().entry.id)
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

extern "C" fn on_download(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        match s.selected_entry().cloned() {
            Some(entry) if !entry.is_folder => download(info, app, s, &entry, false),
            Some(_) => s.error = String::from("Download works on files; open the folder instead."),
            None => s.error = String::from("Select a file to download."),
        }
    })
}

extern "C" fn on_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        match s.selected.clone() {
            Some(key) => activate(info, app, s, &key),
            None => s.error = String::from("Select a folder or a file to open."),
        }
    })
}

extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        match s.selected_entry().cloned() {
            Some(entry) if !entry.is_folder => {
                s.popups_opened += 1;
                s.popup = Some(Popup::ConfirmDelete {
                    key: entry.key,
                    name: entry.name,
                });
            }
            Some(_) => {
                s.error =
                    String::from("Delete works on files; a folder goes away with its last file.")
            }
            None => s.error = String::from("Select a file to delete."),
        }
    })
}

extern "C" fn on_confirm_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::ConfirmDelete { key, .. }) = s.popup.take() else {
            return;
        };
        match s.slots[s.current].open() {
            Ok(drive) => spawn(info, app, s, Job::Delete { drive, key }),
            Err(e) => s.error = e.to_string(),
        }
    })
}

extern "C" fn on_upload(data: RefAny, _info: CallbackInfo) -> Update {
    // A native file dialog; its answer resumes `on_upload_picked`.
    let _request = FileDialog::open_file(
        "Upload a file",
        OptionString::None,
        OptionFileTypeList::None,
        data,
        on_upload_picked,
    );
    Update::DoNothing
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
        let Some(key) = browse::upload_key(&s.prefix, &name) else {
            s.error = format!("\"{name}\" cannot be a file name on a drive.");
            return;
        };
        match s.slots[s.current].open() {
            Ok(drive) => {
                s.error.clear();
                s.notice = format!("Uploading \"{name}\"...");
                spawn(info, app, s, Job::Upload { drive, source, key });
            }
            Err(e) => s.error = e.to_string(),
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

extern "C" fn on_add_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        if s.popup.is_none() {
            open_drive_form(s, None);
        }
    })
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
    if let Some(Popup::AddDrive { form, error, .. }) = s.popup.as_mut() {
        let text = state.get_text().as_str().to_string();
        match field {
            Field::Name => form.name = text,
            Field::Endpoint => form.endpoint = text,
            Field::Region => form.region = text,
            Field::Bucket => form.bucket = text,
            Field::AccessKey => form.access_key = text,
            Field::SecretKey => form.secret_key = text,
        }
        error.clear();
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
        println!("AZDRIVE_ADDED {id}");
        keyring(
            info,
            s,
            KeyringOp::Store {
                drive_id: id.clone(),
            },
            KeyringCall::Store(config::keyring_key(&id), secret),
        );
        select_drive(info, app, s, index);
    })
}

extern "C" fn on_forget_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        let drive_id = s.slot().entry.id.clone();
        if drive_id != HOME_ID && s.popup.is_none() {
            s.popups_opened += 1;
            s.popup = Some(Popup::ConfirmForget { drive_id });
        }
    })
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
        keyring(
            info,
            s,
            KeyringOp::Forget,
            KeyringCall::Delete(config::keyring_key(&drive_id)),
        );
        select_drive(info, app, s, 0);
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
        current: 0,
        prefix: String::new(),
        history: History::default(),
        entries: Vec::new(),
        next: None,
        loading: false,
        list_serial: 0,
        selected: None,
        sort: Sort::default(),
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
