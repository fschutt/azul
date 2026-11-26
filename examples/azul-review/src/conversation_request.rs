//! What an agent is sent for a conversation.
//!
//! One pure function: the note's words (the utterances the dictation heard, with their words and the wall-clock moment of
//! each, or the words as the reviewer typed them), the code of the page the anchor is on with the line under it, and the
//! conversation so far, as the [`ReviewRequest`] the agent module turns into a prompt. The strokes come in as they are
//! (`agent_stroke::StrokeInput`, built where the ink is); this file knows nothing of the window.

use std::path::PathBuf;

use crate::{
    agent_backend::Task,
    agent_review::{Context, ReviewRequest, SaidWord, Speaker, Turn, Utterance},
    agent_stroke::StrokeInput,
    code::SourceFile,
    conversation::{Said, Who},
    model::Note,
    ui,
};

/// The text with its white space folded, to compare what was heard with what the note says now.
fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One utterance the dictation heard, as the request has it: the moment of the first word to the end of the last, on the
/// wall clock, with the words and how sure the recogniser was of each.
fn heard(said: &Said) -> Utterance {
    let first = said.words.first().map_or(0, |w| u64::from(w.start_ms));
    let last = said.words.iter().map(|w| u64::from(w.end_ms)).max().unwrap_or(0);
    Utterance {
        text: said.text.trim().to_string(),
        start_ms: said.started_unix_ms.saturating_add(first),
        end_ms: said.started_unix_ms.saturating_add(last.max(first)),
        words: said
            .words
            .iter()
            .map(|w| SaidWord { w: w.text.clone(), p: Some(w.probability), biased: w.biased })
            .collect(),
    }
}

/// Words typed at `at_unix_ms`: one utterance with no words.
fn typed(text: &str, at_unix_ms: u64) -> Utterance {
    Utterance { text: text.trim().to_string(), start_ms: at_unix_ms, end_ms: at_unix_ms, words: Vec::new() }
}

fn speaker(who: Who) -> Speaker {
    match who {
        Who::Reviewer => Speaker::Reviewer,
        Who::Agent => Speaker::Agent,
    }
}

/// The words a request for `note` is about, as the index reads them: a follow-up alone; else what was dictated, with the
/// probability the recogniser gave each word, or the note's words as they are where they were edited or typed (no words are
/// then rated).
#[must_use]
pub fn spoken_of(note: &Note) -> Said {
    let talk = &note.talk;
    if talk.has_unanswered_follow_up() {
        let text = talk.turns.last().map_or_else(String::new, |t| t.text.clone());
        return Said { text, started_unix_ms: 0, words: Vec::new() };
    }
    let heard_text = talk.said.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    if !talk.said.is_empty() && folded(&heard_text) == folded(&note.text) {
        return Said {
            text: note.text.clone(),
            started_unix_ms: talk.said.first().map_or(0, |s| s.started_unix_ms),
            words: talk.said.iter().flat_map(|s| s.words.iter().cloned()).collect(),
        };
    }
    Said { text: note.text.clone(), started_unix_ms: 0, words: Vec::new() }
}

/// What the agent is asked for the conversation of `note`, in the file `file`, at `now_unix_ms`; `symbols` is where the
/// names the words say are defined (the index's section of the prompt: empty where they named nothing).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn review_request(
    note: &Note,
    file: &SourceFile,
    language: &str,
    strokes: Vec<StrokeInput>,
    workspace: Option<PathBuf>,
    task: Task,
    now_unix_ms: u64,
    symbols: String,
) -> ReviewRequest {
    let talk = &note.talk;
    let follow_up = talk.has_unanswered_follow_up();
    let (utterances, turns) = if follow_up {
        // The follow-up is what is said now; the first message and what was answered are the conversation so far.
        let last = talk.turns.last().map(|t| typed(&t.text, t.at_unix_ms));
        let mut so_far = Vec::new();
        if !note.text.trim().is_empty() {
            so_far.push(Turn { from: Speaker::Reviewer, text: note.text.trim().to_string() });
        }
        let earlier = talk.turns.len().saturating_sub(1);
        so_far.extend(talk.turns[..earlier].iter().map(|t| Turn { from: speaker(t.who), text: t.text.clone() }));
        (last.into_iter().collect(), so_far)
    } else {
        let heard_text = talk.said.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        let utterances = if note.text.trim().is_empty() {
            Vec::new()
        } else if !talk.said.is_empty() && folded(&heard_text) == folded(&note.text) {
            talk.said.iter().map(heard).collect()
        } else {
            vec![typed(&note.text, now_unix_ms)]
        };
        let so_far = talk.turns.iter().map(|t| Turn { from: speaker(t.who), text: t.text.clone() }).collect();
        (utterances, so_far)
    };

    let (first_line, lines) = file.page(note.page);
    let total = file.line_count();
    let anchor = (first_line + ui::line_of(note.y)).min(total.max(1));
    ReviewRequest {
        task,
        conversation: note.id,
        language: language.to_string(),
        utterances,
        strokes,
        context: Context {
            file: file.display.clone(),
            page: note.page,
            first_line,
            lines: lines.to_vec(),
            total_lines: total,
            focus: Some((anchor, anchor)),
            words_under: Vec::new(),
            grid: Some(ui::page_grid(first_line)),
        },
        turns,
        workspace,
        symbols,
    }
}

#[cfg(test)]
#[path = "conversation_request_test.rs"]
mod conversation_request_test;
