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
//! KEYBOARD (WAI-ARIA APG date grid): the day cells are ONE Tab stop - the
//! selected day. Left/Right move focus by a day, Up/Down by a week, Home/End
//! to the ends of the week row; Enter/Space (a click) picks the focused day;
//! PageUp/PageDown act as ‹ / ›. An arrow past the displayed month goes
//! nowhere, for the same TODO2 reason: the grid cannot rebuild itself.
//!
//! Key types: [`DatePicker`], [`DatePickerState`], [`DatePickerOnChange`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        ComponentEventFilter, Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class,
        IdOrClassVec, NodeData, NodeType, TabIndex,
    },
    refany::{OptionRefAny, RefAny},
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
};
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec,
    },
    impl_option_inner,
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            StyleFontSize,
        },
        layout::{
            LayoutAlignItems, LayoutAlignSelf, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutHeight, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight,
            LayoutPaddingTop, LayoutPosition, LayoutWidth,
        },
        property::{CssProperty, *},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderTopStyle, StyleCursor,
            StyleTextAlign, StyleTextColor, StyleUserSelect,
        },
    },
    AzString, OptionString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::themes::system_palette as sys,
};

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// ---- classes ----
static DATE_PICKER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-date-picker"))];
/// The always-visible field (what the calendar hangs off).
static DATE_PICKER_VALUE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-value",
))];
/// The calendar body, inside the `<transient-window>`.
static DATE_PICKER_PANEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-panel",
))];
static HEADER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-header",
))];
static HEADER_LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-label",
))];
static NAV_BTN_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-nav",
))];
static WEEKDAY_ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-weekdays",
))];
static WEEKDAY_CELL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-weekday",
))];
static GRID_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-grid",
))];
static WEEK_ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-week",
))];
static DAY_CELL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-picker-day",
))];

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
    from_handle_byref_fn: AzDatePickerOnChangeCallback_createFromHostHandleByref,
    extra_args:     [ state: DatePickerState ],
}

/// A calendar date picker.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DatePicker {
    pub state: DatePickerStateWrapper,
    /// Style for the outer container.
    /// Style for the container, or `None` for "no opinion" — in which case the
    /// widget's default applies.
    ///
    /// `None` and `Some(empty)` are different answers: the first means the
    /// widget picks, the second means the caller asked for no properties at all
    /// and gets none.
    pub container_style: OptionCssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET so it knows at build time whether it was named;
    /// forwarded into the accessibility declaration it already builds.
    pub accessibility_name: OptionString,
    /// The HTML `name` the value is submitted under in a
    /// [`crate::widgets::form::Form`]; `None` keeps it out of the form data.
    pub name: OptionString,
    /// What the picker picks: a day, a month or an ISO week.
    pub mode: DatePickerMode,
    /// The widget theme, or `None` for the default
    /// (`crate::widgets::themes::UiTheme::default()`, Flat).
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// What a theme decides about a date picker: the style of every part, the two
/// faces of a grid cell, and the fills a click repaints the grid with.
/// [`build`] keeps the structure (the handlers walk it by position) and the
/// behaviour; built by `themes::flat::date_picker` and
/// `themes::flora::date_picker` (the flat one from [`Self::established`]).
///
/// One look serves every mode: the day grid (`date`), the Monday-first day
/// grid whose whole selected row is lit (`week`), and the year over its
/// twelve months (`month`) - a month cell is a day cell's face, three day
/// cells wide.
pub(crate) struct DatePickerLook {
    /// The closed field (a keyboard stop).
    pub field: Vec<CssPropertyWithConditions>,
    /// The value text inside the field.
    pub field_value: Vec<CssPropertyWithConditions>,
    /// The calendar glyph inside the field.
    pub field_icon: Vec<CssPropertyWithConditions>,
    /// The calendar panel, when the caller set no `container_style`.
    pub panel: Vec<CssPropertyWithConditions>,
    /// The header row (month or year header).
    pub header: Vec<CssPropertyWithConditions>,
    /// The previous / next buttons (keyboard stops).
    pub nav: Vec<CssPropertyWithConditions>,
    /// The `Month YYYY` (or `YYYY`) label.
    pub header_label: Vec<CssPropertyWithConditions>,
    /// The weekday row, each week row and each row of months.
    pub row: Vec<CssPropertyWithConditions>,
    /// A weekday name.
    pub weekday: Vec<CssPropertyWithConditions>,
    /// The grid of rows.
    pub grid: Vec<CssPropertyWithConditions>,
    /// A blank (offset) cell.
    pub blank: Vec<CssPropertyWithConditions>,
    /// The face of a selected cell: the picked day, every day of the picked
    /// week, the picked month.
    pub day_selected: Vec<CssPropertyWithConditions>,
    /// The face of every other cell.
    pub day_other: Vec<CssPropertyWithConditions>,
    /// What a click repaints the grid with (an override outranks the faces'
    /// dark twins, so the repaint picks the theme's colours itself).
    pub day_palette: DayPalette,
    /// The theme's marker class on the field, if it has one.
    pub marker: Option<&'static str>,
}

/// The fill and ink of a grid cell, `[light, dark]`, for a selected cell and
/// for every other one - what the click handlers write after a pick.
#[derive(Debug, Clone)]
pub(crate) struct DayPalette {
    /// A selected cell, `[light, dark]`.
    pub selected: [(StyleBackgroundContentVec, ColorU); 2],
    /// Every other cell, `[light, dark]`.
    pub other: [(StyleBackgroundContentVec, ColorU); 2],
}

impl DatePickerLook {
    /// The widget's established styles, part by part (what flat builds on).
    pub(crate) fn established() -> Self {
        Self {
            field: FIELD_STYLE.to_vec(),
            field_value: FIELD_VALUE_STYLE.to_vec(),
            field_icon: FIELD_ICON_STYLE.to_vec(),
            panel: CONTAINER_STYLE.to_vec(),
            header: HEADER_STYLE.to_vec(),
            nav: NAV_BTN_STYLE.to_vec(),
            header_label: HEADER_LABEL_STYLE.to_vec(),
            row: ROW_STYLE.to_vec(),
            weekday: WEEKDAY_CELL_STYLE.to_vec(),
            grid: GRID_STYLE.to_vec(),
            blank: BLANK_CELL_STYLE.to_vec(),
            day_selected: day_cell_style(true).into_library_owned_vec(),
            day_other: day_cell_style(false).into_library_owned_vec(),
            day_palette: DayPalette::established(),
            marker: None,
        }
    }
}

impl DayPalette {
    /// The widget's established repaint colours ([`day_cell_colours`]).
    pub(crate) fn established() -> Self {
        Self {
            selected: [day_cell_colours(true, false), day_cell_colours(true, true)],
            other: [
                day_cell_colours(false, false),
                day_cell_colours(false, true),
            ],
        }
    }
}

/// What a [`DatePicker`] picks - one calendar, the three HTML date inputs.
///
/// The state is the same `(year, month, day)` in every mode, so every mode
/// shares the calendar math and the popup: `Month` ignores the day (it is kept
/// at 1), `Week` holds the MONDAY of the picked ISO week.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum DatePickerMode {
    /// `<input type=date>`: a day, value `YYYY-MM-DD`.
    #[default]
    Date,
    /// `<input type=month>`: a year and a month, value `YYYY-MM`.
    Month,
    /// `<input type=week>`: an ISO 8601 week, value `YYYY-Www`.
    Week,
}

impl DatePickerMode {
    /// The HTML `type` attribute value this mode stands for.
    #[must_use]
    pub const fn html_type(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Month => "month",
            Self::Week => "week",
        }
    }
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
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Number of days in the given (1-based) month of the given year.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant
                                  // (or cross-type bindings that can't merge)
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
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded layout/render numeric cast
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

/// English weekday name for `weekday()`'s `0 = Sunday .. 6 = Saturday`.
const fn weekday_name(weekday: u32) -> &'static str {
    const NAMES: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    let idx = weekday as usize;
    if idx < 7 {
        NAMES[idx]
    } else {
        ""
    }
}

/// What a screen reader announces for one day of the grid: the full date
/// ("Tuesday, 23 June 2026"), because the grid is ONE Tab stop and the arrow
/// keys move within it, so a bare "23" would not say where focus landed.
fn day_accessibility_name(year: u32, month: u32, day: u32) -> AzString {
    AzString::from(format!(
        "{}, {} {} {}",
        weekday_name(weekday(year, month, day)),
        day,
        month_name(month),
        year
    ))
}

// ---------------------------------------------------------------------------
// ISO 8601 weeks (type=week) and month names (type=month). Built on the same
// `weekday` / `days_in_month` math as the day grid.
// ---------------------------------------------------------------------------

/// Days in `year`.
const fn days_in_year(year: u32) -> u32 {
    if is_leap(year) {
        366
    } else {
        365
    }
}

/// 1-based day of the year.
fn day_of_year(year: u32, month: u32, day: u32) -> u32 {
    (1..month.clamp(1, 12))
        .map(|m| days_in_month(year, m))
        .sum::<u32>()
        + day
}

/// ISO weekday: `1 = Monday .. 7 = Sunday`.
fn iso_weekday(year: u32, month: u32, day: u32) -> u32 {
    match weekday(year, month, day) {
        0 => 7,
        w => w,
    }
}

/// Number of ISO weeks in ISO year `year`: 53 when it begins on a Thursday,
/// or on a Wednesday in a leap year; 52 otherwise.
pub(crate) fn iso_weeks_in_year(year: u32) -> u32 {
    let jan1 = iso_weekday(year, 1, 1);
    if jan1 == 4 || (jan1 == 3 && is_leap(year)) {
        53
    } else {
        52
    }
}

/// The ISO 8601 `(week-numbering year, week)` a date falls in. The year can
/// differ from the calendar year in the first and last days of January and
/// December: 2021-01-01 is in week 53 of 2020.
pub(crate) fn iso_week_of(year: u32, month: u32, day: u32) -> (u32, u32) {
    let ordinal = day_of_year(year, month, day);
    let wd = iso_weekday(year, month, day);
    let week = (ordinal + 10).saturating_sub(wd) / 7;
    if week < 1 {
        let prev = year.saturating_sub(1);
        (prev, iso_weeks_in_year(prev))
    } else if week > iso_weeks_in_year(year) {
        (year + 1, 1)
    } else {
        (year, week)
    }
}

/// The calendar date `ordinal` days into `year` (1 = January 1st), where the
/// ordinal may run past either end of the year.
fn date_from_ordinal(year: u32, ordinal: i64) -> (u32, u32, u32) {
    let mut year = year;
    let mut ordinal = ordinal;
    while ordinal < 1 && year > 1 {
        year -= 1;
        ordinal += i64::from(days_in_year(year));
    }
    while ordinal > i64::from(days_in_year(year)) {
        ordinal -= i64::from(days_in_year(year));
        year += 1;
    }
    let mut remaining = u32::try_from(ordinal.max(1)).unwrap_or(1);
    let mut month = 1;
    while month < 12 && remaining > days_in_month(year, month) {
        remaining -= days_in_month(year, month);
        month += 1;
    }
    (year, month, remaining)
}

/// The Monday that starts ISO week `week` of ISO year `year` (clamped to the
/// weeks that year has). January 4th is always in week 1.
pub(crate) fn iso_week_monday(year: u32, week: u32) -> (u32, u32, u32) {
    let week = week.clamp(1, iso_weeks_in_year(year));
    let week1_monday = 4 - i64::from(iso_weekday(year, 1, 4)) + 1;
    date_from_ordinal(year, week1_monday + 7 * (i64::from(week) - 1))
}

impl DatePickerState {
    /// The ISO 8601 `(week-numbering year, week)` of the selected day - what a
    /// `type=week` picker reports.
    #[must_use]
    pub fn iso_week(&self) -> (u32, u32) {
        iso_week_of(self.year, self.month, self.day)
    }
}

/// The HTML value of `state` in `mode`: `YYYY-MM-DD`, `YYYY-MM` or `YYYY-Www`
/// (the ISO week-numbering year and week), zero-padded so the field never
/// changes width as the user moves through the calendar.
pub(crate) fn format_value(state: &DatePickerState, mode: DatePickerMode) -> String {
    match mode {
        DatePickerMode::Date => format_date(state),
        DatePickerMode::Month => alloc::format!("{:04}-{:02}", state.year, state.month),
        DatePickerMode::Week => {
            let (year, week) = state.iso_week();
            alloc::format!("{year:04}-W{week:02}")
        }
    }
}

/// Short English month names for the month grid.
const MONTH_ABBREVIATIONS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

// ---- colours ----
const BORDER_COLOR: ColorU = ColorU {
    r: 206,
    g: 212,
    b: 218,
    a: 255,
};
const TEXT_COLOR: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
const MUTED_COLOR: ColorU = ColorU {
    r: 108,
    g: 117,
    b: 125,
    a: 255,
};
const ACCENT_BG: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};
const WHITE: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const TRANSPARENT: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

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
/// The closed-state field: a one-line box showing the selected date.
static FIELD_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    // Hug the content so the popup (anchored to this node's rect) lines up with
    // the field rather than with a full-width flex line.
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        4,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(WHITE_BG_VEC)),
    // A field surface: the desktop's field colour in the dark theme.
    sys::DARK_CONTROL_BACKGROUND,
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
        StyleBorderBottomColor {
            inner: BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_COLOR,
        },
    )),
    sys::DARK_SEPARATOR_BORDER_TOP,
    sys::DARK_SEPARATOR_BORDER_BOTTOM,
    sys::DARK_SEPARATOR_BORDER_LEFT,
    sys::DARK_SEPARATOR_BORDER_RIGHT,
];

/// The date text inside the field.
static FIELD_VALUE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The little calendar glyph at the field's right edge.
static FIELD_ICON_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(16))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

static CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        8,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(WHITE_BG_VEC)),
    // A field surface: the desktop's field colour in the dark theme.
    sys::DARK_CONTROL_BACKGROUND,
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
        StyleBorderBottomColor {
            inner: BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_COLOR,
        },
    )),
    sys::DARK_SEPARATOR_BORDER_TOP,
    sys::DARK_SEPARATOR_BORDER_BOTTOM,
    sys::DARK_SEPARATOR_BORDER_LEFT,
    sys::DARK_SEPARATOR_BORDER_RIGHT,
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
    sys::DARK_TEXT,
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
    sys::DARK_TEXT,
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
    sys::DARK_SECONDARY_TEXT,
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
];

/// Grid: column of week rows.
static GRID_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
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
pub(crate) fn build_day_cell_style(selected: bool) -> CssPropertyWithConditionsVec {
    let (bg, text) = if selected {
        (DAY_SELECTED_BG_VEC, WHITE)
    } else {
        (TRANSPARENT_BG_VEC, TEXT_COLOR)
    };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(CELL_W))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            CELL_H
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(5,)
        )),
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

/// One day cell's style as the grid renders it: [`build_day_cell_style`]'s
/// light face, then the dark twins of its colours (a day cell has no
/// state-conditional rules, so the twins simply go last). An unselected
/// cell's fill is transparent in both themes and needs no twin.
fn day_cell_style(selected: bool) -> CssPropertyWithConditionsVec {
    let mut v = build_day_cell_style(selected).into_library_owned_vec();
    if selected {
        v.push(sys::DARK_ACCENT_BACKGROUND);
        v.push(sys::DARK_ACCENT_TEXT);
    } else {
        v.push(sys::DARK_TEXT);
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The fill and ink `restyle_days` writes onto a day cell, in the window's
/// theme. A `set_css_property` override outranks every inline declaration,
/// dark twins included, so the restyle has to pick the dark face itself.
fn day_cell_colours(selected: bool, dark: bool) -> (StyleBackgroundContentVec, ColorU) {
    match (selected, dark) {
        (true, false) => (DAY_SELECTED_BG_VEC, WHITE),
        (false, false) => (TRANSPARENT_BG_VEC, TEXT_COLOR),
        (true, true) => (sys::ACCENT_BACKGROUND, sys::ACCENT_TEXT),
        (false, true) => (TRANSPARENT_BG_VEC, sys::TEXT),
    }
}

/// Whether the window a callback runs in cascades in the dark theme - the
/// answer `LayoutWindow::dynamic_selector_context` gives: the `AZ_THEME` pin
/// first, then the window's own theme.
pub(crate) fn window_is_dark(info: &CallbackInfo) -> bool {
    azul_css::dynamic_selector::theme_pinned_by_env().map_or_else(
        || info.get_current_window_state().theme == azul_core::window::WindowTheme::DarkMode,
        |t| t == azul_css::dynamic_selector::ThemeCondition::Dark,
    )
}

/// Per-day-cell callback payload: the cell's day number + a clone of the shared
/// state handle (so the handler can update `state.day` + fire `on_change`).
struct DayCellData {
    day: u32,
    state: RefAny,
    /// The theme's repaint colours: the handler has no other way to know
    /// which look the grid it restyles was built in.
    palette: DayPalette,
}

/// What every date-picker callback shares.
///
/// `DatePickerState` is a `#[repr(C)]` FFI struct, so the popup's open flag
/// cannot live on it — it lives here, beside the state, exactly as
/// `color_input.rs`'s `ColorPickerData` carries the picker's own open flag.
/// The ENGINE owns the authoritative open state (keyed by the transient node's
/// id, and it survives a `RefreshDom`); this flag only decides which way the
/// next field click toggles, which is why `Dismissed` resyncs it.
pub(crate) struct DatePickerData {
    pub(crate) state: DatePickerStateWrapper,
    pub(crate) open: bool,
    /// What the picker picks - decides the field's value format and which
    /// grid the popup shows.
    pub(crate) mode: DatePickerMode,
}

impl DatePickerData {
    /// The HTML value the picker submits (see [`format_value`]).
    pub(crate) fn form_value(&self) -> String {
        format_value(&self.state.inner, self.mode)
    }
}

impl DatePicker {
    /// Creates a new `DatePicker` showing `year`/`month` with `day` selected.
    /// `month` is clamped to `1..=12` and `day` to `1..=days_in_month`.
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    #[must_use]
    pub fn create(year: u32, month: u32, day: u32) -> Self {
        let month = month.clamp(1, 12);
        let dim = days_in_month(year, month);
        let day = day.clamp(1, dim);
        Self {
            state: DatePickerStateWrapper {
                inner: DatePickerState { year, month, day },
                on_change: None.into(),
            },
            container_style: OptionCssPropertyWithConditionsVec::None,
            accessibility_name: OptionString::None,
            name: OptionString::None,
            mode: DatePickerMode::Date,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Pick the widget theme. Unset (`None`), the picker renders in the
    /// default theme (`crate::widgets::themes::UiTheme::default()`).
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// `<input type=month>`: picks a year and a month (value `YYYY-MM`). The
    /// calendar is the year over a grid of its twelve months.
    #[must_use]
    pub fn create_month(year: u32, month: u32) -> Self {
        Self::create(year, month, 1).with_mode(DatePickerMode::Month)
    }

    /// `<input type=week>`: picks an ISO 8601 week (value `YYYY-Www`) of the
    /// week-numbering year `year`; `week` is clamped to the weeks that year
    /// has. The state holds the week's Monday; the calendar is the day grid,
    /// Monday-first, and a click anywhere in a week picks the whole week.
    #[must_use]
    pub fn create_week(year: u32, week: u32) -> Self {
        let (y, m, d) = iso_week_monday(year, week);
        Self::create(y, m, d).with_mode(DatePickerMode::Week)
    }

    /// Switch what the picker picks (see [`DatePickerMode`]).
    pub const fn set_mode(&mut self, mode: DatePickerMode) {
        self.mode = mode;
    }

    /// [`Self::set_mode`] for the builder chain.
    #[must_use]
    pub const fn with_mode(mut self, mode: DatePickerMode) -> Self {
        self.set_mode(mode);
        self
    }

    /// The name the value is submitted under in a
    /// [`crate::widgets::form::Form`].
    pub fn set_name(&mut self, name: AzString) {
        self.name = Some(name).into();
    }

    /// [`Self::set_name`] for the builder chain.
    #[must_use]
    pub fn with_name(mut self, name: AzString) -> Self {
        self.set_name(name);
        self
    }

    /// The container CSS this date picker renders with in the flat look.
    ///
    /// `None` means no opinion, so the widget's default applies (the flora
    /// look draws its own calendar leaf instead); a caller's style is used
    /// as it is in every look.
    #[must_use]
    pub fn resolved_container_style(&self) -> CssPropertyWithConditionsVec {
        self.container_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_const_slice(CONTAINER_STYLE))
    }

    /// Sets the callback invoked when the selection or month changes.
    pub fn set_on_change<C: Into<DatePickerOnChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.state.on_change = Some(DatePickerOnChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder variant of [`Self::set_on_change`].
    #[must_use]
    pub fn with_on_change<C: Into<DatePickerOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// Replaces `self` with the default value and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(2000, 1, 1);
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this picker into its DOM: the field, with the calendar as its
    /// (closed) popup. The look comes from the theme module
    /// (`themes::flat::date_picker` / `themes::flora::date_picker`); `None`
    /// renders flat.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::date_picker(self),
            Some(UiTheme::Flat) | None => crate::widgets::themes::flat::date_picker(self),
        }
    }
}

/// The date picker's DOM in `look`: the field (value, glyph, popup) and the
/// calendar in the popup, structured as the handlers walk it.
pub(crate) fn build(picker: DatePicker, look: &DatePickerLook) -> Dom {
    {
        let inner = picker.state.inner;
        let year = inner.year;
        let month = inner.month.clamp(1, 12);
        let sel_day = inner.day;
        // A caller's `container_style` owns the panel; otherwise the theme's.
        let container_style = match picker.container_style.clone().into_option() {
            Some(own) => own,
            None => CssPropertyWithConditionsVec::from_vec(look.panel.clone()),
        };
        let a11y_name = picker.accessibility_name.clone();
        let name = picker.name.clone();
        let mode = picker.mode;
        let value = format_value(&inner, mode);

        let shared = RefAny::new(DatePickerData {
            state: picker.state,
            open: false,
            mode,
        });

        // One popup, the calendar each mode needs: the twelve months of the
        // year for `month`, the day grid (Monday-first, whole week lit) for
        // `week`, the day grid for `date`.
        let sections = match mode {
            DatePickerMode::Month => alloc::vec![
                build_year_header(year, shared.clone(), look),
                build_month_grid(year, month, shared.clone(), look),
            ],
            DatePickerMode::Week => alloc::vec![
                build_header_in(year, month, shared.clone(), look),
                build_weekday_row_from(WeekStart::Monday, look),
                build_grid_with(
                    year,
                    month,
                    sel_day,
                    shared.clone(),
                    WeekStart::Monday,
                    true,
                    look
                ),
            ],
            DatePickerMode::Date => alloc::vec![
                build_header_in(year, month, shared.clone(), look),
                build_weekday_row_from(WeekStart::Sunday, look),
                build_grid_with(
                    year,
                    month,
                    sel_day,
                    shared.clone(),
                    WeekStart::Sunday,
                    false,
                    look
                ),
            ],
        };

        // The calendar is the body of a REAL OS popup anchored under the field,
        // the same shape `color_input.rs` uses for its colour panel. As an
        // always-visible inline block it took a calendar's worth of space in the
        // page and could not overlap anything below it; a popup window has no
        // such ceiling, and a closed `<transient-window>` costs nothing inline.
        let panel = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(DATE_PICKER_PANEL_CLASS))
            .with_css_props(container_style)
            .with_children(sections.into());

        let mut transient = NodeData::create_node(NodeType::TransientWindow(
            TransientWindowConfig::closed()
                .with_anchor(TransientAnchor::Bottom)
                .with_dismiss(TransientDismiss::Outside),
        ));
        transient.add_callback(
            EventFilter::Component(ComponentEventFilter::Dismissed),
            shared.clone(),
            Callback::from_ptr(on_date_picker_dismissed).to_core(),
        );
        let popup = Dom::create_from_data(transient).with_child(panel);

        let mut classes: Vec<IdOrClass> = DATE_PICKER_CLASS.to_vec();
        if let Some(marker) = look.marker {
            classes.push(Class(AzString::from_const_str(marker)));
        }

        // The field: what the user sees when the calendar is shut. It carries
        // the shared state as its dataset - how a `Form` reads the picked value
        // - and, for the modes that are not a plain date, the HTML `type`.
        let mut field = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(look.field.clone()))
            .with_tab_index(TabIndex::Auto)
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::ComboBox,
                accessibility_name: a11y_name,
                accessibility_value: Some(AzString::from(value.clone())).into(),
                ..Default::default()
            })
            .with_dataset(Some(shared.clone()).into())
            .with_callbacks(
                alloc::vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    refany: shared,
                    callback: CoreCallback {
                        cb: on_date_field_toggle as usize,
                        ctx: OptionRefAny::None,
                    },
                }]
                .into(),
            )
            .with_children(
                alloc::vec![
                    crate::widgets::widget_p_with_text(AzString::from(value))
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(
                            DATE_PICKER_VALUE_CLASS
                        ))
                        .with_css_props(CssPropertyWithConditionsVec::from_vec(
                            look.field_value.clone()
                        )),
                    Dom::create_icon(AzString::from_const_str("calendar_today")).with_css_props(
                        CssPropertyWithConditionsVec::from_vec(look.field_icon.clone())
                    ),
                    popup,
                ]
                .into(),
            );
        if mode != DatePickerMode::Date {
            field = field.with_attribute(azul_core::dom::AttributeType::InputType(
                AzString::from_const_str(mode.html_type()),
            ));
        }
        if let Some(name) = name.into_option() {
            field = field.with_attribute(azul_core::dom::AttributeType::Name(name));
        }
        field
    }
}

/// Which weekday a calendar row starts on: the date grid is Sunday-first (as
/// it always was), the ISO week grid Monday-first so one row IS one week.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum WeekStart {
    Sunday,
    Monday,
}

impl WeekStart {
    /// Blank cells before the 1st of `year`/`month` in a row starting on this
    /// day.
    fn leading_blanks(self, year: u32, month: u32) -> u32 {
        let sunday_based = weekday(year, month, 1);
        match self {
            Self::Sunday => sunday_based,
            Self::Monday => (sunday_based + 6) % 7,
        }
    }
}

impl Default for DatePicker {
    fn default() -> Self {
        Self::create(2000, 1, 1)
    }
}

/// [`build_header_in`] in the established look (what the tests build).
#[cfg(test)]
fn build_header(year: u32, month: u32, shared: RefAny) -> Dom {
    build_header_in(year, month, shared, &DatePickerLook::established())
}

/// The day grid's header: previous arrow / `Month YYYY` / next arrow,
/// turning the MONTH.
fn build_header_in(year: u32, month: u32, shared: RefAny, look: &DatePickerLook) -> Dom {
    let label = AzString::from(format!("{} {}", month_name(month), year));

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(look.header.clone()))
        .with_children(
            alloc::vec![
                header_nav_button(
                    PREV_ARROW,
                    "Previous month",
                    on_prev_month as usize,
                    shared.clone(),
                    look
                ),
                crate::widgets::widget_p_with_text(label)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_LABEL_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(
                        look.header_label.clone(),
                    )),
                header_nav_button(NEXT_ARROW, "Next month", on_next_month as usize, shared, look),
            ]
            .into(),
        )
}

/// The Sunday-first weekday header of the date grid, in the established look.
#[cfg(test)]
fn build_weekday_row() -> Dom {
    build_weekday_row_from(WeekStart::Sunday, &DatePickerLook::established())
}

/// The weekday header, starting on `start`.
fn build_weekday_row_from(start: WeekStart, look: &DatePickerLook) -> Dom {
    let offset = match start {
        WeekStart::Sunday => 0,
        WeekStart::Monday => 1,
    };
    let cells: Vec<Dom> = WEEKDAY_NAMES
        .iter()
        .cycle()
        .skip(offset)
        .take(7)
        .map(|n| {
            crate::widgets::widget_p_with_text(n.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEKDAY_CELL_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(look.weekday.clone()))
        })
        .collect();

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEKDAY_ROW_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(look.row.clone()))
        .with_children(cells.into())
}

/// The Sunday-first day grid of a date picker, in the established look.
#[cfg(test)]
fn build_grid(year: u32, month: u32, sel_day: u32, shared: RefAny) -> Dom {
    build_grid_with(
        year,
        month,
        sel_day,
        shared,
        WeekStart::Sunday,
        false,
        &DatePickerLook::established(),
    )
}

/// The day grid, rows starting on `start`. With `whole_week`, every day in
/// the selected day's ROW is highlighted - the week picker's selection.
#[allow(clippy::needless_pass_by_value)] // shared RefAny handle cloned per day cell
fn build_grid_with(
    year: u32,
    month: u32,
    sel_day: u32,
    shared: RefAny,
    start: WeekStart,
    whole_week: bool,
    look: &DatePickerLook,
) -> Dom {
    let leading = start.leading_blanks(year, month);
    let dim = days_in_month(year, month);
    let row_of = |day: u32| (leading + day - 1) / 7;
    let is_selected = |day: u32| {
        if whole_week {
            (1..=dim).contains(&sel_day) && row_of(day) == row_of(sel_day)
        } else {
            day == sel_day
        }
    };
    let total = leading + dim;
    let rows = total.div_ceil(7);
    // WAI-ARIA APG date grid: the days are ONE Tab stop - the selected day,
    // or the 1st when the selected day is not in this month. The arrow keys
    // move focus within the grid (`on_day_key`).
    let stop_day = if (1..=dim).contains(&sel_day) {
        sel_day
    } else {
        1
    };

    let mut week_rows: Vec<Dom> = Vec::with_capacity(rows as usize);
    for r in 0..rows {
        let mut cells: Vec<Dom> = Vec::with_capacity(7);
        for c in 0..7 {
            let i = r * 7 + c;
            if i < leading || i >= leading + dim {
                cells.push(build_blank_cell_in(look));
            } else {
                let day = i - leading + 1;
                let tab_index = if day == stop_day {
                    TabIndex::Auto
                } else {
                    TabIndex::NoKeyboardFocus
                };
                cells.push(
                    build_day_cell_in(day, is_selected(day), shared.clone(), look)
                        .with_accessibility_name(day_accessibility_name(year, month, day))
                        .with_tab_index(tab_index),
                );
            }
        }
        week_rows.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEK_ROW_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(look.row.clone()))
                .with_children(cells.into()),
        );
    }

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(GRID_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(look.grid.clone()))
        .with_children(week_rows.into())
}

// ---------------------------------------------------------------------------
// type=month: the year over a 4 x 3 grid of its months
// ---------------------------------------------------------------------------

/// Columns of the month grid; four rows of three fill the day grid's width.
const MONTH_GRID_COLUMNS: usize = 3;
/// A month cell is a third of the seven-day grid's width.
const MONTH_CELL_W: isize = CELL_W * 7 / 3;
/// The class string of [`HEADER_CLASS`], for finding the header from a handler.
const HEADER_CLASS_NAME: &str = "__azul-native-date-picker-header";

/// Per-month-cell payload, the month twin of `DayCellData`.
struct MonthCellData {
    month: u32,
    state: RefAny,
    /// The theme's repaint colours (see `DayCellData::palette`).
    palette: DayPalette,
}

/// A header button (the previous / next arrow) that says what it does - its
/// glyph is not a name.
fn header_nav_button(
    arrow: AzString,
    name: &'static str,
    cb: usize,
    refany: RefAny,
    look: &DatePickerLook,
) -> Dom {
    crate::widgets::widget_p_with_text(arrow)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(NAV_BTN_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(look.nav.clone()))
        .with_callbacks(
            alloc::vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb,
                    ctx: OptionRefAny::None,
                },
                refany,
            }]
            .into(),
        )
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::PushButton,
            accessibility_name: Some(AzString::from_const_str(name)).into(),
            ..Default::default()
        })
}

/// The month calendar's header: previous arrow / `YYYY` / next arrow, turning
/// the YEAR.
fn build_year_header(year: u32, shared: RefAny, look: &DatePickerLook) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(look.header.clone()))
        .with_children(
            alloc::vec![
                header_nav_button(
                    PREV_ARROW,
                    "Previous year",
                    on_prev_year as usize,
                    shared.clone(),
                    look
                ),
                crate::widgets::widget_p_with_text(AzString::from(alloc::format!("{year}")))
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_LABEL_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(
                        look.header_label.clone(),
                    )),
                header_nav_button(NEXT_ARROW, "Next year", on_next_year as usize, shared, look),
            ]
            .into(),
        )
}

/// The twelve months, four rows of three. Unlike the day grid it is the same
/// for every year, so turning the year needs no rebuild - only a new header.
/// The selected month is highlighted and is the grid's one Tab stop.
#[allow(clippy::needless_pass_by_value)] // shared RefAny handle cloned per month cell
fn build_month_grid(year: u32, sel_month: u32, shared: RefAny, look: &DatePickerLook) -> Dom {
    let rows: Vec<Dom> = MONTH_ABBREVIATIONS
        .chunks(MONTH_GRID_COLUMNS)
        .enumerate()
        .map(|(r, names)| {
            let cells: Vec<Dom> = names
                .iter()
                .enumerate()
                .map(|(c, abbreviation)| {
                    let month = u32::try_from(r * MONTH_GRID_COLUMNS + c + 1).unwrap_or(1);
                    let selected = month == sel_month;
                    build_month_cell(year, month, abbreviation, selected, shared.clone(), look)
                        .with_tab_index(if selected {
                            TabIndex::Auto
                        } else {
                            TabIndex::NoKeyboardFocus
                        })
                })
                .collect();
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEK_ROW_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(look.row.clone()))
                .with_children(cells.into())
        })
        .collect();

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(GRID_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(look.grid.clone()))
        .with_children(rows.into())
}

/// One month of the grid: the look's day-cell face, three day-cells wide.
fn build_month_cell(
    year: u32,
    month: u32,
    abbreviation: &str,
    selected: bool,
    shared: RefAny,
    look: &DatePickerLook,
) -> Dom {
    use azul_core::events::FocusEventFilter;

    let data = RefAny::new(MonthCellData {
        month,
        state: shared,
        palette: look.day_palette.clone(),
    });
    let mut style = if selected {
        look.day_selected.clone()
    } else {
        look.day_other.clone()
    };
    style.push(CssPropertyWithConditions::simple(CssProperty::const_width(
        LayoutWidth::const_px(MONTH_CELL_W),
    )));
    crate::widgets::widget_p_with_text(AzString::from(abbreviation))
        .with_ids_and_classes(IdOrClassVec::from_const_slice(DAY_CELL_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
        .with_callbacks(
            alloc::vec![
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: on_month_click as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: data.clone(),
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    callback: CoreCallback {
                        cb: on_month_key as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: data,
                },
            ]
            .into(),
        )
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::PushButton,
            accessibility_name: Some(AzString::from(alloc::format!(
                "{} {year}",
                month_name(month)
            )))
            .into(),
            ..Default::default()
        })
}

/// Month-cell click: pick that month (day 1), report it, show `YYYY-MM` in
/// the field and close the calendar - the day click's steps, for a month.
extern "C" fn on_month_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let clicked = info.get_hit_node();
    let (month, mut shared, palette) = {
        let Some(cell) = data.downcast_ref::<MonthCellData>() else {
            return Update::DoNothing;
        };
        (cell.month, cell.state.clone(), cell.palette.clone())
    };

    let (update, label) = {
        let Some(mut w) = shared.downcast_mut::<DatePickerData>() else {
            return Update::DoNothing;
        };
        w.state.inner.month = month.clamp(1, 12);
        w.state.inner.day = 1;
        w.open = false;
        let inner = w.state.inner;
        let label = format_value(&inner, w.mode);
        let w = &mut w.state;
        let update = match w.on_change.as_mut() {
            Some(DatePickerOnChange { callback, refany }) => {
                callback.invoke(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        };
        (update, label)
    };

    restyle_days(&mut info, clicked, &palette);
    close_calendar_showing(&mut info, clicked, label);
    update
}

/// Arrow keys in the month grid - the day grid's keyboard model (one Tab
/// stop, arrows move it, Enter/Space pick, PageUp/PageDown turn the year)
/// over three columns instead of seven.
extern "C" fn on_month_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    use crate::widgets::roving;

    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    if !matches!(
        key,
        K::Left | K::Right | K::Up | K::Down | K::Home | K::End | K::PageUp | K::PageDown
    ) {
        return Update::DoNothing;
    }
    let shared = {
        let Some(cell) = data.downcast_ref::<MonthCellData>() else {
            return Update::DoNothing;
        };
        cell.state.clone()
    };
    match key {
        K::PageUp => {
            info.prevent_default();
            return year_nav(shared, info, -1);
        }
        K::PageDown => {
            info.prevent_default();
            return year_nav(shared, info, 1);
        }
        _ => {}
    }

    let focused = info.get_hit_node();
    let Some(grid) = info.get_parent(focused).and_then(|row| info.get_parent(row)) else {
        return Update::DoNothing;
    };
    let mut cells: Vec<azul_core::dom::DomNodeId> = Vec::new();
    let mut row = info.get_first_child(grid);
    while let Some(r) = row {
        let mut cell = info.get_first_child(r);
        while let Some(c) = cell {
            cells.push(c);
            cell = info.get_next_sibling(c);
        }
        row = info.get_next_sibling(r);
    }
    let Some(current) = cells.iter().position(|n| *n == focused) else {
        return Update::DoNothing;
    };
    let row_start = current - current % MONTH_GRID_COLUMNS;
    let target = match key {
        K::Left => current.checked_sub(1),
        K::Right => Some(current + 1).filter(|t| *t < cells.len()),
        K::Up => current.checked_sub(MONTH_GRID_COLUMNS),
        K::Down => Some(current + MONTH_GRID_COLUMNS).filter(|t| *t < cells.len()),
        K::Home => Some(row_start),
        K::End => Some(row_start + MONTH_GRID_COLUMNS - 1).filter(|t| *t < cells.len()),
        _ => None,
    };

    // The key is the grid's either way: spatial navigation must not walk out
    // of the calendar from its edge.
    info.prevent_default();
    let Some(target) = target.filter(|t| *t != current) else {
        return Update::DoNothing;
    };
    roving::move_stop(&mut info, &cells, target);
    Update::DoNothing
}

extern "C" fn on_prev_year(data: RefAny, info: CallbackInfo) -> Update {
    year_nav(data, info, -1)
}

extern "C" fn on_next_year(data: RefAny, info: CallbackInfo) -> Update {
    year_nav(data, info, 1)
}

/// Turn the month picker's year by `delta`, report it and re-title the
/// header. The month grid is the same in every year, so - unlike the day
/// grid's month navigation - nothing else needs rebuilding.
fn year_nav(mut data: RefAny, mut info: CallbackInfo, delta: i32) -> Update {
    let (update, year) = {
        let Some(mut w) = data.downcast_mut::<DatePickerData>() else {
            return Update::DoNothing;
        };
        let year = (i64::from(w.state.inner.year) + i64::from(delta)).max(1);
        w.state.inner.year = u32::try_from(year).unwrap_or(1);
        let inner = w.state.inner;
        let w = &mut w.state;
        let update = match w.on_change.as_mut() {
            Some(DatePickerOnChange { callback, refany }) => {
                callback.invoke(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        };
        (update, inner.year)
    };

    // header = [prev, label <p>, next]; the label's text leaf takes the year.
    let hit = info.get_hit_node();
    let prev = year_header_of(&info, hit).and_then(|header| info.get_first_child(header));
    let label = prev.and_then(|prev| info.get_next_sibling(prev));
    let text = label.and_then(|label| info.get_first_child(label));
    if let Some(text) = text {
        info.change_node_text(text, AzString::from(alloc::format!("{year}")));
    }
    update
}

/// The calendar header above `hit` - a header button's own parent, or the
/// first child of the panel a grid cell sits in.
fn year_header_of(
    info: &CallbackInfo,
    hit: azul_core::dom::DomNodeId,
) -> Option<azul_core::dom::DomNodeId> {
    let is_header = |n: azul_core::dom::DomNodeId| {
        info.get_node_classes(n)
            .as_ref()
            .iter()
            .any(|c| c.as_str() == HEADER_CLASS_NAME)
    };
    let mut node = hit;
    for _ in 0..5 {
        if is_header(node) {
            return Some(node);
        }
        if let Some(first) = info.get_first_child(node) {
            if is_header(first) {
                return Some(first);
            }
        }
        node = info.get_parent(node)?;
    }
    None
}

/// [`build_blank_cell_in`] in the established look.
#[cfg(test)]
fn build_blank_cell() -> Dom {
    build_blank_cell_in(&DatePickerLook::established())
}

fn build_blank_cell_in(look: &DatePickerLook) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(DAY_CELL_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(look.blank.clone()))
}

/// [`build_day_cell_in`] in the established look.
#[cfg(test)]
fn build_day_cell(day: u32, selected: bool, shared: RefAny) -> Dom {
    build_day_cell_in(day, selected, shared, &DatePickerLook::established())
}

/// One day of the grid: the click picks it, the arrow keys move focus from it
/// (both share the cell's payload). The Tab index is `build_grid_with`'s to
/// set - only the grid knows which day holds its one Tab stop.
fn build_day_cell_in(day: u32, selected: bool, shared: RefAny, look: &DatePickerLook) -> Dom {
    use azul_core::{
        dom::{EventFilter, HoverEventFilter},
        events::FocusEventFilter,
    };

    let data = RefAny::new(DayCellData {
        day,
        state: shared,
        palette: look.day_palette.clone(),
    });
    let face = if selected {
        look.day_selected.clone()
    } else {
        look.day_other.clone()
    };
    crate::widgets::widget_p_with_text(AzString::from(format!("{day}")))
        .with_ids_and_classes(IdOrClassVec::from_const_slice(DAY_CELL_CLASS))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(face))
        .with_callbacks(
            alloc::vec![
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: on_day_click as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: data.clone(),
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    callback: CoreCallback {
                        cb: on_day_key as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: data,
                },
            ]
            .into(),
        )
        // A day of the date grid is a button (Enter / Space / a click picks
        // it). `build_grid` adds its full-date name: only the grid knows the
        // month and year the day belongs to.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::PushButton,
            ..Default::default()
        })
}

/// Day-cell click handler. Reads the cell's baked day, updates the shared
/// `state.day`, fires `on_change`, and live-restyles the whole grid.
extern "C" fn on_day_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let clicked = info.get_hit_node();

    // Read the baked day + clone the shared-state handle and the palette.
    let (day, mut shared, palette) = {
        let Some(cell) = data.downcast_ref::<DayCellData>() else {
            return Update::DoNothing;
        };
        (cell.day, cell.state.clone(), cell.palette.clone())
    };

    let (update, new_label, mode) = {
        let Some(mut w) = shared.downcast_mut::<DatePickerData>() else {
            return Update::DoNothing;
        };
        w.state.inner.day = day;
        w.open = false;
        let inner = w.state.inner;
        let mode = w.mode;
        let label = format_value(&inner, mode);
        let w = &mut w.state;
        let update = match w.on_change.as_mut() {
            Some(DatePickerOnChange { callback, refany }) => {
                callback.invoke(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        };
        (update, label, mode)
    };

    if mode == DatePickerMode::Week {
        restyle_week(&mut info, clicked, &palette);
    } else {
        restyle_days(&mut info, clicked, &palette);
    }
    close_calendar_showing(&mut info, clicked, new_label);
    update
}

/// Shut the calendar a pick was made in and show `label` in the FIELD.
///
/// `clicked` is the picked cell: cell -> row -> grid -> panel -> popup, the
/// same depth for the day grid and the month grid.
fn close_calendar_showing(info: &mut CallbackInfo, clicked: azul_core::dom::DomNodeId, label: String) {
    // Shut the calendar and reflect the pick in the FIELD — the popup is a real
    // window now, so it does not go away by itself on a click inside it.
    // clicked -> week row -> grid -> panel -> popup. All five exist in BOTH
    // doms: in the parent, `popup` is the `<transient-window>` node; inside the
    // popup WINDOW's extracted dom it is the popup's own root, and
    // `set_transient_window_open(false)` on it is the shell's "this popup
    // dismisses itself" (it posts `dismissed` and the parent then fires
    // `Dismissed` on the node). The FIELD only exists in the parent dom — the
    // old version nested the close under `get_parent(popup)`, so a day click
    // inside the real popup window closed NOTHING and filled NOTHING.
    if let Some(row) = info.get_parent(clicked) {
        if let Some(grid) = info.get_parent(row) {
            if let Some(panel) = info.get_parent(grid) {
                if let Some(popup) = info.get_parent(panel) {
                    info.set_transient_window_open(popup, false);
                    // Parent dom only; the popup-side pick reaches the field
                    // through `on_date_picker_dismissed` instead.
                    if let Some(field) = info.get_parent(popup) {
                        if let Some(value_p) = info.get_first_child(field) {
                            if let Some(text) = info.get_first_child(value_p) {
                                info.change_node_text(text, AzString::from(label));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Arrow keys on the focused day (WAI-ARIA APG date grid): Left/Right move
/// focus by a day, Up/Down by a week, Home/End to the first / last DAY of the
/// week row; the target becomes the grid's one Tab stop. Moving is not
/// choosing - Enter or Space (the activation default, i.e. a click) picks the
/// focused day and closes the calendar. PageUp/PageDown turn the month exactly
/// like the header's ‹ / › (see the module's TODO2: the host rebuilds the
/// grid). Every handled key is `prevent_default`-ed; an arrow past the
/// displayed month is consumed and goes nowhere, because the grid cannot
/// rebuild itself into the neighbouring month. Any key held with Alt, Ctrl,
/// Cmd or Shift, and every other key, keeps its default.
extern "C" fn on_day_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    use crate::widgets::roving;

    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    if !matches!(
        key,
        K::Left | K::Right | K::Up | K::Down | K::Home | K::End | K::PageUp | K::PageDown
    ) {
        return Update::DoNothing;
    }
    let shared = {
        let Some(cell) = data.downcast_ref::<DayCellData>() else {
            return Update::DoNothing;
        };
        cell.state.clone()
    };

    match key {
        K::PageUp => {
            info.prevent_default();
            return month_nav(shared, info, -1);
        }
        K::PageDown => {
            info.prevent_default();
            return month_nav(shared, info, 1);
        }
        _ => {}
    }

    // The day cells in reading order, with the week row each sits in. A blank
    // cell has no children; a day cell is a `<p>` around its number.
    let focused = info.get_hit_node();
    let Some(grid) = info.get_parent(focused).and_then(|week| info.get_parent(week)) else {
        return Update::DoNothing;
    };
    let mut days: Vec<(azul_core::dom::DomNodeId, usize)> = Vec::new();
    let mut row = 0usize;
    let mut week = info.get_first_child(grid);
    while let Some(w) = week {
        let mut cell = info.get_first_child(w);
        while let Some(c) = cell {
            if info.get_first_child(c).is_some() {
                days.push((c, row));
            }
            cell = info.get_next_sibling(c);
        }
        row += 1;
        week = info.get_next_sibling(w);
    }
    let Some(current) = days.iter().position(|(n, _)| *n == focused) else {
        return Update::DoNothing;
    };
    let week_of_current = days[current].1;
    let target = match key {
        K::Left => current.checked_sub(1),
        K::Right => Some(current + 1).filter(|t| *t < days.len()),
        K::Up => current.checked_sub(7),
        K::Down => Some(current + 7).filter(|t| *t < days.len()),
        K::Home => days.iter().position(|(_, r)| *r == week_of_current),
        K::End => days.iter().rposition(|(_, r)| *r == week_of_current),
        _ => None,
    };

    // The key is the grid's either way: spatial navigation must not walk out
    // of the calendar from its edge.
    info.prevent_default();
    let Some(target) = target.filter(|t| *t != current) else {
        return Update::DoNothing;
    };
    let items: Vec<azul_core::dom::DomNodeId> = days.iter().map(|(n, _)| *n).collect();
    roving::move_stop(&mut info, &items, target);
    Update::DoNothing
}

/// Accents the clicked cell and neutralises every other grid cell (blanks
/// included — for them transparent bg + a colour on empty text is a no-op).
fn restyle_days(
    info: &mut CallbackInfo,
    clicked: azul_core::dom::DomNodeId,
    palette: &DayPalette,
) {
    restyle_grid(info, clicked, false, palette);
}

/// [`restyle_days`] for the week picker: every DAY in the clicked cell's row
/// (one row is one ISO week in the Monday-first grid) takes the accent.
fn restyle_week(
    info: &mut CallbackInfo,
    clicked: azul_core::dom::DomNodeId,
    palette: &DayPalette,
) {
    restyle_grid(info, clicked, true, palette);
}

/// Accent the picked cell - or, with `whole_row`, every day cell of its row -
/// and neutralise every other cell of the grid, in the colours of the look
/// the grid was built in (`palette`). Works for the day grid and the month
/// grid alike: both are `grid > rows > cells`.
fn restyle_grid(
    info: &mut CallbackInfo,
    clicked: azul_core::dom::DomNodeId,
    whole_row: bool,
    palette: &DayPalette,
) {
    let Some(row) = info.get_parent(clicked) else {
        return;
    };
    let Some(grid) = info.get_parent(row) else {
        return;
    };
    let mode = usize::from(window_is_dark(info));

    let mut week = info.get_first_child(grid);
    while let Some(w) = week {
        let mut cellopt = info.get_first_child(w);
        while let Some(cell) = cellopt {
            // A blank cell has no text child and never takes the accent.
            let selected = if whole_row {
                w == row && info.get_first_child(cell).is_some()
            } else {
                cell == clicked
            };
            let (bg, text) = if selected {
                palette.selected[mode].clone()
            } else {
                palette.other[mode].clone()
            };
            info.set_css_property(cell, CssProperty::const_background_content(bg));
            info.set_css_property(
                cell,
                CssProperty::const_text_color(StyleTextColor { inner: text }),
            );
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

/// The field's text: `YYYY-MM-DD`, zero-padded so the field never changes width
/// as the user moves through the month (a jittering field under an anchored
/// popup would drag the popup with it).
fn format_date(state: &DatePickerState) -> String {
    alloc::format!("{:04}-{:02}-{:02}", state.year, state.month, state.day)
}

/// Field click: open or close the calendar popup.
///
/// The hit node is the field; the popup is its LAST child (value, icon, popup),
/// so `get_last_child` names it without counting siblings.
extern "C" fn on_date_field_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let field = info.get_hit_node();
    let Some(popup) = info.get_last_child(field) else {
        return Update::DoNothing;
    };

    let now_open = {
        let Some(mut w) = data.downcast_mut::<DatePickerData>() else {
            return Update::DoNothing;
        };
        w.open = !w.open;
        w.open
    };

    info.set_transient_window_open(popup, now_open);
    Update::DoNothing
}

/// The popup dismissed itself (outside click / Escape). Clear `open` so the
/// next field click opens instead of "closing" an already-closed calendar.
extern "C" fn on_date_picker_dismissed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let label = {
        let Some(mut w) = data.downcast_mut::<DatePickerData>() else {
            return Update::DoNothing;
        };
        w.open = false;
        format_value(&w.state.inner, w.mode)
    };
    // Runs in the PARENT window (the `Dismissed` event targets the
    // `<transient-window>` node there): the one place a day picked INSIDE the
    // popup window — which mutated the shared state but cannot address the
    // parent's field — reaches the field text. For an outside-click dismissal
    // the state is unchanged, so this re-writes what the field already shows
    // (`ChangeNodeText` short-circuits identical text).
    // popup (= event target) -> field -> value <p> -> text leaf.
    let popup = info.get_hit_node();
    if let Some(field) = info.get_parent(popup) {
        if let Some(value_p) = info.get_first_child(field) {
            if let Some(text) = info.get_first_child(value_p) {
                info.change_node_text(text, AzString::from(label));
            }
        }
    }
    Update::DoNothing
}

/// Month navigation. Updates month/year (wrapping across year boundaries),
/// clamps the selected day into the new month, and fires `on_change` so host
/// code can rebuild the widget. TODO2: the in-widget grid is NOT rebuilt (see
/// module docs) — only the reported state changes.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded layout/render numeric cast
fn month_nav(mut data: RefAny, info: CallbackInfo, delta: i32) -> Update {
    let Some(mut w) = data.downcast_mut::<DatePickerData>() else {
        return Update::DoNothing;
    };
    let w = &mut w.state;

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
        Some(DatePickerOnChange { callback, refany }) => callback.invoke(refany.clone(), info, inner),
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
        window::{MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::roving::test_support as rv,
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ==================================================================
    // Harness
    // ==================================================================

    /// The largest `year` `weekday()` can take before its `y + y / 4` term
    /// overflows `i32`. Sakamoto's accumulator is `i32`, so every input above
    /// this (and below `2^31`, where the `u32 -> i32` cast wraps negative and
    /// the sum becomes small again) is a debug-build overflow panic. Tests stay
    /// strictly inside the safe band; see the report for the finding.
    const MAX_SAFE_WEEKDAY_YEAR: u32 = 1_717_986_916;

    /// A `DomNodeId` in the root DOM pointing at flattened node `idx`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was
    /// hit" case. `CallbackInfo::set_css_property` *panics* on such an id, so
    /// `restyle_days` must bail out before it ever reaches one.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` carrying only a `styled_dom`: the date-picker handlers
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
            ctx: core::cell::RefCell::new(OptionRefAny::None),
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
    // Tree probes (never assume a flatten index — always look the node up)
    // ------------------------------------------------------------------

    /// The three sections a date picker always renders, in order.
    /// `(header, weekday row, grid)`, descending through the field and the
    /// `<transient-window>`.
    ///
    /// A date picker now renders `field[value, icon, <transient-window>[panel]]`
    /// — the calendar is a REAL OS popup anchored under the field, not an
    /// always-visible inline block. The three calendar sections are the panel's
    /// children.
    fn sections(dom: &Dom) -> (&Dom, &Dom, &Dom) {
        let panel = panel_of(dom);
        let c = panel.children.as_ref();
        assert_eq!(
            c.len(),
            3,
            "a date picker renders header + weekday row + grid, got {} children",
            c.len(),
        );
        (&c[0], &c[1], &c[2])
    }

    /// The `<transient-window>` node: the field's LAST child.
    fn popup_of(dom: &Dom) -> &Dom {
        let c = dom.children.as_ref();
        assert_eq!(
            c.len(),
            3,
            "the field is [value, icon, popup], got {}",
            c.len()
        );
        let popup = &c[2];
        assert!(
            matches!(popup.root.get_node_type(), NodeType::TransientWindow(_)),
            "the calendar must live in a <transient-window> so it can overlap what follows it \
             instead of taking a calendar's worth of page space",
        );
        popup
    }

    /// The calendar body inside the popup.
    fn panel_of(dom: &Dom) -> &Dom {
        let popup = popup_of(dom);
        let kids = popup.children.as_ref();
        assert_eq!(kids.len(), 1, "the popup holds exactly the calendar panel");
        &kids[0]
    }

    /// Every `SetTransientWindowOpen` a handler pushed, as `(node, open)`.
    fn transient_writes(changes: &[CallbackChange]) -> Vec<(usize, bool)> {
        changes
            .iter()
            .filter_map(|change| match change {
                CallbackChange::SetTransientWindowOpen { node, open } => {
                    Some((node.node.into_crate_internal()?.index(), *open))
                }
                _ => None,
            })
            .collect()
    }

    /// Every cell of the grid, in reading order (blanks included).
    fn grid_cells(grid: &Dom) -> Vec<&Dom> {
        grid.children
            .as_ref()
            .iter()
            .flat_map(|week| week.children.as_ref().iter())
            .collect()
    }

    /// The text a node renders, looking through the `<p>` block wrapper the
    /// label convention mandates; `None` for a non-text node (a blank cell).
    fn text_of(dom: &Dom) -> Option<String> {
        match dom.root.get_node_type() {
            NodeType::P => match dom.children.as_ref() {
                [only] => only.root.get_node_type().format(),
                _ => None,
            },
            other => other.format(),
        }
    }

    /// The day numbers of a grid in reading order; `None` marks a blank cell.
    fn day_numbers(grid: &Dom) -> Vec<Option<u32>> {
        grid_cells(grid)
            .into_iter()
            .map(|c| text_of(c).map(|t| t.parse::<u32>().expect("a day cell is not a number")))
            .collect()
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

    /// The `RefAny` the widget baked into its own shared state (carried by the
    /// nav buttons and by every day cell).
    fn shared_state(sd: &StyledDom) -> RefAny {
        for nd in sd.node_data.as_ref() {
            for cb in nd.callbacks.as_ref() {
                let matches = {
                    let mut r = cb.refany.clone();
                    let matches = r.downcast_ref::<DatePickerData>().is_some();
                    matches
                };
                if matches {
                    return cb.refany.clone();
                }
            }
        }
        panic!("the rendered date picker carries no DatePickerData");
    }

    /// `(flattened node id, the cell's own payload)` of the day cell showing `day`.
    fn day_cell(sd: &StyledDom, day: u32) -> (DomNodeId, RefAny) {
        for (i, nd) in sd.node_data.as_ref().iter().enumerate() {
            for cb in nd.callbacks.as_ref() {
                let matches = {
                    let mut r = cb.refany.clone();
                    r.downcast_ref::<DayCellData>()
                        .is_some_and(|c| c.day == day)
                };
                if matches {
                    return (node(i), cb.refany.clone());
                }
            }
        }
        panic!("the rendered grid has no cell for day {day}");
    }

    /// `(flattened node id, payload)` of the header button wired to `handler`.
    fn nav_button(sd: &StyledDom, handler: usize) -> (DomNodeId, RefAny) {
        for (i, nd) in sd.node_data.as_ref().iter().enumerate() {
            for cb in nd.callbacks.as_ref() {
                if cb.callback.cb == handler {
                    return (node(i), cb.refany.clone());
                }
            }
        }
        panic!("the rendered header has no button wired to that handler");
    }

    fn read_state(shared: &RefAny) -> DatePickerState {
        let mut s = shared.clone();
        let w = s
            .downcast_ref::<DatePickerData>()
            .expect("the widget state changed type");
        w.state.inner
    }

    /// Renders a picker and hands back its flattened DOM plus the very shared
    /// state its own handlers were wired against — nothing is re-created by
    /// hand, so a mismatch between `dom()` and the handlers cannot hide here.
    fn laid_out(picker: DatePicker) -> (StyledDom, RefAny) {
        let styled = StyledDom::create_from_dom(picker.dom());
        let shared = shared_state(&styled);
        (styled, shared)
    }

    /// One "mouse-up on `hit`" delivered to the widget's own day handler.
    fn click(
        styled_dom: StyledDom,
        payload: &RefAny,
        hit: DomNodeId,
    ) -> (Update, Vec<CallbackChange>) {
        with_info(styled_dom, hit, |info| on_day_click(payload.clone(), *info))
    }

    /// Drives `times` ‹ / › presses against a hand-built state (the arithmetic in
    /// `month_nav` never touches the DOM, so an empty `StyledDom` suffices) and
    /// returns the state left behind plus the last verdict and all pushed changes.
    fn press_nav(
        inner: DatePickerState,
        next: bool,
        times: usize,
    ) -> (DatePickerState, Update, Vec<CallbackChange>) {
        let shared = RefAny::new(DatePickerData {
            state: DatePickerStateWrapper {
                inner,
                on_change: None.into(),
            },
            open: false,
            mode: DatePickerMode::Date,
        });
        let (update, changes) = with_info(StyledDom::default(), node(0), |info| {
            let mut last = Update::DoNothing;
            for _ in 0..times {
                last = if next {
                    on_next_month(shared.clone(), *info)
                } else {
                    on_prev_month(shared.clone(), *info)
                };
            }
            last
        });
        (read_state(&shared), update, changes)
    }

    // ------------------------------------------------------------------
    // Style probes
    // ------------------------------------------------------------------

    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    fn find<T>(
        v: &CssPropertyWithConditionsVec,
        f: impl Fn(&CssProperty) -> Option<T>,
    ) -> Option<T> {
        v.as_ref().iter().find_map(|p| f(&p.property))
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. An
    /// `em`/`%` slipping into the cell geometry would resolve against the parent
    /// font/box, so a 32x28 cell could render at any size at all — and the seven
    /// columns of a week would stop lining up with the seven weekday headers.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "date-picker geometry must be absolute px, got {:?}",
            pv.metric,
        );
        pv.number.get()
    }

    fn width_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    /// The first declared background colour of a style vec.
    fn background(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        find(v, |p| match p {
            CssProperty::BackgroundContent(b) => {
                b.get_property().and_then(|v| match v.as_ref().first() {
                    Some(StyleBackgroundContent::Color(c)) => Some(*c),
                    _ => None,
                })
            }
            _ => None,
        })
    }

    fn text_colour(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        find(v, |p| match p {
            CssProperty::TextColor(t) => t.get_property().map(|t| t.inner),
            _ => None,
        })
    }

    /// The declared background colour of a *rendered* node's inline style.
    fn rendered_background(dom: &Dom) -> Option<ColorU> {
        dom.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::BackgroundContent(b) => {
                    b.get_property().and_then(|v| match v.as_ref().first() {
                        Some(StyleBackgroundContent::Color(c)) => Some(*c),
                        _ => None,
                    })
                }
                _ => None,
            })
    }

    /// The background colours pushed onto nodes by `restyle_days`, in push order.
    fn pushed_backgrounds(changes: &[CallbackChange]) -> Vec<(NodeId, ColorU)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id,
                    properties,
                    ..
                } => {
                    let col = properties.as_ref().iter().find_map(|p| match p {
                        CssProperty::BackgroundContent(b) => {
                            b.get_property().and_then(|v| match v.as_ref().first() {
                                Some(StyleBackgroundContent::Color(c)) => Some(*c),
                                _ => None,
                            })
                        }
                        _ => None,
                    })?;
                    Some((*node_id, col))
                }
                _ => None,
            })
            .collect()
    }

    /// The text colours pushed onto nodes by `restyle_days`, in push order.
    fn pushed_text_colours(changes: &[CallbackChange]) -> Vec<(NodeId, ColorU)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id,
                    properties,
                    ..
                } => {
                    let col = properties.as_ref().iter().find_map(|p| match p {
                        CssProperty::TextColor(t) => t.get_property().map(|t| t.inner),
                        _ => None,
                    })?;
                    Some((*node_id, col))
                }
                _ => None,
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // User callbacks
    // ------------------------------------------------------------------

    /// A payload the change callback writes into. It arrives as the `data: RefAny`
    /// argument — a *shared* clone of what the test still holds — so the test can
    /// read back exactly what the widget reported, without any global state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ChangeLog {
        seen: Vec<DatePickerState>,
        payload: u32,
    }

    extern "C" fn record_change(
        mut data: RefAny,
        _info: CallbackInfo,
        state: DatePickerState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ChangeLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn change_do_nothing(
        _data: RefAny,
        _info: CallbackInfo,
        _state: DatePickerState,
    ) -> Update {
        Update::DoNothing
    }

    extern "C" fn change_refresh_all(
        _data: RefAny,
        _info: CallbackInfo,
        _state: DatePickerState,
    ) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in,
    /// which the `From<Callback>` arm *transmutes* into the 3-arg date-picker
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
    // is_leap
    // ==================================================================

    #[test]
    fn is_leap_applies_all_three_gregorian_rules_including_the_century_exceptions() {
        // The century rule is where naive `% 4` implementations break: 1700/1800/1900
        // are *not* leap years, 1600/2000/2400 are. Getting this wrong silently
        // shifts every date after February by a day.
        for (year, expected) in [
            (1, false),
            (4, true),
            (100, false),
            (400, true),
            (1600, true),
            (1700, false),
            (1800, false),
            (1900, false),
            (2000, true),
            (2023, false),
            (2024, true),
            (2100, false),
            (2400, true),
        ] {
            assert_eq!(is_leap(year), expected, "is_leap({year}) is wrong");
        }
    }

    #[test]
    fn is_leap_is_total_at_the_boundaries_of_u32() {
        // Year 0 satisfies `% 400 == 0`, so the rule says leap; u32::MAX is
        // 4294967295, which is neither `% 4` nor `% 400`. Both must answer without
        // panicking rather than being treated as unreachable.
        assert!(is_leap(0), "year 0 is divisible by 400 and must be leap");
        assert!(
            !is_leap(u32::MAX),
            "u32::MAX (4294967295) is not divisible by 4",
        );
        for year in (u32::MAX - 500)..=u32::MAX {
            // Nothing to compare against here — the point is that no input in the
            // top of the range panics or diverges.
            let _ = is_leap(year);
        }
    }

    #[test]
    fn is_leap_repeats_with_the_400_year_gregorian_cycle() {
        // The whole calendar is periodic mod 400. If any of the three rules is
        // mis-ordered, some year in 0..400 breaks the periodicity.
        for year in 0..1200u32 {
            assert_eq!(
                is_leap(year),
                is_leap(year + 400),
                "the leap rule is not 400-periodic at year {year}",
            );
        }
        // ... and exactly 97 of every 400 years are leap.
        let leaps = (0..400u32).filter(|y| is_leap(*y)).count();
        assert_eq!(
            leaps, 97,
            "a 400-year cycle must contain exactly 97 leap years"
        );
    }

    #[test]
    fn is_leap_agrees_with_the_length_of_february() {
        // The two functions encode the same rule twice; a divergence means the grid
        // would show a 28-day February in a year the header calls a leap year.
        for year in 0..800u32 {
            assert_eq!(
                is_leap(year),
                days_in_month(year, 2) == 29,
                "is_leap and days_in_month disagree about February {year}",
            );
        }
    }

    // ==================================================================
    // days_in_month
    // ==================================================================

    #[test]
    fn days_in_month_returns_the_calendar_length_of_every_real_month() {
        let expected = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        for (i, want) in expected.iter().enumerate() {
            let month = u32::try_from(i).unwrap() + 1;
            assert_eq!(
                days_in_month(2023, month),
                *want,
                "month {month} of the non-leap year 2023 has the wrong length",
            );
        }
    }

    #[test]
    fn days_in_month_february_tracks_the_leap_rule_at_the_extremes() {
        for (year, want) in [
            (0u32, 29), // divisible by 400
            (1900, 28), // century, not divisible by 400
            (2000, 29), // divisible by 400
            (2023, 28),
            (2024, 29),
            (u32::MAX, 28), // 4294967295 % 4 == 3
        ] {
            assert_eq!(
                days_in_month(year, 2),
                want,
                "February {year} has the wrong length",
            );
        }
    }

    #[test]
    fn days_in_month_falls_back_to_thirty_outside_one_to_twelve_without_panicking() {
        // Documented defensive arm: callers clamp the month, but a raw state field
        // can still carry 0 or 13. A panic here would take down the whole frame.
        for month in [0u32, 13, 14, 99, 1000, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            assert_eq!(
                days_in_month(2024, month),
                30,
                "out-of-range month {month} did not take the 30-day fallback",
            );
        }
    }

    #[test]
    fn days_in_month_never_leaves_the_28_to_31_band_for_any_input() {
        // A grid is sized from this number; anything outside 28..=31 would either
        // truncate the month or allocate week rows that render as dead space.
        for year in [0u32, 1, 1900, 2000, 2023, 2024, u32::MAX - 1, u32::MAX] {
            for month in [0u32, 1, 2, 6, 12, 13, u32::MAX] {
                let dim = days_in_month(year, month);
                assert!(
                    (28..=31).contains(&dim),
                    "days_in_month({year}, {month}) = {dim} is outside 28..=31",
                );
            }
        }
    }

    #[test]
    fn days_in_month_sums_to_a_real_year() {
        for year in [1900u32, 1999, 2000, 2023, 2024, 2100, 2400] {
            let total: u32 = (1..=12).map(|m| days_in_month(year, m)).sum();
            let want = if is_leap(year) { 366 } else { 365 };
            assert_eq!(
                total, want,
                "the twelve months of {year} do not add up to a year"
            );
        }
    }

    // ==================================================================
    // weekday
    // ==================================================================

    #[test]
    fn weekday_matches_independently_known_dates() {
        for (y, m, d, want) in [
            (1900u32, 1u32, 1u32, 1u32), // Monday
            (1970, 1, 1, 4),             // Thursday (the unix epoch)
            (2000, 1, 1, 6),             // Saturday
            (2000, 3, 1, 3),             // Wednesday (across a leap February)
            (2024, 2, 29, 4),            // Thursday (a leap day)
            (2024, 12, 31, 2),           // Tuesday
        ] {
            assert_eq!(
                weekday(y, m, d),
                want,
                "weekday({y}, {m}, {d}) disagrees with the real calendar",
            );
        }
    }

    #[test]
    fn weekday_is_always_a_valid_index_into_the_seven_weekday_names() {
        // `build_grid` uses this as a count of leading blank cells; a value >= 7
        // would push the 1st of the month into a second row and desynchronise the
        // whole grid from the `Su..Sa` header.
        //
        // The years here are all small: `day` and `year` share one i32 accumulator,
        // so a huge year *and* a huge day together would overflow it. The saturated
        // year is covered on its own below.
        for year in [0u32, 1, 1899, 1900, 2000, 2024] {
            for month in [0u32, 1, 2, 3, 12, 13, 100, u32::MAX] {
                for day in [0u32, 1, 28, 31, 32, 999, 1_000_000_000, u32::MAX] {
                    let w = weekday(year, month, day);
                    assert!(
                        w < 7,
                        "weekday({year}, {month}, {day}) = {w} is not a weekday index",
                    );
                }
            }
        }
    }

    #[test]
    fn weekday_survives_a_saturated_year_by_wrapping_into_the_negative_range() {
        // `year as i32` turns u32::MAX into -1, so the accumulator stays tiny and no
        // overflow occurs. This is the input `DatePicker::create(u32::MAX, ..)` feeds
        // through `dom()`, so it must not panic.
        for month in 0..=13u32 {
            let w = weekday(u32::MAX, month, 1);
            assert!(
                w < 7,
                "weekday(u32::MAX, {month}, 1) = {w} is not a weekday index"
            );
        }
        assert!(weekday(u32::MAX, 12, u32::MAX) < 7);
        assert!(weekday(MAX_SAFE_WEEKDAY_YEAR, 12, 1) < 7);
    }

    #[test]
    fn weekday_advances_by_exactly_one_per_day_within_a_month() {
        for (year, month) in [(2024u32, 1u32), (2024, 2), (2023, 2), (2000, 12), (1900, 6)] {
            let dim = days_in_month(year, month);
            for day in 1..dim {
                assert_eq!(
                    weekday(year, month, day + 1),
                    (weekday(year, month, day) + 1) % 7,
                    "{year}-{month}: the weekday jumps between day {day} and {}",
                    day + 1,
                );
            }
        }
    }

    #[test]
    fn weekday_of_the_first_chains_across_every_month_of_three_decades() {
        // The strongest available cross-check: the 1st of the next month must be
        // exactly `days_in_month` weekdays after the 1st of this one — across leap
        // Februaries, century non-leap years and year boundaries alike. If the
        // `month < 3` year shift or the `T` table were off by one anywhere, this
        // chain breaks.
        for year in 1995..2025u32 {
            for month in 1..=12u32 {
                let (ny, nm) = if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                };
                assert_eq!(
                    weekday(ny, nm, 1),
                    (weekday(year, month, 1) + days_in_month(year, month)) % 7,
                    "the calendar breaks between {year}-{month} and {ny}-{nm}",
                );
            }
        }
    }

    #[test]
    fn weekday_treats_day_zero_as_the_last_day_of_the_previous_month() {
        // `day` is not range-checked, it is simply summed. Day 0 is therefore the
        // day before the 1st — a consistent extrapolation rather than a panic.
        assert_eq!(
            weekday(2000, 1, 0),
            weekday(1999, 12, 31),
            "day 0 of January is not the same weekday as the preceding 31 December",
        );
        assert_eq!(weekday(2024, 3, 0), weekday(2024, 2, 29));
    }

    #[test]
    fn weekday_collapses_every_month_above_twelve_onto_one_behaviour() {
        // The `idx` fallback picks `T[0]` for any out-of-range month, and the
        // `month < 3` shift never fires above 12 — so 13, 99 and u32::MAX are
        // indistinguishable. Deterministic, which is what the grid needs.
        for day in [0u32, 1, 15, 31] {
            let thirteen = weekday(2024, 13, day);
            for month in [14u32, 99, 1000, u32::MAX] {
                assert_eq!(
                    weekday(2024, month, day),
                    thirteen,
                    "month {month} is not handled like every other out-of-range month",
                );
            }
        }
        // Month 0 *does* take the `< 3` shift, so it behaves exactly like January.
        for day in [0u32, 1, 15, 31] {
            assert_eq!(
                weekday(2024, 0, day),
                weekday(2024, 1, day),
                "month 0 is not handled like January",
            );
        }
    }

    // ==================================================================
    // month_name
    // ==================================================================

    #[test]
    fn month_name_maps_each_real_month_to_a_distinct_english_name() {
        let expected = [
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
        for (i, want) in expected.iter().enumerate() {
            let month = u32::try_from(i).unwrap() + 1;
            assert_eq!(month_name(month), *want, "month {month} has the wrong name");
        }
        let mut names: Vec<&str> = (1..=12).map(month_name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 12, "two months share a name");
    }

    #[test]
    fn month_name_returns_empty_above_december_but_january_at_zero() {
        // `saturating_sub(1)` maps 0 -> index 0, so month 0 silently renders as
        // "January" instead of the empty string the >12 arm produces. `dom()` clamps
        // the month first, so this only shows through a direct `build_header` call —
        // but it *is* the behaviour, and it is not symmetric with the upper bound.
        assert_eq!(
            month_name(0),
            "January",
            "the zero month stopped saturating to January"
        );
        for month in [13u32, 14, 99, 1000, u32::MAX / 2, u32::MAX - 1, u32::MAX] {
            assert_eq!(
                month_name(month),
                "",
                "out-of-range month {month} produced a name",
            );
        }
    }

    #[test]
    fn month_name_is_total_and_only_names_the_first_thirteen_indices() {
        for month in 0..2000u32 {
            let name = month_name(month);
            assert_eq!(
                !name.is_empty(),
                month <= 12,
                "month {month} named {name:?} outside the 0..=12 window",
            );
        }
    }

    // ==================================================================
    // build_day_cell_style
    // ==================================================================

    #[test]
    fn build_day_cell_style_differs_between_the_two_states_only_in_colour() {
        // A selected cell that also changed size would reflow the entire week row
        // on every click — the accent must be purely a repaint.
        let sel = build_day_cell_style(true);
        let plain = build_day_cell_style(false);
        let a = properties(&sel);
        let b = properties(&plain);

        assert_eq!(
            a.len(),
            b.len(),
            "the selected and unselected cell styles declare a different number of properties",
        );
        let differing: Vec<_> = a
            .iter()
            .zip(b.iter())
            .filter(|(x, y)| x != y)
            .map(|(x, _)| core::mem::discriminant(x))
            .collect();
        assert_eq!(
            differing,
            vec![
                core::mem::discriminant(&CssProperty::const_background_content(WHITE_BG_VEC)),
                core::mem::discriminant(&CssProperty::const_text_color(StyleTextColor {
                    inner: WHITE
                })),
            ],
            "the two day-cell styles differ in something other than background + text colour",
        );
    }

    #[test]
    fn build_day_cell_style_paints_the_selection_accent_on_white_and_the_rest_transparent() {
        // Swapping these two branches yields a picker where every day *but* the
        // selected one is highlighted — which still type-checks and still renders.
        assert_eq!(background(&build_day_cell_style(true)), Some(ACCENT_BG));
        assert_eq!(text_colour(&build_day_cell_style(true)), Some(WHITE));
        assert_eq!(background(&build_day_cell_style(false)), Some(TRANSPARENT));
        assert_eq!(text_colour(&build_day_cell_style(false)), Some(TEXT_COLOR));
    }

    #[test]
    fn build_day_cell_style_keeps_the_cell_geometry_identical_and_absolute() {
        // The seven columns of a week must line up with the seven weekday headers,
        // which are also CELL_W wide — so both states have to declare the same
        // absolute px box as `WEEKDAY_CELL_STYLE` and `BLANK_CELL_STYLE`.
        let blanks = CssPropertyWithConditionsVec::from_const_slice(BLANK_CELL_STYLE);
        let headers = CssPropertyWithConditionsVec::from_const_slice(WEEKDAY_CELL_STYLE);
        let want_w = Some(CELL_W as f32);
        let want_h = Some(CELL_H as f32);

        for selected in [false, true] {
            let v = build_day_cell_style(selected);
            assert_eq!(
                width_px(&v),
                want_w,
                "selected={selected}: wrong cell width"
            );
            assert_eq!(
                height_px(&v),
                want_h,
                "selected={selected}: wrong cell height"
            );
        }
        assert_eq!(
            width_px(&blanks),
            want_w,
            "a blank cell is not a full column wide"
        );
        assert_eq!(
            height_px(&blanks),
            want_h,
            "a blank cell is not a full row tall"
        );
        assert_eq!(
            width_px(&headers),
            want_w,
            "the weekday header column is not the same width as a day cell",
        );
    }

    #[test]
    fn build_day_cell_style_is_pure() {
        for selected in [false, true] {
            assert_eq!(
                properties(&build_day_cell_style(selected)),
                properties(&build_day_cell_style(selected)),
                "selected={selected}: two identical calls produced different styles",
            );
        }
    }

    // ==================================================================
    // DatePicker::create
    // ==================================================================

    #[test]
    fn create_stores_the_date_it_was_given_and_installs_no_callback() {
        let p = DatePicker::create(2024, 7, 4);
        assert_eq!(
            p.state.inner,
            DatePickerState {
                year: 2024,
                month: 7,
                day: 4
            },
        );
        assert!(
            p.state.on_change.as_ref().is_none(),
            "create invented a change callback out of nowhere",
        );
    }

    #[test]
    fn create_clamps_the_month_into_one_to_twelve() {
        assert_eq!(DatePicker::create(2024, 0, 1).state.inner.month, 1);
        assert_eq!(DatePicker::create(2024, 13, 1).state.inner.month, 12);
        assert_eq!(DatePicker::create(2024, u32::MAX, 1).state.inner.month, 12);
        // ... and leaves the real ones alone.
        for month in 1..=12u32 {
            assert_eq!(DatePicker::create(2024, month, 1).state.inner.month, month);
        }
    }

    #[test]
    fn create_clamps_the_day_into_the_real_length_of_the_month() {
        // The documented contract, and the one that matters: a picker that stored
        // "31 February" would render a selection highlight on a cell that does not
        // exist, i.e. on nothing at all.
        for (y, m, d, want) in [
            (2024u32, 2u32, 31u32, 29u32), // leap February
            (2023, 2, 31, 28),             // ordinary February
            (2024, 2, 30, 29),
            (2024, 4, 31, 30), // 30-day month
            (2024, 1, 31, 31), // exact fit, untouched
            (2024, 1, 0, 1),   // lower clamp
            (2024, 6, u32::MAX, 30),
            (2024, 0, 99, 31),  // month clamps to January first
            (2024, 13, 99, 31), // ... and to December
        ] {
            assert_eq!(
                DatePicker::create(y, m, d).state.inner.day,
                want,
                "create({y}, {m}, {d}) did not clamp the day correctly",
            );
        }
    }

    #[test]
    fn create_never_leaves_an_impossible_date_for_any_input() {
        for year in [0u32, 1, 1899, 1900, 2000, 2023, 2024, u32::MAX] {
            for month in [0u32, 1, 2, 11, 12, 13, u32::MAX] {
                for day in [0u32, 1, 28, 29, 30, 31, 32, u32::MAX] {
                    let s = DatePicker::create(year, month, day).state.inner;
                    assert!(
                        (1..=12).contains(&s.month),
                        "create({year}, {month}, {day}) left month {}",
                        s.month,
                    );
                    let dim = days_in_month(s.year, s.month);
                    assert!(
                        (1..=dim).contains(&s.day),
                        "create({year}, {month}, {day}) left day {} in a {dim}-day month",
                        s.day,
                    );
                }
            }
        }
    }

    #[test]
    fn create_passes_the_year_through_untouched() {
        // Only the month and the day are documented as clamped. A silent year clamp
        // would make an out-of-range year render a *different* (valid-looking) month
        // grid instead of an obviously wrong one.
        for year in [0u32, 1, u32::MAX] {
            assert_eq!(DatePicker::create(year, 1, 1).state.inner.year, year);
        }
    }

    #[test]
    fn create_is_pure_and_distinct_dates_stay_distinguishable() {
        assert_eq!(
            DatePicker::create(2024, 7, 4),
            DatePicker::create(2024, 7, 4)
        );
        assert_ne!(
            DatePicker::create(2024, 7, 4),
            DatePicker::create(2024, 7, 5)
        );
        assert_ne!(
            DatePicker::create(2024, 7, 4),
            DatePicker::create(2024, 8, 4)
        );
        assert_ne!(
            DatePicker::create(2024, 7, 4),
            DatePicker::create(2025, 7, 4)
        );
        // ... but two inputs that clamp to the same date *are* the same picker.
        assert_eq!(
            DatePicker::create(2023, 2, 31),
            DatePicker::create(2023, 2, 28)
        );
    }

    #[test]
    fn default_is_the_first_of_january_2000() {
        assert_eq!(DatePicker::default(), DatePicker::create(2000, 1, 1));
        assert_eq!(
            DatePickerState::default(),
            DatePickerState {
                year: 2000,
                month: 1,
                day: 1
            },
        );
        assert_eq!(
            DatePickerStateWrapper::default().inner,
            DatePickerState::default(),
        );
    }

    // ==================================================================
    // DatePicker::set_on_change / with_on_change
    // ==================================================================

    #[test]
    fn set_on_change_stores_the_function_pointer_and_the_payload_verbatim() {
        let mut p = DatePicker::create(2024, 1, 1);
        p.set_on_change(
            RefAny::new(0xDEAD_BEEF_u32),
            change_do_nothing as DatePickerOnChangeCallbackType,
        );

        let c = p
            .state
            .on_change
            .as_ref()
            .expect("set_on_change did not store anything");
        assert_eq!(
            c.callback.cb as *const () as usize, change_do_nothing as *const () as usize,
            "the stored function pointer is not the one that was handed in",
        );
        let mut payload = c.refany.clone();
        assert_eq!(
            *payload
                .downcast_ref::<u32>()
                .expect("the payload changed type"),
            0xDEAD_BEEF,
        );
    }

    #[test]
    fn set_on_change_replaces_rather_than_accumulates() {
        let mut p = DatePicker::create(2024, 1, 1);
        p.set_on_change(
            RefAny::new(1u8),
            change_do_nothing as DatePickerOnChangeCallbackType,
        );
        p.set_on_change(
            RefAny::new(2u8),
            change_refresh_all as DatePickerOnChangeCallbackType,
        );

        let c = p.state.on_change.as_ref().expect("the callback vanished");
        assert_eq!(
            c.callback.cb as *const () as usize, change_refresh_all as *const () as usize,
            "the second set_on_change did not win",
        );
        let mut payload = c.refany.clone();
        assert_eq!(
            *payload.downcast_ref::<u8>().expect("wrong payload type"),
            2
        );
    }

    #[test]
    fn set_on_change_does_not_disturb_the_date_or_the_container_style() {
        let before = DatePicker::create(2023, 2, 31);
        let mut after = DatePicker::create(2023, 2, 31);
        after.set_on_change(
            RefAny::new(0u8),
            change_do_nothing as DatePickerOnChangeCallbackType,
        );

        assert_eq!(
            after.state.inner, before.state.inner,
            "installing a callback moved the date"
        );
        assert_eq!(
            properties(&after.resolved_container_style()),
            properties(&before.resolved_container_style()),
            "installing a callback restyled the container",
        );
    }

    #[test]
    fn with_on_change_is_exactly_set_on_change_in_builder_form() {
        let built = DatePicker::create(2024, 5, 9).with_on_change(
            RefAny::new(7u32),
            change_do_nothing as DatePickerOnChangeCallbackType,
        );
        let mut set = DatePicker::create(2024, 5, 9);
        set.set_on_change(
            RefAny::new(7u32),
            change_do_nothing as DatePickerOnChangeCallbackType,
        );

        assert_eq!(built.state.inner, set.state.inner);
        let a = built
            .state
            .on_change
            .as_ref()
            .expect("builder dropped the callback");
        let b = set
            .state
            .on_change
            .as_ref()
            .expect("setter dropped the callback");
        assert_eq!(
            a.callback.cb as *const () as usize,
            b.callback.cb as *const () as usize
        );

        let (mut pa, mut pb) = (a.refany.clone(), b.refany.clone());
        assert_eq!(
            *pa.downcast_ref::<u32>()
                .expect("builder payload changed type"),
            *pb.downcast_ref::<u32>()
                .expect("setter payload changed type"),
        );
    }

    #[test]
    fn with_on_change_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm *transmutes* a 2-arg fn pointer into the 3-arg
        // date-picker slot — this is the FFI (Python/C) path. The pointer must come
        // out bit-identical; a mangled one would be a wild jump on the first click.
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::None,
        };
        let expected = generic_shaped as *const () as usize;

        let p = DatePicker::create(2024, 1, 1).with_on_change(RefAny::new(0u8), generic);
        let c = p
            .state
            .on_change
            .as_ref()
            .expect("the generic callback was dropped");
        assert_eq!(
            c.callback.cb as *const () as usize, expected,
            "the Callback -> DatePickerOnChangeCallback transmute mangled the pointer",
        );
    }

    // ==================================================================
    // DatePicker::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_hands_out_the_original_and_leaves_a_default_behind() {
        let mut p = DatePicker::create(2024, 7, 4);
        let taken = p.swap_with_default();

        assert_eq!(
            taken.state.inner,
            DatePickerState {
                year: 2024,
                month: 7,
                day: 4
            }
        );
        assert_eq!(
            p,
            DatePicker::default(),
            "the picker left behind is not a default one"
        );
    }

    #[test]
    fn swap_with_default_carries_the_callback_out_with_the_original() {
        // If the callback stayed behind on the *default* picker, the host would keep
        // getting change notifications from a widget it thought it had taken away.
        let mut p = DatePicker::create(2024, 7, 4).with_on_change(
            RefAny::new(3u8),
            change_do_nothing as DatePickerOnChangeCallbackType,
        );
        let taken = p.swap_with_default();

        assert!(
            taken.state.on_change.as_ref().is_some(),
            "the callback did not leave with the original"
        );
        assert!(
            p.state.on_change.as_ref().is_none(),
            "the callback stayed behind on the default"
        );
    }

    #[test]
    fn swapping_a_default_twice_is_idempotent() {
        let mut p = DatePicker::default();
        let first = p.swap_with_default();
        let second = p.swap_with_default();
        assert_eq!(first, DatePicker::default());
        assert_eq!(second, DatePicker::default());
        assert_eq!(p, DatePicker::default());
    }

    // ==================================================================
    // DatePicker::dom
    // ==================================================================

    #[test]
    fn dom_builds_a_header_a_weekday_row_and_a_grid() {
        let dom = DatePicker::create(2024, 1, 15).dom();

        assert!(matches!(dom.root.get_node_type(), NodeType::Div));
        assert_eq!(classes(&dom), vec!["__azul-native-date-picker".to_string()]);

        let (header, weekdays, grid) = sections(&dom);
        assert_eq!(
            classes(header),
            vec!["__azul-native-date-picker-header".to_string()]
        );
        assert_eq!(
            classes(weekdays),
            vec!["__azul-native-date-picker-weekdays".to_string()]
        );
        assert_eq!(
            classes(grid),
            vec!["__azul-native-date-picker-grid".to_string()]
        );
        assert_eq!(
            header.children.as_ref().len(),
            3,
            "the header must be prev / label / next",
        );
    }

    #[test]
    fn dom_labels_the_header_with_the_month_name_and_the_year() {
        for (y, m, want) in [
            (2024u32, 1u32, "January 2024"),
            (2024, 12, "December 2024"),
            (0, 6, "June 0"),
            (u32::MAX, 2, "February 4294967295"),
        ] {
            let dom = DatePicker::create(y, m, 1).dom();
            let (header, _, _) = sections(&dom);
            let kids = header.children.as_ref();
            assert_eq!(
                text_of(&kids[0]).as_deref(),
                Some("\u{2039}"),
                "wrong prev arrow"
            );
            assert_eq!(
                text_of(&kids[1]).as_deref(),
                Some(want),
                "wrong header label"
            );
            assert_eq!(
                text_of(&kids[2]).as_deref(),
                Some("\u{203A}"),
                "wrong next arrow"
            );
        }
    }

    #[test]
    fn dom_names_the_seven_weekdays_starting_at_sunday() {
        // The grid's leading blanks are counted from `weekday()`, which returns
        // 0 = Sunday. A header row starting on Monday would silently shift every
        // date in the picker by one column.
        let dom = DatePicker::create(2024, 1, 1).dom();
        let (_, weekdays, _) = sections(&dom);
        let names: Vec<Option<String>> = weekdays.children.as_ref().iter().map(text_of).collect();
        assert_eq!(
            names,
            ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
                .iter()
                .map(|s| Some((*s).to_string()))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn dom_opens_the_month_after_exactly_weekday_of_the_first_blank_cells() {
        // January 2024 starts on a Monday, February 2024 on a Thursday, and
        // February 2015 on a Sunday (no leading blanks at all).
        for (y, m, want_leading) in [
            (2024u32, 1u32, 1usize),
            (2024, 2, 4),
            (2015, 2, 0),
            (2021, 5, 6), // the widest possible offset
        ] {
            let dom = DatePicker::create(y, m, 1).dom();
            let (_, _, grid) = sections(&dom);
            let days = day_numbers(grid);
            let leading = days.iter().take_while(|d| d.is_none()).count();
            assert_eq!(
                leading, want_leading,
                "{y}-{m}: wrong number of leading blank cells",
            );
            assert_eq!(
                u32::try_from(leading).unwrap(),
                weekday(y, m, 1),
                "{y}-{m}: the leading blanks disagree with the weekday of the 1st",
            );
            assert_eq!(
                days[leading],
                Some(1),
                "{y}-{m}: the 1st is not the first day cell"
            );
        }
    }

    #[test]
    fn dom_renders_every_day_of_the_month_exactly_once_and_in_order() {
        for year in [1900u32, 2000, 2023, 2024] {
            for month in 1..=12u32 {
                let dom = DatePicker::create(year, month, 1).dom();
                let (_, _, grid) = sections(&dom);
                let days: Vec<u32> = day_numbers(grid).into_iter().flatten().collect();
                let dim = days_in_month(year, month);
                assert_eq!(
                    days,
                    (1..=dim).collect::<Vec<_>>(),
                    "{year}-{month}: the grid does not render 1..={dim} in order",
                );
            }
        }
    }

    #[test]
    fn dom_grid_rows_are_always_full_weeks_and_never_wholly_blank() {
        // `div_ceil` must produce a row count that covers offset + days without
        // allocating an entirely empty trailing week.
        for year in [1900u32, 2015, 2021, 2024] {
            for month in 1..=12u32 {
                let dom = DatePicker::create(year, month, 1).dom();
                let (_, _, grid) = sections(&dom);
                let rows = grid.children.as_ref();
                assert!(
                    (4..=6).contains(&rows.len()),
                    "{year}-{month}: {} week rows is not a possible month",
                    rows.len(),
                );
                for (i, row) in rows.iter().enumerate() {
                    assert_eq!(
                        row.children.as_ref().len(),
                        7,
                        "{year}-{month}: week row {i} is not seven columns wide",
                    );
                    assert_eq!(
                        classes(row),
                        vec!["__azul-native-date-picker-week".to_string()]
                    );
                }
                let last = &rows[rows.len() - 1];
                assert!(
                    last.children.as_ref().iter().any(|c| text_of(c).is_some()),
                    "{year}-{month}: the last week row is entirely blank",
                );
            }
        }
    }

    #[test]
    fn dom_accents_exactly_the_selected_day_and_nothing_else() {
        for day in [1u32, 15, 29] {
            let dom = DatePicker::create(2024, 2, day).dom();
            let (_, _, grid) = sections(&dom);
            let accented: Vec<Option<u32>> = grid_cells(grid)
                .into_iter()
                .filter(|c| rendered_background(c) == Some(ACCENT_BG))
                .map(|c| text_of(c).map(|t| t.parse().unwrap()))
                .collect();
            assert_eq!(
                accented,
                vec![Some(day)],
                "selecting day {day} did not highlight exactly that one cell",
            );
        }
    }

    #[test]
    fn dom_clamps_an_out_of_range_month_before_computing_the_grid() {
        // `dom()` re-clamps rather than trusting the state field, because the state
        // is `pub` and a host can write 0 or 13 into it directly. Without the clamp,
        // `month_name` and `days_in_month` would disagree about the same month.
        for (raw, want_name, want_dim) in [
            (0u32, "January", 31u32),
            (13, "December", 31),
            (u32::MAX, "December", 31),
        ] {
            let mut p = DatePicker::create(2024, 1, 1);
            p.state.inner.month = raw;
            let dom = p.dom();

            let (header, _, grid) = sections(&dom);
            assert_eq!(
                text_of(&header.children.as_ref()[1]).as_deref(),
                Some(format!("{want_name} 2024").as_str()),
                "raw month {raw} produced a nonsense header",
            );
            let days: Vec<u32> = day_numbers(grid).into_iter().flatten().collect();
            assert_eq!(
                u32::try_from(days.len()).unwrap(),
                want_dim,
                "raw month {raw} produced a grid of the wrong length",
            );
        }
    }

    #[test]
    fn dom_survives_a_saturated_or_zero_year_without_panicking() {
        // `create` does not clamp the year, so `dom()` is reachable with u32::MAX.
        // The `u32 -> i32` cast in `weekday` wraps it to -1, which is fine.
        for (y, m, d) in [
            (u32::MAX, u32::MAX, u32::MAX),
            (u32::MAX, 1, 1),
            (0, 0, 0),
            (1, 12, 31),
        ] {
            let dom = DatePicker::create(y, m, d).dom();
            let (_, _, grid) = sections(&dom);
            let days: Vec<u32> = day_numbers(grid).into_iter().flatten().collect();
            let clamped_month = m.clamp(1, 12);
            assert_eq!(
                u32::try_from(days.len()).unwrap(),
                days_in_month(y, clamped_month),
                "create({y}, {m}, {d}).dom() rendered the wrong number of days",
            );
        }
    }

    #[test]
    fn dom_gives_every_day_cell_a_mouse_up_handler_and_the_blanks_none() {
        let dom = DatePicker::create(2024, 2, 10).dom();
        let (_, _, grid) = sections(&dom);

        let mut with_handler = 0;
        for cell in grid_cells(grid) {
            assert_eq!(
                classes(cell),
                vec!["__azul-native-date-picker-day".to_string()],
                "a grid cell lost the day class",
            );
            let cbs = cell.root.callbacks.as_ref();
            if let Some(day) = text_of(cell) {
                assert_eq!(cbs.len(), 2, "a day cell registers the click and the arrow keys");
                assert_eq!(cbs[0].event, EventFilter::Hover(HoverEventFilter::Click));
                assert_eq!(cbs[0].callback.cb, on_day_click as usize);
                assert_eq!(
                    cbs[1].event,
                    EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown)
                );
                assert_eq!(cbs[1].callback.cb, on_day_key as usize);
                // ONE Tab stop per grid: the selected day (the 10th here); the
                // other days are reached with the arrow keys.
                assert_eq!(
                    cell.root.flags.get_tab_index(),
                    Some(if day == "10" {
                        TabIndex::Auto
                    } else {
                        TabIndex::NoKeyboardFocus
                    }),
                    "day {day} has the wrong tab index",
                );
                with_handler += 1;
            } else {
                assert!(cbs.is_empty(), "a blank cell registered a click handler");
            }
        }
        assert_eq!(
            with_handler,
            days_in_month(2024, 2),
            "not every day is clickable"
        );
    }

    #[test]
    fn dom_bakes_a_distinct_day_number_into_every_cell_payload() {
        // Selection must not depend on re-deriving the grid offset at click time —
        // each cell carries its own day. A shared or off-by-one payload would make
        // every click select the same (or the wrong) date.
        let styled = StyledDom::create_from_dom(DatePicker::create(2024, 2, 1).dom());
        let mut seen: Vec<u32> = Vec::new();
        for nd in styled.node_data.as_ref() {
            // One payload per cell: the click and the arrow-key handler share
            // it, so a cell is counted once.
            for cb in nd.callbacks.as_ref() {
                let mut r = cb.refany.clone();
                let day = r.downcast_ref::<DayCellData>().map(|cell| cell.day);
                if let Some(day) = day {
                    seen.push(day);
                    break;
                }
            }
        }
        seen.sort_unstable();
        assert_eq!(
            seen,
            (1..=29).collect::<Vec<u32>>(),
            "the baked day numbers are not exactly 1..=29 of February 2024",
        );
    }

    #[test]
    fn dom_keeps_its_cached_child_count_in_sync_with_the_tree() {
        // `estimated_total_children` is a cache; if it under-reports, the flatten
        // under-allocates its arenas and panics on an out-of-bounds write.
        for (y, m) in [(2024u32, 1u32), (2015, 2), (2021, 5), (u32::MAX, 12)] {
            let dom = DatePicker::create(y, m, 1).dom();
            assert_eq!(
                dom.estimated_total_children,
                descendants(&dom),
                "{y}-{m}: the cached descendant count is wrong",
            );

            // The flatten must agree with the (just-verified) descendant
            // cache: root + descendants. A per-month closed formula stopped
            // being practical once day/label cells became <p> + text leaf
            // pairs whose count varies with the month's blank cells.
            let expected = 1 + dom.estimated_total_children;
            let styled = StyledDom::create_from_dom(dom);
            assert_eq!(
                styled.node_data.as_ref().len(),
                expected,
                "{y}-{m}: the flatten disagrees with the descendant cache",
            );
        }
    }

    #[test]
    fn from_datepicker_for_dom_is_the_dom_method() {
        let via_from: Dom = DatePicker::create(2024, 3, 7).into();
        let via_dom = DatePicker::create(2024, 3, 7).dom();
        assert_eq!(classes(&via_from), classes(&via_dom));
        assert_eq!(
            via_from.children.as_ref().len(),
            via_dom.children.as_ref().len()
        );
        assert_eq!(
            via_from.estimated_total_children,
            via_dom.estimated_total_children
        );
    }

    // ==================================================================
    // build_grid / build_header / build_weekday_row / build_*_cell
    // ==================================================================

    #[test]
    fn build_grid_uses_the_thirty_day_fallback_for_an_out_of_range_month() {
        // Reachable only directly (`dom()` clamps first), but it must still produce
        // a coherent grid rather than an empty or unbounded one.
        for month in [0u32, 13, u32::MAX] {
            let grid = build_grid(
                2024,
                month,
                1,
                RefAny::new(DatePickerStateWrapper::default()),
            );
            let days: Vec<u32> = day_numbers(&grid).into_iter().flatten().collect();
            assert_eq!(
                days,
                (1..=30).collect::<Vec<_>>(),
                "month {month} did not fall back to a 30-day grid",
            );
        }
    }

    #[test]
    fn build_grid_never_highlights_more_than_one_cell() {
        // `sel_day` is not validated: 0, 32 and u32::MAX must simply match nothing
        // rather than highlighting a blank or panicking.
        for sel in [0u32, 1, 29, 30, 31, 32, u32::MAX] {
            let grid = build_grid(2024, 2, sel, RefAny::new(DatePickerStateWrapper::default()));
            let accented = grid_cells(&grid)
                .into_iter()
                .filter(|c| rendered_background(c) == Some(ACCENT_BG))
                .count();
            let want = usize::from((1..=29).contains(&sel));
            assert_eq!(
                accented, want,
                "sel_day={sel} highlighted {accented} cells in a 29-day February",
            );
        }
    }

    #[test]
    fn build_grid_is_total_across_extreme_years() {
        for year in [0u32, 1, 1900, 2024, MAX_SAFE_WEEKDAY_YEAR, u32::MAX] {
            for month in [0u32, 1, 2, 12, 13] {
                let grid = build_grid(
                    year,
                    month,
                    1,
                    RefAny::new(DatePickerStateWrapper::default()),
                );
                let rows = grid.children.as_ref();
                assert!(
                    (4..=6).contains(&rows.len()),
                    "build_grid({year}, {month}) produced {} week rows",
                    rows.len(),
                );
                for row in rows {
                    assert_eq!(row.children.as_ref().len(), 7);
                }
            }
        }
    }

    #[test]
    fn build_header_is_total_for_out_of_range_months() {
        // `month_name` yields "" above December, so the label degrades to a bare
        // year rather than panicking or indexing out of bounds.
        for (month, want) in [
            (0u32, "January 2024"),
            (12, "December 2024"),
            (13, " 2024"),
            (u32::MAX, " 2024"),
        ] {
            let header = build_header(2024, month, RefAny::new(DatePickerStateWrapper::default()));
            let kids = header.children.as_ref();
            assert_eq!(kids.len(), 3);
            assert_eq!(
                text_of(&kids[1]).as_deref(),
                Some(want),
                "build_header(2024, {month}) produced the wrong label",
            );
        }
    }

    #[test]
    fn build_header_wires_prev_and_next_to_different_handlers() {
        // A copy-paste that pointed both arrows at `on_next_month` would make the
        // month only ever move forwards — and would still render identically.
        let header = build_header(2024, 6, RefAny::new(DatePickerStateWrapper::default()));
        let kids = header.children.as_ref();
        let prev = kids[0].root.callbacks.as_ref();
        let next = kids[2].root.callbacks.as_ref();

        assert_eq!(prev.len(), 1);
        assert_eq!(next.len(), 1);
        assert_eq!(prev[0].callback.cb, on_prev_month as usize);
        assert_eq!(next[0].callback.cb, on_next_month as usize);
        assert_ne!(prev[0].callback.cb, next[0].callback.cb);
        // The label between them is inert.
        assert!(
            kids[1].root.callbacks.as_ref().is_empty(),
            "the header label is clickable"
        );
    }

    #[test]
    fn build_weekday_row_has_exactly_seven_inert_cells() {
        let row = build_weekday_row();
        let cells = row.children.as_ref();
        assert_eq!(cells.len(), 7);
        for cell in cells {
            assert!(
                cell.root.callbacks.as_ref().is_empty(),
                "a weekday header is clickable"
            );
            assert_eq!(
                classes(cell),
                vec!["__azul-native-date-picker-weekday".to_string()]
            );
        }
    }

    #[test]
    fn build_blank_cell_is_an_untexted_uncallbacked_column_placeholder() {
        let blank = build_blank_cell();
        assert!(matches!(blank.root.get_node_type(), NodeType::Div));
        assert_eq!(text_of(&blank), None, "a blank cell renders text");
        assert!(
            blank.root.callbacks.as_ref().is_empty(),
            "a blank cell is clickable"
        );
        assert!(blank.children.as_ref().is_empty());
        assert_eq!(
            classes(&blank),
            vec!["__azul-native-date-picker-day".to_string()]
        );
    }

    #[test]
    fn build_day_cell_renders_and_bakes_whatever_number_it_is_given() {
        // The day is never range-checked here — `build_grid` guarantees 1..=31 —
        // so the extremes must at least round-trip rather than panic.
        for day in [0u32, 1, 31, 99, u32::MAX] {
            for selected in [false, true] {
                let cell = build_day_cell(
                    day,
                    selected,
                    RefAny::new(DatePickerStateWrapper::default()),
                );
                assert_eq!(
                    text_of(&cell).as_deref(),
                    Some(day.to_string().as_str()),
                    "day {day} did not render as its own number",
                );
                assert_eq!(
                    rendered_background(&cell),
                    Some(if selected { ACCENT_BG } else { TRANSPARENT }),
                    "day {day} selected={selected}: wrong background",
                );

                let cbs = cell.root.callbacks.as_ref();
                assert_eq!(cbs.len(), 2, "the click and the arrow-key handler");
                assert_eq!(
                    cbs[0].refany.get_data_ptr(),
                    cbs[1].refany.get_data_ptr(),
                    "both handlers read the same cell payload",
                );
                let mut r = cbs[0].refany.clone();
                let baked = r
                    .downcast_ref::<DayCellData>()
                    .expect("a day cell no longer carries a DayCellData")
                    .day;
                assert_eq!(baked, day, "the rendered number and the baked one disagree");
            }
        }
    }

    // ==================================================================
    // on_day_click / restyle_days
    // ==================================================================

    #[test]
    fn clicking_a_day_selects_it_and_restyles_the_whole_grid() {
        let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1));
        let (cell, payload) = day_cell(&styled, 17);
        let rows = 5; // February 2024: 4 leading blanks + 29 days = 33 -> 5 rows

        let (update, changes) = click(styled, &payload, cell);

        assert_eq!(
            read_state(&shared),
            DatePickerState {
                year: 2024,
                month: 2,
                day: 17
            },
            "the click did not move the selection to the clicked day",
        );
        assert_eq!(
            update,
            Update::DoNothing,
            "with no user callback installed the handler must report DoNothing",
        );

        let bgs = pushed_backgrounds(&changes);
        assert_eq!(
            bgs.len(),
            rows * 7,
            "restyle_days must repaint every cell of every week row",
        );
        let accented: Vec<NodeId> = bgs
            .iter()
            .filter(|(_, c)| *c == ACCENT_BG)
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(
            accented,
            vec![cell.node.into_crate_internal().unwrap()],
            "exactly the clicked cell must end up accented",
        );
        assert!(
            bgs.iter()
                .filter(|(n, _)| *n != accented[0])
                .all(|(_, c)| *c == TRANSPARENT),
            "a cell other than the clicked one kept a background",
        );

        let texts = pushed_text_colours(&changes);
        assert_eq!(
            texts.len(),
            rows * 7,
            "every cell needs its text colour resynced too"
        );
        assert_eq!(
            texts
                .iter()
                .filter(|(_, c)| *c == WHITE)
                .map(|(n, _)| *n)
                .collect::<Vec<_>>(),
            accented,
            "the white-on-accent text did not land on the accented cell",
        );
    }

    #[test]
    fn clicking_a_foreign_payload_changes_nothing_at_all() {
        // A stale/mismatched RefAny must bail out *before* touching the state and
        // before pushing any DOM change.
        let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1));
        let (cell, _) = day_cell(&styled, 17);
        let before = read_state(&shared);

        let (update, changes) = click(styled, &RefAny::new(0xBAD_u32), cell);

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload still restyled the grid"
        );
        assert_eq!(
            read_state(&shared),
            before,
            "a foreign payload moved the selection"
        );
    }

    #[test]
    fn clicking_a_detached_node_updates_the_state_but_pushes_no_style() {
        // `set_css_property` *panics* on a None node id, so `restyle_days` has to
        // bail out at `get_parent`. The selection itself is already committed by
        // then — the state moves, the pixels do not, and nothing panics.
        for hit in [node_none(), node(9999)] {
            let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1));
            let (_, payload) = day_cell(&styled, 17);

            let (update, changes) = click(styled, &payload, hit);

            assert_eq!(update, Update::DoNothing, "{hit:?}: unexpected verdict");
            assert!(
                changes.is_empty(),
                "{hit:?}: a detached hit still pushed a style change"
            );
            assert_eq!(
                read_state(&shared).day,
                17,
                "{hit:?}: the selection was not committed"
            );
        }
    }

    #[test]
    fn clicking_a_node_without_a_grandparent_restyles_nothing() {
        // `restyle_days` walks cell -> week row -> grid. Handing it the container
        // (which has no grandparent) must be a no-op, not a panic or a repaint of
        // the header.
        let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1));
        let (_, payload) = day_cell(&styled, 3);

        let (_, changes) = click(styled, &payload, node(0));

        assert!(
            changes.is_empty(),
            "restyling from the root touched nodes anyway"
        );
        assert_eq!(read_state(&shared).day, 3);
    }

    #[test]
    fn the_change_callback_sees_the_clicked_day_and_its_verdict_is_forwarded() {
        // Order matters: the state is written *before* the user callback runs, so
        // the callback observes the date the user just asked for, not the stale one.
        let probe = log_refany();
        let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1).with_on_change(
            probe.clone(),
            record_change as DatePickerOnChangeCallbackType,
        ));
        let (cell, payload) = day_cell(&styled, 29);

        let (update, changes) = click(styled, &payload, cell);

        let log = read_log(&probe);
        assert_eq!(
            log.seen,
            vec![DatePickerState {
                year: 2024,
                month: 2,
                day: 29
            }],
            "the change callback was not called exactly once with the NEW state",
        );
        assert_eq!(
            log.payload, 0xDEAD_BEEF,
            "the callback was handed something other than the user's own RefAny",
        );
        assert_eq!(
            update,
            Update::RefreshDom,
            "the user callback's Update was swallowed"
        );
        assert_eq!(read_state(&shared).day, 29);
        // ... and the restyle still happens after the user callback returns.
        assert!(
            !changes.is_empty(),
            "a RefreshDom callback suppressed the restyle"
        );
    }

    #[test]
    fn a_change_callback_that_declines_the_update_still_gets_the_grid_restyled() {
        let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1).with_on_change(
            RefAny::new(0u8),
            change_do_nothing as DatePickerOnChangeCallbackType,
        ));
        let (cell, payload) = day_cell(&styled, 12);

        let (update, changes) = click(styled, &payload, cell);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(read_state(&shared).day, 12);
        assert!(
            !changes.is_empty(),
            "a DoNothing user callback suppressed the widget's own repaint",
        );
    }

    #[test]
    fn clicking_every_day_of_the_month_keeps_the_state_and_the_accent_in_agreement() {
        // The class of bug this catches is drift: the state saying one day while the
        // accent sits on another. Re-rendered every iteration, so each click is
        // delivered against a DOM that matches the state it starts from.
        for day in 1..=29u32 {
            let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1));
            let (cell, payload) = day_cell(&styled, day);

            let (_, changes) = click(styled, &payload, cell);

            assert_eq!(
                read_state(&shared).day,
                day,
                "click #{day}: the state drifted"
            );
            let accented: Vec<NodeId> = pushed_backgrounds(&changes)
                .into_iter()
                .filter(|(_, c)| *c == ACCENT_BG)
                .map(|(n, _)| n)
                .collect();
            assert_eq!(
                accented,
                vec![cell.node.into_crate_internal().unwrap()],
                "click #{day}: the accent disagrees with the clicked cell",
            );
        }
    }

    #[test]
    fn repeated_clicks_on_the_same_cell_are_idempotent() {
        let (styled, shared) = laid_out(DatePicker::create(2024, 2, 1));
        let (cell, payload) = day_cell(&styled, 20);
        let styled2 = StyledDom::create_from_dom(DatePicker::create(2024, 2, 1).dom());

        let (_, first) = click(styled, &payload, cell);
        let (_, second) = click(styled2, &payload, cell);

        assert_eq!(
            pushed_backgrounds(&first),
            pushed_backgrounds(&second),
            "clicking the same cell twice produced different repaints",
        );
        assert_eq!(read_state(&shared).day, 20);
    }

    // ==================================================================
    // on_prev_month / on_next_month / month_nav
    // ==================================================================

    #[test]
    fn next_month_wraps_december_into_january_of_the_following_year() {
        let (s, _, _) = press_nav(
            DatePickerState {
                year: 2024,
                month: 12,
                day: 15,
            },
            true,
            1,
        );
        assert_eq!(
            s,
            DatePickerState {
                year: 2025,
                month: 1,
                day: 15
            }
        );
    }

    #[test]
    fn prev_month_wraps_january_into_december_of_the_preceding_year() {
        let (s, _, _) = press_nav(
            DatePickerState {
                year: 2024,
                month: 1,
                day: 15,
            },
            false,
            1,
        );
        assert_eq!(
            s,
            DatePickerState {
                year: 2023,
                month: 12,
                day: 15
            }
        );
    }

    #[test]
    fn twelve_presses_in_either_direction_land_on_the_same_month_one_year_away() {
        let start = DatePickerState {
            year: 2024,
            month: 5,
            day: 15,
        };
        let (fwd, _, _) = press_nav(start, true, 12);
        let (back, _, _) = press_nav(start, false, 12);
        assert_eq!(
            fwd,
            DatePickerState {
                year: 2025,
                month: 5,
                day: 15
            }
        );
        assert_eq!(
            back,
            DatePickerState {
                year: 2023,
                month: 5,
                day: 15
            }
        );
    }

    #[test]
    fn month_nav_walks_every_month_in_order_across_a_year_boundary() {
        let mut state = DatePickerState {
            year: 2023,
            month: 11,
            day: 10,
        };
        let expected = [
            (2023u32, 12u32),
            (2024, 1),
            (2024, 2),
            (2024, 3),
            (2024, 4),
            (2024, 5),
            (2024, 6),
            (2024, 7),
            (2024, 8),
            (2024, 9),
            (2024, 10),
            (2024, 11),
            (2024, 12),
            (2025, 1),
        ];
        for (i, (y, m)) in expected.iter().enumerate() {
            let (next, _, _) = press_nav(state, true, 1);
            assert_eq!(
                (next.year, next.month),
                (*y, *m),
                "step {i}: the month walk went off the rails",
            );
            state = next;
        }
    }

    #[test]
    fn month_nav_clamps_the_selected_day_into_the_shorter_month_and_never_grows_it_back() {
        // 31 January -> February clamps to 29; stepping on to March leaves it at 29
        // rather than restoring the 31 the user originally picked. Lossy, but the
        // alternative (remembering the original) is not what the code does — and a
        // silently un-clamped 31 February would highlight a cell that isn't there.
        let jan31 = DatePickerState {
            year: 2024,
            month: 1,
            day: 31,
        };
        let (feb, _, _) = press_nav(jan31, true, 1);
        assert_eq!(
            feb,
            DatePickerState {
                year: 2024,
                month: 2,
                day: 29
            }
        );

        let (mar, _, _) = press_nav(feb, true, 1);
        assert_eq!(
            mar,
            DatePickerState {
                year: 2024,
                month: 3,
                day: 29
            },
            "the clamp grew back"
        );

        // Non-leap February clamps one day further.
        let (feb23, _, _) = press_nav(
            DatePickerState {
                year: 2023,
                month: 1,
                day: 31,
            },
            true,
            1,
        );
        assert_eq!(feb23.day, 28);
        // A 31st into a 30-day month.
        let (apr, _, _) = press_nav(
            DatePickerState {
                year: 2024,
                month: 3,
                day: 31,
            },
            true,
            1,
        );
        assert_eq!(
            apr,
            DatePickerState {
                year: 2024,
                month: 4,
                day: 30
            }
        );
    }

    #[test]
    fn month_nav_leaves_a_renderable_day_for_every_month_of_a_two_year_walk() {
        // The invariant the widget actually depends on: after any number of presses
        // the day still exists in the displayed month, so the grid always has a cell
        // to accent.
        for start_day in [1u32, 28, 29, 30, 31] {
            let mut state = DatePickerState {
                year: 2023,
                month: 1,
                day: start_day,
            };
            for step in 0..24 {
                let (next, _, _) = press_nav(state, true, 1);
                assert!(
                    (1..=12).contains(&next.month),
                    "step {step} from day {start_day}: month {} is out of range",
                    next.month,
                );
                assert!(
                    next.day <= days_in_month(next.year, next.month),
                    "step {step} from day {start_day}: day {} does not exist in {}-{}",
                    next.day,
                    next.year,
                    next.month,
                );
                state = next;
            }
        }
    }

    #[test]
    fn month_nav_floors_the_year_at_one_instead_of_wrapping_below_zero() {
        // Stepping back out of January of year 1 would give year 0; the `max(1)`
        // pins it. Year 0 itself is likewise lifted to 1.
        let (from_one, _, _) = press_nav(
            DatePickerState {
                year: 1,
                month: 1,
                day: 1,
            },
            false,
            1,
        );
        assert_eq!(
            from_one,
            DatePickerState {
                year: 1,
                month: 12,
                day: 1
            }
        );

        let (from_zero, _, _) = press_nav(
            DatePickerState {
                year: 0,
                month: 1,
                day: 1,
            },
            false,
            1,
        );
        assert_eq!(
            from_zero,
            DatePickerState {
                year: 1,
                month: 12,
                day: 1
            }
        );

        // Twenty more presses cannot push it below the floor.
        let (deep, _, _) = press_nav(
            DatePickerState {
                year: 1,
                month: 1,
                day: 1,
            },
            false,
            20,
        );
        assert!(deep.year >= 1, "the year floor leaked, got {}", deep.year);
        assert!((1..=12).contains(&deep.month));
    }

    #[test]
    fn month_nav_collapses_a_saturated_year_to_the_floor_without_panicking() {
        // `year as i32` turns u32::MAX into -1, so the ±1 lands at 0 or -2 and the
        // `max(1)` floor catches both. Deterministic, and above all not a panic.
        for next in [false, true] {
            let (s, _, _) = press_nav(
                DatePickerState {
                    year: u32::MAX,
                    month: 6,
                    day: 1,
                },
                next,
                1,
            );
            assert_eq!(
                s.year, 1,
                "next={next}: a saturated year did not collapse to the floor"
            );
            assert!((1..=12).contains(&s.month));
        }
        // Month 12 + next is the year-incrementing branch, month 1 + prev the
        // decrementing one — both must survive the wrapped year.
        let (a, _, _) = press_nav(
            DatePickerState {
                year: u32::MAX,
                month: 12,
                day: 1,
            },
            true,
            1,
        );
        assert_eq!(
            a,
            DatePickerState {
                year: 1,
                month: 1,
                day: 1
            }
        );
        let (b, _, _) = press_nav(
            DatePickerState {
                year: u32::MAX,
                month: 1,
                day: 1,
            },
            false,
            1,
        );
        assert_eq!(
            b,
            DatePickerState {
                year: 1,
                month: 12,
                day: 1
            }
        );
    }

    #[test]
    fn month_nav_repairs_an_out_of_range_month_rather_than_propagating_it() {
        // The state is `pub`, so a host can leave 0 or 13 in it. Whatever the nav
        // does, it must hand back something `days_in_month` and `month_name` agree
        // on — i.e. a month in 1..=12.
        for raw in [0u32, 13, 99, u32::MAX] {
            for next in [false, true] {
                let (s, _, _) = press_nav(
                    DatePickerState {
                        year: 2024,
                        month: raw,
                        day: 1,
                    },
                    next,
                    1,
                );
                assert!(
                    (1..=12).contains(&s.month),
                    "raw month {raw}, next={next}: nav left month {}",
                    s.month,
                );
                assert!(s.year >= 1);
            }
        }
    }

    #[test]
    fn month_nav_does_not_touch_the_grid() {
        // TODO2 in the module docs: ‹ / › cannot add or remove day cells, so they
        // deliberately push *no* DOM change at all. A restyle here would leave the
        // old month's grid wearing the new month's highlighting.
        for next in [false, true] {
            let (_, _, changes) = press_nav(
                DatePickerState {
                    year: 2024,
                    month: 3,
                    day: 15,
                },
                next,
                1,
            );
            assert!(
                changes.is_empty(),
                "next={next}: month navigation pushed {} DOM change(s)",
                changes.len(),
            );
        }
    }

    #[test]
    fn month_nav_reports_the_new_month_so_the_host_can_rebuild() {
        // The whole point of the TODO2 design: the widget cannot rebuild its own
        // grid, so it *must* tell the host what changed.
        let probe = log_refany();
        let shared = RefAny::new(DatePickerData {
            state: DatePickerStateWrapper {
                inner: DatePickerState {
                    year: 2024,
                    month: 1,
                    day: 31,
                },
                on_change: Some(DatePickerOnChange {
                    callback: DatePickerOnChangeCallback::from(
                        record_change as DatePickerOnChangeCallbackType,
                    ),
                    refany: probe.clone(),
                })
                .into(),
            },
            open: false,
            mode: DatePickerMode::Date,
        });

        let (update, _) = with_info(StyledDom::default(), node(0), |info| {
            on_next_month(shared.clone(), *info)
        });

        assert_eq!(
            read_log(&probe).seen,
            vec![DatePickerState {
                year: 2024,
                month: 2,
                day: 29
            }],
            "the host was not told the new (clamped) month",
        );
        assert_eq!(
            update,
            Update::RefreshDom,
            "the host's verdict was swallowed"
        );
    }

    #[test]
    fn month_nav_on_the_real_header_buttons_moves_the_real_widget_state() {
        // End-to-end through `dom()`: the arrows the widget actually renders are
        // wired to the state the widget actually shares.
        for (handler, want) in [
            (
                on_prev_month as usize,
                DatePickerState {
                    year: 2024,
                    month: 5,
                    day: 10,
                },
            ),
            (
                on_next_month as usize,
                DatePickerState {
                    year: 2024,
                    month: 7,
                    day: 10,
                },
            ),
        ] {
            let (styled, shared) = laid_out(DatePicker::create(2024, 6, 10));
            let (hit, payload) = nav_button(&styled, handler);

            let (_, changes) = with_info(styled, hit, |info| {
                if handler == on_prev_month as usize {
                    on_prev_month(payload.clone(), *info)
                } else {
                    on_next_month(payload.clone(), *info)
                }
            });

            assert_eq!(
                read_state(&shared),
                want,
                "the header arrow did not move the state"
            );
            assert!(
                changes.is_empty(),
                "the header arrow tried to restyle the stale grid"
            );
        }
    }

    #[test]
    fn month_nav_with_a_foreign_payload_is_a_no_op() {
        for next in [false, true] {
            let foreign = RefAny::new(0xBAD_u32);
            let (update, changes) = with_info(StyledDom::default(), node(0), |info| {
                if next {
                    on_next_month(foreign.clone(), *info)
                } else {
                    on_prev_month(foreign.clone(), *info)
                }
            });
            assert_eq!(
                update,
                Update::DoNothing,
                "next={next}: a foreign payload was acted on"
            );
            assert!(
                changes.is_empty(),
                "next={next}: a foreign payload pushed a DOM change"
            );
        }
    }

    #[test]
    fn navigating_then_clicking_still_selects_within_the_stale_grid() {
        // The documented limitation: after ‹ / › the grid is stale. A click on it
        // must still be coherent — it writes the *displayed* cell's day into the
        // state, alongside the already-navigated month.
        let (styled, shared) = laid_out(DatePicker::create(2024, 6, 10));
        let (nav_hit, nav_payload) = nav_button(&styled, on_next_month as usize);
        let (cell, cell_payload) = day_cell(&styled, 23);

        let (_, _) = with_info(styled, nav_hit, |info| {
            on_next_month(nav_payload.clone(), *info)
        });
        assert_eq!(
            read_state(&shared),
            DatePickerState {
                year: 2024,
                month: 7,
                day: 10
            }
        );

        let fresh = StyledDom::create_from_dom(DatePicker::create(2024, 6, 10).dom());
        let (_, changes) = click(fresh, &cell_payload, cell);

        assert_eq!(
            read_state(&shared),
            DatePickerState {
                year: 2024,
                month: 7,
                day: 23
            },
            "a click on the stale grid did not write the displayed day into the state",
        );
        assert!(!changes.is_empty(), "the stale grid was not restyled");
    }

    // ==================================================================
    // Roving tabindex in the day grid (WAI-ARIA APG date grid, P2-12)
    // ==================================================================

    /// The calendar exactly as its popup WINDOW holds it: the panel is that
    /// window's root DOM (in the parent the whole subtree sits under the
    /// `<transient-window>` node and is out of the parent's Tab order). Hands
    /// back the shared state too.
    ///
    /// February 2024 opens on a Thursday, so its week rows are
    /// `[_ _ _ _ 1 2 3] [4..10] [11..17] [18..24] [25..29 _ _]`.
    fn calendar(year: u32, month: u32, day: u32) -> (StyledDom, RefAny) {
        let dom = DatePicker::create(year, month, day).dom();
        let styled = StyledDom::create_from_dom(panel_of(&dom).clone());
        let shared = shared_state(&styled);
        (styled, shared)
    }

    fn cell_of(styled: &StyledDom, day: u32) -> DomNodeId {
        day_cell(styled, day).0
    }

    /// Presses `key` on the cell of `day`; panics when the cell has no key
    /// handler - the state of every day cell before P2-12.
    fn press_day(
        styled: &StyledDom,
        day: u32,
        key: VirtualKeyCode,
        held: &[VirtualKeyCode],
    ) -> (Update, Vec<CallbackChange>) {
        rv::press(styled, cell_of(styled, day), key, held)
            .expect("every day cell must carry a key handler for the arrow keys")
    }

    #[test]
    fn tab_into_the_day_grid_lands_on_the_selected_day_and_the_next_tab_leaves_it() {
        let (styled, _) = calendar(2024, 2, 14);
        let (prev, _) = nav_button(&styled, on_prev_month as usize);
        let (next, _) = nav_button(&styled, on_next_month as usize);
        assert_eq!(
            rv::tab_walk(&styled, Some(next), true, 2),
            vec![cell_of(&styled, 14), prev],
            "the grid is ONE tab stop: the selected day, then round to the header",
        );
        assert_eq!(
            rv::tab_walk(&styled, Some(prev), false, 2),
            vec![cell_of(&styled, 14), next],
        );
    }

    #[test]
    fn with_the_selected_day_out_of_range_the_first_day_is_the_tab_stop() {
        let mut picker = DatePicker::create(2024, 2, 14);
        picker.state.inner.day = 40;
        let dom = picker.dom();
        let styled = StyledDom::create_from_dom(panel_of(&dom).clone());
        let (prev, _) = nav_button(&styled, on_prev_month as usize);
        let (next, _) = nav_button(&styled, on_next_month as usize);
        assert_eq!(
            rv::tab_walk(&styled, Some(next), true, 2),
            vec![cell_of(&styled, 1), prev],
        );
    }

    #[test]
    fn arrows_move_focus_by_a_day_and_by_a_week_without_picking_the_day() {
        use azul_core::window::VirtualKeyCode as K;

        for (key, to) in [(K::Right, 15), (K::Left, 13), (K::Down, 21), (K::Up, 7)] {
            let (styled, shared) = calendar(2024, 2, 14);
            let (update, changes) = press_day(&styled, 14, key, &[]);
            assert_eq!(
                rv::focus_request(&changes),
                Some(cell_of(&styled, to)),
                "{key:?} from the 14th must focus the {to}th",
            );
            assert!(rv::prevented(&changes), "{key:?} must cancel spatial navigation");
            assert_eq!(update, Update::DoNothing);
            assert_eq!(
                read_state(&shared).day,
                14,
                "moving focus is not choosing: Enter / a click picks the day",
            );
            assert!(transient_writes(&changes).is_empty(), "{key:?} closed the calendar");
        }
    }

    #[test]
    fn home_and_end_jump_to_the_start_and_end_of_the_week() {
        use azul_core::window::VirtualKeyCode as K;

        // (focused day, key, focused day after) - the first and the last row
        // are partial weeks, so the ends are the first / last DAY of the row.
        for (from, key, to) in [
            (14, K::Home, 11),
            (14, K::End, 17),
            (2, K::Home, 1),
            (2, K::End, 3),
            (27, K::Home, 25),
            (27, K::End, 29),
        ] {
            let (styled, _) = calendar(2024, 2, 14);
            let (_, changes) = press_day(&styled, from, key, &[]);
            assert_eq!(
                rv::focus_request(&changes),
                Some(cell_of(&styled, to)),
                "{key:?} from the {from}th",
            );
            assert!(rv::prevented(&changes));
        }
    }

    #[test]
    fn an_arrow_past_the_displayed_month_stays_on_the_grid_and_goes_nowhere() {
        use azul_core::window::VirtualKeyCode as K;

        for (from, key) in [(1, K::Left), (3, K::Up), (29, K::Right), (27, K::Down)] {
            let (styled, _) = calendar(2024, 2, 14);
            let (_, changes) = press_day(&styled, from, key, &[]);
            assert!(
                rv::prevented(&changes),
                "{key:?} on the {from}th must not let spatial navigation leave the grid",
            );
            assert_eq!(rv::focus_request(&changes), None, "{key:?} on the {from}th");
        }
    }

    #[test]
    fn page_up_and_page_down_turn_the_month_like_the_header_buttons() {
        let (styled, shared) = calendar(2024, 2, 14);
        let (_, changes) = press_day(&styled, 14, VirtualKeyCode::PageDown, &[]);
        assert!(rv::prevented(&changes), "PageDown must not scroll the page");
        assert_eq!(
            read_state(&shared),
            DatePickerState {
                year: 2024,
                month: 3,
                day: 14
            },
        );

        let (styled, shared) = calendar(2024, 1, 31);
        let (_, changes) = press_day(&styled, 31, VirtualKeyCode::PageUp, &[]);
        assert!(rv::prevented(&changes));
        assert_eq!(
            read_state(&shared),
            DatePickerState {
                year: 2023,
                month: 12,
                day: 31
            },
        );
    }

    #[test]
    fn after_an_arrow_the_focused_day_is_the_grids_only_tab_stop() {
        let (mut styled, _) = calendar(2024, 2, 14);
        let (_, changes) = press_day(&styled, 14, VirtualKeyCode::Down, &[]);
        rv::apply_tab_index_writes(&mut styled, &changes);
        let (prev, _) = nav_button(&styled, on_prev_month as usize);
        let (next, _) = nav_button(&styled, on_next_month as usize);
        assert_eq!(
            rv::tab_walk(&styled, Some(next), true, 2),
            vec![cell_of(&styled, 21), prev],
        );
    }

    #[test]
    fn modified_and_unused_keys_on_a_day_are_not_consumed() {
        use azul_core::window::VirtualKeyCode as K;

        for (key, held) in [
            (K::Right, Some(K::LAlt)),
            (K::Down, Some(K::LShift)),
            (K::PageDown, Some(K::LControl)),
            (K::Home, Some(K::RWin)),
            (K::Tab, None),
            (K::Escape, None),
            (K::Return, None),
        ] {
            let (styled, shared) = calendar(2024, 2, 14);
            let held: Vec<K> = held.into_iter().collect();
            let (update, changes) = press_day(&styled, 14, key, &held);
            assert_eq!(update, Update::DoNothing);
            assert!(
                changes.is_empty(),
                "{held:?}+{key:?} must not be consumed: {changes:?}"
            );
            assert_eq!(read_state(&shared).month, 2, "{held:?}+{key:?} turned the month");
        }
    }

    // ==================================================================
    // Accessibility: what a screen reader hears on the calendar
    // ==================================================================

    /// The accessibility name a node declares, if any.
    fn a11y_name(dom: &Dom) -> Option<String> {
        dom.root
            .accessibility
            .as_ref()
            .and_then(|info| info.accessibility_name.as_ref().map(|s| s.as_str().to_string()))
    }

    /// Every day of the roving date grid is a button named by its FULL date.
    /// "23" alone does not say which month or weekday it is, and the grid is
    /// one Tab stop, so the name is all a screen-reader user gets per arrow key.
    #[test]
    fn every_day_cell_is_a_button_named_by_its_full_date() {
        let dom = DatePicker::create(2026, 6, 23).dom();
        let (_, _, grid) = sections(&dom);
        let days: Vec<&Dom> = grid_cells(grid)
            .into_iter()
            .filter(|cell| text_of(cell).is_some())
            .collect();
        assert_eq!(days.len(), 30, "June 2026 has 30 days");
        for cell in &days {
            let info = cell
                .root
                .accessibility
                .as_ref()
                .expect("a day cell declares accessibility");
            assert_eq!(
                info.role,
                azul_core::a11y::AccessibilityRole::PushButton,
                "a day cell is a button of the date grid, not a combo box"
            );
            assert!(
                a11y_name(cell).is_some(),
                "day {:?} has no accessible name",
                text_of(cell)
            );
        }
        let day_23 = days
            .iter()
            .find(|cell| text_of(cell).as_deref() == Some("23"))
            .expect("the 23rd is in the grid");
        assert_eq!(a11y_name(day_23).as_deref(), Some("Tuesday, 23 June 2026"));
        let day_1 = days
            .iter()
            .find(|cell| text_of(cell).as_deref() == Some("1"))
            .expect("the 1st is in the grid");
        assert_eq!(a11y_name(day_1).as_deref(), Some("Monday, 1 June 2026"));
    }

    /// The header's ‹ / › glyphs are not names; the buttons say what they do.
    #[test]
    fn the_month_navigation_buttons_are_named() {
        let dom = DatePicker::create(2026, 6, 23).dom();
        let (header, _, _) = sections(&dom);
        let kids = header.children.as_ref();
        assert_eq!(kids.len(), 3, "header is [prev, label, next]");
        assert_eq!(a11y_name(&kids[0]).as_deref(), Some("Previous month"));
        assert_eq!(a11y_name(&kids[2]).as_deref(), Some("Next month"));
    }

    // ==================================================================
    // <input type=month> / <input type=week>: modes of the same calendar
    // ==================================================================

    mod month_and_week {
        use super::*;

        /// The text the FIELD shows (its first child, the value `<p>`).
        fn field_text(dom: &Dom) -> String {
            text_of(&dom.children.as_ref()[0]).expect("the field shows its value")
        }

        /// `(flattened node id, payload)` of the month cell for `month`.
        fn month_cell(sd: &StyledDom, month: u32) -> (DomNodeId, RefAny) {
            for (i, nd) in sd.node_data.as_ref().iter().enumerate() {
                for cb in nd.callbacks.as_ref() {
                    let matches = {
                        let mut r = cb.refany.clone();
                        r.downcast_ref::<MonthCellData>()
                            .is_some_and(|c| c.month == month)
                    };
                    if matches {
                        return (node(i), cb.refany.clone());
                    }
                }
            }
            panic!("the month grid has no cell for month {month}");
        }

        fn seen(log: &RefAny) -> Vec<DatePickerState> {
            let mut log = log.clone();
            let entries = log
                .downcast_ref::<ChangeLog>()
                .expect("the log changed type")
                .seen
                .clone();
            entries
        }

        fn texts_written(changes: &[CallbackChange]) -> Vec<String> {
            changes
                .iter()
                .filter_map(|c| match c {
                    CallbackChange::ChangeNodeText { text, .. } => {
                        Some(text.as_str().to_string())
                    }
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn iso_weeks_follow_iso_8601() {
            // 2026-01-01 is a Thursday: week 1 of 2026.
            assert_eq!(iso_week_of(2026, 1, 1), (2026, 1));
            // 2026-09-29 is the Tuesday of week 40.
            assert_eq!(iso_week_of(2026, 9, 29), (2026, 40));
            // 2021-01-01 is a Friday: still week 53 of 2020 (a leap year that
            // began on a Wednesday).
            assert_eq!(iso_week_of(2021, 1, 1), (2020, 53));
            // 2024-12-30 is a Monday: already week 1 of 2025.
            assert_eq!(iso_week_of(2024, 12, 30), (2025, 1));
            assert_eq!(iso_weeks_in_year(2020), 53);
            assert_eq!(iso_weeks_in_year(2026), 53);
            assert_eq!(iso_weeks_in_year(2025), 52);
        }

        #[test]
        fn the_monday_of_an_iso_week_is_found_across_year_boundaries() {
            assert_eq!(iso_week_monday(2026, 1), (2025, 12, 29));
            assert_eq!(iso_week_monday(2026, 40), (2026, 9, 28));
            assert_eq!(iso_week_monday(2020, 53), (2020, 12, 28));
            assert_eq!(iso_week_monday(2025, 1), (2024, 12, 30));
            for year in 2018..=2030 {
                for week in 1..=iso_weeks_in_year(year) {
                    let (y, m, d) = iso_week_monday(year, week);
                    assert_eq!(weekday(y, m, d), 1, "{year}-W{week} does not start on a Monday");
                    assert_eq!(iso_week_of(y, m, d), (year, week), "{year}-W{week} round trip");
                }
            }
        }

        #[test]
        fn each_mode_formats_its_value_the_html_way() {
            let s = |year, month, day| DatePickerState { year, month, day };
            assert_eq!(format_value(&s(2026, 9, 29), DatePickerMode::Date), "2026-09-29");
            assert_eq!(format_value(&s(2026, 9, 29), DatePickerMode::Month), "2026-09");
            assert_eq!(format_value(&s(2026, 9, 29), DatePickerMode::Week), "2026-W40");
            // The ISO year, not the calendar year, names a week.
            assert_eq!(format_value(&s(2021, 1, 1), DatePickerMode::Week), "2020-W53");
        }

        #[test]
        fn a_date_state_knows_its_iso_week() {
            let s = DatePickerState {
                year: 2024,
                month: 12,
                day: 30,
            };
            assert_eq!(s.iso_week(), (2025, 1));
        }

        #[test]
        fn a_month_picker_shows_year_and_month() {
            let dom = DatePicker::create_month(2026, 9).dom();
            assert_eq!(field_text(&dom), "2026-09");
        }

        #[test]
        fn a_month_pickers_calendar_is_the_twelve_months_of_its_year() {
            let dom = DatePicker::create_month(2026, 9).dom();
            let panel = panel_of(&dom);
            let [header, grid] = panel.children.as_ref() else {
                panic!("a month calendar is header + month grid");
            };
            assert_eq!(
                text_of(&header.children.as_ref()[1]).as_deref(),
                Some("2026"),
                "the header names the YEAR the months belong to"
            );
            let labels: Vec<String> = grid_cells(grid).iter().filter_map(|c| text_of(c)).collect();
            assert_eq!(
                labels,
                [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov",
                    "Dec"
                ]
            );
            // The selected month is highlighted and is the grid's ONE Tab stop.
            let cells = grid_cells(grid);
            let highlighted: Vec<usize> = cells
                .iter()
                .enumerate()
                .filter(|(_, c)| rendered_background(c) == Some(ACCENT_BG))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(highlighted, vec![8]);
            let stops: Vec<usize> = cells
                .iter()
                .enumerate()
                .filter(|(_, c)| c.root.get_tab_index() == Some(TabIndex::Auto))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(stops, vec![8]);
        }

        #[test]
        fn clicking_a_month_selects_it_reports_it_and_closes_the_calendar() {
            let log = log_refany();
            let (sd, shared) = laid_out(
                DatePicker::create_month(2026, 9)
                    .with_on_change(log.clone(), record_change as DatePickerOnChangeCallbackType),
            );
            let (cell, payload) = month_cell(&sd, 3);
            let (update, changes) =
                with_info(sd, cell, |info| on_month_click(payload.clone(), *info));

            assert_eq!(update, Update::RefreshDom, "the change callback's verdict");
            let s = read_state(&shared);
            assert_eq!((s.year, s.month), (2026, 3));
            assert_eq!(seen(&log).last().map(|s| s.month), Some(3));
            assert!(
                texts_written(&changes).iter().any(|t| t == "2026-03"),
                "the field must show the picked month: {changes:?}"
            );
            assert!(
                transient_writes(&changes).iter().any(|(_, open)| !open),
                "the calendar must close on a pick"
            );
        }

        #[test]
        fn the_year_arrows_of_a_month_picker_turn_the_year_and_retitle_the_header() {
            let (sd, shared) = laid_out(DatePicker::create_month(2026, 9));
            let (btn, payload) = nav_button(&sd, on_next_year as usize);
            let (_, changes) = with_info(sd, btn, |info| on_next_year(payload.clone(), *info));
            assert_eq!(read_state(&shared).year, 2027);
            assert!(
                texts_written(&changes).iter().any(|t| t == "2027"),
                "the header must name the new year: {changes:?}"
            );

            let (sd, shared) = laid_out(DatePicker::create_month(2026, 1));
            let (btn, payload) = nav_button(&sd, on_prev_year as usize);
            let _ = with_info(sd, btn, |info| on_prev_year(payload.clone(), *info));
            assert_eq!(read_state(&shared).year, 2025);
        }

        #[test]
        fn a_week_picker_shows_the_iso_week() {
            let dom = DatePicker::create_week(2026, 40).dom();
            assert_eq!(field_text(&dom), "2026-W40");
            let (y, m, d) = (
                read_dom_state(&dom).year,
                read_dom_state(&dom).month,
                read_dom_state(&dom).day,
            );
            assert_eq!((y, m, d), (2026, 9, 28), "a week is held as its Monday");
        }

        #[test]
        fn a_week_pickers_weeks_start_on_monday() {
            let dom = DatePicker::create_week(2026, 40).dom();
            let (_, weekdays, grid) = sections(&dom);
            assert_eq!(text_of(&weekdays.children.as_ref()[0]).as_deref(), Some("Mo"));
            assert_eq!(text_of(&weekdays.children.as_ref()[6]).as_deref(), Some("Su"));
            // September 2026 begins on a Tuesday: ONE blank before the 1st
            // in a Monday-first grid (two in the Sunday-first date grid).
            let days = day_numbers(grid);
            assert_eq!(days[0], None);
            assert_eq!(days[1], Some(1));

            // The plain date picker keeps its Sunday-first grid.
            let dom = DatePicker::create(2026, 9, 29).dom();
            let (_, weekdays, _) = sections(&dom);
            assert_eq!(text_of(&weekdays.children.as_ref()[0]).as_deref(), Some("Su"));
        }

        #[test]
        fn a_week_picker_highlights_the_whole_selected_week() {
            // Week 40 of 2026 is Mon 28 Sep .. Sun 4 Oct; the September grid
            // holds its first three days.
            let dom = DatePicker::create_week(2026, 40).dom();
            let (_, _, grid) = sections(&dom);
            let highlighted: Vec<u32> = grid_cells(grid)
                .iter()
                .filter(|c| rendered_background(c) == Some(ACCENT_BG))
                .filter_map(|c| text_of(c)?.parse().ok())
                .collect();
            assert_eq!(highlighted, vec![28, 29, 30]);
        }

        #[test]
        fn clicking_a_day_in_a_week_picker_selects_its_week() {
            let log = log_refany();
            let (sd, shared) = laid_out(
                DatePicker::create_week(2026, 40)
                    .with_on_change(log.clone(), record_change as DatePickerOnChangeCallbackType),
            );
            let (cell, payload) = day_cell(&sd, 16);
            let (_, changes) = with_info(sd, cell, |info| on_day_click(payload.clone(), *info));
            assert_eq!(read_state(&shared).iso_week(), (2026, 38));
            assert!(
                texts_written(&changes).iter().any(|t| t == "2026-W38"),
                "the field must show the picked WEEK: {changes:?}"
            );
            // The whole row of the 16th (Mon 14 .. Sun 20) is accented.
            let accented = pushed_backgrounds(&changes)
                .into_iter()
                .filter(|(_, c)| *c == ACCENT_BG)
                .count();
            assert_eq!(accented, 7, "a week pick must light up all seven days");
        }

        #[test]
        fn a_picker_carries_its_state_for_a_form() {
            let dom = DatePicker::create_month(2026, 9)
                .with_name("period".into())
                .dom();
            assert!(dom.root.get_dataset().is_some(), "a form reads the picker's state");
            assert!(dom.root.attributes().as_ref().iter().any(
                |a| matches!(a, azul_core::dom::AttributeType::Name(n) if n.as_str() == "period")
            ));
            assert!(dom.root.attributes().as_ref().iter().any(
                |a| matches!(a, azul_core::dom::AttributeType::InputType(t) if t.as_str() == "month")
            ));
        }

        /// The state the rendered widget holds, read through its dataset.
        fn read_dom_state(dom: &Dom) -> DatePickerState {
            let mut data = dom
                .root
                .get_dataset()
                .cloned()
                .expect("the picker carries its state as the field's dataset");
            let d = data
                .downcast_ref::<DatePickerData>()
                .expect("the dataset is the picker's shared state");
            d.state.inner
        }
    }
}

/// The theme option: which look a date picker renders in, and what each look
/// is - in every mode (`date`, `month`, `week`).
#[cfg(test)]
mod theme_tests {
    use azul_css::dynamic_selector::PseudoStateType;

    use super::*;
    use crate::widgets::themes::{flora, OptionUiTheme, UiTheme};

    fn picker(theme: UiTheme) -> Dom {
        DatePicker::create(2024, 2, 15).with_theme(theme).dom()
    }

    /// September 2026, month 9 picked.
    fn month_picker(theme: UiTheme) -> Dom {
        DatePicker::create_month(2026, 9).with_theme(theme).dom()
    }

    /// 2026-W40: Mon 28 Sep .. Sun 4 Oct; the September grid holds 28-30.
    fn week_picker(theme: UiTheme) -> Dom {
        DatePicker::create_week(2026, 40).with_theme(theme).dom()
    }

    fn panel(field: &Dom) -> &Dom {
        let popup = field.children.as_ref().last().expect("the popup");
        &popup.children.as_ref()[0]
    }

    /// The month header (date / week) or the year header (month).
    fn header(field: &Dom) -> &Dom {
        &panel(field).children.as_ref()[0]
    }

    /// The previous / next buttons of the header.
    fn navs(field: &Dom) -> [&Dom; 2] {
        let kids = header(field).children.as_ref();
        [&kids[0], &kids[2]]
    }

    fn header_label(field: &Dom) -> &Dom {
        &header(field).children.as_ref()[1]
    }

    fn weekday_cells(field: &Dom) -> &[Dom] {
        panel(field).children.as_ref()[1].children.as_ref()
    }

    /// Every cell with a text child - the days of a day grid, the months of
    /// the month grid (a blank has none).
    fn cells(field: &Dom) -> Vec<&Dom> {
        panel(field)
            .children
            .as_ref()
            .last()
            .expect("the grid")
            .children
            .as_ref()
            .iter()
            .flat_map(|row| row.children.as_ref().iter())
            .filter(|cell| !cell.children.as_ref().is_empty())
            .collect()
    }

    /// Day `n` of a day grid, or month `n` of the month grid.
    fn cell(field: &Dom, n: usize) -> &Dom {
        cells(field)[n - 1]
    }

    fn declarations(node: &Dom) -> Vec<CssPropertyWithConditions> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, c)| CssPropertyWithConditions {
                property: p.clone(),
                apply_if: c.clone(),
            })
            .collect()
    }

    /// The `(light, dark)` value `pick` finds among the declarations whose
    /// pseudo-states are exactly `states` (`&[]`: at rest), last one wins.
    fn pair<T>(
        node: &Dom,
        states: &[PseudoStateType],
        pick: impl Fn(&CssProperty) -> Option<T>,
    ) -> (Option<T>, Option<T>) {
        let mut light = None;
        let mut dark = None;
        for d in declarations(node) {
            if d.pseudo_state_conditions() != states {
                continue;
            }
            let Some(v) = pick(&d.property) else {
                continue;
            };
            if d.is_dark_twin() {
                dark = Some(v);
            } else {
                light = Some(v);
            }
        }
        (light, dark)
    }

    /// What the node shows when nothing happens to it, by day and night.
    fn at_rest<T>(node: &Dom, pick: impl Fn(&CssProperty) -> Option<T>) -> (Option<T>, Option<T>) {
        pair(node, &[], pick)
    }

    fn on_focus<T>(node: &Dom, pick: impl Fn(&CssProperty) -> Option<T>) -> (Option<T>, Option<T>) {
        pair(node, &[PseudoStateType::Focus], pick)
    }

    fn on_hover<T>(node: &Dom, pick: impl Fn(&CssProperty) -> Option<T>) -> (Option<T>, Option<T>) {
        pair(node, &[PseudoStateType::Hover], pick)
    }

    fn shadow(p: &CssProperty) -> Option<ColorU> {
        match p {
            CssProperty::BoxShadowTop(v)
            | CssProperty::BoxShadowRight(v)
            | CssProperty::BoxShadowBottom(v)
            | CssProperty::BoxShadowLeft(v) => v.get_property().map(|s| s.as_ref().color),
            _ => None,
        }
    }

    fn top_edge(p: &CssProperty) -> Option<ColorU> {
        match p {
            CssProperty::BorderTopColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        }
    }

    fn bg(p: &CssProperty) -> Option<Vec<StyleBackgroundContent>> {
        match p {
            CssProperty::BackgroundContent(v) => v.get_property().map(|v| v.as_ref().to_vec()),
            _ => None,
        }
    }

    fn ink(p: &CssProperty) -> Option<ColorU> {
        match p {
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        }
    }

    fn width(p: &CssProperty) -> Option<LayoutWidth> {
        match p {
            CssProperty::Width(v) => v.get_property().cloned(),
            _ => None,
        }
    }

    fn color(c: ColorU) -> Vec<StyleBackgroundContent> {
        alloc::vec![StyleBackgroundContent::Color(c)]
    }

    fn has_class(dom: &Dom, name: &str) -> bool {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// The payload palette a grid cell carries (day or month cell).
    fn palette_of(cell: &Dom) -> DayPalette {
        let mut payload = cell.root.get_callbacks().as_ref()[0].refany.clone();
        if let Some(day) = payload.downcast_ref::<DayCellData>() {
            return day.palette.clone();
        }
        let month = payload
            .downcast_ref::<MonthCellData>()
            .expect("a grid cell carries a day or a month payload");
        month.palette.clone()
    }

    /// Asserts a flora stone face: the accent's own colour under its rig, the
    /// same by day and night (no dark twin), in --fl-on-acc ink.
    fn assert_flora_stone(node: &Dom, what: &str) {
        let (face, night_face) = at_rest(node, bg);
        let face = face.unwrap_or_else(|| panic!("{what}: no face"));
        assert_eq!(
            face.first(),
            Some(&StyleBackgroundContent::Color(flora::LIGHT_ACC)),
            "{what}: the accent stone"
        );
        assert!(face.len() > 1, "{what}: the stone's rig lies over it");
        assert!(night_face.is_none(), "{what}: a stone is its own colour at night");
        assert_eq!(at_rest(node, ink), (Some(flora::LIGHT_ON_ACC), None), "{what}");
    }

    /// Asserts a flora cell that is not picked: ink by day and night, the
    /// hover face under the pointer.
    fn assert_flora_ink_cell(node: &Dom, what: &str) {
        assert_eq!(
            at_rest(node, ink),
            (Some(flora::LIGHT_INK), Some(flora::DARK_INK)),
            "{what}"
        );
        assert_eq!(
            on_hover(node, bg),
            (
                Some(alloc::vec![flora::HOVER_FACE_LIGHT]),
                Some(alloc::vec![flora::HOVER_FACE_DARK])
            ),
            "{what}: lifts under the pointer"
        );
    }

    fn assert_flora_ring(node: &Dom, what: &str) {
        assert_eq!(
            on_focus(node, shadow),
            (Some(flora::LIGHT_ACC), Some(flora::DARK_GLOW)),
            "{what}: flora's focus colour"
        );
    }

    fn assert_flat_ring(node: &Dom, what: &str) {
        let (l, d) = on_focus(node, shadow);
        assert!(l.is_some() && d.is_some(), "{what}: a focus halo by day and night: {l:?} {d:?}");
    }

    // ------------------------------------------------------------------
    // The option
    // ------------------------------------------------------------------

    #[test]
    fn a_date_picker_without_a_theme_renders_flat() {
        for plain in [
            DatePicker::create(2024, 2, 15),
            DatePicker::create_month(2026, 9),
            DatePicker::create_week(2026, 40),
        ] {
            assert_eq!(plain.theme, OptionUiTheme::None, "no opinion by default");
            let unset = plain.clone().dom();
            let flat = plain.with_theme(UiTheme::Flat).dom();
            assert_eq!(declarations(&unset), declarations(&flat));
            assert_eq!(declarations(panel(&unset)), declarations(panel(&flat)));
        }
    }

    #[test]
    fn set_theme_and_with_theme_record_the_same_theme() {
        let mut set = DatePicker::create(2024, 2, 15);
        set.set_theme(UiTheme::Flora);
        assert_eq!(set.theme, OptionUiTheme::Some(UiTheme::Flora));
        assert_eq!(DatePicker::create(2024, 2, 15).with_theme(UiTheme::Flora), set);
    }

    #[test]
    fn the_theme_survives_the_mode_builders() {
        let week = DatePicker::create_week(2026, 40).with_theme(UiTheme::Flora);
        assert_eq!(week.mode, DatePickerMode::Week);
        assert_eq!(week.theme, OptionUiTheme::Some(UiTheme::Flora));
        let month = DatePicker::create_month(2026, 9)
            .with_theme(UiTheme::Flora)
            .with_mode(DatePickerMode::Month);
        assert_eq!(month.theme, OptionUiTheme::Some(UiTheme::Flora));
    }

    // ------------------------------------------------------------------
    // Flat: the established look, a ring on every keyboard stop
    // ------------------------------------------------------------------

    #[test]
    fn every_flat_keyboard_stop_shows_a_focus_ring_by_day_and_night() {
        let dom = picker(UiTheme::Flat);
        let (l, d) = on_focus(&dom, top_edge);
        assert!(l.is_some() && d.is_some(), "the field rings on its border: {l:?} {d:?}");
        for nav in navs(&dom) {
            assert_flat_ring(nav, "a month button");
        }
        for n in [1, 15] {
            assert_flat_ring(cell(&dom, n), &format!("day {n}"));
        }
    }

    #[test]
    fn the_flat_month_grid_rings_every_stop_and_keeps_its_night_faces() {
        let dom = month_picker(UiTheme::Flat);
        for nav in navs(&dom) {
            assert_flat_ring(nav, "a year button");
        }
        let months = cells(&dom);
        assert_eq!(months.len(), 12);
        for (i, m) in months.iter().enumerate() {
            assert_flat_ring(m, &format!("month {}", i + 1));
            assert_eq!(
                at_rest(m, width).0,
                Some(LayoutWidth::const_px(MONTH_CELL_W)),
                "month {}: three day cells wide",
                i + 1
            );
        }
        let (by_day, at_night) = at_rest(cell(&dom, 9), bg);
        assert_eq!(by_day, Some(color(ACCENT_BG)), "the picked month, by day");
        assert!(at_night.is_some(), "the picked month has a night face");
    }

    #[test]
    fn the_flat_week_grid_rings_every_day_and_lights_the_whole_week() {
        let dom = week_picker(UiTheme::Flat);
        for nav in navs(&dom) {
            assert_flat_ring(nav, "a month button");
        }
        for d in cells(&dom) {
            assert_flat_ring(d, "a day of the week grid");
        }
        for n in [28, 29, 30] {
            let (by_day, at_night) = at_rest(cell(&dom, n), bg);
            assert_eq!(by_day, Some(color(ACCENT_BG)), "day {n} of the picked week");
            assert!(at_night.is_some(), "day {n}: a night face");
        }
    }

    // ------------------------------------------------------------------
    // Flora: a paper field over a paper calendar, in every mode
    // ------------------------------------------------------------------

    #[test]
    fn a_flora_field_is_a_sunken_paper_field_that_rings_in_the_accent() {
        for dom in [
            picker(UiTheme::Flora),
            month_picker(UiTheme::Flora),
            week_picker(UiTheme::Flora),
        ] {
            assert_eq!(
                at_rest(&dom, bg),
                (Some(color(flora::LIGHT_FLD)), Some(color(flora::DARK_FLD))),
                "--fl-fld: a field"
            );
            assert_eq!(
                at_rest(&dom, top_edge),
                (Some(flora::LIGHT_BD2), Some(flora::DARK_BD2)),
                "a --fl-bd2 hairline"
            );
            assert_eq!(
                on_hover(&dom, top_edge),
                (Some(flora::LIGHT_BD3), Some(flora::DARK_BD3)),
                "the rule darkens under the pointer"
            );
            assert_eq!(
                on_focus(&dom, top_edge),
                (Some(flora::LIGHT_ACC), Some(flora::DARK_GLOW)),
                "flora's focus colour on the field's rule"
            );
            assert!(has_class(&dom, "__azul-theme-flora"));
        }
    }

    #[test]
    fn a_flora_calendar_is_a_leaf_with_brass_buttons_in_every_mode() {
        for dom in [
            picker(UiTheme::Flora),
            month_picker(UiTheme::Flora),
            week_picker(UiTheme::Flora),
        ] {
            assert_eq!(
                at_rest(panel(&dom), bg),
                (Some(color(flora::LIGHT_SUR)), Some(color(flora::DARK_SUR))),
                "--fl-sur: a leaf"
            );
            assert_eq!(
                at_rest(panel(&dom), top_edge),
                (Some(flora::LIGHT_BD), Some(flora::DARK_BD))
            );
            assert_eq!(
                at_rest(header_label(&dom), ink),
                (Some(flora::LIGHT_INK), Some(flora::DARK_INK)),
                "the month / year in ink"
            );
            for nav in navs(&dom) {
                assert_eq!(
                    at_rest(nav, ink),
                    (Some(flora::LIGHT_QT), Some(flora::DARK_QT)),
                    "brass ink, --fl-qt"
                );
                assert_eq!(
                    on_hover(nav, ink),
                    (Some(flora::LIGHT_QT2), Some(flora::DARK_QT2)),
                    "deeper brass under the pointer"
                );
                assert_flora_ring(nav, "a header button");
            }
        }
        for dom in [picker(UiTheme::Flora), week_picker(UiTheme::Flora)] {
            for w in weekday_cells(&dom) {
                assert_eq!(
                    at_rest(w, ink),
                    (Some(flora::LIGHT_SOFT2), Some(flora::DARK_SOFT2)),
                    "the weekday names are quiet"
                );
            }
        }
    }

    #[test]
    fn a_flora_selected_day_is_the_accent_stone_and_the_rest_are_ink() {
        let dom = picker(UiTheme::Flora);
        assert_flora_stone(cell(&dom, 15), "the picked day");
        assert_flora_ink_cell(cell(&dom, 3), "day 3");
        for n in [3, 15] {
            assert_flora_ring(cell(&dom, n), &format!("day {n}"));
        }
    }

    #[test]
    fn a_flora_month_grid_cuts_the_picked_month_as_the_stone() {
        let dom = month_picker(UiTheme::Flora);
        let months = cells(&dom);
        assert_eq!(months.len(), 12);
        for (i, m) in months.iter().enumerate() {
            let what = format!("month {}", i + 1);
            if i + 1 == 9 {
                assert_flora_stone(m, &what);
            } else {
                assert_flora_ink_cell(m, &what);
            }
            assert_flora_ring(m, &what);
            assert_eq!(
                at_rest(m, width).0,
                Some(LayoutWidth::const_px(MONTH_CELL_W)),
                "{what}: three day cells wide"
            );
        }
    }

    #[test]
    fn a_flora_week_grid_cuts_every_day_of_the_picked_week_as_the_stone() {
        let dom = week_picker(UiTheme::Flora);
        for (i, d) in cells(&dom).iter().enumerate() {
            let n = i + 1;
            let what = format!("day {n}");
            if (28..=30).contains(&n) {
                assert_flora_stone(d, &what);
            } else {
                assert_flora_ink_cell(d, &what);
            }
            assert_flora_ring(d, &what);
        }
    }

    #[test]
    fn a_flora_pick_repaints_with_flora_s_stone_in_every_mode() {
        for (mode, dom) in [
            ("date", picker(UiTheme::Flora)),
            ("month", month_picker(UiTheme::Flora)),
            ("week", week_picker(UiTheme::Flora)),
        ] {
            let palette = palette_of(cell(&dom, 3));
            for night in 0..2 {
                let (fill, text) = &palette.selected[night];
                assert_eq!(
                    fill.as_ref().first(),
                    Some(&StyleBackgroundContent::Color(flora::LIGHT_ACC)),
                    "{mode}, night={night}: the stone"
                );
                assert_eq!(*text, flora::LIGHT_ON_ACC, "{mode}, night={night}");
            }
            assert_eq!(palette.other[0].1, flora::LIGHT_INK, "{mode}");
            assert_eq!(palette.other[1].1, flora::DARK_INK, "{mode}");
        }
    }

    #[test]
    fn a_flat_pick_repaints_with_the_established_colours_in_every_mode() {
        for dom in [
            picker(UiTheme::Flat),
            month_picker(UiTheme::Flat),
            week_picker(UiTheme::Flat),
        ] {
            let palette = palette_of(cell(&dom, 3));
            assert_eq!(palette.selected[0], day_cell_colours(true, false));
            assert_eq!(palette.selected[1], day_cell_colours(true, true));
            assert_eq!(palette.other[0], day_cell_colours(false, false));
            assert_eq!(palette.other[1], day_cell_colours(false, true));
        }
    }

    #[test]
    fn a_flora_date_picker_keeps_the_field_and_the_grid_in_every_mode() {
        for (mode, flora_dom, flat_dom, count) in [
            ("date", picker(UiTheme::Flora), picker(UiTheme::Flat), 29),
            ("month", month_picker(UiTheme::Flora), month_picker(UiTheme::Flat), 12),
            ("week", week_picker(UiTheme::Flora), week_picker(UiTheme::Flat), 30),
        ] {
            assert!(flora_dom.root.get_tab_index().is_some(), "{mode}: a keyboard stop");
            assert_eq!(
                flora_dom.root.get_accessibility_info().map(|a| a.role),
                flat_dom.root.get_accessibility_info().map(|a| a.role),
                "{mode}"
            );
            assert_eq!(cells(&flora_dom).len(), count, "{mode}");
            let stops = |dom: &Dom| -> Vec<usize> {
                cells(dom)
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.root.get_tab_index() == Some(TabIndex::Auto))
                    .map(|(i, _)| i)
                    .collect()
            };
            assert_eq!(stops(&flora_dom), stops(&flat_dom), "{mode}: one roving stop");
            // The FORM attributes (name, value, type...) - not the ids and
            // classes, where a theme adds its own marker class.
            let form_attributes = |dom: &Dom| {
                dom.root
                    .attributes()
                    .as_ref()
                    .iter()
                    .filter(|a| {
                        !matches!(
                            a,
                            azul_core::dom::AttributeType::Id(_)
                                | azul_core::dom::AttributeType::Class(_)
                        )
                    })
                    .count()
            };
            assert_eq!(
                form_attributes(&flora_dom),
                form_attributes(&flat_dom),
                "{mode}: the form attributes"
            );
        }
    }
}

/// Following the app theme (`theme: None`): the DOM carries every widget
/// theme's `@theme(<name>)` block and renders the app theme's; a pinned
/// widget (`with_theme`) ignores the app theme (T2 migration, T1 report
/// section 4).
#[cfg(test)]
mod app_theme_tests {
    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, UiTheme};

    #[test]
    fn a_date_picker_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "date_picker date",
            || DatePicker::create(2024, 2, 15).dom(),
            |t: UiTheme| DatePicker::create(2024, 2, 15).with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "date_picker month",
            || DatePicker::create_month(2024, 2).dom(),
            |t: UiTheme| DatePicker::create_month(2024, 2).with_theme(t).dom(),
        );
        checks::assert_follows_the_app_theme(
            "date_picker week",
            || DatePicker::create_week(2024, 7).dom(),
            |t: UiTheme| DatePicker::create_week(2024, 7).with_theme(t).dom(),
        );
    }
}
