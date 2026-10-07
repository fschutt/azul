//! What the user asks of the workspace - open a folder or the sample, open
//! a folder of the tree, open / close / save files, the keys - and the
//! answers of the drive threads, the sample writer and the highlight walks.

use std::path::Path;

use azul::{
    callbacks::{TimerCallbackInfo, TimerCallbackReturn},
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    option::OptionString,
    prelude::*,
};
use azul_appkit::ui as kit;

use crate::{
    app::{AppState, Doc, PendingSave},
    highlight::HighlightJob,
    sample,
    storage::{self, DriveJob, DriveOutcome},
    workspace::{Root, Tabs, Workspace},
};

/// The appkit file-job tag of the sample's files.
pub const TAG_SAMPLE: u64 = 1;

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

/// `root` becomes the workspace: its folder is listed.
pub fn open_workspace(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, root: Root) {
    if st.any_dirty() {
        st.notice = "Save the open files first.".to_string();
        return;
    }
    st.tabs = Tabs::default();
    st.workspace = Some(Workspace::new(root.clone()));
    storage::spawn_drive_jobs(
        info,
        &root,
        vec![DriveJob::List { folder: String::new() }],
        app.clone(),
        on_drive_done,
    );
}

/// "Open Folder..." (the welcome screen, Mod+O): the OS folder dialog; the
/// folder picked becomes the workspace ([`on_folder_picked`]).
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
    let folder = std::path::PathBuf::from(path.inner.as_str());
    println!("AZCODE_FOLDER {}", folder.display());
    crate::ui::with_state(&mut data, &mut info, |st, info, app| {
        st.sample = false;
        open_workspace(st, info, app, folder_root(&folder));
    })
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

/// A file opened: its tab if it is open, else it is read.
pub fn open_file(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, key: &str) {
    if let Some(i) = st.tabs.find(key) {
        st.tabs.active = i;
        st.refresh_find();
        return;
    }
    let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) else {
        return;
    };
    storage::spawn_drive_jobs(
        info,
        &root,
        vec![DriveJob::Read { key: key.to_string() }],
        app.clone(),
        on_drive_done,
    );
}

/// The tab in front closed (a file with changes stays: save it first).
pub fn close_tab(st: &mut AppState) {
    let index = st.tabs.active;
    match st.tabs.docs.get(index) {
        Some(doc) if doc.dirty => {
            st.notice = format!("{} has unsaved changes: save it first (Ctrl/Cmd+S).", doc.name);
        }
        Some(_) => {
            st.tabs.close(index);
            st.refresh_find();
        }
        None => {}
    }
}

/// Every file with changes written (through the drive, on a Thread).
pub fn save(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) else {
        return;
    };
    let mut jobs = Vec::new();
    for doc in st.tabs.docs.iter().filter(|d| d.dirty) {
        if st.saving.iter().any(|p| p.key == doc.key) {
            continue;
        }
        if let Some((bytes, depth)) = doc.file_bytes() {
            st.saving.push(PendingSave {
                key: doc.key.clone(),
                depth,
            });
            jobs.push(DriveJob::Write {
                key: doc.key.clone(),
                bytes,
            });
        }
    }
    storage::spawn_drive_jobs(info, &root, jobs, app.clone(), on_drive_done);
}

/// The window's keys (after the kit's): Mod+O open a folder, Mod+S save,
/// Mod+F find, Mod+H replace, Mod+G go to line, Mod+W close the tab, F3 /
/// Shift+F3 the next / previous match, Escape closes the bars. `true` when
/// the key was taken.
pub fn handle_key(
    st: &mut AppState,
    info: &mut CallbackInfo,
    app: &RefAny,
    key: VirtualKeyCode,
    primary: bool,
    shift: bool,
) -> bool {
    match (key, primary) {
        (VirtualKeyCode::O, true) => ask_folder(app),
        (VirtualKeyCode::S, true) => save(st, info, app),
        (VirtualKeyCode::F, true) => {
            st.find.open = true;
            st.find.replace_open = false;
            st.refresh_find();
        }
        (VirtualKeyCode::H, true) => {
            st.find.open = true;
            st.find.replace_open = true;
            st.refresh_find();
        }
        (VirtualKeyCode::G, true) => st.goto = Some(String::new()),
        (VirtualKeyCode::W, true) => close_tab(st),
        (VirtualKeyCode::F3, false) => st.step_match(!shift),
        (VirtualKeyCode::Escape, false) if st.find.open || st.goto.is_some() => {
            st.find.open = false;
            st.goto = None;
        }
        _ => return false,
    }
    true
}

// ==== The answers ====

/// What the drive threads did: a folder listed, a file read or written.
pub extern "C" fn on_drive_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = storage::take_drive_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    for outcome in reply.outcomes {
        match outcome {
            DriveOutcome::Listed {
                folder,
                folders,
                files,
                error,
            } => {
                if let Some(e) = error {
                    st.notice = format!("{folder} could not be listed: {e}");
                }
                let empty_sample = folder.is_empty()
                    && folders.is_empty()
                    && files.is_empty()
                    && st.sample
                    && st.workspace.as_ref().is_some_and(|w| w.root.data_tree);
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
                    let doc = Doc::open(id, &key, &bytes);
                    println!("AZCODE_OPENED {key} {}", doc.line_count);
                    st.tabs.open(doc);
                    st.refresh_find();
                }
                Err(e) => st.notice = format!("{key} could not be read: {e}"),
            },
            DriveOutcome::Written { key, result } => {
                let pending = st
                    .saving
                    .iter()
                    .position(|p| p.key == key)
                    .map(|i| st.saving.remove(i));
                match result {
                    Ok(()) => {
                        if let (Some(p), Some(i)) = (pending, st.tabs.find(&key)) {
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
            DriveOutcome::Indexed { .. } => {}
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
