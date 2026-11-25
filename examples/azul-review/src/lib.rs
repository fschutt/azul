//! AzReview - ink-first code review: the files of a folder as paper pages,
//! annotated with a pen (or the mouse), with voice clips.
//!
//! The window has no title bar: its top row moves it and holds the files'
//! switch, the five meanings and the nib; the page rail (the page numbers,
//! record, save, settings) and the sheets are under it, and the files fly out
//! over the sheets. The app theme and the dark mode come from
//! `ShellThemeScope` (the pages stay paper). azul-appkit gives the switches (`AzReview [FOLDER]`,
//! `--theme`, `--mode`, `--size`, `--shot`, `--data-dir`), the settings page
//! (Mod+,) and About. Every session is an archive in the data tree,
//! `review/<file>.azreview.zip`, written on a Thread (one writer).
//!
//! Dictation: the pen at rest on a page (hovering, or touching without moving after a stroke) for
//! `--dwell-ms` (default 1.2 s; Settings > Dictation) starts a note that listens
//! ([`voice`], [`dwell`], [`dictation`]): a marker on the page, the words live in the side column
//! ([`column`]), kept in the session archive. Drawing, moving away, resting again, Escape, the stop
//! button or a stretch of quiet (`--silence-ms`) ends it. `--dwell-input off|pen|any` says which pointers
//! may start one (default: a pen).
//!
//! stdout, for scripts (`scripts/azreview_e2e.py`): `AZREVIEW_FILE <path>`
//! when a file is opened, `AZREVIEW_FOLDER <path> <files>` when another folder is, `AZREVIEW_LOADED
//! <strokes> <notes>` when a saved session came back, `AZREVIEW_STROKES <n>` when a stroke is kept,
//! `AZREVIEW_SAVED <key>` / `AZREVIEW_SAVE_ERROR <why>` per write, `AZREVIEW_DICTATION started|text|
//! ended|refused ...` for the dictation; `AZREVIEW_PROFILE=1` adds `AZREVIEW_PERF` and `AZREVIEW_ARCHIVE`
//! lines (`scripts/azreview_perf.py`).

use std::path::{Path, PathBuf};

use azul::{
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    option::OptionString,
    prelude::*,
    task::TerminateTimer,
    time::SystemTimeDiff,
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    data::app_key,
    files::{FileJob, FileOutcome},
    l10n::{t, t_args, Arg},
    shortcuts::Shortcut,
    ui as kit,
};

pub mod code;
pub mod column;
pub mod dictation;
pub mod dwell;
pub mod ids;
pub mod ink;
#[cfg(test)]
mod ink_tests;
mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod model;
pub mod perf;
pub mod recorder;
pub mod recorder_azul;
pub mod recorder_clock;
pub mod recorder_codec;
pub mod recorder_edl;
pub mod recorder_events;
pub mod recorder_export;
pub mod recorder_fs;
pub mod recorder_manifest;
pub mod recorder_mp4;
pub mod recorder_policy;
pub mod recorder_screen;
pub mod recorder_settings;
pub mod recorder_track;
pub mod recorder_wb;
pub mod recorder_writer;
pub mod session;
/// The pen strokes every gesture is tested on (a loop, a slash, an underline, a scribble): shared by the
/// whiteboard's lasso and the quick slash that deletes a word.
#[cfg(test)]
pub(crate) mod stroke_fixtures;
pub mod switches;
pub mod ui;
pub mod voice;
pub mod whiteboard;
pub mod whiteboard_cutout;
pub mod whiteboard_drag;
pub mod whiteboard_geom;
pub mod whiteboard_jump;
pub mod whiteboard_json;
pub mod whiteboard_lasso;
pub mod whiteboard_store;

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;

/// The whole of the whiteboard without a window: loop, lift, drop, picture, arrow, file, jump.
#[cfg(test)]
#[path = "whiteboard_flow_test.rs"]
mod whiteboard_flow_test;

use model::{Finding, Note, Semantic, Stroke, Tool, VoiceClip};

/// What azul-appkit's switches know about AzReview.
pub const SPEC: AppSpec = AppSpec {
    name: "AzReview",
    binary: "AzReview",
    summary: "ink-first code review",
    screens: &["review"],
    files_help: "the folder to review (default: the current folder)",
};

/// The About facts.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzReview",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Ink-first code review: the files of a folder as pages, marked with a pen, with \
              voice clips. Sessions are kept in your data folder.",
    license: "MIT",
    app_folder: "review",
};

/// The keys AzReview answers (the kit adds Mod+, / F1 / Escape).
pub const SHORTCUTS: [Shortcut; 6] = [
    Shortcut::new("azreview-shortcuts-review", "Mod+S", "azreview-shortcut-save"),
    Shortcut::new("azreview-shortcuts-review", "Mod+O", "azreview-shortcut-open-folder"),
    Shortcut::new("azreview-shortcuts-review", "Mod+Z", "azreview-shortcut-undo"),
    Shortcut::new("azreview-shortcuts-review", "Pen rest", "azreview-shortcut-dictate"),
    Shortcut::new("azreview-shortcuts-review", "1 to 9", "azreview-shortcut-meaning"),
    Shortcut::new("azreview-shortcuts-review", "Right click", "azreview-shortcut-previous-nib"),
];

/// The sample rate of a voice clip, Hz (the microphone widget is asked for it).
pub const CLIP_RATE: u32 = 48_000;

/// The write-back tag of a session write.
const TAG_SAVE: u64 = 1;


pub struct AppState {
    pub files: Vec<code::SourceFile>,
    pub current: Option<usize>,
    pub strokes: Vec<Stroke>,
    pub live: Option<Stroke>,
    /// The revisions of each page's kept strokes: an ink layer draws its
    /// frame anew when its page's changes (`ink::InkRevisions`).
    pub ink: ink::InkRevisions,
    pub next_stroke_id: u64,
    pub active: Semantic,
    pub tool: Tool,
    pub findings: Vec<Finding>,
    /// The notes dictated at spots of the open file (the side column, the markers on the pages).
    pub notes: Vec<Note>,
    pub next_note_id: u64,
    /// What Mod+Z takes back, newest last, and what Mod+Shift+Z brings back.
    pub history: Vec<Edit>,
    pub redo_stack: Vec<Edit>,
    /// The pen's rest and the dictation it starts (the side column, the markers).
    pub voice: voice::Voice,
    /// AzReview's own switches of this run (they win over the settings file).
    pub switches: switches::Switches,
    /// The ink's own timings (`AZREVIEW_PROFILE=1`).
    pub perf: perf::Perf,
    pub recording: Option<VoiceClip>,
    pub clips: Vec<VoiceClip>,
    pub level_samples: usize,
    /// The level of the last packet of voice, 0 to 1 (the recording's meter).
    pub level: f32,
    pub visible_page: usize,
    pub epoch: u64,
    pub idle_timer: TimerId,
    pub root: PathBuf,
    /// The files fly out over the pages (the title row's switch; a picked
    /// file and Escape fold them away).
    pub sidebar_open: bool,
    /// What went wrong and is told over the pages until dismissed: a session
    /// not written, an archive folder that would not open.
    pub problem: Option<String>,
    pub last_pad_keys: u32,
    /// azul-appkit's kit and the data root the session archives go to.
    pub kit: RefAny,
    pub data_root: PathBuf,
    /// A session write is in flight; another one waits for it (one writer).
    pub saving: bool,
    pub save_pending: bool,
}

impl AppState {
    /// AzReview on the `files` of `root`, the first open and the files out
    /// (they are what one picks from first); `kit` and `data_root` are
    /// azul-appkit's.
    #[must_use]
    pub fn create(root: PathBuf, files: Vec<code::SourceFile>, kit: RefAny, data_root: PathBuf) -> Self {
        Self {
            current: if files.is_empty() { None } else { Some(0) },
            files,
            strokes: Vec::new(),
            live: None,
            ink: ink::InkRevisions::default(),
            next_stroke_id: 1,
            active: Semantic::Scope,
            tool: Tool::Marker,
            findings: Vec::new(),
            notes: Vec::new(),
            next_note_id: 1,
            history: Vec::new(),
            redo_stack: Vec::new(),
            voice: voice::Voice::new(dwell::Config::default(), dictation::DEFAULT_SILENCE_MS),
            switches: switches::Switches::default(),
            perf: perf::Perf::new(false),
            recording: None,
            clips: Vec::new(),
            level_samples: 0,
            level: 0.0,
            visible_page: 0,
            epoch: 0,
            idle_timer: TimerId::unique(),
            root,
            sidebar_open: true,
            problem: None,
            last_pad_keys: 0,
            kit,
            data_root,
            saving: false,
            save_pending: false,
        }
    }

    /// Opens file `index` (its pages start without ink) and folds the files
    /// away, out of the pages' way.
    pub fn open_file(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        self.begin_file(Some(index));
        self.sidebar_open = false;
        self.rederive();
    }

    /// Reviews another folder: its files replace the files, the first is open and the files are out
    /// (they are what one picks from first).
    pub fn open_folder(&mut self, root: PathBuf, files: Vec<code::SourceFile>) {
        self.root = root;
        self.current = None;
        self.files = files;
        let first = if self.files.is_empty() { None } else { Some(0) };
        self.begin_file(first);
        self.sidebar_open = true;
        self.rederive();
    }

    /// What starts afresh with another file: no ink, no notes, nothing to undo, no dictation (what
    /// listens writes into the file being left: it ends, and its words are kept there).
    fn begin_file(&mut self, index: Option<usize>) {
        self.voice.request_stop(dictation::Cause::File);
        self.voice.pending = None;
        self.current = index;
        self.strokes.clear();
        self.notes.clear();
        self.history.clear();
        self.redo_stack.clear();
        self.live = None;
        self.ink.open();
        self.visible_page = 0;
    }

    /// Picks the meaning the ink will have. The marker only highlights (scope) and the pen writes the
    /// rest, so picking one the nib cannot write changes the nib: the key lit in the title row is what
    /// the next stroke is.
    pub fn pick(&mut self, sem: Semantic) {
        self.active = sem;
        if sem.is_highlighter() {
            self.tool = Tool::Marker;
        } else if self.tool == Tool::Marker {
            self.tool = Tool::Pen;
        }
    }

    /// The meaning the next stroke has: the nib's, over the one picked.
    #[must_use]
    pub fn ink_meaning(&self) -> Semantic {
        self.tool.semantic_for(self.active)
    }

    /// Keeps a finished stroke (what Mod+Z takes back); a new stroke ends what Mod+Shift+Z would have
    /// brought back.
    pub fn keep(&mut self, stroke: Stroke) {
        self.ink.appended(stroke.page, stroke.id);
        self.history.push(Edit::Added(stroke.clone()));
        self.redo_stack.clear();
        self.strokes.push(stroke);
        self.rederive();
    }

    /// Takes the last edit back; whether there was one.
    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.history.pop() else {
            return false;
        };
        match &edit {
            Edit::Added(stroke) => {
                self.strokes.retain(|s| s.id != stroke.id);
                self.ink.touch(stroke.page);
            }
            Edit::Erased(stroke, index) => {
                self.strokes.insert((*index).min(self.strokes.len()), stroke.clone());
                self.ink.touch(stroke.page);
            }
        }
        self.redo_stack.push(edit);
        self.rederive();
        true
    }

    /// Brings the last edit taken back again; whether there was one.
    pub fn redo(&mut self) -> bool {
        let Some(edit) = self.redo_stack.pop() else {
            return false;
        };
        match &edit {
            Edit::Added(stroke) => {
                self.strokes.push(stroke.clone());
                self.ink.touch(stroke.page);
            }
            Edit::Erased(stroke, _) => {
                self.strokes.retain(|s| s.id != stroke.id);
                self.ink.touch(stroke.page);
            }
        }
        self.history.push(edit);
        self.rederive();
        true
    }

    /// Erases the stroke at `index` (what Mod+Z brings back).
    pub fn erase(&mut self, index: usize) -> Option<Stroke> {
        if index >= self.strokes.len() {
            return None;
        }
        let stroke = self.strokes.remove(index);
        self.history.push(Edit::Erased(stroke.clone(), index));
        self.redo_stack.clear();
        self.ink.touch(stroke.page);
        self.rederive();
        Some(stroke)
    }

    /// The topmost stroke of `page` the eraser at (`x`, `y`) touches: the pen's line within 6 px, the
    /// marker's broad nib within 12.
    #[must_use]
    pub fn eraser_target(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        self.strokes.iter().rposition(|st| {
            let tolerance = if st.semantic.is_highlighter() { 12.0 } else { 6.0 };
            st.page == page && st.is_near(x, y, tolerance)
        })
    }

    /// Takes back what the archive of the open file holds, under what has been drawn since the file
    /// opened (a stroke, a note: the load is a read on a Thread, and a pen is quick). What is here
    /// is renumbered past the archive's ids, and a new stroke starts a new burst. `false`: the
    /// archive is of another file (a late answer), nothing taken.
    pub fn adopt(&mut self, loaded: session::Loaded) -> bool {
        let Some(name) = self.file().map(|f| f.display.clone()) else {
            return false;
        };
        if loaded.file != name {
            return false;
        }
        let stroke_base = loaded.next_stroke_id() - 1;
        let note_base = loaded.next_note_id() - 1;
        let epoch_after = loaded.strokes.iter().map(|s| s.epoch + 1).max().unwrap_or(0);
        for stroke in &mut self.strokes {
            stroke.id += stroke_base;
            stroke.epoch += epoch_after;
        }
        for note in &mut self.notes {
            note.id += note_base;
        }
        if let Some(running) = self.voice.running.as_mut() {
            running.note += note_base;
        }
        if let Some(live) = self.live.as_mut() {
            live.id += stroke_base;
        }
        // What could be taken back before the archive came kept its old ids: the archive's ink is
        // no history, and the list starts afresh.
        self.history.clear();
        self.redo_stack.clear();
        self.next_stroke_id += stroke_base;
        self.next_note_id += note_base;
        self.epoch = self.epoch.max(epoch_after);
        let mut strokes = loaded.strokes;
        strokes.append(&mut self.strokes);
        self.strokes = strokes;
        let mut notes = loaded.notes;
        notes.append(&mut self.notes);
        self.notes = notes;
        let pages: std::collections::BTreeSet<usize> = self.strokes.iter().map(|s| s.page).collect();
        for page in pages {
            self.ink.touch(page);
        }
        self.rederive();
        true
    }

    pub(crate) fn file(&self) -> Option<&code::SourceFile> {
        self.current.and_then(|i| self.files.get(i))
    }

    fn rederive(&mut self) {
        let Some(file) = self.file().cloned() else {
            self.findings.clear();
            return;
        };
        self.findings = derive_findings(&self.strokes, &file, &self.recording);
    }
}

fn derive_findings(
    strokes: &[Stroke],
    file: &code::SourceFile,
    recording: &Option<VoiceClip>,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for s in strokes {
        let (_, y0, _, y1) = s.bounds();
        let (first_line, _) = file.page(s.page);
        let l0 = first_line + ui::line_of(y0);
        let l1 = first_line + ui::line_of(y1);

        let merged = out.iter_mut().find(|f| {
            f.epoch == s.epoch
                && f.semantic == s.semantic
                && l0 <= f.last_line.saturating_add(2)
                && l1.saturating_add(2) >= f.first_line
        });
        if let Some(f) = merged {
            f.first_line = f.first_line.min(l0);
            f.last_line = f.last_line.max(l1);
            f.stroke_count += 1;
            continue;
        }
        let voice = recording
            .as_ref()
            .filter(|c| c.stroke_ids.contains(&s.id))
            .map(|c| t_args("azreview-voice-note", &[("count", Arg::from(c.samples.len()))]));
        out.push(Finding {
            semantic: s.semantic,
            file: file.display.clone(),
            first_line: l0,
            last_line: l1,
            voice_note: voice,
            stroke_count: 1,
            epoch: s.epoch,
        });
    }
    out.sort_by_key(|f| f.first_line);
    out
}

pub fn run() {
    // AzReview's own switches first; the kit's parser gets the rest (and refuses what it does not know).
    let (own, argv) = match switches::Switches::extract(std::env::args().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let args = match AppArgs::parse(&SPEC, argv) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            if why.contains("USAGE") {
                eprintln!("\nAZREVIEW OPTIONS:\n{}", switches::HELP);
            }
            std::process::exit(2);
        }
    };
    let named_root = args.files.first().cloned();
    // A `--shot` run is a fixture: it never takes the folder the user last reviewed.
    let remember = args.shot.is_none();
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &["dictation"], args);
    // The words first: everything from here is said in the language chosen (Settings,
    // `--language`; the system's otherwise).
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != azul_appkit::args::LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
    let data_root = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>()
            .map(|k| k.data_root.clone())
            .unwrap_or_default()
    };
    let values = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>()
            .map(|k| k.settings.values.clone())
            .unwrap_or_default()
    };
    // The folder named on the command line; else the one reviewed last (an app started from the
    // Finder has the root of the disk as its folder); else the current one.
    let root = switches::pick_root(
        named_root,
        values.get(switches::KEY_FOLDER).map(String::as_str),
        remember,
        Path::is_dir,
        switches::usable_cwd(std::env::current_dir().unwrap_or_default()),
    );
    let files = code::load_tree(&root, 400);
    if let Some(first) = files.first() {
        println!("AZREVIEW_FILE {}", first.display);
    }
    let mut state = AppState::create(root, files, kit_ref.clone(), data_root);
    let resolved = switches::resolve(&values, &own);
    for note in &resolved.notes {
        eprintln!("[AzReview] {note}");
    }
    state.voice = voice::Voice::new(
        dwell::Config {
            allow: resolved.allow,
            rest_ms: resolved.rest_ms,
            ..dwell::Config::default()
        },
        resolved.silence_ms,
    );
    state.switches = own;
    state.perf = perf::Perf::new(std::env::var_os("AZREVIEW_PROFILE").is_some());
    let app = App::create(RefAny::new(state), config);
    let window =
        kit::window_options(&kit_ref, ui::layout, (1280.0, 820.0), (800.0, 520.0), on_window_created);
    app.run(window);
}

/// The kit's handle, out of the app's state.
fn kit_of(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<AppState>().map(|s| s.kit.clone())
}

/// The window exists: azul-appkit's `--shot` timer.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::on_window_created(&kit_ref, &mut info);
    }
    // What was marked on the first file in an earlier run comes back.
    load_session(&data, &mut info);
    Update::DoNothing
}

// ==== The session archive: written into the data tree on a Thread ====

/// Writes the session as it is now (`review/<file>.azreview.zip`), on a
/// Thread; a write asked for while one is in flight runs after it.
pub(crate) fn save_session(data: &RefAny, info: &mut CallbackInfo) {
    let mut handle = data.clone();
    let job = {
        let Some(mut s) = handle.downcast_mut::<AppState>() else {
            return;
        };
        if s.saving {
            s.save_pending = true;
            return;
        }
        s.saving = true;
        s.save_pending = false;
        let began = std::time::Instant::now();
        let (name, bytes) = session::archive(&s);
        if s.perf.is_on() {
            let us = u32::try_from(began.elapsed().as_micros()).unwrap_or(u32::MAX);
            s.perf.archived(us);
            println!("AZREVIEW_ARCHIVE strokes={} bytes={} us={us}", s.strokes.len(), bytes.len());
        }
        (
            s.data_root.clone(),
            FileJob::Put {
                key: app_key(ABOUT.app_folder, &name),
                bytes,
            },
        )
    };
    kit::spawn_file_jobs(info, &job.0, vec![job.1], data.clone(), TAG_SAVE, on_saved);
}

extern "C" fn on_saved(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let (again, told) = {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.saving = false;
        let mut told = false;
        for outcome in &reply.outcomes {
            match (outcome.error(), outcome) {
                (None, FileOutcome::Put { key, .. }) => {
                    println!("AZREVIEW_SAVED {key}");
                }
                (None, _) => {}
                (Some(why), _) => {
                    println!("AZREVIEW_SAVE_ERROR {why}");
                    s.problem =
                        Some(t_args("azreview-save-failed", &[("why", Arg::from(why.to_string()))]));
                    told = true;
                }
            }
        }
        (s.save_pending, told)
    };
    if again {
        save_session(&app, &mut info);
    }
    // A save that worked changes nothing on the screen: the window is rebuilt only to say one failed
    // (it was rebuilt after every stroke, in the middle of the next).
    if told {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The write-back tag of a session read.
const TAG_LOAD: u64 = 2;

/// Reads the session archive of the open file (a folder that has none answers "no file": nothing to
/// bring back), on a Thread.
pub(crate) fn load_session(data: &RefAny, info: &mut CallbackInfo) {
    let mut handle = data.clone();
    let job = {
        let Some(s) = handle.downcast_ref::<AppState>() else {
            return;
        };
        let Some(file) = s.file() else {
            return;
        };
        (
            s.data_root.clone(),
            FileJob::Get {
                key: app_key(ABOUT.app_folder, &session::archive_name(Some(file.display.as_str()))),
            },
        )
    };
    kit::spawn_file_jobs(info, &job.0, vec![job.1], data.clone(), TAG_LOAD, on_loaded);
}

extern "C" fn on_loaded(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let mut changed = false;
    for outcome in reply.outcomes {
        match outcome {
            FileOutcome::Got { result: Ok(Some(bytes)), key } => match session::load(&bytes) {
                Ok(loaded) => {
                    let (strokes, notes) = (loaded.strokes.len(), loaded.notes.len());
                    let taken = app
                        .downcast_mut::<AppState>()
                        .is_some_and(|mut s| s.adopt(loaded));
                    if taken {
                        println!("AZREVIEW_LOADED {strokes} {notes}");
                        changed = true;
                    }
                }
                Err(why) => {
                    println!("AZREVIEW_LOAD_ERROR {key}: {why}");
                    if let Some(mut s) = app.downcast_mut::<AppState>() {
                        s.problem = Some(t_args(
                            "azreview-load-failed",
                            &[("why", Arg::from(why))],
                        ));
                    }
                    changed = true;
                }
            },
            FileOutcome::Got { result: Err(why), .. } => {
                println!("AZREVIEW_LOAD_ERROR {why}");
            }
            _ => {}
        }
    }
    if changed {
        info.trigger_all_virtual_view_rerender();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// "Open folder": a folder dialog; the folder's files replace the files, and the folder is remembered
/// for the next start (an app started from the Finder has no folder of its own).
pub extern "C" fn on_open_folder(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_directory(
        t("azreview-choose-folder"),
        OptionString::None,
        data,
        on_folder_chosen,
    );
    Update::DoNothing
}

extern "C" fn on_folder_chosen(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let folder = PathBuf::from(path.inner.as_str());
    // The file being left is kept first (its archive is built now).
    save_session(&data, &mut info);
    let files = code::load_tree(&folder, 400);
    println!("AZREVIEW_FOLDER {} {}", folder.display(), files.len());
    let kit_ref = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.open_folder(folder.clone(), files);
        s.kit.clone()
    };
    kit::set_value(&kit_ref, &mut info, switches::KEY_FOLDER, &folder.to_string_lossy());
    if let Some(first) = data.downcast_ref::<AppState>().and_then(|s| s.files.first().map(|f| f.display.clone())) {
        println!("AZREVIEW_FILE {first}");
    }
    load_session(&data, &mut info);
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// The Ink menu's Undo and Redo.
pub extern "C" fn on_menu_undo(mut data: RefAny, mut info: CallbackInfo) -> Update {
    step_history(&mut data, &mut info, false)
}

pub extern "C" fn on_menu_redo(mut data: RefAny, mut info: CallbackInfo) -> Update {
    step_history(&mut data, &mut info, true)
}

/// Takes the last edit back (or brings it again), and keeps the session.
fn step_history(data: &mut RefAny, info: &mut CallbackInfo, redo: bool) -> Update {
    let changed = data
        .downcast_mut::<AppState>()
        .is_some_and(|mut s| if redo { s.redo() } else { s.undo() });
    if changed {
        save_session(data, info);
        info.trigger_all_virtual_view_rerender();
    }
    Update::RefreshDom
}

/// The keys 1 to 9, in the order of [`Semantic::ALL`].
const DIGITS: [VirtualKeyCode; 9] = [
    VirtualKeyCode::Key1,
    VirtualKeyCode::Key2,
    VirtualKeyCode::Key3,
    VirtualKeyCode::Key4,
    VirtualKeyCode::Key5,
    VirtualKeyCode::Key6,
    VirtualKeyCode::Key7,
    VirtualKeyCode::Key8,
    VirtualKeyCode::Key9,
];

/// The kit's keys first (Mod+, settings, F1 shortcuts, Escape closes them);
/// then Mod+S saves and 1 to 9 pick the ink's meaning.
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = kit_of(&mut data) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    if matches!(key, VirtualKeyCode::Escape) {
        let acted = data.downcast_mut::<AppState>().is_some_and(|mut s| {
            // What listens stops first; the files fold away on the next Escape.
            if s.voice.request_stop(dictation::Cause::Key) {
                return true;
            }
            let was = s.sidebar_open;
            s.sidebar_open = false;
            was
        });
        return if acted { Update::RefreshDom } else { Update::DoNothing };
    }
    if matches!(key, VirtualKeyCode::S) && info.get_key_modifiers().primary_down() {
        info.prevent_default();
        save_session(&data, &mut info);
        return Update::RefreshDom;
    }
    let mods = info.get_key_modifiers();
    if matches!(key, VirtualKeyCode::O) && mods.primary_down() {
        info.prevent_default();
        return on_open_folder(data, info);
    }
    // Typing in a note: the letters and digits are the note's, and Mod+Z undoes its text, not the ink.
    let typing = info.get_focused_node().into_option().is_some_and(|node| {
        info.get_node_attribute(node, "contenteditable")
            .into_option()
            .is_some_and(|v| v.as_str() == "true")
    });
    if typing {
        return Update::DoNothing;
    }
    let undo_key = matches!(key, VirtualKeyCode::Z) && mods.primary_down();
    let redo_key = (undo_key && mods.shift) || (matches!(key, VirtualKeyCode::Y) && mods.primary_down());
    if undo_key || redo_key {
        info.prevent_default();
        return step_history(&mut data, &mut info, redo_key);
    }
    let Some(index) = DIGITS.iter().position(|d| *d == key) else {
        return Update::DoNothing;
    };
    let Some(&sem) = Semantic::ALL.get(index) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.pick(sem);
    }
    Update::RefreshDom
}

/// The rail's Save button.
pub extern "C" fn on_save_button(data: RefAny, mut info: CallbackInfo) -> Update {
    save_session(&data, &mut info);
    Update::RefreshDom
}

/// The title row's switch: the files fly out over the pages, or fold away.
pub extern "C" fn on_toggle_sidebar(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.sidebar_open = !s.sidebar_open;
    Update::RefreshDom
}

/// Dismiss on the bar that told what went wrong.
pub extern "C" fn on_dismiss_problem(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.problem = None;
    Update::RefreshDom
}

/// The rail's gear: azul-appkit's settings page.
pub extern "C" fn on_settings_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::open_settings(&kit_ref, None);
    }
    Update::RefreshDom
}

pub extern "C" fn on_ink_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ms = info.get_current_mouse_state();
    if ms.right_down || ms.middle_down {
        return Update::DoNothing;
    }
    let Some(page) = ui::page_of(&mut info) else {
        return Update::DoNothing;
    };
    // The pen set down: a dictation it left behind ends (the stroke is the new thing it does).
    let ended = voice::pointer(&mut data, &mut info, Some(true));
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((p, is_eraser)) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    if is_eraser {
        if let Some(i) = s.eraser_target(page, p.x, p.y) {
            let gone = s.erase(i);
            drop(s);
            if let Some(gone) = gone {
                ui::redraw_ink(&mut info, ink::Layer::of(gone.semantic));
            }
        }
        return Update::DoNothing;
    }
    let id = s.next_stroke_id;
    s.next_stroke_id += 1;
    let semantic = s.tool.semantic_for(s.active);
    let epoch = s.epoch;
    s.live = Some(Stroke {
        page,
        semantic,
        points: vec![p],
        id,
        epoch,
    });

    if s.tool.records_audio() && s.recording.is_none() {
        s.recording = Some(VoiceClip {
            sample_rate: CLIP_RATE,
            ..VoiceClip::default()
        });
    }
    if let Some(clip) = s.recording.as_mut() {
        clip.stroke_ids.push(id);
    }
    drop(s);
    // The first dab shows at once; the stroke grows in the frame of its layer.
    ui::redraw_ink(&mut info, ink::Layer::of(semantic));
    ended
}

pub extern "C" fn on_cycle_tool(mut data: RefAny, _: CallbackInfo) -> Update {
    cycle_tool(&mut data, false)
}

pub extern "C" fn on_cycle_tool_back(mut data: RefAny, _: CallbackInfo) -> Update {
    if std::env::var("AZ_REVIEW_DEBUG").is_ok() {
        eprintln!("[review] on_cycle_tool_back FIRED");
    }
    cycle_tool(&mut data, true)
}

fn cycle_tool(data: &mut RefAny, backward: bool) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.tool = if backward {
        s.tool.prev()
    } else {
        s.tool.next()
    };
    if !s.tool.records_audio() {
        if let Some(clip) = s.recording.take() {
            s.clips.push(clip);
        }
    }
    Update::RefreshDom
}

/// A pointer move while a stroke is drawn: its new point, and a new frame of
/// its layer - no new DOM (a DOM per move made drawing unusably slow).
pub extern "C" fn on_ink_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let timed = data.downcast_ref::<AppState>().is_some_and(|s| s.perf.is_on());
    if !timed {
        return ink_move(&mut data, &mut info);
    }
    let began = std::time::Instant::now();
    let update = ink_move(&mut data, &mut info);
    let us = u32::try_from(began.elapsed().as_micros()).unwrap_or(u32::MAX);
    let frames = info.get_frame_timings();
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.perf.moved(us);
        s.perf.frame(
            u32::try_from(frames.frame_work_us).unwrap_or(u32::MAX),
            u32::try_from(frames.frame_interval_us).unwrap_or(u32::MAX),
        );
    }
    update
}

fn ink_move(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    // Where the pen is, drawing or hovering: the dwell times its rest (and a dictation it has
    // moved away from ends - the one DOM refresh a move may ask for).
    let ended = voice::pointer(data, info, None);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.live.is_none() {
        // The pen's eraser end dragged over the page takes every stroke it crosses (a press takes the
        // one under it, `on_ink_down`).
        if let Some((p, true)) = ui::sample(info) {
            if let Some(page) = ui::page_of(info) {
                if let Some(gone) = s.eraser_target(page, p.x, p.y).and_then(|i| s.erase(i)) {
                    drop(s);
                    ui::redraw_ink(info, ink::Layer::of(gone.semantic));
                }
            }
        }
        return ended;
    }
    let Some((p, _)) = ui::sample(info) else {
        return Update::DoNothing;
    };
    let Some(live) = s.live.as_mut() else {
        return Update::DoNothing;
    };
    let far = live
        .points
        .last()
        .is_none_or(|l| (l.x - p.x).abs() + (l.y - p.y).abs() > 0.35);
    if !far {
        return ended;
    }
    live.points.push(p);
    let layer = ink::Layer::of(live.semantic);
    drop(s);
    ui::redraw_ink(info, layer);
    ended
}

/// The pen lifts: a stroke is kept (its page's frame drawn anew with it, no
/// new DOM - nothing else in the window shows the strokes), a click cycles
/// the nib.
pub extern "C" fn on_ink_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // The pen lifted: it rests in the air where it was (not a move: nothing of a dictation ends).
    voice::pointer(&mut data, &mut info, Some(false));
    let (up, timer_id, layer) = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(done) = s.live.take() else {
            return Update::DoNothing;
        };
        let layer = ink::Layer::of(done.semantic);
        if done.points.len() < CLICK_POINT_LIMIT {
            // Dropped (a click cycles the nib): the page's frame no longer shows the dab that was
            // drawn. A kept stroke is told to the page by `keep`: the frame already shows it.
            s.ink.touch(done.page);
            // A touch that rests the pen on the page while a dictation listens is no click: it must
            // not change the nib under the user's hand.
            let up = if s.voice.running.is_some() { Up::Dab } else { Up::Click };
            (up, s.idle_timer, layer)
        } else {
            s.keep(done);
            println!("AZREVIEW_STROKES {}", s.strokes.len());
            if let Some(line) = s.perf.summary() {
                println!("{line}");
            }
            (Up::Kept, s.idle_timer, layer)
        }
    };
    ui::redraw_ink(&mut info, layer);
    match up {
        Up::Click => on_cycle_tool(data, info),
        Up::Dab => Update::DoNothing,
        Up::Kept => {
            save_session(&data, &mut info);
            arm_idle_timer(&mut info, data, timer_id);
            Update::DoNothing
        }
    }
}

/// One change to the ink that Mod+Z can take back.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// A stroke was kept.
    Added(Stroke),
    /// A stroke was erased from place `index` of the file's strokes.
    Erased(Stroke, usize),
}

/// What a pen lifting did to the stroke it had down.
enum Up {
    /// A tap: it cycles the nib.
    Click,
    /// A tap while a dictation listens: nothing.
    Dab,
    /// A stroke, kept.
    Kept,
}

const ANNOTATION_IDLE_MS: u64 = 1_800;

fn arm_idle_timer(info: &mut CallbackInfo, data: RefAny, id: TimerId) {
    info.remove_timer(id);
    let timer = Timer::create(
        data,
        on_annotation_idle,
        info.get_system_time_fn(),
    )
    .with_delay(Duration::System(SystemTimeDiff::from_millis(
        ANNOTATION_IDLE_MS,
    )));
    info.add_timer(id, timer);
}

pub extern "C" fn on_annotation_idle(
    mut data: RefAny,
    mut info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let finished_clip = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return TimerCallbackReturn {
                should_update: Update::DoNothing,
                should_terminate: TerminateTimer::Terminate,
            };
        };
        s.epoch += 1;
        match s.recording.take() {
            Some(clip) => {
                s.clips.push(clip);
                s.level_samples = 0;
                true
            }
            None => false,
        }
    };
    // The strokes were kept when each ended; only a clip that just finished is news to the archive.
    if finished_clip {
        save_session(&data, &mut info.callback_info);
    }
    TimerCallbackReturn {
        // The window changes only when a recording ended (its meter goes): a rebuild of the whole window
        // 1.8 s after every stroke would land in the middle of the next one.
        should_update: if finished_clip { Update::RefreshDom } else { Update::DoNothing },
        should_terminate: TerminateTimer::Terminate,
    }
}

const CLICK_POINT_LIMIT: usize = 3;

pub extern "C" fn on_pad(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(pad) = info.get_tablet_pad().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let pressed = pad.express_keys & !s.last_pad_keys;
    s.last_pad_keys = pad.express_keys;
    if pressed == 0 {
        return Update::DoNothing;
    }
    let index = pressed.trailing_zeros() as usize;
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.pick(sem);
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// The strip scrolled sideways (a wheel, a trackpad): the page that came
/// into view lights its number, and the numbers' row keeps it in view.
pub extern "C" fn on_strip_scroll(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(x) = ui::strip_scroll_x(&info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(pages) = s.file().map(code::SourceFile::page_count) else {
        return Update::DoNothing;
    };
    let page = ui::page_at(x, pages);
    if page == s.visible_page {
        return Update::DoNothing;
    }
    s.visible_page = page;
    drop(s);
    ui::reveal_pill(&mut info, page);
    Update::RefreshDom
}

pub extern "C" fn on_jump_to_page(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(page) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    ui::scroll_to_page(&mut info, page);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.visible_page = page;
    Update::RefreshDom
}

pub extern "C" fn on_menu_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(tag) = data.downcast_ref::<ui::IndexTag>().map(|t| t.index) else {
        return Update::DoNothing;
    };
    let Some(&sem) = Semantic::ALL.get(tag) else {
        return Update::DoNothing;
    };
    let Some(mut app) = info.get_dataset(info.get_hit_node()).into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.pick(sem);
    Update::RefreshDom
}

pub extern "C" fn on_menu_save(data: RefAny, mut info: CallbackInfo) -> Update {
    save_session(&data, &mut info);
    Update::RefreshDom
}

pub extern "C" fn on_menu_reveal(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    // The sessions' folder in the data tree (it exists after the first save), opened by the
    // apps' one opener; what went wrong (no folder yet) is told over the pages.
    let dir = s.data_root.join(ABOUT.app_folder);
    match azul_appkit::files::open_external(&dir.to_string_lossy()) {
        Ok(()) => Update::DoNothing,
        Err(e) => {
            s.problem = Some(e);
            Update::RefreshDom
        }
    }
}

pub extern "C" fn on_pick_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.pick(sem);
    }
    Update::RefreshDom
}

pub extern "C" fn on_pick_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(open) = data.downcast_ref::<AppState>().map(|s| s.current) else {
        return Update::DoNothing;
    };
    if open != Some(index) {
        // The file being left is kept first (its archive is built now).
        save_session(&data, &mut info);
    }
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if open == Some(index) {
        s.sidebar_open = false;
        return Update::RefreshDom;
    }
    let Some(file) = s.files.get(index) else {
        return Update::DoNothing;
    };
    println!("AZREVIEW_FILE {}", file.display);
    s.open_file(index);
    drop(s);
    // What was marked on it in an earlier run comes back.
    load_session(&data, &mut info);
    Update::RefreshDom
}

pub extern "C" fn on_toggle_record(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let stopped = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        match s.recording.take() {
            Some(clip) => {
                s.clips.push(clip);
                s.level_samples = 0;
                true
            }
            None => false,
        }
    };
    if stopped {
        save_session(&data, &mut info);
        return Update::RefreshDom;
    }
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.recording = Some(VoiceClip {
        sample_rate: CLIP_RATE,
        ..VoiceClip::default()
    });
    Update::RefreshDom
}

pub extern "C" fn on_audio_frame(
    mut data: RefAny,
    _: CallbackInfo,
    frame: azul::audio::AudioFrame,
) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(clip) = s.recording.as_mut() else {
        return Update::DoNothing;
    };
    let peak = frame.samples.as_ref().iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    clip.samples.extend(frame.samples.as_ref().iter().copied());
    let total = clip.samples.len();
    s.level = azul::widgets::LevelMeter::peak_level(peak);

    let step = ui::METER_PACKET_SAMPLES;
    if total / step > s.level_samples / step {
        s.level_samples = total;
        return Update::RefreshDom;
    }
    s.level_samples = total;
    Update::DoNothing
}
