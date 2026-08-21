//! Slider / range widget — a horizontal track with a draggable circular thumb
//! that maps a position along the track to a numeric value in `[min, max]`.
//! Combines the value/min/max state + `on_value_change` callback shape of
//! [`crate::widgets::number_input::NumberInput`] with the pointer-drag handling
//! of [`crate::widgets::map`] (cursor-relative-to-node → value), and the
//! switch's "track + knob slid via `margin-left`" rendering.
//!
//! Behaviour: pressing or dragging anywhere on the track sets the value from the
//! cursor's X position (relative to the track, in logical px), slides the thumb
//! live via `set_css_property`, and invokes the user's `on_value_change`.
//!
//! Key types: [`Slider`], [`SliderState`], [`SliderOnValueChange`].

use crate::solver3::layout_tree::LayoutNodeId;
use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, *},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutWidth, LayoutHeight, LayoutMarginLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleCursor},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static SLIDER_TRACK_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-slider"))];
static SLIDER_THUMB_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-slider-thumb"))];

/// Callback function type invoked when the slider value changes.
pub type SliderOnValueChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, SliderState) -> Update;
impl_widget_callback!(
    SliderOnValueChange,
    OptionSliderOnValueChange,
    SliderOnValueChangeCallback,
    SliderOnValueChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SliderOnValueChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SLIDER_ON_VALUE_CHANGE_INVOKER,
    invoker_ty:     AzSliderOnValueChangeCallbackInvoker,
    thunk_fn:       az_slider_on_value_change_callback_thunk,
    setter_fn:      AzApp_setSliderOnValueChangeCallbackInvoker,
    from_handle_fn: AzSliderOnValueChangeCallback_createFromHostHandle,
    extra_args:     [ state: SliderState ],
}

/// A horizontal slider with a draggable thumb and a value-change callback.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Slider {
    pub slider_state: SliderStateWrapper,
    /// Style for the slider track (the horizontal rail).
    pub track_style: CssPropertyWithConditionsVec,
    /// Style for the draggable thumb.
    pub thumb_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct SliderStateWrapper {
    /// Optional: function to call when the value changes.
    pub on_value_change: OptionSliderOnValueChange,
    /// The value/range of this Slider.
    pub inner: SliderState,
    /// `true` while a pointer-drag is in flight (mirrors `map::MapTileCache::drag_anchor`).
    /// Transient; not part of the user-visible [`SliderState`].
    pub dragging: bool,
}

/// State of a [`Slider`]: the current value and the allowed `[min, max]` range.
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct SliderState {
    /// The current value (always within `[min, max]`).
    pub value: f32,
    /// Minimum allowed value (inclusive) — thumb at the far left.
    pub min: f32,
    /// Maximum allowed value (inclusive) — thumb at the far right.
    pub max: f32,
}

impl Default for SliderState {
    fn default() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 100.0,
        }
    }
}

// ---- dimensions (logical px) ----
const TRACK_WIDTH: isize = 200;
const TRACK_HEIGHT: isize = 16;
const TRACK_RADIUS: isize = 8;
const THUMB_SIZE: isize = 16;
const THUMB_RADIUS: isize = 8;

// ---- colours ----
/// Rail colour (#cccccc).
const RAIL_COLOR: ColorU = ColorU {
    r: 204,
    g: 204,
    b: 204,
    a: 255,
};
/// Thumb colour (#0d6efd, accent blue).
const THUMB_COLOR: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};

const RAIL_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(RAIL_COLOR)];
const RAIL_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(RAIL_BG_ITEMS);
const THUMB_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(THUMB_COLOR)];
const THUMB_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(THUMB_BG_ITEMS);

/// The track (rail) style is parameter-independent, so it lives in a const slice.
static SLIDER_TRACK_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(TRACK_WIDTH))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
        TRACK_HEIGHT,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(TRACK_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(TRACK_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(TRACK_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(TRACK_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(RAIL_BG)),
];

/// Maps a value to a `[0, 1]` fraction along the track.
fn value_to_fraction(value: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

/// Builds the thumb style; the `margin-left` is the only position-dependent
/// property and slides the thumb between the left (`min`) and right (`max`) ends.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // bounded layout/render numeric cast
fn build_thumb_style(fraction: f32) -> CssPropertyWithConditionsVec {
    // `fraction` is a bare `f32` with no type-level guard. Its only caller feeds
    // it `value_to_fraction`'s already-clamped output, but the helper must be
    // safe on its own terms: `const_px` encodes the margin as `isize * 1000`
    // (`FloatValue::const_new`), so any |margin| above `isize::MAX / 1000`
    // overflows that multiply — a panic in an overflow-checked build, a wrapped
    // and wildly-wrong margin in release. `as isize` already saturates NaN to 0
    // and ±inf to isize::MIN/MAX, so the only thing missing is the clamp into
    // what the fixed-point encoding can actually hold. Clamping the ENCODED px
    // rather than the fraction keeps every in-contract and out-of-contract-but-
    // representable result (including negative fractions) bit-for-bit unchanged.
    const MAX_ENCODABLE_PX: isize = isize::MAX / 1000;
    let margin = ((fraction * (TRACK_WIDTH - THUMB_SIZE) as f32).round() as isize)
        .clamp(-MAX_ENCODABLE_PX, MAX_ENCODABLE_PX);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(
            THUMB_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            THUMB_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(THUMB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(THUMB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(THUMB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(THUMB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(THUMB_BG)),
        CssPropertyWithConditions::simple(CssProperty::const_margin_left(
            LayoutMarginLeft::const_px(margin),
        )),
    ])
}

/// Clamps `value` into `[min, max]`, tolerating the degenerate bounds that
/// `f32::clamp` panics on: an inverted range (`min > max`) is swapped, and a NaN
/// bound is dropped (if both are NaN the value is returned untouched). `min`/`max`
/// are `pub` fields on a `#[repr(C)]` `SliderState` that crosses the C/FFI
/// boundary, so a caller can supply either — and unwinding across FFI is UB.
fn clamp_to_range(value: f32, min: f32, max: f32) -> f32 {
    let (lo, hi) = match (min.is_nan(), max.is_nan()) {
        (true, true) => return value,
        (true, false) => (max, max),
        (false, true) => (min, min),
        (false, false) if min <= max => (min, max),
        (false, false) => (max, min),
    };
    value.clamp(lo, hi)
}

impl Slider {
    /// Creates a slider with the given current value and `[min, max]` range.
    #[must_use] pub fn create(value: f32, min: f32, max: f32) -> Self {
        let value = clamp_to_range(value, min, max);
        Self {
            slider_state: SliderStateWrapper {
                inner: SliderState { value, min, max },
                ..Default::default()
            },
            track_style: CssPropertyWithConditionsVec::from_const_slice(SLIDER_TRACK_STYLE),
            thumb_style: build_thumb_style(value_to_fraction(value, min, max)),
        }
    }

    /// Sets the current value (clamped to the range), recomputing the thumb position.
    #[inline]
    pub fn set_value(&mut self, value: f32) {
        let min = self.slider_state.inner.min;
        let max = self.slider_state.inner.max;
        let value = clamp_to_range(value, min, max);
        self.slider_state.inner.value = value;
        self.thumb_style = build_thumb_style(value_to_fraction(value, min, max));
    }

    /// Builder-style setter for the current value.
    #[inline]
    #[must_use] pub fn with_value(mut self, value: f32) -> Self {
        self.set_value(value);
        self
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0.0, 0.0, 100.0);
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_value_change<C: Into<SliderOnValueChangeCallback>>(
        &mut self,
        data: RefAny,
        on_value_change: C,
    ) {
        self.slider_state.on_value_change = Some(SliderOnValueChange {
            callback: on_value_change.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use] pub fn with_on_value_change<C: Into<SliderOnValueChangeCallback>>(
        mut self,
        data: RefAny,
        on_value_change: C,
    ) -> Self {
        self.set_on_value_change(data, on_value_change);
        self
    }

    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        // Read the value BEFORE the fields are moved into the DOM below.
        let value_now = self.slider_state.inner.value;

        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        // One shared RefAny across all pointer callbacks so the transient
        // `dragging` flag set on press is visible to the move/release handlers
        // (RefAny::clone shares the underlying data — same pattern as map.rs).
        let state = RefAny::new(self.slider_state);
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
                on_slider_pointer_down as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                on_slider_pointer_move as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                on_slider_pointer_up as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::MouseLeave),
                on_slider_pointer_up as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::TouchStart),
                on_slider_pointer_down as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::TouchMove),
                on_slider_pointer_move as usize,
            ),
            mk(
                EventFilter::Hover(HoverEventFilter::TouchEnd),
                on_slider_pointer_up as usize,
            ),
        ];

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SLIDER_TRACK_CLASS))
            .with_css_props(self.track_style)
            .with_callbacks(callbacks.into())
            .with_tab_index(TabIndex::Auto)
            // For a slider the VALUE is the content. Without it a screen reader
            // announces "slider" and never where the thumb sits, which is the
            // one thing the control exists to communicate. Published on every
            // build so it tracks the thumb rather than freezing at construction.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Slider,
                accessibility_value: Some(AzString::from(
                    alloc::format!("{value_now}"),
                ))
                .into(),
                ..Default::default()
            })
            .with_children(
                vec![Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(SLIDER_THUMB_CLASS))
                    .with_css_props(self.thumb_style)]
                .into(),
            )
    }
}

impl Default for Slider {
    fn default() -> Self {
        Self::create(0.0, 0.0, 100.0)
    }
}

/// Shared logic for press + drag: compute the value from the cursor's X position
/// relative to the track, slide the thumb live, and invoke the user callback.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // bounded layout/render numeric cast
fn apply_cursor_value(slider: &mut SliderStateWrapper, info: &mut CallbackInfo) -> Update {
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    // Track width in LOGICAL px (falls back to the design width before first layout).
    let width = info
        .get_hit_node_rect()
        .map(|r| r.size.width)
        .filter(|w| *w > 0.0)
        .unwrap_or(TRACK_WIDTH as f32);

    let fraction = (pos.x / width).clamp(0.0, 1.0);
    let min = slider.inner.min;
    let max = slider.inner.max;
    slider.inner.value = fraction.mul_add(max - min, min);

    // Slide the thumb (first child of the track) to the new position.
    let track_id = info.get_hit_node();
    if let Some(thumb_id) = info.get_first_child(track_id) {
        let margin = (fraction * (width - THUMB_SIZE as f32)).round() as isize;
        info.set_css_property(
            thumb_id,
            CssProperty::const_margin_left(LayoutMarginLeft::const_px(margin)),
        );
    }

    let inner = slider.inner;
    match slider.on_value_change.as_mut() {
        Some(SliderOnValueChange { callback, refany }) => (callback.cb)(refany.clone(), *info, inner),
        None => Update::DoNothing,
    }
}

/// Pointer down → begin a drag and set the value from the press position.
extern "C" fn on_slider_pointer_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut slider) = data.downcast_mut::<SliderStateWrapper>() else {
        return Update::DoNothing;
    };
    slider.dragging = true;
    apply_cursor_value(&mut slider, &mut info)
}

/// Pointer move → if a drag is active, track the value to the cursor.
extern "C" fn on_slider_pointer_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut slider) = data.downcast_mut::<SliderStateWrapper>() else {
        return Update::DoNothing;
    };
    if !slider.dragging {
        return Update::DoNothing;
    }
    apply_cursor_value(&mut slider, &mut info)
}

/// Pointer up / leave → end the drag.
extern "C" fn on_slider_pointer_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut slider) = data.downcast_mut::<SliderStateWrapper>() {
        slider.dragging = false;
    }
    Update::DoNothing
}

impl From<Slider> for Dom {
    fn from(s: Slider) -> Self {
        s.dom()
    }
}

// ────────── Adversarial autotest coverage ────────────────────────────
//
// The slider is a pure `f32 -> fraction -> whole-pixel margin` pipeline wrapped
// in three pointer callbacks. Everything below feeds it values a real app (or an
// FFI caller writing the `#[repr(C)]` `pub` fields directly) can produce — an
// inverted `[min, max]`, a NaN bound, an infinite range, a cursor outside the
// track, a zero-width track, a foreign payload — and asserts the widget
// *contains* them instead of panicking or sliding the thumb off the rail.
#[cfg(all(test, feature = "std"))]
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::unreadable_literal,
    clippy::too_many_lines
)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        mem::discriminant,
        panic::{catch_unwind, AssertUnwindSafe},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{
            DomId, DomNodeId, EventFilter, FormattingContext, HoverEventFilter, NodeId, NodeType,
        },
        geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};
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

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// The travel the thumb has along the design-width track: a thumb at
    /// `fraction == 1.0` must sit flush against the right end, not past it.
    const TRAVEL: f32 = (TRACK_WIDTH - THUMB_SIZE) as f32; // 184.0

    /// The whole design width of the track, as an `f32` — the value
    /// `apply_cursor_value` falls back to before the first layout.
    const DESIGN_WIDTH: f32 = TRACK_WIDTH as f32; // 200.0

    /// Fractions `value_to_fraction` can actually hand to `build_thumb_style`:
    /// the closed unit interval (plus both signed zeroes) and NaN, which is what
    /// a NaN value/bound collapses to. Nothing else is reachable through the
    /// public API — the out-of-contract extremes get their own probe.
    const REACHABLE_FRACTIONS: [f32; 10] = [
        0.0,
        -0.0,
        f32::MIN_POSITIVE,
        f32::EPSILON,
        0.001,
        0.25,
        0.5,
        0.75,
        1.0,
        f32::NAN,
    ];

    /// `[min, max]` ranges a caller can legally build (`min <= max`, both finite).
    const SANE_RANGES: [(f32, f32); 8] = [
        (0.0, 100.0),
        (0.0, 1.0),
        (-100.0, -50.0),
        (-50.0, 50.0),
        (0.0, 0.0),
        (-7.5, 7.5),
        (1.0, 1.0e9),
        (-1.0e9, 1.0e9),
    ];

    /// Ranges that break `f32::clamp`'s `min <= max` precondition. Every one of
    /// them is expressible: `SliderState`'s `min`/`max` are `pub` fields on a
    /// `#[repr(C)]` struct that crosses the C/FFI boundary.
    const DEGENERATE_RANGES: [(f32, f32); 6] = [
        (100.0, 0.0),
        (1.0, -1.0),
        (f32::NAN, 100.0),
        (0.0, f32::NAN),
        (f32::NAN, f32::NAN),
        (f32::INFINITY, f32::NEG_INFINITY),
    ];

    // ------------------------------------------------------------------
    // Style-vec probes
    // ------------------------------------------------------------------

    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The `f32` behind a `PixelValue`, asserting it is an absolute `px` length.
    /// An `em`/`%` slipping into the thumb offset would resolve against the
    /// parent font/box, so "92px along the rail" could land anywhere.
    fn px(pv: PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "slider geometry must be absolute px, got {:?}",
            pv.metric,
        );
        pv.number.get()
    }

    /// The declared `margin-left` of a style vec — the thumb's position.
    fn margin_left(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::MarginLeft(m) => m.get_property().map(|m| px(m.inner)),
            _ => None,
        })
    }

    fn width_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(px(*pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(px(*pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn background(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(b) => match b.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    /// The thumb offset a freshly built widget declares.
    fn thumb_margin(s: &Slider) -> f32 {
        margin_left(&s.thumb_style).expect("the thumb style must declare a margin-left")
    }

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    fn inline_properties(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    // ------------------------------------------------------------------
    // Callback harness
    // ------------------------------------------------------------------

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

    /// A layout result carrying only the styled DOM: the hierarchy is real (so
    /// `get_first_child` resolves the thumb) but nothing is laid out, so
    /// `get_hit_node_rect` returns `None` — the pre-first-layout state a widget
    /// is in when the very first `MouseDown` arrives.
    fn unlaid(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// The same DOM, but laid out: node 0 (the track) reports `track` as its
    /// used size, so `get_hit_node_rect` yields a real width.
    fn laid_out_at(styled_dom: StyledDom, track: LogicalSize) -> DomLayoutResult {
        let hot = |dom_node: usize, size: LogicalSize, parent: Option<usize>| LayoutNodeHot {
            box_props: PackedBoxProps::default(),
            dom_node_id: Some(NodeId::new(dom_node)),
            used_size: Some(size),
            formatting_context: FormattingContext::Flex,
            parent,
        };
        let mut dom_to_layout = BTreeMap::new();
        dom_to_layout.insert(NodeId::new(0), vec![LayoutNodeId::new(0)]);
        dom_to_layout.insert(NodeId::new(1), vec![LayoutNodeId::new(1)]);

        let mut result = unlaid(styled_dom);
        result.layout_tree.nodes = vec![
            hot(0, track, None),
            hot(1, LogicalSize::new(THUMB_SIZE as f32, THUMB_SIZE as f32), Some(0)),
        ];
        result.layout_tree.dom_to_layout = dom_to_layout;
        result.calculated_positions = vec![LogicalPosition::zero(), LogicalPosition::zero()];
        result
    }

    /// Runs `f` with a real `CallbackInfo` over `layout`, hitting `hit`, with
    /// `cursor` reported as the cursor position relative to the hit node.
    /// Returns `f`'s value plus everything the callback pushed onto the log.
    fn with_info<R>(
        layout: DomLayoutResult,
        hit: DomNodeId,
        cursor: OptionLogicalPosition,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window.layout_results.insert(DomId::ROOT_ID, layout);

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
            system_style: Arc::new(azul_css::system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            cursor,
            OptionLogicalPosition::None,
        );

        let out = f(&mut info);
        let pushed = info.take_changes();
        (out, pushed)
    }

    /// Renders `slider` and hands back the styled DOM *plus the very `RefAny`
    /// the widget registered on its own handlers* — driving the callbacks with
    /// these two is the real wiring, so a mismatch between what `dom()` stores
    /// and what the handlers expect cannot hide behind the fixture.
    fn wired(slider: Slider) -> (StyledDom, RefAny) {
        let dom = slider.dom();
        let state = dom.root.callbacks.as_ref()[0].refany.clone();
        (StyledDom::create_from_dom(dom), state)
    }

    fn cursor(x: f32, y: f32) -> OptionLogicalPosition {
        OptionLogicalPosition::Some(LogicalPosition::new(x, y))
    }

    /// One pointer event of `kind` delivered to the widget's own handler.
    fn deliver(
        slider: Slider,
        state: &RefAny,
        hit: DomNodeId,
        at: OptionLogicalPosition,
        track: Option<LogicalSize>,
        kind: extern "C" fn(RefAny, CallbackInfo) -> Update,
    ) -> (Update, Vec<CallbackChange>) {
        let styled = StyledDom::create_from_dom(slider.dom());
        let layout = match track {
            Some(t) => laid_out_at(styled, t),
            None => unlaid(styled),
        };
        with_info(layout, hit, at, |info| kind(state.clone(), *info))
    }

    /// A press at `x` on a never-laid-out slider — the common path.
    fn press_at(slider: Slider, x: f32) -> (Update, Vec<CallbackChange>, SliderStateWrapper) {
        let (styled, state) = wired(slider);
        let (update, changes) = with_info(unlaid(styled), node(0), cursor(x, 8.0), |info| {
            on_slider_pointer_down(state.clone(), *info)
        });
        (update, changes, read_state(&state))
    }

    fn read_state(state: &RefAny) -> SliderStateWrapper {
        let mut state = state.clone();
        let wrapper = state
            .downcast_ref::<SliderStateWrapper>()
            .expect("the widget state changed type");
        wrapper.clone()
    }

    /// Every `(node, margin-left)` pair a callback pushed onto the change log.
    fn pushed_margins(changes: &[CallbackChange]) -> Vec<(NodeId, f32)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id, properties, ..
                } => properties
                    .as_ref()
                    .iter()
                    .find_map(|p| match p {
                        CssProperty::MarginLeft(m) => m.get_property().map(|m| px(m.inner)),
                        _ => None,
                    })
                    .map(|m| (*node_id, m)),
                _ => None,
            })
            .collect()
    }

    /// What `apply_cursor_value` is documented to compute: the cursor fraction
    /// of the track, mapped onto `[min, max]`. `mul_add` (not `a * b + c`) so
    /// the expectation is bit-identical to the implementation.
    fn expected_value(x: f32, width: f32, min: f32, max: f32) -> f32 {
        (x / width).clamp(0.0, 1.0).mul_add(max - min, min)
    }

    // ------------------------------------------------------------------
    // User-hook probes
    // ------------------------------------------------------------------

    /// A payload the value-change hook writes into. It arrives as the `data`
    /// argument — a *shared* clone of what the test still holds — so the test
    /// reads back exactly what the widget passed, with no global state.
    #[derive(Debug, Clone, Default, PartialEq)]
    struct ValueLog {
        seen: Vec<SliderState>,
    }

    extern "C" fn record_value(mut data: RefAny, _: CallbackInfo, state: SliderState) -> Update {
        if let Some(mut log) = data.downcast_mut::<ValueLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn value_do_nothing(_: RefAny, _: CallbackInfo, _: SliderState) -> Update {
        Update::DoNothing
    }

    extern "C" fn value_refresh_all(_: RefAny, _: CallbackInfo, _: SliderState) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in,
    /// which the `From<Callback>` arm *transmutes* into the 3-arg slider slot.
    extern "C" fn generic_shaped(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    fn log_refany() -> RefAny {
        RefAny::new(ValueLog::default())
    }

    fn read_log(probe: &RefAny) -> ValueLog {
        let mut probe = probe.clone();
        let log = probe
            .downcast_ref::<ValueLog>()
            .expect("the user payload changed type");
        log.clone()
    }

    fn hook_ptr(s: &Slider) -> Option<usize> {
        s.slider_state
            .on_value_change
            .as_ref()
            .map(|h| h.callback.cb as *const () as usize)
    }

    // ==================================================================
    // value_to_fraction  (numeric)
    // ==================================================================

    #[test]
    fn value_to_fraction_maps_the_endpoints_and_the_midpoint_exactly() {
        assert_eq!(value_to_fraction(0.0, 0.0, 100.0), 0.0);
        assert_eq!(value_to_fraction(100.0, 0.0, 100.0), 1.0);
        assert_eq!(value_to_fraction(50.0, 0.0, 100.0), 0.5);
        assert_eq!(value_to_fraction(25.0, 0.0, 100.0), 0.25);
        // A range that does not start at zero catches a `value / max` shortcut.
        assert_eq!(value_to_fraction(150.0, 100.0, 200.0), 0.5);
        assert_eq!(value_to_fraction(-75.0, -100.0, -50.0), 0.5);
    }

    #[test]
    fn value_to_fraction_clamps_instead_of_letting_the_thumb_leave_the_rail() {
        for (value, min, max) in [
            (200.0_f32, 0.0_f32, 100.0_f32),
            (-1.0, 0.0, 100.0),
            (1.0e30, 0.0, 1.0),
            (-1.0e30, 0.0, 1.0),
            (f32::MAX, -1.0, 1.0),
            (f32::MIN, -1.0, 1.0),
        ] {
            let f = value_to_fraction(value, min, max);
            assert!(
                (0.0..=1.0).contains(&f),
                "value_to_fraction({value}, {min}, {max}) = {f} is outside [0, 1]",
            );
        }
        assert_eq!(value_to_fraction(200.0, 0.0, 100.0), 1.0);
        assert_eq!(value_to_fraction(-1.0, 0.0, 100.0), 0.0);
    }

    #[test]
    fn value_to_fraction_guards_the_zero_width_and_inverted_range() {
        // `max <= min` is the documented guard: a zero-width range would be a
        // division by zero (0/0 = NaN, x/0 = ±inf) and an inverted one would
        // run the fraction backwards.
        for (value, min, max) in [
            (0.0_f32, 0.0_f32, 0.0_f32),
            (5.0, 5.0, 5.0),
            (-5.0, -5.0, -5.0),
            (1.0e9, 7.0, 7.0),
            (50.0, 100.0, 0.0),
            (50.0, 1.0, -1.0),
        ] {
            assert_eq!(
                value_to_fraction(value, min, max),
                0.0,
                "a degenerate range [{min}, {max}] must pin the thumb left",
            );
        }
    }

    #[test]
    fn value_to_fraction_is_monotonic_across_the_whole_range() {
        // A sign slip in `(value - min) / (max - min)` still returns values in
        // [0, 1] — only the ordering catches a reversed rail.
        let mut previous = f32::NEG_INFINITY;
        for i in -50..=150 {
            let f = value_to_fraction(i as f32, 0.0, 100.0);
            assert!(
                f >= previous,
                "the fraction went backwards at value = {i} ({f} < {previous})",
            );
            previous = f;
        }
        assert_eq!(previous, 1.0);
    }

    #[test]
    fn value_to_fraction_nan_inputs_do_not_panic_and_stay_nan() {
        // `f32::clamp` only asserts on its *bounds* (0.0/1.0 here), so a NaN
        // `self` propagates rather than panicking. Downstream,
        // `build_thumb_style` turns that NaN into a zero margin.
        assert!(value_to_fraction(f32::NAN, 0.0, 100.0).is_nan());
        // A NaN bound makes `max <= min` false, so the guard does *not* fire and
        // the arithmetic produces NaN — still no panic.
        assert!(value_to_fraction(50.0, f32::NAN, 100.0).is_nan());
        assert!(value_to_fraction(50.0, 0.0, f32::NAN).is_nan());
        assert!(value_to_fraction(f32::NAN, f32::NAN, f32::NAN).is_nan());
    }

    #[test]
    fn value_to_fraction_never_escapes_the_unit_interval_for_any_input() {
        // The only contract that matters downstream: the result is either NaN
        // (which `build_thumb_style` casts to a 0 margin) or a real fraction.
        // Anything else slides the thumb off the track.
        let interesting = [
            0.0_f32,
            -0.0,
            1.0,
            -1.0,
            50.0,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ];
        for value in interesting {
            for min in interesting {
                for max in interesting {
                    let f = value_to_fraction(value, min, max);
                    assert!(
                        f.is_nan() || (0.0..=1.0).contains(&f),
                        "value_to_fraction({value}, {min}, {max}) = {f} escaped [0, 1]",
                    );
                }
            }
        }
    }

    #[test]
    fn value_to_fraction_infinite_bounds_produce_a_defined_result() {
        // An unbounded range has no meaningful fraction: `inf / inf` is NaN, and
        // the widget must treat that as "pin the thumb", not as a panic.
        assert!(value_to_fraction(0.0, f32::NEG_INFINITY, f32::INFINITY).is_nan());
        assert!(value_to_fraction(-50.0, f32::NEG_INFINITY, 0.0).is_nan());
        // A half-open range collapses every finite value onto the closed end.
        assert_eq!(value_to_fraction(50.0, 0.0, f32::INFINITY), 0.0);
        // An infinite *value* inside a finite range saturates instead of wrapping.
        assert_eq!(value_to_fraction(f32::INFINITY, 0.0, 100.0), 1.0);
        assert_eq!(value_to_fraction(f32::NEG_INFINITY, 0.0, 100.0), 0.0);
    }

    #[test]
    fn value_to_fraction_survives_a_range_whose_width_overflows_f32() {
        // `f32::MAX - f32::MIN` overflows to +inf, so the fraction degenerates:
        // every finite value divides down to 0.0 (the thumb pins left instead of
        // tracking the value) and the very top of the range becomes inf/inf =
        // NaN. Neither escapes [0, 1] and neither panics — downstream both park
        // the thumb at the left end, because a NaN margin casts to 0.
        for value in [f32::MIN, -1.0e30, 0.0, 1.0e30] {
            assert_eq!(
                value_to_fraction(value, f32::MIN, f32::MAX),
                0.0,
                "an f32-wide range should collapse {value} onto the left end",
            );
        }
        assert!(value_to_fraction(f32::MAX, f32::MIN, f32::MAX).is_nan());
        assert_eq!(margin_left(&build_thumb_style(f32::NAN)), Some(0.0));
    }

    #[test]
    fn value_to_fraction_is_pure() {
        for (min, max) in SANE_RANGES {
            for value in [min, max, 0.0, -1.0, 1.0e9] {
                let a = value_to_fraction(value, min, max);
                let b = value_to_fraction(value, min, max);
                assert!(
                    a == b || (a.is_nan() && b.is_nan()),
                    "value_to_fraction({value}, {min}, {max}) is not deterministic",
                );
            }
        }
    }

    // ==================================================================
    // build_thumb_style  (numeric)
    // ==================================================================

    #[test]
    fn build_thumb_style_endpoints_keep_the_thumb_inside_the_track() {
        assert_eq!(margin_left(&build_thumb_style(0.0)), Some(0.0));
        assert_eq!(margin_left(&build_thumb_style(1.0)), Some(TRAVEL));
        // The right end must leave exactly one thumb-width of room, otherwise
        // the thumb overhangs the rail it is supposed to sit on.
        assert_eq!(TRAVEL + THUMB_SIZE as f32, DESIGN_WIDTH);
    }

    #[test]
    fn build_thumb_style_rounds_to_whole_pixels() {
        // The margin is an `isize` of logical px: fractional positions must round
        // (not truncate), else the thumb drifts left by up to a pixel.
        for (fraction, expected) in [
            (0.5_f32, 92.0_f32),
            (0.25, 46.0),
            (0.75, 138.0),
            (0.1, 18.0),  // 18.4 -> 18
            (0.9, 166.0), // 165.6 -> 166
            (0.001, 0.0), // 0.184 -> 0
        ] {
            assert_eq!(
                margin_left(&build_thumb_style(fraction)),
                Some(expected),
                "fraction {fraction} landed on the wrong pixel",
            );
        }
    }

    #[test]
    fn build_thumb_style_is_monotonic_and_bounded_over_the_unit_interval() {
        let mut previous = f32::NEG_INFINITY;
        for i in 0..=1000 {
            let m = margin_left(&build_thumb_style(i as f32 / 1000.0))
                .expect("every thumb style declares a margin-left");
            assert!(m >= previous, "the thumb moved backwards at {i}/1000");
            assert!(
                (0.0..=TRAVEL).contains(&m),
                "the thumb left the rail at {i}/1000 (margin {m})",
            );
            previous = m;
        }
    }

    #[test]
    fn build_thumb_style_nan_fraction_pins_the_thumb_left_instead_of_panicking() {
        // `NaN as isize` saturates to 0 in Rust (it has been a defined saturating
        // cast since 1.45, not UB), so a NaN value/bound leaves the thumb parked
        // at the left end rather than at a garbage offset.
        assert_eq!(margin_left(&build_thumb_style(f32::NAN)), Some(0.0));
    }

    #[test]
    fn build_thumb_style_negative_and_signed_zero_fractions_are_deterministic() {
        assert_eq!(margin_left(&build_thumb_style(-0.0)), Some(0.0));
        assert_eq!(margin_left(&build_thumb_style(-1.0)), Some(-TRAVEL));
        assert_eq!(margin_left(&build_thumb_style(-0.5)), Some(-92.0));
    }

    #[test]
    fn build_thumb_style_declares_the_full_thumb_geometry_exactly_once() {
        let props = properties(&build_thumb_style(0.5));
        assert_eq!(props.len(), 9, "the thumb style changed shape: {props:?}");
        let mut seen = Vec::new();
        for p in &props {
            let d = discriminant(p);
            assert!(!seen.contains(&d), "the thumb style declares {p:?} twice");
            seen.push(d);
        }
        // A duplicate declaration would silently let the later one win; a missing
        // margin-left would freeze the thumb at the left end forever.
        assert!(
            matches!(props.last(), Some(CssProperty::MarginLeft(_))),
            "margin-left must be the last (position-dependent) declaration",
        );
    }

    #[test]
    fn build_thumb_style_geometry_is_absolute_px_and_independent_of_the_fraction() {
        for fraction in REACHABLE_FRACTIONS {
            let v = build_thumb_style(fraction);
            assert_eq!(width_px(&v), Some(THUMB_SIZE as f32), "fraction {fraction}");
            assert_eq!(height_px(&v), Some(THUMB_SIZE as f32), "fraction {fraction}");
            assert_eq!(background(&v), Some(THUMB_COLOR), "fraction {fraction}");
        }
    }

    #[test]
    fn build_thumb_style_only_the_margin_depends_on_the_fraction() {
        // Everything but `margin-left` must be byte-identical across fractions —
        // otherwise dragging the thumb would restyle the whole knob every frame.
        let strip = |f: f32| -> Vec<CssProperty> {
            properties(&build_thumb_style(f))
                .into_iter()
                .filter(|p| !matches!(p, CssProperty::MarginLeft(_)))
                .collect()
        };
        let reference = strip(0.0);
        for fraction in REACHABLE_FRACTIONS {
            assert_eq!(strip(fraction), reference, "fraction {fraction} restyled the thumb");
        }
    }

    #[test]
    fn build_thumb_style_is_pure_for_every_reachable_fraction() {
        for fraction in REACHABLE_FRACTIONS {
            assert_eq!(
                build_thumb_style(fraction),
                build_thumb_style(fraction),
                "build_thumb_style({fraction}) is not deterministic",
            );
        }
    }

    #[test]
    fn build_thumb_style_handles_every_fraction_value_to_fraction_can_produce() {
        // The full reachable domain, end to end: nothing here may panic and the
        // thumb may never leave the rail.
        for fraction in REACHABLE_FRACTIONS {
            let m = margin_left(&build_thumb_style(fraction))
                .expect("every thumb style declares a margin-left");
            assert!(
                (0.0..=TRAVEL).contains(&m),
                "fraction {fraction} put the thumb at {m}, off a {TRAVEL}px rail",
            );
        }
    }

    #[test]
    fn build_thumb_style_out_of_contract_fractions_must_not_panic() {
        // `build_thumb_style` takes a bare `f32` with no guard. Its only caller
        // feeds it `value_to_fraction`'s clamped output, so these are currently
        // unreachable — but the margin is encoded as `isize * 1000`
        // (`FloatValue::const_new`), so any |fraction| above ~5e13 overflows that
        // multiply and *panics* in an overflow-checked build instead of
        // saturating. A clamp inside `build_thumb_style` (or a saturating
        // `const_px`) would make the helper safe on its own terms.
        let hostile: [f32; 6] = [
            1.0e14,
            -1.0e14,
            1.0e30,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        let panicked: Vec<f32> = hostile
            .iter()
            .copied()
            .filter(|&f| catch_unwind(AssertUnwindSafe(|| build_thumb_style(f))).is_err())
            .collect();
        assert!(
            panicked.is_empty(),
            "build_thumb_style overflows the isize fixed-point encoding and panics \
             instead of saturating for: {panicked:?}",
        );
    }

    // ==================================================================
    // Slider::create  (numeric)
    // ==================================================================

    #[test]
    fn create_clamps_the_value_into_the_range() {
        assert_eq!(Slider::create(150.0, 0.0, 100.0).slider_state.inner.value, 100.0);
        assert_eq!(Slider::create(-50.0, 0.0, 100.0).slider_state.inner.value, 0.0);
        assert_eq!(Slider::create(50.0, 0.0, 100.0).slider_state.inner.value, 50.0);
        assert_eq!(Slider::create(f32::MAX, -1.0, 1.0).slider_state.inner.value, 1.0);
        assert_eq!(Slider::create(f32::MIN, -1.0, 1.0).slider_state.inner.value, -1.0);
    }

    #[test]
    fn create_stores_the_bounds_verbatim() {
        for (min, max) in SANE_RANGES {
            let s = Slider::create(min, min, max);
            assert_eq!(s.slider_state.inner.min, min, "min was rewritten");
            assert_eq!(s.slider_state.inner.max, max, "max was rewritten");
        }
    }

    #[test]
    fn create_places_the_thumb_where_the_pure_helpers_say_it_belongs() {
        // The composition `create -> value_to_fraction -> build_thumb_style` is
        // the whole widget: a mismatch means the rendered thumb and the stored
        // value disagree.
        for (min, max) in SANE_RANGES {
            for value in [min, max, (min + max) / 2.0, min - 1.0, max + 1.0, 0.0] {
                let s = Slider::create(value, min, max);
                let expected =
                    margin_left(&build_thumb_style(value_to_fraction(value.clamp(min, max), min, max)));
                assert_eq!(
                    margin_left(&s.thumb_style),
                    expected,
                    "create({value}, {min}, {max}) put the thumb in the wrong place",
                );
            }
        }
    }

    #[test]
    fn create_leaves_the_thumb_on_the_rail_for_every_sane_range() {
        for (min, max) in SANE_RANGES {
            for value in [min, max, (min + max) / 2.0, min - 1.0e9, max + 1.0e9] {
                let m = thumb_margin(&Slider::create(value, min, max));
                assert!(
                    (0.0..=TRAVEL).contains(&m),
                    "create({value}, {min}, {max}) parked the thumb at {m}",
                );
            }
        }
    }

    #[test]
    fn create_with_a_zero_width_range_pins_the_thumb_left() {
        for bound in [0.0_f32, 5.0, -5.0, 1.0e9] {
            let s = Slider::create(bound, bound, bound);
            assert_eq!(s.slider_state.inner.value, bound);
            assert_eq!(thumb_margin(&s), 0.0, "a [{bound}, {bound}] range must pin left");
        }
    }

    #[test]
    fn create_with_a_nan_value_keeps_the_nan_but_still_parks_the_thumb() {
        // `f32::clamp` only asserts on its bounds, so a NaN *value* passes
        // straight through — and the NaN fraction has to degrade to a 0 margin
        // rather than an arbitrary offset.
        let s = Slider::create(f32::NAN, 0.0, 100.0);
        assert!(s.slider_state.inner.value.is_nan(), "the NaN value was silently rewritten");
        assert_eq!(thumb_margin(&s), 0.0);
    }

    #[test]
    fn create_with_infinite_bounds_does_not_panic() {
        // `min <= max` holds for these, so `clamp` is happy; the fraction is NaN
        // (inf / inf) and must degrade to a parked thumb.
        for (value, min, max) in [
            (0.0_f32, f32::NEG_INFINITY, f32::INFINITY),
            (f32::INFINITY, 0.0, f32::INFINITY),
            (f32::NEG_INFINITY, f32::NEG_INFINITY, 0.0),
            (50.0, 0.0, f32::INFINITY),
        ] {
            let s = Slider::create(value, min, max);
            let m = thumb_margin(&s);
            assert!(
                m.is_finite() && (0.0..=TRAVEL).contains(&m),
                "create({value}, {min}, {max}) put the thumb at {m}",
            );
        }
    }

    #[test]
    fn create_with_a_degenerate_range_must_not_panic() {
        // `create` runs `value.clamp(min, max)`, and `f32::clamp` **panics**
        // unless `min <= max` — a NaN bound fails that too. `min`/`max` are `pub`
        // fields on a `#[repr(C)]` struct that crosses the C/FFI boundary, so
        // nothing stops a caller from asking for an inverted or NaN-bounded
        // slider, and a panic here takes the whole app with it. Normalising the
        // range (swap, or fall back to the default) would be safe; unwinding is
        // not. Note `apply_cursor_value` already tolerates these ranges — only
        // the constructor and `set_value` do not.
        let panicked: Vec<(f32, f32)> = DEGENERATE_RANGES
            .iter()
            .copied()
            .filter(|&(min, max)| {
                catch_unwind(AssertUnwindSafe(|| Slider::create(0.0, min, max))).is_err()
            })
            .collect();
        assert!(
            panicked.is_empty(),
            "Slider::create panics (f32::clamp asserts min <= max) instead of \
             normalising these ranges: {panicked:?}",
        );
    }

    #[test]
    fn create_is_deterministic_and_distinguishes_distinct_values() {
        for (min, max) in SANE_RANGES {
            assert_eq!(Slider::create(min, min, max), Slider::create(min, min, max));
        }
        assert_ne!(Slider::create(0.0, 0.0, 100.0), Slider::create(100.0, 0.0, 100.0));
        // Same value, different range: the states differ even though the thumb
        // ends up in the same place.
        assert_ne!(Slider::create(0.0, 0.0, 100.0), Slider::create(0.0, 0.0, 200.0));
    }

    #[test]
    fn create_installs_no_hook_and_starts_undragged() {
        let s = Slider::create(50.0, 0.0, 100.0);
        assert!(
            s.slider_state.on_value_change.as_ref().is_none(),
            "create invented a value-change hook out of nowhere",
        );
        assert!(!s.slider_state.dragging, "a fresh slider must not be mid-drag");
    }

    #[test]
    fn create_track_style_is_shared_and_value_independent() {
        // The rail is parameter-free, so every slider must hand out the very same
        // const table — a per-instance copy would allocate on every rebuild.
        let reference = properties(&Slider::create(0.0, 0.0, 100.0).track_style);
        for (min, max) in SANE_RANGES {
            assert_eq!(
                properties(&Slider::create(max, min, max).track_style),
                reference,
                "the track style leaked a dependency on [{min}, {max}]",
            );
        }
        assert_eq!(reference.len(), 13, "the track style changed shape");
    }

    #[test]
    fn create_track_geometry_is_absolute_px_and_declared_once() {
        let s = Slider::create(50.0, 0.0, 100.0);
        assert_eq!(width_px(&s.track_style), Some(DESIGN_WIDTH));
        assert_eq!(height_px(&s.track_style), Some(TRACK_HEIGHT as f32));
        assert_eq!(background(&s.track_style), Some(RAIL_COLOR));

        let props = properties(&s.track_style);
        let mut seen = Vec::new();
        for p in &props {
            let d = discriminant(p);
            assert!(!seen.contains(&d), "the track style declares {p:?} twice");
            seen.push(d);
        }
        // Without `cursor: pointer` the rail looks inert even though it carries
        // every pointer handler; with `flex-grow != 0` it would stretch and stop
        // being the 200px box the fallback width assumes.
        assert!(props.contains(&CssProperty::const_cursor(StyleCursor::Pointer)));
        assert!(props.contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))));
    }

    #[test]
    fn default_is_exactly_create_0_0_100() {
        assert_eq!(Slider::default(), Slider::create(0.0, 0.0, 100.0));
        assert_eq!(Slider::default().slider_state.inner, SliderState::default());
        assert_eq!(SliderState::default().min, 0.0);
        assert_eq!(SliderState::default().max, 100.0);
        assert_eq!(SliderState::default().value, 0.0);
    }

    // ==================================================================
    // Slider::set_value / with_value
    // ==================================================================

    #[test]
    fn set_value_clamps_and_moves_the_thumb_together() {
        let mut s = Slider::create(0.0, 0.0, 100.0);
        for (input, expected_value) in [
            (50.0_f32, 50.0_f32),
            (100.0, 100.0),
            (150.0, 100.0),
            (-1.0, 0.0),
            (f32::INFINITY, 100.0),
            (f32::NEG_INFINITY, 0.0),
            (0.0, 0.0),
        ] {
            s.set_value(input);
            assert_eq!(s.slider_state.inner.value, expected_value, "set_value({input})");
            assert_eq!(
                thumb_margin(&s),
                margin_left(&build_thumb_style(value_to_fraction(expected_value, 0.0, 100.0)))
                    .expect("margin"),
                "set_value({input}) did not move the thumb with the value",
            );
        }
    }

    #[test]
    fn set_value_never_touches_the_bounds() {
        for (min, max) in SANE_RANGES {
            let mut s = Slider::create(min, min, max);
            s.set_value(1.0e9);
            s.set_value(-1.0e9);
            assert_eq!(s.slider_state.inner.min, min);
            assert_eq!(s.slider_state.inner.max, max);
        }
    }

    #[test]
    fn set_value_round_trips_every_value_inside_the_range() {
        // value -> (clamp, fraction, margin) -> value: the stored value must come
        // back bit-identical for anything already inside the range.
        let mut s = Slider::create(0.0, 0.0, 100.0);
        for i in 0..=100 {
            let v = i as f32;
            s.set_value(v);
            assert_eq!(s.slider_state.inner.value, v, "{v} did not survive set_value");
            assert_eq!(
                thumb_margin(&s),
                (v / 100.0 * TRAVEL).round(),
                "{v} landed on the wrong pixel",
            );
        }
    }

    #[test]
    fn set_value_is_idempotent() {
        let mut s = Slider::create(0.0, 0.0, 100.0);
        s.set_value(37.5);
        let once = s.clone();
        s.set_value(37.5);
        assert_eq!(s, once, "re-setting the same value changed the widget");
    }

    #[test]
    fn set_value_with_nan_parks_the_thumb_without_panicking() {
        let mut s = Slider::create(50.0, 0.0, 100.0);
        s.set_value(f32::NAN);
        assert!(s.slider_state.inner.value.is_nan());
        assert_eq!(thumb_margin(&s), 0.0);
        // ...and the widget still recovers on the next sane write.
        s.set_value(25.0);
        assert_eq!(s.slider_state.inner.value, 25.0);
        assert_eq!(thumb_margin(&s), 46.0);
    }

    #[test]
    fn set_value_on_a_degenerate_range_must_not_panic() {
        // Same `f32::clamp` precondition as `create`, but reachable *without*
        // ever calling `create` with a bad range: the bounds are `pub`, so an FFI
        // caller (or a Rust caller doing `s.slider_state.inner.max = ...`) can
        // invert them between construction and the next `set_value`.
        let panicked: Vec<(f32, f32)> = DEGENERATE_RANGES
            .iter()
            .copied()
            .filter(|&(min, max)| {
                let mut s = Slider::create(0.0, 0.0, 100.0);
                s.slider_state.inner.min = min;
                s.slider_state.inner.max = max;
                catch_unwind(AssertUnwindSafe(move || s.set_value(1.0))).is_err()
            })
            .collect();
        assert!(
            panicked.is_empty(),
            "Slider::set_value panics (f32::clamp asserts min <= max) on these \
             externally-set ranges: {panicked:?}",
        );
    }

    #[test]
    fn with_value_is_exactly_set_value() {
        for v in [0.0_f32, 50.0, 100.0, 150.0, -1.0, f32::INFINITY] {
            let mut expected = Slider::create(0.0, 0.0, 100.0);
            expected.set_value(v);
            assert_eq!(
                Slider::create(0.0, 0.0, 100.0).with_value(v),
                expected,
                "with_value({v}) diverged from set_value({v})",
            );
        }
    }

    #[test]
    fn with_value_keeps_the_hook_it_was_handed() {
        // The builder moves `self`; dropping the callback on the way through
        // would silently disconnect a slider that still looks correct.
        let s = Slider::create(0.0, 0.0, 100.0)
            .with_on_value_change(log_refany(), record_value as SliderOnValueChangeCallbackType)
            .with_value(80.0);
        assert_eq!(hook_ptr(&s), Some(record_value as *const () as usize));
        assert_eq!(s.slider_state.inner.value, 80.0);
    }

    #[test]
    fn with_value_chains_to_the_last_write() {
        let s = Slider::create(0.0, 0.0, 100.0)
            .with_value(10.0)
            .with_value(90.0)
            .with_value(45.0);
        assert_eq!(s.slider_state.inner.value, 45.0);
        assert_eq!(thumb_margin(&s), (0.45 * TRAVEL).round());
    }

    // ==================================================================
    // Slider::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_old_widget_and_leaves_a_default_behind() {
        let mut s = Slider::create(75.0, 50.0, 200.0);
        let old = s.swap_with_default();
        assert_eq!(old.slider_state.inner.value, 75.0);
        assert_eq!(old.slider_state.inner.min, 50.0);
        assert_eq!(old.slider_state.inner.max, 200.0);
        assert_eq!(s, Slider::default(), "the hole was not filled with a default");
    }

    #[test]
    fn swap_with_default_moves_the_hook_out_with_the_old_widget() {
        let mut s = Slider::create(10.0, 0.0, 100.0)
            .with_on_value_change(log_refany(), record_value as SliderOnValueChangeCallbackType);
        let old = s.swap_with_default();
        assert_eq!(hook_ptr(&old), Some(record_value as *const () as usize));
        assert_eq!(hook_ptr(&s), None, "the hook survived the swap");
    }

    #[test]
    fn swap_with_default_is_an_involution_when_chained() {
        let original = Slider::create(33.0, 0.0, 100.0);
        let mut s = original.clone();
        let first = s.swap_with_default();
        s = first;
        assert_eq!(s, original, "swap-then-restore lost state");
    }

    #[test]
    fn swap_with_default_does_not_panic_for_hostile_widgets() {
        // The replacement is always `create(0, 0, 100)`, so the swap itself is
        // safe even when the outgoing widget holds NaN/infinite state.
        for (value, min, max) in [
            (f32::NAN, 0.0_f32, 100.0_f32),
            (0.0, f32::NEG_INFINITY, f32::INFINITY),
            (5.0, 5.0, 5.0),
        ] {
            let mut s = Slider::create(value, min, max);
            let old = s.swap_with_default();
            assert_eq!(s, Slider::default());
            assert_eq!(old.slider_state.inner.min, min);
        }
    }

    // ==================================================================
    // Slider::set_on_value_change / with_on_value_change
    // ==================================================================

    #[test]
    fn set_on_value_change_stores_the_pointer_and_the_payload() {
        let probe = RefAny::new(0xDEAD_BEEF_u32);
        let mut s = Slider::create(0.0, 0.0, 100.0);
        s.set_on_value_change(probe.clone(), record_value as SliderOnValueChangeCallbackType);

        let hook = s
            .slider_state
            .on_value_change
            .as_ref()
            .expect("the hook was dropped");
        assert_eq!(hook.callback.cb as *const () as usize, record_value as *const () as usize);
        let mut stored = hook.refany.clone();
        assert_eq!(
            *stored.downcast_ref::<u32>().expect("the payload changed type"),
            0xDEAD_BEEF,
        );
    }

    #[test]
    fn set_on_value_change_replaces_a_previous_hook() {
        // The setter writes `Some(..)` unconditionally; a caller re-registering
        // must end up with exactly one (the newest) hook, not the first one.
        let mut s = Slider::create(0.0, 0.0, 100.0);
        s.set_on_value_change(RefAny::new(1u8), record_value as SliderOnValueChangeCallbackType);
        s.set_on_value_change(RefAny::new(2u8), value_refresh_all as SliderOnValueChangeCallbackType);
        assert_eq!(hook_ptr(&s), Some(value_refresh_all as *const () as usize));
        let mut stored = s
            .slider_state
            .on_value_change
            .as_ref()
            .expect("hook")
            .refany
            .clone();
        assert_eq!(*stored.downcast_ref::<u8>().expect("payload"), 2);
    }

    #[test]
    fn set_on_value_change_leaves_the_value_and_both_styles_untouched() {
        let before = Slider::create(60.0, 0.0, 100.0);
        let mut after = before.clone();
        after.set_on_value_change(log_refany(), record_value as SliderOnValueChangeCallbackType);
        assert_eq!(after.slider_state.inner, before.slider_state.inner);
        assert_eq!(after.track_style, before.track_style);
        assert_eq!(after.thumb_style, before.thumb_style);
    }

    #[test]
    fn with_on_value_change_matches_the_setter() {
        let a = Slider::create(60.0, 0.0, 100.0)
            .with_on_value_change(RefAny::new(9u8), record_value as SliderOnValueChangeCallbackType);
        let mut b = Slider::create(60.0, 0.0, 100.0);
        b.set_on_value_change(RefAny::new(9u8), record_value as SliderOnValueChangeCallbackType);
        assert_eq!(hook_ptr(&a), hook_ptr(&b));
        assert_eq!(a.slider_state.inner, b.slider_state.inner);
    }

    #[test]
    fn with_on_value_change_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm *transmutes* a 2-arg fn pointer into the 3-arg
        // slider slot — this is the FFI (Python/C) path. The pointer must come
        // out bit-identical; a mangled one is a wild jump on the first drag.
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::None,
        };
        let s = Slider::create(0.0, 0.0, 100.0).with_on_value_change(RefAny::new(0u8), generic);
        assert_eq!(
            hook_ptr(&s),
            Some(generic_shaped as *const () as usize),
            "the Callback -> SliderOnValueChangeCallback transmute mangled the pointer",
        );
    }

    #[test]
    fn with_on_value_change_does_not_panic_for_hostile_widgets() {
        for (value, min, max) in [
            (f32::NAN, 0.0_f32, 100.0_f32),
            (0.0, f32::NEG_INFINITY, f32::INFINITY),
            (7.0, 7.0, 7.0),
        ] {
            let s = Slider::create(value, min, max).with_on_value_change(
                log_refany(),
                record_value as SliderOnValueChangeCallbackType,
            );
            assert!(s.slider_state.on_value_change.as_ref().is_some());
        }
    }

    // ==================================================================
    // Slider::dom
    // ==================================================================

    #[test]
    fn dom_renders_a_classed_track_with_exactly_one_thumb_child() {
        let dom = Slider::create(50.0, 0.0, 100.0).dom();
        assert_eq!(classes(&dom), vec!["__azul-native-slider".to_string()]);
        assert!(matches!(dom.root.get_node_type(), NodeType::Div));
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 1, "the track must have exactly one child (the thumb)");
        assert_eq!(classes(&kids[0]), vec!["__azul-native-slider-thumb".to_string()]);
        assert!(kids[0].children.as_ref().is_empty(), "the thumb must be a leaf");
    }

    #[test]
    fn dom_is_keyboard_reachable() {
        // A slider that cannot take focus is unusable without a mouse.
        let dom = Slider::create(0.0, 0.0, 100.0).dom();
        assert_eq!(dom.root.get_tab_index(), Some(TabIndex::Auto));
    }

    #[test]
    fn dom_registers_every_pointer_filter_exactly_once_in_order() {
        let dom = Slider::create(0.0, 0.0, 100.0).dom();
        let expected: [(EventFilter, usize); 7] = [
            (EventFilter::Hover(HoverEventFilter::MouseDown), on_slider_pointer_down as usize),
            (EventFilter::Hover(HoverEventFilter::MouseOver), on_slider_pointer_move as usize),
            (EventFilter::Hover(HoverEventFilter::MouseUp), on_slider_pointer_up as usize),
            (EventFilter::Hover(HoverEventFilter::MouseLeave), on_slider_pointer_up as usize),
            (EventFilter::Hover(HoverEventFilter::TouchStart), on_slider_pointer_down as usize),
            (EventFilter::Hover(HoverEventFilter::TouchMove), on_slider_pointer_move as usize),
            (EventFilter::Hover(HoverEventFilter::TouchEnd), on_slider_pointer_up as usize),
        ];
        let got: Vec<(EventFilter, usize)> = dom
            .root
            .callbacks
            .as_ref()
            .iter()
            .map(|c| (c.event, c.callback.cb))
            .collect();
        assert_eq!(got, expected.to_vec(), "the pointer wiring changed");
        // Touch must not be dropped: without TouchStart/Move/End the slider is
        // dead on a touchscreen even though it looks fine under a mouse.
        assert_eq!(got.len(), 7);
    }

    #[test]
    fn dom_shares_one_state_refany_across_all_seven_handlers() {
        // The transient `dragging` flag set by MouseDown must be visible to the
        // MouseOver/MouseUp handlers — a per-callback `RefAny::new` would give
        // each handler its own copy and the slider would never drag.
        let dom = Slider::create(0.0, 0.0, 100.0).dom();
        let cbs = dom.root.callbacks.as_ref();
        {
            let mut first = cbs[0].refany.clone();
            let mut guard = first
                .downcast_mut::<SliderStateWrapper>()
                .expect("the state changed type");
            guard.dragging = true;
        }
        for (i, cb) in cbs.iter().enumerate() {
            let mut other = cb.refany.clone();
            let guard = other
                .downcast_ref::<SliderStateWrapper>()
                .expect("the state changed type");
            assert!(guard.dragging, "handler {i} does not share the drag state");
        }
    }

    #[test]
    fn dom_carries_the_widgets_own_state_not_a_default() {
        let mut state = Slider::create(42.0, -10.0, 90.0).dom().root.callbacks.as_ref()[0]
            .refany
            .clone();
        let wrapper = state
            .downcast_ref::<SliderStateWrapper>()
            .expect("the state changed type");
        assert_eq!(wrapper.inner.value, 42.0);
        assert_eq!(wrapper.inner.min, -10.0);
        assert_eq!(wrapper.inner.max, 90.0);
        assert!(!wrapper.dragging, "a freshly rendered slider must not be mid-drag");
    }

    #[test]
    fn dom_inlines_the_track_and_thumb_styles_verbatim() {
        let s = Slider::create(75.0, 0.0, 100.0);
        let (track_props, thumb_props) = (properties(&s.track_style), properties(&s.thumb_style));
        let dom = s.dom();
        assert_eq!(inline_properties(&dom), track_props);
        assert_eq!(inline_properties(&dom.children.as_ref()[0]), thumb_props);
    }

    #[test]
    fn dom_does_not_panic_for_hostile_widgets() {
        for (value, min, max) in [
            (f32::NAN, 0.0_f32, 100.0_f32),
            (0.0, f32::NEG_INFINITY, f32::INFINITY),
            (3.0, 3.0, 3.0),
            (f32::MAX, f32::MIN, f32::MAX),
        ] {
            let dom = Slider::create(value, min, max).dom();
            assert_eq!(dom.children.as_ref().len(), 1, "({value}, {min}, {max})");
        }
    }

    #[test]
    fn from_slider_for_dom_is_exactly_dom() {
        let s = Slider::create(25.0, 0.0, 100.0);
        let via_impl: Dom = s.clone().into();
        assert_eq!(inline_properties(&via_impl), inline_properties(&s.dom()));
    }

    // ==================================================================
    // apply_cursor_value + the pointer handlers
    // ==================================================================

    #[test]
    fn a_press_maps_the_cursor_x_onto_the_range_using_the_design_width() {
        // Before the first layout there is no node rect, so the track falls back
        // to its 200px design width. Anything else silently rescales the value.
        for (x, expected) in [
            (0.0_f32, 0.0_f32),
            (50.0, 25.0),
            (100.0, 50.0),
            (200.0, 100.0),
        ] {
            let (_, _, state) = press_at(Slider::create(0.0, 0.0, 100.0), x);
            assert_eq!(
                state.inner.value,
                expected,
                "a press at x = {x} produced the wrong value",
            );
            assert_eq!(state.inner.value, expected_value(x, DESIGN_WIDTH, 0.0, 100.0));
        }
    }

    #[test]
    fn a_press_outside_the_track_clamps_instead_of_overshooting() {
        for (x, expected) in [
            (-1.0_f32, 0.0_f32),
            (-1.0e9, 0.0),
            (201.0, 100.0),
            (1.0e9, 100.0),
            (f32::INFINITY, 100.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            let (_, _, state) = press_at(Slider::create(50.0, 0.0, 100.0), x);
            assert_eq!(state.inner.value, expected, "a press at x = {x} escaped the range");
        }
    }

    #[test]
    fn a_press_maps_onto_a_negative_range_too() {
        let (_, _, state) = press_at(Slider::create(-100.0, -100.0, -50.0), 100.0);
        assert_eq!(state.inner.value, -75.0, "the midpoint of [-100, -50] is -75");
    }

    #[test]
    fn a_press_slides_the_thumb_by_writing_margin_left_on_the_first_child() {
        let (update, changes, _) = press_at(Slider::create(0.0, 0.0, 100.0), 100.0);
        assert_eq!(update, Update::DoNothing, "no hook is installed, so nothing to redraw");
        let margins = pushed_margins(&changes);
        assert_eq!(
            margins.len(),
            1,
            "exactly one thumb move per press, got {changes:?}",
        );
        let (node_id, margin) = margins[0];
        assert_eq!(node_id, NodeId::new(1), "the margin landed on the track, not the thumb");
        assert_eq!(margin, (0.5 * TRAVEL).round(), "the thumb went to the wrong pixel");
    }

    #[test]
    fn a_press_never_slides_the_thumb_off_the_rail() {
        for x in [-1.0e9_f32, -1.0, 0.0, 37.0, 199.0, 200.0, 1.0e9, f32::INFINITY] {
            let (_, changes, _) = press_at(Slider::create(0.0, 0.0, 100.0), x);
            for (_, margin) in pushed_margins(&changes) {
                assert!(
                    (0.0..=TRAVEL).contains(&margin),
                    "a press at x = {x} put the thumb at {margin}, off a {TRAVEL}px rail",
                );
            }
        }
    }

    #[test]
    fn a_press_uses_the_real_track_width_once_the_node_is_laid_out() {
        // A slider stretched (or shrunk) by its container must map the cursor
        // against the *laid-out* width, not the 200px design width — otherwise
        // the value jumps as soon as the layout differs from the design.
        for (width, x, expected) in [
            (400.0_f32, 100.0_f32, 25.0_f32),
            (400.0, 400.0, 100.0),
            (100.0, 50.0, 50.0),
            (50.0, 200.0, 100.0), // clamped: cursor past the (short) track
        ] {
            let (styled, state) = wired(Slider::create(0.0, 0.0, 100.0));
            let layout = laid_out_at(styled, LogicalSize::new(width, TRACK_HEIGHT as f32));
            let (_, _) = with_info(layout, node(0), cursor(x, 8.0), |info| {
                on_slider_pointer_down(state.clone(), *info)
            });
            assert_eq!(
                read_state(&state).inner.value,
                expected,
                "a {width}px-wide track mapped x = {x} wrongly",
            );
        }
    }

    #[test]
    fn a_zero_width_track_falls_back_to_the_design_width_instead_of_dividing_by_zero() {
        // A collapsed track would make `pos.x / 0.0` = ±inf (or NaN at x = 0);
        // the `> 0.0` filter is what keeps the value finite.
        let (styled, state) = wired(Slider::create(0.0, 0.0, 100.0));
        let layout = laid_out_at(styled, LogicalSize::new(0.0, TRACK_HEIGHT as f32));
        with_info(layout, node(0), cursor(100.0, 8.0), |info| {
            on_slider_pointer_down(state.clone(), *info)
        });
        let value = read_state(&state).inner.value;
        assert!(value.is_finite(), "a zero-width track produced {value}");
        assert_eq!(value, 50.0, "the fallback must be the 200px design width");
    }

    #[test]
    fn a_press_without_a_cursor_latches_the_drag_but_changes_nothing() {
        // Touch/synthetic events can arrive with no cursor position at all.
        let (styled, state) = wired(Slider::create(60.0, 0.0, 100.0));
        let (update, changes) = with_info(
            unlaid(styled),
            node(0),
            OptionLogicalPosition::None,
            |info| on_slider_pointer_down(state.clone(), *info),
        );
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "the thumb moved without a cursor: {changes:?}");
        let s = read_state(&state);
        assert_eq!(s.inner.value, 60.0, "the value changed without a cursor");
        assert!(s.dragging, "the press must still arm the drag");
    }

    #[test]
    fn a_nan_cursor_does_not_panic_and_leaves_the_thumb_parked() {
        let (styled, state) = wired(Slider::create(10.0, 0.0, 100.0));
        let (update, changes) = with_info(
            unlaid(styled),
            node(0),
            cursor(f32::NAN, f32::NAN),
            |info| on_slider_pointer_down(state.clone(), *info),
        );
        assert_eq!(update, Update::DoNothing);
        // NaN survives the clamp, so the value goes NaN — but the *pixel* margin
        // saturates to 0 rather than becoming a garbage offset.
        assert!(read_state(&state).inner.value.is_nan());
        assert_eq!(pushed_margins(&changes), vec![(NodeId::new(1), 0.0)]);
    }

    #[test]
    fn a_press_on_an_unknown_node_still_updates_the_value_but_moves_no_thumb() {
        // Hit nodes come from hit-testing, which can name a node this DOM does
        // not have (stale frame) or no node at all.
        for hit in [node_none(), node(99), node(usize::MAX - 1)] {
            let (styled, state) = wired(Slider::create(0.0, 0.0, 100.0));
            let (update, changes) = with_info(unlaid(styled), hit, cursor(100.0, 8.0), |info| {
                on_slider_pointer_down(state.clone(), *info)
            });
            assert_eq!(update, Update::DoNothing);
            assert_eq!(read_state(&state).inner.value, 50.0, "hit {hit:?}");
            assert!(
                pushed_margins(&changes).is_empty(),
                "a thumb was moved for a node that does not exist: {changes:?}",
            );
        }
    }

    #[test]
    fn a_move_is_ignored_until_a_press_starts_the_drag() {
        // Hover fires constantly; without the `dragging` latch the slider would
        // follow the cursor across a hover with no button held.
        let (styled, state) = wired(Slider::create(10.0, 0.0, 100.0));
        let (update, changes) = with_info(unlaid(styled), node(0), cursor(200.0, 8.0), |info| {
            on_slider_pointer_move(state.clone(), *info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a hover moved the thumb: {changes:?}");
        assert_eq!(read_state(&state).inner.value, 10.0, "a hover changed the value");
    }

    #[test]
    fn a_move_tracks_the_cursor_once_the_press_armed_the_drag() {
        let slider = Slider::create(0.0, 0.0, 100.0);
        let (styled, state) = wired(slider.clone());
        with_info(unlaid(styled), node(0), cursor(0.0, 8.0), |info| {
            on_slider_pointer_down(state.clone(), *info)
        });
        assert!(read_state(&state).dragging, "the press did not arm the drag");

        let styled2 = StyledDom::create_from_dom(slider.dom());
        let (update, changes) = with_info(unlaid(styled2), node(0), cursor(150.0, 8.0), |info| {
            on_slider_pointer_move(state.clone(), *info)
        });
        assert_eq!(update, Update::DoNothing);
        assert_eq!(read_state(&state).inner.value, 75.0, "the drag did not track the cursor");
        assert_eq!(pushed_margins(&changes), vec![(NodeId::new(1), (0.75 * TRAVEL).round())]);
    }

    #[test]
    fn a_release_ends_the_drag_and_records_nothing() {
        let (styled, state) = wired(Slider::create(0.0, 0.0, 100.0));
        with_info(unlaid(styled), node(0), cursor(100.0, 8.0), |info| {
            on_slider_pointer_down(state.clone(), *info)
        });
        assert!(read_state(&state).dragging);

        let styled2 = StyledDom::create_from_dom(Slider::create(0.0, 0.0, 100.0).dom());
        let (update, changes) = with_info(unlaid(styled2), node(0), cursor(0.0, 8.0), |info| {
            on_slider_pointer_up(state.clone(), *info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "the release moved the thumb: {changes:?}");
        let s = read_state(&state);
        assert!(!s.dragging, "the drag outlived the release");
        assert_eq!(s.inner.value, 50.0, "the release rewrote the value");
    }

    #[test]
    fn a_release_is_idempotent_and_safe_before_any_press() {
        let (styled, state) = wired(Slider::create(0.0, 0.0, 100.0));
        with_info(unlaid(styled), node(0), OptionLogicalPosition::None, |info| {
            on_slider_pointer_up(state.clone(), *info)
        });
        assert!(!read_state(&state).dragging);
        let styled2 = StyledDom::create_from_dom(Slider::create(0.0, 0.0, 100.0).dom());
        with_info(unlaid(styled2), node(0), OptionLogicalPosition::None, |info| {
            on_slider_pointer_up(state.clone(), *info)
        });
        assert!(!read_state(&state).dragging);
    }

    #[test]
    fn every_pointer_handler_ignores_a_foreign_payload() {
        // A mis-wired DOM (or an FFI caller passing the wrong `RefAny`) must be
        // a no-op, not a downcast panic.
        for handler in [
            on_slider_pointer_down as extern "C" fn(RefAny, CallbackInfo) -> Update,
            on_slider_pointer_move,
            on_slider_pointer_up,
        ] {
            let foreign = RefAny::new(0xABCD_u32);
            let styled = StyledDom::create_from_dom(Slider::create(0.0, 0.0, 100.0).dom());
            let (update, changes) =
                with_info(unlaid(styled), node(0), cursor(100.0, 8.0), |info| {
                    handler(foreign.clone(), *info)
                });
            assert_eq!(update, Update::DoNothing);
            assert!(changes.is_empty(), "a foreign payload still mutated the DOM: {changes:?}");
            let mut foreign = foreign;
            assert_eq!(
                *foreign.downcast_ref::<u32>().expect("the payload was replaced"),
                0xABCD,
            );
        }
    }

    #[test]
    fn a_press_forwards_the_hooks_update_verbatim() {
        for (cb, expected) in [
            (value_do_nothing as SliderOnValueChangeCallbackType, Update::DoNothing),
            (record_value as SliderOnValueChangeCallbackType, Update::RefreshDom),
            (value_refresh_all as SliderOnValueChangeCallbackType, Update::RefreshDomAllWindows),
        ] {
            let (styled, state) =
                wired(Slider::create(0.0, 0.0, 100.0).with_on_value_change(log_refany(), cb));
            let (update, _) = with_info(unlaid(styled), node(0), cursor(100.0, 8.0), |info| {
                on_slider_pointer_down(state.clone(), *info)
            });
            assert_eq!(update, expected, "the handler swallowed {expected:?}");
        }
    }

    #[test]
    fn the_hook_is_told_the_new_value_not_the_old_one() {
        // Passing `SliderState::default()` (or the pre-press value) would
        // type-check and would look right for exactly one press position.
        let probe = log_refany();
        let (styled, state) = wired(Slider::create(0.0, -10.0, 90.0).with_on_value_change(
            probe.clone(),
            record_value as SliderOnValueChangeCallbackType,
        ));
        let (update, _) = with_info(unlaid(styled), node(0), cursor(50.0, 8.0), |info| {
            on_slider_pointer_down(state.clone(), *info)
        });
        assert_eq!(update, Update::RefreshDom);
        assert_eq!(
            read_log(&probe).seen,
            vec![SliderState {
                value: expected_value(50.0, DESIGN_WIDTH, -10.0, 90.0),
                min: -10.0,
                max: 90.0,
            }],
        );
    }

    #[test]
    fn the_hook_is_not_called_when_there_is_no_cursor() {
        let probe = log_refany();
        let (styled, state) = wired(Slider::create(0.0, 0.0, 100.0).with_on_value_change(
            probe.clone(),
            record_value as SliderOnValueChangeCallbackType,
        ));
        with_info(unlaid(styled), node(0), OptionLogicalPosition::None, |info| {
            on_slider_pointer_down(state.clone(), *info)
        });
        assert!(read_log(&probe).seen.is_empty(), "the hook fired without a cursor");
    }

    #[test]
    fn the_hook_is_not_called_on_release() {
        let probe = log_refany();
        let (styled, state) = wired(Slider::create(0.0, 0.0, 100.0).with_on_value_change(
            probe.clone(),
            record_value as SliderOnValueChangeCallbackType,
        ));
        with_info(unlaid(styled), node(0), cursor(100.0, 8.0), |info| {
            on_slider_pointer_up(state.clone(), *info)
        });
        assert!(read_log(&probe).seen.is_empty(), "the release reported a value change");
    }

    #[test]
    fn apply_cursor_value_tolerates_a_degenerate_range_without_panicking() {
        // Unlike `create`/`set_value`, this path never calls `f32::clamp` on the
        // bounds — it interpolates. That means an inverted range is survivable
        // here (the value just runs backwards), which is exactly why the panic in
        // the constructor is worth fixing rather than accepting.
        for (min, max) in DEGENERATE_RANGES {
            let mut wrapper = SliderStateWrapper {
                inner: SliderState { value: 0.0, min, max },
                ..Default::default()
            };
            let styled = StyledDom::create_from_dom(Slider::create(0.0, 0.0, 100.0).dom());
            let (update, changes) =
                with_info(unlaid(styled), node(0), cursor(100.0, 8.0), |info| {
                    apply_cursor_value(&mut wrapper, info)
                });
            assert_eq!(update, Update::DoNothing);
            let v = wrapper.inner.value;
            assert!(
                v.is_nan() || (v >= min.min(max) && v <= min.max(max)),
                "[{min}, {max}] interpolated to {v}, outside the bounds in either order",
            );
            // The thumb still stays on the rail whatever the bounds say — the
            // margin depends only on the cursor fraction, not on the range.
            for (_, margin) in pushed_margins(&changes) {
                assert!(
                    (0.0..=TRAVEL).contains(&margin),
                    "[{min}, {max}] slid the thumb to {margin}",
                );
            }
        }
    }

    #[test]
    fn apply_cursor_value_keeps_the_value_inside_any_sane_range() {
        // The documented invariant on `SliderState::value`: "always within
        // [min, max]". `apply_cursor_value` writes it without a clamp, relying
        // purely on the cursor fraction being in [0, 1].
        for (min, max) in SANE_RANGES {
            for x in [-1.0e9_f32, -1.0, 0.0, 73.0, 200.0, 1.0e9] {
                let mut wrapper = SliderStateWrapper {
                    inner: SliderState { value: min, min, max },
                    ..Default::default()
                };
                let styled = StyledDom::create_from_dom(Slider::create(0.0, 0.0, 100.0).dom());
                with_info(unlaid(styled), node(0), cursor(x, 8.0), |info| {
                    apply_cursor_value(&mut wrapper, info)
                });
                let v = wrapper.inner.value;
                assert!(
                    (min..=max).contains(&v),
                    "a press at x = {x} put the value at {v}, outside [{min}, {max}]",
                );
            }
        }
    }

    #[test]
    fn apply_cursor_value_is_idempotent_for_a_stationary_cursor() {
        let mut wrapper = SliderStateWrapper {
            inner: SliderState { value: 0.0, min: 0.0, max: 100.0 },
            ..Default::default()
        };
        for _ in 0..3 {
            let styled = StyledDom::create_from_dom(Slider::create(0.0, 0.0, 100.0).dom());
            with_info(unlaid(styled), node(0), cursor(100.0, 8.0), |info| {
                apply_cursor_value(&mut wrapper, info)
            });
            assert_eq!(wrapper.inner.value, 50.0);
        }
    }

    #[test]
    fn deliver_smoke_test_covers_every_handler_and_layout_combination() {
        // A last sweep: every handler x {laid out, not laid out} x hostile
        // cursors, asserting only that nothing unwinds and the state stays a
        // `SliderStateWrapper`.
        for handler in [
            on_slider_pointer_down as extern "C" fn(RefAny, CallbackInfo) -> Update,
            on_slider_pointer_move,
            on_slider_pointer_up,
        ] {
            for track in [None, Some(LogicalSize::new(0.0, 0.0)), Some(LogicalSize::new(1.0e9, 16.0))] {
                for at in [
                    OptionLogicalPosition::None,
                    cursor(0.0, 0.0),
                    cursor(-1.0e9, 0.0),
                    cursor(f32::NAN, 0.0),
                    cursor(f32::INFINITY, 0.0),
                ] {
                    let slider = Slider::create(0.0, 0.0, 100.0);
                    let (_, state) = wired(slider.clone());
                    let (_, changes) =
                        deliver(slider, &state, node(0), at, track, handler);
                    // Whatever happened, the shared state must still be readable.
                    let _ = read_state(&state);
                    for (_, margin) in pushed_margins(&changes) {
                        assert!(
                            margin.is_finite(),
                            "a non-finite thumb offset ({margin}) reached the DOM",
                        );
                    }
                }
            }
        }
    }
}
