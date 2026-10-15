//! AzDrive as the file picker of azul apps (`--pick`).
//!
//! Another app asks for a file by starting AzDrive with `--pick` and writing ONE JSON line to
//! its stdin ([`PickRequest`]); AzDrive reads it before its window opens, shows the folders
//! with the picker strip at the window's foot - the file name, the type filter, Open (Save,
//! Select folder) and Cancel - and answers with exactly one `AZPICK <json>` line on stdout
//! ([`PickAnswer`]) before its window closes and the process ends. A file of this computer is
//! answered by its path; an object of a cloud drive by its handle (drive, key, size, ETag),
//! or with `cloud: copy` fetched first through the transfer queue into the cache's `picked/`
//! folder and answered with that copy's path as well.
//!
//! Every normal start writes the registration other apps find the picker by
//! ([`registration`], `file-picker.json` beside the drives file).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType,
        TextInputOnTextInputCallbackType,
    },
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Button, ButtonType, DropDown, OnTextInputReturn, TextInput, TextInputState,
        TextInputValid,
    },
};
use azul_appkit::l10n::{label, t, Phrase};
use azul_storage::{Drive, LocalDrive};
use serde::{Deserialize, Serialize};

use crate::{
    browse::Place,
    fileops::{ConflictChoice, SourceItem, TransferKind},
    ids, with_state, DriveState,
};

/// The request and answer format this AzDrive speaks.
pub const PROTOCOL: u32 = 1;

/// What starts the answer line on stdout (AzDrive's other lines are its script markers).
pub const ANSWER_PREFIX: &str = "AZPICK ";

/// The transfer queue's name for the picker's cloud copies: their end answers the request.
pub const COPY_TARGET: &str = "picker";

/// What a request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PickOp {
    /// A file to read (`multiple`: one or more).
    Open,
    /// A folder.
    OpenDirectory,
    /// Where to write a file: a folder and a name.
    Save,
}

/// Which drives an item may come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Accept {
    #[default]
    LocalAndCloud,
    LocalOnly,
    CloudOnly,
}

/// How an object of a cloud drive is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudMode {
    /// Fetched into the cache first: the app gets a path it can read, and the handle.
    #[default]
    Copy,
    /// The handle alone, at once.
    Handle,
}

/// One entry of the type filter: "Pictures" with `*.png`, `*.jpg`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PickFilter {
    /// What the filter's box says.
    pub name: String,
    /// The file names it lets through (`*` any run of characters, `?` one).
    #[serde(default)]
    pub patterns: Vec<String>,
}

/// An app's request: one JSON line on stdin.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PickRequest {
    /// The protocol version; only [`PROTOCOL`] is answered.
    pub v: u32,
    pub op: PickOp,
    /// Open: several files may be picked.
    #[serde(default)]
    pub multiple: bool,
    /// The window's title ("Insert picture"); a default per operation when empty.
    #[serde(default)]
    pub title: String,
    /// The folder of this computer to start in.
    #[serde(default)]
    pub default_location: Option<String>,
    /// The type filter's entries, the first one active.
    #[serde(default)]
    pub filters: Vec<PickFilter>,
    #[serde(default)]
    pub accept: Accept,
    #[serde(default)]
    pub cloud: CloudMode,
    /// An access link for the picked object (a later phase; read and not used yet).
    #[serde(default)]
    pub want_link: bool,
    /// The asking app's name, for the log.
    #[serde(default)]
    pub app: String,
}

/// Reads a request line; `Err` says why it is not one this AzDrive answers.
pub fn parse_request(line: &str) -> Result<PickRequest, String> {
    let request: PickRequest =
        serde_json::from_str(line.trim()).map_err(|e| format!("not a picker request: {e}"))?;
    if request.v != PROTOCOL {
        return Err(format!(
            "protocol {} is not spoken here (this AzDrive speaks {PROTOCOL})",
            request.v
        ));
    }
    Ok(request)
}

/// A picked object of a cloud drive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloudRef {
    pub drive_id: String,
    pub drive_name: String,
    pub key: String,
    pub size: Option<u64>,
    pub etag: Option<String>,
    /// Seconds since 1970.
    pub modified: Option<u64>,
    /// An access link (never minted yet: the confirmation is a later phase).
    pub link: Option<String>,
}

/// One picked item: a path of this computer, a cloud handle, or a fetched copy with both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PickedItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudRef>,
}

impl PickedItem {
    /// A file or folder of this computer.
    #[must_use]
    pub fn local(path: impl Into<String>) -> Self {
        Self {
            path: Some(path.into()),
            cloud: None,
        }
    }
}

/// The answer: one line on stdout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PickAnswer {
    pub v: u32,
    /// `selected`, `cancelled` or `error`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<PickedItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl PickAnswer {
    #[must_use]
    pub fn selected(items: Vec<PickedItem>) -> Self {
        Self {
            v: PROTOCOL,
            status: "selected",
            items,
            message: None,
        }
    }

    #[must_use]
    pub fn cancelled() -> Self {
        Self {
            v: PROTOCOL,
            status: "cancelled",
            items: Vec::new(),
            message: None,
        }
    }

    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            v: PROTOCOL,
            status: "error",
            items: Vec::new(),
            message: Some(message.into()),
        }
    }

    /// The answer line (no newline in it: JSON escapes them).
    #[must_use]
    pub fn line(&self) -> String {
        let json = serde_json::to_string(self)
            .unwrap_or_else(|_| String::from(r#"{"v":1,"status":"error","message":"no answer"}"#));
        format!("{ANSWER_PREFIX}{json}")
    }
}

/// Whether `name` fits `pattern`: `*` any run of characters, `?` one, case ignored.
#[must_use]
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    // The classic two-pointer match with one backtrack point for the last `*`.
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, n));
                p += 1;
            }
            Some(&c) if c == '?' || c == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match star {
                Some((sp, sn)) => {
                    p = sp + 1;
                    n = sn + 1;
                    star = Some((sp, sn + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// Whether the folder view shows an item under `filter`: folders always (they lead to files),
/// files when one of its patterns fits; everything without a filter.
#[must_use]
pub fn filter_shows(filter: Option<&PickFilter>, name: &str, is_folder: bool) -> bool {
    match filter {
        None => true,
        Some(_) if is_folder => true,
        Some(f) => f.patterns.is_empty() || f.patterns.iter().any(|p| glob_matches(p, name)),
    }
}

/// What the strip's Open / Save / Select folder button does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Go into this folder (a key ending in `/`).
    OpenFolder(String),
    /// Answer these keys of the open drive.
    Answer(Vec<String>),
    /// Save needs a file name first.
    NeedName,
    /// Nothing to do yet.
    Nothing,
}

/// A name typed for a new file: trimmed, one segment, not `.` or `..`.
#[must_use]
pub fn save_name(typed: &str) -> Option<String> {
    let name = typed.trim();
    let bad = name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']);
    (!bad).then(|| name.to_string())
}

/// What the button does for `op`, the selection (`(key, is_folder)`), the open folder's prefix
/// and the typed name.
#[must_use]
pub fn decide(
    op: PickOp,
    multiple: bool,
    selected: &[(String, bool)],
    folder: &str,
    typed: &str,
) -> Decision {
    let files: Vec<String> = selected
        .iter()
        .filter(|(_, is_folder)| !is_folder)
        .map(|(key, _)| key.clone())
        .collect();
    let folders: Vec<&String> = selected
        .iter()
        .filter(|(_, is_folder)| *is_folder)
        .map(|(key, _)| key)
        .collect();
    match op {
        PickOp::OpenDirectory => match folders.as_slice() {
            [one] => Decision::Answer(vec![(*one).clone()]),
            _ => Decision::Answer(vec![folder.to_string()]),
        },
        PickOp::Open => {
            if !files.is_empty() {
                let take = if multiple { files.len() } else { 1 };
                return Decision::Answer(files.into_iter().take(take).collect());
            }
            if let [one] = folders.as_slice() {
                return Decision::OpenFolder((*one).clone());
            }
            match save_name(typed) {
                Some(name) => Decision::Answer(vec![format!("{folder}{name}")]),
                None => Decision::Nothing,
            }
        }
        PickOp::Save => {
            if typed.trim().is_empty() {
                if let Some(file) = files.first() {
                    return Decision::Answer(vec![file.clone()]);
                }
                if let [one] = folders.as_slice() {
                    return Decision::OpenFolder((*one).clone());
                }
                return Decision::NeedName;
            }
            match save_name(typed) {
                Some(name) => Decision::Answer(vec![format!("{folder}{name}")]),
                None => Decision::NeedName,
            }
        }
    }
}

/// The registration other apps find the picker by: this program, how to start it, what it
/// answers.
#[must_use]
pub fn registration(exec: &str) -> serde_json::Value {
    serde_json::json!({
        "format": "azul-storage.file-picker",
        "version": 1,
        "provider": "AzDrive",
        "exec": exec,
        "args": ["--pick"],
        "protocol": PROTOCOL,
        "modes": ["open", "open-multiple", "open-directory", "save"],
        "cloud": true,
    })
}

/// Where the registration lives: beside the drives file (the default one is
/// `<config dir>/azul-storage/drives.json`, so `<config dir>/azul-storage/file-picker.json`).
#[must_use]
pub fn registration_path(drives_file: &Path) -> PathBuf {
    drives_file.with_file_name("file-picker.json")
}

/// Writes the registration for this program beside `drives_file`; a failure is only logged
/// (apps then use the system's dialog).
pub fn register(drives_file: &Path) {
    let Ok(exec) = std::env::current_exe() else {
        return;
    };
    let path = registration_path(drives_file);
    let text = serde_json::to_string_pretty(&registration(&exec.to_string_lossy()))
        .unwrap_or_default();
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, text));
    if let Err(e) = written {
        eprintln!("[azdrive] the file picker registration {} could not be written: {e}", path.display());
    }
}

/// The drive and folder prefix a folder of this computer opens at: the drive whose root is the
/// deepest one holding `path` (`roots`: drive id and root), matching whole folder names.
#[must_use]
pub fn place_of_path(path: &Path, roots: &[(String, PathBuf)]) -> Option<(String, String)> {
    roots
        .iter()
        .filter_map(|(id, root)| {
            let rest = path.strip_prefix(root).ok()?;
            Some((root.components().count(), id, rest))
        })
        .max_by_key(|(depth, _, _)| *depth)
        .map(|(_, id, rest)| {
            let prefix: String = rest
                .components()
                .map(|c| format!("{}/", c.as_os_str().to_string_lossy()))
                .collect();
            (id.clone(), prefix)
        })
}

// ==== The picker in the running window ====

/// A cloud copy on its way: the handles it answers with, and where the copies land.
#[derive(Debug, Clone)]
pub struct PendingCopy {
    /// The handles of the objects being fetched.
    pub refs: Vec<CloudRef>,
    /// The folder of this computer each object's key is fetched under.
    pub root: PathBuf,
}

/// The picker of a `--pick` run: the request, what the strip holds, whether it answered.
#[derive(Debug, Clone)]
pub struct Picker {
    pub request: PickRequest,
    /// What the file name field holds (typed, or empty: the field shows the selection).
    pub typed: String,
    /// The active entry of the type filter: an index of the request's filters, or their count
    /// for "All files".
    pub filter: usize,
    /// A cloud copy the answer waits for.
    pub copying: Option<PendingCopy>,
    /// The answer went out: never a second one (a close after it, a late copy).
    pub answered: bool,
}

impl Picker {
    #[must_use]
    pub fn new(request: PickRequest) -> Self {
        Self {
            request,
            typed: String::new(),
            filter: 0,
            copying: None,
            answered: false,
        }
    }

    /// The type filter's active entry (None: all files).
    #[must_use]
    pub fn active_filter(&self) -> Option<&PickFilter> {
        self.request.filters.get(self.filter)
    }

    /// Whether the folder view shows an item.
    #[must_use]
    pub fn shows(&self, name: &str, is_folder: bool) -> bool {
        filter_shows(self.active_filter(), name, is_folder)
    }

    /// The window's title: the request's, else the operation's.
    #[must_use]
    pub fn title(&self) -> String {
        if self.request.title.trim().is_empty() {
            t(match self.request.op {
                PickOp::Open => "azdrive-pick-title-open",
                PickOp::OpenDirectory => "azdrive-pick-title-folder",
                PickOp::Save => "azdrive-pick-title-save",
            })
        } else {
            self.request.title.clone()
        }
    }
}

/// Prints `answer` (once) and closes the window, which ends the run.
fn answer(info: &mut CallbackInfo, s: &mut DriveState, answer: &PickAnswer) {
    let Some(picker) = s.picker.as_mut() else {
        return;
    };
    if picker.answered {
        return;
    }
    picker.answered = true;
    println!("{}", answer.line());
    info.close_window();
}

/// The request is called off (Escape, Cancel, the window's close button).
pub(crate) fn cancel(info: &mut CallbackInfo, s: &mut DriveState) {
    // A fetch on its way answers nothing when it ends.
    if let Some(picker) = s.picker.as_mut() {
        picker.copying = None;
    }
    answer(info, s, &PickAnswer::cancelled());
}

/// The window is closing: a request not answered yet is cancelled.
pub(crate) fn on_close(s: &mut DriveState) {
    if let Some(picker) = s.picker.as_mut() {
        if !picker.answered {
            picker.answered = true;
            println!("{}", PickAnswer::cancelled().line());
        }
    }
}

/// The strip's button, Enter or a double-click on a file: what [`decide`] says for the open
/// folder.
pub(crate) fn confirm(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(picker) = s.picker.as_ref() else {
        return;
    };
    if picker.copying.is_some() {
        return;
    }
    // Outside a drive (This PC, Quick access) the button opens what is selected.
    let Some(drive_id) = s.current_drive_id() else {
        crate::actions::run_action(info, app, s, crate::actions::Action::Open);
        return;
    };
    let selected: Vec<(String, bool)> = s
        .selected_entries()
        .iter()
        .map(|e| (e.key.clone(), e.is_folder))
        .collect();
    let decision = decide(
        picker.request.op,
        picker.request.multiple,
        &selected,
        s.prefix(),
        &picker.typed,
    );
    match decision {
        Decision::OpenFolder(key) => {
            if let Some(picker) = s.picker.as_mut() {
                picker.typed.clear();
            }
            crate::go(info, app, s, Place::folder(&drive_id, &key), true);
        }
        Decision::Answer(keys) => answer_keys(info, app, s, keys),
        Decision::NeedName => s.warn(Phrase::new("azdrive-pick-name-needed")),
        Decision::Nothing => {}
    }
}

/// Answers `keys` of the open drive: paths on this computer, handles (or fetched copies) of a
/// cloud drive - when the request takes items from this kind of drive.
pub(crate) fn answer_keys(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, keys: Vec<String>) {
    let (Some(index), Some(drive_id)) = (s.current_drive(), s.current_drive_id()) else {
        return;
    };
    let Some(request) = s.picker.as_ref().map(|p| p.request.clone()) else {
        return;
    };
    let local = s.local_root(index).is_some();
    match (request.accept, local) {
        (Accept::CloudOnly, true) => {
            s.warn(Phrase::new("azdrive-pick-cloud-only"));
            return;
        }
        (Accept::LocalOnly, false) => {
            s.warn(Phrase::new("azdrive-pick-local-only"));
            return;
        }
        _ => {}
    }
    if local {
        let items = keys
            .iter()
            .filter_map(|key| s.local_dir(index, key))
            .map(|path| PickedItem::local(path.to_string_lossy().into_owned()))
            .collect();
        answer(info, s, &PickAnswer::selected(items));
        return;
    }
    let drive_name = s
        .slots
        .get(index)
        .map_or_else(String::new, |slot| slot.entry.name.clone());
    let refs: Vec<CloudRef> = keys
        .iter()
        .map(|key| {
            let entry = s.entry(key);
            CloudRef {
                drive_id: drive_id.clone(),
                drive_name: drive_name.clone(),
                key: key.clone(),
                size: entry.and_then(|e| e.size),
                etag: entry.and_then(|e| e.etag.clone()),
                modified: entry.and_then(|e| e.modified),
                link: None,
            }
        })
        .collect();
    // A folder (Select folder) or a handle answers at once; a file is fetched for `copy`.
    let fetch = request.cloud == CloudMode::Copy && request.op != PickOp::OpenDirectory;
    if !fetch {
        let items = refs
            .into_iter()
            .map(|r| PickedItem {
                path: None,
                cloud: Some(r),
            })
            .collect();
        answer(info, s, &PickAnswer::selected(items));
        return;
    }
    start_copy(info, app, s, &drive_id, refs);
}

/// Fetches the objects of `refs` into `<cache>/picked/<drive id>/` through the transfer queue;
/// its end answers ([`copy_finished`]).
fn start_copy(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str, refs: Vec<CloudRef>) {
    let Some(source) = crate::open_current(s) else {
        answer(info, s, &PickAnswer::error(t("azdrive-pick-drive-closed")));
        return;
    };
    let root = s
        .cache_dir
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("AzDrive"))
        .join("picked")
        .join(drive_id);
    let items: Vec<SourceItem> = refs
        .iter()
        .map(|r| SourceItem {
            key: r.key.clone(),
            is_folder: false,
            size: r.size,
        })
        .collect();
    // The selection is of one folder: its copies land under the same folder of the cache.
    let prefix = crate::fileops::parent_of(&refs[0].key);
    let target: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(root.clone()));
    if let Some(picker) = s.picker.as_mut() {
        picker.copying = Some(PendingCopy {
            refs,
            root: root.clone(),
        });
    }
    s.info(Phrase::new("azdrive-pick-fetching"));
    crate::actions::enqueue_with(
        info,
        app,
        s,
        TransferKind::Download,
        (drive_id.to_string(), source),
        items,
        (String::from(COPY_TARGET), target),
        &prefix,
        &root.display().to_string(),
        Some(ConflictChoice::Replace),
    );
}

/// The picker's fetch ended (`done`: every object arrived): the answer, with each copy's path.
pub(crate) fn copy_finished(info: &mut CallbackInfo, s: &mut DriveState, done: bool) {
    let Some(copy) = s.picker.as_mut().and_then(|p| p.copying.take()) else {
        return;
    };
    if !done {
        answer(info, s, &PickAnswer::error(t("azdrive-pick-fetch-failed")));
        return;
    }
    let items = copy
        .refs
        .into_iter()
        .map(|r| PickedItem {
            path: Some(crate::jobs::path_in(&copy.root, &r.key).to_string_lossy().into_owned()),
            cloud: Some(r),
        })
        .collect();
    answer(info, s, &PickAnswer::selected(items));
}

/// The picker strip at the window's foot: "File name:", the field, the type filter, the
/// operation's button and Cancel.
pub(crate) fn strip(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let picker = s.picker.as_ref()?;
    // An empty field shows the selected files' names (Explorer's "a.png" "b.png").
    let shown = if picker.typed.is_empty() {
        let names: Vec<String> = s
            .selected_entries()
            .iter()
            .filter(|e| !e.is_folder || picker.request.op == PickOp::OpenDirectory)
            .map(|e| e.name.clone())
            .collect();
        match names.as_slice() {
            [one] => one.clone(),
            many => many
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(" "),
        }
    } else {
        picker.typed.clone()
    };
    let mut row = Dom::create_div()
        .with_id(ids::PICK_STRIP)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
             padding: 8px 12px; border-top: 1px solid system:separator; \
             background: system:window-background;",
        )
        .with_child(
            Dom::create_div_with_text(label("azdrive-pick-file-name"))
                .with_css("margin-right: 8px; color: system:text;"),
        )
        .with_child(
            TextInput::create()
                .with_text(AzString::from(shown))
                .with_on_text_input(app.clone(), on_name_typed as TextInputOnTextInputCallbackType)
                .with_accessibility_name(label("azdrive-pick-file-name"))
                .dom()
                .with_id(ids::PICK_NAME)
                .with_css("flex-grow: 1; min-width: 120px;"),
        );
    if !picker.request.filters.is_empty() {
        let mut choices: Vec<AzString> = picker
            .request
            .filters
            .iter()
            .map(|f| AzString::from(format!("{} ({})", f.name, f.patterns.join(", "))))
            .collect();
        choices.push(label("azdrive-pick-all-files"));
        row.add_child(
            DropDown::create(StringVec::from(choices))
                .with_selected(picker.filter)
                .with_accessibility_name(label("azdrive-pick-type"))
                .with_on_choice_change(app.clone(), on_filter as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id(ids::PICK_FILTER)
                .with_css("margin-left: 8px;"),
        );
    }
    let ok = match picker.request.op {
        PickOp::Open => "azdrive-pick-open",
        PickOp::OpenDirectory => "azdrive-pick-select-folder",
        PickOp::Save => "azdrive-pick-save",
    };
    row.add_child(
        Button::with_type(label(ok), ButtonType::Primary)
            .with_on_click(app.clone(), on_ok as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::PICK_OK)
            .with_css("margin-left: 8px;"),
    );
    row.add_child(
        Button::create(label("azdrive-pick-cancel"))
            .with_on_click(app.clone(), on_cancel as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::PICK_CANCEL)
            .with_css("margin-left: 6px;"),
    );
    Some(row)
}

extern "C" fn on_name_typed(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<DriveState>() {
        if let Some(picker) = s.picker.as_mut() {
            picker.typed = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_filter(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        if let Some(picker) = s.picker.as_mut() {
            picker.filter = index;
        }
        // Files the new filter hides are no longer picked.
        let shown: Vec<String> = s.visible_entries().iter().map(|e| e.key.clone()).collect();
        let order: Vec<&str> = shown.iter().map(String::as_str).collect();
        s.selection.retain(&order);
    })
}

extern "C" fn on_ok(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| confirm(info, app, s))
}

extern "C" fn on_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, _app, s| cancel(info, s))
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
