//! Calendar date picker widget.
//!
//! Renders a month header (‹ prev / `Month YYYY` / next ›) above a weekday
//! header row (`Su Mo Tu We Th Fr Sa`) and a 7-column grid of day cells laid
//! out as week rows. The grid is computed from real calendar math — days in
//! month (with leap-year February) and the weekday offset of the 1st (Sakamoto's
//! algorithm) — so the leading blank cells and day count are correct for the
//! given month.
//!
//! Clicking a day cell selects it: the handler reads the cell's baked day
//! number, updates `state.day`, fires the optional `on_change(state)`, and
//! live-restyles the grid (accent fill on the selected cell, neutral on the
//! rest) exactly like `segmented.rs` restyles its active segment. The day number
//! is carried per-cell (like `drop_down.rs`'s per-item data) alongside a clone of
//! the shared-state handle, so selection never depends on re-deriving the grid
//! offset at click time.
//!
//! TODO2 — MONTH NAVIGATION CANNOT REBUILD THE GRID IN-WIDGET.
//! Clicking ‹ / › changes a *different month*, which has a different day count
//! and weekday offset, i.e. a different *number of day-cell nodes*. A widget
//! callback can only `set_css_property` / `change_node_text` on the EXISTING
//! nodes — it cannot add/remove/relayout day cells (the same limitation
//! `combobox`'s type-to-filter hit). Therefore the ‹ / › buttons DO update the
//! month/year in the state and fire `on_change(state)` so host code can rebuild
//! the widget (a fresh `DatePicker::create(...)` with the new month), but the
//! in-widget grid does NOT change, and the header is deliberately NOT re-texted
//! either — showing a new month name over the old day grid would be a misleading
//! half-switch. Day-selection (the restyle) is fully functional for the
//! displayed month; after a ‹ / › without a host rebuild the grid is stale (the
//! documented limitation). Computing the initial grid from calendar math is NOT
//! faked behaviour — only the live month rebuild is the limitation.
//!
//! Key types: [`DatePicker`], [`DatePickerState`], [`DatePickerOnChange`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::{OptionRefAny, RefAny},
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignSelf, LayoutFlexGrow, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutAlignItems, LayoutWidth, LayoutHeight},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextAlign, StyleCursor, StyleUserSelect, StyleTextColor},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

// ---- classes ----
static DATE_PICKER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker"))];
static HEADER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-header"))];
static HEADER_LABEL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-label"))];
static NAV_BTN_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-nav"))];
static WEEKDAY_ROW_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-weekdays"))];
static WEEKDAY_CELL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-weekday"))];
static GRID_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-grid"))];
static WEEK_ROW_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-week"))];
static DAY_CELL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker-day"))];

const PREV_ARROW: AzString = AzString::from_const_str("\u{2039}"); // ‹
const NEXT_ARROW: AzString = AzString::from_const_str("\u{203A}"); // ›

const WEEKDAY_NAMES: [AzString; 7] = [
    AzString::from_const_str("Su"),
    AzString::from_const_str("Mo"),
    AzString::from_const_str("Tu"),
    AzString::from_const_str("We"),
    AzString::from_const_str("Th"),
    AzString::from_const_str("Fr"),
    AzString::from_const_str("Sa"),
];

/// Callback type invoked when the selected day or displayed month/year changes.
pub type DatePickerOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, DatePickerState) -> Update;
impl_widget_callback!(
    DatePickerOnChange,
    OptionDatePickerOnChange,
    DatePickerOnChangeCallback,
    DatePickerOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DatePickerOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DATE_PICKER_ON_CHANGE_INVOKER,
    invoker_ty:     AzDatePickerOnChangeCallbackInvoker,
    thunk_fn:       az_date_picker_on_change_callback_thunk,
    setter_fn:      AzApp_setDatePickerOnChangeCallbackInvoker,
    from_handle_fn: AzDatePickerOnChangeCallback_createFromHostHandle,
    extra_args:     [ state: DatePickerState ],
}

/// A calendar date picker.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DatePicker {
    pub state: DatePickerStateWrapper,
    /// Style for the outer container.
    pub container_style: CssPropertyWithConditionsVec,
}

/// Wraps [`DatePickerState`] together with its change callback.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DatePickerStateWrapper {
    pub inner: DatePickerState,
    pub on_change: OptionDatePickerOnChange,
}

/// State of a [`DatePicker`]: the displayed month/year and the selected day.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DatePickerState {
    /// The displayed (and selected) year.
    pub year: u32,
    /// The displayed (and selected) month, `1..=12`.
    pub month: u32,
    /// The selected day of the month, `1..=31`.
    pub day: u32,
}

impl Default for DatePickerState {
    fn default() -> Self {
        Self {
            year: 2000,
            month: 1,
            day: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure calendar math (standard, well-known formulas — not faked behaviour).
// ---------------------------------------------------------------------------

/// Gregorian leap-year test.
const fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Number of days in the given (1-based) month of the given year.
const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 30, // defensive (month is clamped to 1..=12 elsewhere)
    }
}

/// Sakamoto's algorithm: weekday of `(year, month, day)`, returned as
/// `0 = Sunday .. 6 = Saturday`. Verified: 2000-01-01 -> 6 (Saturday).
fn weekday(year: u32, month: u32, day: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = year as i32;
    if month < 3 {
        y -= 1;
    }
    let idx = if (1..=12).contains(&month) {
        (month - 1) as usize
    } else {
        0
    };
    let w = (y + y / 4 - y / 100 + y / 400 + T[idx] + day as i32) % 7;
    (((w % 7) + 7) % 7) as u32
}

/// English month name for a 1-based month index.
const fn month_name(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let idx = month.saturating_sub(1) as usize;
    if idx < 12 {
        NAMES[idx]
    } else {
        ""
    }
}

// ---- colours ----
const BORDER_COLOR: ColorU = ColorU { r: 206, g: 212, b: 218, a: 255 };
const TEXT_COLOR: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };
const MUTED_COLOR: ColorU = ColorU { r: 108, g: 117, b: 125, a: 255 };
const ACCENT_BG: ColorU = ColorU { r: 13, g: 110, b: 253, a: 255 };
const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
const TRANSPARENT: ColorU = ColorU { r: 0, g: 0, b: 0, a: 0 };

const DAY_SELECTED_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ACCENT_BG)];
const DAY_SELECTED_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(DAY_SELECTED_BG_ITEMS);
const TRANSPARENT_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(TRANSPARENT)];
const TRANSPARENT_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(TRANSPARENT_BG_ITEMS);
const WHITE_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(WHITE)];
const WHITE_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(WHITE_BG_ITEMS);

const CELL_W: isize = 32;
const CELL_H: isize = 28;

/// Outer container: a column that hugs its content, bordered + white.
static CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        8,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(WHITE_BG_VEC)),
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

/// Header row: prev button / centred label / next button.
static HEADER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(6),
    )),
];

/// The ‹ / › nav buttons.
static NAV_BTN_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(24))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// The `Month YYYY` label (centred, fills the header).
static HEADER_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
];

/// Weekday header row + cells.
static ROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

static WEEKDAY_CELL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(CELL_W))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: MUTED_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
];

/// Grid: column of week rows.
static GRID_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// Blank (offset / trailing) cell — keeps the column width, no text/callback.
static BLANK_CELL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(CELL_W))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(CELL_H))),
];

/// Builds the per-day-cell style. Only the background + text colour depend on
/// the selected flag (the rest is shared), so the style is built at runtime
/// (mirrors `segmented::build_segment_style`).
fn build_day_cell_style(selected: bool) -> CssPropertyWithConditionsVec {
    let (bg, text) = if selected {
        (DAY_SELECTED_BG_VEC, WHITE)
    } else {
        (TRANSPARENT_BG_VEC, TEXT_COLOR)
    };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(CELL_W))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(CELL_H))),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            5,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
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
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg)),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
    ])
}

/// Per-day-cell callback payload: the cell's day number + a clone of the shared
/// state handle (so the handler can update `state.day` + fire `on_change`).
struct DayCellData {
    day: u32,
    state: RefAny,
}

impl DatePicker {
    /// Creates a new `DatePicker` showing `year`/`month` with `day` selected.
    /// `month` is clamped to `1..=12` and `day` to `1..=days_in_month`.
    #[must_use] pub fn create(year: u32, month: u32, day: u32) -> Self {
        let month = month.clamp(1, 12);
        let dim = days_in_month(year, month);
        let day = day.clamp(1, dim);
        Self {
            state: DatePickerStateWrapper {
                inner: DatePickerState { year, month, day },
                on_change: None.into(),
            },
            container_style: CssPropertyWithConditionsVec::from_const_slice(CONTAINER_STYLE),
        }
    }

    /// Sets the callback invoked when the selection or month changes.
    pub fn set_on_change<C: Into<DatePickerOnChangeCallback>>(&mut self, data: RefAny, callback: C) {
        self.state.on_change = Some(DatePickerOnChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder variant of [`Self::set_on_change`].
    pub fn with_on_change<C: Into<DatePickerOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// Replaces `self` with the default value and returns the original.
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(2000, 1, 1);
        core::mem::swap(&mut s, self);
        s
    }

    #[must_use] pub fn dom(self) -> Dom {
        let inner = self.state.inner;
        let year = inner.year;
        let month = inner.month.clamp(1, 12);
        let sel_day = inner.day;
        let container_style = self.container_style.clone();

        let shared = RefAny::new(self.state);

        let header = build_header(year, month, shared.clone());
        let weekday_row = build_weekday_row();
        let grid = build_grid(year, month, sel_day, shared);

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(DATE_PICKER_CLASS))
            .with_css_props(container_style)
            .with_children(alloc::vec![header, weekday_row, grid].into())
    }
}

impl Default for DatePicker {
    fn default() -> Self {
        Self::create(2000, 1, 1)
    }
}

fn build_header(year: u32, month: u32, shared: RefAny) -> Dom {
    use azul_core::dom::{EventFilter, HoverEventFilter};

    let nav = |arrow: AzString, cb: usize, refany: RefAny| -> Dom {
        Dom::create_text(arrow)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(NAV_BTN_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(NAV_BTN_STYLE))
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

    let label = AzString::from(format!("{} {}", month_name(month), year));

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(HEADER_STYLE))
        .with_children(
            alloc::vec![
                nav(PREV_ARROW, on_prev_month as usize, shared.clone()),
                Dom::create_text(label)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_LABEL_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                        HEADER_LABEL_STYLE,
                    )),
                nav(NEXT_ARROW, on_next_month as usize, shared),
            ]
            .into(),
        )
}

fn build_weekday_row() -> Dom {
    let cells: Vec<Dom> = WEEKDAY_NAMES
        .iter()
        .map(|n| {
            Dom::create_text(n.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEKDAY_CELL_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    WEEKDAY_CELL_STYLE,
                ))
        })
        .collect();

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEKDAY_ROW_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(ROW_STYLE))
        .with_children(cells.into())
}

fn build_grid(year: u32, month: u32, sel_day: u32, shared: RefAny) -> Dom {
    let leading = weekday(year, month, 1);
    let dim = days_in_month(year, month);
    let total = leading + dim;
    let rows = total.div_ceil(7);

    let mut week_rows: Vec<Dom> = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut cells: Vec<Dom> = Vec::with_capacity(7);
        for c in 0..7 {
            let i = r * 7 + c;
            if i < leading || i >= leading + dim {
                cells.push(build_blank_cell());
            } else {
                let day = i - leading + 1;
                cells.push(build_day_cell(day, day == sel_day, shared.clone()));
            }
        }
        week_rows.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEK_ROW_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(ROW_STYLE))
                .with_children(cells.into()),
        );
    }

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(GRID_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(GRID_STYLE))
        .with_children(week_rows.into())
}

fn build_blank_cell() -> Dom {
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(DAY_CELL_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(BLANK_CELL_STYLE))
}

fn build_day_cell(day: u32, selected: bool, shared: RefAny) -> Dom {
    use azul_core::dom::{EventFilter, HoverEventFilter};

    Dom::create_text(AzString::from(format!("{day}")))
        .with_ids_and_classes(IdOrClassVec::from_const_slice(DAY_CELL_CLASS))
        .with_css_props(build_day_cell_style(selected))
        .with_callbacks(
            alloc::vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: on_day_click as usize,
                    ctx: OptionRefAny::None,
                },
                refany: RefAny::new(DayCellData { day, state: shared }),
            }]
            .into(),
        )
        .with_tab_index(TabIndex::Auto)
}

/// Day-cell click handler. Reads the cell's baked day, updates the shared
/// `state.day`, fires `on_change`, and live-restyles the whole grid.
extern "C" fn on_day_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let clicked = info.get_hit_node();

    // Read the baked day + clone the shared-state handle.
    let (day, mut shared) = {
        let cell = match data.downcast_ref::<DayCellData>() {
            Some(c) => c,
            None => return Update::DoNothing,
        };
        (cell.day, cell.state.clone())
    };

    let update = {
        let mut w = match shared.downcast_mut::<DatePickerStateWrapper>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        w.inner.day = day;
        let inner = w.inner;
        let w = &mut *w;
        match w.on_change.as_mut() {
            Some(DatePickerOnChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        }
    };

    restyle_days(&mut info, clicked);

    update
}

/// Accents the clicked cell and neutralises every other grid cell (blanks
/// included — for them transparent bg + a colour on empty text is a no-op).
fn restyle_days(info: &mut CallbackInfo, clicked: azul_core::dom::DomNodeId) {
    let row = match info.get_parent(clicked) {
        Some(r) => r,
        None => return,
    };
    let grid = match info.get_parent(row) {
        Some(g) => g,
        None => return,
    };

    let mut week = info.get_first_child(grid);
    while let Some(w) = week {
        let mut cellopt = info.get_first_child(w);
        while let Some(cell) = cellopt {
            if cell == clicked {
                info.set_css_property(
                    cell,
                    CssProperty::const_background_content(DAY_SELECTED_BG_VEC),
                );
                info.set_css_property(
                    cell,
                    CssProperty::const_text_color(StyleTextColor { inner: WHITE }),
                );
            } else {
                info.set_css_property(
                    cell,
                    CssProperty::const_background_content(TRANSPARENT_BG_VEC),
                );
                info.set_css_property(
                    cell,
                    CssProperty::const_text_color(StyleTextColor { inner: TEXT_COLOR }),
                );
            }
            cellopt = info.get_next_sibling(cell);
        }
        week = info.get_next_sibling(w);
    }
}

extern "C" fn on_prev_month(data: RefAny, info: CallbackInfo) -> Update {
    month_nav(data, info, -1)
}

extern "C" fn on_next_month(data: RefAny, info: CallbackInfo) -> Update {
    month_nav(data, info, 1)
}

/// Month navigation. Updates month/year (wrapping across year boundaries),
/// clamps the selected day into the new month, and fires `on_change` so host
/// code can rebuild the widget. TODO2: the in-widget grid is NOT rebuilt (see
/// module docs) — only the reported state changes.
fn month_nav(mut data: RefAny, info: CallbackInfo, delta: i32) -> Update {
    let mut w = match data.downcast_mut::<DatePickerStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let mut month = w.inner.month as i32 + delta;
    let mut year = w.inner.year as i32;
    if month < 1 {
        month = 12;
        year -= 1;
    } else if month > 12 {
        month = 1;
        year += 1;
    }
    w.inner.year = year.max(1) as u32;
    w.inner.month = month as u32;
    let dim = days_in_month(w.inner.year, w.inner.month);
    if w.inner.day > dim {
        w.inner.day = dim;
    }

    let inner = w.inner;
    let w = &mut *w;
    match w.on_change.as_mut() {
        Some(DatePickerOnChange { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
        None => Update::DoNothing,
    }
}

impl From<DatePicker> for Dom {
    fn from(d: DatePicker) -> Self {
        d.dom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_years() {
        assert!(is_leap(2000));
        assert!(is_leap(2024));
        assert!(!is_leap(1900));
        assert!(!is_leap(2023));
    }

    #[test]
    fn days_per_month() {
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2024, 4), 30);
        assert_eq!(days_in_month(2024, 1), 31);
    }

    #[test]
    fn weekday_known_dates() {
        // 2000-01-01 was a Saturday (6).
        assert_eq!(weekday(2000, 1, 1), 6);
        // 2026-06-01 is a Monday (1).
        assert_eq!(weekday(2026, 6, 1), 1);
        // 1970-01-01 was a Thursday (4).
        assert_eq!(weekday(1970, 1, 1), 4);
    }
}
