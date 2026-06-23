//! Split-pane / splitter widget — a two-pane container (horizontal or vertical)
//! holding two arbitrary child `Dom`s with a draggable divider between them that
//! resizes the panes.
//!
//! This is [`crate::widgets::frame::Frame`]'s "two bordered boxes" composed with
//! the pointer-drag state machine of [`crate::widgets::map`] /
//! [`crate::widgets::slider::Slider`]: the drag callbacks live on the **container**
//! (so the cursor stays inside the callback node for the whole drag, exactly like
//! the map's pan), `MouseDown` near the divider begins the drag, `MouseOver` while
//! dragging recomputes the split ratio from the cursor delta and live-resizes the
//! two panes via `set_css_property` (`flex-grow`), and `MouseUp` / `MouseLeave`
//! ends it.
//!
//! ## Layout model
//! The container is a flex row (horizontal split: panes left/right) or column
//! (vertical split: panes top/bottom). Its three children are
//! `[first-pane, divider, second-pane]`. Both panes use `flex-basis: 0` and a
//! `flex-grow` of `ratio` / `1 - ratio`, so they split the container's main-axis
//! space proportionally while the divider keeps its fixed thickness. Dragging
//! rewrites the two `flex-grow` values.
//!
//! ## Drag tracking (mirrors `map::MapTileCache`)
//! The transient drag fields (`is_dragging`, `drag_start_px`, `ratio_at_drag_start`)
//! live in [`SplitPaneStateWrapper`] (not the user-visible [`SplitPaneState`]),
//! the same way the map keeps `drag_anchor` in its cache. On press we record the
//! cursor's main-axis position and the ratio at that moment; each move applies
//! `ratio_at_drag_start + delta / main_size`, so grabbing the divider anywhere
//! keeps it under the cursor (the map's anchor-delta feel).
//!
//! TODO2 / PARTIAL — continuous drag is NOT verifiable in this headless build.
//! Like `map.rs`'s pan, the live resize depends on the runtime delivering
//! `MouseOver` (with a node-relative cursor) repeatedly while the button is held,
//! and on `set_css_property(flex-grow)` triggering a relayout per move — both are
//! GUI-runtime behaviours with no headless test here. The DOM, the divider, the
//! proportional `flex-grow` sizing, and the press/move/release wiring all compile
//! and mirror the proven map/slider pattern exactly; the moment-to-moment motion
//! is the only unverified part. No motion is faked.
//!
//! Key types: [`SplitPane`], [`SplitPaneState`], [`SplitDirection`],
//! [`SplitPaneOnResize`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    geom::{CursorNodePosition, LogicalSize},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, *},
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::CallbackInfo;

static SPLIT_PANE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-split-pane"))];
static SPLIT_PANE_FIRST_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-split-pane-first"))];
static SPLIT_PANE_DIVIDER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-split-pane-divider"))];
static SPLIT_PANE_SECOND_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-split-pane-second"))];

/// Orientation of a [`SplitPane`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum SplitDirection {
    /// Panes side by side (left / right); a vertical divider dragged horizontally.
    #[default]
    Horizontal,
    /// Panes stacked (top / bottom); a horizontal divider dragged vertically.
    Vertical,
}

/// Callback function type invoked when the split ratio changes (during a drag).
pub type SplitPaneOnResizeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, SplitPaneState) -> Update;
impl_widget_callback!(
    SplitPaneOnResize,
    OptionSplitPaneOnResize,
    SplitPaneOnResizeCallback,
    SplitPaneOnResizeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SplitPaneOnResizeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SPLIT_PANE_ON_RESIZE_INVOKER,
    invoker_ty:     AzSplitPaneOnResizeCallbackInvoker,
    thunk_fn:       az_split_pane_on_resize_callback_thunk,
    setter_fn:      AzApp_setSplitPaneOnResizeCallbackInvoker,
    from_handle_fn: AzSplitPaneOnResizeCallback_createFromHostHandle,
    extra_args:     [ state: SplitPaneState ],
}

/// A two-pane resizable container with a draggable divider.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SplitPane {
    pub split_pane_state: SplitPaneStateWrapper,
    /// The first pane's content (left for horizontal, top for vertical).
    pub first: Dom,
    /// The second pane's content (right for horizontal, bottom for vertical).
    pub second: Dom,
    /// Style for the outer container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct SplitPaneStateWrapper {
    /// The user-visible orientation + split ratio.
    pub inner: SplitPaneState,
    /// Optional: function to call when the split ratio changes.
    pub on_resize: OptionSplitPaneOnResize,
    /// `true` while a divider drag is in flight (mirrors `map::MapTileCache::drag_anchor`).
    /// Transient — not part of the user-visible [`SplitPaneState`].
    pub is_dragging: bool,
    /// Cursor main-axis position (relative to the container) at drag start.
    pub drag_start_px: f32,
    /// Split ratio captured at drag start (the anchor for the delta-based update).
    pub ratio_at_drag_start: f32,
}

/// State of a [`SplitPane`]: the orientation and the first pane's size fraction.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct SplitPaneState {
    /// Orientation of the split.
    pub direction: SplitDirection,
    /// Fraction `[0, 1]` of the container's main-axis size taken by the FIRST
    /// pane. Clamped to `[MIN_RATIO, MAX_RATIO]` so a pane never fully collapses.
    pub ratio: f32,
}

impl Default for SplitPaneState {
    fn default() -> Self {
        Self {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
        }
    }
}

// ---- dimensions / limits ----
/// Divider thickness in logical px.
const DIVIDER_THICKNESS: isize = 6;
/// How far (logical px) from the divider centre a press still grabs it.
const GRAB_THRESHOLD: f32 = 9.0;
/// Smallest / largest allowed first-pane fraction (keeps both panes visible).
const MIN_RATIO: f32 = 0.05;
const MAX_RATIO: f32 = 0.95;

// ---- colours ----
/// Divider colour (#adb5bd, mid grey).
const DIVIDER_COLOR: ColorU = ColorU { r: 173, g: 181, b: 189, a: 255 };

const DIVIDER_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(DIVIDER_COLOR)];
const DIVIDER_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(DIVIDER_BG_ITEMS);

/// `flex-grow: v` as a runtime `CssProperty` (floating-point ratio).
fn flex_grow_prop(v: f32) -> CssProperty {
    CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow {
        inner: FloatValue::new(v),
    }))
}

/// The cursor's main-axis (drag-axis) coordinate for the given direction.
fn main_axis(dir: SplitDirection, pos: CursorNodePosition) -> f32 {
    match dir {
        SplitDirection::Horizontal => pos.x,
        SplitDirection::Vertical => pos.y,
    }
}

/// The container's main-axis (drag-axis) size for the given direction.
fn main_size(dir: SplitDirection, size: LogicalSize) -> f32 {
    match dir {
        SplitDirection::Horizontal => size.width,
        SplitDirection::Vertical => size.height,
    }
}

/// Builds the outer-container style: a full-size flex box laid out along the
/// split's main axis. Overridable via [`SplitPane::with_container_style`].
fn container_style(dir: SplitDirection) -> CssPropertyWithConditionsVec {
    let flex_dir = match dir {
        SplitDirection::Horizontal => LayoutFlexDirection::Row,
        SplitDirection::Vertical => LayoutFlexDirection::Column,
    };
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(flex_dir)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::percent(100.0)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight::Px(PixelValue::percent(100.0)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
        CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    ])
}

/// Builds a pane's style: `flex-grow: grow; flex-basis: 0` so the two panes split
/// the container's main-axis space proportionally, plus `overflow: hidden` and
/// `min-width/height: 0` so a shrinking pane clips its content instead of forcing
/// the container wider.
fn pane_style(grow: f32) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(flex_grow_prop(grow)),
        CssPropertyWithConditions::simple(CssProperty::FlexBasis(LayoutFlexBasisValue::Exact(
            LayoutFlexBasis::Exact(PixelValue::const_px(0)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
        CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    ])
}

/// Builds the divider's style: fixed thickness, no grow/shrink, a resize cursor
/// matching the drag axis, and a visible fill. The cross-axis size is left to the
/// flex default (stretch), so the divider spans the container.
fn divider_style(dir: SplitDirection) -> CssPropertyWithConditionsVec {
    let (size_prop, cursor) = match dir {
        SplitDirection::Horizontal => (
            CssProperty::const_width(LayoutWidth::const_px(DIVIDER_THICKNESS)),
            StyleCursor::ColResize,
        ),
        SplitDirection::Vertical => (
            CssProperty::const_height(LayoutHeight::const_px(DIVIDER_THICKNESS)),
            StyleCursor::RowResize,
        ),
    };
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
        CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        CssPropertyWithConditions::simple(size_prop),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(cursor)),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(DIVIDER_BG)),
    ])
}

impl SplitPane {
    /// Creates a split pane with the two child `Dom`s, split 50/50.
    pub fn create(direction: SplitDirection, first: Dom, second: Dom) -> Self {
        Self {
            split_pane_state: SplitPaneStateWrapper {
                inner: SplitPaneState {
                    direction,
                    ratio: 0.5,
                },
                ..Default::default()
            },
            first,
            second,
            container_style: container_style(direction),
        }
    }

    /// Sets the first-pane fraction, clamped into `[MIN_RATIO, MAX_RATIO]`.
    #[inline]
    pub fn set_ratio(&mut self, ratio: f32) {
        self.split_pane_state.inner.ratio = ratio.clamp(MIN_RATIO, MAX_RATIO);
    }

    /// Builder-style setter for the first-pane fraction.
    #[inline]
    pub fn with_ratio(mut self, ratio: f32) -> Self {
        self.set_ratio(ratio);
        self
    }

    /// Sets the orientation (also refreshes the default container style).
    #[inline]
    pub fn set_direction(&mut self, direction: SplitDirection) {
        self.split_pane_state.inner.direction = direction;
        self.container_style = container_style(direction);
    }

    /// Builder-style setter for the orientation.
    #[inline]
    pub fn with_direction(mut self, direction: SplitDirection) -> Self {
        self.set_direction(direction);
        self
    }

    /// Replaces the default container style.
    #[inline]
    pub fn with_container_style(mut self, css: CssPropertyWithConditionsVec) -> Self {
        self.container_style = css;
        self
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(
            SplitDirection::Horizontal,
            Dom::create_div(),
            Dom::create_div(),
        );
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_resize<C: Into<SplitPaneOnResizeCallback>>(&mut self, data: RefAny, on_resize: C) {
        self.split_pane_state.on_resize = Some(SplitPaneOnResize {
            callback: on_resize.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    pub fn with_on_resize<C: Into<SplitPaneOnResizeCallback>>(
        mut self,
        data: RefAny,
        on_resize: C,
    ) -> Self {
        self.set_on_resize(data, on_resize);
        self
    }

    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let direction = self.split_pane_state.inner.direction;
        let ratio = self.split_pane_state.inner.ratio;

        // One shared RefAny across all pointer callbacks so the transient drag
        // fields set on press are visible to the move/release handlers (RefAny::clone
        // shares the underlying data — same pattern as map.rs / slider.rs).
        let state = RefAny::new(self.split_pane_state);
        let mk = |event: EventFilter, cb: usize| CoreCallbackData {
            event,
            callback: CoreCallback {
                cb,
                ctx: OptionRefAny::None,
            },
            refany: state.clone(),
        };
        let callbacks = vec![
            mk(
                EventFilter::Hover(HoverEventFilter::MouseDown),
                on_split_pointer_down as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                on_split_pointer_move as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                on_split_pointer_up as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::MouseLeave),
                on_split_pointer_up as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::TouchStart),
                on_split_pointer_down as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::TouchMove),
                on_split_pointer_move as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::TouchEnd),
                on_split_pointer_up as usize,
            ),
        ];

        // Children: [first-pane, divider, second-pane] — the order the drag
        // handler relies on (first_child = pane0, then divider, then pane1).
        let first_pane = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SPLIT_PANE_FIRST_CLASS))
            .with_css_props(pane_style(ratio))
            .with_children(vec![self.first].into());

        let divider = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SPLIT_PANE_DIVIDER_CLASS))
            .with_css_props(divider_style(direction));

        let second_pane = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SPLIT_PANE_SECOND_CLASS))
            .with_css_props(pane_style(1.0 - ratio))
            .with_children(vec![self.second].into());

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SPLIT_PANE_CLASS))
            .with_css_props(self.container_style)
            .with_callbacks(callbacks.into())
            .with_tab_index(TabIndex::Auto)
            .with_children(vec![first_pane, divider, second_pane].into())
    }
}

impl Default for SplitPane {
    fn default() -> Self {
        Self::create(
            SplitDirection::Horizontal,
            Dom::create_div(),
            Dom::create_div(),
        )
    }
}

/// Pointer down → if the press lands near the divider, begin a drag and record
/// the anchor (cursor position + ratio at this moment). A press elsewhere is left
/// alone so it can reach the pane content.
extern "C" fn on_split_pointer_down(mut data: RefAny, info: CallbackInfo) -> Update {
    let pos = match info.get_cursor_relative_to_node().into_option() {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let size = match info.get_hit_node_rect() {
        Some(r) => r.size,
        None => return Update::DoNothing,
    };
    let mut sp = match data.downcast_mut::<SplitPaneStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    let dir = sp.inner.direction;
    let msize = main_size(dir, size);
    if msize <= 0.0 {
        return Update::DoNothing;
    }
    let main = main_axis(dir, pos);
    let divider_center = sp.inner.ratio * msize;
    if (main - divider_center).abs() <= GRAB_THRESHOLD {
        sp.is_dragging = true;
        sp.drag_start_px = main;
        sp.ratio_at_drag_start = sp.inner.ratio;
    }
    Update::DoNothing
}

/// Pointer move → while dragging, recompute the ratio from the cursor delta and
/// live-resize the two panes' `flex-grow`, then fire the user's `on_resize`.
extern "C" fn on_split_pointer_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut sp = match data.downcast_mut::<SplitPaneStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    if !sp.is_dragging {
        return Update::DoNothing;
    }
    let dir = sp.inner.direction;
    let pos = match info.get_cursor_relative_to_node().into_option() {
        Some(p) => p,
        None => return Update::DoNothing,
    };
    let size = match info.get_hit_node_rect() {
        Some(r) => r.size,
        None => return Update::DoNothing,
    };
    let msize = main_size(dir, size);
    if msize <= 0.0 {
        return Update::DoNothing;
    }
    let main = main_axis(dir, pos);
    let delta = main - sp.drag_start_px;
    let new_ratio = (sp.ratio_at_drag_start + delta / msize).clamp(MIN_RATIO, MAX_RATIO);
    sp.inner.ratio = new_ratio;

    // Resize the two panes. Children are [pane0, divider, pane1]; the callback
    // node (hit node) is the container.
    let container = info.get_hit_node();
    if let Some(pane0) = info.get_first_child(container) {
        info.set_css_property(pane0, flex_grow_prop(new_ratio));
        if let Some(divider) = info.get_next_sibling(pane0) {
            if let Some(pane1) = info.get_next_sibling(divider) {
                info.set_css_property(pane1, flex_grow_prop(1.0 - new_ratio));
            }
        }
    }

    let inner = sp.inner;
    match sp.on_resize.as_mut() {
        Some(SplitPaneOnResize { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
        None => Update::DoNothing,
    }
}

/// Pointer up / leave → end the drag.
extern "C" fn on_split_pointer_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut sp) = data.downcast_mut::<SplitPaneStateWrapper>() {
        sp.is_dragging = false;
    }
    Update::DoNothing
}

impl From<SplitPane> for Dom {
    fn from(s: SplitPane) -> Dom {
        s.dom()
    }
}
