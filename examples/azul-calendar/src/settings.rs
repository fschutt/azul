//! AzCalendar's settings file, `<data dir>/settings.txt`: one `key=value` line per setting, in
//! AzMeet's settings format (the `meeting_server=` line is AzMeet's, read and written through
//! `meeting`). Saving one setting keeps every other line, so the meeting server and the week's
//! zoom (`week_hour_px=`) do not overwrite each other, and a line a newer version wrote survives.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{meet_rooms, meeting, test_dir::TempDir, week};

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
        let dir = TempDir::create();
        let file = path(&dir.0);
        assert_eq!(file, dir.0.join("settings.txt"));
        assert_eq!(read_text(&file), None, "no file yet");
        write_line(&file, &hour_px_line(72.0)).unwrap();
        meeting::save_server(&file, "https://meet.example.com").unwrap();
        write_line(&file, &hour_px_line(96.0)).unwrap();
        let text = read_text(&file).unwrap();
        assert_eq!(hour_px(&text), Some(96.0));
        assert_eq!(
            meet_rooms::decode_settings(&text).as_deref(),
            Some("https://meet.example.com")
        );
        assert_eq!(
            meeting::read_saved_server(&file).as_deref(),
            Some("https://meet.example.com")
        );
        // Written next to the file and renamed over it: nothing else is left in the folder.
        let names: Vec<_> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }

    /// A file longer than AzMeet reads a settings file is not read, and the next setting saved
    /// replaces it.
    #[test]
    fn a_settings_file_too_long_is_not_read_and_the_next_save_replaces_it() {
        let dir = TempDir::create();
        let file = path(&dir.0);
        std::fs::write(&file, "x".repeat(meet_rooms::MAX_SETTINGS_BYTES + 10)).unwrap();
        assert_eq!(read_text(&file), None);
        write_line(&file, &hour_px_line(60.0)).unwrap();
        assert_eq!(read_text(&file).as_deref(), Some("week_hour_px=60.0\n"));
    }

    /// The data folder is made when the first setting is saved.
    #[test]
    fn saving_a_setting_makes_the_data_folder() {
        let dir = TempDir::create();
        let file = path(&dir.0.join("new").join("AzCalendar"));
        write_line(&file, &hour_px_line(48.0)).unwrap();
        assert_eq!(hour_px(&read_text(&file).unwrap()), Some(48.0));
    }
}
