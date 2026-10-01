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
