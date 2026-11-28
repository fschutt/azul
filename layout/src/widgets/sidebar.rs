//! The standard Sidebar: ONE left column every heavy app shares (Mail's folders, a drive's trees,
//! a document's outline, Paint's tools), so no app builds its own.
//!
//! ```text
//!  expanded                 rail (the collapsed state)     handle
//! ┌──────────────────┐      ┌────┐                          ┌┐
//! │ ● ● ●            │      │ ●●●│  window controls         ││
//! │ Favourites       │      │    │                          ││
//! │ ▸ Inbox      329 │      │ ✉  │  one button per row      │▌│  16 px, grab handle
//! │ ▾ Archive        │      │ ★  │  with an icon, the       ││
//! │   2026         4 │      │ ▤  │  label is a tooltip      ││
//! │ ──────────────── │      │ ── │                          └┘
//! │ Help             │      │ ?  │
//! │ ◔ Sync · 82 %    │      │ ◔  │  status pies, a click
//! │ ◔ Drive · 64 %   │      │ ◔  │  opens a short popover
//! │ « Collapse       │      │ »  │  the toggle, always last
//! └──────────────────┘      └────┘
//! ```
//!
//! The sidebar is the FULL height of the window: it runs beside the tab row and the ribbon, down
//! to the footer, and on a phone its expanded state is a DRAWER that pushes the whole main view
//! aside (`AppFrame`), never an overlay with a dim. It has three states ([`SidebarState`]): the
//! icon RAIL is the collapsed one and the one an app starts in; the 16 px HANDLE is supported and
//! never part of the toggle's round unless the app asks for it.
//!
//! # What the app gives it
//!
//! - SECTIONS ([`SidebarSection`]) of ROWS ([`SidebarRow`]): an icon (a Material name, optionally
//!   tinted), a label, a count, a disclosure triangle for a tree (the app flattens its tree: a
//!   row says its depth, whether it has children and whether they show), a selected row drawn as
//!   a rounded highlight (macOS Mail's), an optional check mark (a calendar's coloured boxes).
//!   Rows with an icon are the rail's buttons; a row opts out with `with_in_rail(false)`.
//! - DOCUMENT ROWS (AzWriter, AzSheets, AzShow list their open documents here): a row may carry
//!   its own sync PIE and a COLLABORATION icon (off / on, a click asks to switch it,
//!   [`SidebarCollab`]); in the rail the document keeps its icon and a small pie at its corner.
//!   UI only: what sync and collaboration do is the app's, nothing behind them is in the widget.
//! - STATUS PIES ([`SidebarStatus`]) stacked directly above the toggle: sync, drive full, battery.
//!   A click or tap opens a short popover (the same `Dialog` popover a desktop has today, on a
//!   phone too) with the app's own content or a title and a line of detail.
//! - A HELP row, a HEADER slot above the rows (an account, a "New" button), a `label` the
//!   screen reader names the landmark by.
//!
//! # What it reports
//!
//! Every action goes through ONE callback ([`SidebarOnEvent`]) with a [`SidebarEvent`]; the
//! sidebar owns no state: the app keeps the state, the selection, which rows are expanded, and
//! rebuilds. The window controls of the OS (macOS's traffic lights) sit over the sidebar's top:
//! `with_window_controls` keeps their band and their width free (`sidebar_layout.rs`).
//!
//! # The surface
//!
//! It takes the app's surface: by default the theme's sidebar ground (`system:sidebar-background`:
//! flora's leaf, the desktop's sidebar material), or one of the built-in materials the apps have
//! (leather, vellum, blue jeans, aluminium: [`SidebarSurface`]), or nothing at all
//! ([`SidebarSurface::Inherit`]) where the app paints one surface under everything.
//!
//! Key types: [`Sidebar`], [`SidebarSection`], [`SidebarRow`], [`SidebarStatus`],
//! [`SidebarEvent`], [`SidebarOnEvent`], [`SidebarState`], [`SidebarSurface`].

use alloc::{format, string::String, vec::Vec};
use core::fmt::Write as _;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, OptionDom, TabIndex},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    transient::TransientAnchor,
    window::VirtualKeyCode,
};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    props::basic::color::{ColorU, OptionColorU},
    AzString,
};

pub use super::sidebar_layout::{FormFactor, SidebarState};
use super::{
    dialog::{Dialog, DialogClosedBy},
    pie_marker::{disc_dom, PieTone},
    roving,
    sidebar_layout::{
        column_width, controls_height, swipe_opens, SidebarColumnWidth,
        INDENT_PER_DEPTH, MAX_DEPTH, MAX_QUICK_ACTIONS, MAX_ROWS, MAX_STATUSES,
        RAIL_BUTTON_DESKTOP, RAIL_BUTTON_PHONE, RAIL_ICON_PX, RAIL_SLOP, RAIL_STATUS_PX,
        RAIL_WIDTH, ROW_HEIGHT_DESKTOP, ROW_HEIGHT_PHONE, STATUS_PIE_PX,
    },
    themes::{OptionUiTheme, UiTheme},
    widget_p_with_text,
};
use crate::callbacks::CallbackInfo;

/// The sidebar's root.
pub const SIDEBAR_CLASS: &str = "__azul-native-sidebar";
/// Added to the root in the expanded state.
pub const SIDEBAR_EXPANDED_CLASS: &str = "__azul-native-sidebar-expanded";
/// Added to the root in the rail state.
pub const SIDEBAR_RAIL_CLASS: &str = "__azul-native-sidebar-rail";
/// Added to the root in the 16 px handle state.
pub const SIDEBAR_HANDLE_CLASS: &str = "__azul-native-sidebar-handle";
/// Added to the root on a phone.
pub const SIDEBAR_PHONE_CLASS: &str = "__azul-native-sidebar-phone";
/// The band at the top the OS's window controls sit in (it moves the window).
pub const SIDEBAR_CONTROLS_CLASS: &str = "__azul-native-sidebar-controls";
/// The scrolling column of sections and rows (or, in the rail, of navigation buttons).
pub const SIDEBAR_LIST_CLASS: &str = "__azul-native-sidebar-list";
/// The rail's quick-access group: the app's own buttons, anchored at the bottom.
pub const SIDEBAR_QUICK_CLASS: &str = "__azul-native-sidebar-quick";
/// A quick-access button of the rail.
pub const SIDEBAR_QUICK_BUTTON_CLASS: &str = "__azul-native-sidebar-quick-button";
/// The invisible hit slop a button of the rail reaches inward over the main view.
pub const SIDEBAR_SLOP_CLASS: &str = "__azul-native-sidebar-slop";
/// A section's title row.
pub const SIDEBAR_SECTION_CLASS: &str = "__azul-native-sidebar-section";
/// A row of the expanded sidebar.
pub const SIDEBAR_ROW_CLASS: &str = "__azul-native-sidebar-row";
/// Added to the selected row (and to the selected button of the rail).
pub const SIDEBAR_SELECTED_CLASS: &str = "__azul-native-sidebar-selected";
/// A row's disclosure triangle.
pub const SIDEBAR_DISCLOSURE_CLASS: &str = "__azul-native-sidebar-disclosure";
/// A row's count.
pub const SIDEBAR_BADGE_CLASS: &str = "__azul-native-sidebar-badge";
/// A document row's own pie (its sync state).
pub const SIDEBAR_ROW_PIE_CLASS: &str = "__azul-native-sidebar-row-pie";
/// A document row's collaboration icon.
pub const SIDEBAR_COLLAB_CLASS: &str = "__azul-native-sidebar-collab";
/// A row's state glyph at its end.
pub const SIDEBAR_STATUS_ICON_CLASS: &str = "__azul-native-sidebar-status-icon";
/// A button of the rail.
pub const SIDEBAR_BUTTON_CLASS: &str = "__azul-native-sidebar-button";
/// The hairline between the sections of the rail and over the foot.
pub const SIDEBAR_RULE_CLASS: &str = "__azul-native-sidebar-rule";
/// The foot: help, the status pies and the toggle.
pub const SIDEBAR_FOOT_CLASS: &str = "__azul-native-sidebar-foot";
/// A status indicator, a pie or a badge (its invoker; in the rail a button, expanded a row).
pub const SIDEBAR_STATUS_CLASS: &str = "__azul-native-sidebar-status";
/// A status indicator that is a badge.
pub const SIDEBAR_STATUS_BADGE_CLASS: &str = "__azul-native-sidebar-status-badge";
/// The status popover's content.
pub const SIDEBAR_POPOVER_CLASS: &str = "__azul-native-sidebar-popover";
/// The help button.
pub const SIDEBAR_HELP_CLASS: &str = "__azul-native-sidebar-help";
/// The toggle between the rail and the expanded sidebar.
pub const SIDEBAR_TOGGLE_CLASS: &str = "__azul-native-sidebar-toggle";
/// The handle's grab pill.
pub const SIDEBAR_GRAB_CLASS: &str = "__azul-native-sidebar-grab";

/// The pie of a document row, and the small one at the corner of a document's rail button, px.
const ROW_PIE_PX: f32 = 14.0;
const RAIL_PIE_PX: f32 = 8.0;
/// What a finger or a pointer must have moved along the rail, px, before a press on it is
/// taken for a swipe (the pointer is then captured, and a click no longer follows).
const SWIPE_CAPTURE_PX: f32 = 6.0;

// ---------------------------------------------------------------------------
// The surface
// ---------------------------------------------------------------------------

/// What the sidebar is painted with.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SidebarSurface {
    /// The theme's sidebar ground: `system:sidebar-background` (flora's leaf, the desktop's sidebar
    /// material), with the theme's selection and ink.
    #[default]
    Theme,
    /// Nothing: the app paints one surface under the whole window (a leather window) and the
    /// sidebar lets it show.
    Inherit,
    /// The leather the tab strip lies on.
    Leather,
    /// Blue jeans.
    BlueJeans,
    /// Brushed aluminium.
    Aluminium,
    /// Vellum, a page of parchment.
    Vellum,
}

/// How a surface paints the parts of the sidebar: the declarations of the root, the ink of a
/// row, the wash under the pointer and the one under the selected row.
struct SurfaceLook {
    /// Root declarations (background, ink).
    root: &'static str,
    /// The wash of a row under the pointer.
    hover: &'static str,
    /// The selected row's fill.
    selected: &'static str,
    /// The secondary ink: section titles, counts, icons at rest.
    quiet: &'static str,
}

/// The leather's tint is the tab strip's: pale grey nappa by day, mixed toward black at night.
static LOOK_THEME: SurfaceLook = SurfaceLook {
    root: "background: system:sidebar-background; color: system:text;",
    hover: "rgba(127, 127, 127, 0.16)",
    selected: "system:sidebar-selection",
    quiet: "system:secondary-text",
};
static LOOK_INHERIT: SurfaceLook = SurfaceLook {
    root: "color: system:text;",
    hover: "rgba(127, 127, 127, 0.16)",
    selected: "rgba(127, 127, 127, 0.28)",
    quiet: "system:secondary-text",
};
static LOOK_LEATHER: SurfaceLook = SurfaceLook {
    root: "background: builtin(leather); color: system:text; --azul-material-tint: #e4e2dd; \
           @theme(dark) { --azul-material-tint: #373635; }",
    hover: "rgba(127, 127, 127, 0.18)",
    selected: "rgba(127, 127, 127, 0.30)",
    quiet: "system:secondary-text",
};
static LOOK_JEANS: SurfaceLook = SurfaceLook {
    root: "background: builtin(bluejeans); color: #eef2fa;",
    hover: "rgba(255, 255, 255, 0.14)",
    selected: "rgba(255, 255, 255, 0.26)",
    quiet: "#c3cde3",
};
static LOOK_ALUMINIUM: SurfaceLook = SurfaceLook {
    root: "background: builtin(aluminium); color: #1d1f22;",
    hover: "rgba(0, 0, 0, 0.10)",
    selected: "rgba(0, 0, 0, 0.20)",
    quiet: "#4a4f57",
};
static LOOK_VELLUM: SurfaceLook = SurfaceLook {
    root: "background: builtin(vellum-leaf); color: #2b2a26;",
    hover: "rgba(60, 50, 30, 0.10)",
    selected: "rgba(60, 50, 30, 0.20)",
    quiet: "#6b6657",
};

impl SidebarSurface {
    /// The declarations that paint a pane in this surface: its ground and its ink (what the
    /// Preview's pane wears too).
    pub(crate) fn root_css(self) -> &'static str {
        self.look().root
    }

    fn look(self) -> &'static SurfaceLook {
        match self {
            Self::Theme => &LOOK_THEME,
            Self::Inherit => &LOOK_INHERIT,
            Self::Leather => &LOOK_LEATHER,
            Self::BlueJeans => &LOOK_JEANS,
            Self::Aluminium => &LOOK_ALUMINIUM,
            Self::Vellum => &LOOK_VELLUM,
        }
    }
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

/// A row's mark: a check box in front of the icon (a calendar's coloured boxes).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SidebarRowMark {
    /// No mark.
    #[default]
    None,
    /// An empty box (the calendar is hidden).
    Unchecked,
    /// A ticked box (the calendar shows).
    Checked,
}

/// A document row's collaboration switch (`AzWriter`, `AzSheets`, `AzShow` list the open documents in
/// the sidebar): UI only, the sync kit behind it is not part of the widget.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SidebarCollab {
    /// The row has no collaboration icon.
    #[default]
    None,
    /// Collaboration is off: the icon invites ("activate collaboration").
    Off,
    /// Collaboration is on: the icon is lit.
    On,
}

/// One row of the sidebar: a destination, a folder of a tree, a tool, an open document.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct SidebarRow {
    /// The label ("Inbox").
    pub label: AzString,
    /// The icon, a Material name (`Dom::create_icon`); empty for none. A row with an icon is a
    /// button of the rail.
    pub icon: AzString,
    /// The count after the label ("329"); empty for none.
    pub badge: AzString,
    /// What the rail's button says under the pointer and on a long press; empty: the label.
    pub tooltip: AzString,
    /// A glyph at the row's end that says the row's own state (a cloud drive's: syncing, locked,
    /// connected; a Material name); empty for none.
    pub status_icon: AzString,
    /// What the status glyph says: its accessible name.
    pub status_tip: AzString,
    /// What the app calls this row: handed back in every event about it.
    pub tag: usize,
    /// How deep a tree row is, 0 at the top (held to `MAX_DEPTH`).
    pub depth: usize,
    /// A check box before the icon.
    pub mark: SidebarRowMark,
    /// How far this row's own pie is, 0 to 1 (a document's sync state); a number below 0 (or no
    /// number) is no pie. APPENDED for ABI stability.
    pub pie_fraction: f32,
    /// What the row's pie means: its colour, and whether it breathes.
    pub pie_tone: PieTone,
    /// The row's collaboration icon: none, off or on.
    pub collab: SidebarCollab,
    /// A colour for the icon (Mail's blue, a calendar's colour); none: the sidebar's ink.
    pub tint: OptionColorU,
    /// Whether the row has children: it shows a disclosure triangle.
    pub has_children: bool,
    /// Whether the children show (the triangle points down).
    pub expanded: bool,
    /// Whether this is the row the main view shows: a rounded highlight.
    pub selected: bool,
    /// Whether the row is a button of the rail (a row with an icon is by default).
    pub in_rail: bool,
    /// Whether items dragged from elsewhere in the window can be dropped on the row: it accepts
    /// the drag and reports [`SidebarEventKind::RowDropped`].
    pub accepts_drops: bool,
}

impl_option!(SidebarRow, OptionSidebarRow, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    SidebarRow,
    SidebarRowVec,
    SidebarRowVecDestructor,
    SidebarRowVecDestructorType,
    SidebarRowVecSlice,
    OptionSidebarRow
);
impl_vec_clone!(SidebarRow, SidebarRowVec, SidebarRowVecDestructor);
impl_vec_debug!(SidebarRow, SidebarRowVec);
impl_vec_partialeq!(SidebarRow, SidebarRowVec);
impl_vec_mut!(SidebarRow, SidebarRowVec);

impl SidebarRow {
    /// A row labelled `label`: no icon, no count, at the top level, not selected.
    #[must_use]
    pub const fn create(label: AzString) -> Self {
        Self {
            label,
            icon: AzString::from_const_str(""),
            tint: OptionColorU::None,
            badge: AzString::from_const_str(""),
            tag: 0,
            depth: 0,
            has_children: false,
            expanded: false,
            selected: false,
            mark: SidebarRowMark::None,
            in_rail: true,
            tooltip: AzString::from_const_str(""),
            pie_fraction: -1.0,
            pie_tone: PieTone::Accent,
            collab: SidebarCollab::None,
            status_icon: AzString::from_const_str(""),
            status_tip: AzString::from_const_str(""),
            accepts_drops: false,
        }
    }

    /// The icon, a Material name.
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// [`Self::set_icon`] for the builder chain.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// A colour for the icon.
    pub const fn set_tint(&mut self, tint: ColorU) {
        self.tint = OptionColorU::Some(tint);
    }

    /// [`Self::set_tint`] for the builder chain.
    #[must_use]
    pub const fn with_tint(mut self, tint: ColorU) -> Self {
        self.set_tint(tint);
        self
    }

    /// The count after the label.
    pub fn set_badge(&mut self, badge: AzString) {
        self.badge = badge;
    }

    /// [`Self::set_badge`] for the builder chain.
    #[must_use]
    pub fn with_badge(mut self, badge: AzString) -> Self {
        self.set_badge(badge);
        self
    }

    /// What the app calls this row.
    pub const fn set_tag(&mut self, tag: usize) {
        self.tag = tag;
    }

    /// [`Self::set_tag`] for the builder chain.
    #[must_use]
    pub const fn with_tag(mut self, tag: usize) -> Self {
        self.set_tag(tag);
        self
    }

    /// How deep a tree row is.
    pub const fn set_depth(&mut self, depth: usize) {
        self.depth = depth;
    }

    /// [`Self::set_depth`] for the builder chain.
    #[must_use]
    pub const fn with_depth(mut self, depth: usize) -> Self {
        self.set_depth(depth);
        self
    }

    /// The row has children (a disclosure triangle), shown or not.
    pub const fn set_disclosure(&mut self, expanded: bool) {
        self.has_children = true;
        self.expanded = expanded;
    }

    /// [`Self::set_disclosure`] for the builder chain.
    #[must_use]
    pub const fn with_disclosure(mut self, expanded: bool) -> Self {
        self.set_disclosure(expanded);
        self
    }

    /// Whether this row is the selected one.
    pub const fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    /// [`Self::set_selected`] for the builder chain.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.set_selected(selected);
        self
    }

    /// A check box before the icon.
    pub const fn set_mark(&mut self, mark: SidebarRowMark) {
        self.mark = mark;
    }

    /// [`Self::set_mark`] for the builder chain.
    #[must_use]
    pub const fn with_mark(mut self, mark: SidebarRowMark) -> Self {
        self.set_mark(mark);
        self
    }

    /// Whether the row is a button of the rail.
    pub const fn set_in_rail(&mut self, in_rail: bool) {
        self.in_rail = in_rail;
    }

    /// [`Self::set_in_rail`] for the builder chain.
    #[must_use]
    pub const fn with_in_rail(mut self, in_rail: bool) -> Self {
        self.set_in_rail(in_rail);
        self
    }

    /// What the rail's button says under the pointer.
    pub fn set_tooltip(&mut self, tooltip: AzString) {
        self.tooltip = tooltip;
    }

    /// [`Self::set_tooltip`] for the builder chain.
    #[must_use]
    pub fn with_tooltip(mut self, tooltip: AzString) -> Self {
        self.set_tooltip(tooltip);
        self
    }

    /// The row's own pie: how far its sync has come, and what that means.
    pub const fn set_pie(&mut self, fraction: f32, tone: PieTone) {
        self.pie_fraction = fraction;
        self.pie_tone = tone;
    }

    /// [`Self::set_pie`] for the builder chain.
    #[must_use]
    pub const fn with_pie(mut self, fraction: f32, tone: PieTone) -> Self {
        self.set_pie(fraction, tone);
        self
    }

    /// The row's collaboration icon.
    pub const fn set_collab(&mut self, collab: SidebarCollab) {
        self.collab = collab;
    }

    /// [`Self::set_collab`] for the builder chain.
    #[must_use]
    pub const fn with_collab(mut self, collab: SidebarCollab) -> Self {
        self.set_collab(collab);
        self
    }

    /// A glyph at the row's end that says its state, with what it says.
    pub fn set_status(&mut self, icon: AzString, tip: AzString) {
        self.status_icon = icon;
        self.status_tip = tip;
    }

    /// [`Self::set_status`] for the builder chain.
    #[must_use]
    pub fn with_status(mut self, icon: AzString, tip: AzString) -> Self {
        self.set_status(icon, tip);
        self
    }

    /// Whether items dragged over the row can be dropped on it.
    pub const fn set_accepts_drops(&mut self, accepts: bool) {
        self.accepts_drops = accepts;
    }

    /// [`Self::set_accepts_drops`] for the builder chain.
    #[must_use]
    pub const fn with_accepts_drops(mut self, accepts: bool) -> Self {
        self.set_accepts_drops(accepts);
        self
    }

    /// Whether the row has its own pie.
    #[must_use]
    pub fn has_pie(&self) -> bool {
        self.pie_fraction.is_finite() && self.pie_fraction >= 0.0
    }

    /// Whether this row is a button of the rail: a row with an icon that did not opt out, at the
    /// top level of its tree.
    #[must_use]
    pub fn is_rail_button(&self) -> bool {
        self.in_rail && self.depth == 0 && !self.icon.as_str().is_empty()
    }

    /// What the rail's button says: its tooltip, or its label.
    #[must_use]
    pub fn rail_name(&self) -> &str {
        if self.tooltip.as_str().is_empty() {
            self.label.as_str()
        } else {
            self.tooltip.as_str()
        }
    }
}

/// A section: a small title over its rows.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct SidebarSection {
    /// The title ("Favourites"); empty for a section without a title (its rows stand under the
    /// hairline of the one before).
    pub title: AzString,
    /// The rows.
    pub rows: SidebarRowVec,
    /// Whether the title hides and shows the rows (macOS Mail's "Hide").
    pub collapsible: bool,
    /// Whether a collapsible section shows its rows.
    pub open: bool,
}

impl_option!(SidebarSection, OptionSidebarSection, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    SidebarSection,
    SidebarSectionVec,
    SidebarSectionVecDestructor,
    SidebarSectionVecDestructorType,
    SidebarSectionVecSlice,
    OptionSidebarSection
);
impl_vec_clone!(SidebarSection, SidebarSectionVec, SidebarSectionVecDestructor);
impl_vec_debug!(SidebarSection, SidebarSectionVec);
impl_vec_partialeq!(SidebarSection, SidebarSectionVec);
impl_vec_mut!(SidebarSection, SidebarSectionVec);

impl SidebarSection {
    /// An open section `title` with no rows.
    #[must_use]
    pub const fn create(title: AzString) -> Self {
        Self {
            title,
            rows: SidebarRowVec::from_const_slice(&[]),
            collapsible: false,
            open: true,
        }
    }

    /// Appends a row.
    pub fn add_row(&mut self, row: SidebarRow) {
        let mut v = self.rows.clone().into_library_owned_vec();
        v.push(row);
        self.rows = SidebarRowVec::from_vec(v);
    }

    /// [`Self::add_row`] for the builder chain.
    #[must_use]
    pub fn with_row(mut self, row: SidebarRow) -> Self {
        self.add_row(row);
        self
    }

    /// Replaces the rows.
    pub fn set_rows(&mut self, rows: SidebarRowVec) {
        self.rows = rows;
    }

    /// [`Self::set_rows`] for the builder chain.
    #[must_use]
    pub fn with_rows(mut self, rows: SidebarRowVec) -> Self {
        self.set_rows(rows);
        self
    }

    /// The title hides and shows the rows; `open` says whether they show.
    pub const fn set_collapsible(&mut self, open: bool) {
        self.collapsible = true;
        self.open = open;
    }

    /// [`Self::set_collapsible`] for the builder chain.
    #[must_use]
    pub const fn with_collapsible(mut self, open: bool) -> Self {
        self.set_collapsible(open);
        self
    }
}

/// What a status indicator looks like.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SidebarStatusShape {
    /// A pie: how far something is (a sync, a drive's fullness, a battery).
    #[default]
    Pie,
    /// A badge: a few characters in a pill of the tone's colour (a count of unread, "!"); no text
    /// is a dot.
    Badge,
}

/// A status indicator in the slot above the toggle: a pie (how far a sync has come, how full a
/// drive is) or a badge. Which indicators appear is the app's: the widget has none of its own.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SidebarStatus {
    /// What it is ("Sync"): the accessible name, the popover's title when it has no content of its
    /// own, and the label of the expanded row.
    pub label: AzString,
    /// A short text beside the pie in the expanded sidebar ("82 %", "3 waiting"); for a badge the
    /// text IN the badge (the first three characters are drawn). Empty for none.
    pub text: AzString,
    /// A line of detail under the title in the default popover; empty for none.
    pub detail: AzString,
    /// The popover's content when it is more than a title and a line: the app's own `Dom`.
    pub popover: OptionDom,
    /// How far, 0 to 1: the pie's wedge (a badge ignores it).
    pub fraction: f32,
    /// What the number means: the colour, and whether a pie breathes.
    pub tone: PieTone,
    /// A pie or a badge.
    pub shape: SidebarStatusShape,
}

impl_option!(SidebarStatus, OptionSidebarStatus, copy = false, [Debug, Clone]);
impl_vec!(
    SidebarStatus,
    SidebarStatusVec,
    SidebarStatusVecDestructor,
    SidebarStatusVecDestructorType,
    SidebarStatusVecSlice,
    OptionSidebarStatus
);
impl_vec_clone!(SidebarStatus, SidebarStatusVec, SidebarStatusVecDestructor);
impl_vec_debug!(SidebarStatus, SidebarStatusVec);
impl_vec_mut!(SidebarStatus, SidebarStatusVec);

impl SidebarStatus {
    /// A pie `fraction` full in `tone`, named `label`, with no text and the default popover.
    #[must_use]
    pub const fn create(label: AzString, fraction: f32, tone: PieTone) -> Self {
        Self {
            label,
            text: AzString::from_const_str(""),
            detail: AzString::from_const_str(""),
            popover: OptionDom::None,
            fraction,
            tone,
            shape: SidebarStatusShape::Pie,
        }
    }

    /// A badge of `text` in `tone`, named `label`, with the default popover.
    #[must_use]
    pub const fn badge(label: AzString, text: AzString, tone: PieTone) -> Self {
        Self {
            label,
            text,
            detail: AzString::from_const_str(""),
            popover: OptionDom::None,
            fraction: 0.0,
            tone,
            shape: SidebarStatusShape::Badge,
        }
    }

    /// The short text beside the pie of the expanded sidebar (the text of a badge).
    pub fn set_text(&mut self, text: AzString) {
        self.text = text;
    }

    /// [`Self::set_text`] for the builder chain.
    #[must_use]
    pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// A line of detail for the default popover.
    pub fn set_detail(&mut self, detail: AzString) {
        self.detail = detail;
    }

    /// [`Self::set_detail`] for the builder chain.
    #[must_use]
    pub fn with_detail(mut self, detail: AzString) -> Self {
        self.set_detail(detail);
        self
    }

    /// The popover's own content.
    pub fn set_popover(&mut self, content: Dom) {
        self.popover = OptionDom::Some(content);
    }

    /// [`Self::set_popover`] for the builder chain.
    #[must_use]
    pub fn with_popover(mut self, content: Dom) -> Self {
        self.set_popover(content);
        self
    }

    /// A pie or a badge.
    pub const fn set_shape(&mut self, shape: SidebarStatusShape) {
        self.shape = shape;
    }

    /// [`Self::set_shape`] for the builder chain.
    #[must_use]
    pub const fn with_shape(mut self, shape: SidebarStatusShape) -> Self {
        self.set_shape(shape);
        self
    }

    /// What a screen reader says: the label and the text.
    fn spoken(&self) -> String {
        if self.text.as_str().is_empty() {
            String::from(self.label.as_str())
        } else {
            format!("{}, {}", self.label.as_str(), self.text.as_str())
        }
    }
}

/// What a quick-access button is doing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum SidebarQuickState {
    /// A plain button (Undo, a colour picker): it does something when pressed and has no state.
    #[default]
    Plain,
    /// A toggle that is off (Bold).
    Off,
    /// A toggle that is on: drawn pressed.
    On,
    /// Cannot run now: dimmed, and a press reports nothing.
    Disabled,
}

/// One quick-access button of the collapsed rail: what an app puts where the thumb of a
/// right-handed user holding the phone upright reaches (`AzWriter`: font colour, bold, highlight,
/// size, undo). The rail draws them at the bottom, above the status slot; the LAST one is the
/// nearest to the thumb, so the app puts the most used button last. An expanded sidebar does not
/// show them (the app's rows and its ribbon have the same commands).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct SidebarQuickAction {
    /// What it is called: the tooltip and the screen reader's name ("Bold").
    pub label: AzString,
    /// The icon, a Material name (`Dom::create_icon`).
    pub icon: AzString,
    /// What the app calls this action: handed back in the event.
    pub tag: usize,
    /// A toggle's state, or `Plain`, or `Disabled`.
    pub state: SidebarQuickState,
    /// A colour bar under the icon (the current font colour, the highlight colour); none for no
    /// bar.
    pub swatch: OptionColorU,
}

impl_option!(SidebarQuickAction, OptionSidebarQuickAction, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    SidebarQuickAction,
    SidebarQuickActionVec,
    SidebarQuickActionVecDestructor,
    SidebarQuickActionVecDestructorType,
    SidebarQuickActionVecSlice,
    OptionSidebarQuickAction
);
impl_vec_clone!(SidebarQuickAction, SidebarQuickActionVec, SidebarQuickActionVecDestructor);
impl_vec_debug!(SidebarQuickAction, SidebarQuickActionVec);
impl_vec_partialeq!(SidebarQuickAction, SidebarQuickActionVec);
impl_vec_mut!(SidebarQuickAction, SidebarQuickActionVec);

impl SidebarQuickAction {
    /// A plain button named `label` with the icon `icon`.
    #[must_use]
    pub const fn create(label: AzString, icon: AzString) -> Self {
        Self {
            label,
            icon,
            tag: 0,
            state: SidebarQuickState::Plain,
            swatch: OptionColorU::None,
        }
    }

    /// What the app calls this action.
    pub const fn set_tag(&mut self, tag: usize) {
        self.tag = tag;
    }

    /// [`Self::set_tag`] for the builder chain.
    #[must_use]
    pub const fn with_tag(mut self, tag: usize) -> Self {
        self.set_tag(tag);
        self
    }

    /// A toggle's state, `Plain` or `Disabled`.
    pub const fn set_state(&mut self, state: SidebarQuickState) {
        self.state = state;
    }

    /// [`Self::set_state`] for the builder chain.
    #[must_use]
    pub const fn with_state(mut self, state: SidebarQuickState) -> Self {
        self.set_state(state);
        self
    }

    /// The colour bar under the icon.
    pub const fn set_swatch(&mut self, swatch: ColorU) {
        self.swatch = OptionColorU::Some(swatch);
    }

    /// [`Self::set_swatch`] for the builder chain.
    #[must_use]
    pub const fn with_swatch(mut self, swatch: ColorU) -> Self {
        self.set_swatch(swatch);
        self
    }
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

/// What happened on the sidebar.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SidebarEventKind {
    /// A row (or the rail's button of a row) was chosen: `index` is its place in the rows as the
    /// app gave them (every section in order, a closed section's rows counted), `tag` its tag.
    RowClicked,
    /// A disclosure triangle (or Left / Right on a row): `index` and `tag` name the row, `expand`
    /// says whether the children should show now.
    RowToggled,
    /// A collapsible section's title: `index` is the section, `expand` whether it should open.
    SectionToggled,
    /// The toggle, the handle or the pushed main view (on a phone): `state` is the state the
    /// sidebar asks to be in.
    StateChanged,
    /// The help row.
    HelpClicked,
    /// A row was pressed and held (a phone's long press: the app opens its quick-action sheet).
    RowLongPressed,
    /// A row's own pie was clicked (a document's sync state): the app shows what it says.
    RowStatusClicked,
    /// A row's collaboration icon: `index` and `tag` name the row, `expand` says whether
    /// collaboration should be on now.
    RowCollabToggled,
    /// A quick-access button of the rail: `index` is its place in the list the app gave, `tag` its
    /// tag, `expand` says whether a toggle should be on now (a plain button: false).
    QuickAction,
    /// A right click on a row (or its rail button): the app opens the row's menu there
    /// (`CallbackInfo::open_menu_for_hit_node`).
    RowMenuRequested,
    /// Items dragged in the window were dropped on a row that accepts drops
    /// ([`SidebarRow::accepts_drops`]); `CallbackInfo::get_key_modifiers` says copy or move.
    RowDropped,
}

/// One report of the sidebar.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct SidebarEvent {
    /// What happened.
    pub kind: SidebarEventKind,
    /// The row's place (`RowClicked`, `RowToggled`, `RowLongPressed`) or the section's
    /// (`SectionToggled`); 0 otherwise.
    pub index: usize,
    /// The row's tag; 0 otherwise.
    pub tag: usize,
    /// The new open state (`RowToggled`, `SectionToggled`), the state a toggle is asked to take
    /// (`RowCollabToggled`, `QuickAction`); false otherwise.
    pub expand: bool,
    /// The state asked for (`StateChanged`); the current one otherwise.
    pub state: SidebarState,
}

/// Callback invoked for every action on the sidebar.
pub type SidebarOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, SidebarEvent) -> Update;
impl_widget_callback!(
    SidebarOnEvent,
    OptionSidebarOnEvent,
    SidebarOnEventCallback,
    SidebarOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SidebarOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SIDEBAR_ON_EVENT_INVOKER,
    invoker_ty:     AzSidebarOnEventCallbackInvoker,
    thunk_fn:       az_sidebar_on_event_callback_thunk,
    setter_fn:      AzApp_setSidebarOnEventCallbackInvoker,
    from_handle_fn: AzSidebarOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzSidebarOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: SidebarEvent ],
}

// ---------------------------------------------------------------------------
// The widget
// ---------------------------------------------------------------------------

/// The standard left sidebar: sections of rows, a rail of quick-access buttons and status
/// indicators as its collapsed state.
///
/// The fields stand in the order of their alignment (the ABI check wants no padding between them).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Sidebar {
    /// The slot above the rows (an account, a "New" button); in the rail it is dropped.
    pub header: OptionDom,
    /// Every action on the sidebar.
    pub on_event: OptionSidebarOnEvent,
    /// The sections, top to bottom.
    pub sections: SidebarSectionVec,
    /// The status indicators above the toggle (held to `MAX_STATUSES`): the app's data, the widget
    /// has none of its own.
    pub statuses: SidebarStatusVec,
    /// The quick-access buttons of the collapsed rail (held to `MAX_QUICK_ACTIONS`), drawn at the
    /// bottom, the last one nearest to the thumb.
    pub quick_access: SidebarQuickActionVec,
    /// The landmark's accessible name.
    pub label: AzString,
    /// The help row's label; empty for no help row.
    pub help: AzString,
    /// The widget theme this sidebar is PINNED to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// The state the app keeps and rebuilds with.
    pub state: SidebarState,
    /// What the sidebar is painted with.
    pub surface: SidebarSurface,
    /// Desktop or phone: `Auto` asks the window.
    pub form: FormFactor,
    /// The expanded width on a desktop, px; 0 is the default (a phone's drawer is a share of the
    /// frame).
    pub expanded_width: f32,
    /// How wide the OS's window controls are at the window's left edge (macOS's traffic lights,
    /// 78 px); 0 where they are not there.
    pub controls_left: f32,
    /// How tall the band they sit in is (the title bar's height).
    pub controls_height: f32,
    /// The width of the frame the sidebar stands in, px; 0 when the app does not say. With it a
    /// phone's drawer has a width in px (a share of the frame up to its cap) like the rail and the
    /// handle, so the column's width can be TWEENED between the states (the sidebar slides open
    /// and shut); without it the drawer is a percent share and the column jumps.
    pub frame_width: f32,
    /// Whether the toggle's round includes the 16 px handle.
    pub handle_in_cycle: bool,
}

impl_option!(Sidebar, OptionSidebar, copy = false, [Debug, Clone]);

impl Sidebar {
    /// An empty sidebar in the rail state.
    #[must_use]
    pub const fn create() -> Self {
        Self {
            header: OptionDom::None,
            on_event: OptionSidebarOnEvent::None,
            sections: SidebarSectionVec::from_const_slice(&[]),
            statuses: SidebarStatusVec::from_const_slice(&[]),
            quick_access: SidebarQuickActionVec::from_const_slice(&[]),
            label: AzString::from_const_str("Sidebar"),
            help: AzString::from_const_str(""),
            theme: OptionUiTheme::None,
            state: SidebarState::Rail,
            surface: SidebarSurface::Theme,
            form: FormFactor::Auto,
            expanded_width: 0.0,
            controls_left: 0.0,
            controls_height: 0.0,
            frame_width: 0.0,
            handle_in_cycle: false,
        }
    }

    /// Appends a section.
    pub fn add_section(&mut self, section: SidebarSection) {
        let mut v = self.sections.clone().into_library_owned_vec();
        v.push(section);
        self.sections = SidebarSectionVec::from_vec(v);
    }

    /// [`Self::add_section`] for the builder chain.
    #[must_use]
    pub fn with_section(mut self, section: SidebarSection) -> Self {
        self.add_section(section);
        self
    }

    /// Replaces the sections.
    pub fn set_sections(&mut self, sections: SidebarSectionVec) {
        self.sections = sections;
    }

    /// [`Self::set_sections`] for the builder chain.
    #[must_use]
    pub fn with_sections(mut self, sections: SidebarSectionVec) -> Self {
        self.set_sections(sections);
        self
    }

    /// Appends a status indicator (a pie or a badge).
    pub fn add_status(&mut self, status: SidebarStatus) {
        let mut v = self.statuses.clone().into_library_owned_vec();
        v.push(status);
        self.statuses = SidebarStatusVec::from_vec(v);
    }

    /// [`Self::add_status`] for the builder chain.
    #[must_use]
    pub fn with_status(mut self, status: SidebarStatus) -> Self {
        self.add_status(status);
        self
    }

    /// Replaces the quick-access buttons of the collapsed rail.
    pub fn set_quick_access(&mut self, quick_access: SidebarQuickActionVec) {
        self.quick_access = quick_access;
    }

    /// [`Self::set_quick_access`] for the builder chain.
    #[must_use]
    pub fn with_quick_access(mut self, quick_access: SidebarQuickActionVec) -> Self {
        self.set_quick_access(quick_access);
        self
    }

    /// Appends a quick-access button (the last one added is the nearest to the thumb).
    pub fn add_quick_action(&mut self, action: SidebarQuickAction) {
        let mut v = self.quick_access.clone().into_library_owned_vec();
        v.push(action);
        self.quick_access = SidebarQuickActionVec::from_vec(v);
    }

    /// [`Self::add_quick_action`] for the builder chain.
    #[must_use]
    pub fn with_quick_action(mut self, action: SidebarQuickAction) -> Self {
        self.add_quick_action(action);
        self
    }

    /// The state: expanded, the rail or the 16 px handle.
    pub const fn set_state(&mut self, state: SidebarState) {
        self.state = state;
    }

    /// [`Self::set_state`] for the builder chain.
    #[must_use]
    pub const fn with_state(mut self, state: SidebarState) -> Self {
        self.set_state(state);
        self
    }

    /// What the sidebar is painted with.
    pub const fn set_surface(&mut self, surface: SidebarSurface) {
        self.surface = surface;
    }

    /// [`Self::set_surface`] for the builder chain.
    #[must_use]
    pub const fn with_surface(mut self, surface: SidebarSurface) -> Self {
        self.set_surface(surface);
        self
    }

    /// Desktop or phone: `Auto` (the default) asks the window.
    pub const fn set_form(&mut self, form: FormFactor) {
        self.form = form;
    }

    /// [`Self::set_form`] for the builder chain.
    #[must_use]
    pub const fn with_form(mut self, form: FormFactor) -> Self {
        self.set_form(form);
        self
    }

    /// The landmark's accessible name.
    pub fn set_label(&mut self, label: AzString) {
        self.label = label;
    }

    /// [`Self::set_label`] for the builder chain.
    #[must_use]
    pub fn with_label(mut self, label: AzString) -> Self {
        self.set_label(label);
        self
    }

    /// The slot above the rows.
    pub fn set_header(&mut self, header: Dom) {
        self.header = OptionDom::Some(header);
    }

    /// [`Self::set_header`] for the builder chain.
    #[must_use]
    pub fn with_header(mut self, header: Dom) -> Self {
        self.set_header(header);
        self
    }

    /// A help row at the foot, named `label`.
    pub fn set_help(&mut self, label: AzString) {
        self.help = label;
    }

    /// [`Self::set_help`] for the builder chain.
    #[must_use]
    pub fn with_help(mut self, label: AzString) -> Self {
        self.set_help(label);
        self
    }

    /// The expanded width on a desktop, px (held to 160 .. 420; 0 is 240).
    pub const fn set_expanded_width(&mut self, width: f32) {
        self.expanded_width = width;
    }

    /// [`Self::set_expanded_width`] for the builder chain.
    #[must_use]
    pub const fn with_expanded_width(mut self, width: f32) -> Self {
        self.set_expanded_width(width);
        self
    }

    /// Keeps the room the OS's window controls need: a band `height` px tall at the top (it moves
    /// the window) and a column never narrower than `left` px (macOS's traffic lights: 78 and the
    /// title bar's height). `left` 0: the controls are not at the left, nothing is kept.
    pub const fn set_window_controls(&mut self, left: f32, height: f32) {
        self.controls_left = left;
        self.controls_height = height;
    }

    /// [`Self::set_window_controls`] for the builder chain.
    #[must_use]
    pub const fn with_window_controls(mut self, left: f32, height: f32) -> Self {
        self.set_window_controls(left, height);
        self
    }

    /// The width of the frame the sidebar stands in, px (what `LayoutCallbackInfo::get_window_width`
    /// says for an app whose window is the frame): the column then animates between its states.
    pub const fn set_frame_width(&mut self, width: f32) {
        self.frame_width = width;
    }

    /// [`Self::set_frame_width`] for the builder chain.
    #[must_use]
    pub const fn with_frame_width(mut self, width: f32) -> Self {
        self.set_frame_width(width);
        self
    }

    /// Whether the toggle's round is expanded, rail, handle, expanded.
    pub const fn set_handle_in_cycle(&mut self, handle: bool) {
        self.handle_in_cycle = handle;
    }

    /// [`Self::set_handle_in_cycle`] for the builder chain.
    #[must_use]
    pub const fn with_handle_in_cycle(mut self, handle: bool) -> Self {
        self.set_handle_in_cycle(handle);
        self
    }

    /// Pin the widget theme; unset, the sidebar follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Every action on the sidebar.
    pub fn set_on_event<C: Into<SidebarOnEventCallback>>(&mut self, data: RefAny, on_event: C) {
        self.on_event = OptionSidebarOnEvent::Some(SidebarOnEvent {
            refany: data,
            callback: on_event.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<SidebarOnEventCallback>>(mut self, data: RefAny, on_event: C) -> Self {
        self.set_on_event(data, on_event);
        self
    }

    /// Replaces `self` with an empty sidebar and returns the original.
    #[must_use]
    pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// How wide the column is in this state and form factor (what the frame lays the main view
    /// beside).
    #[must_use]
    pub fn column(&self) -> SidebarColumnWidth {
        column_width(self.state, self.form, self.expanded_width, self.controls_left)
    }

    /// How far the tab row keeps from the window's left edge to clear the OS's window controls
    /// that the sidebar does not cover: what the app hands the ribbon as `TabsInTitlebar::left`.
    /// The 18 px rail and the 16 px handle are narrower than the controls and cover little of them
    /// (macOS's traffic lights are 78 px: 60 px are left for the tab row to clear); an expanded
    /// sidebar is wider than they are: 0.
    #[must_use]
    pub fn header_left_pad(&self) -> f32 {
        super::sidebar_layout::header_left_pad(self.controls_left, self.column().px)
    }

    /// How far the content beside the sidebar starts from the window's left edge ON A PHONE, below
    /// the tab row: the rail's 18 px (the handle's 16). 0 with the drawer open (it pushes
    /// everything aside) and on a desktop (the sidebar is a column of the frame there, and the
    /// ribbon starts after it). What the app hands the ribbon's content as its inset.
    #[must_use]
    pub fn phone_content_inset(&self) -> f32 {
        if self.form.is_phone() && self.state != SidebarState::Expanded {
            self.column().px
        } else {
            0.0
        }
    }

    /// The state the toggle asks for from the current one.
    #[must_use]
    pub const fn toggled_state(&self) -> SidebarState {
        self.state.toggled(self.handle_in_cycle)
    }

    /// The sidebar's DOM: the column, `height: 100%`, as wide as [`Self::column`] says.
    #[must_use]
    pub fn dom(self) -> Dom {
        build(self)
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::create()
    }
}

impl From<Sidebar> for Dom {
    fn from(s: Sidebar) -> Self {
        s.dom()
    }
}

// ---------------------------------------------------------------------------
// The report's carriers
// ---------------------------------------------------------------------------

/// What a part carries: the app's callback and the event the part reports.
struct PartRef {
    on_event: OptionSidebarOnEvent,
    event: SidebarEvent,
}

const fn event(kind: SidebarEventKind, state: SidebarState) -> SidebarEvent {
    SidebarEvent {
        kind,
        index: 0,
        tag: 0,
        expand: false,
        state,
    }
}

/// Hands the part's event to the app.
fn report(data: &mut RefAny, info: CallbackInfo) -> Update {
    let Some(part) = data.downcast_ref::<PartRef>() else {
        return Update::DoNothing;
    };
    match part.on_event.as_ref() {
        Some(SidebarOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, part.event),
        None => Update::DoNothing,
    }
}

extern "C" fn on_part_click(mut data: RefAny, info: CallbackInfo) -> Update {
    report(&mut data, info)
}

/// A click on a part inside a row (the disclosure triangle, a document's pie, its collaboration
/// icon) is that part's: the row under it must not also be chosen.
extern "C" fn on_inner_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    report(&mut data, info)
}

/// Items dragged over a row that takes them: the drag is accepted, so the drop reaches the row.
extern "C" fn on_row_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

/// A drop on a row: only that row's, not an ancestor's.
extern "C" fn on_row_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    report(&mut data, info)
}

/// The rail's tooltip: the name under the pointer.
struct Tip(AzString);

extern "C" fn on_tip_show(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(tip) = data.downcast_ref::<Tip>().map(|t| t.0.clone()) {
        info.show_tooltip(tip);
    }
    Update::DoNothing
}

extern "C" fn on_tip_hide(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.hide_tooltip();
    Update::DoNothing
}

/// What the rail's swipe handlers share: the app's callback, where the press began, whether the
/// pointer is captured and whether the drawer was asked for already in this gesture.
struct SwipeRef {
    on_event: OptionSidebarOnEvent,
    anchor: Option<(f32, f32)>,
    captured: bool,
    fired: bool,
}

extern "C" fn on_swipe_press(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<SwipeRef>() {
        s.anchor = info.get_cursor_position().map(|p| (p.x, p.y));
        s.captured = false;
        s.fired = false;
    }
    Update::DoNothing
}

/// The swipe in flight moved: inward by the swipe's length it asks for the drawer (once); a drag
/// that is clearly inward captures the pointer, so that it goes on over the main view.
fn swipe_moved(data: &mut RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<SwipeRef>() else {
        return Update::DoNothing;
    };
    if s.fired {
        return Update::DoNothing;
    }
    let (Some((x0, y0)), Some(p)) = (s.anchor, info.get_cursor_position()) else {
        return Update::DoNothing;
    };
    let (dx, dy) = (p.x - x0, p.y - y0);
    if !s.captured && dx >= SWIPE_CAPTURE_PX && dx > 2.0 * dy.abs() {
        s.captured = true;
        let me = info.get_hit_node();
        info.capture_pointer(me);
    }
    if !swipe_opens(dx, dy) {
        return Update::DoNothing;
    }
    s.fired = true;
    match s.on_event.as_ref() {
        Some(SidebarOnEvent { callback, refany }) => {
            callback.invoke(refany.clone(), info, event(SidebarEventKind::StateChanged, SidebarState::Expanded))
        }
        None => Update::DoNothing,
    }
}

/// A move of the mouse counts only with the left button down: a press that ended outside the rail
/// leaves its anchor behind.
extern "C" fn on_swipe_mouse_move(mut data: RefAny, info: CallbackInfo) -> Update {
    if !info.get_current_mouse_state().left_down {
        return Update::DoNothing;
    }
    swipe_moved(&mut data, info)
}

extern "C" fn on_swipe_touch_move(mut data: RefAny, info: CallbackInfo) -> Update {
    swipe_moved(&mut data, info)
}

extern "C" fn on_swipe_end(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<SwipeRef>() {
        s.anchor = None;
        s.captured = false;
    }
    Update::DoNothing
}

/// Up / Down / Home / End move the keyboard stop between the rows (one Tab stop for the list,
/// APG "roving tabindex"); Right opens a row with children and Left closes it (macOS's
/// outline); Enter and Space are the engine's synthetic click.
extern "C" fn on_row_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let me = info.get_hit_node();
    // Right / Left on a row with children: open or close it
    if matches!(key, VirtualKeyCode::Left | VirtualKeyCode::Right) {
        let Some(part) = data.downcast_ref::<RowKeyRef>() else {
            return Update::DoNothing;
        };
        if !part.has_children {
            return Update::DoNothing;
        }
        let open = key == VirtualKeyCode::Right;
        if open == part.expanded {
            return Update::DoNothing;
        }
        let mut ev = part.event;
        ev.kind = SidebarEventKind::RowToggled;
        ev.expand = open;
        let on_event = part.on_event.clone();
        info.prevent_default();
        return match on_event.as_ref() {
            Some(SidebarOnEvent { callback, refany }) => callback.invoke(refany.clone(), info, ev),
            None => Update::DoNothing,
        };
    }
    let step = match key {
        VirtualKeyCode::Up => roving::Step::Previous,
        VirtualKeyCode::Down => roving::Step::Next,
        VirtualKeyCode::Home => roving::Step::First,
        VirtualKeyCode::End => roving::Step::Last,
        _ => return Update::DoNothing,
    };
    let Some(list) = info.get_parent(me) else {
        return Update::DoNothing;
    };
    let mut items = roving::items_of(&info, list, SIDEBAR_ROW_CLASS);
    items.extend(roving::items_of(&info, list, SIDEBAR_BUTTON_CLASS));
    let Some(current) = items.iter().position(|n| *n == me) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, items.len(), step, false) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    roving::move_stop(&mut info, &items, target);
    Update::DoNothing
}

/// What a row's key handler needs: the report of the row, whether it can open or close.
struct RowKeyRef {
    on_event: OptionSidebarOnEvent,
    event: SidebarEvent,
    has_children: bool,
    expanded: bool,
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

/// A class list: `base`, then `extra` for each flag that holds.
fn classes(base: &'static str, extra: &[(bool, &'static str)]) -> IdOrClassVec {
    let mut v: Vec<IdOrClass> = Vec::with_capacity(1 + extra.len());
    v.push(IdOrClass::Class(AzString::from_const_str(base)));
    for (on, class) in extra {
        if *on {
            v.push(IdOrClass::Class(AzString::from_const_str(class)));
        }
    }
    IdOrClassVec::from_vec(v)
}

/// A node styled by `css` as ITS OWN style (a `:hover` block is a condition on the node itself,
/// never a rule that also paints the children under the pointer).
fn styled(mut dom: Dom, css: &str) -> Dom {
    dom.root.set_css(css);
    dom
}

/// A callback on `dom` for `filter`, handing `data` to `cb`.
fn on(dom: Dom, filter: EventFilter, data: RefAny, cb: extern "C" fn(RefAny, CallbackInfo) -> Update) -> Dom {
    dom.with_callbacks(
        alloc::vec![CoreCallbackData {
            event: filter,
            callback: CoreCallback {
                cb: cb as usize,
                ctx: OptionRefAny::None,
            },
            refany: data,
        }]
        .into(),
    )
}

/// Several callbacks on `dom`.
fn on_all(dom: Dom, list: Vec<CoreCallbackData>) -> Dom {
    dom.with_callbacks(list.into())
}

fn callback(
    filter: EventFilter,
    data: RefAny,
    cb: extern "C" fn(RefAny, CallbackInfo) -> Update,
) -> CoreCallbackData {
    CoreCallbackData {
        event: filter,
        callback: CoreCallback {
            cb: cb as usize,
            ctx: OptionRefAny::None,
        },
        refany: data,
    }
}

/// Everything one build of the sidebar shares.
struct Ctx {
    state: SidebarState,
    /// Whether the rows and the rail's slots are finger-sized: the phone's form, or any touch
    /// platform (a tablet at 600 dp and more is the desktop's form with fingers on it).
    touch: bool,
    look: &'static SurfaceLook,
    on_event: OptionSidebarOnEvent,
    /// The row the keyboard stop is on: the selected one, else the first.
    stop: usize,
}

impl Ctx {
    const fn row_h(&self) -> f32 {
        if self.touch {
            ROW_HEIGHT_PHONE
        } else {
            ROW_HEIGHT_DESKTOP
        }
    }

    /// The height of one slot of the rail.
    const fn slot_h(&self) -> f32 {
        if self.touch {
            RAIL_BUTTON_PHONE
        } else {
            RAIL_BUTTON_DESKTOP
        }
    }

    fn part(&self, ev: SidebarEvent) -> RefAny {
        RefAny::new(PartRef {
            on_event: self.on_event.clone(),
            event: ev,
        })
    }
}

/// The icon `name` at `px`, in `color` (a CSS colour, or empty for the ink it stands in).
fn icon(name: &str, px: f32, color: &str) -> Dom {
    let css = if color.is_empty() {
        format!("font-size: {px}px; width: {}px; flex-shrink: 0; text-align: center;", px + 2.0)
    } else {
        format!("font-size: {px}px; width: {}px; flex-shrink: 0; text-align: center; color: {color};", px + 2.0)
    };
    styled(Dom::create_icon(AzString::from(name)), &css)
}

fn rgb_css(c: ColorU) -> String {
    format!("rgb({}, {}, {})", c.r, c.g, c.b)
}

/// One row of the expanded sidebar.
fn row_dom(ctx: &Ctx, row: &SidebarRow, index: usize) -> Dom {
    let h = ctx.row_h();
    let depth = row.depth.min(MAX_DEPTH);
    #[allow(clippy::cast_precision_loss)] // at most 12 levels
    let pad = 8.0 + depth as f32 * INDENT_PER_DEPTH;
    let mut css = format!(
        "display: flex; flex-direction: row; align-items: center; gap: 8px; box-sizing: border-box; \
         flex-shrink: 0; height: {h}px; margin: 1px 6px; padding: 0px 8px 0px {pad}px; border-radius: 6px; \
         cursor: pointer; "
    );
    if row.selected {
        let _ = write!(
            css,
            "background: {sel}; font-weight: 600; :hover {{ background: {sel}; }} ",
            sel = ctx.look.selected
        );
    } else {
        let _ = write!(
            css,
            ":hover {{ background: {}; }} :active {{ background: {}; }} ",
            ctx.look.hover, ctx.look.hover
        );
    }
    css.push_str(":focus-visible { box-shadow: inset 0px 0px 0px 2px #4a8be8; }");

    let mut children: Vec<Dom> = Vec::with_capacity(5);
    let toggle_event = SidebarEvent {
        kind: SidebarEventKind::RowToggled,
        index,
        tag: row.tag,
        expand: !row.expanded,
        state: ctx.state,
    };
    // the triangle: a node of its own for a row with children, the room of one for a row without
    // (so the icons of a level line up)
    if row.has_children {
        let tri = icon(if row.expanded { "expand_more" } else { "chevron_right" }, 16.0, "");
        let tri = styled(tri, &format!("opacity: 0.7; width: 18px; height: {h}px; display: flex; \
             align-items: center; justify-content: center; margin-left: -4px;"))
        .with_ids_and_classes(classes(SIDEBAR_DISCLOSURE_CLASS, &[]))
        .with_accessibility_info(AccessibilityInfo::named(
            if row.expanded { "Collapse" } else { "Expand" },
            AccessibilityRole::PushButton,
        ));
        children.push(on(
            tri,
            EventFilter::Hover(HoverEventFilter::Click),
            ctx.part(toggle_event),
            on_inner_click,
        ));
    }
    match row.mark {
        SidebarRowMark::None => {}
        SidebarRowMark::Unchecked | SidebarRowMark::Checked => {
            let name = if row.mark == SidebarRowMark::Checked {
                "check_box"
            } else {
                "check_box_outline_blank"
            };
            let tint = row.tint.as_ref().map(|c| rgb_css(*c)).unwrap_or_default();
            children.push(icon(name, 18.0, &tint));
        }
    }
    if !row.icon.as_str().is_empty() {
        let tint = row.tint.as_ref().map(|c| rgb_css(*c)).unwrap_or_default();
        let color = if tint.is_empty() { String::from(ctx.look.quiet) } else { tint };
        children.push(icon(row.icon.as_str(), 18.0, &color));
    }
    children.push(styled(
        widget_p_with_text(row.label.clone()),
        "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;",
    ));
    if !row.badge.as_str().is_empty() {
        children.push(
            styled(
                widget_p_with_text(row.badge.clone()),
                &format!(
                    "flex-shrink: 0; margin: 0px; padding: 0px 7px; border-radius: 9px; font-size: 11px; \
                     line-height: 16px; color: {}; background: rgba(127, 127, 127, 0.22);",
                    ctx.look.quiet
                ),
            )
            .with_ids_and_classes(classes(SIDEBAR_BADGE_CLASS, &[])),
        );
    }

    // the row's own state, a glyph at its end (a cloud drive's)
    if !row.status_icon.as_str().is_empty() {
        children.push(
            icon(row.status_icon.as_str(), 16.0, ctx.look.quiet)
                .with_ids_and_classes(classes(SIDEBAR_STATUS_ICON_CLASS, &[]))
                .with_accessibility_info(AccessibilityInfo::named(
                    row.status_tip.clone(),
                    AccessibilityRole::Graphic,
                )),
        );
    }
    // an open document: its own pie (sync) and its collaboration switch
    if row.has_pie() {
        let pie = styled(
            Dom::create_div().with_child(disc_dom(ROW_PIE_PX, row.pie_fraction, row.pie_tone)),
            &format!(
                "display: flex; align-items: center; justify-content: center; flex-shrink: 0; width: 24px; \
                 height: 24px; border-radius: 6px; cursor: pointer; :hover {{ background: {}; }}",
                ctx.look.hover
            ),
        )
        .with_ids_and_classes(classes(SIDEBAR_ROW_PIE_CLASS, &[]))
        .with_accessibility_info(AccessibilityInfo::named(
            format!("Status of {}", row.label.as_str()),
            AccessibilityRole::PushButton,
        ))
        .with_tab_index(TabIndex::NoKeyboardFocus);
        children.push(on(
            pie,
            EventFilter::Hover(HoverEventFilter::Click),
            ctx.part(SidebarEvent {
                kind: SidebarEventKind::RowStatusClicked,
                index,
                tag: row.tag,
                expand: false,
                state: ctx.state,
            }),
            on_inner_click,
        ));
    }
    if row.collab != SidebarCollab::None {
        let lit = row.collab == SidebarCollab::On;
        let button = styled(
            Dom::create_div().with_child(icon(
                if lit { "group" } else { "person_add" },
                18.0,
                if lit { "system:accent" } else { ctx.look.quiet },
            )),
            &format!(
                "display: flex; align-items: center; justify-content: center; flex-shrink: 0; width: 28px; \
                 height: 28px; border-radius: 6px; cursor: pointer; :hover {{ background: {}; }}",
                ctx.look.hover
            ),
        )
        .with_ids_and_classes(classes(SIDEBAR_COLLAB_CLASS, &[]))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::PushButton,
            accessibility_name: azul_css::OptionString::Some(AzString::from(if lit {
                "Collaboration is on, turn it off"
            } else {
                "Activate collaboration"
            })),
            states: AccessibilityStateVec::from_vec(alloc::vec![if lit {
                AccessibilityState::CheckedTrue
            } else {
                AccessibilityState::CheckedFalse
            }]),
            ..Default::default()
        })
        .with_tab_index(TabIndex::NoKeyboardFocus);
        children.push(on(
            button,
            EventFilter::Hover(HoverEventFilter::Click),
            ctx.part(SidebarEvent {
                kind: SidebarEventKind::RowCollabToggled,
                index,
                tag: row.tag,
                expand: !lit,
                state: ctx.state,
            }),
            on_inner_click,
        ));
    }

    let click = ctx.part(SidebarEvent {
        kind: SidebarEventKind::RowClicked,
        index,
        tag: row.tag,
        expand: false,
        state: ctx.state,
    });
    let press = ctx.part(SidebarEvent {
        kind: SidebarEventKind::RowLongPressed,
        index,
        tag: row.tag,
        expand: false,
        state: ctx.state,
    });
    let keys = RefAny::new(RowKeyRef {
        on_event: ctx.on_event.clone(),
        event: SidebarEvent {
            kind: SidebarEventKind::RowClicked,
            index,
            tag: row.tag,
            expand: false,
            state: ctx.state,
        },
        has_children: row.has_children,
        expanded: row.expanded,
    });
    let mut states: Vec<AccessibilityState> = Vec::new();
    if row.selected {
        states.push(AccessibilityState::Selected);
    }
    if row.has_children {
        states.push(if row.expanded { AccessibilityState::Expanded } else { AccessibilityState::Collapsed });
    }
    let node = styled(Dom::create_div().with_children(DomVec::from_vec(children)), &css)
        .with_ids_and_classes(classes(
            SIDEBAR_ROW_CLASS,
            &[(row.selected, SIDEBAR_SELECTED_CLASS)],
        ))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::OutlineItem,
            accessibility_name: azul_css::OptionString::Some(row.label.clone()),
            states: AccessibilityStateVec::from_vec(states),
            ..Default::default()
        })
        .with_tab_index(roving::item_tab_index(index, ctx.stop));
    let mut list = alloc::vec![
        callback(EventFilter::Hover(HoverEventFilter::Click), click, on_part_click),
        callback(EventFilter::Hover(HoverEventFilter::LongPress), press, on_part_click),
        callback(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), keys, on_row_key),
        callback(
            EventFilter::Hover(HoverEventFilter::RightMouseUp),
            ctx.part(row_event(ctx, SidebarEventKind::RowMenuRequested, row, index)),
            on_part_click,
        ),
    ];
    if row.accepts_drops {
        list.push(callback(
            EventFilter::Hover(HoverEventFilter::DragOver),
            RefAny::new(()),
            on_row_drag_over,
        ));
        list.push(callback(
            EventFilter::Hover(HoverEventFilter::Drop),
            ctx.part(row_event(ctx, SidebarEventKind::RowDropped, row, index)),
            on_row_drop,
        ));
    }
    on_all(node, list)
}

/// The event `kind` about row `row` at place `index`.
fn row_event(ctx: &Ctx, kind: SidebarEventKind, row: &SidebarRow, index: usize) -> SidebarEvent {
    SidebarEvent {
        kind,
        index,
        tag: row.tag,
        expand: false,
        state: ctx.state,
    }
}

/// A section's title row.
fn section_dom(ctx: &Ctx, section: &SidebarSection, index: usize) -> Option<Dom> {
    if section.title.as_str().is_empty() {
        return None;
    }
    let mut css = format!(
        "display: flex; flex-direction: row; align-items: center; box-sizing: border-box; flex-shrink: 0; \
         height: 24px; margin: 14px 6px 2px 6px; padding: 0px 8px; font-size: 11px; font-weight: 700; \
         color: {}; @theme(flora) {{ text-transform: uppercase; letter-spacing: 0.08em; }}",
        ctx.look.quiet
    );
    let mut children: Vec<Dom> = alloc::vec![styled(
        widget_p_with_text(section.title.clone()),
        "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;"
    )];
    let mut node;
    if section.collapsible {
        css.push_str(" cursor: pointer;");
        children.push(icon(if section.open { "expand_more" } else { "chevron_right" }, 16.0, ""));
        node = styled(Dom::create_div().with_children(DomVec::from_vec(children)), &css)
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::PushButton,
                accessibility_name: azul_css::OptionString::Some(section.title.clone()),
                states: AccessibilityStateVec::from_vec(alloc::vec![if section.open {
                    AccessibilityState::Expanded
                } else {
                    AccessibilityState::Collapsed
                }]),
                ..Default::default()
            });
        node = on(
            node,
            EventFilter::Hover(HoverEventFilter::Click),
            ctx.part(SidebarEvent {
                kind: SidebarEventKind::SectionToggled,
                index,
                tag: 0,
                expand: !section.open,
                state: ctx.state,
            }),
            on_part_click,
        );
    } else {
        node = styled(Dom::create_div().with_children(DomVec::from_vec(children)), &css);
    }
    Some(node.with_ids_and_classes(classes(SIDEBAR_SECTION_CLASS, &[])))
}

/// The slop of one button of the rail: a transparent child of the button's face, `RAIL_WIDTH` +
/// `RAIL_SLOP` wide and a whole slot tall, so a finger that lands beside the 16 px face still
/// hits it. It is the face's own descendant (a press on it is the face's, and so is `:hover`) and
/// it is positioned, so it lies over the main view beside the rail.
fn slop(ctx: &Ctx) -> Dom {
    styled(
        Dom::create_div(),
        &format!(
            "position: absolute; left: -1px; top: -3px; width: {}px; height: {}px; z-index: 1;",
            RAIL_WIDTH + RAIL_SLOP,
            ctx.slot_h()
        ),
    )
    .with_ids_and_classes(classes(SIDEBAR_SLOP_CLASS, &[]))
}

/// The face of a slot of the rail: 16 px wide in the 18 px column (one px of room before it and
/// the column's own hairline after it), a slot tall less its margins.
fn face_css(ctx: &Ctx) -> String {
    format!(
        "display: flex; align-items: center; justify-content: center; box-sizing: border-box; flex-shrink: 0; \
         position: relative; width: {}px; height: {}px; margin: 3px 0px 3px 1px; border-radius: 6px; \
         cursor: pointer; ",
        RAIL_WIDTH - 2.0,
        ctx.slot_h() - 6.0
    )
}

/// The wash of a face under the pointer, or its fill when it is lit (the selected row, a toggle
/// that is on), and its focus ring.
fn face_states(ctx: &Ctx, css: &mut String, lit: bool) {
    if lit {
        let _ = write!(css, "background: {sel}; :hover {{ background: {sel}; }} ", sel = ctx.look.selected);
    } else {
        let _ = write!(css, ":hover {{ background: {h}; }} :active {{ background: {h}; }} ", h = ctx.look.hover);
    }
    css.push_str(":focus-visible { box-shadow: inset 0px 0px 0px 2px #4a8be8; }");
}

/// The tooltip callbacks of a rail button: the name under the pointer.
fn tip_callbacks(name: &AzString, long_press: bool) -> Vec<CoreCallbackData> {
    let mut list = alloc::vec![callback(
        EventFilter::Hover(HoverEventFilter::MouseEnter),
        RefAny::new(Tip(name.clone())),
        on_tip_show,
    )];
    if long_press {
        list.push(callback(
            EventFilter::Hover(HoverEventFilter::LongPress),
            RefAny::new(Tip(name.clone())),
            on_tip_show,
        ));
    }
    list.push(callback(EventFilter::Hover(HoverEventFilter::MouseLeave), RefAny::new(()), on_tip_hide));
    list
}

/// One navigation button of the rail (a row with an icon).
fn rail_button(ctx: &Ctx, row: &SidebarRow, index: usize) -> Dom {
    let mut css = face_css(ctx);
    face_states(ctx, &mut css, row.selected);
    let tint = row.tint.as_ref().map(|c| rgb_css(*c)).unwrap_or_default();
    let color = if tint.is_empty() { String::from(ctx.look.quiet) } else { tint };
    let name = AzString::from(row.rail_name());
    let mut face: Vec<Dom> = alloc::vec![icon(row.icon.as_str(), RAIL_ICON_PX, &color)];
    // a document keeps its pie in the rail: a small one at the corner of its button
    if row.has_pie() {
        face.push(styled(
            disc_dom(RAIL_PIE_PX, row.pie_fraction, row.pie_tone),
            "position: absolute; right: 0px; bottom: 1px;",
        ));
    }
    face.push(slop(ctx));
    let node = styled(
        Dom::create_div().with_children(DomVec::from_vec(face)),
        &css,
    )
    .with_ids_and_classes(classes(
        SIDEBAR_BUTTON_CLASS,
        &[(row.selected, SIDEBAR_SELECTED_CLASS)],
    ))
    .with_accessibility_info(AccessibilityInfo {
        role: AccessibilityRole::PushButton,
        accessibility_name: azul_css::OptionString::Some(name.clone()),
        states: AccessibilityStateVec::from_vec(if row.selected {
            alloc::vec![AccessibilityState::Selected]
        } else {
            Vec::new()
        }),
        ..Default::default()
    })
    .with_tab_index(roving::item_tab_index(index, ctx.stop));
    let click = ctx.part(SidebarEvent {
        kind: SidebarEventKind::RowClicked,
        index,
        tag: row.tag,
        expand: false,
        state: ctx.state,
    });
    let keys = RefAny::new(RowKeyRef {
        on_event: ctx.on_event.clone(),
        event: SidebarEvent {
            kind: SidebarEventKind::RowClicked,
            index,
            tag: row.tag,
            expand: false,
            state: ctx.state,
        },
        has_children: false,
        expanded: false,
    });
    let mut list = alloc::vec![callback(EventFilter::Hover(HoverEventFilter::Click), click, on_part_click)];
    list.extend(tip_callbacks(&name, true));
    list.push(callback(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), keys, on_row_key));
    // A right click asks the app for the row's menu; a row that takes drops listens to them.
    list.push(callback(
        EventFilter::Hover(HoverEventFilter::RightMouseUp),
        ctx.part(row_event(ctx, SidebarEventKind::RowMenuRequested, row, index)),
        on_part_click,
    ));
    if row.accepts_drops {
        list.push(callback(
            EventFilter::Hover(HoverEventFilter::DragOver),
            RefAny::new(()),
            on_row_drag_over,
        ));
        list.push(callback(
            EventFilter::Hover(HoverEventFilter::Drop),
            ctx.part(row_event(ctx, SidebarEventKind::RowDropped, row, index)),
            on_row_drop,
        ));
    }
    on_all(node, list)
}

/// One quick-access button of the rail: the app's own (font colour, bold, undo). A toggle is lit
/// while it is on, a swatch is a colour bar under the icon, a disabled button is dimmed and
/// reports nothing.
fn quick_button(ctx: &Ctx, action: &SidebarQuickAction, index: usize) -> Dom {
    let disabled = action.state == SidebarQuickState::Disabled;
    let on = action.state == SidebarQuickState::On;
    let mut css = face_css(ctx);
    if disabled {
        css.push_str("opacity: 0.38; cursor: default; ");
    } else {
        face_states(ctx, &mut css, on);
    }
    let ink = if on { "system:text" } else { ctx.look.quiet };
    let mut face: Vec<Dom> = alloc::vec![icon(action.icon.as_str(), RAIL_ICON_PX, ink)];
    if let Some(c) = action.swatch.as_ref() {
        face.push(styled(
            Dom::create_div(),
            &format!(
                "position: absolute; left: 3px; right: 3px; bottom: 3px; height: 3px; border-radius: 1px; \
                 background: {};",
                rgb_css(*c)
            ),
        ));
    }
    if !disabled {
        face.push(slop(ctx));
    }
    let name = action.label.clone();
    let states = match action.state {
        SidebarQuickState::On => alloc::vec![AccessibilityState::CheckedTrue],
        SidebarQuickState::Off => alloc::vec![AccessibilityState::CheckedFalse],
        SidebarQuickState::Disabled => alloc::vec![AccessibilityState::Unavailable],
        SidebarQuickState::Plain => Vec::new(),
    };
    let node = styled(Dom::create_div().with_children(DomVec::from_vec(face)), &css)
        .with_ids_and_classes(classes(
            SIDEBAR_QUICK_BUTTON_CLASS,
            &[(on, SIDEBAR_SELECTED_CLASS)],
        ))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::PushButton,
            accessibility_name: azul_css::OptionString::Some(name.clone()),
            states: AccessibilityStateVec::from_vec(states),
            ..Default::default()
        })
        .with_tab_index(if disabled { TabIndex::NoKeyboardFocus } else { TabIndex::Auto });
    let mut list = Vec::new();
    if !disabled {
        list.push(callback(
            EventFilter::Hover(HoverEventFilter::Click),
            ctx.part(SidebarEvent {
                kind: SidebarEventKind::QuickAction,
                index,
                tag: action.tag,
                expand: action.state == SidebarQuickState::Off,
                state: ctx.state,
            }),
            on_part_click,
        ));
    }
    list.extend(tip_callbacks(&name, !disabled));
    on_all(node, list)
}

/// A hairline across the column (between the sections of the rail, over the foot).
fn rule(width_css: &str) -> Dom {
    styled(
        Dom::create_div(),
        &format!(
            "flex-shrink: 0; height: 1px; margin: 6px auto; {width_css} background: system:separator; opacity: 0.7;"
        ),
    )
    .with_ids_and_classes(classes(SIDEBAR_RULE_CLASS, &[]))
}

/// The popover of a status: the app's own content, or a title and a line.
fn popover_content(status: &SidebarStatus) -> Dom {
    if let Some(dom) = status.popover.as_ref() {
        return dom.clone();
    }
    let mut lines: Vec<Dom> = alloc::vec![styled(
        widget_p_with_text(AzString::from(status.spoken())),
        "margin: 0px; font-weight: 700;"
    )];
    if !status.detail.as_str().is_empty() {
        lines.push(styled(
            widget_p_with_text(status.detail.clone()),
            "margin: 0px; opacity: 0.8;",
        ));
    }
    styled(
        Dom::create_div().with_children(DomVec::from_vec(lines)),
        "display: flex; flex-direction: column; gap: 4px; box-sizing: border-box; padding: 10px 12px; \
         min-width: 160px; max-width: 280px;",
    )
    .with_ids_and_classes(classes(SIDEBAR_POPOVER_CLASS, &[]))
}

/// The ground and the ink of a badge in `tone`.
const fn tone_ink(tone: PieTone) -> (&'static str, &'static str) {
    match tone {
        PieTone::Accent | PieTone::Busy => ("system:accent", "#ffffff"),
        PieTone::Good => ("#2e9e5b", "#ffffff"),
        PieTone::Warning => ("#e8a317", "#1b1b1b"),
        PieTone::Danger => ("system:danger", "#ffffff"),
    }
}

/// A badge `px` tall: the first three characters of `text` in a pill of `tone`'s colour; no text
/// is a dot.
fn badge_dom(text: &str, tone: PieTone, px: f32) -> Dom {
    let (ground, ink) = tone_ink(tone);
    let shown: String = text.chars().take(3).collect();
    if shown.is_empty() {
        let d = (px * 0.6).round().max(6.0);
        return styled(
            Dom::create_div(),
            &format!(
                "flex-shrink: 0; width: {d}px; height: {d}px; border-radius: {}px; background: {ground};",
                d / 2.0
            ),
        )
        .with_ids_and_classes(classes(SIDEBAR_STATUS_BADGE_CLASS, &[]));
    }
    let font = (px * 0.62).round().max(8.0);
    styled(
        Dom::create_div().with_child(styled(
            widget_p_with_text(AzString::from(shown)),
            "margin: 0px; line-height: 1px;",
        )),
        &format!(
            "display: flex; align-items: center; justify-content: center; box-sizing: border-box; flex-shrink: 0; \
             height: {px}px; min-width: {px}px; padding: 0px 3px; border-radius: {}px; background: {ground}; \
             color: {ink}; font-size: {font}px; font-weight: 700;",
            px / 2.0
        ),
    )
    .with_ids_and_classes(classes(SIDEBAR_STATUS_BADGE_CLASS, &[]))
}

/// One status: a pie or a badge in a button (the rail) or in a row with its label and text
/// (expanded), and the popover a click opens.
fn status_dom(ctx: &Ctx, status: &SidebarStatus, expanded: bool, theme: OptionUiTheme) -> Dom {
    let spoken = status.spoken();
    let badge = status.shape == SidebarStatusShape::Badge;
    let mark_px = if expanded { STATUS_PIE_PX } else { RAIL_STATUS_PX };
    let mark = if badge {
        badge_dom(status.text.as_str(), status.tone, mark_px)
    } else {
        disc_dom(mark_px, status.fraction, status.tone)
    };
    let mut css = if expanded {
        let h = ctx.row_h();
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 8px; box-sizing: border-box; \
             flex-shrink: 0; height: {h}px; margin: 1px 6px; padding: 0px 8px; border-radius: 6px; cursor: pointer; \
             width: calc(100% - 12px); "
        )
    } else {
        face_css(ctx)
    };
    let _ = write!(
        css,
        ":hover {{ background: {}; }} :active {{ background: {}; }} :focus-visible {{ box-shadow: inset 0px 0px 0px 2px #4a8be8; }}",
        ctx.look.hover, ctx.look.hover
    );
    let mut children: Vec<Dom> = alloc::vec![if expanded { styled(mark, "margin: 0px 2px;") } else { mark }];
    if expanded {
        children.push(styled(
            widget_p_with_text(status.label.clone()),
            "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;",
        ));
        if !badge && !status.text.as_str().is_empty() {
            children.push(styled(
                widget_p_with_text(status.text.clone()),
                &format!("flex-shrink: 0; margin: 0px; font-size: 12px; color: {};", ctx.look.quiet),
            ));
        }
    } else {
        children.push(slop(ctx));
    }
    let invoker = styled(Dom::create_div().with_children(DomVec::from_vec(children)), &css)
        .with_ids_and_classes(classes(SIDEBAR_STATUS_CLASS, &[]))
        .with_accessibility_info(AccessibilityInfo::named(spoken.clone(), AccessibilityRole::PushButton))
        .with_tab_index(TabIndex::Auto);
    let invoker = if expanded {
        invoker
    } else {
        on_all(invoker, tip_callbacks(&AzString::from(spoken.as_str()), false))
    };
    let mut dialog = Dialog::create(popover_content(status))
        .with_invoker(invoker)
        .with_modal(false)
        .with_anchor(TransientAnchor::Right)
        .with_closed_by(DialogClosedBy::Any)
        .with_close_button(false);
    if let Some(t) = theme.into_option() {
        dialog = dialog.with_theme(t);
    }
    // the dialog's wrapper stands in the column like its invoker would
    styled(
        Dom::create_div().with_child(dialog.dom()),
        "display: flex; justify-content: flex-start; width: 100%; flex-shrink: 0;",
    )
}

/// A button of the rail's foot (help, the toggle): an icon in a face with its slop, its label a
/// tooltip.
fn rail_foot_button(ctx: &Ctx, icon_name: &str, label: &str, class: &'static str, ev: SidebarEvent) -> Dom {
    let mut css = face_css(ctx);
    face_states(ctx, &mut css, false);
    let face: Vec<Dom> = alloc::vec![icon(icon_name, RAIL_ICON_PX, ctx.look.quiet), slop(ctx)];
    let node = styled(Dom::create_div().with_children(DomVec::from_vec(face)), &css)
        .with_ids_and_classes(classes(class, &[]))
        .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::PushButton))
        .with_tab_index(TabIndex::Auto);
    let mut list = alloc::vec![callback(EventFilter::Hover(HoverEventFilter::Click), ctx.part(ev), on_part_click)];
    list.extend(tip_callbacks(&AzString::from(label), false));
    on_all(node, list)
}

/// A row of the expanded sidebar's foot (help, the toggle).
fn foot_row(ctx: &Ctx, icon_name: &str, label: &str, class: &'static str, ev: SidebarEvent) -> Dom {
    let h = ctx.row_h();
    let mut css = format!(
        "display: flex; flex-direction: row; align-items: center; gap: 8px; box-sizing: border-box; \
         flex-shrink: 0; height: {h}px; margin: 1px 6px; padding: 0px 8px; border-radius: 6px; cursor: pointer; \
         width: calc(100% - 12px); "
    );
    let _ = write!(
        css,
        ":hover {{ background: {}; }} :active {{ background: {}; }} :focus-visible {{ box-shadow: inset 0px 0px 0px 2px #4a8be8; }}",
        ctx.look.hover, ctx.look.hover
    );
    let children: Vec<Dom> = alloc::vec![
        icon(icon_name, 18.0, ctx.look.quiet),
        styled(
            widget_p_with_text(AzString::from(label)),
            "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;",
        ),
    ];
    let node = styled(Dom::create_div().with_children(DomVec::from_vec(children)), &css)
        .with_ids_and_classes(classes(class, &[]))
        .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::PushButton))
        .with_tab_index(TabIndex::Auto);
    on(node, EventFilter::Hover(HoverEventFilter::Click), ctx.part(ev), on_part_click)
}

/// The width declarations of a column.
fn width_css(col: SidebarColumnWidth) -> String {
    if col.percent > 0.0 {
        if col.max_px > 0.0 {
            format!("width: {}%; max-width: {}px;", col.percent, col.max_px)
        } else {
            format!("width: {}%;", col.percent)
        }
    } else {
        format!("width: {}px;", col.px)
    }
}

/// The quick-access buttons held to [`MAX_QUICK_ACTIONS`] (with one diagnostic).
fn limited_quick(mut quick: Vec<SidebarQuickAction>) -> Vec<SidebarQuickAction> {
    if quick.len() > MAX_QUICK_ACTIONS {
        let extra = quick.len() - MAX_QUICK_ACTIONS;
        quick.truncate(MAX_QUICK_ACTIONS);
        azul_core::diagnostics::emit_once(0x51de_ba03, || {
            format!("Sidebar: {extra} quick-access button(s) past the limit of {MAX_QUICK_ACTIONS} were dropped")
        });
    }
    quick
}

/// What the app handed in, held to the limits (with one diagnostic per kind).
fn limited(mut sections: Vec<SidebarSection>, mut statuses: Vec<SidebarStatus>) -> (Vec<SidebarSection>, Vec<SidebarStatus>) {
    let mut budget = MAX_ROWS;
    let mut dropped = 0usize;
    for section in &mut sections {
        let rows = section.rows.as_ref();
        if rows.len() > budget {
            dropped += rows.len() - budget;
            let kept: Vec<SidebarRow> = rows[..budget].to_vec();
            section.rows = SidebarRowVec::from_vec(kept);
            budget = 0;
        } else {
            budget -= rows.len();
        }
    }
    if dropped > 0 {
        azul_core::diagnostics::emit_once(0x51de_ba01, || {
            format!("Sidebar: {dropped} row(s) past the limit of {MAX_ROWS} were dropped")
        });
    }
    if statuses.len() > MAX_STATUSES {
        let extra = statuses.len() - MAX_STATUSES;
        statuses.truncate(MAX_STATUSES);
        azul_core::diagnostics::emit_once(0x51de_ba02, || {
            format!("Sidebar: {extra} status indicator(s) past the limit of {MAX_STATUSES} were dropped")
        });
    }
    (sections, statuses)
}

/// Whether the platform is touch-first: its rows and slots are finger-sized whatever the form.
const TOUCH_PLATFORM: bool = cfg!(any(target_os = "android", target_os = "ios"));

/// The sidebar's DOM.
#[allow(clippy::too_many_lines)] // one build: the handle, the rail, the expanded column
fn build(sb: Sidebar) -> Dom {
    let Sidebar {
        header,
        on_event,
        sections,
        statuses,
        quick_access,
        label,
        help,
        theme,
        state,
        surface,
        form,
        expanded_width,
        controls_left,
        controls_height: controls_h,
        frame_width,
        handle_in_cycle,
    } = sb;
    let phone = form.is_phone();
    let look = surface.look();
    let mut col = column_width(state, form, expanded_width, controls_left);
    // a frame of a known width: the drawer's share is a width in px as well, so the column can
    // be tweened between its states
    if frame_width.is_finite() && frame_width > 0.0 && col.percent > 0.0 {
        col = SidebarColumnWidth {
            px: col.in_frame(frame_width),
            percent: 0.0,
            max_px: 0.0,
        };
    }
    let (sections, statuses) = limited(sections.into_library_owned_vec(), statuses.into_library_owned_vec());
    let quick = limited_quick(quick_access.into_library_owned_vec());

    // the keyboard stop: the selected row, else the first
    let mut flat: Vec<(usize, usize)> = Vec::new(); // (section, row) in the app's order
    for (si, section) in sections.iter().enumerate() {
        for ri in 0..section.rows.as_ref().len() {
            flat.push((si, ri));
        }
    }
    let selected = flat.iter().position(|&(si, ri)| sections[si].rows.as_ref()[ri].selected);
    let rail_rows: Vec<usize> = flat
        .iter()
        .enumerate()
        .filter(|(_, &(si, ri))| sections[si].rows.as_ref()[ri].is_rail_button())
        .map(|(i, _)| i)
        .collect();
    let stop = match state {
        SidebarState::Rail => {
            selected.filter(|s| rail_rows.contains(s)).or_else(|| rail_rows.first().copied()).unwrap_or(0)
        }
        _ => selected.unwrap_or(0),
    };
    let ctx = Ctx {
        state,
        touch: phone || TOUCH_PLATFORM,
        look,
        on_event,
        stop,
    };

    let state_class = match state {
        SidebarState::Expanded => SIDEBAR_EXPANDED_CLASS,
        SidebarState::Rail => SIDEBAR_RAIL_CLASS,
        SidebarState::Handle => SIDEBAR_HANDLE_CLASS,
    };
    // the rail's buttons reach over the main view beside it (the hit slop): it clips nothing and
    // lies above what follows it
    let overflow = if state == SidebarState::Rail {
        "overflow: visible; z-index: 6;"
    } else {
        "overflow: hidden;"
    };
    let mut root_css = format!(
        "display: flex; flex-direction: column; flex-shrink: 0; flex-grow: 0; height: 100%; box-sizing: border-box; \
         {overflow} position: relative; font-size: 13px; user-select: none; \
         border-right: 1px solid system:separator; {} {} {}",
        width_css(col),
        // the column slides between its states (a transition of the width: it needs a width in px
        // on both sides, see `frame_width`; where the engine does not tween, the width jumps)
        if col.percent > 0.0 { "" } else { "animation: width 200ms ease-out;" },
        look.root
    );
    match state {
        SidebarState::Rail => root_css.push_str(" align-items: flex-start;"),
        SidebarState::Handle => root_css.push_str(" align-items: center;"),
        SidebarState::Expanded => {}
    }
    let root_classes = classes(
        SIDEBAR_CLASS,
        &[(true, state_class), (phone, SIDEBAR_PHONE_CLASS)],
    );
    let landmark = AccessibilityInfo::named(label, AccessibilityRole::Pane);

    // ---- the 16 px handle ----
    if state == SidebarState::Handle {
        let grab = styled(
            Dom::create_div(),
            "width: 4px; height: 44px; border-radius: 3px; background: currentColor; opacity: 0.5;",
        )
        .with_ids_and_classes(classes(SIDEBAR_GRAB_CLASS, &[]));
        let handle = styled(
            Dom::create_div().with_child(grab),
            "display: flex; align-items: center; justify-content: center; flex-grow: 1; width: 100%; cursor: pointer;",
        )
        .with_accessibility_info(AccessibilityInfo::named("Show the sidebar", AccessibilityRole::PushButton))
        .with_tab_index(TabIndex::Auto);
        let next = state.from_handle();
        let handle = on(
            handle,
            EventFilter::Hover(HoverEventFilter::Click),
            ctx.part(event(SidebarEventKind::StateChanged, next)),
            on_part_click,
        );
        let mut root = styled(Dom::create_nav().with_child(handle), &root_css)
            .with_ids_and_classes(root_classes)
            .with_accessibility_info(landmark);
        // a handle has no band for the controls: they sit over its top where the app leaves room
        let _ = controls_h;
        root.root.set_tab_index(TabIndex::NoKeyboardFocus);
        return root;
    }

    let expanded = state == SidebarState::Expanded;
    let mut kids: Vec<Dom> = Vec::new();

    // ---- the band the OS's window controls sit in ----
    let band = controls_height(controls_left, controls_h);
    if band > 0.0 {
        kids.push(
            styled(
                Dom::create_div(),
                &format!("flex-shrink: 0; width: 100%; height: {band}px; -azul-app-region: drag;"),
            )
            .with_ids_and_classes(classes(SIDEBAR_CONTROLS_CLASS, &[])),
        );
    }

    // ---- the rail: navigation at the top, quick access at the bottom, then the foot ----
    if !expanded {
        // every group is as wide as the face and its slop, so that the slop is inside the group's
        // own clip; the rail itself clips nothing
        let group_w = RAIL_WIDTH + RAIL_SLOP;
        let mut nav: Vec<Dom> = Vec::new();
        let mut index = 0usize;
        let mut first_section = true;
        for section in &sections {
            let mut buttons: Vec<Dom> = Vec::new();
            for row in section.rows.as_ref() {
                if row.is_rail_button() {
                    buttons.push(rail_button(&ctx, row, index));
                }
                index += 1;
            }
            if buttons.is_empty() {
                continue;
            }
            if !first_section {
                nav.push(rule("width: 12px;"));
            }
            first_section = false;
            nav.extend(buttons);
        }
        kids.push(
            styled(
                Dom::create_div().with_children(DomVec::from_vec(nav)),
                &format!(
                    "display: flex; flex-direction: column; align-items: flex-start; flex-grow: 1; flex-shrink: 1; \
                     flex-basis: 0px; min-height: 0px; width: {group_w}px; overflow-y: auto; overflow-x: hidden; \
                     scrollbar-width: none; padding: 4px 0px;"
                ),
            )
            .with_ids_and_classes(classes(SIDEBAR_LIST_CLASS, &[]))
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::Outline,
                ..Default::default()
            }),
        );
        if !quick.is_empty() {
            kids.push(rule("width: 12px;"));
            let buttons: Vec<Dom> = quick.iter().enumerate().map(|(i, a)| quick_button(&ctx, a, i)).collect();
            kids.push(
                styled(
                    Dom::create_div().with_children(DomVec::from_vec(buttons)),
                    &format!(
                        "display: flex; flex-direction: column; align-items: flex-start; flex-shrink: 0; \
                         width: {group_w}px;"
                    ),
                )
                .with_ids_and_classes(classes(SIDEBAR_QUICK_CLASS, &[])),
            );
        }
        let mut foot: Vec<Dom> = Vec::new();
        if !help.as_str().is_empty() {
            foot.push(rail_foot_button(
                &ctx,
                "help",
                help.as_str(),
                SIDEBAR_HELP_CLASS,
                event(SidebarEventKind::HelpClicked, state),
            ));
        }
        for status in &statuses {
            foot.push(status_dom(&ctx, status, false, theme));
        }
        foot.push(rail_foot_button(
            &ctx,
            "chevron_right",
            "Expand the sidebar",
            SIDEBAR_TOGGLE_CLASS,
            event(SidebarEventKind::StateChanged, state.toggled(handle_in_cycle)),
        ));
        kids.push(rule("width: 12px;"));
        kids.push(
            styled(
                Dom::create_div().with_children(DomVec::from_vec(foot)),
                &format!(
                    "display: flex; flex-direction: column; align-items: flex-start; flex-shrink: 0; \
                     width: {group_w}px; padding: 0px 0px 6px 0px;"
                ),
            )
            .with_ids_and_classes(classes(SIDEBAR_FOOT_CLASS, &[])),
        );
        let swipe_on = ctx.on_event.is_some();
        let swipe = RefAny::new(SwipeRef {
            on_event: ctx.on_event,
            anchor: None,
            captured: false,
            fired: false,
        });
        let mut root = styled(Dom::create_nav().with_children(DomVec::from_vec(kids)), &root_css)
            .with_ids_and_classes(root_classes)
            .with_accessibility_info(landmark);
        // a swipe inward from the rail opens the drawer, by a finger or by a drag of the mouse
        if swipe_on {
            root = on_all(
                root,
                alloc::vec![
                    callback(EventFilter::Hover(HoverEventFilter::MouseDown), swipe.clone(), on_swipe_press),
                    callback(EventFilter::Hover(HoverEventFilter::MouseOver), swipe.clone(), on_swipe_mouse_move),
                    callback(EventFilter::Hover(HoverEventFilter::MouseUp), swipe.clone(), on_swipe_end),
                    callback(EventFilter::Hover(HoverEventFilter::TouchStart), swipe.clone(), on_swipe_press),
                    callback(EventFilter::Hover(HoverEventFilter::TouchMove), swipe.clone(), on_swipe_touch_move),
                    callback(EventFilter::Hover(HoverEventFilter::TouchEnd), swipe, on_swipe_end),
                ],
            );
        }
        return root;
    }

    // ---- the expanded sidebar ----
    if let Some(h) = header.into_option() {
        kids.push(h);
    }
    let mut list: Vec<Dom> = Vec::new();
    let mut index = 0usize;
    for (si, section) in sections.iter().enumerate() {
        if let Some(title) = section_dom(&ctx, section, si) {
            list.push(title);
        }
        let shows = !section.collapsible || section.open;
        for row in section.rows.as_ref() {
            if shows {
                list.push(row_dom(&ctx, row, index));
            }
            index += 1;
        }
    }
    kids.push(
        styled(
            Dom::create_div().with_children(DomVec::from_vec(list)),
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; width: 100%; overflow-y: auto; \
             scrollbar-width: none; padding: 4px 0px 8px 0px;",
        )
        .with_ids_and_classes(classes(SIDEBAR_LIST_CLASS, &[]))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Outline,
            ..Default::default()
        }),
    );

    // ---- the foot: help, the status indicators, the toggle ----
    let mut foot: Vec<Dom> = Vec::new();
    if !help.as_str().is_empty() {
        foot.push(foot_row(
            &ctx,
            "help",
            help.as_str(),
            SIDEBAR_HELP_CLASS,
            event(SidebarEventKind::HelpClicked, state),
        ));
    }
    for status in &statuses {
        foot.push(status_dom(&ctx, status, true, theme));
    }
    foot.push(foot_row(
        &ctx,
        "chevron_left",
        "Collapse the sidebar",
        SIDEBAR_TOGGLE_CLASS,
        event(SidebarEventKind::StateChanged, state.toggled(handle_in_cycle)),
    ));
    kids.push(rule("width: calc(100% - 20px);"));
    kids.push(
        styled(
            Dom::create_div().with_children(DomVec::from_vec(foot)),
            "display: flex; flex-direction: column; flex-shrink: 0; width: 100%; padding: 0px 0px 8px 0px;",
        )
        .with_ids_and_classes(classes(SIDEBAR_FOOT_CLASS, &[])),
    );

    styled(Dom::create_nav().with_children(DomVec::from_vec(kids)), &root_css)
        .with_ids_and_classes(root_classes)
        .with_accessibility_info(landmark)
}

#[cfg(test)]
#[path = "sidebar_test.rs"]
mod sidebar_test;

#[cfg(test)]
#[path = "sidebar_row_extras_test.rs"]
mod sidebar_row_extras_test;
