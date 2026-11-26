//! The chips of the bottom bar - one per conversation of the open file - and the colour a file gets from its chips.
//!
//! A conversation is anchored where the pen rested or circled (a page and a line); an agent answers it in the
//! background. Its chip is a blue spinner while the agent works, yellow when it has suggested something that waits
//! to be accepted, green when it is done. A file is as unresolved as its most unresolved conversation: the tree shows
//! the spinner for a file with an agent at work, yellow for one with a suggestion waiting, green for one whose
//! conversations are all done, and no colour for a file nobody talked about.
//!
//! This is the model, with no window and no agent: the agent pool's messages set the states (`set_state`) and the DOM
//! reads the chips. The mock conversations (`mock`) stand in until the real backends answer.

/// Where a conversation stands. The order is how much a file is to be looked at: what works outranks what waits
/// outranks what is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Done,
    Suggested,
    Working,
}

/// One conversation as the bar shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub id: u64,
    /// The file, as the tree names it (its path under the folder).
    pub file: String,
    /// Where it is anchored.
    pub page: usize,
    pub line: u32,
    pub state: State,
    /// The first words of what was said (`short_title`).
    pub title: String,
}

/// How many conversations of a file are in each state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub working: usize,
    pub suggested: usize,
    pub done: usize,
}

/// What `push` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pushed {
    Kept,
    /// The cap was reached: the oldest done conversation made room (what was said, for a log).
    DroppedOldest(String),
    /// The cap was reached and none is done: the new one is not kept (what was said, for a log).
    Refused(String),
}

/// How many conversations the bar keeps: more than a person reads in a session, and a bound on the memory.
pub const MAX_CHIPS: usize = 200;

/// The conversations of the whole folder, oldest first.
#[derive(Debug, Clone, Default)]
pub struct Chips {
    chips: Vec<Chip>,
}

impl Chips {
    /// Keeps a conversation; past `MAX_CHIPS` the oldest done one makes room, and with none done the new one is
    /// refused: the number never grows without bound.
    pub fn push(&mut self, chip: Chip) -> Pushed {
        if self.chips.len() >= MAX_CHIPS {
            if let Some(at) = self.chips.iter().position(|c| c.state == State::Done) {
                let gone = self.chips.remove(at);
                self.chips.push(chip);
                return Pushed::DroppedOldest(format!(
                    "the conversation \"{}\" of {} left the bar: it keeps at most {MAX_CHIPS}",
                    gone.title, gone.file
                ));
            }
            return Pushed::Refused(format!(
                "the conversation \"{}\" of {} is not shown: {MAX_CHIPS} are at work or waiting",
                chip.title, chip.file
            ));
        }
        self.chips.push(chip);
        Pushed::Kept
    }

    /// Sets the state of conversation `id`; whether there is such a one.
    pub fn set_state(&mut self, id: u64, state: State) -> bool {
        match self.chips.iter_mut().find(|c| c.id == id) {
            Some(chip) => {
                chip.state = state;
                true
            }
            None => false,
        }
    }

    /// The conversations of `file`, in the order they began (the bar of the open file).
    #[must_use]
    pub fn of_file(&self, file: &str) -> Vec<&Chip> {
        self.chips.iter().filter(|c| c.file == file).collect()
    }

    /// The colour of a file: the most unresolved state of its conversations (`None`: nobody talked about it).
    #[must_use]
    pub fn file_state(&self, file: &str) -> Option<State> {
        self.chips.iter().filter(|c| c.file == file).map(|c| c.state).max()
    }

    /// How many conversations of `file` are in each state.
    #[must_use]
    pub fn counts(&self, file: &str) -> Counts {
        let mut counts = Counts::default();
        for chip in self.chips.iter().filter(|c| c.file == file) {
            match chip.state {
                State::Working => counts.working += 1,
                State::Suggested => counts.suggested += 1,
                State::Done => counts.done += 1,
            }
        }
        counts
    }

    /// How many conversations there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.chips.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chips.is_empty()
    }
}

/// The first words of what was said as a chip's title: whitespace folded to single spaces, at most `max_chars`
/// characters (not bytes), with an ellipsis where it was cut.
#[must_use]
pub fn short_title(text: &str, max_chars: usize) -> String {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.chars().count() <= max_chars {
        return folded;
    }
    let cut: String = folded.chars().take(max_chars).collect();
    // a cut at a space would leave it before the ellipsis
    format!("{}…", cut.trim_end())
}

/// Phrases the mock conversations say, in turn.
static MOCK_WORDS: [&str; 6] = [
    "why does this loop stop",
    "rename this to what it holds",
    "is this lock held across the await",
    "split this into two functions",
    "the bound looks off by one",
    "what happens when the list is empty",
];

/// The states the mock conversations of a file go through, in turn: every state, working and waiting twice.
static MOCK_STATES: [State; 5] = [State::Working, State::Suggested, State::Done, State::Done, State::Suggested];

/// `count` mock conversations spread over `files`, each file going through every state (until the real backends
/// answer, and for the screenshots of a bar with none, one and many chips).
#[must_use]
pub fn mock(count: usize, files: &[String]) -> Chips {
    let mut chips = Chips::default();
    if files.is_empty() {
        return chips;
    }
    for i in 0..count {
        let file = &files[i % files.len()];
        let turn = i / files.len();
        chips.push(Chip {
            id: i as u64 + 1,
            file: file.clone(),
            page: i % 3,
            line: u32::try_from(i * 7 % 40).unwrap_or(0) + 1,
            state: MOCK_STATES[turn % MOCK_STATES.len()],
            title: short_title(MOCK_WORDS[i % MOCK_WORDS.len()], 28),
        });
    }
    chips
}

#[cfg(test)]
#[path = "chips_test.rs"]
mod chips_test;
