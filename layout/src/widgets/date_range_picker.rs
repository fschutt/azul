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
        Self { start: a, end: b }
    }

    /// Whether `day` is in the span (both ends included).
    #[must_use]
    pub fn contains(&self, day: DatePickerState) -> bool {
        let _ = day;
        false
    }

    /// How many days the span has (1 for a single day).
    #[must_use]
    pub fn day_count(&self) -> u32 {
        0
    }
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
        let _ = week_start;
        DateRange::create(today, today)
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
        let _ = range;
        Self::create(2000, 1)
    }

    /// The months turned by `delta` (negative: back).
    #[must_use]
    pub fn turned(self, delta: i32) -> Self {
        let _ = delta;
        self
    }

    /// The month on the right: `(year, month)`.
    #[must_use]
    pub fn right_month(&self) -> (u32, u32) {
        (self.year, self.month)
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
    let _ = day;
    (view, DateRangePickerEventKind::Anchored)
}

/// The span the grids show in `view` with the pointer (or the focus) on
/// `over`: from the anchor to it while a range is being picked, else the
/// picked range.
#[must_use]
pub(crate) fn shown_range(
    view: &DateRangePickerView,
    over: Option<DatePickerState>,
) -> Option<DateRange> {
    let _ = (view, over);
    None
}

/// The range in words: "4 Mar 2026 - 10 Mar 2026", one day alone as
/// "4 Mar 2026".
#[must_use]
pub(crate) fn range_text(range: &DateRange) -> String {
    let _ = range;
    String::new()
}
