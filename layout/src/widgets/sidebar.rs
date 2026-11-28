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
//! - DOCUMENT ROWS (AzWriter, AzSheets, AzPresent list their open documents here): a row may carry
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
        column_width, controls_height, SidebarColumnWidth, INDENT_PER_DEPTH,
        MAX_DEPTH, MAX_ROWS, MAX_STATUSES, RAIL_BUTTON_DESKTOP, RAIL_BUTTON_PHONE,
        ROW_HEIGHT_DESKTOP, ROW_HEIGHT_PHONE, STATUS_PIE_PX,
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
/// The scrolling column of sections and rows (or, in the rail, of buttons).
pub const SIDEBAR_LIST_CLASS: &str = "__azul-native-sidebar-list";
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
/// A button of the rail.
pub const SIDEBAR_BUTTON_CLASS: &str = "__azul-native-sidebar-button";
/// The hairline between the sections of the rail and over the foot.
pub const SIDEBAR_RULE_CLASS: &str = "__azul-native-sidebar-rule";
/// The foot: help, the status pies and the toggle.
pub const SIDEBAR_FOOT_CLASS: &str = "__azul-native-sidebar-foot";
/// A status pie (its invoker; in the rail a button, expanded a row).
pub const SIDEBAR_STATUS_CLASS: &str = "__azul-native-sidebar-status";
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
const RAIL_PIE_PX: f32 = 11.0;

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
    /// Inspector's pane wears too).
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

/// A document row's collaboration switch (AzWriter, AzSheets, AzPresent list the open documents in
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
    /// A colour for the icon (Mail's blue, a calendar's colour); none: the sidebar's ink.
    pub tint: OptionColorU,
    /// The count after the label ("329"); empty for none.
    pub badge: AzString,
    /// What the app calls this row: handed back in every event about it.
    pub tag: usize,
    /// How deep a tree row is, 0 at the top (held to `MAX_DEPTH`).
    pub depth: usize,
    /// Whether the row has children: it shows a disclosure triangle.
    pub has_children: bool,
    /// Whether the children show (the triangle points down).
    pub expanded: bool,
    /// Whether this is the row the main view shows: a rounded highlight.
    pub selected: bool,
    /// A check box before the icon.
    pub mark: SidebarRowMark,
    /// Whether the row is a button of the rail (a row with an icon is by default).
    pub in_rail: bool,
    /// What the rail's button says under the pointer and on a long press; empty: the label.
    pub tooltip: AzString,
    /// How far this row's own pie is, 0 to 1 (a document's sync state); a number below 0 (or no
    /// number) is no pie. APPENDED for ABI stability.
    pub pie_fraction: f32,
    /// What the row's pie means: its colour, and whether it breathes.
    pub pie_tone: PieTone,
    /// The row's collaboration icon: none, off or on.
    pub collab: SidebarCollab,
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

/// A status pie above the toggle: how far a sync has come, how full a drive is, a battery.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SidebarStatus {
    /// What it is ("Sync"): the accessible name, the popover's title when it has no content of its
    /// own, and the label of the expanded row.
    pub label: AzString,
    /// A short text beside the pie in the expanded sidebar ("82 %", "3 waiting"); empty for none.
    pub text: AzString,
    /// How far, 0 to 1: the pie's wedge.
    pub fraction: f32,
    /// What the number means: the pie's colour, and whether it breathes.
    pub tone: PieTone,
    /// A line of detail under the title in the default popover; empty for none.
    pub detail: AzString,
    /// The popover's content when it is more than a title and a line: the app's own `Dom`.
    pub popover: OptionDom,
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
            fraction,
            tone,
            detail: AzString::from_const_str(""),
            popover: OptionDom::None,
        }
    }

    /// The short text beside the pie of the expanded sidebar.
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

    /// What a screen reader says: the label and the text.
    fn spoken(&self) -> String {
        if self.text.as_str().is_empty() {
            String::from(self.label.as_str())
        } else {
            format!("{}, {}", self.label.as_str(), self.text.as_str())
        }
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
    /// The new open state (`RowToggled`, `SectionToggled`); false otherwise.
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

/// The standard left sidebar: sections of rows, status pies, an icon rail as its collapsed state.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Sidebar {
    /// The sections, top to bottom.
    pub sections: SidebarSectionVec,
    /// The status pies above the toggle (held to `MAX_STATUSES`).
    pub statuses: SidebarStatusVec,
    /// The state the app keeps and rebuilds with.
    pub state: SidebarState,
    /// What the sidebar is painted with.
    pub surface: SidebarSurface,
    /// Desktop or phone: `Auto` asks the window.
    pub form: FormFactor,
    /// The landmark's accessible name.
    pub label: AzString,
    /// The slot above the rows (an account, a "New" button); in the rail it is dropped.
    pub header: OptionDom,
    /// The help row's label; empty for no help row.
    pub help: AzString,
    /// The expanded width on a desktop, px; 0 is the default (a phone's drawer is a share of the
    /// frame).
    pub expanded_width: f32,
    /// How wide the OS's window controls are at the window's left edge (macOS's traffic lights,
    /// 78 px); 0 where they are not there.
    pub controls_left: f32,
    /// How tall the band they sit in is (the title bar's height).
    pub controls_height: f32,
    /// Whether the toggle's round includes the 16 px handle.
    pub handle_in_cycle: bool,
    /// Every action on the sidebar.
    pub on_event: OptionSidebarOnEvent,
    /// The widget theme this sidebar is PINNED to, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
}

impl_option!(Sidebar, OptionSidebar, copy = false, [Debug, Clone]);

impl Sidebar {
    /// An empty sidebar in the rail state.
    #[must_use]
    pub fn create() -> Self {
        Self {
            sections: SidebarSectionVec::from_const_slice(&[]),
            statuses: SidebarStatusVec::from_const_slice(&[]),
            state: SidebarState::Rail,
            surface: SidebarSurface::Theme,
            form: FormFactor::Auto,
            label: AzString::from_const_str("Sidebar"),
            header: OptionDom::None,
            help: AzString::from_const_str(""),
            expanded_width: 0.0,
            controls_left: 0.0,
            controls_height: 0.0,
            handle_in_cycle: false,
            on_event: OptionSidebarOnEvent::None,
            theme: OptionUiTheme::None,
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

    /// Appends a status pie.
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
    pub fn swap_with_default(&mut self) -> Self {
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

    /// The state the toggle asks for from the current one.
    #[must_use]
    pub fn toggled_state(&self) -> SidebarState {
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

fn event(kind: SidebarEventKind, state: SidebarState) -> SidebarEvent {
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
    phone: bool,
    look: &'static SurfaceLook,
    on_event: OptionSidebarOnEvent,
    /// The row the keyboard stop is on: the selected one, else the first.
    stop: usize,
}

impl Ctx {
    fn row_h(&self) -> f32 {
        if self.phone {
            ROW_HEIGHT_PHONE
        } else {
            ROW_HEIGHT_DESKTOP
        }
    }

    fn button(&self) -> f32 {
        if self.phone {
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

    fn at(&self, kind: SidebarEventKind) -> SidebarEvent {
        event(kind, self.state)
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
        css.push_str(&format!(
            "background: {sel}; font-weight: 600; :hover {{ background: {sel}; }} ",
            sel = ctx.look.selected
        ));
    } else {
        css.push_str(&format!(
            ":hover {{ background: {}; }} :active {{ background: {}; }} ",
            ctx.look.hover, ctx.look.hover
        ));
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
    on_all(
        node,
        alloc::vec![
            callback(EventFilter::Hover(HoverEventFilter::Click), click, on_part_click),
            callback(EventFilter::Hover(HoverEventFilter::LongPress), press, on_part_click),
            callback(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), keys, on_row_key),
        ],
    )
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

/// One button of the rail.
fn rail_button(ctx: &Ctx, row: &SidebarRow, index: usize) -> Dom {
    let b = ctx.button();
    let mut css = format!(
        "display: flex; align-items: center; justify-content: center; box-sizing: border-box; flex-shrink: 0; \
         position: relative; width: {b}px; height: {b}px; border-radius: 8px; cursor: pointer; "
    );
    if row.selected {
        css.push_str(&format!("background: {sel}; :hover {{ background: {sel}; }} ", sel = ctx.look.selected));
    } else {
        css.push_str(&format!(":hover {{ background: {}; }} :active {{ background: {}; }} ", ctx.look.hover, ctx.look.hover));
    }
    css.push_str(":focus-visible { box-shadow: inset 0px 0px 0px 2px #4a8be8; }");
    let tint = row.tint.as_ref().map(|c| rgb_css(*c)).unwrap_or_default();
    let color = if tint.is_empty() { String::from(ctx.look.quiet) } else { tint };
    let name = AzString::from(row.rail_name());
    let mut face: Vec<Dom> = alloc::vec![icon(row.icon.as_str(), 22.0, &color)];
    // a document keeps its pie in the rail: a small one at the corner of its button
    if row.has_pie() {
        face.push(styled(
            disc_dom(RAIL_PIE_PX, row.pie_fraction, row.pie_tone),
            "position: absolute; right: 4px; bottom: 4px;",
        ));
    }
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
    on_all(
        node,
        alloc::vec![
            callback(EventFilter::Hover(HoverEventFilter::Click), click, on_part_click),
            callback(EventFilter::Hover(HoverEventFilter::MouseEnter), RefAny::new(Tip(name.clone())), on_tip_show),
            callback(EventFilter::Hover(HoverEventFilter::LongPress), RefAny::new(Tip(name)), on_tip_show),
            callback(EventFilter::Hover(HoverEventFilter::MouseLeave), RefAny::new(()), on_tip_hide),
            callback(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), keys, on_row_key),
        ],
    )
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

/// One status: a pie in a button (the rail) or in a row with its label and text (expanded), and
/// the popover a click opens.
fn status_dom(ctx: &Ctx, status: &SidebarStatus, expanded: bool, theme: OptionUiTheme) -> Dom {
    let b = ctx.button();
    let spoken = status.spoken();
    let disc = disc_dom(STATUS_PIE_PX, status.fraction, status.tone);
    let mut css = if expanded {
        let h = ctx.row_h();
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 8px; box-sizing: border-box; \
             flex-shrink: 0; height: {h}px; margin: 1px 6px; padding: 0px 8px; border-radius: 6px; cursor: pointer; \
             width: calc(100% - 12px); "
        )
    } else {
        format!(
            "display: flex; align-items: center; justify-content: center; box-sizing: border-box; flex-shrink: 0; \
             width: {b}px; height: {b}px; border-radius: 8px; cursor: pointer; "
        )
    };
    css.push_str(&format!(
        ":hover {{ background: {}; }} :active {{ background: {}; }} :focus-visible {{ box-shadow: inset 0px 0px 0px 2px #4a8be8; }}",
        ctx.look.hover, ctx.look.hover
    ));
    let mut children: Vec<Dom> = alloc::vec![styled(disc, "margin: 0px 2px;")];
    if expanded {
        children.push(styled(
            widget_p_with_text(status.label.clone()),
            "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;",
        ));
        if !status.text.as_str().is_empty() {
            children.push(styled(
                widget_p_with_text(status.text.clone()),
                &format!("flex-shrink: 0; margin: 0px; font-size: 12px; color: {};", ctx.look.quiet),
            ));
        }
    }
    let invoker = styled(Dom::create_div().with_children(DomVec::from_vec(children)), &css)
        .with_ids_and_classes(classes(SIDEBAR_STATUS_CLASS, &[]))
        .with_accessibility_info(AccessibilityInfo::named(spoken.clone(), AccessibilityRole::PushButton))
        .with_tab_index(TabIndex::Auto);
    let invoker = if expanded {
        invoker
    } else {
        on_all(
            invoker,
            alloc::vec![
                callback(
                    EventFilter::Hover(HoverEventFilter::MouseEnter),
                    RefAny::new(Tip(AzString::from(spoken.as_str()))),
                    on_tip_show
                ),
                callback(EventFilter::Hover(HoverEventFilter::MouseLeave), RefAny::new(()), on_tip_hide),
            ],
        )
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
        &format!("display: flex; justify-content: {}; width: 100%; flex-shrink: 0;", if expanded { "flex-start" } else { "center" }),
    )
}

/// A button of the foot (help, the toggle): in the rail an icon button, expanded a row.
fn foot_button(
    ctx: &Ctx,
    icon_name: &str,
    label: &str,
    expanded: bool,
    class: &'static str,
    ev: SidebarEvent,
) -> Dom {
    let b = ctx.button();
    let h = ctx.row_h();
    let mut css = if expanded {
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 8px; box-sizing: border-box; \
             flex-shrink: 0; height: {h}px; margin: 1px 6px; padding: 0px 8px; border-radius: 6px; cursor: pointer; \
             width: calc(100% - 12px); "
        )
    } else {
        format!(
            "display: flex; align-items: center; justify-content: center; box-sizing: border-box; flex-shrink: 0; \
             width: {b}px; height: {b}px; border-radius: 8px; cursor: pointer; "
        )
    };
    css.push_str(&format!(
        ":hover {{ background: {}; }} :active {{ background: {}; }} :focus-visible {{ box-shadow: inset 0px 0px 0px 2px #4a8be8; }}",
        ctx.look.hover, ctx.look.hover
    ));
    let mut children: Vec<Dom> = alloc::vec![icon(icon_name, if expanded { 18.0 } else { 22.0 }, ctx.look.quiet)];
    if expanded {
        children.push(styled(
            widget_p_with_text(AzString::from(label)),
            "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; margin: 0px;",
        ));
    }
    let node = styled(Dom::create_div().with_children(DomVec::from_vec(children)), &css)
        .with_ids_and_classes(classes(class, &[]))
        .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::PushButton))
        .with_tab_index(TabIndex::Auto);
    let name = AzString::from(label);
    let mut list = alloc::vec![callback(EventFilter::Hover(HoverEventFilter::Click), ctx.part(ev), on_part_click)];
    if !expanded {
        list.push(callback(
            EventFilter::Hover(HoverEventFilter::MouseEnter),
            RefAny::new(Tip(name)),
            on_tip_show,
        ));
        list.push(callback(EventFilter::Hover(HoverEventFilter::MouseLeave), RefAny::new(()), on_tip_hide));
    }
    on_all(node, list)
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
            format!("Sidebar: {extra} status pie(s) past the limit of {MAX_STATUSES} were dropped")
        });
    }
    (sections, statuses)
}

/// The sidebar's DOM.
fn build(sb: Sidebar) -> Dom {
    let Sidebar {
        sections,
        statuses,
        state,
        surface,
        form,
        label,
        header,
        help,
        expanded_width,
        controls_left,
        controls_height: controls_h,
        handle_in_cycle,
        on_event,
        theme,
    } = sb;
    let phone = form.is_phone();
    let look = surface.look();
    let col = column_width(state, form, expanded_width, controls_left);
    let (sections, statuses) = limited(sections.into_library_owned_vec(), statuses.into_library_owned_vec());

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
        phone,
        look,
        on_event: on_event.clone(),
        stop,
    };

    let state_class = match state {
        SidebarState::Expanded => SIDEBAR_EXPANDED_CLASS,
        SidebarState::Rail => SIDEBAR_RAIL_CLASS,
        SidebarState::Handle => SIDEBAR_HANDLE_CLASS,
    };
    let mut root_css = format!(
        "display: flex; flex-direction: column; flex-shrink: 0; flex-grow: 0; height: 100%; box-sizing: border-box; \
         overflow: hidden; position: relative; font-size: 13px; user-select: none; \
         border-right: 1px solid system:separator; {} {}",
        width_css(col),
        look.root
    );
    if state != SidebarState::Expanded {
        root_css.push_str(" align-items: center;");
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
    if expanded {
        if let Some(h) = header.into_option() {
            kids.push(h);
        }
    }

    // ---- the list ----
    let mut list: Vec<Dom> = Vec::new();
    let mut index = 0usize;
    if expanded {
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
    } else {
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
                list.push(rule("width: 28px;"));
            }
            first_section = false;
            list.extend(buttons);
        }
    }
    let list_css = if expanded {
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; width: 100%; overflow-y: auto; \
         scrollbar-width: none; padding: 4px 0px 8px 0px;"
    } else {
        "display: flex; flex-direction: column; align-items: center; flex-grow: 1; min-height: 0px; width: 100%; \
         overflow-y: auto; scrollbar-width: none; padding: 6px 0px; gap: 2px;"
    };
    kids.push(
        styled(Dom::create_div().with_children(DomVec::from_vec(list)), list_css)
            .with_ids_and_classes(classes(SIDEBAR_LIST_CLASS, &[]))
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::Outline,
                ..Default::default()
            }),
    );

    // ---- the foot: help, the status pies, the toggle ----
    let mut foot: Vec<Dom> = Vec::new();
    if !help.as_str().is_empty() {
        foot.push(foot_button(
            &ctx,
            "help",
            help.as_str(),
            expanded,
            SIDEBAR_HELP_CLASS,
            event(SidebarEventKind::HelpClicked, state),
        ));
    }
    for status in &statuses {
        foot.push(status_dom(&ctx, status, expanded, theme));
    }
    let (toggle_icon, toggle_label) = if expanded {
        ("chevron_left", "Collapse the sidebar")
    } else {
        ("chevron_right", "Expand the sidebar")
    };
    foot.push(foot_button(
        &ctx,
        toggle_icon,
        toggle_label,
        expanded,
        SIDEBAR_TOGGLE_CLASS,
        event(SidebarEventKind::StateChanged, state.toggled(handle_in_cycle)),
    ));
    kids.push(rule("width: calc(100% - 20px);"));
    kids.push(
        styled(
            Dom::create_div().with_children(DomVec::from_vec(foot)),
            if expanded {
                "display: flex; flex-direction: column; flex-shrink: 0; width: 100%; padding: 0px 0px 8px 0px;"
            } else {
                "display: flex; flex-direction: column; align-items: center; flex-shrink: 0; width: 100%; \
                 padding: 0px 0px 8px 0px; gap: 2px;"
            },
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
