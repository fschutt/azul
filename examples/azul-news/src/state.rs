//! What the user did with one feed's articles - read, starred, kept for later - by article id.
//!
//! One small file per feed, `news/feeds/<id>/state.json`, apart from the articles
//! (`items.json`, rewritten by every refresh): a refresh never touches the marks, and marking an
//! article read writes only this file. A read mark of an article that left the feed is
//! forgotten ([`ReadState::prune`]); a star or a "read later" is kept with its article (the
//! library keeps starred and saved articles when the feed drops them).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// The `format` of a state file.
pub const STATE_FORMAT: &str = "aznews.state";
/// The state file version this AzNews writes, and the newest it reads.
pub const STATE_VERSION: u64 = 1;

/// One feed's marks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadState {
    pub format: String,
    pub version: u64,
    pub read: BTreeSet<String>,
    pub starred: BTreeSet<String>,
    pub later: BTreeSet<String>,
}

impl ReadState {
    #[must_use]
    pub fn is_read(&self, _id: &str) -> bool {
        false
    }

    /// Marks the article read or unread; whether that changed anything.
    pub fn set_read(&mut self, _id: &str, _read: bool) -> bool {
        false
    }

    #[must_use]
    pub fn is_starred(&self, _id: &str) -> bool {
        false
    }

    /// Stars or unstars the article; its new state.
    pub fn toggle_starred(&mut self, _id: &str) -> bool {
        false
    }

    #[must_use]
    pub fn is_later(&self, _id: &str) -> bool {
        false
    }

    /// Keeps the article for later, or not; its new state.
    pub fn toggle_later(&mut self, _id: &str) -> bool {
        false
    }

    /// Marks every article of `ids` read; how many were unread.
    pub fn mark_all_read<'a>(&mut self, _ids: impl IntoIterator<Item = &'a str>) -> usize {
        0
    }

    /// Forgets the read marks of articles that are not in `present` (stars and "later" stay).
    pub fn prune(&mut self, _present: &BTreeSet<&str>) {}

    /// The file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        String::new()
    }

    /// The marks of a state file, and a problem when it could not be read (then nothing is
    /// marked).
    #[must_use]
    pub fn from_json(_text: &str) -> (ReadState, Option<String>) {
        (ReadState::default(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marking_read_starred_and_later_changes_only_that_article() {
        let mut s = ReadState::default();
        assert!(!s.is_read("a"));
        assert!(s.set_read("a", true), "a change");
        assert!(!s.set_read("a", true), "already read");
        assert!(s.is_read("a"));
        assert!(!s.is_read("b"));
        assert!(s.set_read("a", false));
        assert!(!s.is_read("a"));
        assert!(s.toggle_starred("b"));
        assert!(s.is_starred("b"));
        assert!(!s.toggle_starred("b"));
        assert!(!s.is_starred("b"));
        assert!(s.toggle_later("c"));
        assert!(s.is_later("c"));
        assert!(!s.is_starred("c"));
    }

    #[test]
    fn mark_all_read_counts_what_was_new() {
        let mut s = ReadState::default();
        s.set_read("a", true);
        assert_eq!(s.mark_all_read(["a", "b", "c"]), 2);
        assert!(s.is_read("b") && s.is_read("c"));
        assert_eq!(s.mark_all_read(["a", "b"]), 0);
    }

    #[test]
    fn the_state_file_round_trips() {
        let mut s = ReadState::default();
        s.set_read("https://example.org/?p=1", true);
        s.toggle_starred("tag:x,2026:2");
        s.toggle_later("3");
        let text = s.to_json();
        assert!(text.contains("\"format\": \"aznews.state\""), "{text}");
        assert!(text.contains("\"version\": 1"), "{text}");
        let (back, problem) = ReadState::from_json(&text);
        assert_eq!(problem, None);
        assert!(back.is_read("https://example.org/?p=1"));
        assert!(back.is_starred("tag:x,2026:2"));
        assert!(back.is_later("3"));
        assert_eq!(back.read, s.read);
    }

    #[test]
    fn a_broken_state_file_marks_nothing_and_says_so() {
        let (s, problem) = ReadState::from_json("{ \"read\": [1, 2");
        assert_eq!(s.read.len(), 0);
        assert!(problem.is_some());
        let (s, problem) = ReadState::from_json("{\"format\": \"aznews.state\", \"version\": 9, \"read\": [\"a\"]}");
        assert!(problem.is_some(), "a newer version is reported");
        assert!(s.is_read("a"), "what it understands is still read");
    }

    #[test]
    fn prune_forgets_read_marks_of_vanished_articles_but_keeps_stars() {
        let mut s = ReadState::default();
        s.set_read("gone", true);
        s.set_read("here", true);
        s.toggle_starred("gone-starred");
        s.toggle_later("gone-later");
        let present: BTreeSet<&str> = ["here"].into_iter().collect();
        s.prune(&present);
        assert!(!s.is_read("gone"));
        assert!(s.is_read("here"));
        assert!(s.is_starred("gone-starred"));
        assert!(s.is_later("gone-later"));
    }
}
