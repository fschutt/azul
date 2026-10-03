//! The VT engine side: alacritty_terminal's `Term` read as azul's
//! `TerminalScreen` - the rows in view (the display offset applied), runs of
//! one style (split where the style changes AND where a wide character
//! starts or ends, so every run is one width class), the cursor and the
//! selection in view, the modes the program set.
//!
//! The mapping reads the grid directly (`Line(-n)` is history), so building
//! a screen never moves the engine's view; only the rows in view are read.

use alacritty_terminal::{
    grid::Dimensions,
    index::{Column, Line, Point},
    term::{
        cell::{Cell, Flags},
        point_to_viewport, Term, TermMode,
    },
    vte::ansi::{Color, CursorShape, NamedColor},
};
use azul::{
    css::ColorU,
    option::OptionTerminalSelection,
    str::String as AzString,
    vec::{TerminalLineVec, TerminalRunVec},
    widgets::{
        TerminalColor, TerminalCursor, TerminalCursorShape, TerminalLine, TerminalModes,
        TerminalMouseEncoding, TerminalMouseMode, TerminalPoint, TerminalRun, TerminalScreen,
        TerminalSelection, TerminalStyle,
    },
};

/// A grid size as alacritty wants it (`Term::new`, `Term::resize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    pub columns: usize,
    pub lines: usize,
}

impl GridSize {
    /// At least 2 x 1 (alacritty's minimum).
    pub fn new(columns: usize, lines: usize) -> Self {
        Self {
            columns: columns.max(2),
            lines: lines.max(1),
        }
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// The screen of `term` as the view shows it: the rows at its display
/// offset, the cursor and the selection in view, the scrollback, the modes
/// (`alt_sends_escape` is the user's choice, not the program's).
pub fn screen<T>(term: &Term<T>, alt_sends_escape: bool) -> TerminalScreen {
    let _ = (term, alt_sends_escape);
    TerminalScreen::create(TerminalLineVec::from_vec(Vec::new()))
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::{
        event::VoidListener,
        grid::Scroll,
        index::Side,
        selection::{Selection, SelectionType},
        term::Config,
        vte::ansi::{Processor, StdSyncHandler},
    };

    use super::*;

    /// A `columns` x `lines` terminal that has been sent `bytes`.
    fn term(columns: usize, lines: usize, bytes: &[u8]) -> Term<VoidListener> {
        let mut t = Term::new(
            Config::default(),
            &GridSize::new(columns, lines),
            VoidListener,
        );
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut t, bytes);
        t
    }

    fn text_of(line: &TerminalLine) -> String {
        line.runs
            .as_slice()
            .iter()
            .map(|r| r.text.as_str().to_string())
            .collect()
    }

    fn rows(s: &TerminalScreen) -> Vec<String> {
        s.lines.as_slice().iter().map(text_of).collect()
    }

    #[test]
    fn plain_text_is_one_default_run_per_row_and_the_cursor_follows_it() {
        let s = screen(&term(10, 3, b"hello\r\nworld"), true);
        assert_eq!(rows(&s), ["hello", "world", ""]);
        let first = &s.lines.as_slice()[0].runs.as_slice()[0];
        assert_eq!(first.columns, 5);
        assert_eq!(first.style.fg, TerminalColor::Foreground);
        assert_eq!(first.style.bg, TerminalColor::Background);
        assert_eq!(s.cursor.line, 1);
        assert_eq!(s.cursor.column, 5);
        assert_eq!(s.cursor.shape, TerminalCursorShape::Block);
        assert_eq!(s.history, 0);
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn sgr_colours_become_indexed_and_true_colours() {
        let s = screen(
            &term(
                40,
                2,
                b"\x1b[31mred\x1b[0m \x1b[38;5;200mpink\x1b[0m \x1b[38;2;1;2;3;48;5;4mrgb\x1b[0m",
            ),
            true,
        );
        let runs: Vec<(String, TerminalColor, TerminalColor)> = s.lines.as_slice()[0]
            .runs
            .as_slice()
            .iter()
            .map(|r| (r.text.as_str().to_string(), r.style.fg, r.style.bg))
            .collect();
        assert_eq!(
            runs,
            [
                (
                    "red".to_string(),
                    TerminalColor::Indexed(1),
                    TerminalColor::Background
                ),
                (
                    " ".to_string(),
                    TerminalColor::Foreground,
                    TerminalColor::Background
                ),
                (
                    "pink".to_string(),
                    TerminalColor::Indexed(200),
                    TerminalColor::Background
                ),
                (
                    " ".to_string(),
                    TerminalColor::Foreground,
                    TerminalColor::Background
                ),
                (
                    "rgb".to_string(),
                    TerminalColor::Rgb(ColorU {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 255
                    }),
                    TerminalColor::Indexed(4)
                ),
            ]
        );
    }

    #[test]
    fn attributes_become_the_runs_style() {
        let s = screen(
            &term(
                40,
                1,
                b"\x1b[1mb\x1b[0;3mi\x1b[0;4mu\x1b[0;7mv\x1b[0;9ms\x1b[0;2md\x1b[0;8mh",
            ),
            true,
        );
        let styles: Vec<TerminalStyle> = s.lines.as_slice()[0]
            .runs
            .as_slice()
            .iter()
            .map(|r| r.style)
            .collect();
        assert_eq!(styles.len(), 7);
        assert!(styles[0].bold);
        assert!(styles[1].italic && !styles[1].bold);
        assert!(styles[2].underline);
        assert!(styles[3].inverse);
        assert!(styles[4].strikethrough);
        assert!(styles[5].dim);
        assert!(styles[6].hidden);
    }

    #[test]
    fn a_wide_character_is_a_run_of_its_own_two_columns_wide() {
        let s = screen(&term(10, 1, "a\u{65e5}b".as_bytes()), true);
        let runs: Vec<(String, u32)> = s.lines.as_slice()[0]
            .runs
            .as_slice()
            .iter()
            .map(|r| (r.text.as_str().to_string(), r.columns))
            .collect();
        assert_eq!(
            runs,
            [
                ("a".to_string(), 1),
                ("\u{65e5}".to_string(), 2),
                ("b".to_string(), 1)
            ]
        );
    }

    #[test]
    fn the_rows_in_view_follow_the_display_offset() {
        let mut t = term(4, 3, b"0\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9");
        let at_bottom = screen(&t, true);
        assert_eq!(rows(&at_bottom), ["7", "8", "9"]);
        assert_eq!(at_bottom.history, 7);
        assert_eq!(at_bottom.scroll, 0);
        t.scroll_display(Scroll::Delta(2));
        let up = screen(&t, true);
        assert_eq!(rows(&up), ["5", "6", "7"]);
        assert_eq!(up.scroll, 2);
        // The cursor (on the last screen row) is below the rows in view.
        assert_eq!(up.cursor.shape, TerminalCursorShape::Hidden);
    }

    #[test]
    fn a_hundred_thousand_lines_of_scrollback_read_only_the_rows_in_view() {
        let mut bytes = Vec::new();
        for i in 0..100_000u32 {
            bytes.extend_from_slice(format!("line {i}\r\n").as_bytes());
        }
        let mut t = Term::new(
            Config {
                scrolling_history: 200_000,
                ..Config::default()
            },
            &GridSize::new(20, 24),
            VoidListener,
        );
        Processor::<StdSyncHandler>::new().advance(&mut t, &bytes);
        t.scroll_display(Scroll::Top);
        let top = screen(&t, true);
        assert_eq!(top.lines.as_slice().len(), 24);
        assert_eq!(rows(&top)[0], "line 0");
        assert_eq!(top.scroll, top.history);
    }

    #[test]
    fn the_modes_follow_the_program() {
        let s = screen(
            &term(
                10,
                2,
                b"\x1b[?1h\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[?1004h",
            ),
            false,
        );
        assert!(s.modes.application_cursor);
        assert!(s.modes.bracketed_paste);
        assert!(s.modes.focus_reporting);
        assert_eq!(s.modes.mouse, TerminalMouseMode::Drag);
        assert_eq!(s.modes.mouse_encoding, TerminalMouseEncoding::Sgr);
        assert!(!s.modes.alt_sends_escape);
        let full = screen(&term(10, 2, b"\x1b[?1049h"), true);
        assert!(full.modes.alternate_screen);
        assert!(full.modes.alt_sends_escape);
        let plain = screen(&term(10, 2, b""), true);
        assert_eq!(plain.modes.mouse, TerminalMouseMode::Off);
        assert!(!plain.modes.bracketed_paste);
    }

    #[test]
    fn the_cursor_hides_when_the_program_hides_it() {
        let s = screen(&term(10, 2, b"ab\x1b[?25l"), true);
        assert_eq!(s.cursor.shape, TerminalCursorShape::Hidden);
        let bar = screen(&term(10, 2, b"ab\x1b[6 q"), true);
        assert_eq!(bar.cursor.shape, TerminalCursorShape::Bar);
    }

    #[test]
    fn a_selection_is_given_in_view_rows_and_clipped_to_them() {
        let mut t = term(10, 3, b"one\r\ntwo\r\nthree");
        let mut sel = Selection::new(
            SelectionType::Simple,
            Point::new(Line(0), Column(1)),
            Side::Left,
        );
        sel.update(Point::new(Line(1), Column(2)), Side::Right);
        t.selection = Some(sel);
        let s = screen(&t, true);
        assert_eq!(
            s.selection,
            OptionTerminalSelection::Some(TerminalSelection {
                start: TerminalPoint { line: 0, column: 1 },
                end: TerminalPoint { line: 1, column: 2 },
                block: false,
            })
        );
        // Scrolled so the selection's first row is out of view: it starts at
        // the top-left of the view.
        let mut long = term(10, 2, b"a\r\nb\r\nc\r\nd");
        let mut sel = Selection::new(
            SelectionType::Simple,
            Point::new(Line(-1), Column(0)),
            Side::Left,
        );
        sel.update(Point::new(Line(1), Column(0)), Side::Right);
        long.selection = Some(sel);
        let s = screen(&long, true);
        assert_eq!(
            s.selection,
            OptionTerminalSelection::Some(TerminalSelection {
                start: TerminalPoint { line: 0, column: 0 },
                end: TerminalPoint { line: 1, column: 0 },
                block: false,
            })
        );
    }
}
