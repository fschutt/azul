//! Thumbnail strip widget - a list of previews the user picks from and
//! reorders: PowerPoint's slide rail (a column: the number beside each
//! slide) and its slide sorter (a wrapping grid: the number under each),
//! with section headers that fold their run of items away.
//!
//! THE APP OWNS THE ITEMS: each [`ThumbnailItem`] carries the preview `Dom`
//! the app drew (the strip gives it a `thumb_width` x `thumb_height` px box
//! that clips it), its number, its accessible name, a badge glyph (a build,
//! a transition), and whether it is selected, hidden (dimmed) or the first
//! of a section ([`ThumbnailItem::section`]: a header titled so goes before
//! it; [`ThumbnailItem::section_collapsed`] folds the section's items). One
//! callback reports what happened ([`ThumbnailStripEvent`]):
//!
//! - `Select` (a click, or an arrow key: the item `index`; `shift` extends
//!   from the anchor, `ctrl` toggles - the app's selection rule);
//! - `Activate` (a double-click or Enter: open it in the editor);
//! - `Move` (a drag dropped on another item, or Ctrl+Up / Ctrl+Down: move
//!   `index` - and the app's other selected items - so they stand before the
//!   item that is now at `target`; `target == count` is the end);
//! - `Delete` (Delete / Backspace on the focused item);
//! - `SectionToggled` (a click on a header: `index` is the section's first
//!   item).
//!
//! DRAG REORDER goes through the engine's drag and drop: every item is
//! `draggable`; `DragStart` sets the item's index as drag data (MIME
//! [`DRAG_MIME`]), `DragOver` accepts a move, `Drop` reports the `Move`.
//! A drop on an item after the dragged one puts the dragged one after it,
//! a drop on one before puts it before.
//!
//! KEYBOARD (WAI-ARIA APG listbox): the items are ONE Tab stop - the active
//! item. Up / Down (and Left / Right in the grid) select the neighbour and
//! take the focus there, Shift extends; Home / End go to the ends; Ctrl /
//! Cmd + Up / Down move the item; Enter activates; Delete / Backspace delete.
//!
//! Key types: [`ThumbnailStrip`], [`ThumbnailItem`], [`ThumbnailStripEvent`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{AttributeType, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignContent, LayoutAlignItems, LayoutBoxSizing, LayoutDisplay,
            LayoutFlexDirection, LayoutFlexShrink, LayoutFlexWrap, LayoutHeight, LayoutMinHeight,
            LayoutOverflow, LayoutPosition, LayoutWidth,
        },
        property::CssProperty,
        style::StyleCursor,
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::{
        roving::{self, Step},
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The MIME type of a thumbnail drag: the dragged item's index as decimal text.
pub const DRAG_MIME: &str = "application/x-azul-thumbnail";

/// The strip's class.
pub(crate) const STRIP_CLASS: &str = "__azul-native-thumbnail-strip";
/// The strip's class in the grid layout (added to [`STRIP_CLASS`]).
pub(crate) const GRID_CLASS: &str = "__azul-native-thumbnail-strip-grid";
/// A section header.
pub(crate) const SECTION_CLASS: &str = "__azul-native-thumbnail-strip-section";
/// An item.
pub(crate) const ITEM_CLASS: &str = "__azul-native-thumbnail-strip-item";
/// A selected item (added to [`ITEM_CLASS`]).
pub(crate) const ITEM_SELECTED_CLASS: &str = "__azul-native-thumbnail-strip-item-selected";
/// A hidden item (added to [`ITEM_CLASS`]).
pub(crate) const ITEM_HIDDEN_CLASS: &str = "__azul-native-thumbnail-strip-item-hidden";
/// An item's number and badge.
pub(crate) const NUMBER_CLASS: &str = "__azul-native-thumbnail-strip-number";
/// An item's badge glyph.
pub(crate) const BADGE_CLASS: &str = "__azul-native-thumbnail-strip-badge";
/// The box that holds an item's preview.
pub(crate) const THUMB_CLASS: &str = "__azul-native-thumbnail-strip-thumb";

// ==== Types ====

/// A column (the slide rail) or a wrapping grid (the slide sorter).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ThumbnailStripLayout {
    #[default]
    Column,
    Grid,
}

/// One preview of the strip.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailItem {
    /// The preview the app drew; the strip clips it to its box.
    pub content: Dom,
    /// The number shown beside (column) or under (grid) the preview.
    pub label: AzString,
    /// The accessible name ("Slide 3: Why local-first").
    pub name: AzString,
    /// A glyph under the number (a `Dom::create_icon` name), or empty.
    pub badge: AzString,
    /// Non-empty: a section titled so starts at this item.
    pub section: AzString,
    /// Selected.
    pub selected: bool,
    /// Hidden from the show: dimmed.
    pub hidden: bool,
    /// The section that starts here is folded: its items are not shown.
    pub section_collapsed: bool,
}

impl ThumbnailItem {
    /// An item showing `content`, numbered `label`, named `name`.
    #[must_use]
    pub fn create(content: Dom, label: AzString, name: AzString) -> Self {
        Self {
            content,
            label,
            name,
            badge: AzString::from_const_str(""),
            section: AzString::from_const_str(""),
            selected: false,
            hidden: false,
            section_collapsed: false,
        }
    }

    /// The badge glyph.
    #[must_use]
    pub fn with_badge(mut self, badge: AzString) -> Self {
        self.badge = badge;
        self
    }

    /// A section titled `title` starts at this item.
    #[must_use]
    pub fn with_section(mut self, title: AzString) -> Self {
        self.section = title;
        self
    }

    /// Selected or not.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Hidden (dimmed) or not.
    #[must_use]
    pub const fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// The section that starts here folded or not.
    #[must_use]
    pub const fn with_section_collapsed(mut self, collapsed: bool) -> Self {
        self.section_collapsed = collapsed;
        self
    }
}

impl_option!(
    ThumbnailItem,
    OptionThumbnailItem,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    ThumbnailItem,
    ThumbnailItemVec,
    ThumbnailItemVecDestructor,
    ThumbnailItemVecDestructorType,
    ThumbnailItemVecSlice,
    OptionThumbnailItem
);
impl_vec_clone!(ThumbnailItem, ThumbnailItemVec, ThumbnailItemVecDestructor);
impl_vec_debug!(ThumbnailItem, ThumbnailItemVec);
impl_vec_partialeq!(ThumbnailItem, ThumbnailItemVec);
impl_vec_mut!(ThumbnailItem, ThumbnailItemVec);

/// What happened in the strip.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThumbnailStripEventKind {
    /// Item `index` was clicked or reached with an arrow key.
    Select,
    /// Item `index` was double-clicked or Enter was pressed on it.
    Activate,
    /// Move item `index` (and the other selected items) before the item at
    /// `target` (`target` = the count: to the end).
    Move,
    /// Delete / Backspace on item `index`.
    Delete,
    /// The section whose first item is `index` was folded or unfolded.
    SectionToggled,
}

/// One action in the strip.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThumbnailStripEvent {
    /// The item acted on.
    pub index: usize,
    /// `Move`: where it goes (before the item now at `target`).
    pub target: usize,
    /// What happened.
    pub kind: ThumbnailStripEventKind,
    /// `Select`: Shift was held (extend from the anchor).
    pub shift: bool,
    /// `Select`: Ctrl / Cmd was held (toggle).
    pub ctrl: bool,
}

impl ThumbnailStripEvent {
    /// A `kind` event on item `index`, no target, no modifiers.
    #[must_use]
    pub const fn create(kind: ThumbnailStripEventKind, index: usize) -> Self {
        Self {
            index,
            target: 0,
            kind,
            shift: false,
            ctrl: false,
        }
    }
}

/// Callback invoked for an action in the strip.
pub type ThumbnailStripOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ThumbnailStripEvent) -> Update;
impl_widget_callback!(
    ThumbnailStripOnEvent,
    OptionThumbnailStripOnEvent,
    ThumbnailStripOnEventCallback,
    ThumbnailStripOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ThumbnailStripOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: THUMBNAIL_STRIP_ON_EVENT_INVOKER,
    invoker_ty:     AzThumbnailStripOnEventCallbackInvoker,
    thunk_fn:       az_thumbnail_strip_on_event_callback_thunk,
    setter_fn:      AzApp_setThumbnailStripOnEventCallbackInvoker,
    from_handle_fn: AzThumbnailStripOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzThumbnailStripOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: ThumbnailStripEvent ],
}

/// The strip of previews (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailStrip {
    /// The items, in order.
    pub items: ThumbnailItemVec,
    /// Every action in the strip.
    pub on_event: OptionThumbnailStripOnEvent,
    /// The strip's accessible name ("Slides").
    pub accessibility_name: AzString,
    /// The item holding the Tab stop (the current slide).
    pub active: usize,
    /// A preview box's width, px.
    pub thumb_width: f32,
    /// A preview box's height, px.
    pub thumb_height: f32,
    /// A column or a grid.
    pub layout: ThumbnailStripLayout,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
}

impl ThumbnailStrip {
    /// A column of `items` with 160 x 90 px previews, the first active.
    #[must_use]
    pub fn create(items: ThumbnailItemVec) -> Self {
        Self {
            items,
            on_event: None.into(),
            accessibility_name: AzString::from_const_str("Thumbnails"),
            active: 0,
            thumb_width: 160.0,
            thumb_height: 90.0,
            layout: ThumbnailStripLayout::Column,
            theme: OptionUiTheme::None,
        }
    }

    /// Adds an item at the end.
    pub fn add_item(&mut self, item: ThumbnailItem) {
        let mut items = core::mem::replace(&mut self.items, ThumbnailItemVec::from_const_slice(&[]))
            .into_library_owned_vec();
        items.push(item);
        self.items = ThumbnailItemVec::from_vec(items);
    }

    /// [`Self::add_item`] for the builder chain.
    #[must_use]
    pub fn with_item(mut self, item: ThumbnailItem) -> Self {
        self.add_item(item);
        self
    }

    /// The item holding the Tab stop.
    pub const fn set_active(&mut self, active: usize) {
        self.active = active;
    }

    /// [`Self::set_active`] for the builder chain.
    #[must_use]
    pub const fn with_active(mut self, active: usize) -> Self {
        self.set_active(active);
        self
    }

    /// The preview box's size, px.
    pub fn set_thumb_size(&mut self, width: f32, height: f32) {
        self.thumb_width = width.max(1.0);
        self.thumb_height = height.max(1.0);
    }

    /// [`Self::set_thumb_size`] for the builder chain.
    #[must_use]
    pub fn with_thumb_size(mut self, width: f32, height: f32) -> Self {
        self.set_thumb_size(width, height);
        self
    }

    /// A column or a grid.
    pub const fn set_layout(&mut self, layout: ThumbnailStripLayout) {
        self.layout = layout;
    }

    /// [`Self::set_layout`] for the builder chain.
    #[must_use]
    pub const fn with_layout(mut self, layout: ThumbnailStripLayout) -> Self {
        self.set_layout(layout);
        self
    }

    /// The strip's accessible name.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// Every action in the strip.
    pub fn set_on_event<C: Into<ThumbnailStripOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_event = Some(ThumbnailStripOnEvent {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<ThumbnailStripOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_event(data, cb);
        self
    }

    /// Pin the widget theme; unset, the strip follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty strip and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The strip's DOM, in the pinned theme's look or both looks merged
    /// (the previews are built once).
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for ThumbnailStrip {
    fn default() -> Self {
        Self::create(ThumbnailItemVec::from_const_slice(&[]))
    }
}

impl From<ThumbnailStrip> for Dom {
    fn from(s: ThumbnailStrip) -> Self {
        s.dom()
    }
}

/// Where a dragged item goes when it is dropped on item `on`: after it when
/// it came from before it, before it otherwise (`target` of a `Move`).
#[must_use]
pub const fn drop_target(from: usize, on: usize) -> usize {
    if from < on {
        on + 1
    } else {
        on
    }
}
