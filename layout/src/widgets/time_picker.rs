//! Time picker widget — two numeric up/down spinners (hour + minute) side by
//! side with value-clamping, plus an optional AM/PM toggle for 12-hour mode.
//!
//! This is the spinner cousin of [`crate::widgets::number_input`]: each spinner
//! is a small column of an up arrow (`▲`), a value display, and a down arrow
//! (`▼`). Clicking an arrow increments/decrements the value, **clamps** it to
//! its range (hour `0..=23` in 24-hour mode or `1..=12` in 12-hour mode, minute
//! `0..=59`), updates the state, retexts the display node via
//! `info.change_node_text`, and invokes the optional `on_change(state)`.
//!
//! The clamping/retext path mirrors `number_input.rs` (a proven pattern) and the
//! clickable-cell + sibling navigation mirrors `segmented.rs`, so this widget is
//! well-supported. The only deliberate behaviour note:
//!
//! PARTIAL — minute wrap-around does NOT roll into the hour. Per the build spec,
//! incrementing minute past 59 (or below 0) simply clamps; it does not carry
//! into the hour spinner. A carry would require coordinating two sibling
//! displays from one handler, which is doable but out of scope here; clamping is
//! the conservative, non-surprising behaviour.
//!
//! Key types: [`TimePicker`], [`TimePickerState`], [`TimePickerOnChange`].

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::{OptionRefAny, RefAny},
};
use azul_css::dynamic_selector::CssPropertyWithConditions;
use azul_css::dynamic_selector::CssPropertyWithConditionsVec;
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutWidth, LayoutMarginLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextAlign, StyleCursor, StyleUserSelect, StyleTextColor},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

// ---- classes ----
static TIME_PICKER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-time-picker"))];
static SPINNER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-time-picker-spinner"))];
static DISPLAY_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-time-picker-display"))];
static ARROW_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-time-picker-arrow"))];
static SEPARATOR_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-time-picker-separator"))];
static AMPM_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-time-picker-ampm"))];

const UP_ARROW: AzString = AzString::from_const_str("\u{25B2}"); // ▲
const DOWN_ARROW: AzString = AzString::from_const_str("\u{25BC}"); // ▼
const SEPARATOR_TEXT: AzString = AzString::from_const_str(":");

/// Callback type invoked when the hour, minute, or AM/PM value changes.
pub type TimePickerOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TimePickerState) -> Update;
impl_widget_callback!(
    TimePickerOnChange,
    OptionTimePickerOnChange,
    TimePickerOnChangeCallback,
    TimePickerOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TimePickerOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TIME_PICKER_ON_CHANGE_INVOKER,
    invoker_ty:     AzTimePickerOnChangeCallbackInvoker,
    thunk_fn:       az_time_picker_on_change_callback_thunk,
    setter_fn:      AzApp_setTimePickerOnChangeCallbackInvoker,
    from_handle_fn: AzTimePickerOnChangeCallback_createFromHostHandle,
    extra_args:     [ state: TimePickerState ],
}

/// A time picker: two clamped spinners (hour + minute) and an optional AM/PM
/// toggle.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TimePicker {
    pub state: TimePickerStateWrapper,
    /// Style for the row container.
    pub container_style: CssPropertyWithConditionsVec,
}

/// Wraps [`TimePickerState`] together with its change callback.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TimePickerStateWrapper {
    pub inner: TimePickerState,
    pub on_change: OptionTimePickerOnChange,
}

/// State of a [`TimePicker`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TimePickerState {
    /// The displayed hour: `0..=23` when [`Self::is_24h`], else `1..=12`.
    pub hour: u32,
    /// The minute, `0..=59`.
    pub minute: u32,
    /// PM flag — only meaningful in 12-hour mode (ignored when `is_24h`).
    pub is_pm: bool,
    /// `true` = 24-hour display (no AM/PM), `false` = 12-hour display + AM/PM.
    pub is_24h: bool,
}

impl Default for TimePickerState {
    fn default() -> Self {
        Self {
            hour: 0,
            minute: 0,
            is_pm: false,
            is_24h: true,
        }
    }
}

impl TimePickerState {
    /// Returns the hour in canonical 24-hour form (`0..=23`), accounting for the
    /// AM/PM flag in 12-hour mode (12 AM -> 0, 12 PM -> 12).
    #[must_use] pub const fn canonical_hour(&self) -> u32 {
        if self.is_24h {
            self.hour
        } else {
            let h12 = self.hour % 12; // 12 -> 0
            h12 + if self.is_pm { 12 } else { 0 }
        }
    }

    #[inline]
    const fn hour_bounds(&self) -> (i64, i64) {
        if self.is_24h {
            (0, 23)
        } else {
            (1, 12)
        }
    }
}

// ---- colours ----
const BORDER_COLOR: ColorU = ColorU { r: 206, g: 212, b: 218, a: 255 };
const ARROW_COLOR: ColorU = ColorU { r: 73, g: 80, b: 87, a: 255 };
const TEXT_COLOR: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };
const ACCENT_BG: ColorU = ColorU { r: 13, g: 110, b: 253, a: 255 };
const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };

const ACCENT_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(ACCENT_BG)];
const ACCENT_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(ACCENT_BG_ITEMS);

/// Container: a horizontal row that hugs its content.
static CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        6,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: BORDER_COLOR },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: BORDER_COLOR },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(6),
    )),
];

/// One spinner column: up arrow, value, down arrow.
static SPINNER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(40))),
];

/// Up/down arrow cell.
static ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: ARROW_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(2),
    )),
];

/// The value display in the middle of a spinner.
static DISPLAY_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(2),
    )),
];

/// The `:` separator between the hour and minute spinners.
static SEPARATOR_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        2,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(2),
    )),
];

/// The clickable AM/PM toggle (12-hour mode only).
static AMPM_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor { inner: WHITE })),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(ACCENT_BG_VEC)),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        8,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(4),
    )),
];

impl TimePicker {
    /// Creates a new 24-hour `TimePicker` with the given initial hour (`0..=23`)
    /// and minute (`0..=59`), both clamped into range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
    #[must_use] pub fn create(hour: u32, minute: u32) -> Self {
        let mut inner = TimePickerState::default();
        let (lo, hi) = inner.hour_bounds();
        inner.hour = i64::from(hour).clamp(lo, hi) as u32;
        inner.minute = i64::from(minute).clamp(0, 59) as u32;
        Self {
            state: TimePickerStateWrapper {
                inner,
                on_change: None.into(),
            },
            container_style: CssPropertyWithConditionsVec::from_const_slice(CONTAINER_STYLE),
        }
    }

    /// Switches between 24-hour (no AM/PM) and 12-hour (with AM/PM) display,
    /// re-clamping the hour into the new range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
    pub fn set_24h(&mut self, is_24h: bool) {
        self.state.inner.is_24h = is_24h;
        let (lo, hi) = self.state.inner.hour_bounds();
        self.state.inner.hour = i64::from(self.state.inner.hour).clamp(lo, hi) as u32;
    }

    /// Builder variant of [`Self::set_24h`].
    #[must_use] pub fn with_24h(mut self, is_24h: bool) -> Self {
        self.set_24h(is_24h);
        self
    }

    /// Sets the AM/PM flag (only meaningful in 12-hour mode).
    pub const fn set_pm(&mut self, is_pm: bool) {
        self.state.inner.is_pm = is_pm;
    }

    /// Builder variant of [`Self::set_pm`].
    #[must_use] pub const fn with_pm(mut self, is_pm: bool) -> Self {
        self.set_pm(is_pm);
        self
    }

    /// Sets the callback invoked when any value changes.
    pub fn set_on_change<C: Into<TimePickerOnChangeCallback>>(&mut self, data: RefAny, callback: C) {
        self.state.on_change = Some(TimePickerOnChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder variant of [`Self::set_on_change`].
    #[must_use] pub fn with_on_change<C: Into<TimePickerOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// Replaces `self` with the default value and returns the original.
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0, 0);
        core::mem::swap(&mut s, self);
        s
    }

    #[must_use] pub fn dom(self) -> Dom {
        let inner = self.state.inner;
        let is_24h = inner.is_24h;
        let hour_text = AzString::from(format!("{}", inner.hour));
        let minute_text = AzString::from(format!("{:02}", inner.minute));
        let container_style = self.container_style.clone();

        let state = RefAny::new(self.state);

        let mut children = alloc::vec![
            build_spinner(
                hour_text,
                state.clone(),
                on_hour_up as usize,
                on_hour_down as usize,
            ),
            Dom::create_p_with_text(SEPARATOR_TEXT)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(SEPARATOR_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SEPARATOR_STYLE)),
            build_spinner(
                minute_text,
                state.clone(),
                on_minute_up as usize,
                on_minute_down as usize,
            ),
        ];

        if !is_24h {
            let ampm_text = if inner.is_pm {
                AzString::from_const_str("PM")
            } else {
                AzString::from_const_str("AM")
            };
            children.push(
                Dom::create_p_with_text(ampm_text)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(AMPM_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(AMPM_STYLE))
                    .with_callbacks(
                        alloc::vec![CoreCallbackData {
                            event: azul_core::dom::EventFilter::Hover(
                                azul_core::dom::HoverEventFilter::MouseUp,
                            ),
                            callback: CoreCallback {
                                cb: on_ampm_toggle as usize,
                                ctx: OptionRefAny::None,
                            },
                            refany: state,
                        }]
                        .into(),
                    )
                    .with_tab_index(TabIndex::Auto)
                    // Hour/minute steppers act as buttons.
                    .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                        role: azul_core::a11y::AccessibilityRole::PushButton,
                        ..Default::default()
                    }),
            );
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TIME_PICKER_CLASS))
            .with_css_props(container_style)
            .with_children(children.into())
    }
}

impl Default for TimePicker {
    fn default() -> Self {
        Self::create(0, 0)
    }
}

/// Builds one spinner column (up arrow / value display / down arrow). The up and
/// down arrows carry the shared `state` `RefAny` and the given click handlers; the
/// middle display is class-tagged so handlers can re-text it.
fn build_spinner(value: AzString, state: RefAny, up_cb: usize, down_cb: usize) -> Dom {
    use azul_core::dom::{EventFilter, HoverEventFilter};

    let arrow_cell = |arrow: AzString, cb: usize, refany: RefAny| -> Dom {
        Dom::create_p_with_text(arrow)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ARROW_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(ARROW_STYLE))
            .with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb,
                        ctx: OptionRefAny::None,
                    },
                    refany,
                }]
                .into(),
            )
            .with_tab_index(TabIndex::Auto)
            // The time field opens a chooser.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::ComboBox,
                ..Default::default()
            })
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SPINNER_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SPINNER_STYLE))
        .with_children(
            alloc::vec![
                arrow_cell(UP_ARROW, up_cb, state.clone()),
                Dom::create_p_with_text(value)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(DISPLAY_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(DISPLAY_STYLE)),
                arrow_cell(DOWN_ARROW, down_cb, state),
            ]
            .into(),
        )
}

/// Shared spinner logic: clamps the targeted field, re-texts the display node
/// (the middle child of the clicked arrow's parent spinner), and fires the
/// optional `on_change`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
fn adjust_spinner(mut data: RefAny, mut info: CallbackInfo, is_hour: bool, delta: i64) -> Update {
    // The clicked node is an arrow `<p>`; its parent is the spinner; the
    // spinner's first child is the up arrow and the next sibling is the value
    // display `<p>`, whose only child is the re-textable bare text node.
    let hit = info.get_hit_node();
    let Some(parent) = info.get_parent(hit) else {
        return Update::DoNothing;
    };
    let Some(up) = info.get_first_child(parent) else {
        return Update::DoNothing;
    };
    let Some(display_box) = info.get_next_sibling(up) else {
        return Update::DoNothing;
    };
    let Some(display) = info.get_first_child(display_box) else {
        return Update::DoNothing;
    };

    let (update, display_text) = {
        let Some(mut w) = data.downcast_mut::<TimePickerStateWrapper>() else {
            return Update::DoNothing;
        };

        let display_text = if is_hour {
            let (lo, hi) = w.inner.hour_bounds();
            w.inner.hour = (i64::from(w.inner.hour) + delta).clamp(lo, hi) as u32;
            AzString::from(format!("{}", w.inner.hour))
        } else {
            // PARTIAL: minute clamps; it does not wrap/carry into the hour.
            w.inner.minute = (i64::from(w.inner.minute) + delta).clamp(0, 59) as u32;
            AzString::from(format!("{:02}", w.inner.minute))
        };

        let inner = w.inner;
        let w = &mut *w;
        let update = match w.on_change.as_mut() {
            Some(TimePickerOnChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        };
        (update, display_text)
    };

    info.change_node_text(display, display_text);
    update
}

extern "C" fn on_hour_up(data: RefAny, info: CallbackInfo) -> Update {
    adjust_spinner(data, info, true, 1)
}

extern "C" fn on_hour_down(data: RefAny, info: CallbackInfo) -> Update {
    adjust_spinner(data, info, true, -1)
}

extern "C" fn on_minute_up(data: RefAny, info: CallbackInfo) -> Update {
    adjust_spinner(data, info, false, 1)
}

extern "C" fn on_minute_down(data: RefAny, info: CallbackInfo) -> Update {
    adjust_spinner(data, info, false, -1)
}

/// Toggles the AM/PM flag and re-texts the clicked toggle node. The hit node is
/// the toggle's `<p>` box; the text it wraps is what gets re-texted.
extern "C" fn on_ampm_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(text_node) = info.get_first_child(hit) else {
        return Update::DoNothing;
    };

    let (update, text) = {
        let Some(mut w) = data.downcast_mut::<TimePickerStateWrapper>() else {
            return Update::DoNothing;
        };
        w.inner.is_pm = !w.inner.is_pm;
        let inner = w.inner;
        let text = if inner.is_pm {
            AzString::from_const_str("PM")
        } else {
            AzString::from_const_str("AM")
        };
        let w = &mut *w;
        let update = match w.on_change.as_mut() {
            Some(TimePickerOnChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        };
        (update, text)
    };

    info.change_node_text(text_node, text);
    update
}

impl From<TimePicker> for Dom {
    fn from(t: TimePicker) -> Self {
        t.dom()
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
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

    // ==================================================================
    // Const-evaluated extremes
    //
    // `canonical_hour` and `hour_bounds` are `const fn`, so evaluating them on
    // states holding the integer extremes in a `const` item makes the compiler
    // itself prove there is no overflow / const-eval panic on those inputs —
    // a stronger statement than any runtime assertion.
    // ==================================================================

    const EXTREME_24H: TimePickerState = TimePickerState {
        hour: u32::MAX,
        minute: u32::MAX,
        is_pm: true,
        is_24h: true,
    };
    const EXTREME_12H: TimePickerState = TimePickerState {
        hour: u32::MAX,
        minute: u32::MAX,
        is_pm: true,
        is_24h: false,
    };

    const _CANONICAL_AT_MAX_24H: u32 = EXTREME_24H.canonical_hour();
    const _CANONICAL_AT_MAX_12H: u32 = EXTREME_12H.canonical_hour();
    const _BOUNDS_AT_MAX_24H: (i64, i64) = EXTREME_24H.hour_bounds();
    const _BOUNDS_AT_MAX_12H: (i64, i64) = EXTREME_12H.hour_bounds();

    // ==================================================================
    // Flattened node layout
    //
    // `convert_dom_into_compact_dom` walks the tree in pre-order, and a time
    // picker is a fixed-shape widget. Every label is a `<p>` box wrapping one
    // bare text node (the widget label convention), so each cell costs two
    // flattened nodes:
    //
    //     container
    //       hour spinner   → [▲ <p> → text, display <p> → text, ▼ <p> → text]
    //       ":" <p> → text
    //       minute spinner → [▲ <p> → text, display <p> → text, ▼ <p> → text]
    //       AM/PM <p> → text   (12-hour mode only)
    //
    // so the flattened indices are constant. `flattened_layout_is_the_fixed
    // _seventeen_or_nineteen_nodes` pins them against the real hierarchy, so the
    // click tests below cannot silently drift onto the wrong node.
    // ==================================================================

    const N_CONTAINER: usize = 0;
    const N_HOUR_SPINNER: usize = 1;
    const N_HOUR_UP: usize = 2;
    const N_HOUR_DISPLAY: usize = 4;
    const N_HOUR_DOWN: usize = 6;
    const N_SEPARATOR: usize = 8;
    const N_MINUTE_SPINNER: usize = 10;
    const N_MINUTE_UP: usize = 11;
    const N_MINUTE_DISPLAY: usize = 13;
    /// The bare text leaf inside the minute display's `<p>` — the node
    /// `adjust_spinner` retexts (the label convention moved the text one
    /// level under the styled wrapper).
    const N_MINUTE_TEXT: usize = 14;
    const N_MINUTE_DOWN: usize = 15;
    const N_AMPM: usize = 17;

    /// The bare text node a label `<p>` wraps — pre-order puts it right after its
    /// box. This is what `change_node_text` has to target.
    const fn text_leaf(label_box: usize) -> usize {
        label_box + 1
    }

    /// Total flattened node count, 24-hour and 12-hour.
    const N_NODES_24H: usize = 17;
    const N_NODES_12H: usize = 19;

    // The class names are part of the widget's public surface: user stylesheets
    // select on them, so a rename is a breaking change and is spelled out here
    // rather than read back out of the statics under test.
    const CLASS_CONTAINER: &str = "__azul-native-time-picker";
    const CLASS_SPINNER: &str = "__azul-native-time-picker-spinner";
    const CLASS_DISPLAY: &str = "__azul-native-time-picker-display";
    const CLASS_ARROW: &str = "__azul-native-time-picker-arrow";
    const CLASS_SEPARATOR: &str = "__azul-native-time-picker-separator";
    const CLASS_AMPM: &str = "__azul-native-time-picker-ampm";

    const UP_GLYPH: &str = "\u{25B2}";
    const DOWN_GLYPH: &str = "\u{25BC}";

    // ==================================================================
    // Harness
    // ==================================================================

    /// A `DomNodeId` in the root DOM pointing at flattened node `idx`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was
    /// hit" case that every hierarchy query must decline rather than index.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` carrying only a `styled_dom`: the time-picker handlers
    /// reach exactly four `CallbackInfo` queries (`get_hit_node`, `get_parent`,
    /// `get_first_child`, `get_next_sibling`), all of which read the node
    /// hierarchy only — no real layout (and no font) is needed.
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

    /// Runs `f` with a `CallbackInfo` whose window holds `styled_dom` as the root
    /// DOM and whose hit node is `hit`. Returns `f`'s value plus every change the
    /// callback pushed onto the transaction log.
    fn with_info<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled_dom));

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
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let r = f(&mut info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    // ------------------------------------------------------------------
    // Tree probes
    // ------------------------------------------------------------------

    /// The text a node renders, or `None` for a non-text node.
    fn text_of(dom: &Dom) -> Option<String> {
        // P-wrap transparent (the label convention): a styled `<p>` wraps the
        // bare text leaf, so read through one wrapper level when present.
        dom.root.get_node_type().format().or_else(|| {
            let c = dom.children.as_ref();
            if c.len() == 1 {
                c[0].root.get_node_type().format()
            } else {
                None
            }
        })
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

    /// The classes of a *flattened* node.
    fn flat_classes(sd: &StyledDom, idx: usize) -> Vec<String> {
        sd.node_data.as_ref()[idx]
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The text of a *flattened* node.
    /// The text a flattened node renders, looking through the `<p>` block
    /// wrapper the label convention mandates (`p > text`).
    fn flat_text(sd: &StyledDom, idx: usize) -> Option<String> {
        let data = sd.node_data.as_ref();
        match data[idx].get_node_type() {
            NodeType::P => data.get(text_leaf(idx))?.get_node_type().format(),
            other => other.format(),
        }
    }

    /// The recursive `1-per-descendant` total of a `Dom`'s children — what
    /// `estimated_total_children` caches. An under-report makes the flatten
    /// under-allocate its arenas and panic on an out-of-bounds write.
    fn descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + descendants(c))
            .sum()
    }

    /// The hour column, the separator and the minute column of a rendered picker.
    fn columns(dom: &Dom) -> (&Dom, &Dom, &Dom) {
        let c = dom.children.as_ref();
        assert!(
            c.len() == 3 || c.len() == 4,
            "a time picker renders hour + ':' + minute (+ AM/PM), got {} children",
            c.len(),
        );
        (&c[0], &c[1], &c[2])
    }

    /// The `(hour, minute)` strings a rendered picker displays.
    fn displayed(dom: &Dom) -> (String, String) {
        let (hour, _, minute) = columns(dom);
        (
            text_of(&hour.children.as_ref()[1]).expect("the hour display is not a text node"),
            text_of(&minute.children.as_ref()[1]).expect("the minute display is not a text node"),
        )
    }

    /// The AM/PM label, or `None` when the widget is in 24-hour mode.
    fn ampm_label(dom: &Dom) -> Option<String> {
        dom.children.as_ref().get(3).and_then(text_of)
    }

    /// Every `RefAny` in the flattened DOM that carries the widget's own state.
    fn state_payloads(sd: &StyledDom) -> Vec<RefAny> {
        let mut out = Vec::new();
        for nd in sd.node_data.as_ref() {
            for cb in nd.callbacks.as_ref() {
                let carries = {
                    let mut r = cb.refany.clone();
                    let carries = r.downcast_ref::<TimePickerStateWrapper>().is_some();
                    carries
                };
                if carries {
                    out.push(cb.refany.clone());
                }
            }
        }
        out
    }

    /// The single shared state `RefAny` the widget baked into its own handlers.
    fn shared_state(sd: &StyledDom) -> RefAny {
        state_payloads(sd)
            .into_iter()
            .next()
            .expect("the rendered time picker carries no TimePickerStateWrapper")
    }

    /// `(flattened node id, payload)` of the node wired to `handler`.
    fn wired_to(sd: &StyledDom, handler: usize) -> (DomNodeId, RefAny) {
        for (i, nd) in sd.node_data.as_ref().iter().enumerate() {
            for cb in nd.callbacks.as_ref() {
                if cb.callback.cb == handler {
                    return (node(i), cb.refany.clone());
                }
            }
        }
        panic!("the rendered time picker has nothing wired to that handler");
    }

    fn read_state(shared: &RefAny) -> TimePickerState {
        let mut s = shared.clone();
        let w = s
            .downcast_ref::<TimePickerStateWrapper>()
            .expect("the widget state changed type");
        w.inner
    }

    /// Renders a picker and hands back its flattened DOM plus the very shared
    /// state its own handlers were wired against — nothing is re-created by
    /// hand, so a mismatch between `dom()` and the handlers cannot hide here.
    fn laid_out(picker: TimePicker) -> (StyledDom, RefAny) {
        let styled = StyledDom::create_from_dom(picker.dom());
        let shared = shared_state(&styled);
        (styled, shared)
    }

    /// A hand-built shared state, for driving the handlers against values the
    /// public constructors clamp away.
    fn wrapper(inner: TimePickerState) -> RefAny {
        RefAny::new(TimePickerStateWrapper {
            inner,
            on_change: None.into(),
        })
    }

    /// One "mouse-up on `hit`" delivered to `handler`.
    fn press(
        styled_dom: StyledDom,
        payload: &RefAny,
        hit: DomNodeId,
        handler: extern "C" fn(RefAny, CallbackInfo) -> Update,
    ) -> (Update, Vec<CallbackChange>) {
        with_info(styled_dom, hit, |info| handler(payload.clone(), *info))
    }

    /// `times` presses of `handler` against `payload`, all delivered on `hit`.
    fn press_n(
        styled_dom: StyledDom,
        payload: &RefAny,
        hit: DomNodeId,
        handler: extern "C" fn(RefAny, CallbackInfo) -> Update,
        times: usize,
    ) -> (Update, Vec<CallbackChange>) {
        with_info(styled_dom, hit, |info| {
            let mut last = Update::DoNothing;
            for _ in 0..times {
                last = handler(payload.clone(), *info);
            }
            last
        })
    }

    /// The `(target, text)` of every text retext pushed onto the transaction log.
    fn pushed_texts(changes: &[CallbackChange]) -> Vec<(DomNodeId, String)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeText { node_id, text } => {
                    Some((*node_id, text.as_str().to_string()))
                }
                _ => None,
            })
            .collect()
    }

    /// The single retext a spinner/toggle press must push, asserting there is
    /// exactly one and that it is the *only* change of any kind.
    fn only_retext(changes: &[CallbackChange]) -> (DomNodeId, String) {
        let texts = pushed_texts(changes);
        assert_eq!(
            texts.len(),
            1,
            "expected exactly one retext, got {} change(s) total",
            changes.len(),
        );
        assert_eq!(
            changes.len(),
            1,
            "the press pushed {} change(s) beyond its retext",
            changes.len() - 1,
        );
        texts.into_iter().next().unwrap()
    }

    // ------------------------------------------------------------------
    // Style probes
    // ------------------------------------------------------------------

    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    // ------------------------------------------------------------------
    // User callbacks
    // ------------------------------------------------------------------

    /// A payload the change callback writes into. It arrives as the `data: RefAny`
    /// argument — a *shared* clone of what the test still holds — so the test can
    /// read back exactly what the widget reported, without any global state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ChangeLog {
        seen: Vec<TimePickerState>,
        payload: u32,
    }

    extern "C" fn record_change(
        mut data: RefAny,
        _info: CallbackInfo,
        state: TimePickerState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ChangeLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn change_do_nothing(
        _data: RefAny,
        _info: CallbackInfo,
        _state: TimePickerState,
    ) -> Update {
        Update::DoNothing
    }

    extern "C" fn change_refresh_all(
        _data: RefAny,
        _info: CallbackInfo,
        _state: TimePickerState,
    ) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in,
    /// which the `From<Callback>` arm *transmutes* into the 3-arg time-picker
    /// slot. Never called.
    extern "C" fn generic_shaped(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    fn log_refany() -> RefAny {
        RefAny::new(ChangeLog {
            seen: Vec::new(),
            payload: 0xDEAD_BEEF,
        })
    }

    fn read_log(probe: &RefAny) -> ChangeLog {
        let mut probe = probe.clone();
        let log = probe
            .downcast_ref::<ChangeLog>()
            .expect("the user payload changed type");
        log.clone()
    }

    // ==================================================================
    // TimePickerState::canonical_hour
    // ==================================================================

    #[test]
    fn canonical_hour_maps_every_twelve_hour_clock_face_to_its_canonical_hour() {
        // The whole point of the function: `12 AM -> 0` and `12 PM -> 12` are the
        // two cases a naive `hour + 12*pm` gets wrong (it would answer 12 and 24).
        for (hour, is_pm, want) in [
            (12u32, false, 0u32),
            (1, false, 1),
            (11, false, 11),
            (12, true, 12),
            (1, true, 13),
            (11, true, 23),
        ] {
            let s = TimePickerState {
                hour,
                minute: 0,
                is_pm,
                is_24h: false,
            };
            assert_eq!(
                s.canonical_hour(),
                want,
                "{hour} {} is not canonical hour {want}",
                if is_pm { "PM" } else { "AM" },
            );
        }
    }

    #[test]
    fn canonical_hour_round_trips_every_hour_of_the_day_through_the_clock_face() {
        // encode (canonical -> 12-hour face + AM/PM) then decode (canonical_hour)
        // must be the identity across the whole day, or a picker built from a
        // 24-hour timestamp would hand a different hour back to the host.
        for canonical in 0..24u32 {
            let face = if canonical % 12 == 0 { 12 } else { canonical % 12 };
            let s = TimePickerState {
                hour: face,
                minute: 0,
                is_pm: canonical >= 12,
                is_24h: false,
            };
            assert_eq!(
                s.canonical_hour(),
                canonical,
                "the 12-hour encoding of {canonical}:00 did not decode back",
            );
        }
    }

    #[test]
    fn canonical_hour_never_leaves_the_day_in_twelve_hour_mode_for_any_hour() {
        // `hour` is a public `u32` field: nothing stops a host from writing 0, 13
        // or u32::MAX into it. In 12-hour mode the `% 12` must keep the answer a
        // real hour whatever it is handed.
        for hour in [
            0u32,
            1,
            12,
            13,
            23,
            24,
            25,
            59,
            100,
            1_000_000,
            u32::MAX / 2,
            u32::MAX - 1,
            u32::MAX,
        ] {
            for is_pm in [false, true] {
                let s = TimePickerState {
                    hour,
                    minute: 0,
                    is_pm,
                    is_24h: false,
                };
                let c = s.canonical_hour();
                assert!(
                    c < 24,
                    "canonical_hour({hour}, pm={is_pm}) = {c} is not an hour of the day",
                );
            }
        }
    }

    #[test]
    fn canonical_hour_in_24h_mode_returns_the_hour_verbatim() {
        // FINDING (pinned, not weakened): the doc comment promises `0..=23`, but
        // the 24-hour arm is a bare field read with no clamp. Every constructor
        // and setter in this file clamps, so the promise holds for widget-managed
        // state — but a host that writes `state.inner.hour` directly gets its own
        // value straight back out, un-normalised.
        for hour in [0u32, 23, 24, 99, u32::MAX] {
            let s = TimePickerState {
                hour,
                minute: 0,
                is_pm: false,
                is_24h: true,
            };
            assert_eq!(
                s.canonical_hour(),
                hour,
                "the 24-hour arm normalised {hour}, which it has never done",
            );
        }
    }

    #[test]
    fn canonical_hour_ignores_the_pm_flag_in_24h_mode() {
        // A stale `is_pm` left over from a 12-hour session must not shift a
        // 24-hour reading by twelve hours.
        for hour in 0..24u32 {
            let am = TimePickerState {
                hour,
                minute: 0,
                is_pm: false,
                is_24h: true,
            };
            let pm = TimePickerState { is_pm: true, ..am };
            assert_eq!(
                am.canonical_hour(),
                pm.canonical_hour(),
                "the PM flag leaked into the 24-hour reading of hour {hour}",
            );
        }
    }

    #[test]
    fn canonical_hour_does_not_depend_on_the_minute() {
        for minute in [0u32, 30, 59, 60, u32::MAX] {
            for is_24h in [false, true] {
                let s = TimePickerState {
                    hour: 7,
                    minute,
                    is_pm: true,
                    is_24h,
                };
                let baseline = TimePickerState { minute: 0, ..s };
                assert_eq!(
                    s.canonical_hour(),
                    baseline.canonical_hour(),
                    "minute {minute} changed the hour reading (is_24h={is_24h})",
                );
            }
        }
    }

    #[test]
    fn canonical_hour_of_a_default_state_is_midnight() {
        assert_eq!(TimePickerState::default().canonical_hour(), 0);
        assert_eq!(TimePicker::default().state.inner.canonical_hour(), 0);
        assert_eq!(TimePickerStateWrapper::default().inner.canonical_hour(), 0);
    }

    #[test]
    fn canonical_hour_is_total_at_the_u32_extremes() {
        // Const-evaluated above too; this pins the *values* the extremes produce.
        assert_eq!(EXTREME_24H.canonical_hour(), u32::MAX);
        // 4294967295 % 12 == 3, so the 12-hour arm answers 3 PM.
        assert_eq!(EXTREME_12H.canonical_hour(), 15);
    }

    // ==================================================================
    // TimePickerState::hour_bounds
    // ==================================================================

    #[test]
    fn hour_bounds_are_the_two_documented_bands() {
        let mut s = TimePickerState {
            is_24h: true,
            ..Default::default()
        };
        assert_eq!(s.hour_bounds(), (0, 23), "the 24-hour band is wrong");
        s.is_24h = false;
        assert_eq!(s.hour_bounds(), (1, 12), "the 12-hour band is wrong");
    }

    #[test]
    fn hour_bounds_depend_on_nothing_but_the_mode() {
        // If the band ever started depending on the current hour, clamping would
        // become order-dependent and `set_24h` would stop being idempotent.
        for hour in [0u32, 1, 12, 13, 23, u32::MAX] {
            for minute in [0u32, 59, u32::MAX] {
                for is_pm in [false, true] {
                    for is_24h in [false, true] {
                        let s = TimePickerState {
                            hour,
                            minute,
                            is_pm,
                            is_24h,
                        };
                        let want = if is_24h { (0, 23) } else { (1, 12) };
                        assert_eq!(
                            s.hour_bounds(),
                            want,
                            "hour_bounds drifted for {hour}:{minute} pm={is_pm} 24h={is_24h}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn hour_bounds_are_ordered_and_fit_in_a_u32() {
        // The bounds are `i64` only so the clamp arithmetic cannot overflow; the
        // cast back to `u32` afterwards is unchecked, so both ends must be
        // non-negative and small.
        for is_24h in [false, true] {
            let s = TimePickerState {
                is_24h,
                ..TimePickerState::default()
            };
            let (lo, hi) = s.hour_bounds();
            assert!(lo <= hi, "hour_bounds({is_24h}) = ({lo}, {hi}) is inverted");
            assert!(lo >= 0, "a negative low bound would cast to a huge u32");
            assert!(hi <= i64::from(u32::MAX), "the high bound does not fit a u32");
            assert!(hi < 24, "the high bound is not an hour of the day");
        }
    }

    #[test]
    fn hour_bounds_agree_with_what_the_constructors_actually_clamp_to() {
        // The band and the clamp are written twice (create / set_24h); a
        // divergence would leave a widget displaying an out-of-band hour.
        let (lo24, hi24) = TimePickerState {
            is_24h: true,
            ..TimePickerState::default()
        }
        .hour_bounds();
        assert_eq!(u32::try_from(lo24).unwrap(), TimePicker::create(0, 0).state.inner.hour);
        assert_eq!(
            u32::try_from(hi24).unwrap(),
            TimePicker::create(u32::MAX, 0).state.inner.hour,
        );

        let (lo12, hi12) = TimePickerState {
            is_24h: false,
            ..TimePickerState::default()
        }
        .hour_bounds();
        assert_eq!(
            u32::try_from(lo12).unwrap(),
            TimePicker::create(0, 0).with_24h(false).state.inner.hour,
        );
        assert_eq!(
            u32::try_from(hi12).unwrap(),
            TimePicker::create(u32::MAX, 0).with_24h(false).state.inner.hour,
        );
    }

    // ==================================================================
    // TimePicker::create
    // ==================================================================

    #[test]
    fn create_clamps_rather_than_wraps_at_every_extreme() {
        // Saturation, not `% 24` / `% 60`: u32::MAX must land on the top of the
        // band, not on `u32::MAX % 24 == 3`.
        for (hour, minute, want_h, want_m) in [
            (0u32, 0u32, 0u32, 0u32),
            (23, 59, 23, 59),
            (24, 60, 23, 59),
            (25, 61, 23, 59),
            (99, 99, 23, 59),
            (u32::MAX - 1, u32::MAX - 1, 23, 59),
            (u32::MAX, u32::MAX, 23, 59),
        ] {
            let p = TimePicker::create(hour, minute);
            assert_eq!(
                (p.state.inner.hour, p.state.inner.minute),
                (want_h, want_m),
                "create({hour}, {minute}) was not clamped into range",
            );
        }
        assert_ne!(
            TimePicker::create(u32::MAX, u32::MAX).state.inner.hour,
            u32::MAX % 24,
            "create wrapped the hour instead of clamping it",
        );
        assert_ne!(
            TimePicker::create(u32::MAX, u32::MAX).state.inner.minute,
            u32::MAX % 60,
            "create wrapped the minute instead of clamping it",
        );
    }

    #[test]
    fn create_lands_inside_its_own_bounds_for_every_input_it_is_given() {
        for hour in [0u32, 1, 12, 23, 24, 1000, u32::MAX / 3, u32::MAX] {
            for minute in [0u32, 1, 30, 59, 60, 12345, u32::MAX] {
                let p = TimePicker::create(hour, minute);
                let (lo, hi) = p.state.inner.hour_bounds();
                let h = i64::from(p.state.inner.hour);
                assert!(
                    (lo..=hi).contains(&h),
                    "create({hour}, {minute}) left hour {h} outside {lo}..={hi}",
                );
                assert!(
                    p.state.inner.minute <= 59,
                    "create({hour}, {minute}) left minute {} outside 0..=59",
                    p.state.inner.minute,
                );
            }
        }
    }

    #[test]
    fn create_passes_every_in_range_value_through_untouched() {
        for hour in 0..24u32 {
            for minute in [0u32, 1, 7, 30, 58, 59] {
                let p = TimePicker::create(hour, minute);
                assert_eq!(
                    (p.state.inner.hour, p.state.inner.minute),
                    (hour, minute),
                    "create mangled the in-range value {hour}:{minute}",
                );
            }
        }
    }

    #[test]
    fn create_always_starts_in_24h_am_without_a_callback() {
        for (hour, minute) in [(0u32, 0u32), (13, 45), (u32::MAX, u32::MAX)] {
            let p = TimePicker::create(hour, minute);
            assert!(p.state.inner.is_24h, "create({hour}, {minute}) did not start in 24-hour mode");
            assert!(!p.state.inner.is_pm, "create({hour}, {minute}) started in PM");
            assert!(
                p.state.on_change.as_ref().is_none(),
                "create({hour}, {minute}) installed a callback nobody asked for",
            );
        }
    }

    #[test]
    fn create_zero_is_exactly_the_default_picker() {
        assert_eq!(TimePicker::create(0, 0), TimePicker::default());
        assert_eq!(TimePicker::create(0, 0).state.inner, TimePickerState::default());
        assert_eq!(TimePicker::default().state, TimePickerStateWrapper::default());
    }

    #[test]
    fn create_uses_the_shared_const_container_style() {
        // A per-instance style vec would allocate on every rebuild; the widget is
        // deliberately built from a `'static` slice.
        let p = TimePicker::create(9, 15);
        assert_eq!(
            properties(&p.container_style),
            CONTAINER_STYLE
                .iter()
                .map(|c| c.property.clone())
                .collect::<Vec<_>>(),
            "the container style is not the shared const declaration",
        );
    }

    #[test]
    fn create_is_deterministic() {
        for (h, m) in [(0u32, 0u32), (7, 8), (u32::MAX, u32::MAX)] {
            assert_eq!(TimePicker::create(h, m), TimePicker::create(h, m));
        }
    }

    // ==================================================================
    // TimePicker::set_24h / with_24h
    // ==================================================================

    #[test]
    fn set_24h_re_clamps_the_hour_into_the_new_band() {
        for (hour, want) in [(0u32, 1u32), (1, 1), (11, 11), (12, 12), (13, 12), (23, 12)] {
            let mut p = TimePicker::create(hour, 0);
            p.set_24h(false);
            assert_eq!(
                p.state.inner.hour, want,
                "switching {hour}:00 to 12-hour mode did not clamp to {want}",
            );
        }
    }

    #[test]
    fn set_24h_clamps_afternoon_hours_instead_of_converting_them() {
        // FINDING (pinned as documented behaviour): the doc says "re-clamping",
        // and that is exactly what happens — 13:00..23:00 all collapse onto 12,
        // and since `is_pm` is left alone the widget then reads back as 12 AM
        // (canonical hour 0), i.e. thirteen hours earlier. A host that toggles a
        // 24-hour picker into 12-hour mode must convert the hour itself.
        let mut p = TimePicker::create(13, 30);
        assert_eq!(p.state.inner.canonical_hour(), 13);
        p.set_24h(false);
        assert_eq!(p.state.inner.hour, 12, "13:00 was not clamped to 12");
        assert!(!p.state.inner.is_pm, "set_24h invented a PM flag");
        assert_eq!(
            p.state.inner.canonical_hour(),
            0,
            "the clamp is expected to read back as midnight, not as 13:00",
        );
    }

    #[test]
    fn set_24h_is_idempotent() {
        for start in [0u32, 5, 13, 23] {
            for target in [false, true] {
                let mut once = TimePicker::create(start, 30);
                once.set_24h(target);
                let mut twice = TimePicker::create(start, 30);
                twice.set_24h(target);
                twice.set_24h(target);
                assert_eq!(
                    once.state.inner, twice.state.inner,
                    "set_24h({target}) is not idempotent from {start}:30",
                );
            }
        }
    }

    #[test]
    fn set_24h_round_trip_is_the_identity_only_inside_the_narrow_band() {
        // 1..=12 survive a 24h -> 12h -> 24h round trip; 0 and 13..=23 do not,
        // because the intermediate 12-hour band cannot represent them.
        for hour in 0..24u32 {
            let mut p = TimePicker::create(hour, 0);
            p.set_24h(false);
            p.set_24h(true);
            if (1..=12).contains(&hour) {
                assert_eq!(p.state.inner.hour, hour, "the round trip lost hour {hour}");
            } else {
                let want = if hour == 0 { 1 } else { 12 };
                assert_eq!(
                    p.state.inner.hour, want,
                    "hour {hour} did not collapse onto {want} as the clamp dictates",
                );
            }
        }
    }

    #[test]
    fn set_24h_never_touches_the_minute_the_pm_flag_or_the_style() {
        for target in [false, true] {
            let before = TimePicker::create(9, 41).with_pm(true);
            let mut after = before.clone();
            after.set_24h(target);
            assert_eq!(after.state.inner.minute, before.state.inner.minute, "the minute moved");
            assert_eq!(after.state.inner.is_pm, before.state.inner.is_pm, "the PM flag moved");
            assert_eq!(
                properties(&after.container_style),
                properties(&before.container_style),
                "switching modes restyled the container",
            );
        }
    }

    #[test]
    fn set_24h_leaves_a_hand_written_out_of_range_hour_inside_the_band() {
        // `hour` is public, so the widget must survive a host writing garbage into
        // it — the `i64` clamp is what keeps the `as u32` cast from wrapping.
        for hour in [0u32, 24, 100, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            for target in [false, true] {
                let mut p = TimePicker::create(0, 0);
                p.state.inner.hour = hour;
                p.set_24h(target);
                let (lo, hi) = p.state.inner.hour_bounds();
                let h = i64::from(p.state.inner.hour);
                assert!(
                    (lo..=hi).contains(&h),
                    "set_24h({target}) left hand-written hour {hour} at {h}, outside {lo}..={hi}",
                );
            }
        }
    }

    #[test]
    fn with_24h_is_exactly_set_24h_in_builder_form() {
        for target in [false, true] {
            for hour in [0u32, 6, 13, 23] {
                let built = TimePicker::create(hour, 12).with_24h(target);
                let mut set = TimePicker::create(hour, 12);
                set.set_24h(target);
                assert_eq!(built, set, "with_24h({target}) diverged from set_24h at {hour}:12");
            }
        }
    }

    #[test]
    fn with_24h_keeps_the_post_construction_invariants() {
        for target in [false, true] {
            let p = TimePicker::create(u32::MAX, u32::MAX).with_24h(target);
            assert_eq!(p.state.inner.is_24h, target, "the mode flag was not stored");
            let (lo, hi) = p.state.inner.hour_bounds();
            assert!((lo..=hi).contains(&i64::from(p.state.inner.hour)));
            assert!(p.state.inner.minute <= 59);
        }
    }

    // ==================================================================
    // TimePicker::set_pm / with_pm
    // ==================================================================

    #[test]
    fn set_pm_moves_nothing_but_the_flag() {
        for target in [false, true] {
            for is_24h in [false, true] {
                let before = TimePicker::create(11, 22).with_24h(is_24h);
                let mut after = before.clone();
                after.set_pm(target);
                assert_eq!(after.state.inner.is_pm, target, "the PM flag was not stored");
                assert_eq!(after.state.inner.hour, before.state.inner.hour, "the hour moved");
                assert_eq!(after.state.inner.minute, before.state.inner.minute, "the minute moved");
                assert_eq!(after.state.inner.is_24h, before.state.inner.is_24h, "the mode moved");
            }
        }
    }

    #[test]
    fn set_pm_is_idempotent_and_two_flips_are_the_identity() {
        let mut p = TimePicker::create(4, 4).with_24h(false);
        p.set_pm(true);
        let once = p.state.inner;
        p.set_pm(true);
        assert_eq!(p.state.inner, once, "set_pm(true) is not idempotent");
        p.set_pm(false);
        p.set_pm(true);
        assert_eq!(p.state.inner, once, "two flips did not return to the same state");
    }

    #[test]
    fn set_pm_does_not_re_clamp_an_out_of_band_hour() {
        // `set_pm` is a `const fn` field write by design; it must not silently
        // start doing the clamping that only `set_24h` / `create` do.
        let mut p = TimePicker::create(0, 0);
        p.state.inner.hour = u32::MAX;
        p.set_pm(true);
        assert_eq!(p.state.inner.hour, u32::MAX, "set_pm re-clamped the hour");
    }

    #[test]
    fn with_pm_is_exactly_set_pm_in_builder_form() {
        for target in [false, true] {
            let built = TimePicker::create(3, 33).with_pm(target);
            let mut set = TimePicker::create(3, 33);
            set.set_pm(target);
            assert_eq!(built, set, "with_pm({target}) diverged from set_pm");
        }
    }

    #[test]
    fn pm_in_24h_mode_is_inert_for_the_canonical_reading() {
        // The docs call the flag "only meaningful in 12-hour mode"; a 24-hour
        // widget must therefore read the same with it set or clear.
        let am = TimePicker::create(15, 0);
        let pm = TimePicker::create(15, 0).with_pm(true);
        assert_eq!(
            am.state.inner.canonical_hour(),
            pm.state.inner.canonical_hour(),
            "the PM flag changed a 24-hour reading",
        );
        assert_eq!(am.state.inner.hour, pm.state.inner.hour);
    }

    // ==================================================================
    // TimePicker::set_on_change / with_on_change
    // ==================================================================

    #[test]
    fn set_on_change_stores_the_function_pointer_and_the_payload_verbatim() {
        let mut p = TimePicker::create(1, 1);
        p.set_on_change(
            RefAny::new(0xDEAD_BEEF_u32),
            change_do_nothing as TimePickerOnChangeCallbackType,
        );

        let c = p
            .state
            .on_change
            .as_ref()
            .expect("set_on_change did not store anything");
        assert_eq!(
            c.callback.cb as *const () as usize,
            change_do_nothing as *const () as usize,
            "the stored function pointer is not the one that was handed in",
        );
        assert!(
            matches!(c.callback.ctx, OptionRefAny::None),
            "a native Rust callback must not carry an FFI context",
        );
        let mut payload = c.refany.clone();
        assert_eq!(
            *payload.downcast_ref::<u32>().expect("the payload changed type"),
            0xDEAD_BEEF,
        );
    }

    #[test]
    fn set_on_change_replaces_rather_than_accumulates() {
        let mut p = TimePicker::create(1, 1);
        p.set_on_change(RefAny::new(1u8), change_do_nothing as TimePickerOnChangeCallbackType);
        p.set_on_change(RefAny::new(2u8), change_refresh_all as TimePickerOnChangeCallbackType);

        let c = p.state.on_change.as_ref().expect("the callback vanished");
        assert_eq!(
            c.callback.cb as *const () as usize,
            change_refresh_all as *const () as usize,
            "the second set_on_change did not win",
        );
        let mut payload = c.refany.clone();
        assert_eq!(*payload.downcast_ref::<u8>().expect("wrong payload type"), 2);
    }

    #[test]
    fn set_on_change_does_not_disturb_the_time_or_the_container_style() {
        let before = TimePicker::create(23, 59);
        let mut after = TimePicker::create(23, 59);
        after.set_on_change(RefAny::new(0u8), change_do_nothing as TimePickerOnChangeCallbackType);

        assert_eq!(after.state.inner, before.state.inner, "installing a callback moved the time");
        assert_eq!(
            properties(&after.container_style),
            properties(&before.container_style),
            "installing a callback restyled the container",
        );
    }

    #[test]
    fn with_on_change_is_exactly_set_on_change_in_builder_form() {
        let built = TimePicker::create(5, 9)
            .with_on_change(RefAny::new(7u32), change_do_nothing as TimePickerOnChangeCallbackType);
        let mut set = TimePicker::create(5, 9);
        set.set_on_change(RefAny::new(7u32), change_do_nothing as TimePickerOnChangeCallbackType);

        assert_eq!(built.state.inner, set.state.inner);
        let a = built.state.on_change.as_ref().expect("builder dropped the callback");
        let b = set.state.on_change.as_ref().expect("setter dropped the callback");
        assert_eq!(a.callback.cb as *const () as usize, b.callback.cb as *const () as usize);

        let (mut pa, mut pb) = (a.refany.clone(), b.refany.clone());
        assert_eq!(
            *pa.downcast_ref::<u32>().expect("builder payload changed type"),
            *pb.downcast_ref::<u32>().expect("setter payload changed type"),
        );
    }

    #[test]
    fn with_on_change_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm *transmutes* a 2-arg fn pointer into the 3-arg
        // time-picker slot — this is the FFI (Python/C) path. The pointer must
        // come out bit-identical; a mangled one would be a wild jump on the first
        // arrow click.
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::None,
        };
        let expected = generic_shaped as *const () as usize;

        let p = TimePicker::create(1, 1).with_on_change(RefAny::new(0u8), generic);
        let c = p.state.on_change.as_ref().expect("the generic callback was dropped");
        assert_eq!(
            c.callback.cb as *const () as usize,
            expected,
            "the Callback -> TimePickerOnChangeCallback transmute mangled the pointer",
        );
    }

    #[test]
    fn installing_a_callback_survives_every_other_builder_step() {
        // Order-independence: the builders are documented as composable, so a
        // callback must not be dropped by a later `with_24h` / `with_pm`.
        let p = TimePicker::create(9, 30)
            .with_on_change(RefAny::new(1u8), change_do_nothing as TimePickerOnChangeCallbackType)
            .with_24h(false)
            .with_pm(true);
        assert!(p.state.on_change.as_ref().is_some(), "a later builder dropped the callback");
        assert_eq!(p.state.inner.hour, 9);
        assert!(p.state.inner.is_pm);
        assert!(!p.state.inner.is_24h);
    }

    // ==================================================================
    // TimePicker::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_hands_out_the_original_and_leaves_a_default_behind() {
        let mut p = TimePicker::create(21, 45).with_24h(false).with_pm(true);
        let inner_before = p.state.inner;
        let taken = p.swap_with_default();

        assert_eq!(taken.state.inner, inner_before, "the original state was not handed out");
        assert_eq!(p, TimePicker::default(), "the picker left behind is not a default one");
    }

    #[test]
    fn swap_with_default_carries_the_callback_out_with_the_original() {
        // If the callback stayed behind on the *default* picker, the host would
        // keep getting change notifications from a widget it thought it had taken.
        let mut p = TimePicker::create(6, 6)
            .with_on_change(RefAny::new(3u8), change_do_nothing as TimePickerOnChangeCallbackType);
        let taken = p.swap_with_default();

        assert!(
            taken.state.on_change.as_ref().is_some(),
            "the callback did not leave with the original",
        );
        assert!(
            p.state.on_change.as_ref().is_none(),
            "the callback stayed behind on the default",
        );
    }

    #[test]
    fn swapping_a_default_repeatedly_is_idempotent() {
        let mut p = TimePicker::default();
        let first = p.swap_with_default();
        let second = p.swap_with_default();
        assert_eq!(first, TimePicker::default());
        assert_eq!(second, TimePicker::default());
        assert_eq!(p, TimePicker::default());
    }

    #[test]
    fn swap_with_default_survives_a_hand_written_out_of_range_state() {
        let mut p = TimePicker::create(0, 0);
        p.state.inner.hour = u32::MAX;
        p.state.inner.minute = u32::MAX;
        let taken = p.swap_with_default();

        assert_eq!(taken.state.inner.hour, u32::MAX, "swap normalised what it took out");
        assert_eq!(p.state.inner, TimePickerState::default(), "the replacement is not a default");
    }

    // ==================================================================
    // TimePicker::dom
    // ==================================================================

    #[test]
    fn dom_renders_three_columns_in_24h_mode_and_four_in_12h() {
        let h24 = TimePicker::create(9, 5).dom();
        assert!(matches!(h24.root.get_node_type(), NodeType::Div));
        assert_eq!(classes(&h24), vec![CLASS_CONTAINER.to_string()]);
        assert_eq!(h24.children.as_ref().len(), 3, "24-hour mode rendered an AM/PM toggle");

        let h12 = TimePicker::create(9, 5).with_24h(false).dom();
        assert_eq!(h12.children.as_ref().len(), 4, "12-hour mode did not render an AM/PM toggle");
        assert_eq!(classes(&h12.children.as_ref()[3]), vec![CLASS_AMPM.to_string()]);
    }

    #[test]
    fn dom_columns_carry_the_documented_classes_and_glyphs() {
        let dom = TimePicker::create(9, 5).dom();
        let (hour, sep, minute) = columns(&dom);

        assert_eq!(classes(sep), vec![CLASS_SEPARATOR.to_string()]);
        assert_eq!(text_of(sep).as_deref(), Some(":"), "the separator is not a colon");

        for (which, col) in [("hour", hour), ("minute", minute)] {
            assert_eq!(classes(col), vec![CLASS_SPINNER.to_string()], "{which}: wrong class");
            let cells = col.children.as_ref();
            assert_eq!(cells.len(), 3, "{which}: a spinner is up-arrow / value / down-arrow");
            assert_eq!(classes(&cells[0]), vec![CLASS_ARROW.to_string()]);
            assert_eq!(classes(&cells[1]), vec![CLASS_DISPLAY.to_string()]);
            assert_eq!(classes(&cells[2]), vec![CLASS_ARROW.to_string()]);
            assert_eq!(text_of(&cells[0]).as_deref(), Some(UP_GLYPH), "{which}: up arrow");
            assert_eq!(text_of(&cells[2]).as_deref(), Some(DOWN_GLYPH), "{which}: down arrow");
        }
    }

    #[test]
    fn the_class_and_glyph_constants_are_the_ones_the_widget_declares() {
        // The strings above are hard-coded so a rename shows up as a test failure
        // rather than silently breaking every user stylesheet.
        let names = |c: &[IdOrClass]| -> Vec<String> {
            c.iter()
                .filter_map(|c| match c {
                    Class(s) => Some(s.as_str().to_string()),
                    IdOrClass::Id(_) => None,
                })
                .collect()
        };
        assert_eq!(names(TIME_PICKER_CLASS), vec![CLASS_CONTAINER.to_string()]);
        assert_eq!(names(SPINNER_CLASS), vec![CLASS_SPINNER.to_string()]);
        assert_eq!(names(DISPLAY_CLASS), vec![CLASS_DISPLAY.to_string()]);
        assert_eq!(names(ARROW_CLASS), vec![CLASS_ARROW.to_string()]);
        assert_eq!(names(SEPARATOR_CLASS), vec![CLASS_SEPARATOR.to_string()]);
        assert_eq!(names(AMPM_CLASS), vec![CLASS_AMPM.to_string()]);
        assert_eq!(UP_ARROW.as_str(), UP_GLYPH);
        assert_eq!(DOWN_ARROW.as_str(), DOWN_GLYPH);
        assert_eq!(SEPARATOR_TEXT.as_str(), ":");
    }

    #[test]
    fn dom_zero_pads_the_minute_but_not_the_hour() {
        // FINDING (pinned): the two displays disagree — `{}` for the hour and
        // `{:02}` for the minute — so 09:05 renders as "9:05". Deliberate or not,
        // the arrow handlers re-text with the same asymmetric formats, so at least
        // the widget is self-consistent.
        assert_eq!(
            displayed(&TimePicker::create(9, 5).dom()),
            ("9".to_string(), "05".to_string()),
        );
        assert_eq!(
            displayed(&TimePicker::create(0, 0).dom()),
            ("0".to_string(), "00".to_string()),
        );
        assert_eq!(
            displayed(&TimePicker::create(23, 59).dom()),
            ("23".to_string(), "59".to_string()),
        );
    }

    #[test]
    fn dom_text_round_trips_the_whole_state_for_every_hour_and_minute() {
        // encode (state -> displayed text) == decode (parse the text back): the
        // rendered digits must be exactly the stored value, never a rounded or
        // re-derived one.
        for hour in 0..24u32 {
            for minute in [0u32, 1, 9, 10, 30, 58, 59] {
                let (h, m) = displayed(&TimePicker::create(hour, minute).dom());
                assert_eq!(
                    h.parse::<u32>().expect("the hour display is not a number"),
                    hour,
                    "the hour display drifted at {hour}:{minute}",
                );
                assert_eq!(
                    m.parse::<u32>().expect("the minute display is not a number"),
                    minute,
                    "the minute display drifted at {hour}:{minute}",
                );
                assert_eq!(m.chars().count(), 2, "the minute is not zero-padded at {hour}:{minute}");
            }
        }
    }

    #[test]
    fn dom_renders_a_hand_written_out_of_range_state_without_panicking() {
        // `dom()` does not clamp — it formats whatever the state holds. The point
        // is that it stays total: no panic, no truncation, no wrap.
        let mut p = TimePicker::create(0, 0);
        p.state.inner.hour = u32::MAX;
        p.state.inner.minute = u32::MAX;
        let (h, m) = displayed(&p.dom());
        assert_eq!(h, u32::MAX.to_string(), "the hour display truncated a huge value");
        assert_eq!(m, u32::MAX.to_string(), "the minute display truncated a huge value");
    }

    #[test]
    fn dom_ampm_label_tracks_the_pm_flag_and_only_exists_in_12h_mode() {
        assert_eq!(
            ampm_label(&TimePicker::create(6, 0).with_24h(false).dom()).as_deref(),
            Some("AM"),
        );
        assert_eq!(
            ampm_label(&TimePicker::create(6, 0).with_24h(false).with_pm(true).dom()).as_deref(),
            Some("PM"),
        );
        // The flag is still set here, but 24-hour mode must not render a toggle.
        assert_eq!(ampm_label(&TimePicker::create(6, 0).with_pm(true).dom()), None);
    }

    #[test]
    fn dom_wires_each_of_the_five_handlers_exactly_once() {
        let styled = StyledDom::create_from_dom(TimePicker::create(8, 8).with_24h(false).dom());
        for (name, handler) in [
            ("on_hour_up", on_hour_up as usize),
            ("on_hour_down", on_hour_down as usize),
            ("on_minute_up", on_minute_up as usize),
            ("on_minute_down", on_minute_down as usize),
            ("on_ampm_toggle", on_ampm_toggle as usize),
        ] {
            let count = styled
                .node_data
                .as_ref()
                .iter()
                .flat_map(|nd| nd.callbacks.as_ref().iter())
                .filter(|cb| cb.callback.cb == handler)
                .count();
            assert_eq!(count, 1, "{name} is wired to {count} node(s), not exactly one");
        }
    }

    #[test]
    fn dom_registers_every_handler_on_mouse_up_and_makes_the_cell_focusable() {
        let styled = StyledDom::create_from_dom(TimePicker::create(8, 8).with_24h(false).dom());
        let mut interactive = 0;
        for nd in styled.node_data.as_ref() {
            for cb in nd.callbacks.as_ref() {
                assert_eq!(
                    cb.event,
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    "a time-picker cell fires on something other than mouse-up",
                );
                assert!(
                    matches!(cb.callback.ctx, OptionRefAny::None),
                    "a native handler carries an FFI context",
                );
                interactive += 1;
            }
            if !nd.callbacks.as_ref().is_empty() {
                assert_eq!(
                    nd.flags.get_tab_index(),
                    Some(TabIndex::Auto),
                    "a clickable time-picker cell is not keyboard-focusable",
                );
            }
        }
        assert_eq!(interactive, 5, "12-hour mode must expose 4 arrows + 1 AM/PM toggle");
    }

    #[test]
    fn dom_leaves_the_displays_and_the_separator_inert() {
        // Only the arrows and the toggle are clickable; a handler on a display
        // would fire on a click meant to select the text.
        let styled = StyledDom::create_from_dom(TimePicker::create(8, 8).with_24h(false).dom());
        for idx in [
            N_CONTAINER,
            N_HOUR_SPINNER,
            N_HOUR_DISPLAY,
            N_SEPARATOR,
            N_MINUTE_SPINNER,
            N_MINUTE_DISPLAY,
        ] {
            assert!(
                styled.node_data.as_ref()[idx].callbacks.as_ref().is_empty(),
                "flattened node {idx} registered a click handler it should not have",
            );
        }
    }

    #[test]
    fn dom_shares_one_state_refany_across_every_handler() {
        // Four arrows and the toggle must all mutate the *same* state; a per-cell
        // copy would let the hour and the minute drift apart.
        let styled = StyledDom::create_from_dom(TimePicker::create(5, 5).with_24h(false).dom());
        let payloads = state_payloads(&styled);
        assert_eq!(payloads.len(), 5, "not every handler carries the widget state");

        {
            let mut first = payloads[0].clone();
            let mut w = first
                .downcast_mut::<TimePickerStateWrapper>()
                .expect("the state changed type");
            w.inner.minute = 42;
        }
        for (i, p) in payloads.iter().enumerate() {
            assert_eq!(
                read_state(p).minute,
                42,
                "handler payload #{i} is a private copy of the state",
            );
        }
    }

    #[test]
    fn dom_keeps_its_cached_child_count_in_sync_with_the_tree() {
        // `estimated_total_children` is a cache; if it under-reports, the flatten
        // under-allocates its arenas and panics on an out-of-bounds write.
        for is_24h in [false, true] {
            for (h, m) in [(0u32, 0u32), (23, 59), (u32::MAX, u32::MAX)] {
                let dom = TimePicker::create(h, m).with_24h(is_24h).dom();
                let expected_nodes = if is_24h { N_NODES_24H } else { N_NODES_12H };
                assert_eq!(
                    dom.estimated_total_children,
                    descendants(&dom),
                    "{h}:{m} 24h={is_24h}: the cached descendant count is wrong",
                );
                let styled = StyledDom::create_from_dom(dom);
                assert_eq!(
                    styled.node_data.as_ref().len(),
                    expected_nodes,
                    "{h}:{m} 24h={is_24h}: the widget did not flatten to {expected_nodes} nodes",
                );
            }
        }
    }

    #[test]
    fn flattened_layout_is_the_fixed_seventeen_or_nineteen_nodes() {
        // Pins the pre-order indices the click tests below index by name.
        let styled = StyledDom::create_from_dom(TimePicker::create(7, 8).with_24h(false).dom());
        for (idx, class, text) in [
            (N_CONTAINER, CLASS_CONTAINER, None),
            (N_HOUR_SPINNER, CLASS_SPINNER, None),
            (N_HOUR_UP, CLASS_ARROW, Some(UP_GLYPH)),
            (N_HOUR_DISPLAY, CLASS_DISPLAY, Some("7")),
            (N_HOUR_DOWN, CLASS_ARROW, Some(DOWN_GLYPH)),
            (N_SEPARATOR, CLASS_SEPARATOR, Some(":")),
            (N_MINUTE_SPINNER, CLASS_SPINNER, None),
            (N_MINUTE_UP, CLASS_ARROW, Some(UP_GLYPH)),
            (N_MINUTE_DISPLAY, CLASS_DISPLAY, Some("08")),
            (N_MINUTE_DOWN, CLASS_ARROW, Some(DOWN_GLYPH)),
            (N_AMPM, CLASS_AMPM, Some("AM")),
        ] {
            assert_eq!(
                flat_classes(&styled, idx),
                vec![class.to_string()],
                "flattened node {idx} is not the {class} node",
            );
            assert_eq!(
                flat_text(&styled, idx).as_deref(),
                text,
                "flattened node {idx} renders the wrong text",
            );
        }
        assert_eq!(
            wired_to(&styled, on_hour_up as usize).0,
            node(N_HOUR_UP),
            "the hour up arrow is not where the indices say it is",
        );
        assert_eq!(wired_to(&styled, on_hour_down as usize).0, node(N_HOUR_DOWN));
        assert_eq!(wired_to(&styled, on_minute_up as usize).0, node(N_MINUTE_UP));
        assert_eq!(wired_to(&styled, on_minute_down as usize).0, node(N_MINUTE_DOWN));
        assert_eq!(wired_to(&styled, on_ampm_toggle as usize).0, node(N_AMPM));
    }

    #[test]
    fn from_timepicker_for_dom_is_the_dom_method() {
        let via_from: Dom = TimePicker::create(4, 20).into();
        let via_dom = TimePicker::create(4, 20).dom();
        assert_eq!(classes(&via_from), classes(&via_dom));
        assert_eq!(displayed(&via_from), displayed(&via_dom));
        assert_eq!(via_from.children.as_ref().len(), via_dom.children.as_ref().len());
        assert_eq!(via_from.estimated_total_children, via_dom.estimated_total_children);
    }

    // ==================================================================
    // build_spinner
    // ==================================================================

    #[test]
    fn build_spinner_stores_the_handler_pointers_verbatim_at_the_usize_extremes() {
        // The handlers are erased to `usize` before they reach the DOM, so nothing
        // type-checks them any more. Every value — including 0 and usize::MAX,
        // which are not valid code addresses — must come back out unchanged
        // rather than being validated, folded or truncated.
        for (up, down) in [
            (0usize, 0usize),
            (0, usize::MAX),
            (usize::MAX, 0),
            (usize::MAX, usize::MAX),
            (1, 2),
            (usize::MAX / 2, usize::MAX - 1),
        ] {
            let dom = build_spinner(AzString::from_const_str("0"), RefAny::new(0u8), up, down);
            let cells = dom.children.as_ref();
            assert_eq!(cells.len(), 3);
            assert_eq!(
                cells[0].root.callbacks.as_ref()[0].callback.cb, up,
                "the up handler {up} was mangled",
            );
            assert_eq!(
                cells[2].root.callbacks.as_ref()[0].callback.cb, down,
                "the down handler {down} was mangled",
            );
        }
    }

    #[test]
    fn build_spinner_puts_the_value_between_the_two_arrows() {
        let dom = build_spinner(AzString::from_const_str("42"), RefAny::new(0u8), 1, 2);
        assert_eq!(classes(&dom), vec![CLASS_SPINNER.to_string()]);
        let cells = dom.children.as_ref();
        assert_eq!(text_of(&cells[0]).as_deref(), Some(UP_GLYPH));
        assert_eq!(text_of(&cells[1]).as_deref(), Some("42"));
        assert_eq!(text_of(&cells[2]).as_deref(), Some(DOWN_GLYPH));
        assert!(
            cells[1].root.callbacks.as_ref().is_empty(),
            "the value display must not be clickable",
        );
    }

    #[test]
    fn build_spinner_preserves_every_kind_of_value_string_byte_for_byte() {
        // The value is host-supplied text in the general case, so it must survive
        // empty, NUL-bearing, astral-plane, combining, RTL and very long inputs
        // without being trimmed, escaped or re-encoded.
        let long = "9".repeat(4096);
        let values: Vec<String> = vec![
            String::new(),
            " ".to_string(),
            "\0".to_string(),
            "\n\t".to_string(),
            "٣٠".to_string(),                 // arabic-indic digits
            "\u{1F55B}".to_string(),          // 🕛
            "e\u{0301}".to_string(),          // combining acute
            "\u{202E}12".to_string(),         // RTL override
            "-1".to_string(),
            "٩٩:٩٩".to_string(),
            long,
        ];
        for v in values {
            let dom = build_spinner(AzString::from(v.clone()), RefAny::new(0u8), 1, 2);
            let shown = text_of(&dom.children.as_ref()[1]);
            assert_eq!(
                shown.as_deref(),
                Some(v.as_str()),
                "the spinner value was not preserved verbatim ({} bytes)",
                v.len(),
            );
        }
    }

    #[test]
    fn build_spinner_shares_the_state_between_both_arrows() {
        let state = RefAny::new(TimePickerStateWrapper::default());
        let dom = build_spinner(AzString::from_const_str("0"), state.clone(), 1, 2);
        let cells = dom.children.as_ref();

        {
            let mut up_payload = cells[0].root.callbacks.as_ref()[0].refany.clone();
            let mut w = up_payload
                .downcast_mut::<TimePickerStateWrapper>()
                .expect("the up arrow does not carry the state");
            w.inner.hour = 17;
        }
        let mut down_payload = cells[2].root.callbacks.as_ref()[0].refany.clone();
        let seen = down_payload
            .downcast_ref::<TimePickerStateWrapper>()
            .expect("the down arrow does not carry the state")
            .inner
            .hour;
        assert_eq!(seen, 17, "the two arrows of one spinner hold separate states");
        assert_eq!(read_state(&state).hour, 17, "the caller's own handle was not shared");
    }

    #[test]
    fn build_spinner_makes_both_arrows_focusable_click_targets() {
        let dom = build_spinner(AzString::from_const_str("0"), RefAny::new(0u8), 1, 2);
        for (which, cell) in [("up", 0usize), ("down", 2usize)] {
            let cell = &dom.children.as_ref()[cell];
            let cbs = cell.root.callbacks.as_ref();
            assert_eq!(cbs.len(), 1, "{which}: an arrow registers exactly one handler");
            assert_eq!(cbs[0].event, EventFilter::Hover(HoverEventFilter::MouseUp));
            assert_eq!(
                cell.root.flags.get_tab_index(),
                Some(TabIndex::Auto),
                "{which}: the arrow is not keyboard-focusable",
            );
            assert_eq!(classes(cell), vec![CLASS_ARROW.to_string()]);
        }
    }

    #[test]
    fn build_spinner_reports_its_three_children() {
        for value in ["", "0", "999999"] {
            let dom = build_spinner(AzString::from(value.to_string()), RefAny::new(0u8), 1, 2);
            assert_eq!(dom.estimated_total_children, descendants(&dom));
            // Three cells (▲ / value / ▼), each a styled `<p>` wrapping its
            // bare text leaf per the label convention: 6 descendants.
            assert_eq!(dom.estimated_total_children, 6);
        }
    }

    // ==================================================================
    // adjust_spinner + the four arrow handlers
    // ==================================================================

    #[test]
    fn hour_arrows_move_the_hour_by_one_and_retext_the_hour_display() {
        let (styled, shared) = laid_out(TimePicker::create(9, 30));
        let (hit, payload) = wired_to(&styled, on_hour_up as usize);

        let (update, changes) = press(styled, &payload, hit, on_hour_up);

        assert_eq!(read_state(&shared).hour, 10, "the up arrow did not increment the hour");
        assert_eq!(read_state(&shared).minute, 30, "the hour arrow moved the minute");
        assert_eq!(update, Update::DoNothing, "no callback is installed, so nothing to report");
        assert_eq!(
            only_retext(&changes),
            (node(text_leaf(N_HOUR_DISPLAY)), "10".to_string()),
            "the up arrow retexted the wrong node (or with the wrong text)",
        );
    }

    #[test]
    fn minute_arrows_move_the_minute_and_retext_it_zero_padded() {
        let (styled, shared) = laid_out(TimePicker::create(9, 30));
        let (hit, payload) = wired_to(&styled, on_minute_down as usize);

        let (_, changes) = press(styled, &payload, hit, on_minute_down);

        assert_eq!(read_state(&shared).minute, 29, "the down arrow did not decrement the minute");
        assert_eq!(read_state(&shared).hour, 9, "the minute arrow moved the hour");
        assert_eq!(
            only_retext(&changes),
            (node(text_leaf(N_MINUTE_DISPLAY)), "29".to_string())
        );

        // ... and single digits keep the two-digit form the initial render used.
        let (styled, _) = laid_out(TimePicker::create(9, 10));
        let (hit, payload) = wired_to(&styled, on_minute_down as usize);
        let (_, changes) = press(styled, &payload, hit, on_minute_down);
        assert_eq!(
            only_retext(&changes).1,
            "09",
            "the retext dropped the zero padding the first render used",
        );
    }

    #[test]
    fn the_hour_clamps_at_both_ends_of_the_24_hour_band() {
        for (start, handler, want) in [
            (23u32, on_hour_up as extern "C" fn(RefAny, CallbackInfo) -> Update, 23u32),
            (0, on_hour_down as extern "C" fn(RefAny, CallbackInfo) -> Update, 0),
        ] {
            let (styled, shared) = laid_out(TimePicker::create(start, 0));
            let handler_addr = handler as usize;
            let (hit, payload) = wired_to(&styled, handler_addr);

            let (_, changes) = press(styled, &payload, hit, handler);

            assert_eq!(read_state(&shared).hour, want, "the hour escaped the band from {start}");
            assert_eq!(
                only_retext(&changes).1,
                want.to_string(),
                "the clamped hour was retexted with something else",
            );
        }
    }

    #[test]
    fn the_hour_clamps_to_the_narrow_band_in_12_hour_mode() {
        for (start, handler, want) in [
            (12u32, on_hour_up as extern "C" fn(RefAny, CallbackInfo) -> Update, 12u32),
            (1, on_hour_down as extern "C" fn(RefAny, CallbackInfo) -> Update, 1),
            (11, on_hour_up as extern "C" fn(RefAny, CallbackInfo) -> Update, 12),
            (2, on_hour_down as extern "C" fn(RefAny, CallbackInfo) -> Update, 1),
        ] {
            let (styled, shared) = laid_out(TimePicker::create(start, 0).with_24h(false));
            let (hit, payload) = wired_to(&styled, handler as usize);

            let (_, _) = press(styled, &payload, hit, handler);

            assert_eq!(
                read_state(&shared).hour,
                want,
                "12-hour mode: pressing from {start} did not land on {want}",
            );
        }
    }

    #[test]
    fn the_minute_clamps_and_never_carries_into_the_hour() {
        // The module's documented PARTIAL: 59 + 1 stays 59 and 0 - 1 stays 0 —
        // the hour must not move either way.
        for (start, handler, want) in [
            (59u32, on_minute_up as extern "C" fn(RefAny, CallbackInfo) -> Update, 59u32),
            (0, on_minute_down as extern "C" fn(RefAny, CallbackInfo) -> Update, 0),
        ] {
            let (styled, shared) = laid_out(TimePicker::create(12, start));
            let (hit, payload) = wired_to(&styled, handler as usize);

            let (_, changes) = press(styled, &payload, hit, handler);

            assert_eq!(read_state(&shared).minute, want, "the minute escaped 0..=59 from {start}");
            assert_eq!(read_state(&shared).hour, 12, "the minute carried into the hour");
            assert_eq!(
                only_retext(&changes),
                (node(N_MINUTE_TEXT), format!("{want:02}")),
                "the clamped minute was retexted wrongly",
            );
        }
    }

    #[test]
    fn holding_an_arrow_walks_the_whole_band_and_then_stops() {
        // 200 presses is far more than the band is wide: the value must saturate
        // at the edge rather than wrapping around or running away.
        let (styled, shared) = laid_out(TimePicker::create(0, 0));
        let (hit, payload) = wired_to(&styled, on_hour_up as usize);
        press_n(styled, &payload, hit, on_hour_up, 200);
        assert_eq!(read_state(&shared).hour, 23, "200 up-presses did not saturate at 23");

        let (styled, shared) = laid_out(TimePicker::create(23, 59));
        let (hit, payload) = wired_to(&styled, on_minute_down as usize);
        press_n(styled, &payload, hit, on_minute_down, 200);
        assert_eq!(read_state(&shared).minute, 0, "200 down-presses did not saturate at 0");
        assert_eq!(read_state(&shared).hour, 23, "the saturating minute borrowed from the hour");
    }

    #[test]
    fn every_step_of_a_full_sweep_is_reported_exactly_once() {
        // Walking 0 -> 23 must push 23 distinct retexts in order and land on 23;
        // a duplicated or skipped step would desynchronise the display from the
        // state, which is the whole failure mode `change_node_text` exists to avoid.
        let (styled, shared) = laid_out(TimePicker::create(0, 0));
        let (hit, payload) = wired_to(&styled, on_hour_up as usize);
        let (_, changes) = press_n(styled, &payload, hit, on_hour_up, 23);

        let texts: Vec<String> = pushed_texts(&changes).into_iter().map(|(_, t)| t).collect();
        assert_eq!(
            texts,
            (1..=23u32).map(|h| h.to_string()).collect::<Vec<_>>(),
            "the sweep did not report every hour exactly once, in order",
        );
        assert_eq!(read_state(&shared).hour, 23);
    }

    #[test]
    fn a_press_on_a_node_without_a_parent_changes_nothing_at_all() {
        // The container has no parent, so the display lookup fails. The state must
        // be left alone — the handler bails out *before* it touches the value.
        let (styled, shared) = laid_out(TimePicker::create(9, 30));
        let (_, payload) = wired_to(&styled, on_hour_up as usize);

        let (update, changes) = press(styled, &payload, node(N_CONTAINER), on_hour_up);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a parentless hit still retexted something");
        assert_eq!(
            read_state(&shared),
            TimePicker::create(9, 30).state.inner,
            "a hit that could not be resolved mutated the state anyway",
        );
    }

    #[test]
    fn a_press_on_a_detached_or_out_of_range_node_changes_nothing_at_all() {
        for (name, hit) in [
            ("no node at all", node_none()),
            ("one past the end", node(N_NODES_24H)),
            ("far out of range", node(usize::MAX / 2)),
        ] {
            let (styled, shared) = laid_out(TimePicker::create(9, 30));
            let (_, payload) = wired_to(&styled, on_minute_up as usize);

            let (update, changes) = press(styled, &payload, hit, on_minute_up);

            assert_eq!(update, Update::DoNothing, "{name}: unexpected verdict");
            assert!(changes.is_empty(), "{name}: a bad hit still retexted something");
            assert_eq!(
                read_state(&shared).minute,
                30,
                "{name}: a bad hit mutated the state",
            );
        }
    }

    #[test]
    fn the_display_lookup_is_positional_and_trusts_whatever_node_it_lands_on() {
        // The walk is `hit -> parent -> first child -> next sibling` with no check
        // that the result is a display cell. Delivered on the separator (whose
        // parent is the *container*), it resolves to the separator itself and
        // overwrites the ":" with the hour. Unreachable through `dom()` — only the
        // arrows are wired — but pinned so the lack of validation is visible, and
        // so a future rewiring of the separator shows up here.
        let (styled, shared) = laid_out(TimePicker::create(9, 30));
        let (_, payload) = wired_to(&styled, on_hour_up as usize);

        let (update, changes) = press(styled, &payload, node(N_SEPARATOR), on_hour_up);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(read_state(&shared).hour, 10, "the edit itself did not happen");
        assert_eq!(read_state(&shared).minute, 30, "an hour press moved the minute");
        assert_eq!(
            only_retext(&changes),
            (node(text_leaf(N_SEPARATOR)), "10".to_string()),
            "the positional lookup landed somewhere other than the separator",
        );
    }

    #[test]
    fn a_press_with_a_foreign_payload_is_declined_without_touching_the_dom() {
        // The `RefAny` is type-erased: a host that wires the wrong payload onto an
        // arrow must get a clean no-op, not a wild reinterpretation of the bytes.
        for payload in [
            RefAny::new(0u8),
            RefAny::new(TimePickerState::default()),
            RefAny::new(String::from("not a time picker")),
        ] {
            let (styled, shared) = laid_out(TimePicker::create(9, 30));
            let (hit, _) = wired_to(&styled, on_hour_up as usize);

            let (update, changes) = press(styled, &payload, hit, on_hour_up);

            assert_eq!(update, Update::DoNothing, "a foreign payload was accepted");
            assert!(changes.is_empty(), "a foreign payload still retexted the display");
            assert_eq!(read_state(&shared).hour, 9, "a foreign payload moved the real state");
        }
    }

    #[test]
    fn adjust_spinner_saturates_at_the_extreme_deltas_it_can_be_handed() {
        // `delta` is an `i64` added to `i64::from(hour)`. From hour 0 the whole
        // `i64` range is representable, so both extremes must land on the ends of
        // the band rather than wrapping.
        //
        // NOTE: `i64::from(hour) + delta` is a plain `+`. It cannot overflow for
        // any delta the widget itself passes (±1), but a delta near `i64::MAX`
        // combined with a non-zero hour would — see the report.
        for (delta, want_hour, want_minute) in [
            (i64::MAX, 23u32, 59u32),
            (i64::MIN, 0, 0),
            (i64::MAX / 2, 23, 59),
            (i64::MIN / 2, 0, 0),
            (1_000_000_000, 23, 59),
            (-1_000_000_000, 0, 0),
        ] {
            let (styled, _) = laid_out(TimePicker::create(0, 0));
            let (hit, _) = wired_to(&styled, on_hour_up as usize);
            let state = wrapper(TimePickerState::default());

            let (_, _) = with_info(styled, hit, |info| {
                let _ = adjust_spinner(state.clone(), *info, true, delta);
                let _ = adjust_spinner(state.clone(), *info, false, delta);
            });

            assert_eq!(
                (read_state(&state).hour, read_state(&state).minute),
                (want_hour, want_minute),
                "delta {delta} did not saturate",
            );
        }
    }

    #[test]
    fn adjust_spinner_saturates_a_hand_written_extreme_hour_back_into_the_band() {
        // `hour = u32::MAX` plus `delta = i64::MIN` is the widest gap the clamp
        // has to close; `i64::from(u32::MAX) + i64::MIN` stays inside `i64`.
        for (start_hour, delta, want) in [
            (u32::MAX, i64::MIN, 0u32),
            (u32::MAX, -1, 23),
            (u32::MAX, 0, 23),
            (u32::MAX, 1, 23),
            (0, -1, 0),
        ] {
            let (styled, _) = laid_out(TimePicker::create(0, 0));
            let (hit, _) = wired_to(&styled, on_hour_up as usize);
            let state = wrapper(TimePickerState {
                hour: start_hour,
                minute: 0,
                is_pm: false,
                is_24h: true,
            });

            let (_, changes) = with_info(styled, hit, |info| {
                adjust_spinner(state.clone(), *info, true, delta)
            });

            assert_eq!(
                read_state(&state).hour,
                want,
                "hour {start_hour} with delta {delta} did not clamp to {want}",
            );
            assert_eq!(
                only_retext(&changes).1,
                want.to_string(),
                "the clamped value and the retext disagree",
            );
        }
    }

    #[test]
    fn adjust_spinner_with_a_zero_delta_still_retexts_and_notifies() {
        // A no-op press is still a press: the display is re-synced with the state
        // and the host is told. (This is the same path a clamped press takes.)
        let probe = log_refany();
        let (styled, shared) = laid_out(
            TimePicker::create(6, 7)
                .with_on_change(probe.clone(), record_change as TimePickerOnChangeCallbackType),
        );
        let (hit, _) = wired_to(&styled, on_hour_up as usize);

        let (update, changes) = with_info(styled, hit, |info| {
            adjust_spinner(shared.clone(), *info, false, 0)
        });

        assert_eq!(read_state(&shared).minute, 7, "a zero delta moved the value");
        assert_eq!(only_retext(&changes).1, "07", "a zero delta skipped the re-sync");
        assert_eq!(update, Update::RefreshDom, "the host's verdict was swallowed");
        assert_eq!(read_log(&probe).seen.len(), 1, "a zero delta did not notify the host");
    }

    #[test]
    fn adjust_spinner_retexts_the_display_of_the_spinner_that_was_clicked() {
        // Both arrows of a column must resolve to the *same* middle display, and
        // never to the other column's.
        for (handler, want_node) in [
            (on_hour_up as usize, N_HOUR_DISPLAY),
            (on_hour_down as usize, N_HOUR_DISPLAY),
            (on_minute_up as usize, N_MINUTE_DISPLAY),
            (on_minute_down as usize, N_MINUTE_DISPLAY),
        ] {
            let (styled, _) = laid_out(TimePicker::create(6, 30));
            let (hit, payload) = wired_to(&styled, handler);
            let is_hour = want_node == N_HOUR_DISPLAY;

            let (_, changes) = with_info(styled, hit, |info| {
                adjust_spinner(payload.clone(), *info, is_hour, 1)
            });

            assert_eq!(
                only_retext(&changes).0,
                node(text_leaf(want_node)),
                "a press on node {hit:?} retexted the wrong display",
            );
        }
    }

    // ==================================================================
    // on_change notification
    // ==================================================================

    #[test]
    fn the_change_callback_sees_the_state_after_the_edit_and_its_verdict_is_forwarded() {
        // Order matters: the state is written *before* the user callback runs, so
        // the callback observes the value the user just asked for, not the stale
        // one it is replacing.
        let probe = log_refany();
        let (styled, shared) = laid_out(
            TimePicker::create(9, 30)
                .with_on_change(probe.clone(), record_change as TimePickerOnChangeCallbackType),
        );
        let (hit, payload) = wired_to(&styled, on_hour_up as usize);

        let (update, changes) = press(styled, &payload, hit, on_hour_up);

        let log = read_log(&probe);
        assert_eq!(
            log.seen,
            vec![TimePickerState {
                hour: 10,
                minute: 30,
                is_pm: false,
                is_24h: true,
            }],
            "the change callback was not called exactly once with the NEW state",
        );
        assert_eq!(
            log.payload, 0xDEAD_BEEF,
            "the callback was handed something other than the user's own RefAny",
        );
        assert_eq!(update, Update::RefreshDom, "the user callback's Update was swallowed");
        assert_eq!(read_state(&shared).hour, 10);
        assert_eq!(changes.len(), 1, "the retext was skipped because a callback ran");
    }

    #[test]
    fn the_change_callback_still_fires_when_the_press_was_clamped_away() {
        // Pinned as-is: a press at the edge of the band reports an unchanged
        // state rather than staying silent. Hosts that treat every notification
        // as an edit will see repeats while an arrow is held down.
        let probe = log_refany();
        let (styled, _) = laid_out(
            TimePicker::create(23, 0)
                .with_on_change(probe.clone(), record_change as TimePickerOnChangeCallbackType),
        );
        let (hit, payload) = wired_to(&styled, on_hour_up as usize);

        press_n(styled, &payload, hit, on_hour_up, 3);

        let log = read_log(&probe);
        assert_eq!(log.seen.len(), 3, "a clamped press stopped notifying the host");
        assert!(
            log.seen.iter().all(|s| s.hour == 23),
            "a clamped press reported a value the widget does not hold",
        );
    }

    #[test]
    fn a_declining_change_callback_does_not_suppress_the_retext() {
        let (styled, shared) = laid_out(
            TimePicker::create(9, 30)
                .with_on_change(RefAny::new(0u8), change_do_nothing as TimePickerOnChangeCallbackType),
        );
        let (hit, payload) = wired_to(&styled, on_minute_up as usize);

        let (update, changes) = press(styled, &payload, hit, on_minute_up);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(read_state(&shared).minute, 31);
        assert_eq!(
            only_retext(&changes),
            (node(text_leaf(N_MINUTE_DISPLAY)), "31".to_string()),
            "a DoNothing user callback suppressed the widget's own repaint",
        );
    }

    #[test]
    fn every_verdict_the_change_callback_returns_is_forwarded_unchanged() {
        for (cb, want) in [
            (change_do_nothing as TimePickerOnChangeCallbackType, Update::DoNothing),
            (change_refresh_all as TimePickerOnChangeCallbackType, Update::RefreshDomAllWindows),
            (record_change as TimePickerOnChangeCallbackType, Update::RefreshDom),
        ] {
            let (styled, _) = laid_out(
                TimePicker::create(9, 30).with_on_change(log_refany(), cb),
            );
            let (hit, payload) = wired_to(&styled, on_hour_down as usize);
            let (update, _) = press(styled, &payload, hit, on_hour_down);
            assert_eq!(update, want, "a user verdict was rewritten on its way out");
        }
    }

    /// A payload that tries to reach back into the widget's own state from
    /// inside the change callback — the re-entrancy the borrow guard must refuse.
    #[derive(Debug)]
    struct ReentrantProbe {
        state: RefAny,
        got_mut: Option<bool>,
        got_ref: Option<bool>,
    }

    extern "C" fn probe_reentrancy(
        mut data: RefAny,
        _info: CallbackInfo,
        _state: TimePickerState,
    ) -> Update {
        if let Some(mut p) = data.downcast_mut::<ReentrantProbe>() {
            let mut s = p.state.clone();
            let got_mut = s.downcast_mut::<TimePickerStateWrapper>().is_some();
            let got_ref = s.downcast_ref::<TimePickerStateWrapper>().is_some();
            p.got_mut = Some(got_mut);
            p.got_ref = Some(got_ref);
        }
        Update::DoNothing
    }

    #[test]
    fn a_reentrant_callback_is_refused_the_state_instead_of_aliasing_it() {
        // The handler holds an exclusive borrow of the state while the user
        // callback runs. A callback that clones the state handle and tries to
        // borrow it again must be told "no" (None), not handed a second `&mut`
        // to memory the handler is still writing through.
        let (styled, shared) = laid_out(TimePicker::create(9, 30));
        let (hit, payload) = wired_to(&styled, on_hour_up as usize);

        let probe = RefAny::new(ReentrantProbe {
            state: shared.clone(),
            got_mut: None,
            got_ref: None,
        });
        {
            let mut s = shared.clone();
            let mut w = s
                .downcast_mut::<TimePickerStateWrapper>()
                .expect("the state changed type");
            w.on_change = Some(TimePickerOnChange {
                callback: TimePickerOnChangeCallback::from(
                    probe_reentrancy as TimePickerOnChangeCallbackType,
                ),
                refany: probe.clone(),
            })
            .into();
        }

        let (update, _) = press(styled, &payload, hit, on_hour_up);
        assert_eq!(update, Update::DoNothing);

        {
            let mut p = probe.clone();
            let seen = p
                .downcast_ref::<ReentrantProbe>()
                .expect("the probe changed type");
            assert_eq!(
                seen.got_mut,
                Some(false),
                "a re-entrant downcast_mut handed out a second &mut to the live state",
            );
            assert_eq!(
                seen.got_ref,
                Some(false),
                "a re-entrant downcast_ref aliased a live &mut borrow",
            );
        }
        assert_eq!(read_state(&shared).hour, 10, "the edit itself was lost");

        // Break the deliberate state -> probe -> state cycle so nothing leaks.
        {
            let mut s = shared.clone();
            if let Some(mut w) = s.downcast_mut::<TimePickerStateWrapper>() {
                w.on_change = None.into();
            };
        }
    }

    // ==================================================================
    // on_ampm_toggle
    // ==================================================================

    #[test]
    fn the_ampm_toggle_flips_the_flag_and_retexts_the_node_that_was_hit() {
        let (styled, shared) = laid_out(TimePicker::create(9, 30).with_24h(false));
        let (hit, payload) = wired_to(&styled, on_ampm_toggle as usize);
        assert_eq!(hit, node(N_AMPM));

        let (update, changes) = press(styled, &payload, hit, on_ampm_toggle);

        assert!(read_state(&shared).is_pm, "the toggle did not flip AM -> PM");
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            only_retext(&changes),
            (node(text_leaf(N_AMPM)), "PM".to_string()),
            "the toggle did not relabel the text inside its <p>",
        );
    }

    #[test]
    fn the_ampm_toggle_is_an_involution() {
        let (styled, shared) = laid_out(TimePicker::create(9, 30).with_24h(false).with_pm(true));
        let (hit, payload) = wired_to(&styled, on_ampm_toggle as usize);

        let (_, changes) = press_n(styled, &payload, hit, on_ampm_toggle, 4);

        assert!(read_state(&shared).is_pm, "four flips did not return to PM");
        let labels: Vec<String> = pushed_texts(&changes).into_iter().map(|(_, t)| t).collect();
        assert_eq!(
            labels,
            vec!["AM", "PM", "AM", "PM"],
            "the labels did not alternate with the flag",
        );
    }

    #[test]
    fn the_ampm_toggle_moves_the_canonical_hour_by_exactly_twelve() {
        for hour in 1..=12u32 {
            let (styled, shared) = laid_out(TimePicker::create(hour, 0).with_24h(false));
            let before = read_state(&shared).canonical_hour();
            let (hit, payload) = wired_to(&styled, on_ampm_toggle as usize);

            press(styled, &payload, hit, on_ampm_toggle);

            let after = read_state(&shared).canonical_hour();
            assert_eq!(
                after,
                before + 12,
                "toggling {hour} AM did not move the canonical hour by twelve",
            );
            assert!(after < 24, "the toggle pushed the canonical hour out of the day");
        }
    }

    #[test]
    fn the_ampm_toggle_leaves_the_hour_and_the_minute_alone() {
        let (styled, shared) = laid_out(TimePicker::create(11, 45).with_24h(false));
        let (hit, payload) = wired_to(&styled, on_ampm_toggle as usize);

        press(styled, &payload, hit, on_ampm_toggle);

        let s = read_state(&shared);
        assert_eq!((s.hour, s.minute), (11, 45), "the toggle moved the time itself");
        assert!(!s.is_24h, "the toggle changed the display mode");
    }

    #[test]
    fn the_ampm_toggle_notifies_the_host_with_the_new_flag() {
        let probe = log_refany();
        let (styled, _) = laid_out(
            TimePicker::create(3, 15)
                .with_24h(false)
                .with_on_change(probe.clone(), record_change as TimePickerOnChangeCallbackType),
        );
        let (hit, payload) = wired_to(&styled, on_ampm_toggle as usize);

        let (update, _) = press(styled, &payload, hit, on_ampm_toggle);

        assert_eq!(
            read_log(&probe).seen,
            vec![TimePickerState {
                hour: 3,
                minute: 15,
                is_pm: true,
                is_24h: false,
            }],
            "the host was not told the new AM/PM state",
        );
        assert_eq!(update, Update::RefreshDom, "the host's verdict was swallowed");
    }

    #[test]
    fn the_ampm_toggle_is_declined_for_a_foreign_payload() {
        for payload in [RefAny::new(0u8), RefAny::new(TimePickerState::default())] {
            let (styled, shared) = laid_out(TimePicker::create(9, 30).with_24h(false));
            let (hit, _) = wired_to(&styled, on_ampm_toggle as usize);

            let (update, changes) = press(styled, &payload, hit, on_ampm_toggle);

            assert_eq!(update, Update::DoNothing, "a foreign payload was accepted");
            assert!(changes.is_empty(), "a foreign payload still relabelled the toggle");
            assert!(!read_state(&shared).is_pm, "a foreign payload flipped the real flag");
        }
    }

    #[test]
    fn the_ampm_toggle_flips_the_flag_even_in_24_hour_mode() {
        // The toggle node is never rendered in 24-hour mode, but the handler is
        // still reachable (an FFI host can wire it anywhere). It flips the flag
        // unconditionally — harmless, because `canonical_hour` ignores the flag
        // in 24-hour mode.
        let (styled, shared) = laid_out(TimePicker::create(15, 0));
        let (hit, payload) = wired_to(&styled, on_hour_up as usize);
        let before = read_state(&shared).canonical_hour();

        press(styled, &payload, hit, on_ampm_toggle);

        assert!(read_state(&shared).is_pm, "the flag did not flip");
        assert_eq!(
            read_state(&shared).canonical_hour(),
            before,
            "a 24-hour widget's reading moved when the meaningless flag flipped",
        );
    }

    #[test]
    fn the_ampm_toggle_declines_a_hit_that_wraps_no_text_node() {
        // The toggle relabels the bare text leaf inside its own `<p>`, so a hit
        // it cannot resolve to one (a detached id, or any childless node) is
        // declined *before* the flag flips — no half-applied toggle.
        let (styled, shared) = laid_out(TimePicker::create(9, 30).with_24h(false));
        let (_, payload) = wired_to(&styled, on_ampm_toggle as usize);

        for hit in [node_none(), node(text_leaf(N_AMPM))] {
            let (update, changes) = press(styled.clone(), &payload, hit, on_ampm_toggle);

            assert_eq!(update, Update::DoNothing, "{hit:?}: unexpected verdict");
            assert!(changes.is_empty(), "{hit:?}: an unresolvable hit still retexted");
            assert!(
                !read_state(&shared).is_pm,
                "{hit:?}: an unresolvable hit flipped the flag anyway",
            );
        }
    }

    #[test]
    fn the_rendered_ampm_label_matches_what_the_toggle_would_push() {
        // End-to-end consistency: re-rendering after a toggle must produce the
        // same label the toggle pushed, or the widget would flicker back on the
        // next relayout.
        for start_pm in [false, true] {
            let picker = TimePicker::create(9, 30).with_24h(false).with_pm(start_pm);
            let (styled, shared) = laid_out(picker);
            let (hit, payload) = wired_to(&styled, on_ampm_toggle as usize);

            let (_, changes) = press(styled, &payload, hit, on_ampm_toggle);
            let pushed = only_retext(&changes).1;

            let after = read_state(&shared);
            let rerendered = ampm_label(
                &TimePicker::create(after.hour, after.minute)
                    .with_24h(false)
                    .with_pm(after.is_pm)
                    .dom(),
            );
            assert_eq!(
                rerendered.as_deref(),
                Some(pushed.as_str()),
                "the pushed label and the re-rendered label disagree (start_pm={start_pm})",
            );
        }
    }
}
