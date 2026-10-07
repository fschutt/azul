//! What the user asks of the workspace - open a folder, a file, a recent
//! folder or the sample, open a folder of the tree, refresh it, open / close
//! / save files, quick open, the keys - and the answers of the drive
//! threads, the sample writer and the highlight walks.

use std::path::{Path, PathBuf};

use azul::{
    callbacks::{TimerCallbackInfo, TimerCallbackReturn},
    dialog::{FileDialog, FileOpenResult},
    dom::{DomId, DomNodeId, FocusTarget, VirtualKeyCode},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use azul_appkit::{shortcuts::display_keys, ui as kit};

use crate::{
    app::{AppState, Doc, IndexState, PendingSave, Side},
    highlight::HighlightJob,
    ids, sample,
    storage::{self, DriveJob, DriveOutcome, INDEX_MAX_FILES},
    workspace::{doc_ident, key_of_path, recent_to_json, remember, Root, Tabs, Workspace},
};

/// The appkit file-job tag of the sample's files.
pub const TAG_SAMPLE: u64 = 1;
/// The settings value that keeps the recent folders (a JSON array).
pub const RECENT_KEY: &str = "recent_folders";

/// `keys` (`Mod+O`) as this platform writes them (`Cmd+O` on macOS).
#[must_use]
pub fn keys(keys: &str) -> String {
    display_keys(keys, cfg!(target_os = "macos"))
}

/// The sample workspace in the data tree.
#[must_use]
pub fn sample_root(data_root: &Path) -> Root {
    Root {
        drive_root: data_root.to_path_buf(),
        prefix: sample::prefix(),
        data_tree: true,
        name: "sample".to_string(),
    }
}

/// A folder of the user's as a workspace (no manifest is written there).
#[must_use]
pub fn folder_root(path: &Path) -> Root {
    Root {
        drive_root: path.to_path_buf(),
        prefix: String::new(),
        data_tree: false,
        name: path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()),
    }
}

/// A file on its own: its folder as the root, its name as the key.
#[must_use]
pub fn file_root(path: &Path) -> Option<(Root, String)> {
    let name = path.file_name()?.to_str()?.to_string();
    let folder = path.parent()?;
    Some((folder_root(folder), name))
}

/// `root` becomes the workspace: the open files close, its folder is
/// listed; a folder of the user's goes first in the recent list.
pub fn open_workspace(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, root: Root) {
    if st.any_dirty() {
        st.notice = format!("Save the open files first ({}).", keys("Mod+S"));
        return;
    }
    st.tabs = Tabs::default();
    st.quick = None;
    st.index.clear();
    st.index_state = IndexState::None;
    st.notice.clear();
    st.side = Side::Explorer;
    st.side_visible = true;
    st.workspace = Some(Workspace::new(root.clone()));
    if !root.data_tree {
        let folder = root.drive_root.display().to_string();
        st.recent = remember(&st.recent, &folder);
        kit::set_value(&st.kit, info, RECENT_KEY, &recent_to_json(&st.recent));
    }
    storage::spawn_drive_jobs(
        info,
        &root,
        vec![DriveJob::List { folder: String::new() }],
        app.clone(),
        on_drive_done,
    );
}

/// Recent folder `index` opened again (one that is gone leaves the list).
pub fn open_recent(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, index: usize) {
    let Some(folder) = st.recent.get(index).cloned() else {
        return;
    };
    let path = PathBuf::from(&folder);
    if path.is_dir() {
        open_workspace(st, info, app, folder_root(&path));
    } else {
        st.recent.retain(|f| *f != folder);
        kit::set_value(&st.kit, info, RECENT_KEY, &recent_to_json(&st.recent));
        st.notice = format!("{folder} is not there any more.");
    }
}

/// "Open Folder..." (the side bar, the welcome page, Mod+O): the OS folder
/// dialog; the folder picked becomes the workspace ([`on_folder_picked`]).
pub fn ask_folder(app: &RefAny) {
    let _request = FileDialog::open_directory(
        "Open a folder",
        OptionString::None,
        app.clone(),
        on_folder_picked,
    );
}

/// The folder dialog's answer: the folder becomes the workspace (a cancel
/// changes nothing).
extern "C" fn on_folder_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let folder = PathBuf::from(path.inner.as_str());
    println!("AZCODE_FOLDER {}", folder.display());
    crate::ui::with_state(&mut data, &mut info, |st, info, app| {
        st.sample = false;
        open_workspace(st, info, app, folder_root(&folder));
    })
}

/// "Open File..." (the welcome page): the OS file dialog; the file picked
/// opens ([`open_path`]).
pub fn ask_file(app: &RefAny) {
    let _request = FileDialog::open_file(
        "Open a file",
        OptionString::None,
        OptionFileTypeList::None,
        app.clone(),
        on_file_picked,
    );
}

/// The file dialog's answer: the file opens (a cancel changes nothing).
extern "C" fn on_file_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let file = PathBuf::from(path.inner.as_str());
    println!("AZCODE_FILE {}", file.display());
    crate::ui::with_state(&mut data, &mut info, |st, info, app| open_path(st, info, app, &file))
}

/// The sample workspace (its files are written the first time).
pub fn open_sample(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    st.sample = true;
    let root = sample_root(&st.data_root);
    open_workspace(st, info, app, root);
}

/// A folder of the tree opened (listed first if needed) or closed.
pub fn toggle_folder(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, key: &str, open: bool) {
    let Some(w) = st.workspace.as_mut() else {
        return;
    };
    if w.toggle(key, open) {
        let root = w.root.clone();
        storage::spawn_drive_jobs(
            info,
            &root,
            vec![DriveJob::List {
                folder: key.to_string(),
            }],
            app.clone(),
            on_drive_done,
        );
    }
}

/// The explorer's Refresh: every open folder listed again, quick open's
/// index forgotten.
pub fn refresh_explorer(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let Some(w) = st.workspace.as_mut() else {
        return;
    };
    let folders = w.refresh();
    let root = w.root.clone();
    st.index.clear();
    st.index_state = IndexState::None;
    let jobs = folders.into_iter().map(|folder| DriveJob::List { folder }).collect();
    storage::spawn_drive_jobs(info, &root, jobs, app.clone(), on_drive_done);
}

/// The file at `path` opened: as a workspace file when it lies in the
/// workspace, else on its own (its folder as its root).
pub fn open_path(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, path: &Path) {
    if let Some(key) = st.workspace.as_ref().and_then(|w| key_of_path(&w.root, path)) {
        open_file(st, info, app, &key);
        return;
    }
    match file_root(path) {
        Some((root, key)) => open_file_in(st, info, app, root, &key),
        None => st.notice = format!("{} is not a file AzCode can open.", path.display()),
    }
}

/// A file of the workspace opened.
pub fn open_file(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, key: &str) {
    let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) else {
        return;
    };
    open_file_in(st, info, app, root, key);
}

/// File `key` of `root` opened: its tab if it is open, else it is read
/// ([`on_drive_done`] opens the tab and gives the editor the focus).
pub fn open_file_in(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, root: Root, key: &str) {
    if let Some(i) = st.tabs.find(&doc_ident(&root, key)) {
        st.tabs.active = i;
        st.refresh_find();
        return;
    }
    storage::spawn_drive_jobs(
        info,
        &root,
        vec![DriveJob::Read { key: key.to_string() }],
        app.clone(),
        on_drive_done,
    );
}

/// The tab in front closed.
pub fn close_tab(st: &mut AppState) {
    let index = st.tabs.active;
    close_tab_at(st, index);
}

/// Tab `index` closed (a file with changes stays: save it first).
pub fn close_tab_at(st: &mut AppState, index: usize) {
    match st.tabs.docs.get(index) {
        Some(doc) if doc.dirty => {
            st.notice = format!("{} has unsaved changes: save it first ({}).", doc.name, keys("Mod+S"));
        }
        Some(_) => {
            st.tabs.close(index);
            st.refresh_find();
        }
        None => {}
    }
}

/// Every file with changes written (through its root's drive, on a
/// Thread; one thread per root).
pub fn save(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let mut batches: Vec<(Root, Vec<DriveJob>)> = Vec::new();
    for doc in st.tabs.docs.iter().filter(|d| d.dirty) {
        if st.saving.iter().any(|p| p.ident == doc.ident) {
            continue;
        }
        let Some((bytes, depth)) = doc.file_bytes() else {
            continue;
        };
        st.saving.push(PendingSave {
            ident: doc.ident.clone(),
            depth,
        });
        let job = DriveJob::Write {
            key: doc.key.clone(),
            bytes,
        };
        match batches.iter_mut().find(|(root, _)| *root == doc.root) {
            Some((_, jobs)) => jobs.push(job),
            None => batches.push((doc.root.clone(), vec![job])),
        }
    }
    for (root, jobs) in batches {
        storage::spawn_drive_jobs(info, &root, jobs, app.clone(), on_drive_done);
    }
}

/// Quick open (Mod+P): the palette over the workspace's files (walked the
/// first time, on a Thread), its field focused.
pub fn quick_open(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) else {
        st.notice = format!("Open a folder first ({}): quick open searches its files.", keys("Mod+O"));
        return;
    };
    st.quick = Some(String::new());
    if st.index_state == IndexState::None {
        st.index_state = IndexState::Running;
        storage::spawn_drive_jobs(
            info,
            &root,
            vec![DriveJob::Index {
                limit: INDEX_MAX_FILES,
            }],
            app.clone(),
            on_drive_done,
        );
    }
    focus_soon(info, ids::QUICK_OPEN.as_str());
}

/// The window's keys (after the kit's): Mod+O open a folder, Mod+P quick
/// open, Mod+B the side bar, Mod+S save, Mod+F find, Mod+H replace, Mod+G go
/// to line, Mod+W close the tab, F3 / Shift+F3 the next / previous match,
/// Escape closes quick open and the bars. `true` when the key was taken.
pub fn handle_key(
    st: &mut AppState,
    info: &mut CallbackInfo,
    app: &RefAny,
    key: VirtualKeyCode,
    primary: bool,
    shift: bool,
) -> bool {
    let has_file = st.tabs.active().is_some();
    match (key, primary) {
        (VirtualKeyCode::O, true) => ask_folder(app),
        (VirtualKeyCode::P, true) => quick_open(st, info, app),
        (VirtualKeyCode::B, true) => st.side_visible = !st.side_visible,
        (VirtualKeyCode::S, true) => save(st, info, app),
        (VirtualKeyCode::F, true) if has_file => {
            st.find.open = true;
            st.find.replace_open = false;
            st.refresh_find();
            focus_soon(info, ids::FIND_INPUT.as_str());
        }
        (VirtualKeyCode::H, true) if has_file => {
            st.find.open = true;
            st.find.replace_open = true;
            st.refresh_find();
            if st.find.query.is_empty() {
                focus_soon(info, ids::FIND_INPUT.as_str());
            } else {
                focus_soon(info, ids::REPLACE_INPUT.as_str());
            }
        }
        (VirtualKeyCode::G, true) if has_file => {
            st.goto = Some(String::new());
            focus_soon(info, ids::GOTO_INPUT.as_str());
        }
        (VirtualKeyCode::W, true) => close_tab(st),
        (VirtualKeyCode::F3, false) => st.step_match(!shift),
        (VirtualKeyCode::Escape, false) if st.quick.is_some() => st.quick = None,
        (VirtualKeyCode::Escape, false) if st.find.open || st.goto.is_some() => {
            st.find.open = false;
            st.goto = None;
            focus_soon(info, ids::EDITOR.as_str());
        }
        _ => return false,
    }
    true
}

// ==== The focus of a node the next build makes ====

/// What a focus timer looks for: a DOM id.
struct FocusRequest {
    id: String,
}

/// Focuses the node with DOM id `id` once the window has it. The engine
/// resolves a focus against the layout it has NOW, so a node that the
/// rebuild this callback asks for makes (the find field, quick open) cannot
/// be focused from here: a timer looks for it every 40 ms (two seconds at
/// most), as AzMail's compose window does for its editor.
pub fn focus_soon(info: &mut CallbackInfo, id: &str) {
    let timer = Timer::create(
        RefAny::new(FocusRequest { id: id.to_string() }),
        on_focus_tick,
        info.get_system_time_fn(),
    )
    .with_interval(Duration::System(SystemTimeDiff::from_millis(40)));
    info.add_timer(TimerId::unique(), timer);
}

extern "C" fn on_focus_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(id) = data.downcast_ref::<FocusRequest>().map(|r| r.id.clone()) else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let tries = info.call_count;
    let mut callback_info = info.callback_info;
    let dom = DomId { inner: 0 };
    let node = callback_info.get_node_id_by_id_attribute(dom, id.as_str());
    if node.into_raw() == 0 {
        return if tries > 50 {
            TimerCallbackReturn::terminate_unchanged()
        } else {
            TimerCallbackReturn::continue_unchanged()
        };
    }
    callback_info.set_focus(FocusTarget::Id(DomNodeId { dom, node }));
    TimerCallbackReturn::terminate_unchanged()
}

// ==== The answers ====

/// What the drive threads did: a folder listed, a file read or written, the
/// workspace's files indexed. A listing or an index of a workspace that is
/// no longer open is dropped.
pub extern "C" fn on_drive_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = storage::take_drive_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let root = reply.root;
    let current = st.workspace.as_ref().is_some_and(|w| w.root == root);
    for outcome in reply.outcomes {
        match outcome {
            DriveOutcome::Listed {
                folder,
                folders,
                files,
                error,
            } => {
                if !current {
                    continue;
                }
                if let Some(e) = error {
                    st.notice = format!("{folder} could not be listed: {e}");
                }
                let empty_sample = folder.is_empty()
                    && folders.is_empty()
                    && files.is_empty()
                    && st.sample
                    && root.data_tree;
                if empty_sample && !st.writing_sample {
                    st.writing_sample = true;
                    kit::spawn_file_jobs(&mut info, &st.data_root, sample::jobs(), handle.clone(), TAG_SAMPLE, on_files_done);
                    continue;
                }
                let n = folders.len() + files.len();
                if let Some(w) = st.workspace.as_mut() {
                    w.set_listing(&folder, folders, files);
                }
                println!("AZCODE_LISTED {} {n}", if folder.is_empty() { "/" } else { folder.as_str() });
            }
            DriveOutcome::Read { key, result } => match result {
                Ok(bytes) => {
                    let id = st.new_id();
                    let doc = Doc::open(id, &root, &key, &bytes);
                    println!("AZCODE_OPENED {key} {}", doc.line_count);
                    st.tabs.open(doc);
                    st.refresh_find();
                    focus_soon(&mut info, ids::EDITOR.as_str());
                }
                Err(e) => st.notice = format!("{key} could not be read: {e}"),
            },
            DriveOutcome::Written { key, result } => {
                let ident = doc_ident(&root, &key);
                let pending = st
                    .saving
                    .iter()
                    .position(|p| p.ident == ident)
                    .map(|i| st.saving.remove(i));
                match result {
                    Ok(()) => {
                        if let (Some(p), Some(i)) = (pending, st.tabs.find(&ident)) {
                            let doc = &mut st.tabs.docs[i];
                            doc.with_text(|t| t.buffer.mark_saved_at(p.depth));
                            doc.refresh();
                        }
                        println!("AZCODE_SAVED {key}");
                    }
                    Err(e) => {
                        st.close_after_save = false;
                        st.notice = format!("{key} could not be saved: {e}");
                    }
                }
            }
            DriveOutcome::Indexed {
                files,
                complete,
                error,
            } => {
                if !current {
                    continue;
                }
                if let Some(e) = error {
                    st.notice = format!("The folder's files could not be listed: {e}");
                } else if !complete {
                    st.notice = format!("Quick open searches the first {} files of the folder.", files.len());
                }
                println!("AZCODE_INDEXED {}", files.len());
                st.index = files;
                st.index_state = IndexState::Done;
            }
        }
    }
    if st.close_after_save && !st.is_saving() && !st.any_dirty() {
        info.close_window();
    }
    Update::RefreshDom
}

/// The sample's files were written: list the workspace again.
pub extern "C" fn on_files_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    if reply.tag != TAG_SAMPLE {
        return Update::DoNothing;
    }
    st.writing_sample = false;
    // Written once: an empty listing after this is not a reason to write again.
    st.sample = false;
    if let Some(e) = reply.outcomes.iter().find_map(|o| o.error()) {
        st.notice = format!("The sample could not be written: {e}");
    }
    if let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) {
        storage::spawn_drive_jobs(
            &mut info,
            &root,
            vec![DriveJob::List { folder: String::new() }],
            handle,
            on_drive_done,
        );
    }
    Update::RefreshDom
}

/// Every quarter second: the open files whose view asked for a line too far
/// down get their walk on a Thread.
pub extern "C" fn highlight_tick(mut app: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let handle = app.clone();
    let jobs: Vec<(u64, HighlightJob)> = match app.downcast_ref::<AppState>() {
        Some(st) => st
            .tabs
            .docs
            .iter()
            .filter_map(|d| {
                d.with_text(|t| {
                    let line = t.walk_to.take()?;
                    if t.walking {
                        return None;
                    }
                    t.walking = true;
                    let buffer = &t.buffer;
                    Some(t.highlighter.job_for(line, &|i| buffer.line(i)))
                })
                .flatten()
                .map(|job| (d.id, job))
            })
            .collect(),
        None => return TimerCallbackReturn::continue_unchanged(),
    };
    for (doc, job) in jobs {
        storage::spawn_highlight(&mut info.callback_info, doc, job, handle.clone(), on_highlight_done);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// A walk came back: its checkpoints taken in (unless the text changed).
pub extern "C" fn on_highlight_done(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = storage::take_highlight_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let Some(guard) = app.downcast_ref::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(doc) = guard.tabs.docs.iter().find(|d| d.id == reply.doc) else {
        return Update::DoNothing;
    };
    let result = reply.result;
    doc.with_text(|t| {
        t.walking = false;
        if let Some(r) = result {
            t.highlighter.adopt(r);
        }
    });
    Update::RefreshDom
}
