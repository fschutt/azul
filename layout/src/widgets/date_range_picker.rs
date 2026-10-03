//! Date range picker widget - a span of days picked on two months side by
//! side, with presets for the spans people ask for: a report's period, a
//! mail search's dates, a booking, the days a calendar shows.
//!
//! ```text
//!   Today          <  March 2026              April 2026      >
//!   Yesterday      Mo Tu We Th Fr Sa Su   Mo Tu We Th Fr Sa Su
//!   Last 7 days                      1          1  2  3  4  5
//!   Last 30 days    2  3 [4][=][=][=][=]    6  7  8  9 10 11 12
//!   This month     [=][=][10] 11 12 13 14   ...
//!   Last month
//!                  4 Mar 2026 - 10 Mar 2026
//! ```
//!
//! PICKING: the first click on a day ANCHORS the range there; while the
//! pointer (or the keyboard focus) moves over the days, the span from the
//! anchor to that day is previewed in place - its ends in the picked face,
//! the days between washed - and the second click PICKS the range, either
//! way round. Escape drops the anchor (the range picked before shows
//! again). A preset picks its span at once.
//!
//! THE APP OWNS THE STATE ([`DateRangePickerView`]: the range, the anchor,
//! the months shown): every action reports a [`DateRangePickerEvent`] whose
//! `view` is the NEXT view. Anchoring, picking and the preview repaint the
//! two grids in place, so a picker works before the app stores anything;
//! turning the months (the header arrows, Page Up / Down, an arrow key past
//! the second month, a preset in another month) is a rebuild: the grids
//! hold other days.
//!
//! THE CALENDARS ARE THE DATE PICKER'S: the same look (`date_picker_look` of
//! the theme), the same grid frame (`date_picker::day_grid`), the same week
//! start ([`DatePickerWeekStart`]) and the same day names for a screen
//! reader; only the day cells' behaviour is the range picker's.
//!
//! KEYBOARD: the days of both months are ONE Tab stop; the arrows move the
//! focus across both months (Left / Right a day, Up / Down a week, Home /
//! End the week's ends), Enter or Space picks the focused day, Page Up /
//! Down turn the months. The presets are buttons.
//!
//! Key types: [`DateRangePicker`], [`DateRangePickerView`], [`DateRange`],
//! [`DateRangePreset`], [`DateRangePickerEvent`].

use alloc::{format, string::String, vec::Vec};

use azul_core::{callbacks::Update, refany::RefAny};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, AzString, OptionString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        date_picker::{DatePickerState, DatePickerWeekStart, OptionDatePickerState},
        themes::{OptionUiTheme, UiTheme},
    },
};

// ---- classes ----

/// The widget's root.
pub const DATE_RANGE_PICKER_CLASS: &str = "__azul-native-date-range-picker";
/// The presets column.
pub const DATE_RANGE_PRESETS_CLASS: &str = "__azul-native-date-range-presets";
/// One preset.
pub const DATE_RANGE_PRESET_CLASS: &str = "__azul-native-date-range-preset";
/// The row of the two months.
pub const DATE_RANGE_MONTHS_CLASS: &str = "__azul-native-date-range-months";
/// One month's calendar.
pub const DATE_RANGE_MONTH_CLASS: &str = "__azul-native-date-range-month";
/// One day of a month's grid.
pub const DATE_RANGE_DAY_CLASS: &str = "__azul-native-date-range-day";
/// The line naming the range under the months.
pub const DATE_RANGE_SUMMARY_CLASS: &str = "__azul-native-date-range-summary";

// ---- the types the app sees ----

/// A span of days, `start` to `end`, both included, `start` never after
/// `end`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    /// The first day.
    pub start: DatePickerState,
    /// The last day.
    pub end: DatePickerState,
}

impl DateRange {
    /// The span between `a` and `b`, either way round.
    #[must_use]
    pub fn create(a: DatePickerState, b: DatePickerState) -> Self {
        if key(a) <= key(b) {
            Self { start: a, end: b }
        } else {
            Self { start: b, end: a }
        }
    }

    /// Whether `day` is in the span (both ends included).
    #[must_use]
    pub fn contains(&self, day: DatePickerState) -> bool {
        key(self.start) <= key(day) && key(day) <= key(self.end)
    }

    /// How many days the span has (1 for a single day).
    #[must_use]
    pub fn day_count(&self) -> u32 {
        let days = day_number(self.end) - day_number(self.start) + 1;
        u32::try_from(days.max(0)).unwrap_or(0)
    }
}

/// The day number of `d`: days since a fixed day, so two dates subtract to
/// the days between them (the proleptic Gregorian calendar).
pub(crate) fn day_number(d: DatePickerState) -> i64 {
    // Days before the year (years 1..year), then the days of the year.
    let y = i64::from(d.year.max(1)) - 1;
    let before_year = y * 365 + y / 4 - y / 100 + y / 400;
    let before_month: i64 = (1..d.month.clamp(1, 12))
        .map(|m| i64::from(crate::widgets::date_picker::days_in_month(d.year, m)))
        .sum();
    before_year + before_month + i64::from(d.day)
}

impl_option!(
    DateRange,
    OptionDateRange,
    [Debug, Copy, Clone, PartialEq, Eq]
);

/// A span people ask for, relative to today.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DateRangePreset {
    /// Today alone.
    #[default]
    Today,
    /// Yesterday alone.
    Yesterday,
    /// The last seven days, today included.
    Last7Days,
    /// The last thirty days, today included.
    Last30Days,
    /// The week today is in (from the week start).
    ThisWeek,
    /// The week before it.
    LastWeek,
    /// The month today is in.
    ThisMonth,
    /// The month before it.
    LastMonth,
    /// The year today is in.
    ThisYear,
    /// The year before it.
    LastYear,
}

impl DateRangePreset {
    /// The preset's name on its button.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::Last7Days => "Last 7 days",
            Self::Last30Days => "Last 30 days",
            Self::ThisWeek => "This week",
            Self::LastWeek => "Last week",
            Self::ThisMonth => "This month",
            Self::LastMonth => "Last month",
            Self::ThisYear => "This year",
            Self::LastYear => "Last year",
        }
    }

    /// The span this preset stands for when it is `today`, weeks starting
    /// on `week_start`.
    #[must_use]
    pub fn range(self, today: DatePickerState, week_start: DatePickerWeekStart) -> DateRange {
        use crate::widgets::date_picker::{days_in_month, shifted_date, weekday};

        let shift = |by: i32| {
            let (year, month, day) = shifted_date(today.year, today.month, today.day, by);
            DatePickerState { year, month, day }
        };
        let month_of = |year: u32, month: u32| {
            DateRange::create(
                DatePickerState {
                    year,
                    month,
                    day: 1,
                },
                DatePickerState {
                    year,
                    month,
                    day: days_in_month(year, month),
                },
            )
        };
        let year_of = |year: u32| {
            DateRange::create(
                DatePickerState {
                    year,
                    month: 1,
                    day: 1,
                },
                DatePickerState {
                    year,
                    month: 12,
                    day: 31,
                },
            )
        };
        // Days since the week started (0 on the week start itself).
        let into_week = {
            let first = match week_start {
                DatePickerWeekStart::Sunday => 0,
                DatePickerWeekStart::Monday => 1,
            };
            let wd = weekday(today.year, today.month, today.day);
            ((wd + 7 - first) % 7) as i32
        };
        match self {
            Self::Today => DateRange::create(today, today),
            Self::Yesterday => DateRange::create(shift(-1), shift(-1)),
            Self::Last7Days => DateRange::create(shift(-6), today),
            Self::Last30Days => DateRange::create(shift(-29), today),
            Self::ThisWeek => DateRange::create(shift(-into_week), shift(6 - into_week)),
            Self::LastWeek => DateRange::create(shift(-into_week - 7), shift(-into_week - 1)),
            Self::ThisMonth => month_of(today.year, today.month),
            Self::LastMonth => {
                if today.month <= 1 {
                    month_of(today.year.saturating_sub(1).max(1), 12)
                } else {
                    month_of(today.year, today.month - 1)
                }
            }
            Self::ThisYear => year_of(today.year),
            Self::LastYear => year_of(today.year.saturating_sub(1).max(1)),
        }
    }
}

impl_option!(
    DateRangePreset,
    OptionDateRangePreset,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(
    DateRangePreset,
    DateRangePresetVec,
    DateRangePresetVecDestructor,
    DateRangePresetVecDestructorType,
    DateRangePresetVecSlice,
    OptionDateRangePreset
);
impl_vec_clone!(
    DateRangePreset,
    DateRangePresetVec,
    DateRangePresetVecDestructor
);
impl_vec_debug!(DateRangePreset, DateRangePresetVec);
impl_vec_mut!(DateRangePreset, DateRangePresetVec);

azul_css::impl_vec_partialeq!(DateRangePreset, DateRangePresetVec);

/// What the picker shows: the picked range, the anchor of a range being
/// picked, and the month on the left (the right one is the month after).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRangePickerView {
    /// The range picked, if any.
    pub range: OptionDateRange,
    /// The first day of a range being picked (one click made), if any.
    pub anchor: OptionDatePickerState,
    /// The year of the month on the left.
    pub year: u32,
    /// The month on the left, `1..=12`.
    pub month: u32,
}

impl DateRangePickerView {
    /// Nothing picked, `year`-`month` on the left.
    #[must_use]
    pub const fn create(year: u32, month: u32) -> Self {
        Self {
            range: OptionDateRange::None,
            anchor: OptionDatePickerState::None,
            year,
            month,
        }
    }

    /// `range` picked, the months turned so it shows (its start's month on
    /// the left, or - for a range ending more than a month later - the
    /// month before its end).
    #[must_use]
    pub fn with_range(range: DateRange) -> Self {
        let start = Self::create(range.start.year, range.start.month);
        let (ry, rm) = start.right_month();
        let left = if (range.end.year, range.end.month) <= (ry, rm) {
            start
        } else {
            Self::create(range.end.year, range.end.month).turned(-1)
        };
        Self {
            range: OptionDateRange::Some(range),
            ..left
        }
    }

    /// The months turned by `delta` (negative: back).
    #[must_use]
    pub fn turned(self, delta: i32) -> Self {
        let index =
            i64::from(self.year) * 12 + i64::from(self.month.clamp(1, 12)) - 1 + i64::from(delta);
        let index = index.max(12); // never before January of year 1
        Self {
            year: u32::try_from(index / 12).unwrap_or(1),
            month: u32::try_from(index % 12).unwrap_or(0) + 1,
            ..self
        }
    }

    /// The month on the right: `(year, month)`.
    #[must_use]
    pub fn right_month(&self) -> (u32, u32) {
        let next = self.turned(1);
        (next.year, next.month)
    }
}

/// What happened.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DateRangePickerEventKind {
    /// The first day was clicked: `view.anchor` holds it.
    #[default]
    Anchored,
    /// The second day was clicked: `view.range` holds the range.
    Picked,
    /// A preset was chosen: `preset` names it, `view.range` holds its span.
    Preset,
    /// The months were turned: the app rebuilds on `view.year` / `month`.
    Navigated,
    /// Escape dropped the anchor.
    Cancelled,
}

/// One action of the picker: what happened and the NEXT view.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRangePickerEvent {
    /// The view after the action - the app stores it and rebuilds.
    pub view: DateRangePickerView,
    /// What happened.
    pub kind: DateRangePickerEventKind,
    /// The preset chosen (meaningful for `Preset`).
    pub preset: DateRangePreset,
}

/// Callback invoked on every action of the picker.
pub type DateRangePickerOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, DateRangePickerEvent) -> Update;
impl_widget_callback!(
    DateRangePickerOnEvent,
    OptionDateRangePickerOnEvent,
    DateRangePickerOnEventCallback,
    DateRangePickerOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DateRangePickerOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DATE_RANGE_PICKER_ON_EVENT_INVOKER,
    invoker_ty:     AzDateRangePickerOnEventCallbackInvoker,
    thunk_fn:       az_date_range_picker_on_event_callback_thunk,
    setter_fn:      AzApp_setDateRangePickerOnEventCallbackInvoker,
    from_handle_fn: AzDateRangePickerOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzDateRangePickerOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: DateRangePickerEvent ],
}

/// A range of days picked on two months, with presets (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DateRangePicker {
    /// The presets offered, in order (empty: no presets column).
    pub presets: DateRangePresetVec,
    /// The hook every action reports to.
    pub on_event: OptionDateRangePickerOnEvent,
    /// What this control is CALLED, for assistive technology.
    pub accessibility_name: OptionString,
    /// What the picker shows.
    pub view: DateRangePickerView,
    /// Today (ringed in the grid; what the presets count from). The picker
    /// cannot ask the clock itself: the app knows the time zone.
    pub today: DatePickerState,
    /// The weekday the rows start on.
    pub week_start: DatePickerWeekStart,
    /// The widget theme, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
}

/// The presets a picker offers unless the app chooses: today, yesterday,
/// the last 7 and 30 days, this and last month.
pub const DEFAULT_PRESETS: [DateRangePreset; 6] = [
    DateRangePreset::Today,
    DateRangePreset::Yesterday,
    DateRangePreset::Last7Days,
    DateRangePreset::Last30Days,
    DateRangePreset::ThisMonth,
    DateRangePreset::LastMonth,
];

impl DateRangePicker {
    /// A picker for `view`, today being `today`, with the default presets.
    #[must_use]
    pub fn create(view: DateRangePickerView, today: DatePickerState) -> Self {
        Self {
            presets: DateRangePresetVec::from_vec(DEFAULT_PRESETS.to_vec()),
            on_event: OptionDateRangePickerOnEvent::None,
            accessibility_name: OptionString::None,
            view,
            today,
            week_start: DatePickerWeekStart::Sunday,
            theme: OptionUiTheme::None,
        }
    }

    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// The view shown (the app's stored `event.view`).
    pub const fn set_view(&mut self, view: DateRangePickerView) {
        self.view = view;
    }

    /// [`Self::set_view`] for the builder chain.
    #[must_use]
    pub const fn with_view(mut self, view: DateRangePickerView) -> Self {
        self.set_view(view);
        self
    }

    /// The presets offered (empty: none).
    pub fn set_presets(&mut self, presets: DateRangePresetVec) {
        self.presets = presets;
    }

    /// [`Self::set_presets`] for the builder chain.
    #[must_use]
    pub fn with_presets(mut self, presets: DateRangePresetVec) -> Self {
        self.set_presets(presets);
        self
    }

    /// The weekday the rows start on.
    pub const fn set_week_start(&mut self, week_start: DatePickerWeekStart) {
        self.week_start = week_start;
    }

    /// [`Self::set_week_start`] for the builder chain.
    #[must_use]
    pub const fn with_week_start(mut self, week_start: DatePickerWeekStart) -> Self {
        self.set_week_start(week_start);
        self
    }

    /// The hook every action reports to (the NEXT view in `event.view`).
    pub fn set_on_event<C: Into<DateRangePickerOnEventCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_event = Some(DateRangePickerOnEvent::create(data, callback)).into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<DateRangePickerOnEventCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// Pin the widget theme. Unset (`None`), it follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty picker (January 2000) and returns the
    /// original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(
            DateRangePickerView::create(2000, 1),
            DatePickerState::default(),
        );
        core::mem::swap(&mut s, self);
        s
    }
}

// ---- the range logic (the pure half) ----

/// `(year, month, day)`: a date's order.
pub(crate) const fn key(d: DatePickerState) -> (u32, u32, u32) {
    (d.year, d.month, d.day)
}

/// A click on `day` in `view`: the first click anchors the range there, the
/// second picks the range from the anchor to it. The view after it, and
/// what happened.
#[must_use]
pub(crate) fn click_day(
    view: DateRangePickerView,
    day: DatePickerState,
) -> (DateRangePickerView, DateRangePickerEventKind) {
    match view.anchor.into_option() {
        None => (
            DateRangePickerView {
                anchor: OptionDatePickerState::Some(day),
                ..view
            },
            DateRangePickerEventKind::Anchored,
        ),
        Some(anchor) => (
            DateRangePickerView {
                range: OptionDateRange::Some(DateRange::create(anchor, day)),
                anchor: OptionDatePickerState::None,
                ..view
            },
            DateRangePickerEventKind::Picked,
        ),
    }
}

/// The span the grids show in `view` with the pointer (or the focus) on
/// `over`: from the anchor to it while a range is being picked, else the
/// picked range.
#[must_use]
pub(crate) fn shown_range(
    view: &DateRangePickerView,
    over: Option<DatePickerState>,
) -> Option<DateRange> {
    match view.anchor.into_option() {
        Some(anchor) => Some(DateRange::create(anchor, over.unwrap_or(anchor))),
        None => view.range.into_option(),
    }
}

/// One day in words: "4 Mar 2026".
fn day_text(d: DatePickerState) -> String {
    let name = crate::widgets::date_picker::month_name(d.month);
    format!("{} {} {}", d.day, name.get(..3).unwrap_or(name), d.year)
}

/// The range in words: "4 Mar 2026 - 10 Mar 2026", one day alone as
/// "4 Mar 2026".
#[must_use]
pub(crate) fn range_text(range: &DateRange) -> String {
    if range.start == range.end {
        day_text(range.start)
    } else {
        format!("{} \u{2013} {}", day_text(range.start), day_text(range.end))
    }
}

#[cfg(test)]
mod range_tests {
    use super::*;

    const fn d(year: u32, month: u32, day: u32) -> DatePickerState {
        DatePickerState { year, month, day }
    }

    /// Wednesday, 4 March 2026.
    const TODAY: DatePickerState = d(2026, 3, 4);

    fn span(p: DateRangePreset, start: DatePickerWeekStart) -> (DatePickerState, DatePickerState) {
        let r = p.range(TODAY, start);
        (r.start, r.end)
    }

    #[test]
    fn a_range_is_ordered_whichever_end_comes_first() {
        let r = DateRange::create(d(2026, 3, 10), d(2026, 3, 4));
        assert_eq!((r.start, r.end), (d(2026, 3, 4), d(2026, 3, 10)));
    }

    #[test]
    fn a_range_holds_both_its_ends_and_counts_its_days() {
        let r = DateRange::create(d(2026, 3, 4), d(2026, 3, 10));
        assert!(
            r.contains(d(2026, 3, 4)) && r.contains(d(2026, 3, 7)) && r.contains(d(2026, 3, 10))
        );
        assert!(!r.contains(d(2026, 3, 3)) && !r.contains(d(2026, 3, 11)));
        assert_eq!(r.day_count(), 7);
        assert_eq!(
            DateRange::create(d(2026, 2, 26), d(2026, 3, 4)).day_count(),
            7
        );
        assert_eq!(
            DateRange::create(d(2025, 12, 30), d(2026, 1, 2)).day_count(),
            4
        );
        assert_eq!(DateRange::create(TODAY, TODAY).day_count(), 1);
    }

    #[test]
    fn the_presets_count_from_today() {
        use DatePickerWeekStart::{Monday, Sunday};
        use DateRangePreset as P;
        assert_eq!(span(P::Today, Monday), (TODAY, TODAY));
        assert_eq!(span(P::Yesterday, Monday), (d(2026, 3, 3), d(2026, 3, 3)));
        assert_eq!(span(P::Last7Days, Monday), (d(2026, 2, 26), TODAY));
        assert_eq!(span(P::Last30Days, Monday), (d(2026, 2, 3), TODAY));
        assert_eq!(span(P::ThisMonth, Monday), (d(2026, 3, 1), d(2026, 3, 31)));
        assert_eq!(span(P::LastMonth, Monday), (d(2026, 2, 1), d(2026, 2, 28)));
        assert_eq!(span(P::ThisYear, Monday), (d(2026, 1, 1), d(2026, 12, 31)));
        assert_eq!(span(P::LastYear, Monday), (d(2025, 1, 1), d(2025, 12, 31)));
        assert_eq!(span(P::ThisWeek, Sunday), (d(2026, 3, 1), d(2026, 3, 7)));
    }

    #[test]
    fn the_week_presets_start_on_the_week_start() {
        use DateRangePreset as P;
        let monday = DatePickerWeekStart::Monday;
        assert_eq!(span(P::ThisWeek, monday), (d(2026, 3, 2), d(2026, 3, 8)));
        assert_eq!(span(P::LastWeek, monday), (d(2026, 2, 23), d(2026, 3, 1)));
        assert_eq!(
            span(P::LastWeek, DatePickerWeekStart::Sunday),
            (d(2026, 2, 22), d(2026, 2, 28))
        );
    }

    #[test]
    fn the_presets_cross_the_new_year() {
        let new_year = d(2026, 1, 1);
        let r = DateRangePreset::Yesterday.range(new_year, DatePickerWeekStart::Monday);
        assert_eq!((r.start, r.end), (d(2025, 12, 31), d(2025, 12, 31)));
        let r = DateRangePreset::LastMonth.range(new_year, DatePickerWeekStart::Monday);
        assert_eq!((r.start, r.end), (d(2025, 12, 1), d(2025, 12, 31)));
    }

    #[test]
    fn the_first_click_anchors_and_the_second_picks_either_way_round() {
        let view = DateRangePickerView::create(2026, 3);
        let (view, kind) = click_day(view, d(2026, 3, 10));
        assert_eq!(kind, DateRangePickerEventKind::Anchored);
        assert_eq!(view.anchor, OptionDatePickerState::Some(d(2026, 3, 10)));
        let (view, kind) = click_day(view, d(2026, 3, 4));
        assert_eq!(kind, DateRangePickerEventKind::Picked);
        assert_eq!(view.anchor, OptionDatePickerState::None);
        assert_eq!(
            view.range,
            OptionDateRange::Some(DateRange::create(d(2026, 3, 4), d(2026, 3, 10)))
        );
        assert_eq!((view.year, view.month), (2026, 3), "picking turns no month");
    }

    #[test]
    fn anchoring_keeps_the_range_picked_before_until_the_second_click() {
        let mut view = DateRangePickerView::create(2026, 3);
        let before = DateRange::create(d(2026, 3, 1), d(2026, 3, 2));
        view.range = OptionDateRange::Some(before);
        let (view, _) = click_day(view, d(2026, 3, 20));
        assert_eq!(
            view.range,
            OptionDateRange::Some(before),
            "Escape shows it again"
        );
    }

    #[test]
    fn the_grids_preview_the_span_from_the_anchor_to_the_pointer() {
        let mut view = DateRangePickerView::create(2026, 3);
        let picked = DateRange::create(d(2026, 3, 1), d(2026, 3, 2));
        view.range = OptionDateRange::Some(picked);
        assert_eq!(
            shown_range(&view, Some(d(2026, 3, 9))),
            Some(picked),
            "no anchor: the range"
        );
        view.anchor = OptionDatePickerState::Some(d(2026, 3, 10));
        assert_eq!(
            shown_range(&view, Some(d(2026, 3, 5))),
            Some(DateRange::create(d(2026, 3, 5), d(2026, 3, 10)))
        );
        assert_eq!(
            shown_range(&view, None),
            Some(DateRange::create(d(2026, 3, 10), d(2026, 3, 10)))
        );
    }

    #[test]
    fn a_range_turns_the_months_so_it_shows() {
        let left = |a, b| {
            let v = DateRangePickerView::with_range(DateRange::create(a, b));
            (v.year, v.month)
        };
        assert_eq!(left(d(2026, 3, 4), d(2026, 3, 10)), (2026, 3));
        assert_eq!(left(d(2026, 3, 20), d(2026, 4, 5)), (2026, 3));
        assert_eq!(
            left(d(2026, 1, 10), d(2026, 3, 5)),
            (2026, 2),
            "the end's month on the right"
        );
        assert_eq!(left(d(2025, 12, 28), d(2026, 1, 3)), (2025, 12));
        let v = DateRangePickerView::with_range(DateRange::create(d(2026, 3, 4), d(2026, 3, 10)));
        assert_eq!(v.anchor, OptionDatePickerState::None);
        assert!(v.range.is_some());
    }

    #[test]
    fn the_months_turn_across_years() {
        let dec = DateRangePickerView::create(2025, 12);
        assert_eq!(dec.right_month(), (2026, 1));
        let jan = dec.turned(1);
        assert_eq!((jan.year, jan.month), (2026, 1));
        let back = jan.turned(-1);
        assert_eq!((back.year, back.month), (2025, 12));
        let far = jan.turned(-13);
        assert_eq!((far.year, far.month), (2024, 12));
    }

    #[test]
    fn a_range_reads_as_its_two_days() {
        assert_eq!(
            range_text(&DateRange::create(d(2026, 3, 4), d(2026, 3, 10))),
            "4 Mar 2026 \u{2013} 10 Mar 2026"
        );
        assert_eq!(range_text(&DateRange::create(TODAY, TODAY)), "4 Mar 2026");
    }
}
