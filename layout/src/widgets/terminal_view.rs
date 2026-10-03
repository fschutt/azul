//! Terminal view widget - the surface of a terminal emulator: a grid of
//! styled character cells (16 / 256 / true colours, bold, dim, italic,
//! underline, strikethrough, inverse, hidden), a cursor (block, hollow block,
//! underline, bar), a selection, and a scrollback of any length. AzTerm's
//! pane; a log viewer's follow view.
//!
//! GENERIC OVER ITS DATA: the view holds no lines and parses no escape
//! sequences. It asks the app for the screen it shows through a DATA callback
//! ([`TerminalView::with_data_source`]), given the grid size it has room for
//! ([`TerminalGridSize`]: whole columns and rows of its cell). The app answers
//! with a [`TerminalScreen`] - the rows in view, the cursor, the selection,
//! how much scrollback there is and how far the view is scrolled up, and the
//! modes the program asked for - read from its VT engine (AzTerm: the
//! `alacritty_terminal` grid). A grid size that differs from the engine's is
//! the app's cue to resize the engine and the PTY (`TIOCSWINSZ`).
//!
//! VIRTUALISED IN WHOLE LINES: the view is a `VirtualView` host (like the map
//! widget: an outer node with the handlers, a `VirtualView` inside it that
//! renders the rows), so it knows its own size and the app can re-render it
//! alone when output arrives (`CallbackInfo::trigger_all_virtual_view_rerender`)
//! without rebuilding the window. Scrolling is the scroll-window pattern of
//! the cell grid and the data table, in whole lines: the position is the
//! engine's DISPLAY OFFSET - lines scrolled up from the bottom, 0 = following
//! the output - so a scrollback of 100,000 or 10,000,000 lines costs the same
//! and no `f32` has to address it in pixels. Only the rows in view are built.
//!
//! THE APP OWNS THE STATE: the scroll position and the selection are the
//! engine's; every action is a [`TerminalViewEvent`]: `Input` carries the
//! bytes for the program (keys, typed text, a paste, a mouse report, a focus
//! report - already encoded for the modes the program asked for), `Scroll`
//! the display offset to show, `SelectStart` / `SelectExtend` / `SelectEnd`
//! the cell a selection gesture is at, `Copy` asks for the selection's text
//! on the clipboard. After a `Scroll` or a selection event the view re-renders
//! itself; after `Input` the app writes the bytes to the PTY.
//!
//! KEYBOARD (the view is ONE Tab stop, and keeps its keys): xterm encodings -
//! the arrows, Home / End (`CSI` or `SS3` in application cursor mode, `CSI 1;m`
//! with modifiers), Insert / Delete / Page Up / Page Down (`CSI n~`), F1-F20,
//! Ctrl+letter (C0 controls), Alt+key as ESC + key (when Alt sends escape),
//! Enter, Tab / Shift+Tab, Backspace, Escape, the keypad in application keypad
//! mode; typed text goes out as UTF-8. Copy / paste: Cmd+C / Cmd+V on macOS,
//! Ctrl+Shift+C / Ctrl+Shift+V elsewhere (Ctrl+C stays the program's
//! interrupt), Shift+Insert pastes; a paste is bracketed (`CSI 200~ ...
//! CSI 201~`) when the program asked for it. Shift+Page Up / Page Down /
//! Home / End scroll the scrollback.
//!
//! POINTER: a press starts a selection (a double-click a word, Alt+drag a
//! block), a drag extends it; when the program asked for mouse reports
//! (`CSI ?1000h` / `1002` / `1003`, encodings `1005` / `1006`) the pointer is
//! the program's instead, Shift+drag still selects. The wheel scrolls the
//! scrollback (3 lines a notch), or is reported, or - on the alternate screen
//! with alternate scroll on - becomes arrow keys. The scroll bar on the right
//! drags.
//!
//! COLOURS: [`TerminalPalette`] - the 16 ANSI colours, the default ink and
//! ground, the cursor and the selection, each by day and at night
//! ([`ChartColor`]); 16..231 is the 6x6x6 cube, 232..255 the grey ramp, true
//! colour as it is. The app's palette, or the app theme's
//! ([`TerminalPalette::flat`], [`TerminalPalette::flora_ink`]).
//!
//! Key types: [`TerminalView`], [`TerminalScreen`], [`TerminalLine`],
//! [`TerminalRun`], [`TerminalStyle`], [`TerminalColor`], [`TerminalModes`],
//! [`TerminalPalette`], [`TerminalViewEvent`].

use alloc::{string::String, vec::Vec};

use azul_core::{callbacks::Update, events::KeyModifiers, refany::RefAny, window::VirtualKeyCode};
use azul_css::{
    corety::U8Vec, impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq, props::basic::color::ColorU, AzString,
};

use crate::{callbacks::CallbackInfo, widgets::chart::ChartColor};

// ---- the cells ----

/// A colour of a cell, as the program asked for it. The palette turns it
/// into a colour ([`TerminalPalette::color_of`]).
#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalColor {
    /// The default ink (`SGR 39`).
    #[default]
    Foreground,
    /// The default ground (`SGR 49`).
    Background,
    /// A palette index: 0..15 the ANSI colours, 16..231 the 6x6x6 cube,
    /// 232..255 the grey ramp (`SGR 30-37 / 90-97`, `SGR 38;5;n`).
    Indexed(u8),
    /// A true colour (`SGR 38;2;r;g;b`).
    Rgb(ColorU),
}

/// How a run of cells is drawn: its colours and its attributes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TerminalStyle {
    /// The ink.
    pub fg: TerminalColor,
    /// The ground.
    pub bg: TerminalColor,
    /// `SGR 1`.
    pub bold: bool,
    /// `SGR 2`: the ink at two thirds.
    pub dim: bool,
    /// `SGR 3`.
    pub italic: bool,
    /// `SGR 4` (any underline style).
    pub underline: bool,
    /// `SGR 9`.
    pub strikethrough: bool,
    /// `SGR 7`: ink and ground swapped.
    pub inverse: bool,
    /// `SGR 8`: the ink is the ground.
    pub hidden: bool,
}

impl TerminalStyle {
    /// The default ink on the default ground, no attributes.
    #[must_use]
    pub const fn create() -> Self {
        Self {
            fg: TerminalColor::Foreground,
            bg: TerminalColor::Background,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            strikethrough: false,
            inverse: false,
            hidden: false,
        }
    }

    /// `fg` on `bg`, no attributes.
    #[must_use]
    pub const fn colored(fg: TerminalColor, bg: TerminalColor) -> Self {
        let mut s = Self::create();
        s.fg = fg;
        s.bg = bg;
        s
    }
}

/// Cells next to each other in one style: their text and how many columns
/// they cover (a wide character covers two).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TerminalRun {
    /// The characters, in order (a wide character once).
    pub text: AzString,
    /// The columns the run covers.
    pub columns: u32,
    /// How the run is drawn.
    pub style: TerminalStyle,
}

impl TerminalRun {
    /// `text` covering `columns` columns, drawn in `style`.
    #[must_use]
    pub const fn create(text: AzString, columns: u32, style: TerminalStyle) -> Self {
        Self {
            text,
            columns,
            style,
        }
    }
}

impl_option!(
    TerminalRun,
    OptionTerminalRun,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash]
);
impl_vec!(
    TerminalRun,
    TerminalRunVec,
    TerminalRunVecDestructor,
    TerminalRunVecDestructorType,
    TerminalRunVecSlice,
    OptionTerminalRun
);
impl_vec_clone!(TerminalRun, TerminalRunVec, TerminalRunVecDestructor);
impl_vec_debug!(TerminalRun, TerminalRunVec);
impl_vec_mut!(TerminalRun, TerminalRunVec);
impl_vec_partialeq!(TerminalRun, TerminalRunVec);

/// One row of the grid: its runs, left to right.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerminalLine {
    /// The runs, left to right; the columns after the last run are blank.
    pub runs: TerminalRunVec,
    /// The row goes on in the next one (a soft wrap): a copy joins them.
    pub wrapped: bool,
}

impl TerminalLine {
    /// A row of `runs`.
    #[must_use]
    pub const fn create(runs: TerminalRunVec) -> Self {
        Self {
            runs,
            wrapped: false,
        }
    }

    /// A row of one run of plain `text` (one column per character).
    #[must_use]
    pub fn plain(text: AzString) -> Self {
        let columns = u32::try_from(text.as_str().chars().count()).unwrap_or(u32::MAX);
        Self::create(TerminalRunVec::from_vec(alloc::vec![TerminalRun::create(
            text,
            columns,
            TerminalStyle::create(),
        )]))
    }

    /// The row's text: every run's, in order.
    #[must_use]
    pub fn text(&self) -> AzString {
        let mut s = String::new();
        for run in self.runs.as_ref() {
            s.push_str(run.text.as_str());
        }
        AzString::from(s)
    }

    /// The columns the runs cover.
    #[must_use]
    pub fn columns(&self) -> u32 {
        self.runs
            .as_ref()
            .iter()
            .fold(0u32, |n, r| n.saturating_add(r.columns))
    }
}

impl_option!(
    TerminalLine,
    OptionTerminalLine,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    TerminalLine,
    TerminalLineVec,
    TerminalLineVecDestructor,
    TerminalLineVecDestructorType,
    TerminalLineVecSlice,
    OptionTerminalLine
);
impl_vec_clone!(TerminalLine, TerminalLineVec, TerminalLineVecDestructor);
impl_vec_debug!(TerminalLine, TerminalLineVec);
impl_vec_mut!(TerminalLine, TerminalLineVec);
impl_vec_partialeq!(TerminalLine, TerminalLineVec);

// ---- the cursor and the selection ----

/// A cell of the view: its row (0 = the top row in view) and its column.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TerminalPoint {
    /// The row in view, 0 at the top.
    pub line: u32,
    /// The column, 0 at the left.
    pub column: u32,
}

impl TerminalPoint {
    /// The cell at `line`, `column`.
    #[must_use]
    pub const fn create(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// A selection in view: every cell from `start` to `end`, both included,
/// reading order (`block`: the rectangle between them).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TerminalSelection {
    /// The first cell (top-left of a block).
    pub start: TerminalPoint,
    /// The last cell (bottom-right of a block).
    pub end: TerminalPoint,
    /// A rectangle, not a text run.
    pub block: bool,
}

impl TerminalSelection {
    /// The cells from `start` to `end` in reading order (given in any order).
    #[must_use]
    pub fn create(a: TerminalPoint, b: TerminalPoint) -> Self {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        Self {
            start,
            end,
            block: false,
        }
    }

    /// The rectangle between two corners (given in any order).
    #[must_use]
    pub fn create_block(a: TerminalPoint, b: TerminalPoint) -> Self {
        Self {
            start: TerminalPoint::create(a.line.min(b.line), a.column.min(b.column)),
            end: TerminalPoint::create(a.line.max(b.line), a.column.max(b.column)),
            block: true,
        }
    }

    /// The columns selected on row `line` of a grid `columns` wide, first
    /// and last included; `None` when the row has none.
    #[must_use]
    pub fn columns_on(&self, line: u32, columns: u32) -> Option<(u32, u32)> {
        let _ = (line, columns);
        None
    }
}

impl_option!(
    TerminalSelection,
    OptionTerminalSelection,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

/// How the cursor is drawn (`DECSCUSR`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalCursorShape {
    /// A filled cell, the character under it in the ground colour.
    #[default]
    Block,
    /// The outline of a cell (the view does not have the focus).
    HollowBlock,
    /// A line under the cell.
    Underline,
    /// A line before the cell.
    Bar,
    /// No cursor (`CSI ?25l`, or the cursor is scrolled out of view).
    Hidden,
}

/// Where the cursor is, in view, and how it is drawn.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TerminalCursor {
    /// The row in view.
    pub line: u32,
    /// The column.
    pub column: u32,
    /// How it is drawn.
    pub shape: TerminalCursorShape,
}

impl TerminalCursor {
    /// A `shape` cursor at `line`, `column` in view.
    #[must_use]
    pub const fn create(line: u32, column: u32, shape: TerminalCursorShape) -> Self {
        Self {
            line,
            column,
            shape,
        }
    }

    /// No cursor.
    #[must_use]
    pub const fn hidden() -> Self {
        Self::create(0, 0, TerminalCursorShape::Hidden)
    }
}

// ---- the modes the program asked for ----

/// Which pointer events the program asked to hear.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalMouseMode {
    /// None: the pointer selects.
    #[default]
    Off,
    /// Presses and releases, the wheel (`CSI ?1000h`).
    Click,
    /// [`Self::Click`] and moves with a button held (`CSI ?1002h`).
    Drag,
    /// [`Self::Click`] and every move (`CSI ?1003h`).
    Motion,
}

/// How a pointer report is written.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalMouseEncoding {
    /// `CSI M b x y`, three bytes of 32 + value (columns and rows up to 223).
    #[default]
    Default,
    /// The same with the coordinates as UTF-8 characters (`CSI ?1005h`).
    Utf8,
    /// `CSI < b ; x ; y M` / `m` (`CSI ?1006h`).
    Sgr,
}

/// The modes that change what the keys, the pointer and a paste send, as
/// the program set them - and the one the USER sets (`alt_sends_escape`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TerminalModes {
    /// The pointer events the program hears.
    pub mouse: TerminalMouseMode,
    /// How they are written.
    pub mouse_encoding: TerminalMouseEncoding,
    /// `DECCKM`: the arrows / Home / End send `SS3` (`ESC O x`).
    pub application_cursor: bool,
    /// `DECKPAM`: the keypad sends `SS3` sequences.
    pub application_keypad: bool,
    /// `CSI ?2004h`: a paste is wrapped in `CSI 200~` / `CSI 201~`.
    pub bracketed_paste: bool,
    /// `CSI ?1004h`: focus in / out are reported (`CSI I` / `CSI O`).
    pub focus_reporting: bool,
    /// `CSI ?1049h`: the alternate screen (a full-screen program).
    pub alternate_screen: bool,
    /// `CSI ?1007h`: on the alternate screen the wheel sends arrow keys.
    pub alternate_scroll: bool,
    /// The user's choice: Alt (Option) + a key sends ESC + the key (Meta),
    /// rather than the character the layout makes.
    pub alt_sends_escape: bool,
}

impl TerminalModes {
    /// A fresh terminal: no mouse reports, normal cursor keys, Alt sends
    /// escape everywhere but macOS (where Option types characters).
    #[must_use]
    pub const fn create() -> Self {
        Self {
            mouse: TerminalMouseMode::Off,
            mouse_encoding: TerminalMouseEncoding::Default,
            application_cursor: false,
            application_keypad: false,
            bracketed_paste: false,
            focus_reporting: false,
            alternate_screen: false,
            alternate_scroll: false,
            alt_sends_escape: !cfg!(target_os = "macos"),
        }
    }

    /// The bytes `key` with `modifiers` sends to the program; empty when the
    /// key sends nothing of its own (a letter, a digit: its text comes as
    /// typed text) or is the app's (anything with Cmd / the Windows key).
    #[must_use]
    pub fn encode_key(&self, key: VirtualKeyCode, modifiers: KeyModifiers) -> U8Vec {
        let _ = (key, modifiers);
        U8Vec::from_vec(Vec::new())
    }

    /// The bytes typed `text` sends: its UTF-8, the control characters left
    /// out (they come from [`Self::encode_key`]).
    #[must_use]
    pub fn encode_text(&self, text: AzString) -> U8Vec {
        let _ = text;
        U8Vec::from_vec(Vec::new())
    }

    /// The bytes a paste of `text` sends: bracketed when the program asked
    /// for it (any ESC in the text dropped, so a paste cannot end the
    /// bracket), otherwise every line break as a carriage return.
    #[must_use]
    pub fn encode_paste(&self, text: AzString) -> U8Vec {
        let _ = text;
        U8Vec::from_vec(Vec::new())
    }

    /// The report of a pointer event at `point`, or empty when the program
    /// did not ask for it (or the cell is past what the encoding can say).
    #[must_use]
    pub fn encode_mouse(
        &self,
        button: TerminalMouseButton,
        action: TerminalMouseAction,
        point: TerminalPoint,
        modifiers: KeyModifiers,
    ) -> U8Vec {
        let _ = (button, action, point, modifiers);
        U8Vec::from_vec(Vec::new())
    }

    /// `CSI I` (in) / `CSI O` (out) when the program asked for focus
    /// reports, otherwise empty.
    #[must_use]
    pub fn encode_focus(&self, focused: bool) -> U8Vec {
        let _ = focused;
        U8Vec::from_vec(Vec::new())
    }
}

/// The button of a pointer report.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalMouseButton {
    /// The primary button.
    #[default]
    Left,
    /// The wheel button.
    Middle,
    /// The secondary button.
    Right,
    /// A wheel notch away from the user.
    WheelUp,
    /// A wheel notch towards the user.
    WheelDown,
    /// No button (a move with nothing held).
    None,
}

/// What happened to the button.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalMouseAction {
    /// Pressed (a wheel notch is a press).
    #[default]
    Press,
    /// Released.
    Release,
    /// The pointer moved (with the button held, or with none).
    Motion,
}

// ---- the palette ----

/// The colours of a terminal, each by day and at night: the 16 ANSI
/// colours, the default ink and ground, the cursor and the selection.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerminalPalette {
    /// ANSI 0.
    pub black: ChartColor,
    /// ANSI 1.
    pub red: ChartColor,
    /// ANSI 2.
    pub green: ChartColor,
    /// ANSI 3.
    pub yellow: ChartColor,
    /// ANSI 4.
    pub blue: ChartColor,
    /// ANSI 5.
    pub magenta: ChartColor,
    /// ANSI 6.
    pub cyan: ChartColor,
    /// ANSI 7.
    pub white: ChartColor,
    /// ANSI 8.
    pub bright_black: ChartColor,
    /// ANSI 9.
    pub bright_red: ChartColor,
    /// ANSI 10.
    pub bright_green: ChartColor,
    /// ANSI 11.
    pub bright_yellow: ChartColor,
    /// ANSI 12.
    pub bright_blue: ChartColor,
    /// ANSI 13.
    pub bright_magenta: ChartColor,
    /// ANSI 14.
    pub bright_cyan: ChartColor,
    /// ANSI 15.
    pub bright_white: ChartColor,
    /// The default ink.
    pub foreground: ChartColor,
    /// The default ground (the view's surface).
    pub background: ChartColor,
    /// The cursor.
    pub cursor: ChartColor,
    /// The selection's wash (drawn over the text: give it some alpha).
    pub selection: ChartColor,
}

impl TerminalPalette {
    /// ANSI colour `index` (0..15; past 15 the last bright one).
    #[must_use]
    pub const fn ansi(&self, index: u8) -> ChartColor {
        let _ = index;
        self.foreground
    }

    /// The colour `color` is drawn in (`foreground` for the default ink,
    /// `background` for the default ground).
    #[must_use]
    pub fn color_of(&self, color: TerminalColor) -> ChartColor {
        let _ = color;
        self.foreground
    }

    /// The ink and the ground `style` is drawn in - inverse, hidden and dim
    /// applied - and whether the ground must be painted (`false`: it is the
    /// view's own surface).
    #[must_use]
    pub fn colors_of(&self, style: &TerminalStyle) -> TerminalStyleColors {
        let _ = style;
        TerminalStyleColors {
            ink: self.foreground,
            ground: self.background,
            paints_ground: false,
        }
    }
}

/// The colours of a style: what [`TerminalPalette::colors_of`] resolves.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerminalStyleColors {
    /// The text colour.
    pub ink: ChartColor,
    /// The cell colour.
    pub ground: ChartColor,
    /// The ground differs from the view's surface and must be painted.
    pub paints_ground: bool,
}

impl_option!(
    TerminalPalette,
    OptionTerminalPalette,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

/// The colour of palette index `index` past the 16 ANSI colours: 16..231
/// the 6x6x6 cube (levels 0, 95, 135, 175, 215, 255), 232..255 the grey
/// ramp (8 to 238 in steps of 10). `None` below 16.
#[must_use]
pub const fn xterm_256_color(index: u8) -> Option<ColorU> {
    let _ = index;
    None
}

// ---- what the app answers ----

/// The grid the view has room for: whole columns and rows of its cell.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TerminalGridSize {
    /// Columns.
    pub columns: u32,
    /// Rows.
    pub rows: u32,
}

impl TerminalGridSize {
    /// `columns` x `rows`.
    #[must_use]
    pub const fn create(columns: u32, rows: u32) -> Self {
        Self { columns, rows }
    }

    /// The grid a box of `width` x `height` px holds in cells of
    /// `cell_width` x `line_height` px, at least 2 x 1 (what a terminal
    /// engine accepts); 2 x 1 for a box that is not there yet.
    #[must_use]
    pub fn fitting(width: f32, height: f32, cell_width: f32, line_height: f32) -> Self {
        let _ = (width, height, cell_width, line_height);
        Self::create(2, 1)
    }
}

/// What the data callback answers: the screen as it is now.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalScreen {
    /// The rows in view, top to bottom (as many as the grid has rows).
    pub lines: TerminalLineVec,
    /// The selection, clipped to the rows in view.
    pub selection: OptionTerminalSelection,
    /// The lines of scrollback above the screen.
    pub history: u32,
    /// How far the view is scrolled up into them (0 = at the bottom,
    /// following the output; at most `history`).
    pub scroll: u32,
    /// The cursor, in view.
    pub cursor: TerminalCursor,
    /// The modes the program set.
    pub modes: TerminalModes,
}

impl TerminalScreen {
    /// A screen of `lines`, no scrollback, the cursor hidden.
    #[must_use]
    pub fn create(lines: TerminalLineVec) -> Self {
        Self {
            lines,
            selection: OptionTerminalSelection::None,
            history: 0,
            scroll: 0,
            cursor: TerminalCursor::hidden(),
            modes: TerminalModes::create(),
        }
    }

    /// No lines at all (no data callback, or it is not there).
    #[must_use]
    pub fn empty() -> Self {
        Self::create(TerminalLineVec::from_const_slice(&[]))
    }

    /// The selected text in view: the selected cells of every row, the rows
    /// joined by a line break (none after a soft-wrapped row), the trailing
    /// blanks of a row dropped.
    #[must_use]
    pub fn selected_text(&self) -> AzString {
        AzString::from(String::new())
    }
}

impl Default for TerminalScreen {
    fn default() -> Self {
        Self::empty()
    }
}

impl azul_core::host_invoker::HostOut for TerminalScreen {
    fn unwritten() -> Self {
        Self::empty()
    }
}

// ---- what the view reports ----

/// What a selection gesture selects.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalSelectionKind {
    /// Cell by cell (a press and a drag).
    #[default]
    Simple,
    /// Whole words (a double-click).
    Word,
    /// Whole lines.
    Line,
    /// A rectangle (Alt + drag).
    Block,
}

/// What happened in the view.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerminalViewEventKind {
    /// Bytes for the program ([`TerminalViewEvent::bytes`]): a key, typed
    /// text, a paste, a pointer report, a focus report. Write them to the
    /// PTY (and scroll to the bottom).
    #[default]
    Input,
    /// Show the scrollback at display offset [`TerminalViewEvent::scroll`].
    Scroll,
    /// A selection starts at [`TerminalViewEvent::point`]
    /// ([`TerminalViewEvent::selection_kind`] says what it selects).
    SelectStart,
    /// The selection now ends at [`TerminalViewEvent::point`].
    SelectExtend,
    /// The selection gesture ended.
    SelectEnd,
    /// The selection is gone (a press without a drag).
    SelectClear,
    /// Put the selection's text on the clipboard (the app has the whole
    /// selection, scrollback included; the view only has the rows in view).
    Copy,
}

/// An action in the view: its kind and what it carries.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalViewEvent {
    /// `Input`: the bytes for the program.
    pub bytes: U8Vec,
    /// The display offset to show (`Scroll`), the current one otherwise.
    pub scroll: u32,
    /// The cell (`Select*`), in view.
    pub point: TerminalPoint,
    /// What happened.
    pub kind: TerminalViewEventKind,
    /// What a `SelectStart` selects.
    pub selection_kind: TerminalSelectionKind,
    /// `Select*`: the pointer is in the right half of the cell (a selection
    /// that starts there leaves the cell out).
    pub right_half: bool,
}

impl TerminalViewEvent {
    /// A `kind` event, nothing else set.
    #[must_use]
    pub fn create(kind: TerminalViewEventKind) -> Self {
        Self {
            bytes: U8Vec::from_vec(Vec::new()),
            scroll: 0,
            point: TerminalPoint::create(0, 0),
            kind,
            selection_kind: TerminalSelectionKind::Simple,
            right_half: false,
        }
    }

    /// An `Input` event of `bytes`.
    #[must_use]
    pub fn input(bytes: U8Vec) -> Self {
        let mut e = Self::create(TerminalViewEventKind::Input);
        e.bytes = bytes;
        e
    }

    /// A `Scroll` event to display offset `scroll`.
    #[must_use]
    pub fn scrolled(scroll: u32) -> Self {
        let mut e = Self::create(TerminalViewEventKind::Scroll);
        e.scroll = scroll;
        e
    }
}

// ---- callbacks ----

/// Callback invoked for an action in the view.
pub type TerminalViewOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TerminalViewEvent) -> Update;
impl_widget_callback!(
    TerminalViewOnEvent,
    OptionTerminalViewOnEvent,
    TerminalViewOnEventCallback,
    TerminalViewOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TerminalViewOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TERMINAL_VIEW_ON_EVENT_INVOKER,
    invoker_ty:     AzTerminalViewOnEventCallbackInvoker,
    thunk_fn:       az_terminal_view_on_event_callback_thunk,
    setter_fn:      AzApp_setTerminalViewOnEventCallbackInvoker,
    from_handle_fn: AzTerminalViewOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzTerminalViewOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: TerminalViewEvent ],
}

/// The DATA callback: the screen, for the grid the view has room for.
pub type TerminalViewDataSourceCallbackType =
    extern "C" fn(RefAny, TerminalGridSize) -> TerminalScreen;
impl_widget_callback!(
    TerminalViewDataSource,
    OptionTerminalViewDataSource,
    TerminalViewDataSourceCallback,
    TerminalViewDataSourceCallbackType
);

// Host-invoker plumbing: the grid size carries no context, so the thunk
// reads it from the invocation slot.
azul_core::impl_managed_callback! {
    wrapper:        TerminalViewDataSourceCallback,
    ctx_field:      ctx,
    data:           data: RefAny,
    args:           [size: TerminalGridSize],
    return_ty:      TerminalScreen,
    default_ret:    TerminalScreen::empty(),
    invoker_static: TERMINAL_VIEW_DATA_SOURCE_INVOKER,
    invoker_ty:     AzTerminalViewDataSourceCallbackInvoker,
    thunk_fn:       az_terminal_view_data_source_callback_thunk,
    setter_fn:      AzApp_setTerminalViewDataSourceCallbackInvoker,
    from_handle_fn: AzTerminalViewDataSourceCallback_createFromHostHandle,
    from_handle_byref_fn: AzTerminalViewDataSourceCallback_createFromHostHandleByref,
}

/// The screen from the data callback (empty without one).
pub(crate) fn screen_of(
    source: &OptionTerminalViewDataSource,
    size: TerminalGridSize,
) -> TerminalScreen {
    match source.as_ref() {
        Some(TerminalViewDataSource { refany, callback }) => callback.invoke(refany.clone(), size),
        None => TerminalScreen::empty(),
    }
}

// ---- the widget ----

/// The font size a view starts with, px.
pub const TERMINAL_FONT_SIZE: f32 = 13.0;
/// The lines one wheel notch scrolls.
pub const TERMINAL_WHEEL_LINES: u32 = 3;
/// A line's height over the font size (when the app gives none).
pub(crate) const LINE_HEIGHT_EM: f32 = 1.3;
/// A cell's width over the font size, until the face is measured (every
/// common monospace face is 0.55 - 0.62 em wide).
pub(crate) const CELL_WIDTH_EM: f32 = 0.6;
/// The scroll bar's width, px - always kept free on the right, so the grid
/// does not lose a column (and the program a resize) when the first line
/// scrolls off.
pub(crate) const SCROLLBAR_PX: f32 = crate::widgets::data_table::SCROLLBAR_PX;

/// The terminal view. See the module documentation.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct TerminalView {
    /// Where the screen comes from.
    pub data_source: OptionTerminalViewDataSource,
    /// Who hears the actions.
    pub on_event: OptionTerminalViewOnEvent,
    /// The name a screen reader says ("Terminal" when empty).
    pub accessibility_name: AzString,
    /// The view's DOM id ("" = none).
    pub id: AzString,
    /// The font size, px.
    pub font_size: f32,
    /// A row's height, px (0: 1.3 x the font size).
    pub line_height: f32,
    /// The colours; `None`: the app theme's ([`TerminalPalette::flat`] /
    /// [`TerminalPalette::flora_ink`]).
    pub palette: OptionTerminalPalette,
}

impl Default for TerminalView {
    fn default() -> Self {
        Self::create()
    }
}

impl TerminalView {
    /// A view at 13 px with the app theme's colours, no data until
    /// [`Self::with_data_source`].
    #[must_use]
    pub fn create() -> Self {
        Self {
            data_source: OptionTerminalViewDataSource::None,
            on_event: OptionTerminalViewOnEvent::None,
            accessibility_name: AzString::from_const_str(""),
            id: AzString::from_const_str(""),
            font_size: TERMINAL_FONT_SIZE,
            line_height: 0.0,
            palette: OptionTerminalPalette::None,
        }
    }

    /// Where the screen comes from: `callback(data, grid)` whenever the view
    /// renders - the grid is the columns x rows it has room for.
    pub fn set_data_source<C: Into<TerminalViewDataSourceCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.data_source = Some(TerminalViewDataSource {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_data_source`] for the builder chain.
    #[must_use]
    pub fn with_data_source<C: Into<TerminalViewDataSourceCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_data_source(data, callback);
        self
    }

    /// The callback that hears every action.
    pub fn set_on_event<C: Into<TerminalViewOnEventCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_event = Some(TerminalViewOnEvent {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<TerminalViewOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// The colours (a user's colour scheme).
    pub const fn set_palette(&mut self, palette: TerminalPalette) {
        self.palette = OptionTerminalPalette::Some(palette);
    }

    /// [`Self::set_palette`] for the builder chain.
    #[must_use]
    pub const fn with_palette(mut self, palette: TerminalPalette) -> Self {
        self.set_palette(palette);
        self
    }

    /// The font size, px.
    pub const fn set_font_size(&mut self, px: f32) {
        self.font_size = px;
    }

    /// [`Self::set_font_size`] for the builder chain.
    #[must_use]
    pub const fn with_font_size(mut self, px: f32) -> Self {
        self.set_font_size(px);
        self
    }

    /// A row's height, px (0: 1.3 x the font size).
    pub const fn set_line_height(&mut self, px: f32) {
        self.line_height = px;
    }

    /// [`Self::set_line_height`] for the builder chain.
    #[must_use]
    pub const fn with_line_height(mut self, px: f32) -> Self {
        self.set_line_height(px);
        self
    }

    /// The name a screen reader says.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// The view's DOM id.
    pub fn set_id(&mut self, id: AzString) {
        self.id = id;
    }

    /// [`Self::set_id`] for the builder chain.
    #[must_use]
    pub fn with_id(mut self, id: AzString) -> Self {
        self.set_id(id);
        self
    }

    /// Replaces `self` with a fresh view and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }
}

// ---- the geometry (pure) ----

/// The size of a cell, px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    /// The font size.
    pub font_size: f32,
    /// A cell's width (the face's advance).
    pub cell_width: f32,
    /// A row's height.
    pub line_height: f32,
}

impl Metrics {
    /// The cell of a `font_size` px face: `line_height` (0: 1.3 em) tall,
    /// `measured_width` (the face's advance; `None` or nonsense: 0.6 em)
    /// wide. A font size that is not a positive number is 13 px.
    pub(crate) fn of(font_size: f32, line_height: f32, measured_width: Option<f32>) -> Self {
        let _ = (font_size, line_height, measured_width);
        Self {
            font_size: TERMINAL_FONT_SIZE,
            cell_width: 1.0,
            line_height: 1.0,
        }
    }

    /// The cell under `(x, y)` px from the text area's top-left in a grid of
    /// `grid` (clamped into it), and whether `x` is in its right half.
    pub(crate) fn cell_at(&self, grid: TerminalGridSize, x: f32, y: f32) -> (TerminalPoint, bool) {
        let _ = (grid, x, y);
        (TerminalPoint::create(0, 0), false)
    }
}

/// The display offset `lines` lines further up (positive: older lines; negative: back
/// towards the output), kept within `0..=history`.
pub(crate) fn scroll_after(scroll: u32, history: u32, lines: i64) -> u32 {
    let _ = (history, lines);
    scroll
}

/// The scroll bar of a view `height` px tall at `x`: the thumb shows
/// `rows` of `history + rows` lines, scrolled up by `scroll`. `None` without
/// scrollback (nothing to scroll).
pub(crate) fn scroll_bar(
    x: f32,
    height: f32,
    rows: u32,
    history: u32,
    scroll: u32,
) -> Option<crate::widgets::data_table::ScrollBar> {
    let _ = (x, height, rows, history, scroll);
    None
}

/// The display offset that puts the thumb of `bar` with its top at `thumb_top` px
/// along the track: the top of the track is the oldest line (`history`), the
/// bottom the output (0).
pub(crate) fn scroll_for_thumb(
    bar: &crate::widgets::data_table::ScrollBar,
    thumb_top: f32,
    history: u32,
) -> u32 {
    let _ = (bar, thumb_top, history);
    0
}

/// What a key does in the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyAction {
    /// The selection to the clipboard (the app's `Copy`).
    Copy,
    /// The clipboard's text to the program (a paste).
    Paste,
    /// Show this display offset.
    Scroll(u32),
    /// These bytes to the program.
    Bytes(Vec<u8>),
    /// Not the view's key.
    Nothing,
}

/// What `key` with `modifiers` does: the copy / paste chords of the
/// platform (`mac`: Cmd+C / Cmd+V; else Ctrl+Shift+C / Ctrl+Shift+V;
/// Shift+Insert pastes everywhere), Shift+Page Up / Page Down / Home / End
/// scroll the scrollback (not on the alternate screen: there they are the
/// program's), everything else [`TerminalModes::encode_key`].
pub(crate) fn key_action(
    screen: &TerminalScreen,
    rows: u32,
    key: VirtualKeyCode,
    modifiers: KeyModifiers,
    mac: bool,
) -> KeyAction {
    let _ = (screen, rows, key, modifiers, mac);
    KeyAction::Nothing
}

/// What the wheel does, `notches` notches (positive: towards the user,
/// down to newer lines) over `point`: reported to a program that asked for
/// pointer reports, arrow keys on the alternate screen with alternate scroll,
/// otherwise [`TERMINAL_WHEEL_LINES`] lines of scrollback a notch.
pub(crate) fn wheel_action(
    screen: &TerminalScreen,
    notches: i64,
    point: TerminalPoint,
    modifiers: KeyModifiers,
) -> KeyAction {
    let _ = (screen, notches, point, modifiers);
    KeyAction::Nothing
}

// TERM9-NEXT: the build, the handlers.
