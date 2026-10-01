//! The history (persisted as `calculator/history.jsonl`) and the memory.
//!
//! One JSON object per line, oldest first, so the file appends naturally and
//! a damaged line costs that line only: `{"expr":"1,280 × 0.19","result":
//! "243.2","mode":"standard","at":1790000000}`. At most [`MAX_HISTORY`]
//! entries are kept (the oldest go first). The memory is not persisted
//! (as on Windows' calculator).

use serde::{Deserialize, Serialize};

/// The history file's name in the app's folder.
pub const HISTORY_FILE: &str = "history.jsonl";

/// Entries kept.
pub const MAX_HISTORY: usize = 200;

/// One calculation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The expression line as shown: `1,280 × 0.19`.
    pub expr: String,
    /// The result as shown: `243.2`.
    pub result: String,
    /// `standard`, `scientific`, `programmer`.
    #[serde(default)]
    pub mode: String,
    /// Seconds since 1970 (0 = unknown).
    #[serde(default)]
    pub at: u64,
}

/// The file's text: one line per entry, oldest first, newline-terminated.
#[must_use]
pub fn to_jsonl(entries: &[HistoryEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        if let Ok(line) = serde_json::to_string(e) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// Reads the file: the entries (oldest first, at most [`MAX_HISTORY`], the
/// newest kept) and how many lines could not be read.
#[must_use]
pub fn parse_jsonl(text: &str) -> (Vec<HistoryEntry>, usize) {
    let mut entries = Vec::new();
    let mut skipped = 0;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match serde_json::from_str::<HistoryEntry>(line) {
            Ok(e) => entries.push(e),
            Err(_) => skipped += 1,
        }
    }
    trim(&mut entries);
    (entries, skipped)
}

/// Drops the oldest entries beyond [`MAX_HISTORY`].
pub fn trim(entries: &mut Vec<HistoryEntry>) {
    if entries.len() > MAX_HISTORY {
        let extra = entries.len() - MAX_HISTORY;
        entries.drain(..extra);
    }
}

/// The memory: a stack of values (newest first) as decimal or integer
/// literals of the mode they were stored in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub items: Vec<String>,
}

impl Memory {
    /// MS: store a new value on top.
    pub fn store(&mut self, value: &str) {
        self.items.insert(0, value.to_string());
    }

    /// MR: the top value.
    #[must_use]
    pub fn recall(&self) -> Option<&str> {
        self.items.first().map(String::as_str)
    }

    /// MC: forget everything.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// M+ / M-: the top value replaced by `f(top)`; with nothing stored,
    /// `f(0)` is stored. `f` returns `None` when it cannot compute.
    pub fn update(&mut self, f: impl Fn(&str) -> Option<String>) -> bool {
        match self.items.first().cloned() {
            Some(top) => match f(&top) {
                Some(v) => {
                    self.items[0] = v;
                    true
                }
                None => false,
            },
            None => match f("0") {
                Some(v) => {
                    self.items.push(v);
                    true
                }
                None => false,
            },
        }
    }

    /// Removes the value at `index`.
    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(expr: &str, result: &str) -> HistoryEntry {
        HistoryEntry {
            expr: expr.to_string(),
            result: result.to_string(),
            mode: "standard".to_string(),
            at: 1_790_000_000,
        }
    }

    #[test]
    fn the_history_file_round_trips_one_entry_per_line() {
        let entries = vec![entry("1,280 \u{d7} 0.19", "243.2"), entry("243.2 + 18.5", "261.7")];
        let text = to_jsonl(&entries);
        assert_eq!(text.lines().count(), 2);
        assert!(text.ends_with('\n'));
        assert!(text.starts_with("{\"expr\":\"1,280 \u{d7} 0.19\""), "{text}");
        assert_eq!(parse_jsonl(&text), (entries, 0));
    }

    #[test]
    fn a_damaged_line_costs_only_that_line() {
        let text = format!(
            "{}\nnot json\n\n{}",
            serde_json::to_string(&entry("1 + 1", "2")).unwrap(),
            r#"{"expr":"2 + 2","result":"4"}"#
        );
        let (entries, skipped) = parse_jsonl(&text);
        assert_eq!(skipped, 1);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].mode, "", "missing fields default");
        assert_eq!(entries[1].at, 0);
    }

    #[test]
    fn only_the_newest_two_hundred_entries_are_kept() {
        let entries: Vec<HistoryEntry> = (0..250).map(|i| entry(&format!("{i} + 0"), &i.to_string())).collect();
        let (kept, _) = parse_jsonl(&to_jsonl(&entries));
        assert_eq!(kept.len(), MAX_HISTORY);
        assert_eq!(kept[0].result, "50");
        assert_eq!(kept.last().unwrap().result, "249");
    }

    #[test]
    fn memory_store_recall_add_and_clear() {
        let mut m = Memory::default();
        assert_eq!(m.recall(), None);
        let add = |n: i64| move |top: &str| top.parse::<i64>().ok().map(|t| (t + n).to_string());
        assert!(m.update(add(5)), "M+ on empty memory stores 0 + 5");
        assert_eq!(m.recall(), Some("5"));
        m.store("10");
        assert_eq!(m.items, vec!["10", "5"]);
        assert!(m.update(add(-3)));
        assert_eq!(m.recall(), Some("7"));
        m.remove(1);
        assert_eq!(m.items, vec!["7"]);
        m.clear();
        assert!(m.items.is_empty());
        assert!(!Memory { items: vec!["x".into()] }.update(add(1)));
    }
}
