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
    pub(crate) fn columns_on(&self, line: u32, columns: u32) -> Option<(u32, u32)> {
        if columns == 0 || line < self.start.line || line > self.end.line {
            return None;
        }
        let last_column = columns - 1;
        let (first, last) = if self.block {
            (self.start.column, self.end.column)
        } else {
            (
                if line == self.start.line {
                    self.start.column
                } else {
                    0
                },
                if line == self.end.line {
                    self.end.column
                } else {
                    last_column
                },
            )
        };
        let last = last.min(last_column);
        if first > last {
            None
        } else {
            Some((first, last))
        }
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
        U8Vec::from_vec(key_bytes(self, key, modifiers).unwrap_or_default())
    }

    /// The bytes typed `text` sends: its UTF-8, the control characters left
    /// out (they come from [`Self::encode_key`]).
    #[must_use]
    pub fn encode_text(&self, text: AzString) -> U8Vec {
        let typed: String = text.as_str().chars().filter(|c| !c.is_control()).collect();
        U8Vec::from_vec(typed.into_bytes())
    }

    /// The bytes a paste of `text` sends: bracketed when the program asked
    /// for it (any ESC in the text dropped, so a paste cannot end the
    /// bracket), otherwise every line break as a carriage return.
    #[must_use]
    pub fn encode_paste(&self, text: AzString) -> U8Vec {
        let text = text.as_str();
        let bytes = if self.bracketed_paste {
            let mut out = Vec::with_capacity(text.len() + 12);
            out.extend_from_slice(b"\x1b[200~");
            out.extend(text.bytes().filter(|b| *b != 0x1b));
            out.extend_from_slice(b"\x1b[201~");
            out
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
        };
        U8Vec::from_vec(bytes)
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
        U8Vec::from_vec(mouse_bytes(self, button, action, point, modifiers).unwrap_or_default())
    }

    /// `CSI I` (in) / `CSI O` (out) when the program asked for focus
    /// reports, otherwise empty.
    #[must_use]
    pub fn encode_focus(&self, focused: bool) -> U8Vec {
        let bytes: &[u8] = match (self.focus_reporting, focused) {
            (false, _) => b"",
            (true, true) => b"\x1b[I",
            (true, false) => b"\x1b[O",
        };
        U8Vec::from_vec(bytes.to_vec())
    }
}

/// xterm's modifier parameter: 1, plus 1 for Shift, 2 for Alt, 4 for Ctrl.
const fn modifier_param(m: KeyModifiers) -> u8 {
    1 + (m.shift as u8) + 2 * (m.alt as u8) + 4 * (m.ctrl as u8)
}

/// `ESC [ <final>` / `ESC O <final>` without modifiers (`ss3`: the SS3
/// form), `ESC [ 1 ; <m> <final>` with them.
fn csi_or_ss3(param: u8, ss3: bool, final_byte: u8) -> Vec<u8> {
    if param == 1 {
        alloc::vec![0x1b, if ss3 { b'O' } else { b'[' }, final_byte]
    } else {
        alloc::format!("\x1b[1;{param}{}", char::from(final_byte)).into_bytes()
    }
}

/// `ESC [ <code> ~`, `ESC [ <code> ; <m> ~` with modifiers.
fn tilde(code: u8, param: u8) -> Vec<u8> {
    if param == 1 {
        alloc::format!("\x1b[{code}~").into_bytes()
    } else {
        alloc::format!("\x1b[{code};{param}~").into_bytes()
    }
}

/// The C0 control Ctrl + `key` makes (Ctrl+A = 0x01 ... Ctrl+[ = ESC).
fn ctrl_byte(key: VirtualKeyCode) -> Option<u8> {
    use VirtualKeyCode as K;
    let index = key as u32;
    // A..Z are 10..=35 (`VirtualKeyCode::from_u32`).
    if (10..=35).contains(&index) {
        return u8::try_from(index - 10 + 1).ok();
    }
    match key {
        K::Space | K::Key2 => Some(0x00),
        K::LBracket | K::Key3 => Some(0x1b),
        K::Backslash | K::Key4 => Some(0x1c),
        K::RBracket | K::Key5 => Some(0x1d),
        K::Key6 => Some(0x1e),
        K::Minus | K::Slash | K::Key7 => Some(0x1f),
        K::Key8 => Some(0x7f),
        _ => None,
    }
}

/// The character `key` types on a US layout (`shift`: a capital letter;
/// a shifted digit or sign is the layout's, so `None`).
fn us_char(key: VirtualKeyCode, shift: bool) -> Option<u8> {
    use VirtualKeyCode as K;
    let index = key as u32;
    if (10..=35).contains(&index) {
        let base = if shift { b'A' } else { b'a' };
        return u8::try_from(index - 10).ok().map(|i| base + i);
    }
    if shift {
        return None;
    }
    // Key1..Key9 are 0..=8, Key0 is 9.
    if index <= 8 {
        return u8::try_from(index).ok().map(|i| b'1' + i);
    }
    Some(match key {
        K::Key0 => b'0',
        K::Space => b' ',
        K::Period => b'.',
        K::Comma => b',',
        K::Minus => b'-',
        K::Equals => b'=',
        K::Slash => b'/',
        K::Backslash => b'\\',
        K::Semicolon => b';',
        K::Apostrophe => b'\'',
        K::Grave => b'`',
        K::LBracket => b'[',
        K::RBracket => b']',
        _ => return None,
    })
}

/// The bytes of [`TerminalModes::encode_key`]; `None` for a key that sends
/// nothing of its own.
fn key_bytes(t: &TerminalModes, key: VirtualKeyCode, m: KeyModifiers) -> Option<Vec<u8>> {
    use VirtualKeyCode as K;
    if m.meta {
        return None;
    }
    let param = modifier_param(m);
    let cursor = match key {
        K::Up => Some(b'A'),
        K::Down => Some(b'B'),
        K::Right => Some(b'C'),
        K::Left => Some(b'D'),
        K::Home => Some(b'H'),
        K::End => Some(b'F'),
        _ => None,
    };
    if let Some(final_byte) = cursor {
        return Some(csi_or_ss3(param, t.application_cursor, final_byte));
    }
    let function = match key {
        K::F1 => Some(b'P'),
        K::F2 => Some(b'Q'),
        K::F3 => Some(b'R'),
        K::F4 => Some(b'S'),
        _ => None,
    };
    if let Some(final_byte) = function {
        return Some(csi_or_ss3(param, true, final_byte));
    }
    let code = match key {
        K::Insert => Some(2),
        K::Delete => Some(3),
        K::PageUp => Some(5),
        K::PageDown => Some(6),
        K::F5 => Some(15),
        K::F6 => Some(17),
        K::F7 => Some(18),
        K::F8 => Some(19),
        K::F9 => Some(20),
        K::F10 => Some(21),
        K::F11 => Some(23),
        K::F12 => Some(24),
        K::F13 => Some(25),
        K::F14 => Some(26),
        K::F15 => Some(28),
        K::F16 => Some(29),
        K::F17 => Some(31),
        K::F18 => Some(32),
        K::F19 => Some(33),
        K::F20 => Some(34),
        _ => None,
    };
    if let Some(code) = code {
        return Some(tilde(code, param));
    }
    if t.application_keypad && param == 1 {
        let ss3 = match key {
            K::Numpad0 => b'p',
            K::Numpad1 => b'q',
            K::Numpad2 => b'r',
            K::Numpad3 => b's',
            K::Numpad4 => b't',
            K::Numpad5 => b'u',
            K::Numpad6 => b'v',
            K::Numpad7 => b'w',
            K::Numpad8 => b'x',
            K::Numpad9 => b'y',
            K::NumpadDecimal => b'n',
            K::NumpadAdd => b'k',
            K::NumpadSubtract => b'm',
            K::NumpadMultiply => b'j',
            K::NumpadDivide => b'o',
            K::NumpadEnter => b'M',
            K::NumpadEquals => b'X',
            _ => 0,
        };
        if ss3 != 0 {
            return Some(alloc::vec![0x1b, b'O', ss3]);
        }
    }
    let meta = m.alt && t.alt_sends_escape;
    let with_meta = |byte: u8| -> Vec<u8> {
        if meta {
            alloc::vec![0x1b, byte]
        } else {
            alloc::vec![byte]
        }
    };
    match key {
        K::Return | K::NumpadEnter => return Some(with_meta(b'\r')),
        K::Tab if m.shift => return Some(b"\x1b[Z".to_vec()),
        K::Tab => return Some(with_meta(b'\t')),
        K::Back => return Some(with_meta(if m.ctrl { 0x08 } else { 0x7f })),
        K::Escape => return Some(with_meta(0x1b)),
        _ => {}
    }
    // Ctrl+Alt is AltGr on Windows: the layout types its character.
    if m.ctrl && m.alt {
        return None;
    }
    if m.ctrl {
        return ctrl_byte(key).map(|c| alloc::vec![c]);
    }
    if meta {
        return us_char(key, m.shift).map(|c| alloc::vec![0x1b, c]);
    }
    None
}

/// The bytes of [`TerminalModes::encode_mouse`]; `None` when nothing is
/// reported.
fn mouse_bytes(
    t: &TerminalModes,
    button: TerminalMouseButton,
    action: TerminalMouseAction,
    point: TerminalPoint,
    m: KeyModifiers,
) -> Option<Vec<u8>> {
    use TerminalMouseAction as A;
    use TerminalMouseButton as B;
    match (t.mouse, action) {
        (TerminalMouseMode::Off, _) | (TerminalMouseMode::Click, A::Motion) => return None,
        (TerminalMouseMode::Drag, A::Motion) if button == B::None => return None,
        _ => {}
    }
    let wheel = matches!(button, B::WheelUp | B::WheelDown);
    if wheel && action != A::Press {
        return None;
    }
    let base: u32 = match button {
        B::Left => 0,
        B::Middle => 1,
        B::Right => 2,
        B::None => 3,
        B::WheelUp => 64,
        B::WheelDown => 65,
    };
    let mods = 4 * u32::from(m.shift) + 8 * u32::from(m.alt) + 16 * u32::from(m.ctrl);
    let motion = if action == A::Motion { 32 } else { 0 };
    let x = point.column.saturating_add(1);
    let y = point.line.saturating_add(1);
    if t.mouse_encoding == TerminalMouseEncoding::Sgr {
        let code = base + mods + motion;
        let final_char = if action == A::Release { 'm' } else { 'M' };
        return Some(alloc::format!("\x1b[<{code};{x};{y}{final_char}").into_bytes());
    }
    // X10 / normal: a release says button 3, every value is 32 + it.
    let pressed = if action == A::Release { 3 } else { base };
    let code = pressed + mods + motion;
    let mut out = alloc::vec![0x1b, b'[', b'M'];
    for value in [code, x, y] {
        let v = value.checked_add(32)?;
        if t.mouse_encoding == TerminalMouseEncoding::Utf8 {
            // xterm's 1005 limit: 2047 as a two-byte character.
            if v > 2047 {
                return None;
            }
            let c = char::from_u32(v)?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        } else {
            out.push(u8::try_from(v).ok()?);
        }
    }
    Some(out)
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
    /// Every colour `c` (a starting point for a palette built in code).
    #[must_use]
    pub const fn uniform(c: ChartColor) -> Self {
        Self {
            black: c,
            red: c,
            green: c,
            yellow: c,
            blue: c,
            magenta: c,
            cyan: c,
            white: c,
            bright_black: c,
            bright_red: c,
            bright_green: c,
            bright_yellow: c,
            bright_blue: c,
            bright_magenta: c,
            bright_cyan: c,
            bright_white: c,
            foreground: c,
            background: c,
            cursor: c,
            selection: c,
        }
    }

    /// Flat's terminal: dark text on the page white by day, the desktop's
    /// night console at night (Windows Terminal's Campbell colours) -
    /// `themes::flat::terminal_palette`.
    #[must_use]
    pub const fn flat() -> Self {
        crate::widgets::themes::flat::terminal_palette()
    }

    /// Flora's "ink" terminal: the code panel's warm ink ground in both
    /// modes (`--fl-code-bg #211F1B`, `--fl-code-fg #E4E1D6`; the dark
    /// room `#141414` / `#E2E2E2` at night), the ANSI colours from Flora's
    /// accent families - `themes::flora::terminal_palette`.
    #[must_use]
    pub const fn flora_ink() -> Self {
        crate::widgets::themes::flora::terminal_palette()
    }

    /// The palette of the app theme the DOM is built for (flat or flora).
    #[must_use]
    pub fn of_app_theme() -> Self {
        match crate::widgets::themes::UiTheme::current() {
            crate::widgets::themes::UiTheme::Flat => Self::flat(),
            crate::widgets::themes::UiTheme::Flora => Self::flora_ink(),
        }
    }

    /// ANSI colour `index` (0..15; past 15 the last bright one).
    #[must_use]
    pub const fn ansi(&self, index: u8) -> ChartColor {
        match index {
            0 => self.black,
            1 => self.red,
            2 => self.green,
            3 => self.yellow,
            4 => self.blue,
            5 => self.magenta,
            6 => self.cyan,
            7 => self.white,
            8 => self.bright_black,
            9 => self.bright_red,
            10 => self.bright_green,
            11 => self.bright_yellow,
            12 => self.bright_blue,
            13 => self.bright_magenta,
            14 => self.bright_cyan,
            _ => self.bright_white,
        }
    }

    /// The colour `color` is drawn in (`foreground` for the default ink,
    /// `background` for the default ground).
    #[must_use]
    pub const fn color_of(&self, color: TerminalColor) -> ChartColor {
        match color {
            TerminalColor::Foreground => self.foreground,
            TerminalColor::Background => self.background,
            TerminalColor::Indexed(i) => match xterm_256_color(i) {
                Some(c) => ChartColor::same(c),
                None => self.ansi(i),
            },
            TerminalColor::Rgb(c) => ChartColor::same(c),
        }
    }

    /// The ink and the ground `style` is drawn in - inverse, hidden and dim
    /// applied - and whether the ground must be painted (`false`: it is the
    /// view's own surface).
    #[must_use]
    pub(crate) fn colors_of(&self, style: &TerminalStyle) -> TerminalStyleColors {
        let mut ink = self.color_of(style.fg);
        let mut ground = self.color_of(style.bg);
        let mut paints_ground = style.bg != TerminalColor::Background;
        if style.inverse {
            core::mem::swap(&mut ink, &mut ground);
            paints_ground = true;
        }
        if style.hidden {
            ink = ground;
        } else if style.dim {
            ink = ChartColor::create(
                ColorU {
                    a: DIM_ALPHA,
                    ..ink.light
                },
                ColorU {
                    a: DIM_ALPHA,
                    ..ink.dark
                },
            );
        }
        TerminalStyleColors {
            ink,
            ground,
            paints_ground,
        }
    }
}

/// The alpha of dim (`SGR 2`) text: two thirds.
const DIM_ALPHA: u8 = 170;

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
    /// A cube step's level: 0, then 95 + 40 a step.
    const fn level(step: u8) -> u8 {
        if step == 0 {
            0
        } else {
            55 + 40 * step
        }
    }
    if index < 16 {
        None
    } else if index < 232 {
        let n = index - 16;
        Some(ColorU::rgb(level(n / 36), level((n / 6) % 6), level(n % 6)))
    } else {
        let grey = 8 + 10 * (index - 232);
        Some(ColorU::rgb(grey, grey, grey))
    }
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // floored, saturating
    pub fn fitting(width: f32, height: f32, cell_width: f32, line_height: f32) -> Self {
        let fit = |len: f32, cell: f32, least: u32| -> u32 {
            if len.is_finite() && cell.is_finite() && len > 0.0 && cell > 0.0 {
                ((len / cell).floor() as u32).max(least)
            } else {
                least
            }
        };
        Self::create(fit(width, cell_width, 2), fit(height, line_height, 1))
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
        let Some(selection) = self.selection.into_option() else {
            return AzString::from_const_str("");
        };
        let mut out = String::new();
        let mut joined_to_previous = true;
        for (i, line) in self.lines.as_slice().iter().enumerate() {
            let row = u32::try_from(i).unwrap_or(u32::MAX);
            let Some((first, last)) = selection.columns_on(row, u32::MAX) else {
                continue;
            };
            if !joined_to_previous {
                out.push('\n');
            }
            let text = cells_text(line, first, last);
            // A soft-wrapped row runs on into the next one: its blanks are
            // the text's, not the end of a line.
            let runs_on = line.wrapped && !selection.block && row < selection.end.line;
            if runs_on {
                out.push_str(&text);
            } else {
                out.push_str(text.trim_end_matches(' '));
            }
            joined_to_previous = runs_on;
        }
        AzString::from(out)
    }
}

/// The characters of `line` in columns `first..=last`: a run's characters
/// cover `columns / chars` columns each (1, or 2 for wide characters - the
/// app keeps the two apart in their own runs); a character counts when any
/// of its columns is selected.
fn cells_text(line: &TerminalLine, first: u32, last: u32) -> String {
    let mut out = String::new();
    let mut column = 0u32;
    for run in line.runs.as_slice() {
        let chars = u32::try_from(run.text.as_str().chars().count()).unwrap_or(u32::MAX);
        let width = if chars > 0 && run.columns >= chars.saturating_mul(2) {
            2
        } else {
            1
        };
        for c in run.text.as_str().chars() {
            let end = column.saturating_add(width - 1);
            if column <= last && end >= first {
                out.push(c);
            }
            column = column.saturating_add(width);
        }
    }
    out
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
        let font_size = if font_size.is_finite() && font_size > 0.0 {
            font_size
        } else {
            TERMINAL_FONT_SIZE
        };
        let line_height = if line_height.is_finite() && line_height > 0.0 {
            line_height
        } else {
            (font_size * LINE_HEIGHT_EM).round().max(1.0)
        };
        let cell_width = match measured_width {
            Some(w) if w.is_finite() && w > 0.0 => w,
            _ => font_size * CELL_WIDTH_EM,
        };
        Self {
            font_size,
            cell_width,
            line_height,
        }
    }

    /// The cell under `(x, y)` px from the text area's top-left in a grid of
    /// `grid` (clamped into it), and whether `x` is in its right half.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // floored, clamped
    pub(crate) fn cell_at(&self, grid: TerminalGridSize, x: f32, y: f32) -> (TerminalPoint, bool) {
        let last_column = grid.columns.max(1) - 1;
        let last_line = grid.rows.max(1) - 1;
        let fx = if x.is_finite() {
            x.max(0.0) / self.cell_width
        } else {
            0.0
        };
        let fy = if y.is_finite() {
            y.max(0.0) / self.line_height
        } else {
            0.0
        };
        let column = fx.floor() as u32;
        let line = (fy.floor() as u32).min(last_line);
        if column > last_column {
            (TerminalPoint::create(line, last_column), true)
        } else {
            (TerminalPoint::create(line, column), fx - fx.floor() >= 0.5)
        }
    }
}

/// The display offset `lines` lines further up (positive: older lines;
/// negative: back towards the output), kept within `0..=history`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to a u32
pub(crate) fn scroll_after(scroll: u32, history: u32, lines: i64) -> u32 {
    i64::from(scroll)
        .saturating_add(lines)
        .clamp(0, i64::from(history)) as u32
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
    if history == 0 || !height.is_finite() || height <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // line counts far below 2^24 per px
    let (page, total) = (rows as f32, history as f32 + rows as f32);
    let top = history - scroll.min(history);
    let (thumb_start, thumb_len) =
        crate::widgets::data_table::thumb(height, page, total, top, history);
    Some(crate::widgets::data_table::ScrollBar {
        track: (x, 0.0, SCROLLBAR_PX, height),
        thumb_start,
        thumb_len,
    })
}

/// The display offset that puts the thumb of `bar` with its top at
/// `thumb_top` px along the track: the top of the track is the oldest line
/// (`history`), the bottom the output (0).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // a fraction of the scrollback, clamped
pub(crate) fn scroll_for_thumb(
    bar: &crate::widgets::data_table::ScrollBar,
    thumb_top: f32,
    history: u32,
) -> u32 {
    let room = bar.track.3 - bar.thumb_len;
    if history == 0 || !room.is_finite() || room <= 0.0 {
        return 0;
    }
    let fraction = if thumb_top.is_finite() {
        (thumb_top / room).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let top = ((fraction * history as f32).round() as u32).min(history);
    history - top
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
    use VirtualKeyCode as K;
    let m = modifiers;
    let chord = if mac {
        m.meta && !m.ctrl && !m.alt
    } else {
        m.ctrl && m.shift && !m.alt && !m.meta
    };
    if chord {
        match key {
            K::C => return KeyAction::Copy,
            K::V => return KeyAction::Paste,
            _ => {}
        }
        // Off macOS Ctrl+Shift+letter is the window's (new tab, close
        // tab, find), as in every Linux terminal; A..Z are 10..=35.
        if !mac && (10..=35).contains(&(key as u32)) {
            return KeyAction::Nothing;
        }
    }
    let shift_only = m.shift && !m.ctrl && !m.alt && !m.meta;
    if shift_only && key == K::Insert {
        return KeyAction::Paste;
    }
    if shift_only && !screen.modes.alternate_screen {
        let page = i64::from(rows.saturating_sub(1).max(1));
        match key {
            K::PageUp => {
                return KeyAction::Scroll(scroll_after(screen.scroll, screen.history, page))
            }
            K::PageDown => {
                return KeyAction::Scroll(scroll_after(screen.scroll, screen.history, -page))
            }
            K::Home => return KeyAction::Scroll(screen.history),
            K::End => return KeyAction::Scroll(0),
            _ => {}
        }
    }
    let bytes = screen.modes.encode_key(key, m);
    if bytes.is_empty() {
        KeyAction::Nothing
    } else {
        KeyAction::Bytes(bytes.as_slice().to_vec())
    }
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
    if notches == 0 {
        return KeyAction::Nothing;
    }
    let t = &screen.modes;
    // A burst of momentum must not send thousands of reports.
    let count = usize::try_from(notches.unsigned_abs().min(64)).unwrap_or(64);
    let up = notches < 0;
    if t.mouse != TerminalMouseMode::Off {
        let button = if up {
            TerminalMouseButton::WheelUp
        } else {
            TerminalMouseButton::WheelDown
        };
        let one = t.encode_mouse(button, TerminalMouseAction::Press, point, modifiers);
        if one.is_empty() {
            return KeyAction::Nothing;
        }
        return KeyAction::Bytes(one.as_slice().repeat(count));
    }
    if t.alternate_screen {
        if !t.alternate_scroll {
            return KeyAction::Nothing;
        }
        let arrow: &[u8] = match (up, t.application_cursor) {
            (true, false) => b"\x1b[A",
            (true, true) => b"\x1bOA",
            (false, false) => b"\x1b[B",
            (false, true) => b"\x1bOB",
        };
        let lines = count * TERMINAL_WHEEL_LINES as usize;
        return KeyAction::Bytes(arrow.repeat(lines));
    }
    let lines = notches
        .saturating_mul(i64::from(TERMINAL_WHEEL_LINES))
        .saturating_neg();
    KeyAction::Scroll(scroll_after(screen.scroll, screen.history, lines))
}

// ---- the build: the rows in view, the selection, the cursor, the bar ----

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{
        CoreCallbackData, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn,
    },
    dom::{
        DatasetMergeCallback, Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, TabIndex,
    },
    events::FocusEventFilter,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::OptionRefAny,
};
use azul_css::{
    css::CssPropertyValue,
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{PixelValue, StyleFontFamily, StyleFontFamilyVec, StyleFontSize, StyleFontStyle},
        layout::LayoutPosition,
        property::CssProperty,
        style::{
            StyleCursor, StyleLineHeight, StyleTextDecoration, StyleUserSelect, StyleWhiteSpace,
        },
    },
    system::SystemFontType,
};

use crate::widgets::{
    cell_grid::{cursor_in, take_wheel},
    data_table::ScrollBar,
    themes::decl,
};

/// The view's class (the outer node also carries the app's id).
pub(crate) const TERMINAL_CLASS_NAME: &str = "__azul-terminal-view";
/// The rendered screen inside the `VirtualView`.
pub(crate) const SCREEN_CLASS_NAME: &str = "__azul-terminal-view-screen";
/// The cursor.
pub(crate) const CURSOR_CLASS_NAME: &str = "__azul-terminal-view-cursor";
/// The scroll bar's thumb.
pub(crate) const THUMB_CLASS_NAME: &str = "__azul-terminal-view-thumb";
/// The characters the face's advance is measured on.
const PROBE_TEXT: &str = "0000000000000000000000000000000000000000";
/// The alpha of the scroll bar's thumb (the ink, washed).
const THUMB_ALPHA: u8 = 0x66;

/// A gesture of the pointer in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Drag {
    /// Nothing held.
    None,
    /// Selecting: the cell the pointer was last over, and whether it left
    /// the press's cell (a press without a move clears the selection).
    Select { last: TerminalPoint, moved: bool },
    /// The scroll bar's thumb, held `grab` px below its top.
    Thumb { grab: f32 },
    /// A button held for a program that hears the pointer, last reported
    /// over `last`.
    Report {
        button: TerminalMouseButton,
        last: TerminalPoint,
    },
}

/// What the outer node's handlers and the `VirtualView`'s render share:
/// one `RefAny`, the node's dataset and the view's data.
pub(crate) struct TerminalShared {
    /// The view as the app built it (its callbacks, its font).
    pub view: TerminalView,
    /// The palette it is drawn in.
    pub palette: TerminalPalette,
    /// The face's measured advance: (the font size, the advance) px.
    pub measured: Option<(f32, f32)>,
    /// The cell, as last rendered.
    pub metrics: Metrics,
    /// The grid, as last rendered.
    pub grid: TerminalGridSize,
    /// The screen, as last rendered.
    pub screen: TerminalScreen,
    /// The scroll bar, as last rendered.
    pub bar: Option<ScrollBar>,
    /// The pointer's gesture.
    pub drag: Drag,
    /// The key just handled sent its own bytes: drop the text it types.
    pub swallow_text: bool,
}

impl TerminalShared {
    fn new(view: TerminalView, palette: TerminalPalette) -> Self {
        let metrics = Metrics::of(view.font_size, view.line_height, None);
        Self {
            view,
            palette,
            measured: None,
            metrics,
            grid: TerminalGridSize::create(0, 0),
            screen: TerminalScreen::empty(),
            bar: None,
            drag: Drag::None,
            swallow_text: false,
        }
    }
}

/// The system monospace face.
fn monospace() -> StyleFontFamilyVec {
    StyleFontFamilyVec::from_vec(alloc::vec![StyleFontFamily::SystemType(
        SystemFontType::Monospace
    )])
}

/// The face, its size, the line height, `pre`, no engine selection (the
/// view selects cells itself).
fn text_props(m: &Metrics) -> Vec<CssPropertyWithConditions> {
    alloc::vec![
        decl::simple(CssProperty::const_font_family(monospace())),
        decl::simple(CssProperty::const_font_size(StyleFontSize::px(m.font_size))),
        decl::simple(CssProperty::line_height(StyleLineHeight::Length(
            PixelValue::px(m.line_height)
        ))),
        decl::simple(CssProperty::WhiteSpace(CssPropertyValue::Exact(
            StyleWhiteSpace::Pre
        ))),
        decl::simple(CssProperty::user_select(StyleUserSelect::None)),
    ]
}

/// `color`: once when both modes agree, else the light value and its dark
/// twin.
fn push_ink(v: &mut Vec<CssPropertyWithConditions>, c: ChartColor) {
    if c.light == c.dark {
        v.push(decl::simple(decl::ink(c.light)));
    } else {
        v.extend(decl::themed_ink(c.light, c.dark));
    }
}

/// `background`: once when both modes agree, else the pair.
fn push_fill(v: &mut Vec<CssPropertyWithConditions>, c: ChartColor) {
    if c.light == c.dark {
        v.push(decl::simple(decl::fill(c.light)));
    } else {
        v.extend(decl::themed_fill(c.light, c.dark));
    }
}

/// An absolute box at `x`, `y`, `w` x `h` px.
fn place(x: f32, y: f32, w: f32, h: f32) -> Vec<CssPropertyWithConditions> {
    alloc::vec![
        decl::position(LayoutPosition::Absolute),
        decl::px_left(x),
        decl::px_top(y),
        decl::px_width(w),
        decl::px_height(h),
    ]
}

/// A div of `props` with `text` as its bare text leaf (the div is the box,
/// the label convention's second shape).
fn boxed_text(props: Vec<CssPropertyWithConditions>, text: AzString) -> Dom {
    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_vec(props))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            text,
        ))
}

/// The face's advance, measured on [`PROBE_TEXT`] (`None` where the
/// render cannot measure).
fn measured_advance(info: &VirtualViewCallbackInfo, m: &Metrics) -> Option<f32> {
    let probe = boxed_text(text_props(m), AzString::from_const_str(PROBE_TEXT));
    let size = info.measure_dom_shrink_to_fit(probe, LogicalSize::new(100_000.0, 1_000.0));
    #[allow(clippy::cast_precision_loss)] // 40
    let advance = size.width / PROBE_TEXT.len() as f32;
    (advance.is_finite() && advance > 0.0).then_some(advance)
}

/// The rows of `screen` in a grid of `grid` cells of `m`, drawn in
/// `palette` on a `size` px screen: one box per run (its ground, its ink
/// and attributes, its text), the selection's washes over them, the
/// cursor, the scroll bar's thumb.
#[allow(clippy::cast_precision_loss)] // cell counts far below 2^24
pub(crate) fn build_screen(
    screen: &TerminalScreen,
    grid: TerminalGridSize,
    m: &Metrics,
    palette: &TerminalPalette,
    size: LogicalSize,
    bar: Option<&ScrollBar>,
) -> Dom {
    let (cw, lh) = (m.cell_width, m.line_height);
    let rows = usize::try_from(grid.rows).unwrap_or(usize::MAX);
    let mut kids: Vec<Dom> = Vec::new();
    for (row, line) in screen.lines.as_slice().iter().take(rows).enumerate() {
        let y = row as f32 * lh;
        let mut column = 0u32;
        for run in line.runs.as_slice() {
            if column >= grid.columns {
                break;
            }
            let columns = run.columns.min(grid.columns - column);
            let colors = palette.colors_of(&run.style);
            let blank = run.text.as_str().trim_end_matches(' ').is_empty();
            let decorated = run.style.underline || run.style.strikethrough;
            if !blank || colors.paints_ground || decorated {
                let mut props = place(column as f32 * cw, y, columns as f32 * cw, lh);
                push_ink(&mut props, colors.ink);
                if colors.paints_ground {
                    push_fill(&mut props, colors.ground);
                }
                if run.style.bold {
                    props.push(decl::bold());
                }
                if run.style.italic {
                    props.push(decl::simple(CssProperty::font_style(
                        StyleFontStyle::Italic,
                    )));
                }
                if run.style.underline {
                    props.push(decl::simple(CssProperty::text_decoration(
                        StyleTextDecoration::Underline,
                    )));
                } else if run.style.strikethrough {
                    props.push(decl::simple(CssProperty::text_decoration(
                        StyleTextDecoration::LineThrough,
                    )));
                }
                kids.push(if blank {
                    Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(props))
                } else {
                    boxed_text(props, run.text.clone())
                });
            }
            column = column.saturating_add(run.columns);
        }
    }
    if let Some(selection) = screen.selection.into_option() {
        let shown = u32::try_from(screen.lines.len().min(rows)).unwrap_or(u32::MAX);
        for row in 0..shown {
            if let Some((first, last)) = selection.columns_on(row, grid.columns) {
                let mut props = place(
                    first as f32 * cw,
                    row as f32 * lh,
                    (last - first + 1) as f32 * cw,
                    lh,
                );
                push_fill(&mut props, palette.selection);
                kids.push(
                    Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(props)),
                );
            }
        }
    }
    if let Some(cursor) = cursor_node(screen, grid, m, palette) {
        kids.push(cursor);
    }
    if let Some(bar) = bar {
        let (x, y, w, _) = bar.track;
        let mut props = place(
            x + 3.0,
            y + bar.thumb_start,
            (w - 6.0).max(2.0),
            bar.thumb_len,
        );
        let ink = palette.foreground;
        push_fill(
            &mut props,
            ChartColor::create(
                ColorU {
                    a: THUMB_ALPHA,
                    ..ink.light
                },
                ColorU {
                    a: THUMB_ALPHA,
                    ..ink.dark
                },
            ),
        );
        props.extend(decl::radius(3));
        kids.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![IdOrClass::Class(
                    AzString::from_const_str(THUMB_CLASS_NAME)
                )]))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(props)),
        );
    }
    let mut root = alloc::vec![
        decl::position(LayoutPosition::Relative),
        decl::px_width(size.width),
        decl::px_height(size.height),
        decl::overflow_x_hidden(),
        decl::overflow_y_hidden(),
        decl::simple(CssProperty::cursor(StyleCursor::Text)),
    ];
    root.extend(text_props(m));
    push_ink(&mut root, palette.foreground);
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![IdOrClass::Class(
            AzString::from_const_str(SCREEN_CLASS_NAME)
        )]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(root))
        .with_children(kids.into())
}

/// The cursor's box: a filled cell with its character in the ground colour,
/// an outlined cell, a line under the cell or before it; `None` when it is
/// hidden or out of view.
#[allow(clippy::cast_precision_loss)] // cell counts far below 2^24
fn cursor_node(
    screen: &TerminalScreen,
    grid: TerminalGridSize,
    m: &Metrics,
    palette: &TerminalPalette,
) -> Option<Dom> {
    let c = screen.cursor;
    if c.line >= grid.rows || c.column >= grid.columns {
        return None;
    }
    let (x, y) = (
        c.column as f32 * m.cell_width,
        c.line as f32 * m.line_height,
    );
    let mut props = match c.shape {
        TerminalCursorShape::Hidden => return None,
        TerminalCursorShape::Block | TerminalCursorShape::HollowBlock => {
            place(x, y, m.cell_width, m.line_height)
        }
        TerminalCursorShape::Underline => place(x, y + m.line_height - 2.0, m.cell_width, 2.0),
        TerminalCursorShape::Bar => place(x, y, 2.0, m.line_height),
    };
    let class = IdOrClassVec::from_vec(alloc::vec![IdOrClass::Class(AzString::from_const_str(
        CURSOR_CLASS_NAME
    ))]);
    if c.shape == TerminalCursorShape::HollowBlock {
        props.extend(decl::border(1));
        props.extend(decl::themed_border_color(
            palette.cursor.light,
            palette.cursor.dark,
        ));
        return Some(
            Dom::create_div()
                .with_ids_and_classes(class)
                .with_css_props(CssPropertyWithConditionsVec::from_vec(props)),
        );
    }
    push_fill(&mut props, palette.cursor);
    let under = if c.shape == TerminalCursorShape::Block {
        screen
            .lines
            .as_slice()
            .get(usize::try_from(c.line).unwrap_or(usize::MAX))
            .map(|line| cells_text(line, c.column, c.column))
            .filter(|t| !t.trim().is_empty())
    } else {
        None
    };
    Some(match under {
        Some(text) => {
            push_ink(&mut props, palette.background);
            boxed_text(props, AzString::from(text)).with_ids_and_classes(class)
        }
        None => Dom::create_div()
            .with_ids_and_classes(class)
            .with_css_props(CssPropertyWithConditionsVec::from_vec(props)),
    })
}

/// The `VirtualView`'s render: the grid that fits its bounds (the scroll
/// bar's strip kept free), the screen for it from the app, the rows.
extern "C" fn render_terminal(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let size = info.get_bounds().get_logical_size();
    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    let keep = VirtualViewReturn::keep_current(rect, rect);
    if !size.width.is_finite()
        || !size.height.is_finite()
        || size.width <= 0.0
        || size.height <= 0.0
    {
        return keep;
    }
    let (source, palette, font_size, line_height, measured) =
        match data.downcast_ref::<TerminalShared>() {
            Some(s) => (
                s.view.data_source.clone(),
                s.palette,
                s.view.font_size,
                s.view.line_height,
                s.measured,
            ),
            None => return keep,
        };
    let unmeasured = Metrics::of(font_size, line_height, None);
    let advance = match measured {
        Some((at, w)) if (at - unmeasured.font_size).abs() < f32::EPSILON => Some(w),
        _ => measured_advance(&info, &unmeasured),
    };
    let metrics = Metrics::of(font_size, line_height, advance);
    let text_width = (size.width - SCROLLBAR_PX).max(0.0);
    let grid = TerminalGridSize::fitting(
        text_width,
        size.height,
        metrics.cell_width,
        metrics.line_height,
    );
    // The app's callback runs with no borrow of the view's data held.
    let screen = screen_of(&source, grid);
    let bar = scroll_bar(
        text_width,
        size.height,
        grid.rows,
        screen.history,
        screen.scroll,
    );
    let dom = build_screen(&screen, grid, &metrics, &palette, size, bar.as_ref());
    if let Some(mut s) = data.downcast_mut::<TerminalShared>() {
        s.measured = advance.map(|w| (metrics.font_size, w));
        s.metrics = metrics;
        s.grid = grid;
        s.bar = bar;
        s.screen = screen;
    }
    VirtualViewReturn::with_dom(dom, rect, rect)
}

/// A rebuilt view keeps what the old one learnt: the measured face, the
/// last screen, a gesture in progress.
extern "C" fn merge_terminal(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    let carried = old_data.downcast_ref::<TerminalShared>().map(|o| {
        (
            o.measured,
            o.metrics,
            o.grid,
            o.screen.clone(),
            o.bar,
            o.drag,
            o.swallow_text,
        )
    });
    if let Some((measured, metrics, grid, screen, bar, drag, swallow_text)) = carried {
        if let Some(mut n) = new_data.downcast_mut::<TerminalShared>() {
            if (n.metrics.font_size - metrics.font_size).abs() < f32::EPSILON {
                n.measured = measured;
                n.metrics = metrics;
            }
            n.grid = grid;
            n.screen = screen;
            n.bar = bar;
            n.drag = drag;
            n.swallow_text = swallow_text;
        }
    }
    new_data
}

impl TerminalView {
    /// The view's DOM: a node that fills its container (`position:
    /// absolute`, inset 0 - give the container `position: relative` and a
    /// size) holding the keys, the pointer and the focus, and a
    /// `VirtualView` in it that renders the rows the data callback gives.
    #[must_use]
    pub fn dom(self) -> Dom {
        let palette = self
            .palette
            .into_option()
            .unwrap_or_else(TerminalPalette::of_app_theme);
        let name = if self.accessibility_name.as_str().is_empty() {
            AzString::from_const_str("Terminal")
        } else {
            self.accessibility_name.clone()
        };
        let mut classes = alloc::vec![IdOrClass::Class(AzString::from_const_str(
            TERMINAL_CLASS_NAME
        ))];
        if !self.id.as_str().is_empty() {
            classes.push(IdOrClass::Id(self.id.clone()));
        }
        let shared = RefAny::new(TerminalShared::new(self, palette));
        let mut ground = Vec::new();
        push_fill(&mut ground, palette.background);
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css("position: absolute; top: 0; left: 0; right: 0; bottom: 0; overflow: hidden;")
            .with_css_props(CssPropertyWithConditionsVec::from_vec(ground))
            .with_tab_index(TabIndex::Auto)
            .with_accessibility_info(AccessibilityInfo::named(name, AccessibilityRole::Document))
            .with_dataset(OptionRefAny::Some(shared.clone()))
            .with_merge_callback(DatasetMergeCallback::from_ptr(merge_terminal))
            .with_callbacks(terminal_callbacks(&shared).into())
            .with_child(
                Dom::create_virtual_view(shared, VirtualViewCallback::create(render_terminal))
                    .with_css("width: 100%; height: 100%; overflow: hidden;"),
            )
    }
}

// ---- the handlers ----

/// The outer node's handlers.
fn terminal_callbacks(shared: &RefAny) -> Vec<CoreCallbackData> {
    let on = |filter: EventFilter, f: usize| CoreCallbackData::create(filter, shared.clone(), f);
    alloc::vec![
        on(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            on_terminal_key as usize
        ),
        on(
            EventFilter::Focus(FocusEventFilter::TextInput),
            on_terminal_text as usize
        ),
        on(
            EventFilter::Focus(FocusEventFilter::Paste),
            on_terminal_paste as usize
        ),
        on(
            EventFilter::Focus(FocusEventFilter::FocusReceived),
            on_terminal_focus as usize
        ),
        on(
            EventFilter::Focus(FocusEventFilter::FocusLost),
            on_terminal_blur as usize
        ),
        on(
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            on_terminal_mouse_down as usize
        ),
        on(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            on_terminal_mouse_move as usize
        ),
        on(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            on_terminal_mouse_up as usize
        ),
        on(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            on_terminal_double_click as usize
        ),
        on(
            EventFilter::Hover(HoverEventFilter::Scroll),
            on_terminal_wheel as usize
        ),
    ]
}

/// What a handler needs of the shared data, copied out.
struct Snap {
    on_event: OptionTerminalViewOnEvent,
    screen: TerminalScreen,
    grid: TerminalGridSize,
    metrics: Metrics,
    bar: Option<ScrollBar>,
    drag: Drag,
}

fn snap(data: &mut RefAny) -> Option<Snap> {
    let s = data.downcast_ref::<TerminalShared>()?;
    Some(Snap {
        on_event: s.view.on_event.clone(),
        screen: s.screen.clone(),
        grid: s.grid,
        metrics: s.metrics,
        bar: s.bar,
        drag: s.drag,
    })
}

fn set_drag(data: &mut RefAny, drag: Drag) {
    if let Some(mut s) = data.downcast_mut::<TerminalShared>() {
        s.drag = drag;
    }
}

fn set_swallow(data: &mut RefAny, swallow: bool) -> bool {
    match data.downcast_mut::<TerminalShared>() {
        Some(mut s) => core::mem::replace(&mut s.swallow_text, swallow),
        None => false,
    }
}

/// Hands `event` to the app (its `scroll` the current display offset
/// unless it is a `Scroll`).
fn fire(s: &Snap, info: CallbackInfo, mut event: TerminalViewEvent) -> Update {
    if event.kind != TerminalViewEventKind::Scroll {
        event.scroll = s.screen.scroll;
    }
    match s.on_event.as_ref() {
        Some(TerminalViewOnEvent { refany, callback }) => {
            callback.invoke(refany.clone(), info, event)
        }
        None => Update::DoNothing,
    }
}

/// `bytes` to the program (nothing for none).
fn send(s: &Snap, info: CallbackInfo, bytes: U8Vec) -> Update {
    if bytes.is_empty() {
        Update::DoNothing
    } else {
        fire(s, info, TerminalViewEvent::input(bytes))
    }
}

/// `event` to the app, then the view re-renders (the app moved its view).
fn fire_and_render(s: &Snap, mut info: CallbackInfo, event: TerminalViewEvent) -> Update {
    let update = fire(s, info, event);
    info.trigger_all_virtual_view_rerender();
    update
}

/// A selection event at `point`.
fn select_event(
    kind: TerminalViewEventKind,
    point: TerminalPoint,
    right_half: bool,
) -> TerminalViewEvent {
    let mut e = TerminalViewEvent::create(kind);
    e.point = point;
    e.right_half = right_half;
    e
}

/// A key: copy, paste (the engine's paste event brings the text), a scroll
/// of the scrollback, or bytes for the program - which keeps the key from
/// everything else (no spatial navigation, no shortcut of the window).
extern "C" fn on_terminal_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    set_swallow(&mut data, false);
    let Some(key) = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    let mac = azul_core::window::mac_shortcut_conventions();
    match key_action(&s.screen, s.grid.rows, key, modifiers, mac) {
        KeyAction::Nothing | KeyAction::Paste => Update::DoNothing,
        KeyAction::Copy => {
            info.prevent_default();
            info.stop_propagation();
            fire(
                &s,
                info,
                TerminalViewEvent::create(TerminalViewEventKind::Copy),
            )
        }
        KeyAction::Scroll(to) => {
            info.prevent_default();
            info.stop_propagation();
            if to == s.screen.scroll {
                return Update::DoNothing;
            }
            fire_and_render(&s, info, TerminalViewEvent::scrolled(to))
        }
        KeyAction::Bytes(bytes) => {
            info.prevent_default();
            info.stop_propagation();
            // Ctrl / Alt + a key sent its own bytes: the character the OS
            // types for it (if any) is not the program's too.
            if modifiers.ctrl || modifiers.alt {
                set_swallow(&mut data, true);
            }
            send(&s, info, U8Vec::from_vec(bytes))
        }
    }
}

/// Typed text (and an IME's commit): its UTF-8 to the program. The view
/// holds no text of its own, so the engine's insertion is cancelled.
extern "C" fn on_terminal_text(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(inserted) = info
        .get_text_changeset()
        .map(|c| AzString::from(c.inserted_text.as_str()))
    else {
        return Update::DoNothing;
    };
    info.prevent_default();
    if set_swallow(&mut data, false) {
        return Update::DoNothing;
    }
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    let bytes = s.screen.modes.encode_text(inserted);
    send(&s, info, bytes)
}

/// The paste the engine read from the clipboard: bracketed for a program
/// that asked. Off macOS, Ctrl+V without Shift is the program's `^V`.
extern "C" fn on_terminal_paste(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let text = info.get_clipboard_content().map(|c| c.plain_text.clone());
    info.prevent_default();
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    let keys = info.get_current_keyboard_state();
    if !azul_core::window::mac_shortcut_conventions() && keys.ctrl_down() && !keys.shift_down() {
        return send(&s, info, U8Vec::from_vec(alloc::vec![0x16]));
    }
    match text {
        Some(text) => send(&s, info, s.screen.modes.encode_paste(text)),
        None => Update::DoNothing,
    }
}

/// The focus came: `CSI I` for a program that asked.
extern "C" fn on_terminal_focus(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    send(&s, info, s.screen.modes.encode_focus(true))
}

/// The focus left: `CSI O` for a program that asked.
extern "C" fn on_terminal_blur(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    send(&s, info, s.screen.modes.encode_focus(false))
}

/// A press: on the scroll bar it grabs the thumb (on the track it jumps
/// there); for a program that hears the pointer it is reported (Shift
/// selects anyway); otherwise a selection starts (Alt: a block).
extern "C" fn on_terminal_mouse_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = cursor_in(&info) else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    if let Some(bar) = s.bar.filter(|b| b.contains(x, y)) {
        let thumb_top = bar.track.1 + bar.thumb_start;
        let grab = if y >= thumb_top && y < thumb_top + bar.thumb_len {
            y - thumb_top
        } else {
            bar.thumb_len / 2.0
        };
        set_drag(&mut data, Drag::Thumb { grab });
        info.prevent_default();
        let to = scroll_for_thumb(&bar, y - grab - bar.track.1, s.screen.history);
        if to == s.screen.scroll {
            return Update::DoNothing;
        }
        return fire_and_render(&s, info, TerminalViewEvent::scrolled(to));
    }
    let (point, right_half) = s.metrics.cell_at(s.grid, x, y);
    if s.screen.modes.mouse != TerminalMouseMode::Off && !modifiers.shift {
        set_drag(
            &mut data,
            Drag::Report {
                button: TerminalMouseButton::Left,
                last: point,
            },
        );
        let bytes = s.screen.modes.encode_mouse(
            TerminalMouseButton::Left,
            TerminalMouseAction::Press,
            point,
            modifiers,
        );
        return send(&s, info, bytes);
    }
    set_drag(
        &mut data,
        Drag::Select {
            last: point,
            moved: false,
        },
    );
    let mut e = select_event(TerminalViewEventKind::SelectStart, point, right_half);
    e.selection_kind = if modifiers.alt {
        TerminalSelectionKind::Block
    } else {
        TerminalSelectionKind::Simple
    };
    fire_and_render(&s, info, e)
}

/// A move: the thumb follows, a selection extends (once a cell), a held
/// button is reported to a program that hears drags.
extern "C" fn on_terminal_mouse_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    if s.drag == Drag::None {
        return Update::DoNothing;
    }
    let Some((x, y)) = cursor_in(&info) else {
        return Update::DoNothing;
    };
    match s.drag {
        Drag::None => Update::DoNothing,
        Drag::Thumb { grab } => {
            let Some(bar) = s.bar else {
                return Update::DoNothing;
            };
            let to = scroll_for_thumb(&bar, y - grab - bar.track.1, s.screen.history);
            if to == s.screen.scroll {
                return Update::DoNothing;
            }
            fire_and_render(&s, info, TerminalViewEvent::scrolled(to))
        }
        Drag::Select { last, .. } => {
            let (point, right_half) = s.metrics.cell_at(s.grid, x, y);
            if point == last {
                return Update::DoNothing;
            }
            set_drag(
                &mut data,
                Drag::Select {
                    last: point,
                    moved: true,
                },
            );
            fire_and_render(
                &s,
                info,
                select_event(TerminalViewEventKind::SelectExtend, point, right_half),
            )
        }
        Drag::Report { button, last } => {
            let (point, _) = s.metrics.cell_at(s.grid, x, y);
            if point == last {
                return Update::DoNothing;
            }
            set_drag(
                &mut data,
                Drag::Report {
                    button,
                    last: point,
                },
            );
            let modifiers = info.get_key_modifiers();
            let bytes =
                s.screen
                    .modes
                    .encode_mouse(button, TerminalMouseAction::Motion, point, modifiers);
            send(&s, info, bytes)
        }
    }
}

/// A release: a selection gesture ends (a press without a move clears the
/// selection), a held button is reported released.
extern "C" fn on_terminal_mouse_up(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    set_drag(&mut data, Drag::None);
    match s.drag {
        Drag::None | Drag::Thumb { .. } => Update::DoNothing,
        Drag::Select { last, moved } => {
            let kind = if moved {
                TerminalViewEventKind::SelectEnd
            } else {
                TerminalViewEventKind::SelectClear
            };
            fire_and_render(&s, info, select_event(kind, last, false))
        }
        Drag::Report { button, last } => {
            let point = cursor_in(&info).map_or(last, |(x, y)| s.metrics.cell_at(s.grid, x, y).0);
            let modifiers = info.get_key_modifiers();
            let bytes =
                s.screen
                    .modes
                    .encode_mouse(button, TerminalMouseAction::Release, point, modifiers);
            send(&s, info, bytes)
        }
    }
}

/// A double-click selects the word under the pointer.
extern "C" fn on_terminal_double_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    if s.screen.modes.mouse != TerminalMouseMode::Off && !info.get_key_modifiers().shift {
        return Update::DoNothing;
    }
    let Some((x, y)) = cursor_in(&info) else {
        return Update::DoNothing;
    };
    // The selection is whole: the release that follows must not clear it.
    set_drag(&mut data, Drag::None);
    let (point, right_half) = s.metrics.cell_at(s.grid, x, y);
    let mut e = select_event(TerminalViewEventKind::SelectStart, point, right_half);
    e.selection_kind = TerminalSelectionKind::Word;
    fire_and_render(&s, info, e)
}

/// The wheel: whole lines (a notch is three), scrolling the scrollback,
/// reported, or arrow keys on the alternate screen ([`wheel_action`]). The
/// view is the scroll surface: the box around it does not scroll.
extern "C" fn on_terminal_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(s) = snap(&mut data) else {
        return Update::DoNothing;
    };
    let hit = info.get_hit_node();
    let Some(node_id) = hit.node.into_crate_internal() else {
        return Update::DoNothing;
    };
    let Some(delta) = info.get_scroll_delta(hit.dom, node_id) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    info.stop_propagation();
    #[allow(clippy::cast_precision_loss)] // 3
    let notch_px = s.metrics.line_height * TERMINAL_WHEEL_LINES as f32;
    let (notches, _) = take_wheel(0.0, delta.y, notch_px, 1.0);
    if notches == 0 {
        return Update::DoNothing;
    }
    let point = cursor_in(&info).map_or(TerminalPoint::create(0, 0), |(x, y)| {
        s.metrics.cell_at(s.grid, x, y).0
    });
    match wheel_action(&s.screen, notches, point, info.get_key_modifiers()) {
        KeyAction::Scroll(to) if to != s.screen.scroll => {
            fire_and_render(&s, info, TerminalViewEvent::scrolled(to))
        }
        KeyAction::Bytes(bytes) => send(&s, info, U8Vec::from_vec(bytes)),
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! A small screen for the widget's own tests and the widget manifest
    //! (`widgets::label_convention::every_widget_dom`).
    use super::*;

    /// A prompt, a coloured line, a selection and a block cursor.
    pub(crate) extern "C" fn sample_screen(_: RefAny, size: TerminalGridSize) -> TerminalScreen {
        let green = TerminalStyle::colored(TerminalColor::Indexed(2), TerminalColor::Background);
        let mut screen = TerminalScreen::create(TerminalLineVec::from_vec(alloc::vec![
            TerminalLine::create(TerminalRunVec::from_vec(alloc::vec![
                TerminalRun::create(AzString::from("~ "), 2, green),
                TerminalRun::create(AzString::from("$ ls"), 4, TerminalStyle::create()),
            ])),
            TerminalLine::plain(AzString::from("Cargo.toml  src")),
            TerminalLine::plain(AzString::from("$ ")),
        ]));
        screen.history = 40;
        screen.cursor = TerminalCursor::create(2, 2, TerminalCursorShape::Block);
        screen.selection = OptionTerminalSelection::Some(TerminalSelection::create(
            TerminalPoint::create(1, 0),
            TerminalPoint::create(1, 9u32.min(size.columns.saturating_sub(1))),
        ));
        screen
    }

    /// The view the manifest builds.
    pub(crate) fn sample() -> TerminalView {
        TerminalView::create()
            .with_data_source(
                RefAny::new(()),
                sample_screen as TerminalViewDataSourceCallbackType,
            )
            .with_accessibility_name(AzString::from("Shell"))
    }
}

#[cfg(test)]
mod build_tests {
    //! The DOM the view and its render build.
    use super::*;

    fn classes_of(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(c) => Some(String::from(c.as_str())),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    fn texts(dom: &Dom, out: &mut Vec<String>) {
        if let azul_core::dom::NodeType::Text(t) = dom.root.get_node_type() {
            out.push(String::from(t.as_ref().as_str()));
        }
        for c in dom.children.as_ref() {
            texts(c, out);
        }
    }

    fn screen_dom() -> Dom {
        let grid = TerminalGridSize::create(40, 5);
        let screen = fixtures::sample_screen(RefAny::new(()), grid);
        let m = Metrics::of(13.0, 17.0, Some(8.0));
        let bar = scroll_bar(320.0, 85.0, grid.rows, screen.history, screen.scroll);
        build_screen(
            &screen,
            grid,
            &m,
            &TerminalPalette::flat(),
            LogicalSize::new(332.0, 85.0),
            bar.as_ref(),
        )
    }

    #[test]
    fn the_view_is_one_tab_stop_hosting_a_virtual_view() {
        let dom = fixtures::sample().dom();
        assert_eq!(dom.root.get_tab_index(), Some(TabIndex::Auto));
        assert!(classes_of(&dom).iter().any(|c| c == TERMINAL_CLASS_NAME));
        assert!(dom.root.get_dataset().is_some());
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 1);
        assert!(kids[0].root.is_virtual_view_node());
    }

    #[test]
    fn only_the_rows_in_view_are_built_one_box_per_run() {
        let dom = screen_dom();
        assert!(classes_of(&dom).iter().any(|c| c == SCREEN_CLASS_NAME));
        let mut t = Vec::new();
        texts(&dom, &mut t);
        // Every run's text, the cursor's character (a blank: none).
        assert_eq!(t, ["~ ", "$ ls", "Cargo.toml  src", "$ "]);
    }

    #[test]
    fn the_cursor_and_the_thumb_are_drawn_when_there_is_scrollback() {
        let dom = screen_dom();
        let all: Vec<Vec<String>> = dom.children.as_ref().iter().map(classes_of).collect();
        assert!(all.iter().any(|c| c.iter().any(|c| c == CURSOR_CLASS_NAME)));
        assert!(all.iter().any(|c| c.iter().any(|c| c == THUMB_CLASS_NAME)));
    }
}

#[cfg(test)]
mod encoding_tests {
    //! The bytes the keys, typed text, a paste, the pointer and the focus
    //! send to the program (xterm's encodings).
    use super::*;
    use VirtualKeyCode as K;

    const NONE: KeyModifiers = KeyModifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };

    fn m(shift: bool, ctrl: bool, alt: bool, meta: bool) -> KeyModifiers {
        KeyModifiers {
            shift,
            ctrl,
            alt,
            meta,
        }
    }

    fn shift() -> KeyModifiers {
        m(true, false, false, false)
    }

    fn ctrl() -> KeyModifiers {
        m(false, true, false, false)
    }

    fn alt() -> KeyModifiers {
        m(false, false, true, false)
    }

    /// A fresh terminal where Alt sends escape (the Linux / Windows default).
    fn modes() -> TerminalModes {
        let mut t = TerminalModes::create();
        t.alt_sends_escape = true;
        t
    }

    fn key(t: &TerminalModes, k: VirtualKeyCode, mods: KeyModifiers) -> Vec<u8> {
        t.encode_key(k, mods).as_slice().to_vec()
    }

    #[test]
    fn an_arrow_key_sends_csi_and_ss3_in_application_cursor_mode() {
        let mut t = modes();
        assert_eq!(key(&t, K::Up, NONE), b"\x1b[A");
        assert_eq!(key(&t, K::Down, NONE), b"\x1b[B");
        assert_eq!(key(&t, K::Right, NONE), b"\x1b[C");
        assert_eq!(key(&t, K::Left, NONE), b"\x1b[D");
        t.application_cursor = true;
        assert_eq!(key(&t, K::Up, NONE), b"\x1bOA");
        assert_eq!(key(&t, K::Left, NONE), b"\x1bOD");
    }

    #[test]
    fn an_arrow_with_modifiers_sends_csi_1_semicolon_the_modifier() {
        let mut t = modes();
        t.application_cursor = true;
        assert_eq!(key(&t, K::Up, shift()), b"\x1b[1;2A");
        assert_eq!(key(&t, K::Left, alt()), b"\x1b[1;3D");
        assert_eq!(key(&t, K::Right, ctrl()), b"\x1b[1;5C");
        assert_eq!(key(&t, K::Down, m(true, true, false, false)), b"\x1b[1;6B");
        assert_eq!(key(&t, K::Down, m(true, true, true, false)), b"\x1b[1;8B");
    }

    #[test]
    fn home_and_end_follow_the_cursor_mode() {
        let mut t = modes();
        assert_eq!(key(&t, K::Home, NONE), b"\x1b[H");
        assert_eq!(key(&t, K::End, NONE), b"\x1b[F");
        assert_eq!(key(&t, K::End, ctrl()), b"\x1b[1;5F");
        t.application_cursor = true;
        assert_eq!(key(&t, K::Home, NONE), b"\x1bOH");
        assert_eq!(key(&t, K::End, NONE), b"\x1bOF");
    }

    #[test]
    fn the_editing_keys_send_tilde_sequences() {
        let t = modes();
        assert_eq!(key(&t, K::Insert, NONE), b"\x1b[2~");
        assert_eq!(key(&t, K::Delete, NONE), b"\x1b[3~");
        assert_eq!(key(&t, K::PageUp, NONE), b"\x1b[5~");
        assert_eq!(key(&t, K::PageDown, NONE), b"\x1b[6~");
        assert_eq!(key(&t, K::Delete, ctrl()), b"\x1b[3;5~");
        assert_eq!(key(&t, K::PageUp, alt()), b"\x1b[5;3~");
    }

    #[test]
    fn function_keys_send_ss3_up_to_f4_and_tilde_codes_after() {
        let t = modes();
        assert_eq!(key(&t, K::F1, NONE), b"\x1bOP");
        assert_eq!(key(&t, K::F2, NONE), b"\x1bOQ");
        assert_eq!(key(&t, K::F3, NONE), b"\x1bOR");
        assert_eq!(key(&t, K::F4, NONE), b"\x1bOS");
        assert_eq!(key(&t, K::F5, NONE), b"\x1b[15~");
        assert_eq!(key(&t, K::F6, NONE), b"\x1b[17~");
        assert_eq!(key(&t, K::F10, NONE), b"\x1b[21~");
        assert_eq!(key(&t, K::F11, NONE), b"\x1b[23~");
        assert_eq!(key(&t, K::F12, NONE), b"\x1b[24~");
        assert_eq!(key(&t, K::F13, NONE), b"\x1b[25~");
        assert_eq!(key(&t, K::F20, NONE), b"\x1b[34~");
        assert_eq!(key(&t, K::F1, shift()), b"\x1b[1;2P");
        assert_eq!(key(&t, K::F5, ctrl()), b"\x1b[15;5~");
    }

    #[test]
    fn ctrl_and_a_letter_sends_its_c0_control() {
        let t = modes();
        assert_eq!(key(&t, K::A, ctrl()), [0x01]);
        assert_eq!(key(&t, K::C, ctrl()), [0x03]);
        assert_eq!(key(&t, K::D, ctrl()), [0x04]);
        assert_eq!(key(&t, K::Z, ctrl()), [0x1a]);
        // Shift does not change a control character.
        assert_eq!(key(&t, K::C, m(true, true, false, false)), [0x03]);
    }

    #[test]
    fn ctrl_and_punctuation_sends_the_remaining_controls() {
        let t = modes();
        assert_eq!(key(&t, K::Space, ctrl()), [0x00]);
        assert_eq!(key(&t, K::Key2, ctrl()), [0x00]);
        assert_eq!(key(&t, K::LBracket, ctrl()), [0x1b]);
        assert_eq!(key(&t, K::Backslash, ctrl()), [0x1c]);
        assert_eq!(key(&t, K::RBracket, ctrl()), [0x1d]);
        assert_eq!(key(&t, K::Key6, ctrl()), [0x1e]);
        assert_eq!(key(&t, K::Minus, ctrl()), [0x1f]);
        assert_eq!(key(&t, K::Slash, ctrl()), [0x1f]);
    }

    #[test]
    fn enter_tab_backspace_and_escape_send_their_bytes() {
        let t = modes();
        assert_eq!(key(&t, K::Return, NONE), b"\r");
        assert_eq!(key(&t, K::NumpadEnter, NONE), b"\r");
        assert_eq!(key(&t, K::Tab, NONE), b"\t");
        assert_eq!(key(&t, K::Tab, shift()), b"\x1b[Z");
        assert_eq!(key(&t, K::Back, NONE), [0x7f]);
        assert_eq!(key(&t, K::Back, ctrl()), [0x08]);
        assert_eq!(key(&t, K::Escape, NONE), [0x1b]);
    }

    #[test]
    fn alt_sends_escape_before_the_key_when_the_user_wants_meta() {
        let t = modes();
        assert_eq!(key(&t, K::B, alt()), b"\x1bb");
        assert_eq!(key(&t, K::B, m(true, false, true, false)), b"\x1bB");
        assert_eq!(key(&t, K::Key1, alt()), b"\x1b1");
        assert_eq!(key(&t, K::Back, alt()), b"\x1b\x7f");
        assert_eq!(key(&t, K::Return, alt()), b"\x1b\r");
        assert_eq!(key(&t, K::Period, alt()), b"\x1b.");
        // Option types characters when Alt does not send escape (macOS).
        let mut mac = modes();
        mac.alt_sends_escape = false;
        assert!(key(&mac, K::B, alt()).is_empty());
    }

    #[test]
    fn ctrl_alt_and_a_letter_is_left_to_the_layout_for_altgr() {
        // AltGr arrives as Ctrl+Alt on Windows: its character comes typed.
        let t = modes();
        assert!(key(&t, K::Q, m(false, true, true, false)).is_empty());
        assert!(key(&t, K::Key7, m(false, true, true, false)).is_empty());
    }

    #[test]
    fn a_letter_without_ctrl_or_alt_sends_nothing_because_its_text_comes_typed() {
        let t = modes();
        assert!(key(&t, K::A, NONE).is_empty());
        assert!(key(&t, K::A, shift()).is_empty());
        assert!(key(&t, K::Key1, NONE).is_empty());
        assert!(key(&t, K::Space, NONE).is_empty());
        assert!(key(&t, K::LShift, shift()).is_empty());
    }

    #[test]
    fn keys_with_cmd_or_the_windows_key_are_the_apps() {
        let t = modes();
        assert!(key(&t, K::C, m(false, false, false, true)).is_empty());
        assert!(key(&t, K::Up, m(false, false, false, true)).is_empty());
        assert!(key(&t, K::Return, m(false, false, false, true)).is_empty());
    }

    #[test]
    fn the_keypad_sends_ss3_in_application_keypad_mode() {
        let mut t = modes();
        assert!(key(&t, K::Numpad5, NONE).is_empty());
        t.application_keypad = true;
        assert_eq!(key(&t, K::Numpad0, NONE), b"\x1bOp");
        assert_eq!(key(&t, K::Numpad5, NONE), b"\x1bOu");
        assert_eq!(key(&t, K::Numpad9, NONE), b"\x1bOy");
        assert_eq!(key(&t, K::NumpadEnter, NONE), b"\x1bOM");
        assert_eq!(key(&t, K::NumpadAdd, NONE), b"\x1bOk");
        assert_eq!(key(&t, K::NumpadSubtract, NONE), b"\x1bOm");
        assert_eq!(key(&t, K::NumpadMultiply, NONE), b"\x1bOj");
        assert_eq!(key(&t, K::NumpadDivide, NONE), b"\x1bOo");
        assert_eq!(key(&t, K::NumpadDecimal, NONE), b"\x1bOn");
    }

    #[test]
    fn typed_text_goes_out_as_utf8_without_control_characters() {
        let t = modes();
        assert_eq!(
            t.encode_text(AzString::from("é€")).as_slice(),
            "é€".as_bytes()
        );
        assert_eq!(t.encode_text(AzString::from("a\u{3}b\r")).as_slice(), b"ab");
        assert!(t.encode_text(AzString::from("")).as_slice().is_empty());
    }

    #[test]
    fn a_paste_is_bracketed_when_the_program_asks_and_cannot_close_the_bracket() {
        let mut t = modes();
        t.bracketed_paste = true;
        assert_eq!(
            t.encode_paste(AzString::from("ls\x1b[201~ -la\n"))
                .as_slice(),
            b"\x1b[200~ls[201~ -la\n\x1b[201~"
        );
    }

    #[test]
    fn an_unbracketed_paste_sends_line_breaks_as_carriage_returns() {
        let t = modes();
        assert_eq!(
            t.encode_paste(AzString::from("a\r\nb\nc")).as_slice(),
            b"a\rb\rc"
        );
    }

    #[test]
    fn focus_is_reported_only_when_the_program_asks() {
        let mut t = modes();
        assert!(t.encode_focus(true).as_slice().is_empty());
        t.focus_reporting = true;
        assert_eq!(t.encode_focus(true).as_slice(), b"\x1b[I");
        assert_eq!(t.encode_focus(false).as_slice(), b"\x1b[O");
    }

    fn mouse(
        t: &TerminalModes,
        b: TerminalMouseButton,
        a: TerminalMouseAction,
        line: u32,
        column: u32,
        mods: KeyModifiers,
    ) -> Vec<u8> {
        t.encode_mouse(b, a, TerminalPoint::create(line, column), mods)
            .as_slice()
            .to_vec()
    }

    use TerminalMouseAction as A;
    use TerminalMouseButton as B;

    #[test]
    fn no_pointer_report_without_a_mouse_mode() {
        let t = modes();
        assert!(mouse(&t, B::Left, A::Press, 0, 0, NONE).is_empty());
        assert!(mouse(&t, B::WheelUp, A::Press, 0, 0, NONE).is_empty());
    }

    #[test]
    fn an_sgr_report_names_the_button_the_cell_and_the_modifiers() {
        let mut t = modes();
        t.mouse = TerminalMouseMode::Click;
        t.mouse_encoding = TerminalMouseEncoding::Sgr;
        assert_eq!(mouse(&t, B::Left, A::Press, 4, 9, NONE), b"\x1b[<0;10;5M");
        assert_eq!(mouse(&t, B::Left, A::Release, 4, 9, NONE), b"\x1b[<0;10;5m");
        assert_eq!(mouse(&t, B::Middle, A::Press, 0, 0, NONE), b"\x1b[<1;1;1M");
        assert_eq!(
            mouse(&t, B::Right, A::Press, 4, 9, ctrl()),
            b"\x1b[<18;10;5M"
        );
        assert_eq!(mouse(&t, B::Left, A::Press, 0, 0, shift()), b"\x1b[<4;1;1M");
        assert_eq!(mouse(&t, B::Left, A::Press, 0, 0, alt()), b"\x1b[<8;1;1M");
        assert_eq!(
            mouse(&t, B::WheelUp, A::Press, 4, 9, NONE),
            b"\x1b[<64;10;5M"
        );
        assert_eq!(
            mouse(&t, B::WheelDown, A::Press, 4, 9, NONE),
            b"\x1b[<65;10;5M"
        );
        // Past the old 223-column limit: SGR says it.
        assert_eq!(
            mouse(&t, B::Left, A::Press, 0, 299, NONE),
            b"\x1b[<0;300;1M"
        );
    }

    #[test]
    fn a_default_report_is_three_offset_bytes_and_a_release_is_button_3() {
        let mut t = modes();
        t.mouse = TerminalMouseMode::Click;
        assert_eq!(
            mouse(&t, B::Left, A::Press, 0, 0, NONE),
            [0x1b, b'[', b'M', 32, 33, 33]
        );
        assert_eq!(
            mouse(&t, B::Right, A::Press, 2, 5, NONE),
            [0x1b, b'[', b'M', 34, 38, 35]
        );
        assert_eq!(
            mouse(&t, B::Left, A::Release, 0, 0, NONE),
            [0x1b, b'[', b'M', 35, 33, 33]
        );
        // A cell the three bytes cannot say is not reported.
        assert!(mouse(&t, B::Left, A::Press, 0, 300, NONE).is_empty());
    }

    #[test]
    fn a_utf8_report_writes_large_coordinates_as_characters() {
        let mut t = modes();
        t.mouse = TerminalMouseMode::Click;
        t.mouse_encoding = TerminalMouseEncoding::Utf8;
        // Column 200 is 32 + 201 = 233 = U+00E9, two bytes in UTF-8.
        assert_eq!(
            mouse(&t, B::Left, A::Press, 0, 200, NONE),
            [0x1b, b'[', b'M', 32, 0xC3, 0xA9, 33]
        );
    }

    #[test]
    fn motion_is_reported_only_in_the_modes_that_ask_for_it() {
        let mut t = modes();
        t.mouse_encoding = TerminalMouseEncoding::Sgr;
        t.mouse = TerminalMouseMode::Click;
        assert!(mouse(&t, B::Left, A::Motion, 1, 1, NONE).is_empty());
        t.mouse = TerminalMouseMode::Drag;
        assert_eq!(mouse(&t, B::Left, A::Motion, 1, 1, NONE), b"\x1b[<32;2;2M");
        assert!(mouse(&t, B::None, A::Motion, 1, 1, NONE).is_empty());
        t.mouse = TerminalMouseMode::Motion;
        assert_eq!(mouse(&t, B::None, A::Motion, 1, 1, NONE), b"\x1b[<35;2;2M");
    }
}

#[cfg(test)]
mod palette_tests {
    //! The colours a cell is drawn in.
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> ColorU {
        ColorU::rgb(r, g, b)
    }

    #[test]
    fn the_256_colour_cube_and_grey_ramp_follow_xterm() {
        assert_eq!(xterm_256_color(15), None);
        assert_eq!(xterm_256_color(16), Some(rgb(0, 0, 0)));
        assert_eq!(xterm_256_color(21), Some(rgb(0, 0, 255)));
        assert_eq!(xterm_256_color(196), Some(rgb(255, 0, 0)));
        assert_eq!(xterm_256_color(110), Some(rgb(135, 175, 215)));
        assert_eq!(xterm_256_color(231), Some(rgb(255, 255, 255)));
        assert_eq!(xterm_256_color(232), Some(rgb(8, 8, 8)));
        assert_eq!(xterm_256_color(255), Some(rgb(238, 238, 238)));
    }

    #[test]
    fn a_palette_index_below_16_is_its_ansi_colour() {
        let p = TerminalPalette::flat();
        assert_eq!(p.ansi(0), p.black);
        assert_eq!(p.ansi(1), p.red);
        assert_eq!(p.ansi(9), p.bright_red);
        assert_eq!(p.ansi(15), p.bright_white);
        assert_eq!(p.color_of(TerminalColor::Indexed(4)), p.blue);
        assert_eq!(p.color_of(TerminalColor::Indexed(12)), p.bright_blue);
        assert_eq!(
            p.color_of(TerminalColor::Indexed(196)),
            ChartColor::same(rgb(255, 0, 0))
        );
        assert_eq!(
            p.color_of(TerminalColor::Rgb(rgb(1, 2, 3))),
            ChartColor::same(rgb(1, 2, 3))
        );
        assert_eq!(p.color_of(TerminalColor::Foreground), p.foreground);
        assert_eq!(p.color_of(TerminalColor::Background), p.background);
    }

    #[test]
    fn the_default_ground_is_not_painted_and_another_one_is() {
        let p = TerminalPalette::flora_ink();
        let plain = p.colors_of(&TerminalStyle::create());
        assert_eq!(plain.ink, p.foreground);
        assert!(!plain.paints_ground);
        let on_blue = p.colors_of(&TerminalStyle::colored(
            TerminalColor::Foreground,
            TerminalColor::Indexed(4),
        ));
        assert!(on_blue.paints_ground);
        assert_eq!(on_blue.ground, p.blue);
    }

    #[test]
    fn inverse_swaps_ink_and_ground_and_paints_the_ground() {
        let p = TerminalPalette::flat();
        let mut s = TerminalStyle::create();
        s.inverse = true;
        let c = p.colors_of(&s);
        assert_eq!(c.ink, p.background);
        assert_eq!(c.ground, p.foreground);
        assert!(c.paints_ground);
    }

    #[test]
    fn hidden_text_is_drawn_in_its_ground() {
        let p = TerminalPalette::flat();
        let mut s = TerminalStyle::colored(TerminalColor::Indexed(1), TerminalColor::Indexed(2));
        s.hidden = true;
        let c = p.colors_of(&s);
        assert_eq!(c.ink, p.green);
        assert_eq!(c.ground, p.green);
    }

    #[test]
    fn dim_text_is_the_ink_at_two_thirds() {
        let p = TerminalPalette::flat();
        let mut s = TerminalStyle::create();
        s.dim = true;
        let c = p.colors_of(&s);
        assert_eq!(c.ink.light.a, 170);
        assert_eq!(c.ink.dark.a, 170);
        assert_eq!(c.ink.light.r, p.foreground.light.r);
    }

    #[test]
    fn both_built_in_palettes_keep_the_text_readable_on_their_ground() {
        let luma =
            |c: ColorU| 0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b);
        for p in [TerminalPalette::flat(), TerminalPalette::flora_ink()] {
            for (ink, ground) in [
                (p.foreground.light, p.background.light),
                (p.foreground.dark, p.background.dark),
            ] {
                assert!(
                    (luma(ink) - luma(ground)).abs() > 120.0,
                    "{ink:?} on {ground:?}"
                );
            }
        }
    }
}

#[cfg(test)]
mod view_tests {
    //! The grid, the scroll window and the selection.
    use super::*;
    use VirtualKeyCode as K;

    const NONE: KeyModifiers = KeyModifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };

    fn m(shift: bool, ctrl: bool, alt: bool, meta: bool) -> KeyModifiers {
        KeyModifiers {
            shift,
            ctrl,
            alt,
            meta,
        }
    }

    /// A screen of `rows` blank rows over `history` lines, scrolled up `scroll`.
    fn screen(rows: u32, history: u32, scroll: u32) -> TerminalScreen {
        let mut s = TerminalScreen::create(TerminalLineVec::from_vec(
            (0..rows)
                .map(|_| TerminalLine::plain(AzString::from("")))
                .collect(),
        ));
        s.history = history;
        s.scroll = scroll;
        s
    }

    #[test]
    fn the_grid_is_the_whole_cells_that_fit() {
        assert_eq!(
            TerminalGridSize::fitting(800.0, 340.0, 8.0, 17.0),
            TerminalGridSize::create(100, 20)
        );
        assert_eq!(
            TerminalGridSize::fitting(807.9, 356.0, 8.0, 17.0),
            TerminalGridSize::create(100, 20)
        );
        // A box that is not there yet, or nonsense: the smallest grid.
        assert_eq!(
            TerminalGridSize::fitting(0.0, 0.0, 8.0, 17.0),
            TerminalGridSize::create(2, 1)
        );
        assert_eq!(
            TerminalGridSize::fitting(f32::NAN, 100.0, 8.0, 17.0),
            TerminalGridSize::create(2, 5)
        );
        assert_eq!(
            TerminalGridSize::fitting(800.0, 340.0, 0.0, 17.0),
            TerminalGridSize::create(2, 20)
        );
    }

    #[test]
    fn the_cell_follows_the_font_size_until_the_face_is_measured() {
        let guess = Metrics::of(10.0, 0.0, None);
        assert!((guess.cell_width - 6.0).abs() < 1e-4);
        assert!((guess.line_height - 13.0).abs() < 1e-4);
        let measured = Metrics::of(10.0, 15.0, Some(6.25));
        assert!((measured.cell_width - 6.25).abs() < 1e-4);
        assert!((measured.line_height - 15.0).abs() < 1e-4);
        let nonsense = Metrics::of(-3.0, 0.0, Some(f32::NAN));
        assert!((nonsense.font_size - TERMINAL_FONT_SIZE).abs() < 1e-4);
        assert!(nonsense.cell_width > 0.0);
    }

    #[test]
    fn a_point_maps_to_its_cell_and_half_clamped_into_the_grid() {
        let metrics = Metrics::of(10.0, 17.0, Some(8.0));
        let grid = TerminalGridSize::create(80, 24);
        assert_eq!(
            metrics.cell_at(grid, 8.0 * 10.0 + 5.0, 17.0 * 3.0 + 1.0),
            (TerminalPoint::create(3, 10), true)
        );
        assert_eq!(
            metrics.cell_at(grid, 8.0 * 10.0 + 1.0, 0.0),
            (TerminalPoint::create(0, 10), false)
        );
        assert_eq!(
            metrics.cell_at(grid, 10_000.0, 10_000.0),
            (TerminalPoint::create(23, 79), true)
        );
        assert_eq!(
            metrics.cell_at(grid, -4.0, -4.0),
            (TerminalPoint::create(0, 0), false)
        );
    }

    #[test]
    fn scrolling_stops_at_the_oldest_line_and_at_the_output() {
        assert_eq!(scroll_after(0, 100, 3), 3);
        assert_eq!(scroll_after(99, 100, 3), 100);
        assert_eq!(scroll_after(2, 100, -3), 0);
        assert_eq!(scroll_after(0, 0, 3), 0);
        assert_eq!(scroll_after(50, 100, i64::MIN), 0);
        assert_eq!(scroll_after(50, 100, i64::MAX), 100);
    }

    #[test]
    fn the_scroll_bar_thumb_sits_where_the_view_is() {
        assert_eq!(scroll_bar(600.0, 400.0, 24, 0, 0), None);
        let at_bottom = scroll_bar(600.0, 400.0, 24, 76, 0).expect("a bar");
        assert_eq!(at_bottom.track, (600.0, 0.0, SCROLLBAR_PX, 400.0));
        assert!((at_bottom.thumb_len - 96.0).abs() < 1e-3);
        assert!((at_bottom.thumb_start + at_bottom.thumb_len - 400.0).abs() < 1e-3);
        let at_top = scroll_bar(600.0, 400.0, 24, 76, 76).expect("a bar");
        assert!(at_top.thumb_start.abs() < 1e-3);
    }

    #[test]
    fn dragging_the_thumb_to_the_top_shows_the_oldest_line() {
        let bar = scroll_bar(600.0, 400.0, 24, 76, 0).expect("a bar");
        assert_eq!(scroll_for_thumb(&bar, 0.0, 76), 76);
        assert_eq!(scroll_for_thumb(&bar, -50.0, 76), 76);
        assert_eq!(scroll_for_thumb(&bar, 400.0 - bar.thumb_len, 76), 0);
        assert_eq!(
            scroll_for_thumb(&bar, (400.0 - bar.thumb_len) / 2.0, 76),
            38
        );
    }

    #[test]
    fn copy_and_paste_chords_follow_the_platform_and_leave_ctrl_c_to_the_program() {
        let s = screen(24, 0, 0);
        let k = |key, mods, mac| key_action(&s, 24, key, mods, mac);
        assert_eq!(k(K::C, m(false, false, false, true), true), KeyAction::Copy);
        assert_eq!(
            k(K::V, m(false, false, false, true), true),
            KeyAction::Paste
        );
        assert_eq!(
            k(K::C, m(false, true, false, false), true),
            KeyAction::Bytes(alloc::vec![3])
        );
        assert_eq!(k(K::C, m(true, true, false, false), false), KeyAction::Copy);
        assert_eq!(
            k(K::V, m(true, true, false, false), false),
            KeyAction::Paste
        );
        assert_eq!(
            k(K::C, m(false, true, false, false), false),
            KeyAction::Bytes(alloc::vec![3])
        );
        assert_eq!(
            k(K::Insert, m(true, false, false, false), false),
            KeyAction::Paste
        );
        assert_eq!(k(K::A, NONE, false), KeyAction::Nothing);
        assert_eq!(k(K::Up, NONE, false), KeyAction::Bytes(b"\x1b[A".to_vec()));
    }

    #[test]
    fn ctrl_shift_and_a_letter_is_the_windows_off_macos() {
        // New tab, close tab, find ... as in every Linux terminal: the
        // program never sees them (Ctrl+C without Shift still goes out).
        let s = screen(24, 0, 0);
        let ctrl_shift = m(true, true, false, false);
        assert_eq!(key_action(&s, 24, K::T, ctrl_shift, false), KeyAction::Nothing);
        assert_eq!(key_action(&s, 24, K::W, ctrl_shift, false), KeyAction::Nothing);
        assert_eq!(
            key_action(&s, 24, K::T, m(false, true, false, false), false),
            KeyAction::Bytes(alloc::vec![0x14])
        );
        // On macOS Ctrl+Shift+T is the program's (the window's keys are Cmd).
        assert_eq!(
            key_action(&s, 24, K::T, ctrl_shift, true),
            KeyAction::Bytes(alloc::vec![0x14])
        );
    }

    #[test]
    fn shift_page_up_scrolls_a_screen_and_shift_end_returns_to_the_output() {
        let shift = m(true, false, false, false);
        let s = screen(24, 100, 10);
        assert_eq!(
            key_action(&s, 24, K::PageUp, shift, false),
            KeyAction::Scroll(33)
        );
        assert_eq!(
            key_action(&s, 24, K::PageDown, shift, false),
            KeyAction::Scroll(0)
        );
        assert_eq!(
            key_action(&s, 24, K::Home, shift, false),
            KeyAction::Scroll(100)
        );
        assert_eq!(
            key_action(&s, 24, K::End, shift, false),
            KeyAction::Scroll(0)
        );
        // On the alternate screen they are the program's.
        let mut full = screen(24, 0, 0);
        full.modes.alternate_screen = true;
        assert_eq!(
            key_action(&full, 24, K::PageUp, shift, false),
            KeyAction::Bytes(b"\x1b[5;2~".to_vec())
        );
    }

    #[test]
    fn the_wheel_scrolls_the_scrollback_or_belongs_to_the_program() {
        let p = TerminalPoint::create(4, 9);
        let s = screen(24, 100, 10);
        assert_eq!(wheel_action(&s, -1, p, NONE), KeyAction::Scroll(13));
        assert_eq!(wheel_action(&s, 2, p, NONE), KeyAction::Scroll(4));
        assert_eq!(wheel_action(&s, 0, p, NONE), KeyAction::Nothing);
        // A program that asked for reports hears every notch.
        let mut reported = screen(24, 100, 0);
        reported.modes.mouse = TerminalMouseMode::Click;
        reported.modes.mouse_encoding = TerminalMouseEncoding::Sgr;
        assert_eq!(
            wheel_action(&reported, -2, p, NONE),
            KeyAction::Bytes(b"\x1b[<64;10;5M\x1b[<64;10;5M".to_vec())
        );
        // A full-screen program with alternate scroll gets arrow keys.
        let mut full = screen(24, 0, 0);
        full.modes.alternate_screen = true;
        full.modes.alternate_scroll = true;
        assert_eq!(
            wheel_action(&full, 1, p, NONE),
            KeyAction::Bytes(b"\x1b[B\x1b[B\x1b[B".to_vec())
        );
        full.modes.application_cursor = true;
        assert_eq!(
            wheel_action(&full, -1, p, NONE),
            KeyAction::Bytes(b"\x1bOA\x1bOA\x1bOA".to_vec())
        );
    }

    #[test]
    fn a_selection_covers_its_rows_from_the_start_to_the_end_column() {
        let sel =
            TerminalSelection::create(TerminalPoint::create(3, 2), TerminalPoint::create(1, 5));
        assert_eq!(sel.start, TerminalPoint::create(1, 5));
        assert_eq!(sel.columns_on(0, 80), None);
        assert_eq!(sel.columns_on(1, 80), Some((5, 79)));
        assert_eq!(sel.columns_on(2, 80), Some((0, 79)));
        assert_eq!(sel.columns_on(3, 80), Some((0, 2)));
        assert_eq!(sel.columns_on(4, 80), None);
        let block = TerminalSelection::create_block(
            TerminalPoint::create(3, 2),
            TerminalPoint::create(1, 5),
        );
        assert_eq!(block.columns_on(2, 80), Some((2, 5)));
        assert_eq!(block.columns_on(1, 80), Some((2, 5)));
        // A column past the grid is cut at its edge.
        let wide =
            TerminalSelection::create(TerminalPoint::create(0, 90), TerminalPoint::create(0, 120));
        assert_eq!(wide.columns_on(0, 80), None);
        let past =
            TerminalSelection::create(TerminalPoint::create(0, 70), TerminalPoint::create(0, 120));
        assert_eq!(past.columns_on(0, 80), Some((70, 79)));
    }

    #[test]
    fn the_selected_text_joins_the_rows_and_drops_trailing_blanks() {
        let mut s = TerminalScreen::create(TerminalLineVec::from_vec(alloc::vec![
            TerminalLine::plain(AzString::from("hello world   ")),
            TerminalLine::plain(AzString::from("second")),
        ]));
        s.selection = OptionTerminalSelection::Some(TerminalSelection::create(
            TerminalPoint::create(0, 6),
            TerminalPoint::create(1, 2),
        ));
        assert_eq!(s.selected_text().as_str(), "world\nsec");
        // A soft-wrapped row runs on into the next one.
        if let Some(first) = s.lines.as_mut().first_mut() {
            first.wrapped = true;
        }
        assert_eq!(s.selected_text().as_str(), "world   sec");
    }

    #[test]
    fn the_selected_text_counts_a_wide_character_as_two_columns() {
        let wide = TerminalRun::create(AzString::from("日本"), 4, TerminalStyle::create());
        let narrow = TerminalRun::create(AzString::from("ab"), 2, TerminalStyle::create());
        let mut s = TerminalScreen::create(TerminalLineVec::from_vec(alloc::vec![
            TerminalLine::create(TerminalRunVec::from_vec(alloc::vec![wide, narrow]))
        ]));
        s.selection = OptionTerminalSelection::Some(TerminalSelection::create(
            TerminalPoint::create(0, 2),
            TerminalPoint::create(0, 4),
        ));
        assert_eq!(s.selected_text().as_str(), "本a");
    }
}
