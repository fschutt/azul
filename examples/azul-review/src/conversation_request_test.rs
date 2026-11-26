//! What an agent is sent for a conversation: its words with their times, the code of the page around the anchor and the
//! conversation so far. Pure: no window, no agent.

use super::*;
use crate::{
    agent_review::Speaker,
    code::{SourceFile, LINES_PER_PAGE},
    conversation::{Conversation, Said, Who},
    dictation::HeardWord,
    model::{Note, NoteState},
};

fn file() -> SourceFile {
    let lines = (1..=120).map(|i| format!("line {i}")).collect();
    SourceFile::new("src/f.rs".into(), "src/f.rs".to_string(), lines)
}

fn note(text: &str) -> Note {
    Note {
        id: 7,
        page: 1,
        x: 120.0,
        y: crate::ui::PAGE_PAD + 3.0 * crate::ui::LINE_H + 2.0,
        text: text.to_string(),
        language: "en".to_string(),
        state: NoteState::Done,
        talk: Conversation::default(),
    }
}

fn word(text: &str, from: u32, to: u32, p: f32) -> HeardWord {
    HeardWord { text: text.to_string(), start_ms: from, end_ms: to, probability: p }
}

fn request(n: &Note) -> ReviewRequest {
    review_request(n, &file(), "en", Vec::new(), None, Task::Editing, 99_000)
}

#[test]
fn the_request_names_the_conversation_the_language_and_the_task() {
    let n = note("rename this");
    let r = review_request(&n, &file(), "de", Vec::new(), Some("/work".into()), Task::Exploration, 5);
    assert_eq!(r.conversation, 7);
    assert_eq!(r.language, "de");
    assert_eq!(r.task, Task::Exploration);
    assert_eq!(r.workspace.as_deref(), Some(std::path::Path::new("/work")));
    assert!(r.turns.is_empty());
}

#[test]
fn the_words_are_the_utterances_with_their_moments_on_the_wall_clock() {
    let mut n = note("this loop is off by one");
    n.talk.hear(Said {
        text: "this loop".to_string(),
        started_unix_ms: 10_000,
        words: vec![word("this", 100, 300, 0.9), word("loop", 320, 600, 0.4)],
    });
    n.talk.hear(Said {
        text: "is off by one".to_string(),
        started_unix_ms: 12_000,
        words: vec![word("is", 0, 100, 0.8)],
    });
    let r = request(&n);
    assert_eq!(r.utterances.len(), 2);
    let first = &r.utterances[0];
    assert_eq!((first.text.as_str(), first.start_ms, first.end_ms), ("this loop", 10_100, 10_600));
    assert_eq!(first.words.len(), 2);
    assert_eq!((first.words[1].w.as_str(), first.words[1].p, first.words[1].biased), ("loop", Some(0.4), false));
    assert_eq!((r.utterances[1].start_ms, r.utterances[1].end_ms), (12_000, 12_100));
}

#[test]
fn an_utterance_with_no_words_has_the_moment_its_call_began() {
    let mut n = note("hello");
    n.talk.hear(Said { text: "hello".to_string(), started_unix_ms: 4_000, words: Vec::new() });
    let r = request(&n);
    assert_eq!((r.utterances[0].start_ms, r.utterances[0].end_ms), (4_000, 4_000));
    assert!(r.utterances[0].words.is_empty());
}

#[test]
fn words_the_reviewer_edited_are_sent_as_typed_with_no_times() {
    // what was heard no longer says what the note says: the note is what is meant
    let mut n = note("this loop is off by two");
    n.talk.hear(Said { text: "this loop is off by one".to_string(), started_unix_ms: 10_000, words: vec![word("this", 0, 200, 0.9)] });
    let r = request(&n);
    assert_eq!(r.utterances.len(), 1);
    assert_eq!(r.utterances[0].text, "this loop is off by two");
    assert!(r.utterances[0].words.is_empty());
    assert_eq!((r.utterances[0].start_ms, r.utterances[0].end_ms), (99_000, 99_000), "typed now");
}

#[test]
fn a_note_that_was_never_dictated_is_one_utterance_of_its_text_typed_now() {
    let r = request(&note("what does this do?"));
    assert_eq!(r.utterances.len(), 1);
    assert_eq!(r.utterances[0].text, "what does this do?");
    assert_eq!(r.utterances[0].start_ms, 99_000);
}

#[test]
fn a_note_with_no_words_sends_no_utterance() {
    assert!(request(&note("   ")).utterances.is_empty());
}

#[test]
fn the_context_is_the_page_the_anchor_is_on_and_the_line_under_it() {
    let r = request(&note("x"));
    let c = &r.context;
    assert_eq!(c.file, "src/f.rs");
    assert_eq!(c.page, 1);
    assert_eq!(c.first_line, LINES_PER_PAGE + 1);
    assert_eq!(c.lines.len(), LINES_PER_PAGE);
    assert_eq!(c.lines[0], format!("line {}", LINES_PER_PAGE + 1));
    assert_eq!(c.total_lines, 120);
    // the anchor is on the fourth line of the page
    assert_eq!(c.focus, Some((LINES_PER_PAGE + 4, LINES_PER_PAGE + 4)));
    let grid = c.grid.expect("the page's grid, so the strokes' coordinates can be read against the code");
    assert_eq!(grid.first_line, LINES_PER_PAGE + 1);
    assert!(c.words_under.is_empty(), "no stroke, no words under one");
}

#[test]
fn an_anchor_under_the_last_line_of_a_short_page_focuses_the_last_line() {
    let mut n = note("x");
    n.page = 2; // lines 93 to 120: 28 lines
    n.y = crate::ui::PAGE_PAD + 40.0 * crate::ui::LINE_H;
    let r = request(&n);
    assert_eq!(r.context.lines.len(), 28);
    assert_eq!(r.context.focus, Some((120, 120)));
}

#[test]
fn a_follow_up_sends_the_conversation_so_far_and_the_follow_up_as_the_words() {
    let mut n = note("why does this loop stop");
    n.talk.asking();
    n.talk.receive(&crate::agent_review::Reply { answer: "It stops at n.".to_string(), ..Default::default() }, 1_000);
    assert!(n.talk.say("and when n is zero?", 50_000));
    let r = request(&n);
    let who: Vec<(Speaker, &str)> = r.turns.iter().map(|t| (t.from, t.text.as_str())).collect();
    assert_eq!(
        who,
        vec![(Speaker::Reviewer, "why does this loop stop"), (Speaker::Agent, "It stops at n.")],
        "the first message and what was answered, not the follow-up"
    );
    assert_eq!(r.utterances.len(), 1);
    assert_eq!(r.utterances[0].text, "and when n is zero?");
    assert_eq!((r.utterances[0].start_ms, r.utterances[0].end_ms), (50_000, 50_000), "typed when it was said");
}

#[test]
fn asking_again_without_a_new_word_sends_the_same_words_and_what_was_said_before() {
    let mut n = note("rename this");
    n.talk.asking();
    n.talk.fail("the agent is not installed".to_string());
    let first = request(&n);
    assert!(first.turns.is_empty());
    n.talk.asking();
    n.talk.receive(&crate::agent_review::Reply { answer: "Renamed.".to_string(), ..Default::default() }, 1);
    n.talk.turns.push(crate::conversation::Turn { who: Who::Agent, text: "More.".to_string(), at_unix_ms: 2 });
    let again = request(&n);
    assert_eq!(again.utterances.len(), 1, "the note's words are asked once more");
    assert_eq!(again.turns.len(), 2, "with what the agent said before");
}
