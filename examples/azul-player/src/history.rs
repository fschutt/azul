//! What was watched: the recent files and where each was left, so a file opens where the user
//! stopped. ONE file in the data tree, `player/history.json` (the S3 split). Plain Rust, tested
//! without a window.

use serde::{Deserialize, Serialize};

/// The history's file, in the app's folder of the data tree.
pub const HISTORY_FILE: &str = "history.json";
/// The most files the history keeps.
pub const MAX_ENTRIES: usize = 20;
/// A file watched to this fraction (or closer than [`END_MARGIN_S`] to its end) starts over.
pub const FINISHED_FRACTION: f64 = 0.95;
pub const END_MARGIN_S: f64 = 10.0;
/// A position this early is not worth resuming.
pub const MIN_RESUME_S: f64 = 5.0;

/// One file watched.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub path: String,
    /// Where it was left, seconds.
    pub position_s: f64,
    /// Its length, seconds (0 = not known yet).
    pub duration_s: f64,
    /// When it was last opened (seconds since 1970).
    pub opened: u64,
}

impl Entry {
    /// The file's name without its folder and extension.
    #[must_use]
    pub fn title(&self) -> String {
        std::path::Path::new(&self.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    }

    /// How far it was watched, `0.0..=1.0` (0 when the length is not known).
    #[must_use]
    pub fn progress(&self) -> f64 {
        if self.duration_s > 0.0 && self.position_s.is_finite() {
            (self.position_s / self.duration_s).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// The history file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    /// Newest first.
    pub entries: Vec<Entry>,
}

impl History {
    /// The history from its file's text, or why not.
    pub fn from_json(text: &str) -> Result<History, String> {
        serde_json::from_str(text).map_err(|e| format!("the history file does not read: {e}"))
    }

    /// The history as its file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// `path` was opened at `now`: it moves to the front (added if new); the oldest beyond
    /// [`MAX_ENTRIES`] are forgotten.
    pub fn touch(&mut self, path: &str, now: u64) {
        let mut entry = match self.entries.iter().position(|e| e.path == path) {
            Some(i) => self.entries.remove(i),
            None => Entry {
                path: path.to_string(),
                ..Entry::default()
            },
        };
        entry.opened = now;
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_ENTRIES);
    }

    /// Where `path` was left and how long it is.
    pub fn set_position(&mut self, path: &str, position_s: f64, duration_s: f64) {
        let index = match self.entries.iter().position(|e| e.path == path) {
            Some(i) => i,
            None => {
                self.entries.push(Entry {
                    path: path.to_string(),
                    ..Entry::default()
                });
                self.entries.len() - 1
            }
        };
        let e = &mut self.entries[index];
        e.position_s = position_s;
        if duration_s > 0.0 {
            e.duration_s = duration_s;
        }
    }

    /// Where `path` should start: where it was left, or 0 when it was hardly started or
    /// (nearly) finished.
    #[must_use]
    pub fn resume_at(&self, path: &str) -> f64 {
        let Some(e) = self.entries.iter().find(|e| e.path == path) else {
            return 0.0;
        };
        let p = e.position_s;
        if !p.is_finite() || p < MIN_RESUME_S {
            return 0.0;
        }
        if e.duration_s > 0.0
            && (p >= e.duration_s * FINISHED_FRACTION || e.duration_s - p < END_MARGIN_S)
        {
            return 0.0;
        }
        p
    }

    /// Forgets `path`.
    pub fn remove(&mut self, path: &str) {
        self.entries.retain(|e| e.path != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_opened_file_goes_to_the_front_and_the_oldest_are_forgotten() {
        let mut h = History::default();
        h.touch("/v/a.mp4", 1);
        h.touch("/v/b.mp4", 2);
        h.touch("/v/a.mp4", 3);
        let order: Vec<&str> = h.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(order, vec!["/v/a.mp4", "/v/b.mp4"]);
        assert_eq!(h.entries[0].opened, 3);
        for i in 0..30 {
            h.touch(&format!("/v/{i}.mp4"), 10 + i);
        }
        assert_eq!(h.entries.len(), MAX_ENTRIES);
        assert_eq!(h.entries[0].path, "/v/29.mp4");
    }

    #[test]
    fn a_file_resumes_where_it_was_left_unless_hardly_started_or_finished() {
        let mut h = History::default();
        h.touch("/v/lecture.mp4", 1);
        assert_eq!(h.resume_at("/v/lecture.mp4"), 0.0, "never played");
        h.set_position("/v/lecture.mp4", 1910.0, 3483.0);
        assert_eq!(h.resume_at("/v/lecture.mp4"), 1910.0);
        h.set_position("/v/lecture.mp4", 3.0, 3483.0);
        assert_eq!(h.resume_at("/v/lecture.mp4"), 0.0, "hardly started");
        h.set_position("/v/lecture.mp4", 3400.0, 3483.0);
        assert_eq!(h.resume_at("/v/lecture.mp4"), 0.0, "finished: start over");
        h.set_position("/v/short.mp4", 20.0, 25.0);
        assert_eq!(
            h.resume_at("/v/short.mp4"),
            0.0,
            "within the end margin; unknown files are added"
        );
        assert_eq!(h.resume_at("/v/never.mp4"), 0.0);
    }

    #[test]
    fn an_entry_says_its_title_and_progress() {
        let e = Entry {
            path: "/home/u/Videos/holiday-2025.mkv".into(),
            position_s: 1800.0,
            duration_s: 3600.0,
            opened: 0,
        };
        assert_eq!(e.title(), "holiday-2025");
        assert!((e.progress() - 0.5).abs() < 1e-9);
        assert_eq!(Entry::default().progress(), 0.0);
    }

    #[test]
    fn the_history_file_round_trips_and_forgets_on_request() {
        let mut h = History::default();
        h.touch("/v/a.mp4", 1);
        h.set_position("/v/a.mp4", 12.5, 100.0);
        let back = History::from_json(&h.to_json()).expect("reads");
        assert_eq!(back, h);
        assert!(History::from_json("nope").is_err());
        h.remove("/v/a.mp4");
        assert!(h.entries.is_empty());
    }
}
