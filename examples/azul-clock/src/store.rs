//! AzClock's files in the data tree - the layout the user's S3 bucket will
//! have (the azlin cloud storage split: durable data are files, one per
//! record):
//!
//! - `clock/alarms/<uuid>.json` - one alarm ([`crate::alarm::Alarm`]);
//! - `clock/timers/<uuid>.json` - one timer, running ones with the instant
//!   they end ([`crate::timer::CountdownTimer`]);
//! - `clock/world.json` - the world clock's cities, in order;
//! - `clock/stopwatch.json` - the stopwatch, running or not;
//! - `clock/settings.json` - azul-appkit's settings file (theme, mode, the
//!   clock's own settings).
//!
//! The window never touches a file: it hands [`load_jobs`] / the writes of
//! its `azul_pim::write_queue::WriteQueue` to azul-appkit's file thread, which
//! runs them on the data root's `Drive` (a `LocalDrive` today), and reads the
//! outcomes back with [`read_loaded`].

use azul_appkit::files::{FileJob, FileOutcome};
use chrono::{DateTime, TimeZone, Utc};

use crate::{
    alarm::Alarm,
    stopwatch::Stopwatch,
    timer::{CountdownTimer, MINUTE_MS},
    world::{self, WorldFile},
};

/// The app's folder in the data tree.
pub const APP_FOLDER: &str = "clock";
/// Where the alarms are (one file each).
pub const ALARMS_PREFIX: &str = "clock/alarms/";
/// Where the timers are (one file each).
pub const TIMERS_PREFIX: &str = "clock/timers/";
pub const WORLD_KEY: &str = "clock/world.json";
pub const STOPWATCH_KEY: &str = "clock/stopwatch.json";
/// Every record file ends so.
pub const SUFFIX: &str = ".json";

/// The file of an alarm.
#[must_use]
pub fn alarm_key(id: &str) -> String {
    format!("{ALARMS_PREFIX}{id}{SUFFIX}")
}

/// The file of a timer.
#[must_use]
pub fn timer_key(id: &str) -> String {
    format!("{TIMERS_PREFIX}{id}{SUFFIX}")
}

/// A new record id (a random UUID: it names a file in the user's bucket).
#[must_use]
pub fn new_id() -> String {
    azul_storage::ids::new_uuid()
}

/// A record as the bytes of its file (pretty JSON, newline-terminated).
#[must_use]
pub fn to_bytes<T: serde::Serialize>(record: &T) -> Vec<u8> {
    let _ = record;
    Vec::new()
}

/// What the window reads when it opens: every alarm and timer file, the
/// world clock and the stopwatch.
#[must_use]
pub fn load_jobs() -> Vec<FileJob> {
    Vec::new()
}

/// What the files said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loaded {
    /// By time of day, then label.
    pub alarms: Vec<Alarm>,
    /// Shortest first.
    pub timers: Vec<CountdownTimer>,
    /// `None`: no file yet (the first run).
    pub world: Option<WorldFile>,
    pub stopwatch: Option<Stopwatch>,
    /// One sentence per file that could not be read.
    pub problems: Vec<String>,
}

impl Loaded {
    /// Nothing at all was found: the first run.
    #[must_use]
    pub fn is_first_run(&self) -> bool {
        self.alarms.is_empty() && self.timers.is_empty() && self.world.is_none() && self.stopwatch.is_none()
    }
}

/// Reads the outcomes of [`load_jobs`]. A file that does not parse is
/// skipped and named in `problems`; one whose id differs from its file name
/// takes the file's name (a copied file must not overwrite its original).
#[must_use]
pub fn read_loaded(outcomes: Vec<FileOutcome>) -> Loaded {
    let _ = outcomes;
    Loaded::default()
}

/// Sorts alarms as the list shows them: by time of day, then label.
pub fn sort_alarms(alarms: &mut [Alarm]) {
    let _ = alarms;
}

/// The plan's sample data (section 6), for `--sample` on a first run:
/// 06:30 "Gym" Mon / Wed / Fri on, 07:15 "Wake up" Mon-Fri on, 09:00
/// "Market" Saturday off; the timers "Tea" (10:00, 05:48 left, running) and
/// "Pasta" (12:00, 07:12 left, paused); the plan's cities; the stopwatch's
/// five laps, stopped.
#[must_use]
pub fn sample<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> Loaded {
    let _ = (now, tz, MINUTE_MS, world::sample_cities);
    Loaded::default()
}

#[cfg(test)]
mod tests {
    use azul_storage::LocalDrive;
    use chrono_tz::Europe::Berlin;

    use super::*;
    use crate::timer::TimerState;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 3, 8, 0, 0).unwrap()
    }

    /// A folder of its own under the temp dir, removed afterwards.
    struct Temp(std::path::PathBuf);

    impl Temp {
        fn new(name: &str) -> Temp {
            let dir = std::env::temp_dir().join(format!("azclock-{name}-{}", new_id()));
            std::fs::create_dir_all(&dir).unwrap();
            Temp(dir)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn every_record_is_a_file_of_its_own_in_the_clock_folder() {
        assert_eq!(alarm_key("7f3a"), "clock/alarms/7f3a.json");
        assert_eq!(timer_key("t1"), "clock/timers/t1.json");
        assert_eq!(
            azul_appkit::data::app_key(APP_FOLDER, "world.json"),
            WORLD_KEY,
            "the same folder appkit's settings live in"
        );
        let id = new_id();
        assert!(azul_storage::ids::is_uuid(&id), "{id}");
        assert_ne!(id, new_id());
    }

    #[test]
    fn the_sample_is_the_plans() {
        let s = sample(now(), &Berlin);
        let alarms: Vec<(u32, u32, &str, bool, String)> = s
            .alarms
            .iter()
            .map(|a| (a.hour, a.minute, a.label.as_str(), a.enabled, a.repeat_label()))
            .collect();
        assert_eq!(
            alarms,
            vec![
                (6, 30, "Gym", true, "Mon Wed Fri".to_string()),
                (7, 15, "Wake up", true, "Weekdays".to_string()),
                (9, 0, "Market", false, "Sat".to_string()),
            ]
        );
        let t0 = now().timestamp_millis();
        let tea = &s.timers[0];
        assert_eq!((tea.label.as_str(), tea.duration_ms), ("Tea", 10 * MINUTE_MS));
        assert_eq!(tea.remaining_ms(t0), 5 * MINUTE_MS + 48_000);
        let pasta = &s.timers[1];
        assert_eq!(pasta.state, TimerState::Paused { remaining_ms: 7 * MINUTE_MS + 12_000 });
        assert_eq!(s.world.as_ref().map(|w| w.cities.len()), Some(4));
        let stopwatch = s.stopwatch.as_ref().unwrap();
        assert_eq!(stopwatch.laps.len(), 5);
        assert!(!stopwatch.is_running());
        assert_eq!(stopwatch.elapsed(t0), 257_360);
    }

    #[test]
    fn what_is_written_reads_back_through_a_drive() {
        let dir = Temp::new("roundtrip");
        let drive = LocalDrive::new(&dir.0);
        let s = sample(now(), &Berlin);
        let mut jobs: Vec<FileJob> = Vec::new();
        for a in &s.alarms {
            jobs.push(FileJob::Put { key: alarm_key(&a.id), bytes: to_bytes(a) });
        }
        for t in &s.timers {
            jobs.push(FileJob::Put { key: timer_key(&t.id), bytes: to_bytes(t) });
        }
        jobs.push(FileJob::Put { key: WORLD_KEY.to_string(), bytes: to_bytes(s.world.as_ref().unwrap()) });
        jobs.push(FileJob::Put { key: STOPWATCH_KEY.to_string(), bytes: to_bytes(s.stopwatch.as_ref().unwrap()) });
        let written = azul_appkit::files::run_jobs(&drive, jobs);
        assert!(written.iter().all(|o| o.error().is_none()), "{written:?}");

        let loaded = read_loaded(azul_appkit::files::run_jobs(&drive, load_jobs()));
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.alarms, s.alarms);
        assert_eq!(loaded.timers, s.timers);
        assert_eq!(loaded.world, s.world);
        assert_eq!(loaded.stopwatch, s.stopwatch);
        assert!(!loaded.is_first_run());
    }

    #[test]
    fn an_empty_data_tree_is_a_first_run() {
        let dir = Temp::new("empty");
        let drive = LocalDrive::new(&dir.0);
        let loaded = read_loaded(azul_appkit::files::run_jobs(&drive, load_jobs()));
        assert!(loaded.is_first_run());
        assert!(loaded.problems.is_empty());
    }

    #[test]
    fn a_damaged_file_costs_only_that_record_and_a_copy_keeps_its_own_name() {
        let good = Alarm::new("good", 7, 0, chrono::NaiveDate::from_ymd_opt(2026, 10, 3).unwrap());
        let outcomes = vec![
            FileOutcome::GotAll {
                prefix: ALARMS_PREFIX.to_string(),
                files: vec![
                    (alarm_key("bad"), b"{ not json".to_vec()),
                    (alarm_key("copy"), to_bytes(&good)),
                    (alarm_key("good"), to_bytes(&good)),
                ],
                errors: vec![],
            },
            FileOutcome::GotAll { prefix: TIMERS_PREFIX.to_string(), files: vec![], errors: vec!["disk on fire".into()] },
            FileOutcome::Got { key: WORLD_KEY.to_string(), result: Ok(None) },
            FileOutcome::Got { key: STOPWATCH_KEY.to_string(), result: Ok(Some(b"{}".to_vec())) },
        ];
        let loaded = read_loaded(outcomes);
        let ids: Vec<&str> = loaded.alarms.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["copy", "good"], "the copy is a record of its own");
        assert_eq!(loaded.problems.len(), 2, "{:?}", loaded.problems);
        assert!(loaded.problems[0].contains("clock/alarms/bad.json"), "{:?}", loaded.problems);
        assert_eq!(loaded.world, None);
        assert_eq!(loaded.stopwatch, Some(Stopwatch::default()));
    }

    #[test]
    fn alarms_list_by_time_of_day_then_label() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 10, 3).unwrap();
        let mut alarms = vec![
            Alarm::new("c", 9, 0, d).labelled("b"),
            Alarm::new("a", 6, 30, d),
            Alarm::new("b", 9, 0, d).labelled("a"),
        ];
        sort_alarms(&mut alarms);
        let ids: Vec<&str> = alarms.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }
}
