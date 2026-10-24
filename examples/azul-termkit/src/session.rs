//! One shell: alacritty's PTY (`tty::new`: forkpty-equivalent on Unix,
//! ConPTY on Windows) and its event loop thread (reads the PTY into the
//! `Term`, writes our input, resizes with `TIOCSWINSZ`), or - for `--sample`
//! and the headless tests - a recorded session replayed into a `Term` with
//! no PTY, which echoes what is typed.
//!
//! The engine talks to the app through [`Signals`]: atomics and a title the
//! UI thread polls from a timer (no callback runs on the PTY thread). A
//! program's answers (`CSI 6n` and friends, `Event::PtyWrite`) go straight
//! back to the PTY from the listener.

use std::{
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use alacritty_terminal::{
    event::{Event, EventListener, Notify, OnResize, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg, Notifier},
    grid::Scroll,
    sync::FairMutex,
    term::{Config, Term},
    tty::{self, Options, Shell},
    vte::ansi::{Processor, StdSyncHandler},
};

use crate::{
    sample::{Stream, STREAM_LINES_PER_TICK},
    vt::GridSize,
};

/// A cell's size the PTY is told (`TIOCSWINSZ` carries pixels too).
const CELL_PX: (u16, u16) = (8, 17);

/// What the engine told the app since the UI last looked.
#[derive(Default)]
pub struct Signals {
    /// The screen changed.
    pub dirty: AtomicBool,
    /// The program set the title (`OSC 0` / `OSC 2`); `Some("")` = reset.
    pub title: Mutex<Option<String>>,
    /// The bell rang.
    pub bell: AtomicBool,
    /// What a program put on the clipboard (OSC 52), not yet handed to the system's.
    pub clipboard: Mutex<Option<String>>,
    /// The program ended.
    pub exited: AtomicBool,
    /// Where a program's answers go (set once the event loop exists).
    sender: Mutex<Option<EventLoopSender>>,
}

impl Signals {
    /// Whether the screen changed since the last call.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    /// The title the program set since the last call.
    pub fn take_title(&self) -> Option<String> {
        self.title.lock().ok().and_then(|mut t| t.take())
    }

    /// Whether the bell rang since the last call.
    pub fn take_bell(&self) -> bool {
        self.bell.swap(false, Ordering::AcqRel)
    }

    /// The text a program put on the clipboard since the last call (the last one wins).
    pub fn take_clipboard(&self) -> Option<String> {
        self.clipboard.lock().ok().and_then(|mut c| c.take())
    }
}

/// The listener the `Term` and the event loop share.
#[derive(Clone)]
pub struct Listener(pub Arc<Signals>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let s = &self.0;
        match event {
            Event::Wakeup | Event::MouseCursorDirty | Event::CursorBlinkingChange => {
                s.dirty.store(true, Ordering::Release);
            }
            Event::Title(title) => {
                if let Ok(mut t) = s.title.lock() {
                    *t = Some(title);
                }
                s.dirty.store(true, Ordering::Release);
            }
            Event::ResetTitle => {
                if let Ok(mut t) = s.title.lock() {
                    *t = Some(String::new());
                }
            }
            Event::Bell => s.bell.store(true, Ordering::Release),
            Event::ChildExit(_) | Event::Exit => {
                s.exited.store(true, Ordering::Release);
                s.dirty.store(true, Ordering::Release);
            }
            Event::PtyWrite(text) => {
                let sender = s.sender.lock().ok().and_then(|g| g.clone());
                if let Some(sender) = sender {
                    let _ = sender.send(Msg::Input(text.into_bytes().into()));
                }
            }
            // OSC 52's store: a program copies (vim, tmux over ssh); the window hands it to
            // the system's clipboard.
            Event::ClipboardStore(_, text) => {
                if let Ok(mut c) = s.clipboard.lock() {
                    *c = Some(text);
                }
                s.dirty.store(true, Ordering::Release);
            }
            // OSC 52's load is never answered: what the user copied elsewhere is not a
            // program's to read (the engine's config allows only the store, see `new_term`).
            // Colour and size queries: not answered either.
            _ => {}
        }
    }
}

/// How the bytes reach the program.
enum Backend {
    /// A shell on a PTY, through its event loop.
    Pty(Notifier),
    /// A recording: what is typed is echoed into the screen - the sample
    /// shell, which runs `seq N` and `yes | head -n N` ([`Stream`]).
    Replay {
        parser: Processor<StdSyncHandler>,
        /// What was typed since the last Return.
        line: String,
        /// The command running, writing a few lines a tick ([`Session::pump`]).
        stream: Option<Stream>,
    },
}

/// The process group running in the foreground of a terminal, from its shell's `ps -o
/// tpgid=,pgid=` line: `Some` when it is not the shell's own group, so a program the shell
/// started holds the terminal (an editor, a build) and closing it would cut that off.
#[must_use]
pub fn foreground_group(ps_line: &str) -> Option<u32> {
    let mut numbers = ps_line.split_whitespace().map(str::parse::<i64>);
    let terminal = numbers.next()?.ok()?;
    let shell = numbers.next()?.ok()?;
    if terminal <= 0 || terminal == shell {
        return None;
    }
    u32::try_from(terminal).ok()
}

/// One line of `ps` for `pid` with `columns` (`tpgid=,pgid=`, `comm=`); `None` when it could
/// not be asked.
fn ps_line(pid: u32, columns: &str) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-o", columns, "-p", &pid.to_string()])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// One shell and its screen.
pub struct Session {
    /// The engine (the event loop thread writes into it).
    pub term: Arc<FairMutex<Term<Listener>>>,
    /// What the engine told the app.
    pub signals: Arc<Signals>,
    backend: Backend,
    size: GridSize,
    /// The shell's process on its PTY (`None` for a recording, and on Windows, where no
    /// process group holds the console): what [`Session::foreground_job`] asks `ps` about.
    shell_pid: Option<u32>,
}

/// The PTY's window size for `size`.
fn window_size(size: GridSize) -> WindowSize {
    WindowSize {
        num_lines: u16::try_from(size.lines).unwrap_or(u16::MAX),
        num_cols: u16::try_from(size.columns).unwrap_or(u16::MAX),
        cell_width: CELL_PX.0,
        cell_height: CELL_PX.1,
    }
}

/// The engine of a `size` grid with `scrollback` lines of history.
fn new_term(size: GridSize, scrollback: usize, signals: &Arc<Signals>) -> Term<Listener> {
    let config = Config {
        scrolling_history: scrollback,
        // OSC 52: a program may copy, never paste (read the clipboard).
        osc52: alacritty_terminal::term::Osc52::OnlyCopy,
        ..Config::default()
    };
    Term::new(config, &size, Listener(signals.clone()))
}

impl Session {
    /// The user's shell (or `program` with `args`) in `cwd` on a new PTY,
    /// `TERM_PROGRAM=AzTerm`.
    ///
    /// # Errors
    /// The PTY could not be opened or the shell not started.
    pub fn spawn(
        program: Option<(String, Vec<String>)>,
        cwd: Option<PathBuf>,
        size: GridSize,
        scrollback: usize,
    ) -> io::Result<Self> {
        Self::spawn_for("AzTerm", program, cwd, size, scrollback)
    }

    /// [`Self::spawn`] for the app `term_program` names (`TERM_PROGRAM`:
    /// what a shell's integration reads - AzCode's panel says `AzCode`).
    ///
    /// # Errors
    /// The PTY could not be opened or the shell not started.
    pub fn spawn_for(
        term_program: &str,
        program: Option<(String, Vec<String>)>,
        cwd: Option<PathBuf>,
        size: GridSize,
        scrollback: usize,
    ) -> io::Result<Self> {
        let signals = Arc::new(Signals::default());
        let term = Arc::new(FairMutex::new(new_term(size, scrollback, &signals)));
        let mut options = Options::default();
        options.shell = program.map(|(p, a)| Shell::new(p, a));
        options.working_directory = cwd;
        options
            .env
            .insert("TERM_PROGRAM".to_string(), term_program.to_string());
        let pty = tty::new(&options, window_size(size), 0)?;
        #[cfg(unix)]
        let shell_pid = Some(pty.child().id());
        #[cfg(not(unix))]
        let shell_pid = None;
        let event_loop =
            EventLoop::new(term.clone(), Listener(signals.clone()), pty, false, false)?;
        let sender = event_loop.channel();
        if let Ok(mut s) = signals.sender.lock() {
            *s = Some(sender.clone());
        }
        // The reader thread owns the PTY; it ends on `Msg::Shutdown` (drop)
        // or when the shell exits.
        let _reader = event_loop.spawn();
        Ok(Self {
            term,
            signals,
            backend: Backend::Pty(Notifier(sender)),
            size,
            shell_pid,
        })
    }

    /// A recorded session: `recording` replayed into a `size` grid, no PTY.
    pub fn replay(recording: &[u8], size: GridSize, scrollback: usize) -> Self {
        let signals = Arc::new(Signals::default());
        let mut term = new_term(size, scrollback, &signals);
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, recording);
        signals.dirty.store(true, Ordering::Release);
        Self {
            term: Arc::new(FairMutex::new(term)),
            signals,
            backend: Backend::Replay {
                parser,
                line: String::new(),
                stream: None,
            },
            size,
            shell_pid: None,
        }
    }

    /// The program in the foreground of this shell's terminal, by its name (`vim`): closing
    /// the session would stop it. `None` while the shell itself waits for input, for a
    /// recording, and where it cannot be told.
    #[must_use]
    pub fn foreground_job(&self) -> Option<String> {
        let group = foreground_group(&ps_line(self.shell_pid?, "tpgid=,pgid=")?)?;
        let name = ps_line(group, "comm=").unwrap_or_default();
        let name = name.rsplit('/').next().unwrap_or("").trim().to_string();
        Some(if name.is_empty() { group.to_string() } else { name })
    }

    /// Whether the sample shell runs a command that is still writing.
    pub fn streaming(&self) -> bool {
        matches!(
            &self.backend,
            Backend::Replay {
                stream: Some(_),
                ..
            }
        )
    }

    /// The next lines of the sample shell's running command, into the
    /// screen (the flag raised); output that comes in like a program's,
    /// leaving the view where the user put it. Returns whether there were
    /// any. A shell on a PTY writes by itself.
    pub fn pump(&mut self) -> bool {
        let Backend::Replay { parser, stream, .. } = &mut self.backend else {
            return false;
        };
        let Some(running) = stream.as_mut() else {
            return false;
        };
        let bytes = running.take(STREAM_LINES_PER_TICK);
        if running.done() {
            *stream = None;
        }
        if bytes.is_empty() {
            return false;
        }
        let mut term = self.term.lock();
        parser.advance(&mut *term, &bytes);
        drop(term);
        self.signals.dirty.store(true, Ordering::Release);
        true
    }

    /// Whether a shell (not a recording) runs here.
    pub fn is_live(&self) -> bool {
        matches!(self.backend, Backend::Pty(_))
    }

    /// The grid the engine has now.
    pub fn size(&self) -> GridSize {
        self.size
    }

    /// `bytes` to the program (a recording echoes them), and the view back
    /// at the output.
    pub fn write(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        match &mut self.backend {
            Backend::Pty(notifier) => {
                notifier.notify(bytes);
                // Typing brings the view back to the output (the engine's
                // scroll raises the flag: drawn on the next tick, even when
                // the program echoes nothing); already there, nothing to do.
                let mut term = self.term.lock();
                if term.grid().display_offset() != 0 {
                    term.scroll_display(Scroll::Bottom);
                }
            }
            Backend::Replay {
                parser,
                line,
                stream,
            } => {
                // The sample shell's line: Return runs it, Backspace takes a
                // character back, Ctrl+C stops what runs.
                let mut command = None;
                let echo: Vec<u8> = if bytes == b"\r" {
                    command = Some(std::mem::take(line));
                    b"\r\n".to_vec()
                } else if bytes == [0x7f] {
                    line.pop();
                    b"\x08 \x08".to_vec()
                } else if bytes == [0x03] {
                    line.clear();
                    *stream = None;
                    b"^C\r\n".to_vec()
                } else {
                    line.push_str(&String::from_utf8_lossy(&bytes));
                    bytes
                };
                let mut term = self.term.lock();
                term.scroll_display(Scroll::Bottom);
                parser.advance(&mut *term, &echo);
                drop(term);
                if let Some(command) = command {
                    if stream.is_none() {
                        *stream = Stream::parse(&command);
                    }
                }
                self.signals.dirty.store(true, Ordering::Release);
            }
        }
    }

    /// The grid the view has room for: the engine and the PTY follow
    /// (`TIOCSWINSZ`, the program gets `SIGWINCH`).
    pub fn resize(&mut self, size: GridSize) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        if let Backend::Pty(notifier) = &mut self.backend {
            notifier.on_resize(window_size(size));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Backend::Pty(notifier) = &self.backend {
            let _ = notifier.0.send(Msg::Shutdown);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_program_holds_the_terminal_when_its_group_is_not_the_shells() {
        use super::foreground_group;
        assert_eq!(foreground_group("  4321   4300\n"), Some(4321), "vim started by the shell");
        assert_eq!(foreground_group("  4300   4300\n"), None, "the shell waits for input");
        assert_eq!(foreground_group("    -1   4300"), None, "no terminal");
        assert_eq!(foreground_group(""), None, "the shell is gone");
        assert_eq!(foreground_group("tpgid pgid"), None);
    }

    use super::*;

    fn text(session: &Session) -> Vec<String> {
        let term = session.term.lock();
        let screen = crate::vt::screen(&*term, true);
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

    #[test]
    fn a_recording_replays_into_the_screen_and_echoes_what_is_typed() {
        let mut s = Session::replay(b"$ ", GridSize::new(20, 3), 100);
        assert!(!s.is_live());
        assert!(s.signals.take_dirty());
        s.write(b"ls".to_vec());
        s.write(b"\r".to_vec());
        assert_eq!(text(&s), ["$ ls", "", ""]);
        assert!(s.signals.take_dirty());
        assert!(!s.signals.take_dirty());
    }

    #[test]
    fn the_sample_shell_streams_seq_a_few_lines_a_tick() {
        let mut s = Session::replay(b"$ ", GridSize::new(20, 5), 1000);
        s.write(b"seq 125".to_vec());
        // Typed with a slip: the sample shell's line has its backspace.
        s.write(b"\x7f".to_vec());
        s.write(b"0".to_vec());
        assert!(!s.streaming());
        s.write(b"\r".to_vec());
        assert!(s.streaming());
        let _ = s.signals.take_dirty();
        let mut ticks = 0;
        while s.pump() {
            ticks += 1;
            assert!(s.signals.take_dirty(), "every chunk is drawn");
        }
        assert_eq!(ticks, 120usize.div_ceil(crate::sample::STREAM_LINES_PER_TICK));
        assert!(!s.streaming());
        let rows = text(&s);
        assert_eq!(rows[rows.len() - 2], "120");
        // Typed text echoes again once it is over.
        s.write(b"ls".to_vec());
        assert!(text(&s).iter().any(|r| r == "ls"));
        // A line that is no command streams nothing.
        s.write(b"\r".to_vec());
        assert!(!s.streaming());
        assert!(!s.pump());
    }

    #[test]
    fn a_resize_changes_the_engine_grid_once() {
        let mut s = Session::replay(b"", GridSize::new(20, 3), 100);
        s.resize(GridSize::new(40, 10));
        assert_eq!(s.size(), GridSize::new(40, 10));
        assert_eq!(text(&s).len(), 10);
    }

    #[test]
    fn the_listener_records_a_title_a_bell_and_an_exit() {
        let signals = Arc::new(Signals::default());
        let l = Listener(signals.clone());
        l.send_event(Event::Title("vim".to_string()));
        l.send_event(Event::Bell);
        assert_eq!(signals.take_title().as_deref(), Some("vim"));
        assert_eq!(signals.take_title(), None);
        assert!(signals.take_bell());
        assert!(!signals.exited.load(Ordering::Acquire));
        l.send_event(Event::Exit);
        assert!(signals.exited.load(Ordering::Acquire));
    }

    /// A command that exits by itself tells how it ended: its exit code, kept until the app
    /// reads it. A program ended by a signal has none.
    #[cfg(unix)]
    #[test]
    fn the_listener_keeps_the_exit_code_of_the_program() {
        use std::os::unix::process::ExitStatusExt;
        let signals = Arc::new(Signals::default());
        let l = Listener(signals.clone());
        assert_eq!(signals.exit_code(), None);
        l.send_event(Event::ChildExit(std::process::ExitStatus::from_raw(3 << 8)));
        assert!(signals.exited.load(Ordering::Acquire));
        assert_eq!(signals.exit_code(), Some(3));
        // SIGINT (2): ended by a signal, no code.
        let killed = Arc::new(Signals::default());
        Listener(killed.clone()).send_event(Event::ChildExit(std::process::ExitStatus::from_raw(2)));
        assert!(killed.exited.load(Ordering::Acquire));
        assert_eq!(killed.exit_code(), None);
    }

    /// The text of the grid's screen lines, trailing blanks cut.
    fn screen_text(session: &Session) -> String {
        text(session).join("\n")
    }

    /// A one-shot command (AzDrive's command bar) gets the extra variables it asks for, starts
    /// in its folder, and its last output is read before the session says it ended: nothing
    /// it printed just before it exited is lost.
    #[cfg(unix)]
    #[test]
    fn a_one_shot_command_runs_with_its_variables_in_its_folder_and_keeps_its_last_output() {
        let dir = std::env::temp_dir();
        let options = SpawnOptions {
            env: vec![(String::from("AZTERM_PROBE"), String::from("probe-42"))],
            drain_on_exit: true,
        };
        let mut session = Session::spawn_with(
            "AzDrive",
            Some((
                String::from("/bin/sh"),
                vec![
                    String::from("-c"),
                    String::from("printf '%s %s\\n' \"$AZTERM_PROBE\" \"$TERM_PROGRAM\"; pwd; exit 3"),
                ],
            )),
            Some(dir.clone()),
            GridSize::new(80, 6),
            100,
            options,
        )
        .expect("a PTY");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !session.signals.exited.load(Ordering::Acquire) && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(session.signals.exited.load(Ordering::Acquire), "the command ended");
        assert_eq!(session.signals.exit_code(), Some(3));
        let shown = screen_text(&session);
        assert!(shown.contains("probe-42 AzDrive"), "{shown}");
        let folder = dir.canonicalize().unwrap_or(dir);
        let name = folder.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        assert!(shown.contains(&name), "{shown}");
        assert!(!session.alternate_screen(), "a plain command stays on the main screen");
        session.write(b"x".to_vec());
    }

    /// A full-screen program switches to the alternate screen; the session says so, so a
    /// one-line command bar can tell the user it needs a real terminal.
    #[test]
    fn the_session_tells_when_a_program_took_the_alternate_screen() {
        let plain = Session::replay(b"plain output\r\n", GridSize::new(20, 3), 10);
        assert!(!plain.alternate_screen());
        let full = Session::replay(b"\x1b[?1049h\x1b[2J", GridSize::new(20, 3), 10);
        assert!(full.alternate_screen());
    }

    /// OSC 52: a program (vim, tmux, over ssh) may put text on the clipboard; it never reads
    /// it - what the user copied elsewhere is not a program's to take.
    #[test]
    fn a_program_may_set_the_clipboard_but_never_read_it() {
        use alacritty_terminal::term::ClipboardType;
        let signals = Arc::new(Signals::default());
        let l = Listener(signals.clone());
        l.send_event(Event::ClipboardStore(ClipboardType::Clipboard, "token 42".to_string()));
        assert_eq!(signals.take_clipboard().as_deref(), Some("token 42"));
        assert_eq!(signals.take_clipboard(), None);
        l.send_event(Event::ClipboardLoad(
            ClipboardType::Clipboard,
            Arc::new(|text: &str| format!("\u{1b}]52;c;{text}\u{7}")),
        ));
        assert_eq!(signals.take_clipboard(), None);
    }
}
