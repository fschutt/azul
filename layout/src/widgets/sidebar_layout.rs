//! The numbers of the sidebar family: how wide the left sidebar is in each of its states, how a
//! phone's drawer pushes the main view aside, how far the Preview may be dragged, where its
//! pane stops - plain functions of plain numbers, so every rule is tested without a window
//! (`sidebar_layout_test.rs`). The DOM of the widgets (`sidebar.rs`, `preview.rs`,
//! `app_frame.rs`) only reads the answers.
//!
//! # The two form factors
//!
//! A DESKTOP has a left [`crate::widgets::sidebar::Sidebar`] (full height, an icon rail when it is
//! collapsed, a wide column when it is expanded) and a Preview docked at the RIGHT below the
//! tab ribbon. A PHONE has the same left sidebar (the rail is 18 px wide, as on a desktop;
//! expanded it is a drawer that PUSHES the whole main view to the side, tab pills included, and
//! never lies over it) and the Preview docked at the BOTTOM as a pane whose height is dragged.
//! Nothing has a bottom pane on a desktop and nothing has a right sidebar on a phone: two side
//! bars on a narrow screen only confuse. The Preview is the app's choice: an app that adds none
//! has no dock, no grip and no bottom pane.
//!
//! # The rail
//!
//! Collapsed, the sidebar is a strip [`RAIL_WIDTH`] (18 px) wide - "a compromise, neither too big
//! nor too hard to hit" - with icons of [`RAIL_ICON_PX`] (13 px). What a finger or a pointer may
//! hit is wider than what is drawn: every button of the rail reaches [`RAIL_SLOP`] (18 px)
//! inward over the main view, invisibly, and a swipe inward from the rail opens the drawer
//! ([`EDGE_SWIPE_PX`]). The rail holds two groups: NAVIGATION icons at the top and the app's
//! QUICK ACCESS buttons at the bottom, above the status slot and the toggle, where the thumb of a
//! right-handed user holding the phone upright reaches.
//!
//! # The OS's window controls
//!
//! No desktop layout has a title bar. The window controls (macOS's traffic lights) are drawn by
//! the OS over the top-left corner of the window, which is the top of the left sidebar: the
//! sidebar keeps a band of [`controls_height`] there. They are wider than the 18 px rail and
//! overhang it: the tab row starts after them ([`header_left_pad`]). A layout without a sidebar
//! leaves the room before its first tab in the same way.

use azul_css::system::Handedness;

use crate::widgets::tabs::phone_build;

/// Which form factor a widget of the sidebar family is built for.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum FormFactor {
    /// What the window is: a phone on a touch platform in a window narrower than the tab
    /// widget's phone width (`PHONE_MAX_WIDTH`, 600 px), a desktop anywhere else. Asked through
    /// the recorded size queries, so a resize across the width builds the layout again.
    #[default]
    Auto,
    /// A desktop whatever the platform and the window say: a left sidebar and a right Preview.
    Desktop,
    /// A phone whatever the platform and the window say: a left sidebar with a drawer and a
    /// bottom pane. What a desktop window shows to try the phone's layout (`AzWidgets`), and what
    /// a test builds.
    Phone,
}

impl FormFactor {
    /// Whether this is the phone's layout, `Auto` answered by the window being built.
    #[must_use]
    pub fn is_phone(self) -> bool {
        match self {
            Self::Auto => phone_build(),
            Self::Desktop => false,
            Self::Phone => true,
        }
    }

    /// This form factor with `Auto` answered: `Desktop` or `Phone`.
    #[must_use]
    pub fn resolved(self) -> Self {
        if self.is_phone() {
            Self::Phone
        } else {
            Self::Desktop
        }
    }
}

/// How much of its width the left sidebar shows.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SidebarState {
    /// The whole sidebar: sections with titles, rows with their labels and counts, the status
    /// pies as rows. On a phone it is the DRAWER.
    Expanded,
    /// The icon rail, the collapsed state every app starts in: 18 px wide, the navigation icons
    /// at the top, the app's quick-access buttons at the bottom, each with its label as a
    /// tooltip, then the status slot and, last, the toggle.
    #[default]
    Rail,
    /// A strip of 16 px at the window's edge with a grab handle in it. Supported by the code and
    /// never part of the toggle's round unless the sidebar asks for it
    /// ([`crate::widgets::sidebar::Sidebar::with_handle_in_cycle`]).
    Handle,
}

impl SidebarState {
    /// The state the toggle button leads to from this one: `Expanded` and `Rail` swap; with
    /// `handle` the round is `Expanded`, `Rail`, `Handle`, `Expanded`; a `Handle` that is
    /// clicked opens the rail first, as the 16 px strip is only a way back.
    #[must_use]
    pub const fn toggled(self, handle: bool) -> Self {
        match (self, handle) {
            (Self::Expanded, _) => Self::Rail,
            (Self::Rail, false) => Self::Expanded,
            (Self::Rail, true) => Self::Handle,
            (Self::Handle, false) => Self::Rail,
            (Self::Handle, true) => Self::Expanded,
        }
    }

    /// Where a click on the handle strip leads: the rail.
    #[must_use]
    pub const fn from_handle(self) -> Self {
        match self {
            Self::Handle => Self::Rail,
            other => other,
        }
    }
}

// ---- the edges: which side of the window a part stands at ----

/// An edge of the window.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Edge {
    /// The left edge.
    Left,
    /// The right edge.
    Right,
}

impl Edge {
    /// The border of a part at this edge that faces the content beside it (`border-right` of a
    /// part at the left edge).
    #[must_use]
    pub const fn inner_border(self) -> &'static str {
        match self {
            Self::Left => "border-right",
            Self::Right => "border-left",
        }
    }

    /// The offset property that places something against this edge of its box (`left`).
    #[must_use]
    pub const fn offset(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    /// The margin property on the side of a part at this edge that faces the window's edge (the
    /// 1 px a button keeps from it).
    #[must_use]
    pub const fn margin_outer(self) -> &'static str {
        match self {
            Self::Left => "margin-left",
            Self::Right => "margin-right",
        }
    }

    /// The offset property that places something against the side of a box that faces AWAY from
    /// this edge (`left` of a pane that is docked at the right edge: its border with the body).
    #[must_use]
    pub const fn inward_offset(self) -> &'static str {
        match self {
            Self::Left => "right",
            Self::Right => "left",
        }
    }

    /// The `align-items` value that puts a column's children against this edge.
    #[must_use]
    pub const fn align_start(self) -> &'static str {
        match self {
            Self::Left => "flex-start",
            Self::Right => "flex-end",
        }
    }

    /// +1.0 where "inward" (away from this edge) is toward larger x, -1.0 where it is toward smaller x:
    /// the sign of a swipe that opens the drawer.
    #[must_use]
    pub const fn inward(self) -> f32 {
        match self {
            Self::Left => 1.0,
            Self::Right => -1.0,
        }
    }
}

/// The hand the family is built for: right-handed users only, for now. EVERYTHING mirrors for a
/// left-handed user later (the sidebar's side, the rail, the sheet's grips, the Preview's dock,
/// the chevron, the close buttons); the mirror is not built, and each left / right decision of a
/// widget is made in ONE place: [`sidebar_edge`] and [`preview_edge`], which read [`Handedness`].
pub const BUILT_FOR: Handedness = Handedness::RightHanded;

/// The edge the sidebar (its rail, its quick access, its drawer) stands at: the left for a
/// right-handed user, whose thumb sweeps the lower left of a phone held upright.
#[must_use]
pub const fn sidebar_edge(hand: Handedness) -> Edge {
    match hand {
        Handedness::RightHanded => Edge::Left,
        Handedness::LeftHanded => Edge::Right,
    }
}

/// The edge the Preview's right dock stands at: the right for a right-handed user.
#[must_use]
pub const fn preview_edge(hand: Handedness) -> Edge {
    match hand {
        Handedness::RightHanded => Edge::Right,
        Handedness::LeftHanded => Edge::Left,
    }
}

// ---- the left sidebar ----

/// The icon rail's width, in logical px, on a desktop AND on a phone: "a compromise, neither too
/// big nor too hard to hit" (the user: the hit slop and the edge swipe make up for it).
pub const RAIL_WIDTH: f32 = 18.0;
/// The size of the icons in the rail, px (12 to 14 in the wireframe).
pub const RAIL_ICON_PX: f32 = 13.0;
/// How far every button of the rail reaches inward over the main view, invisibly, px (16 to 20
/// in the wireframe): what is hit is wider than what is drawn.
pub const RAIL_SLOP: f32 = 18.0;
/// How far a drag that starts on the rail must go inward before it opens the drawer, px.
pub const EDGE_SWIPE_PX: f32 = 24.0;
/// The most quick-access buttons the rail shows: the rest is dropped with a diagnostic.
pub const MAX_QUICK_ACTIONS: usize = 12;
/// The handle's width, on both.
pub const HANDLE_WIDTH: f32 = 16.0;
/// The expanded sidebar's width on a desktop when the app asks for none.
pub const EXPANDED_WIDTH_DEFAULT: f32 = 240.0;
/// The narrowest and the widest an expanded desktop sidebar is made.
pub const EXPANDED_WIDTH_MIN: f32 = 160.0;
pub const EXPANDED_WIDTH_MAX: f32 = 420.0;
/// The drawer's width on a phone: this share of the frame (percent) up to [`DRAWER_MAX_WIDTH`].
pub const DRAWER_PERCENT: f32 = 76.0;
/// The widest a phone's drawer is, logical px: a phone held wide or a large window with the phone
/// layout does not get a sheet as wide as itself.
pub const DRAWER_MAX_WIDTH: f32 = 360.0;
/// A row of the expanded sidebar: 28 px with a pointer, 44 px under a finger.
pub const ROW_HEIGHT_DESKTOP: f32 = 28.0;
pub const ROW_HEIGHT_PHONE: f32 = 44.0;
/// The height of a button's slot in the rail: 30 px with a pointer, 44 px under a finger (the
/// slot is [`RAIL_WIDTH`] + [`RAIL_SLOP`] wide, the drawn button [`RAIL_WIDTH`]).
pub const RAIL_BUTTON_DESKTOP: f32 = 30.0;
pub const RAIL_BUTTON_PHONE: f32 = 44.0;
/// How far a nested row steps in, per level, in px.
pub const INDENT_PER_DEPTH: f32 = 16.0;
/// The deepest nesting the sidebar draws: a tree deeper than this is drawn at this depth (and a
/// diagnostic says so once).
pub const MAX_DEPTH: usize = 12;
/// The most rows a sidebar builds: the rest of a longer list is dropped with a diagnostic.
pub const MAX_ROWS: usize = 5_000;
/// The most status indicators the foot of the sidebar shows.
pub const MAX_STATUSES: usize = 6;
/// The status pie's disc in the expanded sidebar, in logical px: larger than the pie marker's
/// 11 px so the glance reads at arm's length.
pub const STATUS_PIE_PX: f32 = 18.0;
/// The status indicator's size in the rail, px: the icons' size.
pub const RAIL_STATUS_PX: f32 = 13.0;

/// A wanted expanded width held to its limits; no number is the default.
#[must_use]
pub fn clamp_expanded_width(wanted: f32) -> f32 {
    if !wanted.is_finite() || wanted <= 0.0 {
        return EXPANDED_WIDTH_DEFAULT;
    }
    wanted.clamp(EXPANDED_WIDTH_MIN, EXPANDED_WIDTH_MAX)
}

/// How wide the left sidebar's column is.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct SidebarColumnWidth {
    /// A width in px, or 0 where the column is a share of the frame.
    pub px: f32,
    /// A share of the frame in percent (the phone's drawer), or 0 where the width is in px.
    pub percent: f32,
    /// The most the percent share may come to, px; 0 for no cap.
    pub max_px: f32,
}

impl SidebarColumnWidth {
    const fn px(px: f32) -> Self {
        Self {
            px,
            percent: 0.0,
            max_px: 0.0,
        }
    }

    /// The column's width in a frame `frame_w` wide (a share and its cap resolved).
    #[must_use]
    pub fn in_frame(self, frame_w: f32) -> f32 {
        if self.percent > 0.0 {
            let share = frame_w.max(0.0) * self.percent / 100.0;
            if self.max_px > 0.0 {
                share.min(self.max_px)
            } else {
                share
            }
        } else {
            self.px
        }
    }
}

/// The width of the left sidebar's column: the rail's ([`RAIL_WIDTH`], the same on a desktop and
/// on a phone, and narrower than the OS's window controls: they overhang it), the handle's or the
/// expanded one's (a phone's drawer is a share of the frame).
#[must_use]
pub fn column_width(
    state: SidebarState,
    form: FormFactor,
    expanded: f32,
    controls_left: f32,
) -> SidebarColumnWidth {
    match state {
        SidebarState::Handle => SidebarColumnWidth::px(HANDLE_WIDTH),
        SidebarState::Rail => SidebarColumnWidth::px(RAIL_WIDTH),
        SidebarState::Expanded if form.is_phone() => SidebarColumnWidth {
            px: 0.0,
            percent: DRAWER_PERCENT,
            max_px: DRAWER_MAX_WIDTH,
        },
        SidebarState::Expanded => {
            let wanted = clamp_expanded_width(expanded);
            // never narrower than the window controls, which also sit over it
            let controls = if controls_left.is_finite() { controls_left.max(0.0) } else { 0.0 };
            SidebarColumnWidth::px(wanted.max(controls))
        }
    }
}

/// Whether the main view is PUSHED by the sidebar (it keeps the width of the frame and slides
/// out of the window's right edge) instead of shrinking beside it: a phone's open drawer.
#[must_use]
pub fn pushes_main(state: SidebarState, form: FormFactor) -> bool {
    state == SidebarState::Expanded && form.is_phone()
}

/// The height of the band at the top of the left sidebar that the OS's window controls sit
/// in: the title bar's height on a platform whose controls are at the left (`controls_left`
/// above 0), else nothing.
#[must_use]
pub fn controls_height(controls_left: f32, title_height: f32) -> f32 {
    if controls_left.is_finite() && controls_left > 0.0 && title_height.is_finite() {
        title_height.max(0.0)
    } else {
        0.0
    }
}

/// Whether a drag of (`dx`, `dy`) px from a press on the rail is the swipe that opens the
/// drawer: inward by [`EDGE_SWIPE_PX`] or more, and mostly horizontal (at least twice as far
/// along as across), so that scrolling the rail's buttons does not open it.
#[must_use]
pub fn swipe_opens(dx: f32, dy: f32) -> bool {
    dx.is_finite() && dy.is_finite() && dx >= EDGE_SWIPE_PX && dx > 2.0 * dy.abs()
}

/// [`swipe_opens`] for the sidebar standing at `edge`: `dx` is the drag's x in window coordinates,
/// which is inward where `edge.inward()` says.
#[must_use]
pub fn swipe_opens_at(edge: Edge, dx: f32, dy: f32) -> bool {
    swipe_opens(dx * edge.inward(), dy)
}

/// How far the tab row keeps from the window's left edge to clear the window controls the sidebar
/// does not cover: the controls' width less the sidebar column's (a layout without a sidebar
/// passes 0 for the column).
#[must_use]
pub fn header_left_pad(controls_left: f32, column_px: f32) -> f32 {
    let controls = if controls_left.is_finite() { controls_left.max(0.0) } else { 0.0 };
    let column = if column_px.is_finite() { column_px.max(0.0) } else { 0.0 };
    (controls - column).max(0.0)
}

// ---- the Preview ----

/// Where the Preview is docked.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum PreviewDock {
    /// A desktop's right side, below the tab ribbon, dragged by its left border (its width); a
    /// phone's bottom pane (its height, dragged by its grip).
    #[default]
    Auto,
    /// The right side, whatever the form factor.
    Right,
    /// The bottom, whatever the form factor.
    Bottom,
}

impl PreviewDock {
    /// The dock in `form`: `Auto` is the right on a desktop and the bottom on a phone.
    #[must_use]
    pub fn resolved(self, form: FormFactor) -> Self {
        match self {
            Self::Auto if form.is_phone() => Self::Bottom,
            Self::Auto => Self::Right,
            other => other,
        }
    }
}

/// The right-docked Preview's width when the app asks for none, in px.
pub const PREVIEW_WIDTH_DEFAULT: f32 = 320.0;
/// The narrowest a Preview is dragged to (right dock), px.
pub const PREVIEW_WIDTH_MIN: f32 = 200.0;
/// The widest, as a share of the container beside it.
pub const PREVIEW_WIDTH_MAX_SHARE: f32 = 0.60;
/// What the content beside a right-docked Preview keeps, px.
pub const BODY_MIN_WIDTH: f32 = 240.0;
/// The grip's row: a finger-sized strip with the handle in the middle, and in a bottom pane the
/// title at its left and the (?) buttons at its right, px.
pub const GRIP_HEIGHT: f32 = 28.0;
/// The bottom pane has FOUR detents (wireframe v19, rule 11): the COLLAPSED bar, the PEEK, the
/// HALF and the FULL height, with a smooth drag between them. The collapsed bar is the grip row
/// with the title in it, 28 to 32 px: the pane is never closed by a drag, only by the app, and
/// a bar this small is what an app that is being typed into (Writer, Notes) wants.
pub const PANE_COLLAPSED: f32 = 30.0;
/// The peek, px: enough of the item to see what it is.
pub const PANE_PEEK: f32 = 130.0;
/// The half detent, as a share of the container above the footer.
pub const PANE_HALF_SHARE: f32 = 0.50;
/// The full detent, as a share of the container above the footer.
pub const PANE_MAX_SHARE: f32 = 0.85;
/// The bottom pane's height when the app asks for none, px: the peek.
pub const PANE_HEIGHT_DEFAULT: f32 = PANE_PEEK;
/// A pane released this close to a detent settles ON it, px; between the detents it stays where it
/// was left (a drag is smooth, the detents are only magnetic).
pub const PANE_SNAP: f32 = 28.0;
/// The finger's travel before a press on the grip becomes a drag, px.
pub const GRIP_SLOP: f32 = 4.0;
/// The border of the right dock that is grabbed: this wide, centred on the border, px (a
/// pointer; a finger gets the grip's height).
pub const BORDER_GRAB: f32 = 10.0;

/// A Preview width held between its minimum and what the container spares: at most
/// [`PREVIEW_WIDTH_MAX_SHARE`] of it and never so much that the body beside it is narrower
/// than [`BODY_MIN_WIDTH`]; no number (and 0, "the app said nothing") is the default; a negative
/// width is the minimum. The minimum wins over the maximum in a container too narrow for both.
/// A container that is not known (no number) puts no upper limit.
#[must_use]
pub fn clamp_preview_width(wanted: f32, container_w: f32) -> f32 {
    let wanted = if !wanted.is_finite() || wanted == 0.0 { PREVIEW_WIDTH_DEFAULT } else { wanted };
    let container = if container_w.is_finite() { container_w.max(0.0) } else { 0.0 };
    if container <= 0.0 {
        return wanted.max(PREVIEW_WIDTH_MIN);
    }
    let max = (container * PREVIEW_WIDTH_MAX_SHARE)
        .min(container - BODY_MIN_WIDTH)
        .max(PREVIEW_WIDTH_MIN);
    wanted.clamp(PREVIEW_WIDTH_MIN, max)
}

/// The tallest the bottom pane may be in a container `container_h` tall (the full detent); a
/// container that is not known (no number: the DOM is being built, no layout has run) puts no
/// limit.
#[must_use]
pub fn pane_max(container_h: f32) -> f32 {
    if !container_h.is_finite() {
        return f32::MAX;
    }
    (container_h.max(0.0) * PANE_MAX_SHARE).max(PANE_COLLAPSED)
}

/// The half detent in a container `container_h` tall, or `None` where the container is not
/// known.
#[must_use]
pub fn pane_half(container_h: f32) -> Option<f32> {
    if !container_h.is_finite() {
        return None;
    }
    Some((container_h.max(0.0) * PANE_HALF_SHARE).clamp(PANE_COLLAPSED, pane_max(container_h)))
}

/// The four detents of the bottom pane in a container `container_h` tall, lowest first: the
/// collapsed bar, the peek, the half and the full height. The two that need the container are
/// `NAN` while it is not known.
#[must_use]
pub fn pane_detents(container_h: f32) -> [f32; 4] {
    [
        PANE_COLLAPSED,
        PANE_PEEK.min(pane_max(container_h)),
        pane_half(container_h).unwrap_or(f32::NAN),
        if container_h.is_finite() { pane_max(container_h) } else { f32::NAN },
    ]
}

/// A pane height held between the collapsed bar and [`pane_max`]; no number (and 0, "the app
/// said nothing") is the default (the peek), a negative height the collapsed bar.
#[must_use]
pub fn clamp_pane_height(wanted: f32, container_h: f32) -> f32 {
    let wanted = if !wanted.is_finite() || wanted == 0.0 { PANE_HEIGHT_DEFAULT } else { wanted };
    wanted.clamp(PANE_COLLAPSED, pane_max(container_h))
}

/// Where a pane released at `height` settles: ON the nearest detent when it is within
/// [`PANE_SNAP`] of one, else where it is.
#[must_use]
pub fn settle_pane_height(height: f32, container_h: f32) -> f32 {
    let h = clamp_pane_height(height, container_h);
    let mut best = h;
    let mut best_distance = f32::MAX;
    for detent in pane_detents(container_h) {
        let distance = (detent - h).abs();
        if detent.is_finite() && distance <= PANE_SNAP && distance < best_distance {
            best = detent;
            best_distance = distance;
        }
    }
    best
}

/// What a TAP on the grip does (wireframe rule 11): the pane at its peek goes to the half, at the
/// half (or above) back to the peek, and from the collapsed bar (or anywhere below the peek) up
/// to the peek. Where the container is not known the "half" is 2.5 times the peek.
#[must_use]
pub fn toggle_pane_height(height: f32, container_h: f32) -> f32 {
    let h = clamp_pane_height(height, container_h);
    let half = pane_half(container_h).unwrap_or(PANE_PEEK * 2.5);
    if h >= half - PANE_SNAP {
        PANE_PEEK.min(pane_max(container_h))
    } else if h >= PANE_PEEK - PANE_SNAP {
        half
    } else {
        PANE_PEEK.min(pane_max(container_h))
    }
}

/// The pane's height while its grip is dragged: the height it had at the grab plus what the
/// pointer has moved UP since (the pane grows as the finger goes up).
#[must_use]
pub fn dragged_pane_height(start_h: f32, grab_y: f32, now_y: f32, container_h: f32) -> f32 {
    let moved = if grab_y.is_finite() && now_y.is_finite() { grab_y - now_y } else { 0.0 };
    clamp_pane_height(start_h + moved, container_h)
}

/// The right dock's width while its border is dragged: the width it had at the grab plus what the
/// pointer has moved LEFT since.
#[must_use]
pub fn dragged_preview_width(start_w: f32, grab_x: f32, now_x: f32, container_w: f32) -> f32 {
    let moved = if grab_x.is_finite() && now_x.is_finite() { grab_x - now_x } else { 0.0 };
    clamp_preview_width(start_w + moved, container_w)
}

/// [`dragged_preview_width`] for a dock at `edge`: the pane grows as the pointer goes INWARD, away
/// from the edge.
#[must_use]
pub fn dragged_preview_width_at(edge: Edge, start_w: f32, grab_x: f32, now_x: f32, container_w: f32) -> f32 {
    let moved = if grab_x.is_finite() && now_x.is_finite() { (now_x - grab_x) * edge.inward() } else { 0.0 };
    clamp_preview_width(start_w + moved, container_w)
}

/// What a key does to the pane (the APG window splitter).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GripKey {
    /// The pane gets bigger: Left on a right dock, Up on a bottom pane.
    Grow,
    /// The pane gets smaller: Right on a right dock, Down on a bottom pane.
    Shrink,
    /// Home: the smallest the pane goes.
    ToMin,
    /// End: the biggest.
    ToMax,
}

/// How far an arrow key moves the pane, px, and with Ctrl (Cmd on macOS) held.
pub const KEY_STEP: f32 = 16.0;
pub const KEY_STEP_COARSE: f32 = 64.0;

/// The pane's size after `key` in a container `w` x `h`: held to the dock's limits. `coarse` is
/// the Ctrl / Cmd chord.
#[must_use]
pub fn step_size(dock: PreviewDock, key: GripKey, size: f32, coarse: bool, w: f32, h: f32) -> f32 {
    let step = if coarse { KEY_STEP_COARSE } else { KEY_STEP };
    let bottom = dock == PreviewDock::Bottom;
    let held = |v: f32| {
        if bottom {
            clamp_pane_height(v, h)
        } else {
            clamp_preview_width(v, w)
        }
    };
    match key {
        GripKey::Grow => held(size + step),
        GripKey::Shrink => held(size - step),
        GripKey::ToMin => {
            if bottom {
                PANE_COLLAPSED
            } else {
                PREVIEW_WIDTH_MIN
            }
        }
        GripKey::ToMax => {
            if bottom {
                pane_max(h)
            } else {
                clamp_preview_width(f32::MAX, w)
            }
        }
    }
}

#[cfg(test)]
#[path = "sidebar_layout_test.rs"]
mod sidebar_layout_test;
