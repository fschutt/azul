//! AzCode: a code editor on the public azul API, in VSCode's shape.
//!
//! The window is azul's `OfficeShell` with the S8 developer shell's panes
//! (the app-drawn `Titlebar` under `WindowDecorations::NoTitle`; the
//! activity bar, the side bar - the explorer or the search panel - the
//! editor with its tabs over azul's `CodeView`, the status bar, quick open)
//! inside a `ShellThemeScope`, behind a `CloseGuard` that asks "Save
//! changes?". Without a folder the explorer says "You have not yet opened a
//! folder." (Open Folder, the recent folders) and the editor shows the
//! welcome page (Open Folder..., Open File..., the keyboard shortcuts). It
//! is dark by default, as VSCode is, and follows the app theme (flat /
//! flora) and the mode the user picks.
//!
//! - [`buffer`]: the text of an open file, a piece table (plain Rust).
//! - [`highlight`]: syntect, incremental by line, checkpoints, a background
//!   walk for far jumps (plain Rust).
//! - [`search`]: find / replace (azul-appkit's matcher), go to line.
//! - [`workspace`]: the explorer's tree, the tabs, the recent folders,
//!   quick open's ranking (plain Rust).
//! - [`storage`]: the workspace's files through azul-storage's Drive on
//!   azul Threads; [`sample`]: the sample workspace (`--sample`).
//! - [`app`], [`commands`], [`ui`]: the state, the commands, the window.
//!
//! A workspace is a folder named on the command line (`AzCode ~/project`,
//! read and written in place through a drive without the data tree's
//! manifest), picked with Open Folder (Mod+O) or from the recent folders,
//! or the sample in the data tree (`code/sample/`). A file named on the
//! command line or picked with Open File... opens on its own (its folder is
//! its drive) when it is not in the workspace.
//!
//! On stdout, for scripts (`scripts/azcode_e2e.py`): `AZCODE_READY`,
//! `AZCODE_FOLDER <dir>`, `AZCODE_FILE <path>`, `AZCODE_LISTED <folder>
//! <n>`, `AZCODE_OPENED <key> <lines>`, `AZCODE_SAVED <key>`,
//! `AZCODE_INDEXED <n>`, `AZCODE_FOUND <n>`, `AZCODE_REPLACED <n>`.

pub mod app;
pub mod buffer;
pub mod commands;
pub mod highlight;
pub mod ids;
pub mod sample;
pub mod search;
pub mod storage;
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
    summary: "A code editor: the explorer, tabs, syntax colours, find and replace, go to line, files of a \
              million lines. Your folders are edited in place; the sample lives in your data folder.",
    license: "MIT",
    app_folder: sample::APP_FOLDER,
};

pub const SHORTCUTS: [Shortcut; 17] = [
    Shortcut::new("File", "Mod+O", "Open a folder"),
    Shortcut::new("File", "Mod+P", "Quick open a file of the folder"),
    Shortcut::new("File", "Mod+S", "Save every changed file"),
    Shortcut::new("File", "Mod+W", "Close the tab"),
    Shortcut::new("Window", "Mod+B", "Show / hide the side bar"),
    Shortcut::new("Find", "Mod+F", "Find in the file"),
    Shortcut::new("Find", "Mod+H", "Replace in the file"),
    Shortcut::new("Find", "F3 / Shift+F3", "Next / previous match"),
    Shortcut::new("Find", "Mod+G", "Go to line"),
    Shortcut::new("Editing", "Mod+Z / Mod+Shift+Z", "Undo / redo"),
    Shortcut::new("Editing", "Mod+D", "Select the word, then its next occurrence"),
    Shortcut::new("Editing", "Alt+click", "Another cursor"),
    Shortcut::new("Editing", "Tab / Shift+Tab", "Indent / outdent the selected lines"),
    Shortcut::new("Editing", "Mod+X / Mod+C / Mod+V", "Cut / copy / paste (a whole line without a selection)"),
    Shortcut::new("Moving", "Mod+Home / Mod+End", "To the start / end of the file"),
    Shortcut::new("Moving", "Alt+arrows (macOS) / Ctrl+arrows", "By words"),
    Shortcut::new("Window", "Escape", "Close the find bar, the go-to bar"),
];

// ==== Start ====

pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args.clone());
    dark_by_default(&kit_ref, &args);
    let data_root = {
        let mut k = kit_ref.clone();
        let root = k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone());
        root.unwrap_or_default()
    };
    let mut st = AppState::new(kit_ref.clone(), data_root, args.sample);
    st.recent = recent_of(&kit_ref);
    if let Some(named) = args.files.first() {
        // Absolute: `AzCode .` names the folder by its name, not ".".
        let path = std::path::absolute(named).unwrap_or_else(|_| named.clone());
        if path.is_dir() {
            st.workspace_to_open = Some(commands::folder_root(&path));
        } else if path.is_file() {
            st.file_to_open = Some(path);
        } else {
            st.notice = format!("{} is neither a folder nor a file.", path.display());
        }
    }
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let config = kit::app_config(&kit_ref);
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
    // Reading the mode and the theme makes a switch of either rebuild the window.
    let _mode = info.get_mode();
    let _theme = info.get_theme();
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

/// The window's keys: the kit's first (settings, F1), then AzCode's. The
/// editing keys are the CodeView's.
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
    let (primary, shift) = (modifiers.primary_down(), modifiers.shift);
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if commands::handle_key(&mut guard, &mut info, &handle, key, primary, shift) {
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
            "Mod+O", "Mod+P", "Mod+B", "Mod+S", "Mod+W", "Mod+F", "Mod+H", "Mod+G", "F3", "Escape",
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
