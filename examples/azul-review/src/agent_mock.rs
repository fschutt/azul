//! The scripted backend: it answers after a scripted delay on the clock it is given, so the
//! end-to-end tests and a demo run without a model, a login or a key.
//!
//! It is a transport like the others: what it returns is TEXT, the JSON a model would write
//! ([`reply_text`]), and it goes through the same reader as every other backend's answer - so a
//! test of the mock tests the format too. It can be scripted ([`MockBackend::scripted`]: replies,
//! failures, garbage, each with its delay), or answer by what it was asked ([`MockBackend::auto`]:
//! "remove this" removes the word under the stroke, "rename" replaces it, anything else inserts a
//! line after the strokes). It can check the request it gets ([`MockBackend::expecting`]) and keeps
//! a log of the calls that a test (or the window) reads through a handle that outlives the move of
//! the backend to its worker thread.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

use crate::{
    agent_backend::{
        AgentError, Backend, BackendKind, Call, Completion, Control, Identity, Request, Task, Usage,
    },
    agent_export::{cut_list_text, Chapter, CutList, CutSegment, ExportRequest},
    agent_limits::Limits,
    agent_review::{
        is_biased, reply_text, vocabulary, Intent, Pos, Reply, ReviewRequest, SaidWord, Suggestion,
    },
    agent_stroke::Meaning,
    stroke_features::offline_intent_for,
};

/// How often a wait looks at the cancel check, ms.
const SLICE_MS: u64 = 50;

/// One scripted answer.
#[derive(Debug, Clone)]
pub enum Step {
    /// This text, after `delay_ms`.
    Reply { delay_ms: u64, text: String },
    /// This failure, after `delay_ms`.
    Fail { delay_ms: u64, error: AgentError },
}

impl Step {
    /// `reply` written as a model would write it.
    #[must_use]
    pub fn reply(delay_ms: u64, reply: &Reply) -> Self {
        Self::Reply {
            delay_ms,
            text: reply_text(reply),
        }
    }

    /// Any text (garbage, prose, a broken object).
    #[must_use]
    pub fn raw(delay_ms: u64, text: impl Into<String>) -> Self {
        Self::Reply {
            delay_ms,
            text: text.into(),
        }
    }

    #[must_use]
    pub fn fail(delay_ms: u64, error: AgentError) -> Self {
        Self::Fail { delay_ms, error }
    }
}

/// What the mock was asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub task: Task,
    pub conversation: u64,
    pub system: String,
    pub prompt: String,
    pub workdir: Option<PathBuf>,
}

/// A shared view of the calls the mock got.
#[derive(Debug, Clone, Default)]
pub struct MockLog(Arc<Mutex<Vec<Seen>>>);

impl MockLog {
    fn push(&self, seen: Seen) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(seen);
    }

    /// The calls so far, oldest first.
    #[must_use]
    pub fn calls(&self) -> Vec<Seen> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).len()
    }
}

#[derive(Debug)]
enum Mode {
    Script(VecDeque<Step>),
    Auto { delay_ms: u64 },
}

/// The scripted backend.
#[derive(Debug)]
pub struct MockBackend {
    mode: Mode,
    expect: Vec<String>,
    log: MockLog,
}

impl MockBackend {
    /// Answers with `steps`, one per call; after the last the script is used up (a failure).
    #[must_use]
    pub fn scripted(steps: Vec<Step>) -> Self {
        Self {
            mode: Mode::Script(steps.into()),
            expect: Vec::new(),
            log: MockLog::default(),
        }
    }

    /// Answers every request by its own content after `delay_ms`.
    #[must_use]
    pub fn auto(delay_ms: u64) -> Self {
        Self {
            mode: Mode::Auto { delay_ms },
            expect: Vec::new(),
            log: MockLog::default(),
        }
    }

    /// Refuses a call whose system prompt and prompt together lack any of `needles`: the mock
    /// checks the request the window builds.
    #[must_use]
    pub fn expecting(mut self, needles: &[&str]) -> Self {
        self.expect = needles.iter().map(|n| (*n).to_string()).collect();
        self
    }

    /// The log of the calls, to keep before the backend moves to its worker.
    #[must_use]
    pub fn log(&self) -> MockLog {
        self.log.clone()
    }
}

/// Waits `ms` of the clock in slices, looking at the cancel check between them.
fn wait(ctl: &mut Control<'_>, ms: u64) -> Result<(), AgentError> {
    let mut left = ms;
    while left > 0 {
        if ctl.cancelled() {
            return Err(AgentError::Cancelled);
        }
        let step = left.min(SLICE_MS);
        ctl.sleep_ms(step);
        left -= step;
    }
    if ctl.cancelled() {
        Err(AgentError::Cancelled)
    } else {
        Ok(())
    }
}

/// The first words of `text`, at most `max` characters, on one line.
fn first_words(text: &str, max: usize) -> String {
    crate::agent_limits::cap_chars(&text.split_whitespace().collect::<Vec<_>>().join(" "), max).0
}

// ==== What the words are worth ====

/// Words said to try a microphone: what is said is no more than these, it is no instruction.
static TEST_WORDS: &[&str] = &[
    "hello",
    "world",
    "test",
    "testing",
    "one",
    "two",
    "three",
    "check",
    "mic",
    "microphone",
    "sound",
    "hear",
    "hallo",
    "welt",
    "eins",
    "zwei",
    "drei",
    "mikrofon",
    "probe",
];
/// The words of TEST_WORDS that begin a test (one of them has to be there).
static TEST_STARTS: &[&str] = &[
    "test",
    "testing",
    "hello",
    "hallo",
    "check",
    "mic",
    "microphone",
    "mikrofon",
    "sound",
];
/// Words that fill speech and mean nothing.
static FILLERS: &[&str] = &[
    "uh", "um", "umm", "er", "erm", "hmm", "hm", "ah", "oh", "mhm", "äh", "ähm", "ehm", "like",
];
/// Words too small to be what a reviewer means by themselves.
static SMALL_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "to", "of", "it", "is", "are", "this", "that", "these", "those",
    "in", "on", "at", "for", "so", "but", "can", "you", "me", "i", "we", "my", "here", "there",
    "der", "die", "das", "ein", "eine", "und", "oder", "zu", "ist", "es", "dies", "diese", "hier",
    "da", "ich", "du", "mich",
];
/// Words of an instruction or a question: one of them is enough to act on.
static INSTRUCTION_WORDS: &[&str] = &[
    "remove", "delete", "rename", "replace", "entfern", "lösch", "umbenenn", "ersetz", "why",
    "what", "warum", "was",
];

/// The words of `text`, lower-cased: runs of letters, digits and `_`.
fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The number of single-character edits from `a` to `b`.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The name of `names` that `word` is a mishearing of: one edit away for a word of three to six
/// characters, two for a longer one (a name the word is already is not a mishearing of anything).
fn resolve_word<'a>(word: &str, names: &'a [String]) -> Option<&'a String> {
    let len = word.chars().count();
    if len < 3 || names.iter().any(|n| n.eq_ignore_ascii_case(word)) {
        return None;
    }
    let allowed = if len > 6 { 2 } else { 1 };
    names
        .iter()
        .map(|n| (distance(&word.to_lowercase(), &n.to_lowercase()), n))
        .filter(|(d, _)| *d > 0 && *d <= allowed)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n)
}

/// `text` with every word that is a mishearing of a name of `names` put right; none when no word was.
fn corrected(text: &str, names: &[String]) -> Option<String> {
    let mut out = String::new();
    let mut changed = false;
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String, changed: &mut bool| {
        if word.is_empty() {
            return;
        }
        let plain = FILLERS.contains(&word.to_lowercase().as_str())
            || SMALL_WORDS.contains(&word.to_lowercase().as_str())
            || INSTRUCTION_WORDS.contains(&word.to_lowercase().as_str());
        match resolve_word(word, names).filter(|_| !plain) {
            Some(name) => {
                out.push_str(name);
                *changed = true;
            }
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out, &mut changed);
            out.push(c);
        }
    }
    flush(&mut word, &mut out, &mut changed);
    changed.then_some(out)
}

/// What the mock made of the words.
struct Heard {
    /// The transcript with the mishearings put right.
    text: String,
    /// The same, when anything was put right.
    heard_as: Option<String>,
}

/// Whether the mock acts on what was said: the words put right, or why it does nothing. Like the
/// agents it stands for, it does nothing for a test phrase, filler words, words the recogniser was
/// unsure of (not those it was told to listen for: a low probability on a name of the vocabulary
/// is no doubt) and a single word that is no instruction.
fn hear(review: &ReviewRequest, limits: &Limits) -> Result<Heard, String> {
    let mut parts: Vec<&str> = review.utterances.iter().map(|u| u.text.as_str()).collect();
    if parts.is_empty() {
        parts.extend(review.turns.iter().rev().take(1).map(|t| t.text.as_str()));
    }
    let text = parts.join(" ");
    let toks = tokens(&text);
    if toks.is_empty() {
        return Err("nothing was said".to_string());
    }
    let names = vocabulary(&review.context, limits.max_vocabulary);
    let is_plain = |t: &str| FILLERS.contains(&t) || SMALL_WORDS.contains(&t);
    let content: Vec<&str> = toks
        .iter()
        .map(String::as_str)
        .filter(|t| !is_plain(t))
        .collect();
    if content.is_empty() {
        return Err("only filler words were said".to_string());
    }
    if content.iter().all(|t| TEST_WORDS.contains(t))
        && content.iter().any(|t| TEST_STARTS.contains(t))
    {
        return Err("a test phrase, not an instruction".to_string());
    }
    // the recogniser's doubt counts for the words it was not told to listen for
    let said_words: Vec<&SaidWord> = review.utterances.iter().flat_map(|u| &u.words).collect();
    let unbiased: Vec<&&SaidWord> = said_words
        .iter()
        .filter(|w| !is_biased(w, &names))
        .collect();
    let probabilities: Vec<f32> = unbiased.iter().filter_map(|w| w.p).collect();
    if !probabilities.is_empty() {
        let mean = probabilities.iter().sum::<f32>() / probabilities.len() as f32;
        if mean < 0.4 {
            return Err(format!(
                "the words are too uncertain (mean probability {mean:.2})"
            ));
        }
    }
    for w in &unbiased {
        let bare = w.w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        let lower = bare.to_lowercase();
        if let Some(p) = w.p.filter(|p| *p < 0.5) {
            if !bare.is_empty()
                && !is_plain(&lower)
                && !INSTRUCTION_WORDS.contains(&lower.as_str())
                && resolve_word(bare, &names).is_none()
            {
                return Err(format!(
                    "a word the recogniser was unsure of (\"{bare}\", {p:.2}) is nowhere in the code shown"
                ));
            }
        }
    }
    if content.len() == 1
        && !INSTRUCTION_WORDS.iter().any(|k| content[0].starts_with(k))
        && !names.iter().any(|n| n.eq_ignore_ascii_case(content[0]))
    {
        return Err("too short to act on".to_string());
    }
    let heard_as = corrected(&text, &names);
    Ok(Heard {
        text: heard_as.clone().unwrap_or(text),
        heard_as,
    })
}

/// What the auto mock answers to a review request.
fn auto_reply(review: &ReviewRequest) -> Reply {
    let heard = match hear(review, &Limits::default()) {
        Ok(heard) => heard,
        Err(reason) => return Reply::nothing(&reason),
    };
    let said = heard.text.clone();
    let lower = said.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    // what the words say, when they say it plainly; else what the shapes of the strokes the reviewer
    // did not choose a meaning for say (the rule table of the features); else the plain default
    let by_words = if lower.contains('?') || any(&["why", "what", "warum", "was "]) {
        Some(Meaning::Question)
    } else if any(&[
        "remove", "delete", "rename", "replace", "entfern", "lösch", "umbenenn", "ersetz",
    ]) {
        Some(Meaning::Issue)
    } else {
        None
    };
    let rule_of_thumb = |meaning: Meaning| Intent {
        meaning: Some(meaning),
        confidence: 0.3,
        rationale: "the mock's rule of thumb".to_string(),
    };
    let intent = review
        .strokes
        .iter()
        .any(|s| !s.pinned || s.meaning.is_none())
        .then(|| match by_words {
            Some(meaning) => rule_of_thumb(meaning),
            None => offline_intent_for(&review.strokes, &review.context, &Limits::default())
                .unwrap_or_else(|| rule_of_thumb(Meaning::Scope)),
        });
    if review.task == Task::Exploration {
        return Reply {
            answer: format!(
                "Mock answer about {}: {}",
                review.context.file,
                first_words(&said, 80)
            ),
            suggestions: Vec::new(),
            intent,
            heard_as: heard.heard_as,
            no_action: None,
        };
    }
    let word = review.context.words_under.first();
    let suggestion = match word {
        Some(w) if any(&["remove", "delete", "entfern", "lösch"]) => Suggestion::RemoveRange {
            start: Pos {
                line: w.line,
                col: w.col,
            },
            end: Pos {
                line: w.line,
                col: w.col + w.text.len(),
            },
            old: w.text.clone(),
        },
        Some(w) if any(&["rename", "replace", "umbenenn", "ersetz"]) => Suggestion::ReplaceRange {
            start: Pos {
                line: w.line,
                col: w.col,
            },
            end: Pos {
                line: w.line,
                col: w.col + w.text.len(),
            },
            old: w.text.clone(),
            text: "renamed".to_string(),
        },
        _ => {
            let after_line = review
                .context
                .focus
                .map_or(review.context.first_line, |(_, last)| last);
            Suggestion::InsertLines {
                after_line,
                lines: vec![format!("// mock: {}", first_words(&said, 40))],
            }
        }
    };
    Reply {
        answer: format!("Mock agent: {}", first_words(&said, 60)),
        suggestions: vec![suggestion],
        intent,
        heard_as: heard.heard_as,
        no_action: None,
    }
}

/// What the auto mock answers to an export: a fixed cut list of one piece (the first seconds of the
/// first run, the camera as the session has it); no judgement, no cutter.
fn fixed_cut_list(export: &ExportRequest) -> CutList {
    let (start, run_end) = export
        .session
        .runs
        .first()
        .copied()
        .unwrap_or((0, export.session.duration_us));
    let out = (start + 5_000_000).min(if run_end > start {
        run_end
    } else {
        start + 5_000_000
    });
    CutList {
        title: "Mock cut".to_string(),
        segments: vec![CutSegment {
            in_us: start,
            out_us: out,
            camera: export.session.camera,
            speed: 1.0,
            zoom: None,
            caption: None,
            title: Some("Mock chapter".to_string()),
        }],
        chapters: vec![Chapter {
            t_us: start,
            title: "Mock chapter".to_string(),
        }],
    }
}

/// The text the auto mock answers to `request`.
fn auto_text(request: &Request) -> String {
    match request {
        Request::Review(review) => reply_text(&auto_reply(review)),
        Request::Export(export) => cut_list_text(&fixed_cut_list(export)),
    }
}

impl Backend for MockBackend {
    fn identity(&self) -> Identity {
        Identity {
            kind: BackendKind::Mock,
            model: "mock".to_string(),
        }
    }

    fn complete(
        &mut self,
        call: &Call<'_>,
        ctl: &mut Control<'_>,
    ) -> Result<Completion, AgentError> {
        self.log.push(Seen {
            task: call.request.task(),
            conversation: call.request.conversation(),
            system: call.system.clone(),
            prompt: call.prompt.clone(),
            workdir: call.workdir.clone(),
        });
        for needle in &self.expect {
            if !call.prompt.contains(needle.as_str()) && !call.system.contains(needle.as_str()) {
                return Err(AgentError::Settings {
                    why: format!("the mock expected the request to contain {needle:?}"),
                });
            }
        }
        let step = match &mut self.mode {
            Mode::Auto { delay_ms } => Step::Reply {
                delay_ms: *delay_ms,
                text: auto_text(call.request),
            },
            Mode::Script(steps) => steps.pop_front().ok_or_else(|| AgentError::Settings {
                why: "the mock's script is used up".to_string(),
            })?,
        };
        match step {
            Step::Reply { delay_ms, text } => {
                wait(ctl, delay_ms)?;
                let input = u64::try_from(call.prompt.len() / 4).unwrap_or(u64::MAX);
                let output = u64::try_from(text.len() / 4).unwrap_or(u64::MAX);
                Ok(Completion {
                    text,
                    usage: Usage {
                        input_tokens: Some(input),
                        output_tokens: Some(output),
                        cost_usd: Some(0.0),
                        ..Usage::default()
                    },
                    model: "mock".to_string(),
                    notes: Vec::new(),
                })
            }
            Step::Fail { delay_ms, error } => {
                wait(ctl, delay_ms)?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
#[path = "agent_mock_test.rs"]
mod agent_mock_test;
