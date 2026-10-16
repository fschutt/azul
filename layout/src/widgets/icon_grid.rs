//! Icon grid widget - a grid of icons or thumbnails with labels: a file
//! manager's icon view, a photo library, an e-reader's shelf, an app
//! launcher, a slide sorter of a million items.
//!
//! GENERIC OVER ITS DATA: the grid holds no items. It asks the app for the
//! items it shows through a DATA callback ([`IconGrid::with_data_source`]:
//! the item's label, its icon glyph, its thumbnail when the app has one,
//! a badge - and, optionally, extra lines under the label and the colour of
//! a placeholder tile for the glyph), given the item's index. Only the
//! items in view are ever asked for and built - THUMBNAILS ARRIVE LATER:
//! the app answers an icon glyph
//! until its thumbnail thread has the picture, then rebuilds and answers
//! the image.
//!
//! VIRTUALISED IN WHOLE ROWS (the data table's scroll window): the grid
//! lays the items in as many columns of `cell_width` px as its viewport
//! ([`IconGrid::with_viewport`]) holds and shows the rows from
//! [`IconGridView::top_row`] until the viewport is full. The wheel, the
//! keyboard and the grid's own scroll bar move `top_row`, never a pixel
//! offset, so a million items need no 100-million-pixel scroll extent.
//!
//! THE APP OWNS THE STATE: the selection (a [`ListSelection`] over the item
//! indices), the scroll position and a drag in progress are the
//! [`IconGridView`] the app hands in; every action reports an
//! [`IconGridEvent`] whose `view` is the NEXT view. The app stores
//! `event.view` and rebuilds. The kinds say what else happened.
//!
//! THE POINTER (Explorer's rules): a click selects an item (Ctrl / Cmd
//! toggles, Shift extends from the anchor); a press on an item that is
//! already selected keeps the selection until the release, so the whole
//! selection can be dragged out; a press on empty space starts a rubber
//! band that selects the items it crosses (Ctrl adds them) and, dragged
//! past the top or bottom edge, scrolls the rows under it; a double-click
//! activates; a right click reports a context menu (selecting the item
//! first). DRAG OUT goes through the engine's drag and drop: every item is
//! draggable; `DragStart` puts the dragged items' indices on the drag
//! (MIME [`ICON_GRID_DRAG_MIME`], "3,4,7") and reports
//! [`IconGridEventKind::DragStart`] so the app adds its own payload (file
//! paths) in its handler (`CallbackInfo::set_drag_data`).
//!
//! KEYBOARD (the grid is ONE Tab stop): the arrows move the focus by one
//! item or one row (Shift extends, Ctrl / Cmd moves the focus alone),
//! Page Up / Down by a screen, Home / End to the ends, Ctrl+A selects all,
//! Ctrl+Space toggles the focused item, Enter activates, the Menu key or
//! Shift+F10 reports a context menu, F2 edits the focused item's name in
//! place (Enter or leaving the field reports it, Escape gives it up),
//! Escape clears the selection, a letter
//! or digit selects the next item whose label starts with it (type-ahead,
//! around the end; the data callback is asked until one matches).
//!
//! A name wraps to two lines and ends in an ellipsis where it goes on. The
//! focused item's ring shows only while the grid has the focus.
//!
//! ACCESSIBILITY: the grid is a `List` that is `Multiselectable`, its value
//! says how many items are selected; every item shown is a `ListItem` named
//! by its label, `Selected` when it is.
//!
//! Key types: [`IconGrid`], [`IconGridItem`], [`IconGridView`],
//! [`IconGridEvent`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallbackData, Update},
    dom::{AttributeType, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass::Class, IdOrClassVec, TabIndex},
    events::FocusEventFilter,
    refany::RefAny,
    resources::{ImageRef, OptionImageRef},
    window::VirtualKeyCode,
};
use azul_css::{
    corety::{OptionU64, OptionUsize, U64Vec},
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorU, OptionColorU},
            StyleFontSize,
        },
        layout::{LayoutAlignItems, LayoutBoxSizing, LayoutFlexDirection, LayoutPosition},
        property::CssProperty,
        style::StyleCursor,
    },
    AzString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        data_table::ScrollBar,
        list_selection::ListSelection,
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The MIME type of an icon-grid drag: the dragged items' indices as
/// decimal text, comma separated ("3,4,7").
pub const ICON_GRID_DRAG_MIME: &str = "application/x-azul-icon-grid";

/// The grid's class.
pub(crate) const GRID_CLASS: &str = "__azul-native-icon-grid";
/// An item.
pub(crate) const ITEM_CLASS: &str = "__azul-native-icon-grid-item";
/// A selected item (added to [`ITEM_CLASS`]).
pub(crate) const ITEM_SELECTED_CLASS: &str = "__azul-native-icon-grid-item-selected";
/// The item the keyboard is on (added to [`ITEM_CLASS`]).
pub(crate) const ITEM_FOCUSED_CLASS: &str = "__azul-native-icon-grid-item-focused";
/// The box of an item's icon or thumbnail.
pub(crate) const THUMB_CLASS: &str = "__azul-native-icon-grid-thumb";
/// An item's label.
pub(crate) const LABEL_CLASS: &str = "__azul-native-icon-grid-label";
/// An item's badge glyph.
pub(crate) const BADGE_CLASS: &str = "__azul-native-icon-grid-badge";
/// An extra line under an item's label ([`IconGridItem::lines`]).
pub(crate) const LINE_CLASS: &str = "__azul-native-icon-grid-line";
/// Added to the thumbnail's box while it is a placeholder tile
/// ([`IconGridItem::placeholder`]).
pub(crate) const PLACEHOLDER_CLASS: &str = "__azul-native-icon-grid-placeholder";
/// The rubber band.
pub(crate) const MARQUEE_CLASS: &str = "__azul-native-icon-grid-marquee";
/// The scroll bar's track.
pub(crate) const TRACK_CLASS: &str = "__azul-native-icon-grid-track";
/// The scroll bar's thumb.
pub(crate) const SCROLL_THUMB_CLASS: &str = "__azul-native-icon-grid-scroll-thumb";
/// The field an item's name is edited in (F2), in the label's place.
pub(crate) const RENAME_CLASS: &str = "__azul-native-icon-grid-rename";

/// How far a press on a selected item may move before it is a drag (and no
/// longer a click that collapses the selection), px.
pub(crate) const ICON_GRID_CLICK_SLOP_PX: f32 = 4.0;

// ==== Types ====

/// One item - what the DATA callback answers for an index.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct IconGridItem {
    /// The label under the icon ("Holiday.jpg").
    pub label: AzString,
    /// The accessible name; empty = the label.
    pub name: AzString,
    /// The icon glyph (a `Dom::create_icon` name: "folder", "image") shown
    /// while there is no thumbnail.
    pub icon: AzString,
    /// A small glyph over the icon's corner ("`cloud_done`"), or empty.
    pub badge: AzString,
    /// The thumbnail, once the app has it (it replaces the icon).
    pub image: OptionImageRef,
    /// Extra lines under the label, in the secondary ink (an e-reader's
    /// author and reading progress), or empty for none. The app gives the
    /// cells the height they need (`IconGrid::with_cell_size`).
    pub lines: StringVec,
    /// The colour of the tile an item without a thumbnail shows its glyph
    /// on (a book without a cover), the glyph in black or white, whichever
    /// reads; `None`: the bare glyph. A thumbnail replaces the tile.
    pub placeholder: OptionColorU,
}

impl IconGridItem {
    /// An item labelled `label` showing the glyph `icon`.
    #[must_use]
    pub const fn create(label: AzString, icon: AzString) -> Self {
        Self {
            label,
            name: AzString::from_const_str(""),
            icon,
            badge: AzString::from_const_str(""),
            image: OptionImageRef::None,
            lines: StringVec::from_const_slice(&[]),
            placeholder: OptionColorU::None,
        }
    }

    /// An empty item (no label, no icon).
    #[must_use]
    pub const fn empty() -> Self {
        Self::create(AzString::from_const_str(""), AzString::from_const_str(""))
    }

    /// The thumbnail.
    pub fn set_image(&mut self, image: ImageRef) {
        self.image = OptionImageRef::Some(image);
    }

    /// [`Self::set_image`] for the builder chain.
    #[must_use]
    pub fn with_image(mut self, image: ImageRef) -> Self {
        self.set_image(image);
        self
    }

    /// The badge glyph.
    pub fn set_badge(&mut self, badge: AzString) {
        self.badge = badge;
    }

    /// [`Self::set_badge`] for the builder chain.
    #[must_use]
    pub fn with_badge(mut self, badge: AzString) -> Self {
        self.set_badge(badge);
        self
    }

    /// The accessible name (when the label is not enough: "Holiday.jpg,
    /// photo, 3 MB").
    pub fn set_name(&mut self, name: AzString) {
        self.name = name;
    }

    /// [`Self::set_name`] for the builder chain.
    #[must_use]
    pub fn with_name(mut self, name: AzString) -> Self {
        self.set_name(name);
        self
    }

    /// The extra lines under the label ("Jane Austen", "42 %").
    pub fn set_lines(&mut self, lines: StringVec) {
        self.lines = lines;
    }

    /// [`Self::set_lines`] for the builder chain.
    #[must_use]
    pub fn with_lines(mut self, lines: StringVec) -> Self {
        self.set_lines(lines);
        self
    }

    /// The colour of the tile the glyph sits on while there is no
    /// thumbnail.
    pub const fn set_placeholder(&mut self, color: ColorU) {
        self.placeholder = OptionColorU::Some(color);
    }

    /// [`Self::set_placeholder`] for the builder chain.
    #[must_use]
    pub const fn with_placeholder(mut self, color: ColorU) -> Self {
        self.set_placeholder(color);
        self
    }
}

impl Default for IconGridItem {
    fn default() -> Self {
        Self::empty()
    }
}

impl azul_core::host_invoker::HostOut for IconGridItem {
    fn unwritten() -> Self {
        Self::empty()
    }
}

/// The DATA callback: the item at `index`.
pub type IconGridDataSourceCallbackType = extern "C" fn(RefAny, usize) -> IconGridItem;
impl_widget_callback!(
    IconGridDataSource,
    OptionIconGridDataSource,
    IconGridDataSourceCallback,
    IconGridDataSourceCallbackType
);

// Host-invoker plumbing: the index carries no context, so the thunk reads it
// from the invocation slot (as the data table's data callback).
azul_core::impl_managed_callback! {
    wrapper:        IconGridDataSourceCallback,
    ctx_field:      ctx,
    data:           data: RefAny,
    args:           [index: usize],
    return_ty:      IconGridItem,
    default_ret:    IconGridItem::empty(),
    invoker_static: ICON_GRID_DATA_SOURCE_INVOKER,
    invoker_ty:     AzIconGridDataSourceCallbackInvoker,
    thunk_fn:       az_icon_grid_data_source_callback_thunk,
    setter_fn:      AzApp_setIconGridDataSourceCallbackInvoker,
    from_handle_fn: AzIconGridDataSourceCallback_createFromHostHandle,
    from_handle_byref_fn: AzIconGridDataSourceCallback_createFromHostHandleByref,
}

/// The item at `index`, from the data callback.
pub(crate) fn item_at(source: &OptionIconGridDataSource, index: usize) -> IconGridItem {
    match source.as_ref() {
        Some(IconGridDataSource { refany, callback }) => callback.invoke(refany.clone(), index),
        None => IconGridItem::empty(),
    }
}

/// What a pointer drag over the grid is doing.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum IconGridDragKind {
    /// No drag.
    #[default]
    None,
    /// A press on a selected item: the release (without a drag out) makes
    /// it the selection.
    Pending,
    /// A rubber band from (`start_x`, `start_y`) to (`x`, `y`).
    Marquee,
    /// The scroll bar's thumb.
    Thumb,
}

/// A pointer drag in progress (kept in the view between rebuilds).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct IconGridDrag {
    /// Marquee: the selection the band adds to (Ctrl), else empty.
    pub base: U64Vec,
    /// Pending: the pressed item.
    pub index: usize,
    /// Where the press was, px in the grid (Thumb: the window y).
    pub start_x: f32,
    /// See `start_x`.
    pub start_y: f32,
    /// Marquee: where the pointer is now, px in the grid.
    pub x: f32,
    /// See `x`.
    pub y: f32,
    /// Thumb: `top_row` when it was pressed.
    pub start_top: f32,
    /// What the drag does.
    pub kind: IconGridDragKind,
}

impl Default for IconGridDrag {
    fn default() -> Self {
        Self {
            base: U64Vec::from_const_slice(&[]),
            index: 0,
            start_x: 0.0,
            start_y: 0.0,
            x: 0.0,
            y: 0.0,
            start_top: 0.0,
            kind: IconGridDragKind::None,
        }
    }
}

/// The state of a grid the APP keeps. Every [`IconGridEvent`] carries the
/// next one; store it and rebuild.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct IconGridView {
    /// The selected items (keys = item indices), the anchor, the focus.
    pub selection: ListSelection,
    /// A pointer drag in progress.
    pub drag: IconGridDrag,
    /// The first row in view.
    pub top_row: usize,
    /// The item whose name is being edited in place (F2), if any.
    pub renaming: OptionUsize,
}

impl IconGridView {
    /// Nothing selected, at the top.
    #[must_use]
    pub fn create() -> Self {
        Self {
            selection: ListSelection::create(),
            drag: IconGridDrag::default(),
            top_row: 0,
            renaming: OptionUsize::None,
        }
    }

    /// The selection.
    pub fn set_selection(&mut self, selection: ListSelection) {
        self.selection = selection;
    }

    /// [`Self::set_selection`] for the builder chain.
    #[must_use]
    pub fn with_selection(mut self, selection: ListSelection) -> Self {
        self.set_selection(selection);
        self
    }

    /// The first row in view.
    pub const fn set_top_row(&mut self, top_row: usize) {
        self.top_row = top_row;
    }

    /// [`Self::set_top_row`] for the builder chain.
    #[must_use]
    pub const fn with_top_row(mut self, top_row: usize) -> Self {
        self.set_top_row(top_row);
        self
    }
}

impl Default for IconGridView {
    fn default() -> Self {
        Self::create()
    }
}

/// What happened in the grid. Every event carries the next [`IconGridView`];
/// the kinds say what ELSE the app does.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IconGridEventKind {
    /// The selection or the focus changed: store the view.
    Select,
    /// The grid scrolled (`top_row` moved): store the view.
    Scroll,
    /// Item `index` was double-clicked, or Enter was pressed on it: open it.
    Activate,
    /// A right click (or the Menu key, Shift+F10) on item `index` (none:
    /// on empty space) at (`x`, `y`) px in the grid: show a context menu.
    ContextMenu,
    /// A drag of the selection (item `index` under the pointer) left the
    /// grid: add the app's payload with `CallbackInfo::set_drag_data`.
    DragStart,
    /// A drag inside the grid (the rubber band, the scroll thumb) moved:
    /// store the view.
    Drag,
    /// Editing a name in place started (F2 on item `index`) or was given up
    /// (Escape): store the view, whose `renaming` says which.
    Rename,
    /// The name of item `index` was edited to `text` (Enter, or the field
    /// lost the focus): rename the item, store the view.
    Renamed,
}

/// One action in the grid.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct IconGridEvent {
    /// The view after the action: store it.
    pub view: IconGridView,
    /// The item acted on, if any.
    pub index: OptionUsize,
    /// `ContextMenu`: where, px in the grid.
    pub x: f32,
    /// See `x`.
    pub y: f32,
    /// What happened.
    pub kind: IconGridEventKind,
    /// Shift was held.
    pub shift: bool,
    /// The primary modifier was held: Cmd on macOS, Ctrl elsewhere.
    pub ctrl: bool,
    /// `Renamed`: the new name.
    pub text: AzString,
}

impl IconGridEvent {
    /// A `kind` event leaving `view`, nothing else set.
    #[must_use]
    pub const fn create(kind: IconGridEventKind, view: IconGridView) -> Self {
        Self {
            view,
            index: OptionUsize::None,
            x: 0.0,
            y: 0.0,
            kind,
            shift: false,
            ctrl: false,
            text: AzString::from_const_str(""),
        }
    }
}

/// The event that ends an edit in place with the name `text`: `Renamed`
/// with the new name, or `None` when nothing is being renamed.
pub(crate) fn rename_commit(g: &IconGrid, text: &str) -> Option<IconGridEvent> {
    let index = g.view.renaming.into_option()?;
    Some(renamed_event(g, index, text))
}

/// `Renamed` for item `index` with the name `text`; the edit ends.
fn renamed_event(g: &IconGrid, index: usize, text: &str) -> IconGridEvent {
    let mut next = g.view.clone();
    next.renaming = OptionUsize::None;
    next.drag = IconGridDrag::default();
    let mut e = IconGridEvent::create(IconGridEventKind::Renamed, next);
    e.index = OptionUsize::Some(index);
    e.text = AzString::from(text);
    e
}

/// `Rename` for item `index` with the edit given up.
fn rename_cancelled(g: &IconGrid, index: usize) -> IconGridEvent {
    let mut next = g.view.clone();
    next.renaming = OptionUsize::None;
    let mut e = IconGridEvent::create(IconGridEventKind::Rename, next);
    e.index = OptionUsize::Some(index);
    e
}

/// Callback invoked for an action in the grid.
pub type IconGridOnEventCallbackType = extern "C" fn(RefAny, CallbackInfo, IconGridEvent) -> Update;
impl_widget_callback!(
    IconGridOnEvent,
    OptionIconGridOnEvent,
    IconGridOnEventCallback,
    IconGridOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        IconGridOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ICON_GRID_ON_EVENT_INVOKER,
    invoker_ty:     AzIconGridOnEventCallbackInvoker,
    thunk_fn:       az_icon_grid_on_event_callback_thunk,
    setter_fn:      AzApp_setIconGridOnEventCallbackInvoker,
    from_handle_fn: AzIconGridOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzIconGridOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: IconGridEvent ],
}

/// A group of the grid's items under a header ("Today", "Folders"): from
/// item `start` up to the next group's start (or the end). Groups are in
/// the order of their starts; items before the first group have no header.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconGridGroup {
    /// The group's first item.
    pub start: usize,
    /// What its header says.
    pub label: AzString,
}

impl IconGridGroup {
    /// A group from item `start`, headed `label`.
    #[must_use]
    pub const fn create(start: usize, label: AzString) -> Self {
        Self { start, label }
    }
}

azul_css::impl_option!(IconGridGroup, OptionIconGridGroup, copy = false, [Debug, Clone, PartialEq, Eq]);
azul_css::impl_vec!(
    IconGridGroup,
    IconGridGroupVec,
    IconGridGroupVecDestructor,
    IconGridGroupVecDestructorType,
    IconGridGroupVecSlice,
    OptionIconGridGroup
);
azul_css::impl_vec_clone!(IconGridGroup, IconGridGroupVec, IconGridGroupVecDestructor);
azul_css::impl_vec_debug!(IconGridGroup, IconGridGroupVec);
azul_css::impl_vec_partialeq!(IconGridGroup, IconGridGroupVec);
azul_css::impl_vec_mut!(IconGridGroup, IconGridGroupVec);

/// A group header's height, px.
pub(crate) const GROUP_HEADER_PX: f32 = 28.0;
/// The class of a group's header.
pub(crate) const GROUP_HEADER_CLASS: &str = "__azul-native-icon-grid-group";

/// The icon grid (module docs).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct IconGrid {
    /// The app-owned state: selection, scroll, a drag in progress.
    pub view: IconGridView,
    /// Where the items come from; none = empty items.
    pub data_source: OptionIconGridDataSource,
    /// Hears every action.
    pub on_event: OptionIconGridOnEvent,
    /// What a screen reader calls the grid ("Pictures").
    pub accessibility_name: AzString,
    /// The `id` of the grid's node (default "icon-grid"): what an app or a
    /// script focuses it by.
    pub id: AzString,
    /// How many items the app has.
    pub count: usize,
    /// The px the grid fills (its scroll bar included).
    pub viewport_width: f32,
    /// See `viewport_width`.
    pub viewport_height: f32,
    /// A cell's width, px (the icon, its padding and the label's width).
    pub cell_width: f32,
    /// A cell's height, px.
    pub cell_height: f32,
    /// The icon's (and the thumbnail's box's) size, px.
    pub icon_size: f32,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// The groups the items fall into, each under its header; none: one
    /// run of items.
    pub groups: IconGridGroupVec,
}

impl IconGrid {
    /// A grid of `count` items in a `width` x `height` px viewport: 96 x
    /// 104 px cells with 48 px icons, nothing selected, no data until
    /// [`Self::with_data_source`].
    #[must_use]
    pub fn create(count: usize, width: f32, height: f32) -> Self {
        Self {
            view: IconGridView::create(),
            data_source: OptionIconGridDataSource::None,
            on_event: OptionIconGridOnEvent::None,
            accessibility_name: AzString::from_const_str("Items"),
            id: AzString::from_const_str("icon-grid"),
            count,
            viewport_width: width.max(0.0),
            viewport_height: height.max(0.0),
            cell_width: 96.0,
            cell_height: 104.0,
            icon_size: 48.0,
            theme: OptionUiTheme::None,
            groups: IconGridGroupVec::from_const_slice(&[]),
        }
    }

    /// The groups the items fall into (in the order of their starts).
    pub fn set_groups(&mut self, groups: IconGridGroupVec) {
        self.groups = groups;
    }

    /// [`Self::set_groups`] for the builder chain.
    #[must_use]
    pub fn with_groups(mut self, groups: IconGridGroupVec) -> Self {
        self.set_groups(groups);
        self
    }

    /// The view (what the last event carried).
    pub fn set_view(&mut self, view: IconGridView) {
        self.view = view;
    }

    /// [`Self::set_view`] for the builder chain.
    #[must_use]
    pub fn with_view(mut self, view: IconGridView) -> Self {
        self.set_view(view);
        self
    }

    /// Where the items come from.
    pub fn set_data_source<C: Into<IconGridDataSourceCallback>>(&mut self, data: RefAny, callback: C) {
        self.data_source = OptionIconGridDataSource::Some(IconGridDataSource {
            refany: data,
            callback: callback.into(),
        });
    }

    /// [`Self::set_data_source`] for the builder chain.
    #[must_use]
    pub fn with_data_source<C: Into<IconGridDataSourceCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_data_source(data, callback);
        self
    }

    /// Every action in the grid.
    pub fn set_on_event<C: Into<IconGridOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = OptionIconGridOnEvent::Some(IconGridOnEvent {
            refany: data,
            callback: callback.into(),
        });
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<IconGridOnEventCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// How many items the app has.
    pub const fn set_count(&mut self, count: usize) {
        self.count = count;
    }

    /// [`Self::set_count`] for the builder chain.
    #[must_use]
    pub const fn with_count(mut self, count: usize) -> Self {
        self.set_count(count);
        self
    }

    /// The px the grid fills.
    pub const fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width.max(0.0);
        self.viewport_height = height.max(0.0);
    }

    /// [`Self::set_viewport`] for the builder chain.
    #[must_use]
    pub const fn with_viewport(mut self, width: f32, height: f32) -> Self {
        self.set_viewport(width, height);
        self
    }

    /// A cell's size and the icon's, px.
    pub const fn set_cell_size(&mut self, width: f32, height: f32, icon_size: f32) {
        self.cell_width = width.max(1.0);
        self.cell_height = height.max(1.0);
        self.icon_size = icon_size.max(1.0);
    }

    /// [`Self::set_cell_size`] for the builder chain.
    #[must_use]
    pub const fn with_cell_size(mut self, width: f32, height: f32, icon_size: f32) -> Self {
        self.set_cell_size(width, height, icon_size);
        self
    }

    /// What a screen reader calls the grid.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// The `id` of the grid's node.
    pub fn set_id(&mut self, id: AzString) {
        self.id = id;
    }

    /// [`Self::set_id`] for the builder chain.
    #[must_use]
    pub fn with_id(mut self, id: AzString) -> Self {
        self.set_id(id);
        self
    }

    /// Pin the widget theme; unset, the grid follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty grid and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The grid's DOM: only the items in view, asked from the data callback.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        let extras = extras_for(self.theme);
        build(self, &look, &extras)
    }
}

impl Default for IconGrid {
    fn default() -> Self {
        Self::create(0, 0.0, 0.0)
    }
}

impl From<IconGrid> for Dom {
    fn from(g: IconGrid) -> Self {
        g.dom()
    }
}

// ==== Geometry, hit testing and the rules (the pure halves) ====

/// Where everything the grid shows sits.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Geometry {
    /// How many items the grid has.
    pub count: usize,
    /// Items per row (at least one).
    pub columns: usize,
    /// How many rows the items take.
    pub rows: usize,
    /// The rows wholly in view (Page Up / Down move by that many; at least
    /// one).
    pub page_rows: usize,
    /// The first row in view (`view.top_row`, kept in range).
    pub top: usize,
    /// The last `top` there is.
    pub max_top: usize,
    /// The first item in view.
    pub first: usize,
    /// One past the last item in view (a partly shown last row included).
    pub end: usize,
    /// The width the cells get (left of the scroll bar, if any).
    pub body_width: f32,
    /// The grid's height.
    pub height: f32,
    /// A cell's width.
    pub cell_width: f32,
    /// A cell's height.
    pub cell_height: f32,
    /// The scroll bar, when not every row fits.
    pub vbar: Option<ScrollBar>,
    /// A grouped grid's lines, top to bottom (its headers and its rows of
    /// items; `rows` counts them and `top` is one of them); empty for a
    /// grid without groups, whose lines are its uniform rows.
    pub lines: Vec<IconGridLine>,
    /// Where each of `lines` starts, px from the first line.
    pub tops: Vec<f32>,
}

/// One line of a grouped grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IconGridLine {
    /// The header of group `.0`.
    Header(usize),
    /// A row of `count` items from item `first`.
    Items { first: usize, count: usize },
}

impl Geometry {
    /// Where line (or row) `line` starts, px from the first one.
    #[allow(clippy::cast_precision_loss)] // rows in a grid: far fewer than 2^24
    pub(crate) fn line_y(&self, line: usize) -> f32 {
        if self.lines.is_empty() {
            line as f32 * self.cell_height
        } else {
            self.tops.get(line).copied().unwrap_or_else(|| self.tops.last().copied().unwrap_or(0.0))
        }
    }

    /// The height of line `line`.
    fn line_height(&self, line: usize) -> f32 {
        match self.lines.get(line) {
            Some(IconGridLine::Header(_)) => GROUP_HEADER_PX,
            _ => self.cell_height,
        }
    }

    /// The line of a grouped grid holding item `index`, and its column.
    fn line_of(&self, index: usize) -> Option<(usize, usize)> {
        self.lines.iter().enumerate().find_map(|(l, line)| match *line {
            IconGridLine::Items { first, count } if index >= first && index < first + count => {
                Some((l, index - first))
            }
            _ => None,
        })
    }

    /// The line a grouped grid shows `y` px below its top line at.
    fn line_at(&self, y: f32) -> Option<usize> {
        let content = self.line_y(self.top) + y;
        (self.top..self.lines.len())
            .find(|l| content >= self.tops[*l] && content < self.tops[*l] + self.line_height(*l))
    }
}

/// The lines of `count` items in `columns` columns, grouped as `groups`
/// says: the items before the first group, then each group's header and
/// its rows (a group starts a row).
fn group_lines(groups: &[IconGridGroup], count: usize, columns: usize) -> Vec<IconGridLine> {
    fn rows(lines: &mut Vec<IconGridLine>, from: usize, to: usize, columns: usize) {
        let mut first = from;
        while first < to {
            let n = (to - first).min(columns);
            lines.push(IconGridLine::Items { first, count: n });
            first += n;
        }
    }
    let columns = columns.max(1);
    let starts: Vec<usize> = groups.iter().map(|g| g.start.min(count)).collect();
    let mut lines = Vec::new();
    rows(&mut lines, 0, starts.first().copied().unwrap_or(count), columns);
    for (k, &start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(count).max(start);
        lines.push(IconGridLine::Header(k));
        rows(&mut lines, start, end, columns);
    }
    lines
}

/// Where each of `lines` starts, px from the first.
fn line_tops(lines: &[IconGridLine], cell_height: f32) -> Vec<f32> {
    let mut y = 0.0_f32;
    lines
        .iter()
        .map(|line| {
            let top = y;
            y += match line {
                IconGridLine::Header(_) => GROUP_HEADER_PX,
                IconGridLine::Items { .. } => cell_height,
            };
            top
        })
        .collect()
}

/// What a point in the grid is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(variant_size_differences)] // a 16-byte Copy value; boxing the index would cost more
pub(crate) enum Hit {
    /// Item `index`.
    Item(usize),
    /// Empty space between or after the items.
    Empty,
    /// The scroll bar's track, before (`false`) or after (`true`) its thumb.
    Track(bool),
    /// The scroll bar's thumb.
    Thumb,
    /// Outside the grid.
    Nothing,
}

/// How many whole `cell` px steps fit into `px` (at least one when `min_one`).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // finite, non-negative, small
fn steps(px: f32, cell: f32, min_one: bool) -> usize {
    let n = if px.is_finite() && px > 0.0 { (px / cell).floor() as usize } else { 0 };
    if min_one { n.max(1) } else { n }
}

/// The geometry of `g` as it is built now.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
pub(crate) fn geometry(g: &IconGrid) -> Geometry {
    use crate::widgets::data_table::{thumb, SCROLLBAR_PX};

    let width = g.viewport_width.max(0.0);
    let height = g.viewport_height.max(0.0);
    let cell_width = g.cell_width.max(1.0);
    let cell_height = g.cell_height.max(1.0);
    let count = g.count;
    if !g.groups.as_ref().is_empty() {
        return grouped_geometry(g, width, height, cell_width, cell_height);
    }
    let rows_for = |columns: usize| count.div_ceil(columns);
    let page_rows = steps(height, cell_height, true);
    let mut body_width = width;
    let mut columns = steps(body_width, cell_width, true);
    let mut rows = rows_for(columns);
    let overflows = rows > page_rows;
    if overflows {
        body_width = (width - SCROLLBAR_PX).max(0.0);
        columns = steps(body_width, cell_width, true);
        rows = rows_for(columns);
    }
    let max_top = rows.saturating_sub(page_rows);
    let top = g.view.top_row.min(max_top);
    // The rows shown: the whole ones and a part of the next.
    let shown_rows = if height > 0.0 { (height / cell_height).ceil() as usize } else { 0 };
    let first = (top * columns).min(count);
    let end = ((top + shown_rows) * columns).min(count);
    let vbar = overflows.then(|| {
        let clamp = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let (thumb_start, thumb_len) =
            thumb(height, page_rows as f32, rows as f32, clamp(top), clamp(max_top));
        ScrollBar {
            track: (body_width, 0.0, SCROLLBAR_PX.min(width), height),
            thumb_start,
            thumb_len,
        }
    });
    Geometry {
        count,
        columns,
        rows,
        page_rows,
        top,
        max_top,
        first,
        end,
        body_width,
        height,
        cell_width,
        cell_height,
        vbar,
        lines: Vec::new(),
        tops: Vec::new(),
    }
}

/// [`geometry`] of a grid with groups: its lines (headers and rows) of
/// their own heights, `top` the first line in view.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
fn grouped_geometry(g: &IconGrid, width: f32, height: f32, cell_width: f32, cell_height: f32) -> Geometry {
    use crate::widgets::data_table::{thumb, SCROLLBAR_PX};

    let count = g.count;
    let groups = g.groups.as_ref();
    let page_rows = steps(height, cell_height, true);
    let mut body_width = width;
    let mut columns = steps(body_width, cell_width, true);
    let mut lines = group_lines(groups, count, columns);
    let mut tops = line_tops(&lines, cell_height);
    let total = |lines: &[IconGridLine], tops: &[f32]| match (lines.last(), tops.last()) {
        (Some(IconGridLine::Header(_)), Some(t)) => t + GROUP_HEADER_PX,
        (Some(_), Some(t)) => t + cell_height,
        _ => 0.0,
    };
    let overflows = total(&lines, &tops) > height;
    if overflows {
        body_width = (width - SCROLLBAR_PX).max(0.0);
        columns = steps(body_width, cell_width, true);
        lines = group_lines(groups, count, columns);
        tops = line_tops(&lines, cell_height);
    }
    let full = total(&lines, &tops);
    // The last top whose lines fill the view to the end.
    let max_top = (0..lines.len()).find(|t| full - tops[*t] <= height).unwrap_or(0);
    let top = g.view.top_row.min(max_top);
    let origin = tops.get(top).copied().unwrap_or(0.0);
    let mut first = count;
    let mut end = 0;
    for (l, line) in lines.iter().enumerate().skip(top) {
        if tops[l] - origin >= height {
            break;
        }
        if let IconGridLine::Items { first: f, count: n } = *line {
            first = first.min(f);
            end = end.max(f + n);
        }
    }
    if end < first {
        first = 0;
        end = 0;
    }
    let rows = lines.len();
    let vbar = overflows.then(|| {
        let clamp = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let (thumb_start, thumb_len) =
            thumb(height, page_rows as f32, rows as f32, clamp(top), clamp(max_top));
        ScrollBar {
            track: (body_width, 0.0, SCROLLBAR_PX.min(width), height),
            thumb_start,
            thumb_len,
        }
    });
    Geometry {
        count,
        columns,
        rows,
        page_rows,
        top,
        max_top,
        first,
        end,
        body_width,
        height,
        cell_width,
        cell_height,
        vbar,
        lines,
        tops,
    }
}

/// What the point (`x`, `y`) px in the grid is over.
pub(crate) fn hit_test(geo: &Geometry, x: f32, y: f32) -> Hit {
    if let Some(bar) = geo.vbar {
        if bar.contains(x, y) {
            let along = y - bar.track.1;
            return if along < bar.thumb_start {
                Hit::Track(false)
            } else if along < bar.thumb_start + bar.thumb_len {
                Hit::Thumb
            } else {
                Hit::Track(true)
            };
        }
    }
    if !(x.is_finite() && y.is_finite()) || x < 0.0 || y < 0.0 || x >= geo.body_width || y >= geo.height {
        return Hit::Nothing;
    }
    if !geo.lines.is_empty() {
        // A header is no item; past a row's items is empty space.
        let column = steps(x, geo.cell_width, false);
        return match geo.line_at(y).map(|l| geo.lines[l]) {
            Some(IconGridLine::Items { first, count }) if column < count => Hit::Item(first + column),
            _ => Hit::Empty,
        };
    }
    let column = steps(x, geo.cell_width, false);
    if column >= geo.columns {
        return Hit::Empty;
    }
    let row = geo.top + steps(y, geo.cell_height, false);
    let index = row * geo.columns + column;
    if index < geo.count {
        Hit::Item(index)
    } else {
        Hit::Empty
    }
}

/// Item `index`'s cell (x, y, width, height px in the grid), when in view.
#[allow(clippy::cast_precision_loss)] // cells in view: small numbers
pub(crate) fn item_rect(geo: &Geometry, index: usize) -> Option<(f32, f32, f32, f32)> {
    if index < geo.first || index >= geo.end {
        return None;
    }
    if !geo.lines.is_empty() {
        let (line, column) = geo.line_of(index)?;
        return Some((
            column as f32 * geo.cell_width,
            geo.line_y(line) - geo.line_y(geo.top),
            geo.cell_width,
            geo.cell_height,
        ));
    }
    let row = index / geo.columns - geo.top;
    let column = index % geo.columns;
    Some((
        column as f32 * geo.cell_width,
        row as f32 * geo.cell_height,
        geo.cell_width,
        geo.cell_height,
    ))
}

/// The items whose cells the rectangle from (`x0`, `y0`) to (`x1`, `y1`)
/// crosses, where the y are px from the FIRST row (not from the view's top),
/// ascending: a rubber band whose anchor scrolled out of view.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn band_keys(geo: &Geometry, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<u64> {
    let (left, right) = (x0.min(x1).max(0.0), x0.max(x1));
    let (top, bottom) = (y0.min(y1).max(0.0), y0.max(y1));
    if geo.columns == 0 || geo.rows == 0 || right < 0.0 || bottom < 0.0 {
        return Vec::new();
    }
    let first_column = steps(left, geo.cell_width, false);
    if first_column >= geo.columns {
        return Vec::new();
    }
    let last_column = steps(right, geo.cell_width, false).min(geo.columns - 1);
    if !geo.lines.is_empty() {
        // The rows of items the band's height overlaps.
        let mut keys = Vec::new();
        for (l, line) in geo.lines.iter().enumerate() {
            let (line_top, line_bottom) = (geo.tops[l], geo.tops[l] + geo.line_height(l));
            if let IconGridLine::Items { first, count } = *line {
                if line_bottom > top && line_top <= bottom && first_column < count {
                    for column in first_column..=last_column.min(count - 1) {
                        keys.push((first + column) as u64);
                    }
                }
            }
        }
        return keys;
    }
    let first_row = steps(top, geo.cell_height, false);
    let last_row = steps(bottom, geo.cell_height, false).min(geo.rows - 1);
    let mut keys = Vec::new();
    for row in first_row..=last_row {
        for column in first_column..=last_column {
            let index = row * geo.columns + column;
            if index < geo.count {
                keys.push(index as u64);
            }
        }
    }
    keys
}

/// The view scrolled by `rows` rows (kept in range).
pub(crate) fn scroll_by(g: &IconGrid, geo: &Geometry, rows: i64) -> IconGridView {
    let mut next = g.view.clone();
    let top = i64::try_from(geo.top).unwrap_or(i64::MAX).saturating_add(rows);
    let max = i64::try_from(geo.max_top).unwrap_or(i64::MAX);
    next.top_row = usize::try_from(top.clamp(0, max)).unwrap_or(0);
    next
}

/// `view` scrolled so that item `index`'s row is in view.
fn reveal(view: &mut IconGridView, geo: &Geometry, index: usize) {
    if !geo.lines.is_empty() {
        let Some((line, _)) = geo.line_of(index) else {
            return;
        };
        // A group's first row shows with its header.
        let from = if line > 0 && matches!(geo.lines[line - 1], IconGridLine::Header(_)) {
            line - 1
        } else {
            line
        };
        let bottom = geo.line_y(line) + geo.cell_height;
        if from < view.top_row {
            view.top_row = from;
        } else if bottom - geo.line_y(view.top_row) > geo.height {
            view.top_row = (view.top_row..=line)
                .find(|t| bottom - geo.line_y(*t) <= geo.height)
                .unwrap_or(line);
        }
        view.top_row = view.top_row.min(geo.max_top);
        return;
    }
    let row = index / geo.columns.max(1);
    let page = geo.page_rows.max(1);
    if row < view.top_row {
        view.top_row = row;
    } else if row >= view.top_row + page {
        view.top_row = row + 1 - page;
    }
    view.top_row = view.top_row.min(geo.max_top);
}

/// What a press at (`x`, `y`) on `hit` does; `window_y` is the pointer's
/// window y (a thumb drag measures from it).
#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
pub(crate) fn press(
    g: &IconGrid,
    geo: &Geometry,
    hit: Hit,
    x: f32,
    y: f32,
    window_y: f32,
    shift: bool,
    ctrl: bool,
) -> Option<IconGridEvent> {
    let view = &g.view;
    let mut next = view.clone();
    next.drag = IconGridDrag::default();
    let event = match hit {
        Hit::Nothing => return None,
        Hit::Item(index) => {
            let key = index as u64;
            if !shift && !ctrl && view.selection.contains(key) {
                // Explorer: the selection stays until the release, so it
                // can be dragged out whole.
                next.selection.focus = OptionU64::Some(key);
                next.drag = IconGridDrag {
                    start_x: x,
                    start_y: y,
                    x,
                    y,
                    index,
                    kind: IconGridDragKind::Pending,
                    ..IconGridDrag::default()
                };
            } else {
                next.selection.select(key, shift, ctrl);
            }
            let mut e = IconGridEvent::create(IconGridEventKind::Select, next);
            e.index = OptionUsize::Some(index);
            e
        }
        Hit::Empty => {
            let base = if ctrl || shift {
                view.selection.keys.clone()
            } else {
                next.selection.clear();
                U64Vec::from_const_slice(&[])
            };
            next.drag = IconGridDrag {
                base,
                start_x: x,
                start_y: y,
                x,
                y,
                // The row the band is anchored to, should the grid scroll.
                start_top: geo.top as f32,
                kind: IconGridDragKind::Marquee,
                ..IconGridDrag::default()
            };
            IconGridEvent::create(IconGridEventKind::Select, next)
        }
        Hit::Track(after) => {
            let page = i64::try_from(geo.page_rows.max(1)).unwrap_or(1);
            let mut scrolled = scroll_by(g, geo, if after { page } else { -page });
            scrolled.drag = IconGridDrag::default();
            IconGridEvent::create(IconGridEventKind::Scroll, scrolled)
        }
        Hit::Thumb => {
            next.drag = IconGridDrag {
                start_y: window_y,
                start_top: geo.top as f32,
                kind: IconGridDragKind::Thumb,
                ..IconGridDrag::default()
            };
            IconGridEvent::create(IconGridEventKind::Drag, next)
        }
    };
    let mut event = event;
    event.shift = shift;
    event.ctrl = ctrl;
    Some(event)
}

/// What a pointer move to (`x`, `y`) during a drag does (`window_y`: the
/// pointer's window y).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
pub(crate) fn drag_move(g: &IconGrid, geo: &Geometry, x: f32, y: f32, window_y: f32) -> Option<IconGridEvent> {
    let view = &g.view;
    match view.drag.kind {
        // A pending press that moves is the engine's drag out (DragStart).
        IconGridDragKind::None | IconGridDragKind::Pending => None,
        IconGridDragKind::Marquee => {
            let d = &view.drag;
            // Past the top or the bottom edge the rows scroll under the band:
            // a row for every cell height (or part of one) the pointer is
            // beyond the edge.
            let rows = if y < 0.0 {
                -((-y / geo.cell_height).ceil().max(1.0) as i64)
            } else if y > geo.height {
                ((y - geo.height) / geo.cell_height).ceil().max(1.0) as i64
            } else {
                0
            };
            let mut next = view.clone();
            if rows != 0 {
                next.top_row = scroll_by(g, geo, rows).top_row;
            }
            let top = next.top_row.min(geo.max_top);
            // The band from the first row down: its anchor where it was
            // pressed, its end under the pointer.
            let anchor_y = d.start_y + geo.line_y(d.start_top as usize);
            let end_y = y + geo.line_y(top);
            let mut keys = d.base.as_slice().to_vec();
            keys.extend(band_keys(geo, d.start_x, anchor_y, x, end_y));
            next.drag.x = x;
            next.drag.y = y;
            next.selection.select_keys(U64Vec::from_vec(keys));
            Some(IconGridEvent::create(IconGridEventKind::Drag, next))
        }
        IconGridDragKind::Thumb => {
            let bar = geo.vbar?;
            let travel = bar.track.3 - bar.thumb_len;
            if travel <= 0.0 || geo.max_top == 0 {
                return None;
            }
            let moved = (window_y - view.drag.start_y) * geo.max_top as f32 / travel;
            let top = (view.drag.start_top + moved).round().clamp(0.0, geo.max_top as f32) as usize;
            if top == geo.top {
                return None;
            }
            let mut next = view.clone();
            next.top_row = top;
            Some(IconGridEvent::create(IconGridEventKind::Scroll, next))
        }
    }
}

/// What the release of a drag does.
pub(crate) fn drag_end(g: &IconGrid) -> Option<IconGridEvent> {
    let view = &g.view;
    let mut next = view.clone();
    next.drag = IconGridDrag::default();
    match view.drag.kind {
        IconGridDragKind::None => None,
        IconGridDragKind::Pending => {
            let index = view.drag.index;
            next.selection.click(index as u64);
            let mut e = IconGridEvent::create(IconGridEventKind::Select, next);
            e.index = OptionUsize::Some(index);
            Some(e)
        }
        IconGridDragKind::Marquee => Some(IconGridEvent::create(IconGridEventKind::Select, next)),
        IconGridDragKind::Thumb => Some(IconGridEvent::create(IconGridEventKind::Scroll, next)),
    }
}

/// The letter or digit `key` types, lower case: type-ahead matches case
/// folded, so whether Shift is held does not matter.
fn typed_letter(key: VirtualKeyCode) -> Option<char> {
    crate::widgets::terminal_view::us_char(key, false)
        .filter(u8::is_ascii_alphanumeric)
        .map(char::from)
}

/// Does `label` start with `letter` (lower case), case folded?
fn named_with(label: &str, letter: char) -> bool {
    label
        .trim_start()
        .chars()
        .next()
        .is_some_and(|c| c.to_lowercase().next() == Some(letter))
}

/// The item in the same column of the row above (`down` false) or below
/// item `index` in a grouped grid, across headers; a shorter row's last
/// item when the column is past its end. `None` at the first / last row.
fn row_neighbour(geo: &Geometry, index: usize, down: bool) -> Option<usize> {
    let (line, column) = geo.line_of(index)?;
    let mut l = line;
    loop {
        l = if down { l + 1 } else { l.checked_sub(1)? };
        if let IconGridLine::Items { first, count } = *geo.lines.get(l)? {
            return Some(first + column.min(count.saturating_sub(1)));
        }
    }
}

/// What `key` does (`shift`, `ctrl` = the primary modifier).
pub(crate) fn grid_key(g: &IconGrid, geo: &Geometry, key: VirtualKeyCode, shift: bool, ctrl: bool) -> Option<IconGridEvent> {
    use VirtualKeyCode as K;
    let view = &g.view;
    let count = g.count as u64;
    let focus = view.selection.focus.into_option().filter(|f| *f < count);
    let mut next = view.clone();
    next.drag = IconGridDrag::default();
    let on_focus = |kind: IconGridEventKind, next: IconGridView| {
        let index = focus?;
        let mut e = IconGridEvent::create(kind, next);
        e.index = OptionUsize::Some(usize::try_from(index).unwrap_or(0));
        Some(e)
    };
    let columns = i64::try_from(geo.columns.max(1)).unwrap_or(1);
    let page = i64::try_from(geo.page_rows.max(1)).unwrap_or(1) * columns;
    let all = i64::try_from(count).unwrap_or(i64::MAX);
    // A grouped grid: Up / Down go to the row above or below in the same
    // column (across a header, onto a shorter row's last item).
    let vertical = |down: bool, times: i64| -> Option<i64> {
        let start = usize::try_from(focus?).ok()?;
        let mut at = start;
        for _ in 0..times.max(1) {
            match row_neighbour(geo, at, down) {
                Some(next) => at = next,
                None => break,
            }
        }
        if at == start {
            return None;
        }
        Some(i64::try_from(at).ok()? - i64::try_from(start).ok()?)
    };
    let grouped = !geo.lines.is_empty();
    let page_lines = i64::try_from(geo.page_rows.max(1)).unwrap_or(1);
    let delta = match key {
        K::Left => Some(-1),
        K::Right => Some(1),
        K::Up if grouped => vertical(false, 1),
        K::Down if grouped => vertical(true, 1),
        K::PageUp if grouped => vertical(false, page_lines),
        K::PageDown if grouped => vertical(true, page_lines),
        K::Up => Some(-columns),
        K::Down => Some(columns),
        K::PageUp => Some(-page),
        K::PageDown => Some(page),
        K::Home => Some(-all),
        K::End => Some(all),
        _ => None,
    };
    if let Some(delta) = delta {
        let target = next.selection.step(delta, shift, ctrl, count).into_option()?;
        let index = usize::try_from(target).unwrap_or(0);
        reveal(&mut next, geo, index);
        let mut e = IconGridEvent::create(IconGridEventKind::Select, next);
        e.index = OptionUsize::Some(index);
        e.shift = shift;
        e.ctrl = ctrl;
        return Some(e);
    }
    if !ctrl {
        if let Some(letter) = typed_letter(key) {
            // Type-ahead (Explorer, Finder): the next item whose label
            // starts with the letter, after the focused one, around the end.
            let start = focus.map_or(0, |f| f + 1);
            let target = (0..count).map(|i| (start + i) % count).find(|&i| {
                let item = item_at(&g.data_source, usize::try_from(i).unwrap_or(0));
                named_with(item.label.as_str(), letter)
            })?;
            let index = usize::try_from(target).unwrap_or(0);
            next.selection.click(target);
            reveal(&mut next, geo, index);
            let mut e = IconGridEvent::create(IconGridEventKind::Select, next);
            e.index = OptionUsize::Some(index);
            return Some(e);
        }
    }
    match key {
        K::A if ctrl => {
            next.selection.select_all(count);
            Some(IconGridEvent::create(IconGridEventKind::Select, next))
        }
        K::Space if ctrl => {
            next.selection.toggle_focused();
            on_focus(IconGridEventKind::Select, next)
        }
        K::Return | K::NumpadEnter => on_focus(IconGridEventKind::Activate, next),
        K::F2 if !shift && !ctrl => {
            let index = usize::try_from(focus?).ok()?;
            next.renaming = OptionUsize::Some(index);
            reveal(&mut next, geo, index);
            on_focus(IconGridEventKind::Rename, next)
        }
        K::Apps => on_focus(IconGridEventKind::ContextMenu, next),
        K::F10 if shift => on_focus(IconGridEventKind::ContextMenu, next),
        K::Escape if !view.selection.is_empty() => {
            next.selection.clear();
            Some(IconGridEvent::create(IconGridEventKind::Select, next))
        }
        _ => None,
    }
}

// ==== The look and the DOM ====

/// What a theme decides about a grid: the SKIN of each part, laid over the
/// part's base by [`build`].
#[derive(Debug, Clone, Default)]
pub(crate) struct IconGridLook {
    /// The grid (its ground, its font, its focus ring).
    pub grid: Vec<CssPropertyWithConditions>,
    /// An item (its padding, radius, hover).
    pub item: Vec<CssPropertyWithConditions>,
    /// Stacked on a selected item.
    pub item_selected: Vec<CssPropertyWithConditions>,
    /// Stacked on the item the keyboard is on.
    pub item_focused: Vec<CssPropertyWithConditions>,
    /// The icon glyph.
    pub icon: Vec<CssPropertyWithConditions>,
    /// The label.
    pub label: Vec<CssPropertyWithConditions>,
    /// The badge glyph.
    pub badge: Vec<CssPropertyWithConditions>,
    /// The rubber band.
    pub marquee: Vec<CssPropertyWithConditions>,
    /// The scroll bar's track.
    pub track: Vec<CssPropertyWithConditions>,
    /// The scroll bar's thumb.
    pub thumb: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the grid, if it has one.
    pub marker: Option<&'static str>,
}

/// The look a grid with the theme option `theme` is built with: the pinned
/// theme's own look, or both looks merged part by part (the DOM is built
/// once - the data callback is asked once per item in view).
pub(crate) fn look_for(theme: OptionUiTheme) -> IconGridLook {
    use crate::widgets::themes::{flat, flora, theme_blocks::follow_props};
    match theme.into_option() {
        Some(UiTheme::Flat) => flat::icon_grid_look(),
        Some(UiTheme::Flora) => flora::icon_grid_look(),
        None => {
            let (a, b) = (flat::icon_grid_look(), flora::icon_grid_look());
            let both = |x: &[CssPropertyWithConditions], y: &[CssPropertyWithConditions]| {
                follow_props(x, y).into_library_owned_vec()
            };
            IconGridLook {
                grid: both(&a.grid, &b.grid),
                item: both(&a.item, &b.item),
                item_selected: both(&a.item_selected, &b.item_selected),
                item_focused: both(&a.item_focused, &b.item_focused),
                icon: both(&a.icon, &b.icon),
                label: both(&a.label, &b.label),
                badge: both(&a.badge, &b.badge),
                marquee: both(&a.marquee, &b.marquee),
                track: both(&a.track, &b.track),
                thumb: both(&a.thumb, &b.thumb),
                marker: match UiTheme::current() {
                    UiTheme::Flat => a.marker,
                    UiTheme::Flora => b.marker,
                },
            }
        }
    }
}

/// What a theme decides about an item's OPTIONAL extras
/// ([`IconGridItem::lines`], [`IconGridItem::placeholder`]): the skin of
/// each, laid over its base by [`build`]; built by
/// `themes::flat::icon_grid_extras_look` and
/// `themes::flora::icon_grid_extras_look`. An item without extras never
/// reads it.
#[derive(Debug, Clone, Default)]
pub(crate) struct IconGridExtrasLook {
    /// An extra line under the label (the secondary ink, a size down).
    pub line: Vec<CssPropertyWithConditions>,
    /// The placeholder tile (its corners; the item gives its colour).
    pub placeholder: Vec<CssPropertyWithConditions>,
}

/// The extras' look a grid with the theme option `theme` is built with, as
/// [`look_for`]: the pinned theme's own, or both merged part by part.
pub(crate) fn extras_for(theme: OptionUiTheme) -> IconGridExtrasLook {
    use crate::widgets::themes::{flat, flora, theme_blocks::follow_props};
    match theme.into_option() {
        Some(UiTheme::Flat) => flat::icon_grid_extras_look(),
        Some(UiTheme::Flora) => flora::icon_grid_extras_look(),
        None => {
            let (a, b) = (flat::icon_grid_extras_look(), flora::icon_grid_extras_look());
            IconGridExtrasLook {
                line: follow_props(&a.line, &b.line).into_library_owned_vec(),
                placeholder: follow_props(&a.placeholder, &b.placeholder).into_library_owned_vec(),
            }
        }
    }
}

// ---- the base: the grid's structure, in every theme ----

/// A box at (`x`, `y`), `w` x `h` px, absolutely placed in the grid.
fn placed(x: f32, y: f32, w: f32, h: f32) -> [CssPropertyWithConditions; 5] {
    use crate::widgets::themes::decl;
    [
        decl::position(LayoutPosition::Absolute),
        decl::px_left(x),
        decl::px_top(y),
        decl::px_width(w),
        decl::px_height(h),
    ]
}

/// The grid: the viewport's size, the positioning context of its cells,
/// clipping them.
fn grid_base(width: f32, height: f32) -> Vec<CssPropertyWithConditions> {
    use crate::widgets::themes::decl;
    alloc::vec![
        decl::position(LayoutPosition::Relative),
        decl::overflow_x_hidden(),
        decl::overflow_y_hidden(),
        decl::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
        decl::simple(CssProperty::const_cursor(StyleCursor::Default)),
        decl::px_width(width),
        decl::px_height(height),
        decl::no_shrink(),
    ]
}

/// An item: its cell, the thumbnail over the label, centred, a 1 px border
/// (the skin colours it: transparent at rest) and clipped.
fn item_base(x: f32, y: f32, w: f32, h: f32) -> Vec<CssPropertyWithConditions> {
    use crate::widgets::themes::decl;
    let mut props = placed(x, y, w, h).to_vec();
    props.extend([
        decl::display_flex(),
        decl::flex_direction(LayoutFlexDirection::Column),
        decl::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        decl::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
        decl::overflow_x_hidden(),
        decl::overflow_y_hidden(),
    ]);
    props.extend(decl::border(1));
    props
}

/// The thumbnail's box: `size` px square, the glyph or the picture centred
/// in it, the badge's positioning context.
fn thumb_base(size: f32) -> Vec<CssPropertyWithConditions> {
    use crate::widgets::themes::decl;
    alloc::vec![
        decl::position(LayoutPosition::Relative),
        decl::display_flex(),
        decl::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        decl::simple(CssProperty::const_justify_content(
            azul_css::props::layout::LayoutJustifyContent::Center
        )),
        decl::no_shrink(),
        decl::px_width(size),
        decl::px_height(size),
    ]
}

/// An item's name: the cell's width, centred, wrapping to two lines at
/// most, the second ending in an ellipsis where the name goes on; a word
/// too long for a line breaks.
fn name_base() -> Vec<CssPropertyWithConditions> {
    use azul_css::props::style::{StyleLineClamp, StyleOverflowWrap};

    use crate::widgets::themes::decl;
    alloc::vec![
        decl::simple(CssProperty::const_width(azul_css::props::layout::LayoutWidth::Px(
            azul_css::props::basic::pixel::PixelValue::const_percent(100)
        ))),
        decl::simple(CssProperty::const_text_align(azul_css::props::style::StyleTextAlign::Center)),
        decl::simple(CssProperty::from(StyleLineClamp { max_lines: 2 })),
        decl::simple(CssProperty::from(StyleOverflowWrap::Anywhere)),
        decl::overflow_x_hidden(),
        decl::overflow_y_hidden(),
    ]
}

/// The name field's box: the cell's width.
fn rename_base() -> Vec<CssPropertyWithConditions> {
    use crate::widgets::themes::decl;
    alloc::vec![
        decl::simple(CssProperty::const_width(azul_css::props::layout::LayoutWidth::Px(
            azul_css::props::basic::pixel::PixelValue::const_percent(100)
        ))),
        decl::display_flex(),
    ]
}

/// The label: the cell's width, centred, on one line, clipped.
fn label_base() -> Vec<CssPropertyWithConditions> {
    use crate::widgets::themes::decl;
    alloc::vec![
        decl::simple(CssProperty::const_width(azul_css::props::layout::LayoutWidth::Px(
            azul_css::props::basic::pixel::PixelValue::const_percent(100)
        ))),
        decl::simple(CssProperty::const_text_align(azul_css::props::style::StyleTextAlign::Center)),
        decl::nowrap(),
        decl::overflow_x_hidden(),
    ]
}

/// A part's declarations: its base (the structure), then the look's skin.
fn part(base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
}

/// `extra` stacked onto `base` (`theme_blocks::stack_parts`).
fn stacked(base: CssPropertyWithConditionsVec, extra: &[CssPropertyWithConditions]) -> CssPropertyWithConditionsVec {
    crate::widgets::themes::theme_blocks::stack_parts(&base, &CssPropertyWithConditionsVec::from_vec(extra.to_vec()))
}

/// What every handler of one grid shares: the grid as built (its view kept
/// current between rebuilds) and its geometry.
pub(crate) struct GridShared {
    pub grid: IconGrid,
    pub geo: Geometry,
}

/// An item's payload: its index and the shared part.
struct ItemData {
    index: usize,
    shared: RefAny,
}

/// The grid's DOM in `look`: [item..] for the items in view, the rubber
/// band while one is drawn, the scroll bar when the rows overflow. An item
/// is [thumb [picture | glyph, badge?], label, line..]: its extra lines
/// and its placeholder tile in `extras` (only an item that has them).
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
pub(crate) fn build(grid: IconGrid, look: &IconGridLook, extras: &IconGridExtrasLook) -> Dom {
    use crate::widgets::themes::decl;

    let geo = geometry(&grid);
    let view = grid.view.clone();
    let focus = view.selection.focus.into_option();
    let icon_px = grid.icon_size.max(1.0);
    let shared = RefAny::new(GridShared {
        grid: grid.clone(),
        geo: geo.clone(),
    });

    let mut children: Vec<Dom> = Vec::with_capacity(geo.end.saturating_sub(geo.first) + 2);
    for index in geo.first..geo.end {
        let Some((x, y, w, h)) = item_rect(&geo, index) else {
            continue;
        };
        let item = item_at(&grid.data_source, index);
        let key = index as u64;
        let is_selected = view.selection.contains(key);
        let is_focused = focus == Some(key);

        // No thumbnail and a placeholder colour: the glyph sits on a tile of
        // that colour, in black or white, whichever reads on it.
        let tile = if item.image.is_none() {
            item.placeholder.into_option()
        } else {
            None
        };
        let picture = if let Some(image) = item.image.into_option() { Dom::create_image(image).with_css_props(CssPropertyWithConditionsVec::from_vec(
            decl::fill_box().to_vec(),
        )) } else {
            let glyph = part(
                &[decl::simple(CssProperty::const_font_size(StyleFontSize::const_px(
                    icon_px.round() as isize,
                )))],
                &look.icon,
            );
            let glyph = match tile {
                Some(color) => stacked(glyph, &[decl::simple(decl::ink(color.contrast_text()))]),
                None => glyph,
            };
            Dom::create_icon(item.icon.clone()).with_css_props(glyph)
        };
        let mut in_thumb = alloc::vec![picture];
        if !item.badge.as_str().is_empty() {
            in_thumb.push(
                Dom::create_icon(item.badge.clone())
                    .with_class(AzString::from_const_str(BADGE_CLASS))
                    .with_css_props(part(
                        &[
                            decl::position(LayoutPosition::Absolute),
                            decl::px_left((icon_px - 16.0).max(0.0)),
                            decl::px_bottom(0.0),
                        ],
                        &look.badge,
                    )),
            );
        }
        let thumb = Dom::create_div().with_class(AzString::from_const_str(THUMB_CLASS));
        let thumb = match tile {
            Some(color) => {
                let mut style = decl::on_base(&thumb_base(icon_px), &extras.placeholder);
                style.push(decl::simple(decl::fill(color)));
                thumb
                    .with_class(AzString::from_const_str(PLACEHOLDER_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
            }
            None => thumb.with_css_props(CssPropertyWithConditionsVec::from_vec(thumb_base(icon_px))),
        };
        let thumb = thumb.with_children(DomVec::from_vec(in_thumb));
        let label = if view.renaming.into_option() == Some(index) {
            rename_field(&shared, index, item.label.clone(), grid.theme)
        } else {
            crate::widgets::widget_p_with_text(item.label.clone())
                .with_class(AzString::from_const_str(LABEL_CLASS))
                .with_css_props(part(&name_base(), &look.label))
        };
        // The extra lines under the label, in order.
        let mut parts = alloc::vec![thumb, label];
        for line in item.lines.as_ref() {
            parts.push(
                crate::widgets::widget_p_with_text(line.clone())
                    .with_class(AzString::from_const_str(LINE_CLASS))
                    .with_css_props(part(&label_base(), &extras.line)),
            );
        }

        let mut style = part(&item_base(x, y, w, h), &look.item);
        let mut classes = alloc::vec![Class(AzString::from_const_str(ITEM_CLASS))];
        if is_selected {
            style = stacked(style, &look.item_selected);
            classes.push(Class(AzString::from_const_str(ITEM_SELECTED_CLASS)));
        }
        if is_focused {
            // Its ring is the grid's rule (`focus_ring_sheet`): it shows
            // only while the grid has the focus.
            classes.push(Class(AzString::from_const_str(ITEM_FOCUSED_CLASS)));
        }
        let name = if item.name.as_str().is_empty() {
            item.label.clone()
        } else {
            item.name.clone()
        };
        // `<grid id>-<index>`: what an app or a script finds the item by
        // (the item's index, wherever it sits in view).
        if !grid.id.as_str().is_empty() {
            classes.push(azul_core::dom::IdOrClass::Id(AzString::from(alloc::format!(
                "{}-{index}",
                grid.id.as_str()
            ))));
        }
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(classes))
                .with_css_props(style)
                .with_attribute(AttributeType::Draggable(true))
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::ListItem,
                    accessibility_name: Some(name).into(),
                    states: if is_selected {
                        AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Selected])
                    } else {
                        AccessibilityStateVec::from_const_slice(&[])
                    },
                    ..Default::default()
                })
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::DragStart),
                    RefAny::new(ItemData {
                        index,
                        shared: shared.clone(),
                    }),
                    on_item_drag_start as usize,
                )
                .with_children(DomVec::from_vec(parts)),
        );
    }

    // The headers of the groups in view, across the grid.
    for (l, line) in geo.lines.iter().enumerate().skip(geo.top) {
        let y = geo.line_y(l) - geo.line_y(geo.top);
        if y >= geo.height {
            break;
        }
        if let IconGridLine::Header(group) = *line {
            let label = grid
                .groups
                .as_ref()
                .get(group)
                .map_or_else(|| AzString::from_const_str(""), |g| g.label.clone());
            let mut style = placed(0.0, y, geo.body_width, GROUP_HEADER_PX).to_vec();
            style.extend(group_header_style());
            children.push(
                crate::widgets::widget_p_with_text(label)
                    .with_class(AzString::from_const_str(GROUP_HEADER_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(style)),
            );
        }
    }

    // The rubber band, while one is drawn.
    let drag = &view.drag;
    if drag.kind == IconGridDragKind::Marquee {
        // The anchor moves with its row when the grid scrolled under the
        // band; the band stays inside the grid.
        let anchor_y = drag.start_y + geo.line_y(drag.start_top as usize) - geo.line_y(geo.top);
        let (left, top) = (drag.start_x.min(drag.x).max(0.0), anchor_y.min(drag.y).max(0.0));
        let (right, bottom) = (drag.start_x.max(drag.x), anchor_y.max(drag.y).min(geo.height));
        children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(MARQUEE_CLASS))
                .with_css_props(part(
                    &placed(left, top, (right - left).max(0.0), (bottom - top).max(0.0)),
                    &look.marquee,
                )),
        );
    }

    // The scroll bar.
    if let Some(bar) = geo.vbar {
        let (tx, ty, tw, th) = bar.track;
        let thumb = Dom::create_div()
            .with_class(AzString::from_const_str(SCROLL_THUMB_CLASS))
            .with_css_props(part(&placed(0.0, bar.thumb_start, tw, bar.thumb_len), &look.thumb));
        children.push(
            Dom::create_div()
                .with_class(AzString::from_const_str(TRACK_CLASS))
                .with_css_props(part(&placed(tx, ty, tw, th), &look.track))
                .with_child(thumb),
        );
    }

    let mut classes = alloc::vec![Class(AzString::from_const_str(GRID_CLASS))];
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    let selected = view.selection.len();
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_id(grid.id.clone())
        .with_css_props(part(&grid_base(grid.viewport_width, grid.viewport_height), &look.grid))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::List,
            accessibility_name: Some(grid.accessibility_name.clone()).into(),
            accessibility_value: Some(AzString::from(alloc::format!("{selected} of {} selected", grid.count))).into(),
            states: AccessibilityStateVec::from_vec(alloc::vec![AccessibilityState::Multiselectable]),
            ..Default::default()
        })
        .with_callbacks(grid_callbacks(&shared).into())
        .with_component_css(focus_ring_sheet(&look.item_focused))
        .with_children(DomVec::from_vec(children))
}

/// A group header: its label in the secondary ink, bold and small, on the
/// header's midline, over a rule in the separator colour - the desktop's
/// (and flora's) `system:` colours, so it sits with either theme.
fn group_header_style() -> Vec<CssPropertyWithConditions> {
    use azul_css::props::{
        basic::pixel::PixelValue,
        style::{StyleBorderBottomColor, StyleBorderBottomStyle, StyleLineHeight, BorderStyle, LayoutBorderBottomWidth},
    };

    use crate::widgets::themes::{decl, system_palette};
    alloc::vec![
        decl::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
        decl::bold(),
        decl::simple(CssProperty::const_line_height(StyleLineHeight::Length(PixelValue::const_px(27)))),
        decl::px_padding_left(8.0),
        decl::simple(decl::ink(system_palette::SECONDARY_TEXT)),
        decl::simple(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
        decl::simple(CssProperty::const_border_bottom_style(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        })),
        decl::simple(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
            inner: system_palette::SEPARATOR,
        })),
        decl::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
        decl::simple(CssProperty::user_select(azul_css::props::style::StyleUserSelect::None)),
    ]
}

/// The focused item's ring as the grid's own rules: `.grid:focus
/// .item-focused { <ring> }`, one rule per declaration of the theme's ring
/// (its conditions kept: the dark twin, the theme block). Above the items'
/// inline style, so the ring wins over the item's resting border while the
/// grid has the focus, and is not there at all without it.
fn focus_ring_sheet(ring: &[CssPropertyWithConditions]) -> azul_css::css::Css {
    use azul_css::css::{
        rule_priority, Css, CssDeclaration, CssPath, CssPathPseudoSelector, CssPathSelector,
        CssRuleBlock,
    };
    let path = || CssPath {
        selectors: alloc::vec![
            CssPathSelector::Class(AzString::from_const_str(GRID_CLASS)),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Focus),
            CssPathSelector::Children,
            CssPathSelector::Class(AzString::from_const_str(ITEM_FOCUSED_CLASS)),
        ]
        .into(),
    };
    Css {
        rules: ring
            .iter()
            .map(|decl| CssRuleBlock {
                path: path(),
                declarations: alloc::vec![CssDeclaration::Static(decl.property.clone())].into(),
                conditions: decl.apply_if.clone(),
                priority: rule_priority::WIDGETS,
            })
            .collect::<Vec<_>>()
            .into(),
        ..Css::default()
    }
}

/// What the name field of the item being renamed carries.
struct RenameData {
    /// The grid's shared part (its latest view and the app's hook).
    shared: RefAny,
    /// The item renamed.
    index: usize,
    /// The edit was reported (Enter, Escape): the focus leaving the field
    /// afterwards reports nothing more.
    done: bool,
}

/// The field an item's name is edited in, in its label's place: the name
/// as its text, focused when it appears (`AfterMount`).
fn rename_field(shared: &RefAny, index: usize, name: AzString, theme: OptionUiTheme) -> Dom {
    use azul_core::{callbacks::CoreCallback, dom::ComponentEventFilter, refany::OptionRefAny};

    use crate::widgets::text_input::{
        TextInput, TextInputOnFocusLostCallbackType, TextInputOnVirtualKeyDownCallbackType,
    };

    let data = RefAny::new(RenameData {
        shared: shared.clone(),
        index,
        done: false,
    });
    let on_key: TextInputOnVirtualKeyDownCallbackType = on_rename_key;
    let on_blur: TextInputOnFocusLostCallbackType = on_rename_focus_lost;
    let mut input = TextInput::create()
        .with_text(name)
        .with_accessibility_name(AzString::from_const_str("Name"))
        .with_on_virtual_key_down(data.clone(), on_key)
        .with_on_focus_lost(data, on_blur);
    if let Some(theme) = theme.into_option() {
        input = input.with_theme(theme);
    }
    Dom::create_div()
        .with_class(AzString::from_const_str(RENAME_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(rename_base()))
        .with_callbacks(
            alloc::vec![CoreCallbackData {
                event: EventFilter::Component(ComponentEventFilter::AfterMount),
                callback: CoreCallback {
                    cb: on_rename_mounted as usize,
                    ctx: OptionRefAny::None,
                },
                refany: shared.clone(),
            }]
            .into(),
        )
        .with_child(input.dom())
}

/// The name field appeared: it takes the focus (the caret in the name).
extern "C" fn on_rename_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.set_focus(azul_core::callbacks::FocusTarget::Id(node));
    Update::DoNothing
}

/// Reports the end of an edit once: the event `make` builds from the
/// grid's latest state and the item renamed.
fn end_rename(
    data: &mut RefAny,
    info: CallbackInfo,
    make: impl FnOnce(&IconGrid, usize) -> IconGridEvent,
) -> Update {
    let claimed = match data.downcast_mut::<RenameData>() {
        Some(mut r) if !r.done => {
            r.done = true;
            Some((r.shared.clone(), r.index))
        }
        _ => None,
    };
    let Some((mut shared, index)) = claimed else {
        return Update::DoNothing;
    };
    let Some((g, _)) = shared_of(&mut shared) else {
        return Update::DoNothing;
    };
    let event = make(&g, index);
    deliver(&mut shared, &g, info, event)
}

/// Enter takes the name, Escape gives the edit up; every other key is the
/// field's own.
extern "C" fn on_rename_key(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::text_input::TextInputState,
) -> crate::widgets::text_input::OnTextInputReturn {
    use crate::widgets::text_input::{OnTextInputReturn, TextInputValid};
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let update = match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            let text = state.get_text();
            end_rename(&mut data, info, |g, index| renamed_event(g, index, text.as_str()))
        }
        Some(VirtualKeyCode::Escape) => end_rename(&mut data, info, rename_cancelled),
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The field lost the focus (a click elsewhere, Tab): the name is taken,
/// as Explorer takes it.
extern "C" fn on_rename_focus_lost(
    mut data: RefAny,
    info: CallbackInfo,
    state: crate::widgets::text_input::TextInputState,
) -> Update {
    let text = state.get_text();
    end_rename(&mut data, info, |g, index| renamed_event(g, index, text.as_str()))
}

// ==== The callbacks: one set on the grid node, the grid hit-tests itself ====

/// The grid node's handlers.
fn grid_callbacks(shared: &RefAny) -> Vec<CoreCallbackData> {
    let on = |event: EventFilter, cb: usize| CoreCallbackData::create(event, shared.clone(), cb);
    alloc::vec![
        on(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), on_grid_key as usize),
        on(EventFilter::Hover(HoverEventFilter::LeftMouseDown), on_grid_mouse_down as usize),
        on(EventFilter::Hover(HoverEventFilter::MouseMove), on_grid_mouse_move as usize),
        on(EventFilter::Hover(HoverEventFilter::MouseUp), on_grid_mouse_up as usize),
        on(EventFilter::Hover(HoverEventFilter::DoubleClick), on_grid_double_click as usize),
        on(EventFilter::Hover(HoverEventFilter::RightMouseUp), on_grid_right_up as usize),
        on(EventFilter::Hover(HoverEventFilter::Scroll), on_grid_wheel as usize),
    ]
}

/// A copy of the grid and its geometry from a handler's payload.
fn shared_of(data: &mut RefAny) -> Option<(IconGrid, Geometry)> {
    let s = data.downcast_ref::<GridShared>()?;
    Some((s.grid.clone(), s.geo.clone()))
}

/// Records `view` as the grid's view in the payload (and its geometry), so
/// the next event before the rebuild starts from it.
fn store_view(data: &mut RefAny, view: &IconGridView) {
    if let Some(mut s) = data.downcast_mut::<GridShared>() {
        s.grid.view = view.clone();
        let geo = geometry(&s.grid);
        s.geo = geo;
    }
}

/// Stores the event's view, then hands the event to the app.
fn deliver(data: &mut RefAny, g: &IconGrid, info: CallbackInfo, event: IconGridEvent) -> Update {
    store_view(data, &event.view);
    match g.on_event.as_ref() {
        Some(IconGridOnEvent { refany, callback }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}

/// The keys (see the module's KEYBOARD).
extern "C" fn on_grid_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((g, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    // While a name is edited the keys are the field's (they reach the grid
    // on their way up).
    if ks.alt_down() || g.view.renaming.is_some() {
        return Update::DoNothing;
    }
    let Some(event) = grid_key(&g, &geo, key, ks.shift_down(), ks.primary_down()) else {
        return Update::DoNothing;
    };
    // The key is the grid's: no spatial navigation, no scrolling.
    info.prevent_default();
    deliver(&mut data, &g, info, event)
}

/// A press: select, start a rubber band, page or grab the thumb.
extern "C" fn on_grid_mouse_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((g, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = crate::widgets::cell_grid::cursor_in(&info) else {
        return Update::DoNothing;
    };
    let ks = info.get_current_keyboard_state();
    let window_y = info.get_cursor_position().map_or(y, |p| p.y);
    let hit = hit_test(&geo, x, y);
    let Some(event) = press(&g, &geo, hit, x, y, window_y, ks.shift_down(), ks.primary_down()) else {
        return Update::DoNothing;
    };
    if matches!(event.view.drag.kind, IconGridDragKind::Marquee | IconGridDragKind::Thumb) {
        let node = info.get_hit_node();
        info.capture_pointer(node);
    }
    deliver(&mut data, &g, info, event)
}

/// A move while the rubber band or the thumb is held.
extern "C" fn on_grid_mouse_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((g, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    if !matches!(g.view.drag.kind, IconGridDragKind::Marquee | IconGridDragKind::Thumb) {
        return Update::DoNothing;
    }
    let Some((x, y)) = crate::widgets::cell_grid::cursor_in(&info) else {
        return Update::DoNothing;
    };
    let window_y = info.get_cursor_position().map_or(y, |p| p.y);
    let Some(event) = drag_move(&g, &geo, x, y, window_y) else {
        return Update::DoNothing;
    };
    deliver(&mut data, &g, info, event)
}

/// The release ends a drag (a pending press selects its item).
extern "C" fn on_grid_mouse_up(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((g, _)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(event) = drag_end(&g) else {
        return Update::DoNothing;
    };
    deliver(&mut data, &g, info, event)
}

/// A double-click on an item activates it.
extern "C" fn on_grid_double_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((g, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = crate::widgets::cell_grid::cursor_in(&info) else {
        return Update::DoNothing;
    };
    let Hit::Item(index) = hit_test(&geo, x, y) else {
        return Update::DoNothing;
    };
    let mut next = g.view.clone();
    next.drag = IconGridDrag::default();
    let mut event = IconGridEvent::create(IconGridEventKind::Activate, next);
    event.index = OptionUsize::Some(index);
    deliver(&mut data, &g, info, event)
}

/// A right click: the item under the pointer joins the selection if it is
/// not in it (empty space clears it), then the app shows its menu.
extern "C" fn on_grid_right_up(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((g, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = crate::widgets::cell_grid::cursor_in(&info) else {
        return Update::DoNothing;
    };
    let mut next = g.view.clone();
    next.drag = IconGridDrag::default();
    let index = match hit_test(&geo, x, y) {
        Hit::Item(index) => {
            if !next.selection.contains(index as u64) {
                next.selection.click(index as u64);
            }
            Some(index)
        }
        Hit::Empty => {
            next.selection.clear();
            None
        }
        Hit::Track(_) | Hit::Thumb | Hit::Nothing => return Update::DoNothing,
    };
    let mut event = IconGridEvent::create(IconGridEventKind::ContextMenu, next);
    event.index = index.into();
    event.x = x;
    event.y = y;
    deliver(&mut data, &g, info, event)
}

/// The wheel scrolls by whole rows. The grid IS the scroll surface, so the
/// page under it does not scroll as well.
extern "C" fn on_grid_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((g, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    // The offset change the wheel asks for (+y = down: rows forward), not
    // the raw delta (+y = the wheel turned up), which scrolled backwards.
    let Some(delta) = info.get_wheel_scroll_by() else {
        return Update::DoNothing;
    };
    // THE WHEEL HAS ONE CONSUMER (see the cell grid).
    info.prevent_default();
    info.stop_propagation();
    let (rows, _) = crate::widgets::cell_grid::take_wheel(delta.x, delta.y, geo.cell_height, geo.cell_width);
    if rows == 0 {
        return Update::DoNothing;
    }
    let next = scroll_by(&g, &geo, rows);
    if next.top_row == geo.top {
        return Update::DoNothing;
    }
    deliver(&mut data, &g, info, IconGridEvent::create(IconGridEventKind::Scroll, next))
}

/// A drag leaves an item: the selection goes (or the item alone, when it
/// is not selected) - its indices on the drag, the app told.
extern "C" fn on_item_drag_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((index, mut shared)) = data
        .downcast_ref::<ItemData>()
        .map(|d| (d.index, d.shared.clone()))
    else {
        return Update::DoNothing;
    };
    let Some((g, _)) = shared_of(&mut shared) else {
        return Update::DoNothing;
    };
    let mut next = g.view.clone();
    next.drag = IconGridDrag::default();
    if !next.selection.contains(index as u64) {
        next.selection.click(index as u64);
    }
    let indices: Vec<String> = next
        .selection
        .keys
        .as_slice()
        .iter()
        .map(|k| alloc::format!("{k}"))
        .collect();
    info.set_drag_data(
        AzString::from_const_str(ICON_GRID_DRAG_MIME),
        indices.join(",").into_bytes(),
    );
    let mut event = IconGridEvent::create(IconGridEventKind::DragStart, next);
    event.index = OptionUsize::Some(index);
    deliver(&mut shared, &g, info, event)
}

// ==== fixtures (the widget manifest's sample) ====

/// The grid the widget manifest builds (`widgets::label_convention`): 40
/// files in a 400 x 300 viewport, two selected, one with a badge.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    extern "C" fn files(_data: RefAny, index: usize) -> IconGridItem {
        let item = IconGridItem::create(
            AzString::from(alloc::format!("Photo {index}.jpg")),
            AzString::from_const_str("image"),
        );
        if index == 2 {
            item.with_badge(AzString::from_const_str("cloud_done"))
        } else {
            item
        }
    }

    /// The sample grid.
    pub(crate) fn sample() -> IconGrid {
        let mut view = IconGridView::create();
        view.selection.select_keys(U64Vec::from_vec(alloc::vec![1, 2]));
        IconGrid::create(40, 400.0, 300.0)
            .with_view(view)
            .with_data_source(RefAny::new(()), files as IconGridDataSourceCallbackType)
            .with_accessibility_name(AzString::from_const_str("Pictures"))
    }
}

#[cfg(test)]
mod icon_grid_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId},
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

    type Asked = Arc<Mutex<Vec<usize>>>;
    type Log = Arc<Mutex<Vec<String>>>;

    extern "C" fn items(mut data: RefAny, index: usize) -> IconGridItem {
        if let Some(asked) = data.downcast_ref::<Asked>() {
            asked.lock().expect("asked").push(index);
        }
        IconGridItem::create(AzString::from(format!("File {index}")), AzString::from("description"))
    }

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, e: IconGridEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(format!(
                "{:?} {:?} {:?}",
                e.kind,
                e.index.into_option(),
                e.view.selection.keys.as_slice()
            ));
        }
        Update::RefreshDom
    }

    /// 100 items in a 400 x 300 viewport of 96 x 104 cells: 4 columns (the
    /// scroll bar takes 12 px), 25 rows, 2 rows wholly in view, 3 shown.
    fn grid(asked: &Asked, log: &Log) -> IconGrid {
        IconGrid::create(100, 400.0, 300.0)
            .with_data_source(RefAny::new(asked.clone()), items as IconGridDataSourceCallbackType)
            .with_on_event(RefAny::new(log.clone()), record as IconGridOnEventCallbackType)
            .with_accessibility_name(AzString::from("Pictures"))
    }

    fn fresh() -> (Asked, Log) {
        (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())))
    }

    fn selected(view: &IconGridView) -> Vec<u64> {
        view.selection.keys.as_slice().to_vec()
    }

    fn with_selection(g: IconGrid, keys: &[u64], focus: u64) -> IconGrid {
        let mut view = g.view.clone();
        view.selection.select_keys(U64Vec::from_vec(keys.to_vec()));
        view.selection.anchor = OptionU64::Some(focus);
        view.selection.focus = OptionU64::Some(focus);
        g.with_view(view)
    }

    // ---- geometry ----

    #[test]
    fn the_cells_fill_whole_columns_and_the_scroll_bar_takes_its_strip() {
        let (asked, log) = fresh();
        let geo = geometry(&grid(&asked, &log));
        assert_eq!(geo.columns, 4);
        assert_eq!(geo.rows, 25);
        assert_eq!(geo.page_rows, 2);
        assert_eq!(geo.max_top, 23);
        assert_eq!((geo.first, geo.end), (0, 12), "three rows shown, the last one in part");
        assert!((geo.body_width - 388.0).abs() < 0.01);
        let bar = geo.vbar.expect("the rows overflow");
        assert_eq!(bar.track, (388.0, 0.0, 12.0, 300.0));
        let scrolled = geometry(&grid(&asked, &log).with_view(IconGridView::create().with_top_row(5)));
        assert_eq!((scrolled.top, scrolled.first, scrolled.end), (5, 20, 32));
        let past = geometry(&grid(&asked, &log).with_view(IconGridView::create().with_top_row(99)));
        assert_eq!(past.top, 23, "kept in range");
        let few = geometry(&IconGrid::create(5, 400.0, 300.0));
        assert!(few.vbar.is_none(), "every row fits");
        assert_eq!((few.columns, few.rows, few.end), (4, 2, 5));
    }

    #[test]
    fn a_point_is_over_an_item_empty_space_or_the_scroll_bar() {
        let (asked, log) = fresh();
        let geo = geometry(&grid(&asked, &log));
        assert_eq!(hit_test(&geo, 10.0, 10.0), Hit::Item(0));
        assert_eq!(hit_test(&geo, 100.0, 10.0), Hit::Item(1));
        assert_eq!(hit_test(&geo, 10.0, 250.0), Hit::Item(8));
        assert_eq!(hit_test(&geo, 386.0, 10.0), Hit::Empty, "right of the last column");
        assert_eq!(hit_test(&geo, 392.0, 10.0), Hit::Thumb);
        assert_eq!(hit_test(&geo, 392.0, 200.0), Hit::Track(true));
        assert_eq!(hit_test(&geo, -1.0, 10.0), Hit::Nothing);
        let few = geometry(&IconGrid::create(5, 400.0, 300.0));
        assert_eq!(hit_test(&few, 150.0, 120.0), Hit::Empty, "after the last item");
        assert_eq!(item_rect(&geo, 5), Some((96.0, 104.0, 96.0, 104.0)));
        assert_eq!(item_rect(&geo, 40), None, "not in view");
    }

    #[test]
    fn a_rubber_band_selects_the_cells_it_crosses() {
        let (asked, log) = fresh();
        let geo = geometry(&grid(&asked, &log));
        assert_eq!(band_keys(&geo, 150.0, 120.0, 10.0, 10.0), vec![0, 1, 4, 5]);
        assert_eq!(band_keys(&geo, 386.0, 10.0, 387.0, 20.0), Vec::<u64>::new(), "past the columns");
    }

    // ---- the rubber band past an edge, renaming, names, the focus ring ----

    /// Explorer: a rubber band dragged past the grid's bottom edge scrolls
    /// the rows up under it, and the band stays anchored to the item it
    /// started on; past the top edge it scrolls back.
    #[test]
    fn a_rubber_band_past_an_edge_scrolls_the_grid_and_keeps_its_anchor() {
        let (asked, log) = fresh();
        let mut g = grid(&asked, &log);
        let geo = geometry(&g);
        let start = press(&g, &geo, Hit::Empty, 386.0, 10.0, 10.0, false, false).expect("a band");
        g = g.with_view(start.view);
        // 50 px under the bottom edge: one row further down.
        let e = drag_move(&g, &geometry(&g), 10.0, geo.height + 50.0, 0.0).expect("the band moves");
        assert_eq!(e.view.top_row, 1, "the grid scrolled a row");
        let keys = selected(&e.view);
        assert!(keys.contains(&3), "the band still holds its first row: {keys:?}");
        assert!(keys.contains(&16), "and reaches the row under the pointer (row 4): {keys:?}");
        g = g.with_view(e.view);
        // 30 px over the top edge: back up a row, the anchor kept.
        let e = drag_move(&g, &geometry(&g), 10.0, -30.0, 0.0).expect("the band moves");
        assert_eq!(e.view.top_row, 0, "scrolled back");
        assert!(selected(&e.view).contains(&3), "{:?}", selected(&e.view));
    }

    /// F2 edits the focused item's name in place; Enter (or leaving the
    /// field) reports the new name, and the edit ends.
    #[test]
    fn f2_renames_the_focused_item_in_place() {
        let (asked, log) = fresh();
        let g = with_selection(grid(&asked, &log), &[5], 5);
        let geo = geometry(&g);
        let e = grid_key(&g, &geo, K::F2, false, false).expect("F2 is the grid's");
        assert_eq!(e.kind, IconGridEventKind::Rename);
        assert_eq!(e.view.renaming.into_option(), Some(5));
        assert_eq!(e.index.into_option(), Some(5));

        let renaming = g.clone().with_view(e.view.clone());
        let dom = renaming.clone().with_theme(UiTheme::Flat).dom();
        let item = dom
            .children
            .as_ref()
            .iter()
            .find(|c| theme_checks::find(c, RENAME_CLASS).is_some())
            .expect("the item being renamed holds the name field");
        assert!(theme_checks::find(item, LABEL_CLASS).is_none(), "the field takes the label's place");

        let done = rename_commit(&renaming, "Holiday.jpg").expect("an edit to end");
        assert_eq!(done.kind, IconGridEventKind::Renamed);
        assert_eq!(done.index.into_option(), Some(5));
        assert_eq!(done.text.as_str(), "Holiday.jpg");
        assert_eq!(done.view.renaming.into_option(), None);
        assert!(rename_commit(&g, "x").is_none(), "nothing to commit when nothing is renamed");
    }

    /// A name wraps to two lines and ends in an ellipsis where it goes on
    /// (the design's `-webkit-line-clamp: 2`); a long word breaks.
    #[test]
    fn an_items_name_wraps_to_two_lines_and_ends_in_an_ellipsis() {
        use azul_css::props::style::{StyleLineClamp, StyleOverflowWrap, StyleWhiteSpace};
        let name: Vec<CssProperty> = name_base().into_iter().map(|p| p.property).collect();
        assert!(name.contains(&CssProperty::from(StyleLineClamp { max_lines: 2 })), "{name:?}");
        assert!(name.contains(&CssProperty::from(StyleOverflowWrap::Anywhere)), "{name:?}");
        assert!(
            !name.iter().any(|p| matches!(p, CssProperty::WhiteSpace(v) if v.get_property() == Some(&StyleWhiteSpace::Nowrap))),
            "a name wraps"
        );
    }

    /// The focused item's ring shows only while the grid has the focus: it
    /// is the grid's rule for `:focus` on itself, not the item's own style.
    #[test]
    fn the_focused_items_ring_shows_only_while_the_grid_has_the_focus() {
        use azul_css::css::{CssPathPseudoSelector, CssPathSelector};
        let (asked, log) = fresh();
        let g = with_selection(grid(&asked, &log), &[1], 1).with_theme(UiTheme::Flat);
        let look = look_for(OptionUiTheme::Some(UiTheme::Flat));
        let dom = g.dom();
        let focused = dom
            .children
            .as_ref()
            .iter()
            .find(|c| theme_checks::has_class(c, ITEM_FOCUSED_CLASS))
            .expect("the focused item");
        let own: Vec<CssProperty> =
            focused.root.style.iter_inline_properties().map(|(p, _)| p.clone()).collect();
        for ring in &look.item_focused {
            assert!(!own.contains(&ring.property), "the ring is not the item's own: {:?}", ring.property);
        }
        let want = [
            CssPathSelector::Class(AzString::from_const_str(GRID_CLASS)),
            CssPathSelector::PseudoSelector(CssPathPseudoSelector::Focus),
            CssPathSelector::Children,
            CssPathSelector::Class(AzString::from_const_str(ITEM_FOCUSED_CLASS)),
        ];
        let rules: Vec<_> = dom
            .css
            .as_ref()
            .iter()
            .flat_map(|sheet| sheet.rules.as_ref().iter())
            .filter(|rule| rule.path.selectors.as_ref().ends_with(&want))
            .collect();
        assert_eq!(rules.len(), look.item_focused.len(), "one rule per ring declaration");
    }

    // ---- groups ----

    /// 18 items in three groups (5, 3, 10) in a 400 x 600 grid of 4
    /// columns: each group starts a row under its 28 px header.
    fn grouped(asked: &Asked, log: &Log) -> IconGrid {
        IconGrid::create(18, 400.0, 600.0)
            .with_data_source(RefAny::new(asked.clone()), items as IconGridDataSourceCallbackType)
            .with_on_event(RefAny::new(log.clone()), record as IconGridOnEventCallbackType)
            .with_groups(IconGridGroupVec::from_vec(vec![
                IconGridGroup::create(0, AzString::from("Today")),
                IconGridGroup::create(5, AzString::from("Yesterday")),
                IconGridGroup::create(8, AzString::from("Older")),
            ]))
    }

    /// Each group starts a new row under its header: Today's five items
    /// take a row and one more, Yesterday's header comes next.
    #[test]
    fn groups_start_their_rows_under_their_headers() {
        let (asked, log) = fresh();
        let geo = geometry(&grouped(&asked, &log));
        assert_eq!(geo.columns, 4);
        // Today: header, [0..4], [4]; Yesterday: header, [5..8]; Older:
        // header, [8..12], [12..16], [16..18].
        assert_eq!(item_rect(&geo, 0), Some((0.0, 28.0, 96.0, 104.0)));
        assert_eq!(item_rect(&geo, 4), Some((0.0, 132.0, 96.0, 104.0)));
        assert_eq!(item_rect(&geo, 7), Some((192.0, 264.0, 96.0, 104.0)));
        assert_eq!(item_rect(&geo, 8), Some((0.0, 396.0, 96.0, 104.0)));
        assert_eq!(hit_test(&geo, 10.0, 10.0), Hit::Empty, "a header is no item");
        assert_eq!(hit_test(&geo, 10.0, 30.0), Hit::Item(0));
        assert_eq!(hit_test(&geo, 200.0, 270.0), Hit::Item(7));
        assert_eq!(hit_test(&geo, 300.0, 140.0), Hit::Empty, "past Today's last item");
    }

    /// Up and Down move to the row above or below in the same column,
    /// across a header, onto the row's last item when it is shorter.
    #[test]
    fn up_and_down_cross_the_headers_in_their_column() {
        let (asked, log) = fresh();
        let down = |focus: u64| {
            let g = with_selection(grouped(&asked, &log), &[focus], focus);
            let e = grid_key(&g, &geometry(&g), K::Down, false, false).expect("a move");
            e.index.into_option()
        };
        let up = |focus: u64| {
            let g = with_selection(grouped(&asked, &log), &[focus], focus);
            let e = grid_key(&g, &geometry(&g), K::Up, false, false).expect("a move");
            e.index.into_option()
        };
        assert_eq!(down(2), Some(4), "onto the short row's only item");
        assert_eq!(down(4), Some(5), "across Yesterday's header");
        assert_eq!(down(6), Some(9), "across Older's header, same column");
        assert_eq!(up(9), Some(6));
        assert_eq!(up(5), Some(4));
    }

    /// The headers are drawn: one per group, saying its label.
    #[test]
    fn each_group_shows_its_header() {
        let (asked, log) = fresh();
        let dom = grouped(&asked, &log).with_theme(UiTheme::Flat).dom();
        let headers: Vec<String> = dom
            .children
            .as_ref()
            .iter()
            .filter(|c| theme_checks::has_class(c, GROUP_HEADER_CLASS))
            .filter_map(text_of)
            .collect();
        assert_eq!(headers, vec!["Today", "Yesterday", "Older"]);
    }

    // ---- the pointer ----

    #[test]
    fn a_click_selects_ctrl_toggles_shift_extends() {
        let (asked, log) = fresh();
        let g = grid(&asked, &log);
        let geo = geometry(&g);
        let e = press(&g, &geo, Hit::Item(5), 0.0, 0.0, 0.0, false, false).expect("a select");
        assert_eq!(e.kind, IconGridEventKind::Select);
        assert_eq!(selected(&e.view), vec![5]);
        let g = g.with_view(e.view);
        let e = press(&g, &geo, Hit::Item(7), 0.0, 0.0, 0.0, false, true).expect("a toggle");
        assert_eq!(selected(&e.view), vec![5, 7]);
        let e = press(&g, &geo, Hit::Item(9), 0.0, 0.0, 0.0, true, false).expect("an extend");
        assert_eq!(selected(&e.view), vec![5, 6, 7, 8, 9]);
    }

    #[test]
    fn a_press_on_a_selected_item_keeps_the_selection_until_the_release() {
        let (asked, log) = fresh();
        let g = with_selection(grid(&asked, &log), &[5, 7], 7);
        let geo = geometry(&g);
        let e = press(&g, &geo, Hit::Item(5), 10.0, 10.0, 10.0, false, false).expect("a press");
        assert_eq!(selected(&e.view), vec![5, 7], "the selection can be dragged out");
        assert_eq!(e.view.drag.kind, IconGridDragKind::Pending);
        let g = g.with_view(e.view);
        let e = drag_end(&g).expect("the release");
        assert_eq!(e.kind, IconGridEventKind::Select);
        assert_eq!(selected(&e.view), vec![5]);
        assert_eq!(e.view.drag.kind, IconGridDragKind::None);
    }

    #[test]
    fn a_press_on_empty_space_clears_and_starts_a_rubber_band_that_selects_on_the_move() {
        let (asked, log) = fresh();
        let g = with_selection(grid(&asked, &log), &[20], 20);
        let geo = geometry(&g);
        let e = press(&g, &geo, Hit::Empty, 10.0, 10.0, 10.0, false, false).expect("a press");
        assert!(selected(&e.view).is_empty());
        assert_eq!(e.view.drag.kind, IconGridDragKind::Marquee);
        let moving = g.clone().with_view(e.view);
        let e = drag_move(&moving, &geo, 150.0, 120.0, 120.0).expect("a move");
        assert_eq!(selected(&e.view), vec![0, 1, 4, 5]);
        let done = drag_end(&moving.with_view(e.view)).expect("the release");
        assert_eq!(done.view.drag.kind, IconGridDragKind::None);
        assert_eq!(selected(&done.view), vec![0, 1, 4, 5]);
        // Ctrl adds the band to what was selected.
        let e = press(&g, &geo, Hit::Empty, 10.0, 10.0, 10.0, false, true).expect("a press");
        assert_eq!(selected(&e.view), vec![20]);
        let e = drag_move(&g.clone().with_view(e.view), &geo, 150.0, 120.0, 120.0).expect("a move");
        assert_eq!(selected(&e.view), vec![0, 1, 4, 5, 20]);
    }

    #[test]
    fn the_scroll_bar_pages_and_its_thumb_drags_the_rows() {
        let (asked, log) = fresh();
        let g = grid(&asked, &log);
        let geo = geometry(&g);
        let e = press(&g, &geo, Hit::Track(true), 392.0, 200.0, 200.0, false, false).expect("a page");
        assert_eq!((e.kind, e.view.top_row), (IconGridEventKind::Scroll, 2));
        let e = press(&g, &geo, Hit::Thumb, 392.0, 10.0, 10.0, false, false).expect("a grab");
        assert_eq!(e.view.drag.kind, IconGridDragKind::Thumb);
        let held = g.with_view(e.view);
        // 276 px of travel for 23 rows: 138 px is half way.
        let e = drag_move(&held, &geo, 392.0, 148.0, 148.0).expect("a move");
        assert_eq!((e.kind, e.view.top_row), (IconGridEventKind::Scroll, 12));
        assert_eq!(drag_end(&held).map(|e| e.kind), Some(IconGridEventKind::Scroll));
    }

    #[test]
    fn the_wheel_and_the_scroll_stay_in_range() {
        let (asked, log) = fresh();
        let g = grid(&asked, &log);
        let geo = geometry(&g);
        assert_eq!(scroll_by(&g, &geo, 3).top_row, 3);
        assert_eq!(scroll_by(&g, &geo, -3).top_row, 0);
        assert_eq!(scroll_by(&g, &geo, 100).top_row, 23);
    }

    // ---- the keys ----

    #[test]
    fn the_arrows_move_by_an_item_or_a_row_and_shift_extends() {
        let (asked, log) = fresh();
        let g = grid(&asked, &log);
        let geo = geometry(&g);
        let key = |g: &IconGrid, k: K, shift: bool, ctrl: bool| grid_key(g, &geo, k, shift, ctrl).expect("a key");
        assert_eq!(selected(&key(&g, K::Right, false, false).view), vec![0], "nothing focused: the first");
        let at5 = with_selection(g.clone(), &[5], 5);
        assert_eq!(selected(&key(&at5, K::Down, false, false).view), vec![9]);
        assert_eq!(selected(&key(&at5, K::Down, true, false).view), vec![5, 6, 7, 8, 9]);
        let moved = key(&at5, K::Down, false, true).view;
        assert_eq!(selected(&moved), vec![5], "Ctrl moves the focus alone");
        assert_eq!(moved.selection.focus.into_option(), Some(9));
        assert_eq!(selected(&key(&at5, K::Left, false, false).view), vec![4]);
        let at1 = with_selection(g.clone(), &[1], 1);
        assert_eq!(selected(&key(&at1, K::Up, false, false).view), vec![0], "held at the top");
        assert_eq!(selected(&key(&at1, K::PageDown, false, false).view), vec![9]);
        let end = key(&at1, K::End, false, false).view;
        assert_eq!(selected(&end), vec![99]);
        assert_eq!(end.top_row, 23, "the last row is revealed");
        assert_eq!(selected(&key(&at5.clone().with_view(end), K::Home, false, false).view), vec![0]);
    }

    #[test]
    fn ctrl_a_selects_all_enter_activates_the_menu_key_asks_for_a_menu_escape_clears() {
        let (asked, log) = fresh();
        let at5 = with_selection(grid(&asked, &log), &[5], 5);
        let geo = geometry(&at5);
        let all = grid_key(&at5, &geo, K::A, false, true).expect("select all");
        assert_eq!(all.view.selection.keys.len(), 100);
        let open = grid_key(&at5, &geo, K::Return, false, false).expect("activate");
        assert_eq!((open.kind, open.index.into_option()), (IconGridEventKind::Activate, Some(5)));
        let menu = grid_key(&at5, &geo, K::Apps, false, false).expect("a menu");
        assert_eq!((menu.kind, menu.index.into_option()), (IconGridEventKind::ContextMenu, Some(5)));
        let menu = grid_key(&at5, &geo, K::F10, true, false).expect("a menu");
        assert_eq!(menu.kind, IconGridEventKind::ContextMenu);
        let cleared = grid_key(&at5, &geo, K::Escape, false, false).expect("a clear");
        assert!(selected(&cleared.view).is_empty());
        let none = with_selection(grid(&asked, &log), &[], 5);
        assert!(grid_key(&none, &geo, K::Escape, false, false).is_none(), "nothing to clear");
    }

    extern "C" fn fruits(_: RefAny, index: usize) -> IconGridItem {
        const NAMES: [&str; 6] = ["Apple", "banana", "Cherry", "avocado", "Blueberry", "apricot"];
        IconGridItem::create(
            AzString::from(NAMES.get(index).copied().unwrap_or("")),
            AzString::from("description"),
        )
    }

    #[test]
    fn typing_a_letter_moves_the_focus_to_the_next_item_named_with_it() {
        let (_, log) = fresh();
        let g = IconGrid::create(6, 400.0, 300.0)
            .with_data_source(RefAny::new(()), fruits as IconGridDataSourceCallbackType)
            .with_on_event(RefAny::new(log.clone()), record as IconGridOnEventCallbackType);
        let geo = geometry(&g);
        let typed = |g: &IconGrid, k: K| grid_key(g, &geo, k, false, false).expect("a letter is the grid's");
        let first = typed(&g, K::A);
        assert_eq!(
            (first.kind, first.index.into_option()),
            (IconGridEventKind::Select, Some(0)),
            "nothing focused: the first item named with A"
        );
        assert_eq!(selected(&first.view), vec![0]);
        let next = typed(&g.clone().with_view(first.view), K::A);
        assert_eq!(selected(&next.view), vec![3], "the NEXT one named with it, case folded");
        assert_eq!(next.view.selection.focus.into_option(), Some(3), "the focus moves with it");
        assert_eq!(selected(&typed(&g.clone().with_view(next.view), K::A).view), vec![5]);
        let at5 = with_selection(g.clone(), &[5], 5);
        assert_eq!(selected(&typed(&at5, K::A).view), vec![0], "past the last: around to the first");
        assert_eq!(selected(&typed(&at5, K::B).view), vec![1]);
        assert!(grid_key(&at5, &geo, K::Z, false, false).is_none(), "no item named with Z: nothing moves");
        assert_eq!(
            grid_key(&at5, &geo, K::A, false, true).map(|e| e.view.selection.keys.len()),
            Some(6),
            "Ctrl+A still selects all"
        );
    }

    // ---- the DOM ----

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    #[test]
    fn only_the_items_in_view_are_asked_for_and_built() {
        let (asked, log) = fresh();
        let dom = with_selection(grid(&asked, &log), &[1, 3], 1)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(*asked.lock().expect("asked"), (0..12).collect::<Vec<_>>());
        assert!(theme_checks::has_class(&dom, GRID_CLASS));
        let info = dom.root.get_accessibility_info().cloned().unwrap_or_default();
        assert_eq!(info.role, AccessibilityRole::List);
        assert!(info.states.as_ref().contains(&AccessibilityState::Multiselectable));
        assert_eq!(dom.root.get_tab_index(), Some(TabIndex::Auto), "one Tab stop");
        let items: Vec<&Dom> = dom
            .children
            .as_ref()
            .iter()
            .filter(|c| theme_checks::has_class(c, ITEM_CLASS))
            .collect();
        assert_eq!(items.len(), 12);
        assert!(theme_checks::has_class(items[1], ITEM_SELECTED_CLASS));
        assert!(theme_checks::has_class(items[1], ITEM_FOCUSED_CLASS));
        assert!(theme_checks::has_class(items[3], ITEM_SELECTED_CLASS));
        assert!(!theme_checks::has_class(items[3], ITEM_FOCUSED_CLASS));
        assert!(!theme_checks::has_class(items[0], ITEM_SELECTED_CLASS));
        let item = items[1].root.get_accessibility_info().cloned().unwrap_or_default();
        assert_eq!(item.role, AccessibilityRole::ListItem);
        assert_eq!(item.accessibility_name.as_ref().map(|n| n.as_str().to_string()), Some(String::from("File 1")));
        assert!(item.states.as_ref().contains(&AccessibilityState::Selected));
        assert!(items[1].root.attributes().as_ref().contains(&AttributeType::Draggable(true)));
        assert!(theme_checks::find(items[1], THUMB_CLASS).is_some());
        assert!(theme_checks::find(items[1], LABEL_CLASS).is_some());
        assert!(theme_checks::find(&dom, TRACK_CLASS).is_some(), "the scroll bar");
        assert!(theme_checks::find(&dom, SCROLL_THUMB_CLASS).is_some());
    }

    /// User decision D3 (2026-10-05): an item's extras (text lines under
    /// the label, a placeholder tile) are OPTIONAL - an item that sets none
    /// of them is built exactly as before: [thumb [glyph, badge?], label],
    /// the thumb its bare base, the glyph in the look's icon ink.
    #[test]
    fn an_item_without_extras_renders_exactly_as_before() {
        use crate::widgets::themes::decl;
        for theme in checks::BOTH {
            let dom = fixtures::sample().with_theme(theme).dom();
            let look = look_for(OptionUiTheme::Some(theme));
            let bare_thumb = Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(thumb_base(48.0)));
            let glyph = Dom::create_icon(AzString::from("image")).with_css_props(part(
                &[decl::simple(CssProperty::const_font_size(StyleFontSize::const_px(48)))],
                &look.icon,
            ));
            let label = Dom::create_div().with_css_props(part(&name_base(), &look.label));
            let items: Vec<&Dom> =
                dom.children.as_ref().iter().filter(|c| theme_checks::has_class(c, ITEM_CLASS)).collect();
            assert!(!items.is_empty());
            for (index, item) in items.iter().enumerate() {
                let kids = item.children.as_ref();
                assert_eq!(kids.len(), 2, "{} item {index}: the thumb and the label, nothing else", theme.name());
                assert!(theme_checks::has_class(&kids[0], THUMB_CLASS));
                assert_eq!(
                    kids[0].root.get_style(),
                    bare_thumb.root.get_style(),
                    "{} item {index}: the bare thumb",
                    theme.name()
                );
                let in_thumb = kids[0].children.as_ref();
                assert_eq!(in_thumb.len(), if index == 2 { 2 } else { 1 }, "the glyph (and item 2's badge)");
                assert_eq!(
                    in_thumb[0].root.get_style(),
                    glyph.root.get_style(),
                    "{} item {index}: the glyph in the icon ink",
                    theme.name()
                );
                assert!(theme_checks::has_class(&kids[1], LABEL_CLASS));
                assert_eq!(
                    kids[1].root.get_style(),
                    label.root.get_style(),
                    "{} item {index}: the label",
                    theme.name()
                );
            }
        }
    }

    /// A shelf of 9 books: every third with two lines and a red tile, the
    /// next with a yellow tile AND a cover, the next plain.
    extern "C" fn books(_: RefAny, index: usize) -> IconGridItem {
        let item = IconGridItem::create(AzString::from(format!("Book {index}")), AzString::from("menu_book"));
        match index % 3 {
            0 => item
                .with_lines(StringVec::from_vec(vec![AzString::from("Jane Austen"), AzString::from("42 %")]))
                .with_placeholder(ColorU::new(160, 40, 40, 255)),
            1 => item.with_placeholder(ColorU::new(240, 220, 120, 255)).with_image(ImageRef::null_image(
                2,
                2,
                azul_core::resources::RawImageFormat::RGBA8,
                Vec::new(),
            )),
            _ => item,
        }
    }

    fn shelf() -> IconGrid {
        IconGrid::create(9, 400.0, 300.0)
            .with_data_source(RefAny::new(()), books as IconGridDataSourceCallbackType)
            .with_accessibility_name(AzString::from("Library"))
    }

    fn items_of(dom: &Dom) -> Vec<&Dom> {
        dom.children.as_ref().iter().filter(|c| theme_checks::has_class(c, ITEM_CLASS)).collect()
    }

    /// The text of a `p > text` node.
    fn text_of(node: &Dom) -> Option<String> {
        match node.children.as_ref() {
            [only] => match only.root.get_node_type() {
                azul_core::dom::NodeType::Text(s) => Some(s.as_str().to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    /// User decision D3 (2026-10-05): an item may carry extra lines under
    /// its label (AzReader's author and reading progress), in order, in the
    /// secondary ink, day and night; an item without lines shows none.
    #[test]
    fn an_item_shows_its_extra_lines_under_its_label_in_order() {
        for theme in checks::BOTH {
            let dom = shelf().with_theme(theme).dom();
            let items = items_of(&dom);
            let kids = items[0].children.as_ref();
            assert_eq!(kids.len(), 4, "{}: the thumb, the label, two lines", theme.name());
            let lines: Vec<Option<String>> = kids[2..].iter().map(text_of).collect();
            assert_eq!(
                lines,
                vec![Some(String::from("Jane Austen")), Some(String::from("42 %"))],
                "{}: the lines in order",
                theme.name()
            );
            for line in &kids[2..] {
                assert!(theme_checks::has_class(line, LINE_CLASS), "{}", theme.name());
                for dark in [false, true] {
                    let ink = theme_checks::text_color(line, dark);
                    assert!(ink.is_some(), "{} (dark: {dark}): a line has its ink", theme.name());
                    assert_ne!(
                        ink,
                        theme_checks::text_color(&kids[1], dark),
                        "{} (dark: {dark}): a line is in the secondary ink, not the label's",
                        theme.name()
                    );
                }
            }
            assert_eq!(items[2].children.as_ref().len(), 2, "{}: no lines, no line", theme.name());
        }
    }

    /// User decision D3 (2026-10-05): an item without a picture may show its
    /// glyph on a tile of its own colour (a book without a cover), the glyph
    /// in black or white, whichever reads, day and night; a thumbnail
    /// replaces the tile.
    #[test]
    fn an_item_without_a_picture_shows_its_glyph_on_its_placeholder_tile() {
        let red = ColorU::new(160, 40, 40, 255);
        for theme in checks::BOTH {
            let dom = shelf().with_theme(theme).dom();
            let items = items_of(&dom);
            let tile = &items[0].children.as_ref()[0];
            assert!(theme_checks::has_class(tile, THUMB_CLASS));
            assert!(theme_checks::has_class(tile, PLACEHOLDER_CLASS), "{}: a tile", theme.name());
            let glyph = &tile.children.as_ref()[0];
            for dark in [false, true] {
                assert_eq!(
                    theme_checks::background(tile, dark).as_ref().and_then(theme_checks::bg_color),
                    Some(red),
                    "{} (dark: {dark}): the tile in the item's colour",
                    theme.name()
                );
                assert_eq!(
                    theme_checks::text_color(glyph, dark),
                    Some(red.contrast_text()),
                    "{} (dark: {dark}): the glyph reads on the tile",
                    theme.name()
                );
            }
            let covered = &items[1].children.as_ref()[0];
            assert!(!theme_checks::has_class(covered, PLACEHOLDER_CLASS), "{}: a cover, no tile", theme.name());
            assert!(matches!(
                covered.children.as_ref()[0].root.get_node_type(),
                azul_core::dom::NodeType::Image(_)
            ));
            assert_eq!(theme_checks::background(covered, false), None, "{}", theme.name());
        }
    }

    /// The shelf's books without their covers: a cover is a fresh image per
    /// build (its own identity), and the theme checks compare two builds.
    extern "C" fn uncovered(data: RefAny, index: usize) -> IconGridItem {
        let mut item = books(data, index);
        item.image = OptionImageRef::None;
        item
    }

    #[test]
    fn a_shelf_with_extras_follows_the_app_theme_and_keeps_its_theme_invariants() {
        let shelf = || {
            IconGrid::create(9, 400.0, 300.0)
                .with_data_source(RefAny::new(()), uncovered as IconGridDataSourceCallbackType)
                .with_accessibility_name(AzString::from("Library"))
        };
        checks::assert_follows_the_app_theme(
            "icon_grid (extras)",
            || shelf().dom(),
            |t: UiTheme| shelf().with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || shelf().dom());
            theme_checks::assert_structure_is_shared(&format!("icon_grid (extras) built for {}", theme.name()), &dom, &[]);
            theme_checks::assert_theme_invariants(&format!("icon_grid (extras, {})", theme.name()), &dom);
        }
    }

    /// An app's E2E clicks an item by its grid's id and its index
    /// (`#__azreader_book-0`), and so does the app's own code that looks an
    /// item up: every item node carries `<grid id>-<index>` as its DOM id -
    /// the item's index, not its place in view. A grid with no id names no
    /// item (FIX9 APPSB R-3, the gap the Toolbar had).
    #[test]
    fn an_icon_grid_item_carries_its_grids_id_and_its_index_as_its_dom_id() {
        let (asked, log) = fresh();
        let items_of = |dom: &Dom| -> Vec<Dom> {
            dom.children
                .as_ref()
                .iter()
                .filter(|c| theme_checks::has_class(c, ITEM_CLASS))
                .cloned()
                .collect()
        };
        // Scrolled one row down: items 4..16 in view.
        let dom = grid(&asked, &log)
            .with_id(AzString::from("books"))
            .with_view(IconGridView::create().with_top_row(1))
            .with_theme(UiTheme::Flat)
            .dom();
        let items = items_of(&dom);
        assert_eq!(items.len(), 12);
        for (n, item) in items.iter().enumerate() {
            let want = format!("books-{}", n + 4);
            assert!(item.root.has_id(&want), "the item in place {n} carries #{want}");
        }
        assert!(dom.root.has_id("books"), "the grid keeps its own id");

        let bare = grid(&asked, &log)
            .with_id(AzString::from_const_str(""))
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(
            items_of(&bare).iter().all(|item| !item
                .root
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, azul_core::dom::IdOrClass::Id(_)))),
            "a grid with no id names no item"
        );
    }

    #[test]
    fn a_drag_out_carries_the_selected_indices_and_tells_the_app() {
        let (asked, log) = fresh();
        let styled = StyledDom::create_from_dom(
            with_selection(grid(&asked, &log), &[1, 3], 1)
                .with_theme(UiTheme::Flat)
                .dom(),
        );
        let hierarchy = styled.node_hierarchy.as_ref();
        let mut items = Vec::new();
        let mut next = hierarchy[0].first_child_id(NodeId::new(0));
        while let Some(n) = next {
            items.push(n);
            next = hierarchy[n.index()].next_sibling_id();
        }
        let (_, changes) = rv::fire(&styled, id(items[1]), EventFilter::Hover(HoverEventFilter::DragStart))
            .expect("a drag source");
        assert!(changes.iter().any(|c| matches!(
            c,
            CallbackChange::SetDragData { mime_type, data }
                if mime_type.as_str() == ICON_GRID_DRAG_MIME && data.as_slice() == b"1,3"
        )));
        let logged = log.lock().expect("log").clone();
        assert_eq!(logged, vec![String::from("DragStart Some(1) [1, 3]")]);
    }

    #[test]
    fn a_grid_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        let (asked, log) = fresh();
        let sample = || with_selection(grid(&asked, &log), &[1, 3], 1);
        checks::assert_follows_the_app_theme("icon_grid", || sample().dom(), |t: UiTheme| sample().with_theme(t).dom());
        for theme in checks::BOTH {
            let dom = checks::under(theme, || sample().dom());
            theme_checks::assert_structure_is_shared(&format!("icon_grid built for {}", theme.name()), &dom, &[]);
            theme_checks::assert_theme_invariants(&format!("icon_grid ({})", theme.name()), &dom);
        }
    }
}
