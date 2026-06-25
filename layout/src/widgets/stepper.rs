//! Stepper / wizard widget — a horizontal multi-step progress indicator: a row
//! of numbered step circles with labels, joined by connector lines. Completed
//! and current steps are highlighted in the accent colour; upcoming steps (and
//! the connectors that lead to them) are muted.
//!
//! This is a blend of [`crate::widgets::segmented::Segmented`] (a horizontal row
//! of clickable items whose clicked index is derived from sibling position and
//! whose active item is live-restyled via `set_css_property`) and the filled-track
//! look of [`crate::widgets::progressbar::ProgressBar`] (the accent connector).
//!
//! Steps are CLICKABLE (free navigation, like a segmented control): clicking
//! step `i` sets `current_step = i`, invokes the optional `on_step_change(state)`,
//! and live-restyles every circle / connector / label to reflect the new
//! position — no DOM rebuild. (A non-clickable, display-only stepper is also a
//! valid design; this widget chooses clickable to exercise the segmented restyle
//! pattern, and `set_current_step` still drives it from app code on rebuild.)
//!
//! A circle is "reached" (accent) iff its index `<= current_step`; the connector
//! gap between circle `i` and `i+1` is accent iff `i < current_step`. Clicking the
//! already-current step is a no-op (no callback).
//!
//! Key types: [`Stepper`], [`StepperState`], [`StepperOnStepChange`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, PixelValue, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutFlexGrow, LayoutFlexBasis, LayoutWidth, LayoutJustifyContent, LayoutHeight, LayoutMinWidth, LayoutPaddingTop},
        property::{CssProperty, LayoutFlexBasisValue, LayoutWidthValue},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleCursor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextAlign, StyleUserSelect, StyleTextColor},
    },
    impl_option_inner, AzString, StringVec,
};

use crate::callbacks::CallbackInfo;

static STEPPER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-stepper"))];
static STEPPER_STEP_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-stepper-step"))];
static STEPPER_ROW_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-stepper-row"))];
static STEPPER_CIRCLE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-stepper-circle"))];
static STEPPER_CONNECTOR_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-stepper-connector"))];
static STEPPER_LABEL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-stepper-label"))];

/// Callback function type invoked when the current step changes.
pub type StepperOnStepChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, StepperState) -> Update;
impl_widget_callback!(
    StepperOnStepChange,
    OptionStepperOnStepChange,
    StepperOnStepChangeCallback,
    StepperOnStepChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        StepperOnStepChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: STEPPER_ON_STEP_CHANGE_INVOKER,
    invoker_ty:     AzStepperOnStepChangeCallbackInvoker,
    thunk_fn:       az_stepper_on_step_change_callback_thunk,
    setter_fn:      AzApp_setStepperOnStepChangeCallbackInvoker,
    from_handle_fn: AzStepperOnStepChangeCallback_createFromHostHandle,
    extra_args:     [ state: StepperState ],
}

/// A horizontal numbered-step progress indicator with a step-change callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Stepper {
    pub stepper_state: StepperStateWrapper,
    /// The label of each step, in order. The step count is `labels.len()`.
    pub labels: StringVec,
    /// Style for the row container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct StepperStateWrapper {
    /// The current step + total step count.
    pub inner: StepperState,
    /// Optional: function to call when the current step changes.
    pub on_step_change: OptionStepperOnStepChange,
}

/// State of a [`Stepper`]: the zero-based current step and the total step count.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct StepperState {
    /// Zero-based index of the current (active) step.
    pub current_step: usize,
    /// Total number of steps.
    pub total_steps: usize,
}

// ---- colours ----
/// Accent (reached/current) colour (#0d6efd).
const ACCENT_COLOR: ColorU = ColorU { r: 13, g: 110, b: 253, a: 255 };
/// Accent text colour (white) — the number inside a reached circle.
const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
/// Upcoming-circle background (#e9ecef, light grey).
const MUTED_CIRCLE_COLOR: ColorU = ColorU { r: 233, g: 236, b: 239, a: 255 };
/// Muted text colour (#868e96) — upcoming numbers/labels.
const MUTED_TEXT_COLOR: ColorU = ColorU { r: 134, g: 142, b: 150, a: 255 };
/// Reached-label text colour (#212529, dark).
const DARK_TEXT_COLOR: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };
/// Upcoming-connector colour (#ced4da).
const CONNECTOR_MUTED_COLOR: ColorU = ColorU { r: 206, g: 212, b: 218, a: 255 };
/// Transparent — used for the (absent) connector at the row's two ends.
const TRANSPARENT_COLOR: ColorU = ColorU { r: 0, g: 0, b: 0, a: 0 };

const ACCENT_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(ACCENT_COLOR)];
const ACCENT_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(ACCENT_BG_ITEMS);
const MUTED_CIRCLE_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(MUTED_CIRCLE_COLOR)];
const MUTED_CIRCLE_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(MUTED_CIRCLE_BG_ITEMS);
const CONNECTOR_MUTED_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(CONNECTOR_MUTED_COLOR)];
const CONNECTOR_MUTED_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(CONNECTOR_MUTED_BG_ITEMS);
const TRANSPARENT_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(TRANSPARENT_COLOR)];
const TRANSPARENT_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(TRANSPARENT_BG_ITEMS);

const CIRCLE_SIZE: isize = 28;
const CIRCLE_RADIUS: isize = 14;
const CONNECTOR_HEIGHT: isize = 2;

/// Connector fill state for one half-segment.
#[derive(Copy, Clone)]
enum ConnFill {
    /// Reached (accent).
    Accent,
    /// Not reached (muted grey).
    Muted,
    /// At a row end — drawn transparent so the line doesn't stick out.
    Hidden,
}

impl ConnFill {
    const fn bg(self) -> StyleBackgroundContentVec {
        match self {
            Self::Accent => ACCENT_BG,
            Self::Muted => CONNECTOR_MUTED_BG,
            Self::Hidden => TRANSPARENT_BG,
        }
    }
}

/// Row container: a horizontal flex row whose steps spread evenly.
static STEPPER_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// One step cell: a vertical flex column (indicator row over label) that grows to
/// an equal share of the row (`flex-grow: 1; flex-basis: 0`).
static STEPPER_STEP_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::FlexBasis(LayoutFlexBasisValue::Exact(
        LayoutFlexBasis::Exact(PixelValue::const_px(0)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// Builds the indicator-row style: a full-width flex row that vertically centres
/// the connectors (height `CONNECTOR_HEIGHT`) on the circle.
fn row_style() -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        // Full cell width so the flex-grow connectors actually have space to fill.
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::percent(100.0)),
        ))),
    ])
}

/// Builds the style for one numbered circle. Background + number colour are the
/// only reached-dependent properties.
fn circle_style(reached: bool) -> CssPropertyWithConditionsVec {
    let (bg, text) = if reached {
        (ACCENT_BG, WHITE)
    } else {
        (MUTED_CIRCLE_BG, MUTED_TEXT_COLOR)
    };
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_justify_content(
            LayoutJustifyContent::Center,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(
            CIRCLE_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            CIRCLE_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
            CIRCLE_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(CIRCLE_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(CIRCLE_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(CIRCLE_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(CIRCLE_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg)),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
    ])
}

/// Builds the style for one connector half-line (left or right of a circle).
fn connector_style(fill: ConnFill) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            CONNECTOR_HEIGHT,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(fill.bg())),
    ])
}

/// Builds the style for one step label.
fn label_style(reached: bool) -> CssPropertyWithConditionsVec {
    let text = if reached { DARK_TEXT_COLOR } else { MUTED_TEXT_COLOR };
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            6,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
    ])
}

/// Connector fill for the left half-line of step `i` (the gap entering circle `i`).
const fn conn_left_fill(i: usize, current: usize) -> ConnFill {
    if i == 0 {
        ConnFill::Hidden
    } else if i <= current {
        ConnFill::Accent
    } else {
        ConnFill::Muted
    }
}

/// Connector fill for the right half-line of step `i` (the gap leaving circle `i`).
const fn conn_right_fill(i: usize, last: usize, current: usize) -> ConnFill {
    if i == last {
        ConnFill::Hidden
    } else if i < current {
        ConnFill::Accent
    } else {
        ConnFill::Muted
    }
}

impl Stepper {
    /// Creates a stepper from the given step labels, with the first step current.
    #[must_use] pub fn create(labels: StringVec) -> Self {
        let total_steps = labels.as_ref().len();
        Self {
            stepper_state: StepperStateWrapper {
                inner: StepperState {
                    current_step: 0,
                    total_steps,
                },
                ..Default::default()
            },
            labels,
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                STEPPER_CONTAINER_STYLE,
            ),
        }
    }

    /// Sets the current (zero-based) step, clamped into `[0, total_steps - 1]`.
    #[inline]
    pub fn set_current_step(&mut self, current_step: usize) {
        let total = self.stepper_state.inner.total_steps;
        self.stepper_state.inner.current_step = if total == 0 {
            0
        } else {
            current_step.min(total - 1)
        };
    }

    /// Builder-style setter for the current step.
    #[inline]
    #[must_use] pub fn with_current_step(mut self, current_step: usize) -> Self {
        self.set_current_step(current_step);
        self
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_step_change<C: Into<StepperOnStepChangeCallback>>(
        &mut self,
        data: RefAny,
        on_step_change: C,
    ) {
        self.stepper_state.on_step_change = Some(StepperOnStepChange {
            callback: on_step_change.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use] pub fn with_on_step_change<C: Into<StepperOnStepChangeCallback>>(
        mut self,
        data: RefAny,
        on_step_change: C,
    ) -> Self {
        self.set_on_step_change(data, on_step_change);
        self
    }

    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let current = self.stepper_state.inner.current_step;
        let count = self.labels.as_ref().len();
        let last = count.saturating_sub(1);

        // One shared RefAny across every step's callback (RefAny::clone shares the
        // underlying state — same pattern as segmented/pagination/map).
        let state = RefAny::new(self.stepper_state);

        let mut children: Vec<Dom> = Vec::with_capacity(count);
        for (i, label) in self.labels.as_ref().iter().enumerate() {
            let reached = i <= current;

            // Indicator row: [connector-left, circle, connector-right].
            let row = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(STEPPER_ROW_CLASS))
                .with_css_props(row_style())
                .with_children(
                    vec![
                        Dom::create_div()
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                                STEPPER_CONNECTOR_CLASS,
                            ))
                            .with_css_props(connector_style(conn_left_fill(i, current))),
                        Dom::create_text(AzString::from(format!("{}", i + 1).as_str()))
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                                STEPPER_CIRCLE_CLASS,
                            ))
                            .with_css_props(circle_style(reached)),
                        Dom::create_div()
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                                STEPPER_CONNECTOR_CLASS,
                            ))
                            .with_css_props(connector_style(conn_right_fill(i, last, current))),
                    ]
                    .into(),
                );

            let cell = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(STEPPER_STEP_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    STEPPER_STEP_STYLE,
                ))
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: on_step_click as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: state.clone(),
                    }]
                    .into(),
                )
                .with_tab_index(TabIndex::Auto)
                .with_children(
                    vec![
                        row,
                        Dom::create_text(label.clone())
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                                STEPPER_LABEL_CLASS,
                            ))
                            .with_css_props(label_style(reached)),
                    ]
                    .into(),
                );

            children.push(cell);
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(STEPPER_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for Stepper {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

/// Click handler shared by all step cells. Resolves the clicked cell from its
/// sibling position (= the zero-based step index), and — only if the step
/// actually changed — updates the state, invokes the user callback, and
/// live-restyles every circle / connector / label (the segmented pattern).
extern "C" fn on_step_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::dom::DomNodeId;

    let clicked = info.get_hit_node();
    let Some(parent) = info.get_parent(clicked) else {
        return Update::DoNothing;
    };

    // Collect the step cells in document order.
    let mut cells: Vec<DomNodeId> = Vec::new();
    let mut cur = info.get_first_child(parent);
    while let Some(node) = cur {
        cells.push(node);
        cur = info.get_next_sibling(node);
    }
    let count = cells.len();
    if count == 0 {
        return Update::DoNothing;
    }
    let last = count - 1;

    let Some(clicked_idx) = cells.iter().position(|n| *n == clicked) else {
        return Update::DoNothing;
    };

    let current = {
        let Some(st) = data.downcast_ref::<StepperStateWrapper>() else {
            return Update::DoNothing;
        };
        st.inner.current_step
    };
    if clicked_idx == current {
        // Clicked the already-current step — no change, no callback.
        return Update::DoNothing;
    }

    let result = {
        let Some(mut st) = data.downcast_mut::<StepperStateWrapper>() else {
            return Update::DoNothing;
        };
        st.inner.current_step = clicked_idx;
        let inner = st.inner;
        let st = &mut *st;
        match st.on_step_change.as_mut() {
            Some(StepperOnStepChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        }
    };

    // Live-restyle every cell: circle (reached → accent fill + white number),
    // its two connector half-lines, and its label colour.
    for (i, cell) in cells.iter().enumerate() {
        let reached = i <= clicked_idx;

        let Some(row) = info.get_first_child(*cell) else {
            continue;
        };
        let conn_left = info.get_first_child(row);
        let circle = conn_left.and_then(|cl| info.get_next_sibling(cl));
        let conn_right = circle.and_then(|c| info.get_next_sibling(c));
        let label = info.get_next_sibling(row);

        if let Some(circle) = circle {
            let (bg, text) = if reached {
                (ACCENT_BG, WHITE)
            } else {
                (MUTED_CIRCLE_BG, MUTED_TEXT_COLOR)
            };
            info.set_css_property(circle, CssProperty::const_background_content(bg));
            info.set_css_property(
                circle,
                CssProperty::const_text_color(StyleTextColor { inner: text }),
            );
        }
        if let Some(cl) = conn_left {
            info.set_css_property(
                cl,
                CssProperty::const_background_content(conn_left_fill(i, clicked_idx).bg()),
            );
        }
        if let Some(cr) = conn_right {
            info.set_css_property(
                cr,
                CssProperty::const_background_content(conn_right_fill(i, last, clicked_idx).bg()),
            );
        }
        if let Some(label) = label {
            let text = if reached { DARK_TEXT_COLOR } else { MUTED_TEXT_COLOR };
            info.set_css_property(
                label,
                CssProperty::const_text_color(StyleTextColor { inner: text }),
            );
        }
    }

    result
}

impl From<Stepper> for Dom {
    fn from(s: Stepper) -> Self {
        s.dom()
    }
}
