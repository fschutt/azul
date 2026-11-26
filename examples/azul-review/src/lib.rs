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
    app::RendererOptions,
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    option::{OptionRendererOptions, OptionString},
    prelude::*,
    window::HwAcceleration,
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

pub mod agent_api;
pub mod agent_azul;
pub mod agent_backend;
pub mod agent_claude;
#[cfg(test)]
pub mod agent_eval;
pub mod agent_export;
pub mod agent_factory;
pub mod agent_gemini;
pub mod agent_json;
pub mod agent_limits;
pub mod agent_mock;
pub mod agent_pool;
pub mod agent_process;
pub mod agent_review;
pub mod agent_settings;
pub mod agent_stroke;
pub mod census;
pub mod chips;
pub mod chips_op;
pub mod code;
pub mod agent_window;
pub mod column;
pub mod conv_ui;
pub mod conversation;
pub mod conversation_request;
pub mod deletion;
pub mod dictation;
pub mod dwell;
pub mod ids;
pub mod ink;
pub mod layout;
pub mod line_map;
pub mod margin;
pub mod notes_op;
#[cfg(test)]
mod ink_tests;
mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod model;
pub mod perf;
pub mod pens;
pub mod page_marks;
pub mod patch;
pub mod patch_store;
pub mod pointer;
pub mod progress;
pub mod progress_store;
pub mod recorder;
pub mod recorder_app;
pub mod recorder_args;
pub mod recorder_azul;
pub mod recorder_clock;
pub mod recorder_codec;
pub mod recorder_demo;
pub mod recorder_edl;
pub mod recorder_events;
pub mod recorder_hook;
pub mod recorder_export;
pub mod recorder_export_cli;
pub mod recorder_export_job;
pub mod recorder_fs;
pub mod recorder_host;
pub mod recorder_manifest;
pub mod recorder_mp4;
pub mod recorder_panel;
pub mod recorder_patch;
pub mod recorder_policy;
pub mod recorder_probe;
pub mod recorder_screen;
pub mod recorder_ui;
pub mod recorder_settings;
pub mod recorder_track;
pub mod recorder_wb;
pub mod recorder_writer;
pub mod seen_watch;
pub mod session;
pub mod slash;
pub mod speech_gate;
pub mod standby;
pub mod stroke_features;
pub mod symbols_window;
/// The pen strokes every gesture is tested on (a loop, a slash, an underline, a scribble): shared by the
/// whiteboard's lasso and the quick slash that deletes a word.
#[cfg(test)]
pub(crate) mod stroke_fixtures;
pub mod startup;
pub mod state_op;
pub mod switches;
pub mod tree;
pub mod tree_load;
pub mod symbols;
pub mod symbols_context;
pub mod symbols_manifest;
pub mod symbols_resolve;
pub mod symbols_scan;
pub mod symbols_sched;
pub mod symbols_source;
pub mod symbols_spoken;
pub mod symbols_tokens;
pub mod symbols_vocab;
pub mod ui;
pub mod vocabulary;
pub mod voice;
pub mod workspace_view;
pub mod whiteboard;
pub mod whiteboard_app;
pub mod whiteboard_archive;
pub mod whiteboard_boards;
pub mod whiteboard_boards_ui;
pub mod whiteboard_cutout;
pub mod whiteboard_demo;
pub mod whiteboard_drag;
pub mod whiteboard_geom;
pub mod whiteboard_input;
pub mod whiteboard_jump;
pub mod whiteboard_json;
pub mod whiteboard_lasso;
pub mod whiteboard_render;
pub mod whiteboard_store;
pub mod whiteboard_ui;

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
    files_help: "the folder to review (default: the one reviewed last; with none, the window offers Open folder)",
};

/// The About facts.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzReview",
    // The commit it is built from (`build.rs`): a report names its binary.
    version: concat!(env!("CARGO_PKG_VERSION"), " (build ", env!("AZREVIEW_BUILD"), ")"),
    summary: "Ink-first code review: the files of a folder as pages, marked with a pen, with \
              voice clips. Sessions are kept in your data folder.",
    license: "MIT",
    app_folder: "review",
};

/// The window's renderer: the GPU (WebRender) draws it, as mapedit3's - a page is a big picture, the ink a
/// texture, and the CPU renderer (the engine's default) paints the whole page again at every pen move. The
/// engine falls back to the CPU renderer where no GL context can be made, where the driver is on its
/// blacklist (a software GL) or WebRender cannot start; `AZ_BACKEND=cpu|gpu|auto` still wins.
#[must_use]
pub fn window_renderer() -> RendererOptions {
    let mut renderer = RendererOptions::create_default();
    renderer.hw_accel = HwAcceleration::Enabled;
    renderer
}

/// The keys AzReview answers (the kit adds Mod+, / F1 / Escape).
pub const SHORTCUTS: [Shortcut; 10] = [
    Shortcut::new("azreview-shortcuts-review", "Mod+S", "azreview-shortcut-save"),
    Shortcut::new("azreview-shortcuts-review", "Mod+O", "azreview-shortcut-open-folder"),
    Shortcut::new("azreview-shortcuts-review", "Mod+B", "azreview-shortcut-files"),
    Shortcut::new("azreview-shortcuts-review", "Mod+Shift+B", "azreview-shortcut-whiteboard"),
    Shortcut::new("azreview-shortcuts-review", "Mod+Z", "azreview-shortcut-undo"),
    Shortcut::new("azreview-shortcuts-review", "Mod+Shift+R", "azreview-shortcut-not-reviewed"),
    Shortcut::new("azreview-shortcuts-review", "Mod+Shift+C", "azreview-shortcut-continue"),
    Shortcut::new("azreview-shortcuts-review", "Pen rest", "azreview-shortcut-dictate"),
    Shortcut::new("azreview-shortcuts-review", "1 to 5", "azreview-shortcut-meaning"),
    Shortcut::new("azreview-shortcuts-review", "Right click", "azreview-shortcut-previous-nib"),
];

/// The window's size when no `--size` says otherwise, and the least it may be shrunk to (it is narrow enough for
/// the file tree to cover the pages instead of pushing them: `layout::OVERLAY_BELOW_W`).
pub const WINDOW_DEFAULT: (f32, f32) = (1280.0, 820.0);
pub const WINDOW_MIN: (f32, f32) = (560.0, 420.0);

/// How many files of the root are tried for the one that opens at the start.
const START_CANDIDATES: usize = 8;

/// The sample rate of a voice clip, Hz (the microphone widget is asked for it).
pub const CLIP_RATE: u32 = 48_000;

/// The write-back tag of a session write.
const TAG_SAVE: u64 = 1;


pub struct AppState {
    /// Where the files of the workspace are read from: the disk (the folder under review), or memory (the tests').
    pub source: std::sync::Arc<dyn symbols_source::FileSource>,
    /// What the workspace holds: its folders are listed as they are opened.
    pub tree: tree::Tree,
    /// The rows of the tree (the open folders flattened), built again when the tree changes.
    pub rows: Vec<tree::Row>,
    /// The file under review, its lines read, and its place in the tree.
    pub open: Option<code::SourceFile>,
    pub open_node: Option<tree::NodeId>,
    /// Which workspace the listings that come back belong to: a folder chosen later ends the ones under way.
    pub workspace: u64,
    /// Which steps of the start were told (`startup`).
    pub told: startup::Told,
    /// When the state was made, and how long after that the root of the workspace was listed (ms): what a script that
    /// measures the start reads (`AZREVIEW_STARTUP`).
    pub started: std::time::Instant,
    pub root_listed_ms: Option<u64>,
    /// What has been looked at: the pages seen of each file, and what the reviewer said is not reviewed.
    pub progress: progress::Book,
    /// The totals of every node of the tree (`progress::rollup`), built again when what they sum changes.
    pub rollup: Vec<progress::Totals>,
    pub rollup_dirty: bool,
    /// Some of the workspace is not known yet: a folder not listed, a file whose lines are not counted.
    pub counting: bool,
    /// The window is the one the reviewer is at: nothing is seen while it is not.
    pub window_active: bool,
    /// The seen-watcher's timer runs, and when it last looked.
    pub watching: bool,
    pub watched_at: Option<std::time::Instant>,
    /// A write of the progress file is under way (one writer); another change waits for it.
    pub progress_saving: bool,
    pub progress_save_pending: bool,
    /// When the progress file was last asked to be written.
    pub progress_saved_at: Option<std::time::Instant>,
    /// The count of the workspace is under way: setting this ends it.
    pub census_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub strokes: Vec<Stroke>,
    /// The words of the open file that quick slashes took, in the order they were (the page draws them in its own
    /// colour; nothing is written to the file).
    pub slashes: Vec<deletion::Deletion>,
    pub live: Option<Stroke>,
    /// The clock of the live stroke: its samples are stamped with the time since the press.
    pub live_clock: Option<std::time::Instant>,
    /// The revisions of each page's kept strokes: an ink layer draws its
    /// frame anew when its page's changes (`ink::InkRevisions`).
    pub ink: ink::InkRevisions,
    pub next_stroke_id: u64,
    /// The five pens: which is in hand and the nib each has.
    pub pens: pens::Pens,
    /// The conversations of the folder's files (the bottom bar shows the open file's; the tree marks each file).
    pub chips: chips::Chips,
    /// Where what happens in the window is told to a recorder (nobody until a recorder is mounted).
    pub recorder: std::sync::Arc<dyn recorder_hook::RecorderHook>,
    pub findings: Vec<Finding>,
    /// The notes dictated at spots of the open file, each the first message of a conversation (the margins of the pages,
    /// the markers on them).
    pub notes: Vec<Note>,
    pub next_note_id: u64,
    /// The conversation that is open as a popover in the margin of its page.
    pub open_conv: Option<u64>,
    /// The bottom bar shows every finished conversation (it folds the older ones into a count otherwise).
    pub bar_all: bool,
    /// The agents: the settings, the pool of jobs, the answers that wait for a file that is not open.
    pub hub: agent_window::Hub,
    /// The symbol index of the workspace, built behind the window: what the dictation listens for and what the names the
    /// reviewer says are.
    pub symbols: symbols_window::Hub,
    /// The index is to be built (`--symbols`, the setting `symbols-index`); a window of a test has none.
    pub symbols_enabled: bool,
    /// The status sentence of the scan the tree's foot shows now (so that it is built again only when it changes).
    pub symbols_shown: Option<symbols_sched::Progress>,
    /// When the tree's foot last said it.
    pub symbols_shown_at: Option<std::time::Instant>,
    /// The scan was said to be done (`AZREVIEW_SYMBOLS ready`) for this workspace.
    pub symbols_told: bool,
    /// What Mod+Z takes back, newest last, and what Mod+Shift+Z brings back.
    pub history: Vec<Edit>,
    pub redo_stack: Vec<Edit>,
    /// The pen's rest and the dictation it starts (the side column, the markers).
    pub voice: voice::Voice,
    /// AzReview's own switches of this run (they win over the settings file).
    pub switches: switches::Switches,
    /// The ink's own timings (`AZREVIEW_PROFILE=1`).
    pub perf: perf::Perf,
    /// The last pointer events (`AZREVIEW_EVENTS=1`, the Session menu, the debug server).
    pub pointer_log: pointer::PointerLog,
    /// The folder dialog is open: a second request (the menu's key and the key handler both hear Mod+O)
    /// opens no second one.
    pub choosing_folder: bool,
    /// The stroke that is live was begun by a pen (not a mouse or a trackpad).
    pub live_pen: bool,
    /// A folder named on the command line, to be remembered as the one reviewed last once the window exists.
    pub remember_folder: Option<PathBuf>,
    pub recording: Option<VoiceClip>,
    pub clips: Vec<VoiceClip>,
    pub level_samples: usize,
    /// The level of the last packet of voice, 0 to 1 (the recording's meter).
    pub level: f32,
    pub visible_page: usize,
    pub epoch: u64,
    pub idle_timer: TimerId,
    pub root: PathBuf,
    /// What is open and how wide: the file tree (the whole height of the window, it pushes the pages to the
    /// right) and the right panel; remembered in the settings.
    pub panels: layout::Panels,
    /// How far each panel has slid.
    pub slide: layout::Slide,
    /// A drag of a panel's edge.
    pub grip: Option<layout::Grip>,
    /// The slide's timer runs, and the clock (ms) of its last tick.
    pub sliding: bool,
    pub slide_at_ms: u64,
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
    /// The session recorder (the screen, the camera, the voice) and its window; made at the start. What the review tells it
    /// goes through [`Self::recorder`], the hook, which reaches the recording that is going. (Not `recording`: that is the
    /// voice clip being made.)
    pub session_recorder: Option<recorder_ui::RecorderApp>,
}

impl AppState {
    /// AzReview on `files` held in memory (the tests'), the first open and the file tree beside the pages; `kit` and
    /// `data_root` are azul-appkit's. The app of a folder on a disk is [`Self::on_disk`].
    #[must_use]
    pub fn create(root: PathBuf, files: Vec<code::SourceFile>, kit: RefAny, data_root: PathBuf) -> Self {
        let (source, tree) = memory_workspace(&files);
        let mut state = Self::with_source(root, std::sync::Arc::new(source), tree, kit, data_root);
        if let Some(id) = state.tree.first_file() {
            let path = state.tree.path(id);
            if let Some(file) = files.into_iter().find(|f| f.display == path) {
                state.open = Some(file);
                state.open_node = Some(id);
            }
        }
        state.refresh_rows();
        state.refresh_rollup();
        state
    }

    /// AzReview on the folder `root` of a disk: the root is listed now (one `read_dir`), the first file of it is
    /// open, every other folder is listed when it is opened.
    #[must_use]
    pub fn on_disk(root: PathBuf, kit: RefAny, data_root: PathBuf) -> Self {
        let mut state = Self::with_source(root.clone(), std::sync::Arc::new(symbols_source::MemSource::new()), tree::Tree::new(), kit, data_root);
        if !root.as_os_str().is_empty() {
            state.open_workspace(root);
        }
        state
    }

    fn with_source(
        root: PathBuf,
        source: std::sync::Arc<dyn symbols_source::FileSource>,
        tree: tree::Tree,
        kit: RefAny,
        data_root: PathBuf,
    ) -> Self {
        Self {
            source,
            tree,
            rows: Vec::new(),
            open: None,
            open_node: None,
            workspace: 0,
            told: startup::Told::default(),
            started: std::time::Instant::now(),
            root_listed_ms: None,
            progress: progress::Book::default(),
            rollup: Vec::new(),
            rollup_dirty: true,
            counting: true,
            window_active: true,
            watching: false,
            watched_at: None,
            progress_saving: false,
            progress_save_pending: false,
            progress_saved_at: None,
            census_stop: None,
            strokes: Vec::new(),
            slashes: Vec::new(),
            live: None,
            live_clock: None,
            ink: ink::InkRevisions::default(),
            next_stroke_id: 1,
            pens: pens::Pens::new(),
            chips: chips::Chips::default(),
            recorder: recorder_hook::none(),
            findings: Vec::new(),
            notes: Vec::new(),
            next_note_id: 1,
            open_conv: None,
            bar_all: false,
            symbols: symbols_window::Hub::none(),
            symbols_enabled: false,
            symbols_shown: None,
            symbols_shown_at: None,
            symbols_told: false,
            hub: agent_window::Hub::new(&std::collections::BTreeMap::new(), Vec::new(), Path::new(""), None, switches::DEFAULT_AGENT_DELAY_MS, false),
            history: Vec::new(),
            redo_stack: Vec::new(),
            voice: voice::Voice::new(dwell::Config::default(), dictation::DEFAULT_SILENCE_MS),
            switches: switches::Switches::default(),
            perf: perf::Perf::new(false),
            pointer_log: pointer::PointerLog::default(),
            choosing_folder: false,
            live_pen: false,
            remember_folder: None,
            recording: None,
            clips: Vec::new(),
            level_samples: 0,
            level: 0.0,
            visible_page: 0,
            epoch: 0,
            idle_timer: TimerId::unique(),
            root,
            panels: layout::Panels::default(),
            slide: layout::Slide::settled(&layout::Panels::default()),
            grip: None,
            sliding: false,
            slide_at_ms: 0,
            problem: None,
            last_pad_keys: 0,
            kit,
            data_root,
            saving: false,
            save_pending: false,
            session_recorder: None,
        }
    }

    /// The folder dialog may open: `false` while one is open already. Pair with [`Self::folder_dialog_closed`].
    pub fn ask_for_folder(&mut self) -> bool {
        if self.choosing_folder {
            return false;
        }
        self.choosing_folder = true;
        true
    }

    /// The folder dialog was answered (a folder, or none).
    pub fn folder_dialog_closed(&mut self) {
        self.choosing_folder = false;
    }

    /// Opens the file at `path` of the workspace (its pages start without ink); the tree stays as it is (a pick does
    /// not fold it). `false`: there is no such file in the tree, or it is not shown (`problem` says why).
    pub fn open_path(&mut self, path: &str) -> bool {
        let Some(id) = self.tree.find(path).filter(|&id| self.tree.kind(id) == Some(tree::Kind::File)) else {
            return false;
        };
        let read_at = std::time::Instant::now();
        let file = match self.read_file(path) {
            Ok(file) => file,
            Err(why) => {
                self.problem = Some(not_shown(path, &why));
                return false;
            }
        };
        let line_count = file.line_count();
        // how long reading a file takes, for the scripts that measure it (and for a report of one that opens slowly)
        println!(
            "AZREVIEW_OPEN {path} lines={line_count} held_as={} read_ms={}",
            if file.big.is_some() { "text" } else { "lines" },
            read_at.elapsed().as_millis()
        );
        self.begin_file(Some(id));
        self.tree.set_lines(id, u32::try_from(line_count).unwrap_or(u32::MAX));
        self.open = Some(file);
        // the files near the one shown are indexed next
        self.symbols.set_focus(path);
        self.rederive();
        self.say(
            "nav.file",
            &crate::recorder_events::Fields::new()
                .string("file", path)
                .int("lines", i64::try_from(line_count).unwrap_or(i64::MAX)),
        );
        true
    }

    /// The file at `path`, read from the workspace's source: held as lines, or as its text when it is big.
    fn read_file(&self, path: &str) -> Result<code::SourceFile, code::NotShown> {
        match self.source.read(path, code::MAX_OPEN_BYTES) {
            Ok(bytes) => code::open_bytes(self.root.join(path), path.to_string(), bytes),
            Err(symbols_source::ReadError::TooLarge(bytes)) => Err(code::NotShown::TooLarge(bytes)),
            Err(other) => Err(code::NotShown::Unreadable(format!("{other:?}"))),
        }
    }

    /// Reviews the folder `root` of a disk: the files of the folder replace the tree, its root is listed and the first
    /// file of it is open (a folder without files shows the empty page, which has the button).
    pub fn open_folder(&mut self, root: PathBuf) {
        self.open_workspace(root);
    }

    /// Reviews the workspace of `self.source`, which belongs to `root`; the folder dialog's answer, and the start.
    fn open_workspace(&mut self, root: PathBuf) {
        self.root = root.clone();
        self.source = std::sync::Arc::new(symbols_source::DiskSource::new(root));
        self.start_workspace();
    }

    /// The tree starts afresh from the source's root: listed now, the first file open.
    fn start_workspace(&mut self) {
        self.workspace += 1;
        if let Some(old) = self.census_stop.take() {
            old.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.progress = progress::Book::default();
        self.tree = tree::Tree::new();
        self.chips = chips::Chips::default();
        self.begin_file(None);
        let root = self.tree.root();
        match self.source.list("") {
            Ok(entries) => {
                let listed = self.tree.list(root, &entries, &tree::Rules::default());
                if listed.capped {
                    eprintln!("[AzReview] the folder has more entries than the tree keeps: the rest is not shown");
                }
            }
            Err(why) => eprintln!("[AzReview] the folder could not be listed: {why:?}"),
        }
        self.refresh_rows();
        self.root_listed_ms = Some(u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX));
        // The first file of the root that can be shown opens; one that cannot (no text, too large) is passed over
        // without a word: the reviewer did not ask for it.
        let candidates: Vec<String> = self
            .tree
            .children(self.tree.root())
            .iter()
            .copied()
            .filter(|&id| self.tree.kind(id) == Some(tree::Kind::File))
            .take(START_CANDIDATES)
            .map(|id| self.tree.path(id))
            .collect();
        for path in candidates {
            if self.open_path(&path) {
                break;
            }
            self.problem = None;
        }
        self.rederive();
        self.refresh_rollup();
        if self.symbols_enabled {
            self.start_symbols();
        }
    }

    /// The scan of the symbol index moved on since the tree's foot last said how far it is (at most once a second, and
    /// when it is done): the foot says it again.
    pub fn symbols_line_changed(&mut self) -> bool {
        let progress = self.symbols.progress();
        // the scan is done: a script reads when (once for each workspace)
        if let Some(p) = progress.filter(|p| p.idle) {
            if !self.symbols_told {
                self.symbols_told = true;
                println!("AZREVIEW_SYMBOLS ready found={} done={} dirs={}", p.files_found, p.files_done, p.dirs_done);
            }
        }
        let now = progress.filter(|p| symbols_window::status_of(p).is_some());
        if now == self.symbols_shown {
            return false;
        }
        let due = now.is_none() || self.symbols_shown_at.is_none_or(|t| t.elapsed().as_millis() >= 1_000);
        if due {
            self.symbols_shown = now;
            self.symbols_shown_at = Some(std::time::Instant::now());
        }
        due
    }

    /// Builds the symbol index of the workspace behind the window, outwards from the file that is open.
    pub fn start_symbols(&mut self) {
        let open = self.file().map(|f| f.display.clone());
        self.symbols.start(self.source.clone(), open.as_deref());
        self.symbols_shown = None;
        self.symbols_told = false;
    }

    // ---- what has been looked at ----

    /// What the file `id` adds to the totals above it: its lines, the lines on the pages seen, whether it is reviewed
    /// (every page seen, no conversation of it working or waiting, and the reviewer has not said otherwise).
    fn file_info(&self, id: tree::NodeId) -> progress::FileInfo {
        let Some(node) = self.tree.node(id).filter(|n| n.kind == tree::Kind::File) else {
            return progress::FileInfo::default();
        };
        let path = self.tree.path(id);
        let entry = self.progress.get(&path);
        let seen = entry.map_or(0, |e| e.seen.count());
        let held = entry.is_some_and(|e| e.held);
        let pages = node.lines.map_or(0, |l| (l as usize).div_ceil(code::LINES_PER_PAGE).max(1));
        progress::FileInfo {
            lines: node.lines,
            lines_seen: node
                .lines
                .map_or(0, |l| u32::try_from(entry.map_or(0, |e| e.seen.lines_seen(l as usize))).unwrap_or(u32::MAX)),
            reviewed: node.lines.is_some() && progress::is_reviewed(seen, pages, self.chips.counts(&path), held),
        }
    }

    /// Builds the totals of every node again.
    pub fn refresh_rollup(&mut self) {
        let totals = progress::rollup(&self.tree, &|id| self.file_info(id));
        self.counting = totals.get(self.tree.root()).is_some_and(progress::Totals::is_partial)
            || !self.tree.unlisted_dirs().is_empty();
        self.rollup = totals;
        self.rollup_dirty = false;
    }

    /// The totals of a node (a file's own, a folder's, the root's those of the workspace).
    #[must_use]
    pub fn totals_of(&self, id: tree::NodeId) -> progress::Totals {
        self.rollup.get(id).copied().unwrap_or_default()
    }

    /// The mark the tree shows for a file: blue while an agent works on it, yellow while a suggestion waits, green when it
    /// is reviewed; nothing otherwise.
    #[must_use]
    pub fn file_mark(&self, id: tree::NodeId) -> Option<chips::State> {
        match self.chips.file_state(&self.tree.path(id)) {
            Some(chips::State::Working) => Some(chips::State::Working),
            Some(chips::State::Suggested) => Some(chips::State::Suggested),
            _ => self.rollup.get(id).filter(|t| t.files_reviewed == 1).map(|_| chips::State::Done),
        }
    }

    /// Time passes (`elapsed_ms`) with the strip of pages scrolled `scroll_x` in a viewport `view_w` wide: the pages in
    /// view long enough are seen. Whether a page became seen.
    pub fn watch_pages(&mut self, scroll_x: f32, view_w: f32, elapsed_ms: u32) -> bool {
        if !self.window_active {
            return false;
        }
        let Some(file) = self.file() else {
            return false;
        };
        let (path, total) = (file.display.clone(), file.page_count());
        let in_view = progress::pages_in_view(scroll_x, view_w, ui::PAGE_W, ui::page_stride(), total);
        let Some(entry) = self.progress.entry(&path) else {
            return false;
        };
        let changed = entry.seen.watch(&in_view, elapsed_ms);
        if changed {
            self.progress.set_dirty();
            self.rollup_dirty = true;
        }
        changed
    }

    /// A page of the open file carries ink: it is seen.
    pub fn mark_page_seen(&mut self, page: usize) {
        let Some(path) = self.file().map(|f| f.display.clone()) else {
            return;
        };
        if self.progress.entry(&path).is_some_and(|e| e.seen.mark(page)) {
            self.progress.set_dirty();
            self.rollup_dirty = true;
        }
    }

    /// The reviewer says the file `path` is not reviewed (or says it is again): whatever its pages say, it is not until then.
    /// The new answer (`true`: held back).
    pub fn toggle_held(&mut self, path: &str) -> bool {
        let Some(entry) = self.progress.entry(path) else {
            return false;
        };
        entry.held = !entry.held;
        let held = entry.held;
        self.progress.set_dirty();
        self.refresh_rollup();
        held
    }

    /// Conversations changed: what is reviewed may have.
    pub fn chips_changed(&mut self) {
        self.rollup_dirty = true;
    }

    /// Makes the chip of conversation `id` of the open file say what the conversation says: the bottom bar has one chip
    /// for each, the tree marks the file by them. A conversation that is gone takes its chip away.
    pub fn sync_chip(&mut self, id: u64) {
        let Some(name) = self.file().map(|f| f.display.clone()) else {
            return;
        };
        let chip = self.notes.iter().find(|n| n.id == id).map(|n| {
            let first = code::SourceFile::first_line_of(n.page);
            chips::Chip {
                id,
                file: name.clone(),
                page: n.page,
                line: u32::try_from(first + ui::line_of(n.y)).unwrap_or(u32::MAX),
                state: n.talk.chip_state(),
                title: chips::short_title(&n.text, 28),
            }
        });
        match chip {
            Some(chip) => match self.chips.upsert(chip) {
                chips::Pushed::Kept => {}
                chips::Pushed::DroppedOldest(word) | chips::Pushed::Refused(word) => eprintln!("[AzReview] {word}"),
            },
            None => {
                self.chips.remove(&name, id);
            }
        }
        self.chips_changed();
    }

    /// Every conversation of the open file has its chip (a file was opened, an archive came back).
    pub fn sync_chips(&mut self) {
        let ids: Vec<u64> = self.notes.iter().map(|n| n.id).collect();
        for id in ids {
            self.sync_chip(id);
        }
    }

    /// The answers that came for the open file while another was open land now.
    pub fn land_late(&mut self) {
        let Some(name) = self.file().map(|f| f.display.clone()) else {
            return;
        };
        let now = agent_window::unix_now_ms();
        // An agent that is still at work on a conversation of this file, whose archive said it was working when the file
        // was left, is working: not a failure of a window that closed.
        for note in &mut self.notes {
            if self.hub.is_working_on(&name, note.id) && note.talk.phase == conversation::Phase::Failed {
                note.talk.phase = conversation::Phase::Working;
                note.talk.problem = None;
            }
        }
        for late in self.hub.take_late(&name) {
            if let Some(note) = self.notes.iter_mut().find(|n| n.id == late.note) {
                agent_window::land(note, late.outcome, now);
            }
        }
        self.sync_chips();
    }

    /// The strokes an agent is told of for conversation `id`: the ink near its anchor (the stroke samples come with the
    /// request's strokes; none are sent yet).
    #[must_use]
    pub fn strokes_for_request(&self, _id: u64) -> Vec<agent_stroke::StrokeInput> {
        Vec::new()
    }

    /// Whether the workspace has anything to show in the tree.
    #[must_use]
    pub fn has_entries(&self) -> bool {
        !self.tree.children(self.tree.root()).is_empty()
    }

    /// Lists every folder that was not listed and opens all of them, on this thread; how many folders were listed. For a
    /// script that measures a whole tree (`azreview_expand_all`): the app lists a folder when the reviewer opens it.
    pub fn expand_all(&mut self) -> usize {
        let mut listed = 0;
        loop {
            let pending = self.tree.unlisted_dirs();
            if pending.is_empty() || listed >= tree::MAX_NODES {
                break;
            }
            for id in pending {
                let path = self.tree.path(id);
                match self.source.list(&path) {
                    Ok(entries) => {
                        self.tree.list(id, &entries, &tree::Rules::default());
                        listed += 1;
                    }
                    // A folder that cannot be listed is listed as empty: the loop must end.
                    Err(_) => {
                        self.tree.list(id, &[], &tree::Rules::default());
                    }
                }
            }
        }
        self.tree.expand_all();
        self.refresh_rows();
        listed
    }

    /// The rows of the tree, built again (after a folder was opened, closed or listed).
    pub fn refresh_rows(&mut self) {
        self.rows = self.tree.rows();
    }

    /// A click on the folder `id`: it opens or closes. The job that lists it when it was never listed, for the caller to
    /// run on a thread (`tree_load::spawn`).
    pub fn toggle_dir(&mut self, id: tree::NodeId) -> Option<tree_load::ListJob> {
        let job = match self.tree.toggle(id) {
            tree::Toggle::Opened { needs_listing: true } if self.tree.node(id).is_some() => {
                self.tree.set_listing(id, true);
                Some(tree_load::ListJob {
                    source: self.source.clone(),
                    dir: self.tree.path(id),
                    workspace: self.workspace,
                })
            }
            _ => None,
        };
        self.refresh_rows();
        job
    }

    /// A folder was counted by the walk of the workspace `workspace`: its listing and the lines of its files go into the
    /// tree. Whether the rows changed (a folder that is open was listed).
    pub fn take_census(&mut self, workspace: u64, batch: census::Batch) -> bool {
        if workspace != self.workspace {
            return false;
        }
        let Some(dir) = self.tree.find(&batch.dir) else {
            return false;
        };
        self.tree.list(dir, &batch.entries, &tree::Rules::default());
        for (name, lines) in &batch.lines {
            let path = if batch.dir.is_empty() { name.clone() } else { format!("{}/{name}", batch.dir) };
            if let Some(id) = self.tree.find(&path) {
                self.tree.set_lines(id, *lines);
            }
        }
        self.rollup_dirty = true;
        let rows = self.tree.rows();
        let changed = rows != self.rows;
        self.rows = rows;
        changed
    }

    /// A listing of the folder `dir` of workspace `workspace` came back; whether the tree changed (a listing of a
    /// workspace that is gone changes nothing).
    pub fn take_listing(
        &mut self,
        workspace: u64,
        dir: &str,
        result: Result<Vec<symbols_source::DirEntry>, symbols_source::ReadError>,
    ) -> bool {
        if workspace != self.workspace {
            return false;
        }
        let Some(id) = self.tree.find(dir) else {
            return false;
        };
        match result {
            Ok(entries) => {
                let listed = self.tree.list(id, &entries, &tree::Rules::default());
                if listed.capped {
                    eprintln!("[AzReview] {dir}: more entries than the tree keeps, the rest is not shown");
                }
            }
            Err(why) => {
                eprintln!("[AzReview] {dir}: it could not be listed: {why:?}");
                self.tree.set_listing(id, false);
            }
        }
        self.refresh_rows();
        true
    }

    /// What starts afresh with another file: no ink, no notes, nothing to undo, no dictation (what
    /// listens writes into the file being left: it ends, and its words are kept there).
    fn begin_file(&mut self, node: Option<tree::NodeId>) {
        self.voice.request_stop(dictation::Cause::File);
        self.voice.pending = None;
        // What nobody works on or waits on is in the file's archive and comes back with it; an agent at work, and a
        // suggestion that waits, keep their chips (the tree marks the file) and their answers wait for the file.
        if let Some(left) = self.file().map(|f| f.display.clone()) {
            self.chips.remove_resting(&left);
        }
        self.open_conv = None;
        self.open_node = node;
        self.open = None;
        self.strokes.clear();
        self.slashes.clear();
        self.notes.clear();
        self.history.clear();
        self.redo_stack.clear();
        self.live = None;
        self.live_clock = None;
        self.ink.open();
        self.visible_page = 0;
    }

    /// Tells the recorder (one call, the recorder's vocabulary).
    fn say(&self, kind: &str, fields: &crate::recorder_events::Fields) {
        self.recorder.event(kind, &fields.build());
    }

    /// The pen in hand, as the recorder says it.
    fn say_pen(&self) {
        let nib = match self.pens.tool() {
            Tool::Marker => "marker",
            Tool::Pen => "pen",
            Tool::AudioPen => "audio",
        };
        self.say(
            "pen.select",
            &crate::recorder_events::Fields::new().string("meaning", self.pens.semantic().label()).string("nib", nib),
        );
    }

    /// Takes the pen that writes `sem`: the pen lit in the top bar is what the next stroke is (each pen
    /// has the nib it was left with).
    pub fn pick(&mut self, sem: Semantic) {
        if let Some(index) = pens::pen_of(sem) {
            self.select_pen(index);
        }
    }

    /// Takes pen `index` (a click on it).
    pub fn select_pen(&mut self, index: usize) {
        if self.pens.select(index) {
            self.nib_changed();
            self.say_pen();
        }
    }

    /// The key of pen `index`: the pen in hand has its next nib, another pen is taken.
    pub fn press_pen_key(&mut self, index: usize) -> pens::Pressed {
        let pressed = self.pens.press_key(index);
        if pressed != pens::Pressed::Ignored {
            self.nib_changed();
            self.say_pen();
        }
        pressed
    }

    /// A double click on pen `index`: it is taken and has its next nib.
    pub fn double_click_pen(&mut self, index: usize) {
        if index < pens::PENS.len() {
            self.pens.double_click(index);
            self.nib_changed();
            self.say_pen();
        }
    }

    /// The pen in hand has its next (or its previous) nib.
    pub fn cycle_nib(&mut self, backward: bool) {
        self.pens.cycle(backward);
        self.nib_changed();
        self.say_pen();
    }

    /// Opens or closes a panel (the tree, the right panel); the recorder is told when it changed.
    pub fn set_panel_open(&mut self, panel: layout::Panel, open: bool) {
        let slot = match panel {
            layout::Panel::Tree => &mut self.panels.tree_open,
            layout::Panel::Right => &mut self.panels.right_open,
        };
        if *slot == open {
            return;
        }
        *slot = open;
        self.say(
            "layout.toggle",
            &crate::recorder_events::Fields::new()
                .string("panel", if panel == layout::Panel::Tree { "tree" } else { "whiteboard" })
                .flag("open", open),
        );
    }

    /// The conversations of the open file, in the order they began.
    #[must_use]
    pub fn open_chips(&self) -> Vec<&chips::Chip> {
        self.file().map_or_else(Vec::new, |f| self.chips.of_file(&f.display))
    }

    /// The meaning the next stroke has: the meaning of the pen in hand.
    #[must_use]
    pub fn ink_meaning(&self) -> Semantic {
        self.pens.semantic()
    }

    /// The nib of the pen in hand.
    #[must_use]
    pub fn tool(&self) -> Tool {
        self.pens.tool()
    }

    /// What follows from the nib in hand being another: a clip that records leaves with the audio nib.
    pub fn nib_changed(&mut self) {
        if !self.pens.tool().records_audio() {
            if let Some(clip) = self.recording.take() {
                self.clips.push(clip);
            }
        }
    }

    /// Keeps a finished stroke (what Mod+Z takes back); a new stroke ends what Mod+Shift+Z would have
    /// brought back.
    pub fn keep(&mut self, stroke: Stroke) {
        self.mark_page_seen(stroke.page);
        self.ink.appended(stroke.page, stroke.id);
        self.history.push(Edit::Added(stroke.clone()));
        self.redo_stack.clear();
        self.strokes.push(stroke);
        self.rederive();
    }

    /// Keeps a slash's deletion (what Mod+Z takes back): the words it took are drawn in the colour of the page from now
    /// on. The stroke itself is not kept as ink; the clip an audio pen records forgets its id.
    pub fn delete_words(&mut self, deletion: deletion::Deletion) {
        if let Some(clip) = self.recording.as_mut() {
            clip.stroke_ids.retain(|id| *id != deletion.id);
        }
        self.mark_page_seen(deletion.page);
        self.ink.touch(deletion.page);
        self.history.push(Edit::Slashed(deletion.clone()));
        self.redo_stack.clear();
        self.slashes.push(deletion);
    }

    /// The byte ranges of each line of `page` that slashes took, for the page to draw in its own colour.
    #[must_use]
    pub fn hidden_ranges(&self, page: usize) -> Vec<Vec<(usize, usize)>> {
        match self.file() {
            Some(file) if !self.slashes.is_empty() => {
                let (first_line, lines) = file.page(page);
                deletion::hidden_ranges(&self.slashes, first_line, &lines)
            }
            _ => Vec::new(),
        }
    }

    /// Where the reviewer is: the page in view of the open file, and its first line.
    #[must_use]
    pub fn review_place(&self) -> Option<progress::Back> {
        let file = self.file()?;
        Some(progress::Back {
            file: file.display.clone(),
            page: self.visible_page,
            line: u32::try_from(code::SourceFile::first_line_of(self.visible_page)).unwrap_or(u32::MAX),
        })
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
            Edit::Slashed(deletion) => {
                self.slashes.retain(|d| d.id != deletion.id);
                self.ink.touch(deletion.page);
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
            Edit::Slashed(deletion) => {
                self.slashes.push(deletion.clone());
                self.ink.touch(deletion.page);
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
        for deletion in &mut self.slashes {
            deletion.id += stroke_base;
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
        let mut slashes = loaded.slashes;
        slashes.append(&mut self.slashes);
        self.slashes = slashes;
        let mut notes = loaded.notes;
        notes.append(&mut self.notes);
        self.notes = notes;
        let pages: std::collections::BTreeSet<usize> =
            self.strokes.iter().map(|s| s.page).chain(self.slashes.iter().map(|d| d.page)).collect();
        for page in pages {
            self.ink.touch(page);
            self.mark_page_seen(page);
        }
        self.rederive();
        // The conversations that came back have their chips again, and the answers that came while the file was away
        // land in them.
        self.land_late();
        true
    }

    pub(crate) fn file(&self) -> Option<&code::SourceFile> {
        self.open.as_ref()
    }

    /// Where the lines of `page` are drawn: the lines the suggestions of the conversations would put in (the ones that wait
    /// and the ones that were taken) are rows of the page, which grows by them; nothing else moves (page breaks are stable).
    /// A page nobody put lines in is plain.
    pub(crate) fn page_map(&self, page: usize) -> line_map::LineMap {
        let count = self.file().map_or(0, |f| f.lines_on_page(page));
        if count == 0 || !self.notes.iter().any(|n| !n.talk.proposals.is_empty()) {
            return line_map::LineMap::new(count, Vec::new(), ui::PAGE_PAD, ui::LINE_H);
        }
        let inserts = page_marks::inserts_of(&self.notes, code::SourceFile::first_line_of(page), count);
        line_map::LineMap::from_inserts(code::SourceFile::first_line_of(page), count, &inserts, ui::PAGE_PAD, ui::LINE_H)
    }

    /// How many rows the tallest page has grown by: the strip is as high as that page.
    pub(crate) fn most_extra_rows(&self) -> usize {
        let Some(file) = self.file() else {
            return 0;
        };
        page_marks::extra_rows_by_page(&self.notes, |page| file.lines_on_page(page)).values().copied().max().unwrap_or(0)
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
    startup::phase("main");
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
    startup::phase("args");
    let named_root = args.files.first().cloned();
    // The width the window starts at (`--size`, else the default): whether the file tree starts open depends on it.
    let start_w = args.size.map_or(WINDOW_DEFAULT.0, |size| size.0);
    // A `--shot` run is a fixture: it never takes the folder the user last reviewed.
    let remember = args.shot.is_none();
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &["dictation"], args);
    startup::phase("kit");
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
    // The folder named on the command line; else the one reviewed last; else none (the empty page offers
    // Open folder). Never the folder the process happens to be started in.
    let root = switches::pick_root(
        named_root.clone(),
        values.get(switches::KEY_FOLDER).map(String::as_str),
        remember,
        Path::is_dir,
    );
    // A folder named on the command line is the one reviewed last from now on (not for a fixture run).
    let remembered = named_root.filter(|_| remember && !root.as_os_str().is_empty());
    // The root is listed now (one `read_dir`) and the first file of it opened; every other folder when it is opened.
    let mut state = AppState::on_disk(root, kit_ref.clone(), data_root);
    if let Some(first) = state.file() {
        println!("AZREVIEW_FILE {}", first.display);
    }
    startup::phase("state");
    state.remember_folder = remembered;
    let (panels, panel_notes) = layout::Panels::remembered(&|key| values.get(key).cloned());
    for note in &panel_notes {
        eprintln!("[AzReview] {note}");
    }
    let remembered_open = values.contains_key(layout::KEY_TREE_OPEN).then_some(panels.tree_open);
    state.panels = layout::Panels {
        tree_open: layout::initial_tree_open(remembered_open, start_w) && state.has_entries(),
        ..panels
    };
    state.slide = layout::Slide::settled(&state.panels);
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
    state.voice.endpoint_ms = resolved.endpoint_ms;
    state.voice.arming = standby::Arming::new(standby::Config {
        standby: resolved.standby,
        idle_ms: resolved.standby_idle_ms,
    });
    // The session cannot open the microphone ahead of a dictation yet: the policy runs only under the e2e server, where
    // what it asks is counted and no microphone is opened.
    state.voice.can_arm = std::env::var_os("AZ_E2E").is_some();
    state.voice.start_standby();
    // The agents of this run: the settings file's rows, the environment as the process has it, the backend a switch names.
    state.symbols_enabled = resolved.symbols;
    if resolved.symbols {
        state.start_symbols();
    }
    let agent_data = state.data_root.clone();
    state.hub = agent_window::Hub::new(
        &values,
        std::env::vars(),
        &agent_data,
        resolved.agent,
        resolved.agent_delay_ms,
        resolved.agent_auto,
    );
    for problem in &state.hub.problems {
        eprintln!("[AzReview] agent settings: {}", problem.message());
    }
    state.switches = own;
    state.perf = perf::Perf::new(std::env::var_os("AZREVIEW_PROFILE").is_some());
    // The speech session of the window (the chain of recognisers, its vocabulary, the call a stop ends) is made on a thread
    // once the window is there (`voice::spawn_speech_session`): 25 to 95 ms the first window need not wait for.
    state.pointer_log = pointer::PointerLog::new(pointer::Echo::parse(
        std::env::var("AZREVIEW_EVENTS").ok().as_deref(),
    ));
    if std::env::var_os("AZ_DEBUG").is_some() || std::env::var_os("AZ_E2E").is_some() {
        pointer::register_ops(&mut config);
        chips_op::register_ops(&mut config);
        notes_op::register_ops(&mut config);
        state_op::register_ops(&mut config);
    }
    // The session recorder: its settings from the settings file (what was capped is said), the machine's facts once.
    let recorded = recorder_settings::resolve(&values);
    for note in &recorded.notes {
        eprintln!("[AzReview] {note}");
    }
    let recordings = state.data_root.join(ABOUT.app_folder).join("recordings");
    let mut recorder = recorder_app::host(recorded.settings, &recordings, &state.root, '.');
    let recovered = recorder.recover();
    if recovered > 0 {
        println!("AZREVIEW_RECORDINGS_RECOVERED {recovered}");
    }
    recorder.refresh();
    // What the review tells a recorder reaches the recording that is going (and nobody when none is).
    state.recorder = std::sync::Arc::new(recorder.hook());
    state.session_recorder = Some(recorder_ui::RecorderApp::new(recorder));
    let app = App::create(RefAny::new(state), config);
    startup::phase("app_created");
    let mut window =
        kit::window_options(&kit_ref, ui::layout, WINDOW_DEFAULT, WINDOW_MIN, on_window_created);
    window.renderer = OptionRendererOptions::Some(window_renderer());
    startup::phase("run_called");
    app.run(window);
}

/// Runs `f` on the session recorder in the app's state; whether the app has one.
fn with_recorder(app: &mut RefAny, f: &mut dyn FnMut(&mut recorder_ui::RecorderApp)) -> bool {
    let Some(mut state) = app.downcast_mut::<AppState>() else { return false };
    match state.session_recorder.as_mut() {
        Some(session_recorder) => {
            f(session_recorder);
            true
        }
        None => false,
    }
}

/// The recorder's window: what the recorder says, the way the review's other windows are laid out.
extern "C" fn layout_recorder(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    recorder_ui::hud(&recorder_ui::Host { app: data, with: with_recorder }, &info)
}

/// Session > Recording...: the recorder's window.
pub extern "C" fn on_menu_recorder(data: RefAny, mut info: CallbackInfo) -> Update {
    recorder_ui::open(&recorder_ui::Host { app: data, with: with_recorder }, &mut info, layout_recorder);
    Update::DoNothing
}

/// The kit's handle, out of the app's state.
fn kit_of(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<AppState>().map(|s| s.kit.clone())
}

/// The window exists: azul-appkit's `--shot` timer.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(kit_ref) = kit_of(&mut data) {
        kit::on_window_created(&kit_ref, &mut info);
        // A folder named on the command line is the one reviewed last from now on.
        let named = data
            .downcast_mut::<AppState>()
            .and_then(|mut s| s.remember_folder.take());
        if let Some(folder) = named {
            kit::set_value(&kit_ref, &mut info, switches::KEY_FOLDER, &folder.to_string_lossy());
        }
    }
    startup::phase("window_created");
    // How long the start took, for the scripts that measure it (and for a report of a folder that opens slowly).
    if let Some(s) = data.downcast_ref::<AppState>() {
        println!(
            "AZREVIEW_STARTUP root_listed_ms={} window_created_ms={} files={} folders={}",
            s.root_listed_ms.map_or_else(|| "-".to_string(), |ms| ms.to_string()),
            s.started.elapsed().as_millis(),
            s.tree.file_count(),
            s.tree.dir_count(),
        );
    }
    voice::spawn_speech_session(&mut info, &data);
    // The pages that are looked at long enough are seen; the workspace is counted behind the window.
    seen_watch::start(&mut info, &data);
    census::start(&mut info, &data);
    // What was seen of the workspace in an earlier run comes back.
    progress_store::load(&data, &mut info);
    // What was marked on the first file in an earlier run comes back.
    load_session(&data, &mut info);
    Update::DoNothing
}

// ==== The session archive: written into the data tree on a Thread ====

/// Writes the session as it is now (`review/<file>.azreview.zip`), on a
/// Thread; a write asked for while one is in flight runs after it.
pub(crate) fn save_session(data: &RefAny, info: &mut CallbackInfo) {
    // every change of a conversation comes here: one that was resolved is written as its patch file
    patch_store::record_due(data, info);
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
pub extern "C" fn on_open_folder(mut data: RefAny, _info: CallbackInfo) -> Update {
    let may_ask = data
        .downcast_mut::<AppState>()
        .is_some_and(|mut s| s.ask_for_folder());
    if !may_ask {
        return Update::DoNothing;
    }
    let _request = FileDialog::open_directory(
        t("azreview-choose-folder"),
        OptionString::None,
        data,
        on_folder_chosen,
    );
    Update::DoNothing
}

extern "C" fn on_folder_chosen(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.folder_dialog_closed();
    }
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let folder = PathBuf::from(path.inner.as_str());
    // The file being left is kept first (its archive is built now).
    save_session(&data, &mut info);
    let kit_ref = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.open_folder(folder.clone());
        println!("AZREVIEW_FOLDER {} {}", folder.display(), s.tree.file_count() + s.tree.dir_count());
        s.kit.clone()
    };
    kit::set_value(&kit_ref, &mut info, switches::KEY_FOLDER, &folder.to_string_lossy());
    if let Some(first) = data.downcast_ref::<AppState>().and_then(|s| s.file().map(|f| f.display.clone())) {
        println!("AZREVIEW_FILE {first}");
    }
    census::start(&mut info, &data);
    progress_store::load(&data, &mut info);
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
        let overlay = layout::is_overlay(window_width(&info));
        let mut folded = false;
        let mut closed_conversation = false;
        let acted = data.downcast_mut::<AppState>().is_some_and(|mut s| {
            // What listens stops first; then the conversation that is open closes; a tree that covers the pages folds
            // away on the next Escape (one that pushes them is part of the layout and stays).
            if s.voice.request_stop(dictation::Cause::Key) {
                return true;
            }
            if s.open_conv.take().is_some() {
                closed_conversation = true;
                return true;
            }
            if overlay && s.panels.tree_open {
                s.set_panel_open(layout::Panel::Tree, false);
                folded = true;
                return true;
            }
            false
        });
        if folded {
            persist_panels(&mut data, &mut info);
            start_slide(&data, &mut info);
        }
        if closed_conversation {
            info.trigger_all_virtual_view_rerender();
        }
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
    // Mod+B shows or hides the file tree, Mod+Shift+B the right panel.
    if matches!(key, VirtualKeyCode::B) && mods.primary_down() {
        info.prevent_default();
        let panel = if mods.shift { layout::Panel::Right } else { layout::Panel::Tree };
        return toggle_panel(&mut data, &mut info, panel);
    }
    // Mod+Shift+C: continue reviewing, back to where the jump to a conversation began.
    if matches!(key, VirtualKeyCode::C) && mods.primary_down() && mods.shift {
        info.prevent_default();
        return on_back(data, info);
    }
    // Mod+Shift+R: the file under review is not reviewed (or is again), whatever its pages say.
    if matches!(key, VirtualKeyCode::R) && mods.primary_down() && mods.shift {
        info.prevent_default();
        return on_hold_open_file(&mut data, &mut info);
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
    // The key of a pen takes it; the key of the pen in hand takes its next nib.
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        if s.press_pen_key(index) == pens::Pressed::Ignored {
            return Update::DoNothing;
        }
    }
    Update::RefreshDom
}

/// The rail's Save button.
pub extern "C" fn on_save_button(data: RefAny, mut info: CallbackInfo) -> Update {
    save_session(&data, &mut info);
    Update::RefreshDom
}

/// The window's width now, logical px.
fn window_width(info: &CallbackInfo) -> f32 {
    info.get_current_window_state().size.dimensions.width
}

/// The switch next to the traffic lights (and Mod+B): the file tree slides out or in.
pub extern "C" fn on_toggle_sidebar(mut data: RefAny, mut info: CallbackInfo) -> Update {
    toggle_panel(&mut data, &mut info, layout::Panel::Tree)
}

/// The right bar (and Mod+Shift+B): the right panel slides out or in.
pub extern "C" fn on_toggle_right(mut data: RefAny, mut info: CallbackInfo) -> Update {
    toggle_panel(&mut data, &mut info, layout::Panel::Right)
}

/// Opens or closes `panel`: what is chosen is remembered, and the slide begins.
fn toggle_panel(data: &mut RefAny, info: &mut CallbackInfo, panel: layout::Panel) -> Update {
    {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let open = match panel {
            layout::Panel::Tree => s.panels.tree_open,
            layout::Panel::Right => s.panels.right_open,
        };
        s.set_panel_open(panel, !open);
    }
    persist_panels(data, info);
    start_slide(data, info);
    Update::RefreshDom
}

/// What the panels are, into the settings (what is open and how wide comes back at the next start).
fn persist_panels(data: &mut RefAny, info: &mut CallbackInfo) {
    let Some((kit_ref, values)) = data.downcast_ref::<AppState>().map(|s| (s.kit.clone(), s.panels.to_values()))
    else {
        return;
    };
    for (key, value) in values {
        kit::set_value(&kit_ref, info, &key, &value);
    }
}

/// How often the slide of a panel is drawn, ms.
const SLIDE_TICK_MS: u64 = 16;

/// A panel is on its way and nothing draws it yet: one timer ticks until it has arrived.
fn start_slide(data: &RefAny, info: &mut CallbackInfo) {
    let mut handle = data.clone();
    let arm = {
        let Some(mut s) = handle.downcast_mut::<AppState>() else {
            return;
        };
        if s.slide.moving(&s.panels) && !s.sliding {
            s.sliding = true;
            s.slide_at_ms = s.voice.now_ms();
            true
        } else {
            false
        }
    };
    if arm {
        let timer = Timer::create(data.clone(), on_slide_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(SLIDE_TICK_MS)));
        info.add_timer(TimerId::unique(), timer);
    }
}

/// One step of a slide, by the time that has passed: the window is built again at the new width.
extern "C" fn on_slide_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let now = s.voice.now_ms();
    #[allow(clippy::cast_precision_loss)]
    let passed = now.saturating_sub(s.slide_at_ms) as f32;
    s.slide_at_ms = now;
    let panels = s.panels;
    s.slide = s.slide.step(&panels, passed);
    if s.slide.moving(&panels) {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        s.sliding = false;
        TimerCallbackReturn::terminate_and_refresh_dom()
    }
}

/// A press on a panel's edge: its width follows the pointer until the button is up (the node takes the pointer,
/// so a drag that leaves the grip goes on).
pub extern "C" fn on_grip_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(panel) = ui::grip_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(x) = info.get_cursor_position().into_option().map(|p| p.x) else {
        return Update::DoNothing;
    };
    let width = window_width(&info);
    {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let solved = layout::solve(width, &s.panels, &s.slide, 0.0);
        let start_w = match panel {
            layout::Panel::Tree => solved.tree_full_w,
            layout::Panel::Right => s.panels.right_w,
        };
        s.grip = Some(layout::Grip { panel, start_w, grab_x: x });
    }
    let node = info.get_hit_node();
    info.capture_pointer(node);
    Update::DoNothing
}

/// The pointer moves while an edge is held: the panel takes the width the pointer makes of it.
pub extern "C" fn on_grip_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(x) = info.get_cursor_position().into_option().map(|p| p.x) else {
        return Update::DoNothing;
    };
    let width = window_width(&info);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(grip) = s.grip else {
        return Update::DoNothing;
    };
    let solved = layout::solve(width, &s.panels, &s.slide, 0.0);
    let new_w = grip.width_at(x, width, solved.tree_full_w);
    match grip.panel {
        layout::Panel::Tree => s.panels.tree_w = new_w,
        layout::Panel::Right => s.panels.right_w = new_w,
    }
    Update::RefreshDom
}

/// The button is up: the edge is let go and the width is remembered.
pub extern "C" fn on_grip_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let held = data.downcast_mut::<AppState>().is_some_and(|mut s| s.grip.take().is_some());
    if held {
        persist_panels(&mut data, &mut info);
    }
    Update::DoNothing
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

/// The pen layer's press: a stroke begins (the pen's eraser end takes the stroke under it instead).
pub extern "C" fn on_ink_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ms = info.get_current_mouse_state();
    if ms.right_down || ms.middle_down {
        return Update::DoNothing;
    }
    let Some(page) = ui::page_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(sampled) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    log_pointer(&mut data, &mut info, pointer::Kind::Down, Some(page), &sampled);
    // The pen set down: a dictation it left behind ends (the stroke is the new thing it does). The window is
    // not rebuilt for it now: a stroke begins, and the dictation's own end message rebuilds it.
    voice::pointer(&mut data, &mut info, Some(true));
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if sampled.eraser {
        if let Some(i) = s.eraser_target(page, sampled.point.x, sampled.point.y) {
            let gone = s.erase(i);
            drop(s);
            if let Some(gone) = gone {
                ui::redraw_ink(&mut info, ink::Layer::of(gone.semantic));
            }
        }
        return Update::DoNothing;
    }
    let began = s.stroke_begin(page, sampled.point, sampled.pen);
    drop(s);
    // A stroke whose release was not heard is kept as it was.
    if let Some(stale) = began.stale {
        finish_stroke(&mut data, &mut info, stale);
    }
    // The first dab shows at once; the stroke grows in the frame of its layer.
    ui::redraw_ink(&mut info, began.layer);
    Update::DoNothing
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
    s.cycle_nib(backward);
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
    let Some(sampled) = ui::sample(info) else {
        return Update::DoNothing;
    };
    let page = ui::page_of(info);
    log_pointer(data, info, pointer::Kind::Move, page, &sampled);
    // Where the pen is, drawing or hovering: the dwell times its rest (and a dictation it has
    // moved away from ends). The window is not rebuilt for that while a stroke is live: a rebuilt window
    // replaces the pen's layer under the stroke.
    let asked = voice::pointer(data, info, None);
    let live = data.downcast_ref::<AppState>().is_some_and(|s| s.live.is_some());
    let update = pointer::refresh_after_move(live, asked);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.live.is_none() {
        // The pen's eraser end dragged over the page takes every stroke it crosses (a press takes the
        // one under it, `on_ink_down`).
        if sampled.eraser {
            if let Some(page) = page {
                if let Some(gone) = s
                    .eraser_target(page, sampled.point.x, sampled.point.y)
                    .and_then(|i| s.erase(i))
                {
                    drop(s);
                    ui::redraw_ink(info, ink::Layer::of(gone.semantic));
                }
            }
        }
        return update;
    }
    match s.stroke_move(sampled.point, sampled.pressed) {
        pointer::Moved::Nothing => Update::DoNothing,
        pointer::Moved::Grew(layer) => {
            drop(s);
            ui::redraw_ink(info, layer);
            Update::DoNothing
        }
        pointer::Moved::Ended(ended) => {
            drop(s);
            finish_stroke(data, info, ended)
        }
    }
}

/// The pen layer's release: the stroke that is live ends.
pub extern "C" fn on_ink_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(sampled) = ui::sample(&mut info) {
        let page = ui::page_of(&mut info);
        log_pointer(&mut data, &mut info, pointer::Kind::Up, page, &sampled);
    }
    // The pointer lifted: it rests where it was (not a move: nothing of a dictation ends).
    voice::pointer(&mut data, &mut info, Some(false));
    end_stroke(&mut data, &mut info)
}

/// Every release in the window: a stroke whose release the pen layer did not hear (the pointer was off
/// the page, over the margin, between two pages) ends all the same.
pub extern "C" fn on_window_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    log_window(&mut data, &mut info, pointer::Kind::WindowUp);
    end_stroke(&mut data, &mut info)
}

/// The pointer left the window: a stroke live under a pointer that is not pressed ends.
pub extern "C" fn on_window_left(mut data: RefAny, mut info: CallbackInfo) -> Update {
    log_window(&mut data, &mut info, pointer::Kind::WindowLeave);
    if info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    end_stroke(&mut data, &mut info)
}

/// The window lost its focus (a dialog, another app): what was drawn is kept; the release that comes
/// while another window has the focus is not heard here.
pub extern "C" fn on_focus_lost(mut data: RefAny, mut info: CallbackInfo) -> Update {
    log_window(&mut data, &mut info, pointer::Kind::FocusLost);
    // What is on the screen while the reviewer is at another window is not being looked at.
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.window_active = false;
    }
    // The microphone opened ahead of a dictation closes with the window going to the back.
    voice::window_deactivated(&mut data);
    end_stroke(&mut data, &mut info)
}

/// The window got the focus back: what is on the screen is looked at again, and the time away is not counted.
pub extern "C" fn on_focus_received(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.window_active = true;
        s.watched_at = None;
    }
    Update::DoNothing
}

/// The pointer left a page's pen layer: the dwell's rest is over, and a stroke live under a pointer that
/// is not pressed ends.
pub extern "C" fn on_ink_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    log_window(&mut data, &mut info, pointer::Kind::Leave);
    let asked = voice::pointer_left(&mut data);
    if info.get_current_mouse_state().left_down {
        return pointer::refresh_after_move(
            data.downcast_ref::<AppState>().is_some_and(|s| s.live.is_some()),
            asked,
        );
    }
    let ended = end_stroke(&mut data, &mut info);
    if matches!(ended, Update::DoNothing) {
        asked
    } else {
        ended
    }
}

/// Ends the stroke that is live, if one is, and does what its end asks.
fn end_stroke(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let ended = data.downcast_mut::<AppState>().and_then(|mut s| s.stroke_end());
    match ended {
        Some(ended) => finish_stroke(data, info, ended),
        None => Update::DoNothing,
    }
}

/// What a stroke that ended asks: its page's layer drawn anew; a tap cycles the nib; a kept stroke is
/// saved and the idle timer armed.
fn finish_stroke(data: &mut RefAny, info: &mut CallbackInfo, ended: pointer::Ended) -> Update {
    // The ended stroke's page, from wherever the end was heard (the layer, the window): every layer is asked, and
    // one that shows its page already draws nothing.
    info.update_all_image_callbacks();
    match ended.up {
        pointer::Up::Click => cycle_tool(data, false),
        pointer::Up::Dab => Update::DoNothing,
        pointer::Up::Slashed => {
            if let Some(s) = data.downcast_ref::<AppState>() {
                let words = s.slashes.last().map_or(0, |d| d.words.len());
                println!("AZREVIEW_SLASH {words}");
            }
            save_session(data, info);
            // The page's DOM says which words are drawn in the colour of the page.
            info.trigger_all_virtual_view_rerender();
            Update::RefreshDom
        }
        pointer::Up::Kept => {
            let timer_id = {
                let Some(mut s) = data.downcast_mut::<AppState>() else {
                    return Update::DoNothing;
                };
                println!("AZREVIEW_STROKES {}", s.strokes.len());
                if let Some(line) = s.perf.summary() {
                    println!("{line}");
                }
                s.idle_timer
            };
            save_session(data, info);
            arm_idle_timer(info, data.clone(), timer_id);
            Update::DoNothing
        }
    }
}

/// Notes an event of a page's pen layer in the log.
fn log_pointer(
    data: &mut RefAny,
    _info: &mut CallbackInfo,
    kind: pointer::Kind,
    page: Option<usize>,
    sampled: &ui::Sampled,
) {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        let live = s.live.as_ref().map(|l| l.points.len());
        s.pointer_log.push(pointer::Logged {
            kind,
            page,
            x: sampled.point.x,
            y: sampled.point.y,
            pressure: sampled.pressure,
            pressed: sampled.pressed,
            pen: sampled.pen,
            live,
        });
    }
}

/// Notes an event of the window (no position) in the log.
fn log_window(data: &mut RefAny, info: &mut CallbackInfo, kind: pointer::Kind) {
    let ms = info.get_current_mouse_state();
    let pen = info.get_pen_state().into_option();
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        let live = s.live.as_ref().map(|l| l.points.len());
        s.pointer_log.push(pointer::Logged {
            kind,
            page: None,
            x: 0.0,
            y: 0.0,
            pressure: pen.as_ref().map_or(0.0, |p| p.pressure),
            pressed: ms.left_down || pen.as_ref().is_some_and(|p| p.in_contact),
            pen: pen.is_some(),
            live,
        });
    }
}

/// The files of the tests' workspace: kept in memory, and the tree of their paths (every folder listed and open).
fn memory_workspace(files: &[code::SourceFile]) -> (symbols_source::MemSource, tree::Tree) {
    let mut source = symbols_source::MemSource::new();
    for f in files {
        source.put(&f.display, f.lines.join("\n").into_bytes());
    }
    let paths: Vec<&str> = files.iter().map(|f| f.display.as_str()).collect();
    (source, tree::Tree::from_paths(&paths))
}

/// Why a file is not shown, in the words of the window.
fn not_shown(path: &str, why: &code::NotShown) -> String {
    let mib = |bytes: u64| format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    let reason = match why {
        code::NotShown::Binary => t("azreview-not-shown-binary"),
        code::NotShown::TooLarge(bytes) => t_args(
            "azreview-not-shown-large",
            &[
                ("size", Arg::from(mib(*bytes))),
                ("limit", Arg::from(mib(code::MAX_OPEN_BYTES as u64))),
            ],
        ),
        code::NotShown::Unreadable(why) => t_args("azreview-not-shown-unreadable", &[("why", Arg::from(why.clone()))]),
    };
    t_args(
        "azreview-not-shown",
        &[("file", Arg::from(path.to_string())), ("why", Arg::from(reason))],
    )
}

/// One change to the ink that Mod+Z can take back.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// A stroke was kept.
    Added(Stroke),
    /// A stroke was erased from place `index` of the file's strokes.
    Erased(Stroke, usize),
    /// A quick slash took words.
    Slashed(deletion::Deletion),
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
    // The pages are scrolled: the symbol index keeps to the neighbourhood of the file until it is quiet again.
    s.symbols.touch();
    let Some(pages) = s.file().map(code::SourceFile::page_count) else {
        return Update::DoNothing;
    };
    let page = ui::page_at(x, pages);
    if page == s.visible_page {
        return Update::DoNothing;
    }
    s.visible_page = page;
    drop(s);
    // The strip of a file of many pages is rebuilt around the page in view (`layout::strip_items`); one that shows every
    // page is scrolled to its number.
    if pages <= layout::STRIP_ALL {
        ui::reveal_pill(&mut info, page);
    }
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

/// The bar's count of finished conversations (or its key to fold them again): the finished ones are all shown, or folded.
pub extern "C" fn on_bar_more(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.bar_all = !s.bar_all;
    Update::RefreshDom
}

/// "Continue reviewing" (the bar's key, Mod+Shift+C): back to the place the reviewer was taken from, which is then none.
pub extern "C" fn on_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let place = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(place) = s.progress.come_back() else {
            return Update::DoNothing;
        };
        if s.file().is_none_or(|f| f.display != place.file) {
            // another file: it opens, and the session of the one that is left is kept
            let _ = s.open_path(&place.file);
        }
        s.visible_page = place.page;
        place
    };
    ui::scroll_to_page(&mut info, place.page);
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// A click on a conversation's chip: the pages go to where it is anchored.
pub extern "C" fn on_chip_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(page) = data
        .downcast_ref::<AppState>()
        .and_then(|s| s.open_chips().get(index).map(|c| c.page))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    // The place the reviewer was reviewing at is where they come back to: the first jump only (a jump from one
    // conversation to another keeps it).
    let here = s.review_place();
    if let Some(here) = here {
        let _ = s.progress.jumped_from(here);
    }
    drop(s);
    ui::scroll_to_page(&mut info, page);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.visible_page = page;
    // A conversation of the file (not a mock chip) opens in the margin of its page.
    if let Some(id) = s.open_chips().get(index).map(|c| c.id).filter(|id| s.notes.iter().any(|n| n.id == *id)) {
        s.open_conv = Some(id);
    }
    drop(s);
    info.trigger_all_virtual_view_rerender();
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

/// The Session menu's "Save the pointer log": the last pointer events, as text, in the sessions' folder (it
/// is opened, to be sent along with a report of a stroke that went wrong).
pub extern "C" fn on_menu_pointer_log(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let dir = s.data_root.join(ABOUT.app_folder);
    let path = dir.join("pointer-events.log");
    let text = s.pointer_log.text();
    let written = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, text));
    match written {
        Ok(()) => {
            println!("AZREVIEW_POINTER_LOG {}", path.display());
            if let Err(e) = azul_appkit::files::open_external(&dir.to_string_lossy()) {
                s.problem = Some(e);
                return Update::RefreshDom;
            }
            Update::DoNothing
        }
        Err(e) => {
            s.problem = Some(format!("{}: {e}", path.display()));
            Update::RefreshDom
        }
    }
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
    s.select_pen(index);
    Update::RefreshDom
}

/// A double click on a pen: it is taken and has its next nib.
pub extern "C" fn on_pen_double_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.double_click_pen(index);
    Update::RefreshDom
}

/// A click on a file of the tree: it opens (the node is the row's).
pub extern "C" fn on_pick_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(id) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some((open, path)) = data
        .downcast_ref::<AppState>()
        .map(|s| (s.open_node, s.tree.path(id)))
    else {
        return Update::DoNothing;
    };
    if open == Some(id) {
        return Update::DoNothing;
    }
    // The file being left is kept first (its archive is built now), and what was seen of it.
    save_session(&data, &mut info);
    progress_store::save(&data, &mut info);
    let opened = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.open_path(&path)
    };
    if opened {
        println!("AZREVIEW_FILE {path}");
        // What was marked on it in an earlier run comes back.
        load_session(&data, &mut info);
    }
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// The reviewer says the file under review is not reviewed (or says it is again).
fn on_hold_open_file(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let path = data.downcast_ref::<AppState>().and_then(|s| s.file().map(|f| f.display.clone()));
    if let Some(path) = path {
        let held = data.downcast_mut::<AppState>().is_some_and(|mut s| s.toggle_held(&path));
        println!("AZREVIEW_HELD {path} {held}");
        progress_store::save(data, info);
        info.trigger_all_virtual_view_rerender();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// A right click on a file of the tree: it is not reviewed (or is again), whatever its pages say.
pub extern "C" fn on_tree_hold_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(id) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let path = data.downcast_ref::<AppState>().map(|s| s.tree.path(id));
    if let Some(path) = path {
        let held = data.downcast_mut::<AppState>().is_some_and(|mut s| s.toggle_held(&path));
        println!("AZREVIEW_HELD {path} {held}");
        progress_store::save(&data, &mut info);
        info.trigger_all_virtual_view_rerender();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// A click on a folder of the tree: it opens or closes, and is listed on a thread when it never was.
pub extern "C" fn on_toggle_dir(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(id) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let job = data.downcast_mut::<AppState>().and_then(|mut s| s.toggle_dir(id));
    if let Some(job) = job {
        tree_load::spawn(&mut info, &data, job);
    }
    info.trigger_all_virtual_view_rerender();
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
