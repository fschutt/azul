//! AzCalendar's settings file, `settings.txt` in the data folder's drive: one `key=value` line
//! per setting (the meeting server is the `meeting_server=` line, the name AzMeet's settings file
//! used). Saving one setting keeps every other line, so the meeting server and the week's zoom
//! (`week_hour_px=`) do not overwrite each other, and a line a newer version wrote survives. The
//! start reads the file through the drive (`read`); a change is queued as the whole file
//! (`CalState::save_setting`, `writes.rs`).

use azul_storage::Drive;

use crate::{meet_rooms, week};

/// The settings file's name in the data folder.
pub const FILE_NAME: &str = "settings.txt";
/// The longest settings file read: a longer one is no file of AzCalendar's, and the next setting
/// saved replaces it.
pub const MAX_BYTES: usize = 4096;
/// The meeting server's line.
pub const MEETING_SERVER_KEY: &str = "meeting_server=";
/// The week's zoom: the height of an hour, in px.
const HOUR_PX_KEY: &str = "week_hour_px=";

/// The settings file's text as the data folder's drive keeps it (`settings.txt`); `None` without
/// a file, or with one longer than a settings file is read.
pub fn read(drive: &dyn Drive) -> Option<String> {
    let bytes = drive.get(FILE_NAME).ok()?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// The key of a `key=value` line: up to and with its `=` (the whole trimmed line without one).
fn key_of(line: &str) -> &str {
    let line = line.trim();
    line.find('=').map_or(line, |i| &line[..=i])
}

/// `text` with `line` (a `key=value` setting) in place of every line with the same key, at the
/// end; every other line is kept as it is. Every line ends in a newline.
pub fn with_line(text: &str, line: &str) -> String {
    let line = line.trim();
    let key = key_of(line);
    let mut out: String = text
        .lines()
        .filter(|l| key_of(l) != key)
        .flat_map(|l| [l, "\n"])
        .collect();
    out.push_str(line);
    out.push('\n');
    out
}

/// The settings line keeping the week's zoom, `hour_px` px an hour (held to the limits).
pub fn hour_px_line(hour_px: f32) -> String {
    format!("{HOUR_PX_KEY}{:.1}\n", week::clamp_hour_px(hour_px))
}

/// The week's zoom the settings file's text keeps (its last `week_hour_px=` line), held to the
/// limits; `None` without one, or when its value is no finite number.
pub fn hour_px(text: &str) -> Option<f32> {
    let px = value(text, HOUR_PX_KEY)?
        .parse::<f32>()
        .ok()
        .filter(|px| px.is_finite())?;
    Some(week::clamp_hour_px(px))
}

/// The calendar view shown last (`views::ViewKind::name`).
pub const VIEW_KEY: &str = "view=";
/// The calendars "My calendars" hides (`calendars::hidden_value`).
pub const HIDDEN_CALENDARS_KEY: &str = "hidden_calendars=";
/// Whether the To-Do bar is shown (`1` / `0`).
pub const TODO_BAR_KEY: &str = "todo_bar=";
/// Whether the navigation pane is folded to its strip (`1` / `0`).
pub const NAVIGATION_FOLDED_KEY: &str = "navigation_folded=";
/// The language of the words (`system`, `en`, `de`: appkit's `LanguagePref::name`).
pub const LANGUAGE_KEY: &str = "language=";

/// The language the settings file keeps; the system's without one (or an unknown one).
#[must_use]
pub fn language(text: &str) -> azul_appkit::args::LanguagePref {
    value(text, LANGUAGE_KEY)
        .and_then(azul_appkit::args::LanguagePref::parse)
        .unwrap_or(azul_appkit::args::LanguagePref::System)
}

/// The settings line keeping `language`.
#[must_use]
pub fn language_line(language: azul_appkit::args::LanguagePref) -> String {
    line(LANGUAGE_KEY, language.name())
}

/// The value of the settings file's last line with `key` (the key with its `=`), trimmed;
/// `None` without one.
pub fn value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(key))
        .last()
        .map(str::trim)
}

/// The setting line `key` (with its `=`) `value`.
pub fn line(key: &str, value: &str) -> String {
    format!("{key}{}\n", value.trim())
}

/// The settings line keeping the meeting server `server`.
pub fn meeting_server_line(server: &str) -> String {
    line(MEETING_SERVER_KEY, server)
}

/// The meeting server the settings file's text keeps (its last `meeting_server=` line), when it
/// is a meeting server's address (`meet_rooms::normalize_server`).
pub fn meeting_server(text: &str) -> Option<String> {
    value(text, MEETING_SERVER_KEY).and_then(meet_rooms::normalize_server)
}

/// A `1` / `0` setting: `Some(true)` for `1`, `Some(false)` for `0`, `None` for anything else.
pub fn flag(text: &str, key: &str) -> Option<bool> {
    match value(text, key)? {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_dir::TempDir, week};

    #[test]
    fn a_setting_replaces_its_own_lines_and_keeps_every_other_line() {
        let text = "meeting_server=https://meet.example.com\nweek_hour_px=48.0\nfuture=1\n";
        assert_eq!(
            with_line(text, "week_hour_px=96.0\n"),
            "meeting_server=https://meet.example.com\nfuture=1\nweek_hour_px=96.0\n"
        );
        assert_eq!(with_line("", "week_hour_px=96.0\n"), "week_hour_px=96.0\n");
        // A file without a last newline, a line given without one, and the key twice.
        assert_eq!(with_line("a=1", "b=2"), "a=1\nb=2\n");
        assert_eq!(with_line("b=1\na=1\n b=3\n", "b=2\n"), "a=1\nb=2\n");
        // Only the whole key counts: `b=` is not `bb=`.
        assert_eq!(with_line("bb=1\n", "b=2\n"), "bb=1\nb=2\n");
    }

    #[test]
    fn the_zoom_is_read_back_held_to_the_hour_limits_and_a_broken_value_is_ignored() {
        for px in [
            week::MIN_HOUR_PX,
            week::DEFAULT_HOUR_PX,
            96.0,
            137.5,
            week::MAX_HOUR_PX,
        ] {
            assert_eq!(hour_px(&hour_px_line(px)), Some(px), "{px}");
        }
        assert_eq!(hour_px_line(1000.0), hour_px_line(week::MAX_HOUR_PX));
        assert_eq!(hour_px("week_hour_px=1000\n"), Some(week::MAX_HOUR_PX));
        assert_eq!(hour_px("week_hour_px=1\n"), Some(week::MIN_HOUR_PX));
        assert_eq!(hour_px(" week_hour_px=72 \n"), Some(72.0));
        assert_eq!(
            hour_px("week_hour_px=30\nweek_hour_px=60\n"),
            Some(60.0),
            "the last line counts"
        );
        for broken in [
            "",
            "meeting_server=http://127.0.0.1:8787\n",
            "week_hour_px=\n",
            "week_hour_px=wide\n",
            "week_hour_px=NaN\n",
            "week_hour_px=inf\n",
        ] {
            assert_eq!(hour_px(broken), None, "{broken:?}");
        }
    }

    #[test]
    fn the_zoom_and_the_meeting_server_share_the_file_without_losing_each_other() {
        let text = with_line("", &hour_px_line(72.0));
        let text = with_line(&text, &meeting_server_line("https://meet.example.com/"));
        let text = with_line(&text, &hour_px_line(96.0));
        assert_eq!(hour_px(&text), Some(96.0));
        assert_eq!(meeting_server(&text).as_deref(), Some("https://meet.example.com"));
        let text = with_line(&text, &meeting_server_line("http://127.0.0.1:8787"));
        assert_eq!(meeting_server(&text).as_deref(), Some("http://127.0.0.1:8787"));
        assert_eq!(meeting_server("meeting_server=javascript:alert(1)\n"), None);
        assert_eq!(meeting_server("not a setting"), None);
    }

    /// The start reads the settings through the data folder's drive, wherever it keeps them; a
    /// file too long to be AzCalendar's is not read (the next setting saved replaces it).
    #[test]
    fn the_settings_are_read_through_the_drive() {
        use azul_storage::{LocalDrive, ScopedDrive};
        let root = TempDir::create();
        let tree = LocalDrive::new(&root.0);
        let drive = || ScopedDrive::new(LocalDrive::new(&root.0), "calendar/", false).unwrap();
        assert_eq!(read(&drive()), None, "no file yet");
        tree.put("calendar/settings.txt", b"view=week\nweek_hour_px=60.0\n").unwrap();
        assert_eq!(read(&drive()).as_deref(), Some("view=week\nweek_hour_px=60.0\n"));
        let long = "x".repeat(MAX_BYTES + 1);
        tree.put("calendar/settings.txt", long.as_bytes()).unwrap();
        assert_eq!(read(&drive()), None, "a file too long is not read");
    }

    #[test]
    fn a_settings_value_is_its_last_line_and_a_flag_is_one_or_zero() {
        let text = "view=month\ntodo_bar=1\nview= week \nnavigation_folded=yes\n";
        assert_eq!(value(text, VIEW_KEY), Some("week"));
        assert_eq!(value(text, HIDDEN_CALENDARS_KEY), None);
        assert_eq!(flag(text, TODO_BAR_KEY), Some(true));
        assert_eq!(flag(text, NAVIGATION_FOLDED_KEY), None);
        assert_eq!(flag(&line(TODO_BAR_KEY, "0"), TODO_BAR_KEY), Some(false));
        assert_eq!(line(VIEW_KEY, " day "), "view=day\n");
    }
}
