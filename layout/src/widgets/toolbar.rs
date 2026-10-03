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
    match item.kind {
        ToolbarItemKind::Separator => TOOLBAR_SEPARATOR_PX,
        ToolbarItemKind::Spacer => 0.0,
        ToolbarItemKind::Custom => item.width.max(0.0) + TOOLBAR_ITEM_GAP_PX,
        ToolbarItemKind::Button | ToolbarItemKind::Toggle | ToolbarItemKind::MenuButton => {
            // The tool's parts (icon, label, arrow) sit a gap apart.
            let mut content = 0.0_f32;
            let mut parts = 0_u8;
            if !item.icon.as_str().is_empty() {
                content += TOOLBAR_ICON_PX;
                parts += 1;
            }
            if item.shows_label() {
                content += label_px(item.label.as_str());
                parts += 1;
            }
            if item.kind == ToolbarItemKind::MenuButton {
                content += TOOLBAR_ARROW_PX;
                parts += 1;
            }
            let gaps = f32::from(parts.saturating_sub(1)) * TOOLBAR_GAP_PX;
            TOOLBAR_ITEM_PAD_PX + content + gaps + TOOLBAR_ITEM_GAP_PX
        }
    }
}

/// A label's estimated width, px.
#[allow(clippy::cast_precision_loss)] // a label is far shorter than 2^24 characters
fn label_px(label: &str) -> f32 {
    label.chars().count() as f32 * TOOLBAR_FONT_PX * TOOLBAR_CHAR_EM
}

/// Which of `items` fit into `available` px (0 = everything): from the
/// end, the items that do not fit move into the "more" menu (whose button
/// then takes [`TOOLBAR_MORE_PX`]), except `never_overflow` ones and the
/// spacers; a separator left at an end of either list, or doubled, goes.
#[must_use]
pub(crate) fn fit(items: &[ToolbarItem], available: f32) -> ToolbarFit {
    let widths: Vec<f32> = items.iter().map(item_width).collect();
    let total: f32 = widths.iter().sum();
    let mut shown = alloc::vec![true; items.len()];
    if available.is_finite() && available > 0.0 && total > available {
        let budget = (available - TOOLBAR_MORE_PX).max(0.0);
        let mut used = total;
        for index in (0..items.len()).rev() {
            if used <= budget {
                break;
            }
            let item = &items[index];
            if item.never_overflow || item.kind == ToolbarItemKind::Spacer {
                continue;
            }
            shown[index] = false;
            used -= widths[index];
        }
    }
    let on: Vec<usize> = (0..items.len()).filter(|i| shown[*i]).collect();
    let off: Vec<usize> = (0..items.len()).filter(|i| !shown[*i]).collect();
    ToolbarFit {
        shown: without_stray_separators(items, &on),
        overflow: without_stray_separators(items, &off),
    }
}

/// `list` (indices into `items`) without the separators that separate
/// nothing: one before the first item, one after the last, the second of
/// two in a row. Spacers are not items here (a separator before a spacer
/// still separates the groups either side of it).
fn without_stray_separators(items: &[ToolbarItem], list: &[usize]) -> Vec<usize> {
    let is_content = |i: usize| {
        !matches!(
            items[i].kind,
            ToolbarItemKind::Separator | ToolbarItemKind::Spacer
        )
    };
    let mut out = Vec::with_capacity(list.len());
    // Whether the last item kept (spacers aside) is content, not a separator.
    let mut after_content = false;
    for (position, &index) in list.iter().enumerate() {
        if items[index].kind == ToolbarItemKind::Separator {
            let content_follows = list[position + 1..].iter().any(|&j| is_content(j));
            if after_content && content_follows {
                out.push(index);
                after_content = false;
            }
        } else {
            out.push(index);
            if is_content(index) {
                after_content = true;
            }
        }
    }
    out
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

/// The menu of a menu button's choices (item `index`): one entry per choice,
/// reporting `Choose`.
pub(crate) fn choice_menu_items(item: &ToolbarItem, index: usize, shared: &RefAny) -> Vec<MenuItem> {
    let _ = (item, index, shared);
    Vec::new()
}

/// The "more" menu: the `overflow` items of `items` as entries (a button an
/// entry, a toggle a check entry, a menu button a submenu of its choices, a
/// separator a separator, a disabled item a disabled entry).
pub(crate) fn overflow_menu_items(items: &[ToolbarItem], overflow: &[usize], shared: &RefAny) -> Vec<MenuItem> {
    let _ = (items, overflow, shared);
    Vec::new()
}

/// What a menu entry carries: the item, the choice and what picking it
/// reports.
struct MenuPick {
    index: usize,
    choice: usize,
    kind: ToolbarEventKind,
    shared: RefAny,
}

#[cfg(test)]
mod toolbar_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, TabIndex},
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::VirtualKeyCode as K,
    };

    use super::*;
    use crate::{
        callbacks::CallbackChange,
        widgets::{
            roving::test_support as rv,
            themes::{theme_blocks::checks, theme_checks},
        },
    };

    type Log = Arc<Mutex<Vec<String>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, e: ToolbarEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(format!(
                "{:?} {} {}{}{}",
                e.kind,
                e.id.as_str(),
                e.index,
                if e.kind == ToolbarEventKind::Choose {
                    format!(" choice {}", e.choice)
                } else {
                    String::new()
                },
                if e.pressed { " pressed" } else { "" },
            ));
        }
        Update::RefreshDom
    }

    fn log() -> Log {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn logged(log: &Log) -> Vec<String> {
        log.lock().expect("log").clone()
    }

    fn s(text: &str) -> AzString {
        AzString::from(text)
    }

    fn choices(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|c| AzString::from(*c)).collect())
    }

    fn icon_button(id: &str) -> ToolbarItem {
        ToolbarItem::create_button(s(id), s(id), s("star"))
    }

    /// Save, Bold (off), Italic (on), a separator, Align (a menu button with
    /// its label), Delete (disabled), a spacer and a search field that never
    /// overflows.
    fn bar(log: &Log) -> Toolbar {
        Toolbar::create(s("Formatting"))
            .with_item(ToolbarItem::create_button(s("save"), s("Save"), s("save")))
            .with_item(ToolbarItem::create_toggle(s("bold"), s("Bold"), s("format_bold"), false))
            .with_item(ToolbarItem::create_toggle(s("italic"), s("Italic"), s("format_italic"), true))
            .with_item(ToolbarItem::create_separator())
            .with_item(
                ToolbarItem::create_menu_button(
                    s("align"),
                    s("Align"),
                    s("format_align_left"),
                    choices(&["Left", "Center", "Right"]),
                )
                .with_show_label(true),
            )
            .with_item(
                ToolbarItem::create_button(s("delete"), s("Delete"), s("delete"))
                    .with_disabled(s("Select a file first")),
            )
            .with_item(ToolbarItem::create_spacer())
            .with_item(
                ToolbarItem::create_custom(
                    s("search"),
                    s("Search"),
                    Dom::create_div().with_class(s("search-box")),
                    160.0,
                )
                .with_never_overflow(true),
            )
            .with_on_event(RefAny::new(log.clone()), record as ToolbarOnEventCallbackType)
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The bar's direct children's node ids, in order.
    fn children(styled: &StyledDom) -> Vec<NodeId> {
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut out = Vec::new();
        let mut next = hierarchy[0].first_child_id(NodeId::new(0));
        while let Some(n) = next {
            out.push(n);
            next = hierarchy[n.index()].next_sibling_id();
        }
        out
    }

    fn a11y(node: &Dom) -> AccessibilityInfo {
        node.root.get_accessibility_info().cloned().unwrap_or_default()
    }

    fn name_of(node: &Dom) -> Option<String> {
        a11y(node).accessibility_name.as_ref().map(|n| n.as_str().to_string())
    }

    // ---- the overflow estimate ----

    #[test]
    fn an_items_width_counts_its_padding_icon_label_and_arrow() {
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(close(item_width(&icon_button("a")), 34.0), "an icon-only tool");
        assert!(
            close(
                item_width(&ToolbarItem::create_button(s("save"), s("Save"), s(""))),
                12.0 + 4.0 * 13.0 * 0.6 + 2.0
            ),
            "a label without an icon"
        );
        assert!(
            close(
                item_width(&ToolbarItem::create_button(s("open"), s("Open"), s("folder")).with_show_label(true)),
                12.0 + 20.0 + 4.0 + 4.0 * 13.0 * 0.6 + 2.0
            ),
            "an icon and its label"
        );
        assert!(
            close(
                item_width(&ToolbarItem::create_menu_button(s("m"), s("Menu"), s("star"), choices(&["x"]))),
                12.0 + 20.0 + 4.0 + 16.0 + 2.0
            ),
            "an icon-only menu button and its arrow, a gap apart"
        );
        assert!(close(item_width(&ToolbarItem::create_separator()), 9.0));
        assert!(close(item_width(&ToolbarItem::create_spacer()), 0.0));
        assert!(close(
            item_width(&ToolbarItem::create_custom(s("c"), s("C"), Dom::create_div(), 200.0)),
            202.0
        ));
    }

    #[test]
    fn everything_shows_without_a_limit_or_when_the_bar_is_wide_enough() {
        let log = log();
        let items = bar(&log).items.into_library_owned_vec();
        for available in [0.0, 10_000.0] {
            let f = fit(&items, available);
            assert_eq!(f.shown, (0..items.len()).collect::<Vec<_>>(), "{available}");
            assert!(f.overflow.is_empty(), "{available}");
        }
    }

    #[test]
    fn a_narrow_bar_moves_its_last_items_into_the_more_menu() {
        let items: Vec<ToolbarItem> = ["a", "b", "c", "d", "e"].iter().map(|i| icon_button(i)).collect();
        // 5 x 34 = 170 px; 120 px leave 86 beside the more button: two fit.
        let f = fit(&items, 120.0);
        assert_eq!(f.shown, vec![0, 1]);
        assert_eq!(f.overflow, vec![2, 3, 4]);
    }

    #[test]
    fn an_item_that_never_overflows_stays_and_the_ones_before_it_go_first() {
        let mut items: Vec<ToolbarItem> = ["a", "b", "c"].iter().map(|i| icon_button(i)).collect();
        items.push(
            ToolbarItem::create_custom(s("search"), s("Search"), Dom::create_div(), 100.0).with_never_overflow(true),
        );
        // 3 x 34 + 102 = 204 px; 180 px leave 146 beside the more button.
        let f = fit(&items, 180.0);
        assert_eq!(f.shown, vec![0, 3]);
        assert_eq!(f.overflow, vec![1, 2]);
    }

    #[test]
    fn a_separator_at_an_end_of_either_list_goes() {
        let sep = ToolbarItem::create_separator;
        // a b | c d: the separator would lead the menu.
        let items = vec![icon_button("a"), icon_button("b"), sep(), icon_button("c"), icon_button("d")];
        let f = fit(&items, 110.0);
        assert_eq!(f.shown, vec![0, 1]);
        assert_eq!(f.overflow, vec![3, 4]);
        // a | b c: the separator would end the bar.
        let items = vec![icon_button("a"), sep(), icon_button("b"), icon_button("c")];
        let f = fit(&items, 100.0);
        assert_eq!(f.shown, vec![0]);
        assert_eq!(f.overflow, vec![2, 3]);
        // a | | b: a doubled separator shows once.
        let items = vec![icon_button("a"), sep(), sep(), icon_button("b")];
        let f = fit(&items, 0.0);
        assert_eq!(f.shown, vec![0, 1, 3]);
    }

    #[test]
    fn a_spacer_never_overflows() {
        let items = vec![icon_button("a"), ToolbarItem::create_spacer(), icon_button("b"), icon_button("c")];
        let f = fit(&items, 80.0);
        assert_eq!(f.shown, vec![0, 1]);
        assert_eq!(f.overflow, vec![2, 3]);
    }

    // ---- the DOM ----

    #[test]
    fn the_bar_is_a_named_toolbar_with_one_child_per_item() {
        let log = log();
        for theme in checks::BOTH {
            let dom = bar(&log).with_theme(theme).dom();
            assert!(theme_checks::has_class(&dom, TOOLBAR_CLASS));
            let info = a11y(&dom);
            assert_eq!(info.role, AccessibilityRole::Toolbar);
            assert_eq!(name_of(&dom), Some(String::from("Formatting")));
            let kids = dom.children.as_ref();
            assert_eq!(kids.len(), 8, "{}: every item, no more button", theme.name());
            for tool in [0, 1, 2, 4, 5] {
                assert!(theme_checks::has_class(&kids[tool], ITEM_CLASS), "{}: tool {tool}", theme.name());
            }
            assert!(theme_checks::has_class(&kids[2], ITEM_PRESSED_CLASS), "italic is on");
            assert!(!theme_checks::has_class(&kids[1], ITEM_PRESSED_CLASS), "bold is off");
            assert!(theme_checks::has_class(&kids[3], SEPARATOR_CLASS));
            assert_eq!(a11y(&kids[3]).role, AccessibilityRole::Separator);
            assert!(theme_checks::has_class(&kids[6], SPACER_CLASS));
            assert!(theme_checks::has_class(&kids[7], CUSTOM_CLASS));
            assert!(theme_checks::find(&kids[7], "search-box").is_some(), "the app's control sits in the box");
            assert!(theme_checks::find(&dom, MORE_CLASS).is_none());
        }
    }

    #[test]
    fn the_tools_are_named_by_their_labels_and_announce_their_state() {
        let log = log();
        let dom = bar(&log).with_theme(UiTheme::Flat).dom();
        let kids = dom.children.as_ref();
        assert_eq!(name_of(&kids[0]), Some(String::from("Save")), "an icon-only tool is named by its label");
        assert_eq!(name_of(&kids[1]), Some(String::from("Bold")));
        assert!(a11y(&kids[1]).states.as_ref().contains(&AccessibilityState::CheckedFalse));
        assert!(a11y(&kids[2]).states.as_ref().contains(&AccessibilityState::CheckedTrue));
        let menu = a11y(&kids[4]);
        assert_eq!(menu.role, AccessibilityRole::ButtonMenu);
        assert!(menu.states.as_ref().contains(&AccessibilityState::Collapsed));
        let delete = a11y(&kids[5]);
        assert!(delete.states.as_ref().contains(&AccessibilityState::Unavailable));
        assert_eq!(
            delete.description.as_ref().map(|d| d.as_str().to_string()),
            Some(String::from("Select a file first"))
        );
    }

    #[test]
    fn the_tools_are_one_tab_stop_and_the_arrows_walk_them_wrapping() {
        let log = log();
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let kids = children(&styled);
        let tools: Vec<NodeId> = [0, 1, 2, 4, 5].iter().map(|i| kids[*i]).collect();
        let stops: Vec<Option<TabIndex>> = tools
            .iter()
            .map(|n| styled.node_data.as_ref()[n.index()].get_tab_index())
            .collect();
        assert_eq!(
            stops,
            vec![
                Some(TabIndex::Auto),
                Some(TabIndex::NoKeyboardFocus),
                Some(TabIndex::NoKeyboardFocus),
                Some(TabIndex::NoKeyboardFocus),
                Some(TabIndex::NoKeyboardFocus),
            ]
        );
        let step = |from: NodeId, key: K| {
            let (_, changes) = rv::press(&styled, id(from), key, &[]).expect("a key handler");
            assert!(rv::prevented(&changes), "{key:?} is the toolbar's");
            rv::focus_request(&changes)
        };
        assert_eq!(step(tools[0], K::Right), Some(id(tools[1])));
        assert_eq!(step(tools[2], K::Right), Some(id(tools[3])), "over the separator");
        assert_eq!(step(tools[4], K::Right), Some(id(tools[0])), "past the end: the first");
        assert_eq!(step(tools[0], K::Left), Some(id(tools[4])), "before the start: the last");
        assert_eq!(step(tools[1], K::End), Some(id(tools[4])));
        assert_eq!(step(tools[3], K::Home), Some(id(tools[0])));
        let (_, changes) = rv::press(&styled, id(tools[0]), K::Right, &[K::LShift]).expect("a key handler");
        assert_eq!(rv::focus_request(&changes), None, "a held modifier leaves the key alone");
        assert!(!rv::prevented(&changes));
    }

    #[test]
    fn a_click_activates_a_button_flips_a_toggle_and_a_disabled_tool_reports_nothing() {
        let log = log();
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let kids = children(&styled);
        let click = EventFilter::Hover(HoverEventFilter::Click);
        rv::fire(&styled, id(kids[0]), click).expect("a click target");
        rv::fire(&styled, id(kids[1]), click).expect("a click target");
        rv::fire(&styled, id(kids[2]), click).expect("a click target");
        let _ = rv::fire(&styled, id(kids[5]), click);
        assert_eq!(
            logged(&log),
            vec!["Activate save 0", "Toggle bold 1 pressed", "Toggle italic 2"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    /// The menu a callback opened, if it opened one.
    fn opened_menu(changes: &[CallbackChange]) -> Option<Menu> {
        changes.iter().find_map(|c| match c {
            CallbackChange::OpenMenu { menu, .. } => Some(menu.clone()),
            _ => None,
        })
    }

    fn label_of(item: &MenuItem) -> String {
        match item {
            MenuItem::String(s) => s.label.as_str().to_string(),
            MenuItem::Separator => String::from("-"),
            MenuItem::BreakLine => String::from("|"),
        }
    }

    #[test]
    fn a_menu_button_opens_its_choices_on_a_click_and_on_down() {
        let log = log();
        let styled = StyledDom::create_from_dom(bar(&log).with_theme(UiTheme::Flat).dom());
        let kids = children(&styled);
        let (_, changes) = rv::fire(&styled, id(kids[4]), EventFilter::Hover(HoverEventFilter::Click))
            .expect("a click target");
        let menu = opened_menu(&changes).expect("the choices");
        let labels: Vec<String> = menu.items.as_ref().iter().map(label_of).collect();
        assert_eq!(labels, vec!["Left", "Center", "Right"]);
        let (_, changes) = rv::press(&styled, id(kids[4]), K::Down, &[]).expect("a key handler");
        assert!(opened_menu(&changes).is_some(), "Down opens it too");
        assert!(rv::prevented(&changes));
    }

    #[test]
    fn a_picked_choice_names_its_menu_button_and_its_index() {
        let item = ToolbarItem::create_menu_button(s("align"), s("Align"), s("x"), choices(&["Left", "Right"]));
        let shared = RefAny::new(0_u8);
        let entries = choice_menu_items(&item, 4, &shared);
        assert_eq!(entries.len(), 2);
        let MenuItem::String(right) = &entries[1] else {
            panic!("an entry");
        };
        let cb = right.callback.as_ref().expect("a pick");
        let mut data = cb.refany.clone();
        let pick = data.downcast_ref::<MenuPick>().map(|p| (p.index, p.choice, p.kind));
        assert_eq!(pick, Some((4, 1, ToolbarEventKind::Choose)));
    }

    #[test]
    fn a_narrow_bar_ends_with_a_more_button_whose_menu_lists_the_rest() {
        let log = log();
        // 400 px of items in 300: Delete, Align and the separator go (the
        // separator would lead the menu, so it is dropped).
        let dom = bar(&log).with_available_width(300.0).with_theme(UiTheme::Flat).dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 6, "save, bold, italic, the spacer, the search field, more");
        let more = &kids[5];
        assert!(theme_checks::has_class(more, MORE_CLASS));
        assert!(theme_checks::has_class(more, ITEM_CLASS), "the more button is a tool too");
        assert_eq!(name_of(more), Some(String::from(TOOLBAR_MORE_LABEL)));
        assert!(theme_checks::find(&kids[4], "search-box").is_some(), "the search field stays");

        let items = bar(&log).items.into_library_owned_vec();
        let f = fit(&items, 300.0);
        assert_eq!(f.overflow, vec![4, 5]);
        let entries = overflow_menu_items(&items, &f.overflow, &RefAny::new(0_u8));
        let labels: Vec<String> = entries.iter().map(label_of).collect();
        assert_eq!(labels, vec!["Align", "Delete"]);
        let MenuItem::String(align) = &entries[0] else {
            panic!("an entry");
        };
        assert_eq!(align.children.len(), 3, "a menu button is a submenu of its choices");
        let MenuItem::String(delete) = &entries[1] else {
            panic!("an entry");
        };
        assert_eq!(delete.menu_item_state, MenuItemState::Disabled);
        assert!(delete.callback.is_none(), "a disabled entry picks nothing");
    }

    #[test]
    fn a_toggle_in_the_more_menu_is_a_check_entry_after_a_separator() {
        let items = vec![
            icon_button("a"),
            ToolbarItem::create_separator(),
            ToolbarItem::create_toggle(s("bold"), s("Bold"), s("format_bold"), true),
        ];
        let entries = overflow_menu_items(&items, &[0, 1, 2], &RefAny::new(0_u8));
        let labels: Vec<String> = entries.iter().map(label_of).collect();
        assert_eq!(labels, vec!["a", "-", "Bold"]);
        let MenuItem::String(bold) = &entries[2] else {
            panic!("an entry");
        };
        assert_eq!(bold.icon.as_ref(), Some(&MenuItemIcon::Checkbox(true)));
        let cb = bold.callback.as_ref().expect("a pick");
        let mut data = cb.refany.clone();
        let pick = data.downcast_ref::<MenuPick>().map(|p| (p.index, p.kind));
        assert_eq!(pick, Some((2, ToolbarEventKind::Toggle)));
    }

    #[test]
    fn a_toolbar_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let log = log();
        for width in [0.0, 300.0] {
            checks::assert_follows_the_app_theme(
                "toolbar",
                || bar(&log).with_available_width(width).dom(),
                |t: UiTheme| bar(&log).with_available_width(width).with_theme(t).dom(),
            );
            for theme in checks::BOTH {
                let dom = checks::under(theme, || bar(&log).with_available_width(width).dom());
                theme_checks::assert_structure_is_shared(&format!("toolbar built for {}", theme.name()), &dom, &[]);
                theme_checks::assert_theme_invariants(&format!("toolbar ({})", theme.name()), &dom);
            }
        }
    }
}
