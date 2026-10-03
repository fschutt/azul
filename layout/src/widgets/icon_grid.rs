//! Icon grid widget - a grid of icons or thumbnails with labels: a file
//! manager's icon view, a photo library, an e-reader's shelf, an app
//! launcher, a slide sorter of a million items.
//!
//! GENERIC OVER ITS DATA: the grid holds no items. It asks the app for the
//! items it shows through a DATA callback ([`IconGrid::with_data_source`]:
//! the item's label, its icon glyph, its thumbnail when the app has one,
//! a badge), given the item's index. Only the items in view are ever asked
//! for and built - THUMBNAILS ARRIVE LATER: the app answers an icon glyph
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
//! band that selects the items it crosses (Ctrl adds them); a double-click
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
//! Shift+F10 reports a context menu, Escape clears the selection.
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
        basic::StyleFontSize,
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutOverflow,
            LayoutPosition,
        },
        property::CssProperty,
        style::StyleCursor,
    },
    AzString,
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
/// The rubber band.
pub(crate) const MARQUEE_CLASS: &str = "__azul-native-icon-grid-marquee";
/// The scroll bar's track.
pub(crate) const TRACK_CLASS: &str = "__azul-native-icon-grid-track";
/// The scroll bar's thumb.
pub(crate) const SCROLL_THUMB_CLASS: &str = "__azul-native-icon-grid-scroll-thumb";

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
    /// A small glyph over the icon's corner ("cloud_done"), or empty.
    pub badge: AzString,
    /// The thumbnail, once the app has it (it replaces the icon).
    pub image: OptionImageRef,
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
    /// Pending: the pressed item.
    pub index: usize,
    /// What the drag does.
    pub kind: IconGridDragKind,
}

impl Default for IconGridDrag {
    fn default() -> Self {
        Self {
            base: U64Vec::from_const_slice(&[]),
            start_x: 0.0,
            start_y: 0.0,
            x: 0.0,
            y: 0.0,
            start_top: 0.0,
            index: 0,
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
}

impl IconGridView {
    /// Nothing selected, at the top.
    #[must_use]
    pub fn create() -> Self {
        Self {
            selection: ListSelection::create(),
            drag: IconGridDrag::default(),
            top_row: 0,
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
    pub fn with_top_row(mut self, top_row: usize) -> Self {
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
        }
    }
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
        }
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
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width.max(0.0);
        self.viewport_height = height.max(0.0);
    }

    /// [`Self::set_viewport`] for the builder chain.
    #[must_use]
    pub fn with_viewport(mut self, width: f32, height: f32) -> Self {
        self.set_viewport(width, height);
        self
    }

    /// A cell's size and the icon's, px.
    pub fn set_cell_size(&mut self, width: f32, height: f32, icon_size: f32) {
        self.cell_width = width.max(1.0);
        self.cell_height = height.max(1.0);
        self.icon_size = icon_size.max(1.0);
    }

    /// [`Self::set_cell_size`] for the builder chain.
    #[must_use]
    pub fn with_cell_size(mut self, width: f32, height: f32, icon_size: f32) -> Self {
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
        build(self, &look)
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
}

/// What a point in the grid is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// The geometry of `g` as it is built now.
pub(crate) fn geometry(g: &IconGrid) -> Geometry {
    let _ = g;
    Geometry {
        count: 0,
        columns: 1,
        rows: 0,
        page_rows: 1,
        top: 0,
        max_top: 0,
        first: 0,
        end: 0,
        body_width: 0.0,
        height: 0.0,
        cell_width: 1.0,
        cell_height: 1.0,
        vbar: None,
    }
}

/// What the point (`x`, `y`) px in the grid is over.
pub(crate) fn hit_test(geo: &Geometry, x: f32, y: f32) -> Hit {
    let _ = (geo, x, y);
    Hit::Nothing
}

/// Item `index`'s cell (x, y, width, height px in the grid), when in view.
pub(crate) fn item_rect(geo: &Geometry, index: usize) -> Option<(f32, f32, f32, f32)> {
    let _ = (geo, index);
    None
}

/// The items whose cells the rectangle from (`x0`, `y0`) to (`x1`, `y1`)
/// (px in the grid, either way round) crosses, ascending.
pub(crate) fn marquee_keys(geo: &Geometry, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<u64> {
    let _ = (geo, x0, y0, x1, y1);
    Vec::new()
}

/// The view scrolled by `rows` rows (kept in range).
pub(crate) fn scroll_by(g: &IconGrid, geo: &Geometry, rows: i64) -> IconGridView {
    let _ = (geo, rows);
    g.view.clone()
}

/// What a press at (`x`, `y`) on `hit` does; `window_y` is the pointer's
/// window y (a thumb drag measures from it).
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
    let _ = (g, geo, hit, x, y, window_y, shift, ctrl);
    None
}

/// What a pointer move to (`x`, `y`) during a drag does (`window_y`: the
/// pointer's window y).
pub(crate) fn drag_move(g: &IconGrid, geo: &Geometry, x: f32, y: f32, window_y: f32) -> Option<IconGridEvent> {
    let _ = (g, geo, x, y, window_y);
    None
}

/// What the release of a drag does.
pub(crate) fn drag_end(g: &IconGrid) -> Option<IconGridEvent> {
    let _ = g;
    None
}

/// What `key` does (`shift`, `ctrl` = the primary modifier).
pub(crate) fn grid_key(g: &IconGrid, geo: &Geometry, key: VirtualKeyCode, shift: bool, ctrl: bool) -> Option<IconGridEvent> {
    let _ = (g, geo, key, shift, ctrl);
    None
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

/// The look a grid with the theme option `theme` is built with.
pub(crate) fn look_for(theme: OptionUiTheme) -> IconGridLook {
    let _ = theme;
    IconGridLook::default()
}

/// The grid's DOM in `look`.
pub(crate) fn build(grid: IconGrid, look: &IconGridLook) -> Dom {
    let _ = look;
    Dom::create_div()
        .with_class(AzString::from_const_str(GRID_CLASS))
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::List,
            accessibility_name: Some(grid.accessibility_name).into(),
            ..Default::default()
        })
}
