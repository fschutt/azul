//! What the user asks of the workspace - open a folder, a file, a recent
//! folder or the sample, open a folder of the tree, refresh it, open / close
//! / save files, search the folder, quick open, the keys - and the answers
//! of the drive threads, the search threads, the sample writer and the
//! highlight walks.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use azul::{
    callbacks::{TimerCallbackInfo, TimerCallbackReturn},
    dialog::{FileDialog, FileOpenResult},
    dom::{DomId, DomNodeId, FocusTarget, KeyModifiers, VirtualKeyCode},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use azul_appkit::{shortcuts::display_keys, ui as kit};

use azul_appkit::l10n::{label, t_args, Arg};

use crate::{
    actions::{self, Action},
    app::{AppState, Doc, FolderSearch, IndexState, Palette, PaletteKind, PendingSave, Side},
    highlight::HighlightJob,
    ids, sample,
    search::Found,
    storage::{self, DriveJob, DriveOutcome, SearchJob, INDEX_MAX_FILES},
    workspace::{doc_ident, key_of_path, recent_to_json, remember, Root, Tabs, Workspace},
};

/// The appkit file-job tag of the sample's files.
pub const TAG_SAMPLE: u64 = 1;
/// The settings value that keeps the recent folders (a JSON array).
pub const RECENT_KEY: &str = "recent_folders";

/// `keys` (`Mod+O`, a chord `Mod+K Mod+O`) as this platform writes them
/// (`Cmd+O` on macOS).
#[must_use]
pub fn keys(keys: &str) -> String {
    let mac = cfg!(target_os = "macos");
    keys.split(' ')
        .map(|part| display_keys(part, mac))
        .collect::<Vec<_>>()
        .join(" ")
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

/// A folder of the user's as a workspace (no manifest is written there). A
/// trailing `/` (the macOS folder picker's) is left out, so the folder is
/// one recent folder however it was opened.
#[must_use]
pub fn folder_root(path: &Path) -> Root {
    let path: PathBuf = path.components().collect();
    Root {
        name: path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()),
        drive_root: path,
        prefix: String::new(),
        data_tree: false,
    }
}

/// A file on its own: its folder as the root, its name as the key.
#[must_use]
pub fn file_root(path: &Path) -> Option<(Root, String)> {
    let name = path.file_name()?.to_str()?.to_string();
    let folder = path.parent()?;
    Some((folder_root(folder), name))
}

/// The side bar's search forgotten (a search that runs is stopped).
fn reset_search(st: &mut AppState) {
    if let Some(cancel) = st.search.cancel.take() {
        cancel.store(true, Ordering::Relaxed);
    }
    let generation = st.search.generation;
    st.search = FolderSearch {
        generation,
        ..FolderSearch::default()
    };
}

/// `root` becomes the workspace: the open files close, its folder is
/// listed (and its git branch read); a folder of the user's goes first in
/// the recent list.
pub fn open_workspace(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, root: Root) {
    if st.any_dirty() {
        st.notice = t_args("azcode-save-first", &[("keys", Arg::from(keys("Mod+S")))]);
        return;
    }
    st.tabs = Tabs::default();
    st.palette = None;
    st.index.clear();
    st.index_state = IndexState::None;
    st.notice.clear();
    st.side = Side::Explorer;
    st.side_visible = true;
    st.branch = None;
    st.reveal = None;
    reset_search(st);
    st.workspace = Some(Workspace::new(root.clone()));
    let mut jobs = vec![DriveJob::List { folder: String::new() }];
    if !root.data_tree {
        let folder = root.drive_root.display().to_string();
        st.recent = remember(&st.recent, &folder);
        kit::set_value(&st.kit, info, RECENT_KEY, &recent_to_json(&st.recent));
        jobs.push(DriveJob::Branch {
            folder: root.drive_root.clone(),
        });
    }
    if let Some(folder) = st.workspace_folder() {
        println!("AZCODE_WORKSPACE {}", folder.display());
    }
    storage::spawn_drive_jobs(info, &root, jobs, app.clone(), on_drive_done);
}

/// File > Close Folder: the workspace closes with its files (a file with
/// changes keeps it open: save it first); the explorer says that no folder
/// is open.
pub fn close_folder(st: &mut AppState) {
    if st.workspace.is_none() {
        return;
    }
    if st.any_dirty() {
        st.notice = t_args("azcode-save-first", &[("keys", Arg::from(keys("Mod+S")))]);
        return;
    }
    st.workspace = None;
    st.tabs = Tabs::default();
    st.palette = None;
    st.index.clear();
    st.index_state = IndexState::None;
    st.branch = None;
    st.reveal = None;
    st.notice.clear();
    reset_search(st);
    println!("AZCODE_WORKSPACE -");
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
        st.notice = t_args("azcode-folder-gone", &[("folder", Arg::from(folder.as_str()))]);
    }
}

/// "Open Folder..." (the side bar, the welcome page, File > Open Folder,
/// Mod+K Mod+O, Mod+O): the OS folder dialog; the folder picked becomes the
/// workspace ([`on_folder_picked`]).
pub fn ask_folder(app: &RefAny) {
    let _request = FileDialog::open_directory(
        label("azcode-dialog-open-folder"),
        OptionString::None,
        app.clone(),
        on_folder_picked,
    );
}

/// The folder dialog's answer: the folder becomes the workspace (a cancel
/// changes nothing; stdout says which it was).
extern "C" fn on_folder_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let picked = FileOpenResult::downcast(result)
        .into_option()
        .and_then(|picked| picked.path.into_option());
    let Some(path) = picked else {
        println!("AZCODE_FOLDER_CANCELLED");
        return Update::DoNothing;
    };
    let folder = PathBuf::from(path.inner.as_str());
    println!("AZCODE_FOLDER {}", folder.display());
    crate::ui::with_state(&mut data, &mut info, |st, info, app| {
        st.sample = false;
        open_workspace(st, info, app, folder_root(&folder));
    })
}

/// "Open File..." (the welcome page, File > Open File...): the OS file
/// dialog; the file picked opens ([`open_path`]).
pub fn ask_file(app: &RefAny) {
    let _request = FileDialog::open_file(
        label("azcode-dialog-open-file"),
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
        None => {
            st.notice = t_args("azcode-not-a-file", &[("path", Arg::from(path.display().to_string()))]);
        }
    }
}

/// A file of the workspace opened.
pub fn open_file(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, key: &str) {
    let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) else {
        return;
    };
    open_file_in(st, info, app, root, key);
}

/// A file of the workspace opened with `found` selected in it (a result of
/// the side bar's search): at once when it is open, else once it is read.
pub fn open_file_at(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, key: &str, found: Found) {
    let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) else {
        return;
    };
    let ident = doc_ident(&root, key);
    if let Some(i) = st.tabs.find(&ident) {
        st.tabs.active = i;
        st.tabs.docs[i].select(found);
        st.refresh_find();
        focus_soon(info, ids::EDITOR.as_str());
        return;
    }
    st.reveal = Some((ident, found));
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
            st.notice = t_args(
                "azcode-tab-unsaved",
                &[("name", Arg::from(doc.name.as_str())), ("keys", Arg::from(keys("Mod+S")))],
            );
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
        st.notice = t_args("azcode-quick-open-no-folder", &[("keys", Arg::from(keys("Mod+O")))]);
        return;
    };
    st.palette = Some(Palette {
        kind: PaletteKind::Files,
        query: String::new(),
    });
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

/// Mod+F (the find bar) or Mod+H (find and replace), its field focused.
pub fn open_find(st: &mut AppState, info: &mut CallbackInfo, replace: bool) {
    st.find.open = true;
    st.find.replace_open = replace;
    st.refresh_find();
    if replace && !st.find.query.is_empty() {
        focus_soon(info, ids::REPLACE_INPUT.as_str());
    } else {
        focus_soon(info, ids::FIND_INPUT.as_str());
    }
}

/// Mod+Shift+F: the side bar's search, its field focused.
pub fn show_search(st: &mut AppState, info: &mut CallbackInfo) {
    st.side = Side::Search;
    st.side_visible = true;
    focus_soon(info, ids::SEARCH_INPUT.as_str());
}

/// The side bar's search run again for its query (the one that runs is
/// stopped): every file of the workspace, on a Thread
/// ([`on_search_done`]). An empty query clears the results.
pub fn search_folder(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    if let Some(cancel) = st.search.cancel.take() {
        cancel.store(true, Ordering::Relaxed);
    }
    st.search.generation += 1;
    st.search.results.clear();
    st.search.folded.clear();
    st.search.searched = 0;
    st.search.complete = false;
    st.search.running = false;
    let Some(root) = st.workspace.as_ref().map(|w| w.root.clone()) else {
        return;
    };
    if st.search.query.is_empty() {
        return;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    st.search.cancel = Some(cancel.clone());
    st.search.running = true;
    let job = SearchJob {
        query: st.search.query.clone(),
        how: st.search.how,
        generation: st.search.generation,
        cancel,
    };
    storage::spawn_search(info, &root, job, app.clone(), on_search_done);
}

/// Whether `key` is a modifier alone (it waits with a chord instead of
/// ending it).
fn modifier_alone(key: VirtualKeyCode) -> bool {
    matches!(
        key,
        VirtualKeyCode::LControl
            | VirtualKeyCode::RControl
            | VirtualKeyCode::LShift
            | VirtualKeyCode::RShift
            | VirtualKeyCode::LAlt
            | VirtualKeyCode::RAlt
            | VirtualKeyCode::LWin
            | VirtualKeyCode::RWin
    )
}

/// The command of the window key `key` with `m` held (after the kit's
/// keys), `None` for a key the window does not take. Mod+K starts a chord
/// (`chord`: the second key of it; Mod+K Mod+O opens a folder, VSCode's
/// way).
#[must_use]
pub fn command_of(key: VirtualKeyCode, m: KeyModifiers, has_file: bool) -> Option<Action> {
    use VirtualKeyCode as K;
    let primary = m.primary_down();
    let shift = m.shift;
    let action = match key {
        K::O if primary => Action::OpenFolder,
        K::P if primary && shift => Action::CommandPalette,
        K::P if primary => Action::QuickOpen,
        K::B if primary => Action::ToggleSideBar,
        K::S if primary => Action::Save,
        K::F if primary && shift => Action::FindInFiles,
        K::F if primary && has_file => Action::Find,
        K::H if primary && has_file => Action::Replace,
        K::E if primary && shift => Action::ShowExplorer,
        K::G if primary && has_file => Action::GoToLine,
        K::W if primary => Action::CloseEditor,
        K::J if primary => Action::ToggleTerminal,
        K::Grave if m.ctrl && shift => Action::NewTerminal,
        K::Grave if m.ctrl => Action::ToggleTerminal,
        _ => return None,
    };
    Some(action)
}

/// The second key of a Mod+K chord: Mod+O opens a folder, F (or Mod+F)
/// closes it.
#[must_use]
pub fn chord_command(key: VirtualKeyCode, m: KeyModifiers) -> Option<Action> {
    match key {
        VirtualKeyCode::O if m.primary_down() => Some(Action::OpenFolder),
        VirtualKeyCode::F => Some(Action::CloseFolder),
        _ => None,
    }
}

/// The window's keys (after the kit's): the commands of [`command_of`],
/// Mod+K chords, F3 / Shift+F3 the next / previous match, Escape closes the
/// palette and the bars. `true` when the key was taken.
pub fn handle_key(
    st: &mut AppState,
    info: &mut CallbackInfo,
    app: &RefAny,
    key: VirtualKeyCode,
    m: KeyModifiers,
) -> bool {
    if modifier_alone(key) {
        return false;
    }
    if st.chord {
        st.chord = false;
        st.notice.clear();
        match chord_command(key, m) {
            Some(action) => actions::run(st, info, app, action),
            None => st.notice = t_args("azcode-chord-unknown", &[("keys", Arg::from(keys("Mod+K")))]),
        }
        return true;
    }
    if key == VirtualKeyCode::K && m.primary_down() && !m.shift {
        st.chord = true;
        st.notice = t_args("azcode-chord-waiting", &[("keys", Arg::from(keys("Mod+K")))]);
        return true;
    }
    let has_file = st.tabs.active().is_some();
    if let Some(action) = command_of(key, m, has_file) {
        actions::run(st, info, app, action);
        return true;
    }
    match key {
        VirtualKeyCode::F3 if !m.primary_down() => st.step_match(!m.shift),
        VirtualKeyCode::Escape if st.palette.is_some() => {
            st.palette = None;
            if has_file {
                focus_soon(info, ids::EDITOR.as_str());
            }
        }
        VirtualKeyCode::Escape if st.find.open || st.goto.is_some() => {
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
/// workspace's files indexed, the folder's branch read. A listing, an index
/// or a branch of a workspace that is no longer open is dropped.
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
                    st.notice = t_args(
                        "azcode-folder-unlisted",
                        &[("folder", Arg::from(folder.as_str())), ("why", Arg::from(e.to_string()))],
                    );
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
                    let mut doc = Doc::open(id, &root, &key, &bytes);
                    println!("AZCODE_OPENED {key} {}", doc.line_count);
                    if let Some((ident, found)) = st.reveal.take() {
                        if ident == doc.ident {
                            doc.select(found);
                        } else {
                            st.reveal = Some((ident, found));
                        }
                    }
                    st.tabs.open(doc);
                    st.refresh_find();
                    focus_soon(&mut info, ids::EDITOR.as_str());
                }
                Err(e) => {
                    st.notice = t_args(
                        "azcode-file-unread",
                        &[("file", Arg::from(key.as_str())), ("why", Arg::from(e.to_string()))],
                    );
                }
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
                        st.notice = t_args(
                            "azcode-file-unsaved",
                            &[("file", Arg::from(key.as_str())), ("why", Arg::from(e.to_string()))],
                        );
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
                    st.notice = t_args("azcode-index-failed", &[("why", Arg::from(e.to_string()))]);
                } else if !complete {
                    st.notice = t_args("azcode-index-first", &[("count", Arg::from(files.len()))]);
                }
                println!("AZCODE_INDEXED {}", files.len());
                st.index = files;
                st.index_state = IndexState::Done;
            }
            DriveOutcome::Branch { folder, branch } => {
                if !current || folder != root.drive_root {
                    continue;
                }
                println!("AZCODE_BRANCH {}", branch.as_deref().unwrap_or("-"));
                st.branch = branch;
            }
        }
    }
    if st.close_after_save && !st.is_saving() && !st.any_dirty() {
        info.close_window();
    }
    Update::RefreshDom
}

/// A search of the folder came back: its results shown (unless a newer
/// search was asked for, or the workspace changed).
pub extern "C" fn on_search_done(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = storage::take_search_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let current = st.workspace.as_ref().is_some_and(|w| w.root == reply.root);
    if !current || reply.generation != st.search.generation {
        return Update::DoNothing;
    }
    let outcome = reply.outcome;
    st.search.running = false;
    st.search.cancel = None;
    st.search.results = outcome.files;
    st.search.searched = outcome.searched;
    st.search.complete = outcome.complete;
    if let Some(e) = outcome.error {
        st.notice = format!("The folder could not be searched: {e}");
    }
    println!("AZCODE_SEARCHED {} {}", st.search.hit_count(), st.search.results.len());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(primary: bool, shift: bool, ctrl: bool) -> KeyModifiers {
        let mac = cfg!(target_os = "macos");
        KeyModifiers {
            shift,
            ctrl: ctrl || (primary && !mac),
            alt: false,
            meta: primary && mac,
        }
    }

    #[test]
    fn the_window_keys_are_vscodes() {
        use VirtualKeyCode as K;
        let m = |p, s, c| mods(p, s, c);
        assert_eq!(command_of(K::P, m(true, false, false), false), Some(Action::QuickOpen));
        assert_eq!(command_of(K::P, m(true, true, false), false), Some(Action::CommandPalette));
        assert_eq!(command_of(K::F, m(true, true, false), false), Some(Action::FindInFiles));
        assert_eq!(command_of(K::F, m(true, false, false), false), None, "no file: no find bar");
        assert_eq!(command_of(K::F, m(true, false, false), true), Some(Action::Find));
        assert_eq!(command_of(K::Grave, m(false, false, true), false), Some(Action::ToggleTerminal));
        assert_eq!(command_of(K::Grave, m(false, true, true), false), Some(Action::NewTerminal));
        assert_eq!(command_of(K::J, m(true, false, false), false), Some(Action::ToggleTerminal));
        assert_eq!(command_of(K::O, m(true, false, false), false), Some(Action::OpenFolder));
        assert_eq!(command_of(K::A, m(false, false, false), true), None, "typing is the editor's");
        assert_eq!(chord_command(K::O, m(true, false, false)), Some(Action::OpenFolder));
        assert_eq!(chord_command(K::F, m(false, false, false)), Some(Action::CloseFolder));
        assert_eq!(chord_command(K::X, m(true, false, false)), None);
        assert!(modifier_alone(K::LWin) && modifier_alone(K::RControl) && !modifier_alone(K::K));
    }

    #[test]
    fn a_picked_folders_trailing_slash_is_left_out() {
        let root = folder_root(Path::new("/home/me/project/"));
        assert_eq!(root.drive_root, PathBuf::from("/home/me/project"));
        assert_eq!(root.name, "project");
        assert_eq!(root.drive_root.display().to_string(), "/home/me/project");
        assert_eq!(keys("Mod+K Mod+O"), if cfg!(target_os = "macos") { "Cmd+K Cmd+O" } else { "Ctrl+K Ctrl+O" });
        assert_eq!(keys("F3 / Shift+F3"), "F3 / Shift+F3");
    }
}
