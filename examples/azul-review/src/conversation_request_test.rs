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
    HeardWord { text: text.to_string(), start_ms: from, end_ms: to, probability: p, biased: false }
}

fn request(n: &Note) -> ReviewRequest {
    review_request(n, &file(), "en", Vec::new(), None, Task::Editing, 99_000, String::new())
}

#[test]
fn the_request_names_the_conversation_the_language_and_the_task() {
    let n = note("rename this");
    let r = review_request(&n, &file(), "de", Vec::new(), Some("/work".into()), Task::Exploration, 5, String::new());
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
fn what_a_loop_goes_round_is_the_focus_and_not_only_the_line_of_the_anchor() {
    use crate::model::{InkPoint, Semantic, Stroke};
    let mut lines: Vec<String> = (1..=120).map(|i| format!("line {i}")).collect();
    for line in &mut lines[17..23] {
        *line = format!("{}abcdefghijklmnopqrstuvwxyz0123456789", " ".repeat(33));
    }
    let f = SourceFile::new("src/f.rs".into(), "src/f.rs".to_string(), lines);
    let mut n = note("what does this do");
    n.page = 0;
    n.x = 400.0;
    n.y = 297.5;
    let fixture = crate::stroke_fixtures::loop_around_a_block();
    let stroke = Stroke {
        page: 0,
        semantic: Semantic::Scope,
        points: fixture.samples.iter().map(|s| InkPoint { x: s.x, y: s.y, pressure: s.pressure, tilt_x: 0.0, tilt_y: 0.0, t_ms: s.t_ms }).collect(),
        id: 1,
        epoch: 0,
        started_unix_ms: 0,
    };
    let told = crate::request_strokes::pick(&n, &[stroke]);
    assert_eq!(told.len(), 1, "the loop is near the anchor");
    let r = review_request(&n, &f, "en", told, None, Task::Exploration, 5, String::new());
    let (first, last) = r.context.focus.expect("a focus");
    // the anchor is on line 18: the loop goes round the block of lines 18 to 23, and the code the agent reads marks all of them
    assert!(first >= 17 && last >= 20 && last <= 24 && first < last, "lines {first} to {last}");
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

#[test]
fn the_names_the_words_say_are_a_section_of_the_request_of_their_own() {
    let n = note("rename compute stale flag");
    let r = review_request(&n, &file(), "en", Vec::new(), None, Task::Editing, 1, "compute_stale_flag: src/stale.rs line 3".to_string());
    assert_eq!(r.symbols, "compute_stale_flag: src/stale.rs line 3");
    assert!(request(&n).symbols.is_empty(), "words that name nothing add nothing");
}

// ==== The words a request is about, for the index ====

#[test]
fn the_words_the_names_are_read_from_are_what_was_dictated_with_their_probabilities() {
    let mut n = note("this loop is off by one");
    n.talk.hear(Said { text: "this loop".to_string(), started_unix_ms: 10, words: vec![word("this", 0, 100, 0.9), word("loop", 110, 200, 0.4)] });
    n.talk.hear(Said { text: "is off by one".to_string(), started_unix_ms: 20, words: vec![word("is", 0, 50, 0.8)] });
    let said = spoken_of(&n);
    assert_eq!(said.text, "this loop is off by one");
    assert_eq!(said.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), vec!["this", "loop", "is"]);
    assert_eq!(said.words[1].probability, 0.4);
}

#[test]
fn words_that_were_edited_or_typed_are_read_as_words_the_recogniser_did_not_rate() {
    let mut edited = note("this loop is off by two");
    edited.talk.hear(Said { text: "this loop is off by one".to_string(), started_unix_ms: 10, words: vec![word("this", 0, 100, 0.9)] });
    let said = spoken_of(&edited);
    assert_eq!(said.text, "this loop is off by two");
    assert!(said.words.is_empty());
    assert_eq!(spoken_of(&note("typed")).words.len(), 0);
}

#[test]
fn a_follow_up_is_read_alone() {
    let mut n = note("why does this loop stop");
    n.talk.asking();
    n.talk.receive(&crate::agent_review::Reply { answer: "At n.".to_string(), ..Default::default() }, 1);
    assert!(n.talk.say("and compute stale flag?", 5));
    assert_eq!(spoken_of(&n).text, "and compute stale flag?");
}

#[test]
fn the_agent_is_told_which_words_were_favoured_by_the_recogniser_so_that_a_low_probability_on_them_is_no_doubt() {
    let mut favoured = word("compute_stale_flag", 100, 300, 0.2);
    favoured.biased = true;
    let said = Said { text: "compute_stale_flag now".to_string(), started_unix_ms: 1_000, words: vec![favoured, word("now", 320, 500, 0.9)] };
    let utterance = heard(&said);
    assert!(utterance.words[0].biased && !utterance.words[1].biased, "{:?}", utterance.words);
}

#[test]
fn the_request_lists_the_words_the_strokes_are_on_with_their_lines_and_columns() {
    let n = note("rename this");
    // the note is on the second page, which begins at line 47
    let grid = ui::page_grid(47);
    // a stroke along the first word of the first line of that page ("line 47")
    let first = file().lines[46].clone();
    let word_end = first.find(' ').unwrap_or(first.len());
    let (x0, x1) = (grid.origin_x + 0.5, grid.origin_x + (word_end.max(1) as f32 - 0.5) * grid.char_width);
    let y = grid.origin_y + grid.line_height / 2.0;
    let stroke = crate::agent_stroke::StrokeInput {
        id: 3,
        page: 1,
        meaning: None,
        pinned: false,
        nib: "marker".to_string(),
        nib_width: 18.0,
        t0_ms: 0,
        samples: vec![
            crate::agent_stroke::Sample { x: x0, y, pressure: 0.5, dt_ms: 0, tilt: None },
            crate::agent_stroke::Sample { x: x1, y, pressure: 0.5, dt_ms: 40, tilt: None },
        ],
    };
    let r = review_request(&n, &file(), "en", vec![stroke], None, Task::Editing, 99_000, String::new());
    assert_eq!(r.context.words_under.len(), 1, "{:?}", r.context.words_under);
    assert_eq!((r.context.words_under[0].stroke, r.context.words_under[0].line, r.context.words_under[0].col), (3, 47, 0));
    assert_eq!(r.context.words_under[0].text, &first[..word_end]);
    // no strokes, no words
    assert!(request(&n).context.words_under.is_empty());
}
