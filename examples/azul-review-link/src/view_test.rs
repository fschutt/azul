//! What the phone keeps of what the Mac says: the newest pen state, the picture of the page, the folder being
//! listed (in pages), the four buttons and how far each press has got.

use super::*;
use crate::frame::{EntryKind, FileState, NoticeLevel, Reply, Rows, ThumbFormat};

fn pen(seq: u32, line: u32) -> PenState {
    PenState { seq, file: "src/lib.rs".into(), line, thumb_id: 3, ..PenState::empty() }
}

fn thumb(id: u32) -> Thumbnail {
    Thumbnail { id, width: 1, height: 1, format: ThumbFormat::Rgba8, bytes: vec![0, 0, 0, 255] }
}

fn entry(name: &str, kind: EntryKind) -> Entry {
    Entry { name: name.into(), kind, state: FileState::None }
}

fn rows(request_id: u32, dir: &str, offset: u32, names: &[&str], more: bool) -> Frame {
    Frame::Rows(Rows {
        request_id,
        dir: dir.into(),
        offset,
        entries: names.iter().map(|n| entry(n, EntryKind::File)).collect(),
        more,
    })
}

#[test]
fn a_new_view_is_unpaired_with_the_default_buttons_and_nothing_else() {
    let view = PhoneView::new();
    assert_eq!(view.session, Session::Unpaired);
    assert_eq!(view.buttons, ButtonSet::defaults());
    assert!(view.pen.is_none() && view.thumb.is_none() && view.notice.is_none());
    assert_eq!(view.button_status, [None; 4]);
    assert!(!view.is_connected());
    assert!(!view.files.loading());
}

// ---- the pen state

#[test]
fn a_newer_pen_state_replaces_the_one_before_and_a_state_that_is_not_newer_is_ignored() {
    let mut view = PhoneView::new();
    assert!(view.apply_frame(&Frame::PenState(pen(5, 10))));
    assert_eq!(view.pen.as_ref().map(|p| p.line), Some(10));
    assert!(!view.apply_frame(&Frame::PenState(pen(5, 99))), "the same place in the order");
    assert!(!view.apply_frame(&Frame::PenState(pen(4, 99))), "an older state that came late");
    assert_eq!(view.pen.as_ref().map(|p| p.line), Some(10));
    assert!(view.apply_frame(&Frame::PenState(pen(6, 11))));
    assert_eq!(view.pen.as_ref().map(|p| p.line), Some(11));
}

#[test]
fn a_state_that_only_counts_up_changes_nothing_the_screens_draw() {
    let mut view = PhoneView::new();
    assert!(view.apply_frame(&Frame::PenState(pen(5, 10))));
    assert!(!view.apply_frame(&Frame::PenState(pen(6, 10))), "the same page, the same pen");
    assert_eq!(view.pen.as_ref().map(|p| p.seq), Some(6), "but it is the newest now");
}

#[test]
fn the_order_of_pen_states_survives_the_counter_wrapping_round() {
    let mut view = PhoneView::new();
    assert!(view.apply_frame(&Frame::PenState(pen(u32::MAX - 1, 1))));
    assert!(view.apply_frame(&Frame::PenState(pen(u32::MAX, 2))));
    assert!(view.apply_frame(&Frame::PenState(pen(0, 3))), "0 follows u32::MAX");
    assert!(!view.apply_frame(&Frame::PenState(pen(u32::MAX, 9))), "and u32::MAX is then the old one");
    assert_eq!(view.pen.as_ref().map(|p| p.line), Some(3));
}

// ---- the picture

#[test]
fn a_thumbnail_is_kept_and_it_is_the_current_one_when_its_id_is_the_pens() {
    let mut view = PhoneView::new();
    assert!(!view.thumb_is_current());
    assert!(view.apply_frame(&Frame::Thumbnail(thumb(2))));
    view.apply_frame(&Frame::PenState(pen(1, 1)));
    assert!(!view.thumb_is_current(), "the pen state names thumbnail 3, thumbnail 2 is an older page");
    assert!(view.apply_frame(&Frame::Thumbnail(thumb(3))));
    assert!(view.thumb_is_current());
    assert_eq!(view.thumb.as_ref().map(|t| t.id), Some(3));
}

#[test]
fn a_thumbnail_with_no_id_is_ignored_and_a_state_without_a_picture_has_no_current_one() {
    let mut view = PhoneView::new();
    assert!(!view.apply_frame(&Frame::Thumbnail(thumb(0))));
    assert!(view.thumb.is_none());
    view.apply_frame(&Frame::Thumbnail(thumb(7)));
    view.apply_frame(&Frame::PenState(PenState { thumb_id: 0, ..pen(1, 1) }));
    assert!(!view.thumb_is_current());
}

// ---- the folder

#[test]
fn the_rows_of_the_folder_asked_for_replace_the_list_and_end_the_wait() {
    let mut view = PhoneView::new();
    view.begin_listing("src");
    assert!(view.files.loading());
    assert!(view.apply_frame(&rows(4, "src", 0, &["a.rs", "b.rs"], false)));
    assert_eq!(view.files.dir, "src");
    assert_eq!(view.files.entries.len(), 2);
    assert!(!view.files.loading() && view.files.error.is_none() && !view.files.more);
}

#[test]
fn the_rows_of_another_folder_than_the_one_asked_for_are_ignored_while_waiting() {
    let mut view = PhoneView::new();
    view.begin_listing("src");
    assert!(!view.apply_frame(&rows(3, "docs", 0, &["x.md"], false)), "a late answer to an earlier question");
    assert!(view.files.loading(), "still waiting for src");
    assert!(view.files.entries.is_empty());
}

#[test]
fn rows_the_mac_pushes_for_the_folder_on_screen_replace_it_when_nothing_is_awaited() {
    let mut view = PhoneView::new();
    view.begin_listing("src");
    view.apply_frame(&rows(4, "src", 0, &["a.rs"], false));
    let mut pushed = Rows { request_id: 0, dir: "src".into(), offset: 0, entries: vec![entry("a.rs", EntryKind::File)], more: false };
    pushed.entries[0].state = FileState::Unresolved;
    assert!(view.apply_frame(&Frame::Rows(pushed)), "a file turned yellow");
    assert_eq!(view.files.entries[0].state, FileState::Unresolved);
    assert!(!view.apply_frame(&rows(0, "elsewhere", 0, &["z"], false)), "another folder is not on screen");
}

#[test]
fn a_long_folder_comes_in_pages_that_continue_each_other() {
    let mut view = PhoneView::new();
    view.begin_listing("big");
    assert!(view.apply_frame(&rows(1, "big", 0, &["a", "b", "c"], true)));
    assert!(view.files.more);
    assert!(view.apply_frame(&rows(2, "big", 3, &["d", "e"], false)), "the page that starts where the list ends");
    let names: Vec<&str> = view.files.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a", "b", "c", "d", "e"]);
    assert!(!view.files.more);
    assert!(!view.apply_frame(&rows(3, "big", 9, &["z"], false)), "a page that leaves a gap is not appended");
    assert_eq!(view.files.entries.len(), 5);
}

#[test]
fn a_failed_request_keeps_the_rows_on_screen_and_says_why_and_the_next_ask_clears_it() {
    let mut view = PhoneView::new();
    view.begin_listing("src");
    view.apply_frame(&rows(1, "src", 0, &["a"], false));
    view.begin_listing("docs");
    view.files_failed("no answer within 10 s");
    assert!(!view.files.loading());
    assert_eq!(view.files.error.as_deref(), Some("no answer within 10 s"));
    assert_eq!(view.files.dir, "src", "the old list is still the one on screen");
    assert_eq!(view.files.entries.len(), 1);
    view.begin_listing("docs");
    assert_eq!(view.files.error, None);
}

// ---- the buttons

#[test]
fn the_buttons_the_mac_sends_replace_the_set_and_keep_what_became_of_the_presses() {
    let mut view = PhoneView::new();
    view.button_pressed(0, 40);
    view.apply_frame(&Frame::Reply(Reply { request_id: 40, status: ReplyStatus::Working }));
    let mut set = ButtonSet::defaults();
    set.buttons[0].label = "Nicer".into();
    assert!(view.apply_frame(&Frame::Buttons(set.clone())));
    assert_eq!(view.buttons, set);
    assert_eq!(view.button_status[0], Some(ReplyStatus::Working));
    assert!(!view.apply_frame(&Frame::Buttons(set)), "the same set again is nothing new");
}

#[test]
fn a_reply_reaches_the_button_whose_press_it_answers_and_no_other() {
    let mut view = PhoneView::new();
    view.button_pressed(1, 77);
    assert_eq!(view.button_status[1], None, "pressed, no answer yet");
    assert_eq!(view.button_request[1], 77);
    assert!(view.apply_frame(&Frame::Reply(Reply { request_id: 77, status: ReplyStatus::Received })));
    assert_eq!(view.button_status[1], Some(ReplyStatus::Received));
    assert!(view.apply_frame(&Frame::Reply(Reply { request_id: 77, status: ReplyStatus::Suggested })));
    assert_eq!(view.button_status[1], Some(ReplyStatus::Suggested));
    assert!(!view.apply_frame(&Frame::Reply(Reply { request_id: 5, status: ReplyStatus::Done })), "no such press");
    assert_eq!(view.button_status, [None, Some(ReplyStatus::Suggested), None, None]);
}

#[test]
fn a_press_that_was_pressed_again_ignores_the_answers_of_the_first() {
    let mut view = PhoneView::new();
    view.button_pressed(2, 10);
    view.button_pressed(2, 11);
    assert!(!view.apply_frame(&Frame::Reply(Reply { request_id: 10, status: ReplyStatus::Done })));
    assert_eq!(view.button_status[2], None);
    assert!(view.apply_frame(&Frame::Reply(Reply { request_id: 11, status: ReplyStatus::Working })));
    assert_eq!(view.button_status[2], Some(ReplyStatus::Working));
}

#[test]
fn a_button_that_does_not_exist_is_ignored() {
    let mut view = PhoneView::new();
    view.button_pressed(4, 1);
    view.button_pressed(usize::MAX, 1);
    assert_eq!(view.button_request, [0; 4]);
}

// ---- the rest

#[test]
fn a_notice_is_kept_and_a_newer_one_replaces_it() {
    let mut view = PhoneView::new();
    let notice = |text: &str| Notice { level: NoticeLevel::Info, text: text.into() };
    assert!(view.apply_frame(&Frame::Notice(notice("one"))));
    assert!(!view.apply_frame(&Frame::Notice(notice("one"))), "the same line again");
    assert!(view.apply_frame(&Frame::Notice(notice("two"))));
    assert_eq!(view.notice, Some(notice("two")));
}

#[test]
fn frames_that_are_the_clients_business_do_not_change_the_view() {
    let mut view = PhoneView::new();
    let before = view.clone();
    for frame in [
        Frame::Ping { nonce: 1 },
        Frame::Pong { nonce: 1 },
        Frame::Bye,
        Frame::PairAccepted { code: 1 },
        Frame::Refused(crate::frame::Refusal::NotPaired),
        Frame::Button { request_id: 1, index: 0 },
    ] {
        assert!(!view.apply_frame(&frame), "{}", frame.name());
    }
    assert_eq!(view, before);
}
