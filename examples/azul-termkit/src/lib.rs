//! azul-termkit: the terminal plumbing the Azlin apps share - AzTerm's
//! window of tabs and AzCode's terminal panel run the same code.
//!
//! - [`session`]: one shell on a PTY (alacritty_terminal's tty and its event
//!   loop thread) or a recording replayed; what the engine told the app
//!   ([`session::Signals`], polled from a timer - no callback runs on the PTY
//!   thread).
//! - [`vt`]: the engine's grid read as azul's `TerminalScreen` (the rows in
//!   view only, so a scrollback of any length costs what a screen does).
//! - [`pane`]: the glue to azul's `TerminalView` - the screen for the grid
//!   the view has room for (the engine and the PTY follow it), the view's
//!   events applied (the bytes to the program, the scroll, the selection, the
//!   copy), when a flood of output is drawn, the re-render of the view alone.

pub mod pane;
pub mod session;
pub mod vt;
