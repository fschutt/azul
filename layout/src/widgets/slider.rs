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
    let margin = (fraction * (TRACK_WIDTH - THUMB_SIZE) as f32).round() as isize;
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

impl Slider {
    /// Creates a slider with the given current value and `[min, max]` range.
    #[must_use] pub fn create(value: f32, min: f32, max: f32) -> Self {
        let value = value.clamp(min, max);
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
        let value = value.clamp(min, max);
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
