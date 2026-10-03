//! AzMeet's files in the Azlin data tree (azul-appkit's data root: `--data-dir`, `$AZLIN_DATA`,
//! else `<data dir>/Azlin`), keyed as the user's S3 bucket will be (user ruling: meeting data =
//! files in a per-meeting folder):
//!
//! - `meet/settings.json`: azul-appkit's settings file - the app theme and mode, and AzMeet's
//!   values: the meeting server (`server`), the name others see (`name`), the video quality
//!   (`quality`);
//! - `meet/<meeting>/meeting.json`: the meeting - its key, link, meeting server, when this side
//!   joined, who was there;
//! - `meet/<meeting>/chat.jsonl`: the call's chat, one JSON object per line.
//!
//! The settings are read once at start (before the window exists); every write runs on an azul
//! Thread through azul-storage's `LocalDrive` (an `S3Drive` later), never in a callback.

use serde::Serialize;

use crate::chat::ChatMessage;

/// AzMeet's folder in the data tree.
pub const APP_FOLDER: &str = "meet";
/// The settings values AzMeet keeps besides the theme and the mode.
pub const SERVER: &str = "server";
pub const NAME: &str = "name";
pub const QUALITY: &str = "quality";
/// The longest meeting folder name.
const MAX_FOLDER: usize = 64;

/// `meet/settings.json`.
#[must_use]
pub fn settings_key() -> String {
    azul_appkit::data::app_key(APP_FOLDER, azul_appkit::settings::SETTINGS_FILE)
}

/// The folder of meeting `meeting` (its room key, or the demo's code): `meet/<meeting>/`, every
/// character but a letter, a digit, `-` and `_` an underscore, at most 64 of them; `None` for a
/// meeting without a name.
#[must_use]
pub fn meeting_folder(meeting: &str) -> Option<String> {
    let meeting = meeting.trim();
    if meeting.is_empty() {
        return None;
    }
    let safe: String = meeting
        .chars()
        .take(MAX_FOLDER)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Some(azul_appkit::data::app_prefix(&format!("{APP_FOLDER}/{safe}")))
}

/// `meet/<meeting>/chat.jsonl`.
#[must_use]
pub fn chat_key(meeting: &str) -> Option<String> {
    meeting_folder(meeting).map(|folder| format!("{folder}chat.jsonl"))
}

/// `meet/<meeting>/meeting.json`.
#[must_use]
pub fn meeting_key(meeting: &str) -> Option<String> {
    meeting_folder(meeting).map(|folder| format!("{folder}meeting.json"))
}

/// One line of `chat.jsonl`.
#[derive(Serialize)]
struct ChatLine<'a> {
    name: &'a str,
    text: &'a str,
    mine: bool,
}

/// The chat as `chat.jsonl`: one `{"name", "text", "mine"}` object per message, oldest first,
/// every line ending in a newline.
#[must_use]
pub fn chat_lines(messages: &[ChatMessage]) -> String {
    let mut out = String::new();
    for m in messages {
        let line = ChatLine {
            name: &m.name,
            text: &m.text,
            mine: m.mine,
        };
        if let Ok(json) = serde_json::to_string(&line) {
            out.push_str(&json);
            out.push('\n');
        }
    }
    out
}

/// What `meeting.json` says about a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MeetingRecord {
    /// The room key (or the demo's code).
    pub meeting: String,
    /// The link others join with ("" in the demo).
    pub link: String,
    /// The meeting server ("" in the demo).
    pub server: String,
    /// When this side joined, seconds since 1970.
    pub joined: u64,
    /// Everyone this side met in the call, this side first.
    pub people: Vec<String>,
}

impl MeetingRecord {
    /// The record as pretty JSON with a trailing newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"));
        text.push('\n');
        text
    }

    /// Adds `name` to the people met, once.
    pub fn met(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.people.iter().any(|p| p == name) {
            return false;
        }
        self.people.push(name.to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(name: &str, text: &str, mine: bool) -> ChatMessage {
        ChatMessage {
            from: 1,
            mine,
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn the_settings_and_a_meeting_live_in_the_meet_folder() {
        assert_eq!(settings_key(), "meet/settings.json");
        assert_eq!(meeting_folder("abc-defg-hij").as_deref(), Some("meet/abc-defg-hij/"));
        assert_eq!(chat_key("abc-defg-hij").as_deref(), Some("meet/abc-defg-hij/chat.jsonl"));
        assert_eq!(
            meeting_key("abc-defg-hij").as_deref(),
            Some("meet/abc-defg-hij/meeting.json")
        );
    }

    #[test]
    fn a_meeting_folder_is_one_safe_segment_or_none() {
        assert_eq!(meeting_folder("../../etc").as_deref(), Some("meet/______etc/"));
        assert_eq!(meeting_folder("a b/c").as_deref(), Some("meet/a_b_c/"));
        assert_eq!(meeting_folder("   "), None);
        assert_eq!(meeting_folder(""), None);
        let long = "x".repeat(200);
        assert_eq!(meeting_folder(&long).map(|f| f.len()), Some("meet/".len() + 64 + 1));
    }

    #[test]
    fn the_chat_is_one_json_object_per_line_oldest_first() {
        let lines = chat_lines(&[
            message("Ada", "Hello \"Ben\"", true),
            message("Ben", "line one\nline two", false),
        ]);
        let parsed: Vec<serde_json::Value> = lines
            .lines()
            .map(|l| serde_json::from_str(l).expect("a JSON object per line"))
            .collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["name"], "Ada");
        assert_eq!(parsed[0]["text"], "Hello \"Ben\"");
        assert_eq!(parsed[0]["mine"], true);
        assert_eq!(parsed[1]["text"], "line one\nline two", "a newline stays inside its line");
        assert!(lines.ends_with('\n'));
        assert_eq!(chat_lines(&[]), "");
    }

    #[test]
    fn a_meeting_record_lists_everyone_met_once() {
        let mut record = MeetingRecord {
            meeting: String::from("abc-defg-hij"),
            people: vec![String::from("Ada")],
            ..MeetingRecord::default()
        };
        assert!(record.met("Ben"));
        assert!(!record.met("Ben"), "once");
        assert!(!record.met("Ada"));
        assert_eq!(record.people, vec!["Ada", "Ben"]);
        let json: serde_json::Value = serde_json::from_str(&record.to_json()).unwrap();
        assert_eq!(json["meeting"], "abc-defg-hij");
        assert_eq!(json["people"][1], "Ben");
    }
}
