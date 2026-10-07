//! AzTerm: a terminal emulator on the public azul API.
//!
//! The window is the old iTerm's: the app-drawn `Titlebar` (the window is
//! `WindowDecorations::NoTitle`), under it a strip of tabs - one per session:
//! a close button, its title (what the shell set, else the shell and its
//! folder), on macOS its Cmd+number; the active one lit; a "+" for a new one
//! - and the terminal of the active tab filling the rest. The menu bar has
//! what is not on the strip (the text size, the settings, About). Inside a
//! `ShellThemeScope` it follows the app theme (flat / flora) and the OS
//! mode - the terminal's colours are the theme's palette
//! (`TerminalPalette::flat` / `::flora_ink`).
//!
//! Every tab is a [`session::Session`]: the user's shell on a PTY
//! (alacritty_terminal's tty + event loop), or with `--sample` a recorded
//! session replayed into a terminal with no PTY (it echoes what is typed).
//! A shell that ends closes its tab. The terminal is azul's `TerminalView`:
//! its data callback answers with [`vt::screen`] for the grid it has room
//! for (a new grid resizes the engine and the PTY), its events carry the
//! bytes for the program, the scroll and the selection gestures.
//!
//! OUTPUT IN FRAMES: the PTY's reader thread parses into the engine and
//! raises a flag; a 16 ms timer re-renders the terminal's view - only its
//! `VirtualView` (`trigger_virtual_view_rerender`, not every view of the
//! window, not the window's DOM) - when the flag is up: at most once a tick
//! however much output came, and on every other tick in a flood
//! ([`renders_now`]).
//!
//! On stdout, for scripts (`scripts/azterm_e2e.py`): `AZTERM_READY`,
//! `AZTERM_TABS <n>`, `AZTERM_ACTIVE <index>`, `AZTERM_HISTORY <index>
//! <lines>` (a new tab's scrollback), `AZTERM_SCROLL <offset>` (the view
//! after a scroll), `AZTERM_COPIED <chars>`, `AZTERM_EXITED <index>`.

pub mod ids;
pub mod sample;
pub mod session;
pub mod vt;

use std::{path::Path, sync::atomic::Ordering};

use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::{Column, Point, Side},
    selection::{Selection, SelectionType},
    term::viewport_to_point,
};
use azul::{
    app::App,
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, CallbackType, LayoutCallbackInfo, RefAny,
        TerminalViewDataSourceCallbackType, TerminalViewOnEventCallbackType, TimerCallbackInfo,
        TimerCallbackReturn, Update,
    },
    css::{EventFilter, HoverEventFilter},
    dom::{ClipboardContent, Dom, DomId, NodeId, VirtualKeyCode},
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionString,
    shells::{ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StyledTextRunVec,
    widgets::{
        AboutDialog, InfoBar, Modal, ModalState, StandardDialogEvent, TerminalGridSize,
        TerminalScreen, TerminalSelectionKind, TerminalView, TerminalViewEvent,
        TerminalViewEventKind,
    },
    window::WindowEventFilter,
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    shortcuts::Shortcut,
    ui as kit,
};

use crate::{session::Session, vt::GridSize};

// ==== The app's facts ====

pub const SCREENS: [&str; 2] = ["terminal", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzTerm",
    binary: "AzTerm",
    summary: "a terminal: your shell in tabs",
    screens: &SCREENS,
    files_help: "",
};

/// The data folder of the app (settings).
pub const APP_FOLDER: &str = "term";

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzTerm",
    version: env!("CARGO_PKG_VERSION"),
    summary:
        "A terminal: your shell in tabs, xterm keys, 256 and true colours, a scrollback of any \
              length. The terminal engine is alacritty_terminal.",
    license: "MIT",
    app_folder: APP_FOLDER,
};

pub const SHORTCUTS: [Shortcut; 9] = [
    Shortcut::new("Tabs", "Cmd+T / Ctrl+Shift+T", "New tab"),
    Shortcut::new("Tabs", "Cmd+W / Ctrl+Shift+W", "Close the tab"),
    Shortcut::new("Tabs", "Cmd+1..9 / Ctrl+Shift+1..9", "Tab 1 to 9"),
    Shortcut::new("Tabs", "Cmd+Shift+] / Ctrl+Shift+]", "Next tab"),
    Shortcut::new("Tabs", "Cmd+Shift+[ / Ctrl+Shift+[", "Previous tab"),
    Shortcut::new("Terminal", "Cmd+C / Ctrl+Shift+C", "Copy the selection"),
    Shortcut::new("Terminal", "Cmd+V / Ctrl+Shift+V", "Paste"),
    Shortcut::new(
        "Terminal",
        "The wheel, Shift+Page Up / Page Down / Home / End; on macOS Cmd+Up / Down / Page Up / \
         Page Down / Home / End",
        "Scroll the scrollback",
    ),
    Shortcut::new("Terminal", "Mod+= / Mod+-", "Larger / smaller text"),
];

/// Lines of scrollback a tab keeps.
pub const SCROLLBACK: usize = 10_000;
/// The font size a window starts with, px.
pub const FONT_SIZE: f32 = 13.0;
/// The smallest and the largest text, px.
const FONT_RANGE: (f32, f32) = (8.0, 32.0);
/// The grid a tab starts with, before the view says how much room it has.
const START_GRID: GridSize = GridSize {
    columns: 80,
    lines: 24,
};
/// Ticks in a row with new output before the terminal re-renders on every
/// other tick only ([`renders_now`]).
pub const FLOOD_TICKS: u32 = 3;

// ==== The state ====

/// One tab: its session and its title.
pub struct Tab {
    pub session: Session,
    pub title: String,
    pub exited: bool,
}

/// What a window chord does (Cmd+key on macOS, Ctrl+Shift+key elsewhere),
/// and the menu's items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKey {
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    /// Tab 1..9, the number as typed.
    Tab(usize),
    Bigger,
    Smaller,
}

/// The window chord of `key`, if it is one.
#[must_use]
pub fn window_key(key: VirtualKeyCode) -> Option<WindowKey> {
    use VirtualKeyCode as K;
    Some(match key {
        K::T => WindowKey::NewTab,
        K::W => WindowKey::CloseTab,
        K::RBracket => WindowKey::NextTab,
        K::LBracket => WindowKey::PreviousTab,
        K::Key1 => WindowKey::Tab(1),
        K::Key2 => WindowKey::Tab(2),
        K::Key3 => WindowKey::Tab(3),
        K::Key4 => WindowKey::Tab(4),
        K::Key5 => WindowKey::Tab(5),
        K::Key6 => WindowKey::Tab(6),
        K::Key7 => WindowKey::Tab(7),
        K::Key8 => WindowKey::Tab(8),
        K::Key9 => WindowKey::Tab(9),
        K::Equals | K::Plus => WindowKey::Bigger,
        K::Minus => WindowKey::Smaller,
        _ => return None,
    })
}

/// What Cmd + a navigation key does to the scrollback on macOS, the old
/// iTerm's way: a line, a page, the oldest line, the output.
#[must_use]
pub fn scroll_key(key: VirtualKeyCode) -> Option<Scroll> {
    use VirtualKeyCode as K;
    Some(match key {
        K::Up => Scroll::Delta(1),
        K::Down => Scroll::Delta(-1),
        K::PageUp => Scroll::PageUp,
        K::PageDown => Scroll::PageDown,
        K::Home => Scroll::Top,
        K::End => Scroll::Bottom,
        _ => return None,
    })
}

/// Whether the terminal re-renders on the `streak`-th tick in a row with
/// new output: at once for output that has just arrived (an echo, a
/// prompt), on every other tick in a flood (`tree`, a build log) - 30
/// frames a second is smooth for text going by and leaves the UI thread
/// room for the keys and the wheel. A skipped tick keeps the flag up, so
/// the next one draws it.
#[must_use]
pub const fn renders_now(streak: u32) -> bool {
    streak <= FLOOD_TICKS || streak % 2 == 0
}

/// A new shell's tab title until the shell sets one: the shell's name and
/// its folder, the home folder as `~` ("zsh ~", "bash ~/src").
#[must_use]
pub fn shell_title(shell: Option<&str>, cwd: Option<&Path>, home: Option<&Path>) -> String {
    let fallback = if cfg!(windows) { "powershell" } else { "shell" };
    let name = shell
        .and_then(|s| Path::new(s).file_name())
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(fallback);
    let Some(cwd) = cwd else {
        return name.to_string();
    };
    let folder = match home.and_then(|h| cwd.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => cwd.display().to_string(),
    };
    format!("{name} {folder}")
}

/// The app.
pub struct AppState {
    pub kit: RefAny,
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// `--sample`: tabs replay recordings instead of starting a shell.
    pub sample: bool,
    /// Alt (Option) sends ESC + the key (Linux / Windows), or types (macOS).
    pub alt_sends_escape: bool,
    pub font_size: f32,
    /// The last problem ("" = none).
    pub notice: String,
    pub about_open: bool,
    /// Ticks in a row the active terminal had new output on ([`renders_now`]).
    pub streak: u32,
}

impl AppState {
    pub fn new(kit: RefAny, sample: bool) -> Self {
        Self {
            kit,
            tabs: Vec::new(),
            active: 0,
            sample,
            alt_sends_escape: !cfg!(target_os = "macos"),
            font_size: FONT_SIZE,
            notice: String::new(),
            about_open: false,
            streak: 0,
        }
    }

    /// Opens a tab: the user's shell, or (sample) the next recording.
    pub fn open_tab(&mut self) {
        let index = self.tabs.len();
        let tab = if self.sample {
            let (title, bytes) = if index % 2 == 0 {
                ("zsh ~/azul-apps", sample::build_session())
            } else {
                ("ssh build: journalctl", sample::log_session(5_000))
            };
            Some(Tab {
                session: Session::replay(&bytes, START_GRID, SCROLLBACK),
                title: title.to_string(),
                exited: false,
            })
        } else {
            let cwd = home_dir();
            match Session::spawn(None, cwd.clone(), START_GRID, SCROLLBACK) {
                Ok(session) => Some(Tab {
                    session,
                    title: shell_title(
                        std::env::var("SHELL").ok().as_deref(),
                        cwd.as_deref(),
                        cwd.as_deref(),
                    ),
                    exited: false,
                }),
                Err(e) => {
                    self.notice = format!("The shell could not be started: {e}");
                    None
                }
            }
        };
        if let Some(tab) = tab {
            let history = tab.session.term.lock().grid().history_size();
            self.tabs.push(tab);
            self.active = index;
            self.notice.clear();
            println!("AZTERM_HISTORY {index} {history}");
        }
        self.touch_active();
        println!("AZTERM_TABS {}", self.tabs.len());
    }

    /// Closes tab `index` (its shell gets SIGHUP); the session that was
    /// active stays active, the one after a closed active tab takes over.
    pub fn close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if index < self.active {
                self.active -= 1;
            } else if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            }
        }
        self.touch_active();
        println!("AZTERM_TABS {}", self.tabs.len());
    }

    /// Activates tab `index`.
    pub fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.touch_active();
            println!("AZTERM_ACTIVE {index}");
        }
    }

    /// The tab after (`step` 1) or before (-1) the active one, wrapping.
    pub fn cycle_tab(&mut self, step: isize) {
        let n = self.tabs.len();
        if n == 0 {
            return;
        }
        let next = (self.active as isize + step).rem_euclid(n as isize) as usize;
        self.select_tab(next);
    }

    /// Does what a window chord or a menu item asks.
    pub fn apply(&mut self, key: WindowKey) {
        match key {
            WindowKey::NewTab => self.open_tab(),
            WindowKey::CloseTab => {
                let active = self.active;
                self.close_tab(active);
            }
            WindowKey::NextTab => self.cycle_tab(1),
            WindowKey::PreviousTab => self.cycle_tab(-1),
            WindowKey::Tab(number) => {
                if number > 0 {
                    self.select_tab(number - 1);
                }
            }
            WindowKey::Bigger => self.font_size = (self.font_size + 1.0).min(FONT_RANGE.1),
            WindowKey::Smaller => self.font_size = (self.font_size - 1.0).max(FONT_RANGE.0),
        }
    }

    /// Scrolls the active tab's view (Cmd + a navigation key); the caller
    /// re-renders it at once.
    pub fn scroll_active(&mut self, scroll: Scroll) {
        if let Some(tab) = self.tabs.get(self.active) {
            let mut term = tab.session.term.lock();
            term.scroll_display(scroll);
            println!("AZTERM_SCROLL {}", term.grid().display_offset());
            drop(term);
            // Drawn now, not once more on the tick (the scroll raised the flag).
            let _ = tab.session.signals.take_dirty();
        }
    }

    /// The active tab's screen is drawn on the next tick: a tab that was
    /// switched to shows its own screen, also where the engine kept the
    /// view of the window it rebuilt.
    fn touch_active(&mut self) {
        self.streak = 0;
        if let Some(tab) = self.tabs.get(self.active) {
            tab.session.signals.dirty.store(true, Ordering::Release);
        }
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }
}

/// The user's home folder (a new shell starts there).
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

// ==== Start ====

pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    // TERM / COLORTERM for the shells (before any thread exists).
    alacritty_terminal::tty::setup_env();
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args.clone());
    let mut st = AppState::new(kit_ref.clone(), args.sample);
    st.open_tab();
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(
        &kit_ref,
        layout,
        (960.0, 600.0),
        (480.0, 320.0),
        on_window_created,
    );
    App::create(RefAny::new(st), config).run(window);
}

// ==== The window ====

fn column(children: Vec<Dom>) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    for child in children {
        column.add_child(child);
    }
    column
}

/// What a tab (and its close button) carries: the app and the tab.
struct TabClick {
    app: RefAny,
    index: usize,
}

/// The strip: a band a shade under the window, the tabs side by side.
const TAB_BAR_CSS: &str = "display: flex; flex-direction: row; align-items: stretch; \
     flex-shrink: 0; height: 28px; background: system:under-page-background; \
     border-bottom: 1px solid system:separator; font-size: 12px; cursor: default; \
     user-select: none;";
/// A tab: an equal share of the strip up to a width - its close button, its
/// title in the middle, its number.
const TAB_CSS: &str = "display: flex; flex-direction: row; align-items: center; \
     flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 64px; max-width: 260px; \
     padding: 0px 6px; border-right: 1px solid system:separator; \
     color: system:secondary-text; \
     :hover { background: rgba(127, 127, 127, 0.14); }";
/// The active tab: lit, the window's own ground.
const TAB_ACTIVE_CSS: &str = "display: flex; flex-direction: row; align-items: center; \
     flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 64px; max-width: 260px; \
     padding: 0px 6px; border-right: 1px solid system:separator; \
     color: system:text; background: system:window-background;";
/// A tab's close button, at its start (the old iTerm's place).
const TAB_CLOSE_CSS: &str = "width: 16px; height: 16px; flex-shrink: 0; display: flex; \
     align-items: center; justify-content: center; border-radius: 3px; font-size: 13px; \
     line-height: 16px; color: system:tertiary-text; \
     :hover { background: rgba(127, 127, 127, 0.28); color: system:text; }";
/// A tab's title.
const TAB_TITLE_CSS: &str = "flex-grow: 1; min-width: 0px; overflow: hidden; \
     white-space: nowrap; text-overflow: ellipsis; text-align: center; padding: 0px 4px;";
/// A tab's chord ("⌘2").
const TAB_NUMBER_CSS: &str = "flex-shrink: 0; color: system:tertiary-text; font-size: 11px;";
/// The "+" after the tabs.
const NEW_TAB_CSS: &str = "width: 28px; flex-shrink: 0; display: flex; align-items: center; \
     justify-content: center; font-size: 16px; color: system:secondary-text; \
     :hover { background: rgba(127, 127, 127, 0.14); color: system:text; }";
/// The box the terminal fills.
const PANE_CSS: &str = "position: relative; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; \
     min-height: 0px; min-width: 0px;";

/// The chord that selects tab `index`, shown on it the old iTerm's way
/// (macOS: "⌘1" .. "⌘9"; elsewhere none - "Ctrl+Shift+1" is too long).
fn number_label(index: usize) -> Option<String> {
    (cfg!(target_os = "macos") && index < 9).then(|| format!("\u{2318}{}", index + 1))
}

/// One tab of the strip.
fn tab_dom(app: &RefAny, index: usize, tab: &Tab, active: bool) -> Dom {
    let click = RefAny::new(TabClick {
        app: app.clone(),
        index,
    });
    let close = Dom::create_div_with_text(AzString::from("\u{00d7}"))
        .with_id(AzString::from(format!("{}{index}", ids::TAB_CLOSE.as_str())))
        .with_css(TAB_CLOSE_CSS)
        .with_accessibility_name(AzString::from(format!("Close {}", tab.title)))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            click.clone(),
            on_tab_close_click,
        );
    let title =
        Dom::create_div_with_text(AzString::from(tab.title.as_str())).with_css(TAB_TITLE_CSS);
    let mut dom = Dom::create_div()
        .with_id(AzString::from(format!("{}{index}", ids::TAB.as_str())))
        .with_css(if active { TAB_ACTIVE_CSS } else { TAB_CSS })
        .with_accessibility_name(AzString::from(tab.title.as_str()))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            click,
            on_tab_click,
        )
        .with_child(close)
        .with_child(title);
    if let Some(label) = number_label(index) {
        dom.add_child(Dom::create_div_with_text(AzString::from(label)).with_css(TAB_NUMBER_CSS));
    }
    dom
}

/// The strip of tabs, the old iTerm's: one per session, the active one lit,
/// a "+" for a new one after them.
fn tab_bar(app: &RefAny, st: &AppState) -> Dom {
    let mut bar = Dom::create_div().with_id(ids::TABS).with_css(TAB_BAR_CSS);
    for (i, tab) in st.tabs.iter().enumerate() {
        bar.add_child(tab_dom(app, i, tab, i == st.active));
    }
    bar.add_child(
        Dom::create_div_with_text(AzString::from("+"))
            .with_id(ids::NEW_TAB)
            .with_css(NEW_TAB_CSS)
            .with_accessibility_name(AzString::from("New tab"))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                app.clone(),
                on_new_tab,
            ),
    );
    bar
}

/// The last problem, until dismissed.
fn notice_bar(app: &RefAny, st: &AppState) -> Dom {
    InfoBar::create(AzString::from(st.notice.as_str()))
        .with_action(AzString::from("Dismiss"))
        .with_on_action(app.clone(), on_dismiss_notice as ButtonOnClickCallbackType)
        .dom()
        .with_id(ids::NOTICE)
}

/// The terminal of the active tab, in a positioned box it fills.
fn pane(app: &RefAny, st: &AppState) -> Dom {
    let pane = Dom::create_div().with_id(ids::PANE).with_css(PANE_CSS);
    let Some(tab) = st.tabs.get(st.active) else {
        return pane.with_child(
            ShellEmptyState::create(AzString::from("No tab is open"))
                .with_action_label(AzString::from("New tab"))
                .with_on_action(app.clone(), on_new_tab as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::EMPTY),
        );
    };
    let view = TerminalView::create()
        .with_data_source(
            app.clone(),
            terminal_screen as TerminalViewDataSourceCallbackType,
        )
        .with_on_event(app.clone(), on_terminal as TerminalViewOnEventCallbackType)
        .with_font_size(st.font_size)
        .with_accessibility_name(AzString::from(tab.title.as_str()))
        .with_id(ids::TERMINAL)
        .dom();
    pane.with_child(view)
}

/// The menu bar: what is not on the strip of tabs.
fn menu_bar(app: &RefAny) -> Menu {
    let item = |label: &str, callback: CallbackType| {
        MenuItem::string(StringMenuItem::create(label).with_callback(app.clone(), callback))
    };
    Menu::create(vec![
        MenuItem::string(StringMenuItem::create("Shell").with_children(vec![
            item("New Tab", on_new_tab),
            item("Close Tab", on_close_tab),
            MenuItem::separator(),
            item("Next Tab", on_next_tab),
            item("Previous Tab", on_previous_tab),
        ])),
        MenuItem::string(StringMenuItem::create("View").with_children(vec![
            item("Bigger Text", on_bigger_text),
            item("Smaller Text", on_smaller_text),
        ])),
        MenuItem::string(StringMenuItem::create("Help").with_children(vec![
            item("Settings\u{2026}", on_settings),
            item("About AzTerm", on_about_open),
        ])),
    ])
}

/// The About box (open while `about_open`).
fn about_modal(app: &RefAny, st: &AppState) -> Dom {
    let about = AboutDialog::create(
        AzString::from(ABOUT.name),
        AzString::from(format!("Version {}", ABOUT.version)),
    )
    .with_icon(AzString::from("terminal"))
    .with_description(AzString::from(ABOUT.summary))
    .with_copyright(AzString::from("Copyright 2026 the azul contributors"))
    .with_credit(AzString::from("azul"), AzString::from("MIT"))
    .with_credit(
        AzString::from("alacritty_terminal"),
        AzString::from("Apache-2.0"),
    )
    .with_on_event(app.clone(), on_about)
    .dom()
    .with_id(ids::ABOUT);
    Modal::create(about)
        .with_title(AzString::from("About AzTerm"))
        .with_open(st.about_open)
        .with_on_close(app.clone(), on_modal_close)
        .dom()
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode and the theme makes a switch of either rebuild the
    // window (the terminal palette is the theme's).
    let _mode = info.get_mode();
    let _theme = info.get_theme();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let st = &*guard;
    let title = st
        .tabs
        .get(st.active)
        .map_or_else(|| "AzTerm".to_string(), |t| format!("{} - AzTerm", t.title));
    let content = if kit::settings_open(&st.kit) {
        column(vec![
            kit::title_row(&title),
            kit::settings_page(&st.kit, Vec::new()),
        ])
    } else {
        let mut children = vec![kit::title_row(&title), tab_bar(&app, st)];
        if !st.notice.is_empty() {
            children.push(notice_bar(&app, st));
        }
        children.push(pane(&app, st));
        column(children)
    };
    let root = column(vec![content, about_modal(&app, st)]);
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; padding: 0px; height: 100%;")
        .with_menu_bar(menu_bar(&app))
        .with_child(
            ShellThemeScope::create(root)
                .with_accent(ShellThemeAccent::Slate)
                .dom(),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app,
            on_key,
        )
}

// ==== The terminal's callbacks ====

/// The data callback: the active tab's screen for the grid the view has
/// room for (a new grid resizes the engine and the PTY).
extern "C" fn terminal_screen(mut data: RefAny, size: TerminalGridSize) -> TerminalScreen {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return TerminalScreen::empty();
    };
    let alt = st.alt_sends_escape;
    let Some(tab) = st.active_tab_mut() else {
        return TerminalScreen::empty();
    };
    let columns = usize::try_from(size.columns).unwrap_or(usize::MAX);
    let rows = usize::try_from(size.rows).unwrap_or(usize::MAX);
    tab.session.resize(GridSize::new(columns, rows));
    let term = tab.session.term.lock();
    let screen = vt::screen(&*term, alt);
    drop(term);
    screen
}

/// The engine point of a cell in view.
fn grid_point<T>(term: &alacritty_terminal::Term<T>, event: &TerminalViewEvent) -> Point {
    let offset = term.grid().display_offset();
    let last_line = term.screen_lines().saturating_sub(1);
    let last_column = term.columns().saturating_sub(1);
    let line = usize::try_from(event.point.line)
        .unwrap_or(usize::MAX)
        .min(last_line);
    let column = usize::try_from(event.point.column)
        .unwrap_or(usize::MAX)
        .min(last_column);
    viewport_to_point(offset, Point::new(line, Column(column)))
}

/// The view's actions: bytes to the program, the scroll, the selection,
/// the copy.
extern "C" fn on_terminal(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: TerminalViewEvent,
) -> Update {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(tab) = st.active_tab_mut() else {
        return Update::DoNothing;
    };
    if event.kind == TerminalViewEventKind::Input {
        tab.session.write(event.bytes.as_slice().to_vec());
        return Update::DoNothing;
    }
    let mut term = tab.session.term.lock();
    let side = if event.right_half {
        Side::Right
    } else {
        Side::Left
    };
    match event.kind {
        TerminalViewEventKind::Scroll => {
            let now = i64::try_from(term.grid().display_offset()).unwrap_or(0);
            let delta = i64::from(event.scroll) - now;
            term.scroll_display(Scroll::Delta(i32::try_from(delta).unwrap_or(0)));
            println!("AZTERM_SCROLL {}", term.grid().display_offset());
        }
        TerminalViewEventKind::SelectStart => {
            let kind = match event.selection_kind {
                TerminalSelectionKind::Word => SelectionType::Semantic,
                TerminalSelectionKind::Line => SelectionType::Lines,
                TerminalSelectionKind::Block => SelectionType::Block,
                TerminalSelectionKind::Simple => SelectionType::Simple,
            };
            let point = grid_point(&*term, &event);
            term.selection = Some(Selection::new(kind, point, side));
        }
        TerminalViewEventKind::SelectExtend => {
            let point = grid_point(&*term, &event);
            if let Some(selection) = term.selection.as_mut() {
                selection.update(point, side);
            }
        }
        TerminalViewEventKind::SelectClear => term.selection = None,
        TerminalViewEventKind::Copy => {
            let text = term.selection_to_string();
            drop(term);
            if let Some(text) = text.filter(|t| !t.is_empty()) {
                println!("AZTERM_COPIED {}", text.chars().count());
                info.set_clipboard_content(ClipboardContent {
                    plain_text: AzString::from(text.as_str()),
                    styled_runs: StyledTextRunVec::create(),
                    html: OptionString::None,
                });
            }
            return Update::DoNothing;
        }
        TerminalViewEventKind::SelectEnd | TerminalViewEventKind::Input => {}
    }
    drop(term);
    // The view re-renders itself after a scroll or a selection, reading the
    // engine as it is then: the tick need not draw the same screen again
    // (the engine's own scroll raised the flag).
    let _ = tab.session.signals.take_dirty();
    Update::DoNothing
}

// ==== Callbacks ====

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(st) = data.downcast_ref::<AppState>() else {
        return Update::DoNothing;
    };
    kit::on_window_created(&st.kit, &mut info);
    // The engine's changes reach the view (and a title the window).
    let timer = Timer::create(app, output_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(16)));
    info.add_timer(TimerId::unique(), timer);
    println!("AZTERM_READY");
    Update::DoNothing
}

/// The node with the terminal's id in the window's DOM.
fn terminal_node(info: &CallbackInfo) -> Option<(DomId, NodeId)> {
    let dom = DomId { inner: 0 };
    // `into_raw` is the 1-based encoding (0 = none); `NodeId` is 0-based.
    let raw = info.get_node_id_by_id_attribute(dom, ids::TERMINAL).into_raw();
    (raw != 0).then(|| (dom, NodeId { inner: raw - 1 }))
}

/// Re-renders the terminal's view alone - the `VirtualView` inside the node
/// with the terminal's id - not every view of the window (a title bar's
/// maximize glyph, an icon view: each was rebuilt with it on every frame of
/// output).
fn rerender_terminal(info: &mut CallbackInfo) {
    let Some((dom, host)) = terminal_node(info) else {
        return;
    };
    let view = info.get_first_child_node(dom, host).into_raw();
    if view == 0 {
        info.trigger_all_virtual_view_rerender();
    } else {
        info.trigger_virtual_view_rerender(dom, NodeId { inner: view - 1 });
    }
}

/// Gives the terminal the keys when nothing has them - at the start, after
/// a click on the strip, after a tab closed: a terminal's window types into
/// its terminal. (`autofocus` is honoured in popups only.)
fn focus_terminal(info: &mut CallbackInfo) {
    if info.get_focused_node().into_option().is_some() {
        return;
    }
    if let Some((dom, node)) = terminal_node(info) {
        info.set_focus_to_node(dom, node);
    }
}

/// Every 16 ms: new output re-renders the terminal's view (only it, at most
/// once, every other tick in a flood); a new title or an ended shell (its
/// tab closes) rebuilds the window; the terminal gets the keys when nothing
/// has them.
extern "C" fn output_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let active = st.active;
    let mut output = false;
    let mut rebuild = false;
    let mut ended = Vec::new();
    for (i, tab) in st.tabs.iter_mut().enumerate() {
        let signals = &tab.session.signals;
        // The active tab's flag is taken when its view re-renders (a flood
        // skips ticks); a tab in the back has no view to draw.
        if i == active {
            output = signals.dirty.load(Ordering::Acquire);
        } else {
            let _ = signals.take_dirty();
        }
        if let Some(title) = signals.take_title() {
            if !title.is_empty() && title != tab.title {
                tab.title = title;
                rebuild = true;
            }
        }
        let _ = signals.take_bell();
        if !tab.exited && signals.exited.load(Ordering::Acquire) {
            tab.exited = true;
            ended.push(i);
            println!("AZTERM_EXITED {i}");
        }
    }
    // A shell that ended closes its tab.
    for i in ended.into_iter().rev() {
        st.close_tab(i);
        rebuild = true;
    }
    st.streak = if output { st.streak.saturating_add(1) } else { 0 };
    let render = output && renders_now(st.streak);
    if render {
        if let Some(tab) = st.tabs.get(st.active) {
            let _ = tab.session.signals.take_dirty();
        }
    }
    let focus = !st.about_open && !kit::settings_open(&st.kit);
    drop(st);
    if render {
        rerender_terminal(&mut info.callback_info);
    }
    if focus {
        focus_terminal(&mut info.callback_info);
    }
    if rebuild {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// `key` on the app, the window rebuilt.
fn apply_key(data: &mut RefAny, key: WindowKey) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.apply(key);
    }
    Update::RefreshDom
}

extern "C" fn on_new_tab(mut data: RefAny, _info: CallbackInfo) -> Update {
    apply_key(&mut data, WindowKey::NewTab)
}

extern "C" fn on_close_tab(mut data: RefAny, _info: CallbackInfo) -> Update {
    apply_key(&mut data, WindowKey::CloseTab)
}

extern "C" fn on_next_tab(mut data: RefAny, _info: CallbackInfo) -> Update {
    apply_key(&mut data, WindowKey::NextTab)
}

extern "C" fn on_previous_tab(mut data: RefAny, _info: CallbackInfo) -> Update {
    apply_key(&mut data, WindowKey::PreviousTab)
}

extern "C" fn on_bigger_text(mut data: RefAny, _info: CallbackInfo) -> Update {
    apply_key(&mut data, WindowKey::Bigger)
}

extern "C" fn on_smaller_text(mut data: RefAny, _info: CallbackInfo) -> Update {
    apply_key(&mut data, WindowKey::Smaller)
}

extern "C" fn on_settings(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(st) = data.downcast_ref::<AppState>() {
        kit::open_settings(&st.kit, None);
    }
    Update::RefreshDom
}

extern "C" fn on_about_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.about_open = true;
    }
    Update::RefreshDom
}

extern "C" fn on_dismiss_notice(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.notice.clear();
    }
    Update::RefreshDom
}

/// The app and the tab a tab's click carries.
fn tab_click(data: &mut RefAny) -> Option<(RefAny, usize)> {
    data.downcast_ref::<TabClick>()
        .map(|c| (c.app.clone(), c.index))
}

extern "C" fn on_tab_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = tab_click(&mut data) else {
        return Update::DoNothing;
    };
    if let Some(mut st) = app.downcast_mut::<AppState>() {
        st.select_tab(index);
    }
    Update::RefreshDom
}

extern "C" fn on_tab_close_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = tab_click(&mut data) else {
        return Update::DoNothing;
    };
    // The tab under the button must not take the click as well.
    info.stop_propagation();
    if let Some(mut st) = app.downcast_mut::<AppState>() {
        st.close_tab(index);
    }
    Update::RefreshDom
}

extern "C" fn on_about(
    mut data: RefAny,
    _info: CallbackInfo,
    _event: StandardDialogEvent,
) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.about_open = false;
    }
    Update::RefreshDom
}

extern "C" fn on_modal_close(mut data: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.about_open = false;
    }
    Update::RefreshDom
}

/// The window's keys: the kit's first (settings, F1), then the window
/// chords (Cmd+key on macOS, Ctrl+Shift+key elsewhere - the terminal leaves
/// those alone): new / close tab, tab 1..9, next / previous tab, the text
/// size. The terminal keeps every other key (it stops them itself).
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
    let Some(key) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let mac = cfg!(target_os = "macos");
    let chord = if mac {
        m.meta && !m.ctrl && !m.alt
    } else {
        m.ctrl && m.shift && !m.alt && !m.meta
    };
    if !chord {
        return Update::DoNothing;
    }
    // macOS: Cmd + a navigation key scrolls the scrollback - the view alone
    // re-renders, the window stays.
    if let Some(scroll) = scroll_key(key).filter(|_| mac) {
        if let Some(mut st) = data.downcast_mut::<AppState>() {
            st.scroll_active(scroll);
        }
        info.prevent_default();
        rerender_terminal(&mut info);
        return Update::DoNothing;
    }
    let Some(action) = window_key(key) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    apply_key(&mut data, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit() -> RefAny {
        RefAny::new(0u8)
    }

    #[test]
    fn sample_tabs_open_close_and_cycle() {
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        st.open_tab();
        st.open_tab();
        assert_eq!(st.tabs.len(), 3);
        assert_eq!(st.active, 2);
        st.cycle_tab(1);
        assert_eq!(st.active, 0);
        st.cycle_tab(-1);
        assert_eq!(st.active, 2);
        st.close_tab(2);
        assert_eq!(st.tabs.len(), 2);
        assert_eq!(st.active, 1);
        assert!(!st.tabs[0].session.is_live());
    }

    #[test]
    fn closing_a_tab_before_the_active_one_keeps_the_active_session() {
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        st.open_tab();
        st.open_tab();
        st.tabs[2].title = "third".to_string();
        st.close_tab(0);
        assert_eq!(st.tabs.len(), 2);
        assert_eq!(st.active, 1);
        assert_eq!(st.tabs[st.active].title, "third");
        // Closing the active one: the one after it takes over, or the last.
        st.close_tab(1);
        assert_eq!(st.active, 0);
        st.close_tab(0);
        assert!(st.tabs.is_empty());
        assert_eq!(st.active, 0);
    }

    #[test]
    fn the_window_chords_open_close_and_pick_tabs_and_size_the_text() {
        use VirtualKeyCode as K;
        assert_eq!(window_key(K::T), Some(WindowKey::NewTab));
        assert_eq!(window_key(K::W), Some(WindowKey::CloseTab));
        assert_eq!(window_key(K::Key1), Some(WindowKey::Tab(1)));
        assert_eq!(window_key(K::Key9), Some(WindowKey::Tab(9)));
        assert_eq!(window_key(K::RBracket), Some(WindowKey::NextTab));
        assert_eq!(window_key(K::LBracket), Some(WindowKey::PreviousTab));
        assert_eq!(window_key(K::Key0), None);
        assert_eq!(window_key(K::A), None);
        let mut st = AppState::new(kit(), true);
        st.apply(WindowKey::NewTab);
        st.apply(WindowKey::NewTab);
        st.apply(WindowKey::NewTab);
        st.apply(WindowKey::Tab(1));
        assert_eq!(st.active, 0);
        st.apply(WindowKey::Tab(3));
        assert_eq!(st.active, 2);
        // A tab that is not there changes nothing.
        st.apply(WindowKey::Tab(9));
        assert_eq!(st.active, 2);
        st.apply(WindowKey::Tab(0));
        assert_eq!(st.active, 2);
        st.apply(WindowKey::CloseTab);
        assert_eq!(st.tabs.len(), 2);
        for _ in 0..40 {
            st.apply(WindowKey::Bigger);
        }
        assert!((st.font_size - FONT_RANGE.1).abs() < f32::EPSILON);
        for _ in 0..40 {
            st.apply(WindowKey::Smaller);
        }
        assert!((st.font_size - FONT_RANGE.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cmd_and_a_navigation_key_scroll_the_active_tabs_scrollback() {
        use VirtualKeyCode as K;
        assert!(matches!(scroll_key(K::Up), Some(Scroll::Delta(1))));
        assert!(matches!(scroll_key(K::Down), Some(Scroll::Delta(-1))));
        assert!(matches!(scroll_key(K::PageUp), Some(Scroll::PageUp)));
        assert!(matches!(scroll_key(K::PageDown), Some(Scroll::PageDown)));
        assert!(matches!(scroll_key(K::Home), Some(Scroll::Top)));
        assert!(matches!(scroll_key(K::End), Some(Scroll::Bottom)));
        assert!(scroll_key(K::T).is_none());
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        // The log: thousands of lines of scrollback.
        st.open_tab();
        let offset = |st: &AppState| {
            st.tabs[st.active]
                .session
                .term
                .lock()
                .grid()
                .display_offset()
        };
        st.scroll_active(Scroll::PageUp);
        assert_eq!(offset(&st), START_GRID.lines);
        st.scroll_active(Scroll::Delta(1));
        assert_eq!(offset(&st), START_GRID.lines + 1);
        st.scroll_active(Scroll::Bottom);
        assert_eq!(offset(&st), 0);
    }

    #[test]
    fn a_flood_of_output_is_drawn_on_every_other_tick_and_a_little_at_once() {
        // Output that has just arrived: drawn on the tick it came.
        for streak in 1..=FLOOD_TICKS {
            assert!(renders_now(streak), "tick {streak}");
        }
        // A flood: every other tick, never two skipped in a row.
        let drawn: Vec<bool> = (FLOOD_TICKS + 1..FLOOD_TICKS + 9).map(renders_now).collect();
        assert_eq!(drawn.iter().filter(|d| **d).count(), 4);
        assert!(drawn.windows(2).all(|w| w[0] || w[1]));
    }

    #[test]
    fn a_new_shell_is_titled_by_its_name_and_folder() {
        let home = Path::new("/home/u");
        assert_eq!(shell_title(Some("/bin/zsh"), Some(home), Some(home)), "zsh ~");
        let src = home.join("src");
        assert_eq!(
            shell_title(Some("/usr/bin/bash"), Some(src.as_path()), Some(home)),
            "bash ~/src"
        );
        assert_eq!(shell_title(Some("fish"), None, None), "fish");
        assert_eq!(
            shell_title(Some("/bin/zsh"), Some(Path::new("/tmp")), Some(home)),
            format!("zsh {}", Path::new("/tmp").display())
        );
    }

    #[test]
    fn the_data_callback_resizes_the_session_to_the_views_grid() {
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        let mut data = RefAny::new(st);
        let screen = terminal_screen(
            data.clone(),
            TerminalGridSize {
                columns: 100,
                rows: 30,
            },
        );
        assert_eq!(screen.lines.as_slice().len(), 30);
        let st = data.downcast_ref::<AppState>().expect("the app");
        assert_eq!(st.tabs[0].session.size(), GridSize::new(100, 30));
    }

    #[test]
    fn a_switched_tab_is_drawn_on_the_next_tick() {
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        st.open_tab();
        for tab in &st.tabs {
            let _ = tab.session.signals.take_dirty();
        }
        st.streak = 7;
        st.select_tab(0);
        assert!(st.tabs[0].session.signals.take_dirty());
        assert!(!st.tabs[1].session.signals.take_dirty());
        assert_eq!(st.streak, 0);
    }
}
