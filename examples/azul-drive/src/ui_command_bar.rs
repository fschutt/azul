//! The command bar at the foot of the folder view: one line that runs a command in the folder
//! the view shows, so a `git pull` or a `make` needs no trip to a terminal window.
//!
//! It rests as a thin line (non-intrusive) and opens on a click, on typing, or on Ctrl+`. A line
//! runs in the user's login shell with the user's rights (`command_line`), on a PTY through
//! azul-termkit - AzTerm's and AzCode's terminal plumbing - so programs colour and behave as
//! in a terminal; its output streams into a small pane above the line (azul's TerminalView:
//! the colours, the selection, the copy, and the keys when the pane is clicked, for a program
//! that asks something). Ctrl+C (or Stop) interrupts; a second Stop ends the program. Up and
//! down walk the history, kept per profile in the data tree (`drive/command-history.txt`).
//! When a command ends, the folder is listed again and the git chip looks again.
//!
//! A line starting with "/" goes to the micro-action registry (`micro_actions`), a plain `cd`
//! moves the folder view, and a program that takes the whole screen (an editor, a pager) is
//! named before it runs - or, should one switch to the alternate screen anyway, ended at
//! once - with "Open in Terminal" for it. A cloud folder has no folder on this computer to run
//! in: the bar says so and takes no line.
//!
//! On stdout, for scripts: `AZDRIVE_CLI_RUN <program>`, `AZDRIVE_CLI_DONE <code or ->`,
//! `AZDRIVE_CLI_STOPPED`, `AZDRIVE_CLI_FULL_SCREEN <program>`, `AZDRIVE_CLI_ACTION <name>`.
//! The line itself is never printed: it may hold a secret.

use std::{
    path::PathBuf,
    sync::atomic::Ordering,
};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, TerminalViewDataSourceCallbackType,
        TerminalViewOnEventCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, TimerCallbackInfo, TimerCallbackReturn,
    },
    dom::{AttributeType, ClipboardContent, DomId, DomNodeId, FocusTarget, VirtualKeyCode},
    option::{OptionString, OptionU8Vec},
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::{StringVec, StyledTextRunVec},
    widgets::{
        Button, ButtonType, OnTextInputReturn, TerminalGridSize, TerminalScreen, TerminalView,
        TerminalViewEvent, TextInputState, TextInputValid,
    },
};
use azul_appkit::l10n::{label, t, t_args, t_text, Arg, Phrase, Text};
use azul_storage::{config::DriveLocation, Drive, LocalDrive};
use azul_termkit::{
    pane,
    session::{Session, SpawnOptions},
    vt::GridSize,
};

use crate::{
    actions::{self, Action},
    browse::Place,
    command_line::{
        self, classify, invocation, needs_terminal, resolve_cd, CdTarget, History, Line, Platform,
        HISTORY_LIMIT,
    },
    look::themed,
    micro_actions::{Context, Outcome, Registry},
    ui_git, with_state, DriveState,
};

/// The bar's root (its window key handler hangs here).
const BAR_ID: &str = "__azdrive_command_bar";
/// The line.
const INPUT_ID: &str = "__azdrive_command_bar_input";
/// The output's TerminalView.
const OUTPUT_ID: &str = "__azdrive_command_bar_output";
/// The history's file in the data tree.
const HISTORY_KEY: &str = "drive/command-history.txt";
/// Lines of output a command keeps.
const SCROLLBACK: usize = 5_000;
/// The output pane's height (px).
const OUTPUT_PX: f32 = 190.0;
/// The resting bar, the open line, the output's head and a notice (px): what the folder view
/// gives up, for its estimate of the rows in view.
const BAR_PX: f32 = 28.0;
const LINE_PX: f32 = 36.0;
const HEAD_PX: f32 = 24.0;
const NOTICE_PX: f32 = 30.0;
/// The output's text size (px).
const FONT_PX: f32 = 12.0;

/// The bar's strip at the leaf's foot.
const BAR: &str = themed!(
    "display: flex; flex-direction: column; flex-shrink: 0; font-size: 12px;",
    "border-top: 1px solid #D5DBE3; background: #F7F9FB;",
    "border-top: 1px solid #3A3A3A; background: #1F1F1F;",
    "border-top: 1px solid #DAD5C8; background: #F4F1EA;",
    "border-top: 1px solid #333333; background: #191919;",
);
/// The line's row: the folder, the field, the buttons.
const LINE_ROW: &str =
    "display: flex; flex-direction: row; align-items: center; padding: 3px 8px; min-height: 22px;";
/// The folder before the field, as a prompt writes it.
const FOLDER: &str = "font-family: monospace; font-size: 11px; opacity: 0.7; margin-right: 8px; \
                      white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 260px;";
/// The output pane: its head over the TerminalView.
const OUTPUT: &str = themed!(
    "display: flex; flex-direction: column; flex-shrink: 0;",
    "border-bottom: 1px solid #E1E6EC;",
    "border-bottom: 1px solid #333333;",
    "border-bottom: 1px solid #E2DDD0;",
    "border-bottom: 1px solid #2C2C2C;",
);
const OUTPUT_HEAD: &str = "display: flex; flex-direction: row; align-items: center; \
                           padding: 0px 8px; height: 24px; font-size: 11px; opacity: 0.85;";
/// A sentence over the line (a refusal, a hint).
const NOTICE: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; padding: 4px 10px; \
     line-height: 1.35;",
    "background: #FFF7E0;",
    "background: #3A3220;",
    "background: #F6EBD2;",
    "background: #3A3220;",
);

/// What a run is now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunState {
    /// The program runs.
    Running,
    /// It ended by itself: its exit code (`None`: a signal ended it).
    Ended(Option<i32>),
    /// The user stopped it, or it took the whole screen and was ended.
    Stopped,
    /// No program: a micro-action's lines.
    Said,
}

/// The command running, or the last one's output.
pub(crate) struct Run {
    /// What was run (shown in the output's head; never printed).
    pub line: String,
    /// The folder it ran in.
    pub cwd: PathBuf,
    /// Its PTY and screen (a recording for a micro-action's lines).
    pub session: Session,
    pub state: RunState,
    /// Ctrl+C was sent once: the next Stop ends the program.
    pub interrupted: bool,
}

/// A sentence over the line, and whether "Open in Terminal" goes with it.
pub(crate) struct Notice {
    pub text: Text,
    pub open_terminal: bool,
}

/// Where the history file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum HistoryFile {
    /// Not read yet (it is read when the bar first opens).
    #[default]
    Unread,
    /// Being read: lines run meanwhile join it when it arrives.
    Reading,
    /// Read: every line run is written back.
    Read,
}

/// The command bar's state.
pub(crate) struct CommandBar {
    /// Open (the line with its buttons and the output) rather than the thin resting line.
    pub expanded: bool,
    /// The line as the field last reported it (or the app set it).
    pub input: String,
    pub history: History,
    pub history_file: HistoryFile,
    /// A write of the history file runs; `history_dirty` asks for one more after it.
    pub history_writing: bool,
    pub history_dirty: bool,
    /// The "/" actions.
    pub actions: Registry,
    pub run: Option<Run>,
    /// The output pane shows (while the bar is open and there is a run).
    pub output_open: bool,
    pub notice: Option<Notice>,
    /// The output timer runs (while a program does).
    pub ticking: bool,
    /// Ticks in a row with new output ([`pane::renders_now`]).
    pub streak: u32,
}

impl Default for CommandBar {
    fn default() -> Self {
        CommandBar {
            expanded: false,
            input: String::new(),
            history: History::new(HISTORY_LIMIT),
            history_file: HistoryFile::Unread,
            history_writing: false,
            history_dirty: false,
            actions: Registry::with_builtins(),
            run: None,
            output_open: false,
            notice: None,
            ticking: false,
            streak: 0,
        }
    }
}

impl CommandBar {
    /// A program runs.
    pub(crate) fn running(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|run| run.state == RunState::Running)
    }
}

/// The height the bar takes from the folder view now (px).
pub(crate) fn height(s: &DriveState) -> f32 {
    if !matches!(s.place, Place::Folder { .. }) {
        return 0.0;
    }
    let bar = &s.command_bar;
    if !bar.expanded {
        return BAR_PX;
    }
    let output = if bar.output_open && bar.run.is_some() {
        OUTPUT_PX + HEAD_PX
    } else {
        0.0
    };
    let notice = if bar.notice.is_some() { NOTICE_PX } else { 0.0 };
    LINE_PX + output + notice
}

/// Shows `text` over the line (the bar opens to show it).
pub(crate) fn say(s: &mut DriveState, text: Text) {
    s.command_bar.notice = Some(Notice {
        text,
        open_terminal: false,
    });
    s.command_bar.expanded = true;
}

/// Why the open folder takes no command.
fn why_not(s: &DriveState) -> Text {
    match s.current_drive() {
        Some(_) => Text::key("azdrive-cli-why-cloud"),
        None => Text::key("azdrive-cli-why-no-folder"),
    }
}

// ==== Running ====

/// Runs `line` in the open folder's shell (a typed line, a micro-action's, the switcher's
/// `git switch`). One program at a time; a full-screen one is refused with "Open in Terminal".
pub(crate) fn run_line(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, line: &str) {
    let Some(cwd) = ui_git::local_folder(s) else {
        let why = why_not(s);
        say(s, why);
        return;
    };
    if s.command_bar.running() {
        say(s, Text::key("azdrive-cli-busy"));
        return;
    }
    if let Some(program) = needs_terminal(line) {
        println!("AZDRIVE_CLI_FULL_SCREEN {program}");
        s.command_bar.notice = Some(Notice {
            text: Phrase::new("azdrive-cli-needs-terminal")
                .arg("program", program)
                .into(),
            open_terminal: true,
        });
        s.command_bar.expanded = true;
        return;
    }
    let shell = if cfg!(windows) {
        std::env::var("COMSPEC").ok()
    } else {
        std::env::var("SHELL").ok()
    };
    let run = invocation(line, &cwd, shell.as_deref(), Platform::this());
    let spawned = Session::spawn_with(
        "AzDrive",
        Some((run.program, run.args)),
        Some(run.cwd),
        GridSize::new(120, 10),
        SCROLLBACK,
        SpawnOptions {
            env: run.env,
            drain_on_exit: true,
        },
    );
    match spawned {
        Ok(session) => {
            println!("AZDRIVE_CLI_RUN {}", program_of(line));
            let bar = &mut s.command_bar;
            bar.run = Some(Run {
                line: line.to_string(),
                cwd,
                session,
                state: RunState::Running,
                interrupted: false,
            });
            bar.output_open = true;
            bar.expanded = true;
            bar.notice = None;
            bar.streak = 0;
            start_ticking(info, app, s);
        }
        Err(e) => say(
            s,
            Phrase::new("azdrive-cli-spawn-failed")
                .arg("detail", e.to_string())
                .into(),
        ),
    }
}

/// The program a line starts with, for the markers and the full-screen notice.
fn program_of(line: &str) -> String {
    line.split_whitespace()
        .next()
        .map(command_line::program_name)
        .unwrap_or_default()
        .to_string()
}

/// A micro-action's lines in the output pane (a recording, no program). While a program runs
/// they show over the line instead, so its output stays.
fn show_lines(s: &mut DriveState, line: &str, lines: &[Text]) {
    if s.command_bar.running() {
        let joined = lines.iter().map(t_text).collect::<Vec<_>>().join(" ");
        say(s, Text::plain(joined));
        return;
    }
    let mut bytes = Vec::new();
    for text in lines {
        bytes.extend_from_slice(t_text(text).as_bytes());
        bytes.extend_from_slice(b"\r\n");
    }
    let cwd = ui_git::local_folder(s).unwrap_or_default();
    let bar = &mut s.command_bar;
    bar.run = Some(Run {
        line: line.to_string(),
        cwd,
        session: Session::replay(&bytes, GridSize::new(120, 10), SCROLLBACK),
        state: RunState::Said,
        interrupted: false,
    });
    bar.output_open = true;
    bar.expanded = true;
    bar.notice = None;
}

/// The typed line was entered: a shell command, a micro-action or a `cd`.
fn submit(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, text: String) {
    let line = classify(&text);
    if line == Line::Empty {
        return;
    }
    s.command_bar.history.push(&text);
    save_history(info, app, s);
    s.command_bar.input.clear();
    set_input_text(info, "");
    match line {
        Line::Empty => {}
        Line::Shell(command) => run_line(info, app, s, &command),
        Line::ChangeFolder(target) => change_folder(info, app, s, &target),
        Line::Action { name, args } => {
            println!("AZDRIVE_CLI_ACTION {}", if name.is_empty() { "help" } else { name.as_str() });
            let cwd = ui_git::local_folder(s).unwrap_or_default();
            let branch = s
                .git
                .info()
                .and_then(|found| found.head.branch().map(str::to_string));
            let outcome = s.command_bar.actions.dispatch(
                &name,
                &args,
                &Context {
                    cwd: &cwd,
                    branch: branch.as_deref(),
                },
            );
            carry_out(info, app, s, text.trim(), outcome);
        }
    }
}

/// Does what a micro-action asked for.
fn carry_out(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, line: &str, outcome: Outcome) {
    match outcome {
        Outcome::Say(lines) => show_lines(s, line, &lines),
        Outcome::Run(command) => run_line(info, app, s, &command),
        Outcome::OpenSwitcher(filter) => ui_git::open_switcher(info, app, s, filter),
        Outcome::ClearOutput => {
            if s.command_bar.running() {
                say(s, Text::key("azdrive-cli-busy"));
            } else {
                let bar = &mut s.command_bar;
                bar.run = None;
                bar.output_open = false;
                bar.notice = None;
            }
        }
        Outcome::OpenTerminal => {
            actions::run_action(info, app, s, Action::OpenTerminal { azterm: false });
        }
    }
}

/// `cd <target>` moves the folder view: to the folder of a drive of this computer that holds
/// it, or Back for `cd -`.
fn change_folder(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, target: &str) {
    let Some(cwd) = ui_git::local_folder(s) else {
        let why = why_not(s);
        say(s, why);
        return;
    };
    let home = pane::home_dir();
    match resolve_cd(&cwd, target, home.as_deref()) {
        None => say(s, Text::key("azdrive-cli-cd-no-home")),
        Some(CdTarget::Back) => {
            actions::run_command(info, app, s, crate::keys::Command::Back);
        }
        Some(CdTarget::Folder(folder)) => {
            if !folder.is_dir() {
                say(
                    s,
                    Phrase::new("azdrive-cli-cd-missing")
                        .arg("folder", folder.display().to_string())
                        .into(),
                );
                return;
            }
            let roots: Vec<(String, PathBuf)> = s
                .slots
                .iter()
                .filter_map(|slot| match &slot.entry.location {
                    DriveLocation::Local { root } => {
                        Some((slot.entry.id.clone(), PathBuf::from(root)))
                    }
                    _ => None,
                })
                .collect();
            match crate::picker::place_of_path(&folder, &roots) {
                Some((drive, prefix)) => {
                    s.command_bar.notice = None;
                    crate::go(info, app, s, Place::folder(&drive, &prefix), true);
                }
                None => say(
                    s,
                    Phrase::new("azdrive-cli-cd-outside")
                        .arg("folder", folder.display().to_string())
                        .into(),
                ),
            }
        }
    }
}

/// Ctrl+C or Stop: the program is interrupted; a second Stop ends it (its PTY closes, it gets
/// SIGHUP).
fn stop(s: &mut DriveState) {
    let Some(run) = s.command_bar.run.as_mut() else {
        return;
    };
    if run.state != RunState::Running {
        return;
    }
    if run.interrupted {
        // Ending the session closes the PTY; the screen so far stays in a recording.
        let screen = screen_text(&run.session);
        run.session = Session::replay(screen.as_bytes(), GridSize::new(120, 10), SCROLLBACK);
        run.state = RunState::Stopped;
        println!("AZDRIVE_CLI_STOPPED");
    } else {
        run.session.write(vec![0x03]);
        run.interrupted = true;
    }
}

/// The text of a session's screen, line by line (what stays of a program that was ended).
fn screen_text(session: &Session) -> String {
    let term = session.term.lock();
    let screen = azul_termkit::vt::screen(&*term, false);
    drop(term);
    screen
        .lines
        .as_slice()
        .iter()
        .map(|line| {
            line.runs
                .as_slice()
                .iter()
                .map(|run| run.text.as_str().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// The output timer, once (it ends itself when the program has ended).
fn start_ticking(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.command_bar.ticking {
        return;
    }
    s.command_bar.ticking = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(16))),
    );
}

/// What a tick found.
enum Tick {
    /// Nothing to do: the timer ends.
    Idle,
    /// New output to draw (or none).
    Output(bool),
    /// The program took the whole screen: it is ended and named.
    FullScreen(String),
    /// The program ended.
    Ended(Option<i32>),
}

/// Every 16 ms while a program runs: its new output re-renders the TerminalView alone (every
/// other tick in a flood, `pane::renders_now`); a program on the alternate screen is ended
/// and named; an ended one is noted, the folder listed again and the git chip asked again.
extern "C" fn on_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut callback_info = info.callback_info;
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let s = &mut *guard;
    let bar = &mut s.command_bar;
    let tick = match bar.run.as_mut() {
        Some(run) if run.state == RunState::Running => {
            let signals = run.session.signals.clone();
            if run.session.alternate_screen() {
                Tick::FullScreen(program_of(&run.line))
            } else if signals.exited.load(Ordering::Acquire) {
                Tick::Ended(signals.exit_code())
            } else {
                Tick::Output(signals.dirty.load(Ordering::Acquire))
            }
        }
        _ => Tick::Idle,
    };
    match tick {
        Tick::Idle => {
            bar.ticking = false;
            TimerCallbackReturn::terminate_unchanged()
        }
        Tick::FullScreen(program) => {
            println!("AZDRIVE_CLI_FULL_SCREEN {program}");
            if let Some(run) = bar.run.as_mut() {
                // Ending the session closes the PTY (SIGHUP): nothing was edited yet.
                run.session = Session::replay(b"", GridSize::new(120, 10), SCROLLBACK);
                run.state = RunState::Stopped;
            }
            bar.output_open = false;
            bar.ticking = false;
            bar.notice = Some(Notice {
                text: Phrase::new("azdrive-cli-needs-terminal")
                    .arg("program", program)
                    .into(),
                open_terminal: true,
            });
            TimerCallbackReturn::terminate_and_refresh_dom()
        }
        Tick::Ended(code) => {
            println!(
                "AZDRIVE_CLI_DONE {}",
                code.map_or_else(|| String::from("-"), |c| c.to_string())
            );
            if let Some(run) = bar.run.as_mut() {
                run.state = RunState::Ended(code);
            }
            bar.ticking = false;
            // What the command changed: the folder's rows and (in `refresh`) the git chip.
            crate::refresh(&mut callback_info, &app, s);
            TimerCallbackReturn::terminate_and_refresh_dom()
        }
        Tick::Output(dirty) => {
            bar.streak = if dirty { bar.streak.saturating_add(1) } else { 0 };
            let render = dirty && pane::renders_now(bar.streak);
            if render {
                if let Some(run) = bar.run.as_ref() {
                    let _ = run.session.signals.take_dirty();
                }
            }
            drop(guard);
            if render {
                pane::rerender(&mut callback_info, OUTPUT_ID);
            }
            TimerCallbackReturn::continue_unchanged()
        }
    }
}

// ==== The output's view ====

/// The TerminalView's data callback: the run's screen for the grid the view has room for.
extern "C" fn output_screen(mut data: RefAny, size: TerminalGridSize) -> TerminalScreen {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return TerminalScreen::empty();
    };
    match s.command_bar.run.as_mut() {
        Some(run) => pane::screen_for(&mut run.session, size, !cfg!(target_os = "macos")),
        None => TerminalScreen::empty(),
    }
}

/// The TerminalView's actions: keys to the program (the pane was clicked), the scroll, the
/// selection, the copy.
extern "C" fn on_output_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: TerminalViewEvent,
) -> Update {
    let copied = {
        let Some(mut s) = data.downcast_mut::<DriveState>() else {
            return Update::DoNothing;
        };
        let Some(run) = s.command_bar.run.as_mut() else {
            return Update::DoNothing;
        };
        pane::apply_event(&mut run.session, &event)
    };
    if let Some(text) = copied {
        info.set_clipboard_content(ClipboardContent {
            plain_text: AzString::from(text.as_str()),
            styled_runs: StyledTextRunVec::create(),
            html: OptionString::None,
            files: StringVec::create(),
            image: OptionU8Vec::None,
            link: OptionString::None,
            concealed: false,
        });
    }
    Update::DoNothing
}

// ==== The bar ====

/// A button of the bar: its words, icon and tooltip (a sentence), what it does.
fn bar_button(app: &RefAny, key: &str, icon: &str, tip: &str, act: BarAct, disabled: Option<String>) -> Dom {
    let mut button = Button::with_type(label(key), ButtonType::Default)
        .with_icon(AzString::from(icon))
        .with_on_click(
            RefAny::new(ActRef {
                app: app.clone(),
                act,
            }),
            on_bar_button as ButtonOnClickCallbackType,
        );
    if let Some(why) = disabled {
        button = button.with_disabled(AzString::from(why));
    }
    button
        .dom()
        .with_css("margin-left: 6px; flex-shrink: 0;")
        .with_attribute(AttributeType::Title(AzString::from(tip)))
}

/// What a bar button does.
#[derive(Clone, Copy)]
enum BarAct {
    Run,
    Stop,
    ToggleOutput,
    OpenTerminal,
    Fold,
    DismissNotice,
}

struct ActRef {
    app: RefAny,
    act: BarAct,
}

extern "C" fn on_bar_button(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, act)) = data
        .downcast_ref::<ActRef>()
        .map(|r| (r.app.clone(), r.act))
    else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| match act {
        BarAct::Run => {
            let text = s.command_bar.input.clone();
            submit(info, app, s, text);
        }
        BarAct::Stop => stop(s),
        BarAct::ToggleOutput => s.command_bar.output_open = !s.command_bar.output_open,
        BarAct::OpenTerminal => {
            actions::run_action(info, app, s, Action::OpenTerminal { azterm: false });
        }
        BarAct::Fold => s.command_bar.expanded = false,
        BarAct::DismissNotice => s.command_bar.notice = None,
    })
}

/// The bar at the foot of a folder's leaf: the thin resting line, or the line with its
/// buttons, the notice and the output. A cloud folder's bar says why it takes no command.
/// `None` off a folder (This PC, Quick access).
pub(crate) fn bar(s: &DriveState, app: &RefAny) -> Option<Dom> {
    if !matches!(s.place, Place::Folder { .. }) {
        return None;
    }
    let mut root = Dom::create_div()
        .with_id(BAR_ID)
        .with_css(BAR)
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_window_key,
        );
    let Some(cwd) = ui_git::local_folder(s) else {
        let why = t_text(&why_not(s));
        return Some(
            root.with_child(
                Dom::create_div()
                    .with_css(LINE_ROW)
                    .with_attribute(AttributeType::Title(AzString::from(why.as_str())))
                    .with_child(
                        Dom::create_icon(AzString::from("terminal"))
                            .with_css("font-size: 16px; opacity: 0.4; margin-right: 8px;"),
                    )
                    .with_child(
                        Dom::create_div()
                            .with_css("opacity: 0.6; flex-grow: 1; min-width: 0px;")
                            .with_child(Dom::create_span_with_text(AzString::from(why))),
                    ),
            ),
        );
    };
    let bar = &s.command_bar;
    let folder = pane::tilde_folder(&cwd, pane::home_dir().as_deref());
    if bar.expanded {
        if let (true, Some(run)) = (bar.output_open, bar.run.as_ref()) {
            root.add_child(output_pane(app, run));
        }
        if let Some(notice) = &bar.notice {
            root.add_child(notice_row(app, notice));
        }
    }
    let keys = if cfg!(target_os = "macos") { "\u{2303}`" } else { "Ctrl+`" };
    let input = TextInput::create()
        .with_text(AzString::from(bar.input.as_str()))
        .with_placeholder(AzString::from(t_args(
            "azdrive-cli-placeholder",
            &[("folder", Arg::from(folder.as_str())), ("keys", Arg::from(keys))],
        )))
        .with_on_text_input(app.clone(), on_input_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_input_key as TextInputOnVirtualKeyDownCallbackType)
        .with_accessibility_name(label("azdrive-cli-field"))
        .dom()
        .with_id(INPUT_ID)
        .with_css("flex-grow: 1; min-width: 0px;");
    let mut row = Dom::create_div()
        .with_css(LINE_ROW)
        .with_child(
            Dom::create_div()
                .with_css(FOLDER)
                .with_attribute(AttributeType::Title(AzString::from(t_args(
                    "azdrive-cli-folder-tip",
                    &[("folder", Arg::from(cwd.display().to_string()))],
                ))))
                .with_child(Dom::create_span_with_text(AzString::from(format!("{folder} \u{276F}")))),
        )
        .with_child(input);
    if bar.expanded {
        let running = bar.running();
        let empty = bar.input.trim().is_empty();
        row.add_child(bar_button(
            app,
            "azdrive-cli-run",
            "play_arrow",
            &t_args("azdrive-cli-run-tip", &[("folder", Arg::from(folder.as_str()))]),
            BarAct::Run,
            empty.then(|| t("azdrive-cli-why-empty")),
        ));
        if running {
            row.add_child(bar_button(
                app,
                "azdrive-cli-stop",
                "stop",
                &t("azdrive-cli-stop-tip"),
                BarAct::Stop,
                None,
            ));
        }
        row.add_child(bar_button(
            app,
            if bar.output_open { "azdrive-cli-hide-output" } else { "azdrive-cli-show-output" },
            if bar.output_open { "expand_more" } else { "expand_less" },
            &t("azdrive-cli-output-tip"),
            BarAct::ToggleOutput,
            bar.run.is_none().then(|| t("azdrive-cli-why-no-output")),
        ));
        row.add_child(bar_button(
            app,
            "azdrive-cli-open-terminal",
            "open_in_new",
            &t("azdrive-cli-open-terminal-tip"),
            BarAct::OpenTerminal,
            None,
        ));
        row.add_child(bar_button(
            app,
            "azdrive-cli-fold",
            "keyboard_arrow_down",
            &t("azdrive-cli-fold-tip"),
            BarAct::Fold,
            None,
        ));
    }
    row.add_child(ui_git::info_mark("azdrive-cli-info"));
    root.add_child(row);
    // A click on the resting line opens it (and the field gets the keyboard).
    if !bar.expanded {
        root = root.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            app.clone(),
            on_bar_click,
        );
    }
    Some(root)
}

/// The output pane: what ran, where, how it went; the TerminalView of its output.
fn output_pane(app: &RefAny, run: &Run) -> Dom {
    let home = pane::home_dir();
    let status = match run.state {
        RunState::Running => t("azdrive-cli-running"),
        RunState::Ended(Some(0)) | RunState::Said => t("azdrive-cli-done"),
        RunState::Ended(Some(code)) => t_args("azdrive-cli-exit-code", &[("code", Arg::from(i64::from(code)))]),
        RunState::Ended(None) => t("azdrive-cli-ended"),
        RunState::Stopped => t("azdrive-cli-stopped"),
    };
    let mut head = Dom::create_div()
        .with_css(OUTPUT_HEAD)
        .with_child(
            Dom::create_div()
                .with_css(format!("font-family: monospace; {}", crate::look::CLIP))
                .with_child(Dom::create_span_with_text(AzString::from(format!(
                    "{} \u{276F} {}",
                    pane::tilde_folder(&run.cwd, home.as_deref()),
                    run.line
                )))),
        )
        .with_child(
            Dom::create_div()
                .with_css("margin-left: 10px; flex-shrink: 0;")
                .with_child(Dom::create_span_with_text(AzString::from(status))),
        );
    if run.state == RunState::Running {
        head.add_child(ui_git::info_mark("azdrive-cli-typing-info"));
    }
    let view = TerminalView::create()
        .with_data_source(app.clone(), output_screen as TerminalViewDataSourceCallbackType)
        .with_on_event(app.clone(), on_output_event as TerminalViewOnEventCallbackType)
        .with_font_size(FONT_PX)
        .with_accessibility_name(label("azdrive-cli-output"))
        .with_id(OUTPUT_ID)
        .dom();
    Dom::create_div()
        .with_css(OUTPUT)
        .with_child(head)
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; height: {OUTPUT_PX}px; min-height: 0px;"
                ))
                .with_child(view),
        )
}

/// A sentence over the line, with "Open in Terminal" when that is the way, and its close box.
fn notice_row(app: &RefAny, notice: &Notice) -> Dom {
    let mut row = Dom::create_div().with_css(NOTICE).with_child(
        Dom::create_div()
            .with_css("flex-grow: 1; min-width: 0px;")
            .with_child(Dom::create_span_with_text(AzString::from(t_text(&notice.text)))),
    );
    if notice.open_terminal {
        row.add_child(bar_button(
            app,
            "azdrive-cli-open-terminal",
            "open_in_new",
            &t("azdrive-cli-open-terminal-tip"),
            BarAct::OpenTerminal,
            None,
        ));
    }
    row.add_child(bar_button(
        app,
        "azdrive-cli-dismiss",
        "close",
        &t("azdrive-cli-dismiss-tip"),
        BarAct::DismissNotice,
        None,
    ));
    row
}

/// Puts `text` into the line (history up / down, a run that empties it): the field's own text
/// outranks a rebuild until the app sets it.
fn set_input_text(info: &mut CallbackInfo, text: &str) {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, AzString::from(INPUT_ID));
    if node.into_raw() == 0 {
        return;
    }
    TextInput::set_text_in(*info, DomNodeId { dom, node }, AzString::from(text));
}

/// The line takes the keyboard.
fn focus_input(info: &mut CallbackInfo) {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, AzString::from(INPUT_ID));
    if node.into_raw() != 0 {
        info.set_focus(FocusTarget::Id(DomNodeId { dom, node }));
    }
}

/// The bar opens (and its history is read the first time).
fn open(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    s.command_bar.expanded = true;
    read_history(info, app, s);
}

/// A click on the resting line: it opens, the field takes the keyboard.
extern "C" fn on_bar_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        open(info, app, s);
        focus_input(info);
    })
}

/// The bar's keys anywhere in the window: Ctrl+` opens the bar and gives the line the
/// keyboard; Cmd+B (Ctrl+B elsewhere) opens the quick switcher.
extern "C" fn on_window_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(code) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    if code == VirtualKeyCode::Grave && m.ctrl && !m.alt && !m.shift {
        return with_state(&mut data, &mut info, |info, app, s| {
            if ui_git::local_folder(s).is_some() {
                open(info, app, s);
                focus_input(info);
            }
        });
    }
    if code == VirtualKeyCode::B && m.primary_down() && !m.alt && !m.shift {
        return with_state(&mut data, &mut info, |info, app, s| {
            if s.git.info().is_some_and(|found| !found.bare()) {
                ui_git::open_switcher(info, app, s, String::new());
            }
        });
    }
    Update::DoNothing
}

/// Typing: the bar opens, and the history walk starts over. Only an opening rebuilds (the
/// field keeps its own text).
extern "C" fn on_input_text(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let app = data.clone();
    let update = match data.downcast_mut::<DriveState>() {
        Some(mut guard) => {
            let s = &mut *guard;
            s.command_bar.input = text;
            s.command_bar.history.reset_walk();
            if s.command_bar.expanded {
                Update::DoNothing
            } else {
                open(&mut info, &app, s);
                Update::RefreshDom
            }
        }
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The line's keys: Return runs it, up / down walk the history, Tab completes a "/" action,
/// Ctrl+C interrupts a running program (with nothing running it is the field's copy), Escape
/// folds the bar.
extern "C" fn on_input_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let m = info.get_key_modifiers();
    let text = state.get_text().as_str().to_string();
    let update = match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |info, app, s| submit(info, app, s, text))
        }
        Some(VirtualKeyCode::Up) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |info, _app, s| {
                if let Some(line) = s.command_bar.history.up(&text) {
                    set_input_text(info, &line);
                    s.command_bar.input = line;
                }
            })
        }
        Some(VirtualKeyCode::Down) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |info, _app, s| {
                if let Some(line) = s.command_bar.history.down() {
                    set_input_text(info, &line);
                    s.command_bar.input = line;
                }
            })
        }
        Some(VirtualKeyCode::Tab) if text.starts_with('/') && !text.contains(' ') => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |info, _app, s| {
                let names = s.command_bar.actions.complete(&text[1..]);
                if let [only] = names.as_slice() {
                    let line = format!("/{only} ");
                    set_input_text(info, &line);
                    s.command_bar.input = line;
                } else if !names.is_empty() {
                    let listed = names
                        .iter()
                        .map(|name| format!("/{name}"))
                        .collect::<Vec<_>>()
                        .join("  ");
                    say(s, Text::plain(listed));
                }
            })
        }
        Some(VirtualKeyCode::C) if m.ctrl && !m.alt && !m.shift => {
            let running = data
                .downcast_ref::<DriveState>()
                .is_some_and(|s| s.command_bar.running());
            if running {
                info.stop_propagation();
                with_state(&mut data, &mut info, |_info, _app, s| stop(s))
            } else {
                Update::DoNothing
            }
        }
        Some(VirtualKeyCode::Escape) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |_info, _app, s| {
                s.command_bar.expanded = false;
            })
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

// ==== The history file ====

/// A job for the history's worker thread.
enum HistoryJob {
    Read(LocalDrive),
    Write(LocalDrive, String),
}

struct HistoryInit {
    job: Option<HistoryJob>,
}

/// What the history's thread hands back: the file's text (a read), or `None` (a write ended).
struct HistoryDone {
    text: Option<String>,
}

/// Reads the history file once, on a worker thread (the first time the bar opens).
fn read_history(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.command_bar.history_file != HistoryFile::Unread {
        return;
    }
    let Some(drive) = s.settings_drive.clone() else {
        // No data tree (a `--shot` run): the history lives for the session only.
        s.command_bar.history_file = HistoryFile::Read;
        return;
    };
    s.command_bar.history_file = HistoryFile::Reading;
    spawn_history(info, app, HistoryJob::Read(drive));
}

/// Writes the history file (one write at a time; a line run meanwhile writes again after).
fn save_history(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let bar = &mut s.command_bar;
    if bar.history_file != HistoryFile::Read {
        // The lines run so far join the file's when it arrives, and are written then.
        return;
    }
    if bar.history_writing {
        bar.history_dirty = true;
        return;
    }
    let Some(drive) = s.settings_drive.clone() else {
        return;
    };
    let bar = &mut s.command_bar;
    bar.history_writing = true;
    bar.history_dirty = false;
    let text = bar.history.to_text();
    spawn_history(info, app, HistoryJob::Write(drive, text));
}

fn spawn_history(info: &mut CallbackInfo, app: &RefAny, job: HistoryJob) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(HistoryInit { job: Some(job) }),
            app.clone(),
            history_thread,
        ),
    );
}

extern "C" fn history_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    _receiver: ThreadReceiver,
) {
    let Some(job) = init
        .downcast_mut::<HistoryInit>()
        .and_then(|mut init| init.job.take())
    else {
        return;
    };
    let done = match job {
        // No file yet (the first run) reads as an empty history.
        HistoryJob::Read(drive) => HistoryDone {
            text: Some(
                drive
                    .get(HISTORY_KEY)
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_default(),
            ),
        },
        HistoryJob::Write(drive, text) => {
            if let Err(e) = drive.put(HISTORY_KEY, text.as_bytes()) {
                eprintln!("[azdrive] the command history could not be written: {e}");
            }
            HistoryDone { text: None }
        }
    };
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_history_done,
        RefAny::new(done),
    )));
}

extern "C" fn on_history_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(text) = msg.downcast_mut::<HistoryDone>().map(|mut done| done.text.take()) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    match text {
        Some(text) => {
            // The file's lines first, then those run while it was read.
            let mut merged = History::from_text(&text, HISTORY_LIMIT);
            let ran = s.command_bar.history.entries().to_vec();
            for line in &ran {
                merged.push(line);
            }
            s.command_bar.history = merged;
            s.command_bar.history_file = HistoryFile::Read;
            if !ran.is_empty() {
                save_history(&mut info, &handle, s);
            }
        }
        None => {
            s.command_bar.history_writing = false;
            if s.command_bar.history_dirty {
                save_history(&mut info, &handle, s);
            }
        }
    }
    Update::DoNothing
}
