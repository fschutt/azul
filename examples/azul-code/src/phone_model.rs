//! AzCode on a phone: the editor is the screen. Where the window is narrower than [`COMPACT_WIDTH`]
//! or shorter than [`COMPACT_HEIGHT`] (a phone upright, a phone on its side) the window is not the
//! desktop's workbench but a compact one: a top bar (the drawer's button, the file tabs, the
//! command palette), the editor, a key row above the soft keyboard, a status bar of one line, the
//! explorer / search / source control / run as a DRAWER from the left over a scrim, and the bottom
//! panel (terminal, problems, output) as a SHEET from the bottom that is dragged between its
//! detents (azul's `BottomSheet`).
//!
//! This file has the plain part (the state, the numbers: no azul types, tested without a window); the
//! window is built by `phone.rs`, which re-exports everything here.

/// A window narrower than this many px (dp, pt) is a phone's.
pub const COMPACT_WIDTH: f32 = 600.0;
/// A window shorter than this many px is a phone turned on its side.
pub const COMPACT_HEIGHT: f32 = 500.0;
/// The smallest touch target, px: Android's 48 dp (iOS asks for 44 pt, which this covers).
pub const TOUCH: f32 = 48.0;
/// The top bar's height without the status bar's room, px.
pub const TOP_BAR: f32 = 48.0;
/// The key row's height, px.
pub const KEY_ROW: f32 = 48.0;
/// The status bar's height without the home indicator's room, px (iOS asks 44 pt of a touch target).
pub const STATUS_BAR: f32 = 44.0;
/// The drawer's share of the window's width, and its limits, px.
pub const DRAWER_SHARE: f32 = 0.84;
/// The drawer is at least this wide, px.
pub const DRAWER_MIN: f32 = 280.0;
/// The drawer is at most this wide, px.
pub const DRAWER_MAX: f32 = 380.0;
/// The room the sheet leaves above it at its full detent, px (the top bar).
pub const SHEET_TOP_MARGIN: f32 = TOP_BAR;
/// The sheet's collapsed height, px: the grip and the panel's tabs.
pub const SHEET_COLLAPSED: f32 = 76.0;

/// Whether a window `width` x `height` px is a phone's: narrow, or short. A size that is no number
/// is a phone's too (the layout that fits every screen).
#[must_use]
pub fn is_compact(width: f32, height: f32) -> bool {
    !(width.is_finite() && height.is_finite()) || width < COMPACT_WIDTH || height < COMPACT_HEIGHT
}

/// The drawer's width in a window `window_width` px wide: [`DRAWER_SHARE`] of it between
/// [`DRAWER_MIN`] and [`DRAWER_MAX`], never wider than the window less a strip of scrim to tap.
#[must_use]
pub fn drawer_width(window_width: f32) -> f32 {
    let wide = if window_width.is_finite() { window_width.max(0.0) } else { 0.0 };
    (wide * DRAWER_SHARE).clamp(DRAWER_MIN, DRAWER_MAX).min((wide - 40.0).max(0.0))
}

/// Whether the key row shows: on a phone, with a file in front, once the editor was touched or the
/// keyboard is up (a keyboard's height is the proof, a touch is what a window without a real
/// keyboard - a desktop window the size of a phone - has).
#[must_use]
pub fn key_row_visible(compact: bool, has_file: bool, editing: bool, keyboard: f32) -> bool {
    compact && has_file && (editing || keyboard > 0.0)
}

/// The panel's sheet as the phone's state keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Detent {
    /// The grip and the tabs only.
    Collapsed,
    /// A few lines.
    Peek,
    /// Half of the window.
    #[default]
    Half,
    /// The window less the top bar.
    Full,
}

/// What the phone's window keeps between builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhoneState {
    /// The drawer is open (over a scrim).
    pub drawer: bool,
    /// Where the panel's sheet rests.
    pub sheet: Detent,
    /// The editor was touched: the key row shows (see [`key_row_visible`]).
    pub editing: bool,
    /// The keyboard has been up since the editor was touched (see [`PhoneState::observe`]).
    pub keyboard_seen: bool,
}

impl PhoneState {
    /// The drawer opens or closes; opening it leaves the editor (no key row over the drawer).
    pub fn toggle_drawer(&mut self) {
        self.drawer = !self.drawer;
        if self.drawer {
            self.editing = false;
        }
    }

    /// The keyboard's height as the layout saw it now: once it has been up and is gone (the user
    /// dismissed it), the editor is no longer being typed into and the key row goes with it.
    pub fn observe(&mut self, keyboard: f32) {
        if keyboard > 0.0 {
            self.keyboard_seen = true;
        } else if self.keyboard_seen {
            self.keyboard_seen = false;
            self.editing = false;
        }
    }
}

#[cfg(test)]
#[path = "phone_model_test.rs"]
mod tests;
