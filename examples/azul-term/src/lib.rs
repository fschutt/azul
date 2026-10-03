//! AzTerm: a terminal emulator on the public azul API.
//!
//! The window is the S8 `DeveloperShell` (the app-drawn `Titlebar` under
//! `WindowDecorations::NoTitle`, an activity bar, the side bar listing the
//! tabs, the terminal, a status bar) inside a `ShellThemeScope`; it follows
//! the app theme (flat / flora) and the OS mode - the terminal's colours are
//! the theme's palette (`TerminalPalette::flat` / `::flora_ink`).
//!
//! Every tab is a [`session::Session`]: the user's shell on a PTY
//! (alacritty_terminal's tty + event loop), or with `--sample` a recorded
//! session replayed into a terminal with no PTY (it echoes what is typed).
//! The terminal is azul's `TerminalView`: its data callback answers with
//! [`vt::screen`] for the grid it has room for (a new grid resizes the
//! engine and the PTY), its events carry the bytes for the program, the
//! scroll and the selection gestures. A 16 ms timer re-renders the view
//! (only the view: `trigger_all_virtual_view_rerender`) when the engine
//! says the screen changed.
//!
//! On stdout, for scripts (`scripts/azterm_e2e.py`): `AZTERM_READY`,
//! `AZTERM_TABS <n>`, `AZTERM_ACTIVE <index>`, `AZTERM_COPIED <chars>`,
//! `AZTERM_EXITED <index>`.

pub mod ids;
pub mod sample;
pub mod session;
pub mod vt;

use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::{Column, Point, Side},
    selection::{Selection, SelectionType},
    term::viewport_to_point,
};
use azul::{
    app::App,
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, LayoutCallbackInfo, RefAny,
        TerminalViewDataSourceCallbackType, TerminalViewOnEventCallbackType, TimerCallbackInfo,
        TimerCallbackReturn, Update,
    },
    css::EventFilter,
    dom::{ClipboardContent, Dom, VirtualKeyCode},
    option::OptionString,
    shells::{DeveloperShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StyledTextRunVec,
    widgets::{
        AboutDialog, Button, Modal, ModalState, StandardDialogEvent, StatusBar, StatusBarSegment,
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

pub const SHORTCUTS: [Shortcut; 8] = [
    Shortcut::new("Tabs", "Ctrl+Shift+T / Cmd+T", "New tab"),
    Shortcut::new("Tabs", "Ctrl+Shift+W / Cmd+W", "Close the tab"),
    Shortcut::new("Tabs", "Ctrl+Shift+] / Cmd+Shift+]", "Next tab"),
    Shortcut::new("Tabs", "Ctrl+Shift+[ / Cmd+Shift+[", "Previous tab"),
    Shortcut::new("Terminal", "Ctrl+Shift+C / Cmd+C", "Copy the selection"),
    Shortcut::new("Terminal", "Ctrl+Shift+V / Cmd+V", "Paste"),
    Shortcut::new(
        "Terminal",
        "Shift+Page Up / Page Down",
        "Scroll the scrollback",
    ),
    Shortcut::new("Terminal", "Mod+= / Mod+-", "Larger / smaller text"),
];

/// Lines of scrollback a tab keeps.
pub const SCROLLBACK: usize = 10_000;
/// The font size a window starts with, px.
pub const FONT_SIZE: f32 = 13.0;
/// The grid a tab starts with, before the view says how much room it has.
const START_GRID: GridSize = GridSize {
    columns: 80,
    lines: 24,
};

// ==== The state ====

/// One tab: its session and its title.
pub struct Tab {
    pub session: Session,
    pub title: String,
    pub exited: bool,
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
            match Session::spawn(None, home_dir(), START_GRID, SCROLLBACK) {
                Ok(session) => Some(Tab {
                    session,
                    title: "shell".to_string(),
                    exited: false,
                }),
                Err(e) => {
                    self.notice = format!("The shell could not be started: {e}");
                    None
                }
            }
        };
        if let Some(tab) = tab {
            self.tabs.push(tab);
            self.active = index;
        }
        println!("AZTERM_TABS {}", self.tabs.len());
    }

    /// Closes tab `index` (its shell gets SIGHUP).
    pub fn close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            }
        }
        println!("AZTERM_TABS {}", self.tabs.len());
    }

    /// Activates tab `index`.
    pub fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
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

/// What a tab button carries: the app and the tab.
struct TabClick {
    app: RefAny,
    index: usize,
}

/// The activity bar: a new tab, the settings.
fn activity_bar(app: &RefAny) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; align-items: center; padding: 6px 0px;")
        .with_child(
            Button::create(AzString::from("+"))
                .with_on_click(app.clone(), on_new_tab as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::NEW_TAB),
        )
        .with_child(
            Button::create(AzString::from("..."))
                .with_on_click(app.clone(), on_settings as ButtonOnClickCallbackType)
                .dom()
                .with_css("margin-top: 6px;"),
        )
}

/// The side bar: the tabs (the active one toggled).
fn side_bar(app: &RefAny, st: &AppState) -> Dom {
    let mut list = Dom::create_div()
        .with_id(ids::TABS)
        .with_css("display: flex; flex-direction: column; padding: 6px;");
    for (i, tab) in st.tabs.iter().enumerate() {
        let title = if tab.exited {
            format!("{} (ended)", tab.title)
        } else {
            tab.title.clone()
        };
        list.add_child(
            Button::create(AzString::from(title))
                .with_toggled(i == st.active)
                .with_on_click(
                    RefAny::new(TabClick {
                        app: app.clone(),
                        index: i,
                    }),
                    on_tab_click as ButtonOnClickCallbackType,
                )
                .dom()
                .with_id(AzString::from(format!("{}{i}", ids::TAB.as_str())))
                .with_css("margin-bottom: 4px;"),
        );
    }
    list
}

/// The terminal of the active tab, in a positioned box it fills.
fn editor(app: &RefAny, st: &AppState) -> Dom {
    let Some(tab) = st.tabs.get(st.active) else {
        return ShellEmptyState::create(AzString::from("No tab is open"))
            .with_action_label(AzString::from("New tab"))
            .with_on_action(app.clone(), on_new_tab as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::EMPTY);
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
    Dom::create_div()
        .with_id(ids::PANE)
        .with_css(
            "position: relative; flex-grow: 1; min-height: 0px; min-width: 0px; height: 100%;",
        )
        .with_child(view)
}

/// The status bar: the tab, the grid, the scrollback, a notice.
fn status_bar(st: &AppState) -> Dom {
    let mut segments = Vec::new();
    if let Some(tab) = st.tabs.get(st.active) {
        let kind = if tab.session.is_live() {
            "shell"
        } else {
            "recording"
        };
        segments.push(StatusBarSegment::create(AzString::from(format!(
            "{} ({kind})",
            tab.title
        ))));
        let size = tab.session.size();
        segments.push(StatusBarSegment::create(AzString::from(format!(
            "{} x {}",
            size.columns, size.lines
        ))));
        let history = tab.session.term.lock().grid().history_size();
        segments.push(StatusBarSegment::create(AzString::from(format!(
            "scrollback {history} lines"
        ))));
        segments.push(StatusBarSegment::create(AzString::from("UTF-8")));
    }
    if !st.notice.is_empty() {
        segments.push(StatusBarSegment::create(AzString::from(st.notice.as_str())));
    }
    StatusBar::create(segments).dom().with_id(ids::STATUS)
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
        DeveloperShell::create(activity_bar(&app), side_bar(&app, st), editor(&app, st))
            .with_side_bar_ratio(0.2)
            .office_shell()
            .with_title_row(kit::title_row(&title))
            .with_status_bar(status_bar(st))
            .dom()
    };
    let root = column(vec![content, about_modal(&app, st)]);
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; padding: 0px; height: 100%;")
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

/// Every 16 ms: a changed screen re-renders the terminal view (only it); a
/// new title or an ended shell rebuilds the window.
extern "C" fn output_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let active = st.active;
    let mut render = false;
    let mut rebuild = false;
    for (i, tab) in st.tabs.iter_mut().enumerate() {
        let dirty = tab.session.signals.take_dirty();
        render |= dirty && i == active;
        if let Some(title) = tab.session.signals.take_title() {
            if !title.is_empty() && title != tab.title {
                tab.title = title;
                rebuild = true;
            }
        }
        let _ = tab.session.signals.take_bell();
        if !tab.exited
            && tab
                .session
                .signals
                .exited
                .load(std::sync::atomic::Ordering::Acquire)
        {
            tab.exited = true;
            rebuild = true;
            println!("AZTERM_EXITED {i}");
        }
    }
    drop(st);
    if render {
        info.callback_info.trigger_all_virtual_view_rerender();
    }
    if rebuild {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

extern "C" fn on_new_tab(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        st.open_tab();
    }
    Update::RefreshDom
}

extern "C" fn on_settings(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(st) = data.downcast_ref::<AppState>() {
        kit::open_settings(&st.kit, None);
    }
    Update::RefreshDom
}

extern "C" fn on_tab_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<TabClick>()
        .map(|c| (c.app.clone(), c.index))
    else {
        return Update::DoNothing;
    };
    if let Some(mut st) = app.downcast_mut::<AppState>() {
        st.select_tab(index);
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

/// The window's keys: the kit's first (settings, F1), then the tabs and the
/// text size. The terminal keeps every other key (it stops them itself).
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
    // The window's chord: Cmd on macOS, Ctrl+Shift elsewhere (the terminal
    // leaves those alone).
    let chord = if mac {
        m.meta && !m.ctrl && !m.alt
    } else {
        m.ctrl && m.shift && !m.alt && !m.meta
    };
    if !chord {
        return Update::DoNothing;
    }
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    match key {
        VirtualKeyCode::T => st.open_tab(),
        VirtualKeyCode::W => {
            let active = st.active;
            st.close_tab(active);
        }
        VirtualKeyCode::RBracket => st.cycle_tab(1),
        VirtualKeyCode::LBracket => st.cycle_tab(-1),
        VirtualKeyCode::Equals | VirtualKeyCode::Plus => {
            st.font_size = (st.font_size + 1.0).min(32.0)
        }
        VirtualKeyCode::Minus => st.font_size = (st.font_size - 1.0).max(8.0),
        _ => return Update::DoNothing,
    }
    drop(st);
    info.prevent_default();
    Update::RefreshDom
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
}
