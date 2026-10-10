//! The stopwatch: elapsed time and laps, derived from the wall clock (ms
//! since 1970) like the timer, so it keeps running while the app is closed
//! (`clock/stopwatch.json`), as a phone's does.

use serde::{Deserialize, Serialize};

use crate::fmt;

/// The stopwatch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stopwatch {
    /// Running since this instant; `None` = stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_since: Option<i64>,
    /// The time counted before `running_since`.
    #[serde(default)]
    pub accumulated_ms: i64,
    /// The total at each lap, oldest first.
    #[serde(default)]
    pub laps: Vec<i64>,
}

/// How a lap compares with the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapMark {
    None,
    Fastest,
    Slowest,
}

impl LapMark {
    /// The badge's text ("" for none).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            LapMark::None => "",
            LapMark::Fastest => "fastest",
            LapMark::Slowest => "slowest",
        }
    }
}

/// One row of the laps list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LapRow {
    /// 1 for the first lap.
    pub number: usize,
    /// This lap alone.
    pub lap_ms: i64,
    /// The total at its end.
    pub total_ms: i64,
    pub mark: LapMark,
}

impl Stopwatch {
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running_since.is_some()
    }

    /// The time at `now`.
    #[must_use]
    pub fn elapsed(&self, now: i64) -> i64 {
        self.accumulated_ms + self.running_since.map_or(0, |since| (now - since).max(0))
    }

    /// Start (or go on) at `now`.
    pub fn start(&mut self, now: i64) {
        if self.running_since.is_none() {
            self.running_since = Some(now);
        }
    }

    /// Stop at `now`, keeping the time.
    pub fn stop(&mut self, now: i64) {
        if self.running_since.is_some() {
            self.accumulated_ms = self.elapsed(now);
            self.running_since = None;
        }
    }

    /// Start when stopped, stop when running.
    pub fn toggle(&mut self, now: i64) {
        if self.is_running() {
            self.stop(now);
        } else {
            self.start(now);
        }
    }

    /// Record a lap at `now` (only while it runs).
    pub fn lap(&mut self, now: i64) {
        if self.is_running() {
            self.laps.push(self.elapsed(now));
        }
    }

    /// Back to zero, no laps, stopped.
    pub fn reset(&mut self) {
        *self = Stopwatch::default();
    }

    /// The laps, newest first, the fastest and the slowest marked once
    /// there are two or more.
    #[must_use]
    pub fn rows(&self) -> Vec<LapRow> {
        let mut previous = 0;
        let mut rows: Vec<LapRow> = self
            .laps
            .iter()
            .enumerate()
            .map(|(i, &total)| {
                let row = LapRow {
                    number: i + 1,
                    lap_ms: total - previous,
                    total_ms: total,
                    mark: LapMark::None,
                };
                previous = total;
                row
            })
            .collect();
        if rows.len() >= 2 {
            // The first of equal laps is the one marked.
            let fastest = rows
                .iter()
                .enumerate()
                .min_by_key(|(i, r)| (r.lap_ms, *i))
                .map(|(i, _)| i);
            let slowest = rows
                .iter()
                .enumerate()
                .max_by_key(|(i, r)| (r.lap_ms, core::cmp::Reverse(*i)))
                .map(|(i, _)| i);
            if let (Some(f), Some(s)) = (fastest, slowest) {
                if f != s {
                    rows[f].mark = LapMark::Fastest;
                    rows[s].mark = LapMark::Slowest;
                }
            }
        }
        rows.reverse();
        rows
    }

    /// The laps as text for the clipboard: a header and one tab-separated
    /// line per lap, newest first.
    #[must_use]
    pub fn laps_text(&self) -> String {
        let mut out = String::from("Lap\tLap time\tTotal\n");
        for row in self.rows() {
            out.push_str(&format!(
                "{}\t{}\t{}\n",
                row.number,
                fmt::lap(row.lap_ms),
                fmt::lap(row.total_ms)
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_790_000_000_000;

    /// The plan's five laps (section 6): totals 0:50.46 .. 4:17.36.
    fn plan_laps() -> Stopwatch {
        let mut s = Stopwatch::default();
        s.start(T0);
        for total in [50_460, 102_470, 156_440, 206_240, 257_360] {
            s.lap(T0 + total);
        }
        s
    }

    #[test]
    fn the_time_adds_up_across_stops() {
        let mut s = Stopwatch::default();
        assert_eq!(s.elapsed(T0), 0);
        s.start(T0);
        assert_eq!(s.elapsed(T0 + 1_234), 1_234);
        s.stop(T0 + 5_000);
        assert_eq!(s.elapsed(T0 + 9_000), 5_000, "stopped: it stands");
        s.toggle(T0 + 10_000);
        assert!(s.is_running());
        assert_eq!(s.elapsed(T0 + 12_000), 7_000);
        s.start(T0 + 13_000);
        assert_eq!(s.elapsed(T0 + 14_000), 9_000, "starting a running stopwatch changes nothing");
    }

    #[test]
    fn the_laps_show_newest_first_with_the_fastest_and_the_slowest_marked() {
        let rows = plan_laps().rows();
        let numbers: Vec<usize> = rows.iter().map(|r| r.number).collect();
        assert_eq!(numbers, vec![5, 4, 3, 2, 1]);
        let laps: Vec<i64> = rows.iter().map(|r| r.lap_ms).collect();
        assert_eq!(laps, vec![51_120, 49_800, 53_970, 52_010, 50_460]);
        assert_eq!(rows[0].total_ms, 257_360);
        assert_eq!(rows[1].mark, LapMark::Fastest, "lap 4");
        assert_eq!(rows[2].mark, LapMark::Slowest, "lap 3");
        assert_eq!(rows[0].mark, LapMark::None);
    }

    #[test]
    fn one_lap_is_neither_fastest_nor_slowest() {
        let mut s = Stopwatch::default();
        s.start(T0);
        s.lap(T0 + 3_000);
        assert_eq!(s.rows()[0].mark, LapMark::None);
        // A lap while stopped is not taken.
        s.stop(T0 + 4_000);
        s.lap(T0 + 5_000);
        assert_eq!(s.laps.len(), 1);
    }

    #[test]
    fn reset_clears_the_time_and_the_laps() {
        let mut s = plan_laps();
        s.reset();
        assert_eq!(s, Stopwatch::default());
    }

    #[test]
    fn the_laps_copy_as_tab_separated_lines() {
        let text = plan_laps().laps_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Lap\tLap time\tTotal");
        assert_eq!(lines[1], "5\t00:51.12\t04:17.36");
        assert_eq!(lines[5], "1\t00:50.46\t00:50.46");
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn a_running_stopwatch_survives_a_restart() {
        let s = plan_laps();
        let json = serde_json::to_string(&s).unwrap();
        let back: Stopwatch = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.elapsed(T0 + 3_600_000), 3_600_000);
        assert_eq!(serde_json::from_str::<Stopwatch>("{}").unwrap(), Stopwatch::default());
    }
}
