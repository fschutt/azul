//! AzTerm: a terminal emulator on the public azul API.
//!
//! The window is the old iTerm's, without a title row: the window is
//! `WindowDecorations::NoTitle` and its strip of tabs IS the title bar
//! (`kit::tabs_in_titlebar`: a grab strip above the tabs, room for macOS's
//! traffic lights before the first; everything around the tabs moves the
//! window, a double click zooms it). One tab per session, every one as wide
//! (180 px, down to 110 px when they are many, then the strip scrolls
//! sideways - the wheel too - and the active tab is scrolled into it): a
//! close button, its title (what the shell set, else the shell and its
//! folder), on macOS its Cmd+number; the active one lit; a "+" for a new one
//! beside them, always in reach - and the terminal of the active tab filling
//! the rest. The menu bar has what is not on the strip (the text size, the
//! settings, About). Inside a `ShellThemeScope` it follows the app theme
//! (flat / flora) and the OS mode - the terminal's colours are the theme's
//! palette (`TerminalPalette::flat` / `::flora_ink`).
//!
//! Every tab is a [`session::Session`]: the user's shell on a PTY
//! (alacritty_terminal's tty + event loop), or with `--sample` a recorded
//! session replayed into a terminal with no PTY (it echoes what is typed).
//! A shell that ends closes its tab. Closing the LAST tab - Cmd+W, its x,
//! `exit` in its shell alike - closes the window, as iTerm does: a window
//! never stands without a tab. The terminal is azul's `TerminalView`:
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
//! SCROLLED UP, THE VIEW STAYS: the view scrolls by pixels (a trackpad's
//! momentum glides; [`scroll::ViewScroll`] keeps the tab's slide beside the
//! engine's whole-line display offset). Scrolled up while output streams
//! in, the view stays where the user put it - the engine raises its offset
//! with every line, the content in view does not move - and it is drawn a
//! few times a second only ([`renders_scrolled_up`]), its round follow
//! button counting the lines that came in below. The button, the wheel back
//! to the bottom, Shift+End or typing follow the output again.
//!
//! On stdout, for scripts (`scripts/azterm_e2e.py`): `AZTERM_READY`,
//! `AZTERM_TABS <n>`, `AZTERM_ACTIVE <index>`, `AZTERM_HISTORY <index>
//! <lines>` (a new tab's scrollback), `AZTERM_SCROLL <offset>` (the view
//! after a scroll), `AZTERM_FOLLOW <1|0>` (the active view starts / stops
//! following the output), `AZTERM_STREAM <index>` / `AZTERM_STREAMED
//! <index>` (the sample shell's `seq N` / `yes | head -n N` started /
//! ended), `AZTERM_COPIED <chars>`, `AZTERM_EXITED <index>`, `AZTERM_CLOSE`
//! (the last tab closed: the window closes).

pub mod ids;
pub mod sample;
pub mod scroll;
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
    dom::{
        ClipboardContent, Dom, DomId, DomNodeId, NodeId, ScrollIntoViewOptions, VirtualKeyCode,
    },
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionString,
    shells::{ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StyledTextRunVec,
    widgets::{
        AboutDialog, InfoBar, Modal, ModalState, StandardDialogEvent, TabsInTitlebar,
        TerminalGridSize, TerminalScreen, TerminalSelectionKind, TerminalView, TerminalViewEvent,
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

use crate::{scroll::ViewScroll, session::Session, vt::GridSize};

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
/// Ticks a tab that was opened or picked is scrolled into the strip on.
const REVEAL_TICKS: u8 = 2;
/// Ticks between two frames of a view scrolled up while output comes in
/// ([`renders_scrolled_up`]).
pub const SCROLLED_UP_TICKS: u32 = 15;

// ==== The state ====

/// One tab: its session, its title, its view beyond the engine's display
/// offset (the slide, the lines that came in below it).
pub struct Tab {
    pub session: Session,
    pub title: String,
    pub exited: bool,
    pub view: ViewScroll,
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

/// Whether a view scrolled up is drawn again `since_render` ticks after
/// its last frame while output comes in: what is in view does not change -
/// the engine keeps it, the view stays where the user put it - only the
/// count of new lines on its follow button and its thumb do, so four frames
/// a second, not one for every chunk of a flood.
#[must_use]
pub const fn renders_scrolled_up(since_render: u32) -> bool {
    since_render >= SCROLLED_UP_TICKS
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
    /// Ticks the active tab is still to be scrolled into the strip on (a
    /// tab opened or picked; twice, as the rebuilt strip may be laid out
    /// after the first).
    pub reveal_ticks: u8,
    /// Ticks since the terminal's view was last drawn
    /// ([`renders_scrolled_up`]).
    pub since_render: u32,
    /// Whether the active view follows the output, as last printed
    /// (`AZTERM_FOLLOW`).
    pub follow_reported: Option<bool>,
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
            reveal_ticks: 0,
            since_render: 0,
            follow_reported: None,
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
                view: ViewScroll::default(),
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
                    view: ViewScroll::default(),
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
    /// Returns whether that was the last tab: the window closes with it, as
    /// iTerm's does - a window never stands without a tab.
    pub fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        self.touch_active();
        println!("AZTERM_TABS {}", self.tabs.len());
        self.tabs.is_empty()
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

    /// Does what a window chord or a menu item asks; returns whether the
    /// window closes (the last tab closed).
    pub fn apply(&mut self, key: WindowKey) -> bool {
        match key {
            WindowKey::NewTab => self.open_tab(),
            WindowKey::CloseTab => {
                let active = self.active;
                return self.close_tab(active);
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
        false
    }

    /// Scrolls the active tab's view (Cmd + a navigation key) by whole
    /// lines, from where it is; the caller re-renders it at once.
    pub fn scroll_active(&mut self, scroll: Scroll) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let mut term = tab.session.term.lock();
        tab.view.note(term.grid().display_offset());
        term.scroll_display(scroll);
        let now = term.grid().display_offset();
        drop(term);
        println!("AZTERM_SCROLL {now}");
        tab.view.moved_to(now, 0.0);
        // Drawn now, not once more on the tick (the scroll raised the flag).
        let _ = tab.session.signals.take_dirty();
        self.report_follow(now == 0);
    }

    /// The active tab's view to `lines` lines up, slid up by `fraction` of
    /// a line: the view's `Scroll`, worked out from the screen it showed
    /// last. Applied as the move it is from there onto where the view is
    /// now - output may have come in meanwhile, raising the engine's offset
    /// under a view that stayed put - or, for 0, to the output, following
    /// it. Returns the display offset the view is at.
    pub fn scroll_view(&mut self, lines: u32, fraction: f32) -> usize {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return 0;
        };
        let mut term = tab.session.term.lock();
        let offset = term.grid().display_offset();
        tab.view.note(offset);
        let to = f64::from(lines) - f64::from(fraction);
        let (target, slide) = tab.view.target(to, offset, term.grid().history_size());
        let delta = i32::try_from(target)
            .unwrap_or(i32::MAX)
            .saturating_sub(i32::try_from(offset).unwrap_or(i32::MAX));
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
        let now = term.grid().display_offset();
        drop(term);
        tab.view.moved_to(now, slide);
        println!("AZTERM_SCROLL {now}");
        self.report_follow(now == 0);
        now
    }

    /// `bytes` typed (pasted, reported) into the active tab: to its program,
    /// and its view back at the output - typing follows it again.
    pub fn write_active(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        let index = self.active;
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        let was_streaming = tab.session.streaming();
        tab.session.write(bytes);
        tab.view.follow();
        if !was_streaming && tab.session.streaming() {
            println!("AZTERM_STREAM {index}");
        }
        self.report_follow(true);
    }

    /// Prints `AZTERM_FOLLOW 1` / `0` when the active view starts or stops
    /// following the output (for scripts).
    fn report_follow(&mut self, following: bool) {
        if self.follow_reported != Some(following) {
            self.follow_reported = Some(following);
            println!("AZTERM_FOLLOW {}", u8::from(following));
        }
    }

    /// The active tab's screen is drawn on the next tick: a tab that was
    /// switched to shows its own screen, also where the engine kept the
    /// view of the window it rebuilt. And the strip scrolls it into view.
    fn touch_active(&mut self) {
        self.streak = 0;
        self.reveal_ticks = REVEAL_TICKS;
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

/// The row of tabs under the strip's grab strip, px.
const TAB_ROW_PX: f32 = 28.0;
/// A tab's width while the strip has room for every tab (iTerm's fixed
/// width): when it has not, all of them shrink alike down to
/// [`TAB_MIN_PX`], then the strip scrolls sideways. `TAB_CSS` and
/// `TAB_ACTIVE_CSS` say it in CSS.
pub const TAB_PX: f32 = 180.0;
/// The narrowest a tab gets: its x, a few letters of its title, its number.
pub const TAB_MIN_PX: f32 = 110.0;

/// A CSS length of `v` px (none for nonsense).
fn css_px(v: f32) -> f32 {
    if v.is_finite() {
        v.max(0.0)
    } else {
        0.0
    }
}

/// The strip of tabs as the window's TITLE BAR (the window is `NoTitle`,
/// `kit::tabs_in_titlebar` - `TabsInTitlebar::platform()`): the grab strip
/// above the row of tabs and the room for the window controls beside it
/// (macOS's traffic lights before the first tab) are its padding, and
/// everything in it that is not a tab or a button - above the tabs, before
/// and after them - moves the window (`-azul-app-region: drag`; a double
/// click zooms it).
fn strip_css(chrome: TabsInTitlebar) -> String {
    format!(
        "display: flex; flex-direction: row; align-items: stretch; flex-shrink: 0; \
         box-sizing: border-box; height: {height}px; padding-top: {top}px; \
         padding-left: {left}px; padding-right: {right}px; \
         background: system:under-page-background; border-bottom: 1px solid system:separator; \
         font-size: 12px; cursor: default; user-select: none; -azul-app-region: drag;",
        height = css_px(TAB_ROW_PX + chrome.top) + 1.0,
        top = css_px(chrome.top),
        left = css_px(chrome.left),
        right = css_px(chrome.right),
    )
}

/// The tabs' scroller: as wide as its tabs while the strip has room, then
/// what is left of it, scrolled sideways - a wheel over it too (the engine
/// turns a vertical wheel over a box that only scrolls sideways). No bar:
/// the active tab is scrolled into it.
const TAB_SCROLLER_CSS: &str = "display: flex; flex-direction: row; align-items: stretch; \
     flex-grow: 0; flex-shrink: 1; flex-basis: auto; min-width: 0px; \
     overflow-x: auto; overflow-y: hidden; scrollbar-width: none;";
/// A tab: every one as wide ([`TAB_PX`], down to [`TAB_MIN_PX`] when they
/// are many) - its close button, its title in the middle, its number. A tab
/// is a tab, not the title bar.
const TAB_CSS: &str = "display: flex; flex-direction: row; align-items: center; \
     flex-grow: 0; flex-shrink: 1; width: 180px; min-width: 110px; max-width: 180px; \
     box-sizing: border-box; padding: 0px 6px; border-right: 1px solid system:separator; \
     color: system:secondary-text; -azul-app-region: no-drag; \
     :hover { background: rgba(127, 127, 127, 0.14); }";
/// The active tab: lit, the window's own ground.
const TAB_ACTIVE_CSS: &str = "display: flex; flex-direction: row; align-items: center; \
     flex-grow: 0; flex-shrink: 1; width: 180px; min-width: 110px; max-width: 180px; \
     box-sizing: border-box; padding: 0px 6px; border-right: 1px solid system:separator; \
     color: system:text; background: system:window-background; -azul-app-region: no-drag;";
/// A tab's close button, at its start (the old iTerm's place).
const TAB_CLOSE_CSS: &str = "width: 16px; height: 16px; flex-shrink: 0; display: flex; \
     align-items: center; justify-content: center; border-radius: 3px; font-size: 13px; \
     line-height: 16px; color: system:tertiary-text; -azul-app-region: no-drag; \
     :hover { background: rgba(127, 127, 127, 0.28); color: system:text; }";
/// A tab's title.
const TAB_TITLE_CSS: &str = "flex-grow: 1; min-width: 0px; overflow: hidden; \
     white-space: nowrap; text-overflow: ellipsis; text-align: center; padding: 0px 4px;";
/// A tab's chord ("⌘2").
const TAB_NUMBER_CSS: &str = "flex-shrink: 0; color: system:tertiary-text; font-size: 11px;";
/// The "+" after the tabs - beside their scroller, so it stays in reach
/// however many there are.
const NEW_TAB_CSS: &str = "width: 28px; flex-shrink: 0; display: flex; align-items: center; \
     justify-content: center; font-size: 16px; color: system:secondary-text; \
     -azul-app-region: no-drag; \
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

/// The strip of tabs, the window's title bar: one tab per session, every
/// one as wide, the active one lit, in a scroller; a "+" for a new one after
/// them.
fn tab_bar(app: &RefAny, st: &AppState) -> Dom {
    let mut tabs = Dom::create_div()
        .with_id(ids::TABS_SCROLLER)
        .with_css(TAB_SCROLLER_CSS);
    for (i, tab) in st.tabs.iter().enumerate() {
        tabs.add_child(tab_dom(app, i, tab, i == st.active));
    }
    let mut bar = Dom::create_div()
        .with_id(ids::TABS)
        .with_css(strip_css(kit::tabs_in_titlebar()).as_str())
        .with_child(tabs);
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

/// The terminal of the active tab, in a positioned box it fills. No tab is
/// open only when the first shell could not start (closing the last tab
/// closes the window): the notice says why, the empty state offers another.
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
        // The settings page has no strip of tabs: the app-drawn title row.
        column(vec![
            kit::title_row(&title),
            kit::settings_page(&st.kit, Vec::new()),
        ])
    } else {
        // No title row: the strip of tabs is the title bar.
        let mut children = vec![tab_bar(&app, st)];
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
/// room for (a new grid resizes the engine and the PTY), with the tab's
/// slide and the lines that came in below its view; what the view shows
/// from now on (its scroll events are worked out from it).
extern "C" fn terminal_screen(mut data: RefAny, size: TerminalGridSize) -> TerminalScreen {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return TerminalScreen::empty();
    };
    let alt = st.alt_sends_escape;
    st.since_render = 0;
    let Some(tab) = st.active_tab_mut() else {
        return TerminalScreen::empty();
    };
    let columns = usize::try_from(size.columns).unwrap_or(usize::MAX);
    let rows = usize::try_from(size.rows).unwrap_or(usize::MAX);
    tab.session.resize(GridSize::new(columns, rows));
    let term = tab.session.term.lock();
    let offset = term.grid().display_offset();
    let mut screen = vt::screen(&*term, alt);
    drop(term);
    tab.view.note(offset);
    screen.scroll_fraction = if offset > 0 { tab.view.fraction } else { 0.0 };
    screen.new_lines = tab.view.new_lines;
    tab.view.shown = tab.view.up(offset);
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
    match event.kind {
        // Typing follows the output again.
        TerminalViewEventKind::Input => {
            st.write_active(event.bytes.as_slice().to_vec());
            return Update::DoNothing;
        }
        // By pixels, from the screen the view showed (see `scroll_view`).
        TerminalViewEventKind::Scroll => {
            st.scroll_view(event.scroll, event.scroll_fraction);
            // The view re-renders itself now: the tick need not draw the
            // same screen again (the engine's own scroll raised the flag).
            if let Some(tab) = st.tabs.get(st.active) {
                let _ = tab.session.signals.take_dirty();
            }
            return Update::DoNothing;
        }
        _ => {}
    }
    let Some(tab) = st.active_tab_mut() else {
        return Update::DoNothing;
    };
    let mut term = tab.session.term.lock();
    let side = if event.right_half {
        Side::Right
    } else {
        Side::Left
    };
    match event.kind {
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
        TerminalViewEventKind::SelectEnd
        | TerminalViewEventKind::Input
        | TerminalViewEventKind::Scroll => {}
    }
    drop(term);
    // The view re-renders itself after a selection, reading the engine as
    // it is then: the tick need not draw the same screen again.
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

/// Scrolls the strip so tab `index` is in view (the strip scrolls sideways
/// when the tabs do not fit).
fn reveal_tab(info: &mut CallbackInfo, index: usize) {
    let dom = DomId { inner: 0 };
    let id = format!("{}{index}", ids::TAB.as_str());
    let node = info.get_node_id_by_id_attribute(dom, AzString::from(id.as_str()));
    if node.into_raw() != 0 {
        info.scroll_node_into_view(DomNodeId { dom, node }, ScrollIntoViewOptions::nearest());
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
        // The sample shell's running command writes its next lines.
        if tab.session.streaming() {
            tab.session.pump();
            if !tab.session.streaming() {
                println!("AZTERM_STREAMED {i}");
            }
        }
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
    // A shell that ended closes its tab; the last one closes the window.
    let mut closes = false;
    for i in ended.into_iter().rev() {
        closes |= st.close_tab(i);
        rebuild = true;
    }
    if closes {
        drop(st);
        println!("AZTERM_CLOSE");
        info.callback_info.close_window();
        return TimerCallbackReturn::terminate_unchanged();
    }
    // The view follows the output (offset 0), or it was scrolled up: then
    // what is in view stays put - the engine raises its offset with every
    // line that comes in, counted for the follow button - and it is drawn
    // only now and then, not for every chunk of a flood.
    let shown = st.active;
    let following = st.tabs.get_mut(shown).map_or(true, |tab| {
        let offset = tab.session.term.lock().grid().display_offset();
        tab.view.note(offset);
        offset == 0
    });
    st.report_follow(following);
    st.streak = if output { st.streak.saturating_add(1) } else { 0 };
    st.since_render = st.since_render.saturating_add(1);
    let render = output
        && if following {
            renders_now(st.streak)
        } else {
            renders_scrolled_up(st.since_render)
        };
    if render {
        if let Some(tab) = st.tabs.get(st.active) {
            let _ = tab.session.signals.take_dirty();
        }
    }
    let focus = !st.about_open && !kit::settings_open(&st.kit);
    // A tab opened or picked is scrolled into the strip - in the strip as
    // it is laid out (not on a tick that rebuilds it).
    let reveal = (st.reveal_ticks > 0 && !rebuild).then(|| {
        st.reveal_ticks -= 1;
        st.active
    });
    drop(st);
    if render {
        rerender_terminal(&mut info.callback_info);
    }
    if focus {
        focus_terminal(&mut info.callback_info);
    }
    if let Some(index) = reveal {
        reveal_tab(&mut info.callback_info, index);
    }
    if rebuild {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// What a change to the tabs leaves: the window rebuilt - or, the last tab
/// closed, the window closed (never a window without a tab).
fn after_tabs(info: &mut CallbackInfo, closes: bool) -> Update {
    if closes {
        println!("AZTERM_CLOSE");
        info.close_window();
        Update::DoNothing
    } else {
        Update::RefreshDom
    }
}

/// `key` on the app, the window rebuilt (or closed with its last tab).
fn apply_key(data: &mut RefAny, info: &mut CallbackInfo, key: WindowKey) -> Update {
    let closes = data
        .downcast_mut::<AppState>()
        .is_some_and(|mut st| st.apply(key));
    after_tabs(info, closes)
}

extern "C" fn on_new_tab(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply_key(&mut data, &mut info, WindowKey::NewTab)
}

extern "C" fn on_close_tab(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply_key(&mut data, &mut info, WindowKey::CloseTab)
}

extern "C" fn on_next_tab(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply_key(&mut data, &mut info, WindowKey::NextTab)
}

extern "C" fn on_previous_tab(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply_key(&mut data, &mut info, WindowKey::PreviousTab)
}

extern "C" fn on_bigger_text(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply_key(&mut data, &mut info, WindowKey::Bigger)
}

extern "C" fn on_smaller_text(mut data: RefAny, mut info: CallbackInfo) -> Update {
    apply_key(&mut data, &mut info, WindowKey::Smaller)
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
    let closes = app
        .downcast_mut::<AppState>()
        .is_some_and(|mut st| st.close_tab(index));
    after_tabs(&mut info, closes)
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
    apply_key(&mut data, &mut info, action)
}

#[cfg(test)]
mod tests {
    use azul::option::OptionTerminalLine;

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
    fn closing_the_last_tab_closes_the_window_and_no_other_does() {
        // iTerm's way: a window never stands empty.
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        st.open_tab();
        assert!(!st.close_tab(1), "another tab is left");
        assert!(st.close_tab(0), "the last tab closes the window");
        assert!(st.tabs.is_empty());
        // Cmd+W on the only tab: the same.
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        assert!(st.apply(WindowKey::CloseTab));
        // A tab that is not there closes nothing.
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        assert!(!st.close_tab(3));
        assert_eq!(st.tabs.len(), 1);
        // Another window chord never closes the window.
        assert!(!st.apply(WindowKey::NewTab));
        assert!(!st.apply(WindowKey::NextTab));
    }

    #[test]
    fn the_strip_is_the_title_bar_clear_of_the_window_controls() {
        let css = strip_css(TabsInTitlebar::create(8.0, 78.0, 0.0));
        assert!(css.contains("-azul-app-region: drag;"), "{css}");
        assert!(css.contains("padding-top: 8px;"), "{css}");
        assert!(css.contains("padding-left: 78px;"), "{css}");
        assert!(css.contains("padding-right: 0px;"), "{css}");
        // The grab strip is new room above the row of tabs (and its line).
        assert!(css.contains("height: 37px;"), "{css}");
        // The tabs and the buttons stay what they are.
        for part in [TAB_CSS, TAB_ACTIVE_CSS, TAB_CLOSE_CSS, NEW_TAB_CSS] {
            assert!(part.contains("-azul-app-region: no-drag;"), "{part}");
        }
    }

    #[test]
    fn every_tab_is_as_wide_and_the_strip_scrolls_when_they_do_not_fit() {
        let width = format!("width: {TAB_PX}px;");
        let least = format!("min-width: {TAB_MIN_PX}px;");
        for part in [TAB_CSS, TAB_ACTIVE_CSS] {
            assert!(part.contains(&width), "{part}");
            assert!(part.contains(&least), "{part}");
            assert!(part.contains("flex-grow: 0;"), "{part}");
            assert!(part.contains("flex-shrink: 1;"), "{part}");
        }
        assert!(TAB_MIN_PX < TAB_PX);
        // The tabs scroll sideways (a wheel over them too: the engine turns
        // a vertical wheel over a box that only scrolls sideways); the "+"
        // is outside the scroller, always in reach.
        assert!(TAB_SCROLLER_CSS.contains("overflow-x: auto;"));
        assert!(TAB_SCROLLER_CSS.contains("overflow-y: hidden;"));
        assert!(TAB_SCROLLER_CSS.contains("min-width: 0px;"));
        assert!(NEW_TAB_CSS.contains("flex-shrink: 0;"));
    }

    #[test]
    fn a_new_or_picked_tab_is_scrolled_into_the_strip() {
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        assert!(st.reveal_ticks > 0);
        st.reveal_ticks = 0;
        st.open_tab();
        assert!(st.reveal_ticks > 0);
        st.reveal_ticks = 0;
        st.select_tab(0);
        assert!(st.reveal_ticks > 0);
        st.reveal_ticks = 0;
        st.cycle_tab(1);
        assert!(st.reveal_ticks > 0);
    }

    /// The text of every row of `screen`.
    fn rows_of(screen: &TerminalScreen) -> Vec<String> {
        screen
            .lines
            .as_slice()
            .iter()
            .map(|l| {
                l.runs
                    .as_slice()
                    .iter()
                    .map(|r| r.text.as_str().to_string())
                    .collect()
            })
            .collect()
    }

    const GRID: TerminalGridSize = TerminalGridSize {
        columns: 80,
        rows: 24,
    };

    /// `f` on the app inside `data`.
    fn with_app<R>(data: &mut RefAny, f: impl FnOnce(&mut AppState) -> R) -> R {
        let mut st = data.downcast_mut::<AppState>().expect("the app");
        f(&mut st)
    }

    #[test]
    fn the_data_callback_slides_the_rows_and_gives_the_line_below_off_the_output() {
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        // The log: thousands of lines of scrollback.
        st.open_tab();
        let mut data = RefAny::new(st);
        let at_output = terminal_screen(data.clone(), GRID);
        assert_eq!(at_output.scroll, 0);
        assert!(at_output.scroll_fraction.abs() < f32::EPSILON);
        assert!(matches!(at_output.line_below, OptionTerminalLine::None));
        // The view asks for 5 lines up, slid by a quarter of a line.
        with_app(&mut data, |st| st.scroll_view(5, 0.25));
        let up = terminal_screen(data.clone(), GRID);
        assert_eq!(up.scroll, 5);
        assert!((up.scroll_fraction - 0.25).abs() < 1e-6);
        assert!(matches!(up.line_below, OptionTerminalLine::Some(_)));
        // The line below is the first of the five under the rows.
        let OptionTerminalLine::Some(below) = &up.line_below else {
            unreachable!()
        };
        let five_up = rows_of(&up);
        with_app(&mut data, |st| st.scroll_view(4, 0.0));
        let four_up = rows_of(&terminal_screen(data.clone(), GRID));
        assert_eq!(four_up[..23], five_up[1..]);
        let below_text: String = below
            .runs
            .as_slice()
            .iter()
            .map(|r| r.text.as_str().to_string())
            .collect();
        assert_eq!(four_up[23], below_text);
    }

    #[test]
    fn output_while_scrolled_up_keeps_the_view_still_and_counts_the_new_lines() {
        let mut st = AppState::new(kit(), true);
        // The build session: a prompt waiting. `seq 500` streams 500 lines,
        // a few each tick.
        st.open_tab();
        st.write_active(b"seq 500".to_vec());
        st.write_active(b"\r".to_vec());
        assert!(st.tabs[0].session.pump());
        let mut data = RefAny::new(st);
        let _ = terminal_screen(data.clone(), GRID);
        with_app(&mut data, |st| st.scroll_view(10, 0.5));
        let before = terminal_screen(data.clone(), GRID);
        assert_eq!(before.new_lines, 0);
        // Output streams in; the scrolled-up view is not drawn meanwhile.
        for _ in 0..3 {
            assert!(with_app(&mut data, |st| st.tabs[0].session.pump()));
        }
        let tick = u32::try_from(sample::STREAM_LINES_PER_TICK).expect("a few");
        // What is in view did not move: drawn now, it is the same rows.
        let still = terminal_screen(data.clone(), GRID);
        assert_eq!(rows_of(&still), rows_of(&before), "the view stays put");
        assert!((still.scroll_fraction - 0.5).abs() < 1e-6, "to the pixel");
        assert_eq!(still.scroll, before.scroll + 3 * tick);
        assert_eq!(still.new_lines, 3 * tick);
        // More output, then a wheel step the view worked out from the screen
        // it showed (`still`): one line up from what is in view - not to
        // the offset the engine had then, lines away from it now.
        assert!(with_app(&mut data, |st| st.tabs[0].session.pump()));
        with_app(&mut data, |st| st.scroll_view(still.scroll + 1, 0.5));
        let one_more = terminal_screen(data.clone(), GRID);
        assert_eq!(one_more.scroll, still.scroll + tick + 1);
        assert_eq!(rows_of(&one_more)[1..], rows_of(&still)[..23]);
        assert_eq!(one_more.new_lines, 4 * tick, "a scroll is no output");
        // The follow button (a scroll to the output): following again.
        with_app(&mut data, |st| st.scroll_view(0, 0.0));
        let following = terminal_screen(data.clone(), GRID);
        assert_eq!(following.scroll, 0);
        assert!(following.scroll_fraction.abs() < f32::EPSILON);
        assert_eq!(following.new_lines, 0);
    }

    #[test]
    fn typing_follows_the_output_again() {
        let mut st = AppState::new(kit(), true);
        st.open_tab();
        st.open_tab();
        let mut data = RefAny::new(st);
        let _ = terminal_screen(data.clone(), GRID);
        with_app(&mut data, |st| st.scroll_view(40, 0.75));
        assert_eq!(terminal_screen(data.clone(), GRID).scroll, 40);
        with_app(&mut data, |st| st.write_active(b"l".to_vec()));
        let typed = terminal_screen(data.clone(), GRID);
        assert_eq!(typed.scroll, 0);
        assert!(typed.scroll_fraction.abs() < f32::EPSILON);
    }

    #[test]
    fn a_scrolled_up_view_is_drawn_a_few_times_a_second_not_on_every_chunk() {
        // Following, a flood is drawn every other tick; scrolled up, what is
        // in view does not change - only the count and the thumb, now and
        // then.
        let (mut since, mut drawn) = (0, 0);
        for _ in 0..60 {
            since += 1;
            if renders_scrolled_up(since) {
                drawn += 1;
                since = 0;
            }
        }
        assert!((2..=6).contains(&drawn), "{drawn} frames a second");
        assert!(!renders_scrolled_up(1));
        assert!(renders_scrolled_up(SCROLLED_UP_TICKS));
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
