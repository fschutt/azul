//! The terminal panel (View > Terminal, Ctrl+`, Mod+J): VSCode's panel
//! under the editor - its header (TERMINAL, a tab per shell, New Terminal,
//! Kill Terminal, Close) over azul's `TerminalView` of the shell in front.
//!
//! A shell is the user's (`$SHELL`, or `--shell`) on a PTY in the
//! workspace's folder, through azul-termkit - AzTerm's session and screen,
//! the same code. The view is a `VirtualView` scrolled in whole lines (a
//! scrollback of [`SCROLLBACK`] lines costs what a screen does); its data
//! callback gives the screen for the grid it has room for (the engine and
//! the PTY follow), its events are applied to the session. Output
//! re-renders the view alone, from a 16 ms timer that runs while a shell
//! does ([`terminal_tick`]).
//!
//! On stdout: `AZCODE_TERMINAL_READY <n> <folder>` (shell `n` started),
//! `AZCODE_TERMINAL_OUTPUT <n>` (its first output: the prompt),
//! `AZCODE_TERMINAL_EXITED <n>`, `AZCODE_PANEL open|closed`.

use std::{path::PathBuf, sync::atomic::Ordering};

use azul::{
    callbacks::{
        TerminalViewDataSourceCallbackType, TerminalViewOnEventCallbackType, TimerCallbackInfo,
        TimerCallbackReturn,
    },
    dom::{AccessibilityInfo, AccessibilityRole, ClipboardContent},
    option::OptionString,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StyledTextRunVec,
    widgets::{TerminalGridSize, TerminalScreen, TerminalView, TerminalViewEvent},
};
use azul_termkit::{
    pane,
    session::Session,
    vt::GridSize,
};

use crate::{app::AppState, commands, ids, ui};

/// Lines of scrollback a shell keeps.
pub const SCROLLBACK: usize = 10_000;
/// The grid a shell starts with, before the view says how much room it has.
const START_GRID: GridSize = GridSize {
    columns: 80,
    lines: 24,
};
/// The terminal's text size, px (VSCode's).
const FONT_SIZE: f32 = 13.0;
/// The panel's header, px.
pub const HEADER_HEIGHT: f32 = 35.0;

/// One shell of the panel.
pub struct TerminalTab {
    pub session: Session,
    /// What the header calls it: the shell's name and folder, or the title
    /// the program set.
    pub title: String,
    /// Its number (`AZCODE_TERMINAL_READY <n>`), counted from 1.
    pub number: u64,
    pub exited: bool,
    /// Its first output was announced.
    pub spoke: bool,
}

/// The panel and its shells.
pub struct Panel {
    /// The panel shows under the editor.
    pub open: bool,
    pub terminals: Vec<TerminalTab>,
    /// The shell in front.
    pub active: usize,
    /// The editor's share of the height the editor and the panel split (the
    /// splitter between them).
    pub editor_ratio: f32,
    /// Ticks in a row the shell in front had new output on
    /// ([`pane::renders_now`]).
    pub streak: u32,
    /// The output timer runs.
    pub ticking: bool,
    /// The number the next shell gets.
    next: u64,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            open: false,
            terminals: Vec::new(),
            active: 0,
            editor_ratio: 0.66,
            streak: 0,
            ticking: false,
            next: 1,
        }
    }
}

impl Panel {
    /// The shell in front.
    pub fn active_mut(&mut self) -> Option<&mut TerminalTab> {
        self.terminals.get_mut(self.active)
    }

    /// Removes shell `index` (its session ends: the shell gets SIGHUP); the
    /// one after it (else the last) comes to front.
    fn remove(&mut self, index: usize) {
        if index >= self.terminals.len() {
            return;
        }
        self.terminals.remove(index);
        if index < self.active || self.active >= self.terminals.len() {
            self.active = self.active.saturating_sub(1).min(self.terminals.len().saturating_sub(1));
        }
        self.touch_active();
    }

    /// The shell in front is drawn on the next tick.
    fn touch_active(&mut self) {
        self.streak = 0;
        if let Some(t) = self.terminals.get(self.active) {
            t.session.signals.dirty.store(true, Ordering::Release);
        }
    }
}

// ==== What the commands do ====

/// Ctrl+` / Mod+J / View > Terminal: the panel opens (with a shell, the
/// first time) and the shell gets the keys, or it closes.
pub fn toggle(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    if st.panel.open {
        close(st, info);
    } else {
        open(st, info, app);
    }
}

/// Opens the panel; a shell starts when there is none; the shell in front
/// gets the keys.
pub fn open(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    if !st.panel.open {
        st.panel.open = true;
        println!("AZCODE_PANEL open");
    }
    if st.panel.terminals.is_empty() {
        new_terminal(st, info, app);
    } else {
        st.panel.touch_active();
        commands::focus_soon(info, ids::TERMINAL.as_str());
    }
}

/// Closes the panel (its shells keep running); the editor gets the keys.
pub fn close(st: &mut AppState, info: &mut CallbackInfo) {
    if st.panel.open {
        st.panel.open = false;
        println!("AZCODE_PANEL closed");
    }
    if st.tabs.active().is_some() {
        commands::focus_soon(info, ids::EDITOR.as_str());
    }
}

/// Terminal > New Terminal: another shell in the workspace's folder (the
/// home folder without one), in front; the panel opens.
pub fn new_terminal(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let cwd = shell_folder(st.workspace_folder());
    match Session::spawn_for("AzCode", st.shell.clone(), cwd.clone(), START_GRID, SCROLLBACK) {
        Ok(session) => {
            let number = st.panel.next;
            st.panel.next += 1;
            let shell = st
                .shell
                .as_ref()
                .map(|(program, _)| program.clone())
                .or_else(|| std::env::var("SHELL").ok());
            let home = pane::home_dir();
            let title = pane::shell_title(shell.as_deref(), cwd.as_deref(), home.as_deref());
            st.panel.terminals.push(TerminalTab {
                session,
                title,
                number,
                exited: false,
                spoke: false,
            });
            st.panel.active = st.panel.terminals.len() - 1;
            st.panel.touch_active();
            if !st.panel.open {
                st.panel.open = true;
                println!("AZCODE_PANEL open");
            }
            let folder = cwd.map_or_else(String::new, |c| c.display().to_string());
            println!("AZCODE_TERMINAL_READY {number} {folder}");
            start_ticking(st, info, app);
            commands::focus_soon(info, ids::TERMINAL.as_str());
        }
        Err(e) => st.notice = format!("The terminal could not be started: {e}"),
    }
}

/// Terminal > Kill Terminal: the shell in front ends; the panel closes with
/// the last one.
pub fn kill_terminal(st: &mut AppState, info: &mut CallbackInfo) {
    if st.panel.terminals.is_empty() {
        return;
    }
    let active = st.panel.active;
    if let Some(t) = st.panel.terminals.get(active) {
        println!("AZCODE_TERMINAL_EXITED {}", t.number);
    }
    st.panel.remove(active);
    if st.panel.terminals.is_empty() {
        close(st, info);
    }
}

/// The output timer, once (it ends itself when the last shell has gone).
fn start_ticking(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    if st.panel.ticking {
        return;
    }
    st.panel.ticking = true;
    let timer = Timer::create(app.clone(), terminal_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(16)));
    info.add_timer(TimerId::unique(), timer);
}

/// Every 16 ms while a shell runs: new output of the shell in front
/// re-renders the terminal's view (only it; every other tick in a flood), a
/// new title or an ended shell rebuilds the window (a shell that ended
/// leaves the panel; the last one closes it).
pub extern "C" fn terminal_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    // Borrowed elsewhere this tick: the next one looks (ending here would
    // leave `ticking` set and no timer to read the shells).
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let st = &mut *guard;
    let panel = &mut st.panel;
    let (active, shown) = (panel.active, panel.open);
    let mut output = false;
    let mut rebuild = false;
    let mut ended = Vec::new();
    for (i, t) in panel.terminals.iter_mut().enumerate() {
        let signals = &t.session.signals;
        // The shell in front: its flag is taken when its view re-renders (a
        // flood skips ticks); a shell without a view has nothing to draw.
        let dirty = if i == active && shown {
            signals.dirty.load(Ordering::Acquire)
        } else {
            signals.take_dirty()
        };
        if dirty && !t.spoke {
            t.spoke = true;
            println!("AZCODE_TERMINAL_OUTPUT {}", t.number);
        }
        if i == active && shown {
            output = dirty;
        }
        if let Some(title) = signals.take_title() {
            if !title.is_empty() && title != t.title {
                t.title = title;
                rebuild = true;
            }
        }
        let _ = signals.take_bell();
        if !t.exited && signals.exited.load(Ordering::Acquire) {
            t.exited = true;
            ended.push(i);
            println!("AZCODE_TERMINAL_EXITED {}", t.number);
        }
    }
    for i in ended.into_iter().rev() {
        panel.remove(i);
        rebuild = true;
    }
    if panel.terminals.is_empty() {
        if panel.open {
            panel.open = false;
            println!("AZCODE_PANEL closed");
        }
        panel.ticking = false;
        return if rebuild {
            TimerCallbackReturn::terminate_and_refresh_dom()
        } else {
            TimerCallbackReturn::terminate_unchanged()
        };
    }
    panel.streak = if output { panel.streak.saturating_add(1) } else { 0 };
    let render = output && pane::renders_now(panel.streak);
    if render {
        if let Some(t) = panel.terminals.get(panel.active) {
            let _ = t.session.signals.take_dirty();
        }
    }
    drop(guard);
    if render && !rebuild {
        pane::rerender(&mut info.callback_info, ids::TERMINAL.as_str());
    }
    if rebuild {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

// ==== The view's callbacks ====

/// The view's data callback: the screen of the shell in front for the grid
/// the view has room for.
extern "C" fn terminal_screen(mut data: RefAny, size: TerminalGridSize) -> TerminalScreen {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return TerminalScreen::empty();
    };
    match st.panel.active_mut() {
        Some(t) => pane::screen_for(&mut t.session, size, !cfg!(target_os = "macos")),
        None => TerminalScreen::empty(),
    }
}

/// The view's actions: bytes to the shell, the scroll, the selection, the
/// copy.
extern "C" fn on_terminal(mut data: RefAny, mut info: CallbackInfo, event: TerminalViewEvent) -> Update {
    let copied = {
        let Some(mut st) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(t) = st.panel.active_mut() else {
            return Update::DoNothing;
        };
        pane::apply_event(&mut t.session, &event)
    };
    if let Some(text) = copied {
        info.set_clipboard_content(ClipboardContent {
            plain_text: AzString::from(text.as_str()),
            styled_runs: StyledTextRunVec::create(),
            html: OptionString::None,
        });
    }
    Update::DoNothing
}

// ==== The panel ====

/// What a shell's tab in the header acts on.
struct TerminalRef {
    app: RefAny,
    index: usize,
}

/// The panel: its header over the terminal of the shell in front.
pub fn panel(app: &RefAny, st: &AppState) -> Dom {
    let mut header = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; height: {HEADER_HEIGHT}px; \
             flex-shrink: 0; padding: 0px 8px 0px 16px; border-top: 1px solid {};",
            ui::RULE
        ))
        .with_child(
            Dom::create_div()
                .with_css(
                    // Under flora the underline is the stone (its glow at night).
                    "font-size: 11px; padding: 4px 0px; margin-right: 16px; \
                     border-bottom: 1px solid var(--az-accent, #0078D4); @theme(flora) { \
                     border-bottom: 1px solid system:accent; }",
                )
                .with_child(Dom::create_span_with_text("TERMINAL")),
        );
    for (i, t) in st.panel.terminals.iter().enumerate() {
        header.add_child(terminal_tab(app, i, t, i == st.panel.active));
    }
    header.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    header.add_child(ui::icon_button(ids::TERMINAL_NEW, "add", "New Terminal", app.clone(), on_new_terminal));
    header.add_child(ui::icon_button(ids::TERMINAL_KILL, "delete", "Kill Terminal", app.clone(), on_kill_terminal));
    header.add_child(ui::icon_button(ids::PANEL_CLOSE, "close", "Close Panel", app.clone(), on_close_panel));
    let body = match st.panel.terminals.get(st.panel.active) {
        Some(t) => TerminalView::create()
            .with_data_source(app.clone(), terminal_screen as TerminalViewDataSourceCallbackType)
            .with_on_event(app.clone(), on_terminal as TerminalViewOnEventCallbackType)
            .with_font_size(FONT_SIZE)
            .with_accessibility_name(AzString::from(t.title.as_str()))
            .with_id(ids::TERMINAL)
            .dom(),
        None => Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; justify-content: center; flex-grow: 1;")
            .with_child(Dom::create_span_with_text("The shell has ended. New Terminal starts another.")),
    };
    Dom::create_div()
        .with_id(ids::PANEL)
        // The split pane is a block: the panel takes its whole height itself
        // (`flex-grow` alone left it as tall as its header, and the terminal
        // under it 0 px tall - its view never rendered a row).
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; height: 100%; min-height: 0px; \
             min-width: 0px;",
        )
        .with_child(header)
        .with_child(
            // Positioned: the terminal fills it.
            Dom::create_div()
                .with_css(
                    "position: relative; flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-height: 0px; \
                     min-width: 0px; padding-left: 16px;",
                )
                .with_child(body),
        )
}

/// Shell `index`'s tab in the header: its number and title; a click brings
/// it to front.
fn terminal_tab(app: &RefAny, index: usize, t: &TerminalTab, active: bool) -> Dom {
    let look = if active {
        "opacity: 1; background: rgba(128, 128, 128, 0.18);"
    } else {
        "opacity: 0.7;"
    };
    Dom::create_div()
        .with_id(ids::terminal_tab(index))
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; height: 22px; padding: 0px 8px; \
             margin-right: 4px; border-radius: 4px; font-size: 12px; white-space: nowrap; cursor: pointer; \
             :hover {{ background: rgba(128, 128, 128, 0.25); }} {look}"
        ))
        .with_accessibility_info(AccessibilityInfo::named(format!("Terminal {}: {}", t.number, t.title), AccessibilityRole::PageTab))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(TerminalRef {
                app: app.clone(),
                index,
            }),
            on_terminal_tab_click,
        )
        .with_child(Dom::create_icon("terminal").with_css("font-size: 14px; padding-right: 4px;"))
        .with_child(Dom::create_span_with_text(format!("{}: {}", t.number, t.title)))
}

extern "C" fn on_new_terminal(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, new_terminal)
}

extern "C" fn on_kill_terminal(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, |st, info, _| kill_terminal(st, info))
}

extern "C" fn on_close_panel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, |st, info, _| close(st, info))
}

extern "C" fn on_terminal_tab_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<TerminalRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, _| {
        if index < st.panel.terminals.len() {
            st.panel.active = index;
            st.panel.touch_active();
            commands::focus_soon(info, ids::TERMINAL.as_str());
        }
    })
}

/// The cwd a new shell gets for a workspace at `folder` (the home folder
/// without one) - a plain fallback, kept apart for the tests.
#[must_use]
pub fn shell_folder(workspace: Option<PathBuf>) -> Option<PathBuf> {
    workspace.or_else(pane::home_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replayed(number: u64) -> TerminalTab {
        TerminalTab {
            session: Session::replay(b"$ ", START_GRID, 100),
            title: format!("sh {number}"),
            number,
            exited: false,
            spoke: false,
        }
    }

    #[test]
    fn removing_a_shell_keeps_the_one_in_front_or_takes_its_neighbour() {
        let mut p = Panel::default();
        for n in 1..=3 {
            p.terminals.push(replayed(n));
        }
        p.active = 2;
        p.remove(0);
        assert_eq!(p.terminals[p.active].number, 3, "the shell in front stays in front");
        p.remove(p.active);
        assert_eq!(p.terminals[p.active].number, 2, "the last one closed: the one before it");
        p.remove(0);
        assert!(p.terminals.is_empty());
        assert_eq!(p.active, 0);
        p.remove(5);
        assert!(p.active_mut().is_none());
    }

    #[test]
    fn a_new_shell_starts_in_the_workspace_or_at_home() {
        let folder = PathBuf::from("/work/project");
        assert_eq!(shell_folder(Some(folder.clone())), Some(folder));
        assert_eq!(shell_folder(None), pane::home_dir());
    }
}
