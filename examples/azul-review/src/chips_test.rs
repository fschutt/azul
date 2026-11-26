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

#[test]
fn a_conversation_nobody_waits_on_has_no_colour_of_its_own_and_never_outranks_one_that_works_or_waits() {
    assert!(State::Done > State::Failed);
    assert!(State::Failed > State::Draft);
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Draft));
    chips.push(chip(2, "a.rs", State::Failed));
    assert_eq!(chips.file_state("a.rs"), Some(State::Failed), "the most unresolved is still the answer");
    assert_eq!(chips.counts("a.rs"), Counts::default(), "a draft and a failure are none of working, waiting and done");
    chips.push(chip(3, "a.rs", State::Done));
    assert_eq!(chips.file_state("a.rs"), Some(State::Done));
}

#[test]
fn a_chip_is_kept_by_its_file_and_its_number_and_changed_in_place() {
    let mut chips = Chips::default();
    assert_eq!(chips.upsert(chip(1, "a.rs", State::Working)), Pushed::Kept);
    assert_eq!(chips.upsert(chip(1, "b.rs", State::Done)), Pushed::Kept, "the same number in another file is another conversation");
    assert_eq!(chips.len(), 2);
    let mut changed = chip(1, "a.rs", State::Suggested);
    changed.title = "new words".to_string();
    assert_eq!(chips.upsert(changed), Pushed::Kept);
    assert_eq!(chips.len(), 2, "replaced, not added");
    let a = chips.of_file("a.rs");
    assert_eq!((a[0].state, a[0].title.as_str()), (State::Suggested, "new words"));
    assert_eq!(chips.of_file("b.rs")[0].state, State::Done);
}

#[test]
fn a_chip_goes_with_its_conversation() {
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Working));
    chips.push(chip(1, "b.rs", State::Working));
    assert!(chips.remove("a.rs", 1));
    assert!(!chips.remove("a.rs", 1), "it was gone already");
    assert_eq!(chips.len(), 1);
    assert_eq!(chips.of_file("b.rs").len(), 1);
}

#[test]
fn the_files_chips_are_taken_out_when_asked_for_the_ones_of_another_kind_stay() {
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Done));
    chips.push(chip(2, "a.rs", State::Working));
    chips.push(chip(3, "b.rs", State::Done));
    assert_eq!(chips.remove_resting("a.rs"), 1, "only what nobody works on or waits on");
    assert_eq!(chips.of_file("a.rs").iter().map(|c| c.id).collect::<Vec<_>>(), vec![2]);
    assert_eq!(chips.of_file("b.rs").len(), 1);
}

fn ids(items: &[BarItem<'_>]) -> Vec<String> {
    items
        .iter()
        .map(|i| match i {
            BarItem::Chip(c) => c.id.to_string(),
            BarItem::MoreDone(n) => format!("+{n}"),
        })
        .collect()
}

#[test]
fn the_bar_shows_what_works_first_then_what_waits_then_the_rest_and_the_done_ones_last() {
    let list = vec![
        chip(1, "a.rs", State::Done),
        chip(2, "a.rs", State::Suggested),
        chip(3, "a.rs", State::Draft),
        chip(4, "a.rs", State::Working),
        chip(5, "a.rs", State::Failed),
        chip(6, "a.rs", State::Working),
    ];
    let refs: Vec<&Chip> = list.iter().collect();
    assert_eq!(ids(&bar_items(&refs, 10)), vec!["4", "6", "2", "5", "3", "1"]);
}

#[test]
fn many_done_conversations_are_folded_into_a_count_and_the_newest_stay() {
    let list: Vec<Chip> = (1..=40).map(|i| chip(i, "a.rs", State::Done)).collect();
    let refs: Vec<&Chip> = list.iter().collect();
    let items = bar_items(&refs, 6);
    assert_eq!(items.len(), 7);
    assert_eq!(ids(&items), vec!["35", "36", "37", "38", "39", "40", "+34"], "the six newest, then how many more");
    // none, one and many
    assert!(bar_items(&[], 6).is_empty());
    let one = [&list[0]];
    assert_eq!(ids(&bar_items(&one, 6)), vec!["1"]);
    // what works is never folded
    let mut busy: Vec<Chip> = (1..=30).map(|i| chip(i, "a.rs", State::Working)).collect();
    busy.push(chip(99, "a.rs", State::Done));
    let refs: Vec<&Chip> = busy.iter().collect();
    assert_eq!(bar_items(&refs, 0).len(), 31, "thirty at work and the done one folded into a count of one");
    assert_eq!(ids(&bar_items(&refs, 0)).last().map(String::as_str), Some("+1"));
}

#[test]
fn the_state_of_a_conversation_is_set_by_its_file_and_number() {
    let mut chips = Chips::default();
    chips.push(chip(1, "a.rs", State::Working));
    chips.push(chip(1, "b.rs", State::Working));
    assert!(chips.set_state_of("b.rs", 1, State::Done));
    assert_eq!(chips.of_file("a.rs")[0].state, State::Working, "the same number in another file is not touched");
    assert_eq!(chips.of_file("b.rs")[0].state, State::Done);
    assert!(!chips.set_state_of("c.rs", 1, State::Done));
    assert!(!chips.set_state_of("a.rs", 9, State::Done));
}
