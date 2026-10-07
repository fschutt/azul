//! The glue between a [`Session`] and azul's `TerminalView` that every app
//! with a terminal needs: the data callback's screen for the grid the view
//! has room for, the view's events applied to the engine, when a flood of
//! output is drawn, a new shell's title, and the re-render of the view alone
//! (its `VirtualView`, not the window) when output arrives.

use std::path::{Path, PathBuf};

use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::{Column, Point, Side},
    selection::{Selection, SelectionType},
    term::{viewport_to_point, Term},
};
use azul::{
    callbacks::CallbackInfo,
    dom::{DomId, NodeId},
    widgets::{
        TerminalGridSize, TerminalScreen, TerminalSelectionKind, TerminalViewEvent,
        TerminalViewEventKind,
    },
};

use crate::{
    session::Session,
    vt::{self, GridSize},
};

/// Ticks in a row with new output before the terminal re-renders on every
/// other tick only ([`renders_now`]).
pub const FLOOD_TICKS: u32 = 3;

/// Whether the terminal re-renders on the `streak`-th tick in a row with
/// new output: at once for output that has just arrived (an echo, a
/// prompt), on every other tick in a flood (`tree`, a build log) - smooth
/// for text going by, and the UI thread keeps room for the keys and the
/// wheel. A skipped tick keeps the flag up, so the next one draws it.
#[must_use]
pub const fn renders_now(streak: u32) -> bool {
    streak <= FLOOD_TICKS || streak % 2 == 0
}

/// The screen the view shows for the grid it has room for: the engine and
/// the PTY are resized to `size` first (`TIOCSWINSZ`; nothing happens when
/// the size is the engine's already).
pub fn screen_for(
    session: &mut Session,
    size: TerminalGridSize,
    alt_sends_escape: bool,
) -> TerminalScreen {
    let columns = usize::try_from(size.columns).unwrap_or(usize::MAX);
    let rows = usize::try_from(size.rows).unwrap_or(usize::MAX);
    session.resize(GridSize::new(columns, rows));
    let term = session.term.lock();
    let screen = vt::screen(&*term, alt_sends_escape);
    drop(term);
    screen
}

/// The engine point of the cell in view an event names.
pub fn grid_point<T>(term: &Term<T>, event: &TerminalViewEvent) -> Point {
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

/// The view's action applied to the session: `Input` goes to the program,
/// a scroll moves the engine's view, a selection gesture selects. Returns
/// the text a `Copy` asks to put on the clipboard (`None`: nothing to put).
///
/// The view re-renders itself after a scroll or a selection, reading the
/// engine as it is then, so the output flag the engine's own scroll raised
/// is taken: the next tick need not draw the same screen again.
pub fn apply_event(session: &mut Session, event: &TerminalViewEvent) -> Option<String> {
    if event.kind == TerminalViewEventKind::Input {
        session.write(event.bytes.as_slice().to_vec());
        return None;
    }
    let mut term = session.term.lock();
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
            let point = grid_point(&*term, event);
            term.selection = Some(Selection::new(kind, point, side));
        }
        TerminalViewEventKind::SelectExtend => {
            let point = grid_point(&*term, event);
            if let Some(selection) = term.selection.as_mut() {
                selection.update(point, side);
            }
        }
        TerminalViewEventKind::SelectClear => term.selection = None,
        TerminalViewEventKind::Copy => {
            let text = term.selection_to_string();
            drop(term);
            return text.filter(|t| !t.is_empty());
        }
        TerminalViewEventKind::SelectEnd | TerminalViewEventKind::Input => {}
    }
    drop(term);
    let _ = session.signals.take_dirty();
    None
}

/// Re-renders the terminal view in the node with DOM id `id` (of the
/// window's own DOM) - only its `VirtualView`, the node's first child; not
/// every view of the window (a title bar's maximize glyph, an icon view:
/// each would be rebuilt with it on every frame of output). `false` when
/// the window has no such node (the view is not shown).
pub fn rerender(info: &mut CallbackInfo, id: &str) -> bool {
    let dom = DomId { inner: 0 };
    // `into_raw` is the 1-based encoding (0 = none); `NodeId` is 0-based.
    let raw = info.get_node_id_by_id_attribute(dom, id).into_raw();
    if raw == 0 {
        return false;
    }
    let host = NodeId { inner: raw - 1 };
    let view = info.get_first_child_node(dom, host).into_raw();
    if view == 0 {
        info.trigger_all_virtual_view_rerender();
    } else {
        info.trigger_virtual_view_rerender(dom, NodeId { inner: view - 1 });
    }
    true
}

/// A new shell's title until the shell sets one: the shell's name and its
/// folder, the home folder as `~` ("zsh ~", "bash ~/src").
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

/// The user's home folder (where a shell starts without a folder of its
/// own).
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flood_of_output_is_drawn_on_every_other_tick_and_a_little_at_once() {
        for streak in 1..=FLOOD_TICKS {
            assert!(renders_now(streak), "tick {streak}");
        }
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
    }

    #[test]
    fn the_screen_follows_the_grid_the_view_has_room_for() {
        let mut s = Session::replay(b"$ ", GridSize::new(20, 3), 100);
        let screen = screen_for(
            &mut s,
            TerminalGridSize {
                columns: 50,
                rows: 7,
            },
            true,
        );
        assert_eq!(screen.lines.as_slice().len(), 7);
        assert_eq!(s.size(), GridSize::new(50, 7));
    }

    #[test]
    fn input_goes_to_the_program_and_a_scroll_moves_the_engines_view() {
        let mut lines = Vec::new();
        for i in 0..100u32 {
            lines.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let mut s = Session::replay(&lines, GridSize::new(20, 5), 1_000);
        let _ = s.signals.take_dirty();
        let up = TerminalViewEvent::scrolled(10);
        assert_eq!(apply_event(&mut s, &up), None);
        assert_eq!(s.term.lock().grid().display_offset(), 10);
        assert!(!s.signals.take_dirty(), "the view draws its own scroll");
        let typed = TerminalViewEvent::input(b"ls".to_vec());
        assert_eq!(apply_event(&mut s, &typed), None);
        assert_eq!(
            s.term.lock().grid().display_offset(),
            0,
            "typing brings the view back to the output"
        );
    }
}
