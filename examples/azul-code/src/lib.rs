//! AzCode: a code editor on the public azul API, in VSCode's shape.
//!
//! The window is azul's `OfficeShell` with the S8 developer shell's panes
//! (the app-drawn `Titlebar` under `WindowDecorations::NoTitle`, VSCode's
//! menu bar; the activity bar, the side bar - the explorer or the search
//! over the folder - the editor with its tabs over azul's `CodeView`, the
//! terminal panel under it, the status bar, quick open and the command
//! palette) inside a `ShellThemeScope`, behind a `CloseGuard` that asks
//! "Save changes?". Without a folder the explorer says "You have not yet
//! opened a folder." (Open Folder, the recent folders) and the editor shows
//! the welcome page (Open Folder..., Open File..., the keyboard shortcuts).
//! It is dark by default, as VSCode is, and follows the app theme (flat /
//! flora) and the mode the user picks.
//!
//! Everything long is virtualized: the explorer and the search results are
//! `VirtualView`s that build the rows in view, the CodeView builds the lines
//! in view (in a `VirtualView` of its own, which a scroll renders again
//! without a rebuild of the window), the terminal's scrollback is read a
//! screen at a time.
//!
//! - [`buffer`]: the text of an open file, a piece table (plain Rust).
//! - [`highlight`]: syntect, incremental by line, checkpoints, a background
//!   walk for far jumps (plain Rust).
//! - [`search`]: find / replace (azul-appkit's matcher), go to line, the
//!   matches of a file for the search over the folder.
//! - [`workspace`]: the explorer's tree, the tabs, the recent folders,
//!   quick open's ranking; [`git`]: the branch (plain Rust).
//! - [`storage`]: the workspace's files through azul-storage's Drive on
//!   azul Threads (list, read, write, index, search); [`sample`]: the sample
//!   workspace (`--sample`).
//! - [`app`], [`commands`], [`actions`]: the state, what the user asks, the
//!   commands the menu, the palette and the keys run.
//! - [`ui`], [`explorer`], [`find_in_files`], [`terminal`], [`palette`],
//!   [`menu`]: the window.
//!
//! A workspace is a folder named on the command line (`AzCode ~/project`,
//! `--folder ~/project`; read and written in place through a drive without
//! the data tree's manifest), picked with Open Folder (Mod+K Mod+O, Mod+O,
//! File > Open Folder...) or from the recent folders, or the sample in the
//! data tree (`code/sample/`). A file named on the command line or picked
//! with Open File... opens on its own (its folder is its drive) when it is
//! not in the workspace.
//!
//! On stdout, for scripts (`scripts/azcode_e2e.py`): `AZCODE_READY`,
//! `AZCODE_FOLDER <dir>` (the folder dialog's answer) /
//! `AZCODE_FOLDER_CANCELLED`, `AZCODE_WORKSPACE <dir>` (a workspace opened,
//! however), `AZCODE_LISTED <folder> <n>`, `AZCODE_BRANCH <name>`,
//! `AZCODE_FILE <path>`, `AZCODE_OPENED <key> <lines>`, `AZCODE_DIRTY <key>
//! 1|0`, `AZCODE_SAVED <key>`, `AZCODE_INDEXED <n>`, `AZCODE_SEARCHED
//! <matches> <files>`, `AZCODE_FOUND <n>`, `AZCODE_REPLACED <n>`,
//! `AZCODE_COMMAND <name>`, `AZCODE_PANEL open|closed`,
//! `AZCODE_TERMINAL_READY <n> <dir>`, `AZCODE_TERMINAL_OUTPUT <n>`,
//! `AZCODE_TERMINAL_EXITED <n>`.

pub mod actions;
pub mod app;
pub mod args;
pub mod buffer;
pub mod commands;
pub mod explorer;
pub mod find_in_files;
pub mod git;
pub mod highlight;
pub mod ids;
pub mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod menu;
pub mod palette;
pub mod sample;
pub mod search;
pub mod storage;
pub mod terminal;
pub mod ui;
pub mod workspace;

use azul::{
    app::App,
    callbacks::{CallbackInfo, LayoutCallbackInfo, RefAny, Update},
    css::EventFilter,
    dom::Dom,
    shells::{ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{CloseGuard, CloseGuardEvent, CloseGuardEventKind},
    window::WindowEventFilter,
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec, ModePref},
    shortcuts::Shortcut,
    ui as kit,
};

pub use crate::app::AppState;

// ==== The app's facts ====

pub const SCREENS: [&str; 2] = ["editor", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzCode",
    binary: "AzCode",
    summary: "a code editor for folders of source files and files of a million lines",
    screens: &SCREENS,
    files_help: "a folder to open as the workspace",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzCode",
    version: env!("CARGO_PKG_VERSION"),
    // The About page says it by `azcode-about-summary` (l10n::app_word).
    summary: "A code editor: the explorer, tabs, syntax colours, find and replace, search in the folder, \
              a terminal, go to line, files of a million lines. Your folders are edited in place; the \
              sample lives in your data folder.",
    license: "MIT",
    app_folder: sample::APP_FOLDER,
};

pub const SHORTCUTS: [Shortcut; 24] = [
    Shortcut::new("azcode-menu-file", "Mod+K Mod+O / Mod+O", "azcode-shortcut-open-folder"),
    Shortcut::new("azcode-menu-file", "Mod+K F", "azcode-shortcut-close-folder"),
    Shortcut::new("azcode-menu-file", "Mod+P", "azcode-shortcut-quick-open"),
    Shortcut::new("azcode-menu-file", "Mod+S", "azcode-shortcut-save"),
    Shortcut::new("azcode-menu-file", "Mod+W", "azcode-shortcut-close-tab"),
    Shortcut::new("azcode-shortcut-window", "Mod+Shift+P", "azcode-shortcut-palette"),
    Shortcut::new("azcode-shortcut-window", "Mod+B", "azcode-shortcut-side-bar"),
    Shortcut::new("azcode-shortcut-window", "Mod+Shift+E", "azcode-shortcut-explorer"),
    Shortcut::new("azcode-shortcut-window", "Ctrl+` / Mod+J", "azcode-shortcut-terminal"),
    Shortcut::new("azcode-shortcut-window", "Ctrl+Shift+`", "azcode-shortcut-new-terminal"),
    Shortcut::new("azcode-action-find", "Mod+Shift+F", "azcode-shortcut-find-in-files"),
    Shortcut::new("azcode-action-find", "Mod+F", "azcode-shortcut-find"),
    Shortcut::new("azcode-action-find", "Mod+H", "azcode-shortcut-replace"),
    Shortcut::new("azcode-action-find", "F3 / Shift+F3", "azcode-shortcut-next-match"),
    Shortcut::new("azcode-action-find", "Mod+G", "azcode-shortcut-go-to-line"),
    Shortcut::new("azcode-shortcut-editing", "Mod+Z / Mod+Shift+Z", "azcode-shortcut-undo"),
    Shortcut::new("azcode-shortcut-editing", "Mod+D", "azcode-shortcut-select-word"),
    Shortcut::new("azcode-shortcut-editing", "Alt+click", "azcode-shortcut-cursor"),
    Shortcut::new("azcode-shortcut-editing", "Tab / Shift+Tab", "azcode-shortcut-indent"),
    Shortcut::new("azcode-shortcut-editing", "Mod+X / Mod+C / Mod+V", "azcode-shortcut-clipboard"),
    Shortcut::new("azcode-shortcut-moving", "Mod+Home / Mod+End", "azcode-shortcut-file-ends"),
    Shortcut::new("azcode-shortcut-moving", "Alt+arrows (macOS) / Ctrl+arrows", "azcode-shortcut-words"),
    Shortcut::new("azcode-action-explorer", "Up / Down / Left / Right / Enter", "azcode-shortcut-tree"),
    Shortcut::new("azcode-shortcut-window", "Escape", "azcode-shortcut-escape"),
];

// ==== Start ====

pub fn start() {
    let args = match args::Args::from_env() {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    // TERM / COLORTERM for the terminal panel's shells (before any thread exists).
    azul_termkit::pane::setup_env();
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args.kit.clone());
    dark_by_default(&kit_ref, &args.kit);
    let data_root = {
        let mut k = kit_ref.clone();
        let root = k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone());
        root.unwrap_or_default()
    };
    // appkit's words and AzCode's, before any is said (the start's own notice below); the
    // language chosen (Settings, `--language`) says them from here.
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != azul_appkit::args::LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
    let mut st = AppState::new(kit_ref.clone(), data_root, args.kit.sample);
    st.recent = recent_of(&kit_ref);
    st.shell = args.shell.clone();
    // `--folder`, else a bare folder or file.
    let named = args.folder.clone().or_else(|| args.kit.files.first().cloned());
    if let Some(named) = named {
        // Absolute: `AzCode .` names the folder by its name, not ".".
        let path = std::path::absolute(&named).unwrap_or(named);
        if path.is_dir() {
            st.workspace_to_open = Some(commands::folder_root(&path));
        } else if path.is_file() && args.folder.is_none() {
            st.file_to_open = Some(path);
        } else {
            st.notice = azul_appkit::l10n::t_args(
                "azcode-not-a-folder",
                &[("path", azul_appkit::l10n::Arg::from(path.display().to_string()))],
            );
        }
    }
    if args.kit.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let window = kit::window_options(&kit_ref, layout, (1280.0, 800.0), (720.0, 480.0), on_window_created);
    App::create(RefAny::new(st), config).run(window);
}

/// VSCode's default: dark until the user picks a mode - a `--mode` switch,
/// or the settings page's Mode, which the settings file keeps. (The kit's
/// own default follows the OS.) The settings page shows Dark then, and a
/// save of the settings keeps it.
fn dark_by_default(kit_ref: &RefAny, args: &AppArgs) {
    if args.mode.is_some() {
        return;
    }
    let mut kit_ref = kit_ref.clone();
    let Some(mut k) = kit_ref.downcast_mut::<kit::Kit>() else {
        return;
    };
    let file = azul_appkit::data::local_path(&k.data_root, &k.settings_key());
    if !names_a_mode(&file) {
        k.settings.mode = ModePref::Dark;
    }
}

/// Whether the settings file at `path` names a mode (no file, a file that
/// is not JSON or an empty `mode`: no).
fn names_a_mode(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|file| file.get("mode").and_then(|m| m.as_str()).map(|m| !m.trim().is_empty()))
        .unwrap_or(false)
}

/// The recent folders the settings keep.
fn recent_of(kit_ref: &RefAny) -> Vec<String> {
    let mut kit_ref = kit_ref.clone();
    let recent = kit_ref
        .downcast_ref::<kit::Kit>()
        .and_then(|k| k.settings.get(commands::RECENT_KEY).map(workspace::recent_from_json));
    recent.unwrap_or_default()
}

// ==== The window ====

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode and the theme makes a switch of either rebuild the window; the layout's
    // language says the words (a switch of it too).
    let _mode = info.get_mode();
    let _theme = info.get_theme();
    azul_appkit::l10n::begin_layout(&info);
    let size = (info.get_window_width(), info.get_window_height());
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.window = size;
    }
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let st = &*guard;
    let content = if kit::settings_open(&st.kit) {
        ui::column(vec![kit::title_row(&st.title()), kit::settings_page(&st.kit, Vec::new())])
    } else {
        ui::window(&app, st)
    };
    let document = st.tabs.active().map_or_else(|| "AzCode".to_string(), |d| d.name.clone());
    let guarded = CloseGuard::create(content, AzString::from(document))
        .with_dirty(st.any_dirty())
        .with_asking(st.asking_close)
        .with_on_event(app.clone(), on_close_guard)
        .dom();
    ShellThemeScope::create(ui::column(vec![guarded]))
        .with_accent(ShellThemeAccent::Blue)
        .body()
        .with_menu_bar(menu::menu_bar(&app, st))
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
}

// ==== Callbacks ====

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    kit::on_window_created(&st.kit, &mut info);
    if let Some(root) = st.workspace_to_open.take() {
        commands::open_workspace(st, &mut info, &app, root);
    } else if st.sample {
        commands::open_sample(st, &mut info, &app);
    }
    if let Some(path) = st.file_to_open.take() {
        commands::open_path(st, &mut info, &app, &path);
    }
    // Far jumps are coloured on a Thread; the timer starts the walks.
    let timer = Timer::create(app.clone(), commands::highlight_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(250)));
    info.add_timer(TimerId::unique(), timer);
    println!("AZCODE_READY 1");
    Update::RefreshDom
}

/// The close guard: a close was asked while files have changes, or the
/// question was answered (Save writes every changed file, then closes).
extern "C" fn on_close_guard(mut data: RefAny, mut info: CallbackInfo, event: CloseGuardEvent) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    match event.kind {
        CloseGuardEventKind::Ask => st.asking_close = true,
        CloseGuardEventKind::Save => {
            st.asking_close = false;
            st.close_after_save = true;
            commands::save(st, &mut info, &handle);
        }
        CloseGuardEventKind::Discard | CloseGuardEventKind::Cancel => st.asking_close = false,
    }
    Update::RefreshDom
}

/// The window's keys: the kit's first (settings, F1), then AzCode's (the
/// commands, Mod+K chords). The editing keys are the CodeView's, the
/// terminal's keys the TerminalView's.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<AppState>().map(|s| s.kit.clone()) else {
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
    let modifiers = info.get_key_modifiers();
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if commands::handle_key(&mut guard, &mut info, &handle, key, modifiers) {
        info.prevent_default();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

#[cfg(test)]
mod tests {
    use super::SHORTCUTS;

    /// The keys `commands::handle_key` takes (Mod+O, Mod+P, Mod+B included: the welcome page
    /// promises them); the F1 list must show every one of them.
    #[test]
    fn the_shortcut_list_names_every_key_the_window_takes() {
        let window_keys = [
            "Mod+K Mod+O", "Mod+O", "Mod+K F", "Mod+P", "Mod+Shift+P", "Mod+B", "Mod+Shift+E", "Mod+S",
            "Mod+W", "Mod+F", "Mod+Shift+F", "Mod+H", "Mod+G", "F3", "Escape", "Ctrl+`", "Mod+J",
            "Ctrl+Shift+`",
        ];
        for key in window_keys {
            assert!(
                SHORTCUTS
                    .iter()
                    .any(|s| s.keys.split(" / ").any(|k| k == key)),
                "the F1 list does not name {key}"
            );
        }
    }

    /// Every command with keys names keys the F1 list shows too.
    #[test]
    fn every_command_with_keys_is_in_the_shortcut_list() {
        for action in crate::actions::Action::ALL {
            let keys = action.keys();
            if keys.is_empty() || keys == "Mod+," || keys == "F1" || keys == "Mod+Z" || keys == "Mod+Shift+Z" {
                continue;
            }
            assert!(
                SHORTCUTS.iter().any(|s| s.keys.split(" / ").any(|k| k == keys)),
                "{action:?}: {keys} is not in the F1 list"
            );
        }
    }

    /// AzCode is dark by default, as VSCode is: only a settings file that names a mode
    /// (the settings page's Mode writes it) or a `--mode` switch picks another.
    #[test]
    fn azcode_is_dark_until_the_settings_file_names_a_mode() {
        let dir = std::env::temp_dir().join(format!("azcode-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        let file = dir.join("settings.json");
        assert!(!super::names_a_mode(&file), "no file names no mode");
        for (text, named) in [
            ("{\"theme\": \"flat\", \"mode\": \"light\"}", true),
            ("{\"theme\": \"flat\", \"mode\": \"system\"}", true),
            ("{\"theme\": \"flat\", \"mode\": \"\"}", false),
            ("{\"theme\": \"flora\"}", false),
            ("not json", false),
        ] {
            std::fs::write(&file, text).expect("the settings file");
            assert_eq!(super::names_a_mode(&file), named, "{text}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
