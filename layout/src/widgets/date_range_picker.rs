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
//! Down turn the months, Shift+Page Up / Down a year. The presets are
//! buttons and ONE Tab stop: Up / Down walk them, Home / End go to the
//! ends; the stop rests on the preset whose span is picked (else the
//! first).
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
    /// What happened.
    pub kind: DateRangePickerEventKind,
    /// The view after the action - the app stores it and rebuilds.
    pub view: DateRangePickerView,
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

/// The summary line under the months: the range shown, or what to do.
#[must_use]
pub(crate) fn summary_text(view: &DateRangePickerView, over: Option<DatePickerState>) -> String {
    match (view.anchor.is_some(), shown_range(view, over)) {
        (true, Some(r)) if r.start == r.end => {
            format!("{} \u{2013} pick the last day", day_text(r.start))
        }
        (_, Some(r)) => range_text(&r),
        (_, None) => String::from("Pick the first day"),
    }
}

// ==== the look ====

/// What a theme adds to the date picker's calendars for a range picker: the
/// row, the presets column, one preset, the summary line. Built by
/// `themes::flat::date_range_picker_skin` and `themes::flora::..`.
#[derive(Debug, Clone)]
pub(crate) struct DateRangePickerSkin {
    /// The root row.
    pub(crate) root: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
    /// The presets column.
    pub(crate) presets: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
    /// One preset button (a Tab stop: it owes the focus ring).
    pub(crate) preset: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
    /// The summary line.
    pub(crate) summary: Vec<azul_css::dynamic_selector::CssPropertyWithConditions>,
}

/// The date picker's look for the range picker's calendars: the pinned
/// theme's, or - unpinned - flat's and flora's merged part by part
/// (`theme_blocks::part_of`), so the DOM is built once and follows the app
/// theme. The marker is the structure theme's.
pub(crate) fn calendar_look(theme: OptionUiTheme) -> crate::widgets::date_picker::DatePickerLook {
    use crate::widgets::{
        date_picker::DatePickerLook,
        themes::{
            flat, flora,
            theme_blocks::{part_of, skins_of, structure_skin},
        },
    };
    let looks = skins_of(theme, flat::date_picker_look, flora::date_picker_look);
    macro_rules! merged {
        ($part:ident) => {
            part_of(&looks, |l: &DatePickerLook| l.$part.clone()).into_library_owned_vec()
        };
    }
    DatePickerLook {
        field: merged!(field),
        field_value: merged!(field_value),
        field_icon: merged!(field_icon),
        panel: merged!(panel),
        header: merged!(header),
        nav: merged!(nav),
        header_label: merged!(header_label),
        row: merged!(row),
        weekday: merged!(weekday),
        grid: merged!(grid),
        blank: merged!(blank),
        day_selected: merged!(day_selected),
        day_other: merged!(day_other),
        day_today: merged!(day_today),
        day_in_range: merged!(day_in_range),
        marker: structure_skin(&looks, theme).and_then(|l| l.marker),
    }
}

// ==== the DOM ====

/// What every handler of one picker shares.
pub(crate) struct RangeShared {
    pub(crate) view: DateRangePickerView,
    pub(crate) today: DatePickerState,
    pub(crate) week_start: DatePickerWeekStart,
    pub(crate) on_event: OptionDateRangePickerOnEvent,
    /// The faces the day cells were built in (both themes' blocks when the
    /// picker follows the app theme): what a repaint writes.
    pub(crate) faces: crate::widgets::date_picker::CellFaces,
}

/// One day cell's payload: its date and the picker's shared state.
struct RangeDayData {
    date: DatePickerState,
    shared: RefAny,
}

/// One preset button's payload.
struct PresetData {
    preset: DateRangePreset,
    shared: RefAny,
}

/// The face a day wears: the picked face at the ends of `shown`, the wash
/// between them, the plain face elsewhere; today's ring over whichever.
fn day_face(
    date: DatePickerState,
    shown: Option<DateRange>,
    today: DatePickerState,
    faces: &crate::widgets::date_picker::CellFaces,
) -> azul_css::dynamic_selector::CssPropertyWithConditionsVec {
    // The faces are merged parts (both themes' blocks when the picker
    // follows the app theme): a state part goes ON its base with
    // `stack_parts`, never by appending (a themed base declaration would
    // outrank a shared one appended after it).
    use crate::widgets::themes::theme_blocks::stack_parts;
    let face = match shown {
        Some(r) if date == r.start || date == r.end => faces.selected.clone(),
        Some(r) if r.contains(date) => stack_parts(&faces.other, &faces.in_range),
        _ => faces.other.clone(),
    };
    if date == today {
        stack_parts(&face, &faces.today)
    } else {
        face
    }
}

/// `n` px wide, never shrinking: the spacer opposite a header's one arrow.
fn spacer(px: f32) -> azul_core::dom::Dom {
    use crate::widgets::themes::decl;
    azul_core::dom::Dom::create_div().with_css_props(
        azul_css::dynamic_selector::CssPropertyWithConditionsVec::from_vec(alloc::vec![
            decl::px_width(px),
            decl::no_shrink(),
        ]),
    )
}

/// A class list of one.
fn one_class(name: &'static str) -> azul_core::dom::IdOrClassVec {
    azul_core::dom::IdOrClassVec::from_vec(alloc::vec![azul_core::dom::IdOrClass::Class(
        AzString::from_const_str(name)
    )])
}

impl DateRangePicker {
    /// Renders the picker: the presets, the two months, the summary line -
    /// the calendars in the date picker's look, the rest in the theme's
    /// range-picker skin (pinned, or both merged to follow the app theme).
    #[must_use]
    pub fn dom(self) -> azul_core::dom::Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks::skins_of};
        let skins = skins_of(
            self.theme,
            flat::date_range_picker_skin,
            flora::date_range_picker_skin,
        );
        let look = calendar_look(self.theme);
        self.build(&skins, &look)
    }

    /// The DOM in `skins` and the calendar `look`.
    pub(crate) fn build(
        self,
        skins: &[DateRangePickerSkin],
        look: &crate::widgets::date_picker::DatePickerLook,
    ) -> azul_core::dom::Dom {
        use azul_core::{
            a11y::{AccessibilityInfo, AccessibilityRole},
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec, TabIndex},
            events::FocusEventFilter,
            refany::OptionRefAny,
        };
        use azul_css::{
            dynamic_selector::CssPropertyWithConditionsVec,
            props::layout::{LayoutAlignItems, LayoutFlexDirection},
        };

        use crate::widgets::{
            date_picker::{
                build_weekday_row_from, day_accessibility_name, day_grid, header_nav_button,
                HEADER_CLASS, HEADER_LABEL_CLASS, NEXT_ARROW, PREV_ARROW,
            },
            themes::{decl, theme_blocks::part_of},
        };

        let faces = look.cell_faces(&[]);
        let view = self.view;
        let today = self.today;
        let shown = shown_range(&view, None);
        let shared = RefAny::new(RangeShared {
            view,
            today,
            week_start: self.week_start,
            on_event: self.on_event.clone(),
            faces: faces.clone(),
        });

        // The one Tab stop of both grids: the anchor, the range's start, or
        // the left month's 1st - where it shows.
        let (ry, rm) = view.right_month();
        let in_view = |d: &DatePickerState| {
            (d.year, d.month) == (view.year, view.month) || (d.year, d.month) == (ry, rm)
        };
        let stop = view
            .anchor
            .into_option()
            .or_else(|| shown.map(|r| r.start))
            .filter(in_view)
            .unwrap_or(DatePickerState {
                year: view.year,
                month: view.month,
                day: 1,
            });

        let flex_row = |gap: isize| {
            alloc::vec![
                decl::display_flex(),
                decl::flex_direction(LayoutFlexDirection::Row),
                decl::simple(azul_css::props::property::CssProperty::const_align_items(
                    LayoutAlignItems::Start
                )),
                decl::simple(azul_css::props::property::CssProperty::ColumnGap(
                    azul_css::props::property::LayoutColumnGapValue::Exact(
                        azul_css::props::layout::LayoutColumnGap {
                            inner: azul_css::props::basic::PixelValue::const_px(gap),
                        }
                    )
                )),
            ]
        };
        let flex_column = |gap: isize| {
            alloc::vec![
                decl::display_flex(),
                decl::flex_direction(LayoutFlexDirection::Column),
                decl::simple(azul_css::props::property::CssProperty::RowGap(
                    azul_css::props::property::LayoutRowGapValue::Exact(
                        azul_css::props::layout::LayoutRowGap {
                            inner: azul_css::props::basic::PixelValue::const_px(gap),
                        }
                    )
                )),
            ]
        };

        // ---- the two months ----
        let mut months: Vec<Dom> = Vec::with_capacity(2);
        for (k, (year, month)) in [(view.year, view.month), (ry, rm)].into_iter().enumerate() {
            let label = crate::widgets::widget_p_with_text(AzString::from(format!(
                "{} {}",
                crate::widgets::date_picker::month_name(month),
                year
            )))
            .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_LABEL_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(
                look.header_label.clone(),
            ));
            let header_kids = if k == 0 {
                alloc::vec![
                    header_nav_button(
                        PREV_ARROW,
                        "Previous month",
                        on_range_prev as usize,
                        shared.clone(),
                        look
                    ),
                    label,
                    spacer(24.0),
                ]
            } else {
                alloc::vec![
                    spacer(24.0),
                    label,
                    header_nav_button(
                        NEXT_ARROW,
                        "Next month",
                        on_range_next as usize,
                        shared.clone(),
                        look
                    ),
                ]
            };
            let header = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(look.header.clone()))
                .with_children(header_kids.into());

            let grid = day_grid(year, month, self.week_start, look, &mut |day| {
                let date = DatePickerState { year, month, day };
                let name = if date == today {
                    AzString::from(format!(
                        "{}, today",
                        day_accessibility_name(year, month, day).as_str()
                    ))
                } else {
                    day_accessibility_name(year, month, day)
                };
                let data = RefAny::new(RangeDayData {
                    date,
                    shared: shared.clone(),
                });
                let callback = |event: EventFilter, cb: usize| CoreCallbackData {
                    event,
                    callback: CoreCallback {
                        cb,
                        ctx: OptionRefAny::None,
                    },
                    refany: data.clone(),
                };
                crate::widgets::widget_p_with_text(AzString::from(format!("{day}")))
                    .with_ids_and_classes(one_class(DATE_RANGE_DAY_CLASS))
                    .with_css_props(day_face(date, shown, today, &faces))
                    .with_callbacks(
                        alloc::vec![
                            callback(
                                EventFilter::Hover(HoverEventFilter::Click),
                                on_range_day_click as usize
                            ),
                            callback(
                                EventFilter::Hover(HoverEventFilter::MouseEnter),
                                on_range_day_hover as usize
                            ),
                            callback(
                                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                                on_range_day_key as usize
                            ),
                        ]
                        .into(),
                    )
                    .with_accessibility_info(AccessibilityInfo {
                        role: AccessibilityRole::PushButton,
                        accessibility_name: OptionString::Some(name),
                        ..Default::default()
                    })
                    .with_tab_index(if date == stop {
                        TabIndex::Auto
                    } else {
                        TabIndex::NoKeyboardFocus
                    })
                    .with_key((DATE_RANGE_DAY_CLASS, year, month, day))
            });

            let mut month_classes: Vec<IdOrClass> = alloc::vec![IdOrClass::Class(
                AzString::from_const_str(DATE_RANGE_MONTH_CLASS)
            )];
            if let Some(marker) = look.marker {
                month_classes.push(IdOrClass::Class(AzString::from_const_str(marker)));
            }
            months.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_vec(month_classes))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(look.panel.clone()))
                    .with_children(
                        alloc::vec![header, build_weekday_row_from(self.week_start, look), grid,]
                            .into(),
                    ),
            );
        }
        let months_row = Dom::create_div()
            .with_ids_and_classes(one_class(DATE_RANGE_MONTHS_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(flex_row(12)))
            .with_children(months.into());

        // ---- the summary: the range in words, a live region ----
        let summary = crate::widgets::widget_p_with_text(AzString::from(summary_text(&view, None)))
            .with_ids_and_classes(one_class(DATE_RANGE_SUMMARY_CLASS))
            .with_css_props(part_of(skins, |s| s.summary.clone()))
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::StaticText,
                is_live_region: true,
                ..Default::default()
            });
        let body = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(flex_column(6)))
            .with_children(alloc::vec![months_row, summary].into());

        // ---- the presets ----
        let mut row_kids: Vec<Dom> = Vec::with_capacity(2);
        if !self.presets.as_slice().is_empty() {
            // The presets are ONE Tab stop (a roving group, the arrows walk
            // them): the preset whose span is the range picked, else the first.
            let week_start = self.week_start;
            let picked = view.range.into_option();
            let preset_stop = crate::widgets::roving::stop_index(
                self.presets
                    .as_slice()
                    .iter()
                    .position(|p| Some(p.range(today, week_start)) == picked),
                self.presets.as_slice().len(),
            );
            let items: Vec<Dom> = self
                .presets
                .as_slice()
                .iter()
                .enumerate()
                .map(|(position, preset)| {
                    let data = RefAny::new(PresetData {
                        preset: *preset,
                        shared: shared.clone(),
                    });
                    crate::widgets::widget_p_with_text(AzString::from_const_str(preset.label()))
                        .with_ids_and_classes(one_class(DATE_RANGE_PRESET_CLASS))
                        .with_css_props(part_of(skins, |s| {
                            let mut v = alloc::vec![decl::simple(
                                azul_css::props::property::CssProperty::const_cursor(
                                    azul_css::props::style::StyleCursor::Pointer
                                )
                            )];
                            v.extend(s.preset.iter().cloned());
                            v
                        }))
                        .with_callbacks(
                            alloc::vec![
                                CoreCallbackData {
                                    event: EventFilter::Hover(HoverEventFilter::Click),
                                    callback: CoreCallback {
                                        cb: on_range_preset as usize,
                                        ctx: OptionRefAny::None,
                                    },
                                    refany: data.clone(),
                                },
                                CoreCallbackData {
                                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                                    callback: CoreCallback {
                                        cb: on_range_preset_key as usize,
                                        ctx: OptionRefAny::None,
                                    },
                                    refany: data,
                                },
                            ]
                            .into(),
                        )
                        .with_tab_index(crate::widgets::roving::item_tab_index(position, preset_stop))
                        .with_accessibility_info(AccessibilityInfo {
                            role: AccessibilityRole::PushButton,
                            accessibility_name: OptionString::Some(AzString::from_const_str(
                                preset.label(),
                            )),
                            ..Default::default()
                        })
                })
                .collect();
            row_kids.push(
                Dom::create_div()
                    .with_ids_and_classes(one_class(DATE_RANGE_PRESETS_CLASS))
                    .with_css_props(part_of(skins, |s| {
                        let mut v = flex_column(2);
                        v.extend(s.presets.iter().cloned());
                        v
                    }))
                    .with_accessibility_info(AccessibilityInfo {
                        role: AccessibilityRole::List,
                        accessibility_name: OptionString::Some(AzString::from_const_str("Presets")),
                        ..Default::default()
                    })
                    .with_children(items.into()),
            );
        }
        row_kids.push(body);

        let mut classes: Vec<IdOrClass> = alloc::vec![IdOrClass::Class(AzString::from_const_str(
            DATE_RANGE_PICKER_CLASS
        ))];
        if let Some(marker) = look.marker {
            classes.push(IdOrClass::Class(AzString::from_const_str(marker)));
        }
        let name = self.accessibility_name.clone();
        crate::widgets::warn_widget_needs_a_name("DateRangePicker", name.is_some());
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(part_of(skins, |s| {
                let mut v = flex_row(16);
                v.extend(s.root.iter().cloned());
                v
            }))
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::Grouping,
                accessibility_name: name,
                accessibility_value: OptionString::Some(AzString::from(summary_text(&view, None))),
                ..Default::default()
            })
            .with_children(row_kids.into())
    }
}

impl From<DateRangePicker> for azul_core::dom::Dom {
    fn from(p: DateRangePicker) -> Self {
        p.dom()
    }
}

// ==== the handlers ====

use azul_core::dom::DomNodeId;

/// Every day cell of both months shown, in order, with its date: from any
/// day cell up to the months row (day -> week row -> grid -> month -> row),
/// then each month's grid (its last child) by position. `None` when `cell`
/// is not in a range picker's grid.
fn days_around(
    info: &CallbackInfo,
    cell: DomNodeId,
    view: &DateRangePickerView,
) -> Option<(DomNodeId, Vec<(DomNodeId, DatePickerState)>)> {
    let row = info.get_parent(cell)?;
    let grid = info.get_parent(row)?;
    let month = info.get_parent(grid)?;
    let months = info.get_parent(month)?;
    let (ry, rm) = view.right_month();
    let mut out = Vec::with_capacity(62);
    let mut calendar = info.get_first_child(months);
    for (year, month) in [(view.year, view.month), (ry, rm)] {
        let Some(cal) = calendar else { break };
        if let Some(grid) = info.get_last_child(cal) {
            let mut day = 1;
            let mut week = info.get_first_child(grid);
            while let Some(w) = week {
                let mut c = info.get_first_child(w);
                while let Some(node) = c {
                    if info.get_first_child(node).is_some() {
                        out.push((node, DatePickerState { year, month, day }));
                        day += 1;
                    }
                    c = info.get_next_sibling(node);
                }
                week = info.get_next_sibling(w);
            }
        }
        calendar = info.get_next_sibling(cal);
    }
    Some((months, out))
}

/// Repaints both grids for `view` with the pointer (or focus) on `over`, and
/// rewrites the summary line - in place, no rebuild.
fn repaint(
    info: &mut CallbackInfo,
    cell: DomNodeId,
    shared: &RangeShared,
    over: Option<DatePickerState>,
) {
    let Some((months, days)) = days_around(info, cell, &shared.view) else {
        return;
    };
    let shown = shown_range(&shared.view, over);
    for (node, date) in days {
        info.set_node_style(
            node,
            day_face(date, shown, shared.today, &shared.faces).into(),
        );
    }
    if let Some(summary) = info.get_next_sibling(months) {
        if let Some(text) = info.get_first_child(summary) {
            info.change_node_text(text, AzString::from(summary_text(&shared.view, over)));
        }
    }
}

/// Reports `kind` with the stored (next) view to the app's hook.
fn report(
    shared: &mut RefAny,
    info: CallbackInfo,
    kind: DateRangePickerEventKind,
    preset: DateRangePreset,
) -> Update {
    let (view, hook) = match shared.downcast_ref::<RangeShared>() {
        Some(s) => (s.view, s.on_event.clone()),
        None => return Update::DoNothing,
    };
    match hook.as_ref() {
        Some(DateRangePickerOnEvent { callback, refany }) => callback.invoke(
            refany.clone(),
            info,
            DateRangePickerEvent { view, kind, preset },
        ),
        None => Update::DoNothing,
    }
}

/// The payload of a day cell: its date and the shared state.
fn day_of(data: &mut RefAny) -> Option<(DatePickerState, RefAny)> {
    let d = data.downcast_ref::<RangeDayData>()?;
    Some((d.date, d.shared.clone()))
}

/// A click on a day: the first anchors, the second picks (module docs).
extern "C" fn on_range_day_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let cell = info.get_hit_node();
    let Some((date, mut shared)) = day_of(&mut data) else {
        return Update::DoNothing;
    };
    let kind = {
        let Some(mut s) = shared.downcast_mut::<RangeShared>() else {
            return Update::DoNothing;
        };
        let (view, kind) = click_day(s.view, date);
        s.view = view;
        repaint(&mut info, cell, &s, Some(date));
        kind
    };
    report(&mut shared, info, kind, DateRangePreset::default())
}

/// The pointer over a day while a range is being picked: the preview.
extern "C" fn on_range_day_hover(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let cell = info.get_hit_node();
    let Some((date, mut shared)) = day_of(&mut data) else {
        return Update::DoNothing;
    };
    if let Some(s) = shared.downcast_ref::<RangeShared>() {
        if s.view.anchor.is_some() {
            repaint(&mut info, cell, &s, Some(date));
        }
    }
    Update::DoNothing
}

/// The keys on the focused day (module docs): the arrows across both
/// months, Page Up / Down and an arrow past the months turn them, Escape
/// drops the anchor. Enter / Space are the click.
extern "C" fn on_range_day_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    let ks = info.get_current_keyboard_state();
    // Shift+Page Up / Down turn a year - the one chord the days claim
    // (`plain_key` leaves every chord to the OS and the app).
    let year_turn = !(ks.alt_down() || ks.ctrl_down() || ks.super_down())
        && ks.shift_down()
        && matches!(
            ks.current_virtual_keycode.into_option(),
            Some(K::PageUp | K::PageDown)
        );
    let key = if year_turn {
        ks.current_virtual_keycode.into_option()
    } else {
        crate::widgets::roving::plain_key(&ks)
    };
    let Some(key) = key else {
        return Update::DoNothing;
    };
    let cell = info.get_hit_node();
    let Some((date, mut shared)) = day_of(&mut data) else {
        return Update::DoNothing;
    };
    let turn = |shared: &mut RefAny, info: &mut CallbackInfo, delta: i32| {
        if let Some(mut s) = shared.downcast_mut::<RangeShared>() {
            s.view = s.view.turned(delta);
        }
        info.prevent_default();
    };
    match key {
        K::PageUp | K::PageDown => {
            let months = if year_turn { 12 } else { 1 };
            turn(
                &mut shared,
                &mut info,
                if key == K::PageUp { -months } else { months },
            );
            return report(
                &mut shared,
                info,
                DateRangePickerEventKind::Navigated,
                DateRangePreset::default(),
            );
        }
        K::Escape => {
            let had_anchor = {
                let Some(mut s) = shared.downcast_mut::<RangeShared>() else {
                    return Update::DoNothing;
                };
                let had = s.view.anchor.is_some();
                if had {
                    s.view.anchor = OptionDatePickerState::None;
                    repaint(&mut info, cell, &s, None);
                }
                had
            };
            if !had_anchor {
                return Update::DoNothing;
            }
            info.prevent_default();
            return report(
                &mut shared,
                info,
                DateRangePickerEventKind::Cancelled,
                DateRangePreset::default(),
            );
        }
        K::Left | K::Right | K::Up | K::Down | K::Home | K::End => {}
        _ => return Update::DoNothing,
    }

    let view = match shared.downcast_ref::<RangeShared>() {
        Some(s) => s.view,
        None => return Update::DoNothing,
    };
    let Some((_, days)) = days_around(&info, cell, &view) else {
        return Update::DoNothing;
    };
    let Some(current) = days.iter().position(|(_, d)| *d == date) else {
        return Update::DoNothing;
    };
    // Home / End: the ends of the focused day's week row, found by its
    // weekday column.
    let column = {
        let start = match shared.downcast_ref::<RangeShared>().map(|s| s.week_start) {
            Some(DatePickerWeekStart::Monday) => 1,
            _ => 0,
        };
        let wd = crate::widgets::date_picker::weekday(date.year, date.month, date.day);
        ((wd + 7 - start) % 7) as usize
    };
    let target = match key {
        K::Left => current.checked_sub(1),
        K::Right => Some(current + 1),
        K::Up => current.checked_sub(7),
        K::Down => Some(current + 7),
        K::Home => Some(current - column.min(current)),
        K::End => Some(current + (6 - column)),
        _ => None,
    };
    info.prevent_default();
    match target.filter(|t| *t < days.len()) {
        Some(t) => {
            let nodes: Vec<DomNodeId> = days.iter().map(|(n, _)| *n).collect();
            crate::widgets::roving::move_stop(&mut info, &nodes, t);
            if let Some(s) = shared.downcast_ref::<RangeShared>() {
                if s.view.anchor.is_some() {
                    repaint(&mut info, cell, &s, Some(days[t].1));
                }
            }
            Update::DoNothing
        }
        None if matches!(key, K::Left | K::Right | K::Up | K::Down) => {
            // Past the two months: they turn (the app rebuilds).
            let delta = if matches!(key, K::Left | K::Up) {
                -1
            } else {
                1
            };
            turn(&mut shared, &mut info, delta);
            report(
                &mut shared,
                info,
                DateRangePickerEventKind::Navigated,
                DateRangePreset::default(),
            )
        }
        None => Update::DoNothing,
    }
}

/// The header arrows: the months turn (the app rebuilds).
fn header_turn(mut data: RefAny, info: CallbackInfo, delta: i32) -> Update {
    if let Some(mut s) = data.downcast_mut::<RangeShared>() {
        s.view = s.view.turned(delta);
    } else {
        return Update::DoNothing;
    }
    report(
        &mut data,
        info,
        DateRangePickerEventKind::Navigated,
        DateRangePreset::default(),
    )
}

extern "C" fn on_range_prev(data: RefAny, info: CallbackInfo) -> Update {
    header_turn(data, info, -1)
}

extern "C" fn on_range_next(data: RefAny, info: CallbackInfo) -> Update {
    header_turn(data, info, 1)
}

/// A preset: its span is picked at once, the months turned so it shows.
extern "C" fn on_range_preset(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((preset, mut shared)) = data
        .downcast_ref::<PresetData>()
        .map(|p| (p.preset, p.shared.clone()))
    else {
        return Update::DoNothing;
    };
    {
        let Some(mut s) = shared.downcast_mut::<RangeShared>() else {
            return Update::DoNothing;
        };
        let range = preset.range(s.today, s.week_start);
        s.view = DateRangePickerView::with_range(range);
    }
    report(&mut shared, info, DateRangePickerEventKind::Preset, preset)
}

/// The keys on a focused preset: the presets are ONE Tab stop (module
/// docs), Up / Down walk them (the ends hold), Home / End go to the ends.
/// Enter / Space are the click.
extern "C" fn on_range_preset_key(_data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    use crate::widgets::roving::{self, Step};

    let Some(key) = roving::plain_key(&info.get_current_keyboard_state()) else {
        return Update::DoNothing;
    };
    let step = match key {
        K::Up => Step::Previous,
        K::Down => Step::Next,
        K::Home => Step::First,
        K::End => Step::Last,
        _ => return Update::DoNothing,
    };
    let preset = info.get_hit_node();
    let Some(column) = info.get_parent(preset) else {
        return Update::DoNothing;
    };
    let items = roving::items_of(&info, column, DATE_RANGE_PRESET_CLASS);
    let Some(current) = items.iter().position(|n| *n == preset) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    if let Some(target) = roving::step_target(current, items.len(), step, false) {
        roving::move_stop(&mut info, &items, target);
    }
    Update::DoNothing
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

// ==== fixtures (the widget manifest's sample) ====

/// Samples for the widget manifest (`widgets::label_convention`) and the
/// tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Wednesday, 4 March 2026.
    pub(crate) const TODAY: DatePickerState = DatePickerState {
        year: 2026,
        month: 3,
        day: 4,
    };

    /// 4 - 10 March 2026 picked, March and April shown, Monday-first,
    /// named "Report period".
    pub(crate) fn sample() -> DateRangePicker {
        let range = DateRange::create(
            TODAY,
            DatePickerState {
                year: 2026,
                month: 3,
                day: 10,
            },
        );
        DateRangePicker::create(DateRangePickerView::with_range(range), TODAY)
            .with_week_start(DatePickerWeekStart::Monday)
            .with_accessibility_name("Report period")
    }

    /// Nothing picked yet, March and April shown.
    pub(crate) fn empty() -> DateRangePicker {
        DateRangePicker::create(DateRangePickerView::create(2026, 3), TODAY)
            .with_week_start(DatePickerWeekStart::Monday)
            .with_accessibility_name("Period")
    }
}

#[cfg(test)]
mod dom_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeType, TabIndex},
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::VirtualKeyCode,
    };

    use super::{fixtures::*, *};
    use crate::{
        callbacks::CallbackChange,
        widgets::{
            roving::test_support as rv,
            themes::{theme_blocks::checks, theme_checks as tc},
        },
    };

    type Log = Arc<Mutex<Vec<DateRangePickerEvent>>>;

    extern "C" fn record(
        mut data: RefAny,
        _info: CallbackInfo,
        event: DateRangePickerEvent,
    ) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    const fn d(year: u32, month: u32, day: u32) -> DatePickerState {
        DatePickerState { year, month, day }
    }

    fn node(i: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        }
    }

    /// The picker styled, with its log.
    fn styled(p: DateRangePicker) -> (StyledDom, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let p = p.with_theme(UiTheme::Flat).with_on_event(
            RefAny::new(log.clone()),
            record as DateRangePickerOnEventCallbackType,
        );
        (StyledDom::create_from_dom(p.dom()), log)
    }

    /// Every node with `class`, in order.
    fn with_class(styled: &StyledDom, class: &str) -> Vec<usize> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, n)| n.has_class(class))
            .map(|(i, _)| i)
            .collect()
    }

    /// The day cells: March's 31, then April's 30.
    fn days(styled: &StyledDom) -> Vec<usize> {
        with_class(styled, DATE_RANGE_DAY_CLASS)
    }

    fn texts(dom: &Dom) -> Vec<String> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| match n.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    fn styles_written(changes: &[CallbackChange]) -> usize {
        changes
            .iter()
            .filter(|c| matches!(c, CallbackChange::SetNodeStyle { .. }))
            .count()
    }

    fn texts_written(changes: &[CallbackChange]) -> Vec<String> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeText { text, .. } => Some(text.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    fn click(styled: &StyledDom, i: usize) -> Vec<CallbackChange> {
        rv::fire(styled, node(i), EventFilter::Hover(HoverEventFilter::Click))
            .expect("the node takes the click")
            .1
    }

    #[test]
    fn the_picker_shows_two_months_of_days() {
        let (s, _) = styled(sample());
        assert_eq!(days(&s).len(), 31 + 30);
        let all = texts(&sample().with_theme(UiTheme::Flat).dom());
        assert!(all.iter().any(|t| t == "March 2026"), "{all:?}");
        assert!(all.iter().any(|t| t == "April 2026"), "{all:?}");
    }

    #[test]
    fn the_days_of_both_months_are_one_tab_stop_on_the_ranges_start() {
        let (s, _) = styled(sample());
        let nodes = s.node_data.as_ref();
        let stops: Vec<usize> = days(&s)
            .into_iter()
            .filter(|i| nodes[*i].get_tab_index() == Some(TabIndex::Auto))
            .collect();
        assert_eq!(stops, vec![days(&s)[3]], "4 March holds the stop");
    }

    #[test]
    fn the_presets_and_the_summary_are_shown() {
        let (s, _) = styled(sample());
        assert_eq!(
            with_class(&s, DATE_RANGE_PRESET_CLASS).len(),
            DEFAULT_PRESETS.len()
        );
        let all = texts(&sample().with_theme(UiTheme::Flat).dom());
        assert!(all.iter().any(|t| t == "Last 7 days"));
        assert!(
            all.iter().any(|t| t == "4 Mar 2026 \u{2013} 10 Mar 2026"),
            "{all:?}"
        );
        let no_presets = sample()
            .with_presets(DateRangePresetVec::from_const_slice(&[]))
            .dom();
        assert!(tc::find(&no_presets, DATE_RANGE_PRESETS_CLASS).is_none());
    }

    #[test]
    fn two_clicks_anchor_then_pick_and_repaint_both_months_in_place() {
        let (s, log) = styled(empty());
        let all = days(&s);
        let changes = click(&s, all[9]); // 10 March
        assert_eq!(styles_written(&changes), 61, "every day repainted");
        assert!(texts_written(&changes)
            .iter()
            .any(|t| t == "10 Mar 2026 \u{2013} pick the last day"));
        let changes = click(&s, all[3]); // 4 March
        assert_eq!(styles_written(&changes), 61);
        assert!(texts_written(&changes)
            .iter()
            .any(|t| t == "4 Mar 2026 \u{2013} 10 Mar 2026"));
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, DateRangePickerEventKind::Anchored);
        assert_eq!(
            events[0].view.anchor,
            OptionDatePickerState::Some(d(2026, 3, 10))
        );
        assert_eq!(events[1].kind, DateRangePickerEventKind::Picked);
        assert_eq!(
            events[1].view.range,
            OptionDateRange::Some(DateRange::create(d(2026, 3, 4), d(2026, 3, 10)))
        );
    }

    #[test]
    fn the_pointer_previews_the_span_from_the_anchor_and_nothing_before() {
        let (s, log) = styled(empty());
        let all = days(&s);
        let hover = |i: usize| {
            rv::fire(
                &s,
                node(all[i]),
                EventFilter::Hover(HoverEventFilter::MouseEnter),
            )
            .expect("a day hears the pointer")
            .1
        };
        assert_eq!(styles_written(&hover(14)), 0, "no anchor, no preview");
        let _ = click(&s, all[9]);
        let changes = hover(31 + 4); // 5 April
        assert_eq!(styles_written(&changes), 61);
        assert!(texts_written(&changes)
            .iter()
            .any(|t| t == "10 Mar 2026 \u{2013} 5 Apr 2026"));
        assert_eq!(
            log.lock().expect("log").len(),
            1,
            "a preview reports nothing"
        );
    }

    #[test]
    fn escape_drops_the_anchor_and_says_so() {
        let (s, log) = styled(empty());
        let all = days(&s);
        let _ = click(&s, all[9]);
        let (_, changes) = rv::press(&s, node(all[9]), VirtualKeyCode::Escape, &[]).expect("keys");
        assert!(rv::prevented(&changes));
        let events = log.lock().expect("log").clone();
        assert_eq!(
            events.last().map(|e| e.kind),
            Some(DateRangePickerEventKind::Cancelled)
        );
        assert_eq!(
            events.last().map(|e| e.view.anchor),
            Some(OptionDatePickerState::None)
        );
        // Without an anchor Escape is not the picker's.
        let (_, changes) = rv::press(&s, node(all[9]), VirtualKeyCode::Escape, &[]).expect("keys");
        assert!(!rv::prevented(&changes));
    }

    #[test]
    fn page_down_and_the_arrows_past_the_months_turn_them() {
        let (s, log) = styled(sample());
        let all = days(&s);
        let _ = rv::press(&s, node(all[0]), VirtualKeyCode::PageDown, &[]).expect("keys");
        let last = log.lock().expect("log").last().copied().expect("an event");
        assert_eq!(last.kind, DateRangePickerEventKind::Navigated);
        assert_eq!((last.view.year, last.view.month), (2026, 4));
        // An arrow before 1 March turns back (a fresh picker: after a turn
        // the app rebuilds on the new months).
        let (s, log) = styled(sample());
        let all = days(&s);
        let _ = rv::press(&s, node(all[0]), VirtualKeyCode::Left, &[]).expect("keys");
        let last = log.lock().expect("log").last().copied().expect("an event");
        assert_eq!(last.kind, DateRangePickerEventKind::Navigated);
        assert_eq!((last.view.year, last.view.month), (2026, 2));
    }

    #[test]
    fn the_arrow_keys_cross_from_one_month_into_the_next() {
        let (s, _) = styled(sample());
        let all = days(&s);
        let (_, changes) = rv::press(&s, node(all[30]), VirtualKeyCode::Right, &[]).expect("keys"); // 31 March
        assert_eq!(
            rv::focus_request(&changes),
            Some(node(all[31])),
            "to 1 April"
        );
        let (_, changes) = rv::press(&s, node(all[3]), VirtualKeyCode::Down, &[]).expect("keys");
        assert_eq!(
            rv::focus_request(&changes),
            Some(node(all[10])),
            "a week down"
        );
    }

    #[test]
    fn the_presets_are_one_tab_stop_and_arrows_walk_them() {
        let (s, _) = styled(sample());
        let presets = with_class(&s, DATE_RANGE_PRESET_CLASS);
        let nodes = s.node_data.as_ref();
        let stops: Vec<usize> = presets
            .iter()
            .copied()
            .filter(|i| nodes[*i].get_tab_index() == Some(TabIndex::Auto))
            .collect();
        assert_eq!(stops, vec![presets[0]], "the first preset holds the presets' one Tab stop");
        let walk = |from: usize, key: VirtualKeyCode| {
            let (_, changes) = rv::press(&s, node(from), key, &[]).expect("a preset hears the arrows");
            assert!(rv::prevented(&changes), "{key:?} is the presets'");
            rv::focus_request(&changes)
        };
        let last = presets.len() - 1;
        assert_eq!(walk(presets[0], VirtualKeyCode::Down), Some(node(presets[1])), "Down: the next");
        assert_eq!(walk(presets[1], VirtualKeyCode::Up), Some(node(presets[0])), "Up: the one before");
        assert_eq!(walk(presets[0], VirtualKeyCode::End), Some(node(presets[last])), "End: the last");
        assert_eq!(walk(presets[last], VirtualKeyCode::Home), Some(node(presets[0])), "Home: the first");
    }

    #[test]
    fn shift_page_down_turns_a_year() {
        let turned = |key: VirtualKeyCode| {
            let (s, log) = styled(sample());
            let all = days(&s);
            let (_, changes) =
                rv::press(&s, node(all[0]), key, &[VirtualKeyCode::LShift]).expect("keys");
            assert!(rv::prevented(&changes), "Shift+{key:?} is the picker's");
            let last = log.lock().expect("log").last().copied().expect("an event");
            assert_eq!(last.kind, DateRangePickerEventKind::Navigated);
            (last.view.year, last.view.month)
        };
        assert_eq!(turned(VirtualKeyCode::PageDown), (2027, 3), "Shift+Page Down: a year on");
        assert_eq!(turned(VirtualKeyCode::PageUp), (2025, 3), "Shift+Page Up: a year back");
    }

    #[test]
    fn a_preset_picks_its_span_and_shows_its_months() {
        let (s, log) = styled(empty());
        let presets = with_class(&s, DATE_RANGE_PRESET_CLASS);
        let _ = click(&s, presets[2]); // Last 7 days
        let event = log.lock().expect("log").last().copied().expect("an event");
        assert_eq!(event.kind, DateRangePickerEventKind::Preset);
        assert_eq!(event.preset, DateRangePreset::Last7Days);
        assert_eq!(
            event.view.range,
            OptionDateRange::Some(DateRange::create(d(2026, 2, 26), d(2026, 3, 4)))
        );
        assert_eq!((event.view.year, event.view.month), (2026, 2));
    }

    #[test]
    fn the_picker_is_a_named_group_saying_its_range() {
        let dom = sample().dom();
        let info = dom.root.get_accessibility_info().expect("a group");
        assert_eq!(
            info.accessibility_name
                .as_ref()
                .map(|n| n.as_str().to_string()),
            Some(String::from("Report period"))
        );
        assert_eq!(
            info.accessibility_value
                .as_ref()
                .map(|n| n.as_str().to_string()),
            Some(String::from("4 Mar 2026 \u{2013} 10 Mar 2026"))
        );
    }

    #[test]
    fn a_date_range_picker_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "date range picker",
            || sample().dom(),
            |theme| sample().with_theme(theme).dom(),
        );
    }

    #[test]
    fn a_pinned_date_range_picker_keeps_its_theme_invariants() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            tc::assert_theme_invariants("date range picker", &sample().with_theme(theme).dom());
        }
    }
}
