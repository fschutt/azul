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

use std::collections::BTreeSet;

use azul_storage::Drive;
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
        match self {
            Colour::Blue => "blue",
            Colour::Green => "green",
            Colour::Purple => "purple",
            Colour::Orange => "orange",
            Colour::Red => "red",
            Colour::Teal => "teal",
            Colour::Olive => "olive",
            Colour::Grey => "grey",
        }
    }

    /// The colour a file names; `None` for a name no colour has.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Colour> {
        Colour::ALL.into_iter().find(|c| c.name() == name.trim())
    }

    /// The colour's name for people: "Blue".
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Colour::Blue => "Blue",
            Colour::Green => "Green",
            Colour::Purple => "Purple",
            Colour::Orange => "Orange",
            Colour::Red => "Red",
            Colour::Teal => "Teal",
            Colour::Olive => "Olive",
            Colour::Grey => "Grey",
        }
    }

    /// The colour's paint. Blue is the tint events had before calendars had colours.
    #[must_use]
    pub const fn paint(self) -> Paint {
        let (light_fill, light_edge, dark_fill, dark_edge) = match self {
            Colour::Blue => ("#dbe7ff", "#2f6db0", "#233a5e", "#6ea8ff"),
            Colour::Green => ("#dcefdc", "#3a8a3a", "#1f3d24", "#7cc47c"),
            Colour::Purple => ("#e8dcf5", "#7a4bb0", "#3a2a52", "#b58ee8"),
            Colour::Orange => ("#fde5cc", "#c46a14", "#4a3016", "#f0a35c"),
            Colour::Red => ("#f9dada", "#b83232", "#4d2222", "#f08a8a"),
            Colour::Teal => ("#d4eeee", "#22817f", "#183d3d", "#6cc9c6"),
            Colour::Olive => ("#ececcc", "#7d7d23", "#3a3a1c", "#c9c97a"),
            Colour::Grey => ("#e6e6e6", "#6b6b6b", "#333333", "#a8a8a8"),
        };
        Paint {
            light_fill,
            light_edge,
            dark_fill,
            dark_edge,
        }
    }

    /// An event's box in this colour: the tint with the edge on its left, and their dark twins.
    #[must_use]
    pub fn event_css(self) -> String {
        let p = self.paint();
        format!(
            "background: {}; border-left: 3px solid {}; @media (prefers-color-scheme: dark) {{ \
             background: {}; border-left: 3px solid {}; }}",
            p.light_fill, p.light_edge, p.dark_fill, p.dark_edge
        )
    }

    /// A whole-day bar in this colour (the all-day row, the month view): the tint, edged all
    /// round.
    #[must_use]
    pub fn bar_css(self) -> String {
        let p = self.paint();
        format!(
            "background: {}; border: 1px solid {}; @media (prefers-color-scheme: dark) {{ \
             background: {}; border: 1px solid {}; }}",
            p.light_fill, p.light_edge, p.dark_fill, p.dark_edge
        )
    }

    /// The swatch: the edge colour, in both modes.
    #[must_use]
    pub fn swatch_css(self) -> String {
        let p = self.paint();
        format!(
            "background: {}; @media (prefers-color-scheme: dark) {{ background: {}; }}",
            p.light_edge, p.dark_edge
        )
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
        Calendar {
            id: String::new(),
            name: String::from(DEFAULT_NAME),
            colour: Colour::Blue,
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        self.id.is_empty()
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
    if id.is_empty() {
        String::from(DEFAULT_FILE)
    } else {
        format!("{id}.json")
    }
}

/// `calendars/<file>`: the calendar's path under the data folder, and its key in a bucket.
#[must_use]
pub fn object_key(id: &str) -> String {
    format!("{CALENDARS_DIR}/{}", file_name(id))
}

/// The calendar's file contents (pretty JSON, ending in a newline).
#[must_use]
pub fn to_json(calendar: &Calendar) -> String {
    let file = CalendarFile {
        format: FORMAT.to_string(),
        version: VERSION,
        id: calendar.id.clone(),
        name: calendar.name.clone(),
        colour: calendar.colour.name().to_string(),
    };
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads a calendar file: its format and version, an id that names a calendar, a name (trimmed,
/// not empty) and a colour.
pub fn from_json(text: &str) -> Result<Calendar, String> {
    let file: CalendarFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.format != FORMAT {
        return Err(format!(
            "not an AzCalendar calendar (no \"format\": \"{FORMAT}\")"
        ));
    }
    if file.version != VERSION {
        return Err(format!(
            "version {} (this AzCalendar reads {VERSION})",
            file.version
        ));
    }
    if !is_calendar_id(&file.id) {
        return Err(format!("the id {:?} is not a calendar's", file.id));
    }
    let name = file.name.trim();
    if name.is_empty() {
        return Err(String::from("the calendar has no name"));
    }
    let colour = Colour::from_name(&file.colour)
        .ok_or_else(|| format!("the colour {:?} is not one AzCalendar has", file.colour))?;
    Ok(Calendar {
        id: file.id,
        name: name.to_string(),
        colour,
    })
}

/// Every calendar the drive keeps (`calendars/<id>.json`, `calendars/default.json`): the default
/// one first (its file, or as it is before one), then the others by name. A file that does not
/// read, or holds another calendar than its name says, is left out.
#[must_use]
pub fn load(drive: &dyn Drive) -> Vec<Calendar> {
    let mut default = Calendar::default_calendar();
    let mut others = Vec::new();
    let prefix = format!("{CALENDARS_DIR}/");
    for object in azul_storage::ops::list_all(drive, &prefix).unwrap_or_default() {
        let Some(name) = object.key.strip_prefix(&prefix) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        let id = if name == DEFAULT_FILE { "" } else { id };
        if !is_calendar_id(id) {
            continue;
        }
        let Ok(calendar) = drive
            .get(&object.key)
            .map_err(|e| e.to_string())
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
            .and_then(|text| from_json(&text))
        else {
            continue;
        };
        if calendar.id != id {
            continue;
        }
        if calendar.is_default() {
            default = calendar;
        } else {
            others.push(calendar);
        }
    }
    others.sort_by(|a, b| (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)));
    let mut all = vec![default];
    all.extend(others);
    all
}


/// A new calendar's id: a random version-4 UUID, as an event's.
#[must_use]
pub fn new_calendar_id() -> String {
    event::new_event_id()
}

/// The colour a new calendar takes: the first one no calendar has, else the one fewest have.
#[must_use]
pub fn next_colour(calendars: &[Calendar]) -> Colour {
    Colour::ALL
        .into_iter()
        .min_by_key(|c| {
            let used = calendars.iter().filter(|cal| cal.colour == *c).count();
            let order = Colour::ALL.iter().position(|x| x == c).unwrap_or(0);
            (used, order)
        })
        .unwrap_or(Colour::Blue)
}

/// The calendar an event with calendar id `id` is in: that calendar, else (a calendar that is
/// gone) the default one.
#[must_use]
pub fn calendar_of<'a>(calendars: &'a [Calendar], id: &str) -> Option<&'a Calendar> {
    calendars
        .iter()
        .find(|c| c.id == id)
        .or_else(|| calendars.iter().find(|c| c.is_default()))
}

/// The settings value listing the hidden calendars' ids (`default` for the default calendar),
/// sorted, comma-separated.
#[must_use]
pub fn hidden_value(hidden: &BTreeSet<String>) -> String {
    hidden
        .iter()
        .map(|id| {
            if id.is_empty() {
                DEFAULT_TOKEN
            } else {
                id.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The hidden calendars a settings value lists (what `hidden_value` wrote); ids that name no
/// calendar are left out.
#[must_use]
pub fn hidden_of(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .filter_map(|t| {
            if t == DEFAULT_TOKEN {
                Some(String::new())
            } else {
                event::is_event_id(t).then(|| t.to_string())
            }
        })
        .collect()
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
        let drive = azul_storage::LocalDrive::new(&dir.0);
        let put = |c: &Calendar| drive.put(&object_key(&c.id), to_json(c).as_bytes()).unwrap();
        assert_eq!(load(&drive), vec![Calendar::default_calendar()]);
        put(&work());
        let mut renamed = Calendar::default_calendar();
        renamed.name = String::from("Home");
        renamed.colour = Colour::Teal;
        put(&renamed);
        assert!(dir.0.join("calendars").join("default.json").is_file());
        assert_eq!(load(&drive), vec![renamed, work()]);
        drive.delete(&object_key("")).unwrap();
        drive.delete(&object_key(WORK)).unwrap();
        assert_eq!(load(&drive), vec![Calendar::default_calendar()]);
    }

    /// The start reads the calendars through the data folder's drive, wherever it keeps them.
    #[test]
    fn the_calendars_are_read_through_the_drive() {
        use azul_storage::{LocalDrive, ScopedDrive};
        let root = TempDir::create();
        let tree = LocalDrive::new(&root.0);
        let mut renamed = Calendar::default_calendar();
        renamed.name = String::from("Home");
        tree.put(&format!("calendar/{}", object_key(WORK)), to_json(&work()).as_bytes()).unwrap();
        tree.put(&format!("calendar/{}", object_key("")), to_json(&renamed).as_bytes()).unwrap();
        tree.put("calendar/calendars/33333333-2222-4333-8444-555555555555.json", b"{").unwrap();
        let drive = ScopedDrive::new(LocalDrive::new(&root.0), "calendar/", false).unwrap();
        assert_eq!(load(&drive), vec![renamed, work()]);
        let empty = ScopedDrive::new(LocalDrive::new(&root.0), "nothing-here/", false).unwrap();
        assert_eq!(load(&empty), vec![Calendar::default_calendar()]);
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

    /// `#rrggbb` as linear-light WCAG luminance.
    fn luminance(rgb: [f32; 3]) -> f32 {
        let lin = |v: f32| {
            let v = v / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])
    }

    fn rgb(hex: &str) -> [f32; 3] {
        let h = hex.trim_start_matches('#');
        let c = |i: usize| f32::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap());
        [c(0), c(2), c(4)]
    }

    /// The contrast of `ink` at `alpha` over `ground`, as a reader sees it.
    fn reads(ink: [f32; 3], alpha: f32, ground: [f32; 3]) -> f32 {
        let seen = [0, 1, 2].map(|i| ink[i] * alpha + ground[i] * (1.0 - alpha));
        let (a, b) = (luminance(seen), luminance(ground));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// An event's text is the desktop's ink (macOS: black at 85 %, secondary at 50 %; white at
    /// 85 % / 55 % in dark mode) on its calendar's tint, and the "Join meeting" button's label the
    /// button's own ink on its face over the tint: every tint keeps the main ink at 4.5:1 and the
    /// secondary at 3:1 in both modes, so no event, line or button on it fades out.
    #[test]
    fn every_calendar_colour_reads_with_the_desktops_ink_in_light_and_dark() {
        for c in Colour::ALL {
            let p = c.paint();
            let light = rgb(p.light_fill);
            let dark = rgb(p.dark_fill);
            let black = [0.0, 0.0, 0.0];
            let white = [255.0, 255.0, 255.0];
            assert!(
                reads(black, 0.85, light) >= 4.5,
                "{c:?}: text on the light tint"
            );
            assert!(
                reads(black, 0.50, light) >= 3.0,
                "{c:?}: secondary text on the light tint"
            );
            assert!(
                reads(white, 0.85, dark) >= 4.5,
                "{c:?}: text on the dark tint"
            );
            assert!(
                reads(white, 0.55, dark) >= 3.0,
                "{c:?}: secondary text on the dark tint"
            );
            // The swatch stands out from the window in both modes.
            assert!(
                reads(rgb(p.light_edge), 1.0, white) >= 3.0,
                "{c:?}: light swatch"
            );
            assert!(
                reads(rgb(p.dark_edge), 1.0, [30.0, 30.0, 30.0]) >= 3.0,
                "{c:?}: dark swatch"
            );
        }
    }
}
