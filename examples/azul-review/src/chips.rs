//! The chips of the bottom bar (stub: the tests come first).

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Working,
    Suggested,
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub id: u64,
    pub file: String,
    pub page: usize,
    pub line: u32,
    pub state: State,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub working: usize,
    pub suggested: usize,
    pub done: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pushed {
    Kept,
    DroppedOldest(String),
    Refused(String),
}

pub const MAX_CHIPS: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct Chips {
    chips: Vec<Chip>,
}

impl Chips {
    pub fn push(&mut self, _chip: Chip) -> Pushed {
        Pushed::Kept
    }
    pub fn set_state(&mut self, _id: u64, _state: State) -> bool {
        true
    }
    #[must_use]
    pub fn of_file(&self, _file: &str) -> Vec<&Chip> {
        self.chips.iter().collect()
    }
    #[must_use]
    pub fn file_state(&self, _file: &str) -> Option<State> {
        Some(State::Done)
    }
    #[must_use]
    pub fn counts(&self, _file: &str) -> Counts {
        Counts { working: 1, suggested: 1, done: 1 }
    }
    #[must_use]
    pub fn len(&self) -> usize {
        0
    }
}

#[must_use]
pub fn short_title(text: &str, _max_chars: usize) -> String {
    text.to_string()
}

#[must_use]
pub fn mock(_count: usize, _files: &[String]) -> Chips {
    Chips::default()
}

#[cfg(test)]
#[path = "chips_test.rs"]
mod chips_test;
