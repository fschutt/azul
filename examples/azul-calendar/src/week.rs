//! Week math for the week view: which events fall in a week, where each one sits in its day's
//! column, and the view's own geometry - the time at a y, the event a press lands on, the event a
//! click or a drag makes, and the zoom. Weeks start on Monday; the view holds the whole day,
//! 00:00 to 24:00, and scrolls.

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike};

use crate::event::Event;

/// Minutes in a day: the week view holds all of them.
pub const DAY_MINUTES: u32 = 24 * 60;
/// The last minute an event can end at: an event does not cross midnight, and 24:00 is not a
/// time of day.
pub const LAST_MINUTE: u32 = DAY_MINUTES - 1;
/// A new event starts (and a dragged one ends) on this grid, in minutes.
pub const SNAP_MINUTES: u32 = 15;
/// How long an event a click makes is (cut at the end of the day).
pub const CLICK_MINUTES: u32 = 60;
/// The height of an hour when the app starts, in logical px.
pub const DEFAULT_HOUR_PX: f32 = 48.0;
/// The zoom's limits: the smallest and the largest hour, in logical px.
pub const MIN_HOUR_PX: f32 = 20.0;
pub const MAX_HOUR_PX: f32 = 240.0;
/// An event block is never drawn shorter than this, so its title stays readable; a press on the
/// drawn block finds the event.
pub const MIN_BLOCK_PX: f32 = 18.0;
/// How far a press moves before it is a drag, in logical px.
pub const DRAG_THRESHOLD_PX: f32 = 4.0;
/// The hour at the top of the view when it opens on a week that is not today's.
pub const MORNING_HOUR: u32 = 8;
/// Wheel pixels that double (or halve) the hour height: one notch (60 px) is a quarter of that.
const WHEEL_PX_PER_DOUBLING: f32 = 240.0;
/// No single wheel event zooms more than this, however far it scrolled (a trackpad flick is
/// dozens of events).
const MAX_WHEEL_STEP: f32 = 1.25;
const MIN_WHEEL_STEP: f32 = 0.8;
/// How much a y read back from pixels may fall short of the line it was on, in minutes.
const LINE_TOLERANCE: f32 = 1e-3;

/// The Monday on or before `day` (`azul_pim::dates::start_of_week`; the calendar's weeks start
/// on Monday).
pub fn week_start(day: NaiveDate) -> NaiveDate {
    azul_pim::dates::start_of_week(day, chrono::Weekday::Mon)
}

/// The Monday `weeks` weeks after (or, negative, before) the week of `day`.
pub fn shift_weeks(day: NaiveDate, weeks: i64) -> NaiveDate {
    week_start(day) + Duration::days(7 * weeks)
}

/// The seven days of the week of `day`, Monday first.
pub fn week_days(day: NaiveDate) -> [NaiveDate; 7] {
    let monday = week_start(day);
    std::array::from_fn(|i| monday + Duration::days(i as i64))
}

/// The events of the week of `day`, one list per day (Monday first), each in order of start,
/// end and title.
pub fn events_in_week(events: &[Event], day: NaiveDate) -> [Vec<&Event>; 7] {
    let monday = week_start(day);
    let mut days: [Vec<&Event>; 7] = Default::default();
    for event in events {
        let offset = (event.date - monday).num_days();
        if (0..7).contains(&offset) {
            days[offset as usize].push(event);
        }
    }
    for list in &mut days {
        list.sort_by(|a, b| {
            (a.start, a.end, &a.title, &a.id).cmp(&(b.start, b.end, &b.title, &b.id))
        });
    }
    days
}

/// Minutes since midnight.
pub fn minute_of_day(t: NaiveTime) -> u32 {
    t.hour() * 60 + t.minute()
}

/// The time `minute` minutes after midnight; a minute past the day is its last minute.
pub fn time_of_minute(minute: u32) -> NaiveTime {
    let minute = minute.min(LAST_MINUTE);
    NaiveTime::from_hms_opt(minute / 60, minute % 60, 0).unwrap_or(NaiveTime::MIN)
}

/// Where an event sits in its day's column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The event's index in the day's list.
    pub index: usize,
    /// Minutes from midnight to the event's top edge.
    pub top: u32,
    /// The event's length in minutes.
    pub height: u32,
    /// Which of the `lanes` side-by-side columns the event takes; 0 is the leftmost.
    pub lane: u32,
    /// How many side-by-side columns its group of overlapping events needs.
    pub lanes: u32,
}

/// Lays out one day's events. Events that overlap in time sit side by side: a group of events
/// linked by overlaps shares the column's width in as many lanes as it needs at once, each
/// event in the leftmost lane that is free at its start. An event that ends as another starts
/// does not overlap it.
pub fn lay_out_day(events: &[&Event]) -> Vec<Placement> {
    let mut placements: Vec<Placement> = Vec::with_capacity(events.len());
    let mut order: Vec<usize> = (0..events.len()).collect();
    order.sort_by_key(|&i| (minute_of_day(events[i].start), minute_of_day(events[i].end)));
    // The group being built: where its placements begin, when its last event ends, and when
    // each of its lanes is free again.
    let mut group_from = 0;
    let mut group_end = 0;
    let mut lane_free_at: Vec<u32> = Vec::new();
    for index in order {
        let start = minute_of_day(events[index].start);
        let end = minute_of_day(events[index].end);
        if !lane_free_at.is_empty() && start >= group_end {
            set_lanes(&mut placements[group_from..], lane_free_at.len());
            group_from = placements.len();
            lane_free_at.clear();
        }
        group_end = if lane_free_at.is_empty() {
            end
        } else {
            group_end.max(end)
        };
        let lane = match lane_free_at.iter().position(|&free| free <= start) {
            Some(lane) => {
                lane_free_at[lane] = end;
                lane
            }
            None => {
                lane_free_at.push(end);
                lane_free_at.len() - 1
            }
        };
        placements.push(Placement {
            index,
            top: start,
            height: end.saturating_sub(start),
            lane: lane as u32,
            lanes: 0,
        });
    }
    set_lanes(&mut placements[group_from..], lane_free_at.len());
    placements
}

fn set_lanes(group: &mut [Placement], lanes: usize) {
    for placement in group {
        placement.lanes = lanes as u32;
    }
}

// ==== The view's geometry ====

/// The y of `minute` in a day column whose hours are `hour_px` high.
pub fn y_of_minute(minute: f32, hour_px: f32) -> f32 {
    minute * hour_px / 60.0
}

/// The height of the whole day at `hour_px` an hour.
pub fn day_height(hour_px: f32) -> f32 {
    y_of_minute(DAY_MINUTES as f32, hour_px)
}

/// The minute at `y` in a day column whose hours are `hour_px` high, held to the day.
pub fn minute_at_y(y: f32, hour_px: f32) -> f32 {
    if !y.is_finite() || !hour_px.is_finite() || hour_px <= 0.0 {
        return 0.0;
    }
    (y * 60.0 / hour_px).clamp(0.0, DAY_MINUTES as f32)
}

/// A minute read from pixels, held to the day (nothing read is 0).
fn in_day(minute: f32) -> f32 {
    if minute.is_finite() {
        minute.clamp(0.0, DAY_MINUTES as f32)
    } else {
        0.0
    }
}

/// The start of the quarter hour `minute` is in. A minute a hair under a line (a y read back
/// from pixels) counts as the line.
fn snap_down(minute: f32) -> u32 {
    let snap = SNAP_MINUTES as f32;
    ((in_day(minute) + LINE_TOLERANCE) / snap).floor() as u32 * SNAP_MINUTES
}

/// The end of the quarter hour `minute` is in; a minute a hair past a line counts as the line.
fn snap_up(minute: f32) -> u32 {
    let snap = SNAP_MINUTES as f32;
    ((in_day(minute) - LINE_TOLERANCE).max(0.0) / snap).ceil() as u32 * SNAP_MINUTES
}

/// The event a click at `minute` makes: an hour from the start of the quarter hour it lands
/// in, cut at the end of the day. `(start, end)` in minutes from midnight, `start < end`.
pub fn click_range(minute: f32) -> (u32, u32) {
    let start = snap_down(minute).min(DAY_MINUTES - SNAP_MINUTES);
    (start, (start + CLICK_MINUTES).min(LAST_MINUTE))
}

/// The event a drag from `from` to `to` (minutes, either way round) makes: every quarter hour
/// it touched, at least one, inside the day. `(start, end)`, `start < end`.
pub fn drag_range(from: f32, to: f32) -> (u32, u32) {
    let (from, to) = (in_day(from), in_day(to));
    let (low, high) = if from <= to { (from, to) } else { (to, from) };
    let start = snap_down(low).min(DAY_MINUTES - SNAP_MINUTES);
    let end = snap_up(high).max(start + SNAP_MINUTES).min(LAST_MINUTE);
    (start, end)
}

/// Whether a press that went down at `from_y` and is now at `to_y` is a drag.
pub fn is_drag(from_y: f32, to_y: f32) -> bool {
    (to_y - from_y).abs() >= DRAG_THRESHOLD_PX
}

/// The event (its index in the day's list) whose block a press at `y` lands on, `x_frac` of the
/// way across the column (0 = left edge, 1 = right edge), with hours `hour_px` high. A block is
/// its event's time, drawn at least `MIN_BLOCK_PX` tall, in its lane.
pub fn event_at(placements: &[Placement], y: f32, x_frac: f32, hour_px: f32) -> Option<usize> {
    placements
        .iter()
        .find(|p| {
            let top = y_of_minute(p.top as f32, hour_px);
            let height = y_of_minute(p.height as f32, hour_px).max(MIN_BLOCK_PX);
            let lanes = p.lanes.max(1) as f32;
            let left = p.lane as f32 / lanes;
            let right = (p.lane + 1) as f32 / lanes;
            y >= top && y < top + height && x_frac >= left && x_frac < right
        })
        .map(|p| p.index)
}

// ==== Zoom ====

/// `hour_px` held to the zoom's limits (the default for nonsense).
pub fn clamp_hour_px(hour_px: f32) -> f32 {
    if hour_px.is_finite() {
        hour_px.clamp(MIN_HOUR_PX, MAX_HOUR_PX)
    } else {
        DEFAULT_HOUR_PX
    }
}

/// The zoom factor of one wheel event with the zoom modifier held: `dy` > 0 (wheel up, as the
/// engine reports it) zooms in. Proportional to the delta, and bounded per event.
pub fn wheel_zoom_factor(dy: f32) -> f32 {
    if !dy.is_finite() {
        return 1.0;
    }
    2f32.powf(dy / WHEEL_PX_PER_DOUBLING)
        .clamp(MIN_WHEEL_STEP, MAX_WHEEL_STEP)
}

/// One pinch update, as the engine reports it (`DetectedPinch`): the scale since the gesture
/// began (cumulative, on every source), and whether this update began the gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinchSample {
    pub scale: f32,
    pub began: bool,
}

/// How much one pinch update zooms: its ratio to the gesture's previous update, `previous`
/// (that update's cumulative scale; 1.0 when this update begins the gesture, or when the
/// gesture's start was missed). An unusable scale zooms nothing.
pub fn pinch_step(previous: Option<f32>, now: PinchSample) -> f32 {
    let usable = |scale: f32| scale.is_finite() && scale > 0.0;
    if !usable(now.scale) {
        return 1.0;
    }
    let base = match previous {
        Some(p) if !now.began && usable(p) => p,
        _ => 1.0,
    };
    now.scale / base
}

/// How far down a view `view_height` px high can scroll over the day at `hour_px` an hour.
pub fn max_scroll(hour_px: f32, view_height: f32) -> f32 {
    (day_height(hour_px) - view_height).max(0.0)
}

/// The scroll offset that keeps the time under the pointer under it when the hour height goes
/// from `old_px` to `new_px`: `pointer` is the pointer's y in the view, `scroll_y` the view's
/// offset before, `view_height` its height. Held to the day.
pub fn zoom_scroll(old_px: f32, new_px: f32, pointer: f32, scroll_y: f32, view_height: f32) -> f32 {
    let minute = minute_at_y(scroll_y + pointer, old_px);
    let target = y_of_minute(minute, new_px) - pointer;
    if !target.is_finite() {
        return 0.0;
    }
    target.clamp(0.0, max_scroll(new_px, view_height))
}

/// Where to scroll a view `view_height` px high, now at `scroll_y`, to show an event that starts
/// `start` minutes after midnight: `None` while its drawn block (`MIN_BLOCK_PX` at least) is
/// inside the view, else the offset that puts the hour before it at the top, held to the day.
pub fn reveal_scroll(start: u32, hour_px: f32, scroll_y: f32, view_height: f32) -> Option<f32> {
    let top = y_of_minute(start as f32, hour_px);
    if top >= scroll_y && top + MIN_BLOCK_PX <= scroll_y + view_height {
        return None;
    }
    let above = y_of_minute(start.saturating_sub(60) as f32, hour_px);
    Some(above.min(max_scroll(hour_px, view_height)))
}

/// The minute at the top of the view when it opens: an hour before now (on the hour) when the
/// week shows today, else `MORNING_HOUR`.
pub fn first_minute_shown(today_shown: bool, now: NaiveTime) -> u32 {
    if today_shown {
        minute_of_day(now).saturating_sub(60) / 60 * 60
    } else {
        MORNING_HOUR * 60
    }
}

// ==== Labels ====

/// "Wed 30"
pub fn day_label(day: NaiveDate) -> String {
    day.format("%a %-d").to_string()
}

/// "28 September - 4 October 2026", "5 - 11 October 2026" or "28 December 2026 - 3 January 2027".
pub fn week_title(day: NaiveDate) -> String {
    let monday = week_start(day);
    let sunday = monday + Duration::days(6);
    let first = if monday.year() != sunday.year() {
        "%-d %B %Y"
    } else if monday.month() != sunday.month() {
        "%-d %B"
    } else {
        "%-d"
    };
    format!("{} - {}", monday.format(first), sunday.format("%-d %B %Y"))
}

/// "09:00 - 10:00"
pub fn time_range(start: NaiveTime, end: NaiveTime) -> String {
    format!("{} - {}", start.format("%H:%M"), end.format("%H:%M"))
}

/// "13:00", the label of an hour in the view's gutter.
pub fn hour_label(hour: u32) -> String {
    format!("{hour:02}:00")
}

/// "Wednesday 30 September, 10:30 - 11:30": what a new event's popover says it is.
pub fn draft_label(date: NaiveDate, start: NaiveTime, end: NaiveTime) -> String {
    format!("{}, {}", date.format("%A %-d %B"), time_range(start, end))
}

/// The day a new event starts on: today when today is in the shown week, else the shown week's
/// Monday.
pub fn default_day(today: NaiveDate, shown: NaiveDate) -> NaiveDate {
    let monday = week_start(shown);
    if week_start(today) == monday {
        today
    } else {
        monday
    }
}

/// Where the "New event" sheet opens: on today's week, the next full hour after `now` (an hour
/// long, Google Calendar's way), or tomorrow at 09:00 when that hour would run past 23:00; on any
/// other week, its Monday at 09:00.
pub fn new_event_slot(
    today: NaiveDate,
    shown: NaiveDate,
    now: NaiveTime,
) -> (NaiveDate, NaiveTime, NaiveTime) {
    let at = |h: u32| NaiveTime::from_hms_opt(h, 0, 0).unwrap_or(NaiveTime::MIN);
    let date = default_day(today, shown);
    if date != today {
        return (date, at(9), at(10));
    }
    let next = now.hour() + 1;
    if next > 22 {
        return (today + Duration::days(1), at(9), at(10));
    }
    (today, at(next), at(next + 1))
}

/// The date a date picker shows, with the day cut to the month's length (the picker keeps its
/// day when it turns the month); `None` for a month that does not exist.
pub fn picked_date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    (28..=day.clamp(1, 31))
        .rev()
        .chain(std::iter::once(day.clamp(1, 28)))
        .find_map(|d| NaiveDate::from_ymd_opt(year, month, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    /// An event with a made-up UUID (`n` in its last group).
    fn event(n: u32, title: &str, date: NaiveDate, start: NaiveTime, end: NaiveTime) -> Event {
        let id = format!("00000000-0000-4000-8000-{n:012}");
        Event::create(&id, title, date, start, end, None).unwrap()
    }

    /// (lane, lanes) of each placed event, by title, in the order of the day's list.
    fn lanes(day: &[&Event]) -> Vec<(String, u32, u32)> {
        let placements = lay_out_day(day);
        let mut out: Vec<(usize, String, u32, u32)> = placements
            .iter()
            .map(|p| (p.index, day[p.index].title.clone(), p.lane, p.lanes))
            .collect();
        out.sort();
        out.into_iter().map(|(_, t, l, n)| (t, l, n)).collect()
    }

    /// A placement for the hit tests: `top` / `height` in minutes from midnight.
    fn placed(index: usize, top: u32, height: u32, lane: u32, lanes: u32) -> Placement {
        Placement {
            index,
            top,
            height,
            lane,
            lanes,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_week_starts_on_monday() {
        let monday = day(2026, 9, 28);
        assert_eq!(week_start(day(2026, 9, 30)), monday);
        assert_eq!(week_start(monday), monday);
        assert_eq!(week_start(day(2026, 10, 4)), monday);
        assert_eq!(week_start(day(2026, 10, 5)), day(2026, 10, 5));
        assert_eq!(week_start(day(2027, 1, 1)), day(2026, 12, 28));
    }

    #[test]
    fn the_week_view_steps_a_week_at_a_time_across_months_and_years() {
        assert_eq!(shift_weeks(day(2026, 9, 30), 1), day(2026, 10, 5));
        assert_eq!(shift_weeks(day(2026, 9, 30), -1), day(2026, 9, 21));
        assert_eq!(shift_weeks(day(2026, 9, 30), 0), day(2026, 9, 28));
        assert_eq!(shift_weeks(day(2026, 12, 30), 1), day(2027, 1, 4));
        assert_eq!(shift_weeks(day(2027, 1, 4), -1), day(2026, 12, 28));
    }

    #[test]
    fn a_week_is_seven_days_from_monday() {
        let days = week_days(day(2026, 10, 1));
        assert_eq!(days[0], day(2026, 9, 28));
        assert_eq!(days[3], day(2026, 10, 1));
        assert_eq!(days[6], day(2026, 10, 4));
    }

    #[test]
    fn only_the_events_of_the_shown_week_are_listed_each_under_its_day() {
        let events = vec![
            event(1, "Sunday before", day(2026, 9, 27), at(9, 0), at(10, 0)),
            event(2, "Late Wednesday", day(2026, 9, 30), at(15, 0), at(16, 0)),
            event(3, "Monday", day(2026, 9, 28), at(9, 0), at(10, 0)),
            event(4, "Early Wednesday", day(2026, 9, 30), at(8, 0), at(9, 0)),
            event(5, "Sunday", day(2026, 10, 4), at(19, 0), at(20, 0)),
            event(6, "Monday after", day(2026, 10, 5), at(9, 0), at(10, 0)),
            event(
                7,
                "Also early Wednesday",
                day(2026, 9, 30),
                at(8, 0),
                at(8, 30),
            ),
        ];
        let week = events_in_week(&events, day(2026, 10, 1));
        let titles: Vec<Vec<&str>> = week
            .iter()
            .map(|d| d.iter().map(|e| e.title.as_str()).collect())
            .collect();
        assert_eq!(
            titles,
            vec![
                vec!["Monday"],
                vec![],
                vec!["Also early Wednesday", "Early Wednesday", "Late Wednesday"],
                vec![],
                vec![],
                vec![],
                vec!["Sunday"],
            ]
        );
    }

    #[test]
    fn events_that_do_not_overlap_take_the_whole_column_at_their_minute_of_the_day() {
        let d = day(2026, 9, 30);
        let a = event(1, "A", d, at(9, 0), at(10, 0));
        let b = event(2, "B", d, at(11, 0), at(12, 30));
        assert_eq!(
            lay_out_day(&[&a, &b]),
            vec![placed(0, 540, 60, 0, 1), placed(1, 660, 90, 0, 1)]
        );
    }

    #[test]
    fn overlapping_events_sit_side_by_side() {
        let d = day(2026, 9, 30);
        let a = event(1, "A", d, at(10, 0), at(11, 0));
        let b = event(2, "B", d, at(10, 0), at(10, 30));
        let c = event(3, "C", d, at(10, 15), at(12, 0));
        let alone = event(4, "Alone", d, at(14, 0), at(15, 0));
        let week = [a, b, c, alone];
        let listed = events_in_week(&week, d);
        assert_eq!(
            lanes(&listed[2]),
            vec![
                (String::from("B"), 0, 3),
                (String::from("A"), 1, 3),
                (String::from("C"), 2, 3),
                (String::from("Alone"), 0, 1),
            ]
        );
    }

    #[test]
    fn a_chain_of_overlaps_shares_its_lanes() {
        let d = day(2026, 9, 30);
        // A overlaps B, B overlaps C, A does not overlap C: two lanes, C reuses A's.
        let a = event(1, "A", d, at(9, 0), at(10, 0));
        let b = event(2, "B", d, at(9, 30), at(10, 30));
        let c = event(3, "C", d, at(10, 0), at(11, 0));
        assert_eq!(
            lanes(&[&a, &b, &c]),
            vec![
                (String::from("A"), 0, 2),
                (String::from("B"), 1, 2),
                (String::from("C"), 0, 2),
            ]
        );
    }

    #[test]
    fn an_event_that_ends_as_another_starts_does_not_overlap_it() {
        let d = day(2026, 9, 30);
        let a = event(1, "A", d, at(9, 0), at(10, 0));
        let b = event(2, "B", d, at(10, 0), at(11, 0));
        assert_eq!(
            lanes(&[&a, &b]),
            vec![(String::from("A"), 0, 1), (String::from("B"), 0, 1)]
        );
    }

    #[test]
    fn the_view_holds_the_whole_day_from_midnight_to_the_last_minute() {
        let d = day(2026, 9, 30);
        let midnight = event(1, "Midnight", d, at(0, 0), at(0, 30));
        let dawn = event(2, "Dawn", d, at(6, 0), at(7, 30));
        let night = event(3, "Night", d, at(22, 0), at(23, 59));
        let spans: Vec<(usize, u32, u32)> = lay_out_day(&[&midnight, &dawn, &night])
            .iter()
            .map(|p| (p.index, p.top, p.height))
            .collect();
        assert_eq!(spans, vec![(0, 0, 30), (1, 360, 90), (2, 1320, 119)]);
        assert_eq!(DAY_MINUTES, 24 * 60);
        assert_eq!(minute_of_day(at(23, 59)), LAST_MINUTE);
    }

    #[test]
    fn events_before_eight_share_lanes_like_any_others() {
        let d = day(2026, 9, 30);
        // Early overlaps Dawn: both are in the view now, so they sit side by side; Nine starts
        // as Dawn ends and takes the whole column.
        let dawn = event(1, "Dawn", d, at(6, 0), at(9, 0));
        let early = event(2, "Early", d, at(7, 0), at(7, 45));
        let nine = event(3, "Nine", d, at(9, 0), at(10, 0));
        assert_eq!(
            lanes(&[&dawn, &early, &nine]),
            vec![
                (String::from("Dawn"), 0, 2),
                (String::from("Early"), 1, 2),
                (String::from("Nine"), 0, 1),
            ]
        );
    }

    #[test]
    fn labels_read_as_a_person_writes_them() {
        assert_eq!(day_label(day(2026, 9, 30)), "Wed 30");
        assert_eq!(day_label(day(2026, 10, 4)), "Sun 4");
        assert_eq!(
            week_title(day(2026, 9, 30)),
            "28 September - 4 October 2026"
        );
        assert_eq!(week_title(day(2026, 10, 7)), "5 - 11 October 2026");
        assert_eq!(
            week_title(day(2026, 12, 31)),
            "28 December 2026 - 3 January 2027"
        );
        assert_eq!(time_range(at(9, 0), at(10, 30)), "09:00 - 10:30");
        assert_eq!(hour_label(0), "00:00");
        assert_eq!(hour_label(13), "13:00");
    }

    #[test]
    fn a_draft_names_its_day_and_times() {
        assert_eq!(
            draft_label(day(2026, 9, 30), at(10, 30), at(11, 30)),
            "Wednesday 30 September, 10:30 - 11:30"
        );
        assert_eq!(
            draft_label(day(2026, 10, 4), at(23, 45), at(23, 59)),
            "Sunday 4 October, 23:45 - 23:59"
        );
    }

    #[test]
    fn a_new_event_starts_today_in_this_week_else_on_the_shown_monday() {
        let today = day(2026, 9, 30);
        assert_eq!(default_day(today, day(2026, 9, 28)), today);
        assert_eq!(default_day(today, day(2026, 10, 3)), today);
        assert_eq!(default_day(today, day(2026, 10, 7)), day(2026, 10, 5));
        assert_eq!(default_day(today, day(2026, 9, 21)), day(2026, 9, 21));
    }

    #[test]
    fn a_new_event_on_todays_week_takes_the_next_full_hour() {
        let today = day(2026, 9, 30);
        // Today's week: the next full hour after now, an hour long.
        assert_eq!(
            new_event_slot(today, today, at(15, 20)),
            (today, at(16, 0), at(17, 0))
        );
        assert_eq!(
            new_event_slot(today, day(2026, 10, 2), at(8, 0)),
            (today, at(9, 0), at(10, 0))
        );
        // Late at night: tomorrow at 09:00 (an event cannot cross midnight).
        assert_eq!(
            new_event_slot(today, today, at(22, 10)),
            (day(2026, 10, 1), at(9, 0), at(10, 0))
        );
        // Another week: its Monday at 09:00, as before.
        assert_eq!(
            new_event_slot(today, day(2026, 10, 7), at(15, 20)),
            (day(2026, 10, 5), at(9, 0), at(10, 0))
        );
    }

    #[test]
    fn a_picked_day_past_the_end_of_its_month_is_cut_to_the_last_day() {
        assert_eq!(picked_date(2026, 9, 30), Some(day(2026, 9, 30)));
        assert_eq!(picked_date(2026, 2, 31), Some(day(2026, 2, 28)));
        assert_eq!(picked_date(2028, 2, 30), Some(day(2028, 2, 29)));
        assert_eq!(picked_date(2026, 4, 31), Some(day(2026, 4, 30)));
        assert_eq!(picked_date(2026, 1, 0), Some(day(2026, 1, 1)));
        assert_eq!(picked_date(2026, 13, 1), None);
        assert_eq!(picked_date(2026, 0, 1), None);
    }

    // ==== Pixels and times ====

    #[test]
    fn a_minute_sits_at_its_share_of_the_hour_height() {
        assert!(close(y_of_minute(0.0, 48.0), 0.0));
        assert!(close(y_of_minute(90.0, 48.0), 72.0));
        assert!(close(y_of_minute(600.0, 48.0), 480.0));
        assert!(close(y_of_minute(600.0, 120.0), 1200.0));
        assert!(close(day_height(48.0), 1152.0));
        assert!(close(day_height(20.0), 480.0));
    }

    #[test]
    fn a_y_reads_back_as_its_minute_at_any_hour_height() {
        for hour_px in [20.0, 48.0, 57.6, 120.0, 240.0] {
            for minute in [0.0, 1.0, 59.5, 605.0, 1439.0] {
                let y = y_of_minute(minute, hour_px);
                assert!(
                    close(minute_at_y(y, hour_px), minute),
                    "{minute} min at {hour_px} px/h: y {y} reads back as {}",
                    minute_at_y(y, hour_px)
                );
            }
        }
    }

    #[test]
    fn a_y_outside_the_day_reads_as_its_nearest_end() {
        assert!(close(minute_at_y(-30.0, 48.0), 0.0));
        assert!(close(minute_at_y(5000.0, 48.0), DAY_MINUTES as f32));
        assert!(close(minute_at_y(f32::NAN, 48.0), 0.0));
        assert!(close(minute_at_y(100.0, 0.0), 0.0));
    }

    #[test]
    fn a_minute_of_the_day_is_a_time() {
        assert_eq!(time_of_minute(0), at(0, 0));
        assert_eq!(time_of_minute(630), at(10, 30));
        assert_eq!(time_of_minute(LAST_MINUTE), at(23, 59));
        assert_eq!(time_of_minute(DAY_MINUTES + 10), at(23, 59));
    }

    // ==== Click and drag to create ====

    #[test]
    fn a_click_makes_an_hour_from_the_quarter_hour_it_lands_in() {
        // 10:36 -> 10:30 - 11:30; exactly on 10:00 -> 10:00 - 11:00; 10:59.9 -> 10:45.
        assert_eq!(click_range(636.0), (630, 690));
        assert_eq!(click_range(600.0), (600, 660));
        assert_eq!(click_range(659.9), (645, 705));
        assert_eq!(click_range(0.0), (0, 60));
    }

    #[test]
    fn a_click_on_a_line_is_not_pushed_into_the_quarter_before_it() {
        // A y on the 14:00 line can read back a hair under 840 minutes.
        assert_eq!(click_range(839.9999), (840, 900));
        assert_eq!(click_range(840.0001), (840, 900));
    }

    #[test]
    fn a_click_late_in_the_day_is_cut_at_the_end_of_the_day() {
        assert_eq!(click_range(23.0 * 60.0 + 20.0), (1395, LAST_MINUTE));
        assert_eq!(click_range(1439.5), (1425, LAST_MINUTE));
        assert_eq!(click_range(DAY_MINUTES as f32), (1425, LAST_MINUTE));
    }

    #[test]
    fn a_drag_covers_the_quarter_hours_it_touched_whichever_way_it_went() {
        // 13:05 -> 14:50: 13:00 - 15:00, down or up.
        assert_eq!(drag_range(785.0, 890.0), (780, 900));
        assert_eq!(drag_range(890.0, 785.0), (780, 900));
        // Lines exactly: 13:00 -> 14:00 is 13:00 - 14:00.
        assert_eq!(drag_range(780.0, 840.0), (780, 840));
        assert_eq!(drag_range(840.0, 780.0), (780, 840));
    }

    #[test]
    fn a_drag_inside_one_quarter_hour_makes_that_quarter_hour() {
        assert_eq!(drag_range(782.0, 790.0), (780, 795));
        assert_eq!(drag_range(790.0, 782.0), (780, 795));
        assert_eq!(drag_range(780.0, 780.0), (780, 795));
    }

    #[test]
    fn a_drag_stays_inside_the_day() {
        assert_eq!(drag_range(1390.0, DAY_MINUTES as f32), (1380, LAST_MINUTE));
        assert_eq!(drag_range(-50.0, 20.0), (0, 30));
        assert_eq!(drag_range(1435.0, 1439.0), (1425, LAST_MINUTE));
        assert_eq!(drag_range(f32::NAN, 20.0), (0, 30));
    }

    #[test]
    fn a_press_becomes_a_drag_once_it_moves_a_few_pixels() {
        assert!(!is_drag(100.0, 100.0));
        assert!(!is_drag(100.0, 100.0 + DRAG_THRESHOLD_PX - 0.5));
        assert!(!is_drag(100.0, 100.0 - DRAG_THRESHOLD_PX + 0.5));
        assert!(is_drag(100.0, 100.0 + DRAG_THRESHOLD_PX));
        assert!(is_drag(100.0, 40.0));
    }

    #[test]
    fn a_press_on_an_event_finds_it_and_a_press_beside_it_finds_nothing() {
        // 09:00 - 10:00 at 48 px/h: y 432 .. 480.
        let blocks = [placed(7, 540, 60, 0, 1)];
        assert_eq!(event_at(&blocks, 450.0, 0.5, 48.0), Some(7));
        assert_eq!(event_at(&blocks, 432.0, 0.0, 48.0), Some(7));
        assert_eq!(event_at(&blocks, 431.0, 0.5, 48.0), None);
        assert_eq!(event_at(&blocks, 480.0, 0.5, 48.0), None);
        // The same event at 120 px/h: y 1080 .. 1200.
        assert_eq!(event_at(&blocks, 1150.0, 0.5, 120.0), Some(7));
    }

    #[test]
    fn a_press_between_side_by_side_events_finds_the_one_in_its_lane() {
        let blocks = [placed(0, 600, 60, 0, 2), placed(1, 600, 60, 1, 2)];
        assert_eq!(event_at(&blocks, 500.0, 0.25, 48.0), Some(0));
        assert_eq!(event_at(&blocks, 500.0, 0.75, 48.0), Some(1));
        // Lane 0 of two, from 10:00 to 11:00: a press in lane 1's column is free.
        let one = [placed(0, 600, 60, 0, 2)];
        assert_eq!(event_at(&one, 500.0, 0.75, 48.0), None);
    }

    #[test]
    fn a_short_event_is_pressed_where_its_block_is_drawn() {
        // Five minutes at 48 px/h is 4 px; the block is drawn MIN_BLOCK_PX tall.
        let blocks = [placed(3, 600, 5, 0, 1)];
        assert_eq!(
            event_at(&blocks, 480.0 + MIN_BLOCK_PX - 1.0, 0.5, 48.0),
            Some(3)
        );
        assert_eq!(
            event_at(&blocks, 480.0 + MIN_BLOCK_PX + 1.0, 0.5, 48.0),
            None
        );
    }

    // ==== Zoom ====

    #[test]
    fn the_hour_height_stays_between_its_limits() {
        assert!(close(clamp_hour_px(48.0), 48.0));
        assert!(close(clamp_hour_px(5.0), MIN_HOUR_PX));
        assert!(close(clamp_hour_px(1000.0), MAX_HOUR_PX));
        assert!(close(clamp_hour_px(f32::NAN), DEFAULT_HOUR_PX));
        assert!(close(MIN_HOUR_PX, 20.0));
        assert!(close(MAX_HOUR_PX, 240.0));
        assert!(close(DEFAULT_HOUR_PX, 48.0));
    }

    #[test]
    fn a_wheel_notch_zooms_a_step_and_a_flick_is_bounded() {
        assert!(close(wheel_zoom_factor(0.0), 1.0));
        // One notch (60 px) up zooms in by 2^(1/4); down zooms out by as much.
        assert!(close(wheel_zoom_factor(60.0), 2f32.powf(0.25)));
        assert!(close(wheel_zoom_factor(-60.0), 2f32.powf(-0.25)));
        assert!(close(wheel_zoom_factor(10_000.0), 1.25));
        assert!(close(wheel_zoom_factor(-10_000.0), 0.8));
        assert!(close(wheel_zoom_factor(f32::NAN), 1.0));
    }

    /// A pinch update as the engine reports it: the scale since the gesture began, and
    /// whether this update began it.
    fn sample(scale: f32, began: bool) -> PinchSample {
        PinchSample { scale, began }
    }

    #[test]
    fn a_pinch_zooms_by_the_change_since_the_gestures_last_update() {
        // The first update of a gesture is measured from 1.0.
        assert!(close(pinch_step(None, sample(1.0, true)), 1.0));
        assert!(close(pinch_step(Some(1.8), sample(1.1, true)), 1.1));
        // Later updates by their ratio to the one before.
        assert!(close(pinch_step(Some(1.2), sample(1.5, false)), 1.25));
        assert!(close(pinch_step(Some(1.5), sample(1.2, false)), 0.8));
        // An update whose gesture start was missed counts from 1.0.
        assert!(close(pinch_step(None, sample(1.3, false)), 1.3));
    }

    #[test]
    fn a_pinch_that_reports_no_usable_scale_does_not_zoom() {
        assert!(close(pinch_step(None, sample(0.0, true)), 1.0));
        assert!(close(pinch_step(Some(1.2), sample(-1.0, false)), 1.0));
        assert!(close(pinch_step(Some(1.2), sample(f32::NAN, false)), 1.0));
        assert!(close(pinch_step(Some(0.0), sample(1.2, false)), 1.2));
    }

    /// REPORTED (AzMaps' twin, 2026-09-30): a trackpad pinch jittered between zooming in and
    /// out, because macOS's per-event magnification deltas were read as cumulative scales.
    /// Fed the engine's cumulative updates, the hour only grows while the fingers spread, and
    /// a second gesture starts where the first left off.
    #[test]
    fn a_zoom_in_pinch_only_grows_the_hour_and_a_second_pinch_does_not_jump() {
        let mut hour = DEFAULT_HOUR_PX;
        let mut last: Option<f32> = None;
        let mut apply = |s: PinchSample| {
            hour = clamp_hour_px(hour * pinch_step(last, s));
            last = Some(s.scale);
            hour
        };
        // Trackpad updates of +2 %, +1 %, +3 %: cumulative 1.02, 1.0302, 1.061106.
        let mut before = DEFAULT_HOUR_PX;
        for s in [
            sample(1.0, true),
            sample(1.02, false),
            sample(1.0302, false),
            sample(1.061_106, false),
        ] {
            let now = apply(s);
            assert!(
                now >= before,
                "a zoom-in update (scale {}) shrank the hour: {before} -> {now}",
                s.scale
            );
            before = now;
        }
        assert!(close(before, DEFAULT_HOUR_PX * 1.061_106));
        // A second gesture begins at 1.0: the hour stays where the first left it.
        let at_start = apply(sample(1.0, true));
        assert!(
            close(at_start, before),
            "a new gesture jumped the hour: {before} -> {at_start}"
        );
        let after = apply(sample(1.05, false));
        assert!(close(after, before * 1.05));
    }

    #[test]
    fn zooming_keeps_the_time_under_the_pointer_under_the_pointer() {
        // 08:00 at the top of a 600 px view at 48 px/h, the pointer 100 px down (10:05).
        let (scroll, pointer, view) = (384.0, 100.0, 600.0);
        let before = minute_at_y(scroll + pointer, 48.0);
        assert!(close(before, 605.0));
        for new_px in [60.0, 96.0, 120.0, 240.0] {
            let after_scroll = zoom_scroll(48.0, new_px, pointer, scroll, view);
            let after = minute_at_y(after_scroll + pointer, new_px);
            assert!(
                close(after, before),
                "at {new_px} px/h the pointer is over {after} min, not {before}"
            );
        }
    }

    #[test]
    fn zooming_near_the_top_or_the_bottom_stays_inside_the_day() {
        // Zooming out with 00:30 under the pointer near the top: the view cannot scroll above
        // midnight.
        assert!(close(zoom_scroll(96.0, 48.0, 50.0, 0.0, 600.0), 0.0));
        // Zooming out at the bottom of the day: the view ends at 24:00.
        let bottom = zoom_scroll(96.0, 48.0, 500.0, day_height(96.0) - 600.0, 600.0);
        assert!(close(bottom, max_scroll(48.0, 600.0)));
        assert!(close(max_scroll(48.0, 600.0), 1152.0 - 600.0));
        // A day shorter than the view does not scroll at all.
        assert!(close(max_scroll(20.0, 600.0), 0.0));
        assert!(close(zoom_scroll(48.0, 20.0, 300.0, 400.0, 600.0), 0.0));
    }

    #[test]
    fn a_saved_event_out_of_view_is_scrolled_to_with_an_hour_above_it() {
        // A 600 px view at 48 px/h scrolled to 14:00 (672 px): an event at 09:00 is above it.
        assert_eq!(reveal_scroll(9 * 60, 48.0, 672.0, 600.0), Some(384.0));
        // At 00:30 the hour above it is cut at midnight.
        assert_eq!(reveal_scroll(30, 48.0, 672.0, 600.0), Some(0.0));
        // Below the view: 23:00 with the view at midnight; the view stops at the day's end.
        assert_eq!(
            reveal_scroll(23 * 60, 48.0, 0.0, 600.0),
            Some(max_scroll(48.0, 600.0))
        );
        // At 60 px/h a y is its minute: a block whose top is 1 px too low for its drawn
        // height to fit is out of view.
        assert_eq!(reveal_scroll(583, 60.0, 0.0, 600.0), Some(523.0));
    }

    #[test]
    fn a_saved_event_in_view_leaves_the_view_where_it_is() {
        assert_eq!(reveal_scroll(10 * 60, 48.0, 384.0, 600.0), None);
        // At 60 px/h: its top 18 px (MIN_BLOCK_PX) above the bottom edge still fits.
        assert_eq!(reveal_scroll(582, 60.0, 0.0, 600.0), None);
        assert_eq!(reveal_scroll(0, 48.0, 0.0, 600.0), None);
    }

    #[test]
    fn the_view_opens_at_eight_or_an_hour_before_now_on_todays_week() {
        assert_eq!(first_minute_shown(false, at(14, 20)), 8 * 60);
        assert_eq!(first_minute_shown(true, at(14, 20)), 13 * 60);
        assert_eq!(first_minute_shown(true, at(14, 0)), 13 * 60);
        assert_eq!(first_minute_shown(true, at(0, 30)), 0);
        assert_eq!(first_minute_shown(true, at(23, 59)), 22 * 60);
    }
}
