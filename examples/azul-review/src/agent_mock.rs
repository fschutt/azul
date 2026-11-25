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
    agent_review::{reply_text, Intent, Pos, Reply, ReviewRequest, Suggestion},
    agent_stroke::Meaning,
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

/// What the auto mock answers to a review request.
fn auto_reply(review: &ReviewRequest) -> Reply {
    let mut said: Vec<&str> = review.utterances.iter().map(|u| u.text.as_str()).collect();
    if said.is_empty() {
        said.extend(review.turns.iter().rev().take(1).map(|t| t.text.as_str()));
    }
    let said = said.join(" ");
    let lower = said.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let intent = review
        .strokes
        .iter()
        .any(|s| !s.pinned || s.meaning.is_none())
        .then(|| {
            let meaning = if lower.contains('?') || any(&["why", "what", "warum", "was "]) {
                Meaning::Question
            } else if any(&[
                "remove", "delete", "rename", "replace", "entfern", "lösch", "umbenenn", "ersetz",
            ]) {
                Meaning::Issue
            } else {
                Meaning::Scope
            };
            Intent {
                meaning: Some(meaning),
                confidence: 0.3,
                rationale: "the mock's rule of thumb".to_string(),
            }
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
            let note = if said.trim().is_empty() {
                "reviewed".to_string()
            } else {
                first_words(&said, 40)
            };
            Suggestion::InsertLines {
                after_line,
                lines: vec![format!("// mock: {note}")],
            }
        }
    };
    Reply {
        answer: format!("Mock agent: {}", first_words(&said, 60)),
        suggestions: vec![suggestion],
        intent,
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
