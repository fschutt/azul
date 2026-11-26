//! The chips of the bottom bar - one per conversation of the open file - and the colour a file gets from its chips.

use super::*;

fn chip(id: u64, file: &str, state: State) -> Chip {
    Chip { id, file: file.to_string(), page: 0, line: 1, state, title: format!("c{id}") }
}

#[test]
fn what_is_working_outranks_what_waits_for_an_answer_and_that_outranks_what_is_done() {
    assert!(State::Working > State::Suggested);
    assert!(State::Suggested > State::Done);
}

#[test]
fn a_file_without_conversations_has_no_colour() {
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Done));
    assert_eq!(chips.file_state("b.rs"), None);
}

#[test]
fn a_file_is_as_unresolved_as_its_most_unresolved_conversation() {
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Done));
    assert_eq!(chips.file_state("a.rs"), Some(State::Done), "all done: green");
    chips.push(chip(2, "a.rs", State::Suggested));
    assert_eq!(chips.file_state("a.rs"), Some(State::Suggested), "a suggestion waits: yellow");
    chips.push(chip(3, "a.rs", State::Working));
    assert_eq!(chips.file_state("a.rs"), Some(State::Working), "an agent is at work: the spinner");
    assert!(chips.set_state(3, State::Done));
    assert_eq!(chips.file_state("a.rs"), Some(State::Suggested));
}

#[test]
fn the_bar_shows_the_chips_of_the_open_file_in_the_order_they_began() {
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Done));
    chips.push(chip(2, "b.rs", State::Working));
    chips.push(chip(3, "a.rs", State::Suggested));
    let ids: Vec<u64> = chips.of_file("a.rs").iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![1, 3]);
}

#[test]
fn counts_say_how_many_work_wait_and_are_done() {
    let mut chips = Chips::default();
    for (id, state) in [(1, State::Working), (2, State::Working), (3, State::Suggested), (4, State::Done)] {
        chips.push(chip(id, "a.rs", state));
    }
    assert_eq!(chips.counts("a.rs"), Counts { working: 2, suggested: 1, done: 1 });
    assert_eq!(chips.counts("other.rs"), Counts::default());
}

#[test]
fn changing_the_state_of_a_conversation_that_is_not_there_changes_nothing() {
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Working));
    assert!(!chips.set_state(9, State::Done));
    assert_eq!(chips.file_state("a.rs"), Some(State::Working));
}

#[test]
fn past_the_cap_the_oldest_done_chips_go_first_with_a_word_about_it() {
    let mut chips = Chips::default();
    for id in 0..MAX_CHIPS as u64 {
        assert_eq!(chips.push(chip(id, "a.rs", State::Done)), Pushed::Kept);
    }
    let pushed = chips.push(chip(9_999, "a.rs", State::Working));
    assert!(matches!(pushed, Pushed::DroppedOldest(_)), "{pushed:?}");
    assert_eq!(chips.len(), MAX_CHIPS);
    assert!(chips.of_file("a.rs").iter().all(|c| c.id != 0), "the oldest done chip went");
    assert!(chips.of_file("a.rs").iter().any(|c| c.id == 9_999), "the new one is in");
}

#[test]
fn past_the_cap_with_nothing_done_the_new_chip_is_refused_with_a_word() {
    let mut chips = Chips::default();
    for id in 0..MAX_CHIPS as u64 {
        chips.push(chip(id, "a.rs", State::Working));
    }
    let pushed = chips.push(chip(9_999, "a.rs", State::Working));
    assert!(matches!(pushed, Pushed::Refused(_)), "{pushed:?}");
    assert_eq!(chips.len(), MAX_CHIPS, "no growth without bound");
}

#[test]
fn a_title_is_the_first_words_cut_at_a_character_not_in_the_middle_of_one() {
    assert_eq!(short_title("why does this loop stop", 40), "why does this loop stop");
    assert_eq!(short_title("  a   long\nconversation   title that goes on and on  ", 20), "a long conversation…");
    assert_eq!(short_title("äöüäöüäöü", 4), "äöüä…", "four characters, not four bytes");
    assert_eq!(short_title("", 10), "");
    assert_eq!(short_title("   \n ", 10), "");
    assert_eq!(short_title("abc", 0), "…");
}

#[test]
fn the_mock_conversations_cover_every_state_over_the_given_files() {
    let files = vec!["a.rs".to_string(), "b.rs".to_string()];
    let chips = mock(50, &files);
    assert_eq!(chips.len(), 50);
    let counts_a = chips.counts("a.rs");
    let counts_b = chips.counts("b.rs");
    assert!(counts_a.working > 0 && counts_a.suggested > 0 && counts_a.done > 0, "{counts_a:?}");
    assert!(counts_b.working > 0 && counts_b.suggested > 0 && counts_b.done > 0, "{counts_b:?}");
    assert_eq!(mock(0, &files).len(), 0);
    assert_eq!(mock(5, &[]).len(), 0, "no file, no conversation");
}
