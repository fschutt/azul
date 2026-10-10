//! AzNotes: notes on the public azul API, in the three-pane PIM shell (S4).
//!
//! The navigation pane lists All notes / Pinned, the notebooks as a tree
//! (nested, with counts) and the tags; the note list (azul's SummaryList,
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
//! The switches, the data root (`--data-dir`, else `AZLIN_DATA`, else
//! `Azlin` in the user's data folder: the notes are `<root>/notes/...`, the
//! layout the user's bucket will have), the settings file
//! (`notes/settings.json`: theme, mode and AzNotes' own values), the
//! settings page, the About facts and the shortcut table are azul-appkit's.
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
    app::App,
    callbacks::{CallbackInfo, RefAny, Update},
    image::ImageRef,
};
use azul_appkit::{about::AboutInfo, args::AppSpec, shortcuts::Shortcut, ui as kit};
use azul_storage::{Drive, LocalDrive};

pub use crate::args::Args;
use crate::{
    look::TextSize,
    model::{Library, Note, Query},
};

// ==== The app's facts (azul-appkit) ====

pub const SPEC: AppSpec = AppSpec {
    name: "AzNotes",
    binary: "AzNotes",
    summary: "notes as Markdown files, with a rich-text editor",
    screens: &args::Screen::NAMES,
    files_help: "",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzNotes",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Notes as plain Markdown files, with notebooks, tags, pinning, search, version \
              history and a rich-text editor.",
    license: "MIT",
    app_folder: model::APP_FOLDER,
};

/// The settings page's own categories (before the kit's Appearance, Data,
/// Shortcuts and About).
pub const APP_CATEGORIES: [&str; 2] = ["Editor", "Storage"];

/// The keyboard shortcuts (the settings page and F1 list them).
pub const SHORTCUTS: [Shortcut; 16] = [
    Shortcut::new("Notes", "Mod+N", "New note"),
    Shortcut::new("Notes", "Mod+K", "Command palette"),
    Shortcut::new("Notes", "Mod+S", "Save now (and keep a version)"),
    Shortcut::new("Notes", "Mod+Shift+P", "Pin or unpin the note"),
    Shortcut::new("Notes", "Mod+Shift+H", "Version history"),
    Shortcut::new("Notes", "Escape", "Close a sheet, leave the history"),
    Shortcut::new("Editing", "Mod+Z / Mod+Shift+Z", "Undo / redo"),
    Shortcut::new("Editing", "Mod+B / I / U", "Bold / italic / underline"),
    Shortcut::new("Editing", "Mod+Shift+X", "Strikethrough"),
    Shortcut::new("Editing", "Mod+E", "Inline code"),
    Shortcut::new("Editing", "Mod+Shift+K", "Link"),
    Shortcut::new("Editing", "Mod+0 / 1 / 2 / 3", "Paragraph / heading 1-3"),
    Shortcut::new("Editing", "Mod+Shift+7 / 8 / 9", "Numbered / bulleted / check list"),
    Shortcut::new("Editing", "Mod+Enter", "Tick a check item"),
    Shortcut::new("Editing", "Tab / Shift+Tab", "Indent / outdent a list item"),
    Shortcut::new("Panes", "F6 / Shift+F6", "Next / previous pane"),
];

// ==== State ====

/// What the window shows in place of the notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Notes,
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

/// AzNotes' own settings, values of the kit's settings file (the theme and
/// the mode are the kit's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
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
            text_size: TextSize::Medium,
            autosave_ms: 500,
            version_minutes: 5,
        }
    }
}

impl Settings {
    /// The settings' values as the kit's settings file keeps them.
    #[must_use]
    pub fn values(&self) -> [(&'static str, String); 3] {
        [
            ("text_size", self.text_size.name().to_string()),
            ("autosave_ms", self.autosave_ms.to_string()),
            ("version_minutes", self.version_minutes.to_string()),
        ]
    }

    /// The settings of the values `get` answers; unknown or bad values keep
    /// the defaults.
    #[must_use]
    pub fn from_values(get: impl Fn(&str) -> Option<String>) -> Settings {
        let mut s = Settings::default();
        if let Some(size) = get("text_size").and_then(|v| TextSize::from_name(v.trim())) {
            s.text_size = size;
        }
        if let Some(ms) = get("autosave_ms").and_then(|v| v.trim().parse::<u64>().ok()) {
            s.autosave_ms = ms.clamp(100, 60_000);
        }
        if let Some(m) = get("version_minutes").and_then(|v| v.trim().parse::<u64>().ok()) {
            s.version_minutes = m.min(24 * 60);
        }
        s
    }
}

/// The app.
pub struct AppState {
    pub args: Args,
    /// azul-appkit's kit: the settings file, the data root, the settings page.
    pub kit: RefAny,
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
    /// The About box is open.
    pub about_open: bool,
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

pub fn start(args: Args) {
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.app.clone());
    let (root, settings) = {
        let mut k = kit_ref.clone();
        let found = k.downcast_ref::<kit::Kit>().map(|k| {
            let settings = Settings::from_values(|key| k.settings.get(key).map(str::to_string));
            (k.data_root.clone(), settings)
        });
        found.unwrap_or_default()
    };
    let drive: Arc<dyn Drive> = Arc::new(LocalDrive::new(&root));
    println!("AZNOTES_DATA {}", root.display());

    let screen = match args.screen {
        args::Screen::Settings => {
            kit::open_settings(&kit_ref, None);
            Screen::Notes
        }
        args::Screen::About => {
            kit::open_settings(&kit_ref, Some("About"));
            Screen::Notes
        }
        args::Screen::Shortcuts => {
            kit::open_settings(&kit_ref, Some("Shortcuts"));
            Screen::Notes
        }
        args::Screen::History => Screen::History,
        args::Screen::Notes | args::Screen::Palette => Screen::Notes,
    };
    let overlay = if args.screen == args::Screen::Palette {
        Overlay::Palette
    } else {
        Overlay::None
    };
    let config = kit::app_config(&kit_ref).with_app_id("org.azul.AzNotes");
    let window = kit::window_options(&kit_ref, ui::layout, (1200.0, 760.0), (720.0, 480.0), jobs::on_startup);

    let state = AppState {
        args,
        kit: kit_ref,
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
        about_open: false,
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
    App::create(RefAny::new(state), config).run(window);
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

    fn values_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_string())
    }

    #[test]
    fn settings_round_trip_and_bad_values_keep_the_defaults() {
        let s = Settings {
            text_size: TextSize::Large,
            autosave_ms: 2000,
            version_minutes: 10,
        };
        let values = s.values();
        let pairs: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(Settings::from_values(values_of(&pairs)), s);
        let bad = Settings::from_values(values_of(&[("autosave_ms", "fast"), ("text_size", "huge")]));
        assert_eq!(bad, Settings::default());
        assert_eq!(
            Settings::from_values(values_of(&[("autosave_ms", "1")])).autosave_ms,
            100,
            "clamped"
        );
    }
}

#[cfg(test)]
mod ids_tests;
