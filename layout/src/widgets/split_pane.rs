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

use crate::solver3::layout_tree::LayoutNodeId;
use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    geom::{CursorNodePosition, LogicalSize},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    impl_option_inner,
    props::{
        basic::{color::ColorU, FloatValue, PixelValue},
        layout::{
            LayoutDisplay, LayoutFlexBasis, LayoutFlexDirection, LayoutFlexGrow, LayoutFlexShrink,
            LayoutHeight, LayoutMinHeight, LayoutMinWidth, LayoutOverflow, LayoutWidth,
        },
        property::{
            CssProperty, LayoutFlexBasisValue, LayoutFlexGrowValue, LayoutHeightValue,
            LayoutWidthValue,
        },
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleCursor},
    },
    AzString,
};

use crate::callbacks::CallbackInfo;

static SPLIT_PANE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-split-pane"))];
static SPLIT_PANE_FIRST_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-split-pane-first",
))];
static SPLIT_PANE_DIVIDER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-split-pane-divider",
))];
static SPLIT_PANE_SECOND_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-split-pane-second",
))];

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
const DIVIDER_COLOR: ColorU = ColorU {
    r: 173,
    g: 181,
    b: 189,
    a: 255,
};

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
const fn main_axis(dir: SplitDirection, pos: CursorNodePosition) -> f32 {
    match dir {
        SplitDirection::Horizontal => pos.x,
        SplitDirection::Vertical => pos.y,
    }
}

/// The container's main-axis (drag-axis) size for the given direction.
const fn main_size(dir: SplitDirection, size: LogicalSize) -> f32 {
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
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            1,
        ))),
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
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_min_height(
            LayoutMinHeight::const_px(0),
        )),
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
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
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
    #[must_use]
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
    pub const fn set_ratio(&mut self, ratio: f32) {
        self.split_pane_state.inner.ratio = ratio.clamp(MIN_RATIO, MAX_RATIO);
    }

    /// Builder-style setter for the first-pane fraction.
    #[inline]
    #[must_use]
    pub const fn with_ratio(mut self, ratio: f32) -> Self {
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
    #[must_use]
    pub fn with_direction(mut self, direction: SplitDirection) -> Self {
        self.set_direction(direction);
        self
    }

    /// Replaces the default container style.
    #[inline]
    #[must_use]
    pub fn with_container_style(mut self, css: CssPropertyWithConditionsVec) -> Self {
        self.container_style = css;
        self
    }

    #[inline]
    #[must_use]
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
    pub fn set_on_resize<C: Into<SplitPaneOnResizeCallback>>(
        &mut self,
        data: RefAny,
        on_resize: C,
    ) {
        self.split_pane_state.on_resize = Some(SplitPaneOnResize {
            callback: on_resize.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use]
    pub fn with_on_resize<C: Into<SplitPaneOnResizeCallback>>(
        mut self,
        data: RefAny,
        on_resize: C,
    ) -> Self {
        self.set_on_resize(data, on_resize);
        self
    }

    #[must_use]
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
            // Role so the accessibility tree knows what this IS:
            // the splitter is a draggable grip. The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Grip,
                ..Default::default()
            })
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
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let size = match info.get_hit_node_rect() {
        Some(r) => r.size,
        None => return Update::DoNothing,
    };
    let Some(mut sp) = data.downcast_mut::<SplitPaneStateWrapper>() else {
        return Update::DoNothing;
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
    let Some(mut sp) = data.downcast_mut::<SplitPaneStateWrapper>() else {
        return Update::DoNothing;
    };
    if !sp.is_dragging {
        return Update::DoNothing;
    }
    let dir = sp.inner.direction;
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
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
    fn from(s: SplitPane) -> Self {
        s.dom()
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::unreadable_literal
)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        string::{String, ToString},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, FormattingContext, HoverEventFilter, NodeId},
        geom::{LogicalPosition, LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{props::basic::length::SizeMetric, system::SystemStyle};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{
            display_list::DisplayList,
            geometry::PackedBoxProps,
            layout_tree::{LayoutNodeHot, LayoutTree},
        },
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ==================================================================
    // Fixtures / probes
    // ==================================================================

    const BOTH_DIRECTIONS: [SplitDirection; 2] =
        [SplitDirection::Horizontal, SplitDirection::Vertical];

    /// Ratios that survive the `[MIN_RATIO, MAX_RATIO]` clamp untouched *and*
    /// round-trip exactly through `FloatValue`'s ×1000 fixed-point encoding.
    const IN_RANGE_RATIOS: [f32; 7] = [0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95];

    fn size(width: f32, height: f32) -> LogicalSize {
        LogicalSize::new(width, height)
    }

    fn cursor(x: f32, y: f32) -> OptionLogicalPosition {
        OptionLogicalPosition::Some(LogicalPosition::new(x, y))
    }

    fn pos(x: f32, y: f32) -> CursorNodePosition {
        CursorNodePosition::new(x, y)
    }

    fn pane(first: Dom, second: Dom) -> SplitPane {
        SplitPane::create(SplitDirection::Horizontal, first, second)
    }

    fn plain(direction: SplitDirection) -> SplitPane {
        SplitPane::create(direction, Dom::create_div(), Dom::create_div())
    }

    fn div_with_class(class: &str) -> Dom {
        Dom::create_div().with_ids_and_classes(vec![Class(AzString::from(class))].into())
    }

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The *kind* of every declared property, in order (values ignored).
    fn kinds(v: &CssPropertyWithConditionsVec) -> Vec<core::mem::Discriminant<CssProperty>> {
        v.as_ref()
            .iter()
            .map(|p| core::mem::discriminant(&p.property))
            .collect()
    }

    fn find<T>(
        v: &CssPropertyWithConditionsVec,
        f: impl Fn(&CssProperty) -> Option<T>,
    ) -> Option<T> {
        v.as_ref().iter().find_map(|p| f(&p.property))
    }

    fn flex_grow_of(p: &CssProperty) -> Option<f32> {
        match p {
            CssProperty::FlexGrow(g) => g.get_property().map(|g| g.inner.get()),
            _ => None,
        }
    }

    /// The raw ×1000 fixed-point encoding behind a `flex-grow` declaration —
    /// the thing that actually saturates, not the `f32` it decodes back to.
    fn flex_grow_raw(p: &CssProperty) -> Option<isize> {
        match p {
            CssProperty::FlexGrow(g) => g.get_property().map(|g| g.inner.number()),
            _ => None,
        }
    }

    fn grow(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, flex_grow_of)
    }

    fn shrink(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::FlexShrink(s) => s.get_property().map(|s| s.inner.get()),
            _ => None,
        })
    }

    fn width_pv(v: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        find(v, |p| match p {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(*pv),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_pv(v: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        find(v, |p| match p {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(*pv),
                _ => None,
            },
            _ => None,
        })
    }

    fn flex_basis_pv(v: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        find(v, |p| match p {
            CssProperty::FlexBasis(b) => match b.get_property() {
                Some(LayoutFlexBasis::Exact(pv)) => Some(*pv),
                _ => None,
            },
            _ => None,
        })
    }

    fn min_width_pv(v: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        find(v, |p| match p {
            CssProperty::MinWidth(w) => w.get_property().map(|w| w.inner),
            _ => None,
        })
    }

    fn min_height_pv(v: &CssPropertyWithConditionsVec) -> Option<PixelValue> {
        find(v, |p| match p {
            CssProperty::MinHeight(h) => h.get_property().map(|h| h.inner),
            _ => None,
        })
    }

    fn display(v: &CssPropertyWithConditionsVec) -> Option<LayoutDisplay> {
        find(v, |p| match p {
            CssProperty::Display(d) => d.get_property().copied(),
            _ => None,
        })
    }

    fn flex_direction(v: &CssPropertyWithConditionsVec) -> Option<LayoutFlexDirection> {
        find(v, |p| match p {
            CssProperty::FlexDirection(d) => d.get_property().copied(),
            _ => None,
        })
    }

    fn cursor_style(v: &CssPropertyWithConditionsVec) -> Option<StyleCursor> {
        find(v, |p| match p {
            CssProperty::Cursor(c) => c.get_property().copied(),
            _ => None,
        })
    }

    fn background_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(b) => match b.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    /// `(overflow-x, overflow-y)` declarations, if any.
    fn overflows(
        v: &CssPropertyWithConditionsVec,
    ) -> (Option<LayoutOverflow>, Option<LayoutOverflow>) {
        (
            find(v, |p| match p {
                CssProperty::OverflowX(o) => o.get_property().copied(),
                _ => None,
            }),
            find(v, |p| match p {
                CssProperty::OverflowY(o) => o.get_property().copied(),
                _ => None,
            }),
        )
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length —
    /// an `em`/`%` slipping into the divider thickness or the pane's zero basis
    /// would resolve against the parent font/box instead of being fixed.
    fn px(pv: PixelValue) -> f32 {
        assert_eq!(pv.metric, SizeMetric::Px, "expected an absolute px length");
        pv.number.get()
    }

    fn percent(pv: PixelValue) -> f32 {
        assert_eq!(pv.metric, SizeMetric::Percent, "expected a percentage");
        pv.number.get()
    }

    // ---- DOM probes ----

    fn dom_classes(d: &Dom) -> Vec<String> {
        d.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The inline (`with_css_props`) declarations of a rendered node, in order.
    fn inline_properties(d: &Dom) -> Vec<CssProperty> {
        d.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    fn inline_grow(d: &Dom) -> Option<f32> {
        inline_properties(d).iter().find_map(flex_grow_of)
    }

    fn child(d: &Dom, idx: usize) -> &Dom {
        &d.children.as_ref()[idx]
    }

    // ---- callback harness ----

    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    fn empty_layout_tree() -> LayoutTree {
        LayoutTree {
            nodes: Vec::new(),
            warm: Vec::new(),
            cold: Vec::new(),
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena: Vec::new(),
            children_offsets: Vec::new(),
            subtree_needs_intrinsic: Vec::new(),
        }
    }

    /// A `DomLayoutResult` over `styled_dom` in which every `(dom node, size)`
    /// pair in `boxes` has a real laid-out box at the origin — enough for
    /// `get_hit_node_rect()` to report a size. An empty `boxes` models the
    /// "callback fired before the first layout" case (no rect at all).
    fn layout_result(styled_dom: StyledDom, boxes: &[(usize, LogicalSize)]) -> DomLayoutResult {
        let mut lr = DomLayoutResult {
            styled_dom,
            layout_tree: empty_layout_tree(),
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        };
        for (layout_index, (node_index, used)) in boxes.iter().enumerate() {
            lr.layout_tree.dom_to_layout.insert(
                NodeId::new(*node_index),
                vec![LayoutNodeId::new(layout_index)],
            );
            lr.layout_tree.nodes.push(LayoutNodeHot {
                box_props: PackedBoxProps::default(),
                dom_node_id: Some(NodeId::new(*node_index)),
                used_size: Some(*used),
                formatting_context: FormattingContext::Flex,
                parent: None,
            });
            lr.calculated_positions.push(LogicalPosition::zero());
        }
        lr
    }

    /// Runs `f` against a real `CallbackInfo` over `styled_dom`, with `hit` as
    /// the hit node and `cur` as the node-relative cursor. Returns `f`'s value
    /// plus every change the callback pushed onto the transaction log.
    fn drive<R>(
        styled_dom: StyledDom,
        boxes: &[(usize, LogicalSize)],
        hit: DomNodeId,
        cur: OptionLogicalPosition,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled_dom, boxes));

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let info = CallbackInfo::new(&ref_data, &changes, hit, cur, OptionLogicalPosition::None);

        let out = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    /// Renders `sp` and hands back both the styled DOM *and* the very `RefAny`
    /// the widget registered on its own pointer callbacks. Driving the handlers
    /// with these two is the real wiring — nothing is rebuilt by hand, so a
    /// mismatch between what `dom()` stores and what the handlers expect cannot
    /// hide behind the fixture.
    fn laid_out(sp: SplitPane) -> (StyledDom, RefAny) {
        let dom = sp.dom();
        let state = dom.root.callbacks.as_ref()[0].refany.clone();
        (StyledDom::create_from_dom(dom), state)
    }

    fn wrapper(state: &mut RefAny) -> SplitPaneStateWrapper {
        let guard = state
            .downcast_ref::<SplitPaneStateWrapper>()
            .expect("the widget state changed type");
        (*guard).clone()
    }

    /// The `(node, flex-grow)` pairs a callback wrote through `set_css_property`.
    fn css_changes(changes: &[CallbackChange]) -> Vec<(NodeId, f32)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id,
                    properties,
                    ..
                } => properties
                    .as_ref()
                    .first()
                    .and_then(flex_grow_of)
                    .map(|g| (*node_id, g)),
                _ => None,
            })
            .collect()
    }

    /// Payload for the user's `on_resize` hook: every state it was handed.
    #[derive(Debug, Default)]
    struct ResizeLog {
        seen: Vec<SplitPaneState>,
    }

    extern "C" fn record_resize(
        mut data: RefAny,
        _info: CallbackInfo,
        state: SplitPaneState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ResizeLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn resize_do_nothing(
        _data: RefAny,
        _info: CallbackInfo,
        _state: SplitPaneState,
    ) -> Update {
        Update::DoNothing
    }

    fn logged(log: &mut RefAny) -> Vec<SplitPaneState> {
        let guard = log.downcast_ref::<ResizeLog>().expect("payload type");
        guard.seen.clone()
    }

    // ==================================================================
    // flex_grow_prop  (numeric)
    // ==================================================================

    #[test]
    fn flex_grow_prop_round_trips_the_representative_ratios() {
        for v in IN_RANGE_RATIOS {
            let got = flex_grow_of(&flex_grow_prop(v)).expect("flex-grow declaration");
            assert!((got - v).abs() < 1e-6, "flex-grow({v}) decoded as {got}");
        }
    }

    #[test]
    fn flex_grow_prop_zero_is_exactly_zero() {
        assert_eq!(flex_grow_raw(&flex_grow_prop(0.0)), Some(0));
        assert_eq!(flex_grow_of(&flex_grow_prop(0.0)), Some(0.0));
        // -0.0 encodes to the same slot (no negative-zero isize).
        assert_eq!(flex_grow_raw(&flex_grow_prop(-0.0)), Some(0));
    }

    #[test]
    fn flex_grow_prop_quantizes_to_three_decimals_by_truncation() {
        // FloatValue stores value * 1000 truncated to an isize.
        assert_eq!(flex_grow_raw(&flex_grow_prop(1.0 / 3.0)), Some(333));
        assert_eq!(flex_grow_raw(&flex_grow_prop(2.0 / 3.0)), Some(666));
        // Sub-precision ratios are indistinguishable from a fully collapsed
        // pane: anything under 0.001 encodes as flex-grow: 0.
        assert_eq!(flex_grow_raw(&flex_grow_prop(0.0009)), Some(0));
        assert_eq!(flex_grow_of(&flex_grow_prop(0.0009)), Some(0.0));
    }

    #[test]
    fn flex_grow_prop_nan_silently_becomes_zero_not_a_panic() {
        // `NaN as isize` saturates to 0, so a NaN ratio does not panic — it
        // renders as `flex-grow: 0`, i.e. a fully collapsed pane.
        assert_eq!(flex_grow_raw(&flex_grow_prop(f32::NAN)), Some(0));
        assert_eq!(flex_grow_of(&flex_grow_prop(f32::NAN)), Some(0.0));
    }

    #[test]
    fn flex_grow_prop_infinities_saturate_to_the_isize_bounds() {
        assert_eq!(
            flex_grow_raw(&flex_grow_prop(f32::INFINITY)),
            Some(isize::MAX)
        );
        assert_eq!(
            flex_grow_raw(&flex_grow_prop(f32::NEG_INFINITY)),
            Some(isize::MIN)
        );
        // ...and decode back to a finite (huge) number, never inf/NaN.
        for v in [f32::INFINITY, f32::NEG_INFINITY] {
            let got = flex_grow_of(&flex_grow_prop(v)).expect("flex-grow declaration");
            assert!(got.is_finite(), "flex-grow({v}) decoded as {got}");
        }
    }

    #[test]
    fn flex_grow_prop_f32_extremes_do_not_panic() {
        for v in [
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            1.0e30,
            -1.0e30,
            f32::EPSILON,
        ] {
            let got = flex_grow_of(&flex_grow_prop(v)).expect("flex-grow declaration");
            assert!(!got.is_nan(), "flex-grow({v}) decoded as NaN");
        }
        // f32::MAX * 1000 overflows to +inf before the cast, so it saturates
        // exactly like +inf does (no wrap, no debug panic).
        assert_eq!(flex_grow_raw(&flex_grow_prop(f32::MAX)), Some(isize::MAX));
        assert_eq!(flex_grow_raw(&flex_grow_prop(f32::MIN)), Some(isize::MIN));
    }

    #[test]
    fn flex_grow_prop_passes_negative_values_through_unclamped() {
        // The primitive does not clamp: clamping is `set_ratio`'s job.
        assert_eq!(flex_grow_raw(&flex_grow_prop(-1.0)), Some(-1000));
        assert_eq!(flex_grow_of(&flex_grow_prop(-1.0)), Some(-1.0));
        assert_eq!(flex_grow_of(&flex_grow_prop(-0.5)), Some(-0.5));
    }

    #[test]
    fn flex_grow_prop_is_deterministic() {
        for v in [0.0, 0.5, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                flex_grow_raw(&flex_grow_prop(v)),
                flex_grow_raw(&flex_grow_prop(v))
            );
        }
    }

    // ==================================================================
    // main_axis  (other)
    // ==================================================================

    #[test]
    fn main_axis_picks_x_for_horizontal_and_y_for_vertical() {
        // Distinct components: an x/y swap cannot hide behind a square input.
        let p = pos(3.0, 7.0);
        assert_eq!(main_axis(SplitDirection::Horizontal, p), 3.0);
        assert_eq!(main_axis(SplitDirection::Vertical, p), 7.0);
    }

    #[test]
    fn main_axis_propagates_nan_and_infinities_verbatim() {
        assert!(main_axis(SplitDirection::Horizontal, pos(f32::NAN, 0.0)).is_nan());
        assert!(main_axis(SplitDirection::Vertical, pos(0.0, f32::NAN)).is_nan());
        assert_eq!(
            main_axis(SplitDirection::Horizontal, pos(f32::INFINITY, 0.0)),
            f32::INFINITY
        );
        assert_eq!(
            main_axis(SplitDirection::Vertical, pos(0.0, f32::NEG_INFINITY)),
            f32::NEG_INFINITY
        );
        // The *other* axis being NaN must not leak into the selected one.
        assert_eq!(
            main_axis(SplitDirection::Horizontal, pos(5.0, f32::NAN)),
            5.0
        );
        assert_eq!(main_axis(SplitDirection::Vertical, pos(f32::NAN, 5.0)), 5.0);
    }

    #[test]
    fn main_axis_extreme_finite_values_are_exact() {
        for v in [0.0, -0.0, f32::MAX, f32::MIN, 1.0e30, -1.0e30, f32::EPSILON] {
            assert_eq!(main_axis(SplitDirection::Horizontal, pos(v, 1.0)), v);
            assert_eq!(main_axis(SplitDirection::Vertical, pos(1.0, v)), v);
        }
    }

    // ==================================================================
    // main_size  (numeric)
    // ==================================================================

    #[test]
    fn main_size_picks_width_for_horizontal_and_height_for_vertical() {
        let s = size(11.0, 22.0);
        assert_eq!(main_size(SplitDirection::Horizontal, s), 11.0);
        assert_eq!(main_size(SplitDirection::Vertical, s), 22.0);
    }

    #[test]
    fn main_size_zero_and_negative_pass_straight_through() {
        // The callers - not this helper - reject non-positive sizes.
        assert_eq!(main_size(SplitDirection::Horizontal, size(0.0, 5.0)), 0.0);
        assert_eq!(main_size(SplitDirection::Vertical, size(5.0, 0.0)), 0.0);
        assert_eq!(
            main_size(SplitDirection::Horizontal, size(-40.0, 5.0)),
            -40.0
        );
        assert_eq!(main_size(SplitDirection::Vertical, size(5.0, -40.0)), -40.0);
    }

    #[test]
    fn main_size_nan_and_infinities_do_not_panic() {
        assert!(main_size(SplitDirection::Horizontal, size(f32::NAN, 1.0)).is_nan());
        assert!(main_size(SplitDirection::Vertical, size(1.0, f32::NAN)).is_nan());
        assert_eq!(
            main_size(SplitDirection::Horizontal, size(f32::INFINITY, 1.0)),
            f32::INFINITY
        );
        assert_eq!(
            main_size(SplitDirection::Vertical, size(1.0, f32::NEG_INFINITY)),
            f32::NEG_INFINITY
        );
    }

    #[test]
    fn main_size_nan_slips_past_the_non_positive_guard_the_callers_use() {
        // Both pointer handlers gate on `msize <= 0.0`. NaN fails that
        // comparison, so a NaN container size is treated as usable and
        // poisons every ratio computed from it (see the pointer_move tests).
        let msize = main_size(SplitDirection::Horizontal, size(f32::NAN, 1.0));
        // Spelled out as a binding so the assertion is `!caught`, not a negated
        // partial-ord comparison — the guard below is verbatim what the handlers use.
        let caught_by_the_guard = msize <= 0.0;
        assert!(
            !caught_by_the_guard,
            "NaN must not be caught by the <= 0 guard"
        );
    }

    #[test]
    fn main_size_extreme_finite_values_are_exact() {
        for v in [f32::MAX, f32::MIN, f32::MIN_POSITIVE, 1.0e30, -1.0e30] {
            assert_eq!(main_size(SplitDirection::Horizontal, size(v, 1.0)), v);
            assert_eq!(main_size(SplitDirection::Vertical, size(1.0, v)), v);
        }
    }

    // ==================================================================
    // container_style  (other)
    // ==================================================================

    #[test]
    fn container_style_declares_a_full_size_flex_box() {
        for dir in BOTH_DIRECTIONS {
            let s = container_style(dir);
            assert_eq!(properties(&s).len(), 7, "{dir:?}");
            assert_eq!(display(&s), Some(LayoutDisplay::Flex), "{dir:?}");
            assert_eq!(grow(&s), Some(1.0), "{dir:?}");
            assert_eq!(percent(width_pv(&s).expect("width")), 100.0, "{dir:?}");
            assert_eq!(percent(height_pv(&s).expect("height")), 100.0, "{dir:?}");
            assert_eq!(
                overflows(&s),
                (Some(LayoutOverflow::Hidden), Some(LayoutOverflow::Hidden)),
                "{dir:?}"
            );
        }
    }

    #[test]
    fn container_style_direction_only_changes_the_flex_direction() {
        let h = container_style(SplitDirection::Horizontal);
        let v = container_style(SplitDirection::Vertical);
        assert_eq!(
            flex_direction(&h),
            Some(LayoutFlexDirection::Row),
            "horizontal splits lay the panes out left/right"
        );
        assert_eq!(
            flex_direction(&v),
            Some(LayoutFlexDirection::Column),
            "vertical splits lay the panes out top/bottom"
        );
        // Same declarations, same order — only the direction value differs.
        assert_eq!(kinds(&h), kinds(&v));
        let (ph, pv) = (properties(&h), properties(&v));
        let differing = ph.iter().zip(pv.iter()).filter(|(a, b)| a != b).count();
        assert_eq!(
            differing, 1,
            "exactly one declaration may depend on the axis"
        );
    }

    #[test]
    fn container_style_is_deterministic() {
        for dir in BOTH_DIRECTIONS {
            assert_eq!(
                properties(&container_style(dir)),
                properties(&container_style(dir))
            );
        }
    }

    // ==================================================================
    // pane_style  (numeric)
    // ==================================================================

    #[test]
    fn pane_style_declares_the_same_six_properties_for_every_grow() {
        let reference = kinds(&pane_style(0.5));
        assert_eq!(reference.len(), 6);
        for g in [
            0.0,
            1.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            1.0e30,
        ] {
            assert_eq!(kinds(&pane_style(g)), reference, "grow = {g}");
        }
    }

    #[test]
    fn pane_style_basis_and_minimums_are_zero_px() {
        let s = pane_style(0.5);
        // flex-basis: 0 is what makes the grow values a *proportion* of the
        // container instead of a share of the leftover space.
        assert_eq!(px(flex_basis_pv(&s).expect("flex-basis")), 0.0);
        assert_eq!(px(min_width_pv(&s).expect("min-width")), 0.0);
        assert_eq!(px(min_height_pv(&s).expect("min-height")), 0.0);
        assert_eq!(
            overflows(&s),
            (Some(LayoutOverflow::Hidden), Some(LayoutOverflow::Hidden))
        );
    }

    #[test]
    fn pane_style_carries_the_grow_through_verbatim() {
        for g in IN_RANGE_RATIOS {
            let got = grow(&pane_style(g)).expect("flex-grow");
            assert!((got - g).abs() < 1e-6, "pane_style({g}) declared {got}");
        }
        assert_eq!(grow(&pane_style(0.0)), Some(0.0));
        assert_eq!(grow(&pane_style(1.0)), Some(1.0));
    }

    #[test]
    fn pane_style_complementary_grows_sum_to_one() {
        // `FloatValue` truncates at 1/1000, so each pane can lose up to 0.001:
        // the pair must still add up to the whole container.
        for r in IN_RANGE_RATIOS {
            let a = grow(&pane_style(r)).expect("first pane grow");
            let b = grow(&pane_style(1.0 - r)).expect("second pane grow");
            assert!(
                (a + b - 1.0).abs() < 3e-3,
                "ratio {r}: {a} + {b} does not fill the container"
            );
        }
    }

    #[test]
    fn pane_style_nan_grow_collapses_the_pane_instead_of_panicking() {
        // A NaN ratio produces NaN for *both* panes (1.0 - NaN is NaN), and
        // both encode as flex-grow: 0 — the split collapses to nothing rather
        // than crashing.
        assert_eq!(grow(&pane_style(f32::NAN)), Some(0.0));
        assert_eq!(grow(&pane_style(1.0 - f32::NAN)), Some(0.0));
    }

    #[test]
    fn pane_style_extreme_grows_do_not_panic() {
        for g in [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            -1.0e30,
        ] {
            let got = grow(&pane_style(g)).expect("flex-grow");
            assert!(got.is_finite(), "pane_style({g}) declared {got}");
        }
    }

    // ==================================================================
    // divider_style  (other)
    // ==================================================================

    #[test]
    fn divider_style_horizontal_is_a_fixed_width_col_resize_bar() {
        let s = divider_style(SplitDirection::Horizontal);
        assert_eq!(px(width_pv(&s).expect("width")), DIVIDER_THICKNESS as f32);
        assert_eq!(cursor_style(&s), Some(StyleCursor::ColResize));
        // The cross axis is deliberately left to the flex default (stretch).
        assert!(height_pv(&s).is_none(), "vertical size must stay unset");
    }

    #[test]
    fn divider_style_vertical_is_a_fixed_height_row_resize_bar() {
        let s = divider_style(SplitDirection::Vertical);
        assert_eq!(px(height_pv(&s).expect("height")), DIVIDER_THICKNESS as f32);
        assert_eq!(cursor_style(&s), Some(StyleCursor::RowResize));
        assert!(width_pv(&s).is_none(), "horizontal size must stay unset");
    }

    #[test]
    fn divider_style_never_grows_or_shrinks_and_is_visible() {
        for dir in BOTH_DIRECTIONS {
            let s = divider_style(dir);
            assert_eq!(properties(&s).len(), 5, "{dir:?}");
            // A grow/shrink of anything but 0 would let the divider eat the
            // panes' space and silently change the split ratio.
            assert_eq!(grow(&s), Some(0.0), "{dir:?}");
            assert_eq!(shrink(&s), Some(0.0), "{dir:?}");
            assert_eq!(background_color(&s), Some(DIVIDER_COLOR), "{dir:?}");
        }
    }

    #[test]
    fn divider_style_axes_never_agree() {
        let h = divider_style(SplitDirection::Horizontal);
        let v = divider_style(SplitDirection::Vertical);
        assert_ne!(cursor_style(&h), cursor_style(&v));
        assert_ne!(properties(&h), properties(&v));
    }

    // ==================================================================
    // SplitPane::create / Default  (constructor)
    // ==================================================================

    #[test]
    fn create_starts_centred_idle_and_hookless() {
        for dir in BOTH_DIRECTIONS {
            let sp = plain(dir);
            assert_eq!(sp.split_pane_state.inner.direction, dir);
            assert_eq!(sp.split_pane_state.inner.ratio, 0.5);
            assert!(!sp.split_pane_state.is_dragging, "{dir:?}");
            assert_eq!(sp.split_pane_state.drag_start_px, 0.0, "{dir:?}");
            assert_eq!(sp.split_pane_state.ratio_at_drag_start, 0.0, "{dir:?}");
            assert!(sp.split_pane_state.on_resize.is_none(), "{dir:?}");
            assert_eq!(
                properties(&sp.container_style),
                properties(&container_style(dir)),
                "{dir:?}"
            );
        }
    }

    #[test]
    fn default_split_pane_matches_a_horizontal_create() {
        let d = SplitPane::default();
        let c = plain(SplitDirection::Horizontal);
        assert_eq!(d.split_pane_state, c.split_pane_state);
        assert_eq!(
            properties(&d.container_style),
            properties(&c.container_style)
        );
    }

    #[test]
    fn create_keeps_the_children_in_first_second_order() {
        let sp = pane(div_with_class("alpha"), div_with_class("beta"));
        assert_eq!(dom_classes(&sp.first), vec!["alpha".to_string()]);
        assert_eq!(dom_classes(&sp.second), vec!["beta".to_string()]);
    }

    // ==================================================================
    // set_ratio / with_ratio  (numeric)
    // ==================================================================

    #[test]
    fn set_ratio_keeps_in_range_values_verbatim() {
        for r in IN_RANGE_RATIOS {
            let mut sp = plain(SplitDirection::Horizontal);
            sp.set_ratio(r);
            assert_eq!(sp.split_pane_state.inner.ratio, r);
        }
    }

    #[test]
    fn set_ratio_clamps_everything_out_of_range_into_min_max() {
        let below = [
            0.0_f32,
            -0.0,
            -1.0,
            0.049_999,
            f32::MIN,
            f32::NEG_INFINITY,
            -1.0e30,
        ];
        let above = [1.0_f32, 2.0, 0.950_001, f32::MAX, f32::INFINITY, 1.0e30];
        for r in below {
            let mut sp = plain(SplitDirection::Horizontal);
            sp.set_ratio(r);
            assert_eq!(
                sp.split_pane_state.inner.ratio, MIN_RATIO,
                "{r} must clamp up to MIN_RATIO"
            );
        }
        for r in above {
            let mut sp = plain(SplitDirection::Horizontal);
            sp.set_ratio(r);
            assert_eq!(
                sp.split_pane_state.inner.ratio, MAX_RATIO,
                "{r} must clamp down to MAX_RATIO"
            );
        }
    }

    #[test]
    fn set_ratio_nan_escapes_the_documented_clamp() {
        // PIN + KNOWN DEFECT: `f32::clamp` returns NaN for a NaN input, so a
        // NaN ratio is stored verbatim even though the doc comment promises
        // `[MIN_RATIO, MAX_RATIO]`. Downstream it renders as flex-grow: 0 on
        // BOTH panes (see `dom_with_a_nan_ratio_collapses_both_panes`), i.e. an
        // empty split pane rather than a panic. Flip this test loudly if the
        // clamp is ever made NaN-safe.
        let mut sp = plain(SplitDirection::Horizontal);
        sp.set_ratio(f32::NAN);
        assert!(
            sp.split_pane_state.inner.ratio.is_nan(),
            "NaN is currently stored as-is"
        );
    }

    #[test]
    fn set_ratio_is_idempotent_and_a_projection() {
        for r in [-5.0_f32, 0.0, 0.3, 0.5, 0.95, 7.0, f32::INFINITY] {
            let mut once = plain(SplitDirection::Horizontal);
            once.set_ratio(r);
            let settled = once.split_pane_state.inner.ratio;
            once.set_ratio(settled);
            assert_eq!(once.split_pane_state.inner.ratio, settled, "input {r}");
            assert!(
                (MIN_RATIO..=MAX_RATIO).contains(&settled),
                "input {r} settled outside the documented range at {settled}"
            );
        }
    }

    #[test]
    fn set_ratio_touches_nothing_but_the_ratio() {
        let mut sp = plain(SplitDirection::Vertical);
        let before = properties(&sp.container_style);
        sp.set_ratio(0.2);
        assert_eq!(
            sp.split_pane_state.inner.direction,
            SplitDirection::Vertical
        );
        assert!(!sp.split_pane_state.is_dragging);
        assert_eq!(properties(&sp.container_style), before);
    }

    #[test]
    fn with_ratio_agrees_with_set_ratio_on_every_input() {
        for r in [
            -1.0_f32,
            0.0,
            0.05,
            0.5,
            0.95,
            1.0,
            f32::MAX,
            f32::NEG_INFINITY,
        ] {
            let built = plain(SplitDirection::Horizontal).with_ratio(r);
            let mut mutated = plain(SplitDirection::Horizontal);
            mutated.set_ratio(r);
            assert_eq!(
                built.split_pane_state.inner.ratio, mutated.split_pane_state.inner.ratio,
                "input {r}"
            );
        }
    }

    #[test]
    fn with_ratio_preserves_the_rest_of_the_widget() {
        let sp = SplitPane::create(
            SplitDirection::Vertical,
            div_with_class("alpha"),
            div_with_class("beta"),
        )
        .with_ratio(0.25);
        assert_eq!(sp.split_pane_state.inner.ratio, 0.25);
        assert_eq!(
            sp.split_pane_state.inner.direction,
            SplitDirection::Vertical
        );
        assert_eq!(dom_classes(&sp.first), vec!["alpha".to_string()]);
        assert_eq!(dom_classes(&sp.second), vec!["beta".to_string()]);
        assert_eq!(
            properties(&sp.container_style),
            properties(&container_style(SplitDirection::Vertical))
        );
    }

    // ==================================================================
    // set_direction / with_direction  (other / constructor)
    // ==================================================================

    #[test]
    fn set_direction_updates_both_the_state_and_the_container_style() {
        let mut sp = plain(SplitDirection::Horizontal);
        sp.set_direction(SplitDirection::Vertical);
        assert_eq!(
            sp.split_pane_state.inner.direction,
            SplitDirection::Vertical
        );
        assert_eq!(
            flex_direction(&sp.container_style),
            Some(LayoutFlexDirection::Column),
            "a stale Row here would lay a vertical split out sideways"
        );
    }

    #[test]
    fn set_direction_is_idempotent_and_round_trips() {
        let mut sp = plain(SplitDirection::Horizontal);
        let original = properties(&sp.container_style);
        sp.set_direction(SplitDirection::Vertical);
        sp.set_direction(SplitDirection::Vertical);
        assert_eq!(
            properties(&sp.container_style),
            properties(&container_style(SplitDirection::Vertical))
        );
        sp.set_direction(SplitDirection::Horizontal);
        assert_eq!(properties(&sp.container_style), original);
    }

    #[test]
    fn set_direction_discards_a_custom_container_style() {
        // PIN: `with_container_style` then `set_direction` silently throws the
        // custom style away — the two builders are order-dependent.
        let sp = plain(SplitDirection::Horizontal)
            .with_container_style(CssPropertyWithConditionsVec::from_vec(vec![]))
            .with_direction(SplitDirection::Vertical);
        assert_eq!(
            properties(&sp.container_style),
            properties(&container_style(SplitDirection::Vertical)),
            "set_direction overwrites, it does not merge"
        );
    }

    #[test]
    fn with_direction_leaves_the_ratio_and_children_alone() {
        let sp = SplitPane::create(
            SplitDirection::Horizontal,
            div_with_class("alpha"),
            div_with_class("beta"),
        )
        .with_ratio(0.3)
        .with_direction(SplitDirection::Vertical);
        assert_eq!(sp.split_pane_state.inner.ratio, 0.3);
        assert_eq!(dom_classes(&sp.first), vec!["alpha".to_string()]);
        assert_eq!(dom_classes(&sp.second), vec!["beta".to_string()]);
    }

    // ==================================================================
    // with_container_style  (constructor)
    // ==================================================================

    #[test]
    fn with_container_style_replaces_the_default_verbatim() {
        let custom =
            CssPropertyWithConditionsVec::from_vec(vec![CssPropertyWithConditions::simple(
                CssProperty::const_display(LayoutDisplay::Block),
            )]);
        let sp = plain(SplitDirection::Horizontal).with_container_style(custom.clone());
        assert_eq!(properties(&sp.container_style), properties(&custom));
        assert_eq!(display(&sp.container_style), Some(LayoutDisplay::Block));
    }

    #[test]
    fn with_container_style_accepts_an_empty_vec() {
        let sp = plain(SplitDirection::Horizontal)
            .with_container_style(CssPropertyWithConditionsVec::from_vec(vec![]));
        assert!(properties(&sp.container_style).is_empty());
        // The state is untouched, and rendering an unstyled container must not
        // panic even though the flex layout is gone.
        assert_eq!(sp.split_pane_state.inner.ratio, 0.5);
        let dom = sp.dom();
        assert_eq!(dom.children.as_ref().len(), 3);
    }

    // ==================================================================
    // swap_with_default  (other)
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_old_value_and_leaves_a_default() {
        let mut sp = SplitPane::create(
            SplitDirection::Vertical,
            div_with_class("alpha"),
            div_with_class("beta"),
        )
        .with_ratio(0.8);
        let old = sp.swap_with_default();

        assert_eq!(old.split_pane_state.inner.ratio, 0.8);
        assert_eq!(
            old.split_pane_state.inner.direction,
            SplitDirection::Vertical
        );
        assert_eq!(dom_classes(&old.first), vec!["alpha".to_string()]);
        assert_eq!(dom_classes(&old.second), vec!["beta".to_string()]);

        assert_eq!(sp.split_pane_state, SplitPaneStateWrapper::default());
        assert!(dom_classes(&sp.first).is_empty());
        assert_eq!(
            properties(&sp.container_style),
            properties(&container_style(SplitDirection::Horizontal))
        );
    }

    #[test]
    fn swap_with_default_moves_the_on_resize_hook_out() {
        let log = RefAny::new(ResizeLog::default());
        let mut sp = plain(SplitDirection::Horizontal)
            .with_on_resize(log, record_resize as SplitPaneOnResizeCallbackType);
        let old = sp.swap_with_default();
        assert!(old.split_pane_state.on_resize.is_some());
        assert!(sp.split_pane_state.on_resize.is_none());
    }

    #[test]
    fn swap_with_default_twice_is_stable() {
        let mut sp = plain(SplitDirection::Vertical).with_ratio(0.1);
        let _first = sp.swap_with_default();
        let second = sp.swap_with_default();
        assert_eq!(second.split_pane_state, SplitPaneStateWrapper::default());
        assert_eq!(sp.split_pane_state, SplitPaneStateWrapper::default());
    }

    // ==================================================================
    // set_on_resize / with_on_resize  (other / constructor)
    // ==================================================================

    #[test]
    fn set_on_resize_installs_the_hook_and_keeps_the_payload() {
        let mut log = RefAny::new(ResizeLog::default());
        let mut sp = plain(SplitDirection::Horizontal);
        sp.set_on_resize(log.clone(), record_resize as SplitPaneOnResizeCallbackType);
        let hook = sp.split_pane_state.on_resize.as_ref().expect("hook");
        assert_eq!(hook.callback.cb as usize, record_resize as usize);
        // The payload is shared, not copied: the widget holds the same data.
        assert!(logged(&mut log).is_empty());
    }

    #[test]
    fn set_on_resize_replaces_a_previous_hook() {
        let mut sp = plain(SplitDirection::Horizontal);
        sp.set_on_resize(
            RefAny::new(ResizeLog::default()),
            record_resize as SplitPaneOnResizeCallbackType,
        );
        sp.set_on_resize(
            RefAny::new(ResizeLog::default()),
            resize_do_nothing as SplitPaneOnResizeCallbackType,
        );
        let hook = sp.split_pane_state.on_resize.as_ref().expect("hook");
        assert_eq!(hook.callback.cb as usize, resize_do_nothing as usize);
    }

    #[test]
    fn with_on_resize_matches_set_on_resize() {
        let built = plain(SplitDirection::Horizontal).with_on_resize(
            RefAny::new(ResizeLog::default()),
            record_resize as SplitPaneOnResizeCallbackType,
        );
        let mut mutated = plain(SplitDirection::Horizontal);
        mutated.set_on_resize(
            RefAny::new(ResizeLog::default()),
            record_resize as SplitPaneOnResizeCallbackType,
        );
        assert_eq!(
            built
                .split_pane_state
                .on_resize
                .as_ref()
                .map(|h| h.callback.cb as usize),
            mutated
                .split_pane_state
                .on_resize
                .as_ref()
                .map(|h| h.callback.cb as usize),
        );
        // ...and it changes nothing else.
        assert_eq!(built.split_pane_state.inner, mutated.split_pane_state.inner);
    }

    // ==================================================================
    // SplitPane::dom  (other)
    // ==================================================================

    #[test]
    fn dom_is_first_pane_divider_second_pane_in_that_order() {
        let dom = plain(SplitDirection::Horizontal).dom();
        assert_eq!(
            dom_classes(&dom),
            vec!["__azul-native-split-pane".to_string()]
        );
        assert_eq!(dom.root.get_tab_index(), Some(TabIndex::Auto));
        let children = dom.children.as_ref();
        assert_eq!(children.len(), 3);
        assert_eq!(
            dom_classes(&children[0]),
            vec!["__azul-native-split-pane-first".to_string()]
        );
        assert_eq!(
            dom_classes(&children[1]),
            vec!["__azul-native-split-pane-divider".to_string()]
        );
        assert_eq!(
            dom_classes(&children[2]),
            vec!["__azul-native-split-pane-second".to_string()]
        );
    }

    #[test]
    fn dom_wraps_the_user_children_one_per_pane() {
        let dom = pane(div_with_class("alpha"), div_with_class("beta")).dom();
        let first = child(&dom, 0);
        let second = child(&dom, 2);
        assert_eq!(first.children.as_ref().len(), 1);
        assert_eq!(second.children.as_ref().len(), 1);
        assert_eq!(dom_classes(child(first, 0)), vec!["alpha".to_string()]);
        assert_eq!(dom_classes(child(second, 0)), vec!["beta".to_string()]);
        // The divider is a leaf: anything inside it would sit under the cursor
        // during a drag.
        assert!(child(&dom, 1).children.as_ref().is_empty());
    }

    #[test]
    fn dom_pane_grows_are_complementary_for_every_ratio() {
        for r in IN_RANGE_RATIOS {
            let dom = plain(SplitDirection::Horizontal).with_ratio(r).dom();
            let a = inline_grow(child(&dom, 0)).expect("first pane grow");
            let b = inline_grow(child(&dom, 2)).expect("second pane grow");
            assert!((a - r).abs() < 2e-3, "ratio {r}: first pane got {a}");
            assert!((a + b - 1.0).abs() < 3e-3, "ratio {r}: {a} + {b} != 1");
        }
    }

    #[test]
    fn dom_clamped_extremes_still_leave_both_panes_visible() {
        for r in [-100.0_f32, 0.0, 1.0, f32::INFINITY] {
            let dom = plain(SplitDirection::Horizontal).with_ratio(r).dom();
            let a = inline_grow(child(&dom, 0)).expect("first pane grow");
            let b = inline_grow(child(&dom, 2)).expect("second pane grow");
            assert!(a > 0.0 && b > 0.0, "ratio {r} collapsed a pane ({a}, {b})");
            assert!((a + b - 1.0).abs() < 3e-3, "ratio {r}: {a} + {b} != 1");
        }
    }

    #[test]
    fn dom_with_a_nan_ratio_collapses_both_panes() {
        // Consequence of `set_ratio_nan_escapes_the_documented_clamp`: NaN
        // survives into the layout and both panes render at flex-grow: 0.
        let dom = plain(SplitDirection::Horizontal).with_ratio(f32::NAN).dom();
        assert_eq!(inline_grow(child(&dom, 0)), Some(0.0));
        assert_eq!(inline_grow(child(&dom, 2)), Some(0.0));
    }

    #[test]
    fn dom_divider_matches_divider_style_for_the_direction() {
        for dir in BOTH_DIRECTIONS {
            let dom = plain(dir).dom();
            assert_eq!(
                inline_properties(child(&dom, 1)),
                properties(&divider_style(dir)),
                "{dir:?}"
            );
        }
    }

    #[test]
    fn dom_container_keeps_the_configured_style() {
        for dir in BOTH_DIRECTIONS {
            let dom = plain(dir).dom();
            assert_eq!(
                inline_properties(&dom),
                properties(&container_style(dir)),
                "{dir:?}"
            );
        }
    }

    #[test]
    fn dom_registers_every_pointer_event_on_the_container() {
        let dom = plain(SplitDirection::Horizontal).dom();
        let wired: Vec<(EventFilter, usize)> = dom
            .root
            .callbacks
            .as_ref()
            .iter()
            .map(|c| (c.event, c.callback.cb))
            .collect();
        let expected: Vec<(EventFilter, usize)> = vec![
            (
                EventFilter::Hover(HoverEventFilter::MouseDown),
                on_split_pointer_down as usize,
            ),
            (
                EventFilter::Hover(HoverEventFilter::MouseOver),
                on_split_pointer_move as usize,
            ),
            (
                EventFilter::Hover(HoverEventFilter::MouseUp),
                on_split_pointer_up as usize,
            ),
            (
                EventFilter::Hover(HoverEventFilter::MouseLeave),
                on_split_pointer_up as usize,
            ),
            (
                EventFilter::Hover(HoverEventFilter::TouchStart),
                on_split_pointer_down as usize,
            ),
            (
                EventFilter::Hover(HoverEventFilter::TouchMove),
                on_split_pointer_move as usize,
            ),
            (
                EventFilter::Hover(HoverEventFilter::TouchEnd),
                on_split_pointer_up as usize,
            ),
        ];
        assert_eq!(wired, expected);
        // The drag lives on the container, never on the divider or the panes -
        // otherwise the cursor would leave the callback node mid-drag.
        for i in 0..3 {
            assert!(
                child(&dom, i).root.callbacks.as_ref().is_empty(),
                "child {i} must not carry pointer callbacks"
            );
        }
    }

    #[test]
    fn dom_shares_one_state_refany_across_all_callbacks() {
        let dom = plain(SplitDirection::Horizontal).dom();
        let mut first = dom.root.callbacks.as_ref()[0].refany.clone();
        let mut last = dom.root.callbacks.as_ref()[6].refany.clone();
        {
            let mut sp = first
                .downcast_mut::<SplitPaneStateWrapper>()
                .expect("state type");
            sp.is_dragging = true;
            sp.drag_start_px = 42.0;
        }
        let seen = wrapper(&mut last);
        assert!(
            seen.is_dragging && seen.drag_start_px == 42.0,
            "press/move/release must observe one shared drag state"
        );
    }

    #[test]
    fn dom_carries_the_state_into_the_callback_payload() {
        let mut sp = plain(SplitDirection::Vertical).with_ratio(0.25);
        sp.set_on_resize(
            RefAny::new(ResizeLog::default()),
            record_resize as SplitPaneOnResizeCallbackType,
        );
        let (_sd, mut state) = laid_out(sp);
        let w = wrapper(&mut state);
        assert_eq!(w.inner.direction, SplitDirection::Vertical);
        assert_eq!(w.inner.ratio, 0.25);
        assert!(!w.is_dragging);
        assert!(w.on_resize.is_some());
    }

    #[test]
    fn dom_of_deeply_nested_children_does_not_panic() {
        // A pathological child tree must survive the flatten that `dom()`
        // feeds into `StyledDom`.
        let mut deep = Dom::create_div();
        for _ in 0..200 {
            deep = Dom::create_div().with_children(vec![deep].into());
        }
        let (sd, _state) = laid_out(pane(deep, Dom::create_div()));
        assert!(sd.node_count() > 200);
    }

    #[test]
    fn from_split_pane_for_dom_matches_dom() {
        let via_into: Dom = plain(SplitDirection::Vertical).with_ratio(0.3).into();
        let direct = plain(SplitDirection::Vertical).with_ratio(0.3).dom();
        assert_eq!(dom_classes(&via_into), dom_classes(&direct));
        assert_eq!(
            via_into.children.as_ref().len(),
            direct.children.as_ref().len()
        );
        assert_eq!(
            inline_grow(child(&via_into, 0)),
            inline_grow(child(&direct, 0))
        );
    }

    // ==================================================================
    // on_split_pointer_down  (other)
    // ==================================================================

    #[test]
    fn pointer_down_without_a_cursor_is_a_no_op() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let (update, changes) = drive(
            sd,
            &[(0, size(200.0, 100.0))],
            node(0),
            OptionLogicalPosition::None,
            |info| on_split_pointer_down(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        let mut state = state;
        assert!(!wrapper(&mut state).is_dragging);
    }

    #[test]
    fn pointer_down_without_a_laid_out_rect_is_a_no_op() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let (update, changes) = drive(sd, &[], node(0), cursor(100.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        let mut state = state;
        assert!(!wrapper(&mut state).is_dragging);
    }

    #[test]
    fn pointer_down_on_a_zero_sized_container_is_a_no_op() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let (update, _) = drive(
            sd,
            &[(0, size(0.0, 0.0))],
            node(0),
            cursor(0.0, 0.0),
            |info| on_split_pointer_down(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        let mut state = state;
        assert!(
            !wrapper(&mut state).is_dragging,
            "a 0-wide container has no divider to grab"
        );
    }

    #[test]
    fn pointer_down_on_a_negative_sized_container_is_a_no_op() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let (update, _) = drive(
            sd,
            &[(0, size(-200.0, -100.0))],
            node(0),
            cursor(-100.0, -50.0),
            |info| on_split_pointer_down(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        let mut state = state;
        assert!(!wrapper(&mut state).is_dragging);
    }

    #[test]
    fn pointer_down_records_the_anchor_when_it_lands_on_the_divider() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal).with_ratio(0.25));
        // 200px wide, ratio 0.25 -> the grab zone is centred on x = 50.
        let (update, changes) = drive(
            sd,
            &[(0, size(200.0, 100.0))],
            node(0),
            cursor(52.0, 10.0),
            |info| on_split_pointer_down(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing, "the press itself never redraws");
        assert!(changes.is_empty(), "the press must not touch the DOM");
        let mut state = state;
        let w = wrapper(&mut state);
        assert!(w.is_dragging);
        assert_eq!(w.drag_start_px, 52.0);
        assert_eq!(w.ratio_at_drag_start, 0.25);
        assert_eq!(w.inner.ratio, 0.25, "the press must not move the divider");
    }

    #[test]
    fn pointer_down_far_from_the_divider_leaves_the_press_to_the_pane() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let (update, _) = drive(
            sd,
            &[(0, size(200.0, 100.0))],
            node(0),
            cursor(10.0, 50.0),
            |info| on_split_pointer_down(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        let mut state = state;
        assert!(!wrapper(&mut state).is_dragging);
    }

    #[test]
    fn pointer_down_grabs_exactly_within_the_threshold() {
        // 200px, ratio 0.5 -> divider centre at 100. GRAB_THRESHOLD is 9.0 and
        // the comparison is inclusive.
        for (x, expected) in [
            (100.0_f32, true),
            (109.0, true),
            (91.0, true),
            (109.5, false),
            (90.5, false),
            (0.0, false),
            (200.0, false),
        ] {
            let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
            let (_, _) = drive(
                sd,
                &[(0, size(200.0, 100.0))],
                node(0),
                cursor(x, 50.0),
                |info| on_split_pointer_down(state.clone(), info),
            );
            let mut state = state;
            assert_eq!(
                wrapper(&mut state).is_dragging,
                expected,
                "press at x = {x} (|{x} - 100| vs {GRAB_THRESHOLD})"
            );
        }
    }

    #[test]
    fn pointer_down_uses_the_axis_that_matches_the_direction() {
        // 200x100. Horizontal: centre x = 100. Vertical: centre y = 50.
        // The same cursor must grab in one orientation and miss in the other.
        for (dir, cur, expected) in [
            (SplitDirection::Horizontal, (100.0, 5.0), true),
            (SplitDirection::Vertical, (100.0, 5.0), false),
            (SplitDirection::Horizontal, (5.0, 50.0), false),
            (SplitDirection::Vertical, (5.0, 50.0), true),
        ] {
            let (sd, state) = laid_out(plain(dir));
            let (_, _) = drive(
                sd,
                &[(0, size(200.0, 100.0))],
                node(0),
                cursor(cur.0, cur.1),
                |info| on_split_pointer_down(state.clone(), info),
            );
            let mut state = state;
            assert_eq!(
                wrapper(&mut state).is_dragging,
                expected,
                "{dir:?} press at {cur:?}"
            );
        }
    }

    #[test]
    fn pointer_down_with_a_nan_cursor_or_size_never_grabs() {
        for (s, cur) in [
            (size(200.0, 100.0), (f32::NAN, 50.0)),
            (size(f32::NAN, 100.0), (100.0, 50.0)),
            (size(f32::INFINITY, 100.0), (100.0, 50.0)),
            (size(200.0, 100.0), (f32::INFINITY, 50.0)),
        ] {
            let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
            let (update, _) = drive(sd, &[(0, s)], node(0), cursor(cur.0, cur.1), |info| {
                on_split_pointer_down(state.clone(), info)
            });
            assert_eq!(update, Update::DoNothing);
            let mut state = state;
            assert!(
                !wrapper(&mut state).is_dragging,
                "a non-finite comparison must fail closed, not grab"
            );
        }
    }

    #[test]
    fn pointer_down_with_a_wrong_typed_payload_is_a_no_op() {
        let (sd, _state) = laid_out(plain(SplitDirection::Horizontal));
        let stranger = RefAny::new(0u16);
        let (update, changes) = drive(
            sd,
            &[(0, size(200.0, 100.0))],
            node(0),
            cursor(100.0, 50.0),
            |info| on_split_pointer_down(stranger.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn pointer_down_re_anchors_on_every_press() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let boxes = [(0, size(200.0, 100.0))];
        for x in [95.0_f32, 104.0] {
            let sd_clone = sd.clone();
            let (_, _) = drive(sd_clone, &boxes, node(0), cursor(x, 50.0), |info| {
                on_split_pointer_down(state.clone(), info)
            });
        }
        let mut state = state;
        let w = wrapper(&mut state);
        assert!(w.is_dragging);
        assert_eq!(w.drag_start_px, 104.0, "the newest press wins");
    }

    // ==================================================================
    // on_split_pointer_move  (other)
    // ==================================================================

    /// Presses at `press` (which must land on the divider), then moves to
    /// `to`. Returns the move's `Update` plus the CSS writes it made.
    fn press_then_move(
        sp: SplitPane,
        boxes: &[(usize, LogicalSize)],
        press: (f32, f32),
        to: (f32, f32),
    ) -> (Update, Vec<CallbackChange>, RefAny) {
        let (sd, state) = laid_out(sp);
        let down_sd = sd.clone();
        let (_, _) = drive(down_sd, boxes, node(0), cursor(press.0, press.1), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        let (update, changes) = drive(sd, boxes, node(0), cursor(to.0, to.1), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        (update, changes, state)
    }

    #[test]
    fn pointer_move_without_a_drag_changes_nothing() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let (update, changes) = drive(
            sd,
            &[(0, size(200.0, 100.0))],
            node(0),
            cursor(180.0, 50.0),
            |info| on_split_pointer_move(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a hover must not resize the panes");
        let mut state = state;
        assert_eq!(wrapper(&mut state).inner.ratio, 0.5);
    }

    #[test]
    fn pointer_move_applies_the_cursor_delta_and_resizes_both_panes() {
        let boxes = [(0, size(200.0, 100.0))];
        let (update, changes, mut state) = press_then_move(
            plain(SplitDirection::Horizontal),
            &boxes,
            (100.0, 50.0),
            (150.0, 50.0),
        );
        // +50px over a 200px container = +0.25 on the anchor ratio of 0.5.
        assert_eq!(wrapper(&mut state).inner.ratio, 0.75);
        assert_eq!(update, Update::DoNothing, "no hook installed");

        let writes = css_changes(&changes);
        assert_eq!(writes.len(), 2, "exactly one flex-grow per pane");
        assert_eq!(writes[0].1, 0.75);
        assert_eq!(writes[1].1, 0.25);
        assert_ne!(
            writes[0].0, writes[1].0,
            "the two panes must be distinct nodes"
        );
    }

    #[test]
    fn pointer_move_writes_to_the_first_and_third_children() {
        // The handler walks first_child -> next_sibling -> next_sibling, so
        // the pane/divider/pane order in `dom()` is load-bearing.
        let boxes = [(0, size(200.0, 100.0))];
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let classes: Vec<Vec<String>> = sd
            .node_data
            .as_ref()
            .iter()
            .map(|n| {
                n.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .filter_map(|c| match c {
                        Class(s) => Some(s.as_str().to_string()),
                        IdOrClass::Id(_) => None,
                    })
                    .collect()
            })
            .collect();
        let down_sd = sd.clone();
        let (_, _) = drive(down_sd, &boxes, node(0), cursor(100.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        let (_, changes) = drive(sd, &boxes, node(0), cursor(150.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        let writes = css_changes(&changes);
        assert_eq!(writes.len(), 2);
        assert_eq!(
            classes[writes[0].0.index()],
            vec!["__azul-native-split-pane-first".to_string()],
            "the larger grow must land on the first pane"
        );
        assert_eq!(
            classes[writes[1].0.index()],
            vec!["__azul-native-split-pane-second".to_string()]
        );
    }

    #[test]
    fn pointer_move_clamps_at_both_ends_and_never_collapses_a_pane() {
        let boxes = [(0, size(200.0, 100.0))];
        for (to_x, expected) in [(-10_000.0_f32, MIN_RATIO), (10_000.0, MAX_RATIO)] {
            let (_, changes, mut state) = press_then_move(
                plain(SplitDirection::Horizontal),
                &boxes,
                (100.0, 50.0),
                (to_x, 50.0),
            );
            assert_eq!(wrapper(&mut state).inner.ratio, expected, "moved to {to_x}");
            let writes = css_changes(&changes);
            assert_eq!(writes.len(), 2);
            assert!(writes[0].1 > 0.0 && writes[1].1 > 0.0, "moved to {to_x}");
            assert!(
                (writes[0].1 + writes[1].1 - 1.0).abs() < 3e-3,
                "moved to {to_x}"
            );
        }
    }

    #[test]
    fn pointer_move_grows_always_sum_to_one() {
        let boxes = [(0, size(400.0, 100.0))];
        for to_x in [0.0_f32, 40.0, 133.0, 200.0, 267.0, 360.0, 400.0] {
            let (_, changes, _) = press_then_move(
                plain(SplitDirection::Horizontal),
                &boxes,
                (200.0, 50.0),
                (to_x, 50.0),
            );
            let writes = css_changes(&changes);
            assert_eq!(writes.len(), 2, "moved to {to_x}");
            // Budget: the ×1000 truncation can shave up to 0.001 off each pane.
            assert!(
                (writes[0].1 + writes[1].1 - 1.0).abs() < 3e-3,
                "moved to {to_x}: {} + {} != 1",
                writes[0].1,
                writes[1].1
            );
        }
    }

    #[test]
    fn pointer_move_is_anchor_relative_not_cursor_absolute() {
        // Grabbing the divider off-centre must not teleport it under the
        // cursor: the ratio moves by the *delta*, from the ratio at press.
        let boxes = [(0, size(200.0, 100.0))];
        let (_, _, mut state) = press_then_move(
            plain(SplitDirection::Horizontal),
            &boxes,
            (108.0, 50.0),
            (128.0, 50.0),
        );
        // press at 108 (within 9 of the 100 centre), moved +20 over 200px.
        let r = wrapper(&mut state).inner.ratio;
        assert!((r - 0.6).abs() < 1e-6, "expected 0.5 + 20/200, got {r}");
    }

    #[test]
    fn pointer_move_back_to_the_press_point_restores_the_ratio() {
        let boxes = [(0, size(200.0, 100.0))];
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal).with_ratio(0.4));
        // ratio 0.4 over 200px -> the grab zone is centred on x = 80.
        let a = sd.clone();
        let (_, _) = drive(a, &boxes, node(0), cursor(80.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        let b = sd.clone();
        let (_, _) = drive(b, &boxes, node(0), cursor(160.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        let (_, changes) = drive(sd, &boxes, node(0), cursor(80.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        let mut state = state;
        assert_eq!(
            wrapper(&mut state).inner.ratio,
            0.4,
            "the drag is not cumulative"
        );
        assert_eq!(css_changes(&changes)[0].1, 0.4);
    }

    #[test]
    fn pointer_move_uses_the_axis_that_matches_the_direction() {
        let boxes = [(0, size(200.0, 100.0))];
        // Vertical: centre y = 50, main size = 100. +25px = +0.25.
        let (_, _, mut state) = press_then_move(
            plain(SplitDirection::Vertical),
            &boxes,
            (10.0, 50.0),
            (999.0, 75.0),
        );
        assert_eq!(
            wrapper(&mut state).inner.ratio,
            0.75,
            "a vertical split must ignore horizontal cursor motion"
        );
    }

    #[test]
    fn pointer_move_fires_the_hook_with_the_new_state_and_returns_its_update() {
        let boxes = [(0, size(200.0, 100.0))];
        let mut log = RefAny::new(ResizeLog::default());
        let sp = plain(SplitDirection::Horizontal)
            .with_on_resize(log.clone(), record_resize as SplitPaneOnResizeCallbackType);
        let (update, _, _) = press_then_move(sp, &boxes, (100.0, 50.0), (150.0, 50.0));
        assert_eq!(
            update,
            Update::RefreshDom,
            "the hook's Update is returned verbatim"
        );
        let seen = logged(&mut log);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].ratio, 0.75);
        assert_eq!(seen[0].direction, SplitDirection::Horizontal);
    }

    #[test]
    fn pointer_move_returns_the_hooks_do_nothing_too() {
        let boxes = [(0, size(200.0, 100.0))];
        let sp = plain(SplitDirection::Horizontal).with_on_resize(
            RefAny::new(ResizeLog::default()),
            resize_do_nothing as SplitPaneOnResizeCallbackType,
        );
        let (update, changes, _) = press_then_move(sp, &boxes, (100.0, 50.0), (150.0, 50.0));
        assert_eq!(update, Update::DoNothing);
        assert_eq!(css_changes(&changes).len(), 2, "the panes still resize");
    }

    #[test]
    fn pointer_move_fires_the_hook_even_when_the_ratio_did_not_change() {
        let boxes = [(0, size(200.0, 100.0))];
        let mut log = RefAny::new(ResizeLog::default());
        let sp = plain(SplitDirection::Horizontal)
            .with_on_resize(log.clone(), record_resize as SplitPaneOnResizeCallbackType);
        let (_, changes, mut state) = press_then_move(sp, &boxes, (100.0, 50.0), (100.0, 50.0));
        assert_eq!(wrapper(&mut state).inner.ratio, 0.5);
        assert_eq!(
            logged(&mut log).len(),
            1,
            "a zero-delta move still notifies"
        );
        assert_eq!(css_changes(&changes).len(), 2);
    }

    #[test]
    fn pointer_move_without_a_cursor_or_rect_keeps_the_drag_alive() {
        let boxes = [(0, size(200.0, 100.0))];
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let a = sd.clone();
        let (_, _) = drive(a, &boxes, node(0), cursor(100.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        // No cursor.
        let b = sd.clone();
        let (u1, c1) = drive(b, &boxes, node(0), OptionLogicalPosition::None, |info| {
            on_split_pointer_move(state.clone(), info)
        });
        // No laid-out rect.
        let (u2, c2) = drive(sd, &[], node(0), cursor(150.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        assert_eq!((u1, u2), (Update::DoNothing, Update::DoNothing));
        assert!(c1.is_empty() && c2.is_empty());
        let mut state = state;
        let w = wrapper(&mut state);
        assert!(
            w.is_dragging,
            "a dropped move event must not cancel the drag"
        );
        assert_eq!(w.inner.ratio, 0.5);
    }

    #[test]
    fn pointer_move_on_a_zero_sized_container_is_a_no_op() {
        let boxes = [(0, size(200.0, 100.0))];
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let a = sd.clone();
        let (_, _) = drive(a, &boxes, node(0), cursor(100.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        let (update, changes) = drive(
            sd,
            &[(0, size(0.0, 0.0))],
            node(0),
            cursor(0.0, 0.0),
            |info| on_split_pointer_move(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "no division by a zero main size");
        let mut state = state;
        assert_eq!(wrapper(&mut state).inner.ratio, 0.5);
    }

    #[test]
    fn pointer_move_with_a_wrong_typed_payload_is_a_no_op() {
        let (sd, _state) = laid_out(plain(SplitDirection::Horizontal));
        let stranger = RefAny::new([0u8; 3]);
        let (update, changes) = drive(
            sd,
            &[(0, size(200.0, 100.0))],
            node(0),
            cursor(150.0, 50.0),
            |info| on_split_pointer_move(stranger.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn pointer_move_on_a_childless_hit_node_still_tracks_the_ratio() {
        // The handler assumes the hit node is the container. Aim it at a leaf
        // (the divider's DOM node) instead: the CSS writes are skipped, but
        // nothing panics and the ratio bookkeeping still runs.
        let boxes = [(0, size(200.0, 100.0)), (3, size(6.0, 100.0))];
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let a = sd.clone();
        let (_, _) = drive(a, &boxes, node(0), cursor(100.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        let (update, changes) = drive(sd, &boxes, node(3), cursor(3.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(
            css_changes(&changes).len() <= 2,
            "at most the two panes may be written"
        );
        let mut state = state;
        assert!(wrapper(&mut state).inner.ratio.is_finite());
    }

    #[test]
    fn pointer_move_with_a_nan_cursor_poisons_the_ratio() {
        // PIN + KNOWN DEFECT: `clamp` passes NaN through, so a NaN cursor
        // leaves the widget with a NaN ratio, which then encodes as
        // flex-grow: 0 on BOTH panes (an invisible split). No panic, but the
        // documented `[MIN_RATIO, MAX_RATIO]` invariant is broken.
        let boxes = [(0, size(200.0, 100.0))];
        let (_, changes, mut state) = press_then_move(
            plain(SplitDirection::Horizontal),
            &boxes,
            (100.0, 50.0),
            (f32::NAN, 50.0),
        );
        assert!(wrapper(&mut state).inner.ratio.is_nan());
        let writes = css_changes(&changes);
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].1, 0.0);
        assert_eq!(writes[1].1, 0.0);
    }

    #[test]
    fn pointer_move_with_a_nan_container_size_poisons_the_ratio() {
        // Same defect from the other side: `msize <= 0.0` does not reject NaN,
        // so `delta / NaN` reaches the clamp. Pinned, not endorsed.
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let a = sd.clone();
        let (_, _) = drive(
            a,
            &[(0, size(200.0, 100.0))],
            node(0),
            cursor(100.0, 50.0),
            |info| on_split_pointer_down(state.clone(), info),
        );
        let (update, _) = drive(
            sd,
            &[(0, size(f32::NAN, 100.0))],
            node(0),
            cursor(150.0, 50.0),
            |info| on_split_pointer_move(state.clone(), info),
        );
        assert_eq!(update, Update::DoNothing);
        let mut state = state;
        assert!(wrapper(&mut state).inner.ratio.is_nan());
    }

    #[test]
    fn pointer_move_with_extreme_cursors_stays_inside_the_clamp() {
        let boxes = [(0, size(200.0, 100.0))];
        for x in [
            f32::MAX,
            f32::MIN,
            1.0e30,
            -1.0e30,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let (_, _, mut state) = press_then_move(
                plain(SplitDirection::Horizontal),
                &boxes,
                (100.0, 50.0),
                (x, 50.0),
            );
            let r = wrapper(&mut state).inner.ratio;
            assert!(
                (MIN_RATIO..=MAX_RATIO).contains(&r),
                "cursor {x} produced ratio {r}"
            );
        }
    }

    #[test]
    fn pointer_move_on_a_huge_container_is_still_a_proportion() {
        let boxes = [(0, size(1.0e9, 100.0))];
        let (_, _, mut state) = press_then_move(
            plain(SplitDirection::Horizontal),
            &boxes,
            (5.0e8, 50.0),
            (7.5e8, 50.0),
        );
        let r = wrapper(&mut state).inner.ratio;
        assert!((r - 0.75).abs() < 1e-4, "expected ~0.75, got {r}");
    }

    #[test]
    fn pointer_move_on_a_sub_pixel_container_does_not_explode() {
        let boxes = [(0, size(f32::MIN_POSITIVE, 100.0))];
        let (_, _, mut state) = press_then_move(
            plain(SplitDirection::Horizontal),
            &boxes,
            (0.0, 50.0),
            (1.0, 50.0),
        );
        // delta / MIN_POSITIVE is astronomically large; the clamp catches it
        // instead of letting a garbage ratio through.
        assert_eq!(wrapper(&mut state).inner.ratio, MAX_RATIO);
    }

    // ==================================================================
    // on_split_pointer_up  (other)
    // ==================================================================

    #[test]
    fn pointer_up_ends_the_drag_and_keeps_the_ratio() {
        let boxes = [(0, size(200.0, 100.0))];
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let a = sd.clone();
        let (_, _) = drive(a, &boxes, node(0), cursor(100.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        let b = sd.clone();
        let (_, _) = drive(b, &boxes, node(0), cursor(150.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        let (update, changes) = drive(sd, &boxes, node(0), cursor(150.0, 50.0), |info| {
            on_split_pointer_up(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "release must not rewrite the panes");
        let mut state = state;
        let w = wrapper(&mut state);
        assert!(!w.is_dragging);
        assert_eq!(w.inner.ratio, 0.75, "the drag result survives the release");
    }

    #[test]
    fn pointer_up_is_idempotent_and_safe_without_a_drag() {
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let a = sd.clone();
        let (u1, _) = drive(a, &[], node_none(), OptionLogicalPosition::None, |info| {
            on_split_pointer_up(state.clone(), info)
        });
        let (u2, _) = drive(sd, &[], node_none(), OptionLogicalPosition::None, |info| {
            on_split_pointer_up(state.clone(), info)
        });
        assert_eq!((u1, u2), (Update::DoNothing, Update::DoNothing));
        let mut state = state;
        let w = wrapper(&mut state);
        assert!(!w.is_dragging);
        assert_eq!(w.inner.ratio, 0.5);
    }

    #[test]
    fn pointer_up_with_a_wrong_typed_payload_is_a_no_op() {
        let (sd, _state) = laid_out(plain(SplitDirection::Horizontal));
        let stranger = RefAny::new("not a split pane".to_string());
        let (update, changes) = drive(sd, &[], node_none(), OptionLogicalPosition::None, |info| {
            on_split_pointer_up(stranger.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn a_released_drag_ignores_further_motion() {
        let boxes = [(0, size(200.0, 100.0))];
        let (sd, state) = laid_out(plain(SplitDirection::Horizontal));
        let a = sd.clone();
        let (_, _) = drive(a, &boxes, node(0), cursor(100.0, 50.0), |info| {
            on_split_pointer_down(state.clone(), info)
        });
        let b = sd.clone();
        let (_, _) = drive(b, &boxes, node(0), cursor(150.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        let c = sd.clone();
        let (_, _) = drive(c, &boxes, node(0), cursor(150.0, 50.0), |info| {
            on_split_pointer_up(state.clone(), info)
        });
        let (update, changes) = drive(sd, &boxes, node(0), cursor(20.0, 50.0), |info| {
            on_split_pointer_move(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "post-release motion must not resize");
        let mut state = state;
        assert_eq!(wrapper(&mut state).inner.ratio, 0.75);
    }
}
