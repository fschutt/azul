//! Selection adorner widget - the editing layer of a canvas of objects (a
//! slide, a page, a drawing): it draws the selection's frame with its eight
//! resize handles and its rotate handle, the snapping guides and the marquee
//! over the app's own content, and turns the pointer and the keyboard into
//! object edits. PowerPoint's / Keynote's selection handles.
//!
//! THE APP OWNS THE OBJECTS: it hands the adorner every object's frame
//! ([`AdornerItem`]: x, y, width, height, rotation in CANVAS units, and
//! whether it is selected) in z-order (the last is on top), the canvas size in
//! units and the view's `scale` (px per unit). The adorner hit-tests those
//! frames itself - the topmost frame under the pointer, rotated frames
//! included - so the app's content needs no hit areas of its own, and it
//! reports in canvas units ([`SelectionAdornerEvent`]):
//!
//! - `Select` (a press on an object; `shift` / `ctrl` held say "add / toggle"),
//!   `Clear` (a press on the empty canvas);
//! - `Transform` while a drag moves, resizes (Shift keeps the ratio of a corner
//!   drag) or rotates (Shift snaps to 15 degrees) the selection - `frames` are
//!   the new frames of `indices`, `guides` the snapping guides the app hands
//!   back to be drawn - and `Commit` when the drag ends;
//! - `Marquee` while a drag on the empty canvas spans a rectangle (`frames[0]`,
//!   `indices` = the objects inside it) and `MarqueeEnd` when it is let go;
//! - `Activate` (a double-click, Enter or F2 on the selection: edit its text);
//! - `Nudge` (the arrow keys move the selection by `nudge` units, ten times
//!   that without Ctrl / Cmd), `Delete`, `Escape`; Tab / Shift+Tab select the
//!   next / previous object.
//!
//! SNAPPING: a moved selection snaps its left / centre / right and top /
//! middle / bottom to the canvas's edges and centre lines and to the other
//! objects' edges and centres, within `snap_distance` px; a resized
//! unrotated selection snaps the edges it drags.
//!
//! TEXT EDITING: while an object is being edited (`editing`), a press inside
//! it is left to the text engine (no capture, no event), and it is outlined
//! by four thin edges instead of a box, so the caret can be placed in it.
//!
//! STRUCTURE: the root holds the app's content, then the adorner's pieces as
//! absolutely positioned siblings after it (azul has no `pointer-events`, so a
//! full-size overlay would take the editing text's clicks). The pointer
//! callbacks sit on the root and capture the pointer for a drag (the split
//! pane's pattern); the drag survives the app's rebuild between two moves
//! through the root's dataset and its merge callback.
//!
//! Key types: [`SelectionAdorner`], [`AdornerItem`], [`AdornerFrame`],
//! [`SelectionAdornerEvent`].

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    corety::{OptionUsize, U32Vec},
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::{
        basic::{angle::AngleValue, length::FloatValue, pixel::PixelValue},
        layout::{
            LayoutBoxSizing, LayoutFlexShrink, LayoutHeight, LayoutLeft, LayoutMarginLeft,
            LayoutMarginTop, LayoutOverflow, LayoutPosition, LayoutTop, LayoutWidth,
        },
        property::CssProperty,
        style::{StyleCursor, StyleTransform, StyleTransformVec, StyleUserSelect},
    },
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::themes::{OptionUiTheme, UiTheme},
};

/// The root's class.
pub(crate) const ROOT_CLASS: &str = "__azul-native-selection-adorner";
/// A selected object's frame.
pub(crate) const FRAME_CLASS: &str = "__azul-native-selection-adorner-frame";
/// The box around a multi-selection (it carries the resize handles).
pub(crate) const GROUP_CLASS: &str = "__azul-native-selection-adorner-group";
/// One edge of the object being edited.
pub(crate) const EDITING_CLASS: &str = "__azul-native-selection-adorner-editing";
/// A resize handle.
pub(crate) const HANDLE_CLASS: &str = "__azul-native-selection-adorner-handle";
/// The rotate handle.
pub(crate) const ROTATE_CLASS: &str = "__azul-native-selection-adorner-rotate";
/// The stem from the top edge to the rotate handle.
pub(crate) const STEM_CLASS: &str = "__azul-native-selection-adorner-stem";
/// A snapping guide.
pub(crate) const GUIDE_CLASS: &str = "__azul-native-selection-adorner-guide";
/// The marquee.
pub(crate) const MARQUEE_CLASS: &str = "__azul-native-selection-adorner-marquee";

/// A handle's drawn size, px.
pub(crate) const HANDLE_PX: f32 = 8.0;
/// How far from a handle's centre a press still grabs it, px.
pub(crate) const GRAB_PX: f32 = 6.0;
/// The rotate handle's distance above the top edge, px.
pub(crate) const ROTATE_OFFSET_PX: f32 = 24.0;
/// A press that moves less than this (px) is a click, not a drag.
pub(crate) const CLICK_SLOP_PX: f32 = 3.0;
/// The smallest width / height a resize leaves, canvas units.
pub(crate) const MIN_SIZE: f32 = 1.0;

// ==== Types ====

/// An object's box on the canvas, in canvas units, and its rotation in
/// degrees, clockwise, about the box's centre.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdornerFrame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
}

impl AdornerFrame {
    /// An unrotated frame.
    #[must_use]
    pub const fn create(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            rotation: 0.0,
        }
    }

    /// The frame turned by `rotation` degrees.
    #[must_use]
    pub const fn with_rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }

    /// The centre's x.
    #[must_use]
    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    /// The centre's y.
    #[must_use]
    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    /// Whether the point (`x`, `y`) lies in the (rotated) frame.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (lx, ly) = to_local(self, x, y);
        lx.abs() <= self.width / 2.0 && ly.abs() <= self.height / 2.0
    }

    /// The axis-aligned box around the rotated frame.
    #[must_use]
    pub fn bounds(&self) -> AdornerFrame {
        if self.rotation.abs() < f32::EPSILON {
            return AdornerFrame::create(self.x, self.y, self.width, self.height);
        }
        let (s, c) = self.rotation.to_radians().sin_cos();
        let (hw, hh) = (self.width / 2.0, self.height / 2.0);
        let ex = (hw * c).abs() + (hh * s).abs();
        let ey = (hw * s).abs() + (hh * c).abs();
        AdornerFrame::create(self.center_x() - ex, self.center_y() - ey, ex * 2.0, ey * 2.0)
    }

    /// The frame moved by (`dx`, `dy`).
    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> AdornerFrame {
        AdornerFrame {
            x: self.x + dx,
            y: self.y + dy,
            ..*self
        }
    }
}

impl_option!(
    AdornerFrame,
    OptionAdornerFrame,
    [Debug, Copy, Clone, PartialEq]
);
impl_vec!(
    AdornerFrame,
    AdornerFrameVec,
    AdornerFrameVecDestructor,
    AdornerFrameVecDestructorType,
    AdornerFrameVecSlice,
    OptionAdornerFrame
);
impl_vec_clone!(AdornerFrame, AdornerFrameVec, AdornerFrameVecDestructor);
impl_vec_debug!(AdornerFrame, AdornerFrameVec);
impl_vec_partialeq!(AdornerFrame, AdornerFrameVec);
impl_vec_mut!(AdornerFrame, AdornerFrameVec);

/// One object of the canvas: its frame and whether it is selected.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdornerItem {
    pub frame: AdornerFrame,
    pub selected: bool,
}

impl AdornerItem {
    /// An object at `frame`, not selected.
    #[must_use]
    pub const fn create(frame: AdornerFrame) -> Self {
        Self {
            frame,
            selected: false,
        }
    }

    /// Selected or not.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl_option!(AdornerItem, OptionAdornerItem, [Debug, Copy, Clone, PartialEq]);
impl_vec!(
    AdornerItem,
    AdornerItemVec,
    AdornerItemVecDestructor,
    AdornerItemVecDestructorType,
    AdornerItemVecSlice,
    OptionAdornerItem
);
impl_vec_clone!(AdornerItem, AdornerItemVec, AdornerItemVecDestructor);
impl_vec_debug!(AdornerItem, AdornerItemVec);
impl_vec_partialeq!(AdornerItem, AdornerItemVec);
impl_vec_mut!(AdornerItem, AdornerItemVec);

/// A snapping guide: a vertical line at x = `position` from y = `start` to
/// `end`, or a horizontal one at y = `position` from x = `start` to `end`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdornerGuide {
    pub position: f32,
    pub start: f32,
    pub end: f32,
    pub vertical: bool,
}

impl AdornerGuide {
    /// A guide at `position` spanning `start..end`.
    #[must_use]
    pub const fn create(position: f32, start: f32, end: f32, vertical: bool) -> Self {
        Self {
            position,
            start,
            end,
            vertical,
        }
    }
}

impl_option!(AdornerGuide, OptionAdornerGuide, [Debug, Copy, Clone, PartialEq]);
impl_vec!(
    AdornerGuide,
    AdornerGuideVec,
    AdornerGuideVecDestructor,
    AdornerGuideVecDestructorType,
    AdornerGuideVecSlice,
    OptionAdornerGuide
);
impl_vec_clone!(AdornerGuide, AdornerGuideVec, AdornerGuideVecDestructor);
impl_vec_debug!(AdornerGuide, AdornerGuideVec);
impl_vec_partialeq!(AdornerGuide, AdornerGuideVec);
impl_vec_mut!(AdornerGuide, AdornerGuideVec);

/// The part of the selection a press or a drag holds.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum AdornerHandle {
    /// The empty canvas (a marquee, a clear).
    #[default]
    Canvas,
    /// An object's body: a move.
    Body,
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    /// The rotate handle above the top edge.
    Rotate,
}

impl AdornerHandle {
    /// The eight resize handles, clockwise from the top left.
    pub const RESIZE: [AdornerHandle; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];

    /// Where the handle sits on the frame: (-1 | 0 | 1, -1 | 0 | 1) for
    /// left / centre / right and top / middle / bottom.
    #[must_use]
    pub const fn anchor(self) -> (i8, i8) {
        match self {
            Self::TopLeft => (-1, -1),
            Self::Top => (0, -1),
            Self::TopRight => (1, -1),
            Self::Right => (1, 0),
            Self::BottomRight => (1, 1),
            Self::Bottom => (0, 1),
            Self::BottomLeft => (-1, 1),
            Self::Left => (-1, 0),
            Self::Canvas | Self::Body | Self::Rotate => (0, 0),
        }
    }

    /// A corner (it resizes both ways).
    #[must_use]
    pub const fn is_corner(self) -> bool {
        matches!(
            self,
            Self::TopLeft | Self::TopRight | Self::BottomRight | Self::BottomLeft
        )
    }

    /// One of the eight resize handles.
    #[must_use]
    pub const fn is_resize(self) -> bool {
        !matches!(self, Self::Canvas | Self::Body | Self::Rotate)
    }
}

/// What happened on the canvas.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SelectionAdornerEventKind {
    /// A press on object `indices[0]`; `shift` / `ctrl` held: add it to the
    /// selection or toggle it, else select it alone.
    Select,
    /// A press on the empty canvas without a modifier: select nothing.
    Clear,
    /// A drag step: `frames` are the new frames of the objects `indices`
    /// (`handle` says move, resize or rotate), `guides` the snapping guides.
    Transform,
    /// The drag ended: `frames` are the final frames of `indices`.
    Commit,
    /// A drag on the empty canvas: `frames[0]` is the marquee, `indices` the
    /// objects wholly inside it.
    Marquee,
    /// The marquee was let go: select `indices`.
    MarqueeEnd,
    /// A double-click on object `indices[0]`, or Enter / F2 on the selection:
    /// edit it.
    Activate,
    /// The arrow keys moved the selection: `frames` are the new frames of
    /// `indices`.
    Nudge,
    /// Delete / Backspace: delete `indices`.
    Delete,
    /// Escape: leave the text, or select nothing.
    Escape,
}

/// One action on the canvas, in canvas units.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionAdornerEvent {
    /// The new frames of `indices` (`Transform`, `Commit`, `Nudge`), the
    /// marquee (`Marquee`, `MarqueeEnd`: one frame); empty otherwise.
    pub frames: AdornerFrameVec,
    /// The snapping guides to draw (`Transform`); empty otherwise.
    pub guides: AdornerGuideVec,
    /// The objects the event is about, as indices into the items.
    pub indices: U32Vec,
    /// The pointer, in canvas units (0 for a key).
    pub x: f32,
    /// The pointer, in canvas units (0 for a key).
    pub y: f32,
    /// What happened.
    pub kind: SelectionAdornerEventKind,
    /// What the drag holds (`Transform`, `Commit`).
    pub handle: AdornerHandle,
    /// Shift was held.
    pub shift: bool,
    /// Ctrl (or Cmd) was held.
    pub ctrl: bool,
}

impl SelectionAdornerEvent {
    /// A `kind` event about `indices`, nothing else set.
    #[must_use]
    pub fn create(kind: SelectionAdornerEventKind, indices: U32Vec) -> Self {
        Self {
            frames: AdornerFrameVec::from_const_slice(&[]),
            guides: AdornerGuideVec::from_const_slice(&[]),
            indices,
            x: 0.0,
            y: 0.0,
            kind,
            handle: AdornerHandle::Canvas,
            shift: false,
            ctrl: false,
        }
    }
}

/// Callback invoked for an action on the canvas.
pub type SelectionAdornerOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, SelectionAdornerEvent) -> Update;
impl_widget_callback!(
    SelectionAdornerOnEvent,
    OptionSelectionAdornerOnEvent,
    SelectionAdornerOnEventCallback,
    SelectionAdornerOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SelectionAdornerOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SELECTION_ADORNER_ON_EVENT_INVOKER,
    invoker_ty:     AzSelectionAdornerOnEventCallbackInvoker,
    thunk_fn:       az_selection_adorner_on_event_callback_thunk,
    setter_fn:      AzApp_setSelectionAdornerOnEventCallbackInvoker,
    from_handle_fn: AzSelectionAdornerOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzSelectionAdornerOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: SelectionAdornerEvent ],
}

/// The editing layer of a canvas of objects (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionAdorner {
    /// The canvas's own content, drawn under the adorner, `width * scale` by
    /// `height * scale` px.
    pub content: Dom,
    /// Every object's frame in z-order (the last is on top) and whether it
    /// is selected.
    pub items: AdornerItemVec,
    /// The snapping guides to draw (the last `Transform`'s, handed back).
    pub guides: AdornerGuideVec,
    /// Every action on the canvas.
    pub on_event: OptionSelectionAdornerOnEvent,
    /// The canvas's accessible name ("Slide 3").
    pub accessibility_name: AzString,
    /// The object whose text is being edited, if any.
    pub editing: OptionUsize,
    /// The marquee to draw (the last `Marquee`'s, handed back).
    pub marquee: OptionAdornerFrame,
    /// The canvas's width, in canvas units.
    pub width: f32,
    /// The canvas's height, in canvas units.
    pub height: f32,
    /// px per canvas unit.
    pub scale: f32,
    /// How close (px) an edge or a centre must come to snap.
    pub snap_distance: f32,
    /// An arrow key's step with Ctrl / Cmd, canvas units (ten without).
    pub nudge: f32,
    /// The widget theme this widget is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: OptionUiTheme,
    /// Moves and resizes snap to the canvas and the other objects.
    pub snap: bool,
}

impl SelectionAdorner {
    /// An adorner over `content`, a canvas of `width` by `height` units at
    /// scale 1, no objects, snapping on.
    #[must_use]
    pub fn create(content: Dom, width: f32, height: f32) -> Self {
        Self {
            content,
            items: AdornerItemVec::from_const_slice(&[]),
            guides: AdornerGuideVec::from_const_slice(&[]),
            on_event: None.into(),
            accessibility_name: AzString::from_const_str("Canvas"),
            editing: OptionUsize::None,
            marquee: OptionAdornerFrame::None,
            width,
            height,
            scale: 1.0,
            snap_distance: 6.0,
            nudge: 1.0,
            theme: OptionUiTheme::None,
            snap: true,
        }
    }

    /// Every object's frame and selection, in z-order.
    pub fn set_items(&mut self, items: AdornerItemVec) {
        self.items = items;
    }

    /// [`Self::set_items`] for the builder chain.
    #[must_use]
    pub fn with_items(mut self, items: AdornerItemVec) -> Self {
        self.set_items(items);
        self
    }

    /// Adds an object on top.
    pub fn add_item(&mut self, item: AdornerItem) {
        let mut items = core::mem::replace(&mut self.items, AdornerItemVec::from_const_slice(&[]))
            .into_library_owned_vec();
        items.push(item);
        self.items = AdornerItemVec::from_vec(items);
    }

    /// [`Self::add_item`] for the builder chain.
    #[must_use]
    pub fn with_item(mut self, item: AdornerItem) -> Self {
        self.add_item(item);
        self
    }

    /// px per canvas unit.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    }

    /// [`Self::set_scale`] for the builder chain.
    #[must_use]
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.set_scale(scale);
        self
    }

    /// The snapping guides to draw.
    pub fn set_guides(&mut self, guides: AdornerGuideVec) {
        self.guides = guides;
    }

    /// [`Self::set_guides`] for the builder chain.
    #[must_use]
    pub fn with_guides(mut self, guides: AdornerGuideVec) -> Self {
        self.set_guides(guides);
        self
    }

    /// The marquee to draw.
    pub fn set_marquee(&mut self, marquee: AdornerFrame) {
        self.marquee = OptionAdornerFrame::Some(marquee);
    }

    /// [`Self::set_marquee`] for the builder chain.
    #[must_use]
    pub fn with_marquee(mut self, marquee: AdornerFrame) -> Self {
        self.set_marquee(marquee);
        self
    }

    /// The object whose text is being edited.
    pub fn set_editing(&mut self, index: usize) {
        self.editing = OptionUsize::Some(index);
    }

    /// [`Self::set_editing`] for the builder chain.
    #[must_use]
    pub fn with_editing(mut self, index: usize) -> Self {
        self.set_editing(index);
        self
    }

    /// Snapping on or off.
    pub fn set_snap(&mut self, snap: bool) {
        self.snap = snap;
    }

    /// [`Self::set_snap`] for the builder chain.
    #[must_use]
    pub fn with_snap(mut self, snap: bool) -> Self {
        self.set_snap(snap);
        self
    }

    /// How close (px) an edge must come to snap.
    pub fn set_snap_distance(&mut self, px: f32) {
        self.snap_distance = px.max(0.0);
    }

    /// [`Self::set_snap_distance`] for the builder chain.
    #[must_use]
    pub fn with_snap_distance(mut self, px: f32) -> Self {
        self.set_snap_distance(px);
        self
    }

    /// An arrow key's fine step (with Ctrl / Cmd), canvas units.
    pub fn set_nudge(&mut self, units: f32) {
        self.nudge = units.max(0.0);
    }

    /// [`Self::set_nudge`] for the builder chain.
    #[must_use]
    pub fn with_nudge(mut self, units: f32) -> Self {
        self.set_nudge(units);
        self
    }

    /// The canvas's accessible name.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// Every action on the canvas.
    pub fn set_on_event<C: Into<SelectionAdornerOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_event = Some(SelectionAdornerOnEvent {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<SelectionAdornerOnEventCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_event(data, cb);
        self
    }

    /// Pin the widget theme; unset, the adorner follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty adorner and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The adorner's DOM: the content, then the frames, handles, guides and
    /// marquee, in the pinned theme's look or both looks merged.
    #[must_use]
    pub fn dom(self) -> Dom {
        let look = look_for(self.theme);
        build(self, &look)
    }
}

impl Default for SelectionAdorner {
    fn default() -> Self {
        Self::create(Dom::create_div(), 0.0, 0.0)
    }
}

impl From<SelectionAdorner> for Dom {
    fn from(a: SelectionAdorner) -> Self {
        a.dom()
    }
}

// ==== Geometry (canvas units) ====

/// (`x`, `y`) in `f`'s own axes, relative to its centre (the frame's
/// rotation undone).
pub(crate) fn to_local(f: &AdornerFrame, x: f32, y: f32) -> (f32, f32) {
    let (dx, dy) = (x - f.center_x(), y - f.center_y());
    if f.rotation.abs() < f32::EPSILON {
        return (dx, dy);
    }
    let (s, c) = (-f.rotation).to_radians().sin_cos();
    (dx * c - dy * s, dx * s + dy * c)
}

/// A point in `f`'s own axes (relative to its centre) back on the canvas.
pub(crate) fn from_local(f: &AdornerFrame, lx: f32, ly: f32) -> (f32, f32) {
    if f.rotation.abs() < f32::EPSILON {
        return (f.center_x() + lx, f.center_y() + ly);
    }
    let (s, c) = f.rotation.to_radians().sin_cos();
    (f.center_x() + lx * c - ly * s, f.center_y() + lx * s + ly * c)
}

/// The handle of `f` under (`x`, `y`) at `scale` px per unit: a resize
/// handle, the rotate handle (when `rotate`), the body, or none.
pub(crate) fn handle_at(
    f: &AdornerFrame,
    x: f32,
    y: f32,
    scale: f32,
    rotate: bool,
) -> Option<AdornerHandle> {
    let scale = if scale > f32::EPSILON { scale } else { 1.0 };
    let reach = GRAB_PX / scale;
    let (lx, ly) = to_local(f, x, y);
    let (hw, hh) = (f.width / 2.0, f.height / 2.0);
    if rotate {
        let ry = -hh - ROTATE_OFFSET_PX / scale;
        if lx.abs() <= reach && (ly - ry).abs() <= reach {
            return Some(AdornerHandle::Rotate);
        }
    }
    // The corners win over the edges where a small frame's handles overlap.
    let order = [
        AdornerHandle::TopLeft,
        AdornerHandle::TopRight,
        AdornerHandle::BottomRight,
        AdornerHandle::BottomLeft,
        AdornerHandle::Top,
        AdornerHandle::Right,
        AdornerHandle::Bottom,
        AdornerHandle::Left,
    ];
    for handle in order {
        let (ax, ay) = handle.anchor();
        let (px, py) = (f32::from(ax) * hw, f32::from(ay) * hh);
        if (lx - px).abs() <= reach && (ly - py).abs() <= reach {
            return Some(handle);
        }
    }
    (lx.abs() <= hw && ly.abs() <= hh).then_some(AdornerHandle::Body)
}

/// The topmost item whose frame holds (`x`, `y`).
pub(crate) fn hit_item(items: &[AdornerItem], x: f32, y: f32) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .rev()
        .find(|(_, it)| it.frame.contains(x, y))
        .map(|(i, _)| i)
}

/// The axis-aligned box around the rotated `frames`.
pub(crate) fn union(frames: &[AdornerFrame]) -> Option<AdornerFrame> {
    let mut boxes = frames.iter().map(AdornerFrame::bounds);
    let first = boxes.next()?;
    let (mut x0, mut y0) = (first.x, first.y);
    let (mut x1, mut y1) = (first.x + first.width, first.y + first.height);
    for b in boxes {
        x0 = x0.min(b.x);
        y0 = y0.min(b.y);
        x1 = x1.max(b.x + b.width);
        y1 = y1.max(b.y + b.height);
    }
    Some(AdornerFrame::create(x0, y0, x1 - x0, y1 - y0))
}

/// `start` dragged by `handle` over (`dx`, `dy`) canvas units: the opposite
/// edge (or corner) stays where it is, along the frame's own axes; a corner
/// keeps the ratio when `keep_ratio`.
pub(crate) fn resized(
    start: &AdornerFrame,
    handle: AdornerHandle,
    dx: f32,
    dy: f32,
    keep_ratio: bool,
) -> AdornerFrame {
    // The drag in the frame's own axes.
    let (s, c) = (-start.rotation).to_radians().sin_cos();
    let (ldx, ldy) = (dx * c - dy * s, dx * s + dy * c);
    let (hw, hh) = (start.width / 2.0, start.height / 2.0);
    let (ax, ay) = handle.anchor();
    let (mut left, mut right, mut top, mut bottom) = (-hw, hw, -hh, hh);
    if ax < 0 {
        left = (left + ldx).min(right - MIN_SIZE);
    } else if ax > 0 {
        right = (right + ldx).max(left + MIN_SIZE);
    }
    if ay < 0 {
        top = (top + ldy).min(bottom - MIN_SIZE);
    } else if ay > 0 {
        bottom = (bottom + ldy).max(top + MIN_SIZE);
    }
    if keep_ratio && handle.is_corner() && start.width > 0.0 && start.height > 0.0 {
        let k = ((right - left) / start.width).max((bottom - top) / start.height);
        let (w, h) = (start.width * k, start.height * k);
        if ax < 0 {
            left = right - w;
        } else {
            right = left + w;
        }
        if ay < 0 {
            top = bottom - h;
        } else {
            bottom = top + h;
        }
    }
    let (w, h) = (right - left, bottom - top);
    let (cx, cy) = from_local(start, (left + right) / 2.0, (top + bottom) / 2.0);
    AdornerFrame {
        x: cx - w / 2.0,
        y: cy - h / 2.0,
        width: w,
        height: h,
        rotation: start.rotation,
    }
}

/// `start` turned so its top points at (`x`, `y`); `snap15` snaps to 15
/// degrees, `magnet` pulls to a right angle within 3 degrees.
pub(crate) fn rotated(
    start: &AdornerFrame,
    x: f32,
    y: f32,
    snap15: bool,
    magnet: bool,
) -> AdornerFrame {
    let (cx, cy) = (start.center_x(), start.center_y());
    // The top of an unturned frame points up: -90 degrees on the screen.
    let mut deg = (y - cy).atan2(x - cx).to_degrees() + 90.0;
    if snap15 {
        deg = (deg / 15.0).round() * 15.0;
    } else if magnet {
        let right_angle = (deg / 90.0).round() * 90.0;
        if (deg - right_angle).abs() <= 3.0 {
            deg = right_angle;
        }
    }
    deg = deg.rem_euclid(360.0);
    if (deg - 360.0).abs() < 1e-3 {
        deg = 0.0;
    }
    AdornerFrame {
        rotation: deg,
        ..*start
    }
}

/// `f` mapped from the box `from` onto the box `to` (a member of a resized
/// multi-selection).
pub(crate) fn map_frame(f: &AdornerFrame, from: &AdornerFrame, to: &AdornerFrame) -> AdornerFrame {
    let sx = if from.width.abs() > f32::EPSILON {
        to.width / from.width
    } else {
        1.0
    };
    let sy = if from.height.abs() > f32::EPSILON {
        to.height / from.height
    } else {
        1.0
    };
    AdornerFrame {
        x: to.x + (f.x - from.x) * sx,
        y: to.y + (f.y - from.y) * sy,
        width: f.width * sx,
        height: f.height * sy,
        rotation: f.rotation,
    }
}

/// The snap lines of the canvas and of `others`, one axis: the canvas's
/// edges and centre line, and every other object's edges and centre.
fn snap_lines(others: &[AdornerFrame], size: f32, vertical: bool) -> Vec<f32> {
    let mut lines = vec![0.0, size / 2.0, size];
    for o in others {
        let b = o.bounds();
        if vertical {
            lines.extend([b.x, b.center_x(), b.x + b.width]);
        } else {
            lines.extend([b.y, b.center_y(), b.y + b.height]);
        }
    }
    lines
}

/// The nearest snap: (the shift, the line it snaps to) for the first of
/// `edges` closest to one of `lines`, within `tolerance`.
fn nearest_snap(edges: &[f32], lines: &[f32], tolerance: f32) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for &edge in edges {
        for &line in lines {
            let shift = line - edge;
            if shift.abs() <= tolerance && best.map_or(true, |(b, _)| shift.abs() < b.abs()) {
                best = Some((shift, line));
            }
        }
    }
    best
}

/// The shift that snaps the box `moving` to the canvas (`width` x `height`:
/// edges and centre lines) and to `others` (their edges and centres) within
/// `tolerance` units, per axis, and the guides of the snaps it made.
pub(crate) fn snap_move(
    moving: &AdornerFrame,
    others: &[AdornerFrame],
    width: f32,
    height: f32,
    tolerance: f32,
) -> (f32, f32, Vec<AdornerGuide>) {
    let xs = [moving.x, moving.center_x(), moving.x + moving.width];
    let ys = [moving.y, moving.center_y(), moving.y + moving.height];
    let sx = nearest_snap(&xs, &snap_lines(others, width, true), tolerance);
    let sy = nearest_snap(&ys, &snap_lines(others, height, false), tolerance);
    let mut guides = Vec::new();
    if let Some((_, line)) = sx {
        guides.push(AdornerGuide::create(line, 0.0, height, true));
    }
    if let Some((_, line)) = sy {
        guides.push(AdornerGuide::create(line, 0.0, width, false));
    }
    (sx.map_or(0.0, |s| s.0), sy.map_or(0.0, |s| s.0), guides)
}

/// `target` (an unturned frame being resized by `handle`) with the edges
/// the handle drags snapped to the canvas and `others`, and the guides.
fn snap_resize(
    target: &AdornerFrame,
    handle: AdornerHandle,
    others: &[AdornerFrame],
    width: f32,
    height: f32,
    tolerance: f32,
) -> (AdornerFrame, Vec<AdornerGuide>) {
    let (ax, ay) = handle.anchor();
    let mut out = *target;
    let mut guides = Vec::new();
    if ax != 0 {
        let edge = if ax < 0 { out.x } else { out.x + out.width };
        if let Some((shift, line)) = nearest_snap(&[edge], &snap_lines(others, width, true), tolerance) {
            if ax < 0 {
                out.x += shift;
                out.width = (out.width - shift).max(MIN_SIZE);
            } else {
                out.width = (out.width + shift).max(MIN_SIZE);
            }
            guides.push(AdornerGuide::create(line, 0.0, height, true));
        }
    }
    if ay != 0 {
        let edge = if ay < 0 { out.y } else { out.y + out.height };
        if let Some((shift, line)) = nearest_snap(&[edge], &snap_lines(others, height, false), tolerance) {
            if ay < 0 {
                out.y += shift;
                out.height = (out.height - shift).max(MIN_SIZE);
            } else {
                out.height = (out.height + shift).max(MIN_SIZE);
            }
            guides.push(AdornerGuide::create(line, 0.0, width, false));
        }
    }
    (out, guides)
}

// ==== The state machine (the root's dataset) ====

/// What a drag in flight does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DragKind {
    #[default]
    Idle,
    Transform,
    Marquee,
}

/// A drag in flight: what it holds and where everything was at the press.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct AdornerDrag {
    pub kind: DragKind,
    pub handle: AdornerHandle,
    pub start_x: f32,
    pub start_y: f32,
    /// The objects the drag transforms.
    pub indices: Vec<u32>,
    /// Their frames at the press.
    pub frames: Vec<AdornerFrame>,
    /// The frame the handle was on: the object, or the box around several.
    pub reference: AdornerFrame,
    /// Past the click slop.
    pub moved: bool,
    /// A press on a selected object of a multi-selection: selecting it
    /// alone if the press stays a click.
    pub click: Option<usize>,
    /// The frames last reported.
    pub last: Vec<AdornerFrame>,
}

/// Everything the root's callbacks share: the objects, the canvas, the
/// app's hook and the drag in flight.
#[derive(Debug, Clone)]
pub(crate) struct AdornerState {
    pub items: Vec<AdornerItem>,
    pub on_event: OptionSelectionAdornerOnEvent,
    pub editing: Option<usize>,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub snap_distance: f32,
    pub nudge: f32,
    pub snap: bool,
    pub drag: AdornerDrag,
}

impl AdornerState {
    /// The state of a canvas of `items`, `width` x `height` units at scale 1,
    /// snapping off, no hook.
    pub(crate) fn new(items: Vec<AdornerItem>, width: f32, height: f32) -> Self {
        Self {
            items,
            on_event: None.into(),
            editing: None,
            width,
            height,
            scale: 1.0,
            snap_distance: 6.0,
            nudge: 1.0,
            snap: false,
            drag: AdornerDrag::default(),
        }
    }

    /// The selected items, in z-order.
    pub(crate) fn selected(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.selected)
            .map(|(i, _)| i)
            .collect()
    }

    /// A press at (`x`, `y`) canvas units: the events it reports and whether
    /// it starts a drag (the pointer is captured).
    pub(crate) fn press(
        &mut self,
        x: f32,
        y: f32,
        shift: bool,
        ctrl: bool,
    ) -> (Vec<SelectionAdornerEvent>, bool) {
        let _ = (x, y, shift, ctrl);
        (Vec::new(), false)
    }

    /// The pointer moved to (`x`, `y`) during a drag.
    pub(crate) fn drag_to(&mut self, x: f32, y: f32, shift: bool) -> Option<SelectionAdornerEvent> {
        let _ = (x, y, shift);
        None
    }

    /// The button went up at (`x`, `y`).
    pub(crate) fn release(&mut self, x: f32, y: f32) -> Option<SelectionAdornerEvent> {
        let _ = (x, y);
        None
    }

    /// A double-click at (`x`, `y`).
    pub(crate) fn activate(&self, x: f32, y: f32) -> Option<SelectionAdornerEvent> {
        let _ = (x, y);
        None
    }

    /// A key on the focused canvas.
    pub(crate) fn key(
        &self,
        key: VirtualKeyCode,
        shift: bool,
        ctrl: bool,
    ) -> Option<SelectionAdornerEvent> {
        let _ = (key, shift, ctrl);
        None
    }
}

/// The reconciler's merge: a drag in flight survives the app's rebuild.
pub(crate) extern "C" fn merge_adorner_state(new_data: RefAny, old_data: RefAny) -> RefAny {
    let _ = old_data;
    new_data
}

#[cfg(test)]
mod geometry_and_drag_tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn same(a: &AdornerFrame, b: &AdornerFrame) -> bool {
        close(a.x, b.x)
            && close(a.y, b.y)
            && close(a.width, b.width)
            && close(a.height, b.height)
            && close(a.rotation, b.rotation)
    }

    fn item(x: f32, y: f32, w: f32, h: f32, selected: bool) -> AdornerItem {
        AdornerItem::create(AdornerFrame::create(x, y, w, h)).with_selected(selected)
    }

    fn indices(e: &SelectionAdornerEvent) -> Vec<u32> {
        e.indices.as_ref().to_vec()
    }

    fn frames(e: &SelectionAdornerEvent) -> Vec<AdornerFrame> {
        e.frames.as_ref().to_vec()
    }

    #[test]
    fn a_rotated_frame_contains_the_points_of_its_turned_box() {
        // 200 x 20 turned upright: it covers x 190..210, y 50..250.
        let f = AdornerFrame::create(100.0, 140.0, 200.0, 20.0).with_rotation(90.0);
        assert!(f.contains(200.0, 60.0));
        assert!(f.contains(205.0, 240.0));
        assert!(!f.contains(120.0, 150.0), "the unturned box's end is not in it");
        let b = f.bounds();
        assert!(same(&b, &AdornerFrame::create(190.0, 50.0, 20.0, 200.0)), "{b:?}");
        let (lx, ly) = to_local(&f, 200.0, 50.0);
        assert!(
            close(lx, -100.0) && close(ly, 0.0),
            "the top of the turned box is its left end: {lx} {ly}"
        );
        let (x, y) = from_local(&f, -100.0, 0.0);
        assert!(close(x, 200.0) && close(y, 50.0));
    }

    #[test]
    fn a_press_on_a_corner_grabs_the_handle_and_a_press_inside_grabs_the_body() {
        let f = AdornerFrame::create(100.0, 100.0, 200.0, 100.0);
        assert_eq!(handle_at(&f, 100.0, 100.0, 1.0, true), Some(AdornerHandle::TopLeft));
        assert_eq!(handle_at(&f, 302.0, 198.0, 1.0, true), Some(AdornerHandle::BottomRight));
        assert_eq!(handle_at(&f, 200.0, 200.0, 1.0, true), Some(AdornerHandle::Bottom));
        assert_eq!(handle_at(&f, 100.0, 150.0, 1.0, true), Some(AdornerHandle::Left));
        assert_eq!(handle_at(&f, 150.0, 130.0, 1.0, true), Some(AdornerHandle::Body));
        assert_eq!(handle_at(&f, 400.0, 130.0, 1.0, true), None);
        // At half scale a handle reaches 6 px = 12 units: 110 is still on it.
        assert_eq!(handle_at(&f, 110.0, 110.0, 0.5, true), Some(AdornerHandle::TopLeft));
        assert_eq!(handle_at(&f, 110.0, 110.0, 1.0, true), Some(AdornerHandle::Body));
    }

    #[test]
    fn the_rotate_handle_sits_above_the_top_edge_and_turns_with_the_frame() {
        let f = AdornerFrame::create(100.0, 100.0, 200.0, 100.0);
        assert_eq!(
            handle_at(&f, 200.0, 100.0 - ROTATE_OFFSET_PX, 1.0, true),
            Some(AdornerHandle::Rotate)
        );
        assert_eq!(
            handle_at(&f, 200.0, 100.0 - ROTATE_OFFSET_PX, 1.0, false),
            None,
            "no rotate handle asked for"
        );
        // Turned 90 degrees clockwise its top faces right.
        let turned = f.with_rotation(90.0);
        let (cx, cy) = (turned.center_x(), turned.center_y());
        assert_eq!(
            handle_at(&turned, cx + 50.0 + ROTATE_OFFSET_PX, cy, 1.0, true),
            Some(AdornerHandle::Rotate)
        );
    }

    #[test]
    fn the_topmost_object_under_the_pointer_is_hit() {
        let items = [
            item(0.0, 0.0, 100.0, 100.0, false),
            item(50.0, 50.0, 100.0, 100.0, false),
        ];
        assert_eq!(hit_item(&items, 75.0, 75.0), Some(1), "the later one is on top");
        assert_eq!(hit_item(&items, 25.0, 25.0), Some(0));
        assert_eq!(hit_item(&items, 175.0, 25.0), None);
    }

    #[test]
    fn a_press_on_an_object_selects_it_and_a_drag_moves_it_until_the_release_commits() {
        let mut s = AdornerState::new(vec![item(100.0, 100.0, 100.0, 50.0, false)], 1000.0, 1000.0);
        let (events, capture) = s.press(150.0, 120.0, false, false);
        assert!(capture, "the press starts a drag");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SelectionAdornerEventKind::Select);
        assert_eq!(indices(&events[0]), vec![0]);
        s.items[0].selected = true; // the app's answer
        let step = s.drag_to(200.0, 170.0, false).expect("a transform");
        assert_eq!(step.kind, SelectionAdornerEventKind::Transform);
        assert_eq!(step.handle, AdornerHandle::Body);
        assert_eq!(indices(&step), vec![0]);
        assert!(same(&frames(&step)[0], &AdornerFrame::create(150.0, 150.0, 100.0, 50.0)));
        // Shift keeps the move on one axis.
        let step = s.drag_to(260.0, 140.0, true).expect("a transform");
        assert!(same(&frames(&step)[0], &AdornerFrame::create(210.0, 100.0, 100.0, 50.0)));
        let end = s.release(260.0, 140.0).expect("a commit");
        assert_eq!(end.kind, SelectionAdornerEventKind::Commit);
        assert!(same(&frames(&end)[0], &AdornerFrame::create(210.0, 100.0, 100.0, 50.0)));
        assert_eq!(s.drag.kind, DragKind::Idle);
        assert!(s.release(260.0, 140.0).is_none(), "one release, one commit");
    }

    #[test]
    fn a_small_wiggle_is_a_click_not_a_drag() {
        let mut s = AdornerState::new(vec![item(100.0, 100.0, 100.0, 50.0, true)], 1000.0, 1000.0);
        let (events, _) = s.press(150.0, 120.0, false, false);
        assert!(events.is_empty(), "the object is selected already");
        assert!(s.drag_to(151.0, 121.0, false).is_none());
        assert!(s.release(151.0, 121.0).is_none());
    }

    #[test]
    fn shift_adds_to_the_selection_and_ctrl_on_a_selected_object_toggles_it_without_a_drag() {
        let mut s = AdornerState::new(
            vec![
                item(0.0, 0.0, 100.0, 100.0, true),
                item(200.0, 0.0, 100.0, 100.0, false),
            ],
            1000.0,
            1000.0,
        );
        let (events, capture) = s.press(250.0, 50.0, true, false);
        assert!(capture);
        assert_eq!(events[0].kind, SelectionAdornerEventKind::Select);
        assert!(events[0].shift);
        assert_eq!(indices(&events[0]), vec![1]);
        assert_eq!(s.drag.indices, vec![0, 1], "the drag moves the grown selection");
        s.release(250.0, 50.0);

        let (events, capture) = s.press(50.0, 50.0, false, true);
        assert!(!capture, "a toggle is no drag");
        assert_eq!(events[0].kind, SelectionAdornerEventKind::Select);
        assert!(events[0].ctrl);
        assert_eq!(indices(&events[0]), vec![0]);
    }

    #[test]
    fn a_click_on_one_object_of_a_multi_selection_selects_it_alone_on_release() {
        let mut s = AdornerState::new(
            vec![
                item(0.0, 0.0, 100.0, 100.0, true),
                item(200.0, 0.0, 100.0, 100.0, true),
            ],
            1000.0,
            1000.0,
        );
        let (events, capture) = s.press(250.0, 50.0, false, false);
        assert!(capture && events.is_empty(), "a press on the selection may start a move");
        let up = s.release(250.0, 50.0).expect("a select");
        assert_eq!(up.kind, SelectionAdornerEventKind::Select);
        assert_eq!(indices(&up), vec![1]);
    }

    #[test]
    fn a_press_on_the_empty_canvas_clears_and_a_drag_spans_a_marquee_of_the_objects_inside() {
        let mut s = AdornerState::new(
            vec![
                item(100.0, 100.0, 50.0, 50.0, true),
                item(300.0, 300.0, 50.0, 50.0, false),
                item(800.0, 800.0, 50.0, 50.0, false),
            ],
            1000.0,
            1000.0,
        );
        let (events, capture) = s.press(500.0, 50.0, false, false);
        assert!(capture);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SelectionAdornerEventKind::Clear);
        let span = s.drag_to(90.0, 400.0, false).expect("a marquee");
        assert_eq!(span.kind, SelectionAdornerEventKind::Marquee);
        assert!(same(&frames(&span)[0], &AdornerFrame::create(90.0, 50.0, 410.0, 350.0)));
        assert_eq!(indices(&span), vec![0, 1], "the objects wholly inside");
        let end = s.release(90.0, 400.0).expect("the marquee's end");
        assert_eq!(end.kind, SelectionAdornerEventKind::MarqueeEnd);
        assert_eq!(indices(&end), vec![0, 1]);
    }

    #[test]
    fn a_corner_drag_resizes_from_the_opposite_corner_and_shift_keeps_the_ratio() {
        let f = AdornerFrame::create(100.0, 100.0, 200.0, 100.0);
        let br = resized(&f, AdornerHandle::BottomRight, 50.0, 20.0, false);
        assert!(same(&br, &AdornerFrame::create(100.0, 100.0, 250.0, 120.0)), "{br:?}");
        let kept = resized(&f, AdornerHandle::BottomRight, 50.0, 20.0, true);
        assert!(same(&kept, &AdornerFrame::create(100.0, 100.0, 250.0, 125.0)), "{kept:?}");
        let tl = resized(&f, AdornerHandle::TopLeft, 50.0, 20.0, false);
        assert!(same(&tl, &AdornerFrame::create(150.0, 120.0, 150.0, 80.0)), "{tl:?}");
        let edge = resized(&f, AdornerHandle::Right, 50.0, 20.0, false);
        assert!(
            same(&edge, &AdornerFrame::create(100.0, 100.0, 250.0, 100.0)),
            "an edge moves one way: {edge:?}"
        );
        let tiny = resized(&f, AdornerHandle::Right, -500.0, 0.0, false);
        assert!(
            close(tiny.width, MIN_SIZE) && close(tiny.x, 100.0),
            "no flip past the opposite edge: {tiny:?}"
        );
    }

    #[test]
    fn an_edge_drag_of_a_turned_frame_resizes_along_its_own_axis_and_keeps_the_opposite_edge() {
        let f = AdornerFrame::create(100.0, 100.0, 200.0, 100.0).with_rotation(90.0);
        // Its right edge faces down: dragging down by 40 widens it by 40.
        let r = resized(&f, AdornerHandle::Right, 0.0, 40.0, false);
        assert!(
            same(&r, &AdornerFrame::create(80.0, 120.0, 240.0, 100.0).with_rotation(90.0)),
            "{r:?}"
        );
        let (x0, y0) = from_local(&f, -100.0, 0.0);
        let (x1, y1) = from_local(&r, -120.0, 0.0);
        assert!(close(x0, x1) && close(y0, y1), "the left edge stays");
    }

    #[test]
    fn a_rotate_drag_turns_about_the_centre_and_shift_snaps_to_15_degrees() {
        let f = AdornerFrame::create(100.0, 100.0, 200.0, 100.0);
        let right = rotated(&f, 300.0, 150.0, false, false);
        assert!(close(right.rotation, 90.0), "{right:?}");
        assert!(close(right.x, 100.0) && close(right.width, 200.0), "the box stays, it turns");
        assert!(close(rotated(&f, 200.0, 250.0, false, false).rotation, 180.0));
        assert!(close(rotated(&f, 100.0, 150.0, false, false).rotation, 270.0));
        let ten = 10f32.to_radians();
        let (px, py) = (200.0 + 100.0 * ten.cos(), 150.0 + 100.0 * ten.sin());
        assert!(close(rotated(&f, px, py, false, false).rotation, 100.0));
        assert!(close(rotated(&f, px, py, true, false).rotation, 105.0), "Shift: steps of 15");
        let two = 2f32.to_radians();
        let (qx, qy) = (200.0 + 100.0 * two.cos(), 150.0 + 100.0 * two.sin());
        assert!(
            close(rotated(&f, qx, qy, false, true).rotation, 90.0),
            "the magnet pulls to a right angle"
        );
    }

    #[test]
    fn a_move_snaps_to_the_canvas_centre_and_to_another_objects_edge_and_reports_the_guides() {
        let mut s = AdornerState::new(
            vec![
                item(100.0, 250.0, 100.0, 100.0, true),
                item(620.0, 100.0, 100.0, 100.0, false),
            ],
            1000.0,
            600.0,
        );
        s.snap = true;
        s.press(150.0, 300.0, false, false);
        // x 445 puts the centre at 495, 5 off the centre line at 500; the
        // middle stays on the canvas's middle.
        let step = s.drag_to(150.0 + 345.0, 300.0, false).expect("a transform");
        assert!(
            same(&frames(&step)[0], &AdornerFrame::create(450.0, 250.0, 100.0, 100.0)),
            "{:?}",
            frames(&step)
        );
        let guides = step.guides.as_ref().to_vec();
        assert!(guides.iter().any(|g| g.vertical && close(g.position, 500.0)), "{guides:?}");
        assert!(guides.iter().any(|g| !g.vertical && close(g.position, 300.0)), "{guides:?}");
        // The right edge at 618 is 2 off the other object's left edge at 620.
        let step = s.drag_to(150.0 + 418.0, 300.0, false).expect("a transform");
        assert!(close(frames(&step)[0].x, 520.0), "{:?}", frames(&step));
        assert!(step
            .guides
            .as_ref()
            .iter()
            .any(|g| g.vertical && close(g.position, 620.0)));
        // Far from everything: no snap, no guide.
        let step = s.drag_to(150.0 + 200.0, 300.0 + 77.0, false).expect("a transform");
        assert!(same(&frames(&step)[0], &AdornerFrame::create(300.0, 327.0, 100.0, 100.0)));
        assert!(step.guides.as_ref().is_empty());
    }

    #[test]
    fn a_multi_selection_resizes_as_one_box() {
        let mut s = AdornerState::new(
            vec![
                item(0.0, 0.0, 100.0, 100.0, true),
                item(200.0, 0.0, 100.0, 100.0, true),
            ],
            2000.0,
            2000.0,
        );
        let (events, capture) = s.press(300.0, 100.0, false, false);
        assert!(capture && events.is_empty());
        assert_eq!(
            s.drag.handle,
            AdornerHandle::BottomRight,
            "the box's corner, not the object's body"
        );
        let step = s.drag_to(600.0, 200.0, false).expect("a transform");
        let f = frames(&step);
        assert!(same(&f[0], &AdornerFrame::create(0.0, 0.0, 200.0, 200.0)), "{f:?}");
        assert!(same(&f[1], &AdornerFrame::create(400.0, 0.0, 200.0, 200.0)), "{f:?}");
    }

    #[test]
    fn a_press_inside_the_object_being_edited_is_left_to_the_text() {
        let mut s = AdornerState::new(vec![item(100.0, 100.0, 200.0, 100.0, true)], 1000.0, 1000.0);
        s.editing = Some(0);
        let (events, capture) = s.press(150.0, 120.0, false, false);
        assert!(events.is_empty() && !capture);
        assert_eq!(s.drag.kind, DragKind::Idle);
        let (events, capture) = s.press(600.0, 600.0, false, false);
        assert!(capture);
        assert_eq!(
            events[0].kind,
            SelectionAdornerEventKind::Clear,
            "a press outside ends the editing"
        );
    }

    #[test]
    fn the_arrows_nudge_delete_and_escape_report_and_tab_walks_the_objects() {
        let s = AdornerState::new(
            vec![
                item(10.0, 10.0, 10.0, 10.0, true),
                item(50.0, 50.0, 10.0, 10.0, false),
            ],
            1000.0,
            1000.0,
        );
        let e = s.key(VirtualKeyCode::Right, false, false).expect("a nudge");
        assert_eq!(e.kind, SelectionAdornerEventKind::Nudge);
        assert_eq!(indices(&e), vec![0]);
        assert!(same(&frames(&e)[0], &AdornerFrame::create(20.0, 10.0, 10.0, 10.0)));
        let fine = s.key(VirtualKeyCode::Up, false, true).expect("a fine nudge");
        assert!(same(&frames(&fine)[0], &AdornerFrame::create(10.0, 9.0, 10.0, 10.0)));
        assert_eq!(
            s.key(VirtualKeyCode::Delete, false, false).map(|e| e.kind),
            Some(SelectionAdornerEventKind::Delete)
        );
        assert_eq!(
            s.key(VirtualKeyCode::Back, false, false).map(|e| e.kind),
            Some(SelectionAdornerEventKind::Delete)
        );
        assert_eq!(
            s.key(VirtualKeyCode::Escape, false, false).map(|e| e.kind),
            Some(SelectionAdornerEventKind::Escape)
        );
        let next = s.key(VirtualKeyCode::Tab, false, false).expect("the next object");
        assert_eq!((next.kind, indices(&next)), (SelectionAdornerEventKind::Select, vec![1]));
        let prev = s.key(VirtualKeyCode::Tab, true, false).expect("the previous object");
        assert_eq!(indices(&prev), vec![1], "Shift+Tab wraps");
        let enter = s.key(VirtualKeyCode::Return, false, false).expect("activate");
        assert_eq!(
            (enter.kind, indices(&enter)),
            (SelectionAdornerEventKind::Activate, vec![0])
        );
        let none = AdornerState::new(vec![item(0.0, 0.0, 1.0, 1.0, false)], 10.0, 10.0);
        assert!(
            none.key(VirtualKeyCode::Right, false, false).is_none(),
            "nothing selected, nothing to nudge"
        );
        assert_eq!(
            none.key(VirtualKeyCode::Tab, false, false).map(|e| indices(&e)),
            Some(vec![0])
        );
    }

    #[test]
    fn a_double_click_activates_the_object_under_the_pointer() {
        let s = AdornerState::new(vec![item(100.0, 100.0, 200.0, 100.0, false)], 1000.0, 1000.0);
        let e = s.activate(150.0, 120.0).expect("activate");
        assert_eq!((e.kind, indices(&e)), (SelectionAdornerEventKind::Activate, vec![0]));
        assert!(s.activate(900.0, 900.0).is_none());
    }

    #[test]
    fn a_rebuild_mid_drag_keeps_the_drag() {
        let mut old = AdornerState::new(vec![item(100.0, 100.0, 100.0, 50.0, true)], 1000.0, 1000.0);
        old.press(150.0, 120.0, false, false);
        old.drag_to(200.0, 170.0, false);
        let drag = old.drag.clone();
        assert_eq!(drag.kind, DragKind::Transform);
        let fresh = AdornerState::new(vec![item(150.0, 150.0, 100.0, 50.0, true)], 1000.0, 1000.0);
        let mut merged = merge_adorner_state(RefAny::new(fresh), RefAny::new(old));
        let merged = merged.downcast_ref::<AdornerState>().expect("the state");
        assert_eq!(merged.drag, drag, "the drag goes on");
        assert!(
            same(&merged.items[0].frame, &AdornerFrame::create(150.0, 150.0, 100.0, 50.0)),
            "the app's new frames stay"
        );
    }
}
