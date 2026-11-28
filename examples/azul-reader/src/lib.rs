//! AzReader: an e-reader on the public azul API.
//!
//! EPUB books (the zip, the OPF package, the XHTML chapters with their CSS - [`epub`]) are read
//! through azul's own parsers (the strict XML loader, the HTML5-like parser as the fallback -
//! [`xmltree`]) into a reading tree ([`content`], the book's sheets fitted by [`bookcss`]) and
//! laid out ONCE per chapter by azul's paged layout with the reading break policy (whole
//! lines, widows / orphans - [`paginate`]); a page on screen is a clip window over that column
//! ([`position::PageMap`]). Plain text and HTML files are books too ([`plainbook`]).
//!
//! The window is like the Chrome PDF viewer's: a thin toolbar of icons over the page on a quiet
//! ground, with a strip of page thumbnails (the contents, the bookmarks) beside it on a desktop;
//! on a phone the page fills the screen, a tap in its middle shows the toolbar and a slider
//! under it that jumps through the book, a tap on its sides turns the page. It is inside a
//! `ShellThemeScope`, follows the app theme (flat / flora) and the OS mode; the PAPER (auto /
//! white / sepia / night) is the reader's own. The library is the shelves and a covers grid.
//!
//! Every book is a folder in the data tree (`reader/books/<uuid>/` - [`library`]) written
//! through azul-storage's `Drive` from an azul `Thread` ([`jobs`]); the reading settings are
//! azul-appkit's settings file. No network request is ever made.
//!
//! On stdout, for scripts (`scripts/azreader_e2e.py`): `AZREADER_READY` and the lines of
//! [`commands`].

pub mod app;
pub mod bookcss;
pub mod chrome;
pub mod commands;
pub mod content;
/// The ops of the scenario files and the debug server (the sample book).
pub mod e2e_ops;
pub mod epub;
pub mod ids;
pub mod jobs;
mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod library;
pub mod pagecache;
pub mod paginate;
pub mod plainbook;
pub mod position;
pub mod sample;
pub mod settings;
mod strip;
mod toolbar;
mod ui_library;
mod ui_reader;
mod ui_settings;
pub mod xmltree;

use azul::{
    app::App,
    callbacks::{
        CallbackInfo, LayoutCallbackInfo, RefAny, TimerCallbackInfo, TimerCallbackReturn, Update,
    },
    css::{DarkLightMode, EventFilter},
    dom::{Dom, VirtualKeyCode},
    font::FontCacheSnapshot,
    shells::{ShellThemeAccent, ShellThemeScope},
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    window::WindowEventFilter,
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec, LanguagePref},
    insets::{phone_window, SafeInsets},
    l10n::begin_layout,
    shortcuts::Shortcut,
    ui as kit,
};

pub use crate::app::AppState;
use crate::{
    app::{Command, Pane, Screen},
    settings::ReadingSettings,
};

// ==== The app's facts ====

pub const SCREENS: [&str; 2] = ["library", "reader"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzReader",
    binary: "AzReader",
    summary: "an e-reader for EPUB books, text and HTML files",
    screens: &SCREENS,
    files_help: "EPUB (.epub), text (.txt) or HTML (.html) files to add to the library",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzReader",
    version: env!("CARGO_PKG_VERSION"),
    summary: "An e-reader: EPUB books, text and HTML files on reflowable pages, a library of \
              covers, the table of contents, bookmarks and your reading position, kept as files \
              in your data folder.",
    license: "MIT",
    app_folder: library::APP_FOLDER,
};

/// The keyboard shortcuts the settings page lists: the groups and what the keys do are keys of
/// AzReader's words.
pub const SHORTCUTS: [Shortcut; 10] = [
    Shortcut::new(
        "azreader-category-reading",
        "Right / Down / PageDown / Space",
        "azreader-shortcut-next-page",
    ),
    Shortcut::new(
        "azreader-category-reading",
        "Left / Up / PageUp",
        "azreader-shortcut-previous-page",
    ),
    Shortcut::new(
        "azreader-category-reading",
        "Home",
        "azreader-shortcut-start",
    ),
    Shortcut::new(
        "azreader-category-reading",
        "Mod+D",
        "azreader-shortcut-bookmark",
    ),
    Shortcut::new("azreader-category-reading", "Mod+T", "azreader-contents"),
    Shortcut::new("azreader-category-reading", "Mod+B", "azreader-bookmarks"),
    Shortcut::new("azreader-category-reading", "Mod+P", "azreader-print"),
    Shortcut::new(
        "azreader-category-reading",
        "Mod+Plus / Mod+Minus",
        "azreader-shortcut-text-size",
    ),
    Shortcut::new("azreader-library", "Mod+O", "azreader-add-a-book"),
    Shortcut::new("azreader-library", "Mod+L", "azreader-shortcut-library"),
];

/// The app's own settings category (first on the settings page): an id, shown as
/// `azreader-category-reading`.
pub const SETTINGS_CATEGORY: &str = "Reading";

// ==== Start ====

pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[SETTINGS_CATEGORY], args.clone());
    // The words first: everything from here is said in the language chosen (Settings,
    // `--language`; the system's otherwise).
    let mut config = kit::app_config(&kit_ref);
    l10n::register(&mut config);
    e2e_ops::register_ops(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
    let (data_root, settings) = {
        let mut k = kit_ref.clone();
        let found = k.downcast_ref::<kit::Kit>().map(|k| {
            (
                k.data_root.clone(),
                ReadingSettings::from_settings(&k.settings),
            )
        });
        found.unwrap_or_default()
    };
    let mut st = AppState::new(kit_ref.clone(), data_root, settings, args.sample);
    st.import_on_start = args.files.clone();
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, Some(SETTINGS_CATEGORY));
    }
    let window = kit::window_options(
        &kit_ref,
        layout,
        (1180.0, 820.0),
        (640.0, 480.0),
        on_window_created,
    );
    App::create(RefAny::new(st), config).run(window);
}

// ==== The window ====

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // The window's language for every word this pass says.
    begin_layout(&info);
    // Reading the mode and the theme makes a switch of either rebuild the window.
    let mode = info.get_mode();
    let theme = info.get_theme();
    let size = (info.get_window_width(), info.get_window_height());
    let safe = SafeInsets::of(&info);
    let phone = phone_window(size.0, size.1);
    let app = data.clone();
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.dark = matches!(mode, DarkLightMode::Dark);
        // flora and its spins ("flora:green") - a switch lays the open chapter out again
        // (the theme is part of the layout key: flora's serif is another face).
        st.flora = theme.as_str() == "flora" || theme.as_str().starts_with("flora:");
        if st.window != size || st.phone != phone {
            st.window = size;
            st.phone = phone;
            if phone && !st.pane_chosen {
                // A phone keeps the page whole: the strip is a sheet the reader opens.
                st.pane = Pane::None;
            }
            // Measured again after this frame.
            st.area = None;
        }
        if st.fonts.is_none() {
            st.fonts = Some(FontCacheSnapshot::from_layout_info(&info));
        }
        st.dpi = info.get_dpi_factor();
    }
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let st = &*guard;
    let content = if kit::settings_open(&st.kit) {
        let mut page = Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
        if !phone && cfg!(not(any(target_os = "android", target_os = "ios"))) {
            page.add_child(kit::title_row(&st.title()));
        }
        page.with_child(kit::settings_page_with_reload(
            &st.kit,
            vec![ui_settings::section(&app, st)],
            &app,
            commands::reload_settings,
        ))
    } else {
        match st.screen {
            Screen::Library => ui_library::screen(&app, st),
            Screen::Reader => ui_reader::screen(&app, st),
        }
    };
    // The room the system keeps at the window's edges (a camera cutout, the home indicator).
    let root = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; {}",
            safe.padding()
        ))
        .with_child(content);
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; padding: 0px; height: 100%;")
        .with_child(
            ShellThemeScope::create(root)
                .with_accent(ShellThemeAccent::Blue)
                .dom(),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app,
            on_key,
        )
}

// ==== Callbacks ====

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    kit::on_window_created(&st.kit, &mut info);
    commands::scan_library(st, &mut info, &app);
    let files = st.import_on_start.clone();
    if !files.is_empty() {
        // `import_on_start` stays non-empty until the first one is in: it opens then.
        commands::import_files(st, &mut info, &app, files);
    }
    // The reading area is measured after each frame; a change lays the pages out again.
    let timer = Timer::create(app.clone(), area_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(400)));
    info.add_timer(TimerId::unique(), timer);
    println!("AZREADER_READY");
    Update::DoNothing
}

/// Measures the reading area (its node, found by its marker) and lays the pages out again
/// when it changed size, the settings changed or a chapter waits for the fonts.
extern "C" fn area_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let st = &mut *guard;
    if st.screen != Screen::Reader || kit::settings_open(&st.kit) {
        return TimerCallbackReturn::continue_unchanged();
    }
    let measured = info
        .callback_info
        .get_node_id_by_marker(st.area_marker.clone())
        .into_option()
        .and_then(|node| info.callback_info.get_node_rect(node).into_option())
        .map(|rect| (rect.size.width.floor(), rect.size.height.floor()))
        .filter(|(w, h)| *w > 0.0 && *h > 0.0);
    if let Some(area) = measured {
        st.area = Some(area);
    }
    // `ensure_layout` asks only for what is missing or stale (a chapter waiting for the fonts,
    // one laid out for other settings or another page size, the first one).
    commands::ensure_layout(st, &mut info.callback_info, &app);
    // The pages around the view, drawn ahead (the chapter's pictures are in the image cache by
    // now: they were added a callback ago).
    commands::ensure_pages_drawn(st, &mut info.callback_info, &app);
    TimerCallbackReturn::continue_unchanged()
}

/// The window's keys: the kit's first (settings, F1), then the reader's and the library's.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((kit_ref, screen)) = data
        .downcast_ref::<AppState>()
        .map(|s| (s.kit.clone(), s.screen))
    else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let primary = info.get_key_modifiers().primary_down();
    let reading = screen == Screen::Reader;
    let cmd = match (key, primary) {
        (VirtualKeyCode::O, true) => Command::AddBooks,
        (VirtualKeyCode::L, true) => Command::ShowLibrary,
        (
            VirtualKeyCode::Right
            | VirtualKeyCode::Down
            | VirtualKeyCode::PageDown
            | VirtualKeyCode::Space,
            false,
        ) if reading => Command::NextPage,
        (VirtualKeyCode::Left | VirtualKeyCode::Up | VirtualKeyCode::PageUp, false) if reading => {
            Command::PrevPage
        }
        (VirtualKeyCode::Home, false) if reading => Command::BookStart,
        (VirtualKeyCode::D, true) if reading => Command::ToggleBookmark,
        (VirtualKeyCode::T, true) if reading => Command::Pane(Pane::Contents),
        (VirtualKeyCode::P, true) if reading => Command::Print,
        (VirtualKeyCode::B, true) if reading => Command::Pane(Pane::Bookmarks),
        (VirtualKeyCode::Equals | VirtualKeyCode::Plus | VirtualKeyCode::NumpadAdd, true)
            if reading =>
        {
            Command::FontSize(2)
        }
        (VirtualKeyCode::Minus | VirtualKeyCode::NumpadSubtract, true) if reading => {
            Command::FontSize(-2)
        }
        (VirtualKeyCode::Escape, false) if reading => Command::ShowLibrary,
        _ => return Update::DoNothing,
    };
    info.prevent_default();
    commands::run(&mut data, cmd, &mut info)
}

/// The Android entry: the loaded library's constructor starts the app (the other phone apps do the same).
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
