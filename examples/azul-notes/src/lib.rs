//! AzNotes: notes on the public azul API, in the three-pane PIM shell (S4).
//!
//! The navigation pane lists All notes / Pinned, the notebooks as a tree
//! (nested, with counts) and the tags; the note list (azul's MessageList,
//! with the pin as its row mark) shows the scope's notes under "Pinned" /
//! date sections, searched as you type over title, text, tags and
//! notebook, sorted by date modified / created / title; the editor pane
//! holds the title field, the tag chips, the formatting toolbar and azul's
//! shared rich-text editor (`editor.rs`) with Markdown shortcuts. Ctrl/Cmd+K opens
//! the command palette (open a note, new note, move to a notebook, ...).
//!
//! Every note is a Markdown file with a small front matter,
//! `notes/<notebook>/<uuid>.md`, its images next to it, a version per save
//! under `notes/.history/<uuid>/` (at most one every few minutes while
//! typing), written through azul-storage's `Drive` - a `LocalDrive` on the
//! AzNotes folder today, the user's bucket later - from an azul `Thread`
//! (`jobs.rs`), never from a callback. Edits are saved 500 ms after the last
//! keystroke, when another note opens and when the window closes; files
//! changed by another program are read again when the window gets the
//! focus.
//!
//! Environment: `AZNOTES_DATA` names the AzNotes folder (default: `AzNotes`
//! in the user's data folder; `--data` wins).
//!
//! On stdout, for scripts (`scripts/aznotes_e2e.py`): `AZNOTES_DATA <dir>`,
//! `AZNOTES_LOADED <notes>`, `AZNOTES_OPEN <id>`, `AZNOTES_NEW <id>`,
//! `AZNOTES_SAVED <id> <key>`, `AZNOTES_SCREEN <name>`.

pub mod args;
pub mod editor;
pub mod ids;
mod jobs;
pub mod look;
pub mod markdown;
pub mod model;
pub mod sample;
pub mod store;
mod ui;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use azul::{
    app::{App, AppConfig},
    callbacks::{CallbackInfo, RefAny, Update},
    css::{DarkLightMode, LogicalSize},
    dom::Callback,
    file::FilePath,
    image::ImageRef,
    option::OptionDarkLightMode,
    str::String as AzString,
    window::{WindowCreateOptions, WindowDecorations},
};
use azul_storage::{Drive, LocalDrive};

pub use crate::args::Args;
use crate::{
    look::TextSize,
    model::{Library, Note, Query},
};

/// The environment variable naming the AzNotes folder.
pub const DATA_VAR: &str = "AZNOTES_DATA";
/// The settings object in the AzNotes folder (beside `notes/`).
pub const SETTINGS_KEY: &str = "aznotes-settings.txt";

// ==== State ====

/// What the window shows in place of the notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Notes,
    Settings,
    /// The open note's versions.
    History,
}

/// A sheet over the window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    None,
    /// The command palette (Ctrl/Cmd+K).
    Palette,
    /// The name of a new notebook.
    NewNotebook { name: String, error: String },
    /// The URL to link the selection to.
    Link {
        url: String,
        /// The selection to link, `(block, start, end)`, taken when the
        /// sheet opened (the field takes the focus from the text); empty:
        /// the address goes in at the caret.
        spans: Vec<(usize, usize, usize)>,
    },
    /// Delete a trashed note for good.
    ConfirmDelete { id: String },
}

/// The save state the status bar shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Idle,
    Editing,
    Saving,
    Saved,
    Error(String),
}

impl Status {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Status::Idle => String::new(),
            Status::Editing => "Edited".to_string(),
            Status::Saving => "Saving...".to_string(),
            Status::Saved => "Saved".to_string(),
            Status::Error(e) => format!("Not saved: {e}"),
        }
    }
}

/// The navigation pane's own state.
#[derive(Debug, Clone, PartialEq)]
pub struct NavState {
    pub collapsed: bool,
    /// Library, Notebooks, Tags.
    pub groups_open: [bool; 3],
    /// Notebooks shown open in the tree.
    pub expanded: BTreeSet<String>,
    /// The navigation's and the list's share of the width.
    pub navigation_ratio: f32,
    pub list_ratio: f32,
}

impl Default for NavState {
    fn default() -> Self {
        NavState {
            collapsed: false,
            groups_open: [true, true, true],
            expanded: BTreeSet::new(),
            navigation_ratio: 0.2,
            list_ratio: 0.32,
        }
    }
}

/// The open note's versions (the History screen).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryView {
    pub id: String,
    /// `(key, time)`, newest first.
    pub versions: Vec<(String, u64)>,
    pub selected: Option<usize>,
    /// The selected version's file.
    pub text: Option<String>,
    pub loading: bool,
    pub error: String,
}

/// What the user set; kept in [`SETTINGS_KEY`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `flat` / `flora`.
    pub theme: String,
    /// `system` / `light` / `dark`.
    pub mode: String,
    pub text_size: TextSize,
    /// Save this long after the last edit.
    pub autosave_ms: u64,
    /// Keep a version at most this often while typing (an explicit save,
    /// switching notes and closing always keep one).
    pub version_minutes: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "flat".to_string(),
            mode: "system".to_string(),
            text_size: TextSize::Medium,
            autosave_ms: 500,
            version_minutes: 5,
        }
    }
}

impl Settings {
    /// The settings file: `key=value` lines.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!(
            "theme={}\nmode={}\ntext_size={}\nautosave_ms={}\nversion_minutes={}\n",
            self.theme,
            self.mode,
            self.text_size.name(),
            self.autosave_ms,
            self.version_minutes
        )
    }

    /// The settings of a file; unknown or bad lines keep the defaults.
    #[must_use]
    pub fn from_text(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "theme" if matches!(value, "flat" | "flora") => s.theme = value.to_string(),
                "mode" if matches!(value, "system" | "light" | "dark") => s.mode = value.to_string(),
                "text_size" => {
                    if let Some(size) = TextSize::from_name(value) {
                        s.text_size = size;
                    }
                }
                "autosave_ms" => {
                    if let Ok(ms) = value.parse::<u64>() {
                        s.autosave_ms = ms.clamp(100, 60_000);
                    }
                }
                "version_minutes" => {
                    if let Ok(m) = value.parse::<u64>() {
                        s.version_minutes = m.min(24 * 60);
                    }
                }
                _ => {}
            }
        }
        s
    }
}

/// The app.
pub struct AppState {
    pub args: Args,
    pub drive: Arc<dyn Drive>,
    /// The AzNotes folder (the LocalDrive's root).
    pub root: PathBuf,
    pub library: Library,
    /// The first load answered.
    pub loaded: bool,
    /// An error or notice over the list.
    pub notice: String,
    pub query: Query,
    /// The open note's id.
    pub open: Option<String>,
    pub nav: NavState,
    /// The open note's editor: its document, its ONE undo history, the
    /// caret (azul's shared rich-text editor; `editor.rs`).
    pub editor: azul::widgets::RichTextEditorState,
    pub overlay: Overlay,
    pub palette_query: String,
    pub screen: Screen,
    pub history: Option<HistoryView>,
    pub settings: Settings,
    pub settings_category: usize,
    pub settings_search: String,
    /// The tag being typed in the tag field.
    pub tag_draft: String,
    /// Notes with a save on the way.
    pub saving: BTreeSet<String>,
    /// When the last edit happened (the autosave waits for a pause).
    pub last_edit: Option<Instant>,
    /// When each note's last version was kept.
    pub last_version: BTreeMap<String, u64>,
    /// The window waits for its saves to close.
    pub closing: bool,
    pub status: Status,
    /// Decoded images by key.
    pub images: HashMap<String, ImageRef>,
    /// Image keys asked for (loaded or on the way).
    pub images_requested: BTreeSet<String>,
    /// Storage jobs on the way.
    pub running: usize,
}

impl AppState {
    /// The open note.
    #[must_use]
    pub fn open_note(&self) -> Option<&Note> {
        self.open.as_deref().and_then(|id| self.library.get(id))
    }

    /// The open note, to change.
    pub fn open_note_mut(&mut self) -> Option<&mut Note> {
        let id = self.open.clone()?;
        self.library.get_mut(&id)
    }

    /// The open note was edited just now: dated, searchable, due a save.
    pub fn edited(&mut self) {
        let now = azul_storage::time::now_unix();
        if let Some(note) = self.open_note_mut() {
            note.touch(now);
            note.refresh();
        }
        self.last_edit = Some(Instant::now());
        self.status = Status::Editing;
    }

    /// The local time's offset from UTC, seconds (the list's days).
    #[must_use]
    pub fn utc_offset() -> i64 {
        i64::from(chrono::Local::now().offset().local_minus_utc())
    }
}

// ==== Start ====

fn path_of(dir: Option<FilePath>) -> Option<PathBuf> {
    dir.map(|d| PathBuf::from(d.inner.as_str()))
        .filter(|p| !p.as_os_str().is_empty())
}

/// The AzNotes folder: `--data`, else `AZNOTES_DATA`, else `AzNotes` in the
/// user's data folder, else `AzNotes` here.
#[must_use]
pub fn data_root(flag: Option<PathBuf>, env: Option<String>, user_data: Option<PathBuf>) -> PathBuf {
    flag.or_else(|| {
        env.map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    })
    .or_else(|| user_data.map(|d| d.join("AzNotes")))
    .unwrap_or_else(|| PathBuf::from("AzNotes"))
}

pub fn start(args: Args) {
    let root = data_root(
        args.data.clone(),
        std::env::var(DATA_VAR).ok(),
        path_of(FilePath::get_data_dir().into_option()),
    );
    let drive: Arc<dyn Drive> = Arc::new(LocalDrive::new(&root));
    // The settings are read once before the window exists (a small file;
    // every later write goes through a job).
    let mut settings = drive
        .get(SETTINGS_KEY)
        .map(|b| Settings::from_text(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    if let Some(theme) = &args.theme {
        settings.theme = theme.clone();
    }
    if let Some(mode) = &args.mode {
        settings.mode = mode.clone();
    }
    println!("AZNOTES_DATA {}", root.display());

    let screen = match args.screen {
        args::Screen::Settings | args::Screen::About | args::Screen::Shortcuts => Screen::Settings,
        args::Screen::History => Screen::History,
        args::Screen::Notes | args::Screen::Palette => Screen::Notes,
    };
    let settings_category = match args.screen {
        args::Screen::Shortcuts => ui::SETTINGS_SHORTCUTS,
        args::Screen::About => ui::SETTINGS_ABOUT,
        _ => 0,
    };
    let overlay = if args.screen == args::Screen::Palette {
        Overlay::Palette
    } else {
        Overlay::None
    };

    let mut config = AppConfig::create()
        .with_app_id("org.azul.AzNotes")
        .with_theme(settings.theme.as_str());
    match settings.mode.as_str() {
        "light" => config = config.with_mode(OptionDarkLightMode::Some(DarkLightMode::Light)),
        "dark" => config = config.with_mode(OptionDarkLightMode::Some(DarkLightMode::Dark)),
        _ => {}
    }
    let size = args.size.unwrap_or((1200.0, 760.0));

    let state = AppState {
        args,
        drive,
        root,
        library: Library::default(),
        loaded: false,
        notice: String::new(),
        query: Query::default(),
        open: None,
        nav: NavState::default(),
        editor: editor::state_for(&azul::widgets::RichTextDoc::create()),
        overlay,
        palette_query: String::new(),
        screen,
        history: None,
        settings,
        settings_category,
        settings_search: String::new(),
        tag_draft: String::new(),
        saving: BTreeSet::new(),
        last_edit: None,
        last_version: BTreeMap::new(),
        closing: false,
        status: Status::Idle,
        images: HashMap::new(),
        images_requested: BTreeSet::new(),
        running: 0,
    };
    let app = App::create(RefAny::new(state), config);
    let mut window = WindowCreateOptions::create(ui::layout);
    window.window_state.size.dimensions = LogicalSize::create(size.0, size.1);
    window.window_state.title = AzString::from("AzNotes");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(jobs::on_startup)).into();
    app.run(window);
}

/// `Update::RefreshDom` when `changed`.
#[must_use]
pub fn refresh_if(changed: bool) -> Update {
    if changed {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// Runs `f` on the state with the app handle and the callback info.
pub fn with_state(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut AppState, &mut CallbackInfo, &RefAny) -> Update,
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    f(&mut *guard, info, &app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_folder_is_the_flag_then_the_variable_then_the_users_data_folder() {
        let user = Some(PathBuf::from("/home/u/.local/share"));
        assert_eq!(
            data_root(Some(PathBuf::from("/x")), Some("/y".to_string()), user.clone()),
            PathBuf::from("/x")
        );
        assert_eq!(data_root(None, Some(" /y ".to_string()), user.clone()), PathBuf::from("/y"));
        assert_eq!(
            data_root(None, Some(" ".to_string()), user),
            PathBuf::from("/home/u/.local/share/AzNotes")
        );
        assert_eq!(data_root(None, None, None), PathBuf::from("AzNotes"));
    }

    #[test]
    fn settings_round_trip_and_bad_lines_keep_the_defaults() {
        let s = Settings {
            theme: "flora".to_string(),
            mode: "dark".to_string(),
            text_size: TextSize::Large,
            autosave_ms: 2000,
            version_minutes: 10,
        };
        assert_eq!(Settings::from_text(&s.to_text()), s);
        let bad = Settings::from_text("theme=neon\nautosave_ms=fast\nnonsense\n");
        assert_eq!(bad, Settings::default());
        assert_eq!(Settings::from_text("autosave_ms=1").autosave_ms, 100, "clamped");
    }
}

#[cfg(test)]
mod ids_tests;
