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
    impl_option_inner,
    props::{
        basic::{color::ColorU, PixelValue, StyleFontSize},
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexBasis, LayoutFlexDirection, LayoutFlexGrow,
            LayoutHeight, LayoutJustifyContent, LayoutMinWidth, LayoutPaddingTop, LayoutWidth,
        },
        property::{CssProperty, LayoutFlexBasisValue, LayoutWidthValue},
        style::{
            StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderBottomLeftRadius,
            StyleBorderBottomRightRadius, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
            StyleCursor, StyleTextAlign, StyleTextColor, StyleUserSelect,
        },
    },
    AzString, StringVec,
};

use crate::callbacks::CallbackInfo;

static STEPPER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-stepper"))];
static STEPPER_STEP_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-stepper-step",
))];
static STEPPER_ROW_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-stepper-row"))];
static STEPPER_CIRCLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-stepper-circle",
))];
static STEPPER_CONNECTOR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-stepper-connector",
))];
static STEPPER_LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-stepper-label",
))];

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
const ACCENT_COLOR: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};
/// Accent text colour (white) — the number inside a reached circle.
const WHITE: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Upcoming-circle background (#e9ecef, light grey).
const MUTED_CIRCLE_COLOR: ColorU = ColorU {
    r: 233,
    g: 236,
    b: 239,
    a: 255,
};
/// Muted text colour (#868e96) — upcoming numbers/labels.
const MUTED_TEXT_COLOR: ColorU = ColorU {
    r: 134,
    g: 142,
    b: 150,
    a: 255,
};
/// Reached-label text colour (#212529, dark).
const DARK_TEXT_COLOR: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
/// Upcoming-connector colour (#ced4da).
const CONNECTOR_MUTED_COLOR: ColorU = ColorU {
    r: 206,
    g: 212,
    b: 218,
    a: 255,
};
/// Transparent — used for the (absent) connector at the row's two ends.
const TRANSPARENT_COLOR: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

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
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
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
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
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
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
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
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            1,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            CONNECTOR_HEIGHT,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(fill.bg())),
    ])
}

/// Builds the style for one step label.
fn label_style(reached: bool) -> CssPropertyWithConditionsVec {
    let text = if reached {
        DARK_TEXT_COLOR
    } else {
        MUTED_TEXT_COLOR
    };
    CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            12,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(6),
        )),
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
    #[must_use]
    pub fn create(labels: StringVec) -> Self {
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
    #[must_use]
    pub fn with_current_step(mut self, current_step: usize) -> Self {
        self.set_current_step(current_step);
        self
    }

    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
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
    #[must_use]
    pub fn with_on_step_change<C: Into<StepperOnStepChangeCallback>>(
        mut self,
        data: RefAny,
        on_step_change: C,
    ) -> Self {
        self.set_on_step_change(data, on_step_change);
        self
    }

    #[must_use]
    pub fn dom(self) -> Dom {
        // Read before the state is moved into the callbacks below.
        let step_now = self.stepper_state.inner.current_step;
        let steps_total = self.stepper_state.inner.total_steps;

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
                        crate::widgets::widget_p_with_text(AzString::from(
                            format!("{}", i + 1).as_str(),
                        ))
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(STEPPER_CIRCLE_CLASS))
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
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: on_step_click as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: state.clone(),
                    }]
                    .into(),
                )
                .with_tab_index(TabIndex::Auto)
                // A stepper is a spin button: the VALUE is which step you are
                // on, and "step 2 of 5" is the entire content of the control.
                .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                    role: azul_core::a11y::AccessibilityRole::SpinButton,
                    accessibility_value: Some(AzString::from(alloc::format!(
                        "step {} of {}",
                        step_now.saturating_add(1),
                        steps_total
                    )))
                    .into(),
                    ..Default::default()
                })
                .with_children(
                    vec![
                        row,
                        crate::widgets::widget_p_with_text(label.clone())
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
            let text = if reached {
                DARK_TEXT_COLOR
            } else {
                MUTED_TEXT_COLOR
            };
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

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap, HashSet},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{
        props::basic::{length::SizeMetric, pixel::PixelValue},
        system::SystemStyle,
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Const-evaluated extremes
    //
    // `conn_left_fill` / `conn_right_fill` are `const fn`, so evaluating them at
    // the integer extremes in a `const` item makes the compiler itself prove
    // there is no overflow / no const-eval panic on the whole `usize` range.
    // ------------------------------------------------------------------

    const _LEFT_AT_MAX: ConnFill = conn_left_fill(usize::MAX, usize::MAX);
    const _LEFT_MAX_AT_ZERO: ConnFill = conn_left_fill(usize::MAX, 0);
    const _LEFT_ZERO_AT_MAX: ConnFill = conn_left_fill(0, usize::MAX);
    const _RIGHT_AT_MAX: ConnFill = conn_right_fill(usize::MAX, usize::MAX, usize::MAX);
    const _RIGHT_MAX_AT_ZERO: ConnFill = conn_right_fill(usize::MAX, 0, 0);
    const _RIGHT_ZERO_AT_MAX: ConnFill = conn_right_fill(0, usize::MAX, usize::MAX);

    // ------------------------------------------------------------------
    // Flattened node layout
    //
    // `convert_dom_into_compact_dom` walks the tree in pre-order, and one step
    // cell contributes exactly eight nodes — the circle and the label are both
    // `<p>` boxes wrapping one bare text node each (the widget label convention):
    //
    //     cell → row → [conn-left, circle <p> → text, conn-right] , label <p> → text
    //
    // so step `i` occupies `1 + 8*i ..= 8 + 8*i`. `flattened_layout_is_eight_nodes_per_step`
    // pins this against the real hierarchy so the click tests below cannot drift.
    // ------------------------------------------------------------------

    const NODES_PER_STEP: usize = 8;

    fn cell_node(i: usize) -> usize {
        1 + NODES_PER_STEP * i
    }
    fn row_node(i: usize) -> usize {
        2 + NODES_PER_STEP * i
    }
    fn conn_left_node(i: usize) -> usize {
        3 + NODES_PER_STEP * i
    }
    /// The circle's `<p>` box — the node the restyle writes to.
    fn circle_node(i: usize) -> usize {
        4 + NODES_PER_STEP * i
    }
    fn conn_right_node(i: usize) -> usize {
        6 + NODES_PER_STEP * i
    }
    /// The label's `<p>` box — the node the restyle writes to.
    fn label_node(i: usize) -> usize {
        7 + NODES_PER_STEP * i
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn labels(v: &[&str]) -> StringVec {
        StringVec::from_vec(v.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    /// `n` distinct labels: `s0, s1, … s{n-1}`.
    fn n_labels(n: usize) -> StringVec {
        StringVec::from_vec(
            (0..n)
                .map(|i| AzString::from(format!("s{i}")))
                .collect::<Vec<_>>(),
        )
    }

    fn stepper(v: &[&str]) -> Stepper {
        Stepper::create(labels(v))
    }

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The *kind* of every declared property, in order (ignores the values).
    fn property_kinds(
        v: &CssPropertyWithConditionsVec,
    ) -> Vec<core::mem::Discriminant<CssProperty>> {
        v.as_ref()
            .iter()
            .map(|p| core::mem::discriminant(&p.property))
            .collect()
    }

    /// Asserts a style vec declares each property kind at most once and attaches
    /// no selector conditions. A duplicate is silently last-wins (hiding a value
    /// conflict); a stray condition leaves the node unstyled until some selector
    /// state happens to match.
    fn assert_unconditional_and_unique(v: &CssPropertyWithConditionsVec, ctx: &str) {
        let mut seen = HashSet::new();
        for p in v.as_ref() {
            assert!(
                p.apply_if.as_ref().is_empty(),
                "{ctx}: {:?} is conditional, not an unconditional declaration",
                p.property
            );
            assert!(
                seen.insert(core::mem::discriminant(&p.property)),
                "{ctx}: {:?} is declared twice",
                p.property
            );
        }
        assert_eq!(
            seen.len(),
            v.as_ref().len(),
            "{ctx}: declaration count / kind count disagree"
        );
    }

    /// The single background layer of a background vec, asserting there is
    /// exactly one and that it is a flat colour (a gradient would not be `Color`).
    fn only_color(bg: &StyleBackgroundContentVec) -> ColorU {
        assert_eq!(
            bg.as_ref().len(),
            1,
            "a stepper fill must be a single background layer"
        );
        match &bg.as_ref()[0] {
            StyleBackgroundContent::Color(c) => *c,
            other => panic!("stepper background is not a flat colour: {other:?}"),
        }
    }

    /// `ConnFill` derives neither `Debug` nor `PartialEq`, so the three variants
    /// are compared through the one thing they actually control: the colour.
    fn fill_color(f: ConnFill) -> ColorU {
        only_color(&f.bg())
    }

    fn background_color(props: &[CssProperty]) -> Option<ColorU> {
        let found: Vec<&StyleBackgroundContentVec> = props
            .iter()
            .filter_map(|p| match p {
                CssProperty::BackgroundContent(v) => v.get_property(),
                _ => None,
            })
            .collect();
        assert!(
            found.len() <= 1,
            "a stepper node must declare at most one background"
        );
        found.first().map(|bg| only_color(bg))
    }

    fn text_color(props: &[CssProperty]) -> Option<ColorU> {
        let found: Vec<ColorU> = props
            .iter()
            .filter_map(|p| match p {
                CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
                _ => None,
            })
            .collect();
        assert!(
            found.len() <= 1,
            "a stepper node must declare at most one text colour"
        );
        found.first().copied()
    }

    /// The raw `PixelValue` of `width` / `height` (the sizing *enums*), so the
    /// metric can be asserted separately — the circle is absolute `px`, the
    /// indicator row is a `%`.
    fn width_value(props: &[CssProperty]) -> Option<PixelValue> {
        props.iter().find_map(|p| match p {
            CssProperty::Width(v) => match v.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(*pv),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_value(props: &[CssProperty]) -> Option<PixelValue> {
        props.iter().find_map(|p| match p {
            CssProperty::Height(v) => match v.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(*pv),
                _ => None,
            },
            _ => None,
        })
    }

    fn min_width_value(props: &[CssProperty]) -> Option<PixelValue> {
        props.iter().find_map(|p| match p {
            CssProperty::MinWidth(v) => v.get_property().map(|x| x.inner),
            _ => None,
        })
    }

    fn font_size_value(props: &[CssProperty]) -> Option<PixelValue> {
        props.iter().find_map(|p| match p {
            CssProperty::FontSize(v) => v.get_property().map(|x| x.inner),
            _ => None,
        })
    }

    fn padding_top_value(props: &[CssProperty]) -> Option<PixelValue> {
        props.iter().find_map(|p| match p {
            CssProperty::PaddingTop(v) => v.get_property().map(|x| x.inner),
            _ => None,
        })
    }

    fn flex_grow_value(props: &[CssProperty]) -> Option<f32> {
        props.iter().find_map(|p| match p {
            CssProperty::FlexGrow(v) => v.get_property().map(|f| f.inner.get()),
            _ => None,
        })
    }

    /// The four corner radii as `(top-left, top-right, bottom-left, bottom-right)`.
    fn radii(
        props: &[CssProperty],
    ) -> (
        Option<PixelValue>,
        Option<PixelValue>,
        Option<PixelValue>,
        Option<PixelValue>,
    ) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<PixelValue>| props.iter().find_map(f);
        (
            find(&|p| match p {
                CssProperty::BorderTopLeftRadius(v) => v.get_property().map(|r| r.inner),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderTopRightRadius(v) => v.get_property().map(|r| r.inner),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderBottomLeftRadius(v) => v.get_property().map(|r| r.inner),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderBottomRightRadius(v) => v.get_property().map(|r| r.inner),
                _ => None,
            }),
        )
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. An
    /// `em`/`%` slipping into the circle geometry would resolve against the
    /// parent font/box instead of the intended fixed size.
    fn px(pv: PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "stepper geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    /// Perceived brightness (0..=255) of an sRGB colour, Rec.709 weights. Kept to
    /// plain `+`/`*` (no gamma expansion) so the readability assertions stay exact
    /// and toolchain-independent.
    fn luma(c: ColorU) -> f32 {
        0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b)
    }

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match node.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The true recursive descendant count of a `Dom` — what
    /// `estimated_total_children` is documented to cache. Under-counting makes
    /// `convert_dom_into_compact_dom` under-allocate and panic.
    fn recursive_descendants(node: &Dom) -> usize {
        node.children
            .as_ref()
            .iter()
            .map(|c| 1 + recursive_descendants(c))
            .sum()
    }

    fn step_cell(dom: &Dom, i: usize) -> &Dom {
        &dom.children.as_ref()[i]
    }
    fn row_of(cell: &Dom) -> &Dom {
        &cell.children.as_ref()[0]
    }
    fn label_of(cell: &Dom) -> &Dom {
        &cell.children.as_ref()[1]
    }
    fn conn_left_of(row: &Dom) -> &Dom {
        &row.children.as_ref()[0]
    }
    fn circle_of(row: &Dom) -> &Dom {
        &row.children.as_ref()[1]
    }
    fn conn_right_of(row: &Dom) -> &Dom {
        &row.children.as_ref()[2]
    }

    /// Boundary + "negative" step indices. `usize` has no negative values, so a
    /// `-1` handed in through FFI arrives here as `usize::MAX`; both wrapped
    /// forms are included so the setter is exercised at the two's-complement ends.
    fn boundary_indices() -> Vec<usize> {
        vec![
            0,
            1,
            2,
            usize::MAX / 2,
            usize::MAX / 2 + 1,
            usize::MAX - 1,
            usize::MAX,
            (-1i64) as usize,
            i64::MIN as usize,
            u32::MAX as usize,
        ]
    }

    /// Adversarial step labels: empty, whitespace, combining marks, ZWJ emoji,
    /// RTL, embedded NULs (`AzString` is length-based, so a NUL must not
    /// truncate), control characters, and a string far longer than any plausible
    /// step caption.
    fn adversarial_strings() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            "Details",
            " ",
            "e\u{0301}",                                   // e + combining acute
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family emoji
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",                // RTL Hebrew
            "\0",                                          // a single NUL
            "a\0b",                                        // embedded NUL
            "\u{FFFD}\u{202E}\u{200B}",                    // replacement, RTL override, ZWSP
            "line\nbreak\ttab",                            // control characters
            "-9223372036854775808",                        // i64::MIN as a caption
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(100_000));
        v
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn cb(f: StepperOnStepChangeCallbackType) -> StepperOnStepChangeCallback {
        f.into()
    }

    /// A `RefAny` payload recording every state a user `on_step_change` observes.
    struct StepLog {
        seen: Vec<StepperState>,
    }

    extern "C" fn record_step(mut data: RefAny, _: CallbackInfo, state: StepperState) -> Update {
        if let Some(mut log) = data.downcast_mut::<StepLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn step_do_nothing(_: RefAny, _: CallbackInfo, _: StepperState) -> Update {
        Update::DoNothing
    }

    extern "C" fn step_refresh_all(_: RefAny, _: CallbackInfo, state: StepperState) -> Update {
        // `current_step` is read (and discarded) purely so this body cannot be
        // identical-code-folded onto another handler; the tests below compare
        // callback function pointers for equality/inequality.
        let _ = state.current_step;
        Update::RefreshDomAllWindows
    }

    /// A payload whose callback tries to read the *same* `StepperStateWrapper`
    /// `RefAny` that the handler is currently holding a mutable borrow on.
    struct ReentrantProbe {
        /// A clone of the state `RefAny` the handler was invoked with.
        state: RefAny,
        /// `Some(step)` if the re-entrant read succeeded, `None` if it was
        /// refused. Starts as `Some(usize::MAX)` so "never ran" is distinguishable.
        saw_step: Option<usize>,
        calls: usize,
    }

    extern "C" fn probe_state_reentrantly(
        mut data: RefAny,
        _: CallbackInfo,
        _: StepperState,
    ) -> Update {
        if let Some(mut probe) = data.downcast_mut::<ReentrantProbe>() {
            probe.calls += 1;
            let mut state = probe.state.clone();
            probe.saw_step = state
                .downcast_ref::<StepperStateWrapper>()
                .map(|w| w.inner.current_step);
        }
        Update::DoNothing
    }

    fn logged(data: &mut RefAny) -> Vec<StepperState> {
        data.downcast_ref::<StepLog>()
            .expect("payload must still be a StepLog")
            .seen
            .clone()
    }

    fn current_step_of(data: &mut RefAny) -> usize {
        data.downcast_ref::<StepperStateWrapper>()
            .expect("payload must still be a StepperStateWrapper")
            .inner
            .current_step
    }

    /// The `RefAny` carried by step `i`'s click callback.
    fn step_state(dom: &Dom, i: usize) -> RefAny {
        dom.children.as_ref()[i]
            .root
            .get_callbacks()
            .as_ref()
            .first()
            .expect("every step cell must carry the click callback")
            .refany
            .clone()
    }

    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was
    /// hit" case. `CallbackInfo::set_css_property` *panics* on such an id, so the
    /// handler must bail out long before reaching the restyle loop.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` with an *empty* layout tree: `on_step_click` only walks
    /// `styled_dom.node_hierarchy`, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
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

    /// Renders `s`, then hands back both the flattened DOM *and* the very `RefAny`
    /// the widget registered on step 0's mouse-up callback. Driving the handler
    /// with these two is the real wiring — nothing is re-created by hand, so a
    /// mismatch between what `dom()` stores and what the handler expects cannot
    /// hide behind the fixture. Requires at least one label.
    fn flatten(s: Stepper) -> (StyledDom, RefAny) {
        let dom = s.dom();
        let state = step_state(&dom, 0);
        (StyledDom::create_from_dom(dom), state)
    }

    /// Invokes `on_step_click` against a `LayoutWindow` holding `styled` (or
    /// nothing at all, when `styled` is `None`), with `hit` as the hit node.
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(
        styled: Option<StyledDom>,
        hit: DomNodeId,
        data: RefAny,
    ) -> (Update, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd));
        }

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

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = on_step_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// Every colour the live restyle wrote, as `(flattened node index, "bg" |
    /// "text", colour)` in emission order. Panics on any property other than the
    /// two the handler is documented to write.
    fn restyle_writes(changes: &[CallbackChange]) -> Vec<(usize, &'static str, ColorU)> {
        let mut out = Vec::new();
        for change in changes {
            let CallbackChange::ChangeNodeCssProperties {
                node_id,
                properties,
                ..
            } = change
            else {
                panic!("the restyle must only emit ChangeNodeCssProperties, got {change:?}");
            };
            for p in properties.as_ref() {
                match p {
                    CssProperty::BackgroundContent(v) => {
                        let layers = v
                            .get_property()
                            .expect("the restyle must write an exact background");
                        out.push((node_id.index(), "bg", only_color(layers)));
                    }
                    CssProperty::TextColor(v) => {
                        let c = v
                            .get_property()
                            .expect("the restyle must write an exact text colour");
                        out.push((node_id.index(), "text", c.inner));
                    }
                    other => panic!("unexpected restyle property: {other:?}"),
                }
            }
        }
        out
    }

    /// What a correct restyle of an `n`-step stepper landing on `clicked` looks
    /// like: per step the circle fill + number, both connector halves and the
    /// label colour, in the handler's documented emission order.
    fn expected_restyle(n: usize, clicked: usize) -> Vec<(usize, &'static str, ColorU)> {
        let last = n.saturating_sub(1);
        let mut out = Vec::with_capacity(5 * n);
        for i in 0..n {
            let reached = i <= clicked;
            out.push((
                circle_node(i),
                "bg",
                if reached {
                    ACCENT_COLOR
                } else {
                    MUTED_CIRCLE_COLOR
                },
            ));
            out.push((
                circle_node(i),
                "text",
                if reached { WHITE } else { MUTED_TEXT_COLOR },
            ));
            out.push((
                conn_left_node(i),
                "bg",
                fill_color(conn_left_fill(i, clicked)),
            ));
            out.push((
                conn_right_node(i),
                "bg",
                fill_color(conn_right_fill(i, last, clicked)),
            ));
            out.push((
                label_node(i),
                "text",
                if reached {
                    DARK_TEXT_COLOR
                } else {
                    MUTED_TEXT_COLOR
                },
            ));
        }
        out
    }

    // ==================================================================
    // ConnFill::bg
    // ==================================================================

    #[test]
    fn conn_fill_bg_maps_each_variant_to_its_own_flat_colour() {
        assert_eq!(fill_color(ConnFill::Accent), ACCENT_COLOR);
        assert_eq!(fill_color(ConnFill::Muted), CONNECTOR_MUTED_COLOR);
        assert_eq!(fill_color(ConnFill::Hidden), TRANSPARENT_COLOR);
    }

    #[test]
    fn conn_fill_bg_colours_are_pairwise_distinct_and_correctly_opaque() {
        let accent = fill_color(ConnFill::Accent);
        let muted = fill_color(ConnFill::Muted);
        let hidden = fill_color(ConnFill::Hidden);

        assert_ne!(
            accent, muted,
            "reached and unreached connectors must be distinguishable"
        );
        assert_ne!(accent, hidden);
        assert_ne!(muted, hidden);

        assert_eq!(
            accent.a, 255,
            "a translucent accent track lets the page bleed through"
        );
        assert_eq!(
            muted.a, 255,
            "a translucent muted track lets the page bleed through"
        );
        assert_eq!(
            hidden.a, 0,
            "the row-end connector must be fully transparent, not merely pale"
        );
    }

    #[test]
    fn conn_fill_bg_is_pure_and_single_layered() {
        // Called four times per step on every click; a hidden `static mut` cache
        // or an accumulating vec would show up as drift between two calls.
        for f in [ConnFill::Accent, ConnFill::Muted, ConnFill::Hidden] {
            let a = f.bg();
            let b = f.bg();
            assert_eq!(a, b, "ConnFill::bg is not pure");
            assert_eq!(a.as_ref().len(), 1, "a connector fill is exactly one layer");
        }
    }

    // ==================================================================
    // row_style
    // ==================================================================

    #[test]
    fn row_style_is_a_full_width_vertically_centred_flex_row() {
        let style = row_style();
        let props = properties(&style);
        assert_eq!(
            props.len(),
            4,
            "the indicator row declares exactly four properties"
        );

        assert!(
            props.iter().any(|p| matches!(
                p, CssProperty::Display(d) if d.get_property() == Some(&LayoutDisplay::Flex))),
            "the indicator row must be a flex box"
        );
        assert!(
            props.iter().any(|p| matches!(
                p, CssProperty::FlexDirection(d)
                    if d.get_property() == Some(&LayoutFlexDirection::Row))),
            "connectors sit left/right of the circle, so the row is horizontal"
        );
        assert!(
            props.iter().any(|p| matches!(
                p, CssProperty::AlignItems(a)
                    if a.get_property() == Some(&LayoutAlignItems::Center))),
            "the 2px connectors must be centred on the 28px circle"
        );
    }

    #[test]
    fn row_style_width_is_a_full_percentage_not_a_pixel_length() {
        // A `px` width here would stop the flex-grow connectors from having any
        // space to fill, collapsing the track to nothing at every cell width.
        let style = row_style();
        let w = width_value(&properties(&style)).expect("the indicator row must declare a width");
        assert_eq!(
            w.metric,
            SizeMetric::Percent,
            "the row width must be relative to the cell"
        );
        assert!(
            (w.number.get() - 100.0).abs() < f32::EPSILON,
            "the row must span the whole cell, got {}",
            w.number.get()
        );
    }

    #[test]
    fn row_style_is_unconditional_unique_and_pure() {
        assert_unconditional_and_unique(&row_style(), "row_style");
        assert_eq!(
            properties(&row_style()),
            properties(&row_style()),
            "row_style is not pure"
        );
    }

    // ==================================================================
    // circle_style
    // ==================================================================

    #[test]
    fn circle_style_declares_the_same_property_set_for_both_states() {
        // A property present in one state but not the other would not be reset on
        // restyle — the circle would keep a stale declaration after a click.
        let reached = circle_style(true);
        let unreached = circle_style(false);

        assert_eq!(
            reached.as_ref().len(),
            18,
            "the circle declares eighteen properties"
        );
        assert_eq!(unreached.as_ref().len(), 18);
        assert_eq!(
            property_kinds(&reached),
            property_kinds(&unreached),
            "the two circle states must declare the same properties in the same order"
        );
    }

    #[test]
    fn circle_style_colours_are_the_only_reached_dependent_declarations() {
        let reached = properties(&circle_style(true));
        let unreached = properties(&circle_style(false));

        assert_eq!(background_color(&reached), Some(ACCENT_COLOR));
        assert_eq!(text_color(&reached), Some(WHITE));
        assert_eq!(background_color(&unreached), Some(MUTED_CIRCLE_COLOR));
        assert_eq!(text_color(&unreached), Some(MUTED_TEXT_COLOR));

        // Everything that is not a colour must be byte-identical between states.
        let strip = |v: &[CssProperty]| -> Vec<CssProperty> {
            v.iter()
                .filter(|p| {
                    !matches!(
                        p,
                        CssProperty::BackgroundContent(_) | CssProperty::TextColor(_)
                    )
                })
                .cloned()
                .collect()
        };
        assert_eq!(
            strip(&reached),
            strip(&unreached),
            "reached-ness leaked into a non-colour property"
        );
    }

    #[test]
    fn circle_style_geometry_is_a_fixed_pixel_circle() {
        for reached in [false, true] {
            let props = properties(&circle_style(reached));
            let ctx = format!("reached={reached}");

            let w = px(width_value(&props).expect("width"));
            let h = px(height_value(&props).expect("height"));
            let mw = px(min_width_value(&props).expect("min-width"));

            assert!(
                (w - CIRCLE_SIZE as f32).abs() < f32::EPSILON,
                "{ctx}: width"
            );
            assert!(
                (h - CIRCLE_SIZE as f32).abs() < f32::EPSILON,
                "{ctx}: height"
            );
            assert!(
                (mw - CIRCLE_SIZE as f32).abs() < f32::EPSILON,
                "{ctx}: without a min-width the circle would squash in a tight flex row"
            );
            assert!(
                (w - h).abs() < f32::EPSILON,
                "{ctx}: a circle must be square"
            );

            let (tl, tr, bl, br) = radii(&props);
            let r = CIRCLE_RADIUS as f32;
            for (corner, v) in [
                ("top-left", tl),
                ("top-right", tr),
                ("bottom-left", bl),
                ("bottom-right", br),
            ] {
                let v = px(v.unwrap_or_else(|| panic!("{ctx}: missing {corner} radius")));
                assert!(
                    (v - r).abs() < f32::EPSILON,
                    "{ctx}: {corner} radius is {v}, want {r}"
                );
            }

            assert_eq!(
                CIRCLE_RADIUS * 2,
                CIRCLE_SIZE,
                "a radius that is not half the box renders a rounded square, not a circle"
            );
            assert_eq!(
                flex_grow_value(&props),
                Some(0.0),
                "{ctx}: the circle must keep its fixed size, not stretch"
            );
            assert!(
                (px(font_size_value(&props).expect("font size")) - 13.0).abs() < f32::EPSILON,
                "{ctx}: font size"
            );
        }
    }

    #[test]
    fn circle_style_centres_its_number_and_declares_the_interaction_affordances() {
        for reached in [false, true] {
            let props = properties(&circle_style(reached));
            let ctx = format!("reached={reached}");

            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::JustifyContent(j)
                        if j.get_property() == Some(&LayoutJustifyContent::Center))),
                "{ctx}: the step number must be horizontally centred in the circle"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::AlignItems(a)
                        if a.get_property() == Some(&LayoutAlignItems::Center))),
                "{ctx}: the step number must be vertically centred in the circle"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::TextAlign(t)
                        if t.get_property() == Some(&StyleTextAlign::Center))),
                "{ctx}: text-align"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::Cursor(c) if c.get_property() == Some(&StyleCursor::Pointer))),
                "{ctx}: a clickable step must show the pointer cursor"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::UserSelect(u)
                        if u.get_property() == Some(&StyleUserSelect::None))),
                "{ctx}: click-dragging a step must not select its number"
            );
        }
    }

    #[test]
    fn circle_style_keeps_the_number_readable_and_opaque() {
        for reached in [false, true] {
            let props = properties(&circle_style(reached));
            let bg = background_color(&props).expect("background");
            let fg = text_color(&props).expect("text colour");

            assert_eq!(
                bg.a, 255,
                "reached={reached}: a translucent circle lets the page bleed through"
            );
            assert_eq!(fg.a, 255, "reached={reached}: translucent number");
            assert_ne!(
                bg, fg,
                "reached={reached}: an invisible number is not a step indicator"
            );
            assert!(
                (luma(bg) - luma(fg)).abs() >= 60.0,
                "reached={reached}: brightness gap {:.1} is too low to read",
                (luma(bg) - luma(fg)).abs()
            );
        }

        // The two states must be visually distinguishable — that is the entire
        // point of a progress indicator.
        assert_ne!(
            background_color(&properties(&circle_style(true))),
            background_color(&properties(&circle_style(false)))
        );
    }

    #[test]
    fn circle_style_is_unconditional_unique_and_pure() {
        for reached in [false, true] {
            let ctx = format!("circle_style({reached})");
            assert_unconditional_and_unique(&circle_style(reached), &ctx);
            assert_eq!(
                properties(&circle_style(reached)),
                properties(&circle_style(reached)),
                "{ctx} is not pure"
            );
        }
    }

    // ==================================================================
    // connector_style
    // ==================================================================

    #[test]
    fn connector_style_declares_only_grow_height_and_fill() {
        for f in [ConnFill::Accent, ConnFill::Muted, ConnFill::Hidden] {
            let style = connector_style(f);
            let props = properties(&style);
            assert_eq!(
                props.len(),
                3,
                "a connector half-line declares exactly three properties"
            );

            assert_eq!(
                flex_grow_value(&props),
                Some(1.0),
                "a connector must absorb all leftover width, otherwise the track is invisible"
            );
            let h = px(height_value(&props).expect("a connector must declare a height"));
            assert!(
                (h - CONNECTOR_HEIGHT as f32).abs() < f32::EPSILON,
                "connector height {h}, want {CONNECTOR_HEIGHT}"
            );
            assert_eq!(background_color(&props), Some(fill_color(f)));
        }
    }

    #[test]
    fn connector_style_geometry_is_independent_of_the_fill() {
        // A hidden connector must still reserve exactly the same box as a visible
        // one, or the circles would shift horizontally at the row's two ends.
        let strip = |f: ConnFill| -> Vec<CssProperty> {
            properties(&connector_style(f))
                .into_iter()
                .filter(|p| !matches!(p, CssProperty::BackgroundContent(_)))
                .collect()
        };
        let accent = strip(ConnFill::Accent);
        assert_eq!(
            accent,
            strip(ConnFill::Muted),
            "muted connectors changed shape"
        );
        assert_eq!(
            accent,
            strip(ConnFill::Hidden),
            "hidden connectors changed shape"
        );
        assert_eq!(
            property_kinds(&connector_style(ConnFill::Accent)),
            property_kinds(&connector_style(ConnFill::Hidden)),
            "a hidden connector must declare the same properties, only a different colour"
        );
    }

    #[test]
    fn connector_style_is_unconditional_unique_and_pure() {
        for (name, f) in [
            ("accent", ConnFill::Accent),
            ("muted", ConnFill::Muted),
            ("hidden", ConnFill::Hidden),
        ] {
            let ctx = format!("connector_style({name})");
            assert_unconditional_and_unique(&connector_style(f), &ctx);
            assert_eq!(
                properties(&connector_style(f)),
                properties(&connector_style(f)),
                "{ctx} is not pure"
            );
        }
    }

    // ==================================================================
    // label_style
    // ==================================================================

    #[test]
    fn label_style_declares_the_same_six_properties_for_both_states() {
        let reached = label_style(true);
        let unreached = label_style(false);
        assert_eq!(
            reached.as_ref().len(),
            6,
            "a step label declares exactly six properties"
        );
        assert_eq!(unreached.as_ref().len(), 6);
        assert_eq!(property_kinds(&reached), property_kinds(&unreached));
    }

    #[test]
    fn label_style_colour_is_the_only_reached_dependent_declaration() {
        let reached = properties(&label_style(true));
        let unreached = properties(&label_style(false));

        assert_eq!(text_color(&reached), Some(DARK_TEXT_COLOR));
        assert_eq!(text_color(&unreached), Some(MUTED_TEXT_COLOR));
        assert_ne!(
            text_color(&reached),
            text_color(&unreached),
            "reached and upcoming labels must be distinguishable"
        );
        assert_eq!(
            background_color(&reached),
            None,
            "a step label paints no background"
        );

        let strip = |v: &[CssProperty]| -> Vec<CssProperty> {
            v.iter()
                .filter(|p| !matches!(p, CssProperty::TextColor(_)))
                .cloned()
                .collect()
        };
        assert_eq!(
            strip(&reached),
            strip(&unreached),
            "reached-ness leaked past the colour"
        );
    }

    #[test]
    fn label_style_geometry_and_affordances() {
        for reached in [false, true] {
            let props = properties(&label_style(reached));
            let ctx = format!("reached={reached}");

            assert!(
                (px(font_size_value(&props).expect("font size")) - 12.0).abs() < f32::EPSILON,
                "{ctx}: labels are one point smaller than the circle number"
            );
            assert!(
                (px(padding_top_value(&props).expect("padding-top")) - 6.0).abs() < f32::EPSILON,
                "{ctx}: the label must clear the indicator row"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::TextAlign(t)
                        if t.get_property() == Some(&StyleTextAlign::Center))),
                "{ctx}: labels are centred under their circle"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::Cursor(c) if c.get_property() == Some(&StyleCursor::Pointer))),
                "{ctx}: the label is part of the clickable cell"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::UserSelect(u)
                        if u.get_property() == Some(&StyleUserSelect::None))),
                "{ctx}: click-dragging must not select the caption"
            );
        }
    }

    #[test]
    fn label_style_stays_readable_on_a_white_page() {
        for reached in [false, true] {
            let fg = text_color(&properties(&label_style(reached))).expect("text colour");
            assert_eq!(fg.a, 255, "reached={reached}: translucent label text");
            assert!(
                (luma(WHITE) - luma(fg)) >= 60.0,
                "reached={reached}: label brightness gap against white is only {:.1}",
                luma(WHITE) - luma(fg)
            );
        }
    }

    #[test]
    fn label_style_is_unconditional_unique_and_pure() {
        for reached in [false, true] {
            let ctx = format!("label_style({reached})");
            assert_unconditional_and_unique(&label_style(reached), &ctx);
            assert_eq!(
                properties(&label_style(reached)),
                properties(&label_style(reached)),
                "{ctx} is not pure"
            );
        }
    }

    // ==================================================================
    // conn_left_fill  (numeric)
    // ==================================================================

    #[test]
    fn conn_left_fill_hides_the_leading_edge_whatever_the_current_step() {
        // Step 0 has nothing to its left; a visible stub there would stick out of
        // the row. Index 0 must win over *every* `current`, including `usize::MAX`.
        for current in [0usize, 1, 2, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            assert_eq!(
                fill_color(conn_left_fill(0, current)),
                TRANSPARENT_COLOR,
                "current={current}: the row's leading edge must stay hidden"
            );
        }
    }

    #[test]
    fn conn_left_fill_is_accent_up_to_and_including_the_current_step() {
        for current in 0..6usize {
            for i in 1..8usize {
                let want = if i <= current {
                    ACCENT_COLOR
                } else {
                    CONNECTOR_MUTED_COLOR
                };
                assert_eq!(
                    fill_color(conn_left_fill(i, current)),
                    want,
                    "i={i}, current={current}: wrong left-connector fill"
                );
            }
        }
    }

    #[test]
    fn conn_left_fill_at_the_integer_extremes_does_not_panic() {
        // `usize::MAX` is also what a `-1` handed in through FFI looks like.
        let cases: [(usize, usize, ColorU); 7] = [
            (usize::MAX, usize::MAX, ACCENT_COLOR),
            (usize::MAX, usize::MAX - 1, CONNECTOR_MUTED_COLOR),
            (usize::MAX - 1, usize::MAX, ACCENT_COLOR),
            (usize::MAX, 0, CONNECTOR_MUTED_COLOR),
            (1, usize::MAX, ACCENT_COLOR),
            ((-1i64) as usize, (-1i64) as usize, ACCENT_COLOR),
            (i64::MIN as usize, usize::MAX, ACCENT_COLOR),
        ];
        for (i, current, want) in cases {
            assert_eq!(
                fill_color(conn_left_fill(i, current)),
                want,
                "i={i}, current={current}"
            );
        }
    }

    #[test]
    fn conn_left_fill_is_monotone_in_the_current_step() {
        // Advancing the wizard may only ever fill more of the track, never unfill
        // an already-accented segment.
        for i in 1..6usize {
            let mut seen_accent = false;
            for current in 0..12usize {
                let accent = fill_color(conn_left_fill(i, current)) == ACCENT_COLOR;
                if seen_accent {
                    assert!(
                        accent,
                        "i={i}: the left connector un-filled at current={current}"
                    );
                }
                seen_accent |= accent;
            }
            assert!(seen_accent, "i={i}: the left connector never fills");
        }
    }

    #[test]
    fn conn_left_fill_only_ever_returns_one_of_the_three_known_colours() {
        for i in 0..8usize {
            for current in 0..8usize {
                let c = fill_color(conn_left_fill(i, current));
                assert!(
                    c == ACCENT_COLOR || c == CONNECTOR_MUTED_COLOR || c == TRANSPARENT_COLOR,
                    "i={i}, current={current}: unexpected colour {c:?}"
                );
            }
        }
    }

    // ==================================================================
    // conn_right_fill  (numeric)
    // ==================================================================

    #[test]
    fn conn_right_fill_hides_the_trailing_edge_of_the_last_step() {
        for last in [0usize, 1, 4, usize::MAX] {
            for current in [0usize, 1, usize::MAX] {
                assert_eq!(
                    fill_color(conn_right_fill(last, last, current)),
                    TRANSPARENT_COLOR,
                    "last={last}, current={current}: the row's trailing edge must stay hidden"
                );
            }
        }
    }

    #[test]
    fn conn_right_fill_is_accent_strictly_before_the_current_step() {
        let last = 7usize;
        for current in 0..8usize {
            for i in 0..last {
                let want = if i < current {
                    ACCENT_COLOR
                } else {
                    CONNECTOR_MUTED_COLOR
                };
                assert_eq!(
                    fill_color(conn_right_fill(i, last, current)),
                    want,
                    "i={i}, last={last}, current={current}: wrong right-connector fill"
                );
            }
        }
    }

    #[test]
    fn conn_right_fill_lets_last_win_over_an_accent_current() {
        // `i == last` is checked first, so a `current` beyond the end must not
        // paint a stub past the final circle.
        for current in [3usize, 4, 100, usize::MAX] {
            assert_eq!(
                fill_color(conn_right_fill(3, 3, current)),
                TRANSPARENT_COLOR,
                "current={current}: the last step grew a trailing accent stub"
            );
        }
    }

    #[test]
    fn conn_right_fill_at_the_integer_extremes_does_not_panic() {
        let cases: [(usize, usize, usize, ColorU); 7] = [
            (usize::MAX, usize::MAX, usize::MAX, TRANSPARENT_COLOR),
            (usize::MAX - 1, usize::MAX, usize::MAX, ACCENT_COLOR),
            (usize::MAX - 1, usize::MAX, 0, CONNECTOR_MUTED_COLOR),
            (0, usize::MAX, usize::MAX, ACCENT_COLOR),
            (0, 0, usize::MAX, TRANSPARENT_COLOR),
            ((-1i64) as usize, (-1i64) as usize, 0, TRANSPARENT_COLOR),
            (i64::MIN as usize, usize::MAX, usize::MAX, ACCENT_COLOR),
        ];
        for (i, last, current, want) in cases {
            assert_eq!(
                fill_color(conn_right_fill(i, last, current)),
                want,
                "i={i}, last={last}, current={current}"
            );
        }
    }

    #[test]
    fn the_two_halves_of_every_gap_agree() {
        // The gap between circle `i` and circle `i+1` is drawn by two independent
        // half-lines: `conn_right_fill(i, ..)` and `conn_left_fill(i + 1, ..)`.
        // If they ever disagree the track renders half accent / half grey.
        for n in 1..8usize {
            let last = n - 1;
            for current in 0..(n + 2) {
                for i in 0..last {
                    assert_eq!(
                        fill_color(conn_right_fill(i, last, current)),
                        fill_color(conn_left_fill(i + 1, current)),
                        "n={n}, current={current}: the two halves of gap {i}→{} disagree",
                        i + 1
                    );
                }
            }
        }
    }

    // ==================================================================
    // Stepper::create
    // ==================================================================

    #[test]
    fn create_preserves_labels_verbatim() {
        for case in [
            vec![],
            vec!["only"],
            vec!["Account", "Address"],
            vec!["Account", "Address", "Payment", "Review"],
            vec!["dup", "dup", "dup"],
        ] {
            let s = Stepper::create(labels(&case));
            let got: Vec<&str> = s.labels.as_ref().iter().map(AzString::as_str).collect();
            assert_eq!(
                got, case,
                "create must not reorder/drop/dedupe/rewrite labels"
            );
        }
    }

    #[test]
    fn create_preserves_adversarial_labels_byte_for_byte() {
        for s in adversarial_strings() {
            let w = Stepper::create(labels(&[s.as_str()]));
            assert_eq!(
                w.labels.as_ref()[0].as_str(),
                s.as_str(),
                "the caption changed on its way into the widget"
            );
            assert_eq!(
                w.labels.as_ref()[0].as_ref().len(),
                s.len(),
                "byte length changed (NUL truncation?)"
            );
            assert_eq!(w.stepper_state.inner.total_steps, 1);
        }
    }

    #[test]
    fn create_derives_total_steps_from_the_label_count_and_starts_at_zero() {
        for n in [0usize, 1, 2, 7, 4096] {
            let s = Stepper::create(n_labels(n));
            assert_eq!(s.stepper_state.inner.total_steps, n, "n={n}: total_steps");
            assert_eq!(
                s.stepper_state.inner.current_step, 0,
                "n={n}: a fresh stepper starts at 0"
            );
            assert_eq!(s.labels.as_ref().len(), n, "n={n}: label count");
            assert!(
                s.stepper_state.on_step_change.as_ref().is_none(),
                "n={n}: create wires no callback"
            );
        }
    }

    #[test]
    fn create_keeps_total_steps_and_the_label_count_in_lockstep() {
        // `dom()` counts labels while `set_current_step` clamps against
        // `total_steps`; a mismatch between the two is what would let the clamp
        // admit a step that does not render.
        for n in [0usize, 1, 3, 64] {
            let s = Stepper::create(n_labels(n));
            assert_eq!(
                s.stepper_state.inner.total_steps,
                s.labels.as_ref().len(),
                "n={n}: total_steps drifted from the label count"
            );
        }
    }

    #[test]
    fn create_installs_the_shared_container_style() {
        let s = stepper(&["a", "b"]);
        assert_eq!(
            s.container_style.as_ref(),
            STEPPER_CONTAINER_STYLE,
            "create must install the shared container style"
        );

        let props = properties(&s.container_style);
        assert_eq!(props.len(), 4);
        assert!(props.iter().any(|p| matches!(
            p, CssProperty::Display(d) if d.get_property() == Some(&LayoutDisplay::Flex))));
        assert!(props.iter().any(|p| matches!(
            p, CssProperty::FlexDirection(d)
                if d.get_property() == Some(&LayoutFlexDirection::Row))));
        assert!(props.iter().any(|p| matches!(
            p, CssProperty::AlignItems(a) if a.get_property() == Some(&LayoutAlignItems::Start))),
            "step cells must be top-aligned so labels of different heights do not shift the circles");
        assert_eq!(
            flex_grow_value(&props),
            Some(0.0),
            "the stepper hugs its steps instead of filling the parent"
        );
        assert_unconditional_and_unique(&s.container_style, "STEPPER_CONTAINER_STYLE");
    }

    #[test]
    fn create_with_no_labels_equals_default() {
        let empty = Stepper::create(StringVec::from_const_slice(&[]));
        assert_eq!(
            empty,
            Stepper::default(),
            "Default must be the empty stepper"
        );
        assert_eq!(empty.labels.as_ref().len(), 0);
        assert_eq!(
            empty.stepper_state.inner,
            StepperState {
                current_step: 0,
                total_steps: 0
            }
        );
    }

    #[test]
    fn create_scales_to_a_very_long_label_list() {
        let n = 4096;
        let s = Stepper::create(n_labels(n));
        assert_eq!(s.labels.as_ref().len(), n);
        assert_eq!(s.labels.as_ref()[n - 1].as_str(), format!("s{}", n - 1));
        assert_eq!(s.stepper_state.inner.total_steps, n);
    }

    // ==================================================================
    // Stepper::set_current_step  (numeric)
    // ==================================================================

    #[test]
    fn set_current_step_clamps_every_boundary_value_into_range() {
        for i in boundary_indices() {
            let mut s = stepper(&["a", "b", "c"]);
            s.set_current_step(i);
            assert_eq!(
                s.stepper_state.inner.current_step,
                i.min(2),
                "index {i} was not clamped to [0, total_steps - 1]"
            );
        }
    }

    #[test]
    fn set_current_step_on_an_empty_stepper_never_underflows() {
        // `total - 1` on an empty stepper would underflow to `usize::MAX` in
        // release and panic in debug; the `total == 0` guard must win.
        let mut s = Stepper::default();
        for i in boundary_indices() {
            s.set_current_step(i);
            assert_eq!(
                s.stepper_state.inner.current_step, 0,
                "index {i} on an empty stepper"
            );
            assert_eq!(s.stepper_state.inner.total_steps, 0);
        }
    }

    #[test]
    fn set_current_step_at_zero_and_at_the_last_index_is_exact() {
        for n in [1usize, 2, 5, 64] {
            let mut s = Stepper::create(n_labels(n));
            s.set_current_step(0);
            assert_eq!(s.stepper_state.inner.current_step, 0, "n={n}: zero");
            s.set_current_step(n - 1);
            assert_eq!(
                s.stepper_state.inner.current_step,
                n - 1,
                "n={n}: last index is in range"
            );
            s.set_current_step(n);
            assert_eq!(
                s.stepper_state.inner.current_step,
                n - 1,
                "n={n}: one past the end clamps"
            );
        }
    }

    #[test]
    fn set_current_step_on_a_single_step_stepper_is_always_zero() {
        let mut s = stepper(&["only"]);
        for i in boundary_indices() {
            s.set_current_step(i);
            assert_eq!(
                s.stepper_state.inner.current_step, 0,
                "index {i} on a one-step stepper"
            );
        }
    }

    #[test]
    fn set_current_step_is_idempotent_and_last_write_wins() {
        let mut s = stepper(&["a", "b", "c"]);
        for _ in 0..3 {
            s.set_current_step(1);
        }
        assert_eq!(s.stepper_state.inner.current_step, 1);

        for i in [0usize, usize::MAX, 2, 0] {
            s.set_current_step(i);
        }
        assert_eq!(
            s.stepper_state.inner.current_step, 0,
            "the last write must win"
        );
    }

    #[test]
    fn set_current_step_leaves_every_other_field_alone() {
        let mut s = stepper(&["a", "b"]).with_on_step_change(RefAny::new(7u8), cb(step_do_nothing));
        let before = s.clone();

        s.set_current_step(usize::MAX);

        assert_eq!(s.labels, before.labels, "labels changed");
        assert_eq!(
            s.container_style, before.container_style,
            "container style changed"
        );
        assert_eq!(
            s.stepper_state.inner.total_steps, before.stepper_state.inner.total_steps,
            "total_steps changed"
        );
        assert!(
            s.stepper_state.on_step_change.as_ref().is_some(),
            "the callback was dropped"
        );
    }

    #[test]
    fn set_current_step_clamps_against_total_steps_not_the_label_count() {
        // `total_steps` is a public field, so app code can desync it from
        // `labels`. This documents which of the two the clamp actually consults.
        let mut s = stepper(&["a", "b", "c"]);
        s.stepper_state.inner.total_steps = 100;
        s.set_current_step(50);
        assert_eq!(
            s.stepper_state.inner.current_step, 50,
            "the clamp follows total_steps, not labels.len()"
        );
        s.set_current_step(usize::MAX);
        assert_eq!(s.stepper_state.inner.current_step, 99);
        assert_eq!(
            s.labels.as_ref().len(),
            3,
            "the setter must not touch the labels"
        );
    }

    // ==================================================================
    // Stepper::with_current_step  (constructor)
    // ==================================================================

    #[test]
    fn with_current_step_round_trips_through_the_setter() {
        for i in boundary_indices() {
            let via_builder = stepper(&["a", "b", "c"]).with_current_step(i);
            let mut via_setter = stepper(&["a", "b", "c"]);
            via_setter.set_current_step(i);

            assert_eq!(
                via_builder, via_setter,
                "index {i}: builder and setter diverge"
            );
            assert_eq!(via_builder.stepper_state.inner.current_step, i.min(2));
        }
    }

    #[test]
    fn with_current_step_stores_exactly_what_it_reads_back_in_range() {
        // encode == decode for every in-range step of a 6-step wizard.
        let n = 6;
        for i in 0..n {
            let s = Stepper::create(n_labels(n)).with_current_step(i);
            assert_eq!(
                s.stepper_state.inner.current_step, i,
                "step {i} did not round-trip"
            );
            assert_eq!(s.stepper_state.inner.total_steps, n);
        }
    }

    #[test]
    fn with_current_step_does_not_panic_on_extreme_arguments() {
        for n in [0usize, 1, 3] {
            for i in boundary_indices() {
                let s = Stepper::create(n_labels(n)).with_current_step(i);
                let cur = s.stepper_state.inner.current_step;
                assert!(
                    n == 0 && cur == 0 || n > 0 && cur < n,
                    "n={n}, i={i}: current_step {cur} escaped the valid range"
                );
            }
        }
    }

    #[test]
    fn with_current_step_preserves_the_rest_of_the_widget() {
        let base = stepper(&["a", "b", "c"]);
        let built = base.clone().with_current_step(2);

        assert_eq!(built.labels, base.labels);
        assert_eq!(built.container_style, base.container_style);
        assert_eq!(
            built.labels.as_ref().len(),
            3,
            "len/contents must stay consistent"
        );
        assert_eq!(built.stepper_state.inner.total_steps, 3);
        assert!(built.stepper_state.on_step_change.as_ref().is_none());
    }

    #[test]
    fn with_current_step_chains_with_last_wins() {
        let s = stepper(&["a", "b", "c"])
            .with_current_step(usize::MAX)
            .with_current_step(0)
            .with_current_step(1);
        assert_eq!(s.stepper_state.inner.current_step, 1);
    }

    // ==================================================================
    // Stepper::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_default_behind() {
        let mut s = stepper(&["Account", "Address", "Payment"]).with_current_step(2);
        let expected = s.clone();

        let taken = s.swap_with_default();

        assert_eq!(
            taken, expected,
            "the caller must get the original widget back"
        );
        assert_eq!(s, Stepper::default(), "a default must be left in its place");
        assert_eq!(s.labels.as_ref().len(), 0);
        assert_eq!(
            s.stepper_state.inner,
            StepperState {
                current_step: 0,
                total_steps: 0
            }
        );
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_with_the_widget() {
        let mut s = stepper(&["a", "b"]).with_on_step_change(RefAny::new(1u8), cb(record_step));

        let taken = s.swap_with_default();

        assert!(
            taken.stepper_state.on_step_change.as_ref().is_some(),
            "the callback must travel with the taken widget"
        );
        assert!(
            s.stepper_state.on_step_change.as_ref().is_none(),
            "the leftover default must not keep a handle on the callback"
        );
    }

    #[test]
    fn swap_with_default_on_a_default_is_a_no_op() {
        let mut s = Stepper::default();
        let taken = s.swap_with_default();
        assert_eq!(taken, Stepper::default());
        assert_eq!(s, Stepper::default());
    }

    #[test]
    fn swap_with_default_twice_yields_a_default_the_second_time() {
        let mut s = stepper(&["a", "b"]).with_current_step(1);
        let first = s.swap_with_default();
        let second = s.swap_with_default();

        assert_eq!(first.labels.as_ref().len(), 2);
        assert_eq!(first.stepper_state.inner.current_step, 1);
        assert_eq!(
            second,
            Stepper::default(),
            "the second take is the default we left behind"
        );
        assert_eq!(s, Stepper::default());
    }

    #[test]
    fn swap_with_default_does_not_truncate_a_large_label_list() {
        let n = 1024;
        let mut s = Stepper::create(n_labels(n)).with_current_step(n - 1);
        let taken = s.swap_with_default();

        assert_eq!(taken.labels.as_ref().len(), n);
        assert_eq!(taken.labels.as_ref()[n - 1].as_str(), format!("s{}", n - 1));
        assert_eq!(taken.stepper_state.inner.current_step, n - 1);
        assert_eq!(taken.stepper_state.inner.total_steps, n);
    }

    // ==================================================================
    // Stepper::set_on_step_change  /  with_on_step_change
    // ==================================================================

    #[test]
    fn set_on_step_change_installs_the_callback_and_shares_its_payload() {
        let mut s = stepper(&["a", "b"]);
        let mut payload = RefAny::new(StepLog { seen: Vec::new() });
        s.set_on_step_change(payload.clone(), cb(record_step));

        let installed = s
            .stepper_state
            .on_step_change
            .as_ref()
            .expect("set_on_step_change must install a callback");
        assert_eq!(
            installed.callback.cb as usize, record_step as usize,
            "wrong function installed"
        );
        assert!(
            matches!(installed.callback.ctx, OptionRefAny::None),
            "a native Rust callback carries no FFI context"
        );

        // The stored `RefAny` must be a *share* of the caller's, not a copy:
        // writing through the widget's handle must be visible to the caller.
        let mut stored = installed.refany.clone();
        {
            let mut log = stored
                .downcast_mut::<StepLog>()
                .expect("payload type must survive");
            log.seen.push(StepperState {
                current_step: 42,
                total_steps: 43,
            });
        }
        assert_eq!(
            logged(&mut payload),
            vec![StepperState {
                current_step: 42,
                total_steps: 43
            }],
            "the payload was copied, not shared"
        );
    }

    #[test]
    fn set_on_step_change_overwrites_a_previously_installed_callback() {
        let mut s = stepper(&["a", "b"]);
        s.set_on_step_change(RefAny::new(1u8), cb(record_step));
        s.set_on_step_change(RefAny::new(2u8), cb(step_refresh_all));

        let installed = s.stepper_state.on_step_change.as_ref().expect("callback");
        assert_eq!(
            installed.callback.cb as usize, step_refresh_all as usize,
            "the last setter must win"
        );
        assert_ne!(installed.callback.cb as usize, record_step as usize);
    }

    #[test]
    fn set_on_step_change_does_not_disturb_the_labels_or_the_state() {
        let mut s = stepper(&["a", "b", "c"]).with_current_step(2);
        s.set_on_step_change(RefAny::new(0u8), cb(step_do_nothing));

        assert_eq!(s.labels.as_ref().len(), 3);
        assert_eq!(
            s.stepper_state.inner,
            StepperState {
                current_step: 2,
                total_steps: 3
            },
            "installing a callback moved the current step"
        );
    }

    #[test]
    fn set_on_step_change_accepts_an_arbitrary_payload_without_reading_it() {
        for payload in [
            RefAny::new(0u8),
            RefAny::new(String::new()),
            RefAny::new([0u64; 64]),
        ] {
            let mut s = stepper(&["a"]);
            s.set_on_step_change(payload, cb(step_do_nothing));
            assert!(s.stepper_state.on_step_change.as_ref().is_some());
        }
    }

    #[test]
    fn with_on_step_change_matches_the_setter_exactly() {
        let payload = RefAny::new(9u8);
        let via_builder =
            stepper(&["a", "b"]).with_on_step_change(payload.clone(), cb(step_do_nothing));
        let mut via_setter = stepper(&["a", "b"]);
        via_setter.set_on_step_change(payload, cb(step_do_nothing));

        assert_eq!(
            via_builder, via_setter,
            "builder and setter must produce the same widget"
        );
    }

    #[test]
    fn with_on_step_change_holds_its_invariants_after_construction() {
        let s = Stepper::create(n_labels(5))
            .with_current_step(3)
            .with_on_step_change(RefAny::new(0u8), cb(step_refresh_all));

        assert_eq!(
            s.labels.as_ref().len(),
            5,
            "label count must survive the builder chain"
        );
        assert_eq!(
            s.stepper_state.inner,
            StepperState {
                current_step: 3,
                total_steps: 5
            },
            "the state must survive"
        );
        assert_eq!(
            s.container_style.as_ref(),
            STEPPER_CONTAINER_STYLE,
            "the container style must survive"
        );
        let installed = s.stepper_state.on_step_change.as_ref().expect("callback");
        assert_eq!(installed.callback.cb as usize, step_refresh_all as usize);
    }

    #[test]
    fn with_on_step_change_chains_with_last_wins() {
        let s = stepper(&["a"])
            .with_on_step_change(RefAny::new(0u8), cb(record_step))
            .with_on_step_change(RefAny::new(0u8), cb(step_do_nothing));
        let installed = s.stepper_state.on_step_change.as_ref().expect("callback");
        assert_eq!(installed.callback.cb as usize, step_do_nothing as usize);
    }

    // ==================================================================
    // Stepper::dom
    // ==================================================================

    #[test]
    fn dom_emits_one_six_node_cell_per_label_in_order() {
        let case = ["Account", "Address", "Payment", "Review"];
        let dom = Stepper::create(labels(&case)).dom();

        assert!(
            matches!(dom.root.get_node_type(), NodeType::Div),
            "the stepper is a div"
        );
        assert!(dom.root.has_class("__azul-native-stepper"));
        assert!(
            dom.root.get_callbacks().as_ref().is_empty(),
            "the container itself is not clickable"
        );

        let children = dom.children.as_ref();
        assert_eq!(children.len(), case.len());
        for (i, cell) in children.iter().enumerate() {
            assert!(
                cell.root.has_class("__azul-native-stepper-step"),
                "step {i}: cell class"
            );
            assert_eq!(
                cell.children.as_ref().len(),
                2,
                "step {i}: cell holds a row + a label"
            );

            let row = row_of(cell);
            assert!(
                row.root.has_class("__azul-native-stepper-row"),
                "step {i}: row class"
            );
            assert_eq!(
                row.children.as_ref().len(),
                3,
                "step {i}: connector, circle, connector"
            );

            assert!(
                conn_left_of(row)
                    .root
                    .has_class("__azul-native-stepper-connector"),
                "step {i}: left connector class"
            );
            assert!(
                conn_right_of(row)
                    .root
                    .has_class("__azul-native-stepper-connector"),
                "step {i}: right connector class"
            );
            assert!(
                circle_of(row)
                    .root
                    .has_class("__azul-native-stepper-circle"),
                "step {i}: circle class"
            );
            assert!(
                label_of(cell).root.has_class("__azul-native-stepper-label"),
                "step {i}: label class"
            );
            assert_eq!(
                text_of(label_of(cell)),
                Some(case[i]),
                "step {i}: wrong caption"
            );
        }
    }

    #[test]
    fn dom_numbers_the_circles_from_one() {
        // The circle shows a *one-based* number while every index in the widget is
        // zero-based; an off-by-one here is invisible to every other assertion.
        let n = 12;
        let dom = Stepper::create(n_labels(n)).dom();
        for (i, cell) in dom.children.as_ref().iter().enumerate() {
            let want = format!("{}", i + 1);
            assert_eq!(
                text_of(circle_of(row_of(cell))),
                Some(want.as_str()),
                "step {i} shows the wrong number"
            );
        }
    }

    #[test]
    fn dom_of_an_empty_stepper_is_a_childless_container() {
        // `count == 0` makes `last = count.saturating_sub(1)`; the saturation must
        // hold and no stray child may be emitted.
        let dom = Stepper::default().dom();
        assert_eq!(dom.children.as_ref().len(), 0);
        assert_eq!(dom.estimated_total_children, 0);
        assert!(dom.root.has_class("__azul-native-stepper"));

        let styled = StyledDom::create_from_dom(dom);
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            1,
            "just the container"
        );
    }

    #[test]
    fn dom_styles_every_circle_and_label_by_reached_ness() {
        for n in [1usize, 2, 3, 5] {
            for current in 0..n {
                let dom = Stepper::create(n_labels(n))
                    .with_current_step(current)
                    .dom();
                for (i, cell) in dom.children.as_ref().iter().enumerate() {
                    let reached = i <= current;
                    assert_eq!(
                        inline_properties(circle_of(row_of(cell))),
                        properties(&circle_style(reached)),
                        "n={n} current={current}: circle {i} carries the wrong style"
                    );
                    assert_eq!(
                        inline_properties(label_of(cell)),
                        properties(&label_style(reached)),
                        "n={n} current={current}: label {i} carries the wrong style"
                    );
                }
            }
        }
    }

    #[test]
    fn dom_paints_the_connectors_exactly_as_the_fill_helpers_say() {
        for n in [1usize, 2, 3, 6] {
            let last = n - 1;
            for current in 0..n {
                let dom = Stepper::create(n_labels(n))
                    .with_current_step(current)
                    .dom();
                for (i, cell) in dom.children.as_ref().iter().enumerate() {
                    let row = row_of(cell);
                    assert_eq!(
                        background_color(&inline_properties(conn_left_of(row))),
                        Some(fill_color(conn_left_fill(i, current))),
                        "n={n} current={current}: left connector of step {i}"
                    );
                    assert_eq!(
                        background_color(&inline_properties(conn_right_of(row))),
                        Some(fill_color(conn_right_fill(i, last, current))),
                        "n={n} current={current}: right connector of step {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn dom_hides_the_two_outer_connectors_of_every_stepper() {
        for n in [1usize, 2, 5] {
            for current in 0..n {
                let dom = Stepper::create(n_labels(n))
                    .with_current_step(current)
                    .dom();
                let children = dom.children.as_ref();

                let first_row = row_of(&children[0]);
                assert_eq!(
                    background_color(&inline_properties(conn_left_of(first_row))),
                    Some(TRANSPARENT_COLOR),
                    "n={n} current={current}: the track sticks out to the left of step 1"
                );

                let last_row = row_of(&children[n - 1]);
                assert_eq!(
                    background_color(&inline_properties(conn_right_of(last_row))),
                    Some(TRANSPARENT_COLOR),
                    "n={n} current={current}: the track sticks out past the final step"
                );
            }
        }
    }

    #[test]
    fn dom_of_a_single_step_hides_both_of_its_connectors() {
        let dom = stepper(&["only"]).dom();
        let row = row_of(step_cell(&dom, 0));
        assert_eq!(
            background_color(&inline_properties(conn_left_of(row))),
            Some(TRANSPARENT_COLOR)
        );
        assert_eq!(
            background_color(&inline_properties(conn_right_of(row))),
            Some(TRANSPARENT_COLOR)
        );
        assert_eq!(
            inline_properties(circle_of(row)),
            properties(&circle_style(true)),
            "a lone step is always the current one"
        );
    }

    #[test]
    fn dom_marks_a_contiguous_reached_prefix() {
        // "Reached" must be a prefix, never a hole: exactly `current + 1` accent
        // circles, all at the front.
        for n in [1usize, 4, 9] {
            for current in 0..n {
                let dom = Stepper::create(n_labels(n))
                    .with_current_step(current)
                    .dom();
                let accent: Vec<usize> = dom
                    .children
                    .as_ref()
                    .iter()
                    .enumerate()
                    .filter(|(_, cell)| {
                        background_color(&inline_properties(circle_of(row_of(cell))))
                            == Some(ACCENT_COLOR)
                    })
                    .map(|(i, _)| i)
                    .collect();
                assert_eq!(
                    accent,
                    (0..=current).collect::<Vec<_>>(),
                    "n={n} current={current}: the reached prefix is not contiguous"
                );
            }
        }
    }

    #[test]
    fn dom_with_an_out_of_range_current_step_renders_fully_complete_without_panicking() {
        // `current_step` is a public field, so it can be written past the end
        // without going through the clamping setter. Rendering must degrade to
        // "everything reached" rather than panic or wrap onto a real step.
        for current in [3usize, 4, 1_000, usize::MAX - 1, usize::MAX] {
            let mut s = Stepper::create(n_labels(3));
            s.stepper_state.inner.current_step = current;
            let dom = s.dom();

            assert_eq!(
                dom.children.as_ref().len(),
                3,
                "current={current}: child count changed"
            );
            for (i, cell) in dom.children.as_ref().iter().enumerate() {
                assert_eq!(
                    inline_properties(circle_of(row_of(cell))),
                    properties(&circle_style(true)),
                    "current={current}: circle {i} must render reached"
                );
            }
            // The two row-end connectors still win over the accent fill.
            let children = dom.children.as_ref();
            assert_eq!(
                background_color(&inline_properties(conn_left_of(row_of(&children[0])))),
                Some(TRANSPARENT_COLOR),
                "current={current}: leading edge"
            );
            assert_eq!(
                background_color(&inline_properties(conn_right_of(row_of(&children[2])))),
                Some(TRANSPARENT_COLOR),
                "current={current}: trailing edge"
            );
        }
    }

    #[test]
    fn dom_ignores_a_total_steps_that_disagrees_with_the_label_count() {
        // Only `labels.len()` drives rendering; a stale `total_steps` must not
        // emit phantom cells or truncate real ones.
        let mut s = stepper(&["a", "b", "c"]);
        s.stepper_state.inner.total_steps = 99;
        let dom = s.dom();
        assert_eq!(dom.children.as_ref().len(), 3);
        assert_eq!(
            background_color(&inline_properties(conn_right_of(row_of(step_cell(
                &dom, 2
            ))))),
            Some(TRANSPARENT_COLOR),
            "`last` must come from the rendered label count, not from total_steps"
        );
    }

    #[test]
    fn dom_makes_every_step_clickable_and_keyboard_reachable() {
        let n = 3;
        let dom = Stepper::create(n_labels(n)).dom();
        for (i, cell) in dom.children.as_ref().iter().enumerate() {
            let cbs = cell.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 1, "step {i}: exactly one handler");
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::Click)
            );
            assert_eq!(cbs.as_ref()[0].callback.cb, on_step_click as usize);
            assert!(matches!(cbs.as_ref()[0].callback.ctx, OptionRefAny::None));
            assert_eq!(
                cell.root.get_tab_index(),
                Some(TabIndex::Auto),
                "step {i} must be tab-reachable"
            );

            // Only the cell is clickable — a handler on an inner node would
            // resolve its index against the wrong sibling list.
            let row = row_of(cell);
            for (name, inner) in [
                ("row", row),
                ("conn-left", conn_left_of(row)),
                ("circle", circle_of(row)),
                ("conn-right", conn_right_of(row)),
                ("label", label_of(cell)),
            ] {
                assert!(
                    inner.root.get_callbacks().as_ref().is_empty(),
                    "step {i}: the {name} node must not carry its own handler"
                );
            }
        }
    }

    #[test]
    fn dom_shares_one_state_refany_across_every_step() {
        // The handler resolves the clicked index from the DOM, so all cells *must*
        // observe the same state — a per-cell copy would let two steps believe
        // they are both current.
        let dom = Stepper::create(n_labels(4)).dom();

        let mut first = step_state(&dom, 0);
        {
            let mut w = first
                .downcast_mut::<StepperStateWrapper>()
                .expect("step state must be a StepperStateWrapper");
            w.inner.current_step = 3;
        }

        for i in 1..4 {
            let mut other = step_state(&dom, i);
            assert_eq!(
                current_step_of(&mut other),
                3,
                "step {i} does not share step 0's state"
            );
        }
    }

    #[test]
    fn dom_carries_the_installed_callback_and_the_total_into_the_shared_state() {
        let dom = Stepper::create(n_labels(4))
            .with_current_step(2)
            .with_on_step_change(RefAny::new(0u8), cb(step_refresh_all))
            .dom();
        let mut state = step_state(&dom, 0);
        let wrapper = state
            .downcast_ref::<StepperStateWrapper>()
            .expect("StepperStateWrapper");

        assert_eq!(
            wrapper.inner,
            StepperState {
                current_step: 2,
                total_steps: 4
            }
        );
        let installed = wrapper
            .on_step_change
            .as_ref()
            .expect("the callback must reach the DOM");
        assert_eq!(installed.callback.cb as usize, step_refresh_all as usize);
    }

    #[test]
    fn dom_preserves_adversarial_labels_verbatim() {
        for s in adversarial_strings() {
            let dom = Stepper::create(labels(&[s.as_str(), "other"])).dom();
            let children = dom.children.as_ref();
            assert_eq!(children.len(), 2);
            match label_of(&children[0]).children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(t) => {
                        assert_eq!(
                            t.as_ref().as_str(),
                            s.as_str(),
                            "the caption changed inside dom()"
                        );
                        assert_eq!(
                            t.as_ref().len(),
                            s.len(),
                            "byte length changed (NUL truncation?)"
                        );
                    }
                    other => panic!("expected a text node, got {other:?}"),
                },
                other => panic!("expected `p > text`, got {} children", other.len()),
            }
            // The circle number must not be affected by the caption next to it.
            assert_eq!(text_of(circle_of(row_of(&children[0]))), Some("1"));
        }
    }

    #[test]
    fn dom_keeps_estimated_total_children_in_sync() {
        // `estimated_total_children` is a cached count; if it under-counts,
        // `convert_dom_into_compact_dom` under-allocates and panics.
        for n in [0usize, 1, 2, 3, 5, 64, 257] {
            let dom = Stepper::create(n_labels(n)).dom();
            assert_eq!(dom.children.as_ref().len(), n, "child count for n={n}");
            assert_eq!(
                dom.estimated_total_children,
                recursive_descendants(&dom),
                "cached descendant count desynced for n={n}"
            );
            assert_eq!(
                dom.estimated_total_children,
                NODES_PER_STEP * n,
                "a step must cost exactly {NODES_PER_STEP} nodes (n={n})"
            );
        }
    }

    #[test]
    fn dom_of_many_steps_flattens_without_panicking() {
        let n = 512;
        let styled = StyledDom::create_from_dom(Stepper::create(n_labels(n)).dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            NODES_PER_STEP * n + 1,
            "root + eight nodes per step"
        );
    }

    #[test]
    fn flattened_layout_is_eight_nodes_per_step() {
        // Pins the pre-order node numbering the click tests below depend on.
        let n = 4;
        let styled = StyledDom::create_from_dom(Stepper::create(n_labels(n)).dom());
        let data = styled.node_data.as_container();
        assert_eq!(styled.node_hierarchy.as_ref().len(), NODES_PER_STEP * n + 1);

        assert!(data
            .get(NodeId::new(0))
            .expect("root")
            .has_class("__azul-native-stepper"));

        for i in 0..n {
            for (idx, class) in [
                (cell_node(i), "__azul-native-stepper-step"),
                (row_node(i), "__azul-native-stepper-row"),
                (conn_left_node(i), "__azul-native-stepper-connector"),
                (circle_node(i), "__azul-native-stepper-circle"),
                (conn_right_node(i), "__azul-native-stepper-connector"),
                (label_node(i), "__azul-native-stepper-label"),
            ] {
                let nd = data
                    .get(NodeId::new(idx))
                    .unwrap_or_else(|| panic!("step {i}: node {idx} is missing"));
                assert!(nd.has_class(class), "step {i}: node {idx} is not a {class}");
            }
            assert!(
                matches!(
                    data.get(NodeId::new(circle_node(i)))
                        .expect("circle")
                        .get_node_type(),
                    NodeType::P
                ),
                "step {i}: the circle box must be a <p> — a text node owns no rect, so \
                 width/height/border-radius could never make it a circle"
            );
            let want = format!("{}", i + 1);
            match data
                .get(NodeId::new(circle_node(i) + 1))
                .expect("circle text")
                .get_node_type()
            {
                NodeType::Text(t) => {
                    assert_eq!(
                        t.as_ref().as_str(),
                        want.as_str(),
                        "step {i}: circle number"
                    );
                }
                other => panic!("step {i}: the circle does not wrap a text node: {other:?}"),
            }
        }
    }

    #[test]
    fn dom_via_from_matches_dom_exactly() {
        let build = || Stepper::create(n_labels(4)).with_current_step(2);
        let via_into: Dom = build().into();
        let via_dom = build().dom();

        assert_eq!(
            via_into.children.as_ref().len(),
            via_dom.children.as_ref().len()
        );
        assert_eq!(
            via_into.estimated_total_children,
            via_dom.estimated_total_children
        );
        for i in 0..via_dom.children.as_ref().len() {
            let a = step_cell(&via_into, i);
            let b = step_cell(&via_dom, i);
            assert_eq!(
                inline_properties(a),
                inline_properties(b),
                "`From` diverges at cell {i}"
            );
            assert_eq!(
                inline_properties(circle_of(row_of(a))),
                inline_properties(circle_of(row_of(b))),
                "`From` diverges at circle {i}"
            );
            assert_eq!(text_of(label_of(a)), text_of(label_of(b)));
        }
    }

    #[test]
    fn dom_with_duplicate_labels_still_produces_distinct_positional_steps() {
        // Position, not caption, decides reached-ness.
        let dom = stepper(&["same", "same", "same"])
            .with_current_step(1)
            .dom();
        for (i, cell) in dom.children.as_ref().iter().enumerate() {
            assert_eq!(text_of(label_of(cell)), Some("same"));
            assert_eq!(
                inline_properties(circle_of(row_of(cell))),
                properties(&circle_style(i <= 1)),
                "step {i}"
            );
            let want = format!("{}", i + 1);
            assert_eq!(
                text_of(circle_of(row_of(cell))),
                Some(want.as_str()),
                "step {i}: the number must still be positional"
            );
        }
    }

    #[test]
    fn dom_gives_every_cell_the_shared_equal_share_style() {
        let dom = Stepper::create(n_labels(3)).dom();
        for (i, cell) in dom.children.as_ref().iter().enumerate() {
            let props = inline_properties(cell);
            assert_eq!(
                props,
                properties(&CssPropertyWithConditionsVec::from_const_slice(
                    STEPPER_STEP_STYLE
                )),
                "cell {i} does not carry the shared step style"
            );
            assert_eq!(
                flex_grow_value(&props),
                Some(1.0),
                "cell {i}: steps must spread evenly across the row"
            );
            assert!(
                props.iter().any(|p| matches!(
                    p, CssProperty::FlexBasis(b)
                        if matches!(b.get_property(), Some(LayoutFlexBasis::Exact(pv))
                            if pv.metric == SizeMetric::Px && pv.number.get() == 0.0))),
                "cell {i}: flex-basis must be 0 so flex-grow alone decides the share"
            );
        }
    }

    // ==================================================================
    // on_step_click
    // ==================================================================

    #[test]
    fn click_moves_to_the_clicked_step_and_restyles_the_whole_row() {
        let n = 4;
        for clicked in 1..n {
            let (styled, state) = flatten(Stepper::create(n_labels(n)));
            let mut probe = state.clone();

            let (update, changes) = run_click(Some(styled), node(cell_node(clicked)), state);

            assert_eq!(
                update,
                Update::DoNothing,
                "with no on_step_change installed the handler reports nothing to redraw"
            );
            assert_eq!(
                current_step_of(&mut probe),
                clicked,
                "the stored step did not move"
            );
            assert_eq!(
                restyle_writes(&changes),
                expected_restyle(n, clicked),
                "clicked={clicked}: the live restyle is wrong"
            );
        }
    }

    #[test]
    fn click_restyle_agrees_with_a_freshly_built_dom() {
        // The live restyle and a full rebuild must not drift apart, or a click
        // followed by a `RefreshDom` would visibly change the widget twice.
        let n = 5;
        for clicked in 1..n {
            let (styled, state) = flatten(Stepper::create(n_labels(n)));
            let (_, changes) = run_click(Some(styled), node(cell_node(clicked)), state);
            let writes = restyle_writes(&changes);
            assert_eq!(
                writes.len(),
                5 * n,
                "clicked={clicked}: five writes per step"
            );

            let rebuilt = Stepper::create(n_labels(n))
                .with_current_step(clicked)
                .dom();
            for i in 0..n {
                let cell = step_cell(&rebuilt, i);
                let row = row_of(cell);
                let circle = inline_properties(circle_of(row));
                let label = inline_properties(label_of(cell));

                assert_eq!(
                    writes[5 * i],
                    (
                        circle_node(i),
                        "bg",
                        background_color(&circle).expect("circle bg")
                    ),
                    "clicked={clicked}: circle {i} background"
                );
                assert_eq!(
                    writes[5 * i + 1],
                    (
                        circle_node(i),
                        "text",
                        text_color(&circle).expect("circle text")
                    ),
                    "clicked={clicked}: circle {i} number colour"
                );
                assert_eq!(
                    writes[5 * i + 2],
                    (
                        conn_left_node(i),
                        "bg",
                        background_color(&inline_properties(conn_left_of(row)))
                            .expect("left connector bg")
                    ),
                    "clicked={clicked}: left connector {i}"
                );
                assert_eq!(
                    writes[5 * i + 3],
                    (
                        conn_right_node(i),
                        "bg",
                        background_color(&inline_properties(conn_right_of(row)))
                            .expect("right connector bg")
                    ),
                    "clicked={clicked}: right connector {i}"
                );
                assert_eq!(
                    writes[5 * i + 4],
                    (
                        label_node(i),
                        "text",
                        text_color(&label).expect("label text")
                    ),
                    "clicked={clicked}: label {i} colour"
                );
            }
        }
    }

    #[test]
    fn click_on_the_already_current_step_is_a_complete_no_op() {
        // Documented: no state write, no callback, and — critically — no restyle,
        // so a repeated click cannot flicker the row.
        for current in 0..4usize {
            let mut log = RefAny::new(StepLog { seen: Vec::new() });
            let s = Stepper::create(n_labels(4))
                .with_current_step(current)
                .with_on_step_change(log.clone(), cb(record_step));
            let (styled, state) = flatten(s);
            let mut probe = state.clone();

            let (update, changes) = run_click(Some(styled), node(cell_node(current)), state);

            assert_eq!(update, Update::DoNothing, "current={current}");
            assert!(
                changes.is_empty(),
                "current={current}: a no-op click restyled the row"
            );
            assert_eq!(
                current_step_of(&mut probe),
                current,
                "current={current}: state moved"
            );
            assert!(
                logged(&mut log).is_empty(),
                "current={current}: the callback was invoked"
            );
        }
    }

    #[test]
    fn click_invokes_the_user_callback_with_the_updated_state() {
        let mut log = RefAny::new(StepLog { seen: Vec::new() });
        let s = Stepper::create(n_labels(4)).with_on_step_change(log.clone(), cb(record_step));
        let (styled, state) = flatten(s);

        let (update, changes) = run_click(Some(styled), node(cell_node(2)), state.clone());
        assert_eq!(
            update,
            Update::RefreshDom,
            "the user's Update must propagate"
        );
        assert_eq!(
            logged(&mut log),
            vec![StepperState {
                current_step: 2,
                total_steps: 4
            }],
            "the callback must see the *new* step, with the total intact"
        );
        assert_eq!(
            restyle_writes(&changes).len(),
            20,
            "the restyle must still run"
        );

        // A second click moves the shared state again — the step is not sticky.
        let (styled2, _) = flatten(Stepper::create(n_labels(4)));
        let (_, _) = run_click(Some(styled2), node(cell_node(1)), state.clone());
        assert_eq!(
            logged(&mut log),
            vec![
                StepperState {
                    current_step: 2,
                    total_steps: 4
                },
                StepperState {
                    current_step: 1,
                    total_steps: 4
                },
            ]
        );

        let mut state = state;
        assert_eq!(
            current_step_of(&mut state),
            1,
            "the state holds the *last* clicked step"
        );
    }

    #[test]
    fn click_propagates_every_update_variant_unchanged() {
        for (handler, expected) in [
            (cb(step_do_nothing), Update::DoNothing),
            (cb(step_refresh_all), Update::RefreshDomAllWindows),
        ] {
            let s = Stepper::create(n_labels(2)).with_on_step_change(RefAny::new(0u8), handler);
            let (styled, state) = flatten(s);
            let (update, changes) = run_click(Some(styled), node(cell_node(1)), state);
            assert_eq!(update, expected);
            assert_eq!(
                restyle_writes(&changes).len(),
                10,
                "the restyle runs regardless of what the user returns"
            );
        }
    }

    #[test]
    fn click_restyles_even_without_a_user_callback() {
        let (styled, state) = flatten(stepper(&["a", "b"]));
        let (update, changes) = run_click(Some(styled), node(cell_node(1)), state);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            restyle_writes(&changes),
            expected_restyle(2, 1),
            "progress feedback must not depend on the user wiring a callback"
        );
    }

    #[test]
    fn click_on_a_single_step_stepper_stays_at_zero() {
        let (styled, state) = flatten(stepper(&["only"]));
        let mut probe = state.clone();
        let (update, changes) = run_click(Some(styled), node(cell_node(0)), state);

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a one-step stepper has nothing to restyle"
        );
        assert_eq!(current_step_of(&mut probe), 0);
    }

    #[test]
    fn click_can_walk_backwards() {
        // Free navigation: a stepper is not a one-way ratchet, so clicking an
        // earlier step must un-fill the track behind it.
        let n = 5;
        let (styled, state) = flatten(Stepper::create(n_labels(n)).with_current_step(4));
        let mut probe = state.clone();

        let (_, changes) = run_click(Some(styled), node(cell_node(1)), state);

        assert_eq!(current_step_of(&mut probe), 1);
        assert_eq!(restyle_writes(&changes), expected_restyle(n, 1));
        // Steps 2..4 must have been actively reset, not merely left alone.
        assert!(
            restyle_writes(&changes).contains(&(circle_node(4), "bg", MUTED_CIRCLE_COLOR)),
            "walking back left a stale accent circle behind"
        );
    }

    #[test]
    fn click_on_the_root_node_does_nothing() {
        // The container has no parent -> the handler must bail, not index into nothing.
        let (styled, state) = flatten(stepper(&["a", "b"]));
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), node(0), state);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a parentless hit pushed a DOM change");
        assert_eq!(
            current_step_of(&mut probe),
            0,
            "the state must be untouched"
        );
    }

    #[test]
    fn click_on_a_stale_or_absent_node_does_nothing() {
        // Stale hit ids reach callbacks after a DOM mutation, and
        // `set_css_property` *panics* on a None node id — so the handler has to
        // bail out well before the restyle loop.
        for hit in [node(9999), node(usize::MAX - 1), node_none()] {
            let (styled, state) = flatten(Stepper::create(n_labels(3)).with_current_step(1));
            let mut probe = state.clone();

            let (update, changes) = run_click(Some(styled), hit, state);

            assert_eq!(
                update,
                Update::DoNothing,
                "{hit:?}: a stale hit was acted on"
            );
            assert!(
                changes.is_empty(),
                "{hit:?}: a stale hit pushed a DOM change"
            );
            assert_eq!(
                current_step_of(&mut probe),
                1,
                "{hit:?}: a stale hit moved the step"
            );
        }
    }

    #[test]
    fn click_with_no_layout_result_does_nothing() {
        let dom = stepper(&["a", "b"]).dom();
        let state = step_state(&dom, 0);

        let (update, changes) = run_click(None, node(cell_node(1)), state);

        assert_eq!(
            update,
            Update::DoNothing,
            "an empty LayoutWindow must be handled, not unwrapped"
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn click_with_a_foreign_payload_does_nothing_and_leaves_it_intact() {
        // The handler downcasts blind; a foreign RefAny must bail out, not
        // reinterpret the bytes as a StepperStateWrapper.
        let (styled, _) = flatten(stepper(&["a", "b"]));
        let foreign = RefAny::new(0xDEAD_BEEF_u32);

        let (update, changes) = run_click(Some(styled), node(cell_node(1)), foreign.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a failed downcast must not leave a half-applied restyle"
        );
        let mut foreign = foreign;
        assert_eq!(
            *foreign
                .downcast_ref::<u32>()
                .expect("the foreign payload was reinterpreted"),
            0xDEAD_BEEF,
            "the handler corrupted a RefAny it did not understand"
        );
    }

    #[test]
    fn click_with_the_state_already_borrowed_does_nothing() {
        let (styled, state) = flatten(stepper(&["a", "b"]));

        // A live mutable borrow on a sibling clone: the `downcast_ref` inside the
        // handler must fail (returning DoNothing) instead of aliasing `&mut`.
        let mut held = state.clone();
        let guard = held
            .downcast_mut::<StepperStateWrapper>()
            .expect("first borrow succeeds");

        let (update, changes) = run_click(Some(styled), node(cell_node(1)), state);

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "the handler restyled after failing to read the state"
        );
        drop(guard);
    }

    #[test]
    fn click_holds_the_state_borrow_across_the_user_callback() {
        // The handler invokes the user callback while its own `downcast_mut` on
        // the state is still live. A user callback that re-enters the *same*
        // state `RefAny` is therefore refused — it must get `None` back rather
        // than a second aliasing borrow (or a panic).
        //
        // NOTE: probe <-> state form a RefAny reference cycle, so this fixture
        // leaks. That is deliberate and harmless for a single test.
        let mut probe = RefAny::new(ReentrantProbe {
            state: RefAny::new(0u8),
            saw_step: Some(usize::MAX),
            calls: 0,
        });
        let s = Stepper::create(n_labels(3))
            .with_on_step_change(probe.clone(), cb(probe_state_reentrantly));
        let (styled, state) = flatten(s);
        {
            let mut p = probe.downcast_mut::<ReentrantProbe>().expect("probe");
            p.state = state.clone();
        }

        let (update, changes) = run_click(Some(styled), node(cell_node(2)), state);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            restyle_writes(&changes).len(),
            15,
            "the restyle must still run"
        );

        let p = probe.downcast_ref::<ReentrantProbe>().expect("probe");
        assert_eq!(p.calls, 1, "the user callback must run exactly once");
        assert_eq!(
            p.saw_step, None,
            "a re-entrant read of the state must be refused, not aliased"
        );
    }

    #[test]
    fn a_hit_inside_a_cell_stays_memory_safe() {
        // The handler documents `currentTarget` semantics: only the step cells
        // carry the callback, so the hit node is always a cell. Should an inner
        // node ever reach it anyway, it must stay memory-safe and push no
        // half-finished restyle — the sibling walk simply finds no cells to
        // update. (Documenting the actual resolution, not endorsing it: an inner
        // hit resolves against *its own* siblings, so it can move the stored step
        // without any visual feedback.)
        for (hit, expected_step) in [
            (row_node(0), 0usize),   // row is child 0 of its cell -> no change
            (row_node(1), 0),        // ditto, whichever cell it belongs to
            (conn_left_node(0), 0),  // connector-left is child 0 of its row
            (circle_node(0), 1),     // circle is child 1 of its row
            (conn_right_node(0), 2), // connector-right is child 2 of its row
        ] {
            let (styled, state) = flatten(Stepper::create(n_labels(3)));
            let mut probe = state.clone();

            let (update, changes) = run_click(Some(styled), node(hit), state);

            assert_eq!(update, Update::DoNothing, "node {hit}");
            assert!(
                changes.is_empty(),
                "node {hit}: an inner hit pushed a partial restyle"
            );
            assert_eq!(current_step_of(&mut probe), expected_step, "node {hit}");
        }
    }

    #[test]
    fn many_clicks_keep_the_state_and_the_restyle_in_agreement() {
        // A drift between the stored step and the pushed colours is exactly the
        // class of bug that makes a wizard render a step it does not hold.
        let n = 5;
        let (_, state) = flatten(Stepper::create(n_labels(n)));

        for click in 0..60usize {
            // Never click the current step twice in a row — that is a documented
            // no-op and would push no changes at all.
            let expected = (click * 2 + 1) % n;
            let mut probe = state.clone();
            if current_step_of(&mut probe) == expected {
                continue;
            }

            let (styled, _) = flatten(Stepper::create(n_labels(n)));
            let (_, changes) = run_click(Some(styled), node(cell_node(expected)), state.clone());

            assert_eq!(
                current_step_of(&mut probe),
                expected,
                "click #{click}: the stored step drifted"
            );
            assert_eq!(
                restyle_writes(&changes),
                expected_restyle(n, expected),
                "click #{click}: the pushed colours disagree with the stored step"
            );
        }
    }

    #[test]
    fn click_never_touches_total_steps() {
        let n = 4;
        let (styled, state) = flatten(Stepper::create(n_labels(n)));
        let (_, _) = run_click(Some(styled), node(cell_node(3)), state.clone());

        let mut state = state;
        let wrapper = state
            .downcast_ref::<StepperStateWrapper>()
            .expect("StepperStateWrapper");
        assert_eq!(
            wrapper.inner,
            StepperState {
                current_step: 3,
                total_steps: n
            },
            "a click must move the current step and nothing else"
        );
    }
}
