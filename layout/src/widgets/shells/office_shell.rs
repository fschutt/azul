//! `OfficeShell` - the frame every desktop shell is: Outlook 2010's window.
//!
//! ```text
//! ┌ title row (app-drawn Titlebar, or nothing under native decorations) ──┐
//! │ ribbon (or a toolbar)          -- or the full-window Backstage        │
//! ├────────────┬──────────────┬───────────────────────────┬──────────────┤
//! │ pane 0     │ pane 1       │ pane 2                    │ right bar    │
//! │ (nav)      │              │ (main)                    │ (To-Do bar)  │
//! ├────────────┴──────────────┴───────────────────────────┴──────────────┤
//! │ bottom pane (optional: a details pane, a terminal, a node graph)      │
//! ├───────────────────────────────────────────────────────────────────────┤
//! │ status bar                                                            │
//! └───────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Every slot is a `Dom` the app hands in. The shell owns:
//!
//! - the LAYOUT: the panes of the row sit between `SplitPane` splitters (a
//!   pane with a `width` is a rail that keeps it - a tool palette, an
//!   activity bar - and stands outside the splits), the bottom pane hangs
//!   under the row on a vertical splitter, the right bar keeps its width;
//! - the KEYBOARD: F6 moves the focus to the next pane, Shift+F6 to the
//!   previous one, round and round (Office's rule), reported through
//!   `on_pane_focus`; every pane is a keyboard stop with a focus ring;
//! - the LANDMARKS: a pane is a `<nav>`, `<main>`, `<aside>` or `<section>`
//!   element by its [`ShellPaneKind`], named by its label, so a screen reader
//!   lists the window's regions; the title row is a `<header>`, the status
//!   bar a `<footer>`;
//! - the THEMING: the shell's ground, hairlines and focus rings come from
//!   the theme (`ShellLook`), in both widget themes and both modes.
//!
//! A pane's `id` is its DOM id - what a test finds it by
//! (`get_node_layout`), and what F6 looks up.
//!
//! Key types: [`OfficeShell`], [`ShellPane`], [`ShellPaneKind`],
//! [`ShellOnPaneFocus`], [`ShellOnPaneResize`].

use alloc::vec::Vec;

use azul_core::{
    a11y::AccessibilityInfo,
    callbacks::{CoreCallback, CoreCallbackData, FocusTarget, Update},
    dom::{
        Dom, DomNodeId, DomVec, EventFilter, IdOrClass, IdOrClassVec, NodeType, OptionDom,
        TabIndex,
    },
    events::WindowEventFilter,
    refany::{OptionRefAny, RefAny},
    styled_dom::NodeHierarchyItemId,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{basic::pixel::PixelValue, layout::LayoutWidth, property::CssProperty},
    AzString,
};

use super::{
    id_and_class, inner_theme, look_for, part, root_classes, ShellLook, CHROME_ROW_BASE,
    FILL_COLUMN_BASE, GROW_COLUMN_BASE, GROW_ROW_BASE, PANE_BASE, RAIL_BASE,
};
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        split_pane::{SplitDirection, SplitPane, SplitPaneOnResizeCallbackType, SplitPaneState},
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The shell root's class.
pub const SHELL_CLASS: &str = "__azul-native-office-shell";
/// The title row host's class and id.
pub const TITLE_CLASS: &str = "__azul-native-office-shell-title";
/// The title row host's DOM id.
pub const TITLE_ID: &str = "shell-title";
/// The ribbon host's class.
pub const RIBBON_CLASS: &str = "__azul-native-office-shell-ribbon";
/// The ribbon host's DOM id.
pub const RIBBON_ID: &str = "shell-ribbon";
/// The backstage host's class.
pub const BACKSTAGE_CLASS: &str = "__azul-native-office-shell-backstage";
/// The backstage host's DOM id.
pub const BACKSTAGE_ID: &str = "shell-backstage";
/// The body's class (the row of panes and the right bar).
pub const BODY_CLASS: &str = "__azul-native-office-shell-body";
/// The class of the box that holds the split tree of the row's panes.
pub const SPLITS_CLASS: &str = "__azul-native-office-shell-splits";
/// A pane's class (a rail carries it too, with [`RAIL_CLASS`]).
pub const PANE_CLASS: &str = "__azul-native-office-shell-pane";
/// Added to a pane that keeps a fixed width.
pub const RAIL_CLASS: &str = "__azul-native-office-shell-rail";
/// The right bar's class.
pub const RIGHT_BAR_CLASS: &str = "__azul-native-office-shell-right-bar";
/// The right bar's DOM id.
pub const RIGHT_BAR_ID: &str = "shell-right-bar";
/// The status row host's class.
pub const STATUS_CLASS: &str = "__azul-native-office-shell-status";
/// The status row host's DOM id.
pub const STATUS_ID: &str = "shell-status";
/// The class of the box that stacks the body over the bottom pane.
pub const STACK_CLASS: &str = "__azul-native-office-shell-stack";

/// Callback invoked when F6 (or Shift+F6) moved the focus to a pane: the
/// index of the pane in the cycle (the row's visible panes in order, then
/// the right bar, then the bottom pane).
pub type ShellOnPaneFocusCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    ShellOnPaneFocus,
    OptionShellOnPaneFocus,
    ShellOnPaneFocusCallback,
    ShellOnPaneFocusCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellOnPaneFocusCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_ON_PANE_FOCUS_INVOKER,
    invoker_ty:     AzShellOnPaneFocusCallbackInvoker,
    thunk_fn:       az_shell_on_pane_focus_callback_thunk,
    setter_fn:      AzApp_setShellOnPaneFocusCallbackInvoker,
    from_handle_fn: AzShellOnPaneFocusCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellOnPaneFocusCallback_createFromHostHandleByref,
    extra_args:     [ pane_index: usize ],
}

/// Callback invoked while a splitter is dragged (or moved by its arrow
/// keys): the index of the pane LEFT of (or above) the splitter and the new
/// share that pane takes - what the app stores and hands back through
/// [`ShellPane::with_ratio`] when it rebuilds.
pub type ShellOnPaneResizeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, usize, f32) -> Update;
impl_widget_callback!(
    ShellOnPaneResize,
    OptionShellOnPaneResize,
    ShellOnPaneResizeCallback,
    ShellOnPaneResizeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ShellOnPaneResizeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SHELL_ON_PANE_RESIZE_INVOKER,
    invoker_ty:     AzShellOnPaneResizeCallbackInvoker,
    thunk_fn:       az_shell_on_pane_resize_callback_thunk,
    setter_fn:      AzApp_setShellOnPaneResizeCallbackInvoker,
    from_handle_fn: AzShellOnPaneResizeCallback_createFromHostHandle,
    from_handle_byref_fn: AzShellOnPaneResizeCallback_createFromHostHandleByref,
    extra_args:     [ pane_index: usize, ratio: f32 ],
}

/// What a pane is to assistive technology: the element it is built as.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ShellPaneKind {
    /// A `<section>`: a region named by its label (a list, a panel).
    #[default]
    Region = 0,
    /// A `<nav>`: the navigation pane, the folder tree, the sidebar.
    Navigation = 1,
    /// A `<main>`: the document, the reading pane, the canvas.
    Main = 2,
    /// An `<aside>`: a side pane (comments, styles, properties).
    Side = 3,
}

impl ShellPaneKind {
    /// The element a pane of this kind is built as.
    #[must_use]
    pub const fn node_type(self) -> NodeType {
        match self {
            Self::Region => NodeType::Section,
            Self::Navigation => NodeType::Nav,
            Self::Main => NodeType::Main,
            Self::Side => NodeType::Aside,
        }
    }
}

/// One pane of the shell: its content, its DOM id, its accessible label,
/// its place in the splits.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellPane {
    /// The app's content.
    pub content: Dom,
    /// The pane's DOM id ("shell-navigation"): what a test and F6 find it by.
    pub id: AzString,
    /// The pane's accessible name ("Navigation", "Message list").
    pub label: AzString,
    /// The pane's share of the splitter it is the first pane of (the space
    /// between it and everything right of it in the row); `0.0` = an even
    /// share. Ignored for the last pane of the row and for a rail.
    pub ratio: f32,
    /// A fixed width in px makes the pane a RAIL that stands outside the
    /// splits (a tool palette, an activity bar); `0.0` = a flexible pane.
    pub width: f32,
    /// The element the pane is built as.
    pub kind: ShellPaneKind,
    /// A hidden pane is left out of the tree and of the F6 cycle (a view
    /// toggle rebuilds).
    pub visible: bool,
}

impl_option!(ShellPane, OptionShellPane, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    ShellPane,
    ShellPaneVec,
    ShellPaneVecDestructor,
    ShellPaneVecDestructorType,
    ShellPaneVecSlice,
    OptionShellPane
);
impl_vec_clone!(ShellPane, ShellPaneVec, ShellPaneVecDestructor);
impl_vec_debug!(ShellPane, ShellPaneVec);
impl_vec_partialeq!(ShellPane, ShellPaneVec);
impl_vec_mut!(ShellPane, ShellPaneVec);

impl ShellPane {
    /// A visible, flexible region pane with `id`, named by its id until
    /// [`Self::with_label`].
    #[must_use]
    pub fn create(id: AzString, content: Dom) -> Self {
        Self {
            content,
            label: id.clone(),
            id,
            ratio: 0.0,
            width: 0.0,
            kind: ShellPaneKind::Region,
            visible: true,
        }
    }

    /// The pane's accessible name.
    pub fn set_label(&mut self, label: AzString) {
        self.label = label;
    }

    /// [`Self::set_label`] for the builder chain.
    #[must_use]
    pub fn with_label(mut self, label: AzString) -> Self {
        self.set_label(label);
        self
    }

    /// The element the pane is built as.
    pub const fn set_kind(&mut self, kind: ShellPaneKind) {
        self.kind = kind;
    }

    /// [`Self::set_kind`] for the builder chain.
    #[must_use]
    pub const fn with_kind(mut self, kind: ShellPaneKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// The pane's share of its splitter (see [`Self::ratio`]).
    pub const fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio;
    }

    /// [`Self::set_ratio`] for the builder chain.
    #[must_use]
    pub const fn with_ratio(mut self, ratio: f32) -> Self {
        self.set_ratio(ratio);
        self
    }

    /// A fixed width: the pane becomes a rail (see [`Self::width`]).
    pub const fn set_width(&mut self, width: f32) {
        self.width = width;
    }

    /// [`Self::set_width`] for the builder chain.
    #[must_use]
    pub const fn with_width(mut self, width: f32) -> Self {
        self.set_width(width);
        self
    }

    /// Show or hide the pane.
    pub const fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// [`Self::set_visible`] for the builder chain.
    #[must_use]
    pub const fn with_visible(mut self, visible: bool) -> Self {
        self.set_visible(visible);
        self
    }

    /// Whether the pane is a rail.
    #[must_use]
    pub fn is_rail(&self) -> bool {
        self.width > 0.0
    }
}

/// The Office-like window frame: title row, ribbon or backstage, the panes,
/// the right bar, the bottom pane, the status bar.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct OfficeShell {
    /// The app-drawn title row (a `Titlebar`), or nothing under native
    /// decorations.
    pub title_row: OptionDom,
    /// The ribbon (or a toolbar). Hidden while the backstage is open.
    pub ribbon: OptionDom,
    /// The backstage: when set, it takes the whole window under the title
    /// row, in place of the ribbon and the panes (Office's FILE view).
    pub backstage: OptionDom,
    /// The row's panes, left to right.
    pub panes: ShellPaneVec,
    /// A pane under the row (a details pane, a terminal, a node graph),
    /// on a vertical splitter; its `ratio` is ITS share of the height.
    pub bottom: OptionShellPane,
    /// The right bar (Outlook's To-Do bar): keeps its width, outside the
    /// splits.
    pub right_bar: OptionDom,
    /// The status bar.
    pub status_bar: OptionDom,
    /// F6 moved the focus to a pane.
    pub on_pane_focus: OptionShellOnPaneFocus,
    /// A splitter moved.
    pub on_pane_resize: OptionShellOnPaneResize,
    /// The right bar's accessible name.
    pub right_bar_label: AzString,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl OfficeShell {
    /// An empty shell: no title row, no ribbon, no panes, no bars.
    #[must_use]
    pub fn create() -> Self {
        Self {
            title_row: OptionDom::None,
            ribbon: OptionDom::None,
            backstage: OptionDom::None,
            panes: ShellPaneVec::from_const_slice(&[]),
            bottom: OptionShellPane::None,
            right_bar: OptionDom::None,
            status_bar: OptionDom::None,
            on_pane_focus: None.into(),
            on_pane_resize: None.into(),
            right_bar_label: AzString::from_const_str("To-Do bar"),
            theme: OptionUiTheme::None,
        }
    }

    /// The app-drawn title row.
    pub fn set_title_row(&mut self, title_row: Dom) {
        self.title_row = OptionDom::Some(title_row);
    }

    /// [`Self::set_title_row`] for the builder chain.
    #[must_use]
    pub fn with_title_row(mut self, title_row: Dom) -> Self {
        self.set_title_row(title_row);
        self
    }

    /// The ribbon (or a toolbar).
    pub fn set_ribbon(&mut self, ribbon: Dom) {
        self.ribbon = OptionDom::Some(ribbon);
    }

    /// [`Self::set_ribbon`] for the builder chain.
    #[must_use]
    pub fn with_ribbon(mut self, ribbon: Dom) -> Self {
        self.set_ribbon(ribbon);
        self
    }

    /// The backstage, shown in place of the ribbon and the panes.
    pub fn set_backstage(&mut self, backstage: Dom) {
        self.backstage = OptionDom::Some(backstage);
    }

    /// [`Self::set_backstage`] for the builder chain.
    #[must_use]
    pub fn with_backstage(mut self, backstage: Dom) -> Self {
        self.set_backstage(backstage);
        self
    }

    /// Appends a pane to the row.
    pub fn add_pane(&mut self, pane: ShellPane) {
        let mut panes = self.panes.clone().into_library_owned_vec();
        panes.push(pane);
        self.panes = ShellPaneVec::from_vec(panes);
    }

    /// [`Self::add_pane`] for the builder chain.
    #[must_use]
    pub fn with_pane(mut self, pane: ShellPane) -> Self {
        self.add_pane(pane);
        self
    }

    /// Replaces the row's panes.
    pub fn set_panes(&mut self, panes: ShellPaneVec) {
        self.panes = panes;
    }

    /// [`Self::set_panes`] for the builder chain.
    #[must_use]
    pub fn with_panes(mut self, panes: ShellPaneVec) -> Self {
        self.set_panes(panes);
        self
    }

    /// The pane under the row.
    pub fn set_bottom(&mut self, bottom: ShellPane) {
        self.bottom = OptionShellPane::Some(bottom);
    }

    /// [`Self::set_bottom`] for the builder chain.
    #[must_use]
    pub fn with_bottom(mut self, bottom: ShellPane) -> Self {
        self.set_bottom(bottom);
        self
    }

    /// The right bar.
    pub fn set_right_bar(&mut self, right_bar: Dom) {
        self.right_bar = OptionDom::Some(right_bar);
    }

    /// [`Self::set_right_bar`] for the builder chain.
    #[must_use]
    pub fn with_right_bar(mut self, right_bar: Dom) -> Self {
        self.set_right_bar(right_bar);
        self
    }

    /// The right bar's accessible name.
    pub fn set_right_bar_label(&mut self, label: AzString) {
        self.right_bar_label = label;
    }

    /// [`Self::set_right_bar_label`] for the builder chain.
    #[must_use]
    pub fn with_right_bar_label(mut self, label: AzString) -> Self {
        self.set_right_bar_label(label);
        self
    }

    /// The status bar.
    pub fn set_status_bar(&mut self, status_bar: Dom) {
        self.status_bar = OptionDom::Some(status_bar);
    }

    /// [`Self::set_status_bar`] for the builder chain.
    #[must_use]
    pub fn with_status_bar(mut self, status_bar: Dom) -> Self {
        self.set_status_bar(status_bar);
        self
    }

    /// F6 moved the focus to a pane.
    pub fn set_on_pane_focus<C: Into<ShellOnPaneFocusCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_pane_focus = Some(ShellOnPaneFocus {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_pane_focus`] for the builder chain.
    #[must_use]
    pub fn with_on_pane_focus<C: Into<ShellOnPaneFocusCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_pane_focus(data, callback);
        self
    }

    /// A splitter moved.
    pub fn set_on_pane_resize<C: Into<ShellOnPaneResizeCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_pane_resize = Some(ShellOnPaneResize {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_pane_resize`] for the builder chain.
    #[must_use]
    pub fn with_on_pane_resize<C: Into<ShellOnPaneResizeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_pane_resize(data, callback);
        self
    }

    /// Pin the widget theme; unset, the shell follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty shell and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// The ids of the panes F6 cycles through, in order: the row's visible
    /// panes, the right bar, the bottom pane.
    #[must_use]
    pub(crate) fn cycle_ids(&self) -> Vec<AzString> {
        let mut ids: Vec<AzString> = self
            .panes
            .as_ref()
            .iter()
            .filter(|p| p.visible)
            .map(|p| p.id.clone())
            .collect();
        if self.right_bar.is_some() {
            ids.push(AzString::from_const_str(RIGHT_BAR_ID));
        }
        if let OptionShellPane::Some(b) = &self.bottom {
            if b.visible {
                ids.push(b.id.clone());
            }
        }
        ids
    }

    /// The shell's DOM. The look comes from the theme module
    /// (`themes::flat::shell_look` / `themes::flora::shell_look`); `None`
    /// carries both looks, each in its `@theme(<name>)` block, built ONCE
    /// from the merged look so the app's content is never cloned.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for OfficeShell {
    fn default() -> Self {
        Self::create()
    }
}

impl From<OfficeShell> for Dom {
    fn from(s: OfficeShell) -> Self {
        s.dom()
    }
}

// ---------------------------------------------------------------------------
// F6: the pane cycle
// ---------------------------------------------------------------------------

/// The window-level F6 handler's payload: the cycle's pane ids, the pane
/// last reached, the app's callback.
struct PaneCycle {
    ids: Vec<AzString>,
    on_pane_focus: OptionShellOnPaneFocus,
    active: usize,
    reached: bool,
}

/// The node with the DOM id `id` in the DOM the hit node belongs to.
fn node_by_id(info: &CallbackInfo, id: &AzString) -> Option<DomNodeId> {
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, id.as_str())?;
    Some(DomNodeId {
        dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
}

/// The index in `ids` of the pane that holds the keyboard focus now: the
/// focused node, or the nearest ancestor of it, whose DOM id is one of
/// `ids`. `None` when nothing in a pane is focused.
fn focused_pane(info: &CallbackInfo, ids: &[AzString]) -> Option<usize> {
    let mut node = info
        .get_focused_node_for_seat(azul_core::window::PRIMARY_POINTER_SEAT)
        .into_option()?;
    loop {
        if let Some(id) = info.get_node_id(node) {
            if let Some(i) = ids.iter().position(|x| x.as_str() == id.as_str()) {
                return Some(i);
            }
        }
        node = info.get_parent(node)?;
    }
}

/// The pane F6 (or Shift+F6) goes to from `current`, in a cycle of `count`
/// panes: the next (or previous) one, wrapping; the first pane when nothing
/// has been reached yet.
#[must_use]
pub(crate) const fn f6_target(current: Option<usize>, count: usize, backwards: bool) -> usize {
    match current {
        None => {
            if backwards && count > 0 {
                count - 1
            } else {
                0
            }
        }
        Some(i) => {
            if backwards {
                if i == 0 || i >= count {
                    count.saturating_sub(1)
                } else {
                    i - 1
                }
            } else if i + 1 >= count {
                0
            } else {
                i + 1
            }
        }
    }
}

/// Window-level: F6 / Shift+F6 move the focus through the panes. Any other
/// key, or F6 with Ctrl, Alt or Cmd held, is left to the app and the OS.
extern "C" fn on_shell_key_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ks = info.get_current_keyboard_state();
    if !matches!(
        ks.current_virtual_keycode.into_option(),
        Some(VirtualKeyCode::F6)
    ) || ks.ctrl_down()
        || ks.alt_down()
        || ks.super_down()
    {
        return Update::DoNothing;
    }
    let backwards = ks.shift_down();
    let (ids, on_pane_focus, remembered) = {
        let Some(cycle) = data.downcast_ref::<PaneCycle>() else {
            return Update::DoNothing;
        };
        (
            cycle.ids.clone(),
            cycle.on_pane_focus.clone(),
            if cycle.reached { Some(cycle.active) } else { None },
        )
    };
    if ids.is_empty() {
        return Update::DoNothing;
    }
    let current = focused_pane(&info, &ids).or(remembered);
    let target = f6_target(current, ids.len(), backwards);
    let Some(node) = node_by_id(&info, &ids[target]) else {
        return Update::DoNothing;
    };
    info.set_focus(FocusTarget::Id(node));
    info.prevent_default();
    if let Some(mut cycle) = data.downcast_mut::<PaneCycle>() {
        cycle.active = target;
        cycle.reached = true;
    }
    match on_pane_focus.into_option() {
        Some(ShellOnPaneFocus { refany, callback }) => callback.invoke(refany, info, target),
        None => Update::DoNothing,
    }
}

// ---------------------------------------------------------------------------
// Splitters: the resize report
// ---------------------------------------------------------------------------

/// A splitter's payload: which pane sits before it, the app's callback.
struct SplitRef {
    pane_index: usize,
    on_pane_resize: OptionShellOnPaneResize,
}

extern "C" fn on_split_resize(mut data: RefAny, info: CallbackInfo, state: SplitPaneState) -> Update {
    let (pane_index, on_pane_resize) = {
        let Some(split) = data.downcast_ref::<SplitRef>() else {
            return Update::DoNothing;
        };
        (split.pane_index, split.on_pane_resize.clone())
    };
    match on_pane_resize.into_option() {
        Some(ShellOnPaneResize { refany, callback }) => {
            callback.invoke(refany, info, pane_index, state.ratio)
        }
        None => Update::DoNothing,
    }
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

/// A pane's node: its element, id, label, base and skin, a keyboard stop.
/// A rail's box is its base and skin, then the width the app asked for.
fn pane_node(pane: ShellPane, look: &ShellLook) -> Dom {
    let (classes, css) = if pane.is_rail() {
        let mut classes = id_and_class(&pane.id, PANE_CLASS).into_library_owned_vec();
        classes.push(IdOrClass::Class(AzString::from_const_str(RAIL_CLASS)));
        let mut skin: Vec<CssPropertyWithConditions> = look.shell_rail.clone();
        #[allow(clippy::cast_possible_truncation)]
        skin.push(CssPropertyWithConditions::simple(CssProperty::const_width(
            LayoutWidth::Px(PixelValue::const_px(pane.width as isize)),
        )));
        (IdOrClassVec::from_vec(classes), part(RAIL_BASE, &skin))
    } else {
        (
            id_and_class(&pane.id, PANE_CLASS),
            part(PANE_BASE, &look.shell_pane),
        )
    };
    Dom::create_node(pane.kind.node_type())
        .with_ids_and_classes(classes)
        .with_css_props(css)
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo::named(
            pane.label.clone(),
            azul_core::a11y::AccessibilityRole::Pane,
        ))
        .with_child(pane.content)
}

/// A splitter between `first` and `rest`.
fn split(
    direction: SplitDirection,
    first: Dom,
    rest: Dom,
    ratio: f32,
    pane_index: usize,
    on_pane_resize: &OptionShellOnPaneResize,
    inner: Option<UiTheme>,
) -> Dom {
    let mut sp = SplitPane::create(direction, first, rest).with_ratio(ratio);
    if let Some(t) = inner {
        sp = sp.with_theme(t);
    }
    if on_pane_resize.is_some() {
        let on_resize: SplitPaneOnResizeCallbackType = on_split_resize;
        sp = sp.with_on_resize(
            RefAny::new(SplitRef {
                pane_index,
                on_pane_resize: on_pane_resize.clone(),
            }),
            on_resize,
        );
    }
    sp.dom()
}

/// The split tree of the flexible panes `panes` (at least one), whose first
/// pane is pane `first_index` of the row.
fn split_tree(
    mut panes: Vec<ShellPane>,
    first_index: usize,
    on_pane_resize: &OptionShellOnPaneResize,
    inner: Option<UiTheme>,
    look: &ShellLook,
) -> Dom {
    let count = panes.len();
    let first = panes.remove(0);
    if panes.is_empty() {
        return pane_node(first, look);
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = if first.ratio > 0.0 {
        first.ratio
    } else {
        1.0 / count as f32
    };
    let rest = split_tree(panes, first_index + 1, on_pane_resize, inner, look);
    split(
        SplitDirection::Horizontal,
        pane_node(first, look),
        rest,
        ratio,
        first_index,
        on_pane_resize,
        inner,
    )
}

/// The row's children: rails where they stand, every run of flexible panes
/// between them as one split tree.
fn row_children(
    panes: Vec<ShellPane>,
    on_pane_resize: &OptionShellOnPaneResize,
    inner: Option<UiTheme>,
    look: &ShellLook,
) -> Vec<Dom> {
    let mut out: Vec<Dom> = Vec::new();
    let mut run: Vec<ShellPane> = Vec::new();
    let mut run_start = 0;
    for (i, pane) in panes.into_iter().enumerate() {
        if pane.is_rail() {
            if !run.is_empty() {
                out.push(splits_box(core::mem::take(&mut run), run_start, on_pane_resize, inner, look));
            }
            out.push(pane_node(pane, look));
        } else {
            if run.is_empty() {
                run_start = i;
            }
            run.push(pane);
        }
    }
    if !run.is_empty() {
        out.push(splits_box(run, run_start, on_pane_resize, inner, look));
    }
    out
}

/// The box that holds a split tree: a row that takes the space the rails
/// leave, so the split pane's `100%` has something to be a percentage of.
fn splits_box(
    run: Vec<ShellPane>,
    first_index: usize,
    on_pane_resize: &OptionShellOnPaneResize,
    inner: Option<UiTheme>,
    look: &ShellLook,
) -> Dom {
    Dom::create_div()
        .with_class(AzString::from_const_str(SPLITS_CLASS))
        .with_css_props(part(GROW_ROW_BASE, &[]))
        .with_child(split_tree(run, first_index, on_pane_resize, inner, look))
}

/// A chrome row host (the title row, the ribbon, the status bar).
fn chrome_row(node_type: NodeType, id: &str, class: &'static str, skin: &[CssPropertyWithConditions], content: Dom) -> Dom {
    Dom::create_node(node_type)
        .with_ids_and_classes(id_and_class(&AzString::from(id), class))
        .with_css_props(part(CHROME_ROW_BASE, skin))
        .with_child(content)
}

/// The shell's DOM in `look`: root [title row?, (backstage | ribbon?, body
/// [splits and rails, right bar?] (over the bottom pane)?), status bar?].
pub(crate) fn build(shell: OfficeShell, look: &ShellLook) -> Dom {
    let ids = shell.cycle_ids();
    let OfficeShell {
        title_row,
        ribbon,
        backstage,
        panes,
        bottom,
        right_bar,
        status_bar,
        on_pane_focus,
        on_pane_resize,
        right_bar_label,
        theme,
    } = shell;
    let inner = inner_theme(theme);
    let mut children: Vec<Dom> = Vec::with_capacity(4);

    if let Some(t) = title_row.into_option() {
        children.push(chrome_row(NodeType::Header, TITLE_ID, TITLE_CLASS, &look.shell_title, t));
    }

    if let Some(bs) = backstage.into_option() {
        children.push(
            Dom::create_div()
                .with_ids_and_classes(id_and_class(
                    &AzString::from_const_str(BACKSTAGE_ID),
                    BACKSTAGE_CLASS,
                ))
                .with_css_props(part(GROW_COLUMN_BASE, &look.shell_backstage))
                .with_child(bs),
        );
    } else {
        if let Some(r) = ribbon.into_option() {
            children.push(chrome_row(NodeType::Div, RIBBON_ID, RIBBON_CLASS, &look.shell_ribbon, r));
        }
        let visible: Vec<ShellPane> = panes
            .into_library_owned_vec()
            .into_iter()
            .filter(|p| p.visible)
            .collect();
        let mut body_children = row_children(visible, &on_pane_resize, inner, look);
        if let Some(rb) = right_bar.into_option() {
            body_children.push(
                Dom::create_node(NodeType::Aside)
                    .with_ids_and_classes(id_and_class(
                        &AzString::from_const_str(RIGHT_BAR_ID),
                        RIGHT_BAR_CLASS,
                    ))
                    .with_css_props(part(RAIL_BASE, &look.shell_right_bar))
                    .with_tab_index(TabIndex::Auto)
                    .with_accessibility_info(AccessibilityInfo::named(
                        right_bar_label,
                        azul_core::a11y::AccessibilityRole::Pane,
                    ))
                    .with_child(rb),
            );
        }
        let body = Dom::create_div()
            .with_class(AzString::from_const_str(BODY_CLASS))
            .with_css_props(part(GROW_ROW_BASE, &look.shell_body))
            .with_children(DomVec::from_vec(body_children));
        match bottom.into_option().filter(|b| b.visible) {
            Some(b) => {
                let share = if b.ratio > 0.0 { b.ratio } else { 0.3 };
                let pane_index = ids.len().saturating_sub(1);
                let stack = split(
                    SplitDirection::Vertical,
                    body,
                    pane_node(b, look),
                    1.0 - share,
                    pane_index,
                    &on_pane_resize,
                    inner,
                );
                children.push(
                    Dom::create_div()
                        .with_class(AzString::from_const_str(STACK_CLASS))
                        .with_css_props(part(GROW_COLUMN_BASE, &[]))
                        .with_child(stack),
                );
            }
            None => children.push(body),
        }
    }

    if let Some(s) = status_bar.into_option() {
        children.push(chrome_row(NodeType::Footer, STATUS_ID, STATUS_CLASS, &look.shell_status, s));
    }

    let mut root = Dom::create_div()
        .with_ids_and_classes(root_classes(SHELL_CLASS, look))
        .with_css_props(part(FILL_COLUMN_BASE, &look.shell_root))
        .with_children(DomVec::from_vec(children));

    // F6 cycles the panes: window-level, focus-independent.
    if !ids.is_empty() {
        root = root.with_callbacks(
            alloc::vec![CoreCallbackData {
                event: EventFilter::Window(WindowEventFilter::VirtualKeyDown),
                callback: CoreCallback {
                    cb: on_shell_key_down as usize,
                    ctx: OptionRefAny::None,
                },
                refany: RefAny::new(PaneCycle {
                    ids,
                    on_pane_focus,
                    active: 0,
                    reached: false,
                }),
            }]
            .into(),
        );
    }
    root
}

#[cfg(test)]
mod office_shell_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        a11y::AccessibilityRole,
        dom::{DomId, IdOrClass},
        styled_dom::StyledDom,
        window::VirtualKeyCode,
    };

    use super::*;
    use crate::widgets::{
        shells::fixtures::{office_shell, slot},
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn id_of(node: &Dom) -> Option<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .find_map(|c| match c {
                IdOrClass::Id(s) => Some(s.as_str().to_string()),
                IdOrClass::Class(_) => None,
            })
    }

    /// Every node with an id, depth first, with its element.
    fn ids_and_types(dom: &Dom) -> Vec<(String, NodeType)> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| {
                id_of(n).map(|id| (id, n.root.get_node_type().clone()))
            })
            .collect()
    }

    #[test]
    fn the_slots_come_in_office_order_title_ribbon_body_status() {
        let dom = office_shell().with_theme(UiTheme::Flat).dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 4, "title, ribbon, body, status");
        assert_eq!(id_of(&kids[0]).as_deref(), Some(TITLE_ID));
        assert!(matches!(kids[0].root.get_node_type(), NodeType::Header));
        assert_eq!(id_of(&kids[1]).as_deref(), Some(RIBBON_ID));
        assert!(tc::has_class(&kids[2], BODY_CLASS));
        assert_eq!(id_of(&kids[3]).as_deref(), Some(STATUS_ID));
        assert!(matches!(kids[3].root.get_node_type(), NodeType::Footer));
    }

    #[test]
    fn the_panes_are_landmarks_named_by_their_labels_in_row_order() {
        let dom = office_shell().with_theme(UiTheme::Flat).dom();
        let found = ids_and_types(&dom);
        let panes: Vec<&(String, NodeType)> = found
            .iter()
            .filter(|(id, _)| id.starts_with("shell-"))
            .collect();
        let order: Vec<&str> = panes.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            order,
            vec![
                TITLE_ID,
                RIBBON_ID,
                "shell-navigation",
                "shell-list",
                "shell-reading",
                RIGHT_BAR_ID,
                STATUS_ID
            ]
        );
        let kind = |id: &str| {
            found
                .iter()
                .find(|(x, _)| x == id)
                .map(|(_, t)| t.clone())
                .expect(id)
        };
        assert!(matches!(kind("shell-navigation"), NodeType::Nav));
        assert!(matches!(kind("shell-list"), NodeType::Section));
        assert!(matches!(kind("shell-reading"), NodeType::Main));
        assert!(matches!(kind(RIGHT_BAR_ID), NodeType::Aside));
        let nav = tc::find(&dom, PANE_CLASS).expect("a pane");
        let info = nav.root.get_accessibility_info().expect("a11y");
        assert_eq!(info.role, AccessibilityRole::Pane);
        assert_eq!(
            info.accessibility_name.as_ref().map(|s| s.as_str()),
            Some("Navigation")
        );
        assert!(nav.root.get_tab_index().is_some(), "F6 needs a stop");
    }

    #[test]
    fn three_flexible_panes_sit_on_two_nested_splitters_with_their_ratios() {
        let dom = office_shell().with_theme(UiTheme::Flat).dom();
        let splits = tc::find_all(&dom, "__azul-native-split-pane");
        assert_eq!(splits.len(), 2, "nav | (list | reading)");
        // The outer split's first pane holds the navigation pane; its second
        // pane holds the inner split.
        let outer = splits[0];
        let first = &outer.children.as_ref()[0];
        assert_eq!(id_of(&first.children.as_ref()[0]).as_deref(), Some("shell-navigation"));
        let second = &outer.children.as_ref()[2];
        assert!(tc::has_class(&second.children.as_ref()[0], "__azul-native-split-pane"));
    }

    #[test]
    fn a_hidden_pane_leaves_the_tree_and_the_cycle() {
        let mut shell = office_shell();
        let mut panes = shell.panes.clone().into_library_owned_vec();
        panes[1].visible = false;
        shell.panes = ShellPaneVec::from_vec(panes);
        assert_eq!(
            shell.cycle_ids().iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            vec!["shell-navigation", "shell-reading", RIGHT_BAR_ID]
        );
        let dom = shell.with_theme(UiTheme::Flat).dom();
        assert!(tc::nodes(&dom)
            .iter()
            .all(|(_, n)| id_of(n).as_deref() != Some("shell-list")));
        assert_eq!(tc::find_all(&dom, "__azul-native-split-pane").len(), 1);
    }

    #[test]
    fn a_rail_keeps_its_width_outside_the_splits() {
        let dom = OfficeShell::create()
            .with_pane(
                ShellPane::create(AzString::from("shell-tools"), slot())
                    .with_width(44.0)
                    .with_label(AzString::from("Tools")),
            )
            .with_pane(ShellPane::create(AzString::from("shell-canvas"), slot()))
            .with_pane(ShellPane::create(AzString::from("shell-panels"), slot()))
            .with_theme(UiTheme::Flat)
            .dom();
        let body = tc::find(&dom, BODY_CLASS).expect("body");
        let kids = body.children.as_ref();
        assert_eq!(kids.len(), 2, "the rail, then the splits box");
        assert!(tc::has_class(&kids[0], RAIL_CLASS));
        assert_eq!(id_of(&kids[0]).as_deref(), Some("shell-tools"));
        assert!(tc::has_class(&kids[1], SPLITS_CLASS));
        assert_eq!(tc::find_all(&dom, "__azul-native-split-pane").len(), 1);
        let width = tc::resolve(&kids[0], azul_css::props::property::CssPropertyType::Width, false, None);
        assert!(
            matches!(width, Some(CssProperty::Width(_))),
            "the rail declares its width: {width:?}"
        );
    }

    #[test]
    fn the_backstage_replaces_the_ribbon_and_the_panes_under_the_title_row() {
        let dom = office_shell()
            .with_backstage(slot())
            .with_theme(UiTheme::Flat)
            .dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 3, "title, backstage, status");
        assert_eq!(id_of(&kids[1]).as_deref(), Some(BACKSTAGE_ID));
        assert!(tc::find(&dom, BODY_CLASS).is_none());
        assert!(tc::find(&dom, RIBBON_CLASS).is_none());
    }

    #[test]
    fn the_bottom_pane_hangs_under_the_row_on_a_vertical_splitter_and_ends_the_cycle() {
        let shell = office_shell().with_bottom(
            ShellPane::create(AzString::from("shell-details"), slot())
                .with_label(AzString::from("Details"))
                .with_ratio(0.25),
        );
        assert_eq!(
            shell.cycle_ids().last().map(|s| s.as_str()),
            Some("shell-details")
        );
        let dom = shell.with_theme(UiTheme::Flat).dom();
        let splits = tc::find_all(&dom, "__azul-native-split-pane");
        assert_eq!(splits.len(), 3, "the vertical one and the row's two");
        let vertical = splits[0];
        assert!(tc::has_class(
            &vertical.children.as_ref()[0].children.as_ref()[0],
            BODY_CLASS
        ));
        assert_eq!(id_of(&vertical.children.as_ref()[2].children.as_ref()[0]).as_deref(), Some("shell-details")
        );
    }

    #[test]
    fn the_f6_target_walks_the_cycle_both_ways_and_starts_at_the_ends() {
        assert_eq!(f6_target(None, 3, false), 0);
        assert_eq!(f6_target(None, 3, true), 2);
        assert_eq!(f6_target(Some(0), 3, false), 1);
        assert_eq!(f6_target(Some(2), 3, false), 0);
        assert_eq!(f6_target(Some(0), 3, true), 2);
        assert_eq!(f6_target(Some(1), 3, true), 0);
        assert_eq!(f6_target(Some(9), 3, false), 0, "a stale index wraps");
        assert_eq!(f6_target(Some(9), 3, true), 2);
        assert_eq!(f6_target(None, 0, false), 0);
    }

    #[test]
    fn the_root_listens_for_f6_at_window_level_only_when_it_has_panes() {
        let dom = office_shell().with_theme(UiTheme::Flat).dom();
        let events: Vec<EventFilter> = dom
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|cb| cb.event)
            .collect();
        assert_eq!(
            events,
            vec![EventFilter::Window(WindowEventFilter::VirtualKeyDown)]
        );
        let bare = OfficeShell::create()
            .with_title_row(slot())
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(bare.root.get_callbacks().as_ref().is_empty());
    }

    type Log = Arc<Mutex<Vec<usize>>>;

    extern "C" fn record_pane(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(index);
        }
        Update::DoNothing
    }

    /// Presses `key` (with `held`) as the window delivers it to the shell
    /// root's window-level handler.
    fn press_window(
        styled: &StyledDom,
        key: VirtualKeyCode,
        held: &[VirtualKeyCode],
    ) -> Option<(Update, Vec<crate::callbacks::CallbackChange>)> {
        let root = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(azul_core::id::NodeId::ZERO)),
        };
        crate::widgets::roving::test_support::press_window(styled, root, key, held)
    }

    #[test]
    fn f6_focuses_the_first_pane_then_the_next_and_shift_f6_goes_back() {
        use crate::widgets::roving::test_support as rv;
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = office_shell()
            .with_on_pane_focus(RefAny::new(log.clone()), record_pane as ShellOnPaneFocusCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        let styled = StyledDom::create_from_dom(dom);
        let (_, changes) = press_window(&styled, VirtualKeyCode::F6, &[]).expect("the F6 hook");
        assert!(rv::prevented(&changes), "F6 is the shell's");
        let first = rv::focus_request(&changes).expect("focus moved");
        let (_, changes) = press_window(&styled, VirtualKeyCode::F6, &[]).expect("the F6 hook");
        let second = rv::focus_request(&changes).expect("focus moved again");
        assert_ne!(first, second, "F6 moves on");
        let (_, changes) =
            press_window(&styled, VirtualKeyCode::F6, &[VirtualKeyCode::LShift]).expect("hook");
        assert_eq!(rv::focus_request(&changes), Some(first), "Shift+F6 goes back");
        assert_eq!(*log.lock().expect("log"), vec![0, 1, 0]);
    }

    #[test]
    fn f6_with_ctrl_or_alt_is_left_to_the_app() {
        use crate::widgets::roving::test_support as rv;
        let styled =
            StyledDom::create_from_dom(office_shell().with_theme(UiTheme::Flat).dom());
        for held in [VirtualKeyCode::LControl, VirtualKeyCode::LAlt, VirtualKeyCode::LWin] {
            let (_, changes) = press_window(&styled, VirtualKeyCode::F6, &[held]).expect("hook");
            assert!(rv::focus_request(&changes).is_none(), "{held:?}+F6 must not move focus");
            assert!(!rv::prevented(&changes));
        }
        let (_, changes) = press_window(&styled, VirtualKeyCode::Tab, &[]).expect("hook");
        assert!(rv::focus_request(&changes).is_none(), "Tab is the engine's");
    }

    #[test]
    fn every_pane_is_ringed_on_focus_in_both_modes_and_themes() {
        for theme in checks::BOTH {
            let dom = office_shell().with_theme(theme).dom();
            for pane in tc::find_all(&dom, PANE_CLASS) {
                assert!(tc::has_focus_ring(pane, false), "{}: light ring", theme.name());
                assert!(tc::has_focus_ring(pane, true), "{}: dark ring", theme.name());
            }
        }
    }

    #[test]
    fn a_shell_without_a_theme_follows_the_app_theme_and_a_flora_one_is_marked() {
        checks::assert_follows_the_app_theme(
            "office_shell",
            || office_shell().dom(),
            |t: UiTheme| office_shell().with_theme(t).dom(),
        );
        assert!(tc::has_class(
            &office_shell().with_theme(UiTheme::Flora).dom(),
            "__azul-theme-flora"
        ));
    }
}
