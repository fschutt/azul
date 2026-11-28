//! The sizes the window lays its table out in, as plain arithmetic.
//!
//! azul's DataTable is virtualised in whole rows and takes the px it fills (`with_viewport`): how
//! many rows and columns it builds. The window knows what it is made of - the family's sidebar at
//! the left, the tab ribbon on top (a row of tabs and a band of one fixed height), the status line
//! under everything, the Preview docked at the right of the table (a desktop) or under it (a
//! phone) - and from it the room the table has. The numbers are the widgets' own
//! (`TAB_BAND_PX` 106, a tab row 28 px on a desktop and 48 px for a finger, the status bar 23 px,
//! the rail 18 px); the result errs SMALL by a few px (an empty strip under the table, never a
//! clipped row), the way AzDrive's icon grid does.

/// What the tab ribbon takes on top of the main region on a desktop: the row of tabs (28 px), the
/// golden line under it (2 px) and the band of the active tab (106 px), plus a few px of room.
pub const RIBBON_DESKTOP: f32 = 140.0;

/// The same on a phone: a row of tabs a finger can hit (48 px) and the band (106 px).
pub const RIBBON_PHONE: f32 = 156.0;

/// The status line at the foot of the window (the status bar's 23 px, and its hairline).
pub const FOOTER: f32 = 25.0;

/// The width of a phone's rail, which stands beside the whole main region under the tab row.
pub const RAIL: f32 = 18.0;

/// The width of the Preview docked at the right (a desktop) until the user drags it: the widget's
/// own default (`PREVIEW_WIDTH_DEFAULT`).
pub const PREVIEW_WIDTH: f32 = 320.0;

/// The height of the bottom pane (a phone) at its peek until the user drags it: the widget's own
/// (`PANE_PEEK`).
pub const PREVIEW_PEEK: f32 = 130.0;

/// The most a table is ever laid out in, px: the cap of an input that is no size (never a
/// crash, never a table of millions of px).
pub const MAX_PX: f32 = 16_384.0;

/// The least, px: a window not measured yet still lays a table out.
pub const MIN_WIDTH: f32 = 200.0;
/// See [`MIN_WIDTH`].
pub const MIN_HEIGHT: f32 = 120.0;

/// What the main region is made of beside the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    /// The root's size, px: the window less the system's bars (`Insets::root_size`).
    pub root: (f32, f32),
    /// Whether the window is a phone's.
    pub phone: bool,
    /// How wide the sidebar's column is beside the main region (a desktop's expanded sidebar or
    /// its rail); 0 where it stands over the region (a phone's rail has its own inset) or pushes it
    /// aside (a phone's open drawer).
    pub sidebar: f32,
    /// The Preview's size: its width at the right (a desktop), its height at the foot (a phone);
    /// 0 for no Preview.
    pub preview: f32,
    /// Whether the window has the status line.
    pub footer: bool,
}

/// A size that is a number of px: finite and positive, held to [`MAX_PX`]; anything else is 0.
#[must_use]
pub fn clean(px: f32) -> f32 {
    if px.is_finite() && px > 0.0 {
        px.min(MAX_PX)
    } else {
        0.0
    }
}

/// The room of the table's body (the region under the ribbon and above the status line, beside
/// the sidebar and the Preview): `(width, height)` px.
#[must_use]
pub fn body(room: &Room) -> (f32, f32) {
    let (w, h) = (clean(room.root.0), clean(room.root.1));
    let mut width = w - clean(room.sidebar);
    let mut height = h - if room.phone {
        RIBBON_PHONE
    } else {
        RIBBON_DESKTOP
    };
    if room.footer {
        height -= FOOTER;
    }
    if room.phone {
        width -= RAIL;
        height -= clean(room.preview);
    } else {
        width -= clean(room.preview);
    }
    (width.max(MIN_WIDTH), height.max(MIN_HEIGHT))
}

/// The room of the table itself: the body less the room a page's own header takes above it
/// (`header` px) and the frame's hairlines (2 px at each side).
#[must_use]
pub fn table(room: &Room, header: f32) -> (f32, f32) {
    let (w, h) = body(room);
    (
        (w - 4.0).max(MIN_WIDTH),
        (h - clean(header) - 2.0).max(MIN_HEIGHT),
    )
}

#[cfg(test)]
#[path = "metrics_test.rs"]
mod metrics_test;
