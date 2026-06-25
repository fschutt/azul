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
            Dom::create_text(SEPARATOR_TEXT)
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
                Dom::create_text(ampm_text)
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
                    .with_tab_index(TabIndex::Auto),
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
        Dom::create_text(arrow)
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
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SPINNER_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SPINNER_STYLE))
        .with_children(
            alloc::vec![
                arrow_cell(UP_ARROW, up_cb, state.clone()),
                Dom::create_text(value)
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
    // The clicked node is an arrow; its parent is the spinner; the spinner's
    // first child is the up arrow and the next sibling is the value display.
    let hit = info.get_hit_node();
    let Some(parent) = info.get_parent(hit) else {
        return Update::DoNothing;
    };
    let Some(up) = info.get_first_child(parent) else {
        return Update::DoNothing;
    };
    let Some(display) = info.get_next_sibling(up) else {
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

/// Toggles the AM/PM flag and re-texts the clicked toggle node.
extern "C" fn on_ampm_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();

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

    info.change_node_text(hit, text);
    update
}

impl From<TimePicker> for Dom {
    fn from(t: TimePicker) -> Self {
        t.dom()
    }
}
