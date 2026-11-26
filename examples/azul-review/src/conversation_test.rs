//! A conversation: what the agent's answers do to it, what the window shows of it, what is kept and what is cut. No
//! window, no agent and no clock but the one the test passes in.

use super::*;
use crate::agent_review::{Reply, Suggestion};

fn said(text: &str, at: u64) -> Said {
    Said { text: text.to_string(), started_unix_ms: at, words: Vec::new() }
}

fn answer(text: &str) -> Reply {
    Reply { answer: text.to_string(), ..Reply::default() }
}

fn insert() -> Suggestion {
    Suggestion::InsertLines { after_line: 3, lines: vec!["let n = 1;".to_string()] }
}

#[test]
fn a_new_conversation_is_a_draft_nothing_was_asked_of() {
    let c = Conversation::default();
    assert_eq!(c.phase, Phase::Draft);
    assert!(c.turns.is_empty() && c.said.is_empty());
    assert_eq!(c.asked, 0);
    assert_eq!(c.chip_state(), chips::State::Draft);
    assert!(c.can_ask());
}

#[test]
fn asking_makes_it_work_and_one_that_works_is_not_asked_again() {
    let mut c = Conversation::default();
    assert!(c.asking());
    assert_eq!(c.phase, Phase::Working);
    assert_eq!(c.asked, 1);
    assert_eq!(c.chip_state(), chips::State::Working);
    assert!(!c.can_ask());
    assert!(!c.asking(), "a second ask while the agent works changes nothing");
    assert_eq!(c.asked, 1);
}

#[test]
fn an_answer_of_words_is_an_agent_turn_and_ends_the_conversation() {
    let mut c = Conversation::default();
    c.asking();
    let landed = c.receive(&Reply { heard_as: Some("compute_stale_flag".to_string()), ..answer("It is read twice.") }, 1_000);
    assert_eq!(landed, Landed::Answered);
    assert_eq!(c.phase, Phase::Done);
    assert_eq!(c.turns.len(), 1);
    assert_eq!(c.turns[0].who, Who::Agent);
    assert_eq!(c.turns[0].text, "It is read twice.");
    assert_eq!(c.turns[0].at_unix_ms, 1_000);
    assert_eq!(c.heard_as.as_deref(), Some("compute_stale_flag"), "what the agent made of the words, beside the raw ones");
    assert_eq!(c.chip_state(), chips::State::Done);
}

#[test]
fn an_answer_with_suggestions_waits_for_the_reviewer() {
    let mut c = Conversation::default();
    c.asking();
    let reply = Reply { suggestions: vec![insert(), insert()], ..answer("Two lines.") };
    assert_eq!(c.receive(&reply, 5), Landed::Suggested(2));
    assert_eq!(c.phase, Phase::Suggested);
    assert_eq!(c.chip_state(), chips::State::Suggested);
    assert_eq!(c.pending, 2);
    // taking them all is what ends it
    assert!(c.settle(1));
    assert_eq!((c.phase, c.pending), (Phase::Suggested, 1));
    assert!(c.settle(1));
    assert_eq!((c.phase, c.pending), (Phase::Done, 0));
    assert!(!c.settle(1), "nothing is left to take");
}

#[test]
fn an_agent_that_does_nothing_ends_the_conversation_quietly() {
    // The reason is for the log; the reviewer's words stay as they were and nothing is added to what is shown.
    let mut c = Conversation::default();
    c.asking();
    let landed = c.receive(&Reply::nothing("a test phrase"), 7);
    assert_eq!(landed, Landed::Quiet);
    assert_eq!(c.phase, Phase::Draft);
    assert!(c.turns.is_empty());
    assert_eq!(c.heard_as, None);
    assert_eq!(c.problem, None);
    assert_eq!(c.chip_state(), chips::State::Draft, "no colour: nothing is waiting on anybody");
}

#[test]
fn a_failure_keeps_its_reason_and_can_be_asked_again() {
    let mut c = Conversation::default();
    c.asking();
    c.fail("the agent is not installed".to_string());
    assert_eq!(c.phase, Phase::Failed);
    assert_eq!(c.problem.as_deref(), Some("the agent is not installed"));
    assert_eq!(c.chip_state(), chips::State::Failed);
    assert!(c.can_ask());
    assert!(c.asking());
    assert_eq!(c.problem, None, "asking again clears the reason");
    assert_eq!(c.asked, 2);
}

#[test]
fn a_follow_up_is_a_turn_of_the_reviewer_and_waits_to_be_asked() {
    let mut c = Conversation::default();
    c.asking();
    c.receive(&answer("Done."), 10);
    assert!(c.say("and the second loop?", 20));
    assert_eq!(c.phase, Phase::Draft);
    let last = c.turns.last().expect("the turn");
    assert_eq!((last.who, last.text.as_str(), last.at_unix_ms), (Who::Reviewer, "and the second loop?", 20));
    assert!(c.has_unanswered_follow_up());
    c.asking();
    c.receive(&answer("Same."), 30);
    assert!(!c.has_unanswered_follow_up());
    assert!(!c.say("   \n ", 40), "nothing said is no turn");
    assert_eq!(c.turns.len(), 3);
}

#[test]
fn a_conversation_the_reviewer_closes_is_done_and_can_be_opened_again() {
    let mut c = Conversation::default();
    c.asking();
    c.receive(&Reply { suggestions: vec![insert()], ..answer("One line.") }, 1);
    c.resolve();
    assert_eq!((c.phase, c.pending), (Phase::Done, 0));
    c.reopen();
    assert_eq!(c.phase, Phase::Draft);
}

#[test]
fn the_turns_are_kept_to_a_number_with_a_word_about_what_went() {
    let mut c = Conversation::default();
    let mut words = Vec::new();
    for i in 0..MAX_TURNS + 5 {
        if let Some(word) = c.push(Who::Reviewer, &format!("turn {i}"), 0) {
            words.push(word);
        }
    }
    assert_eq!(c.turns.len(), MAX_TURNS);
    assert_eq!(c.turns[0].text, "turn 5", "the oldest went first");
    assert_eq!(words.len(), 5, "one word for each cut");
    assert!(words[0].contains("200"), "the word names the number it keeps: {}", words[0]);
}

#[test]
fn a_long_text_is_cut_at_a_letter_with_a_word_about_it() {
    let mut c = Conversation::default();
    let long = "ü".repeat(MAX_TURN_BYTES);
    let word = c.push(Who::Agent, &long, 0).expect("it was cut");
    assert!(word.contains("bytes"), "{word}");
    let text = &c.turns[0].text;
    assert!(text.len() <= MAX_TURN_BYTES, "{} bytes", text.len());
    assert!(text.is_char_boundary(text.len()), "never in the middle of a letter");
    assert!(text.chars().all(|ch| ch == 'ü'));
}

#[test]
fn the_last_three_lines_are_the_last_three_utterances_and_the_words_so_far() {
    let list: Vec<Said> = ["one", "two", "three", "four"].iter().enumerate().map(|(i, t)| said(t, i as u64)).collect();
    assert_eq!(last_lines(&list, "", 3), vec!["two", "three", "four"]);
    assert_eq!(last_lines(&list, "fi", 3), vec!["three", "four", "fi"], "the words being spoken are the last line");
    assert_eq!(last_lines(&list[..1], "", 3), vec!["one"], "fewer than three are all there are");
    assert_eq!(last_lines(&[], "so far", 3), vec!["so far"]);
    assert!(last_lines(&[], "  ", 3).is_empty(), "blank words so far are no line");
    assert_eq!(last_lines(&list, "", 0), Vec::<String>::new());
}

#[test]
fn the_utterances_kept_are_the_newest_ones() {
    let mut c = Conversation::default();
    for i in 0..MAX_SAID + 3 {
        c.hear(said(&format!("u{i}"), i as u64));
    }
    assert_eq!(c.said.len(), MAX_SAID);
    assert_eq!(c.said[0].text, "u3");
    c.hear(said("  ", 1));
    assert_eq!(c.said.len(), MAX_SAID, "blank words are no utterance");
}

#[test]
fn what_the_agent_was_working_on_when_the_window_closed_comes_back_as_a_failure_to_ask_again() {
    let mut c = Conversation::default();
    c.asking();
    c.restored();
    assert_eq!(c.phase, Phase::Failed);
    assert!(c.problem.as_deref().is_some_and(|p| p.contains("closed")), "{:?}", c.problem);
    // what was settled stays settled
    let mut done = Conversation::default();
    done.asking();
    done.receive(&answer("x"), 1);
    done.restored();
    assert_eq!(done.phase, Phase::Done);
}

#[test]
fn the_phases_have_names_for_the_archive_and_the_scripts() {
    for phase in Phase::ALL {
        assert_eq!(Phase::parse(phase.name()), Some(phase));
    }
    let names: Vec<&str> = Phase::ALL.iter().map(|p| p.name()).collect();
    assert_eq!(names, vec!["draft", "working", "suggested", "done", "failed"]);
    assert_eq!(Phase::parse("sleeping"), None);
}
