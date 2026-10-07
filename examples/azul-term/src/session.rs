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

use crate::vt::GridSize;

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
            // OSC 52 (the clipboard), colour and size queries: not answered.
            _ => {}
        }
    }
}

/// How the bytes reach the program.
enum Backend {
    /// A shell on a PTY, through its event loop.
    Pty(Notifier),
    /// A recording: what is typed is echoed into the screen.
    Replay(Processor<StdSyncHandler>),
}

/// One shell and its screen.
pub struct Session {
    /// The engine (the event loop thread writes into it).
    pub term: Arc<FairMutex<Term<Listener>>>,
    /// What the engine told the app.
    pub signals: Arc<Signals>,
    backend: Backend,
    size: GridSize,
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
        ..Config::default()
    };
    Term::new(config, &size, Listener(signals.clone()))
}

impl Session {
    /// The user's shell (or `program` with `args`) in `cwd` on a new PTY.
    ///
    /// # Errors
    /// The PTY could not be opened or the shell not started.
    pub fn spawn(
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
            .insert("TERM_PROGRAM".to_string(), "AzTerm".to_string());
        let pty = tty::new(&options, window_size(size), 0)?;
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
            backend: Backend::Replay(parser),
            size,
        }
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
            Backend::Replay(parser) => {
                let echo: Vec<u8> = if bytes == b"\r" {
                    b"\r\n".to_vec()
                } else if bytes == [0x7f] {
                    b"\x08 \x08".to_vec()
                } else {
                    bytes
                };
                let mut term = self.term.lock();
                term.scroll_display(Scroll::Bottom);
                parser.advance(&mut *term, &echo);
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
}
