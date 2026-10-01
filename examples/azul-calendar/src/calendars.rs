//! Calendars: the lists of "My calendars", each with a name and a colour, one JSON file each,
//! `<data dir>/calendars/<calendar id>.json` (the object key `calendars/<id>.json`, as the events'
//! files are). An event names its calendar by id (`event.rs`, `"calendar"`); an event without one
//! is in the default calendar, which has the empty id and the file `calendars/default.json` once
//! it is renamed or recoloured (until then it is "Calendar", in blue, with no file).
//!
//! ```json
//! { "format": "azcalendar.calendar", "version": 1, "id": "9d4c1f3a-...", "name": "Work", "colour": "green" }
//! ```
//!
//! Which calendars are shown is how this device looks at them, not data: the settings file keeps
//! the hidden ones (`hidden_calendars=`, `settings.rs`).

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::event::{self, is_calendar_id};

/// The `format` of a calendar file.
pub const FORMAT: &str = "azcalendar.calendar";
/// The version this AzCalendar writes and reads.
pub const VERSION: u64 = 1;
/// The folder, and the object-key prefix, of the calendar files.
pub const CALENDARS_DIR: &str = "calendars";
/// The default calendar's name until it is renamed.
pub const DEFAULT_NAME: &str = "Calendar";
/// The default calendar's file name (its id is empty).
const DEFAULT_FILE: &str = "default.json";
/// How the default calendar is written in the settings file's list of hidden calendars.
const DEFAULT_TOKEN: &str = "default";

/// A calendar's colour: its events' tint and edge, and its swatch in "My calendars" (Outlook's
/// calendar colours).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Colour {
    Blue,
    Green,
    Purple,
    Orange,
    Red,
    Teal,
    Olive,
    Grey,
}

/// A colour's paint in the light and the dark mode: an event's background and its edge (the
/// edge is the swatch too).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint {
    pub light_fill: &'static str,
    pub light_edge: &'static str,
    pub dark_fill: &'static str,
    pub dark_edge: &'static str,
}

impl Colour {
    /// Every colour, in the order new calendars take them.
    pub const ALL: [Colour; 8] = [
        Colour::Blue,
        Colour::Green,
        Colour::Purple,
        Colour::Orange,
        Colour::Red,
        Colour::Teal,
        Colour::Olive,
        Colour::Grey,
    ];

    /// The colour's name in a file: "blue".
    #[must_use]
    pub const fn name(self) -> &'static str {
        todo!()
    }

    /// The colour a file names; `None` for a name no colour has.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Colour> {
        todo!()
    }

    /// The colour's name for people: "Blue".
    #[must_use]
    pub const fn label(self) -> &'static str {
        todo!()
    }

    /// The colour's paint. Blue is the tint events had before calendars had colours.
    #[must_use]
    pub const fn paint(self) -> Paint {
        todo!()
    }

    /// An event's box in this colour: the tint with the edge on its left, and their dark twins.
    #[must_use]
    pub fn event_css(self) -> String {
        todo!()
    }

    /// A whole-day bar in this colour (the all-day row, the month view): the tint, edged all
    /// round.
    #[must_use]
    pub fn bar_css(self) -> String {
        todo!()
    }

    /// The swatch: the edge colour, in both modes.
    #[must_use]
    pub fn swatch_css(self) -> String {
        todo!()
    }
}

/// A calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    /// Empty for the default calendar, else a UUID in lower case (it names the file).
    pub id: String,
    pub name: String,
    pub colour: Colour,
}

impl Calendar {
    /// The default calendar as it is before anyone renamed or recoloured it.
    #[must_use]
    pub fn default_calendar() -> Calendar {
        todo!()
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        todo!()
    }
}

#[derive(Serialize, Deserialize)]
struct CalendarFile {
    format: String,
    version: u64,
    #[serde(default)]
    id: String,
    name: String,
    colour: String,
}

/// `default.json` for the default calendar, `<id>.json` for any other.
#[must_use]
pub fn file_name(id: &str) -> String {
    todo!()
}

/// `calendars/<file>`: the calendar's path under the data folder, and its key in a bucket.
#[must_use]
pub fn object_key(id: &str) -> String {
    todo!()
}

/// Where the calendar `id` is stored under `data_dir`.
#[must_use]
pub fn calendar_path(data_dir: &Path, id: &str) -> PathBuf {
    todo!()
}

/// The calendar's file contents (pretty JSON, ending in a newline).
#[must_use]
pub fn to_json(calendar: &Calendar) -> String {
    todo!()
}

/// Reads a calendar file: its format and version, an id that names a calendar, a name (trimmed,
/// not empty) and a colour.
pub fn from_json(text: &str) -> Result<Calendar, String> {
    todo!()
}

/// Writes `calendar` to its file, atomically, and returns the file's path.
pub fn save(data_dir: &Path, calendar: &Calendar) -> std::io::Result<PathBuf> {
    todo!()
}

/// Removes the calendar's file (a missing file is not an error). The default calendar cannot be
/// removed: its file goes and it is "Calendar" in blue again.
pub fn remove(data_dir: &Path, id: &str) -> std::io::Result<()> {
    todo!()
}

/// Every calendar: the default one first (its file, or as it is before one), then the others by
/// name. A file that does not read, or holds another calendar than its name says, is left out.
#[must_use]
pub fn load_all(data_dir: &Path) -> Vec<Calendar> {
    todo!()
}

/// A new calendar's id: a random version-4 UUID, as an event's.
#[must_use]
pub fn new_calendar_id() -> String {
    todo!()
}

/// The colour a new calendar takes: the first one no calendar has, else the one fewest have.
#[must_use]
pub fn next_colour(calendars: &[Calendar]) -> Colour {
    todo!()
}

/// The calendar an event with calendar id `id` is in: that calendar, else (a calendar that is
/// gone) the default one.
#[must_use]
pub fn calendar_of<'a>(calendars: &'a [Calendar], id: &str) -> Option<&'a Calendar> {
    todo!()
}

/// The settings value listing the hidden calendars' ids (`default` for the default calendar),
/// sorted, comma-separated.
#[must_use]
pub fn hidden_value(hidden: &BTreeSet<String>) -> String {
    todo!()
}

/// The hidden calendars a settings value lists (what `hidden_value` wrote); ids that name no
/// calendar are left out.
#[must_use]
pub fn hidden_of(value: &str) -> BTreeSet<String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_dir::TempDir;

    const WORK: &str = "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3";

    fn work() -> Calendar {
        Calendar {
            id: String::from(WORK),
            name: String::from("Work"),
            colour: Colour::Green,
        }
    }

    #[test]
    fn a_calendar_file_round_trips_and_says_what_it_is() {
        let text = to_json(&work());
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["format"], FORMAT);
        assert_eq!(json["version"], 1);
        assert_eq!(json["name"], "Work");
        assert_eq!(json["colour"], "green");
        assert_eq!(from_json(&text), Ok(work()));
        assert!(from_json(&text.replace("green", "chartreuse")).is_err());
        assert!(from_json(&text.replace(WORK, "../x")).is_err());
        assert!(from_json(&text.replace("\"Work\"", "\"  \"")).is_err());
    }

    #[test]
    fn the_default_calendar_is_there_without_a_file_and_first() {
        let dir = TempDir::create();
        assert_eq!(load_all(&dir.0), vec![Calendar::default_calendar()]);
        save(&dir.0, &work()).unwrap();
        let mut renamed = Calendar::default_calendar();
        renamed.name = String::from("Home");
        renamed.colour = Colour::Teal;
        save(&dir.0, &renamed).unwrap();
        assert!(dir.0.join("calendars").join("default.json").is_file());
        assert_eq!(load_all(&dir.0), vec![renamed, work()]);
        remove(&dir.0, "").unwrap();
        remove(&dir.0, WORK).unwrap();
        remove(&dir.0, WORK).unwrap();
        assert_eq!(load_all(&dir.0), vec![Calendar::default_calendar()]);
    }

    #[test]
    fn a_new_calendar_takes_the_first_colour_no_calendar_has() {
        let mut all = vec![Calendar::default_calendar()];
        assert_eq!(next_colour(&all), Colour::Green);
        all.push(work());
        assert_eq!(next_colour(&all), Colour::Purple);
        assert_eq!(next_colour(&[]), Colour::Blue);
    }

    #[test]
    fn an_event_of_a_calendar_that_is_gone_is_in_the_default_one() {
        let all = vec![Calendar::default_calendar(), work()];
        assert_eq!(
            calendar_of(&all, WORK).map(|c| c.name.as_str()),
            Some("Work")
        );
        assert_eq!(
            calendar_of(&all, "11111111-2222-4333-8444-555555555555").map(|c| c.name.as_str()),
            Some(DEFAULT_NAME)
        );
    }

    #[test]
    fn hidden_calendars_round_trip_through_their_settings_value() {
        let hidden: BTreeSet<String> = [String::new(), String::from(WORK)].into_iter().collect();
        let value = hidden_value(&hidden);
        assert_eq!(value, format!("default,{WORK}"));
        assert_eq!(hidden_of(&value), hidden);
        assert_eq!(
            hidden_of(" ,../x, default"),
            [String::new()].into_iter().collect()
        );
    }

    #[test]
    fn every_colour_has_a_dark_twin_and_a_name_that_reads_back() {
        for c in Colour::ALL {
            assert_eq!(Colour::from_name(c.name()), Some(c));
            for css in [c.event_css(), c.bar_css(), c.swatch_css()] {
                assert!(
                    css.contains("@media (prefers-color-scheme: dark) {"),
                    "{css}"
                );
            }
        }
    }
}
