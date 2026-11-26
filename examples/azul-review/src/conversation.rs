//! A conversation: what the reviewer said at a spot of a page and what the agent made of it.
//!
//! The note the pen's rest began (its page, its anchor, the words dictated into it) is the FIRST MESSAGE of the
//! conversation; everything after it - the agent's answers, the reviewer's follow-ups - are the turns kept here, with
//! the utterances the dictation heard (their words and times: what a request ties to the strokes drawn while they were
//! said), where the conversation stands ([`Phase`]) and what an agent that failed said.
//!
//! This is the model, with no window and no agent: the window's glue (`agent_window.rs`) calls [`Conversation::asking`]
//! when it sends a request, [`Conversation::receive`] or [`Conversation::fail`] when the worker answers, and the DOM
//! reads [`Conversation::phase`]. Everything is bounded (`MAX_TURNS`, `MAX_TURN_BYTES`, `MAX_SAID`): a cut says so in the
//! word it returns, and never panics.

use crate::{agent_review::Reply, chips};

pub use crate::dictation::Said;

/// How many turns a conversation keeps: more than a reviewer reads, and a bound on the memory and on the archive.
pub const MAX_TURNS: usize = 200;
/// How long one turn may be: an agent's answer is a few paragraphs; more is cut.
pub const MAX_TURN_BYTES: usize = 16 * 1024;
/// How many utterances of the dictation are kept.
pub const MAX_SAID: usize = 60;

/// Who said a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Reviewer,
    Agent,
}

impl Who {
    /// The word the archive keeps.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Reviewer => "reviewer",
            Self::Agent => "agent",
        }
    }

    /// The speaker a name stands for.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "reviewer" => Some(Self::Reviewer),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// One thing said after the first message.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub who: Who,
    pub text: String,
    /// When it was said, Unix ms.
    pub at_unix_ms: u64,
}

/// Where a conversation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The reviewer's words are there and nothing waits on anybody: a dictation that is over before it was asked, an
    /// agent that chose to do nothing, a follow-up typed and not sent.
    Draft,
    /// The agent works on it.
    Working,
    /// The agent proposed edits that wait for the reviewer.
    Suggested,
    /// Finished: the agent answered, every suggestion was taken or refused, or the reviewer closed it.
    Done,
    /// The agent could not answer (the reason is [`Conversation::problem`]).
    Failed,
}

impl Phase {
    pub const ALL: [Self; 5] = [Self::Draft, Self::Working, Self::Suggested, Self::Done, Self::Failed];

    /// The word the archive and the scripts keep.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Working => "working",
            Self::Suggested => "suggested",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    /// The phase a name stands for.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// What an answer did to the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    /// The agent chose to do nothing: nothing is shown, the reviewer's words stay.
    Quiet,
    /// Words came back (and nothing to take).
    Answered,
    /// Words and this many suggestions that wait for the reviewer.
    Suggested(usize),
}

/// The part of a conversation after its first message.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Conversation {
    pub phase: Phase,
    /// The agent's answers and the reviewer's follow-ups, oldest first.
    pub turns: Vec<Turn>,
    /// What the dictation heard, utterance by utterance.
    pub said: Vec<Said>,
    /// What each utterance was understood as where the index of the workspace read its words as names (none where they
    /// were taken as heard): one for each utterance of `said`, never in place of it.
    pub understood: Vec<Option<String>>,
    /// What the agent understood the first message to say, where it had to correct the transcript (the raw words are
    /// never touched).
    pub heard_as: Option<String>,
    /// Why the agent could not answer (while the phase is `Failed`).
    pub problem: Option<String>,
    /// How many times an agent was asked.
    pub asked: u32,
    /// How many suggestions wait for the reviewer.
    pub pending: usize,
    /// The follow-up the reviewer is typing (not yet a turn; not kept in the archive).
    pub draft: String,
}

impl Default for Phase {
    fn default() -> Self {
        Self::Draft
    }
}

/// `text` as at most `max` bytes, cut at a letter; whether it was cut.
fn cut(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

impl Conversation {
    /// Whether an agent may be asked now (not while one works).
    #[must_use]
    pub fn can_ask(&self) -> bool {
        self.phase != Phase::Working
    }

    /// An agent is being asked: the conversation works. `false` (and nothing changes) while one already does.
    pub fn asking(&mut self) -> bool {
        if !self.can_ask() {
            return false;
        }
        self.phase = Phase::Working;
        self.asked += 1;
        self.problem = None;
        true
    }

    /// The agent answered. A reply that chose to do nothing ends the conversation quietly (nothing is shown, the
    /// reviewer's words stay); words are an agent's turn; suggestions wait for the reviewer.
    pub fn receive(&mut self, reply: &Reply, now_unix_ms: u64) -> Landed {
        self.problem = None;
        if reply.is_no_action() {
            self.phase = Phase::Draft;
            self.pending = 0;
            return Landed::Quiet;
        }
        if reply.heard_as.as_deref().is_some_and(|h| !h.trim().is_empty()) {
            self.heard_as.clone_from(&reply.heard_as);
        }
        if !reply.answer.trim().is_empty() {
            self.push(Who::Agent, &reply.answer, now_unix_ms);
        }
        self.pending = reply.suggestions.len();
        if self.pending > 0 {
            self.phase = Phase::Suggested;
            Landed::Suggested(self.pending)
        } else {
            self.phase = Phase::Done;
            Landed::Answered
        }
    }

    /// The agent could not answer: why, for the reviewer.
    pub fn fail(&mut self, why: String) {
        self.phase = Phase::Failed;
        self.pending = 0;
        self.problem = Some(why);
    }

    /// `count` suggestions were taken or refused; when none is left the conversation is done. `false` when nothing waited.
    pub fn settle(&mut self, count: usize) -> bool {
        if self.pending == 0 || count == 0 {
            return false;
        }
        self.pending = self.pending.saturating_sub(count);
        if self.pending == 0 && self.phase == Phase::Suggested {
            self.phase = Phase::Done;
        }
        true
    }

    /// The reviewer says it is finished, whatever waits.
    pub fn resolve(&mut self) {
        self.phase = Phase::Done;
        self.pending = 0;
    }

    /// The reviewer opens a finished conversation again.
    pub fn reopen(&mut self) {
        if self.phase == Phase::Done {
            self.phase = Phase::Draft;
        }
    }

    /// The reviewer typed a follow-up: a turn, and a draft until it is sent. `false` for words that are blank.
    pub fn say(&mut self, text: &str, now_unix_ms: u64) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        self.push(Who::Reviewer, text, now_unix_ms);
        if self.phase != Phase::Working {
            self.phase = Phase::Draft;
        }
        true
    }

    /// The last turn is the reviewer's: a follow-up no agent has answered yet.
    #[must_use]
    pub fn has_unanswered_follow_up(&self) -> bool {
        self.turns.last().is_some_and(|t| t.who == Who::Reviewer)
    }

    /// Keeps a turn, cut to `MAX_TURN_BYTES` and with the oldest turns going past `MAX_TURNS`; a word about what was cut.
    pub fn push(&mut self, who: Who, text: &str, now_unix_ms: u64) -> Option<String> {
        let mut word = None;
        let (kept, was_cut) = cut(text.trim(), MAX_TURN_BYTES);
        if was_cut {
            word = Some(format!("a turn of {} bytes was cut to {MAX_TURN_BYTES} bytes", text.trim().len()));
        }
        if self.turns.len() >= MAX_TURNS {
            self.turns.remove(0);
            word = Some(format!("the oldest turn went: a conversation keeps {MAX_TURNS}"));
        }
        self.turns.push(Turn { who, text: kept.to_string(), at_unix_ms: now_unix_ms });
        word
    }

    /// The dictation heard an utterance (blank words are none); the newest `MAX_SAID` are kept.
    pub fn hear(&mut self, said: Said) {
        self.hear_understood(said, None);
    }

    /// The dictation heard an utterance and `understood` is what its words came to against the workspace's names (none: as
    /// heard).
    pub fn hear_understood(&mut self, said: Said, understood: Option<String>) {
        if said.text.trim().is_empty() {
            return;
        }
        if self.said.len() >= MAX_SAID {
            self.said.remove(0);
            self.understood.remove(0);
        }
        self.said.push(said);
        self.understood.push(understood);
    }

    /// What the whole of what was heard was understood as, when any of it was read as names: each utterance as it was
    /// understood, or as heard where it was not.
    #[must_use]
    pub fn understood_text(&self) -> Option<String> {
        let changed = self.said.iter().zip(&self.understood).any(|(s, u)| u.as_deref().is_some_and(|u| u.trim() != s.text.trim()));
        if !changed {
            return None;
        }
        let parts: Vec<&str> = self
            .said
            .iter()
            .zip(&self.understood)
            .map(|(s, u)| u.as_deref().unwrap_or(&s.text).trim())
            .filter(|t| !t.is_empty())
            .collect();
        Some(parts.join(" "))
    }

    /// What a conversation read from an archive comes to: an agent that was working when the window closed is not
    /// working now, and says so (a failure the reviewer can ask again).
    pub fn restored(&mut self) {
        if self.phase == Phase::Working {
            self.fail("the window was closed while the agent worked: ask again".to_string());
        }
    }

    /// The colour the bottom bar and the tree give it.
    #[must_use]
    pub const fn chip_state(&self) -> chips::State {
        match self.phase {
            Phase::Draft => chips::State::Draft,
            Phase::Working => chips::State::Working,
            Phase::Suggested => chips::State::Suggested,
            Phase::Done => chips::State::Done,
            Phase::Failed => chips::State::Failed,
        }
    }
}

/// The last `n` lines of what is being said: the newest utterances and, last, the words so far of the one being spoken.
#[must_use]
pub fn last_lines(said: &[Said], partial: &str, n: usize) -> Vec<String> {
    let mut lines: Vec<String> = said.iter().map(|s| s.text.trim().to_string()).filter(|t| !t.is_empty()).collect();
    let partial = partial.trim();
    if !partial.is_empty() {
        lines.push(partial.to_string());
    }
    let from = lines.len().saturating_sub(n);
    lines.split_off(from)
}

#[cfg(test)]
#[path = "conversation_test.rs"]
mod conversation_test;
