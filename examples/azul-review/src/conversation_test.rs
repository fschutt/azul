//! A conversation: what the agent's answers do to it, what the window shows of it, what is kept and what is cut. No
//! window, no agent and no clock but the one the test passes in.

use super::*;
use crate::agent_review::{Pos, Reply, Suggestion};

fn said(text: &str, at: u64) -> Said {
    Said { text: text.to_string(), started_unix_ms: at, words: Vec::new() }
}

fn answer(text: &str) -> Reply {
    Reply { answer: text.to_string(), ..Reply::default() }
}

fn insert() -> Suggestion {
    Suggestion::InsertLines { after_line: 3, lines: vec!["let n = 1;".to_string()] }
}

fn remove_range() -> Suggestion {
    Suggestion::RemoveRange { start: Pos { line: 4, col: 4 }, end: Pos { line: 4, col: 9 }, old: "stale".to_string() }
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
fn an_answer_with_suggestions_waits_for_the_reviewer_who_takes_or_refuses_each() {
    let mut c = Conversation::default();
    c.asking();
    let reply = Reply { suggestions: vec![insert(), insert()], ..answer("Two lines.") };
    assert_eq!(c.receive(&reply, 5), Landed::Suggested(2));
    assert_eq!(c.phase, Phase::Suggested);
    assert_eq!(c.chip_state(), chips::State::Suggested);
    assert_eq!(c.pending(), 2);
    assert!(c.proposals.iter().all(|p| p.decision == Decision::Waiting));
    // taking and refusing them all is what ends it
    assert!(c.decide(0, Decision::Accepted));
    assert_eq!((c.phase, c.pending()), (Phase::Suggested, 1));
    assert!(c.decide(1, Decision::Rejected));
    assert_eq!((c.phase, c.pending()), (Phase::Done, 0));
    assert!(!c.decide(5, Decision::Accepted), "there is no such suggestion");
    assert!(!c.decide(0, Decision::Accepted), "it was taken already: nothing changed");
}

#[test]
fn a_decision_can_be_taken_back_and_the_conversation_waits_again() {
    let mut c = Conversation::default();
    c.asking();
    c.receive(&Reply { suggestions: vec![insert()], ..answer("One.") }, 1);
    assert!(c.decide(0, Decision::Accepted));
    assert_eq!(c.phase, Phase::Done);
    assert!(c.decide(0, Decision::Waiting));
    assert_eq!((c.phase, c.pending()), (Phase::Suggested, 1), "waiting again for the reviewer");
}

#[test]
fn all_that_waits_can_be_taken_or_refused_at_once_and_what_was_taken_is_listed_in_order() {
    let mut c = Conversation::default();
    c.asking();
    let reply = Reply { suggestions: vec![insert(), remove_range(), insert()], ..answer("Three.") };
    c.receive(&reply, 1);
    assert!(c.decide(1, Decision::Rejected));
    assert_eq!(c.accept_all(), 2, "the two that waited");
    assert_eq!((c.phase, c.pending()), (Phase::Done, 0));
    assert_eq!(c.accepted().count(), 2);
    assert!(c.accepted().all(|s| matches!(s, Suggestion::InsertLines { .. })));
    let mut d = Conversation::default();
    d.asking();
    d.receive(&Reply { suggestions: vec![insert(), insert()], ..answer("Two.") }, 1);
    assert_eq!(d.reject_all(), 2);
    assert_eq!(d.accepted().count(), 0);
    assert_eq!(d.phase, Phase::Done);
    assert_eq!(d.reject_all(), 0, "nothing waits");
}

#[test]
fn a_second_answer_adds_its_suggestions_to_the_ones_before_and_keeps_what_was_decided() {
    let mut c = Conversation::default();
    c.asking();
    c.receive(&Reply { suggestions: vec![insert()], ..answer("First.") }, 1);
    c.decide(0, Decision::Accepted);
    assert!(c.say("and one more", 2));
    c.asking();
    c.receive(&Reply { suggestions: vec![remove_range()], ..answer("Second.") }, 3);
    assert_eq!(c.proposals.len(), 2);
    assert_eq!(c.proposals[0].decision, Decision::Accepted, "what was decided stays");
    assert_eq!(c.proposals[1].decision, Decision::Waiting);
    assert_eq!(c.phase, Phase::Suggested);
}

#[test]
fn the_suggestions_are_kept_to_a_number_the_decided_ones_going_first() {
    let mut c = Conversation::default();
    c.asking();
    let many = Reply { suggestions: (0..MAX_PROPOSALS + 5).map(|_| insert()).collect(), ..answer("Many.") };
    c.receive(&many, 1);
    assert_eq!(c.proposals.len(), MAX_PROPOSALS, "no more than it keeps");
    assert_eq!(c.pending(), MAX_PROPOSALS);
    // decided ones make room for a later answer
    c.accept_all();
    c.say("again", 2);
    c.asking();
    c.receive(&Reply { suggestions: vec![remove_range()], ..answer("Later.") }, 3);
    assert_eq!(c.proposals.len(), MAX_PROPOSALS);
    assert!(matches!(c.proposals.last().map(|p| &p.suggestion), Some(Suggestion::RemoveRange { .. })), "the newest is in");
    assert_eq!(c.proposals.last().map(|p| p.decision), Some(Decision::Waiting));
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
    assert_eq!((c.phase, c.pending()), (Phase::Done, 0));
    assert_eq!(c.proposals[0].decision, Decision::Rejected, "closing it takes nothing that waited");
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

// ==== What the words were understood as ====

#[test]
fn what_the_words_were_understood_as_is_kept_beside_each_utterance_and_never_replaces_it() {
    let mut c = Conversation::default();
    c.hear_understood(said("compute stale flag", 1), Some("compute_stale_flag".to_string()));
    c.hear_understood(said("and then rename it", 2), None);
    assert_eq!(c.said[0].text, "compute stale flag", "the words as heard are untouched");
    assert_eq!(c.understood, vec![Some("compute_stale_flag".to_string()), None]);
    assert_eq!(
        c.understood_text().as_deref(),
        Some("compute_stale_flag and then rename it"),
        "the whole, with what was not changed as it was said"
    );
}

#[test]
fn nothing_understood_differently_says_nothing() {
    let mut c = Conversation::default();
    c.hear_understood(said("rename this", 1), None);
    c.hear_understood(said("same words", 2), Some("same words".to_string()));
    assert_eq!(c.understood_text(), None);
    assert_eq!(Conversation::default().understood_text(), None);
}

#[test]
fn what_was_understood_goes_with_the_utterance_when_the_oldest_go() {
    let mut c = Conversation::default();
    for i in 0..MAX_SAID + 2 {
        c.hear_understood(said(&format!("u{i}"), i as u64), Some(format!("U{i}")));
    }
    assert_eq!(c.said.len(), MAX_SAID);
    assert_eq!(c.understood.len(), MAX_SAID, "one for each utterance kept");
    assert_eq!(c.understood[0].as_deref(), Some("U2"));
    c.hear(said("plain", 9));
    assert_eq!(c.understood.len(), c.said.len());
    assert_eq!(c.understood.last(), Some(&None));
}

// ==== What is due for the log of resolved conversations ====

#[test]
fn a_conversation_is_due_for_the_log_when_it_becomes_done_and_not_before() {
    let mut c = Conversation::default();
    assert!(!c.due(), "a draft is not resolved");
    c.asking();
    assert!(!c.due(), "nor is one the agent works on");
    c.receive(&answer("It is read twice."), 1_000);
    assert_eq!(c.phase, Phase::Done);
    assert!(c.due(), "an answer of words resolves it");
    c.mark_logged(None);
    assert!(!c.due(), "once it is written it is not due again");
    c.reopen();
    assert!(!c.due(), "a conversation opened again is not resolved");
    c.resolve();
    assert!(c.due(), "resolved a second time, it is due a second time");
}

#[test]
fn a_conversation_that_waits_is_not_due_and_one_whose_last_suggestion_is_decided_is_due_again_each_time() {
    let mut c = Conversation::default();
    c.asking();
    c.receive(&Reply { suggestions: vec![insert()], ..answer("One line.") }, 1);
    assert!(!c.due());
    assert!(c.decide(0, Decision::Accepted));
    assert!(c.due(), "the last suggestion is decided: done");
    c.mark_logged(Some("20251126-054012-c1.patch".to_string()));
    assert_eq!(c.patch.as_deref(), Some("20251126-054012-c1.patch"), "the name of what was written is kept");
    assert!(c.decide(0, Decision::Waiting));
    assert_eq!(c.phase, Phase::Suggested);
    assert!(!c.due());
    assert!(c.decide(0, Decision::Rejected));
    assert!(c.due(), "decided again: a second entry that replaces the first");
    assert_eq!(c.patch.as_deref(), Some("20251126-054012-c1.patch"), "the old name stays until the new entry is written");
}

#[test]
fn how_a_conversation_ended_is_what_was_taken_what_was_refused_what_was_answered_or_nothing() {
    use crate::patch::Outcome;
    let mut taken = Conversation::default();
    taken.receive(&Reply { suggestions: vec![insert(), remove_range()], ..answer("Two edits.") }, 1);
    taken.decide(0, Decision::Accepted);
    taken.decide(1, Decision::Rejected);
    assert_eq!(taken.outcome(), Outcome::Accepted, "one taken is enough");
    let mut refused = Conversation::default();
    refused.receive(&Reply { suggestions: vec![insert()], ..answer("One edit.") }, 1);
    refused.resolve();
    assert_eq!(refused.outcome(), Outcome::Rejected, "closing it takes nothing");
    let mut answered = Conversation::default();
    answered.receive(&answer("It is read twice."), 1);
    assert_eq!(answered.outcome(), Outcome::Answered);
    let mut closed = Conversation::default();
    closed.resolve();
    assert_eq!(closed.outcome(), Outcome::Closed, "no answer, no suggestion");
}
