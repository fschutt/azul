//! The limits of what the agents are given and what they give back: ONE table with realistic
//! defaults (a hostile or silly input - a stroke of a million samples, a reply of a hundred
//! megabytes, a thousand suggestions - is cut to a sane size, never a stall or a crash), the
//! override a job that really needs more asks for (`agent-limit-scale` in the settings, 1 to 8), and
//! the notes a cut leaves behind ("diagnostics"): everything that was capped, repaired or dropped
//! is said, nothing is silently lost.

/// The most notes one request keeps (a reply of a thousand bad suggestions is one note about them).
pub const MAX_NOTES: usize = 24;

/// The smallest and the largest scale of the limits.
pub const MIN_SCALE: u32 = 1;
pub const MAX_SCALE: u32 = 8;

/// The caps of one agent call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Strokes sent with one request.
    pub max_strokes: usize,
    /// Samples read of one stroke; more are thinned evenly.
    pub max_samples_per_stroke: usize,
    /// Points of one stroke's path after it was simplified.
    pub max_svg_points: usize,
    /// Lines of code around the strokes.
    pub max_context_lines: usize,
    /// Characters of one line of code; a longer line is cut.
    pub max_line_chars: usize,
    /// Words listed as under the strokes.
    pub max_words_under: usize,
    /// Utterances of the user's words.
    pub max_utterances: usize,
    /// Characters of all the user's words together.
    pub max_transcript_chars: usize,
    /// Turns of the conversation so far.
    pub max_turns: usize,
    /// Characters of one turn.
    pub max_turn_chars: usize,
    /// Bytes of the whole prompt (system and user parts).
    pub max_prompt_bytes: usize,
    /// Bytes of a reply's text that are looked at.
    pub max_reply_bytes: usize,
    /// Suggestions of one reply.
    pub max_suggestions: usize,
    /// Lines of one insertion.
    pub max_insert_lines: usize,
    /// Bytes of the text of one suggestion.
    pub max_suggestion_bytes: usize,
    /// Characters of the answer shown to the reviewer.
    pub max_answer_chars: usize,
    /// Characters of an intent's reason.
    pub max_rationale_chars: usize,
    /// Bytes of a program's standard output that are kept.
    pub max_stdout_bytes: usize,
    /// Bytes of a program's standard error that are kept.
    pub max_stderr_bytes: usize,
    /// Bytes of an HTTP answer that are accepted.
    pub max_http_response_bytes: usize,
    /// Segments of an export's cut list that are kept.
    pub max_cut_segments: usize,
    /// Stretches of speech an export prompt carries.
    pub max_export_lines: usize,
    /// Files, conversations, strokes, markers and idle gaps an export prompt lists, each.
    pub max_export_items: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_strokes: 8,
            max_samples_per_stroke: 4_000,
            max_svg_points: 96,
            max_context_lines: 160,
            max_line_chars: 300,
            max_words_under: 48,
            max_utterances: 40,
            max_transcript_chars: 6_000,
            max_turns: 20,
            max_turn_chars: 2_000,
            max_prompt_bytes: 64 * 1024,
            max_reply_bytes: 256 * 1024,
            max_suggestions: 12,
            max_insert_lines: 100,
            max_suggestion_bytes: 16 * 1024,
            max_answer_chars: 16_000,
            max_rationale_chars: 240,
            max_stdout_bytes: 2 * 1024 * 1024,
            max_stderr_bytes: 64 * 1024,
            max_http_response_bytes: 2 * 1024 * 1024,
            max_cut_segments: 200,
            max_export_lines: 600,
            max_export_items: 40,
        }
    }
}

impl Limits {
    /// Every cap `factor` times as large (`factor` is held to 1 to 8): the explicit override for a
    /// job that needs more than the defaults.
    #[must_use]
    pub fn scaled(&self, factor: u32) -> Self {
        let k = usize::try_from(factor.clamp(MIN_SCALE, MAX_SCALE)).unwrap_or(1);
        let s = |n: usize| n.saturating_mul(k);
        Self {
            max_strokes: s(self.max_strokes),
            max_samples_per_stroke: s(self.max_samples_per_stroke),
            max_svg_points: s(self.max_svg_points),
            max_context_lines: s(self.max_context_lines),
            max_line_chars: s(self.max_line_chars),
            max_words_under: s(self.max_words_under),
            max_utterances: s(self.max_utterances),
            max_transcript_chars: s(self.max_transcript_chars),
            max_turns: s(self.max_turns),
            max_turn_chars: s(self.max_turn_chars),
            max_prompt_bytes: s(self.max_prompt_bytes),
            max_reply_bytes: s(self.max_reply_bytes),
            max_suggestions: s(self.max_suggestions),
            max_insert_lines: s(self.max_insert_lines),
            max_suggestion_bytes: s(self.max_suggestion_bytes),
            max_answer_chars: s(self.max_answer_chars),
            max_rationale_chars: s(self.max_rationale_chars),
            max_stdout_bytes: s(self.max_stdout_bytes),
            max_stderr_bytes: s(self.max_stderr_bytes),
            max_http_response_bytes: s(self.max_http_response_bytes),
            max_cut_segments: s(self.max_cut_segments),
            max_export_lines: s(self.max_export_lines),
            max_export_items: s(self.max_export_items),
        }
    }
}

/// `text` cut to `max` characters on a character boundary, the last one an ellipsis when it was cut;
/// whether it was.
#[must_use]
pub fn cap_chars(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text.to_string(), false);
    }
    let keep = max.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    if max > 0 {
        out.push('\u{2026}');
    }
    (out, true)
}

/// `text` cut to at most `max` bytes on a character boundary (no ellipsis); whether it was.
#[must_use]
pub fn cap_bytes(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// What was capped, repaired or dropped on the way, in words: kept in order, at most
/// [`MAX_NOTES`], the same sentence once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notes {
    items: Vec<String>,
    dropped: usize,
}

impl Notes {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Keeps `note` (unless it is kept already, or the notes are full).
    pub fn push(&mut self, note: impl Into<String>) {
        let note = note.into();
        if self.items.contains(&note) {
            return;
        }
        if self.items.len() >= MAX_NOTES {
            self.dropped += 1;
        } else {
            self.items.push(note);
        }
    }

    /// Keeps every note of `notes`.
    pub fn extend(&mut self, notes: impl IntoIterator<Item = String>) {
        for note in notes {
            self.push(note);
        }
    }

    /// The notes, with a last one that counts the ones that did not fit.
    #[must_use]
    pub fn finish(mut self) -> Vec<String> {
        if self.dropped > 0 {
            self.items
                .push(format!("{} more notes were not kept", self.dropped));
        }
        self.items
    }

    /// How many are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
#[path = "agent_limits_test.rs"]
mod agent_limits_test;
