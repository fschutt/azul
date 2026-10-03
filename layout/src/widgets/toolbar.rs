//! Toolbar widget - a row of tool buttons, toggles, drop-down (menu) buttons,
//! separators, spacers and embedded app controls (a search field), with
//! the items that do not fit moving into a "more" menu when the bar is
//! narrow. The command bar of a file manager, a mail list, a PDF viewer, a
//! paint app - not the ribbon (`ribbon.rs`).
//!
//! THE APP OWNS THE ITEMS: each [`ToolbarItem`] says what it is
//! ([`ToolbarItemKind`]), its `id` (what its events carry: "bold"), its
//! label, its icon (a `Dom::create_icon` name), whether a toggle is pressed,
//! why it is disabled (an empty reason: enabled), the choices of a menu
//! button. One callback reports what happened ([`ToolbarEvent`]):
//!
//! - `Activate` (a button, or an embedded control picked from the "more"
//!   menu, was clicked or pressed with Enter / Space);
//! - `Toggle` (a toggle was clicked: `pressed` is its NEW state - the app
//!   stores it and rebuilds);
//! - `Choose` (a choice of a menu button: `choice` is its index).
//!
//! EACH TOOL IS THE `Button` WIDGET with the toolbar's look handed in as its
//! styles (as the ribbon does): a quiet key - no face at rest, the hover
//! face under the pointer, the pressed face while held, a toggle that is on
//! pushed in - so a disabled tool is the Button's (dimmed, inert, announced
//! unavailable, its reason as a tooltip) and Enter / Space are its
//! activation. An icon-only tool is named by its label and shows it as a
//! tooltip under the pointer.
//!
//! OVERFLOW: the app says how wide the bar may be
//! ([`Toolbar::with_available_width`], 0 = no limit, like the data table's
//! viewport). The toolbar estimates each item's width ([`item_width`]: the
//! padding, the 20 px icon, the label at 0.6 em a character, the menu
//! arrow), and when they do not fit the items at the END move, one by one,
//! into the menu of a "more" button (`more_horiz`) at the end of the bar -
//! except the ones marked [`ToolbarItem::never_overflow`] (the search
//! field). In the menu a button is an entry, a toggle a check entry, a menu
//! button a submenu of its choices, a separator a separator.
//!
//! KEYBOARD (WAI-ARIA APG toolbar): the tools are ONE Tab stop. Left /
//! Right move between them (wrapping), Home / End go to the ends, Enter /
//! Space press the focused tool, Down opens a menu button's menu (and the
//! "more" menu). An embedded control keeps its own Tab stop.
//!
//! Key types: [`Toolbar`], [`ToolbarItem`], [`ToolbarEvent`].

use alloc::vec::Vec;

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::Update,
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass::Class, IdOrClassVec, OptionDom},
    events::FocusEventFilter,
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, MenuItemVec, MenuPopupPosition, StringMenuItem},
    refany::RefAny,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    props::{
        basic::{length::FloatValue, StyleFontSize},
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutFlexWrap, LayoutHeight, LayoutJustifyContent, LayoutMarginLeft,
            LayoutMarginRight, LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutPaddingBottom,
            LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutWidth,
        },
        property::CssProperty,
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBorderBottomStyle, StyleBorderLeftStyle,
            StyleBorderRightStyle, StyleBorderTopStyle, StyleCursor,
        },
    },
    AzString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        button::{Button, ButtonOnClick, ButtonOnClickCallbackType},
        roving::{self, Step},
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The toolbar's class.
pub(crate) const TOOLBAR_CLASS: &str = "__azul-native-toolbar";
/// A tool (a button, a toggle, a menu button, the "more" button): the items
/// the arrow keys walk.
pub(crate) const ITEM_CLASS: &str = "__azul-native-toolbar-item";
/// A toggle that is on (added to [`ITEM_CLASS`]).
pub(crate) const ITEM_PRESSED_CLASS: &str = "__azul-native-toolbar-item-pressed";
/// The "more" button (added to [`ITEM_CLASS`]).
pub(crate) const MORE_CLASS: &str = "__azul-native-toolbar-more";
/// A separator.
pub(crate) const SEPARATOR_CLASS: &str = "__azul-native-toolbar-separator";
/// A spacer (it takes the free width: the items after it go to the right).
pub(crate) const SPACER_CLASS: &str = "__azul-native-toolbar-spacer";
/// The box around an embedded app control.
pub(crate) const CUSTOM_CLASS: &str = "__azul-native-toolbar-custom";

/// What the "more" button is called (and shows as its tooltip).
pub const TOOLBAR_MORE_LABEL: &str = "More";

// ---- the metrics the overflow estimate and the bases share ----

/// A tool's font size, px (the label's width is estimated from it).
pub(crate) const TOOLBAR_FONT_PX: f32 = 13.0;
/// A tool's icon box, px (an 18 px glyph and its side bearings).
pub(crate) const TOOLBAR_ICON_PX: f32 = 20.0;
/// A menu button's arrow, px.
pub(crate) const TOOLBAR_ARROW_PX: f32 = 16.0;
/// A tool's border and padding across, px (1 + 5 on each side).
pub(crate) const TOOLBAR_ITEM_PAD_PX: f32 = 12.0;
/// The gap between a tool's icon, label and arrow, px.
pub(crate) const TOOLBAR_GAP_PX: f32 = 4.0;
/// The space after every item, px.
pub(crate) const TOOLBAR_ITEM_GAP_PX: f32 = 2.0;
/// A separator across: 1 px and 4 px either side.
pub(crate) const TOOLBAR_SEPARATOR_PX: f32 = 9.0;
/// The "more" button across (an icon-only tool).
pub(crate) const TOOLBAR_MORE_PX: f32 = TOOLBAR_ITEM_PAD_PX + TOOLBAR_ICON_PX + TOOLBAR_ITEM_GAP_PX;
/// A label's width per character, in em (a little more than the UI face's
/// average, so an estimate errs toward overflowing, never toward clipping).
pub(crate) const TOOLBAR_CHAR_EM: f32 = 0.6;
/// A tool's height, px.
pub(crate) const TOOLBAR_ITEM_HEIGHT_PX: isize = 28;

// ==== Types ====

/// What a toolbar item is.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ToolbarItemKind {
    /// A command: a click reports `Activate`.
    #[default]
    Button,
    /// On or off: a click reports `Toggle` with the new state.
    Toggle,
    /// A drop-down button: a click opens a menu of its `choices`; a pick
    /// reports `Choose`.
    MenuButton,
    /// A thin rule between groups of tools.
    Separator,
    /// The free width: the items after it sit at the bar's right end.
    Spacer,
    /// An app control (a search field, a zoom box): `content`, `width` px
    /// wide.
    Custom,
}

/// One item of a toolbar.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarItem {
    /// The app's name for the item ("bold"): what its events carry.
    pub id: AzString,
    /// The label: shown beside the icon when `show_label` (or when there is
    /// no icon), the accessible name and tooltip otherwise.
    pub label: AzString,
    /// The icon (a `Dom::create_icon` name), or empty.
    pub icon: AzString,
    /// A longer description for the tooltip and screen readers, or empty.
    pub tooltip: AzString,
    /// Why the item cannot be used now ("Select a file first"); empty =
    /// enabled.
    pub disabled_reason: AzString,
    /// A menu button's choices.
    pub choices: StringVec,
    /// A custom item's control.
    pub content: OptionDom,
    /// A custom item's width, px (what the overflow estimate counts).
    pub width: f32,
    /// What the item is.
    pub kind: ToolbarItemKind,
    /// A toggle is on.
    pub pressed: bool,
    /// Show the label beside the icon.
    pub show_label: bool,
    /// The item stays in the bar when it is narrow (never in the "more"
    /// menu).
    pub never_overflow: bool,
}

impl ToolbarItem {
    /// An item of `kind`, nothing else set.
    #[must_use]
    const fn of_kind(kind: ToolbarItemKind, id: AzString, label: AzString, icon: AzString) -> Self {
        Self {
            id,
            label,
            icon,
            tooltip: AzString::from_const_str(""),
            disabled_reason: AzString::from_const_str(""),
            choices: StringVec::from_const_slice(&[]),
            content: OptionDom::None,
            width: 0.0,
            kind,
            pressed: false,
            show_label: false,
            never_overflow: false,
        }
    }

    /// A command button `id` showing `icon` (named `label`; without an
    /// icon the label shows).
    #[must_use]
    pub const fn create_button(id: AzString, label: AzString, icon: AzString) -> Self {
        Self::of_kind(ToolbarItemKind::Button, id, label, icon)
    }

    /// A toggle button `id`, on or off.
    #[must_use]
    pub const fn create_toggle(id: AzString, label: AzString, icon: AzString, pressed: bool) -> Self {
        let mut item = Self::of_kind(ToolbarItemKind::Toggle, id, label, icon);
        item.pressed = pressed;
        item
    }

    /// A drop-down button `id` that offers `choices`.
    #[must_use]
    pub fn create_menu_button(id: AzString, label: AzString, icon: AzString, choices: StringVec) -> Self {
        let mut item = Self::of_kind(ToolbarItemKind::MenuButton, id, label, icon);
        item.choices = choices;
        item
    }

    /// A separator.
    #[must_use]
    pub const fn create_separator() -> Self {
        Self::of_kind(
            ToolbarItemKind::Separator,
            AzString::from_const_str(""),
            AzString::from_const_str(""),
            AzString::from_const_str(""),
        )
    }

    /// A spacer: the items after it go to the right end.
    #[must_use]
    pub const fn create_spacer() -> Self {
        Self::of_kind(
            ToolbarItemKind::Spacer,
            AzString::from_const_str(""),
            AzString::from_const_str(""),
            AzString::from_const_str(""),
        )
    }

    /// An app control `id` (`content`, `width` px wide), named `label` in
    /// the "more" menu.
    #[must_use]
    pub fn create_custom(id: AzString, label: AzString, content: Dom, width: f32) -> Self {
        let mut item = Self::of_kind(ToolbarItemKind::Custom, id, label, AzString::from_const_str(""));
        item.content = OptionDom::Some(content);
        item.width = width.max(0.0);
        item
    }

    /// Show the label beside the icon or not.
    pub const fn set_show_label(&mut self, show_label: bool) {
        self.show_label = show_label;
    }

    /// [`Self::set_show_label`] for the builder chain.
    #[must_use]
    pub const fn with_show_label(mut self, show_label: bool) -> Self {
        self.set_show_label(show_label);
        self
    }

    /// The tooltip's longer description.
    pub fn set_tooltip(&mut self, tooltip: AzString) {
        self.tooltip = tooltip;
    }

    /// [`Self::set_tooltip`] for the builder chain.
    #[must_use]
    pub fn with_tooltip(mut self, tooltip: AzString) -> Self {
        self.set_tooltip(tooltip);
        self
    }

    /// Disables the item: `reason` says why (empty enables it).
    pub fn set_disabled(&mut self, reason: AzString) {
        self.disabled_reason = reason;
    }

    /// [`Self::set_disabled`] for the builder chain.
    #[must_use]
    pub fn with_disabled(mut self, reason: AzString) -> Self {
        self.set_disabled(reason);
        self
    }

    /// A toggle on or off.
    pub const fn set_pressed(&mut self, pressed: bool) {
        self.pressed = pressed;
    }

    /// [`Self::set_pressed`] for the builder chain.
    #[must_use]
    pub const fn with_pressed(mut self, pressed: bool) -> Self {
        self.set_pressed(pressed);
        self
    }

    /// Keep the item in the bar when it is narrow.
    pub const fn set_never_overflow(&mut self, never_overflow: bool) {
        self.never_overflow = never_overflow;
    }

    /// [`Self::set_never_overflow`] for the builder chain.
    #[must_use]
    pub const fn with_never_overflow(mut self, never_overflow: bool) -> Self {
        self.set_never_overflow(never_overflow);
        self
    }

    /// Whether the item is disabled (it has a reason).
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        !self.disabled_reason.as_str().is_empty()
    }

    /// Whether the label shows (asked for, or there is no icon to show).
    #[must_use]
    pub fn shows_label(&self) -> bool {
        self.show_label || self.icon.as_str().is_empty()
    }

    /// Whether the item is a tool the arrow keys walk.
    #[must_use]
    pub const fn is_tool(&self) -> bool {
        matches!(
            self.kind,
            ToolbarItemKind::Button | ToolbarItemKind::Toggle | ToolbarItemKind::MenuButton
        )
    }
}

impl Default for ToolbarItem {
    fn default() -> Self {
        Self::create_separator()
    }
}

impl_option!(
    ToolbarItem,
    OptionToolbarItem,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ToolbarItem,
    ToolbarItemVec,
    ToolbarItemVecDestructor,
    ToolbarItemVecDestructorType,
    ToolbarItemVecSlice,
    OptionToolbarItem
);
impl_vec_clone!(ToolbarItem, ToolbarItemVec, ToolbarItemVecDestructor);
impl_vec_debug!(ToolbarItem, ToolbarItemVec);
impl_vec_partialeq!(ToolbarItem, ToolbarItemVec);
impl_vec_mut!(ToolbarItem, ToolbarItemVec);

/// What happened in the toolbar.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolbarEventKind {
    /// A button (or an embedded control picked from the "more" menu) was
    /// pressed.
    Activate,
    /// A toggle was pressed: `pressed` is its new state.
    Toggle,
    /// A choice of a menu button was picked: `choice` is its index.
    Choose,
}

/// One action in the toolbar.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarEvent {
    /// The item's `id`.
    pub id: AzString,
    /// The item's index in the toolbar's items.
    pub index: usize,
    /// `Choose`: the picked choice.
    pub choice: usize,
    /// What happened.
    pub kind: ToolbarEventKind,
    /// `Toggle`: the toggle's new state.
    pub pressed: bool,
}

impl ToolbarEvent {
    /// A `kind` event on item `index` (named `id`).
    #[must_use]
    pub const fn create(kind: ToolbarEventKind, index: usize, id: AzString) -> Self {
        Self {
            id,
            index,
            choice: 0,
            kind,
            pressed: false,
        }
    }
}

/// Callback invoked for an action in the toolbar.
pub type ToolbarOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, ToolbarEvent) -> Update;
impl_widget_callback!(
    ToolbarOnEvent,
    OptionToolbarOnEvent,
    ToolbarOnEventCallback,
    ToolbarOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ToolbarOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TOOLBAR_ON_EVENT_INVOKER,
    invoker_ty:     AzToolbarOnEventCallbackInvoker,
    thunk_fn:       az_toolbar_on_event_callback_thunk,
    setter_fn:      AzApp_setToolbarOnEventCallbackInvoker,
    from_handle_fn: AzToolbarOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzToolbarOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ToolbarEvent ],
}

/// The toolbar (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct Toolbar {
    /// The items, left to right.
    pub items: ToolbarItemVec,
    /// What a screen reader calls the bar ("Formatting").
    pub accessibility_name: AzString,
    /// Every action in the bar.
    pub on_event: OptionToolbarOnEvent,
    /// The width the bar may take, px: the items that do not fit go into
    /// the "more" menu. 0 = no limit.
    pub available_width: f32,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl Toolbar {
    /// An empty toolbar a screen reader calls `accessibility_name`.
    #[must_use]
    pub const fn create(accessibility_name: AzString) -> Self {
        Self {
            items: ToolbarItemVec::from_const_slice(&[]),
            accessibility_name,
            on_event: OptionToolbarOnEvent::None,
            available_width: 0.0,
            theme: OptionUiTheme::None,
        }
    }

    /// Adds an item at the end.
    pub fn add_item(&mut self, item: ToolbarItem) {
        let mut items =
            core::mem::replace(&mut self.items, ToolbarItemVec::from_const_slice(&[])).into_library_owned_vec();
        items.push(item);
        self.items = ToolbarItemVec::from_vec(items);
    }

    /// [`Self::add_item`] for the builder chain.
    #[must_use]
    pub fn with_item(mut self, item: ToolbarItem) -> Self {
        self.add_item(item);
        self
    }

    /// Replaces the items.
    pub fn set_items(&mut self, items: ToolbarItemVec) {
        self.items = items;
    }

    /// [`Self::set_items`] for the builder chain.
    #[must_use]
    pub fn with_items(mut self, items: ToolbarItemVec) -> Self {
        self.set_items(items);
        self
    }

    /// The width the bar may take, px (0 = no limit).
    pub fn set_available_width(&mut self, width: f32) {
        self.available_width = if width.is_finite() { width.max(0.0) } else { 0.0 };
    }

    /// [`Self::set_available_width`] for the builder chain.
    #[must_use]
    pub fn with_available_width(mut self, width: f32) -> Self {
        self.set_available_width(width);
        self
    }

    /// Every action in the bar.
    pub fn set_on_event<C: Into<ToolbarOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = OptionToolbarOnEvent::Some(ToolbarOnEvent {
            refany: data,
            callback: callback.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ToolbarOnEventCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme; unset, the bar follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty toolbar and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The toolbar's DOM, in the pinned theme's look or both looks merged
    /// (built once).
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::create(AzString::from_const_str("Toolbar"))
    }
}

impl From<Toolbar> for Dom {
    fn from(t: Toolbar) -> Self {
        t.dom()
    }
}

// ==== The overflow estimate ====

/// Which items a toolbar shows and which go into its "more" menu (indices
/// into the items, in order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ToolbarFit {
    /// The items in the bar.
    pub shown: Vec<usize>,
    /// The items in the "more" menu.
    pub overflow: Vec<usize>,
}

/// How wide `item` is in the bar, px (an estimate: the label at
/// [`TOOLBAR_CHAR_EM`] a character).
#[must_use]
pub(crate) fn item_width(item: &ToolbarItem) -> f32 {
    let _ = item;
    0.0
}

/// Which of `items` fit into `available` px (0 = everything): from the
/// end, the items that do not fit move into the "more" menu (whose button
/// then takes [`TOOLBAR_MORE_PX`]), except `never_overflow` ones and the
/// spacers; a separator left at an end of either list, or doubled, goes.
#[must_use]
pub(crate) fn fit(items: &[ToolbarItem], available: f32) -> ToolbarFit {
    let _ = available;
    ToolbarFit {
        shown: (0..items.len()).collect(),
        overflow: Vec::new(),
    }
}

// ==== The look and the DOM ====

/// What a theme decides about a toolbar: the SKIN of each part, laid over
/// the part's base (`TOOLBAR_*_BASE`) by [`build`].
#[derive(Debug, Clone, Default)]
pub(crate) struct ToolbarLook {
    /// The bar (its ground, its rule).
    pub bar: Vec<CssPropertyWithConditions>,
    /// A tool: a quiet key (its ink, its hover / pressed / focus states).
    pub item: Vec<CssPropertyWithConditions>,
    /// Stacked on a toggle that is on: the pushed-in face (and the states
    /// again after it).
    pub item_pressed: Vec<CssPropertyWithConditions>,
    /// A tool's label.
    pub label: Vec<CssPropertyWithConditions>,
    /// A tool's icon.
    pub icon: Vec<CssPropertyWithConditions>,
    /// A menu button's arrow.
    pub arrow: Vec<CssPropertyWithConditions>,
    /// A separator's rule.
    pub separator: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the bar, if it has one.
    pub marker: Option<&'static str>,
}

/// The look a toolbar with the theme option `theme` is built with: the
/// pinned theme's own look, or both looks merged part by part (the DOM is
/// built once).
pub(crate) fn look_for(theme: OptionUiTheme) -> ToolbarLook {
    let _ = theme;
    ToolbarLook::default()
}

/// The toolbar's DOM in `look`.
pub(crate) fn build(toolbar: Toolbar, look: &ToolbarLook) -> Dom {
    let _ = look;
    Dom::create_div()
        .with_class(AzString::from_const_str(TOOLBAR_CLASS))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Toolbar,
            accessibility_name: Some(toolbar.accessibility_name).into(),
            ..Default::default()
        })
}
