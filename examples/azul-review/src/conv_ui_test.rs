//! The conversations of a page, built without a window: the chips in the margin, the open conversation and what it
//! holds while it listens, while an agent works and after, that every control has a name for the screen reader.

use std::path::PathBuf;

use azul::{prelude::*, str::String as AzString};

use super::*;
use crate::{
    agent_review::Reply,
    code::SourceFile,
    conversation::{Phase, Said, Turn, Who},
    dictation::Cause,
    model::{Note, NoteState},
    voice::Running,
};

fn app() -> AppState {
    crate::l10n::in_english();
    let file = SourceFile {
        path: PathBuf::from("f.rs"),
        display: "f.rs".to_string(),
        lines: (0..100).map(|i| format!("fn f{i}() {{}}")).collect(),
        big: None,
    };
    AppState::create(PathBuf::new(), vec![file], RefAny::new(0u8), PathBuf::new())
}

fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn with_id<'a>(dom: &'a Dom, id: &str) -> Option<&'a Dom> {
    let id = AzString::from(id);
    nodes(dom).into_iter().find(|n| n.root.has_id(id.clone()))
}

fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom)
        .into_iter()
        .filter_map(|n| n.root.node_type.get_text().into_option())
        .map(|t| t.as_str().to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

fn note(id: u64, page: usize, y: f32, text: &str) -> Note {
    Note {
        id,
        page,
        x: 120.0,
        y,
        text: text.to_string(),
        language: "en-US".to_string(),
        state: NoteState::Done,
        talk: crate::conversation::Conversation::default(),
    }
}

fn data() -> RefAny {
    RefAny::new(0u8)
}

/// The popover of conversation `id` open on page 0.
fn open(s: &mut AppState, id: u64) -> Dom {
    s.open_conv = Some(id);
    popover(s, &data(), 0, ui::PAGE_W).expect("the open conversation's popover")
}

#[test]
fn a_page_with_no_conversation_has_no_margin_and_nothing_open() {
    let mut s = app();
    assert!(margin(&s, &data(), 0).is_none());
    s.notes.push(note(1, 1, 100.0, "on the second page"));
    assert!(margin(&s, &data(), 0).is_none(), "the note is on page 1");
    s.open_conv = Some(1);
    assert!(popover(&s, &data(), 0, ui::PAGE_W).is_none(), "and so is its popover");
    assert!(popover(&s, &data(), 1, ui::PAGE_W).is_some());
}

#[test]
fn a_conversation_is_a_chip_in_the_margin_of_its_page_named_for_the_screen_reader() {
    let mut s = app();
    // the fourth line of page 0
    s.notes.push(note(1, 0, ui::PAGE_PAD + 3.0 * ui::LINE_H + 2.0, "the bound is off by one"));
    s.notes.push(note(2, 1, 100.0, "another page"));
    let m = margin(&s, &data(), 0).expect("the margin of page 0");
    assert!(with_id(&m, "__azreview_margin-1").is_some());
    let chip = with_id(&m, "__azreview_conv-1").expect("its chip");
    assert!(!chip.root.accessibility.is_null(), "the chip is named");
    assert!(with_id(&m, "__azreview_conv-2").is_none(), "the other page's conversation is in its own margin");
    let said = texts(chip).join(" | ");
    assert!(said.contains("the bound is off by one"), "{said}");
    assert!(said.contains('1'), "its number: {said}");
}

#[test]
fn a_margin_that_cannot_hold_all_says_how_many_more_there_are() {
    let mut s = app();
    for id in 1..=40 {
        s.notes.push(note(id, 0, 200.0, &format!("conversation {id}")));
    }
    let m = margin(&s, &data(), 0).expect("a margin");
    let shown = (1..=40u64).filter(|id| with_id(&m, &format!("__azreview_conv-{id}")).is_some()).count();
    assert_eq!(shown, margin::capacity(ui::page_h() - 2.0 * ui::PAGE_PAD, CHIP_H, CHIP_GAP));
    let more = with_id(&m, "__azreview_margin-more-1").expect("the count of the rest");
    assert!(texts(more).join(" ").contains(&format!("{} more", 40 - shown)), "{:?}", texts(more));
}

#[test]
fn what_works_stays_in_a_full_margin_and_so_does_the_open_one() {
    let mut s = app();
    for id in 1..=40 {
        s.notes.push(note(id, 0, 200.0, "x"));
    }
    s.notes[38].talk.asking(); // 39 works
    s.open_conv = Some(40);
    let m = margin(&s, &data(), 0).expect("a margin");
    assert!(with_id(&m, "__azreview_conv-39").is_some(), "the one at work is in sight");
    assert!(with_id(&m, "__azreview_conv-40").is_some(), "so is the open one");
}

#[test]
fn the_open_conversation_has_its_place_a_text_area_and_keys_that_have_names() {
    let mut s = app();
    s.notes.push(note(1, 0, ui::PAGE_PAD + 3.0 * ui::LINE_H + 2.0, "the bound is off by one"));
    let pop = open(&mut s, 1);
    assert!(with_id(&pop, "__azreview_conv-pop-1").is_some());
    let said = texts(&pop).join(" | ");
    assert!(said.contains("Page 1, line 4"), "{said}");
    let area = with_id(&pop, "__azreview_note-text-1").expect("the words are the reviewer's to edit");
    assert!(!area.root.accessibility.is_null(), "the text area names itself");
    assert!(with_id(&pop, "__azreview_conv-reply-1").is_some(), "a place for the reply");
    for key in ["__azreview_note-delete-1", "__azreview_conv-close-1", "__azreview_conv-send-1", "__azreview_conv-resolve-1"] {
        let b = with_id(&pop, key).unwrap_or_else(|| panic!("{key} is a key of the open conversation"));
        assert!(!b.root.accessibility.is_null(), "{key} has no name");
    }
    let buttons = nodes(&pop).into_iter().filter(|n| matches!(n.root.node_type, NodeType::Button)).count();
    assert_eq!(buttons, 4, "delete, close, resolve, send: no key without a use");
    assert!(!nodes(&pop).iter().all(|n| n.root.accessibility.is_null()), "the popover is a dialog with a name");
}

#[test]
fn a_conversation_that_listens_shows_the_last_three_lines_said_and_does_not_let_them_be_edited() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "one two three four");
    n.state = NoteState::Listening;
    for (i, line) in ["one", "two", "three", "four"].iter().enumerate() {
        n.talk.hear(Said { text: (*line).to_string(), started_unix_ms: i as u64, words: Vec::new() });
    }
    s.notes.push(n);
    s.voice.running = Some(Running::for_test(1, 1));
    let pop = open(&mut s, 1);
    assert!(with_id(&pop, "__azreview_note-text-1").is_none(), "the dictation owns the text while it listens");
    let live = with_id(&pop, "__azreview_conv-live-1").expect("the live lines");
    assert_eq!(texts(live), vec!["two", "three", "four"], "the last three");
    assert!(with_id(&pop, "__azreview_listening").is_some(), "the listening pill");
    let stop = with_id(&pop, "__azreview_dictation-stop").expect("a stop button");
    assert!(!stop.root.accessibility.is_null(), "the stop button has a name");
    assert!(with_id(&pop, "__azreview_conv-reply-1").is_none(), "nothing to reply to while it listens");
}

#[test]
fn the_words_being_spoken_are_the_last_line_in_italics_and_for_that_conversation_only() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "this loop");
    n.state = NoteState::Listening;
    n.talk.hear(Said { text: "this loop".to_string(), started_unix_ms: 0, words: Vec::new() });
    s.notes.push(n);
    s.notes.push(note(2, 0, 80.0, "another"));
    let mut running = Running::for_test(1, 1);
    running.ready = true;
    running.partial = "is off by".to_string();
    s.voice.running = Some(running);
    let pop = open(&mut s, 1);
    let spoken = with_id(&pop, "__azreview_note-partial-1").expect("the words so far");
    assert_eq!(texts(spoken), vec!["is off by".to_string()]);
    assert_eq!(texts(with_id(&pop, "__azreview_conv-live-1").expect("lines")), vec!["this loop", "is off by"]);
    // none while nothing is being said, and none for another conversation
    if let Some(r) = s.voice.running.as_mut() {
        r.partial.clear();
    }
    assert!(with_id(&popover(&s, &data(), 0, ui::PAGE_W).expect("open"), "__azreview_note-partial-1").is_none());
    s.open_conv = Some(2);
    assert!(with_id(&popover(&s, &data(), 0, ui::PAGE_W).expect("open"), "__azreview_note-partial-2").is_none());
}

#[test]
fn a_dictation_says_listening_when_its_microphone_is_ready_and_before_that_that_it_is_opening() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "");
    n.state = NoteState::Listening;
    s.notes.push(n);
    s.voice.running = Some(Running::for_test(1, 1));
    let pop = open(&mut s, 1);
    let pill = texts(with_id(&pop, "__azreview_listening").expect("the pill")).join(" ");
    assert!(pill.contains("Opening the microphone"), "{pill}");
    assert!(with_id(&pop, "__azreview_dictation-stop").is_some(), "a dictation that is opening can be called off");
    assert!(texts(with_id(&pop, "__azreview_conv-live-1").expect("lines")).iter().any(|t| t.contains("Waiting")), "it waits for words");

    if let Some(r) = s.voice.running.as_mut() {
        r.ready = true;
    }
    let pop = popover(&s, &data(), 0, ui::PAGE_W).expect("open");
    let pill = texts(with_id(&pop, "__azreview_listening").expect("the pill")).join(" ");
    assert!(pill.contains("Listening") && !pill.contains("Opening"), "{pill}");
}

#[test]
fn a_dictation_that_finishes_has_no_stop_button_and_says_so() {
    let mut s = app();
    s.notes.push(note(1, 0, 40.0, "words"));
    let mut running = Running::for_test(1, 1);
    running.stopping = Some(Cause::Key);
    s.voice.running = Some(running);
    let pop = open(&mut s, 1);
    assert!(with_id(&pop, "__azreview_dictation-stop").is_none(), "it was stopped already");
    let pill = texts(with_id(&pop, "__azreview_listening").expect("the pill stays until the last call ends")).join(" ");
    assert!(pill.contains("Finishing"), "{pill}");
}

#[test]
fn the_thread_shows_who_said_what_and_what_the_agent_understood() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "compute stale flag is read twice");
    n.talk.heard_as = Some("compute_stale_flag is read twice".to_string());
    n.talk.turns.push(Turn { who: Who::Agent, text: "It is read on lines 4 and 9.".to_string(), at_unix_ms: 1 });
    n.talk.turns.push(Turn { who: Who::Reviewer, text: "and the second loop?".to_string(), at_unix_ms: 2 });
    n.talk.phase = Phase::Done;
    s.notes.push(n);
    let pop = open(&mut s, 1);
    let heard = texts(with_id(&pop, "__azreview_conv-heard-1").expect("what it understood")).join(" ");
    assert_eq!(heard, "Heard as: compute_stale_flag is read twice");
    let first = texts(with_id(&pop, "__azreview_conv-turn-1-0").expect("the first turn"));
    assert_eq!(first, vec!["Agent", "It is read on lines 4 and 9."]);
    let second = texts(with_id(&pop, "__azreview_conv-turn-1-1").expect("the second turn"));
    assert_eq!(second, vec!["You", "and the second loop?"]);
    assert!(with_id(&pop, "__azreview_conv-done-1").is_some(), "a finished conversation says so");
}

#[test]
fn the_agents_understanding_is_not_repeated_when_it_is_what_the_reviewer_said() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "rename this");
    n.talk.heard_as = Some("rename this".to_string());
    s.notes.push(n);
    assert!(with_id(&open(&mut s, 1), "__azreview_conv-heard-1").is_none());
}

#[test]
fn an_agent_at_work_shows_the_spinner_and_a_failure_shows_its_reason_and_a_key_to_ask_again() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "rename this");
    n.talk.asking();
    s.notes.push(n);
    let pop = open(&mut s, 1);
    assert!(with_id(&pop, "__azreview_conv-working-1").is_some());
    assert!(with_id(&pop, "__azreview_conv-problem-1").is_none());

    s.notes[0].talk.fail("claude is not installed".to_string());
    let pop = popover(&s, &data(), 0, ui::PAGE_W).expect("open");
    assert!(with_id(&pop, "__azreview_conv-working-1").is_none());
    assert_eq!(texts(with_id(&pop, "__azreview_conv-problem-1").expect("the reason")), vec!["claude is not installed"]);
    let send = with_id(&pop, "__azreview_conv-send-1").expect("a key to ask again");
    assert!(!send.root.accessibility.is_null());
}

#[test]
fn a_finished_conversation_offers_to_open_it_again_and_an_unfinished_one_to_finish_it() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "x");
    n.talk.asking();
    n.talk.receive(&Reply { answer: "Done.".to_string(), ..Reply::default() }, 1);
    s.notes.push(n);
    let pop = open(&mut s, 1);
    assert!(with_id(&pop, "__azreview_conv-reopen-1").is_some());
    assert!(with_id(&pop, "__azreview_conv-resolve-1").is_none());
    s.notes[0].talk.reopen();
    let pop = popover(&s, &data(), 0, ui::PAGE_W).expect("open");
    assert!(with_id(&pop, "__azreview_conv-reopen-1").is_none());
    assert!(with_id(&pop, "__azreview_conv-resolve-1").is_some());
}

#[test]
fn suggestions_that_wait_are_counted_in_the_open_conversation() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "x");
    n.talk.phase = Phase::Suggested;
    n.talk.pending = 3;
    s.notes.push(n);
    let pop = open(&mut s, 1);
    assert_eq!(texts(with_id(&pop, "__azreview_conv-suggestions-1").expect("the count")), vec!["3 suggestions wait"]);
    s.notes[0].talk.pending = 1;
    let pop = popover(&s, &data(), 0, ui::PAGE_W).expect("open");
    assert_eq!(texts(with_id(&pop, "__azreview_conv-suggestions-1").expect("the count")), vec!["One suggestion waits"], "one is not many");
}

// ==== How tall the open conversation is ====

#[test]
fn a_text_takes_as_many_lines_as_its_letters_need() {
    assert_eq!(lines_of(0), 1, "even nothing takes a line");
    assert_eq!(lines_of(1), 1);
    assert_eq!(lines_of(LINE_CHARS), 1);
    assert_eq!(lines_of(LINE_CHARS + 1), 2);
    assert_eq!(lines_of(10 * LINE_CHARS), 10);
}

#[test]
fn the_open_conversation_grows_with_what_is_said_in_it_and_never_shrinks_for_it() {
    let mut s = app();
    s.notes.push(note(1, 0, 40.0, "rename this"));
    let empty = popover_height(&s.notes[0], false, 0);
    assert!(empty > 120.0 && empty < 320.0, "a header, the words and a reply: {empty}");
    let mut last = empty;
    for k in 0..4 {
        s.notes[0].talk.turns.push(Turn { who: Who::Agent, text: "word ".repeat(30 + k * 20), at_unix_ms: k as u64 });
        let now = popover_height(&s.notes[0], false, 0);
        assert!(now > last, "a turn adds height: {last} then {now}");
        last = now;
    }
    s.notes[0].talk.heard_as = Some("something else".to_string());
    assert!(popover_height(&s.notes[0], false, 0) > last, "what the agent understood takes a line");
    s.notes[0].talk.phase = Phase::Failed;
    s.notes[0].talk.problem = Some("no".to_string());
    assert!(popover_height(&s.notes[0], false, 0) > last, "so does a status");
}

#[test]
fn only_the_newest_turns_are_shown_and_the_conversation_says_how_many_are_not() {
    let mut s = app();
    let mut n = note(1, 0, 40.0, "x");
    for k in 0..(SHOW_TURNS + 3) {
        n.talk.turns.push(Turn { who: Who::Agent, text: format!("turn {k}"), at_unix_ms: k as u64 });
    }
    s.notes.push(n);
    let pop = open(&mut s, 1);
    assert!(with_id(&pop, "__azreview_conv-turn-1-0").is_some());
    assert!(with_id(&pop, &format!("__azreview_conv-turn-1-{}", SHOW_TURNS - 1)).is_some());
    assert!(with_id(&pop, &format!("__azreview_conv-turn-1-{SHOW_TURNS}")).is_none(), "no more than the newest are built");
    let first = texts(with_id(&pop, "__azreview_conv-turn-1-0").expect("a turn"));
    assert_eq!(first[1], "turn 3", "the first shown is the fourth: the three before it are not shown");
    let said = texts(with_id(&pop, "__azreview_conv-earlier-1").expect("the count of the turns not shown")).join(" ");
    assert!(said.contains('3'), "{said}");
}

#[test]
fn a_very_long_turn_is_cut_at_a_letter_with_an_ellipsis() {
    assert_eq!(shown("short"), "short");
    let long = "é".repeat(SHOW_TURN_CHARS + 50);
    let cut = shown(&long);
    assert_eq!(cut.chars().count(), SHOW_TURN_CHARS + 1, "the letters kept and the ellipsis");
    assert!(cut.ends_with('…'));
}
