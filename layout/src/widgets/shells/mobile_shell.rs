//! S11 - Mobile stack: every phone layout (04-app-shells.md, S11).
//!
//! ```text
//! ┌──────────────────────────┐
//! │ <-  Inbox          🔍  ⋮ │  app bar: back, title, actions
//! ├──────────────────────────┤
//! │ Anna Berg          10:42 │
//! │ Re: Quarterly numbers    │  the top page of the stack
//! │ Build bot          09:13 │
//! │                    (+)   │  the floating action button
//! ├──────────────────────────┤
//! │  ✉      📅      👥     ⚙  │  bottom tab bar
//! └──────────────────────────┘
//! ```
//!
//! A STACK of pages the app pushes and pops (`pages`: the last one is
//! shown; the app owns the stack and rebuilds), an APP BAR with a Back
//! button whenever there is a page to go back to (or the app asks for
//! one), the title and an actions slot, a page host the FAB floats over,
//! and a BOTTOM TAB BAR of icon-over-label tabs (one Tab stop; Left /
//! Right / Home / End move, Enter or a tap chooses). Each desktop shell maps
//! to a stack: S4's list pushes the detail, S5's grid pushes the viewer.
//!
//! Phone-width rules: the app bar takes a top inset for the safe area
//! ([`MobileShell::with_top_inset`]); under [`NARROW_MAX_PX`] the tab labels
//! hide and the icons stand alone (a viewport rule the engine evaluates, no
//! rebuild); the layout is meant for a window under [`PHONE_MAX_PX`], the
//! breakpoint the ribbon switches to its touch chrome at.
//!
//! Key types: [`MobileShell`], [`ShellBottomTab`], [`MobileShellOnTab`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, NodeType, OptionDom},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, DynamicSelector, MinMaxRange},
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
            LayoutPaddingTop, LayoutPosition,
        },
        property::CssProperty,
    },
    AzString,
};

use super::{
    id_and_class, inner_theme, look_for, part, root_classes, stack_state, state_classes, text,
    ShellLook, FILL_COLUMN_BASE, GROW_LABEL_BASE, LABEL_BASE, RELATIVE_COLUMN_BASE, ROW_BASE,
    TAB_CELL_BASE,
};
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        badge::Badge,
        button::{Button, ButtonOnClick, ButtonOnClickCallback, OptionButtonOnClick},
        roving,
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The shell root's class.
pub const MOBILE_CLASS: &str = "__azul-native-mobile-shell";
/// The app bar's class.
pub const APP_BAR_CLASS: &str = "__azul-native-mobile-shell-app-bar";
/// The app bar's DOM id.
pub const APP_BAR_ID: &str = "shell-app-bar";
/// The Back button host's class.
pub const BACK_CLASS: &str = "__azul-native-mobile-shell-back";
/// The title's class.
pub const TITLE_CLASS: &str = "__azul-native-mobile-shell-title";
/// The actions slot's class.
pub const ACTIONS_CLASS: &str = "__azul-native-mobile-shell-actions";
/// The page host's class.
pub const PAGE_CLASS: &str = "__azul-native-mobile-shell-page";
/// The page host's DOM id.
pub const PAGE_ID: &str = "shell-page";
/// The FAB host's class.
pub const FAB_CLASS: &str = "__azul-native-mobile-shell-fab";
/// The bottom tab bar's class.
pub const TABS_CLASS: &str = "__azul-native-mobile-shell-bottom-tabs";
/// The bottom tab bar's DOM id.
pub const TABS_ID: &str = "shell-bottom-tabs";
/// One tab's class.
pub const TAB_CLASS: &str = "__azul-native-mobile-shell-tab";
/// Added to the active tab.
pub const TAB_ACTIVE_CLASS: &str = "__azul-native-mobile-shell-tab-active";
/// A tab's icon.
pub const TAB_ICON_CLASS: &str = "__azul-native-mobile-shell-tab-icon";
/// A tab's label.
pub const TAB_LABEL_CLASS: &str = "__azul-native-mobile-shell-tab-label";
/// A tab's badge host.
pub const TAB_BADGE_CLASS: &str = "__azul-native-mobile-shell-tab-badge";
/// The widest window the stack is meant for: the ribbon's touch breakpoint.
pub const PHONE_MAX_PX: f32 = 720.0;
/// Under this width the tab labels hide and the icons stand alone.
pub const NARROW_MAX_PX: f32 = 359.0;

/// Callback invoked when a bottom tab is chosen: its index.
pub type MobileShellOnTabCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    MobileShellOnTab,
    OptionMobileShellOnTab,
    MobileShellOnTabCallback,
    MobileShellOnTabCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        MobileShellOnTabCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MOBILE_SHELL_ON_TAB_INVOKER,
    invoker_ty:     AzMobileShellOnTabCallbackInvoker,
    thunk_fn:       az_mobile_shell_on_tab_callback_thunk,
    setter_fn:      AzApp_setMobileShellOnTabCallbackInvoker,
    from_handle_fn: AzMobileShellOnTabCallback_createFromHostHandle,
    from_handle_byref_fn: AzMobileShellOnTabCallback_createFromHostHandleByref,
    extra_args:     [ tab_index: usize ],
}

/// One tab of the bottom bar: an icon over a label, with an optional badge.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ShellBottomTab {
    /// The label ("Mail").
    pub label: AzString,
    /// The icon (a `Dom::create_icon` name).
    pub icon: AzString,
    /// The badge text ("12"), or empty for none.
    pub badge: AzString,
}

impl_option!(
    ShellBottomTab,
    OptionShellBottomTab,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ShellBottomTab,
    ShellBottomTabVec,
    ShellBottomTabVecDestructor,
    ShellBottomTabVecDestructorType,
    ShellBottomTabVecSlice,
    OptionShellBottomTab
);
impl_vec_clone!(ShellBottomTab, ShellBottomTabVec, ShellBottomTabVecDestructor);
impl_vec_debug!(ShellBottomTab, ShellBottomTabVec);
impl_vec_partialeq!(ShellBottomTab, ShellBottomTabVec);
impl_vec_mut!(ShellBottomTab, ShellBottomTabVec);

impl ShellBottomTab {
    /// A tab with `label` and `icon`, no badge.
    #[must_use]
    pub fn create(label: AzString, icon: AzString) -> Self {
        Self {
            label,
            icon,
            badge: AzString::from_const_str(""),
        }
    }

    /// The badge text (empty: none).
    pub fn set_badge(&mut self, badge: AzString) {
        self.badge = badge;
    }

    /// [`Self::set_badge`] for the builder chain.
    #[must_use]
    pub fn with_badge(mut self, badge: AzString) -> Self {
        self.set_badge(badge);
        self
    }
}

/// S11: the app bar over a stack of pages, the bottom tab bar under them.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MobileShell {
    /// The stack of pages; the LAST one is shown.
    pub pages: DomVec,
    /// The app bar's actions (search, more), or none.
    pub actions: OptionDom,
    /// The floating action button, or none.
    pub fab: OptionDom,
    /// The bottom tabs.
    pub tabs: ShellBottomTabVec,
    /// The Back button: the app pops the top page.
    pub on_back: OptionButtonOnClick,
    /// A bottom tab was chosen.
    pub on_tab: OptionMobileShellOnTab,
    /// The app bar's title.
    pub title: AzString,
    /// The active tab.
    pub active_tab: usize,
    /// The safe-area inset above the app bar, in px.
    pub top_inset: f32,
    /// The widget theme this shell is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// Show the Back button even with one page (a page pushed from a
    /// desktop shell's list, say).
    pub show_back: bool,
}

impl MobileShell {
    /// A shell titled `title` with no pages and no tabs.
    #[must_use]
    pub fn create(title: AzString) -> Self {
        Self {
            pages: DomVec::from_const_slice(&[]),
            actions: OptionDom::None,
            fab: OptionDom::None,
            tabs: ShellBottomTabVec::from_const_slice(&[]),
            on_back: None.into(),
            on_tab: None.into(),
            title,
            active_tab: 0,
            top_inset: 0.0,
            theme: OptionUiTheme::None,
            show_back: false,
        }
    }

    /// Pushes a page onto the stack (the last pushed is shown).
    pub fn add_page(&mut self, page: Dom) {
        let mut v = self.pages.clone().into_library_owned_vec();
        v.push(page);
        self.pages = DomVec::from_vec(v);
    }

    /// [`Self::add_page`] for the builder chain.
    #[must_use]
    pub fn with_page(mut self, page: Dom) -> Self {
        self.add_page(page);
        self
    }

    /// Replaces the stack.
    pub fn set_pages(&mut self, pages: DomVec) {
        self.pages = pages;
    }

    /// [`Self::set_pages`] for the builder chain.
    #[must_use]
    pub fn with_pages(mut self, pages: DomVec) -> Self {
        self.set_pages(pages);
        self
    }

    /// The app bar's actions.
    pub fn set_actions(&mut self, actions: Dom) {
        self.actions = OptionDom::Some(actions);
    }

    /// [`Self::set_actions`] for the builder chain.
    #[must_use]
    pub fn with_actions(mut self, actions: Dom) -> Self {
        self.set_actions(actions);
        self
    }

    /// The floating action button.
    pub fn set_fab(&mut self, fab: Dom) {
        self.fab = OptionDom::Some(fab);
    }

    /// [`Self::set_fab`] for the builder chain.
    #[must_use]
    pub fn with_fab(mut self, fab: Dom) -> Self {
        self.set_fab(fab);
        self
    }

    /// Appends a bottom tab.
    pub fn add_tab(&mut self, tab: ShellBottomTab) {
        let mut v = self.tabs.clone().into_library_owned_vec();
        v.push(tab);
        self.tabs = ShellBottomTabVec::from_vec(v);
    }

    /// [`Self::add_tab`] for the builder chain.
    #[must_use]
    pub fn with_tab(mut self, tab: ShellBottomTab) -> Self {
        self.add_tab(tab);
        self
    }

    /// Replaces the bottom tabs.
    pub fn set_tabs(&mut self, tabs: ShellBottomTabVec) {
        self.tabs = tabs;
    }

    /// [`Self::set_tabs`] for the builder chain.
    #[must_use]
    pub fn with_tabs(mut self, tabs: ShellBottomTabVec) -> Self {
        self.set_tabs(tabs);
        self
    }

    /// The active tab.
    pub const fn set_active_tab(&mut self, index: usize) {
        self.active_tab = index;
    }

    /// [`Self::set_active_tab`] for the builder chain.
    #[must_use]
    pub const fn with_active_tab(mut self, index: usize) -> Self {
        self.set_active_tab(index);
        self
    }

    /// The safe-area inset above the app bar.
    pub const fn set_top_inset(&mut self, inset: f32) {
        self.top_inset = inset;
    }

    /// [`Self::set_top_inset`] for the builder chain.
    #[must_use]
    pub const fn with_top_inset(mut self, inset: f32) -> Self {
        self.set_top_inset(inset);
        self
    }

    /// Show the Back button even with one page.
    pub const fn set_back(&mut self, show: bool) {
        self.show_back = show;
    }

    /// [`Self::set_back`] for the builder chain.
    #[must_use]
    pub const fn with_back(mut self, show: bool) -> Self {
        self.set_back(show);
        self
    }

    /// The Back button's click.
    pub fn set_on_back<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_back = Some(ButtonOnClick {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_back`] for the builder chain.
    #[must_use]
    pub fn with_on_back<C: Into<ButtonOnClickCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_back(data, callback);
        self
    }

    /// A bottom tab was chosen.
    pub fn set_on_tab<C: Into<MobileShellOnTabCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_tab = Some(MobileShellOnTab {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_tab`] for the builder chain.
    #[must_use]
    pub fn with_on_tab<C: Into<MobileShellOnTabCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_tab(data, callback);
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
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Whether the app bar shows a Back button: a page under the top one,
    /// or the app asked for it.
    #[must_use]
    pub fn has_back(&self) -> bool {
        self.show_back || self.pages.as_ref().len() > 1
    }

    /// The shell's DOM: root [app bar, page, bottom tabs].
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for MobileShell {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<MobileShell> for Dom {
    fn from(s: MobileShell) -> Self {
        s.dom()
    }
}

// ---------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------

struct TabRef {
    on_tab: OptionMobileShellOnTab,
    index: usize,
}

extern "C" fn on_tab_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(t) = data.downcast_ref::<TabRef>() else {
        return Update::DoNothing;
    };
    match t.on_tab.as_ref() {
        Some(MobileShellOnTab { callback, refany }) => callback.invoke(refany.clone(), info, t.index),
        None => Update::DoNothing,
    }
}

/// Left / Right / Home / End move the stop between the tabs.
extern "C" fn on_tab_key(_data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let step = match key {
        VirtualKeyCode::Left => roving::Step::Previous,
        VirtualKeyCode::Right => roving::Step::Next,
        VirtualKeyCode::Home => roving::Step::First,
        VirtualKeyCode::End => roving::Step::Last,
        _ => return Update::DoNothing,
    };
    let me = info.get_hit_node();
    let Some(parent) = info.get_parent(me) else {
        return Update::DoNothing;
    };
    let items = roving::items_of(&info, parent, TAB_CLASS);
    let Some(current) = items.iter().position(|n| *n == me) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, items.len(), step, true) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    roving::move_stop(&mut info, &items, target);
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// The build
// ---------------------------------------------------------------------------

fn hook(event: EventFilter, cb: usize, refany: RefAny) -> CoreCallbackData {
    CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany,
    }
}

/// The app bar: a row of its parts, centred, that keeps its height.
static APP_BAR_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The bottom tab bar: a row of equal cells that keeps its height.
static TAB_BAR_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The FAB host floats over the page; where it floats is the theme's.
static FAB_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
];

/// The phone-width rule on a tab label: hidden under [`NARROW_MAX_PX`].
#[must_use]
pub(crate) fn narrow_hides_label() -> CssPropertyWithConditions {
    CssPropertyWithConditions::with_condition(
        CssProperty::const_display(LayoutDisplay::None),
        DynamicSelector::ViewportWidth(MinMaxRange::with_max(NARROW_MAX_PX)),
    )
}

/// One bottom tab.
fn tab(
    t: ShellBottomTab,
    index: usize,
    stop: usize,
    active: bool,
    on_tab: &OptionMobileShellOnTab,
    inner: Option<UiTheme>,
    look: &ShellLook,
) -> Dom {
    let ShellBottomTab { label, icon, badge } = t;
    let base = part(TAB_CELL_BASE, &look.bottom_tab);
    let css = if active {
        stack_state(&base, &look.bottom_tab_active)
    } else {
        base
    };
    let icon_node = if icon.as_str().is_empty() {
        Dom::create_div()
    } else {
        Dom::create_icon(icon)
    };
    let mut label_skin: Vec<CssPropertyWithConditions> = look.bottom_tab_label.clone();
    label_skin.push(narrow_hides_label());
    let mut children: Vec<Dom> = alloc::vec![
        icon_node
            .with_class(AzString::from_const_str(TAB_ICON_CLASS))
            .with_css_props(part(LABEL_BASE, &look.bottom_tab_icon)),
        text(label.clone())
            .with_class(AzString::from_const_str(TAB_LABEL_CLASS))
            .with_css_props(part(LABEL_BASE, &label_skin)),
    ];
    if !badge.as_str().is_empty() {
        let mut b = Badge::create(badge);
        if let Some(th) = inner {
            b = b.with_theme(th);
        }
        children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(TAB_BADGE_CLASS))
                .with_css_props(part(LABEL_BASE, &[]))
                .with_child(b.dom()),
        );
    }
    let tab_ref = RefAny::new(TabRef {
        on_tab: on_tab.clone(),
        index,
    });
    Dom::create_div()
        .with_ids_and_classes(state_classes(TAB_CLASS, active, TAB_ACTIVE_CLASS))
        .with_css_props(css)
        .with_tab_index(roving::item_tab_index(index, stop))
        .with_accessibility_info(AccessibilityInfo {
            states: if active {
                AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Selected])
            } else {
                AccessibilityStateVec::from_const_slice(&[])
            },
            ..AccessibilityInfo::named(label, AccessibilityRole::PageTab)
        })
        .with_callbacks(
            alloc::vec![
                hook(
                    EventFilter::Hover(HoverEventFilter::Click),
                    on_tab_click as usize,
                    tab_ref.clone()
                ),
                hook(
                    EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    on_tab_key as usize,
                    tab_ref
                ),
            ]
            .into(),
        )
        .with_children(DomVec::from_vec(children))
}

/// The shell's DOM in `look`.
pub(crate) fn build(shell: MobileShell, look: &ShellLook) -> Dom {
    let has_back = shell.has_back();
    let MobileShell {
        pages,
        actions,
        fab,
        tabs,
        on_back,
        on_tab,
        title,
        active_tab,
        top_inset,
        theme,
        show_back: _,
    } = shell;
    let inner = inner_theme(theme);

    // The app bar: [back?, title, actions?], with the safe-area inset.
    let mut bar_children: Vec<Dom> = Vec::with_capacity(3);
    if has_back {
        let mut back = Button::create(AzString::from_const_str(""))
            .with_icon(AzString::from_const_str("arrow_back"));
        if let Some(ButtonOnClick { refany, callback }) = on_back.into_option() {
            back = back.with_on_click(refany, callback);
        }
        if let Some(th) = inner {
            back = back.with_theme(th);
        }
        bar_children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(BACK_CLASS))
                .with_css_props(part(LABEL_BASE, &[]))
                .with_child(back.dom().with_accessibility_name("Back")),
        );
    }
    bar_children.push(
        text(title.clone())
            .with_class(AzString::from_const_str(TITLE_CLASS))
            .with_css_props(part(GROW_LABEL_BASE, &look.app_bar_title)),
    );
    if let Some(a) = actions.into_option() {
        bar_children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(ACTIONS_CLASS))
                .with_css_props(part(ROW_BASE, &[]))
                .with_child(a),
        );
    }
    let mut bar_base: Vec<CssPropertyWithConditions> = APP_BAR_BASE.to_vec();
    #[allow(clippy::cast_possible_truncation)]
    if top_inset > 0.0 {
        bar_base.push(CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(top_inset as isize),
        )));
    }
    let app_bar = Dom::create_node(NodeType::Header)
        .with_ids_and_classes(id_and_class(&AzString::from_const_str(APP_BAR_ID), APP_BAR_CLASS))
        .with_css_props(part(&bar_base, &look.app_bar))
        .with_accessibility_name(title)
        .with_children(DomVec::from_vec(bar_children));

    // The page: the top of the stack, the FAB floating over it.
    let mut pages = pages.into_library_owned_vec();
    let mut page_children: Vec<Dom> = Vec::with_capacity(2);
    if let Some(top) = pages.pop() {
        page_children.push(top);
    }
    if let Some(f) = fab.into_option() {
        page_children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(FAB_CLASS))
                .with_css_props(part(FAB_BASE, &look.fab))
                .with_child(f),
        );
    }
    let page = Dom::create_node(NodeType::Main)
        .with_ids_and_classes(id_and_class(&AzString::from_const_str(PAGE_ID), PAGE_CLASS))
        .with_css_props(part(RELATIVE_COLUMN_BASE, &look.page))
        .with_children(DomVec::from_vec(page_children));

    // The bottom tabs.
    let tabs = tabs.into_library_owned_vec();
    let stop = roving::stop_index(Some(active_tab), tabs.len());
    let cells: Vec<Dom> = tabs
        .into_iter()
        .enumerate()
        .map(|(i, t)| tab(t, i, stop, i == active_tab, &on_tab, inner, look))
        .collect();
    let mut children: Vec<Dom> = alloc::vec![app_bar, page];
    if !cells.is_empty() {
        children.push(
            Dom::create_node(NodeType::Nav)
                .with_ids_and_classes(id_and_class(&AzString::from_const_str(TABS_ID), TABS_CLASS))
                .with_css_props(part(TAB_BAR_BASE, &look.bottom_tabs))
                .with_accessibility_info(AccessibilityInfo::named("Tabs", AccessibilityRole::PageTabList))
                .with_children(DomVec::from_vec(cells)),
        );
    }

    Dom::create_div()
        .with_ids_and_classes(root_classes(MOBILE_CLASS, look))
        .with_css_props(part(FILL_COLUMN_BASE, &look.shell_root))
        .with_children(DomVec::from_vec(children))
}

#[cfg(test)]
mod mobile_shell_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, IdOrClass, TabIndex},
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        shells::fixtures::{mobile_shell, slot},
        themes::{theme_blocks::checks, theme_checks as tc, UiTheme},
    };

    fn indices_of(dom: &Dom, class: &str) -> Vec<usize> {
        tc::nodes(dom)
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| tc::has_class(n, class))
            .map(|(i, _)| i)
            .collect()
    }

    fn node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    #[test]
    fn s11_is_an_app_bar_over_the_top_page_over_the_bottom_tabs() {
        let dom = mobile_shell().with_theme(UiTheme::Flat).dom();
        let ids: Vec<String> = tc::nodes(&dom)
            .into_iter()
            .filter_map(|(_, n)| {
                n.root.get_ids_and_classes().as_ref().iter().find_map(|c| match c {
                    IdOrClass::Id(s) => Some(s.as_str().to_string()),
                    IdOrClass::Class(_) => None,
                })
            })
            .collect();
        assert_eq!(ids, vec![APP_BAR_ID, PAGE_ID, TABS_ID]);
        let kids = dom.children.as_ref();
        assert!(matches!(kids[0].root.get_node_type(), NodeType::Header));
        assert!(matches!(kids[1].root.get_node_type(), NodeType::Main));
        assert!(matches!(kids[2].root.get_node_type(), NodeType::Nav));
        // Two pages pushed: the Back button shows, and only the top page is in the tree.
        assert!(tc::find(&dom, BACK_CLASS).is_some());
        let page = &kids[1];
        assert_eq!(page.children.as_ref().len(), 2, "the top page, the FAB");
        assert!(tc::has_class(&page.children.as_ref()[1], FAB_CLASS));
        let tabs = tc::find_all(&dom, TAB_CLASS);
        assert_eq!(tabs.len(), 3);
        assert!(tc::has_class(tabs[0], TAB_ACTIVE_CLASS));
        assert_eq!(tabs[0].root.get_tab_index(), Some(TabIndex::Auto));
        assert_eq!(tabs[1].root.get_tab_index(), Some(TabIndex::NoKeyboardFocus));
        assert_eq!(
            tabs[0].root.get_accessibility_info().map(|i| i.role),
            Some(AccessibilityRole::PageTab)
        );
    }

    #[test]
    fn one_page_has_no_back_button_unless_asked_for() {
        let one = MobileShell::create(AzString::from("Inbox")).with_page(slot());
        assert!(!one.has_back());
        let dom = one.with_theme(UiTheme::Flat).dom();
        assert!(tc::find(&dom, BACK_CLASS).is_none());
        let asked = MobileShell::create(AzString::from("Inbox"))
            .with_page(slot())
            .with_back(true)
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(tc::find(&asked, BACK_CLASS).is_some());
        assert_eq!(dom.children.as_ref().len(), 2, "no tabs: no tab bar");
    }

    #[test]
    fn the_tab_labels_hide_under_the_narrow_width_by_a_viewport_rule() {
        let dom = mobile_shell().with_theme(UiTheme::Flat).dom();
        let label = tc::find(&dom, TAB_LABEL_CLASS).expect("a label");
        let rule = label
            .root
            .style
            .iter_inline_properties()
            .find(|(p, _)| matches!(p, CssProperty::Display(_)))
            .map(|(_, c)| c.clone())
            .expect("the narrow rule");
        assert_eq!(
            rule.as_ref(),
            &[DynamicSelector::ViewportWidth(MinMaxRange::with_max(NARROW_MAX_PX))]
        );
        let bar = tc::find(&dom, APP_BAR_CLASS).expect("app bar");
        assert!(tc::resolve(bar, azul_css::props::property::CssPropertyType::PaddingTop, false, None).is_none());
        let inset = mobile_shell().with_top_inset(44.0).with_theme(UiTheme::Flat).dom();
        let bar = tc::find(&inset, APP_BAR_CLASS).expect("app bar");
        assert!(tc::resolve(bar, azul_css::props::property::CssPropertyType::PaddingTop, false, None).is_some());
    }

    type Log = Arc<Mutex<Vec<usize>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(index);
        }
        Update::DoNothing
    }

    #[test]
    fn a_tap_chooses_the_tab_and_the_arrows_move_the_stop_wrapping() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = mobile_shell()
            .with_on_tab(RefAny::new(log.clone()), record as MobileShellOnTabCallbackType)
            .with_theme(UiTheme::Flat)
            .dom();
        let styled = StyledDom::create_from_dom(dom.clone());
        let tabs = indices_of(&dom, TAB_CLASS);
        rv::fire(&styled, node(tabs[1]), EventFilter::Hover(HoverEventFilter::Click)).expect("tap");
        assert_eq!(*log.lock().expect("log"), vec![1]);
        let (_, changes) = rv::press(&styled, node(tabs[2]), VirtualKeyCode::Right, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(tabs[0])), "wraps");
        assert!(rv::prevented(&changes));
        let (_, changes) = rv::press(&styled, node(tabs[0]), VirtualKeyCode::Left, &[]).expect("keys");
        assert_eq!(rv::focus_request(&changes), Some(node(tabs[2])));
    }

    #[test]
    fn s11_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "mobile_shell",
            || mobile_shell().dom(),
            |t: UiTheme| mobile_shell().with_theme(t).dom(),
        );
    }
}
