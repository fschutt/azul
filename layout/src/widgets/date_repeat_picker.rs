//! Date repeat picker widget - how an appointment or a to-do repeats: never,
//! daily, weekly (on chosen weekdays), monthly (on the first day's day of
//! the month, or on its nth weekday) or yearly; every N of them; ending
//! never, after a number of times or on a date - and, for a to-do, counted
//! from the day it is completed. Outlook's "Appointment Recurrence" and
//! Google Calendar's "Custom recurrence" as one form, row by row.
//!
//! COMPOSED, not drawn: a [`Segmented`] for the frequency, a
//! [`NumberInput`] for the interval, toggle [`Button`]s for the weekdays, a
//! [`Segmented`] for a month's day or weekday, a [`Segmented`] and a
//! [`NumberInput`] or a [`DatePicker`] for the end, a [`CheckBox`] for
//! "from completion". Each part folds its change into ONE
//! [`DateRepeatRule`] and reports that to the app, which keeps it and
//! rebuilds - the form shows only the rows its frequency needs.
//!
//! The rule speaks RFC 5545: [`DateRepeatRule::to_rrule`] is the `RRULE`
//! value an app stores or exports, [`DateRepeatRule::from_rrule`] reads one
//! back - the part of RRULE this form can show; any other rule is a custom
//! one the app keeps as it is (and says so beside the form).
//!
//! Key types: [`DateRepeatPicker`], [`DateRepeatRule`],
//! [`DateRepeatPickerOnChange`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::length::FloatValue,
        layout::{
            LayoutAlignItems, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutFlexWrap, LayoutMinWidth,
        },
        property::CssProperty,
        style::StyleUserSelect,
    },
    AzString, OptionString, StringVec,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        button::{Button, ButtonOnClickCallbackType},
        check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState},
        date_picker::{
            DatePicker, DatePickerOnChangeCallbackType, DatePickerState, DatePickerWeekStart,
        },
        number_input::{NumberInput, NumberInputOnValueChangeCallbackType, NumberInputState},
        segmented::{Segmented, SegmentedOnChangeCallbackType, SegmentedState},
        themes::{OptionUiTheme, UiTheme},
    },
};

/// The class of the editor (the column of rows).
pub const RECURRENCE_EDITOR_CLASS: &str = "__azul-native-date-repeat-picker";

static EDITOR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(RECURRENCE_EDITOR_CLASS))];
static ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-repeat-picker-row",
))];
static LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-repeat-picker-label",
))];
static UNIT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-repeat-picker-unit",
))];
static NUMBER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-repeat-picker-number",
))];
static WEEKDAYS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-date-repeat-picker-weekdays",
))];

/// The RRULE codes of the weekdays, Monday first (bit 0 of
/// [`DateRepeatRule::weekdays`] is Monday).
const WEEKDAY_CODES: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
/// What a weekday's toggle says.
const WEEKDAY_SHORT: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
/// A weekday's name, for the monthly choice.
const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
/// The frequency row's segments, in [`DateRepeatFrequency`] order.
const FREQUENCY_LABELS: [&str; 5] = ["Never", "Daily", "Weekly", "Monthly", "Yearly"];
/// The end row's segments, in [`DateRepeatEnd`] order.
const END_LABELS: [&str; 3] = ["Never", "After", "On"];
/// The largest interval and count the form takes.
const MAX_NUMBER: u32 = 999;
/// [`MAX_NUMBER`] for a number field.
const MAX_NUMBER_F32: f32 = 999.0;
/// The count an end "after a number of times" starts at.
const DEFAULT_COUNT: u32 = 10;
/// The frequencies, in the order of the frequency row's segments.
const FREQUENCIES: [DateRepeatFrequency; 5] = [
    DateRepeatFrequency::Never,
    DateRepeatFrequency::Daily,
    DateRepeatFrequency::Weekly,
    DateRepeatFrequency::Monthly,
    DateRepeatFrequency::Yearly,
];
/// The ends, in the order of the end row's segments.
const ENDS: [DateRepeatEnd; 3] = [
    DateRepeatEnd::Never,
    DateRepeatEnd::AfterCount,
    DateRepeatEnd::OnDate,
];
/// What the "from completion" box says.
const COMPLETION_LABEL: &str = "Repeat from the day it is completed";

/// How often a rule repeats.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum DateRepeatFrequency {
    /// It does not repeat.
    #[default]
    Never,
    /// Every `interval` days.
    Daily,
    /// Every `interval` weeks, on the chosen weekdays.
    Weekly,
    /// Every `interval` months, on the start's day or its nth weekday.
    Monthly,
    /// Every `interval` years, on the start's date.
    Yearly,
}

/// Which day of the month a monthly rule repeats on.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum DateRepeatMonthly {
    /// The start's day of the month ("on day 14").
    #[default]
    DayOfMonth,
    /// The start's nth weekday ("on the second Wednesday"; a fifth one is
    /// "the last").
    Weekday,
}

/// When a rule stops.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum DateRepeatEnd {
    /// It goes on.
    #[default]
    Never,
    /// After `count` occurrences, the first one included.
    AfterCount,
    /// On `until`, the last day an occurrence may fall on.
    OnDate,
}

/// What a [`DateRepeatPicker`] edits: a repeat rule, from its first
/// occurrence on.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DateRepeatRule {
    /// The first occurrence: an appointment's first day, a to-do's due
    /// date. The weekly, monthly and yearly choices are made from it.
    pub start: DatePickerState,
    /// The last day an occurrence may fall on (`end == OnDate`).
    pub until: DatePickerState,
    /// Every `interval` days, weeks, months or years (1 to 999).
    pub interval: u32,
    /// How many occurrences (`end == AfterCount`, 1 to 999).
    pub count: u32,
    pub frequency: DateRepeatFrequency,
    pub monthly: DateRepeatMonthly,
    pub end: DateRepeatEnd,
    /// The weekdays of a weekly rule: bit 0 Monday .. bit 6 Sunday. 0
    /// means the start's weekday.
    pub weekdays: u8,
    /// The next occurrence counts from the day the to-do was completed, not
    /// from its due date (RRULE has no word for it: the app keeps it).
    pub from_completion: bool,
}

azul_css::impl_option!(
    DateRepeatRule,
    OptionDateRepeatRule,
    [Debug, Copy, Clone, PartialEq, Eq]
);

impl Default for DateRepeatRule {
    fn default() -> Self {
        Self::create(DatePickerState::default())
    }
}

impl DateRepeatRule {
    /// A rule that does not repeat, from `start`: every 1, ending never
    /// (10 times, or on `start`, once an end is chosen).
    #[must_use]
    pub const fn create(start: DatePickerState) -> Self {
        Self {
            start,
            until: start,
            interval: 1,
            count: DEFAULT_COUNT,
            frequency: DateRepeatFrequency::Never,
            monthly: DateRepeatMonthly::DayOfMonth,
            end: DateRepeatEnd::Never,
            weekdays: 0,
            from_completion: false,
        }
    }

    /// The start's weekday, 0 = Monday .. 6 = Sunday.
    pub(crate) fn start_weekday(&self) -> usize {
        let sunday_based =
            crate::widgets::date_picker::weekday(self.start.year, self.start.month, self.start.day);
        ((sunday_based + 6) % 7) as usize
    }

    /// The weekdays a weekly rule repeats on (bit 0 Monday .. bit 6
    /// Sunday): the chosen ones, else the start's.
    pub(crate) fn effective_weekdays(&self) -> u8 {
        let chosen = self.weekdays & 0x7f;
        if chosen == 0 {
            1 << self.start_weekday()
        } else {
            chosen
        }
    }

    /// Which of its month's weekdays of its kind the start is: 1 to 4, or
    /// -1 (the last) for a fifth one, which most months do not have.
    pub(crate) fn start_nth(&self) -> i32 {
        let nth = i32::try_from(self.start.day.saturating_sub(1) / 7 + 1).unwrap_or(1);
        if nth >= 5 {
            -1
        } else {
            nth
        }
    }

    /// The rule as the value of an RFC 5545 `RRULE` (`FREQ=WEEKLY;
    /// INTERVAL=2;COUNT=10;BYDAY=MO,WE`), its parts in the order `FREQ`,
    /// `INTERVAL` (when not 1), `COUNT` / `UNTIL` (a date), `BYMONTHDAY` /
    /// `BYDAY`; empty when it does not repeat.
    #[must_use]
    pub fn to_rrule(&self) -> AzString {
        let freq = match self.frequency {
            DateRepeatFrequency::Never => return AzString::from_const_str(""),
            DateRepeatFrequency::Daily => "DAILY",
            DateRepeatFrequency::Weekly => "WEEKLY",
            DateRepeatFrequency::Monthly => "MONTHLY",
            DateRepeatFrequency::Yearly => "YEARLY",
        };
        let mut parts: Vec<String> = alloc::vec![alloc::format!("FREQ={freq}")];
        if self.interval > 1 {
            parts.push(alloc::format!("INTERVAL={}", self.interval));
        }
        match self.end {
            DateRepeatEnd::Never => {}
            DateRepeatEnd::AfterCount => {
                parts.push(alloc::format!("COUNT={}", self.count.max(1)));
            }
            DateRepeatEnd::OnDate => parts.push(alloc::format!(
                "UNTIL={:04}{:02}{:02}",
                self.until.year,
                self.until.month,
                self.until.day
            )),
        }
        match (self.frequency, self.monthly) {
            (DateRepeatFrequency::Weekly, _) => {
                let days = self.effective_weekdays();
                let codes: Vec<&str> = (0..7)
                    .filter(|i| days & (1 << i) != 0)
                    .map(|i| WEEKDAY_CODES[i])
                    .collect();
                parts.push(alloc::format!("BYDAY={}", codes.join(",")));
            }
            (DateRepeatFrequency::Monthly, DateRepeatMonthly::DayOfMonth) => {
                parts.push(alloc::format!("BYMONTHDAY={}", self.start.day));
            }
            (DateRepeatFrequency::Monthly, DateRepeatMonthly::Weekday) => {
                parts.push(alloc::format!(
                    "BYDAY={}{}",
                    self.start_nth(),
                    WEEKDAY_CODES[self.start_weekday()]
                ));
            }
            _ => {}
        }
        AzString::from(parts.join(";"))
    }

    /// The rule an RRULE value (with or without its `RRULE:` name) makes for
    /// an event or a to-do starting on `start`, when the form can show it;
    /// `None` for a rule it cannot (another frequency, `BYSETPOS`, days that
    /// are not the start's): the app keeps such a rule as it is. An empty
    /// value is a rule that does not repeat.
    #[must_use]
    pub fn from_rrule(rrule: AzString, start: DatePickerState) -> OptionDateRepeatRule {
        parse_rrule(rrule.as_str(), start).into()
    }
}

/// `20261231`, or the date of `20261231T235959` / `20261231T235959Z`.
fn parse_basic_date(text: &str) -> Option<DatePickerState> {
    let digits = text.get(..8)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let rest = &text[8..];
    if !(rest.is_empty() || rest.starts_with('T')) {
        return None;
    }
    let year: u32 = digits[..4].parse().ok()?;
    let month: u32 = digits[4..6].parse().ok()?;
    let day: u32 = digits[6..].parse().ok()?;
    if !(1..=12).contains(&month)
        || day == 0
        || day > crate::widgets::date_picker::days_in_month(year, month)
    {
        return None;
    }
    Some(DatePickerState { year, month, day })
}

/// A positive whole number of an RRULE part (`INTERVAL`, `COUNT`).
fn positive(text: &str) -> Option<u32> {
    text.trim().parse::<u32>().ok().filter(|n| *n > 0)
}

/// `value` (a `BYMONTH` / `BYMONTHDAY` list) is absent or exactly `want`.
fn absent_or(value: Option<&str>, want: u32) -> bool {
    value.is_none_or(|v| v.trim().parse::<u32>().ok() == Some(want))
}

/// What [`DateRepeatRule::from_rrule`] reads: the rule, when the form can
/// show it.
fn parse_rrule(text: &str, start: DatePickerState) -> Option<DateRepeatRule> {
    let mut rule = DateRepeatRule::create(start);
    let mut text = text.trim();
    if text.len() >= 6 && text.is_char_boundary(6) && text[..6].eq_ignore_ascii_case("RRULE:") {
        text = text[6..].trim();
    }
    if text.is_empty() {
        return Some(rule);
    }
    let mut freq = None;
    let mut by_day: Option<String> = None;
    let mut by_month_day: Option<&str> = None;
    let mut by_month: Option<&str> = None;
    for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (key, value) = part.split_once('=')?;
        let value = value.trim();
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                freq = Some(match value.to_ascii_uppercase().as_str() {
                    "DAILY" => DateRepeatFrequency::Daily,
                    "WEEKLY" => DateRepeatFrequency::Weekly,
                    "MONTHLY" => DateRepeatFrequency::Monthly,
                    "YEARLY" => DateRepeatFrequency::Yearly,
                    _ => return None,
                });
            }
            "INTERVAL" => rule.interval = positive(value)?.min(MAX_NUMBER),
            "COUNT" => {
                rule.count = positive(value)?.min(MAX_NUMBER);
                rule.end = DateRepeatEnd::AfterCount;
            }
            "UNTIL" => {
                rule.until = parse_basic_date(value)?;
                rule.end = DateRepeatEnd::OnDate;
            }
            "BYDAY" => by_day = Some(value.to_ascii_uppercase()),
            "BYMONTHDAY" => by_month_day = Some(value),
            "BYMONTH" => by_month = Some(value),
            // The week's first day changes nothing the form shows.
            "WKST" => {}
            _ => return None,
        }
    }
    rule.frequency = freq?;
    match rule.frequency {
        DateRepeatFrequency::Daily => {
            if by_day.is_some() || by_month_day.is_some() || by_month.is_some() {
                return None;
            }
        }
        DateRepeatFrequency::Weekly => {
            if by_month_day.is_some() || by_month.is_some() {
                return None;
            }
            if let Some(days) = by_day {
                let mut bits = 0u8;
                for code in days.split(',') {
                    let i = WEEKDAY_CODES.iter().position(|c| *c == code.trim())?;
                    bits |= 1 << i;
                }
                // The start's weekday alone is the rule's own default.
                rule.weekdays = if bits == 1 << rule.start_weekday() {
                    0
                } else {
                    bits
                };
            }
        }
        DateRepeatFrequency::Monthly => {
            if by_month.is_some() {
                return None;
            }
            match (by_day, by_month_day) {
                (None, day) if absent_or(day, start.day) => {
                    rule.monthly = DateRepeatMonthly::DayOfMonth;
                }
                (Some(day), None) => {
                    let split = day.len().checked_sub(2)?;
                    if !day.is_char_boundary(split) {
                        return None;
                    }
                    let (nth, code) = day.split_at(split);
                    let nth: i32 = nth.trim_start_matches('+').parse().ok()?;
                    if code != WEEKDAY_CODES[rule.start_weekday()] || nth != rule.start_nth() {
                        return None;
                    }
                    rule.monthly = DateRepeatMonthly::Weekday;
                }
                _ => return None,
            }
        }
        DateRepeatFrequency::Yearly => {
            if by_day.is_some()
                || !absent_or(by_month, start.month)
                || !absent_or(by_month_day, start.day)
            {
                return None;
            }
        }
        DateRepeatFrequency::Never => return None,
    }
    Some(rule)
}

/// One part of the form changed (what [`apply`] folds into the rule).
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum Part {
    /// The frequency row's segment.
    Frequency(usize),
    /// The interval as typed.
    Interval(f32),
    /// A weekday toggle (0 = Monday).
    Weekday(u32),
    /// The monthly row's segment.
    Monthly(usize),
    /// The end row's segment.
    End(usize),
    /// The count as typed.
    Count(f32),
    /// The last day.
    Until(DatePickerState),
    /// "From completion" ticked or cleared.
    Completion(bool),
}

/// Folds one part's change into `rule`.
pub(crate) fn apply(rule: &mut DateRepeatRule, part: Part) {
    match part {
        Part::Frequency(index) => {
            if let Some(frequency) = FREQUENCIES.get(index) {
                rule.frequency = *frequency;
            }
        }
        Part::Interval(typed) => {
            if let Some(n) = whole(typed) {
                rule.interval = n;
            }
        }
        Part::Weekday(day) => {
            if day < 7 {
                let toggled = rule.effective_weekdays() ^ (1 << day);
                // A weekly rule repeats on some day: the last one stays.
                if toggled != 0 {
                    rule.weekdays = toggled;
                }
            }
        }
        Part::Monthly(index) => match index {
            0 => rule.monthly = DateRepeatMonthly::DayOfMonth,
            1 => rule.monthly = DateRepeatMonthly::Weekday,
            _ => {}
        },
        Part::End(index) => {
            if let Some(end) = ENDS.get(index) {
                rule.end = *end;
                let until = (rule.until.year, rule.until.month, rule.until.day);
                if *end == DateRepeatEnd::OnDate
                    && until < (rule.start.year, rule.start.month, rule.start.day)
                {
                    rule.until = rule.start;
                }
            }
        }
        Part::Count(typed) => {
            if let Some(n) = whole(typed) {
                rule.count = n;
            }
        }
        Part::Until(day) => rule.until = day,
        Part::Completion(on) => rule.from_completion = on,
    }
}

/// A typed number as a whole number from 1 to 999; `None` for no number.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 1..=999 first
const fn whole(typed: f32) -> Option<u32> {
    if typed.is_nan() {
        return None;
    }
    Some(typed.round().clamp(1.0, MAX_NUMBER_F32) as u32)
}

/// Callback invoked when any part of the form changes; it is handed the
/// whole rule.
pub type DateRepeatPickerOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, DateRepeatRule) -> Update;
impl_widget_callback!(
    DateRepeatPickerOnChange,
    OptionDateRepeatPickerOnChange,
    DateRepeatPickerOnChangeCallback,
    DateRepeatPickerOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DateRepeatPickerOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RECURRENCE_EDITOR_ON_CHANGE_INVOKER,
    invoker_ty:     AzDateRepeatPickerOnChangeCallbackInvoker,
    thunk_fn:       az_date_repeat_picker_on_change_callback_thunk,
    setter_fn:      AzApp_setDateRepeatPickerOnChangeCallbackInvoker,
    from_handle_fn: AzDateRepeatPickerOnChangeCallback_createFromHostHandle,
    from_handle_byref_fn: AzDateRepeatPickerOnChangeCallback_createFromHostHandleByref,
    extra_args:     [ rule: DateRepeatRule ],
}

/// [`DateRepeatRule`] with the app's change callback: the state every part
/// of one editor shares.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DateRepeatPickerStateWrapper {
    pub inner: DateRepeatRule,
    pub on_change: OptionDateRepeatPickerOnChange,
}

/// The date repeat picker: a repeat rule, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DateRepeatPicker {
    pub state: DateRepeatPickerStateWrapper,
    /// What the editor is CALLED, for assistive technology ("Repeat" when
    /// unset).
    pub accessibility_name: OptionString,
    /// The widget theme this editor and its parts are PINNED to
    /// (`with_theme`), or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// The weekday the weekday toggles and the end date's calendar start on.
    pub week_start: DatePickerWeekStart,
    /// Show "Repeat from the day it is completed" (a to-do's choice).
    pub completion_option: bool,
    /// Show the "Ends" row (never / after N times / on a date). A repeat that cannot end (a
    /// to-do's) leaves it out.
    pub end_option: bool,
    /// Offer a monthly rule on the start's nth weekday ("On the second Wednesday"); without
    /// it a monthly rule repeats on the start's day, and the form says so.
    pub month_weekday_option: bool,
}

/// What a theme decides about a date repeat picker: the SKIN of each part,
/// laid over the part's base (the structure, the same in every theme) by
/// [`build`].
pub(crate) struct DateRepeatPickerLook {
    /// The editor (the column of rows).
    pub editor: Vec<CssPropertyWithConditions>,
    /// One row.
    pub row: Vec<CssPropertyWithConditions>,
    /// A row's label ("Repeats", "Every", "On", "Ends").
    pub label: Vec<CssPropertyWithConditions>,
    /// A unit after a number ("weeks", "times") and the completion text.
    pub unit: Vec<CssPropertyWithConditions>,
    /// The box around a number field.
    pub number: Vec<CssPropertyWithConditions>,
    /// The row of weekday toggles.
    pub weekdays: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the editor, if it has one.
    pub marker: Option<&'static str>,
}

impl DateRepeatPicker {
    /// An editor showing `rule`.
    #[must_use]
    pub fn create(rule: DateRepeatRule) -> Self {
        Self {
            state: DateRepeatPickerStateWrapper {
                inner: rule,
                on_change: None.into(),
            },
            accessibility_name: OptionString::None,
            theme: OptionUiTheme::None,
            week_start: DatePickerWeekStart::Monday,
            completion_option: false,
            end_option: true,
            month_weekday_option: true,
        }
    }

    /// Sets the callback invoked when any part changes.
    pub fn set_on_change<C: Into<DateRepeatPickerOnChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.state.on_change = Some(DateRepeatPickerOnChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// [`Self::set_on_change`] for the builder chain.
    #[must_use]
    pub fn with_on_change<C: Into<DateRepeatPickerOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// The weekday the weekday toggles and the end date's calendar start on.
    pub const fn set_week_start(&mut self, week_start: DatePickerWeekStart) {
        self.week_start = week_start;
    }

    /// [`Self::set_week_start`] for the builder chain.
    #[must_use]
    pub const fn with_week_start(mut self, week_start: DatePickerWeekStart) -> Self {
        self.set_week_start(week_start);
        self
    }

    /// Show the "Repeat from the day it is completed" box (a to-do's
    /// choice; an appointment has none).
    pub const fn set_completion_option(&mut self, shown: bool) {
        self.completion_option = shown;
    }

    /// [`Self::set_completion_option`] for the builder chain.
    #[must_use]
    pub const fn with_completion_option(mut self, shown: bool) -> Self {
        self.set_completion_option(shown);
        self
    }

    /// Show the "Ends" row; a repeat that cannot end leaves it out.
    pub const fn set_end_option(&mut self, shown: bool) {
        self.end_option = shown;
    }

    /// [`Self::set_end_option`] for the builder chain.
    #[must_use]
    pub const fn with_end_option(mut self, shown: bool) -> Self {
        self.set_end_option(shown);
        self
    }

    /// Offer a monthly rule on the start's nth weekday; without it, a monthly rule repeats on
    /// the start's day.
    pub const fn set_month_weekday_option(&mut self, shown: bool) {
        self.month_weekday_option = shown;
    }

    /// [`Self::set_month_weekday_option`] for the builder chain.
    #[must_use]
    pub const fn with_month_weekday_option(mut self, shown: bool) -> Self {
        self.set_month_weekday_option(shown);
        self
    }

    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Pin the widget theme of the editor and its parts; unset, they follow
    /// the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with the default editor and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The editor's DOM. The look comes from the theme module
    /// (`themes::flat::date_repeat_picker` / `themes::flora::
    /// date_repeat_picker`); `None` carries both looks, each in its
    /// `@theme(<name>)` block, and the app theme picks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks};
        match self.theme.into_option() {
            Some(UiTheme::Flora) => flora::date_repeat_picker(self),
            Some(UiTheme::Flat) => flat::date_repeat_picker(self),
            None => theme_blocks::follow_app_theme(
                self,
                flat::date_repeat_picker,
                flora::date_repeat_picker,
            ),
        }
    }
}

impl Default for DateRepeatPicker {
    fn default() -> Self {
        Self::create(DateRepeatRule::default())
    }
}

impl From<DateRepeatPicker> for Dom {
    fn from(e: DateRepeatPicker) -> Self {
        e.dom()
    }
}

// ---- the base: the editor's structure, in every theme ----

/// The editor: a column of rows.
pub(crate) static RECURRENCE_COLUMN_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
];

/// A row (and the row of weekday toggles): its parts on one midline,
/// wrapping when the form is narrow.
pub(crate) static RECURRENCE_ROW_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_wrap(LayoutFlexWrap::Wrap)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
];

/// A label or a unit: keeps its size, its text never selected by a drag.
pub(crate) static RECURRENCE_LABEL_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// The box around a number field: keeps its size.
pub(crate) static RECURRENCE_FIXED_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// "day" / "days", "week" / "weeks", ...: the unit after the interval.
const fn unit_word(frequency: DateRepeatFrequency, n: u32) -> &'static str {
    let one = n == 1;
    match frequency {
        DateRepeatFrequency::Daily if one => "day",
        DateRepeatFrequency::Daily => "days",
        DateRepeatFrequency::Weekly if one => "week",
        DateRepeatFrequency::Weekly => "weeks",
        DateRepeatFrequency::Monthly if one => "month",
        DateRepeatFrequency::Monthly => "months",
        DateRepeatFrequency::Yearly if one => "year",
        DateRepeatFrequency::Yearly => "years",
        DateRepeatFrequency::Never => "",
    }
}

/// The monthly row's two choices for `rule`'s start: "On day 14", "On the
/// second Wednesday" (a fifth weekday: "On the last Wednesday").
fn monthly_labels(rule: &DateRepeatRule) -> Vec<String> {
    let ordinal = match rule.start_nth() {
        1 => "first",
        2 => "second",
        3 => "third",
        4 => "fourth",
        _ => "last",
    };
    alloc::vec![
        alloc::format!("On day {}", rule.start.day),
        alloc::format!("On the {ordinal} {}", WEEKDAY_NAMES[rule.start_weekday()]),
    ]
}

/// The editor's DOM in `look`: editor [row [label, part..]..]. Every part
/// is its base (the structure), then the look's skin; the controls are the
/// toolkit's own widgets, pinned to the editor's theme (or following the
/// app theme with it).
#[allow(clippy::too_many_lines)]
pub(crate) fn build(editor: DateRepeatPicker, look: &DateRepeatPickerLook) -> Dom {
    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec(crate::widgets::themes::decl::on_base(base, skin))
    };
    let DateRepeatPicker {
        state,
        accessibility_name,
        theme,
        week_start,
        completion_option,
        end_option,
        month_weekday_option,
    } = editor;
    let rule = state.inner;
    let theme = theme.into_option();
    let shared = RefAny::new(state);

    let text = |words: String, classes: &'static [IdOrClass], skin: &[CssPropertyWithConditions]| {
        crate::widgets::widget_p_with_text(AzString::from(words))
            .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
            .with_css_props(part(RECURRENCE_LABEL_BASE, skin))
    };
    let label = |words: &str| text(String::from(words), LABEL_CLASS, &look.label);
    let unit = |words: &str| text(String::from(words), UNIT_CLASS, &look.unit);
    let row = |children: Vec<Dom>| {
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_CLASS))
            .with_css_props(part(RECURRENCE_ROW_BASE, &look.row))
            .with_children(DomVec::from_vec(children))
    };
    let segmented = |labels: Vec<String>, selected: usize, cb: SegmentedOnChangeCallbackType| {
        let mut s = Segmented::create(StringVec::from_vec(
            labels.into_iter().map(AzString::from).collect(),
        ))
        .with_selected_index(selected)
        .with_on_change(shared.clone(), cb);
        if let Some(t) = theme {
            s = s.with_theme(t);
        }
        s.dom()
    };
    let number = |value: u32, name: &'static str, cb: NumberInputOnValueChangeCallbackType| {
        let mut n = NumberInput::create(f32::from(u16::try_from(value).unwrap_or(u16::MAX)))
            .with_accessibility_name(AzString::from_const_str(name))
            .with_on_value_change(shared.clone(), cb);
        n.number_input_state.inner.min = 1.0;
        n.number_input_state.inner.max = MAX_NUMBER_F32;
        if let Some(t) = theme {
            n = n.with_theme(t);
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(NUMBER_CLASS))
            .with_css_props(part(RECURRENCE_FIXED_BASE, &look.number))
            .with_child(n.dom())
    };
    let labels = |words: &[&str]| words.iter().map(|w| String::from(*w)).collect::<Vec<_>>();

    let mut rows = alloc::vec![row(alloc::vec![
        label("Repeats"),
        segmented(
            labels(&FREQUENCY_LABELS),
            rule.frequency as usize,
            on_frequency_part
        ),
    ])];
    if rule.frequency != DateRepeatFrequency::Never {
        let mut every = alloc::vec![
            label("Every"),
            number(rule.interval, "Repeat every", on_interval_part),
            unit(unit_word(rule.frequency, rule.interval)),
        ];
        // Without the nth weekday a monthly rule repeats on the start's day: said here.
        if rule.frequency == DateRepeatFrequency::Monthly && !month_weekday_option {
            every.push(unit(&alloc::format!("on day {}", rule.start.day)));
        }
        rows.push(row(every));
        match rule.frequency {
            DateRepeatFrequency::Weekly => {
                let chosen = rule.effective_weekdays();
                let first = match week_start {
                    DatePickerWeekStart::Monday => 0,
                    DatePickerWeekStart::Sunday => 6,
                };
                let toggles: Vec<Dom> = (0..7usize)
                    .map(|i| (first + i) % 7)
                    .map(|day| {
                        let data = RefAny::new(WeekdayData {
                            day: u32::try_from(day).unwrap_or(0),
                            shared: shared.clone(),
                        });
                        let on_click: ButtonOnClickCallbackType = on_weekday_part;
                        let mut b = Button::create(AzString::from_const_str(WEEKDAY_SHORT[day]))
                            .with_toggled(chosen & (1 << day) != 0)
                            .with_on_click(data, on_click);
                        if let Some(t) = theme {
                            b = b.with_theme(t);
                        }
                        b.dom()
                    })
                    .collect();
                rows.push(row(alloc::vec![
                    label("On"),
                    Dom::create_div()
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(WEEKDAYS_CLASS))
                        .with_css_props(part(RECURRENCE_ROW_BASE, &look.weekdays))
                        .with_children(DomVec::from_vec(toggles)),
                ]));
            }
            DateRepeatFrequency::Monthly if month_weekday_option => {
                rows.push(row(alloc::vec![
                    label("On"),
                    segmented(monthly_labels(&rule), rule.monthly as usize, on_monthly_part),
                ]));
            }
            _ => {}
        }
        let mut ends = alloc::vec![
            label("Ends"),
            segmented(labels(&END_LABELS), rule.end as usize, on_end_part),
        ];
        match rule.end {
            DateRepeatEnd::Never => {}
            DateRepeatEnd::AfterCount => {
                ends.push(number(rule.count, "Number of times", on_count_part));
                ends.push(unit(if rule.count == 1 { "time" } else { "times" }));
            }
            DateRepeatEnd::OnDate => {
                let on_change: DatePickerOnChangeCallbackType = on_until_part;
                let mut picker =
                    DatePicker::create(rule.until.year, rule.until.month, rule.until.day)
                        .with_week_start(week_start)
                        .with_accessibility_name("Last date")
                        .with_on_change(shared.clone(), on_change);
                if let Some(t) = theme {
                    picker = picker.with_theme(t);
                }
                ends.push(picker.dom());
            }
        }
        if end_option {
            rows.push(row(ends));
        }
        if completion_option {
            let on_toggle: CheckBoxOnToggleCallbackType = on_completion_part;
            let mut check = CheckBox::create(rule.from_completion)
                .with_accessibility_name(AzString::from_const_str(COMPLETION_LABEL))
                .with_on_toggle(shared.clone(), on_toggle);
            if let Some(t) = theme {
                check = check.with_theme(t);
            }
            rows.push(row(alloc::vec![
                label(""),
                check.dom(),
                unit(COMPLETION_LABEL),
            ]));
        }
    }

    let mut classes: Vec<IdOrClass> = EDITOR_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_css_props(part(RECURRENCE_COLUMN_BASE, &look.editor))
        // A GROUP named by the caller, or "Repeat"; its value is the rule.
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Grouping,
            accessibility_name: Some(
                accessibility_name
                    .into_option()
                    .unwrap_or_else(|| AzString::from_const_str("Repeat")),
            )
            .into(),
            accessibility_value: Some(rule.to_rrule()).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(rows))
}

// ---- the parts' handlers ----

/// A weekday toggle's payload.
struct WeekdayData {
    /// 0 = Monday.
    day: u32,
    shared: RefAny,
}

/// Folds `part` into the editor's rule and hands the app the whole rule.
fn change(shared: &mut RefAny, info: CallbackInfo, part: Part) -> Update {
    let Some(mut w) = shared.downcast_mut::<DateRepeatPickerStateWrapper>() else {
        return Update::DoNothing;
    };
    apply(&mut w.inner, part);
    let rule = w.inner;
    match w.on_change.as_mut() {
        Some(DateRepeatPickerOnChange { callback, refany }) => {
            callback.invoke(refany.clone(), info, rule)
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_frequency_part(mut data: RefAny, info: CallbackInfo, s: SegmentedState) -> Update {
    change(&mut data, info, Part::Frequency(s.selected_index))
}

extern "C" fn on_interval_part(mut data: RefAny, info: CallbackInfo, s: NumberInputState) -> Update {
    change(&mut data, info, Part::Interval(s.number))
}

extern "C" fn on_weekday_part(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((day, mut shared)) = data
        .downcast_ref::<WeekdayData>()
        .map(|w| (w.day, w.shared.clone()))
    else {
        return Update::DoNothing;
    };
    change(&mut shared, info, Part::Weekday(day))
}

extern "C" fn on_monthly_part(mut data: RefAny, info: CallbackInfo, s: SegmentedState) -> Update {
    change(&mut data, info, Part::Monthly(s.selected_index))
}

extern "C" fn on_end_part(mut data: RefAny, info: CallbackInfo, s: SegmentedState) -> Update {
    change(&mut data, info, Part::End(s.selected_index))
}

extern "C" fn on_count_part(mut data: RefAny, info: CallbackInfo, s: NumberInputState) -> Update {
    change(&mut data, info, Part::Count(s.number))
}

extern "C" fn on_until_part(mut data: RefAny, info: CallbackInfo, d: DatePickerState) -> Update {
    change(&mut data, info, Part::Until(d))
}

extern "C" fn on_completion_part(mut data: RefAny, info: CallbackInfo, s: CheckBoxState) -> Update {
    change(&mut data, info, Part::Completion(s.checked))
}

/// The manifest's date repeat picker: weekly on Monday and Wednesday,
/// ending after ten times.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub(crate) fn sample() -> DateRepeatPicker {
        let mut rule = DateRepeatRule::create(DatePickerState {
            year: 2026,
            month: 10,
            day: 14,
        });
        rule.frequency = DateRepeatFrequency::Weekly;
        rule.weekdays = 0b000_0101;
        rule.end = DateRepeatEnd::AfterCount;
        DateRepeatPicker::create(rule)
    }
}

#[cfg(test)]
mod date_repeat_picker_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks},
    };

    /// Wednesday 30 September 2026: the fifth (so the last) Wednesday.
    const WED_30_SEP: DatePickerState = DatePickerState {
        year: 2026,
        month: 9,
        day: 30,
    };
    /// Wednesday 14 October 2026: the second Wednesday.
    const WED_14_OCT: DatePickerState = DatePickerState {
        year: 2026,
        month: 10,
        day: 14,
    };

    fn rrule(rule: &DateRepeatRule) -> String {
        rule.to_rrule().as_str().to_string()
    }

    fn read(text: &str, start: DatePickerState) -> Option<DateRepeatRule> {
        DateRepeatRule::from_rrule(AzString::from(text), start).into_option()
    }

    fn weekly(start: DatePickerState) -> DateRepeatRule {
        let mut r = DateRepeatRule::create(start);
        r.frequency = DateRepeatFrequency::Weekly;
        r
    }

    #[test]
    fn a_new_rule_does_not_repeat_and_writes_no_rrule() {
        let r = DateRepeatRule::create(WED_30_SEP);
        assert_eq!(r.frequency, DateRepeatFrequency::Never);
        assert_eq!((r.interval, r.end), (1, DateRepeatEnd::Never));
        assert_eq!(r.until, WED_30_SEP, "an end date starts on the first day");
        assert!(r.count >= 1, "an end after a number of times starts at a count");
        assert_eq!(rrule(&r), "");
    }

    #[test]
    fn a_weekly_rule_without_chosen_days_repeats_on_the_starts_weekday() {
        assert_eq!(rrule(&weekly(WED_30_SEP)), "FREQ=WEEKLY;BYDAY=WE");
        let mut r = DateRepeatRule::create(WED_30_SEP);
        r.frequency = DateRepeatFrequency::Daily;
        assert_eq!(rrule(&r), "FREQ=DAILY");
        r.frequency = DateRepeatFrequency::Yearly;
        assert_eq!(rrule(&r), "FREQ=YEARLY");
    }

    #[test]
    fn the_rrule_names_interval_end_and_days_in_the_order_azul_pim_writes_them() {
        let mut r = weekly(WED_30_SEP);
        r.interval = 2;
        r.weekdays = 0b000_0101; // Monday and Wednesday
        r.end = DateRepeatEnd::AfterCount;
        r.count = 10;
        assert_eq!(rrule(&r), "FREQ=WEEKLY;INTERVAL=2;COUNT=10;BYDAY=MO,WE");
        r.end = DateRepeatEnd::OnDate;
        r.until = DatePickerState {
            year: 2026,
            month: 12,
            day: 31,
        };
        assert_eq!(rrule(&r), "FREQ=WEEKLY;INTERVAL=2;UNTIL=20261231;BYDAY=MO,WE");
    }

    #[test]
    fn a_monthly_rule_repeats_on_the_starts_day_or_its_nth_weekday() {
        let mut r = DateRepeatRule::create(WED_14_OCT);
        r.frequency = DateRepeatFrequency::Monthly;
        assert_eq!(rrule(&r), "FREQ=MONTHLY;BYMONTHDAY=14");
        r.monthly = DateRepeatMonthly::Weekday;
        assert_eq!(rrule(&r), "FREQ=MONTHLY;BYDAY=2WE");
        // A fifth weekday, which most months lack, is "the last".
        r.start = WED_30_SEP;
        assert_eq!(rrule(&r), "FREQ=MONTHLY;BYDAY=-1WE");
    }

    #[test]
    fn every_rrule_the_form_writes_reads_back_as_the_same_rule() {
        for (text, start) in [
            ("", WED_30_SEP),
            ("FREQ=DAILY", WED_30_SEP),
            ("FREQ=DAILY;INTERVAL=3;COUNT=5", WED_30_SEP),
            ("FREQ=WEEKLY;BYDAY=WE", WED_30_SEP),
            ("FREQ=WEEKLY;INTERVAL=2;UNTIL=20261231;BYDAY=MO,WE", WED_30_SEP),
            ("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR", WED_30_SEP),
            ("FREQ=MONTHLY;BYMONTHDAY=14", WED_14_OCT),
            ("FREQ=MONTHLY;BYDAY=2WE", WED_14_OCT),
            ("FREQ=MONTHLY;BYDAY=-1WE", WED_30_SEP),
            ("FREQ=YEARLY;INTERVAL=2", WED_30_SEP),
        ] {
            let r =
                read(text, start).unwrap_or_else(|| panic!("{text:?} is a rule the form shows"));
            assert_eq!(rrule(&r), text, "{text:?} comes back as written");
        }
    }

    #[test]
    fn the_short_and_named_forms_of_a_rule_read_as_the_rule_they_mean() {
        // `RRULE:` in front, lower case, WKST, the bare weekly / monthly /
        // yearly forms, an UNTIL with a time.
        let r = read("RRULE:freq=weekly;wkst=MO", WED_30_SEP).expect("a weekly rule");
        assert_eq!(rrule(&r), "FREQ=WEEKLY;BYDAY=WE");
        let r = read("FREQ=MONTHLY", WED_14_OCT).expect("a monthly rule");
        assert_eq!(rrule(&r), "FREQ=MONTHLY;BYMONTHDAY=14");
        let r = read("FREQ=YEARLY;BYMONTH=10;BYMONTHDAY=14", WED_14_OCT).expect("a yearly rule");
        assert_eq!(rrule(&r), "FREQ=YEARLY");
        let r = read("FREQ=DAILY;UNTIL=20261231T235959Z", WED_30_SEP).expect("a daily rule");
        assert_eq!(r.end, DateRepeatEnd::OnDate);
        assert_eq!((r.until.year, r.until.month, r.until.day), (2026, 12, 31));
    }

    #[test]
    fn an_rrule_the_form_cannot_show_reads_as_none() {
        for text in [
            "FREQ=HOURLY",
            "FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1",
            "FREQ=DAILY;BYDAY=MO",
            "FREQ=MONTHLY;BYMONTHDAY=3",
            "FREQ=MONTHLY;BYDAY=3WE",
            "FREQ=MONTHLY;BYDAY=TU,TH",
            "FREQ=YEARLY;BYMONTH=3",
            "FREQ=WEEKLY;BYDAY=XX",
            "FREQ=DAILY;INTERVAL=0",
            "FREQ=DAILY;COUNT=many",
            "INTERVAL=2",
            "nonsense",
        ] {
            assert_eq!(read(text, WED_14_OCT), None, "{text:?} is no rule the form shows");
        }
    }

    #[test]
    fn a_frequency_click_keeps_the_interval_and_the_end() {
        let mut r = DateRepeatRule::create(WED_30_SEP);
        apply(&mut r, Part::Frequency(2));
        assert_eq!(r.frequency, DateRepeatFrequency::Weekly);
        apply(&mut r, Part::Interval(3.0));
        apply(&mut r, Part::End(1));
        apply(&mut r, Part::Count(4.0));
        apply(&mut r, Part::Frequency(3));
        assert_eq!(r.frequency, DateRepeatFrequency::Monthly);
        assert_eq!((r.interval, r.end, r.count), (3, DateRepeatEnd::AfterCount, 4));
        apply(&mut r, Part::Frequency(0));
        assert_eq!(rrule(&r), "", "Never writes no rule, whatever else is set");
        // A segment that is not there changes nothing.
        apply(&mut r, Part::Frequency(9));
        assert_eq!(r.frequency, DateRepeatFrequency::Never);
    }

    #[test]
    fn a_weekday_toggle_adds_and_removes_days_but_never_the_last_one() {
        let mut r = weekly(WED_30_SEP);
        apply(&mut r, Part::Weekday(0)); // Monday joins Wednesday
        assert_eq!(rrule(&r), "FREQ=WEEKLY;BYDAY=MO,WE");
        apply(&mut r, Part::Weekday(2)); // Wednesday goes
        assert_eq!(rrule(&r), "FREQ=WEEKLY;BYDAY=MO");
        apply(&mut r, Part::Weekday(0)); // the last day stays
        assert_eq!(rrule(&r), "FREQ=WEEKLY;BYDAY=MO");
        apply(&mut r, Part::Weekday(7)); // no such day
        assert_eq!(rrule(&r), "FREQ=WEEKLY;BYDAY=MO");
    }

    #[test]
    fn an_interval_or_a_count_is_held_between_1_and_999() {
        let mut r = weekly(WED_30_SEP);
        apply(&mut r, Part::Interval(0.0));
        assert_eq!(r.interval, 1);
        apply(&mut r, Part::Interval(2.6));
        assert_eq!(r.interval, 3);
        apply(&mut r, Part::Interval(5000.0));
        assert_eq!(r.interval, 999);
        apply(&mut r, Part::Interval(f32::NAN));
        assert_eq!(r.interval, 999, "no number keeps the interval");
        apply(&mut r, Part::Count(-4.0));
        assert_eq!(r.count, 1);
        apply(&mut r, Part::Count(12.0));
        assert_eq!(r.count, 12);
    }

    #[test]
    fn choosing_an_end_date_before_the_start_moves_it_to_the_start() {
        let mut r = weekly(WED_30_SEP);
        r.until = DatePickerState {
            year: 2026,
            month: 1,
            day: 2,
        };
        apply(&mut r, Part::End(2));
        assert_eq!(r.end, DateRepeatEnd::OnDate);
        assert_eq!(r.until, WED_30_SEP);
        let later = DatePickerState {
            year: 2027,
            month: 3,
            day: 1,
        };
        apply(&mut r, Part::Until(later));
        assert_eq!(r.until, later);
        apply(&mut r, Part::Completion(true));
        assert!(r.from_completion);
        assert_eq!(
            rrule(&r),
            "FREQ=WEEKLY;UNTIL=20270301;BYDAY=WE",
            "from completion is the app's, not RRULE's"
        );
    }

    /// Every text of the subtree, in document order.
    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            out.push(s.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn all_texts(dom: &Dom) -> Vec<String> {
        let mut out = Vec::new();
        texts(dom, &mut out);
        out
    }

    fn rows(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .filter(|c| theme_checks::has_class(c, "__azul-native-date-repeat-picker-row"))
            .count()
    }

    #[test]
    fn the_editor_shows_only_the_rows_its_frequency_needs() {
        let never = DateRepeatPicker::create(DateRepeatRule::create(WED_30_SEP))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(rows(&never), 1, "Never: the frequency row alone");
        assert!(theme_checks::has_class(&never, RECURRENCE_EDITOR_CLASS));

        let dom = DateRepeatPicker::create(weekly(WED_30_SEP))
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(rows(&dom), 4, "Weekly: frequency, every, weekdays, ends");
        let t = all_texts(&dom);
        for want in ["Repeats", "Every", "week", "On", "Mo", "Su", "Ends"] {
            assert!(t.iter().any(|s| s == want), "{want:?} is missing from {t:?}");
        }

        let mut monthly = DateRepeatRule::create(WED_14_OCT);
        monthly.frequency = DateRepeatFrequency::Monthly;
        let dom = DateRepeatPicker::create(monthly)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(rows(&dom), 4, "Monthly: frequency, every, the day, ends");
        let t = all_texts(&dom);
        assert!(t.iter().any(|s| s == "On day 14"), "{t:?}");
        assert!(t.iter().any(|s| s == "On the second Wednesday"), "{t:?}");

        let mut daily = DateRepeatRule::create(WED_30_SEP);
        daily.frequency = DateRepeatFrequency::Daily;
        daily.interval = 2;
        daily.end = DateRepeatEnd::AfterCount;
        let dom = DateRepeatPicker::create(daily)
            .with_completion_option(true)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(rows(&dom), 4, "Daily: frequency, every, ends, from completion");
        let t = all_texts(&dom);
        assert!(t.iter().any(|s| s == "days"), "two days: {t:?}");
        assert!(t.iter().any(|s| s == "times"), "the count's unit: {t:?}");
        assert!(
            t.iter().any(|s| s == "Repeat from the day it is completed"),
            "{t:?}"
        );
    }

    /// A to-do's repeat (azul_pim's) has no end and no "second Wednesday": its editor leaves
    /// those choices out instead of offering what cannot be kept.
    #[test]
    fn an_editor_can_leave_out_the_end_and_the_months_nth_weekday() {
        let mut monthly = DateRepeatRule::create(WED_14_OCT);
        monthly.frequency = DateRepeatFrequency::Monthly;
        let full = DateRepeatPicker::create(monthly).with_theme(UiTheme::Flat);
        assert!(full.end_option && full.month_weekday_option, "both are shown by default");
        let dom = full
            .with_end_option(false)
            .with_month_weekday_option(false)
            .dom();
        let t = all_texts(&dom);
        assert!(!t.iter().any(|s| s == "Ends"), "no end row: {t:?}");
        assert!(!t.iter().any(|s| s.starts_with("On the ")), "no nth weekday: {t:?}");
        assert!(t.iter().any(|s| s == "on day 14"), "the day it repeats on is said: {t:?}");
        assert_eq!(rows(&dom), 2, "Monthly: frequency and every");
    }

    #[test]
    fn the_weekday_toggles_follow_the_week_start() {
        let first_day = |start: DatePickerWeekStart| {
            let dom = DateRepeatPicker::create(weekly(WED_30_SEP))
                .with_week_start(start)
                .with_theme(UiTheme::Flat)
                .dom();
            let toggles = theme_checks::find(&dom, "__azul-native-date-repeat-picker-weekdays")
                .expect("the weekday toggles");
            all_texts(toggles).first().cloned()
        };
        assert_eq!(first_day(DatePickerWeekStart::Monday).as_deref(), Some("Mo"));
        assert_eq!(first_day(DatePickerWeekStart::Sunday).as_deref(), Some("Su"));
    }

    #[test]
    fn the_editor_is_a_group_named_repeat_unless_named_otherwise() {
        let dom = DateRepeatPicker::create(DateRepeatRule::create(WED_30_SEP))
            .with_theme(UiTheme::Flat)
            .dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Grouping);
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some("Repeat")
        );
    }

    type Log = Arc<Mutex<Vec<DateRepeatRule>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, rule: DateRepeatRule) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(rule);
        }
        Update::RefreshDom
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    /// The nearest node at or above the text `label` that takes a click.
    fn clickable(styled: &StyledDom, label: &str) -> NodeId {
        let hierarchy = styled.node_hierarchy.as_ref();
        let nodes = styled.node_data.as_ref();
        let text = nodes
            .iter()
            .position(|nd| {
                matches!(nd.get_node_type(), NodeType::Text(s) if s.as_ref().as_str() == label)
            })
            .unwrap_or_else(|| panic!("no text {label:?}"));
        let mut at = Some(NodeId::new(text));
        while let Some(n) = at {
            if nodes[n.index()]
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click))
            {
                return n;
            }
            at = hierarchy[n.index()].parent_id();
        }
        panic!("nothing takes a click on {label:?}");
    }

    #[test]
    fn a_click_on_a_part_reports_the_whole_rule_to_the_app() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let editor = || {
            DateRepeatPicker::create(weekly(WED_30_SEP))
                .with_on_change(
                    RefAny::new(log.clone()),
                    record as DateRepeatPickerOnChangeCallbackType,
                )
                .with_theme(UiTheme::Flat)
        };
        // The frequency: "Daily".
        let styled = StyledDom::create_from_dom(editor().dom());
        let (update, _) = rv::fire(
            &styled,
            id(clickable(&styled, "Daily")),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("a segment takes the click");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        // A weekday: Friday joins Wednesday.
        let styled = StyledDom::create_from_dom(editor().dom());
        rv::fire(
            &styled,
            id(clickable(&styled, "Fr")),
            EventFilter::Hover(HoverEventFilter::Click),
        )
        .expect("a weekday takes the click");
        let seen = log.lock().expect("log").clone();
        assert_eq!(seen.len(), 2, "one report per click: {seen:?}");
        assert_eq!(seen[0].frequency, DateRepeatFrequency::Daily);
        assert_eq!(rrule(&seen[1]), "FREQ=WEEKLY;BYDAY=WE,FR");
    }

    #[test]
    fn both_themes_style_the_editor_in_light_and_dark() {
        for theme in checks::BOTH {
            let dom = DateRepeatPicker::create(weekly(WED_30_SEP))
                .with_theme(theme)
                .dom();
            let label = theme_checks::find(&dom, "__azul-native-date-repeat-picker-label")
                .expect("a row label");
            assert!(
                !crate::widgets::theme_probe::dark(label).is_empty(),
                "{theme:?}: a row label has no dark-mode ink"
            );
        }
    }
}

/// Following the app theme (`theme: None`): the DOM carries every widget
/// theme's `@theme(<name>)` block and renders the app theme's.
#[cfg(test)]
mod app_theme_tests {
    use super::*;
    use crate::widgets::themes::{theme_blocks::checks, UiTheme};

    fn rule() -> DateRepeatRule {
        let mut r = DateRepeatRule::create(DatePickerState {
            year: 2026,
            month: 10,
            day: 14,
        });
        r.frequency = DateRepeatFrequency::Weekly;
        r.end = DateRepeatEnd::AfterCount;
        r
    }

    #[test]
    fn a_date_repeat_picker_without_a_theme_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "date_repeat_picker",
            || DateRepeatPicker::create(rule()).dom(),
            |t: UiTheme| DateRepeatPicker::create(rule()).with_theme(t).dom(),
        );
    }

    /// Only the editor's own nodes: its parts are other widgets, which
    /// answer for their own structure in their own tests.
    fn own_nodes(dom: &Dom) -> Dom {
        let mut copy = dom.clone();
        let kept: Vec<Dom> = dom
            .children
            .as_ref()
            .iter()
            .filter(|c| {
                c.root.get_ids_and_classes().as_ref().iter().any(|class| {
                    matches!(class, IdOrClass::Class(s)
                        if s.as_str().starts_with(RECURRENCE_EDITOR_CLASS))
                })
            })
            .map(own_nodes)
            .collect();
        copy.children = DomVec::from_vec(kept);
        copy
    }

    /// R5: the editor's structure (display, flex, alignment, ...) is
    /// declared ONCE, outside every `@theme` block.
    #[test]
    fn a_date_repeat_picker_declares_its_structure_once_for_every_theme() {
        use crate::widgets::themes::theme_checks::assert_structure_is_shared;
        for theme in checks::BOTH {
            let editor = checks::under(theme, || DateRepeatPicker::create(rule()).dom());
            assert_structure_is_shared(
                &format!("date_repeat_picker built for {}", theme.name()),
                &own_nodes(&editor),
                &[],
            );
        }
    }
}
