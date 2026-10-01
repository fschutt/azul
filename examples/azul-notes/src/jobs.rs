//! The storage jobs on azul `Thread`s and what their answers do; the
//! note's life around them: open, new, save (debounced, on switching notes,
//! on closing), versions, external edits, images.

use std::{sync::Arc, time::Duration as StdDuration};

use azul::{
    callbacks::{CallbackInfo, RefAny, TimerCallbackInfo, TimerCallbackReturn, Update},
    error::ResultRawImageDecodeImageError,
    image::{ImageRef, RawImage},
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg, Timer,
        TimerId,
    },
    time::{Duration, SystemTimeDiff},
    vec::U8VecRef,
};
use azul_storage::Drive;

use crate::{
    editor,
    model::{self, ListRow, Note, Scope},
    store::{self, Job, Outcome, SaveJob},
    AppState, Overlay, Screen, Status,
};

/// A thread's start data, taken out once.
struct JobInit {
    drive: Option<Arc<dyn Drive>>,
    job: Option<Job>,
}

/// A thread's answer, taken out once by the write-back.
struct Done {
    outcome: Option<Outcome>,
}

/// Runs on a worker thread: the blocking storage job, then its answer to
/// the UI thread.
extern "C" fn job_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let taken = init.downcast_mut::<JobInit>().and_then(|mut init| {
        let drive = init.drive.take()?;
        let job = init.job.take()?;
        Some((drive, job))
    });
    let Some((drive, job)) = taken else {
        return;
    };
    let outcome = store::run_job(&*drive, job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_job_done,
        RefAny::new(Done {
            outcome: Some(outcome),
        }),
    )));
}

/// Starts `job` on a thread of its own.
pub fn spawn(info: &mut CallbackInfo, app: &RefAny, state: &mut AppState, job: Job) {
    state.running += 1;
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit {
                drive: Some(state.drive.clone()),
                job: Some(job),
            }),
            app.clone(),
            job_thread,
        ),
    );
}

// ==== Startup, the autosave clock, focus and close ====

/// The window exists: load the library (the sample first with `--sample`)
/// and start the autosave clock.
pub extern "C" fn on_startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    let job = if state.args.sample {
        Job::Seed {
            files: crate::sample::sample_files(azul_storage::time::now_unix()),
        }
    } else {
        Job::Load
    };
    spawn(&mut info, &app, state, job);
    let timer = Timer::create(app.clone(), autosave_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(250)));
    info.add_timer(TimerId::unique(), timer);
    Update::DoNothing
}

/// Every 250 ms: the edited notes are saved once the typing paused for the
/// autosave delay.
extern "C" fn autosave_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let state = &mut *guard;
    let pause = StdDuration::from_millis(state.settings.autosave_ms);
    let due = state.last_edit.is_some_and(|t| t.elapsed() >= pause);
    if !due {
        return TimerCallbackReturn::continue_unchanged();
    }
    state.last_edit = None;
    if save_all(&mut info.callback_info, &app, state, false) {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// One-shot: the keyboard focus goes into the editor (a new note's body
/// exists only after the layout that follows its creation).
extern "C" fn focus_editor_tick(_data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    editor::focus_editor(&mut info.callback_info);
    TimerCallbackReturn::terminate_unchanged()
}

/// Puts the focus into the editor after the next layout.
pub fn focus_editor_soon(info: &mut CallbackInfo) {
    let timer = Timer::create(RefAny::new(()), focus_editor_tick, info.get_system_time_fn())
        .with_delay(Duration::System(SystemTimeDiff::from_millis(120)));
    info.add_timer(TimerId::unique(), timer);
}

/// The window got the focus: read what other programs changed.
pub extern "C" fn on_window_focus(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    if !state.loaded {
        return Update::DoNothing;
    }
    let known = state
        .library
        .notes
        .iter()
        .filter(|n| !n.saved.is_empty())
        .map(|n| (n.key(), n.file_modified))
        .collect();
    spawn(&mut info, &app, state, Job::Rescan { known });
    Update::DoNothing
}

/// The window is asked to close: unsaved notes are saved first (with a
/// version), and the window closes when the last save answered.
pub extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    let pending = state.library.notes.iter().any(|n| n.dirty) || !state.saving.is_empty();
    if !pending {
        return Update::DoNothing;
    }
    let mut window = info.get_current_window_state();
    window.flags.close_requested = false;
    info.modify_window_state(window);
    state.closing = true;
    save_all(&mut info, &app, state, true);
    Update::RefreshDom
}

// ==== Saving ====

/// Starts the save of note `id` when it is dirty and no save of it is on
/// the way (that one's answer saves again when the note changed since).
/// Returns whether a save started.
pub fn save_note(info: &mut CallbackInfo, app: &RefAny, state: &mut AppState, id: &str, version: bool) -> bool {
    if state.saving.contains(id) {
        return false;
    }
    let now = azul_storage::time::now_unix();
    let every = state.settings.version_minutes * 60;
    let last = state.last_version.get(id).copied().unwrap_or(0);
    let Some(note) = state.library.get_mut(id) else {
        return false;
    };
    if !note.dirty {
        return false;
    }
    let text = note.to_file();
    if text == note.saved && note.moved_from.is_none() {
        note.dirty = false;
        return false;
    }
    let keep_version = version || now.saturating_sub(last) >= every;
    let move_assets = note.moved_from.as_deref().and_then(model::parse_note_key).map(|(old_notebook, id)| {
        (model::assets_prefix(&old_notebook, &id), note.assets_prefix())
    });
    let job = Job::Save(SaveJob {
        id: note.id.clone(),
        key: note.key(),
        text,
        generation: note.generation,
        old_key: note.moved_from.clone(),
        move_assets,
        version: keep_version.then(|| model::history_key(id, now)),
    });
    if keep_version {
        state.last_version.insert(id.to_string(), now);
    }
    state.saving.insert(id.to_string());
    state.status = Status::Saving;
    spawn(info, app, state, job);
    true
}

/// Saves every dirty note. Returns whether a save started.
pub fn save_all(info: &mut CallbackInfo, app: &RefAny, state: &mut AppState, version: bool) -> bool {
    let dirty: Vec<String> = state
        .library
        .notes
        .iter()
        .filter(|n| n.dirty)
        .map(|n| n.id.clone())
        .collect();
    let mut started = false;
    for id in dirty {
        started |= save_note(info, app, state, &id, version);
    }
    started
}

// ==== Opening notes ====

/// Asks for the images of the open note that are not loaded yet.
pub fn request_images(info: &mut CallbackInfo, app: &RefAny, state: &mut AppState) {
    let Some(note) = state.open_note() else {
        return;
    };
    let keys: Vec<String> = note
        .doc
        .image_srcs()
        .iter()
        .filter_map(|src| store::image_key(&note.notebook, src))
        .filter(|key| !state.images_requested.contains(key))
        .collect();
    if keys.is_empty() {
        return;
    }
    state.images_requested.extend(keys.iter().cloned());
    spawn(info, app, state, Job::Images { keys });
}

/// Opens note `id` in the editor: the note that was open is saved (with a
/// version), the editor's old content and caret go.
pub fn open_note(info: &mut CallbackInfo, app: &RefAny, state: &mut AppState, id: &str) {
    if state.open.as_deref() == Some(id) {
        return;
    }
    if let Some(previous) = state.open.clone() {
        save_note(info, app, state, &previous, true);
    }
    if state.library.get(id).is_none() {
        return;
    }
    // The host's content is replaced: drop the typing and the caret of the
    // note that was there (the engine would paint them over the new one).
    if let Some(host) = editor::host_node(info, editor::root_dom()) {
        info.reset_editor_content(host, false);
    }
    state.open = Some(id.to_string());
    state.editor = editor::EditorState::default();
    state.tag_draft.clear();
    if state.screen == Screen::History {
        state.screen = Screen::Notes;
        state.history = None;
    }
    println!("AZNOTES_OPEN {id}");
    request_images(info, app, state);
}

/// The notebook a new note goes into: the one shown, else the default.
fn notebook_for_new(state: &AppState) -> String {
    match &state.query.scope {
        Scope::Notebook(path) => path.clone(),
        _ => model::DEFAULT_NOTEBOOK.to_string(),
    }
}

/// Creates a note in the shown notebook (tagged with the shown tag, pinned
/// in Pinned), opens it, and puts the focus into its body.
pub fn new_note(info: &mut CallbackInfo, app: &RefAny, state: &mut AppState) {
    let id = azul::uuid::Uuid::v4().as_str().to_string();
    let now = azul_storage::time::now_unix();
    let mut note = Note::new(&id, &notebook_for_new(state), now);
    match &state.query.scope {
        Scope::Tag(tag) => {
            note.add_tag(tag);
        }
        Scope::Pinned => note.meta.pinned = true,
        Scope::Trash => state.query.scope = Scope::All,
        _ => {}
    }
    state.query.search.clear();
    state.library.upsert(note);
    state.overlay = Overlay::None;
    state.screen = Screen::Notes;
    println!("AZNOTES_NEW {id}");
    open_note(info, app, state, &id);
    state.last_edit = Some(std::time::Instant::now());
    focus_editor_soon(info);
}

/// The first note the list shows for the current query.
fn first_listed(state: &AppState) -> Option<String> {
    state
        .library
        .rows(&state.query, azul_storage::time::now_unix(), AppState::utc_offset())
        .into_iter()
        .find_map(|row| match row {
            ListRow::Note(i) => Some(state.library.notes[i].id.clone()),
            ListRow::Section(_) => None,
        })
}

// ==== Answers ====

/// A thread's answer arrives on the UI thread.
extern "C" fn on_job_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(outcome) = msg
        .downcast_mut::<Done>()
        .and_then(|mut done| done.outcome.take())
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let state = &mut *guard;
    state.running = state.running.saturating_sub(1);
    apply(&mut info, &handle, state, outcome)
}

/// A note read from a file, dated by it.
fn note_of(file: &store::FileText) -> Option<Note> {
    let mut note = Note::from_file(&file.key, &file.text, file.modified)?;
    note.file_modified = file.modified;
    Some(note)
}

#[allow(clippy::too_many_lines)]
fn apply(info: &mut CallbackInfo, app: &RefAny, state: &mut AppState, outcome: Outcome) -> Update {
    match outcome {
        Outcome::Loaded {
            files,
            markers,
            errors,
        } => {
            for file in &files {
                if let Some(note) = note_of(file) {
                    state.library.upsert(note);
                }
            }
            state.library.notebooks.extend(markers);
            for path in state.library.notebook_paths() {
                state.nav.expanded.insert(path);
            }
            state.loaded = true;
            if !errors.is_empty() {
                state.notice = errors.join("; ");
            }
            println!("AZNOTES_LOADED {}", state.library.notes.len());
            let wanted = state
                .args
                .note
                .clone()
                .filter(|id| state.library.get(id).is_some())
                .or_else(|| first_listed(state));
            if let Some(id) = wanted {
                open_note(info, app, state, &id);
            }
            if state.screen == Screen::History {
                crate::ui::show_history(info, app, state);
            }
            Update::RefreshDom
        }
        Outcome::Rescanned {
            changed,
            removed,
            markers,
            errors,
        } => {
            let mut any = false;
            for file in &changed {
                let Some(note) = note_of(file) else { continue };
                let open = state.open.as_deref() == Some(note.id.as_str());
                match state.library.get(&note.id) {
                    Some(mine) if mine.dirty => {
                        state.notice = format!(
                            "\"{}\" changed on disk; your edits are kept and will be saved over it.",
                            mine.display_title()
                        );
                    }
                    Some(mine) if mine.saved == note.saved && mine.key() == note.key() => {}
                    _ => {
                        if open {
                            if let Some(host) = editor::host_node(info, editor::root_dom()) {
                                info.reset_editor_content(host, false);
                            }
                        }
                        state.library.upsert(note);
                        any = true;
                    }
                }
            }
            for key in removed {
                let gone: Vec<String> = state
                    .library
                    .notes
                    .iter()
                    .filter(|n| n.key() == key && !n.dirty)
                    .map(|n| n.id.clone())
                    .collect();
                for id in gone {
                    state.library.notes.retain(|n| n.id != id);
                    if state.open.as_deref() == Some(id.as_str()) {
                        state.open = None;
                    }
                    any = true;
                }
            }
            let before = state.library.notebooks.len();
            state.library.notebooks.extend(markers);
            any |= state.library.notebooks.len() != before;
            if !errors.is_empty() {
                state.notice = errors.join("; ");
                any = true;
            }
            if any && state.open.is_none() {
                if let Some(id) = first_listed(state) {
                    open_note(info, app, state, &id);
                }
            }
            crate::refresh_if(any)
        }
        Outcome::Saved {
            id,
            key,
            text,
            generation,
            modified,
            result,
        } => {
            state.saving.remove(&id);
            match result {
                Ok(()) => {
                    if let Some(note) = state.library.get_mut(&id) {
                        note.saved = text;
                        note.file_modified = modified;
                        if note.key() == key {
                            note.moved_from = None;
                        }
                        if note.generation == generation {
                            note.dirty = false;
                        }
                    }
                    println!("AZNOTES_SAVED {id} {key}");
                    let dirty = state.library.notes.iter().any(|n| n.dirty);
                    state.status = if dirty { Status::Editing } else { Status::Saved };
                    if state.closing {
                        if dirty {
                            save_all(info, app, state, true);
                        } else if state.saving.is_empty() {
                            info.close_window();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[aznotes] saving {key} failed: {e}");
                    state.status = Status::Error(e);
                    state.closing = false;
                }
            }
            Update::RefreshDom
        }
        Outcome::Deleted { id, result } => {
            if let Err(e) = result {
                state.notice = format!("The note could not be deleted: {e}");
            }
            state.library.notes.retain(|n| n.id != id);
            Update::RefreshDom
        }
        Outcome::Done { what, result } => match result {
            Ok(()) => Update::DoNothing,
            Err(e) => {
                state.notice = format!("{what} could not be written: {e}");
                Update::RefreshDom
            }
        },
        Outcome::History {
            id,
            versions,
            result,
        } => {
            if let Some(view) = state.history.as_mut().filter(|h| h.id == id) {
                view.versions = versions;
                view.loading = false;
                view.error = result.err().unwrap_or_default();
            }
            Update::RefreshDom
        }
        Outcome::Version { id, key, result } => {
            if let Some(view) = state.history.as_mut().filter(|h| h.id == id) {
                let selected = view.selected.and_then(|i| view.versions.get(i)).map(|(k, _)| k.clone());
                if selected.as_deref() == Some(key.as_str()) {
                    match result {
                        Ok(text) => view.text = Some(text),
                        Err(e) => view.error = e,
                    }
                }
            }
            Update::RefreshDom
        }
        Outcome::Imported {
            id,
            at,
            images,
            errors,
        } => {
            if !errors.is_empty() {
                state.notice = errors.join("; ");
            }
            let now = azul_storage::time::now_unix();
            if let Some(note) = state.library.get_mut(&id) {
                let mut after = at.min(note.doc.blocks.len().saturating_sub(1));
                for (src, alt) in images {
                    after = note
                        .doc
                        .insert_after(after, crate::doc::Block::new(crate::doc::BlockKind::Image { src, alt }, Vec::new()));
                }
                note.touch(now);
                note.refresh();
                state.last_edit = Some(std::time::Instant::now());
            }
            request_images(info, app, state);
            Update::RefreshDom
        }
        Outcome::Images { images, errors } => {
            for e in errors {
                eprintln!("[aznotes] image: {e}");
            }
            for (key, bytes) in images {
                if let ResultRawImageDecodeImageError::Ok(raw) =
                    RawImage::decode_image_bytes_any(U8VecRef::from(bytes.as_slice()))
                {
                    if let Some(image) = ImageRef::create_rawimage(raw).into_option() {
                        state.images.insert(key, image);
                    }
                }
            }
            Update::RefreshDom
        }
    }
}
